//! Runtime state, port of `rid_state_t` from `esp_remote_id.h` and of
//! `state_to_json()` from `web_config.c` (the `/api/status` response).

use alloc::string::String;
use rid_interface::{GpsData, Identity, Protocol, Standard, Stats};
use serde_json::{Map, Value};

use crate::json::{num1, num6};

/// Mirror of `ESP_RID_VERSION` from `esp_remote_id.h`.
pub const FW_VERSION: &str = "1.0.0";

/// Full BSP runtime snapshot, mirror of `rid_state_t`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct State {
    pub gps: GpsData,
    pub identity: Identity,
    pub active_protocol: Protocol,
    pub last_update_ms: u32,
    pub transmissions_count: u32,
    pub wifi_bcn_count: u32,
    pub wifi_nan_count: u32,
    pub ble4_count: u32,
    pub ble5_count: u32,
    pub gps_valid: bool,
    pub identity_ready: bool,
    pub mavlink_armed: bool,
    pub mavlink_sysid: u32,
    pub operator_lat: f64,
    pub operator_lon: f64,
    pub operator_alt: f32,
    pub operator_position_updated_ms: u32,
    pub operator_location_type: u8,
    pub auth_enabled: bool,
    pub active_standard: Standard,
    pub standard_fallback: bool,
    pub takeoff_lat: f64,
    pub takeoff_lon: f64,
    pub takeoff_alt: f32,
    pub takeoff_captured: bool,
    pub stats: Stats,
}

/// Same names as `g_standard_names` in `rid_output.c`, in `rid_standard_t` order.
const STANDARD_NAMES: [&str; 3] = ["ASTM F3411-22a", "China GB 42590", "FRDID"];

/// Port of `rid_output_standard_name()`.
pub fn standard_name(s: Standard) -> &'static str {
    STANDARD_NAMES.get(s as usize).copied().unwrap_or("?")
}

