//! CLI console, port of the pure logic in `cli.c`: line tokenization,
//! `config set <field> <value>`, protocol/mode/log-level resolution and the
//! option toggles. The hardware-backed parts (reading/writing the global
//! config, heap/uptime/MAC, log level set, restart/reset) stay in the BSP chip
//! layer; every decision here is host-testable.

use alloc::vec::Vec;
use rid_interface::{
    Protocol, TRANSMIT_BLE4, TRANSMIT_BLE5, TRANSMIT_WIFI_BCN, TRANSMIT_WIFI_NAN, MAX_STR_LEN,
};
use rid_interface::{OPT_DEMO_MODE, OPT_KALMAN_FILTER};

use crate::config::BspConfig;
use crate::json::parse_region_name;

/// Mirror of `MAX_ARGS` in `cli.c`.
pub const MAX_ARGS: usize = 16;

/// Port of `proto_name()`.
pub fn proto_name(p: Protocol) -> &'static str {
    match p {
        Protocol::Unknown => "UNKNOWN",
        Protocol::Mavlink => "MAVLink",
        Protocol::Msp => "MSP",
        Protocol::Nmea => "NMEA",
        Protocol::None => "NONE",
        Protocol::Auto => "AUTO",
    }
}

/// Port of `parse_line()`: whitespace tokenizer capped at `MAX_ARGS - 1`
/// tokens (the C writes at most `MAX_ARGS - 1` pointers and NUL-terminates).
pub fn parse_line(line: &str) -> Vec<&str> {
    let mut args = Vec::new();
    for tok in line.split_ascii_whitespace() {
        if args.len() >= MAX_ARGS - 1 {
            break;
        }
        args.push(tok);
    }
    args
}

/// Errors of `config_set_field()`, mirroring the `config set` messages.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConfigSetError {
    /// `Usage: config set <field> <value>`
    Usage,
    /// `Unknown field: <field>`
    UnknownField,
    /// `Unknown region: <value> (use: auto, EUR, ...)`
    UnknownRegion,
}

