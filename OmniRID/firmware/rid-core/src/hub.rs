//! Hourglass output hub, port of `rid_output.c`.
//!
//! All transports receive their UAS message pack from here instead of calling
//! an encoder directly. The hub binds the neutral GPS+identity data to the
//! single active broadcast standard, which is selected exclusively by
//! `Config::region`: enabling one region turns the other standards' outputs
//! off.
//!
//! Adding a new broadcast standard = implement its encoder in an `out-*`
//! crate and register it here; no transport, input or UI code needs changes.

use rid_interface::region::Standard;
use rid_interface::{GpsData, Identity, Region, RegionRules, UasBuild, MAX_STR_LEN};

/// Region -> standard binding (exclusive) and message gating rules.
/// Port of `g_region_rules[]` (column order: standard, op_id, self, b2,
/// req_op, req_id).
const REGION_RULES: [RegionRules; Region::COUNT] = [
    RegionRules {
        standard: Standard::Astm,
        operator_id_en: true,
        self_id_en: true,
        basic_id_2_en: true,
        require_operator_id: true,
        require_uas_id: true,
    }, // AUTO
    RegionRules {
        standard: Standard::Astm,
        operator_id_en: true,
        self_id_en: true,
        basic_id_2_en: true,
        require_operator_id: true,
        require_uas_id: true,
    }, // EUR
    RegionRules {
        standard: Standard::Astm,
        operator_id_en: true,
        self_id_en: true,
        basic_id_2_en: true,
        require_operator_id: true,
        require_uas_id: true,
    }, // FAA
    RegionRules {
        standard: Standard::Astm,
        operator_id_en: true,
        self_id_en: true,
        basic_id_2_en: true,
        require_operator_id: false,
        require_uas_id: true,
    }, // JPN
    RegionRules {
        standard: Standard::Astm,
        operator_id_en: true,
        self_id_en: true,
        basic_id_2_en: true,
        require_operator_id: false,
        require_uas_id: true,
    }, // SGP
    RegionRules {
        standard: Standard::Astm,
        operator_id_en: true,
        self_id_en: true,
        basic_id_2_en: true,
        require_operator_id: false,
        require_uas_id: true,
    }, // KOR
    RegionRules {
        standard: Standard::ChnGb,
        operator_id_en: false,
        self_id_en: false,
        basic_id_2_en: false,
        require_operator_id: false,
        require_uas_id: true,
    }, // CHN
    RegionRules {
        standard: Standard::Astm,
        operator_id_en: true,
        self_id_en: true,
        basic_id_2_en: true,
        require_operator_id: true,
        require_uas_id: true,
    }, // CAN
    RegionRules {
        standard: Standard::Astm,
        operator_id_en: true,
        self_id_en: true,
        basic_id_2_en: true,
        require_operator_id: true,
        require_uas_id: true,
    }, // AUS
    RegionRules {
        standard: Standard::Astm,
        operator_id_en: true,
        self_id_en: true,
        basic_id_2_en: true,
        require_operator_id: false,
        require_uas_id: true,
    }, // BRA
    RegionRules {
        standard: Standard::Astm,
        operator_id_en: true,
        self_id_en: true,
        basic_id_2_en: true,
        require_operator_id: false,
        require_uas_id: true,
    }, // NZL
];

/// Port of `g_region_names[]`.
const REGION_NAMES: [&str; Region::COUNT] = [
    "AUTO", "EUR", "FAA", "JPN", "SGP", "KOR", "CHN", "CAN", "AUS", "BRA", "NZL",
];

/// Port of `g_standard_names[]`.
const STANDARD_NAMES: [&str; Standard::COUNT] = ["ASTM F3411-22a", "China GB 42590", "FRDID"];

/// Standard selected by the configured region (exclusive).
/// Port of `rid_output_active_standard()`.
pub fn active_standard(region: Region) -> Standard {
    REGION_RULES[region as usize].standard
}

/// True when an encoder for the given standard exists.
/// Port of `rid_output_has_encoder()`.
pub fn has_encoder(standard: Standard) -> bool {
    standard == Standard::Astm
}

