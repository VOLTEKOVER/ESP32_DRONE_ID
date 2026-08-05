#include <string.h>
#include "esp_log.h"
#include "mbedtls/pk.h"
#include "mbedtls/error.h"
#include "rid_auth.h"

#define TAG "RID_AUTH"

static bool g_auth_initialized = false;
static bool g_auth_enabled = false;
static mbedtls_pk_context g_pk;

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
                                   strlen(pem_key) + 1, NULL, 0);
    if (ret != 0) {
        char err[128];
        mbedtls_strerror(ret, err, sizeof(err));
        ESP_LOGW(TAG, "Failed to parse private key: %s", err);
        g_auth_enabled = false;
        g_auth_initialized = true;
        return false;
    }

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

bool rid_auth_sign_identity(const char *uas_id, ODID_Auth_data *auth_out, uint8_t *page_count)
{
    if (!g_auth_enabled || !g_auth_initialized) return false;
    if (!uas_id || uas_id[0] == '\0') return false;

    size_t id_len = strlen(uas_id);
    uint8_t sig[RID_AUTH_SIG_SIZE];
    size_t sig_len = 0;

    int ret = mbedtls_pk_sign(&g_pk, MBEDTLS_MD_NONE,
                              (const unsigned char *)uas_id, id_len,
                              sig, sizeof(sig), &sig_len);
    if (ret != 0 || sig_len == 0) {
        char err[128];
        mbedtls_strerror(ret, err, sizeof(err));
        ESP_LOGE(TAG, "Sign failed: %s", err);
        return false;
    }

    /* Page 0 carries ODID_AUTH_PAGE_ZERO_DATA_SIZE bytes, later pages carry
     * ODID_AUTH_PAGE_NONZERO_DATA_SIZE bytes each. */
    uint8_t pages = 1;
    if (sig_len > ODID_AUTH_PAGE_ZERO_DATA_SIZE) {
        pages += (uint8_t)((sig_len - ODID_AUTH_PAGE_ZERO_DATA_SIZE +
                            ODID_AUTH_PAGE_NONZERO_DATA_SIZE - 1) /
                           ODID_AUTH_PAGE_NONZERO_DATA_SIZE);
    }
    if (pages > ODID_AUTH_MAX_PAGES) {
        ESP_LOGW(TAG, "Auth exceeds max pages (%d > %d)", pages, ODID_AUTH_MAX_PAGES);
        return false;
    }

    uint8_t offset = 0;
    for (uint8_t p = 0; p < pages; p++) {
        memset(&auth_out[p], 0, sizeof(ODID_Auth_data));
        auth_out[p].DataPage = p;
        auth_out[p].AuthType = ODID_AUTH_UAS_ID_SIGNATURE;
        auth_out[p].LastPageIndex = pages - 1;
        auth_out[p].Length = (uint8_t)sig_len;

        uint8_t cap = (p == 0) ? ODID_AUTH_PAGE_ZERO_DATA_SIZE
                               : ODID_AUTH_PAGE_NONZERO_DATA_SIZE;
        uint8_t chunk = (sig_len - offset < cap) ? (uint8_t)(sig_len - offset) : cap;
        memcpy(auth_out[p].AuthData, sig + offset, chunk);
        offset += chunk;
    }

    *page_count = pages;
    return true;
}
