#include <string.h>
#include <stdio.h>
#include <sys/param.h>
#include "esp_log.h"
#include "esp_wifi.h"
#include "esp_event.h"
#include "esp_netif.h"
#include "esp_http_server.h"
#include "esp_ota_ops.h"
#include "esp_partition.h"
#include "nvs_flash.h"
#include "driver/gpio.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "rid_ota.h"
#include "esp_remote_id.h"
#include "rid_security.h"
#include "psa/crypto.h"

#define TAG "RID_OTA"

#define OTA_BUF_SIZE 1024

static bool g_ota_mode = false;
static httpd_handle_t g_ota_server = NULL;

static esp_err_t ota_get_handler(httpd_req_t *req)
{
    const char *resp =
        "<!DOCTYPE html><html><head><meta charset='utf-8'>"
        "<title>RemoteID OTA</title>"
        "<style>body{font-family:sans-serif;margin:2em}</style>"
        "</head><body>"
        "<h1>ESP Remote ID OTA</h1>"
        "<form method='POST' action='/update' enctype='multipart/form-data'>"
        "<h2>Firmware Update</h2>"
        "<input type='file' name='firmware' accept='.bin'><br><br>"
        "<input type='submit' value='Upload &amp; Flash'>"
        "</form>"
        "<hr>"
        "<form method='POST' action='/factory_reset'>"
        "<h2>Factory Reset</h2>"
        "<input type='submit' value='Reset to Factory Defaults'>"
        "</form>"
        "<form method='POST' action='/rollback'>"
        "<h2>Rollback</h2>"
        "<input type='submit' value='Rollback to Previous Version'>"
        "</form>"
        "</body></html>";
    httpd_resp_set_type(req, "text/html");
    httpd_resp_send(req, resp, strlen(resp));
    return ESP_OK;
}

static rid_config_t *s_ota_cfg = NULL;

