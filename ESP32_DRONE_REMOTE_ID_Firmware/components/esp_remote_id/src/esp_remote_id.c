#include <stdio.h>
#include <string.h>
#include "esp_log.h"
#include "esp_timer.h"
#include "esp_heap_caps.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "freertos/semphr.h"
#include "esp_remote_id.h"
#include "protocol_detect.h"
#include "nmea_parser.h"
#include "msp_parser.h"
#include "mavlink_parser.h"
#include "wifi_tx.h"
#include "ble_tx.h"
#include "esp_netif.h"
#include "web_config.h"
#include "nvs_storage.h"
#include "led_status.h"
#include "rid_patrol.h"
#include "cli.h"
#include "rid_kalman.h"
#include "rid_mavlink_tx.h"
#include "rid_auth.h"
#include "rid_ota.h"
#include "led_ws2812.h"
#include "rid_lighting.h"
#include "rid_dronecan.h"
#include "rid_mavlink_usb.h"
#include "rid_output.h"
#include "esp_task_wdt.h"

#define TAG "ESP_RID"

#define C_BLU  "\x1b[34m"
#define C_GRN  "\x1b[32m"
#define C_AMB  "\x1b[33m"
#define C_RED  "\x1b[31m"
#define C_RST  "\x1b[0m"

static rid_config_t g_config;
static rid_state_t g_state;
static bool g_running = false;
static SemaphoreHandle_t g_lock = NULL;
static rid_kalman_3d_t g_kalman;

static void default_config(rid_config_t *cfg)
{
    memset(cfg, 0, sizeof(rid_config_t));

    cfg->protocol = RID_PROTOCOL_AUTO;
    cfg->uart_port = 1;
    cfg->baud_rate = 57600;
    cfg->tx_pin = 17;
    cfg->rx_pin = 18;

    cfg->region = RID_REGION_AUTO;

    cfg->ua_type = 1;
    cfg->id_type = 1;
    snprintf(cfg->uas_id, sizeof(cfg->uas_id), "%s", "ESP32-RID-001");
    snprintf(cfg->operator_id, sizeof(cfg->operator_id), "%s", "OP-UNKNOWN");
    cfg->self_id_text[0] = '\0';
    cfg->operator_lat = 0.0;
    cfg->operator_lon = 0.0;
    cfg->operator_alt = 0.0f;

    cfg->ua_type_2 = 0;
    cfg->id_type_2 = 0;
    cfg->uas_id_2[0] = '\0';

    cfg->tx_modes = RID_TRANSMIT_WIFI_BCN;
    cfg->wifi_channel = 6;
    cfg->wifi_power_dbm = 20.0f;
    cfg->wifi_bcn_rate_hz = 1.0f;
    cfg->wifi_nan_rate_hz = 0.0f;
    cfg->ble4_rate_hz = 1.0f;
    cfg->ble4_power_dbm = 18.0f;
    cfg->ble5_rate_hz = 1.0f;
    cfg->ble5_power_dbm = 18.0f;

    cfg->wifi_ssid[0] = '\0';
    snprintf(cfg->wifi_ssid, sizeof(cfg->wifi_ssid), "%s", "ESP-RID");
    cfg->wifi_password[0] = '\0';
    cfg->webserver_en = 1;

    cfg->mavlink_sysid = 0;
    cfg->bcast_powerup = 1;

    cfg->options = 0;
    cfg->lock_level = 0;

    cfg->led_r_gpio = -1;
    cfg->led_g_gpio = -1;
    cfg->led_b_gpio = -1;

    cfg->ws2812_gpio = -1;
    cfg->ws2812_brightness = 16;

    memset(cfg->lighting_pins, -1, sizeof(cfg->lighting_pins));
    memset(cfg->lighting_patterns, 0, sizeof(cfg->lighting_patterns));
    memset(cfg->lighting_phase_offsets, 0, sizeof(cfg->lighting_phase_offsets));

    cfg->dronecan_rx_gpio = -1;
    cfg->dronecan_tx_gpio = -1;
    cfg->dronecan_bitrate = 1000000;

    cfg->mavlink_usb_enable = false;

    cfg->ota_trigger_gpio = -1;

    cfg->auth_private_key[0] = '\0';

    cfg->start_delay_ms = 10000;

    for (int i = 0; i < ESP_RID_NUM_KEYS; i++)
        cfg->public_keys[i][0] = '\0';
}

