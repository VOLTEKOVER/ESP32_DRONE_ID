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
#include "rid_auth.h"

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
    g_uas_data.Location.AltitudeBaro = gps->altitude_baro;
    g_uas_data.Location.SpeedHorizontal = gps->speed;
    g_uas_data.Location.SpeedVertical = gps->speed_vertical;
    g_uas_data.Location.Direction = gps->heading;
    g_uas_data.Location.HorizAccuracy = ble_horiz_acc(gps->fix_type, gps->satellites);
    g_uas_data.Location.VertAccuracy = ble_vert_acc(gps->fix_type, gps->satellites);

    if (identity->self_id_text[0] != '\0') {
        g_uas_data.SelfIDValid = 1;
        g_uas_data.SelfID.DescType = identity->has_self_id
                                         ? (ODID_desctype_t)identity->self_id_desc_type
                                         : ODID_DESC_TYPE_TEXT;
        strncpy((char *)g_uas_data.SelfID.Desc, identity->self_id_text, ODID_STR_SIZE);
    }

    g_uas_data.SystemValid = 1;
    g_uas_data.System.OperatorLatitude = gps->operator_lat;
    g_uas_data.System.OperatorLongitude = gps->operator_lon;
    g_uas_data.System.OperatorAltitudeGeo = gps->operator_alt;

    g_uas_data.OperatorIDValid = 1;
    strncpy((char *)g_uas_data.OperatorID.OperatorId, identity->operator_id, ODID_ID_SIZE);

    /* Authentication: MAVLink-relayed pages take priority, otherwise sign locally */
    uint8_t auth_pages = 0;
    ODID_Auth_data auth[ODID_AUTH_MAX_PAGES];

    if (identity->has_ext_auth && identity->ext_auth_last_page < ODID_AUTH_MAX_PAGES) {
        uint16_t last = identity->ext_auth_last_page;
        uint16_t need = (uint16_t)((1u << (last + 1)) - 1);
        if ((identity->ext_auth_pages_received & need) == need) {
            for (uint16_t p = 0; p <= last; p++) {
                memset(&auth[p], 0, sizeof(ODID_Auth_data));
                auth[p].DataPage = (uint8_t)p;
                auth[p].AuthType = (ODID_authtype_t)identity->ext_auth_type;
                auth[p].LastPageIndex = identity->ext_auth_last_page;
                auth[p].Length = identity->ext_auth_length;
                memcpy(auth[p].AuthData, identity->ext_auth_pages[p],
                       ODID_AUTH_PAGE_NONZERO_DATA_SIZE);
            }
            auth_pages = (uint8_t)(last + 1);
        }
    } else if (rid_auth_enabled()) {
        (void)rid_auth_sign_identity(identity->uas_id, auth, &auth_pages);
    }

    if (auth_pages > 0) {
        uint8_t fixed = 1 + (identity->uas_id_2[0] ? 1 : 0) + 1 +
                        (identity->self_id_text[0] ? 1 : 0) + 1 + 1;
        if (auth_pages <= ODID_PACK_MAX_MESSAGES - fixed) {
            for (uint8_t p = 0; p < auth_pages; p++) {
                g_uas_data.Auth[p] = auth[p];
                g_uas_data.AuthValid[p] = 1;
            }
        }
    }
}

