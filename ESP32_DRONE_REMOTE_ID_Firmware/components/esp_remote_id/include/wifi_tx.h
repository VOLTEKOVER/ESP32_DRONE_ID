#ifndef WIFI_TX_H
#define WIFI_TX_H

#include <stdint.h>
#include <stdbool.h>
#include "esp_remote_id.h"

void wifi_tx_init(const rid_config_t *cfg);
void wifi_tx_get_mac(uint8_t mac[6]);
void wifi_tx_reconfigure_ap(const rid_config_t *cfg);
bool wifi_tx_transmit(rid_gps_data_t *gps, rid_identity_t *identity, const rid_config_t *cfg);
bool wifi_tx_transmit_nan(rid_gps_data_t *gps, rid_identity_t *identity, uint8_t counter,
                          const rid_config_t *cfg);

#endif
