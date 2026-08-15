#include <stdio.h>
#include <string.h>
#include "esp_log.h"
#include "rid_output.h"
#include "odid_common.h"

#define TAG "RID_OUTPUT"

/* Region -> standard binding (exclusive) and message gating rules.
 * AUTO keeps the legacy behaviour (all messages allowed, both identity
 * fields required). CHN/FRDID select a national standard whose encoder is
 * not implemented yet; until then the EU-specific messages (Operator ID,
 * Self-ID, second Basic ID) are dropped from the broadcast. */
static const rid_region_rules_t g_region_rules[] = {
    /*          standard          op_id self b2   req_op req_id */
    [RID_REGION_AUTO] = { RID_STANDARD_ASTM,     true,  true,  true,  true,  true },
    [RID_REGION_EUR]  = { RID_STANDARD_ASTM,     true,  true,  true,  true,  true },
    [RID_REGION_FAA]  = { RID_STANDARD_ASTM,     true,  true,  true,  true,  true },
    [RID_REGION_JPN]  = { RID_STANDARD_ASTM,     true,  true,  true,  false, true },
    [RID_REGION_SGP]  = { RID_STANDARD_ASTM,     true,  true,  true,  false, true },
    [RID_REGION_KOR]  = { RID_STANDARD_ASTM,     true,  true,  true,  false, true },
    [RID_REGION_CHN]  = { RID_STANDARD_CHN_GB,   false, false, false, false, true },
    [RID_REGION_CAN]  = { RID_STANDARD_ASTM,     true,  true,  true,  true,  true },
    [RID_REGION_AUS]  = { RID_STANDARD_ASTM,     true,  true,  true,  true,  true },
    [RID_REGION_BRA]  = { RID_STANDARD_ASTM,     true,  true,  true,  false, true },
    [RID_REGION_NZL]  = { RID_STANDARD_ASTM,     true,  true,  true,  false, true },
};

static const char *g_region_names[] = {
    "AUTO", "EUR", "FAA", "JPN", "SGP", "KOR", "CHN", "CAN", "AUS", "BRA", "NZL",
};

static const char *g_standard_names[] = {
    "ASTM F3411-22a", "China GB 42590", "FRDID",
};

static rid_region_rules_t default_rules(void)
{
    return g_region_rules[RID_REGION_AUTO];
}

rid_standard_t rid_output_active_standard(const rid_config_t *cfg)
{
    if (!cfg) return RID_STANDARD_ASTM;
    unsigned r = (unsigned)cfg->region;
    if (r >= sizeof(g_region_rules) / sizeof(g_region_rules[0])) return RID_STANDARD_ASTM;
    return g_region_rules[r].standard;
}

bool rid_output_has_encoder(rid_standard_t standard)
{
    return standard == RID_STANDARD_ASTM;
}

rid_region_rules_t rid_output_region_rules(rid_region_t region)
{
    unsigned r = (unsigned)region;
    if (r >= sizeof(g_region_rules) / sizeof(g_region_rules[0])) return default_rules();
    return g_region_rules[r];
}

const char *rid_output_region_name(rid_region_t region)
{
    unsigned r = (unsigned)region;
    if (r >= sizeof(g_region_names) / sizeof(g_region_names[0])) return "?";
    return g_region_names[r];
}

const char *rid_output_standard_name(rid_standard_t standard)
{
    unsigned s = (unsigned)standard;
    if (s >= sizeof(g_standard_names) / sizeof(g_standard_names[0])) return "?";
    return g_standard_names[s];
}

bool rid_output_build_uas(ODID_UAS_Data *d, const rid_gps_data_t *gps,
                          const rid_identity_t *identity, const rid_config_t *cfg)
{
    if (!d || !gps || !identity) return false;

    rid_region_rules_t rules = rid_output_region_rules(cfg ? cfg->region : RID_REGION_AUTO);

    /* Gate the identity copy: messages not allowed by the region are
     * dropped from the broadcast. */
    rid_identity_t gated = *identity;
    if (!rules.operator_id_en) gated.operator_id[0] = '\0';
    if (!rules.self_id_en) gated.self_id_text[0] = '\0';
    if (!rules.basic_id_2_en) gated.uas_id_2[0] = '\0';

    /* Encoder dispatch (exclusive). Non-ASTM standards without an encoder
     * fall back to ASTM so the aircraft keeps broadcasting; the UI flags
     * this via rid_state_t.standard_fallback. */
    rid_standard_t std = rid_output_active_standard(cfg);
    if (std != RID_STANDARD_ASTM) {
        ESP_LOGW(TAG, "Standard '%s' not implemented - falling back to ASTM",
                 rid_output_standard_name(std));
    }

    odid_common_build_uas_data(d, gps, &gated);
    return true;
}
