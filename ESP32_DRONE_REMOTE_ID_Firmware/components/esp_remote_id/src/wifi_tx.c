#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include "esp_log.h"
#include "esp_system.h"
#include "esp_random.h"
#include "esp_mac.h"
#include "esp_efuse.h"
#include "esp_wifi.h"
#include "esp_event.h"
#include "nvs_flash.h"
#include "esp_netif.h"
#include "wifi_tx.h"
#include "opendroneid.h"
#include "odid_wifi.h"
#include "esp_remote_id.h"

#define TAG "WIFI_TX"

static bool g_initialized = false;
static uint8_t g_mac[6];
static uint8_t g_message_counter = 0;
static ODID_UAS_Data g_uas_data;
static char g_ssid[33] = "ESP-RID";
static uint8_t g_ssid_len = 7;
static uint8_t g_channel = 6;


static void generate_random_mac(uint8_t mac[6])
{
    for (int i = 0; i < 6; i++) {
        mac[i] = (uint8_t)(esp_random() & 0xFF);
    }
    mac[0] |= 0x02;
    mac[0] &= 0xFE;
}

static void read_mac_from_efuse(uint8_t mac[6])
{
    esp_err_t err = esp_efuse_mac_get_default(mac);
    if (err != ESP_OK || (mac[0] == 0 && mac[1] == 0 && mac[2] == 0)) {
        ESP_LOGW(TAG, "eFuse MAC CRC error — using random MAC");
        generate_random_mac(mac);
    }
}

static ODID_Horizontal_accuracy_t horiz_acc_from_gps(uint8_t fix_type, uint8_t satellites)
{
    if (fix_type >= 4 && satellites >= 15) return ODID_HOR_ACC_1_METER;
    if (fix_type >= 4 && satellites >= 10) return ODID_HOR_ACC_3_METER;
    if (fix_type >= 4) return ODID_HOR_ACC_10_METER;
    if (fix_type >= 3) return ODID_HOR_ACC_10_METER;
    return ODID_HOR_ACC_30_METER;
}

static ODID_Vertical_accuracy_t vert_acc_from_gps(uint8_t fix_type, uint8_t satellites)
{
    if (fix_type >= 4 && satellites >= 15) return ODID_VER_ACC_3_METER;
    if (fix_type >= 4 && satellites >= 10) return ODID_VER_ACC_10_METER;
    if (fix_type >= 4) return ODID_VER_ACC_25_METER;
    if (fix_type >= 3) return ODID_VER_ACC_25_METER;
    return ODID_VER_ACC_45_METER;
}

