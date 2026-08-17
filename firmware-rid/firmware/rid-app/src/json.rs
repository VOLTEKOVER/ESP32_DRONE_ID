//! Web configuration JSON, port of `apply_json()` and `config_to_json()`
//! from `web_config.c`.
//!
//! cJSON is replaced by serde_json (the standard no_std+alloc choice). The
//! per-field validation rules of the C are kept verbatim: fields that are
//! absent or of the wrong type are left untouched and numeric ranges are
//! enforced. The `%.1f`/`%.6f` float formatting of the C is preserved by
//! pre-formatting the value and wrapping it in a `serde_json::Number`
//! (`arbitrary_precision` keeps the exact digits). Unlike cJSON, serde_json
//! escapes strings on output, so the web UI always receives well-formed JSON.

use alloc::format;
use alloc::string::String;
use rid_interface::{Protocol, Region, MAX_KEY_LEN, MAX_STR_LEN, NUM_KEYS};
use serde_json::{Map, Number, Value};

use crate::config::{cstr, BspConfig, AUTH_KEY_MAX_LEN, NUM_LIGHTING_PINS};

/// Same names as `g_region_names` in `rid_output.c`, in `rid_region_t` order.
const REGION_NAMES: [&str; Region::COUNT] = [
    "AUTO", "EUR", "FAA", "JPN", "SGP", "KOR", "CHN", "CAN", "AUS", "BRA", "NZL",
];

/// Port of `rid_output_region_name()`.
pub fn region_name(r: Region) -> &'static str {
    REGION_NAMES.get(r as usize).copied().unwrap_or("?")
}

/// Case-insensitive region name lookup, port of the `strcasecmp` loops in
/// `web_config.c` and `cli.c`.
pub fn parse_region_name(s: &str) -> Option<Region> {
    for (r, name) in REGION_NAMES.iter().enumerate() {
        if s.eq_ignore_ascii_case(name) {
            return Region::from_raw(r as u8);
        }
    }
    None
}

