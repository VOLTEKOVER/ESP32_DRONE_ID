//! Neutral data types, port of `esp_remote_id.h`.
//!
//! Fixed-size string fields mirror the C `char[...]` arrays (NUL-terminated,
//! truncated to `MAX_STR_LEN`). Authentication pages mirror
//! `ext_auth_pages[ODID_AUTH_MAX_PAGES][ODID_MESSAGE_SIZE]`.

use crate::region::Region;
use crate::region::Standard;

/// `ESP_RID_MAX_STR_LEN` from the C header.
pub const MAX_STR_LEN: usize = 20;
/// `ESP_RID_MAX_KEY_LEN` from the C header.
pub const MAX_KEY_LEN: usize = 256;
/// `ESP_RID_NUM_KEYS` from the C header.
pub const NUM_KEYS: usize = 5;
/// `ODID_AUTH_MAX_PAGES` (ASTM F3411-22a).
pub const AUTH_MAX_PAGES: usize = 16;
/// `ODID_MESSAGE_SIZE` from the C header.
pub const MESSAGE_SIZE: usize = 25;
/// `INV_ALT` (invalid altitude, identity readiness gate).
pub const INV_ALT: f32 = -1000.0;
/// `INV_SPEED_H` (invalid horizontal speed).
pub const INV_SPEED_H: u8 = 255;
/// `INV_SPEED_V` (invalid vertical speed).
pub const INV_SPEED_V: u8 = 63;
/// `INV_DIR` (invalid direction).
pub const INV_DIR: u16 = 361;
/// `MAX_TIMESTAMP`.
pub const MAX_TIMESTAMP: u16 = 0xFFFF;

/// `RID_OPT_FORCE_ARM_OK`
pub const OPT_FORCE_ARM_OK: u16 = 1 << 0;
/// `RID_OPT_DONT_SAVE_BASIC_ID`
pub const OPT_DONT_SAVE_BASIC_ID: u16 = 1 << 1;
/// `RID_OPT_PRINT_RID_MAVLINK`
pub const OPT_PRINT_RID_MAVLINK: u16 = 1 << 2;
/// `RID_OPT_DEMO_MODE`
pub const OPT_DEMO_MODE: u16 = 1 << 3;
/// `RID_OPT_KALMAN_FILTER`
pub const OPT_KALMAN_FILTER: u16 = 1 << 4;
/// `RID_OPT_AUTH_ED25519`
pub const OPT_AUTH_ED25519: u16 = 1 << 5;
/// `RID_OPT_MAVLINK_ARM_STATUS`
pub const OPT_MAVLINK_ARM_STATUS: u16 = 1 << 6;
/// `RID_OPT_MAVLINK_OP_LOC_LOOP`
pub const OPT_MAVLINK_OP_LOC_LOOP: u16 = 1 << 7;
/// `RID_OPT_IDENTITY_READY_GATE`
pub const OPT_IDENTITY_READY_GATE: u16 = 1 << 8;

/// `RID_TRANSMIT_WIFI_BCN`
pub const TRANSMIT_WIFI_BCN: u8 = 1 << 0;
/// `RID_TRANSMIT_WIFI_NAN`
pub const TRANSMIT_WIFI_NAN: u8 = 1 << 1;
/// `RID_TRANSMIT_BLE4`
pub const TRANSMIT_BLE4: u8 = 1 << 2;
/// `RID_TRANSMIT_BLE5`
pub const TRANSMIT_BLE5: u8 = 1 << 3;

/// Fixed-size C string buffer: `MAX_STR_LEN` chars + NUL.
pub type FixedStr = [u8; MAX_STR_LEN + 1];

/// Fixed-size public-key buffer: `MAX_KEY_LEN` chars + NUL
/// (`public_keys[ESP_RID_NUM_KEYS][ESP_RID_MAX_KEY_LEN + 1]`).
pub type FixedKeyStr = [u8; MAX_KEY_LEN + 1];

/// Builds a fixed string from `&str`, truncating to `MAX_STR_LEN` chars and
/// zero-filling the rest (mirrors `snprintf` into a `char[21]`).
pub fn fixed_str(s: &str) -> FixedStr {
    let mut out = [0u8; MAX_STR_LEN + 1];
    let n = s.len().min(MAX_STR_LEN);
    out[..n].copy_from_slice(&s.as_bytes()[..n]);
    out
}