/// Port of the `config set <field> <value>` branch of `cmd_config()`. Applies
/// the value to `cfg` exactly like the C (`strtol`/`strtoul` with base 0,
/// `strtod`, case-insensitive field names, strings truncated to 20 + NUL).
pub fn config_set_field(cfg: &mut BspConfig, field: &str, value: &str) -> Result<(), ConfigSetError> {
    if field.eq_ignore_ascii_case("uas_id")
        || field.eq_ignore_ascii_case("operator_id")
        || field.eq_ignore_ascii_case("self_id")
        || field.eq_ignore_ascii_case("wifi_ssid")
        || field.eq_ignore_ascii_case("wifi_password")
    {
        let dst = if field.eq_ignore_ascii_case("uas_id") {
            &mut cfg.uas_id
        } else if field.eq_ignore_ascii_case("operator_id") {
            &mut cfg.operator_id
        } else if field.eq_ignore_ascii_case("self_id") {
            &mut cfg.self_id_text
        } else if field.eq_ignore_ascii_case("wifi_ssid") {
            &mut cfg.wifi_ssid
        } else {
            &mut cfg.wifi_password
        };
        let bytes = value.as_bytes();
        let n = bytes.len().min(MAX_STR_LEN);
        dst[..n].copy_from_slice(&bytes[..n]);
        dst[n..].fill(0);
        return Ok(());
    }

    if field.eq_ignore_ascii_case("ua_type") {
        cfg.ua_type = parse_i64_base0(value) as u8;
    } else if field.eq_ignore_ascii_case("id_type") {
        cfg.id_type = parse_i64_base0(value) as u8;
    } else if field.eq_ignore_ascii_case("wifi_channel") {
        cfg.wifi_channel = parse_i64_base0(value) as u8;
    } else if field.eq_ignore_ascii_case("mavlink_sysid") {
        cfg.mavlink_sysid = parse_i64_base0(value) as u8;
    } else if field.eq_ignore_ascii_case("bcast_powerup") {
        cfg.bcast_powerup = parse_i64_base0(value) as u8;
    } else if field.eq_ignore_ascii_case("webserver") {
        cfg.webserver_en = (value.eq_ignore_ascii_case("on") || parse_i64_base0(value) != 0) as u8;
    } else if field.eq_ignore_ascii_case("lock_level") {
        cfg.lock_level = parse_i64_base0(value) as i8;
    } else if field.eq_ignore_ascii_case("baud_rate") {
        cfg.baud_rate = parse_u32_base0(value);
    } else if field.eq_ignore_ascii_case("wifi_power_dbm") {
        cfg.wifi_power_dbm = parse_f64(value) as f32;
    } else if field.eq_ignore_ascii_case("wifi_bcn_rate") {
        cfg.wifi_bcn_rate_hz = parse_f64(value) as f32;
    } else if field.eq_ignore_ascii_case("wifi_nan_rate") {
        cfg.wifi_nan_rate_hz = parse_f64(value) as f32;
    } else if field.eq_ignore_ascii_case("ble4_rate") {
        cfg.ble4_rate_hz = parse_f64(value) as f32;
    } else if field.eq_ignore_ascii_case("ble4_power") {
        cfg.ble4_power_dbm = parse_f64(value) as f32;
    } else if field.eq_ignore_ascii_case("ble5_rate") {
        cfg.ble5_rate_hz = parse_f64(value) as f32;
    } else if field.eq_ignore_ascii_case("ble5_power") {
        cfg.ble5_power_dbm = parse_f64(value) as f32;
    } else if field.eq_ignore_ascii_case("operator_lat") {
        cfg.operator_lat = parse_f64(value);
    } else if field.eq_ignore_ascii_case("operator_lon") {
        cfg.operator_lon = parse_f64(value);
    } else if field.eq_ignore_ascii_case("operator_alt") {
        cfg.operator_alt = parse_f64(value) as f32;
    } else if field.eq_ignore_ascii_case("start_delay_ms") {
        cfg.start_delay_ms = parse_u32_base0(value);
    } else if field.eq_ignore_ascii_case("region") {
        match parse_region_name(value) {
            Some(r) => cfg.region = r,
            None => return Err(ConfigSetError::UnknownRegion),
        }
    } else {
        return Err(ConfigSetError::UnknownField);
    }
    Ok(())
}

/// Port of the value parsing in `cmd_protocol()`.
pub fn parse_protocol_name(s: &str) -> Option<Protocol> {
    if s.eq_ignore_ascii_case("auto") {
        Some(Protocol::Auto)
    } else if s.eq_ignore_ascii_case("mavlink") {
        Some(Protocol::Mavlink)
    } else if s.eq_ignore_ascii_case("msp") {
        Some(Protocol::Msp)
    } else if s.eq_ignore_ascii_case("nmea") {
        Some(Protocol::Nmea)
    } else if s.eq_ignore_ascii_case("none") {
        Some(Protocol::None)
    } else {
        None
    }
}

/// Errors of `set_tx_mode()`, mirroring the `transmit` messages.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TxModeError {
    /// `Missing on/off`
    MissingArg,
    /// `Unknown mode: <mode>`
    UnknownMode,
}

/// Port of the `transmit <mode> <on|off>` branch of `cmd_transmit()`.
pub fn set_tx_mode(cfg: &mut BspConfig, mode: &str, on: bool) -> Result<(), TxModeError> {
    let mask = if mode.eq_ignore_ascii_case("wifi_bcn") {
        TRANSMIT_WIFI_BCN
    } else if mode.eq_ignore_ascii_case("wifi_nan") {
        TRANSMIT_WIFI_NAN
    } else if mode.eq_ignore_ascii_case("ble4") {
        TRANSMIT_BLE4
    } else if mode.eq_ignore_ascii_case("ble5") {
        TRANSMIT_BLE5
    } else if mode.eq_ignore_ascii_case("all") {
        0x0F
    } else {
        return Err(TxModeError::UnknownMode);
    };

    if mask == 0x0F {
        cfg.tx_modes = if on { 0x0F } else { 0 };
    } else if on {
        cfg.tx_modes |= mask;
    } else {
        cfg.tx_modes &= !mask;
    }
    Ok(())
}

