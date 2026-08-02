#include <stdio.h>
#include <string.h>
#include <strings.h>
#include <stdlib.h>
#include <stdarg.h>
#include "esp_log.h"
#include "esp_http_server.h"
#include "esp_wifi.h"
#include "esp_ota_ops.h"
#include "psa/crypto.h"
#include "mbedtls/pk.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "freertos/task.h"
#include "web_config.h"
#include "nvs_storage.h"
#include "esp_remote_id.h"
#include "protocol_detect.h"
#include "esp_efuse.h"
#include "led_status.h"
#include "cJSON.h"
#include "rid_security.h"

#define TAG "WEB_CFG"
#define BUF_SIZE 4096
#define MAX_POST 4096
#define EFUSE_LOCK_MAGIC 0x52494421 // "RID!" stored as uint32_t

extern const char config_html_start[] asm("_binary_config_html_start");
extern const char config_html_end[] asm("_binary_config_html_end");
#define config_html_size ((size_t)(config_html_end - config_html_start))

static httpd_handle_t g_server = NULL;

#define SIG_RATE_MAX_FAILS   10
#define SIG_RATE_WINDOW_MS   60000

static uint32_t s_sig_fail_times[SIG_RATE_MAX_FAILS];
static int s_sig_fail_count = 0;

static bool sig_rate_check(void)
{
    uint32_t now = xTaskGetTickCount() * portTICK_PERIOD_MS;
    int valid = 0;
    for (int i = 0; i < s_sig_fail_count; i++) {
        if ((now - s_sig_fail_times[i]) < SIG_RATE_WINDOW_MS)
            s_sig_fail_times[valid++] = s_sig_fail_times[i];
    }
    s_sig_fail_count = valid;
    return s_sig_fail_count < SIG_RATE_MAX_FAILS;
}

static void sig_rate_record_fail(void)
{
    if (s_sig_fail_count < SIG_RATE_MAX_FAILS)
        s_sig_fail_times[s_sig_fail_count++] = xTaskGetTickCount() * portTICK_PERIOD_MS;
}

static int get_lock_level(void)
{
    uint8_t efuse_data[4] = {0};
    esp_efuse_read_block(EFUSE_BLK3, efuse_data, 0, 32);
    uint32_t magic = (uint32_t)efuse_data[0] | ((uint32_t)efuse_data[1] << 8)
                   | ((uint32_t)efuse_data[2] << 16) | ((uint32_t)efuse_data[3] << 24);
    if (magic == EFUSE_LOCK_MAGIC) return 2;

    rid_config_t cfg;
    esp_rid_get_config(&cfg);
    return cfg.lock_level;
}

/* ---------- Log ring buffer ---------- */
#define LOG_RING_MAX 64
#define LOG_MSG_MAX 240

typedef struct {
    uint32_t time_ms;
    char level;
    char msg[LOG_MSG_MAX];
} log_entry_t;

static log_entry_t s_log_ring[LOG_RING_MAX];
static int s_log_head = 0;
static int s_log_count = 0;
static SemaphoreHandle_t s_log_lock = NULL;
static int (*s_orig_vprintf)(const char *, va_list) = NULL;

static void log_push(char level, const char *msg)
{
    if (!s_log_lock) return;
    if (xSemaphoreTake(s_log_lock, pdMS_TO_TICKS(10)) == pdTRUE) {
        int i = (s_log_head + s_log_count) % LOG_RING_MAX;
        s_log_ring[i].time_ms = xTaskGetTickCount() * portTICK_PERIOD_MS;
        s_log_ring[i].level = level;
        strncpy(s_log_ring[i].msg, msg, LOG_MSG_MAX - 1);
        s_log_ring[i].msg[LOG_MSG_MAX - 1] = '\0';
        if (s_log_count < LOG_RING_MAX) s_log_count++;
        else s_log_head = (s_log_head + 1) % LOG_RING_MAX;
        xSemaphoreGive(s_log_lock);
    }
}

static int log_vprintf(const char *fmt, va_list args)
{
    va_list copy;
    va_copy(copy, args);
    int ret = 0;
    if (s_orig_vprintf) ret = s_orig_vprintf(fmt, args);
    char buf[LOG_MSG_MAX];
    int n = vsnprintf(buf, sizeof(buf), fmt, copy);
    va_end(copy);
    if (n > 0) {
        char lv = 'I';
        if (buf[0] == 'E' || buf[0] == 'W' || buf[0] == 'I' || buf[0] == 'D' || buf[0] == 'V') {
            lv = buf[0];
        }
        log_push(lv, buf);
    }
    return ret;
}

static void log_init(void)
{
    s_log_lock = xSemaphoreCreateMutex();
    s_orig_vprintf = esp_log_set_vprintf(log_vprintf);
}