/// Applies a config JSON to `cfg`. Port of `apply_json()`. Returns `false`
/// when the document cannot be parsed (the C silently returns in that case).
pub fn apply_json(cfg: &mut BspConfig, json: &str) -> bool {
    let Some(Value::Object(root)) = serde_json::from_str(json).ok() else {
        return false;
    };

    // cJSON string copy: `strncpy(dst, s, cap)` + NUL at `cap` (zero-pads).
    let short = |dst: &mut [u8], s: &str, cap: usize| {
        let bytes = s.as_bytes();
        let n = bytes.len().min(cap);
        dst[..n].copy_from_slice(&bytes[..n]);
        dst[n..].fill(0);
    };

    // cJSON `valueint` = (int)valuedouble (truncation toward zero).
    let int = |k: &str| -> Option<i64> { root.get(k).and_then(Value::as_f64).map(|f| f as i64) };
    // cJSON `valuedouble`.
    let dbl = |k: &str| -> Option<f64> { root.get(k).and_then(Value::as_f64) };
    // cJSON string items (`cJSON_IsString`).
    let str_ = |k: &str| -> Option<&str> { root.get(k).and_then(Value::as_str) };

    if let Some(s) = str_("uas_id") {
        short(&mut cfg.uas_id, s, MAX_STR_LEN);
    }
    if let Some(s) = str_("operator_id") {
        short(&mut cfg.operator_id, s, MAX_STR_LEN);
    }
    if let Some(s) = str_("self_id_text") {
        short(&mut cfg.self_id_text, s, MAX_STR_LEN);
    }
    if let Some(s) = str_("uas_id_2") {
        short(&mut cfg.uas_id_2, s, MAX_STR_LEN);
    }

    if let Some(v) = int("id_type") {
        cfg.id_type = v as u8;
    }
    if let Some(v) = int("ua_type") {
        cfg.ua_type = v as u8;
    }
    if let Some(v) = int("id_type_2") {
        cfg.id_type_2 = v as u8;
    }
    if let Some(v) = int("ua_type_2") {
        cfg.ua_type_2 = v as u8;
    }

    if let Some(v) = int("protocol") {
        // The C maps 1..=4 to MAVLINK/MSP/NMEA/NONE, everything else to AUTO.
        cfg.protocol = match v {
            1 => Protocol::Mavlink,
            2 => Protocol::Msp,
            3 => Protocol::Nmea,
            4 => Protocol::None,
            _ => Protocol::Auto,
        };
    }

    if let Some(v) = int("region") {
        if (0..=Region::Nzl as i64).contains(&v) {
            cfg.region = Region::from_raw(v as u8).unwrap_or(Region::Auto);
        }
    } else if let Some(s) = str_("region") {
        if let Some(r) = parse_region_name(s) {
            cfg.region = r;
        }
    }

    if let Some(v) = int("tx_modes") {
        cfg.tx_modes = v as u8;
    }
    if let Some(v) = int("wifi_channel") {
        if (1..=13).contains(&v) {
            cfg.wifi_channel = v as u8;
        }
    }
    if let Some(v) = int("webserver_en") {
        cfg.webserver_en = v as u8;
    }
    if let Some(v) = int("mavlink_sysid") {
        cfg.mavlink_sysid = v as u8;
    }
    if let Some(v) = int("bcast_powerup") {
        cfg.bcast_powerup = v as u8;
    }
    if let Some(v) = int("options") {
        cfg.options = v as u16;
    }

    if let Some(v) = int("lock_level") {
        if v >= 2 {
            // The C burns the eFuse here (one-time permanent lock). That is a
            // hardware step owned by the BSP chip layer; the config value is
            // forced to 2 exactly like the C.
            cfg.lock_level = 2;
        } else if v >= 1 {
            cfg.lock_level = v as i8;
        } else {
            cfg.lock_level = 0;
        }
    }

    if let Some(v) = int("led_r_gpio") {
        cfg.led_r_gpio = v as i8;
    }
    if let Some(v) = int("led_g_gpio") {
        cfg.led_g_gpio = v as i8;
    }
    if let Some(v) = int("led_b_gpio") {
        cfg.led_b_gpio = v as i8;
    }

    if let Some(v) = int("uart_port") {
        cfg.uart_port = v as u8;
    }
    if let Some(v) = int("tx_pin") {
        cfg.tx_pin = v as u8;
    }
    if let Some(v) = int("rx_pin") {
        cfg.rx_pin = v as u8;
    }

    if let Some(v) = int("baud_rate") {
        if v > 0 {
            cfg.baud_rate = v as u32;
        }
    }

    if let Some(v) = dbl("wifi_power_dbm") {
        if (2.0..=20.0).contains(&v) {
            cfg.wifi_power_dbm = v as f32;
        }
    }
    if let Some(v) = dbl("wifi_bcn_rate_hz") {
        if (0.0..=5.0).contains(&v) {
            cfg.wifi_bcn_rate_hz = v as f32;
        }
    }
    if let Some(v) = dbl("wifi_nan_rate_hz") {
        if (0.0..=5.0).contains(&v) {
            cfg.wifi_nan_rate_hz = v as f32;
        }
    }
    if let Some(v) = dbl("ble4_rate_hz") {
        if (0.0..=5.0).contains(&v) {
            cfg.ble4_rate_hz = v as f32;
        }
    }
    if let Some(v) = dbl("ble4_power_dbm") {
        if (-27.0..=18.0).contains(&v) {
            cfg.ble4_power_dbm = v as f32;
        }
    }
    if let Some(v) = dbl("ble5_rate_hz") {
        if (0.0..=5.0).contains(&v) {
            cfg.ble5_rate_hz = v as f32;
        }
    }
    if let Some(v) = dbl("ble5_power_dbm") {
        if (-27.0..=18.0).contains(&v) {
            cfg.ble5_power_dbm = v as f32;
        }
    }

    if let Some(v) = dbl("operator_lat") {
        cfg.operator_lat = v;
    }
    if let Some(v) = dbl("operator_lon") {
        cfg.operator_lon = v;
    }
    if let Some(v) = dbl("operator_alt") {
        cfg.operator_alt = v as f32;
    }

    if let Some(s) = str_("wifi_ssid") {
        short(&mut cfg.wifi_ssid, s, MAX_STR_LEN);
    }
    if let Some(s) = str_("wifi_password") {
        short(&mut cfg.wifi_password, s, MAX_STR_LEN);
    }

    if let Some(v) = int("ws2812_gpio") {
        cfg.ws2812_gpio = v as i8;
    }
    if let Some(v) = int("ws2812_brightness") {
        cfg.ws2812_brightness = v as u8;
    }

    for i in 0..NUM_LIGHTING_PINS {
        if let Some(v) = int(&format!("lighting_pin_{}", i)) {
            cfg.lighting_pins[i] = v as i8;
        }
        if let Some(v) = int(&format!("lighting_pattern_{}", i)) {
            cfg.lighting_patterns[i] = v as u8;
        }
        if let Some(v) = int(&format!("lighting_phase_{}", i)) {
            cfg.lighting_phase_offsets[i] = v as i16;
        }
    }

    if let Some(v) = int("dronecan_rx_gpio") {
        cfg.dronecan_rx_gpio = v as i8;
    }
    if let Some(v) = int("dronecan_tx_gpio") {
        cfg.dronecan_tx_gpio = v as i8;
    }
    if let Some(v) = int("dronecan_bitrate") {
        if v > 0 {
            cfg.dronecan_bitrate = v as u32;
        }
    }

    if let Some(v) = int("mavlink_usb_enable") {
        cfg.mavlink_usb_enable = v != 0;
    }

    if let Some(v) = int("ota_trigger_gpio") {
        cfg.ota_trigger_gpio = v as i8;
    }

    if let Some(v) = int("start_delay_ms") {
        if v >= 0 {
            cfg.start_delay_ms = v as u32;
        }
    }

    if let Some(s) = str_("auth_private_key") {
        let bytes = s.as_bytes();
        let n = bytes.len().min(AUTH_KEY_MAX_LEN - 1);
        cfg.auth_private_key[..n].copy_from_slice(&bytes[..n]);
        cfg.auth_private_key[n..].fill(0);
    }

    for i in 1..=NUM_KEYS {
        let key = format!("public_key_{}", i);
        if let Some(s) = str_(&key) {
            let bytes = s.as_bytes();
            let n = bytes.len().min(MAX_KEY_LEN);
            let dst = &mut cfg.public_keys[i - 1];
            dst[..n].copy_from_slice(&bytes[..n]);
            dst[n..].fill(0);
        }
    }

    true
}

