#include <string.h>
#include "esp_log.h"
#include "esp_random.h"
#include "mbedtls/pk.h"
#include "mbedtls/error.h"
#include "rid_auth.h"

#define TAG "RID_AUTH"

static bool g_auth_initialized = false;
static bool g_auth_enabled = false;
static mbedtls_pk_context g_pk;

static int esp_rng(void *ctx, unsigned char *buf, size_t len)
{
    (void)ctx;
    return (esp_random(buf, len) == (int)len) ? 0 : -1;
}

bool rid_auth_init(const char *pem_key)
{
    if (g_auth_initialized) {
        mbedtls_pk_free(&g_pk);
    }

    mbedtls_pk_init(&g_pk);

    if (!pem_key || pem_key[0] == '\0') {
        ESP_LOGW(TAG, "No auth private key configured");
        g_auth_enabled = false;
        g_auth_initialized = true;
        return false;
    }

    int ret = mbedtls_pk_parse_key(&g_pk, (const unsigned char *)pem_key,
                                   strlen(pem_key) + 1, NULL, 0,
                                   esp_rng, NULL);
    if (ret != 0) {
        char err[128];
        mbedtls_strerror(ret, err, sizeof(err));
        ESP_LOGW(TAG, "Failed to parse private key: %s", err);
        g_auth_enabled = false;
        g_auth_initialized = true;
        return false;
    }

    if (!mbedtls_pk_can_do(&g_pk, MBEDTLS_PK_ED25519)) {
        ESP_LOGW(TAG, "Key is not Ed25519");
        mbedtls_pk_free(&g_pk);
        g_auth_enabled = false;
        g_auth_initialized = true;
        return false;
    }

    /* Validate Ed25519 key bit-length (must be exactly 256 bits = 32 bytes) */
    size_t key_bitlen = mbedtls_pk_get_bitlen(&g_pk);
    if (key_bitlen != 256) {
        ESP_LOGW(TAG, "Ed25519 key has invalid bit-length: %u (expected 256)", (unsigned)key_bitlen);
        mbedtls_pk_free(&g_pk);
        g_auth_enabled = false;
        g_auth_initialized = true;
        return false;
    }

    g_auth_enabled = true;
    g_auth_initialized = true;
    ESP_LOGI(TAG, "Ed25519 auth initialized");
    return true;
}

bool rid_auth_enabled(void)
{
    return g_auth_initialized && g_auth_enabled;
}

bool rid_auth_sign_message(uint8_t msg_type, const uint8_t *msg_data, uint8_t msg_len,
                           uint8_t page_buf[ODID_AUTH_PAGE_SIZE], uint8_t *page_count)
{
    if (!g_auth_enabled || !g_auth_initialized) return false;

    uint8_t sig[RID_AUTH_SIG_SIZE];
    size_t sig_len = sizeof(sig);

    int ret = mbedtls_pk_sign(&g_pk, MBEDTLS_MD_NONE, msg_data, msg_len,
                             sig, &sig_len, esp_rng, NULL);
    if (ret != 0) {
        char err[128];
        mbedtls_strerror(ret, err, sizeof(err));
        ESP_LOGE(TAG, "Sign failed: %s", err);
        return false;
    }

    uint8_t pages_needed = (sig_len + ODID_AUTH_PAGE_DATA_SIZE - 1) / ODID_AUTH_PAGE_DATA_SIZE;
    if (pages_needed > ODID_AUTH_MAX_PAGES) {
        ESP_LOGW(TAG, "Auth exceeds max pages (%d > %d)", pages_needed, ODID_AUTH_MAX_PAGES);
        return false;
    }

    *page_count = pages_needed;
    for (uint8_t p = 0; p < pages_needed; p++) {
        memset(page_buf + p * ODID_AUTH_PAGE_SIZE, 0, ODID_AUTH_PAGE_SIZE);
        page_buf[p * ODID_AUTH_PAGE_SIZE + 0] = msg_type;
        page_buf[p * ODID_AUTH_PAGE_SIZE + 1] = p;
        page_buf[p * ODID_AUTH_PAGE_SIZE + 2] = pages_needed - 1;
        page_buf[p * ODID_AUTH_PAGE_SIZE + 3] = 0;
        uint8_t copy_len = ODID_AUTH_PAGE_DATA_SIZE;
        if (p == pages_needed - 1) {
            copy_len = sig_len - p * ODID_AUTH_PAGE_DATA_SIZE;
        }
        page_buf[p * ODID_AUTH_PAGE_SIZE + 4] = 7;
        memcpy(page_buf + p * ODID_AUTH_PAGE_SIZE + 5, sig + p * ODID_AUTH_PAGE_DATA_SIZE, copy_len);
    }

    return true;
}