static void apply_json(rid_config_t *cfg, const char *json)
{
    cJSON *root = cJSON_Parse(json);
    if (!root) return;

    cJSON *item;

    item = cJSON_GetObjectItem(root, "uas_id");
    if (cJSON_IsString(item)) { strncpy(cfg->uas_id, item->valuestring, ESP_RID_MAX_STR_LEN); cfg->uas_id[ESP_RID_MAX_STR_LEN] = '\0'; }
    item = cJSON_GetObjectItem(root, "operator_id");
    if (cJSON_IsString(item)) { strncpy(cfg->operator_id, item->valuestring, ESP_RID_MAX_STR_LEN); cfg->operator_id[ESP_RID_MAX_STR_LEN] = '\0'; }
    item = cJSON_GetObjectItem(root, "self_id_text");
    if (cJSON_IsString(item)) { strncpy(cfg->self_id_text, item->valuestring, ESP_RID_MAX_STR_LEN); cfg->self_id_text[ESP_RID_MAX_STR_LEN] = '\0'; }
    item = cJSON_GetObjectItem(root, "uas_id_2");
    if (cJSON_IsString(item)) { strncpy(cfg->uas_id_2, item->valuestring, ESP_RID_MAX_STR_LEN); cfg->uas_id_2[ESP_RID_MAX_STR_LEN] = '\0'; }

    item = cJSON_GetObjectItem(root, "id_type");
    if (cJSON_IsNumber(item)) cfg->id_type = (uint8_t)item->valueint;
    item = cJSON_GetObjectItem(root, "ua_type");
    if (cJSON_IsNumber(item)) cfg->ua_type = (uint8_t)item->valueint;
    item = cJSON_GetObjectItem(root, "id_type_2");
    if (cJSON_IsNumber(item)) cfg->id_type_2 = (uint8_t)item->valueint;
    item = cJSON_GetObjectItem(root, "ua_type_2");
    if (cJSON_IsNumber(item)) cfg->ua_type_2 = (uint8_t)item->valueint;

    item = cJSON_GetObjectItem(root, "protocol");
    if (cJSON_IsNumber(item)) {
        int p = item->valueint;
        if (p >= 1 && p <= 4) cfg->protocol = (rid_protocol_t)p;
        else cfg->protocol = RID_PROTOCOL_AUTO;
    }

    item = cJSON_GetObjectItem(root, "tx_modes");
    if (cJSON_IsNumber(item)) cfg->tx_modes = (uint8_t)item->valueint;
    item = cJSON_GetObjectItem(root, "wifi_channel");
    if (cJSON_IsNumber(item) && item->valueint >= 1 && item->valueint <= 13)
        cfg->wifi_channel = (uint8_t)item->valueint;
    item = cJSON_GetObjectItem(root, "webserver_en");
    if (cJSON_IsNumber(item)) cfg->webserver_en = (uint8_t)item->valueint;
    item = cJSON_GetObjectItem(root, "mavlink_sysid");
    if (cJSON_IsNumber(item)) cfg->mavlink_sysid = (uint8_t)item->valueint;
    item = cJSON_GetObjectItem(root, "bcast_powerup");
    if (cJSON_IsNumber(item)) cfg->bcast_powerup = (uint8_t)item->valueint;
    item = cJSON_GetObjectItem(root, "options");
    if (cJSON_IsNumber(item)) cfg->options = (uint16_t)item->valueint;

    item = cJSON_GetObjectItem(root, "lock_level");
    if (cJSON_IsNumber(item)) {
        int iv = item->valueint;
        if (iv >= 2) {
            uint8_t efuse_data[4] = {0};
            esp_efuse_read_block(EFUSE_BLK3, efuse_data, 0, 32);
            uint32_t magic = (uint32_t)efuse_data[0] | ((uint32_t)efuse_data[1] << 8)
                           | ((uint32_t)efuse_data[2] << 16) | ((uint32_t)efuse_data[3] << 24);
            if (magic != EFUSE_LOCK_MAGIC) {
                uint32_t val = EFUSE_LOCK_MAGIC;
                esp_err_t err = esp_efuse_write_block(EFUSE_BLK3, &val, 0, 32);
                if (err == ESP_OK) ESP_LOGI(TAG, "eFuse permanent lock burned");
                else ESP_LOGE(TAG, "eFuse write failed: %s", esp_err_to_name(err));
            }
            cfg->lock_level = 2;
        } else if (iv >= 1) {
            cfg->lock_level = (int8_t)iv;
        } else {
            cfg->lock_level = 0;
        }
    }

    item = cJSON_GetObjectItem(root, "led_r_gpio");
    if (cJSON_IsNumber(item)) cfg->led_r_gpio = (int8_t)item->valueint;
    item = cJSON_GetObjectItem(root, "led_g_gpio");
    if (cJSON_IsNumber(item)) cfg->led_g_gpio = (int8_t)item->valueint;
    item = cJSON_GetObjectItem(root, "led_b_gpio");
    if (cJSON_IsNumber(item)) cfg->led_b_gpio = (int8_t)item->valueint;

    item = cJSON_GetObjectItem(root, "uart_port");
    if (cJSON_IsNumber(item)) cfg->uart_port = (uint8_t)item->valueint;
    item = cJSON_GetObjectItem(root, "tx_pin");
    if (cJSON_IsNumber(item)) cfg->tx_pin = (uint8_t)item->valueint;
    item = cJSON_GetObjectItem(root, "rx_pin");
    if (cJSON_IsNumber(item)) cfg->rx_pin = (uint8_t)item->valueint;

    item = cJSON_GetObjectItem(root, "baud_rate");
    if (cJSON_IsNumber(item) && item->valueint > 0) cfg->baud_rate = (uint32_t)item->valueint;

    item = cJSON_GetObjectItem(root, "wifi_power_dbm");
    if (cJSON_IsNumber(item) && item->valuedouble >= 2 && item->valuedouble <= 20) cfg->wifi_power_dbm = (float)item->valuedouble;
    item = cJSON_GetObjectItem(root, "wifi_bcn_rate_hz");
    if (cJSON_IsNumber(item) && item->valuedouble >= 0 && item->valuedouble <= 5) cfg->wifi_bcn_rate_hz = (float)item->valuedouble;
    item = cJSON_GetObjectItem(root, "wifi_nan_rate_hz");
    if (cJSON_IsNumber(item) && item->valuedouble >= 0 && item->valuedouble <= 5) cfg->wifi_nan_rate_hz = (float)item->valuedouble;
    item = cJSON_GetObjectItem(root, "ble4_rate_hz");
    if (cJSON_IsNumber(item) && item->valuedouble >= 0 && item->valuedouble <= 5) cfg->ble4_rate_hz = (float)item->valuedouble;
    item = cJSON_GetObjectItem(root, "ble4_power_dbm");
    if (cJSON_IsNumber(item) && item->valuedouble >= -27 && item->valuedouble <= 18) cfg->ble4_power_dbm = (float)item->valuedouble;
    item = cJSON_GetObjectItem(root, "ble5_rate_hz");
    if (cJSON_IsNumber(item) && item->valuedouble >= 0 && item->valuedouble <= 5) cfg->ble5_rate_hz = (float)item->valuedouble;
    item = cJSON_GetObjectItem(root, "ble5_power_dbm");
    if (cJSON_IsNumber(item) && item->valuedouble >= -27 && item->valuedouble <= 18) cfg->ble5_power_dbm = (float)item->valuedouble;

    item = cJSON_GetObjectItem(root, "operator_lat");
    if (cJSON_IsNumber(item)) cfg->operator_lat = item->valuedouble;
    item = cJSON_GetObjectItem(root, "operator_lon");
    if (cJSON_IsNumber(item)) cfg->operator_lon = item->valuedouble;
    item = cJSON_GetObjectItem(root, "operator_alt");
    if (cJSON_IsNumber(item)) cfg->operator_alt = (float)item->valuedouble;

    item = cJSON_GetObjectItem(root, "wifi_ssid");
    if (cJSON_IsString(item)) { strncpy(cfg->wifi_ssid, item->valuestring, ESP_RID_MAX_STR_LEN); cfg->wifi_ssid[ESP_RID_MAX_STR_LEN] = '\0'; }
    item = cJSON_GetObjectItem(root, "wifi_password");
    if (cJSON_IsString(item)) { strncpy(cfg->wifi_password, item->valuestring, ESP_RID_MAX_STR_LEN); cfg->wifi_password[ESP_RID_MAX_STR_LEN] = '\0'; }

    item = cJSON_GetObjectItem(root, "ws2812_gpio");
    if (cJSON_IsNumber(item)) cfg->ws2812_gpio = (int8_t)item->valueint;
    item = cJSON_GetObjectItem(root, "ws2812_brightness");
    if (cJSON_IsNumber(item)) cfg->ws2812_brightness = (uint8_t)item->valueint;

    char pin_name[20], pat_name[20], ph_name[20];
    for (int i = 0; i < 5; i++) {
        snprintf(pin_name, sizeof(pin_name), "lighting_pin_%d", i);
        snprintf(pat_name, sizeof(pat_name), "lighting_pattern_%d", i);
        snprintf(ph_name, sizeof(ph_name), "lighting_phase_%d", i);
        item = cJSON_GetObjectItem(root, pin_name);
        if (cJSON_IsNumber(item)) cfg->lighting_pins[i] = (int8_t)item->valueint;
        item = cJSON_GetObjectItem(root, pat_name);
        if (cJSON_IsNumber(item)) cfg->lighting_patterns[i] = (uint8_t)item->valueint;
        item = cJSON_GetObjectItem(root, ph_name);
        if (cJSON_IsNumber(item)) cfg->lighting_phase_offsets[i] = (int16_t)item->valueint;
    }

    item = cJSON_GetObjectItem(root, "dronecan_rx_gpio");
    if (cJSON_IsNumber(item)) cfg->dronecan_rx_gpio = (int8_t)item->valueint;
    item = cJSON_GetObjectItem(root, "dronecan_tx_gpio");
    if (cJSON_IsNumber(item)) cfg->dronecan_tx_gpio = (int8_t)item->valueint;
    item = cJSON_GetObjectItem(root, "dronecan_bitrate");
    if (cJSON_IsNumber(item) && item->valueint > 0) cfg->dronecan_bitrate = (uint32_t)item->valueint;

    item = cJSON_GetObjectItem(root, "mavlink_usb_enable");
    if (cJSON_IsNumber(item)) cfg->mavlink_usb_enable = (bool)item->valueint;

    item = cJSON_GetObjectItem(root, "ota_trigger_gpio");
    if (cJSON_IsNumber(item)) cfg->ota_trigger_gpio = (int8_t)item->valueint;

    item = cJSON_GetObjectItem(root, "start_delay_ms");
    if (cJSON_IsNumber(item) && item->valueint >= 0) cfg->start_delay_ms = (uint32_t)item->valueint;

    item = cJSON_GetObjectItem(root, "auth_private_key");
    if (cJSON_IsString(item)) { strncpy(cfg->auth_private_key, item->valuestring, sizeof(cfg->auth_private_key) - 1); cfg->auth_private_key[sizeof(cfg->auth_private_key) - 1] = '\0'; }

    char kname[16];
    for (int i = 1; i <= ESP_RID_NUM_KEYS; i++) {
        snprintf(kname, sizeof(kname), "public_key_%d", i);
        item = cJSON_GetObjectItem(root, kname);
        if (cJSON_IsString(item)) { strncpy(cfg->public_keys[i - 1], item->valuestring, ESP_RID_MAX_KEY_LEN); cfg->public_keys[i - 1][ESP_RID_MAX_KEY_LEN] = '\0'; }
    }

    cJSON_Delete(root);
}