/// Builds a fixed public-key buffer from `&str`, truncating to `MAX_KEY_LEN`
/// chars and zero-filling the rest (mirrors `snprintf` into `char[257]`).
pub fn key_str(s: &str) -> FixedKeyStr {
    let mut out = [0u8; MAX_KEY_LEN + 1];
    let n = s.len().min(MAX_KEY_LEN);
    out[..n].copy_from_slice(&s.as_bytes()[..n]);
    out
}

/// Helpers to operate on fixed-size buffers as C strings (stop at first NUL).
pub trait CStr {
    /// Length up to the first NUL byte.
    fn c_len(&self) -> usize;
    /// True when the first byte is NUL (empty C string).
    fn c_is_empty(&self) -> bool;
    /// `strstr(hay, needle) == hay` (prefix match on the C string).
    fn c_starts_with(&self, needle: &str) -> bool;
    /// `strstr(hay, needle) != NULL`.
    fn c_contains(&self, needle: &str) -> bool;
}

impl CStr for [u8] {
    fn c_len(&self) -> usize {
        self.iter().position(|&b| b == 0).unwrap_or(self.len())
    }

    fn c_is_empty(&self) -> bool {
        self.is_empty() || self[0] == 0
    }

    fn c_starts_with(&self, needle: &str) -> bool {
        let hay = &self[..self.c_len()];
        hay.starts_with(needle.as_bytes())
    }

    fn c_contains(&self, needle: &str) -> bool {
        let hay = &self[..self.c_len()];
        if needle.is_empty() {
            return true;
        }
        hay.windows(needle.len()).any(|w| w == needle.as_bytes())
    }
}

impl<const N: usize> CStr for [u8; N] {
    fn c_len(&self) -> usize {
        self[..].c_len()
    }

    fn c_is_empty(&self) -> bool {
        self[..].c_is_empty()
    }

    fn c_starts_with(&self, needle: &str) -> bool {
        self[..].c_starts_with(needle)
    }

    fn c_contains(&self, needle: &str) -> bool {
        self[..].c_contains(needle)
    }
}

/// Input protocols, port of `rid_protocol_t`.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Protocol {
    #[default]
    Unknown = 0,
    Mavlink = 1,
    Msp = 2,
    Nmea = 3,
    None = 4,
    Auto = 255,
}

/// GPS/telemetry snapshot, port of `rid_gps_data_t`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct GpsData {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude_msl: f32,
    pub altitude_relative: f32,
    pub altitude_baro: f32,
    pub speed: f32,
    pub speed_vertical: f32,
    pub heading: i16,
    pub fix_type: u8,
    pub satellites: u8,
    pub armed: bool,
    pub operator_lat: f64,
    pub operator_lon: f64,
    pub operator_alt: f32,
}

/// Identity + authentication payload, port of `rid_identity_t`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Identity {
    pub uas_id: FixedStr,
    pub operator_id: FixedStr,
    pub self_id_text: FixedStr,
    pub id_type: u8,
    pub ua_type: u8,
    pub uas_id_2: FixedStr,
    pub id_type_2: u8,
    pub ua_type_2: u8,
    /// Self-ID from MAVLink.
    pub has_self_id: bool,
    pub self_id_desc_type: u8,
    /// Authentication from MAVLink relay.
    pub has_ext_auth: bool,
    pub ext_auth_last_page: u8,
    pub ext_auth_type: u8,
    pub ext_auth_length: u8,
    pub ext_auth_pages_received: u16,
    pub ext_auth_pages: [[u8; MESSAGE_SIZE]; AUTH_MAX_PAGES],
}

impl Default for Identity {
    fn default() -> Self {
        Self {
            uas_id: [0; MAX_STR_LEN + 1],
            operator_id: [0; MAX_STR_LEN + 1],
            self_id_text: [0; MAX_STR_LEN + 1],
            id_type: 0,
            ua_type: 0,
            uas_id_2: [0; MAX_STR_LEN + 1],
            id_type_2: 0,
            ua_type_2: 0,
            has_self_id: false,
            self_id_desc_type: 0,
            has_ext_auth: false,
            ext_auth_last_page: 0,
            ext_auth_type: 0,
            ext_auth_length: 0,
            ext_auth_pages_received: 0,
            ext_auth_pages: [[0; MESSAGE_SIZE]; AUTH_MAX_PAGES],
        }
    }
}

