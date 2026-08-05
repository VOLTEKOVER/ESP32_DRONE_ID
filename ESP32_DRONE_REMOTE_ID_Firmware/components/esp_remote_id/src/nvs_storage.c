#include <stdio.h>
#include <string.h>
#include "esp_log.h"
#include "nvs_flash.h"
#include "nvs_storage.h"

#define TAG "NVS"
#define NS "esp_rid"

void nvs_storage_init(void)
{
    esp_err_t ret = nvs_flash_init();
    if (ret == ESP_ERR_NVS_NO_FREE_PAGES || ret == ESP_ERR_NVS_NEW_VERSION_FOUND) {
        ESP_ERROR_CHECK(nvs_flash_erase());
        ret = nvs_flash_init();
    }
    ESP_ERROR_CHECK(ret);
}

static void store_str(nvs_handle_t h, const char *key, const char *val)
{
    nvs_set_str(h, key, val);
}

static void load_str(nvs_handle_t h, const char *key, char *out, size_t max, const char *def)
{
    size_t len = max;
    if (nvs_get_str(h, key, out, &len) != ESP_OK)
        strncpy(out, def, max);
}

static void store_u8(nvs_handle_t h, const char *key, uint8_t val)
{
    nvs_set_u8(h, key, val);
}

static uint8_t load_u8_def(nvs_handle_t h, const char *key, uint8_t def)
{
    uint8_t v = def;
    nvs_get_u8(h, key, &v);
    return v;
}

static void store_u32(nvs_handle_t h, const char *key, uint32_t val)
{
    nvs_set_u32(h, key, val);
}

static uint32_t load_u32_def(nvs_handle_t h, const char *key, uint32_t def)
{
    uint32_t v = def;
    nvs_get_u32(h, key, &v);
    return v;
}

static void store_f32(nvs_handle_t h, const char *key, float val)
{
    nvs_set_blob(h, key, &val, sizeof(float));
}

static float load_f32_def(nvs_handle_t h, const char *key, float def)
{
    float v = def;
    size_t sz = sizeof(float);
    nvs_get_blob(h, key, &v, &sz);
    return v;
}

static void store_i8(nvs_handle_t h, const char *key, int8_t val)
{
    nvs_set_i8(h, key, val);
}

static int8_t load_i8_def(nvs_handle_t h, const char *key, int8_t def)
{
    int8_t v = def;
    nvs_get_i8(h, key, &v);
    return v;
}

void nvs_storage_save(rid_config_t *cfg)
{
    nvs_handle_t h;
    if (nvs_open(NS, NVS_READWRITE, &h) != ESP_OK) return;

    store_str(h, "uas_id", cfg->uas_id);
    store_str(h, "op_id", cfg->operator_id);
    store_str(h, "self_id", cfg->self_id_text);
    store_str(h, "uas_id2", cfg->uas_id_2);
    store_str(h, "wifi_ssid", cfg->wifi_ssid);
    store_str(h, "wifi_pass", cfg->wifi_password);

    store_u8(h, "ua_type", cfg->ua_type);
    store_u8(h, "id_type", cfg->id_type);
    store_u8(h, "ua_type2", cfg->ua_type_2);
    store_u8(h, "id_type2", cfg->id_type_2);
    store_u8(h, "wifi_ch", cfg->wifi_channel);
    store_u8(h, "websrv_en", cfg->webserver_en);
    store_u8(h, "mav_sysid", cfg->mavlink_sysid);
    store_u8(h, "bcast_pwr", cfg->bcast_powerup);
    store_u8(h, "tx_modes", cfg->tx_modes);
    store_u32(h, "options", cfg->options);
    store_i8(h, "lock_lvl", cfg->lock_level);
    store_i8(h, "led_r", cfg->led_r_gpio);
    store_i8(h, "led_g", cfg->led_g_gpio);
    store_i8(h, "led_b", cfg->led_b_gpio);

    store_u32(h, "baud", cfg->baud_rate);

    store_f32(h, "wifi_pwr", cfg->wifi_power_dbm);
    store_f32(h, "wifi_bcn", cfg->wifi_bcn_rate_hz);
    store_f32(h, "wifi_nan", cfg->wifi_nan_rate_hz);
    store_f32(h, "bt4_rate", cfg->ble4_rate_hz);
    store_f32(h, "bt4_pwr", cfg->ble4_power_dbm);
    store_f32(h, "bt5_rate", cfg->ble5_rate_hz);
    store_f32(h, "bt5_pwr", cfg->ble5_power_dbm);

    store_f32(h, "op_lat", (float)cfg->operator_lat);
    store_f32(h, "op_lon", (float)cfg->operator_lon);
    store_f32(h, "op_alt", cfg->operator_alt);

    for (int i = 0; i < ESP_RID_NUM_KEYS; i++) {
        char key[16];
        snprintf(key, sizeof(key), "pubkey%d", i + 1);
        store_str(h, key, cfg->public_keys[i]);
    }

    nvs_commit(h);
    nvs_close(h);
}