static bool build_legacy_adv(rid_gps_data_t *gps, rid_identity_t *identity, uint8_t *buf, uint16_t buf_size, uint16_t *len)
{
    /* Legacy BLE advertising data is limited to 31 bytes, so exactly one
     * 25-byte ODID message fits per advertisement, sent as a Service Data
     * AD structure on the Remote ID UUID 0xFFFA:
     *   0x1E 0x16 0xFA 0xFF | 0x0D (app code) | counter | 25-byte message
     * Messages are rotated across advertising cycles. */
    if (buf_size < 31) return false;
    memset(buf, 0, buf_size);

    prepare_uas_data(gps, identity);

    static uint8_t rotation = 0;

    /* Count the currently valid messages */
    uint8_t total = 0;
    for (int i = 0; i < ODID_BASIC_ID_MAX_MESSAGES; i++) {
        if (g_uas_data.BasicIDValid[i]) total++;
    }
    if (g_uas_data.LocationValid) total++;
    for (int i = 0; i < ODID_AUTH_MAX_PAGES; i++) {
        if (g_uas_data.AuthValid[i]) total++;
    }
    if (g_uas_data.SelfIDValid) total++;
    if (g_uas_data.SystemValid) total++;
    if (g_uas_data.OperatorIDValid) total++;

    if (total == 0) return false;

    uint8_t target = rotation++ % total;
    uint8_t n = 0;
    bool found = false;
    ODID_Message_encoded msg;

    for (int i = 0; i < ODID_BASIC_ID_MAX_MESSAGES && !found; i++) {
        if (!g_uas_data.BasicIDValid[i]) continue;
        if (n++ == target) {
            found = encodeBasicIDMessage(&msg.basicId, &g_uas_data.BasicID[i]) == ODID_SUCCESS;
            break;
        }
    }
    if (!found && g_uas_data.LocationValid && n++ == target) {
        found = encodeLocationMessage(&msg.location, &g_uas_data.Location) == ODID_SUCCESS;
    }
    for (int i = 0; i < ODID_AUTH_MAX_PAGES && !found; i++) {
        if (!g_uas_data.AuthValid[i]) continue;
        if (n++ == target) {
            found = encodeAuthMessage(&msg.auth, &g_uas_data.Auth[i]) == ODID_SUCCESS;
            break;
        }
    }
    if (!found && g_uas_data.SelfIDValid && n++ == target) {
        found = encodeSelfIDMessage(&msg.selfId, &g_uas_data.SelfID) == ODID_SUCCESS;
    }
    if (!found && g_uas_data.SystemValid && n++ == target) {
        found = encodeSystemMessage(&msg.system, &g_uas_data.System) == ODID_SUCCESS;
    }
    if (!found && g_uas_data.OperatorIDValid && n++ == target) {
        found = encodeOperatorIDMessage(&msg.operatorId, &g_uas_data.OperatorID) == ODID_SUCCESS;
    }

    if (!found) return false;

    /* Service Data AD structure, 31 bytes total */
    buf[0] = 0x1E;              /* length: 30 bytes follow */
    buf[1] = 0x16;              /* Service Data - 16-bit UUID */
    buf[2] = 0xFA;              /* UUID 0xFFFA (little endian) */
    buf[3] = 0xFF;
    buf[4] = 0x0D;              /* ASTM Open Drone ID application code */
    buf[5] = rotation - 1;      /* message counter */
    memcpy(buf + 6, msg.rawData, ODID_MESSAGE_SIZE);

    *len = 31;
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

#if defined(CONFIG_BT_BLE_50_EXTEND_ADV_EN)
    /* esp32s3/esp32c6: the legacy GAP advertising API is not linked in
     * Bluedroid when extended advertising is enabled, so broadcast the
     * legacy-compatible 31-byte adv on the 1M PHY via a dedicated ext adv
     * instance (instances 0/1 are used by the BLE5 long-range path). */
    esp_ble_gap_ext_adv_params_t ext_params = {
        .type = ESP_BLE_GAP_SET_EXT_ADV_PROP_LEGACY_NONCONN,
        .interval_min = 0x100,
        .interval_max = 0x100,
        .channel_map = ADV_CHNL_ALL,
        .own_addr_type = BLE_ADDR_TYPE_RANDOM,
        .primary_phy = ESP_BLE_GAP_PHY_1M,
        .secondary_phy = ESP_BLE_GAP_PHY_1M,
        .scan_req_notif = false,
    };
    esp_ble_gap_ext_adv_set_params(2, &ext_params);
    esp_ble_gap_config_ext_adv_data_raw(2, len, g_adv_data);
    esp_ble_gap_ext_adv_t adv = { .instance = 2, .duration = 0, .max_events = 0 };
    esp_ble_gap_ext_adv_start(1, &adv);
#else
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
#endif

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
    if (dbm > 9) dbm = 9;
    if (dbm < -12) dbm = -12;
    esp_power_level_t level = (esp_power_level_t)((dbm + 12) / 3);
    esp_ble_tx_power_set(ESP_BLE_PWR_TYPE_DEFAULT, level);
    esp_ble_tx_power_set(ESP_BLE_PWR_TYPE_ADV, level);
    esp_ble_tx_power_set(ESP_BLE_PWR_TYPE_SCAN, level);
    ESP_LOGI(TAG, "BLE TX power set to %d dBm", dbm);
#endif
}