static esp_err_t ota_update_handler(httpd_req_t *req)
{
    /* Read lock level */
    int lock_level = 0;
    if (s_ota_cfg) lock_level = s_ota_cfg->lock_level;

    /* At lock_level >= 2, reject OTA from GPIO mode too */
    if (lock_level >= 2) {
        httpd_resp_set_type(req, "text/html");
        httpd_resp_send(req, "<html><body><h1>OTA Rejected</h1><p>Device permanently locked.</p></body></html>", -1);
        return ESP_OK;
    }

    /* Read X-Expected-SHA256 header (mandatory at every lock level) */
    char expected_hex[65] = {0};
    bool has_expected = false;
    size_t hdr_len = httpd_req_get_hdr_value_len(req, "X-Expected-SHA256");
    if (hdr_len > 0 && hdr_len <= 64) {
        httpd_req_get_hdr_value_str(req, "X-Expected-SHA256", expected_hex, sizeof(expected_hex));
        has_expected = true;
    }

    /* Read X-Signature header (required at lock_level >= 1) */
    char sig_hdr[512] = {0};
    bool has_sig = false;
    size_t sig_hdr_len = httpd_req_get_hdr_value_len(req, "X-Signature");
    if (sig_hdr_len > 0 && sig_hdr_len < sizeof(sig_hdr)) {
        httpd_req_get_hdr_value_str(req, "X-Signature", sig_hdr, sizeof(sig_hdr));
        has_sig = true;
    }

    /* Require signature at lock_level >= 1 */
    if (lock_level >= 1 && !has_sig) {
        httpd_resp_set_type(req, "text/html");
        httpd_resp_send(req, "<html><body><h1>OTA Rejected</h1><p>Signature required at lock level >= 1.</p></body></html>", -1);
        return ESP_OK;
    }

    esp_ota_handle_t ota_handle = 0;
    const esp_partition_t *update_part = esp_ota_get_next_update_partition(NULL);
    if (!update_part) {
        httpd_resp_send_500(req);
        return ESP_FAIL;
    }

    esp_err_t err = esp_ota_begin(update_part, OTA_SIZE_UNKNOWN, &ota_handle);
    if (err != ESP_OK) {
        httpd_resp_send_500(req);
        return ESP_FAIL;
    }

    /* Compute running SHA-256 hash */
    psa_hash_operation_t sha_ctx = psa_hash_operation_init();
    if (psa_hash_setup(&sha_ctx, PSA_ALG_SHA_256) != PSA_SUCCESS) {
        esp_ota_abort(ota_handle);
        httpd_resp_send_500(req);
        return ESP_FAIL;
    }

    /* Also buffer entire body for signature verification if needed */
    uint8_t *body_buf = NULL;
    size_t body_len = 0;
    size_t body_cap = 0;
    if (lock_level >= 1) {
        body_cap = req->content_len > 0 ? req->content_len : 512 * 1024;
        body_buf = (uint8_t *)malloc(body_cap);
    }

    char buf[OTA_BUF_SIZE];
    int remaining = req->content_len;
    bool success = false;

    while (remaining > 0) {
        int recv = httpd_req_recv(req, buf, MIN(remaining, OTA_BUF_SIZE));
        if (recv <= 0) {
            if (recv == HTTPD_SOCK_ERR_TIMEOUT) continue;
            break;
        }

        if (psa_hash_update(&sha_ctx, (const unsigned char *)buf, recv) != PSA_SUCCESS) {
            psa_hash_abort(&sha_ctx);
            if (body_buf) free(body_buf);
            esp_ota_abort(ota_handle);
            httpd_resp_send_500(req);
            return ESP_FAIL;
        }

        if (body_buf && body_len + recv <= body_cap) {
            memcpy(body_buf + body_len, buf, recv);
            body_len += recv;
        }

        if (esp_ota_write(ota_handle, buf, recv) != ESP_OK) {
            psa_hash_abort(&sha_ctx);
            if (body_buf) free(body_buf);
            esp_ota_abort(ota_handle);
            httpd_resp_send_500(req);
            return ESP_FAIL;
        }
        remaining -= recv;
    }

    /* Verify SHA-256 hash (mandatory at every lock level) */
    uint8_t hash[32];
    size_t hash_len;
    if (psa_hash_finish(&sha_ctx, hash, sizeof(hash), &hash_len) != PSA_SUCCESS) {
        if (body_buf) free(body_buf);
        esp_ota_abort(ota_handle);
        httpd_resp_sendstr(req, "OTA failed: SHA-256 finalize error");
        return ESP_FAIL;
    }

    if (!has_expected) {
        if (body_buf) free(body_buf);
        esp_ota_abort(ota_handle);
        httpd_resp_sendstr(req, "OTA rejected: X-Expected-SHA256 header required");
        return ESP_FAIL;
    }

    {
        uint8_t expected_hash[32];
        if (!rid_security_hex_to_bytes(expected_hex, expected_hash, 32) ||
            memcmp(hash, expected_hash, 32) != 0) {
            char got_hex[65];
            rid_security_bytes_to_hex(hash, 32, got_hex);
            if (body_buf) free(body_buf);
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

    /* Verify signature at lock_level >= 1 */
    if (lock_level >= 1 && body_buf) {
        char *body_str = (char *)malloc(body_len + 1);
        if (body_str) {
            memcpy(body_str, body_buf, body_len);
            body_str[body_len] = '\0';
            bool sig_ok = rid_security_verify_signed_body(body_str, sig_hdr, s_ota_cfg);
            free(body_str);
            if (!sig_ok) {
                free(body_buf);
                esp_ota_abort(ota_handle);
                httpd_resp_sendstr(req, "OTA rejected: invalid signature");
                return ESP_FAIL;
            }
        }
    }

    if (body_buf) free(body_buf);

    if (remaining > 0 || esp_ota_end(ota_handle) != ESP_OK || esp_ota_set_boot_partition(update_part) != ESP_OK) {
        httpd_resp_send_500(req);
        return ESP_OK;
    }

    success = true;
    if (success) {
        httpd_resp_set_type(req, "text/html");
        httpd_resp_send(req, "<html><body><h1>Update OK</h1><p>Rebooting...</p></body></html>", -1);
        vTaskDelay(pdMS_TO_TICKS(500));
        esp_restart();
    }

    return ESP_OK;
}

static esp_err_t ota_factory_reset_handler(httpd_req_t *req)
{
    nvs_flash_erase();
    httpd_resp_set_type(req, "text/html");
    const char *ok = "<html><body><h1>Factory Reset</h1><p>Rebooting...</p></body></html>";
    httpd_resp_send(req, ok, strlen(ok));
    vTaskDelay(pdMS_TO_TICKS(500));
    esp_restart();
    return ESP_OK;
}

static esp_err_t ota_rollback_handler(httpd_req_t *req)
{
    const esp_partition_t *boot = esp_ota_get_boot_partition();
    const esp_partition_t *running = esp_ota_get_running_partition();
    if (boot != running) {
        esp_ota_set_boot_partition(running);
    }
    httpd_resp_set_type(req, "text/html");
    const char *ok = "<html><body><h1>Rollback</h1><p>Rebooting...</p></body></html>";
    httpd_resp_send(req, ok, strlen(ok));
    vTaskDelay(pdMS_TO_TICKS(500));
    esp_restart();
    return ESP_OK;
}

static void start_ota_server(void)
{
    httpd_config_t config = HTTPD_DEFAULT_CONFIG();
    config.lru_purge_enable = true;

    if (httpd_start(&g_ota_server, &config) == ESP_OK) {
        httpd_register_uri_handler(g_ota_server, &(httpd_uri_t){
            .uri = "/", .method = HTTP_GET, .handler = ota_get_handler });
        httpd_register_uri_handler(g_ota_server, &(httpd_uri_t){
            .uri = "/update", .method = HTTP_POST, .handler = ota_update_handler });
        httpd_register_uri_handler(g_ota_server, &(httpd_uri_t){
            .uri = "/factory_reset", .method = HTTP_POST, .handler = ota_factory_reset_handler });
        httpd_register_uri_handler(g_ota_server, &(httpd_uri_t){
            .uri = "/rollback", .method = HTTP_POST, .handler = ota_rollback_handler });
        ESP_LOGI(TAG, "OTA HTTP server started");
    }
}

static void start_ota_ap(void)
{
    esp_netif_create_default_wifi_ap();
    wifi_config_t wifi_config = {
        .ap = {
            .ssid = "RemoteID-OTA",
            .ssid_len = 13,
            .channel = 6,
            .max_connection = 1,
            .authmode = WIFI_AUTH_OPEN,
        },
    };
    esp_wifi_set_mode(WIFI_MODE_AP);
    esp_wifi_set_config(WIFI_IF_AP, &wifi_config);
    ESP_LOGI(TAG, "OTA AP started: RemoteID-OTA");
}

bool rid_ota_check_and_run(rid_config_t *cfg)
{
    bool enter_ota = false;

    if (cfg->ota_trigger_gpio >= 0) {
        gpio_config_t io_conf = {
            .pin_bit_mask = (1ULL << cfg->ota_trigger_gpio),
            .mode = GPIO_MODE_INPUT,
            .pull_up_en = true,
        };
        gpio_config(&io_conf);
        vTaskDelay(pdMS_TO_TICKS(10));
        if (gpio_get_level(cfg->ota_trigger_gpio) == 0) {
            enter_ota = true;
        }
    }

    if (!enter_ota) return false;

    ESP_LOGI(TAG, "Entering OTA mode");
    g_ota_mode = true;
    s_ota_cfg = cfg;

    esp_netif_init();
    esp_event_loop_create_default();
    start_ota_ap();
    esp_wifi_start();
    start_ota_server();

    while (1) {
        vTaskDelay(pdMS_TO_TICKS(1000));
    }

    return true;
}

bool rid_ota_is_active(void)
{
    return g_ota_mode;
}