/// Port of the level parsing in `cmd_log_level()`. Returns the `esp_log_level_t`
/// numeric value (NONE=0 .. VERBOSE=5).
pub fn parse_log_level(s: &str) -> Option<u8> {
    if s.eq_ignore_ascii_case("NONE") {
        Some(0)
    } else if s.eq_ignore_ascii_case("ERROR") {
        Some(1)
    } else if s.eq_ignore_ascii_case("WARN") {
        Some(2)
    } else if s.eq_ignore_ascii_case("INFO") {
        Some(3)
    } else if s.eq_ignore_ascii_case("DEBUG") {
        Some(4)
    } else if s.eq_ignore_ascii_case("VERBOSE") {
        Some(5)
    } else {
        None
    }
}

/// Port of `cmd_patrol()`: `on`/`off` set the bit, anything else toggles.
pub fn apply_demo_mode(cfg: &mut BspConfig, arg: Option<&str>) {
    match arg {
        Some(a) if a.eq_ignore_ascii_case("off") => cfg.options &= !OPT_DEMO_MODE,
        Some(a) if a.eq_ignore_ascii_case("on") => cfg.options |= OPT_DEMO_MODE,
        _ => cfg.options ^= OPT_DEMO_MODE,
    }
}

/// Error for `kalman`: `Usage: kalman [on|off]`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KalmanUsageError;

/// Port of `cmd_kalman()`: no argument only reports (no change); a bad
/// argument is the `Usage: kalman [on|off]` error.
pub fn apply_kalman(cfg: &mut BspConfig, arg: Option<&str>) -> Result<bool, KalmanUsageError> {
    match arg {
        None => Ok(false),
        Some(a) if a.eq_ignore_ascii_case("on") => {
            cfg.options |= OPT_KALMAN_FILTER;
            Ok(true)
        }
        Some(a) if a.eq_ignore_ascii_case("off") => {
            cfg.options &= !OPT_KALMAN_FILTER;
            Ok(true)
        }
        Some(_) => Err(KalmanUsageError),
    }
}

/// `strtol(v, NULL, 0)`: base auto-detected (0x hex, leading 0 octal),
/// stops at the first invalid digit, 0 when nothing parses. Overflow wraps.
pub fn parse_i64_base0(s: &str) -> i64 {
    let (sign, rest) = match s.strip_prefix('-') {
        Some(r) => (-1i64, r),
        None => (1i64, s.strip_prefix('+').unwrap_or(s)),
    };
    if rest.is_empty() {
        return 0;
    }
    let (radix, digits) = if let Some(h) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
        (16, h)
    } else if rest.len() > 1 && rest.starts_with('0') {
        (8, &rest[1..])
    } else {
        (10, rest)
    };
    let mut acc: i64 = 0;
    for c in digits.chars() {
        match c.to_digit(radix) {
            Some(d) => acc = acc.wrapping_mul(radix as i64).wrapping_add(d as i64),
            None => break,
        }
    }
    sign * acc
}

/// `strtoul(v, NULL, 0)` truncated to 32 bits (negative values wrap).
fn parse_u32_base0(s: &str) -> u32 {
    parse_i64_base0(s) as u32
}