static bool identity_is_sane(const rid_identity_t *id, const rid_region_rules_t *rules)
{
    if (rules->require_uas_id && id->uas_id[0] == '\0') return false;
    if (strstr((const char *)id->uas_id, "ESP32-RID-") == (const char *)id->uas_id) return false;
    if (rules->require_operator_id) {
        if (strstr((const char *)id->operator_id, "OP-UNKNOWN") != NULL) return false;
        if (id->operator_id[0] == '\0') return false;
    }
    return true;
}

static bool position_is_sane(const rid_gps_data_t *gps)
{
    if (gps->latitude < -90.0 || gps->latitude > 90.0) return false;
    if (gps->longitude < -180.0 || gps->longitude > 180.0) return false;
    return true;
}

void esp_rid_init(void)
{
    ESP_LOGI(TAG, "ESP DRONE REMOTEID v%s initializing", ESP_RID_VERSION);

    nvs_storage_init();
    default_config(&g_config);
    nvs_storage_load(&g_config);

    /* Bind the active broadcast standard to the configured region */
    g_state.active_standard = rid_output_active_standard(&g_config);
    g_state.standard_fallback = !rid_output_has_encoder(g_state.active_standard);

    /* Check OTA mode (reads ota_trigger_gpio from config) */
    rid_ota_check_and_run(&g_config);

    g_lock = xSemaphoreCreateMutex();

    memset(&g_state, 0, sizeof(rid_state_t));

    /* Startup delay */
    if (g_config.start_delay_ms > 0) {
        ESP_LOGI(TAG, "Startup delay %lu ms", (unsigned long)g_config.start_delay_ms);
        vTaskDelay(pdMS_TO_TICKS(g_config.start_delay_ms));
    }

    /* Probe at 115200 for AUTO detection; honour the configured baud for a fixed protocol */
    uint32_t boot_baud = (g_config.protocol == RID_PROTOCOL_AUTO) ? 115200 : g_config.baud_rate;
    if (boot_baud == 0) boot_baud = 115200;
    protocol_detect_init(g_config.uart_port, g_config.tx_pin, g_config.rx_pin, boot_baud);
    nmea_parser_init(g_config.uart_port);
    msp_parser_init(g_config.uart_port);
    mavlink_parser_init(g_config.uart_port);
    mavlink_parser_set_sysid_filter(g_config.mavlink_sysid);

    esp_netif_init();

    wifi_tx_init(&g_config);
    ble_tx_init();

    /* Apply BLE TX power after BLE controller is initialized */
    ble_tx_set_power(9);

    /* MAVLink bidirectional link */
    if (g_config.options & (RID_OPT_MAVLINK_ARM_STATUS | RID_OPT_MAVLINK_OP_LOC_LOOP) ||
        g_config.mavlink_usb_enable) {
        rid_mavlink_tx_init(g_config.uart_port);
        BaseType_t mavlink_tx_ret = xTaskCreate(rid_mavlink_tx_task, "rid_mavlink_tx", 2048, NULL, 3, NULL);
        if (mavlink_tx_ret != pdPASS) {
            ESP_LOGE(TAG, "Failed to create rid_mavlink_tx task (err=%d)", (int)mavlink_tx_ret);
        }
    }

    /* Authentication */
    if (g_config.options & RID_OPT_AUTH_ED25519) {
        rid_auth_init(g_config.auth_private_key);
    }
    g_state.auth_enabled = rid_auth_enabled();

    /* WS2812 addressable LED */
    led_ws2812_init(g_config.ws2812_gpio, g_config.ws2812_brightness);

    /* External lighting outputs */
    for (int i = 0; i < RID_LIGHTING_MAX_OUTPUTS; i++) {
        if (g_config.lighting_pins[i] >= 0) {
            rid_lighting_init(g_config.lighting_pins, g_config.lighting_patterns,
                             g_config.lighting_phase_offsets);
            break;
        }
    }

    /* DroneCAN */
    if (g_config.dronecan_rx_gpio >= 0 && g_config.dronecan_tx_gpio >= 0) {
        rid_dronecan_init(g_config.dronecan_rx_gpio, g_config.dronecan_tx_gpio,
                         g_config.dronecan_bitrate);
    }

    /* USB Serial MAVLink */
    if (g_config.mavlink_usb_enable) {
        rid_mavlink_usb_init();
    }

    if (strcmp(g_config.uas_id, "ESP32-RID-001") == 0 ||
        strcmp(g_config.operator_id, "OP-UNKNOWN") == 0) {
        uint8_t mac[6];
        wifi_tx_get_mac(mac);
        snprintf(g_config.uas_id, sizeof(g_config.uas_id), "ESP32-RID-%02X%02X", mac[4], mac[5]);
        snprintf(g_config.operator_id, sizeof(g_config.operator_id), "ESP32-OP-%02X%02X", mac[4], mac[5]);
        nvs_storage_save(&g_config);
    }

    led_status_reconfigure(g_config.led_r_gpio, g_config.led_g_gpio, g_config.led_b_gpio);
    web_config_init(g_config.webserver_en != 0);

    cli_init();

    rid_kalman_init(&g_kalman);

    ESP_LOGI(TAG, "\xE2\x9C\x93 Remote ID initialized");
}