static void config_to_json(const rid_config_t *c, char *buf, size_t sz)
{
    int off = 0;
    off += snprintf(buf + off, sz - off,
        "{"
        "\"protocol\":%u,"
        "\"uas_id\":\"%s\",\"id_type\":%u,\"ua_type\":%u,\"operator_id\":\"%s\",\"self_id_text\":\"%s\","
        "\"uas_id_2\":\"%s\",\"id_type_2\":%u,\"ua_type_2\":%u,"
        "\"tx_modes\":%u,\"wifi_channel\":%u,\"wifi_power_dbm\":%.1f,"
        "\"wifi_bcn_rate_hz\":%.1f,\"wifi_nan_rate_hz\":%.1f,"
        "\"ble4_rate_hz\":%.1f,\"ble4_power_dbm\":%.1f,"
        "\"ble5_rate_hz\":%.1f,\"ble5_power_dbm\":%.1f,"
        "\"wifi_ssid\":\"%s\",\"wifi_password\":\"%s\",\"webserver_en\":%u,"
        "\"baud_rate\":%lu,\"mavlink_sysid\":%u,\"bcast_powerup\":%u,"
        "\"operator_lat\":%.6f,\"operator_lon\":%.6f,\"operator_alt\":%.1f,"
        "\"options\":%u,\"lock_level\":%d,"
        "\"led_r_gpio\":%d,\"led_g_gpio\":%d,\"led_b_gpio\":%d,"
        "\"uart_port\":%u,\"tx_pin\":%u,\"rx_pin\":%u,"
        "\"ws2812_gpio\":%d,\"ws2812_brightness\":%u,"
        "\"dronecan_rx_gpio\":%d,\"dronecan_tx_gpio\":%d,\"dronecan_bitrate\":%lu,"
        "\"mavlink_usb_enable\":%s,\"ota_trigger_gpio\":%d,"
        "\"start_delay_ms\":%lu,"
        "\"public_key_1\":\"%s\",\"public_key_2\":\"%s\","
        "\"public_key_3\":\"%s\",\"public_key_4\":\"%s\",\"public_key_5\":\"%s\","
        "\"lighting_pin_0\":%d,\"lighting_pin_1\":%d,\"lighting_pin_2\":%d,\"lighting_pin_3\":%d,\"lighting_pin_4\":%d,"
        "\"lighting_pattern_0\":%u,\"lighting_pattern_1\":%u,\"lighting_pattern_2\":%u,\"lighting_pattern_3\":%u,\"lighting_pattern_4\":%u,"
        "\"lighting_phase_0\":%d,\"lighting_phase_1\":%d,\"lighting_phase_2\":%d,\"lighting_phase_3\":%d,\"lighting_phase_4\":%d"
        "}",
        (unsigned)c->protocol,
        c->uas_id, c->id_type, c->ua_type, c->operator_id, c->self_id_text,
        c->uas_id_2, c->id_type_2, c->ua_type_2,
        c->tx_modes, c->wifi_channel, (double)c->wifi_power_dbm,
        (double)c->wifi_bcn_rate_hz, (double)c->wifi_nan_rate_hz,
        (double)c->ble4_rate_hz, (double)c->ble4_power_dbm,
        (double)c->ble5_rate_hz, (double)c->ble5_power_dbm,
        c->wifi_ssid, c->wifi_password, c->webserver_en,
        (unsigned long)c->baud_rate, c->mavlink_sysid, c->bcast_powerup,
        c->operator_lat, c->operator_lon, (double)c->operator_alt,
        c->options, c->lock_level,
        c->led_r_gpio, c->led_g_gpio, c->led_b_gpio,
        c->uart_port, c->tx_pin, c->rx_pin,
        c->ws2812_gpio, c->ws2812_brightness,
        c->dronecan_rx_gpio, c->dronecan_tx_gpio, (unsigned long)c->dronecan_bitrate,
        c->mavlink_usb_enable ? "true" : "false", c->ota_trigger_gpio,
        (unsigned long)c->start_delay_ms,
        c->public_keys[0], c->public_keys[1],
        c->public_keys[2], c->public_keys[3], c->public_keys[4],
        c->lighting_pins[0], c->lighting_pins[1], c->lighting_pins[2], c->lighting_pins[3], c->lighting_pins[4],
        c->lighting_patterns[0], c->lighting_patterns[1], c->lighting_patterns[2], c->lighting_patterns[3], c->lighting_patterns[4],
        c->lighting_phase_offsets[0], c->lighting_phase_offsets[1], c->lighting_phase_offsets[2], c->lighting_phase_offsets[3], c->lighting_phase_offsets[4]);
}

