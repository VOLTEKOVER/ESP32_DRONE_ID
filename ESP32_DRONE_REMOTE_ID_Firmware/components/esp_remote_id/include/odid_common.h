#ifndef ODID_COMMON_H
#define ODID_COMMON_H

#include <stdint.h>
#include <stdbool.h>
#include "opendroneid.h"
#include "esp_remote_id.h"

/* Fills an ODID_UAS_Data message pack (Basic ID, Location, System,
 * Self-ID, Operator ID, Auth) from GPS + identity data. Shared by the
 * WiFi Beacon/NAN and BLE transmit paths so the pack is built identically
 * on every transport. */
void odid_common_build_uas_data(ODID_UAS_Data *d, const rid_gps_data_t *gps,
                                const rid_identity_t *identity);

#endif