void wifi_tx_init(const rid_config_t *cfg)
{
    if (g_initialized) return;

    esp_err_t ret = esp_event_loop_create_default();
    if (ret != ESP_OK && ret != ESP_ERR_INVALID_STATE) {
        ESP_LOGE(TAG, "event loop: %s", esp_err_to_name(ret));
    }

    esp_netif_create_default_wifi_ap();

    wifi_init_config_t wifi_cfg = WIFI_INIT_CONFIG_DEFAULT();
    ESP_ERROR_CHECK(esp_wifi_init(&wifi_cfg));

    read_mac_from_efuse(g_mac);
    esp_base_mac_addr_set(g_mac);

    ESP_ERROR_CHECK(esp_wifi_set_storage(WIFI_STORAGE_RAM));
    ESP_ERROR_CHECK(esp_wifi_set_mode(WIFI_MODE_AP));

    wifi_config_t ap_config = {0};
    const char *ssid = (cfg && cfg->wifi_ssid[0] != '\0') ? cfg->wifi_ssid : "ESP-RID";
    size_t ssid_len = strlen(ssid);
    if (ssid_len > 32) ssid_len = 32;
    memcpy(ap_config.ap.ssid, ssid, ssid_len);
    ap_config.ap.ssid_len = ssid_len;
    ap_config.ap.channel = (cfg && cfg->wifi_channel > 0) ? cfg->wifi_channel : 6;

    if (cfg && cfg->wifi_password[0] != '\0') {
        ap_config.ap.authmode = WIFI_AUTH_WPA2_PSK;
        size_t pass_len = strlen(cfg->wifi_password);
        if (pass_len > 63) pass_len = 63;
        memcpy(ap_config.ap.password, cfg->wifi_password, pass_len);
    } else {
        ap_config.ap.authmode = WIFI_AUTH_OPEN;
    }

    ap_config.ap.max_connection = 4;
    ap_config.ap.beacon_interval = 100;

    ESP_ERROR_CHECK(esp_wifi_set_config(WIFI_IF_AP, &ap_config));
    ESP_ERROR_CHECK(esp_wifi_start());

    esp_wifi_set_bandwidth(WIFI_IF_AP, WIFI_BW20);

    int8_t power_dbm = (cfg && cfg->wifi_power_dbm > 0) ? (int8_t)cfg->wifi_power_dbm : 8;
    esp_wifi_set_max_tx_power(power_dbm * 4);

    memcpy(g_ssid, ap_config.ap.ssid, ap_config.ap.ssid_len);
    g_ssid[ap_config.ap.ssid_len] = '\0';
    g_ssid_len = ap_config.ap.ssid_len;
    g_channel = ap_config.ap.channel;

    g_initialized = true;
    ESP_LOGI(TAG, "WiFi TX initialized, SSID: %s, CH: %d, MAC: %02x:%02x:%02x:%02x:%02x:%02x",
             ap_config.ap.ssid, ap_config.ap.channel,
             g_mac[0], g_mac[1], g_mac[2],
             g_mac[3], g_mac[4], g_mac[5]);
}

void wifi_tx_get_mac(uint8_t mac[6])
{
    memcpy(mac, g_mac, 6);
}

void wifi_tx_reconfigure_ap(const rid_config_t *cfg)
{
    if (!g_initialized || !cfg) return;

    wifi_config_t ap_config = {0};
    const char *ssid = (cfg->wifi_ssid[0] != '\0') ? cfg->wifi_ssid : "ESP-RID";
    size_t ssid_len = strlen(ssid);
    if (ssid_len > 32) ssid_len = 32;
    memcpy(ap_config.ap.ssid, ssid, ssid_len);
    ap_config.ap.ssid_len = ssid_len;
    ap_config.ap.channel = (cfg->wifi_channel > 0) ? cfg->wifi_channel : 6;

    if (cfg->wifi_password[0] != '\0') {
        ap_config.ap.authmode = WIFI_AUTH_WPA2_PSK;
        size_t pass_len = strlen(cfg->wifi_password);
        if (pass_len > 63) pass_len = 63;
        memcpy(ap_config.ap.password, cfg->wifi_password, pass_len);
    } else {
        ap_config.ap.authmode = WIFI_AUTH_OPEN;
    }
    ap_config.ap.max_connection = 4;
    ap_config.ap.beacon_interval = 100;

    esp_wifi_set_config(WIFI_IF_AP, &ap_config);

    int8_t power_dbm = (cfg->wifi_power_dbm > 0) ? (int8_t)cfg->wifi_power_dbm : 8;
    esp_wifi_set_max_tx_power(power_dbm * 4);

    memcpy(g_ssid, ap_config.ap.ssid, ap_config.ap.ssid_len);
    g_ssid[ap_config.ap.ssid_len] = '\0';
    g_ssid_len = ap_config.ap.ssid_len;
    g_channel = ap_config.ap.channel;

    ESP_LOGI(TAG, "AP reconfigured: SSID=%s CH=%d", g_ssid, g_channel);
}