static void state_to_json(const rid_state_t *s, char *buf, size_t sz)
{
    snprintf(buf, sz,
        "{"
        "\"fw_version\":\"%s\",\"protocol\":%d,\"gps_valid\":%s,\"lat\":%.6f,\"lon\":%.6f,"
        "\"alt\":%.1f,\"speed\":%.1f,\"heading\":%d,\"satellites\":%u,\"fix_type\":%u,"
        "\"tx_total\":%lu,\"tx_wifi_bcn\":%lu,\"tx_wifi_nan\":%lu,"
        "\"tx_ble4\":%lu,\"tx_ble5\":%lu,"
        "\"takeoff_captured\":%s,\"takeoff_lat\":%.6f,\"takeoff_lon\":%.6f,\"takeoff_alt\":%.1f,"
        "\"uptime_ms\":%lu"
        "}",
        ESP_RID_VERSION,
        (int)s->active_protocol, s->gps_valid ? "true" : "false",
        s->gps.latitude, s->gps.longitude,
        (double)s->gps.altitude_msl, (double)s->gps.speed,
        s->gps.heading, s->gps.satellites, s->gps.fix_type,
        (unsigned long)s->transmissions_count,
        (unsigned long)s->wifi_bcn_count, (unsigned long)s->wifi_nan_count,
        (unsigned long)s->ble4_count, (unsigned long)s->ble5_count,
        s->takeoff_captured ? "true" : "false",
        s->takeoff_lat, s->takeoff_lon, (double)s->takeoff_alt,
        (unsigned long)s->last_update_ms);
}