void nvs_storage_load(rid_config_t *cfg)
{
    nvs_handle_t h;
    if (nvs_open(NS, NVS_READWRITE, &h) != ESP_OK) return;

    load_str(h, "uas_id", cfg->uas_id, ESP_RID_MAX_STR_LEN + 1, "");
    load_str(h, "op_id", cfg->operator_id, ESP_RID_MAX_STR_LEN + 1, "");
    load_str(h, "self_id", cfg->self_id_text, ESP_RID_MAX_STR_LEN + 1, "");
    load_str(h, "uas_id2", cfg->uas_id_2, ESP_RID_MAX_STR_LEN + 1, "");
    load_str(h, "wifi_ssid", cfg->wifi_ssid, ESP_RID_MAX_STR_LEN + 1, "ESP-RID");
    load_str(h, "wifi_pass", cfg->wifi_password, ESP_RID_MAX_STR_LEN + 1, "");

    cfg->ua_type = load_u8_def(h, "ua_type", cfg->ua_type);
    cfg->id_type = load_u8_def(h, "id_type", cfg->id_type);
    cfg->ua_type_2 = load_u8_def(h, "ua_type2", cfg->ua_type_2);
    cfg->id_type_2 = load_u8_def(h, "id_type2", cfg->id_type_2);
    cfg->wifi_channel = load_u8_def(h, "wifi_ch", cfg->wifi_channel);
    cfg->webserver_en = load_u8_def(h, "websrv_en", cfg->webserver_en);
    cfg->mavlink_sysid = load_u8_def(h, "mav_sysid", cfg->mavlink_sysid);
    cfg->bcast_powerup = load_u8_def(h, "bcast_pwr", cfg->bcast_powerup);
    cfg->tx_modes = load_u8_def(h, "tx_modes", cfg->tx_modes);
    cfg->options = (uint16_t)load_u32_def(h, "options", cfg->options);
    cfg->lock_level = load_i8_def(h, "lock_lvl", cfg->lock_level);
    cfg->led_r_gpio = load_i8_def(h, "led_r", cfg->led_r_gpio);
    cfg->led_g_gpio = load_i8_def(h, "led_g", cfg->led_g_gpio);
    cfg->led_b_gpio = load_i8_def(h, "led_b", cfg->led_b_gpio);

    cfg->baud_rate = load_u32_def(h, "baud", cfg->baud_rate);

    cfg->wifi_power_dbm = load_f32_def(h, "wifi_pwr", cfg->wifi_power_dbm);
    cfg->wifi_bcn_rate_hz = load_f32_def(h, "wifi_bcn", cfg->wifi_bcn_rate_hz);
    cfg->wifi_nan_rate_hz = load_f32_def(h, "wifi_nan", cfg->wifi_nan_rate_hz);
    cfg->ble4_rate_hz = load_f32_def(h, "bt4_rate", cfg->ble4_rate_hz);
    cfg->ble4_power_dbm = load_f32_def(h, "bt4_pwr", cfg->ble4_power_dbm);
    cfg->ble5_rate_hz = load_f32_def(h, "bt5_rate", cfg->ble5_rate_hz);
    cfg->ble5_power_dbm = load_f32_def(h, "bt5_pwr", cfg->ble5_power_dbm);

    cfg->operator_lat = load_f32_def(h, "op_lat", (float)cfg->operator_lat);
    cfg->operator_lon = load_f32_def(h, "op_lon", (float)cfg->operator_lon);
    cfg->operator_alt = load_f32_def(h, "op_alt", cfg->operator_alt);

    for (int i = 0; i < ESP_RID_NUM_KEYS; i++) {
        char key[16];
        snprintf(key, sizeof(key), "pubkey%d", i + 1);
        load_str(h, key, cfg->public_keys[i], ESP_RID_MAX_KEY_LEN + 1, "");
    }

    nvs_close(h);
}

void nvs_storage_erase(void)
{
    nvs_handle_t h;
    if (nvs_open(NS, NVS_READWRITE, &h) == ESP_OK) {
        nvs_erase_all(h);
        nvs_commit(h);
        nvs_close(h);
    }
}

void nvs_storage_reset_preserve_keys(void)
{
    char keys[ESP_RID_NUM_KEYS][ESP_RID_MAX_KEY_LEN + 1];
    memset(keys, 0, sizeof(keys));

    nvs_handle_t h;
    if (nvs_open(NS, NVS_READWRITE, &h) != ESP_OK) return;

    for (int i = 0; i < ESP_RID_NUM_KEYS; i++) {
        char key[16];
        snprintf(key, sizeof(key), "pubkey%d", i + 1);
        load_str(h, key, keys[i], ESP_RID_MAX_KEY_LEN + 1, "");
    }

    nvs_erase_all(h);

    for (int i = 0; i < ESP_RID_NUM_KEYS; i++) {
        if (keys[i][0] != '\0') {
            char key[16];
            snprintf(key, sizeof(key), "pubkey%d", i + 1);
            store_str(h, key, keys[i]);
        }
    }

    nvs_commit(h);
    nvs_close(h);
}
