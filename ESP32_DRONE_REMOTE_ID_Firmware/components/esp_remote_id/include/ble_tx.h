#ifndef BLE_TX_H
#define BLE_TX_H

#include "esp_remote_id.h"

void ble_tx_init(void);
bool ble_tx_transmit_legacy(rid_gps_data_t *gps, rid_identity_t *identity, const rid_config_t *cfg);
bool ble_tx_transmit_lr(rid_gps_data_t *gps, rid_identity_t *identity, const rid_config_t *cfg);
void ble_tx_set_power(int8_t dbm);

#endif