static bool verify_signed_body(const char *body, const char *sig_b64, const rid_config_t *cfg)
{
    return rid_security_verify_signed_body(body, sig_b64, cfg);
}

static esp_err_t handle_get_config(httpd_req_t *req)
{
    rid_config_t cfg;
    esp_rid_get_config(&cfg);
    char buf[BUF_SIZE];
    config_to_json(&cfg, buf, sizeof(buf));
    httpd_resp_set_type(req, "application/json");
    httpd_resp_send(req, buf, strlen(buf));
    return ESP_OK;
}

static esp_err_t handle_post_config(httpd_req_t *req)
{
    char *body = (char *)malloc(MAX_POST);
    if (!body) { httpd_resp_send_500(req); return ESP_FAIL; }

    int ret = httpd_req_recv(req, body, MAX_POST - 1);
    if (ret <= 0) { free(body); httpd_resp_send_500(req); return ESP_FAIL; }
    body[ret] = '\0';

    if (get_lock_level() >= 1) {
        if (!sig_rate_check()) {
            free(body);
            httpd_resp_set_type(req, "application/json");
            httpd_resp_send(req, "{\"status\":\"rate_limited\"}", 23);
            return ESP_OK;
        }

        char sig_hdr[512] = {0};
        size_t hdr_len = httpd_req_get_hdr_value_len(req, "X-Signature");
        if (hdr_len > 0 && hdr_len < sizeof(sig_hdr)) {
            httpd_req_get_hdr_value_str(req, "X-Signature", sig_hdr, sizeof(sig_hdr));
        }

        rid_config_t cfg;
        esp_rid_get_config(&cfg);

        if (!verify_signed_body(body, sig_hdr, &cfg)) {
            sig_rate_record_fail();
            free(body);
            httpd_resp_set_type(req, "application/json");
            httpd_resp_send(req, "{\"status\":\"invalid_signature\"}", 33);
            return ESP_OK;
        }
    }

    rid_config_t cfg;
    esp_rid_get_config(&cfg);
    apply_json(&cfg, body);
    free(body);
    esp_rid_set_config(&cfg);

    httpd_resp_set_type(req, "application/json");
    httpd_resp_send(req, "{\"status\":\"ok\"}", 15);
    return ESP_OK;
}