/// Serializes `c` to the JSON served by `/api/config`. Port of
/// `config_to_json()`. The field order and the `%.1f`/`%.6f` formatting of
/// the C are preserved; string values are escaped (a fix over cJSON, which
/// emitted them raw).
pub fn config_to_json(c: &BspConfig) -> String {
    let mut m = Map::new();

    m.insert("protocol".into(), Value::from(c.protocol as u8));
    m.insert("region".into(), Value::from(region_name(c.region)));
    m.insert("uas_id".into(), Value::from(cstr(&c.uas_id)));
    m.insert("id_type".into(), Value::from(c.id_type));
    m.insert("ua_type".into(), Value::from(c.ua_type));
    m.insert("operator_id".into(), Value::from(cstr(&c.operator_id)));
    m.insert("self_id_text".into(), Value::from(cstr(&c.self_id_text)));
    m.insert("uas_id_2".into(), Value::from(cstr(&c.uas_id_2)));
    m.insert("id_type_2".into(), Value::from(c.id_type_2));
    m.insert("ua_type_2".into(), Value::from(c.ua_type_2));
    m.insert("tx_modes".into(), Value::from(c.tx_modes));
    m.insert("wifi_channel".into(), Value::from(c.wifi_channel));
    m.insert("wifi_power_dbm".into(), num1(c.wifi_power_dbm as f64));
    m.insert("wifi_bcn_rate_hz".into(), num1(c.wifi_bcn_rate_hz as f64));
    m.insert("wifi_nan_rate_hz".into(), num1(c.wifi_nan_rate_hz as f64));
    m.insert("ble4_rate_hz".into(), num1(c.ble4_rate_hz as f64));
    m.insert("ble4_power_dbm".into(), num1(c.ble4_power_dbm as f64));
    m.insert("ble5_rate_hz".into(), num1(c.ble5_rate_hz as f64));
    m.insert("ble5_power_dbm".into(), num1(c.ble5_power_dbm as f64));
    m.insert("wifi_ssid".into(), Value::from(cstr(&c.wifi_ssid)));
    m.insert("wifi_password".into(), Value::from(cstr(&c.wifi_password)));
    m.insert("webserver_en".into(), Value::from(c.webserver_en));
    m.insert("baud_rate".into(), Value::from(c.baud_rate));
    m.insert("mavlink_sysid".into(), Value::from(c.mavlink_sysid));
    m.insert("bcast_powerup".into(), Value::from(c.bcast_powerup));
    m.insert("operator_lat".into(), num6(c.operator_lat));
    m.insert("operator_lon".into(), num6(c.operator_lon));
    m.insert("operator_alt".into(), num1(c.operator_alt as f64));
    m.insert("options".into(), Value::from(c.options));
    m.insert("lock_level".into(), Value::from(c.lock_level));
    m.insert("led_r_gpio".into(), Value::from(c.led_r_gpio));
    m.insert("led_g_gpio".into(), Value::from(c.led_g_gpio));
    m.insert("led_b_gpio".into(), Value::from(c.led_b_gpio));
    m.insert("uart_port".into(), Value::from(c.uart_port));
    m.insert("tx_pin".into(), Value::from(c.tx_pin));
    m.insert("rx_pin".into(), Value::from(c.rx_pin));
    m.insert("ws2812_gpio".into(), Value::from(c.ws2812_gpio));
    m.insert("ws2812_brightness".into(), Value::from(c.ws2812_brightness));
    m.insert("dronecan_rx_gpio".into(), Value::from(c.dronecan_rx_gpio));
    m.insert("dronecan_tx_gpio".into(), Value::from(c.dronecan_tx_gpio));
    m.insert("dronecan_bitrate".into(), Value::from(c.dronecan_bitrate));
    m.insert("mavlink_usb_enable".into(), Value::from(c.mavlink_usb_enable));
    m.insert("ota_trigger_gpio".into(), Value::from(c.ota_trigger_gpio));
    m.insert("start_delay_ms".into(), Value::from(c.start_delay_ms));
    for i in 1..=NUM_KEYS {
        m.insert(format!("public_key_{}", i), Value::from(cstr(&c.public_keys[i - 1])));
    }
    for i in 0..NUM_LIGHTING_PINS {
        m.insert(format!("lighting_pin_{}", i), Value::from(c.lighting_pins[i]));
    }
    for i in 0..NUM_LIGHTING_PINS {
        m.insert(format!("lighting_pattern_{}", i), Value::from(c.lighting_patterns[i]));
    }
    for i in 0..NUM_LIGHTING_PINS {
        m.insert(format!("lighting_phase_{}", i), Value::from(c.lighting_phase_offsets[i]));
    }

    serde_json::to_string(&Value::Object(m)).expect("BspConfig serialization")
}

