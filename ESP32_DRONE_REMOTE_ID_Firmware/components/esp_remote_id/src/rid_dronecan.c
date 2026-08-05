#include <string.h>
#include "esp_log.h"
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wcpp"
#include "driver/twai.h"
#pragma GCC diagnostic pop
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "rid_dronecan.h"

#define TAG "DRONECAN"

/*
 * DroneCAN message IDs (simplified):
 *  uavcan.equipment.gnss.Fix2        -> CAN ID 2000 (broadcast)
 *  uavcan.equipment.ahrs.Solution    -> CAN ID 1000 (broadcast)
 *  org.drone_id.Identity             -> CAN ID 8192 (broadcast, custom)
 */

#define DRONECAN_FIX2_ID    2000
#define DRONECAN_AHRS_ID    1000
#define DRONECAN_IDENTITY_ID 8192

static rid_gps_data_t g_last_gps;
static uint32_t g_last_update = 0;
static bool g_active = false;
static bool g_initialized = false;

#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Warray-bounds"
static void decode_fix2(const uint8_t *data, uint8_t len)
{
    /* DroneCAN Fix2 (32 bytes) spans multiple classic-CAN frames, so the
     * len < 32 guard below is never satisfied by a single twai frame
     * (DLC <= 8). The body is unreachable until multi-frame transfer
     * reassembly is implemented; bounds reads are suppressed for -Os. */
    if (len < 32) return;

    int32_t lat_deg_1e7 = (data[0] << 24) | (data[1] << 16) | (data[2] << 8) | data[3];
    int32_t lon_deg_1e7 = (data[4] << 24) | (data[5] << 16) | (data[6] << 8) | data[7];

    g_last_gps.latitude = lat_deg_1e7 / 1.0e7;
    g_last_gps.longitude = lon_deg_1e7 / 1.0e7;

    int32_t height_mm = (data[8] << 24) | (data[9] << 16) | (data[10] << 8) | data[11];
    g_last_gps.altitude_msl = height_mm / 1000.0f;

    int32_t height_ell_mm = (data[12] << 24) | (data[13] << 16) | (data[14] << 8) | data[15];
    g_last_gps.altitude_relative = height_ell_mm / 1000.0f;

    uint16_t speed_2d_cm_s = (data[20] << 8) | data[21];
    g_last_gps.speed = speed_2d_cm_s / 100.0f;

    uint16_t heading_deg_1e2 = (data[22] << 8) | data[23];
    g_last_gps.heading = heading_deg_1e2 / 100;

    uint8_t gnss_fix = data[24];
    if (gnss_fix >= 2) {
        g_last_gps.fix_type = gnss_fix;
        g_last_update = xTaskGetTickCount() * portTICK_PERIOD_MS;
    }

    uint8_t num_sats = data[25];
    g_last_gps.satellites = num_sats;
}
#pragma GCC diagnostic pop

static void decode_ahrs(const uint8_t *data, uint8_t len)
{
    (void)len;
}

static void decode_identity(const uint8_t *data, uint8_t len)
{
    (void)len;
}

bool rid_dronecan_init(int rx_gpio, int tx_gpio, uint32_t bitrate)
{
    if (rx_gpio < 0 || tx_gpio < 0) {
        ESP_LOGW(TAG, "DroneCAN pins not configured");
        return false;
    }

    twai_general_config_t g_config = TWAI_GENERAL_CONFIG_DEFAULT(
        (gpio_num_t)tx_gpio, (gpio_num_t)rx_gpio, TWAI_MODE_NORMAL);
    g_config.rx_queue_len = 10;

    twai_timing_config_t t_config = TWAI_TIMING_CONFIG_1MBITS();
    if (bitrate == 500000)
        t_config = (twai_timing_config_t)TWAI_TIMING_CONFIG_500KBITS();
    else if (bitrate == 250000)
        t_config = (twai_timing_config_t)TWAI_TIMING_CONFIG_250KBITS();

    twai_filter_config_t f_config = TWAI_FILTER_CONFIG_ACCEPT_ALL();

    if (twai_driver_install(&g_config, &t_config, &f_config) != ESP_OK) {
        ESP_LOGE(TAG, "TWAI install failed");
        return false;
    }

    if (twai_start() != ESP_OK) {
        ESP_LOGE(TAG, "TWAI start failed");
        return false;
    }

    g_initialized = true;
    ESP_LOGI(TAG, "DroneCAN init on RX=%d TX=%d %lu bps", rx_gpio, tx_gpio,
             (unsigned long)bitrate);
    return true;
}

bool rid_dronecan_get(rid_gps_data_t *gps)
{
    if (!g_initialized) return false;

    twai_message_t msg;
    while (twai_receive(&msg, 0) == ESP_OK) {
        g_active = true;
        switch (msg.identifier) {
        case DRONECAN_FIX2_ID:
            decode_fix2(msg.data, msg.data_length_code);
            break;
        case DRONECAN_AHRS_ID:
            decode_ahrs(msg.data, msg.data_length_code);
            break;
        case DRONECAN_IDENTITY_ID:
            decode_identity(msg.data, msg.data_length_code);
            break;
        default:
            break;
        }
    }

    if (g_last_update != 0 &&
        (xTaskGetTickCount() * portTICK_PERIOD_MS - g_last_update) < 5000) {
        memcpy(gps, &g_last_gps, sizeof(rid_gps_data_t));
        return true;
    }

    return false;
}

bool rid_dronecan_is_active(void)
{
    return g_active;
}
