#include <stdio.h>
#include <string.h>
#include "esp_log.h"
#include "soc/soc_caps.h"
#if defined(CONFIG_BT_BLUEDROID_ENABLED) && defined(SOC_BT_SUPPORTED)
#include "esp_bt.h"
#include "esp_bt_main.h"
#include "esp_gap_ble_api.h"
#endif
#include "ble_tx.h"
#include "opendroneid.h"

#define TAG "BLE_TX"

#define RID_SERVICE_UUID 0xFFFA

static bool g_initialized = false;
#if defined(CONFIG_BT_BLUEDROID_ENABLED) && defined(SOC_BT_SUPPORTED)
static ODID_UAS_Data g_uas_data;
static uint8_t g_adv_data[254];

static ODID_Horizontal_accuracy_t ble_horiz_acc(uint8_t fix_type, uint8_t satellites)
{
    if (fix_type >= 4 && satellites >= 15) return ODID_HOR_ACC_1_METER;
    if (fix_type >= 4 && satellites >= 10) return ODID_HOR_ACC_3_METER;
    if (fix_type >= 3) return ODID_HOR_ACC_10_METER;
    return ODID_HOR_ACC_30_METER;
}

static ODID_Vertical_accuracy_t ble_vert_acc(uint8_t fix_type, uint8_t satellites)
{
    if (fix_type >= 4 && satellites >= 15) return ODID_VER_ACC_3_METER;
    if (fix_type >= 4 && satellites >= 10) return ODID_VER_ACC_10_METER;
    if (fix_type >= 3) return ODID_VER_ACC_25_METER;
    return ODID_VER_ACC_45_METER;
}

static void prepare_uas_data(rid_gps_data_t *gps, rid_identity_t *identity)
{
    memset(&g_uas_data, 0, sizeof(g_uas_data));

    g_uas_data.BasicIDValid[0] = 1;
    g_uas_data.BasicID[0].IDType = (ODID_idtype_t)identity->id_type;
    g_uas_data.BasicID[0].UAType = (ODID_uatype_t)identity->ua_type;
    strncpy((char *)g_uas_data.BasicID[0].UASID, identity->uas_id, ODID_ID_SIZE);

    if (identity->uas_id_2[0] != '\0') {
        g_uas_data.BasicIDValid[1] = 1;
        g_uas_data.BasicID[1].IDType = (ODID_idtype_t)identity->id_type_2;
        g_uas_data.BasicID[1].UAType = (ODID_uatype_t)identity->ua_type_2;
        strncpy((char *)g_uas_data.BasicID[1].UASID, identity->uas_id_2, ODID_ID_SIZE);
    }

    g_uas_data.LocationValid = 1;
    g_uas_data.Location.Latitude = gps->latitude;
    g_uas_data.Location.Longitude = gps->longitude;
    g_uas_data.Location.AltitudeGeo = gps->altitude_msl;
    g_uas_data.Location.Height = gps->altitude_relative;
    g_uas_data.Location.SpeedHorizontal = gps->speed;
    g_uas_data.Location.SpeedVertical = gps->speed_vertical;
    g_uas_data.Location.Direction = gps->heading;
    g_uas_data.Location.HorizAccuracy = ble_horiz_acc(gps->fix_type, gps->satellites);
    g_uas_data.Location.VertAccuracy = ble_vert_acc(gps->fix_type, gps->satellites);

    if (identity->self_id_text[0] != '\0') {
        g_uas_data.SelfIDValid = 1;
        g_uas_data.SelfID.DescType = ODID_DESC_TYPE_TEXT;
        strncpy((char *)g_uas_data.SelfID.Desc, identity->self_id_text, ODID_STR_SIZE);
    }

    g_uas_data.SystemValid = 1;
    g_uas_data.System.OperatorLatitude = gps->operator_lat;
    g_uas_data.System.OperatorLongitude = gps->operator_lon;

    g_uas_data.OperatorIDValid = 1;
    strncpy((char *)g_uas_data.OperatorID.OperatorId, identity->operator_id, ODID_ID_SIZE);
}

static int build_ble_header(uint8_t *buf, uint16_t buf_size)
{
    uint16_t idx = 0;
    if (idx + 3 > buf_size) return 0;
    buf[idx++] = 2;
    buf[idx++] = 0x01;
    buf[idx++] = 0x06;

    if (idx + 3 > buf_size) return 0;
    buf[idx++] = 3;
    buf[idx++] = 0x03;
    buf[idx++] = 0xFA;
    buf[idx++] = 0xFF;

    return idx;
}

static bool build_legacy_adv(rid_gps_data_t *gps, rid_identity_t *identity, uint8_t *buf, uint16_t buf_size, uint16_t *len)
{
    if (buf_size < 16) return false;
    memset(buf, 0, buf_size);
    uint16_t idx = build_ble_header(buf, buf_size);
    if (idx == 0) return false;

    prepare_uas_data(gps, identity);

    uint8_t pack_buf[ODID_PACK_MAX_MESSAGES * ODID_MESSAGE_SIZE + 8];
    int pack_len = odid_message_build_pack(&g_uas_data, pack_buf, sizeof(pack_buf));
    if (pack_len <= 0) return false;

    /* Send only one complete message per advertising cycle for legacy BLE.
     * ODID_MESSAGE_SIZE = 25 bytes, plus 4-byte service data header = 29 bytes.
     * Legacy BLE advertising data limit is 31 bytes (27 usable after headers). */
    int copy_len = pack_len;
    if (copy_len > (int)(buf_size - idx - 5)) copy_len = buf_size - idx - 5;

    uint8_t adv_idx = idx;
    if (idx + 4 + copy_len > buf_size) return false;
    buf[idx++] = 0;
    buf[idx++] = 0x16;
    buf[idx++] = 0xFA;
    buf[idx++] = 0xFF;

    memcpy(buf + idx, pack_buf, copy_len);
    idx += copy_len;
    buf[adv_idx] = idx - adv_idx - 1;

    *len = idx;
    return true;
}
#endif

