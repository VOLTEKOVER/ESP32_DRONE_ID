//! ASTM F3411-22a broadcast encoder.
//!
//! Port of the encode side of `odid_common.c` / `rid_output.c`:
//! - `uas::build_uas_data` fills the normative `ODID_UAS_Data` from the neutral
//!   `GpsData`/`Identity` types (including the region gating already applied
//!   by the hourglass hub in `rid-core`);
//! - `pack::build_pack` assembles the WiFi message pack (port of
//!   `odid_message_build_pack` from `wifi.c`);
//! - `ble4::next_message` rotates and encodes a single 25-byte message for the
//!   BLE 4.0 legacy advertisement (port of the encode part of `build_legacy_adv`
//!   from `ble_tx.c`; the 31-byte Service Data AD framing stays in the BSP);
//! - `wifi::build_beacon_frame` and `wifi::build_nan_action_frame` assemble the
//!   full IEEE 802.11 frames (ports of `odid_wifi_build_message_pack_beacon_frame`
//!   and `odid_wifi_build_message_pack_nan_action_frame` from `wifi.c`);
//! - `stubs` declares the not-yet-implemented GB 42590/FRDID encoders (the hub
//!   already falls back to ASTM for those standards).
//!
//! The official Intel C encoder is used for the actual message encoding via
//! `opendroneid-sys`; only the firmware-specific data mapping is ported here.

#![no_std]

use core::ffi::{c_char, c_int};

use opendroneid_sys::{
    ODID_AUTH_MAX_PAGES, ODID_ID_SIZE, ODID_PACK_MAX_MESSAGES, ODID_STR_SIZE, UasData,
};
use rid_interface::odid::{AuthPack, AUTH_PAGE_NONZERO_DATA_SIZE};
use rid_interface::region::Standard;
use rid_interface::types::{Config, GpsData, Identity};
use rid_interface::CStr;

pub mod ble4;
pub mod pack;
pub mod stubs;
pub mod wifi;

/// A fully built ASTM UAS snapshot plus the hub decisions that produced it.
#[derive(Clone, Copy, Debug)]
pub struct BuildOutcome {
    /// Normative ODID data (input for the pack/message encoders).
    pub uas: UasData,
    /// Exclusive standard selected by the configured region.
    pub standard: Standard,
    /// True when `standard` has no encoder yet and ASTM was used as fallback.
    pub fallback: bool,
}

/// Port of `rid_output_build_uas`: region gating + exclusive standard
/// selection via the hub, then the ASTM `ODID_UAS_Data` mapping.
pub fn build_uas(
    gps: &GpsData,
    identity: &Identity,
    config: &Config,
    signed_auth: Option<&AuthPack>,
) -> BuildOutcome {
    let build = rid_core::hub::build_uas(gps, identity, config.region);
    let uas = build_uas_data(&build.gps, &build.gated_identity, signed_auth);
    BuildOutcome {
        uas,
        standard: build.standard,
        fallback: build.fallback,
    }
}

/// `horiz_acc()` from `odid_common.c`.
fn horiz_acc(fix_type: u8, satellites: u8) -> c_int {
    if fix_type >= 4 && satellites >= 15 {
        return opendroneid_sys::ODID_HOR_ACC_1_METER;
    }
    if fix_type >= 4 && satellites >= 10 {
        return opendroneid_sys::ODID_HOR_ACC_3_METER;
    }
    if fix_type >= 4 {
        return opendroneid_sys::ODID_HOR_ACC_10_METER;
    }
    if fix_type >= 3 {
        return opendroneid_sys::ODID_HOR_ACC_10_METER;
    }
    opendroneid_sys::ODID_HOR_ACC_30_METER
}

/// `vert_acc()` from `odid_common.c`.
fn vert_acc(fix_type: u8, satellites: u8) -> c_int {
    if fix_type >= 4 && satellites >= 15 {
        return opendroneid_sys::ODID_VER_ACC_3_METER;
    }
    if fix_type >= 4 && satellites >= 10 {
        return opendroneid_sys::ODID_VER_ACC_10_METER;
    }
    if fix_type >= 4 {
        return opendroneid_sys::ODID_VER_ACC_25_METER;
    }
    if fix_type >= 3 {
        return opendroneid_sys::ODID_VER_ACC_25_METER;
    }
    opendroneid_sys::ODID_VER_ACC_45_METER
}

