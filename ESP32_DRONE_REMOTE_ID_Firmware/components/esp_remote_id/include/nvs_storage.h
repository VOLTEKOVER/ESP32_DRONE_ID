#ifndef NVS_STORAGE_H
#define NVS_STORAGE_H

#include "esp_remote_id.h"

void nvs_storage_init(void);
void nvs_storage_save(rid_config_t *config);
void nvs_storage_load(rid_config_t *config);
void nvs_storage_erase(void);
/* Factory reset that preserves the provisioned public keys (pubkey1..5). */
void nvs_storage_reset_preserve_keys(void);

#endif