static esp_err_t handle_get_status(httpd_req_t *req)
{
    rid_state_t state;
    esp_rid_get_state(&state);
    char buf[BUF_SIZE];
    state_to_json(&state, buf, sizeof(buf));
    httpd_resp_set_type(req, "application/json");
    httpd_resp_send(req, buf, strlen(buf));
    return ESP_OK;
}

static esp_err_t handle_index(httpd_req_t *req)
{
    httpd_resp_set_type(req, "text/html");
    httpd_resp_send(req, config_html_start, config_html_size);
    return ESP_OK;
}

static esp_err_t handle_factory_reset(httpd_req_t *req)
{
    if (get_lock_level() >= 1) {
        if (!sig_rate_check()) {
            httpd_resp_set_type(req, "application/json");
            httpd_resp_send(req, "{\"status\":\"rate_limited\"}", 23);
            return ESP_OK;
        }

        char sig_hdr[512] = {0};
        size_t hdr_len = httpd_req_get_hdr_value_len(req, "X-Signature");
        if (hdr_len > 0 && hdr_len < sizeof(sig_hdr)) {
            httpd_req_get_hdr_value_str(req, "X-Signature", sig_hdr, sizeof(sig_hdr));
        }
        rid_config_t cfg;
        esp_rid_get_config(&cfg);
        if (!verify_signed_body("factory_reset", sig_hdr, &cfg)) {
            sig_rate_record_fail();
            httpd_resp_set_type(req, "application/json");
            httpd_resp_send(req, "{\"status\":\"invalid_signature\"}", 33);
            return ESP_OK;
        }
    }
    esp_rid_factory_reset();
    httpd_resp_set_type(req, "application/json");
    httpd_resp_send(req, "{\"status\":\"reset\"}", 18);
    esp_restart();
    return ESP_OK;
}

static void bytes_to_hex(const uint8_t *bytes, size_t len, char *out)
{
    rid_security_bytes_to_hex(bytes, len, out);
}

static bool hex_to_bytes(const char *hex, uint8_t *out, size_t out_len)
{
    return rid_security_hex_to_bytes(hex, out, out_len);
}