void esp_rid_set_config(const rid_config_t *config)
{
    if (g_lock && xSemaphoreTake(g_lock, pdMS_TO_TICKS(100)) == pdTRUE) {
        uint32_t old_baud = g_config.baud_rate;
        memcpy(&g_config, config, sizeof(rid_config_t));
        nvs_storage_save(&g_config);
        g_state.active_standard = rid_output_active_standard(&g_config);
        g_state.standard_fallback = !rid_output_has_encoder(g_state.active_standard);
        if (g_config.baud_rate != old_baud && g_config.baud_rate > 0) {
            protocol_detect_reinit(g_config.uart_port, g_config.tx_pin, g_config.rx_pin, g_config.baud_rate);
        }
        mavlink_parser_set_sysid_filter(g_config.mavlink_sysid);
        led_status_reconfigure(g_config.led_r_gpio, g_config.led_g_gpio, g_config.led_b_gpio);
        led_ws2812_init(g_config.ws2812_gpio, g_config.ws2812_brightness);
        ble_tx_set_power(g_config.ble4_power_dbm > 0 ? (int8_t)g_config.ble4_power_dbm : 9);
        wifi_tx_reconfigure_ap(&g_config);
        xSemaphoreGive(g_lock);
    }
}

void esp_rid_get_config(rid_config_t *config)
{
    if (g_lock && xSemaphoreTake(g_lock, pdMS_TO_TICKS(100)) == pdTRUE) {
        memcpy(config, &g_config, sizeof(rid_config_t));
        xSemaphoreGive(g_lock);
    }
}

void esp_rid_get_state(rid_state_t *state)
{
    if (g_lock && xSemaphoreTake(g_lock, pdMS_TO_TICKS(100)) == pdTRUE) {
        memcpy(state, &g_state, sizeof(rid_state_t));
        xSemaphoreGive(g_lock);
    }
}

void esp_rid_factory_reset(void)
{
    char keys[ESP_RID_NUM_KEYS][ESP_RID_MAX_KEY_LEN + 1];
    memset(keys, 0, sizeof(keys));

    if (g_lock && xSemaphoreTake(g_lock, pdMS_TO_TICKS(100)) == pdTRUE) {
        for (int i = 0; i < ESP_RID_NUM_KEYS; i++)
            strncpy(keys[i], g_config.public_keys[i], sizeof(keys[i]));
        default_config(&g_config);
        for (int i = 0; i < ESP_RID_NUM_KEYS; i++)
            strncpy(g_config.public_keys[i], keys[i], sizeof(g_config.public_keys[i]));
        xSemaphoreGive(g_lock);
    }
    nvs_storage_reset_preserve_keys();
    nvs_storage_save(&g_config);
}

static int64_t last_tx_wifi_bcn = 0;
static int64_t last_tx_wifi_nan = 0;
static int64_t last_tx_ble4 = 0;
static int64_t last_tx_ble5 = 0;

static bool rate_allowed(int64_t *last_us, float rate_hz)
{
    if (rate_hz <= 0.0f) return false;
    int64_t now = esp_timer_get_time();
    int64_t interval = (int64_t)(1000000.0f / rate_hz);
    if (now - *last_us >= interval) {
        *last_us = now;
        return true;
    }
    return false;
}

static uint8_t g_nan_counter = 0;