/// Gating rules for a region.
/// Port of `rid_output_region_rules()`.
pub fn region_rules(region: Region) -> RegionRules {
    REGION_RULES[region as usize]
}

/// Port of `rid_output_region_name()`.
pub fn region_name(region: Region) -> &'static str {
    REGION_NAMES[region as usize]
}

/// Port of `rid_output_standard_name()`.
pub fn standard_name(standard: Standard) -> &'static str {
    STANDARD_NAMES[standard as usize]
}

/// Port of `rid_output_build_uas()` (minus the ODID pack encoding, which the
/// `out-*` encoder crates perform on the gated data).
///
/// Gates the identity copy: messages not allowed by the region are dropped
/// from the broadcast. Then dispatches to the exclusive standard; non-ASTM
/// standards without an encoder fall back to ASTM so the aircraft keeps
/// broadcasting (surfaced via `UasBuild::fallback`).
pub fn build_uas(gps: &GpsData, identity: &Identity, region: Region) -> UasBuild {
    let rules = region_rules(region);

    let mut gated = *identity;
    if !rules.operator_id_en {
        gated.operator_id = [0; MAX_STR_LEN + 1];
    }
    if !rules.self_id_en {
        gated.self_id_text = [0; MAX_STR_LEN + 1];
    }
    if !rules.basic_id_2_en {
        gated.uas_id_2 = [0; MAX_STR_LEN + 1];
    }

    let std = active_standard(region);
    let fallback = !has_encoder(std);

    UasBuild {
        standard: std,
        gps: *gps,
        gated_identity: gated,
        fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rid_interface::{fixed_str, CStr};

    fn identity() -> Identity {
        Identity {
            uas_id: fixed_str("TEST-UAS-123"),
            operator_id: fixed_str("OP-001234"),
            self_id_text: fixed_str("Drone test"),
            id_type: 1,
            ua_type: 1,
            uas_id_2: fixed_str("TEST-UAS-2"),
            ..Identity::default()
        }
    }

    fn gps() -> GpsData {
        GpsData {
            latitude: 45.5,
            longitude: 9.2,
            altitude_msl: 120.0,
            altitude_relative: 100.0,
            fix_type: 4,
            satellites: 14,
            ..GpsData::default()
        }
    }

    #[test]
    fn active_standard_is_exclusive() {
        assert_eq!(active_standard(Region::Eur), Standard::Astm);
        assert_eq!(active_standard(Region::Chn), Standard::ChnGb);
        assert_eq!(active_standard(Region::Faa), Standard::Astm);
    }

    #[test]
    fn only_astm_has_encoder() {
        assert!(has_encoder(Standard::Astm));
        assert!(!has_encoder(Standard::ChnGb));
        assert!(!has_encoder(Standard::Frdid));
    }

    #[test]
    fn names() {
        assert_eq!(region_name(Region::Chn), "CHN");
        assert_eq!(region_name(Region::Auto), "AUTO");
        assert_eq!(standard_name(Standard::Astm), "ASTM F3411-22a");
    }

    #[test]
    fn eur_keeps_all_messages() {
        let b = build_uas(&gps(), &identity(), Region::Eur);
        assert_eq!(b.standard, Standard::Astm);
        assert!(!b.fallback);
        assert!(!b.gated_identity.operator_id.c_is_empty());
        assert!(!b.gated_identity.self_id_text.c_is_empty());
        assert!(!b.gated_identity.uas_id_2.c_is_empty());
    }

    #[test]
    fn chn_gates_eu_only_messages() {
        let b = build_uas(&gps(), &identity(), Region::Chn);
        assert_eq!(b.standard, Standard::ChnGb);
        assert!(b.fallback);
        assert!(b.gated_identity.operator_id.c_is_empty());
        assert!(b.gated_identity.self_id_text.c_is_empty());
        assert!(b.gated_identity.uas_id_2.c_is_empty());
        assert!(!b.gated_identity.uas_id.c_is_empty());
    }

    #[test]
    fn jpn_does_not_require_operator_id() {
        let r = region_rules(Region::Jpn);
        assert!(!r.require_operator_id);
        assert!(r.require_uas_id);
        assert!(r.operator_id_en);
    }
}