static esp_err_t handle_ota(httpd_req_t *req)
{
    if (get_lock_level() >= 2) {
        httpd_resp_set_type(req, "application/json");
        httpd_resp_send(req, "{\"status\":\"locked\"}", 20);
        return ESP_OK;
    }

    /* Read optional X-Expected-SHA256 header */
    char expected_hex[65] = {0};
    bool has_expected = false;
    size_t hdr_len = httpd_req_get_hdr_value_len(req, "X-Expected-SHA256");
    if (hdr_len > 0 && hdr_len <= 64) {
        httpd_req_get_hdr_value_str(req, "X-Expected-SHA256", expected_hex, sizeof(expected_hex));
        has_expected = true;
    }

    esp_ota_handle_t ota_handle = 0;
    const esp_partition_t *ota_part = esp_ota_get_next_update_partition(NULL);
    if (!ota_part) {
        httpd_resp_send_500(req);
        return ESP_FAIL;
    }

    char buf[1024];
    int ret;
    esp_err_t err = esp_ota_begin(ota_part, OTA_SIZE_UNKNOWN, &ota_handle);
    if (err != ESP_OK) {
        httpd_resp_sendstr(req, "OTA begin failed");
        return ESP_FAIL;
    }

    psa_hash_operation_t sha_ctx = psa_hash_operation_init();
    if (psa_hash_setup(&sha_ctx, PSA_ALG_SHA_256) != PSA_SUCCESS) {
        esp_ota_abort(ota_handle);
        httpd_resp_sendstr(req, "OTA failed: SHA-256 setup error");
        return ESP_FAIL;
    }

    led_status_set_state(RID_LED_OTA);

    while ((ret = httpd_req_recv(req, buf, sizeof(buf))) > 0) {
        if (psa_hash_update(&sha_ctx, (const unsigned char *)buf, ret) != PSA_SUCCESS) {
            psa_hash_abort(&sha_ctx);
            esp_ota_abort(ota_handle);
            httpd_resp_send_500(req);
            return ESP_FAIL;
        }
        if (esp_ota_write(ota_handle, buf, ret) != ESP_OK) {
            psa_hash_abort(&sha_ctx);
            esp_ota_abort(ota_handle);
            httpd_resp_send_500(req);
            return ESP_FAIL;
        }
    }

    uint8_t hash[32];
    size_t hash_len;
    if (psa_hash_finish(&sha_ctx, hash, sizeof(hash), &hash_len) != PSA_SUCCESS) {
        esp_ota_abort(ota_handle);
        httpd_resp_sendstr(req, "OTA failed: SHA-256 finalize error");
        return ESP_FAIL;
    }

    if (!has_expected) {
        esp_ota_abort(ota_handle);
        httpd_resp_set_type(req, "text/plain");
        httpd_resp_sendstr(req, "OTA rejected: X-Expected-SHA256 header required");
        return ESP_FAIL;
    }

    {
        uint8_t expected_hash[32];
        if (!hex_to_bytes(expected_hex, expected_hash, 32) ||
            memcmp(hash, expected_hash, 32) != 0) {
            char got_hex[65];
            bytes_to_hex(hash, 32, got_hex);
            esp_ota_abort(ota_handle);
            char err_msg[192];
            snprintf(err_msg, sizeof(err_msg),
                "SHA-256 mismatch\nexpected: %s\nreceived: %s",
                expected_hex, got_hex);
            httpd_resp_set_type(req, "text/plain");
            httpd_resp_sendstr(req, err_msg);
            return ESP_FAIL;
        }
    }

    if (esp_ota_end(ota_handle) != ESP_OK || esp_ota_set_boot_partition(ota_part) != ESP_OK) {
        httpd_resp_sendstr(req, "OTA finalize failed");
        return ESP_FAIL;
    }

    httpd_resp_set_type(req, "text/plain");
    httpd_resp_sendstr(req, "OTA OK, rebooting...");
    esp_restart();
    return ESP_OK;
}

static esp_err_t handle_get_logs(httpd_req_t *req)
{
    char *buf = (char *)malloc(4096);
    if (!buf) { httpd_resp_send_500(req); return ESP_FAIL; }
    int off = 0;
    off += snprintf(buf + off, 4096 - off, "[");
    if (s_log_lock && xSemaphoreTake(s_log_lock, pdMS_TO_TICKS(50)) == pdTRUE) {
        int n = s_log_count;
        int start = (n < LOG_RING_MAX) ? 0 : s_log_head;
        for (int i = 0; i < n; i++) {
            int idx = (start + i) % LOG_RING_MAX;
            log_entry_t *e = &s_log_ring[idx];
            char lvstr[2] = { e->level, '\0' };
            char escaped[LOG_MSG_MAX * 2];
            int eo = 0;
            for (int si = 0; e->msg[si] && eo < (int)sizeof(escaped) - 4; si++) {
                char c = e->msg[si];
                if (c == '"' || c == '\\') { escaped[eo++] = '\\'; escaped[eo++] = c; }
                else if (c == '\n') { escaped[eo++] = '\\'; escaped[eo++] = 'n'; }
                else if (c == '\r') { escaped[eo++] = '\\'; escaped[eo++] = 'r'; }
                else if (c == '\t') { escaped[eo++] = '\\'; escaped[eo++] = 't'; }
                else if (c < 0x20) continue;
                else escaped[eo++] = c;
            }
            escaped[eo] = '\0';
            if (i > 0) off += snprintf(buf + off, 4096 - off, ",");
            off += snprintf(buf + off, 4096 - off,
                "{\"t\":%lu,\"l\":\"%s\",\"m\":\"%s\"}",
                (unsigned long)e->time_ms, lvstr, escaped);
            if (off >= 4096 - 128) { off = 4096 - 128; break; }
        }
        xSemaphoreGive(s_log_lock);
    }
    off += snprintf(buf + off, 4096 - off, "]");
    httpd_resp_set_type(req, "application/json");
    httpd_resp_send(req, buf, strlen(buf));
    free(buf);
    return ESP_OK;
}