/// Authentication resolution (port of the C block inside
/// `odid_common_build_uas_data`): MAVLink-relayed pages take priority,
/// otherwise the locally signed pack is used.
pub fn resolve_auth(identity: &Identity, signed: Option<&AuthPack>) -> Option<AuthPack> {
    if identity.has_ext_auth && (identity.ext_auth_last_page as usize) < ODID_AUTH_MAX_PAGES {
        let last = identity.ext_auth_last_page;
        let need = ((1u32 << (last + 1)) - 1) as u16;
        if (identity.ext_auth_pages_received & need) == need {
            let mut pack = AuthPack {
                pages: [rid_interface::odid::AuthPage::default(); ODID_AUTH_MAX_PAGES],
                count: last + 1,
            };
            for p in 0..=last as usize {
                pack.pages[p].data_page = p as u8;
                pack.pages[p].auth_type = identity.ext_auth_type;
                pack.pages[p].last_page_index = identity.ext_auth_last_page;
                pack.pages[p].length = identity.ext_auth_length;
                pack.pages[p].auth_data[..AUTH_PAGE_NONZERO_DATA_SIZE].copy_from_slice(
                    &identity.ext_auth_pages[p][..AUTH_PAGE_NONZERO_DATA_SIZE],
                );
            }
            return Some(pack);
        }
    }
    signed.cloned()
}

/// Copies ASCII bytes into a C `char` array (the FFI structs use `c_char`).
fn copy_chars(dst: &mut [c_char], src: &[u8]) {
    for (d, s) in dst.iter_mut().zip(src) {
        *d = *s as c_char;
    }
}