/// A JSON number pre-formatted with `%.1f`, matching the C `snprintf` output.
pub(crate) fn num1(v: f64) -> Value {
    Value::Number(Number::from_string_unchecked(format!("{:.1}", v)))
}

/// A JSON number pre-formatted with `%.6f`, matching the C `snprintf` output.
pub(crate) fn num6(v: f64) -> Value {
    Value::Number(Number::from_string_unchecked(format!("{:.6}", v)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rid_interface::fixed_str;

    #[test]
    fn apply_json_basic_fields() {
        let mut c = BspConfig::default();
        let ok = apply_json(
            &mut c,
            r#"{"uas_id":"NEO-X-77","operator_id":"OP-ROMA","id_type":1,"ua_type":2,
                "wifi_channel":11,"wifi_power_dbm":12.5,"baud_rate":115200,
                "region":"jpn","protocol":3,"mavlink_usb_enable":1}"#,
        );
        assert!(ok);
        assert_eq!(cstr(&c.uas_id), "NEO-X-77");
        assert_eq!(cstr(&c.operator_id), "OP-ROMA");
        assert_eq!(c.id_type, 1);
        assert_eq!(c.ua_type, 2);
        assert_eq!(c.wifi_channel, 11);
        assert_eq!(c.wifi_power_dbm, 12.5);
        assert_eq!(c.baud_rate, 115200);
        assert_eq!(c.region, Region::Jpn);
        assert_eq!(c.protocol, Protocol::Nmea);
        assert!(c.mavlink_usb_enable);
    }

    #[test]
    fn apply_json_protocol_and_region_rules() {
        let mut c = BspConfig::default();
        assert!(apply_json(&mut c, r#"{"protocol":4}"#));
        assert_eq!(c.protocol, Protocol::None);
        assert!(apply_json(&mut c, r#"{"protocol":9}"#));
        assert_eq!(c.protocol, Protocol::Auto);
        assert!(apply_json(&mut c, r#"{"protocol":0}"#));
        assert_eq!(c.protocol, Protocol::Auto);
        assert!(apply_json(&mut c, r#"{"protocol":"mavlink"}"#));
        assert_eq!(c.protocol, Protocol::Auto);

        // Region number in range, then out of range (left unchanged).
        assert!(apply_json(&mut c, r#"{"region":6}"#));
        assert_eq!(c.region, Region::Chn);
        assert!(apply_json(&mut c, r#"{"region":42}"#));
        assert_eq!(c.region, Region::Chn);
        // Region string, case-insensitive; unknown name left unchanged.
        assert!(apply_json(&mut c, r#"{"region":"nzl"}"#));
        assert_eq!(c.region, Region::Nzl);
        assert!(apply_json(&mut c, r#"{"region":"XXX"}"#));
        assert_eq!(c.region, Region::Nzl);
    }

    #[test]
    fn apply_json_rejects_out_of_range() {
        let mut c = BspConfig::default();
        assert!(apply_json(
            &mut c,
            r#"{"wifi_channel":0,"wifi_channel2":14,"baud_rate":-5,
                "wifi_power_dbm":25,"wifi_power_dbm_low":1.9,"ble4_power_dbm":-30}"#,
        ));
        assert_eq!(c.wifi_channel, 6);
        assert_eq!(c.baud_rate, 57600);
        assert_eq!(c.wifi_power_dbm, 20.0);
        assert_eq!(c.ble4_power_dbm, 18.0);

        // Boundaries are accepted.
        assert!(apply_json(
            &mut c,
            r#"{"wifi_channel":13,"wifi_power_dbm":2.0,"baud_rate":1,"ble4_power_dbm":-27}"#,
        ));
        assert_eq!(c.wifi_channel, 13);
        assert_eq!(c.wifi_power_dbm, 2.0);
        assert_eq!(c.baud_rate, 1);
        assert_eq!(c.ble4_power_dbm, -27.0);
    }

    #[test]
    fn apply_json_truncates_strings() {
        let mut c = BspConfig::default();
        let long = "x".repeat(50);
        assert!(apply_json(&mut c, &format!(r#"{{"uas_id":"{}"}}"#, long)));
        assert_eq!(cstr(&c.uas_id), "x".repeat(20));
    }

    #[test]
    fn apply_json_lock_level() {
        let mut c = BspConfig::default();
        assert!(apply_json(&mut c, r#"{"lock_level":2}"#));
        assert_eq!(c.lock_level, 2);
        assert!(apply_json(&mut c, r#"{"lock_level":9}"#));
        assert_eq!(c.lock_level, 2);
        assert!(apply_json(&mut c, r#"{"lock_level":1}"#));
        assert_eq!(c.lock_level, 1);
        assert!(apply_json(&mut c, r#"{"lock_level":0}"#));
        assert_eq!(c.lock_level, 0);
        assert!(apply_json(&mut c, r#"{"lock_level":-1}"#));
        assert_eq!(c.lock_level, 0);
    }

    #[test]
    fn apply_json_lighting_and_keys() {
        let mut c = BspConfig::default();
        assert!(apply_json(
            &mut c,
            r#"{"lighting_pin_0":12,"lighting_pattern_2":3,"lighting_phase_4":-45,
                "public_key_1":"0123456789abcdef","public_key_5":"pk5"}"#,
        ));
        assert_eq!(c.lighting_pins[0], 12);
        assert_eq!(c.lighting_patterns[2], 3);
        assert_eq!(c.lighting_phase_offsets[4], -45);
        assert_eq!(cstr(&c.public_keys[0]), "0123456789abcdef");
        assert_eq!(cstr(&c.public_keys[4]), "pk5");
        assert_eq!(cstr(&c.public_keys[1]), "");
    }

    #[test]
    fn apply_json_bad_document() {
        let mut c = BspConfig::default();
        assert!(!apply_json(&mut c, "{not json"));
        assert!(!apply_json(&mut c, "[1,2,3]"));
        assert!(!apply_json(&mut c, r#""just a string""#));
        assert!(!apply_json(&mut c, "42"));
        assert_eq!(c, BspConfig::default());
    }

    #[test]
    fn config_to_json_default_format() {
        let out = config_to_json(&BspConfig::default());
        assert!(
            out.starts_with(
                "{\"protocol\":255,\"region\":\"AUTO\",\"uas_id\":\"ESP32-RID-001\",\"id_type\":1"
            ),
            "{}",
            out
        );
        assert!(out.contains("\"operator_lat\":0.000000"), "{}", out);
        assert!(out.contains("\"operator_lon\":0.000000"), "{}", out);
        assert!(out.contains("\"wifi_power_dbm\":20.0"), "{}", out);
        assert!(out.contains("\"start_delay_ms\":10000"), "{}", out);
        assert!(out.contains("\"mavlink_usb_enable\":false"), "{}", out);
        assert!(out.ends_with('}'), "{}", out);

        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["uas_id"], "ESP32-RID-001");
        assert_eq!(v["wifi_channel"], 6);
        assert_eq!(v["region"], "AUTO");
    }

    #[test]
    fn config_to_json_float_precision() {
        let c = BspConfig {
            operator_lat: 45.304,
            operator_lon: 9.2123,
            operator_alt: 123.4,
            wifi_power_dbm: 17.5,
            ..BspConfig::default()
        };
        let out = config_to_json(&c);
        assert!(out.contains("\"operator_lat\":45.304000"), "{}", out);
        assert!(out.contains("\"operator_lon\":9.212300"), "{}", out);
        assert!(out.contains("\"operator_alt\":123.4"), "{}", out);
        assert!(out.contains("\"wifi_power_dbm\":17.5"), "{}", out);
    }

    #[test]
    fn config_json_roundtrip() {
        let c = BspConfig {
            protocol: Protocol::Nmea,
            region: Region::Chn,
            uas_id: fixed_str("RT-42"),
            operator_id: fixed_str("OP-TEST"),
            self_id_text: fixed_str("hello world"),
            ua_type: 4,
            id_type: 5,
            wifi_channel: 1,
            wifi_power_dbm: 3.5,
            wifi_bcn_rate_hz: 0.5,
            ble4_power_dbm: -10.0,
            operator_lat: 41.9,
            operator_lon: 12.5,
            operator_alt: 99.5,
            baud_rate: 230400,
            mavlink_sysid: 7,
            start_delay_ms: 2000,
            lighting_pins: [0, 1, 2, 3, 4],
            lighting_phase_offsets: [10, 20, 30, 40, 50],
            ws2812_brightness: 32,
            ..BspConfig::default()
        };

        let out = config_to_json(&c);
        let mut restored = BspConfig::default();
        assert!(apply_json(&mut restored, &out));
        assert_eq!(restored, c);
    }

    #[test]
    fn config_to_json_escapes_strings() {
        let c = BspConfig {
            wifi_password: fixed_str("pa\"ss\\word"),
            ..BspConfig::default()
        };
        let out = config_to_json(&c);
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["wifi_password"], "pa\"ss\\word");
    }
}