static esp_err_t handle_post_command(httpd_req_t *req)
{
    int locked = get_lock_level();

    char body[256];
    int ret = httpd_req_recv(req, body, sizeof(body) - 1);
    if (ret <= 0) { httpd_resp_send_500(req); return ESP_FAIL; }
    body[ret] = '\0';

    /* strip quotes if wrapped */
    char *cmd = body;
    while (*cmd == ' ' || *cmd == '\t') cmd++;
    if (cmd[0] == '"') { cmd++; char *e = strchr(cmd, '"'); if (e) *e = '\0'; }

    /* Check if command needs auth when locked */
    bool needs_auth = (strcmp(cmd, "restart") == 0 || strcmp(cmd, "reboot") == 0 ||
                       strcmp(cmd, "reset") == 0 || strcmp(cmd, "factory") == 0);

    if (locked >= 1 && needs_auth) {
        char sig_hdr[512] = {0};
        size_t hdr_len = httpd_req_get_hdr_value_len(req, "X-Signature");
        if (hdr_len > 0 && hdr_len < sizeof(sig_hdr)) {
            httpd_req_get_hdr_value_str(req, "X-Signature", sig_hdr, sizeof(sig_hdr));
        }
        rid_config_t cfg;
        esp_rid_get_config(&cfg);
        if (!verify_signed_body(cmd, sig_hdr, &cfg)) {
            httpd_resp_set_type(req, "application/json");
            httpd_resp_send(req, "{\"status\":\"invalid_signature\"}", 33);
            return ESP_OK;
        }
    }

    esp_err_t res = ESP_OK;
    const char *reply = "ok";

    if (strcmp(cmd, "restart") == 0 || strcmp(cmd, "reboot") == 0) {
        reply = "restarting";
        httpd_resp_set_type(req, "application/json");
        httpd_resp_send(req, "{\"status\":\"restarting\"}", 22);
        esp_restart();
        return ESP_OK;
    } else if (strcmp(cmd, "reset") == 0 || strcmp(cmd, "factory") == 0) {
        esp_rid_factory_reset();
        reply = "factory reset, restarting";
        httpd_resp_set_type(req, "application/json");
        httpd_resp_send(req, "{\"status\":\"reset\"}", 18);
        esp_restart();
        return ESP_OK;
    } else if (strcmp(cmd, "status") == 0) {
        rid_state_t st;
        esp_rid_get_state(&st);
        char tmp[512];
        state_to_json(&st, tmp, sizeof(tmp));
        httpd_resp_set_type(req, "application/json");
        httpd_resp_send(req, tmp, strlen(tmp));
        return ESP_OK;
    } else {
        /* forward unknown command as log entry */
        ESP_LOGI("CMD", "Received command: %s", cmd);
        reply = "unknown command";
    }

    char resp[128];
    snprintf(resp, sizeof(resp), "{\"status\":\"%s\"}", reply);
    httpd_resp_set_type(req, "application/json");
    httpd_resp_send(req, resp, strlen(resp));
    return res;
}

static const httpd_uri_t uri_index = { "/", HTTP_GET, handle_index, NULL };
static const httpd_uri_t uri_get_cfg = { "/api/config", HTTP_GET, handle_get_config, NULL };
static const httpd_uri_t uri_set_cfg = { "/api/config", HTTP_POST, handle_post_config, NULL };
static const httpd_uri_t uri_status = { "/api/status", HTTP_GET, handle_get_status, NULL };
static const httpd_uri_t uri_reset = { "/api/reset", HTTP_POST, handle_factory_reset, NULL };
static const httpd_uri_t uri_ota = { "/ota", HTTP_POST, handle_ota, NULL };
static const httpd_uri_t uri_logs = { "/api/logs", HTTP_GET, handle_get_logs, NULL };
static const httpd_uri_t uri_cmd = { "/api/command", HTTP_POST, handle_post_command, NULL };

void web_config_init(void)
{
    log_init();
    httpd_config_t config = HTTPD_DEFAULT_CONFIG();
    config.server_port = 80;
    config.max_uri_handlers = 16;
    config.lru_purge_enable = true;

    if (httpd_start(&g_server, &config) == ESP_OK) {
        httpd_register_uri_handler(g_server, &uri_index);
        httpd_register_uri_handler(g_server, &uri_get_cfg);
        httpd_register_uri_handler(g_server, &uri_set_cfg);
        httpd_register_uri_handler(g_server, &uri_status);
        httpd_register_uri_handler(g_server, &uri_reset);
        httpd_register_uri_handler(g_server, &uri_ota);
        httpd_register_uri_handler(g_server, &uri_logs);
        httpd_register_uri_handler(g_server, &uri_cmd);
        ESP_LOGI(TAG, "Web server started on port 80");
    } else {
        ESP_LOGE(TAG, "Failed to start web server");
    }
}