static bool update_transmissions(void)
{
    if (!g_state.gps_valid && !g_config.bcast_powerup) return false;
    if (g_state.active_protocol == RID_PROTOCOL_UNKNOWN) return false;

    /* Identity readiness gate */
    if (g_config.options & RID_OPT_IDENTITY_READY_GATE) {
        if (!g_state.identity_ready) return false;
    }

    bool tx = false;

    if ((g_config.tx_modes & RID_TRANSMIT_WIFI_BCN) &&
        rate_allowed(&last_tx_wifi_bcn, g_config.wifi_bcn_rate_hz)) {
        wifi_tx_transmit(&g_state.gps, &g_state.identity, &g_config);
        g_state.wifi_bcn_count++;
        g_state.transmissions_count++;
        tx = true;
    }

    if ((g_config.tx_modes & RID_TRANSMIT_WIFI_NAN) &&
        rate_allowed(&last_tx_wifi_nan, g_config.wifi_nan_rate_hz)) {
        wifi_tx_transmit_nan(&g_state.gps, &g_state.identity, g_nan_counter++, &g_config);
        g_state.wifi_nan_count++;
        g_state.transmissions_count++;
        tx = true;
    }

    if ((g_config.tx_modes & RID_TRANSMIT_BLE4) &&
        rate_allowed(&last_tx_ble4, g_config.ble4_rate_hz)) {
        ble_tx_transmit_legacy(&g_state.gps, &g_state.identity, &g_config);
        g_state.ble4_count++;
        g_state.transmissions_count++;
        tx = true;
    }

    if ((g_config.tx_modes & RID_TRANSMIT_BLE5) &&
        rate_allowed(&last_tx_ble5, g_config.ble5_rate_hz)) {
        ble_tx_transmit_lr(&g_state.gps, &g_state.identity, &g_config);
        g_state.ble5_count++;
        g_state.transmissions_count++;
        tx = true;
    }

    return tx;
}

static const char *proto_name(rid_protocol_t p)
{
    switch (p) {
        case RID_PROTOCOL_UNKNOWN: return "UNKNOWN";
        case RID_PROTOCOL_MAVLINK: return "MAVLink";
        case RID_PROTOCOL_MSP:     return "MSP";
        case RID_PROTOCOL_NMEA:   return "NMEA";
        case RID_PROTOCOL_NONE:   return "NONE";
        case RID_PROTOCOL_AUTO:   return "AUTO";
        default: return "?";
    }
}

static void print_status_box(void)
{
    char lat_str[16], lon_str[16];
    if (g_state.gps_valid)
        snprintf(lat_str, sizeof(lat_str), "%.4f", g_state.gps.latitude);
    else
        snprintf(lat_str, sizeof(lat_str), "--");
    if (g_state.gps_valid)
        snprintf(lon_str, sizeof(lon_str), "%.4f", g_state.gps.longitude);
    else
        snprintf(lon_str, sizeof(lon_str), "--");

    const char *gps_str = g_state.gps_valid ? "YES" : "NO";
    const char *proto = proto_name(g_state.active_protocol);
    const char *kal_str;
    if (g_config.options & RID_OPT_KALMAN_FILTER)
        kal_str = rid_kalman_valid_age(&g_kalman, esp_timer_get_time()) ? "ON" : "INIT";
    else
        kal_str = "OFF";

    const char *rdy_str = g_state.identity_ready ? "YES" : "NO";

    printf(C_GRN);
    printf("\n");
    printf("  \xE2\x94\x8C\xE2\x94\x80 ESP Drone Remote ID \xE2\x94\x80 status \xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x90\n");
    printf("  \xE2\x94\x82  Protocol  : %-33s \xE2\x94\x82\n", proto);
    printf("  \xE2\x94\x82  GPS Fix   : %-3s           (%d/%d)       \xE2\x94\x82\n",
           gps_str, g_state.gps.fix_type, g_state.gps.satellites);
    printf("  \xE2\x94\x82  Lat / Lon : %s / %-12s       \xE2\x94\x82\n", lat_str, lon_str);
    printf("  \xE2\x94\x82  TX count  : %-35lu \xE2\x94\x82\n",
           (unsigned long)g_state.transmissions_count);
    printf("  \xE2\x94\x82  Kalman    : %-33s \xE2\x94\x82\n", kal_str);
    printf("  \xE2\x94\x82  Identity  : %-33s \xE2\x94\x82\n", rdy_str);
    printf("  \xE2\x94\x94\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x98\n");
    printf(C_RST);
}

