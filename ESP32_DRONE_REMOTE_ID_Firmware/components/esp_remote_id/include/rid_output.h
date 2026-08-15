#ifndef RID_OUTPUT_H
#define RID_OUTPUT_H

#include <stdbool.h>
#include <stdint.h>
#include "esp_remote_id.h"
#include "opendroneid.h"

/* Hourglass output hub.
 *
 * All transports (WiFi Beacon/NAN, BLE 4.0/5.0) receive their UAS message
 * pack from here instead of calling an encoder directly. The hub binds the
 * neutral GPS+identity data to the single active broadcast standard, which
 * is selected exclusively by rid_config_t.region: enabling one region turns
 * the other standards' outputs off.
 *
 * Adding a new broadcast standard = implement its encoder and register it
 * in rid_output_build_uas(); no transport, input or UI code needs changes. */

/* Message gating rules for a region: which ODID messages are allowed to be
 * broadcast and which identity fields are mandatory for readiness. */
typedef struct {
    rid_standard_t standard;
    bool operator_id_en;
    bool self_id_en;
    bool basic_id_2_en;
    bool require_operator_id;
    bool require_uas_id;
} rid_region_rules_t;

/* Standard selected by the configured region (exclusive). */
rid_standard_t rid_output_active_standard(const rid_config_t *cfg);

/* True when an encoder for the given standard exists. */
bool rid_output_has_encoder(rid_standard_t standard);

/* Gating rules for a region. */
rid_region_rules_t rid_output_region_rules(rid_region_t region);

const char *rid_output_region_name(rid_region_t region);
const char *rid_output_standard_name(rid_standard_t standard);

/* Builds the active standard's UAS message pack into d from the neutral
 * GPS+identity data. Messages not allowed by the region are dropped.
 * If the active standard's encoder is not implemented yet, it falls back
 * to ASTM so the aircraft keeps broadcasting (surfaced to the UI via
 * rid_state_t.standard_fallback). Returns false only on bad arguments. */
bool rid_output_build_uas(ODID_UAS_Data *d, const rid_gps_data_t *gps,
                          const rid_identity_t *identity, const rid_config_t *cfg);

#endif