/// `strtod(v, NULL)`: 0.0 when nothing parses.
fn parse_f64(s: &str) -> f64 {
    s.parse::<f64>().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use rid_interface::Region;

    #[test]
    fn proto_name_matches_c() {
        assert_eq!(proto_name(Protocol::Unknown), "UNKNOWN");
        assert_eq!(proto_name(Protocol::Mavlink), "MAVLink");
        assert_eq!(proto_name(Protocol::Msp), "MSP");
        assert_eq!(proto_name(Protocol::Nmea), "NMEA");
        assert_eq!(proto_name(Protocol::None), "NONE");
        assert_eq!(proto_name(Protocol::Auto), "AUTO");
    }

    #[test]
    fn parse_line_tokens() {
        assert_eq!(parse_line("config set uas_id ABC"), ["config", "set", "uas_id", "ABC"]);
        assert_eq!(parse_line("  status  \n"), ["status"]);
        assert_eq!(parse_line("  "), Vec::<&str>::new());
        assert_eq!(parse_line(""), Vec::<&str>::new());
    }

    #[test]
    fn parse_line_caps_at_max_args() {
        let many = (0..30u32).map(|i| i.to_string()).collect::<Vec<_>>().join(" ");
        let args = parse_line(&many);
        assert_eq!(args.len(), MAX_ARGS - 1);
    }

    #[test]
    fn config_set_string_fields() {
        let mut c = BspConfig::default();
        assert_eq!(config_set_field(&mut c, "uas_id", "NEO-9"), Ok(()));
        assert_eq!(crate::config::cstr(&c.uas_id), "NEO-9");
        assert_eq!(config_set_field(&mut c, "SELF_ID", "hello"), Ok(()));
        assert_eq!(crate::config::cstr(&c.self_id_text), "hello");
        assert_eq!(config_set_field(&mut c, "wifi_ssid", "ESP-RID-2"), Ok(()));
        assert_eq!(crate::config::cstr(&c.wifi_ssid), "ESP-RID-2");

        let long = "x".repeat(50);
        assert_eq!(config_set_field(&mut c, "operator_id", &long), Ok(()));
        assert_eq!(crate::config::cstr(&c.operator_id), "x".repeat(20));
    }

    #[test]
    fn config_set_numeric_fields() {
        let mut c = BspConfig::default();
        // strtol base 0: hex and octal.
        assert_eq!(config_set_field(&mut c, "ua_type", "0x1A"), Ok(()));
        assert_eq!(c.ua_type, 26);
        assert_eq!(config_set_field(&mut c, "id_type", "010"), Ok(()));
        assert_eq!(c.id_type, 8);
        assert_eq!(config_set_field(&mut c, "wifi_channel", "11"), Ok(()));
        assert_eq!(c.wifi_channel, 11);
        assert_eq!(config_set_field(&mut c, "baud_rate", "230400"), Ok(()));
        assert_eq!(c.baud_rate, 230400);
        assert_eq!(config_set_field(&mut c, "lock_level", "2"), Ok(()));
        assert_eq!(c.lock_level, 2);
        assert_eq!(config_set_field(&mut c, "wifi_power_dbm", "17.5"), Ok(()));
        assert_eq!(c.wifi_power_dbm, 17.5);
        assert_eq!(config_set_field(&mut c, "operator_lat", "45.304"), Ok(()));
        assert_eq!(c.operator_lat, 45.304);
        assert_eq!(config_set_field(&mut c, "start_delay_ms", "5000"), Ok(()));
        assert_eq!(c.start_delay_ms, 5000);
    }

    #[test]
    fn config_set_webserver() {
        let mut c = BspConfig::default();
        assert_eq!(config_set_field(&mut c, "webserver", "on"), Ok(()));
        assert_eq!(c.webserver_en, 1);
        assert_eq!(config_set_field(&mut c, "webserver", "off"), Ok(()));
        assert_eq!(c.webserver_en, 0);
        assert_eq!(config_set_field(&mut c, "webserver", "5"), Ok(()));
        assert_eq!(c.webserver_en, 1);
        assert_eq!(config_set_field(&mut c, "webserver", "0"), Ok(()));
        assert_eq!(c.webserver_en, 0);
    }

    #[test]
    fn config_set_region() {
        let mut c = BspConfig::default();
        assert_eq!(config_set_field(&mut c, "region", "JPN"), Ok(()));
        assert_eq!(c.region, Region::Jpn);
        assert_eq!(config_set_field(&mut c, "region", "auto"), Ok(()));
        assert_eq!(c.region, Region::Auto);
        assert_eq!(
            config_set_field(&mut c, "region", "XXX"),
            Err(ConfigSetError::UnknownRegion)
        );
        assert_eq!(c.region, Region::Auto);
    }

    #[test]
    fn config_set_unknown_field() {
        let mut c = BspConfig::default();
        assert_eq!(
            config_set_field(&mut c, "nope", "1"),
            Err(ConfigSetError::UnknownField)
        );
    }

    #[test]
    fn parse_protocol_names() {
        assert_eq!(parse_protocol_name("auto"), Some(Protocol::Auto));
        assert_eq!(parse_protocol_name("MAVLINK"), Some(Protocol::Mavlink));
        assert_eq!(parse_protocol_name("msp"), Some(Protocol::Msp));
        assert_eq!(parse_protocol_name("nmea"), Some(Protocol::Nmea));
        assert_eq!(parse_protocol_name("none"), Some(Protocol::None));
        assert_eq!(parse_protocol_name("ble"), None);
    }

    #[test]
    fn tx_mode_logic() {
        let mut c = BspConfig::default();
        assert_eq!(set_tx_mode(&mut c, "ble4", true), Ok(()));
        assert_eq!(c.tx_modes, TRANSMIT_WIFI_BCN | TRANSMIT_BLE4);
        assert_eq!(set_tx_mode(&mut c, "wifi_bcn", false), Ok(()));
        assert_eq!(c.tx_modes, TRANSMIT_BLE4);
        assert_eq!(set_tx_mode(&mut c, "all", true), Ok(()));
        assert_eq!(c.tx_modes, 0x0F);
        assert_eq!(set_tx_mode(&mut c, "all", false), Ok(()));
        assert_eq!(c.tx_modes, 0);
        assert_eq!(set_tx_mode(&mut c, "lora", true), Err(TxModeError::UnknownMode));
    }

    #[test]
    fn log_levels() {
        assert_eq!(parse_log_level("NONE"), Some(0));
        assert_eq!(parse_log_level("error"), Some(1));
        assert_eq!(parse_log_level("WARN"), Some(2));
        assert_eq!(parse_log_level("info"), Some(3));
        assert_eq!(parse_log_level("DEBUG"), Some(4));
        assert_eq!(parse_log_level("verbose"), Some(5));
        assert_eq!(parse_log_level("FOO"), None);
    }

    #[test]
    fn demo_and_kalman_toggles() {
        let mut c = BspConfig::default();
        apply_demo_mode(&mut c, Some("on"));
        assert_ne!(c.options & OPT_DEMO_MODE, 0);
        apply_demo_mode(&mut c, Some("on"));
        assert_ne!(c.options & OPT_DEMO_MODE, 0);
        apply_demo_mode(&mut c, Some("off"));
        assert_eq!(c.options & OPT_DEMO_MODE, 0);
        apply_demo_mode(&mut c, None);
        assert_ne!(c.options & OPT_DEMO_MODE, 0);

        let mut c = BspConfig::default();
        assert_eq!(apply_kalman(&mut c, Some("on")), Ok(true));
        assert_ne!(c.options & OPT_KALMAN_FILTER, 0);
        assert_eq!(apply_kalman(&mut c, None), Ok(false));
        assert_eq!(apply_kalman(&mut c, Some("wat")), Err(KalmanUsageError));
        assert_eq!(apply_kalman(&mut c, Some("off")), Ok(true));
        assert_eq!(c.options & OPT_KALMAN_FILTER, 0);
    }

    #[test]
    fn base0_int_parsing() {
        assert_eq!(parse_i64_base0("0x1A"), 26);
        assert_eq!(parse_i64_base0("0X2f"), 47);
        assert_eq!(parse_i64_base0("010"), 8);
        assert_eq!(parse_i64_base0("0"), 0);
        assert_eq!(parse_i64_base0("17"), 17);
        assert_eq!(parse_i64_base0("-0x10"), -16);
        assert_eq!(parse_i64_base0("12abc"), 12);
        assert_eq!(parse_i64_base0("abc"), 0);
        assert_eq!(parse_i64_base0(""), 0);
        assert_eq!(parse_i64_base0("-1") as u32, u32::MAX);
    }
}
