//! Identity readiness gate and position sanity, port of the helpers in
//! `esp_remote_id.c` (`identity_is_sane`, `position_is_sane`) and the
//! per-update state transitions (takeoff capture, relative altitude).

use rid_interface::{CStr, GpsData, Identity, Region, RegionRules, State, OPT_IDENTITY_READY_GATE};

/// Port of `identity_is_sane()`.
///
/// The identity is sane when every identity field required by the region
/// rules is present and meaningful (not the factory defaults).
pub fn identity_is_sane(id: &Identity, rules: &RegionRules) -> bool {
    if rules.require_uas_id && id.uas_id.c_is_empty() {
        return false;
    }
    if id.uas_id.c_starts_with("ESP32-RID-") {
        return false;
    }
    if rules.require_operator_id {
        if id.operator_id.c_contains("OP-UNKNOWN") {
            return false;
        }
        if id.operator_id.c_is_empty() {
            return false;
        }
    }
    true
}

/// Port of `position_is_sane()`.
pub fn position_is_sane(gps: &GpsData) -> bool {
    if gps.latitude < -90.0 || gps.latitude > 90.0 {
        return false;
    }
    if gps.longitude < -180.0 || gps.longitude > 180.0 {
        return false;
    }
    true
}

/// Port of the identity readiness gate in the main loop.
///
/// With `OPT_IDENTITY_READY_GATE` set, readiness is granted only when the
/// identity and position are sane; otherwise it is granted unconditionally.
pub fn update_identity_ready(state: &mut State, options: u16, region: Region) {
    if options & OPT_IDENTITY_READY_GATE != 0 {
        let rules = region_rules_for(region);
        if identity_is_sane(&state.identity, &rules) && position_is_sane(&state.gps) {
            state.identity_ready = true;
        }
    } else {
        state.identity_ready = true;
    }
}

/// Port of the takeoff capture in the main loop (once at first 3D fix).
pub fn maybe_capture_takeoff(
    state: &mut State,
    fix_type: u8,
    latitude: f64,
    longitude: f64,
    altitude_msl: f32,
) {
    if !state.takeoff_captured && fix_type >= 3 && latitude != 0.0 && longitude != 0.0 {
        state.takeoff_lat = latitude;
        state.takeoff_lon = longitude;
        state.takeoff_alt = altitude_msl;
        state.takeoff_captured = true;
    }
}

/// Port of the MSP/NMEA relative-altitude derivation.
pub fn derive_relative_altitude(state: &mut State) {
    if state.takeoff_captured {
        state.gps.altitude_relative = state.gps.altitude_msl - state.takeoff_alt;
    }
}

fn region_rules_for(region: Region) -> RegionRules {
    crate::hub::region_rules(region)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::region_rules;
    use rid_interface::fixed_str;

    fn sane_identity() -> Identity {
        Identity {
            uas_id: fixed_str("TEST-UAS-123"),
            operator_id: fixed_str("OP-001234"),
            ..Identity::default()
        }
    }

    #[test]
    fn empty_uas_id_fails_when_required() {
        let id = Identity::default();
        let rules = region_rules(Region::Eur);
        assert!(!identity_is_sane(&id, &rules));
    }

    #[test]
    fn factory_placeholder_uas_id_fails() {
        let id = Identity {
            uas_id: fixed_str("ESP32-RID-001"),
            operator_id: fixed_str("OP-001234"),
            ..Identity::default()
        };
        let rules = region_rules(Region::Eur);
        assert!(!identity_is_sane(&id, &rules));
    }

    #[test]
    fn op_unknown_fails_when_required() {
        let id = Identity {
            uas_id: fixed_str("TEST-UAS-123"),
            operator_id: fixed_str("OP-UNKNOWN"),
            ..Identity::default()
        };
        let rules = region_rules(Region::Eur);
        assert!(!identity_is_sane(&id, &rules));
        // JPN does not require the operator id: placeholder is allowed.
        let jpn = region_rules(Region::Jpn);
        assert!(identity_is_sane(&id, &jpn));
    }

    #[test]
    fn sane_identity_passes() {
        let rules = region_rules(Region::Eur);
        assert!(identity_is_sane(&sane_identity(), &rules));
    }

    #[test]
    fn position_out_of_range_fails() {
        let mut gps = GpsData {
            latitude: 100.0,
            longitude: 9.2,
            ..GpsData::default()
        };
        assert!(!position_is_sane(&gps));
        gps.latitude = 45.5;
        gps.longitude = 9.2;
        assert!(position_is_sane(&gps));
        gps.longitude = -181.0;
        assert!(!position_is_sane(&gps));
    }

    #[test]
    fn gate_requires_sane_identity() {
        let mut state = State::default();
        state.gps.latitude = 45.5;
        state.gps.longitude = 9.2;
        // Placeholder identity + gate enabled -> not ready.
        update_identity_ready(&mut state, OPT_IDENTITY_READY_GATE, Region::Eur);
        assert!(!state.identity_ready);

        // Without the gate it becomes ready unconditionally.
        let mut state = State::default();
        update_identity_ready(&mut state, 0, Region::Eur);
        assert!(state.identity_ready);
    }

    #[test]
    fn takeoff_captured_once() {
        let mut state = State::default();
        maybe_capture_takeoff(&mut state, 3, 45.5, 9.2, 120.0);
        assert!(state.takeoff_captured);
        assert_eq!(state.takeoff_lat, 45.5);
        // Second call must not overwrite.
        maybe_capture_takeoff(&mut state, 4, 46.0, 9.0, 500.0);
        assert_eq!(state.takeoff_lat, 45.5);
        assert_eq!(state.takeoff_alt, 120.0);
    }

    #[test]
    fn relative_altitude_derived() {
        let mut state = State::default();
        maybe_capture_takeoff(&mut state, 3, 45.5, 9.2, 20.0);
        state.gps.altitude_msl = 120.0;
        state.gps.altitude_relative = 0.0;
        derive_relative_altitude(&mut state);
        assert_eq!(state.gps.altitude_relative, 100.0);
    }
}