/// Serializes `s` to the JSON served by `/api/status`. Port of
/// `state_to_json()` with the C field order and `%.1f`/`%.6f` formatting.
pub fn state_to_json(s: &State) -> String {
    let mut m = Map::new();

    m.insert("fw_version".into(), Value::from(FW_VERSION));
    m.insert("protocol".into(), Value::from(s.active_protocol as u8));
    m.insert("gps_valid".into(), Value::from(s.gps_valid));
    m.insert("lat".into(), num6(s.gps.latitude));
    m.insert("lon".into(), num6(s.gps.longitude));
    m.insert("standard".into(), Value::from(standard_name(s.active_standard)));
    m.insert("standard_fallback".into(), Value::from(s.standard_fallback));
    m.insert("alt".into(), num1(s.gps.altitude_msl as f64));
    m.insert("speed".into(), num1(s.gps.speed as f64));
    m.insert("heading".into(), Value::from(s.gps.heading));
    m.insert("satellites".into(), Value::from(s.gps.satellites));
    m.insert("fix_type".into(), Value::from(s.gps.fix_type));
    m.insert("tx_total".into(), Value::from(s.transmissions_count));
    m.insert("tx_wifi_bcn".into(), Value::from(s.wifi_bcn_count));
    m.insert("tx_wifi_nan".into(), Value::from(s.wifi_nan_count));
    m.insert("tx_ble4".into(), Value::from(s.ble4_count));
    m.insert("tx_ble5".into(), Value::from(s.ble5_count));
    m.insert("takeoff_captured".into(), Value::from(s.takeoff_captured));
    m.insert("takeoff_lat".into(), num6(s.takeoff_lat));
    m.insert("takeoff_lon".into(), num6(s.takeoff_lon));
    m.insert("takeoff_alt".into(), num1(s.takeoff_alt as f64));
    m.insert("uptime_ms".into(), Value::from(s.last_update_ms));
    m.insert("ticks".into(), Value::from(s.stats.ticks));
    m.insert("gps_updates".into(), Value::from(s.stats.gps_updates));
    m.insert("gps_discarded".into(), Value::from(s.stats.gps_discarded));
    m.insert("parse_errors".into(), Value::from(s.stats.parse_errors));
    m.insert("signatures_total".into(), Value::from(s.stats.signatures_total));
    m.insert("signatures_ok".into(), Value::from(s.stats.signatures_ok));
    m.insert("wifi_tx_fail".into(), Value::from(s.stats.wifi_tx_fail));
    m.insert("ble_tx_fail".into(), Value::from(s.stats.ble_tx_fail));
    m.insert("ota_count".into(), Value::from(s.stats.ota_count));

    serde_json::to_string(&Value::Object(m)).expect("State serialization")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_to_json_default() {
        let out = state_to_json(&State::default());
        assert!(
            out.starts_with(
                "{\"fw_version\":\"1.0.0\",\"protocol\":0,\"gps_valid\":false,\"lat\":0.000000,"
            ),
            "{}",
            out
        );
        assert!(out.contains("\"standard\":\"ASTM F3411-22a\""), "{}", out);
        assert!(out.contains("\"standard_fallback\":false"), "{}", out);
        assert!(out.contains("\"tx_total\":0"), "{}", out);
        assert!(out.contains("\"heading\":0"), "{}", out);
        assert!(out.contains("\"uptime_ms\":0"), "{}", out);
        assert!(out.contains("\"ticks\":0"), "{}", out);
        assert!(out.contains("\"gps_updates\":0"), "{}", out);
        assert!(out.contains("\"parse_errors\":0"), "{}", out);
        assert!(out.contains("\"wifi_tx_fail\":0"), "{}", out);
        assert!(out.contains("\"ota_count\":0"), "{}", out);
        assert!(out.ends_with('}'), "{}", out);
    }

    #[test]
    fn state_to_json_values() {
        let mut s = State::default();
        s.gps.latitude = 45.304;
        s.gps.longitude = 9.2123;
        s.gps.altitude_msl = 123.4;
        s.gps.speed = 12.5;
        s.gps.heading = -45;
        s.gps.satellites = 12;
        s.gps.fix_type = 3;
        s.active_protocol = Protocol::Mavlink;
        s.gps_valid = true;
        s.active_standard = Standard::ChnGb;
        s.standard_fallback = true;
        s.transmissions_count = 12345;
        s.wifi_bcn_count = 100;
        s.wifi_nan_count = 20;
        s.ble4_count = 3;
        s.ble5_count = 4;
        s.takeoff_captured = true;
        s.takeoff_lat = 41.9;
        s.takeoff_lon = 12.5;
        s.takeoff_alt = 99.5;
        s.last_update_ms = 123456;
        s.stats.ticks = 500;
        s.stats.gps_updates = 480;
        s.stats.gps_discarded = 20;
        s.stats.parse_errors = 3;
        s.stats.signatures_total = 10;
        s.stats.signatures_ok = 9;
        s.stats.wifi_tx_fail = 1;
        s.stats.ble_tx_fail = 0;
        s.stats.ota_count = 2;

        let out = state_to_json(&s);
        assert!(out.contains("\"protocol\":1"), "{}", out);
        assert!(out.contains("\"gps_valid\":true"), "{}", out);
        assert!(out.contains("\"lat\":45.304000"), "{}", out);
        assert!(out.contains("\"lon\":9.212300"), "{}", out);
        assert!(out.contains("\"standard\":\"China GB 42590\""), "{}", out);
        assert!(out.contains("\"standard_fallback\":true"), "{}", out);
        assert!(out.contains("\"alt\":123.4"), "{}", out);
        assert!(out.contains("\"speed\":12.5"), "{}", out);
        assert!(out.contains("\"heading\":-45"), "{}", out);
        assert!(out.contains("\"satellites\":12"), "{}", out);
        assert!(out.contains("\"fix_type\":3"), "{}", out);
        assert!(out.contains("\"tx_total\":12345"), "{}", out);
        assert!(out.contains("\"tx_wifi_bcn\":100"), "{}", out);
        assert!(out.contains("\"tx_wifi_nan\":20"), "{}", out);
        assert!(out.contains("\"tx_ble4\":3"), "{}", out);
        assert!(out.contains("\"tx_ble5\":4"), "{}", out);
        assert!(out.contains("\"takeoff_captured\":true"), "{}", out);
        assert!(out.contains("\"takeoff_lat\":41.900000"), "{}", out);
        assert!(out.contains("\"takeoff_lon\":12.500000"), "{}", out);
        assert!(out.contains("\"takeoff_alt\":99.5"), "{}", out);
        assert!(out.contains("\"uptime_ms\":123456"), "{}", out);
        assert!(out.contains("\"ticks\":500"), "{}", out);
        assert!(out.contains("\"gps_updates\":480"), "{}", out);
        assert!(out.contains("\"gps_discarded\":20"), "{}", out);
        assert!(out.contains("\"parse_errors\":3"), "{}", out);
        assert!(out.contains("\"signatures_total\":10"), "{}", out);
        assert!(out.contains("\"signatures_ok\":9"), "{}", out);
        assert!(out.contains("\"wifi_tx_fail\":1"), "{}", out);
        assert!(out.contains("\"ota_count\":2"), "{}", out);

        // Always valid JSON, parseable back.
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["heading"], -45);
        assert_eq!(v["fw_version"], "1.0.0");
    }

    #[test]
    fn standard_name_matches_c() {
        assert_eq!(standard_name(Standard::Astm), "ASTM F3411-22a");
        assert_eq!(standard_name(Standard::ChnGb), "China GB 42590");
        assert_eq!(standard_name(Standard::Frdid), "FRDID");
    }
}