static void print_system_box(void)
{
    int64_t us = esp_timer_get_time();
    uint32_t sec = (uint32_t)(us / 1000000);
    uint32_t h = sec / 3600, m = (sec % 3600) / 60, s = sec % 60;
    uint32_t heap_free = esp_get_free_heap_size();
    uint32_t heap_total = heap_caps_get_total_size(MALLOC_CAP_DEFAULT);
    printf(C_BLU);
    printf("\n");
    printf("  \xE2\x94\x8C\xE2\x94\x80 System Info \xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x90\n");
    printf("  \xE2\x94\x82  Heap free : %-5lu KB / %-5lu KB                        \xE2\x94\x82\n",
           (unsigned long)(heap_free / 1024), (unsigned long)(heap_total / 1024));
    printf("  \xE2\x94\x82  Uptime    : %02lu:%02lu:%02lu                                      \xE2\x94\x82\n",
           (unsigned long)h, (unsigned long)m, (unsigned long)s);
    printf("  \xE2\x94\x94\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x80\xE2\x94\x98\n");
    printf(C_RST);
}

static void rid_task(void *arg)
{
    g_state.active_protocol = RID_PROTOCOL_UNKNOWN;
    uint32_t log_cycle = 0;
    bool had_gps = false;

    esp_err_t wdt_ret = esp_task_wdt_add(NULL);
    if (wdt_ret != ESP_OK) {
        ESP_LOGW(TAG, "esp_task_wdt_add failed: %s", esp_err_to_name(wdt_ret));
    }

    while (g_running) {
        rid_gps_data_t gps_data;
        rid_protocol_t proto = RID_PROTOCOL_UNKNOWN;

        memset(&gps_data, 0, sizeof(rid_gps_data_t));

        if (g_lock) xSemaphoreTake(g_lock, portMAX_DELAY);
        rid_protocol_t cfg_proto = g_config.protocol;
        uint16_t cfg_opts = g_config.options;
        if (g_lock) xSemaphoreGive(g_lock);

        if (cfg_proto == RID_PROTOCOL_AUTO)
            proto = protocol_detect_auto();
        else
            proto = cfg_proto;

        g_state.active_protocol = proto;

        bool have_data = false;

        switch (proto) {
        case RID_PROTOCOL_NMEA:
            have_data = nmea_parser_get(&gps_data);
            break;
        case RID_PROTOCOL_MSP:
            have_data = msp_parser_get(&gps_data);
            break;
        case RID_PROTOCOL_MAVLINK:
            have_data = mavlink_parser_get(&gps_data);
            break;
        default:
            break;
        }

        /* Try DroneCAN as secondary input */
        if (!have_data && rid_dronecan_is_active()) {
            have_data = rid_dronecan_get(&gps_data);
            if (have_data) g_state.active_protocol = RID_PROTOCOL_NONE;
        }

        had_gps = false;

        if (have_data && gps_data.latitude != 0.0) {
            bool force_tx = (cfg_opts & RID_OPT_FORCE_ARM_OK) && gps_data.armed;

            if (force_tx || gps_data.fix_type >= 2) {
                had_gps = true;
                if (g_lock) xSemaphoreTake(g_lock, portMAX_DELAY);
                memcpy(&g_state.gps, &gps_data, sizeof(rid_gps_data_t));
                g_state.gps_valid = true;
                g_state.last_update_ms = xTaskGetTickCount() * portTICK_PERIOD_MS;

                /* MAVLink arm status */
                if (proto == RID_PROTOCOL_MAVLINK) {
                    mavlink_parser_get_armed(&g_state.mavlink_armed);
                    g_state.gps.armed = g_state.mavlink_armed;
                    uint8_t sysid = 0;
                    if (mavlink_parser_get_sysid(&sysid)) {
                        g_state.mavlink_sysid = sysid;
                    }
                }

                rid_identity_t mav_id;
                bool have_mav_id = false;
                if (proto == RID_PROTOCOL_MAVLINK) {
                    have_mav_id = mavlink_parser_get_identity(&mav_id);
                }

                if (have_mav_id && mav_id.uas_id[0] != '\0') {
                    memcpy(&g_state.identity, &mav_id, sizeof(rid_identity_t));
                } else {
                    snprintf(g_state.identity.uas_id, sizeof(g_state.identity.uas_id), "%s", g_config.uas_id);
                    snprintf(g_state.identity.operator_id, sizeof(g_state.identity.operator_id), "%s", g_config.operator_id);
                    snprintf(g_state.identity.self_id_text, sizeof(g_state.identity.self_id_text), "%s", g_config.self_id_text);
                    g_state.identity.id_type = g_config.id_type;
                    g_state.identity.ua_type = g_config.ua_type;
                    snprintf(g_state.identity.uas_id_2, sizeof(g_state.identity.uas_id_2), "%s", g_config.uas_id_2);
                    g_state.identity.id_type_2 = g_config.id_type_2;
                    g_state.identity.ua_type_2 = g_config.ua_type_2;
                }

                if (!g_state.takeoff_captured && gps_data.fix_type >= 3 &&
                    gps_data.latitude != 0.0 && gps_data.longitude != 0.0) {
                    g_state.takeoff_lat = gps_data.latitude;
                    g_state.takeoff_lon = gps_data.longitude;
                    g_state.takeoff_alt = gps_data.altitude_msl;
                    g_state.takeoff_captured = true;
                }

                /* MSP/NMEA do not provide relative altitude; derive it from takeoff */
                if ((proto == RID_PROTOCOL_MSP || proto == RID_PROTOCOL_NMEA) &&
                    g_state.takeoff_captured) {
                    g_state.gps.altitude_relative = g_state.gps.altitude_msl - g_state.takeoff_alt;
                }

                /* Update operator location from MAVLink */
                double op_lat, op_lon;
                float op_alt;
                if (mavlink_parser_get_operator_location(&op_lat, &op_lon, &op_alt)) {
                    g_state.operator_lat = op_lat;
                    g_state.operator_lon = op_lon;
                    g_state.operator_alt = op_alt;
                    g_state.operator_position_updated_ms = xTaskGetTickCount() * portTICK_PERIOD_MS;
                    g_state.operator_location_type = 1;
                    /* Also feed the TX paths, which read gps->operator_* */
                    g_state.gps.operator_lat = op_lat;
                    g_state.gps.operator_lon = op_lon;
                    g_state.gps.operator_alt = op_alt;
                } else {
                    g_state.gps.operator_lat = g_config.operator_lat;
                    g_state.gps.operator_lon = g_config.operator_lon;
                    g_state.gps.operator_alt = g_config.operator_alt;
                }

                if (g_lock) xSemaphoreGive(g_lock);

                if (cfg_opts & RID_OPT_DONT_SAVE_BASIC_ID) {
                    g_state.identity.uas_id[0] = '\0';
                    g_state.identity.uas_id_2[0] = '\0';
                }

                /* Identity readiness gate */
                if ((cfg_opts & RID_OPT_IDENTITY_READY_GATE)) {
                    rid_region_rules_t rules = rid_output_region_rules(g_config.region);
                    if (identity_is_sane(&g_state.identity, &rules) &&
                        position_is_sane(&g_state.gps)) {
                        g_state.identity_ready = true;
                    }
                } else if (!(cfg_opts & RID_OPT_IDENTITY_READY_GATE)) {
                    g_state.identity_ready = true;
                }
            }
        } else if (cfg_opts & RID_OPT_DEMO_MODE) {
            if (g_lock) xSemaphoreTake(g_lock, portMAX_DELAY);
            rid_patrol_tick(&g_state.gps);
            g_state.gps_valid = true;
            had_gps = true;
            g_state.last_update_ms = xTaskGetTickCount() * portTICK_PERIOD_MS;
            g_state.active_protocol = RID_PROTOCOL_NONE;

            snprintf(g_state.identity.uas_id, sizeof(g_state.identity.uas_id), "%s", g_config.uas_id);
            snprintf(g_state.identity.operator_id, sizeof(g_state.identity.operator_id), "%s", g_config.operator_id);
            snprintf(g_state.identity.self_id_text, sizeof(g_state.identity.self_id_text), "%s", g_config.self_id_text);
            g_state.identity.id_type = g_config.id_type;
            g_state.identity.ua_type = g_config.ua_type;

            g_state.gps.operator_lat = g_config.operator_lat;
            g_state.gps.operator_lon = g_config.operator_lon;
            g_state.gps.operator_alt = g_config.operator_alt;

            if (g_lock) xSemaphoreGive(g_lock);

            g_state.identity_ready = true;
        }

        bool kalman_en = (cfg_opts & RID_OPT_KALMAN_FILTER) && !(cfg_opts & RID_OPT_DEMO_MODE);
        uint64_t now_us = kalman_en ? esp_timer_get_time() : 0;
        if (kalman_en) {

            if (had_gps && gps_data.latitude != 0.0 && gps_data.fix_type >= 2) {
                rid_kalman_update(&g_kalman, gps_data.latitude, gps_data.longitude,
                                 gps_data.altitude_msl, now_us);
            }

            rid_kalman_predict(&g_kalman, now_us);

            if (rid_kalman_valid_age(&g_kalman, now_us)) {
                double klat, klon;
                float kalt, kspeed, kclimb;
                int16_t kheading;
                rid_kalman_get(&g_kalman, &klat, &klon, &kalt, &kspeed, &kclimb, &kheading);
                g_state.gps.latitude = klat;
                g_state.gps.longitude = klon;
                g_state.gps.altitude_msl = kalt;
                g_state.gps.speed = kspeed;
                g_state.gps.speed_vertical = kclimb;
                g_state.gps.heading = kheading;
                g_state.gps_valid = true;
            } else if (!had_gps) {
                g_state.gps_valid = false;
            }
        }

        uint32_t now_ms = xTaskGetTickCount() * portTICK_PERIOD_MS;
        /* Absolute GPS timeout: without fresh parser data, GPS is stale
         * regardless of Kalman predictions. */
        if (g_state.gps_valid && (now_ms - g_state.last_update_ms > 10000)) {
            g_state.gps_valid = false;
            ESP_LOGW(TAG, "GPS data stale (no fix for %lu ms)",
                     (unsigned long)(now_ms - g_state.last_update_ms));
        }

        if (had_gps) {
            bool tx = update_transmissions();
            if (tx) led_status_tx_flash();
        }

        if (g_config.lock_level >= 2) {
            led_status_set_state(RID_LED_LOCKED);
        } else if (cfg_opts & RID_OPT_DEMO_MODE) {
            led_status_set_state(RID_LED_DEMO);
        } else if (g_state.gps_valid) {
            led_status_set_state(RID_LED_GPS_OK);
        } else {
            led_status_set_state(RID_LED_NO_GPS);
        }
        led_status_tick();

        /* WS2812 LED (if configured) */
        if (g_state.gps_valid) {
            led_ws2812_set_rgb(0, 255, 0);
        } else {
            led_ws2812_set_rgb(255, 200, 0);
        }

        /* External lighting outputs */
        rid_lighting_set_state(g_state.gps.armed, g_state.gps_valid);
        rid_lighting_tick();

        if (cfg_opts & RID_OPT_PRINT_RID_MAVLINK) {
            ESP_LOGI(TAG, "RID uas=%s lat=%.6f lon=%.6f alt=%.1f speed=%.1f hdg=%d fix=%d sat=%u ready=%d",
                g_state.identity.uas_id,
                g_state.gps.latitude, g_state.gps.longitude,
                (double)g_state.gps.altitude_msl, (double)g_state.gps.speed,
                g_state.gps.heading, g_state.gps.fix_type, g_state.gps.satellites,
                g_state.identity_ready);
        }

        log_cycle++;
        if (log_cycle % 100 == 0) {
            print_status_box();
        }
        if (log_cycle % 500 == 0) {
            print_system_box();
        }

        vTaskDelay(pdMS_TO_TICKS(100));
        esp_task_wdt_reset();
    }

    vTaskDelete(NULL);
}

void esp_rid_start(void)
{
    if (g_running) return;
    g_running = true;
    BaseType_t ret = xTaskCreate(rid_task, "rid_task", 4096, NULL, 5, NULL);
    if (ret != pdPASS) {
        g_running = false;
        ESP_LOGE(TAG, "Failed to create rid_task (err=%d)", (int)ret);
        return;
    }
    ESP_LOGI(TAG, "\xE2\x9C\x93 Remote ID started");
}

void esp_rid_stop(void)
{
    g_running = false;
    ESP_LOGI(TAG, "Remote ID stopped");
}