/// Firmware configuration (core subset), port of `rid_config_t`.
///
/// Transport/pin fields arrive with the BSP phase; this carries the fields
/// the hub, the identity readiness gate and the operator-location loop use.
#[derive(Clone, PartialEq, Debug)]
pub struct Config {
    pub region: Region,
    pub ua_type: u8,
    pub id_type: u8,
    pub uas_id: FixedStr,
    pub operator_id: FixedStr,
    pub ua_type_2: u8,
    pub id_type_2: u8,
    pub uas_id_2: FixedStr,
    pub self_id_text: FixedStr,
    pub options: u16,
    pub operator_lat: f64,
    pub operator_lon: f64,
    pub operator_alt: f32,
    /// `tx_modes` (which transports broadcast).
    pub tx_modes: u8,
    /// `wifi_bcn_rate_hz`.
    pub wifi_bcn_rate_hz: f32,
    /// `wifi_nan_rate_hz`.
    pub wifi_nan_rate_hz: f32,
    /// `ble4_rate_hz`.
    pub ble4_rate_hz: f32,
    /// `ble5_rate_hz`.
    pub ble5_rate_hz: f32,
    /// `bcast_powerup`: transmit even without a valid fix.
    pub bcast_powerup: bool,
    /// `mavlink_sysid`: MAVLink parser sysid filter (0 = accept any system).
    pub mavlink_sysid: u8,
    /// `lock_level`: >= 2 locks the firmware (LED shows locked).
    pub lock_level: i8,
}

impl Default for Config {
    /// Mirrors `default_config()` in `esp_remote_id.c` for the core fields.
    fn default() -> Self {
        Self {
            region: Region::Auto,
            ua_type: 1,
            id_type: 1,
            uas_id: fixed_str("ESP32-RID-001"),
            operator_id: fixed_str("OP-UNKNOWN"),
            ua_type_2: 0,
            id_type_2: 0,
            uas_id_2: [0; MAX_STR_LEN + 1],
            self_id_text: [0; MAX_STR_LEN + 1],
            options: 0,
            operator_lat: 0.0,
            operator_lon: 0.0,
            operator_alt: 0.0,
            tx_modes: TRANSMIT_WIFI_BCN,
            wifi_bcn_rate_hz: 1.0,
            wifi_nan_rate_hz: 0.0,
            ble4_rate_hz: 1.0,
            ble5_rate_hz: 1.0,
            bcast_powerup: true,
            mavlink_sysid: 0,
            lock_level: 0,
        }
    }
}

/// Runtime state, port of `rid_state_t`.
#[derive(Clone, PartialEq, Debug)]
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
    /// Identity readiness.
    pub identity_ready: bool,
    /// MAVLink status.
    pub mavlink_armed: bool,
    pub mavlink_sysid: u32,
    /// Operator location.
    pub operator_lat: f64,
    pub operator_lon: f64,
    pub operator_alt: f32,
    pub operator_position_updated_ms: u32,
    pub operator_location_type: u8,
    pub auth_enabled: bool,
    /// Active broadcast standard (exclusive, derived from region).
    pub active_standard: Standard,
    /// True when the active standard has no encoder yet and ASTM is used.
    pub standard_fallback: bool,
    /// Takeoff location (captured once at first 3D fix, per ASTM F3411).
    pub takeoff_lat: f64,
    pub takeoff_lon: f64,
    pub takeoff_alt: f32,
    pub takeoff_captured: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            gps: GpsData::default(),
            identity: Identity::default(),
            active_protocol: Protocol::Unknown,
            last_update_ms: 0,
            transmissions_count: 0,
            wifi_bcn_count: 0,
            wifi_nan_count: 0,
            ble4_count: 0,
            ble5_count: 0,
            gps_valid: false,
            identity_ready: false,
            mavlink_armed: false,
            mavlink_sysid: 0,
            operator_lat: 0.0,
            operator_lon: 0.0,
            operator_alt: 0.0,
            operator_position_updated_ms: 0,
            operator_location_type: 0,
            auth_enabled: false,
            // `esp_rid_init` clears g_state after binding region; the memset
            // leaves ASTM (0) and no fallback.
            active_standard: Standard::Astm,
            standard_fallback: false,
            takeoff_lat: 0.0,
            takeoff_lon: 0.0,
            takeoff_alt: 0.0,
            takeoff_captured: false,
        }
    }
}

/// Result of the hourglass hub: the gated identity plus the single active
/// broadcast standard (exclusive per region) and the fallback flag.
/// Consumed by the `out-*` encoder crates.
#[derive(Clone, PartialEq, Debug)]
pub struct UasBuild {
    /// Exclusive standard selected by the configured region.
    pub standard: Standard,
    /// GPS/telemetry snapshot (passed through to the encoder).
    pub gps: GpsData,
    /// Identity copy with region-disallowed messages dropped.
    pub gated_identity: Identity,
    /// True when the active standard has no encoder yet and ASTM is used.
    pub fallback: bool,
}