static void populate_uas_data(ODID_UAS_Data *d, rid_gps_data_t *gps, rid_identity_t *identity)
{
    memset(d, 0, sizeof(ODID_UAS_Data));

    d->BasicIDValid[0] = 1;
    d->BasicID[0].IDType = (ODID_idtype_t)identity->id_type;
    d->BasicID[0].UAType = (ODID_uatype_t)identity->ua_type;
    strncpy((char *)d->BasicID[0].UASID, identity->uas_id, ODID_ID_SIZE);

    if (identity->uas_id_2[0] != '\0') {
        d->BasicIDValid[1] = 1;
        d->BasicID[1].IDType = (ODID_idtype_t)identity->id_type_2;
        d->BasicID[1].UAType = (ODID_uatype_t)identity->ua_type_2;
        strncpy((char *)d->BasicID[1].UASID, identity->uas_id_2, ODID_ID_SIZE);
    }

    d->LocationValid = 1;
    d->Location.Latitude = gps->latitude;
    d->Location.Longitude = gps->longitude;
    d->Location.AltitudeGeo = gps->altitude_msl;
    d->Location.Height = gps->altitude_relative;
    d->Location.SpeedHorizontal = gps->speed;
    d->Location.Direction = gps->heading;
    d->Location.SpeedVertical = gps->speed_vertical;
    d->Location.HorizAccuracy = horiz_acc_from_gps(gps->fix_type, gps->satellites);
    d->Location.VertAccuracy = vert_acc_from_gps(gps->fix_type, gps->satellites);

    d->SystemValid = 1;
    d->System.OperatorLatitude = gps->operator_lat;
    d->System.OperatorLongitude = gps->operator_lon;
    d->System.AreaCount = 0;
    d->System.AreaRadius = 0;

    if (identity->self_id_text[0] != '\0') {
        d->SelfIDValid = 1;
        d->SelfID.DescType = ODID_DESC_TYPE_TEXT;
        strncpy((char *)d->SelfID.Desc, identity->self_id_text, ODID_STR_SIZE);
    }

    d->OperatorIDValid = 1;
    strncpy((char *)d->OperatorID.OperatorId, identity->operator_id, ODID_ID_SIZE);
}

bool wifi_tx_transmit(rid_gps_data_t *gps, rid_identity_t *identity)
{
    if (!g_initialized || !gps || !identity) return false;

    populate_uas_data(&g_uas_data, gps, identity);

    static uint8_t buffer[1024];
    uint8_t counter = g_message_counter++;
    int length = odid_wifi_build_message_pack_beacon_frame(
        &g_uas_data, (char *)g_mac,
        g_ssid, g_ssid_len, 100, counter,
        buffer, sizeof(buffer));

    if (length > 0) {
        /* 4-attempt TX fallback: try STA/AP × no-seq/with-seq */
        static const wifi_interface_t ifaces[] = { WIFI_IF_AP, WIFI_IF_STA, WIFI_IF_AP, WIFI_IF_STA };
        static const bool seqs[] = { false, false, true, true };
        for (int attempt = 0; attempt < 4; attempt++) {
            esp_err_t ret = esp_wifi_80211_tx(ifaces[attempt], buffer, length, seqs[attempt]);
            if (ret == ESP_OK) return true;
        }
        ESP_LOGW(TAG, "TX failed after 4 attempts");
        return false;
    }

    return false;
}

bool wifi_tx_transmit_nan(rid_gps_data_t *gps, rid_identity_t *identity, uint8_t counter)
{
    if (!g_initialized || !gps || !identity) return false;

    populate_uas_data(&g_uas_data, gps, identity);

    static uint8_t buffer[1024];
    int length = odid_wifi_build_message_pack_nan_action_frame(
        &g_uas_data, (char *)g_mac,
        counter, buffer, sizeof(buffer));

    if (length > 0) {
        esp_err_t ret = esp_wifi_80211_tx(WIFI_IF_AP, buffer, length, true);
        if (ret != ESP_OK) {
            ESP_LOGW(TAG, "NAN TX failed: %s", esp_err_to_name(ret));
            return false;
        }
        return true;
    }

    return false;
}
