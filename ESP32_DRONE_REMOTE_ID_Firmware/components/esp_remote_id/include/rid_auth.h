#ifndef RID_AUTH_H
#define RID_AUTH_H

#include <stdint.h>
#include <stdbool.h>
#include "opendroneid.h"

#define RID_AUTH_KEY_SIZE 32
#define RID_AUTH_SIG_SIZE 64

bool rid_auth_init(const char *pem_key);
bool rid_auth_enabled(void);

/* Signs the UAS ID with the configured Ed25519 key and fills the ODID
 * Auth message data pages (AuthType = ODID_AUTH_UAS_ID_SIGNATURE).
 * Returns true and sets *page_count when at least one page was produced. */
bool rid_auth_sign_identity(const char *uas_id, ODID_Auth_data *auth_out, uint8_t *page_count);

#endif