void ble_tx_init(void)
{
    if (g_initialized) return;
#if defined(CONFIG_BT_BLUEDROID_ENABLED) && defined(SOC_BT_SUPPORTED)
    esp_bt_controller_mem_release(ESP_BT_MODE_CLASSIC_BT);

    esp_bt_controller_config_t bt_cfg = BT_CONTROLLER_INIT_CONFIG_DEFAULT();
    if (esp_bt_controller_init(&bt_cfg) != ESP_OK) return;
    if (esp_bt_controller_enable(ESP_BT_MODE_BLE) != ESP_OK) return;
    if (esp_bluedroid_init() != ESP_OK) return;
    if (esp_bluedroid_enable() != ESP_OK) return;

    g_initialized = true;
    ESP_LOGI(TAG, "BLE initialized");
#else
    ESP_LOGW(TAG, "BLE not available on this target");
#endif
}

bool ble_tx_transmit_legacy(rid_gps_data_t *gps, rid_identity_t *identity)
{
    if (!g_initialized || !gps || !identity) return false;

#if defined(CONFIG_BT_BLUEDROID_ENABLED) && defined(SOC_BT_SUPPORTED)
    uint16_t len;
    if (!build_legacy_adv(gps, identity, g_adv_data, sizeof(g_adv_data), &len)) return false;

    esp_ble_adv_params_t adv_params = {
        .adv_int_min = 0x100,
        .adv_int_max = 0x100,
        .adv_type = ADV_TYPE_SCAN_IND,
        .channel_map = ADV_CHNL_ALL,
        .own_addr_type = BLE_ADDR_TYPE_RANDOM,
        .peer_addr_type = BLE_ADDR_TYPE_PUBLIC,
        .peer_addr = {0},
        .adv_filter_policy = ADV_FILTER_ALLOW_SCAN_ANY_CON_ANY,
    };

    esp_ble_gap_config_adv_data_raw(g_adv_data, len);
    esp_ble_gap_start_advertising(&adv_params);

    return true;
#else
    return false;
#endif
}

bool ble_tx_transmit_lr(rid_gps_data_t *gps, rid_identity_t *identity)
{
    if (!g_initialized || !gps || !identity) return false;

#if defined(CONFIG_BT_BLUEDROID_ENABLED) && defined(SOC_BT_SUPPORTED) && defined(CONFIG_BT_BLE_50_EXTEND_ADV_EN)
    prepare_uas_data(gps, identity);

    /* Build full ODID pack — extended advertising supports up to 254 bytes */
    uint8_t pack_buf[ODID_PACK_MAX_MESSAGES * ODID_MESSAGE_SIZE + 8];
    int pack_len = odid_message_build_pack(&g_uas_data, pack_buf, sizeof(pack_buf));
    if (pack_len <= 0) return false;

    /* Instance 0: legacy-compatible (1M PHY, visible to BLE 4.2 scanners) */
    {
        esp_ble_gap_ext_adv_params_t ext_params_legacy = {
            .type = ESP_BLE_GAP_SET_EXT_ADV_PROP_NONCONN_NONSCANNABLE_UNDIRECTED,
            .interval_min = 0x100,
            .interval_max = 0x100,
            .channel_map = ADV_CHNL_ALL,
            .own_addr_type = BLE_ADDR_TYPE_RANDOM,
            .primary_phy = ESP_BLE_GAP_PHY_1M,
            .secondary_phy = ESP_BLE_GAP_PHY_1M,
            .scan_req_notif = false,
        };
        esp_ble_gap_ext_adv_set_params(0, &ext_params_legacy);
        esp_ble_gap_config_ext_adv_data_raw(0, pack_len, pack_buf);
        esp_ble_gap_ext_adv_t adv_legacy = { .instance = 0, .duration = 0, .max_events = 0 };
        esp_ble_gap_ext_adv_start(1, &adv_legacy);
    }

    /* Instance 1: long-range (Coded PHY, 200+ m range) */
    {
        esp_ble_gap_ext_adv_params_t ext_params_lr = {
            .type = ESP_BLE_GAP_SET_EXT_ADV_PROP_NONCONN_NONSCANNABLE_UNDIRECTED,
            .interval_min = 0x100,
            .interval_max = 0x100,
            .channel_map = ADV_CHNL_ALL,
            .own_addr_type = BLE_ADDR_TYPE_RANDOM,
            .primary_phy = ESP_BLE_GAP_PHY_CODED,
            .secondary_phy = ESP_BLE_GAP_PHY_CODED,
            .scan_req_notif = false,
        };
        esp_ble_gap_ext_adv_set_params(1, &ext_params_lr);
        esp_ble_gap_config_ext_adv_data_raw(1, pack_len, pack_buf);
        esp_ble_gap_ext_adv_t adv_lr = { .instance = 1, .duration = 0, .max_events = 0 };
        esp_ble_gap_ext_adv_start(1, &adv_lr);
    }

    return true;
#else
    return false;
#endif
}

void ble_tx_set_power(int8_t dbm)
{
#if defined(CONFIG_BT_BLUEDROID_ENABLED) && defined(SOC_BT_SUPPORTED)
    esp_ble_tx_power_set(ESP_BLE_PWR_TYPE_ADV, (esp_power_level_t)((dbm + 12) / 3));
    esp_ble_tx_power_set(ESP_BLE_PWR_TYPE_SCAN, (esp_power_level_t)((dbm + 12) / 3));
    ESP_LOGI(TAG, "BLE TX power set to %d dBm", dbm);
#endif
}