/// Port of `odid_common_build_uas_data`: fills the normative `ODID_UAS_Data`
/// with `memset(0)` semantics, then the fields the C function sets.
pub fn build_uas_data(
    gps: &GpsData,
    identity: &Identity,
    signed_auth: Option<&AuthPack>,
) -> UasData {
    let mut d: UasData = unsafe { core::mem::zeroed() };

    d.basic_id_valid[0] = 1;
    d.basic_id[0].id_type = identity.id_type as c_int;
    d.basic_id[0].ua_type = identity.ua_type as c_int;
    copy_chars(&mut d.basic_id[0].uas_id, &identity.uas_id[..ODID_ID_SIZE]);

    if !identity.uas_id_2.c_is_empty() {
        d.basic_id_valid[1] = 1;
        d.basic_id[1].id_type = identity.id_type_2 as c_int;
        d.basic_id[1].ua_type = identity.ua_type_2 as c_int;
        copy_chars(&mut d.basic_id[1].uas_id, &identity.uas_id_2[..ODID_ID_SIZE]);
    }

    d.location_valid = 1;
    d.location.latitude = gps.latitude;
    d.location.longitude = gps.longitude;
    d.location.altitude_geo = gps.altitude_msl;
    d.location.height = gps.altitude_relative;
    d.location.altitude_baro = gps.altitude_baro;
    d.location.speed_horizontal = gps.speed;
    d.location.direction = gps.heading as f32;
    d.location.speed_vertical = gps.speed_vertical;
    d.location.horiz_accuracy = horiz_acc(gps.fix_type, gps.satellites);
    d.location.vert_accuracy = vert_acc(gps.fix_type, gps.satellites);

    d.system_valid = 1;
    d.system.operator_latitude = gps.operator_lat;
    d.system.operator_longitude = gps.operator_lon;
    d.system.operator_altitude_geo = gps.operator_alt;
    d.system.area_count = 0;
    d.system.area_radius = 0;

    if !identity.self_id_text.c_is_empty() {
        d.self_id_valid = 1;
        d.self_id.desc_type = if identity.has_self_id {
            identity.self_id_desc_type as c_int
        } else {
            opendroneid_sys::ODID_DESC_TYPE_TEXT
        };
        copy_chars(&mut d.self_id.desc[..ODID_STR_SIZE], &identity.self_id_text);
    }

    if !identity.operator_id.c_is_empty() {
        d.operator_id_valid = 1;
        d.operator_id.operator_id_type = opendroneid_sys::ODID_OPERATOR_ID;
        copy_chars(
            &mut d.operator_id.operator_id[..ODID_ID_SIZE],
            &identity.operator_id[..ODID_ID_SIZE],
        );
    }

    if let Some(auth) = resolve_auth(identity, signed_auth) {
        let auth_pages = auth.count as usize;
        let fixed = 1
            + if !identity.uas_id_2.c_is_empty() { 1 } else { 0 }
            + 1
            + if !identity.self_id_text.c_is_empty() { 1 } else { 0 }
            + 1
            + 1;
        if auth_pages > 0 && auth_pages <= ODID_PACK_MAX_MESSAGES - fixed {
            for p in 0..auth_pages {
                d.auth[p].data_page = auth.pages[p].data_page;
                d.auth[p].auth_type = auth.pages[p].auth_type as c_int;
                d.auth[p].last_page_index = auth.pages[p].last_page_index;
                d.auth[p].length = auth.pages[p].length;
                d.auth[p].timestamp = auth.pages[p].timestamp;
                d.auth[p].auth_data
                    .copy_from_slice(&auth.pages[p].auth_data[..AUTH_PAGE_NONZERO_DATA_SIZE + 1]);
                d.auth_valid[p] = 1;
            }
        }
    }

    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use rid_interface::fixed_str;

    /// ASCII comparison of a C `char` array against a byte literal.
    fn eq_chars(dst: &[core::ffi::c_char], src: &[u8]) -> bool {
        dst.iter().zip(src).all(|(&a, &b)| a as u8 == b)
    }

    fn gps() -> GpsData {
        GpsData {
            latitude: 45.30405,
            longitude: 11.95375,
            altitude_msl: 123.4,
            altitude_relative: 60.0,
            altitude_baro: 122.0,
            speed: 12.0,
            speed_vertical: 0.0,
            heading: 90,
            fix_type: 4,
            satellites: 12,
            armed: true,
            operator_lat: 45.30,
            operator_lon: 11.95,
            operator_alt: 110.0,
        }
    }

    fn identity() -> Identity {
        Identity {
            id_type: 1,
            ua_type: 2,
            uas_id: fixed_str("ESP32-RID-001"),
            operator_id: fixed_str("OP-123456"),
            ..Identity::default()
        }
    }

    #[test]
    fn fill_odid_uas_data_matches_c_odid_common() {
        let g = gps();
        let id = identity();
        let d = build_uas_data(&g, &id, None);

        assert_eq!(d.basic_id_valid[0], 1);
        assert_eq!(d.basic_id[0].id_type, 1);
        assert_eq!(d.basic_id[0].ua_type, 2);
        assert!(eq_chars(&d.basic_id[0].uas_id, b"ESP32-RID-001"));
        assert_eq!(d.basic_id_valid[1], 0, "no second id");
        assert_eq!(d.basic_id[1].uas_id[0], 0);

        assert_eq!(d.location_valid, 1);
        assert_eq!(d.location.latitude, 45.30405);
        assert_eq!(d.location.longitude, 11.95375);
        assert_eq!(d.location.altitude_geo, 123.4);
        assert_eq!(d.location.height, 60.0);
        assert_eq!(d.location.altitude_baro, 122.0);
        assert_eq!(d.location.speed_horizontal, 12.0);
        assert_eq!(d.location.direction, 90.0);
        assert_eq!(d.location.speed_vertical, 0.0);
        // fix_type 4 + 12 sats -> 3 m horiz / 10 m vert
        assert_eq!(d.location.horiz_accuracy, opendroneid_sys::ODID_HOR_ACC_3_METER);
        assert_eq!(d.location.vert_accuracy, opendroneid_sys::ODID_VER_ACC_10_METER);

        assert_eq!(d.system_valid, 1);
        assert_eq!(d.system.operator_latitude, 45.30);
        assert_eq!(d.system.operator_longitude, 11.95);
        assert_eq!(d.system.operator_altitude_geo, 110.0);
        assert_eq!(d.system.area_count, 0, "odid_common sets AreaCount=0");
        assert_eq!(d.system.area_radius, 0);

        assert_eq!(d.self_id_valid, 0);
        assert_eq!(d.operator_id_valid, 1);
        assert!(eq_chars(&d.operator_id.operator_id, b"OP-123456"));
        assert!(!d.auth_valid.iter().any(|&v| v != 0), "no auth");
    }

    #[test]
    fn accuracy_lookup_table() {
        assert_eq!(horiz_acc(4, 15), opendroneid_sys::ODID_HOR_ACC_1_METER);
        assert_eq!(horiz_acc(4, 10), opendroneid_sys::ODID_HOR_ACC_3_METER);
        assert_eq!(horiz_acc(4, 5), opendroneid_sys::ODID_HOR_ACC_10_METER);
        assert_eq!(horiz_acc(3, 20), opendroneid_sys::ODID_HOR_ACC_10_METER);
        assert_eq!(horiz_acc(2, 20), opendroneid_sys::ODID_HOR_ACC_30_METER);
        assert_eq!(vert_acc(4, 15), opendroneid_sys::ODID_VER_ACC_3_METER);
        assert_eq!(vert_acc(4, 10), opendroneid_sys::ODID_VER_ACC_10_METER);
        assert_eq!(vert_acc(4, 5), opendroneid_sys::ODID_VER_ACC_25_METER);
        assert_eq!(vert_acc(3, 5), opendroneid_sys::ODID_VER_ACC_25_METER);
        assert_eq!(vert_acc(2, 5), opendroneid_sys::ODID_VER_ACC_45_METER);
    }

    #[test]
    fn second_id_and_self_id_fill() {
        let g = gps();
        let mut id = identity();
        id.uas_id_2 = fixed_str("SN-2ND-ID-000");
        id.id_type_2 = 3;
        id.ua_type_2 = 4;
        id.self_id_text = fixed_str("TEST-DRONE");
        let d = build_uas_data(&g, &id, None);
        assert_eq!(d.basic_id_valid[1], 1);
        assert_eq!(d.basic_id[1].id_type, 3);
        assert_eq!(d.basic_id[1].ua_type, 4);
        assert!(eq_chars(&d.basic_id[1].uas_id, b"SN-2ND-ID-000"));
        assert_eq!(d.self_id_valid, 1);
        assert_eq!(d.self_id.desc_type, opendroneid_sys::ODID_DESC_TYPE_TEXT);
        assert!(eq_chars(&d.self_id.desc, b"TEST-DRONE"));
    }

    #[test]
    fn self_id_uses_mavlink_desc_type_when_present() {
        let g = gps();
        let mut id = identity();
        id.self_id_text = fixed_str("HELP");
        id.has_self_id = true;
        id.self_id_desc_type = 2;
        let d = build_uas_data(&g, &id, None);
        assert_eq!(d.self_id.desc_type, 2);
    }

    #[test]
    fn ext_auth_pages_take_priority_over_signed() {
        let g = gps();
        let mut id = identity();
        id.has_ext_auth = true;
        id.ext_auth_last_page = 2;
        id.ext_auth_type = 1;
        id.ext_auth_length = 63;
        id.ext_auth_pages_received = 0b111;
        id.ext_auth_pages[0][..4].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);
        id.ext_auth_pages[1][..2].copy_from_slice(&[0x11, 0x22]);
        id.ext_auth_pages[2][..2].copy_from_slice(&[0x33, 0x44]);

        // A signed pack that must be ignored because ext pages are complete.
        let signed = AuthPack {
            pages: [rid_interface::odid::AuthPage::default(); ODID_AUTH_MAX_PAGES],
            count: 1,
        };

        let d = build_uas_data(&g, &id, Some(&signed));
        assert_eq!(d.auth_valid[0], 1);
        assert_eq!(d.auth_valid[1], 1);
        assert_eq!(d.auth_valid[2], 1);
        assert_eq!(d.auth_valid[3], 0);
        assert_eq!(d.auth[0].data_page, 0);
        assert_eq!(d.auth[0].auth_type, 1);
        assert_eq!(d.auth[0].last_page_index, 2);
        assert_eq!(d.auth[0].length, 63);
        assert_eq!(&d.auth[0].auth_data[..4], &[0xAA, 0xBB, 0xCC, 0xDD]);
        assert_eq!(&d.auth[1].auth_data[..2], &[0x11, 0x22]);
        assert_eq!(&d.auth[2].auth_data[..2], &[0x33, 0x44]);
    }

    #[test]
    fn incomplete_ext_auth_falls_back_to_signed() {
        let g = gps();
        let mut id = identity();
        id.has_ext_auth = true;
        id.ext_auth_last_page = 2;
        id.ext_auth_pages_received = 0b011; // page 2 missing
        let signed = AuthPack {
            pages: [rid_interface::odid::AuthPage::default(); ODID_AUTH_MAX_PAGES],
            count: 1,
        };
        let d = build_uas_data(&g, &id, Some(&signed));
        assert_eq!(d.auth_valid[0], 1);
        assert_eq!(d.auth_valid[1], 0, "only the signed page 0");
    }

    #[test]
    fn too_many_auth_pages_are_dropped() {
        let g = gps();
        let mut id = identity();
        // Self-ID makes `fixed` = 5, so 9 - 5 = 4 slots: 5 pages are too many.
        id.self_id_text = fixed_str("TEXT");
        let mut pack = AuthPack {
            pages: [rid_interface::odid::AuthPage::default(); ODID_AUTH_MAX_PAGES],
            count: 5,
        };
        for p in 0..5 {
            pack.pages[p].data_page = p as u8;
        }
        let d = build_uas_data(&g, &id, Some(&pack));
        assert!(!d.auth_valid.iter().any(|&v| v != 0));
    }

    #[test]
    fn build_uas_runs_hub_gating() {
        // China region drops Operator ID / Self-ID / second Basic ID.
        let g = gps();
        let mut id = identity();
        id.operator_id = fixed_str("OP-123456");
        id.self_id_text = fixed_str("TEXT");
        id.uas_id_2 = fixed_str("SECOND");
        let cfg = Config {
            region: rid_interface::region::Region::Chn,
            ..Config::default()
        };
        let out = build_uas(&g, &id, &cfg, None);
        assert_eq!(out.standard, rid_interface::region::Standard::ChnGb);
        assert!(out.fallback, "GB has no encoder, ASTM fallback");
        assert_eq!(out.uas.operator_id_valid, 0);
        assert_eq!(out.uas.self_id_valid, 0);
        assert_eq!(out.uas.basic_id_valid[1], 0);
        assert_eq!(out.uas.basic_id_valid[0], 1);
    }
}

