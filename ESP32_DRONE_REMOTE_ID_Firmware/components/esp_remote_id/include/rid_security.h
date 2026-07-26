#ifndef RID_SECURITY_H
#define RID_SECURITY_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include "esp_remote_id.h"

bool rid_security_verify_signed_body(const char *body, const char *sig_b64,
                                     const rid_config_t *cfg);
bool rid_security_verify_sha256(const uint8_t *data, size_t data_len,
                                const char *expected_hex);
void rid_security_bytes_to_hex(const uint8_t *bytes, size_t len, char *out);
bool rid_security_hex_to_bytes(const char *hex, uint8_t *out, size_t out_len);

#endif
