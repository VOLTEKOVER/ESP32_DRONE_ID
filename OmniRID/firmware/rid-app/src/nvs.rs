//! NVS persistence, port of `nvs_storage.c`. The key/value semantics of the
//! ESP-IDF NVS are abstracted behind [`NvsStore`], so the exact field/key
//! handling, defaults, clamping and the reset-preserving-keys logic are
//! host-testable. The ESP32 NVS implementation of the trait lands with the
//! hardware phase.

use rid_interface::{MAX_KEY_LEN, NUM_KEYS};

use crate::config::{clamp_region, cstr, BspConfig, NUM_LIGHTING_PINS};

/// Abstract non-volatile key/value store, mirroring the ESP-IDF NVS subset
/// used by `nvs_storage.c`.
pub trait NvsStore {
    /// Writes the stored string (NUL-terminated, truncated to `out`) and
    /// returns `true`. Returns `false` when the key is absent.
    fn get_str(&mut self, key: &str, out: &mut [u8]) -> bool;
    fn set_str(&mut self, key: &str, value: &str);
    fn get_u8(&mut self, key: &str) -> Option<u8>;
    fn set_u8(&mut self, key: &str, value: u8);
    fn get_i8(&mut self, key: &str) -> Option<i8>;
    fn set_i8(&mut self, key: &str, value: i8);
    fn get_u32(&mut self, key: &str) -> Option<u32>;
    fn set_u32(&mut self, key: &str, value: u32);
    fn get_f32(&mut self, key: &str) -> Option<f32>;
    fn set_f32(&mut self, key: &str, value: f32);
    fn get_f64(&mut self, key: &str) -> Option<f64>;
    fn set_f64(&mut self, key: &str, value: f64);
    /// Returns the stored raw bytes (`false` when the key is absent or the
    /// stored blob does not fit `out`; `out` is left unmodified on failure).
    fn get_blob(&mut self, key: &str, out: &mut [u8]) -> bool;
    fn set_blob(&mut self, key: &str, value: &[u8]);
    fn erase_all(&mut self);
}

/// Loads a `[i16; N]` blob; on a missing/mismatched blob `out` is left
/// untouched matching the C `if (nvs_get_...)` guard.
fn load_blob_i16(nvs: &mut impl NvsStore, key: &str, out: &mut [i16]) {
    let n = core::mem::size_of_val(out);
    let mut buf = [0u8; NUM_LIGHTING_PINS * 2];
    if buf.len() < n || !nvs.get_blob(key, &mut buf[..n]) {
        return;
    }
    for (i, slot) in out.iter_mut().enumerate() {
        let mut b = [0u8; 2];
        b.copy_from_slice(&buf[i * 2..i * 2 + 2]);
        *slot = i16::from_le_bytes(b);
    }
}

/// Saves a `[i16; N]` as a little-endian blob.
fn save_blob_i16(nvs: &mut impl NvsStore, key: &str, value: &[i16]) {
    let n = core::mem::size_of_val(value);
    let mut buf = [0u8; NUM_LIGHTING_PINS * 2];
    if buf.len() < n {
        return;
    }
    for (i, v) in value.iter().enumerate() {
        buf[i * 2..i * 2 + 2].copy_from_slice(&v.to_le_bytes());
    }
    nvs.set_blob(key, &buf[..n]);
}

/// Reinterprets a `[i8; N]` slice as its byte representation.
fn i8_bytes(v: &[i8]) -> &[u8] {
    // SAFETY: `i8` and `u8` have identical size/alignment and there is no
    // padding, so viewing as bytes is well-defined.
    unsafe { core::slice::from_raw_parts(v.as_ptr() as *const u8, v.len()) }
}

/// Reinterprets a byte slice as `[i8; N]` (byte-identical layout).
fn bytes_i8(v: &[u8]) -> &[i8] {
    unsafe { core::slice::from_raw_parts(v.as_ptr() as *const i8, v.len()) }
}

/// Builds the `pubkey%d` NVS key (1-based, like the C `snprintf`).
fn pubkey_key(i: usize) -> [u8; 7] {
    let mut k = [0u8; 7];
    k[..6].copy_from_slice(b"pubkey");
    k[6] = b'0' + (i as u8) + 1;
    k
}

/// Port of the C `load_str`: on a missing key, copies `def` into `out`
/// (zero-padded, `strncpy` semantics).
fn load_str(nvs: &mut impl NvsStore, key: &str, out: &mut [u8], def: &str) {
    if !nvs.get_str(key, out) {
        copy_cstr(out, def);
    }
}

/// `strncpy(out, s, out.len())`: copies up to `out.len()-1` bytes and
/// zero-pads the rest.
fn copy_cstr(out: &mut [u8], s: &str) {
    let n = s.len().min(out.len().saturating_sub(1));
    out[..n].copy_from_slice(&s.as_bytes()[..n]);
    out[n..].fill(0);
}

/// Port of `nvs_storage_save()`.
pub fn save(cfg: &BspConfig, nvs: &mut impl NvsStore) {
    nvs.set_str("uas_id", cstr(&cfg.uas_id));
    nvs.set_str("op_id", cstr(&cfg.operator_id));
    nvs.set_str("self_id", cstr(&cfg.self_id_text));
    nvs.set_str("uas_id2", cstr(&cfg.uas_id_2));
    nvs.set_str("wifi_ssid", cstr(&cfg.wifi_ssid));
    nvs.set_str("wifi_pass", cstr(&cfg.wifi_password));

    nvs.set_u8("ua_type", cfg.ua_type);
    nvs.set_u8("id_type", cfg.id_type);
    nvs.set_u8("ua_type2", cfg.ua_type_2);
    nvs.set_u8("id_type2", cfg.id_type_2);
    nvs.set_u8("wifi_ch", cfg.wifi_channel);
    nvs.set_u8("websrv_en", cfg.webserver_en);
    nvs.set_u8("mav_sysid", cfg.mavlink_sysid);
    nvs.set_u8("bcast_pwr", cfg.bcast_powerup);
    nvs.set_u8("tx_modes", cfg.tx_modes);
    nvs.set_u8("region", cfg.region as u8);
    nvs.set_u32("options", cfg.options as u32);
    nvs.set_i8("lock_lvl", cfg.lock_level);
    nvs.set_i8("led_r", cfg.led_r_gpio);
    nvs.set_i8("led_g", cfg.led_g_gpio);
    nvs.set_i8("led_b", cfg.led_b_gpio);

    // Input transport.
    nvs.set_u8("protocol", cfg.protocol as u8);
    nvs.set_u8("uart_port", cfg.uart_port);
    nvs.set_u8("tx_pin", cfg.tx_pin);
    nvs.set_u8("rx_pin", cfg.rx_pin);

    // WS2812.
    nvs.set_i8("ws2812_gpio", cfg.ws2812_gpio);
    nvs.set_u8("ws2812_br", cfg.ws2812_brightness);

    // External lighting.
    nvs.set_blob("light_pins", i8_bytes(&cfg.lighting_pins));
    nvs.set_blob("light_pat", &cfg.lighting_patterns);
    save_blob_i16(nvs, "light_phase", &cfg.lighting_phase_offsets);

    // DroneCAN.
    nvs.set_i8("dronecan_rx", cfg.dronecan_rx_gpio);
    nvs.set_i8("dronecan_tx", cfg.dronecan_tx_gpio);
    nvs.set_u32("dronecan_br", cfg.dronecan_bitrate);

    // MAVLink USB transport.
    nvs.set_u8("mav_usb", cfg.mavlink_usb_enable as u8);

    // OTA trigger GPIO.
    nvs.set_i8("ota_trig", cfg.ota_trigger_gpio);

    // Auth private key (persisted so it survives reboot).
    nvs.set_blob("auth_key", &cfg.auth_private_key);

    // Startup delay.
    nvs.set_u32("start_delay", cfg.start_delay_ms);

    nvs.set_u32("baud", cfg.baud_rate);

    nvs.set_f32("wifi_pwr", cfg.wifi_power_dbm);
    nvs.set_f32("wifi_bcn", cfg.wifi_bcn_rate_hz);
    nvs.set_f32("wifi_nan", cfg.wifi_nan_rate_hz);
    nvs.set_f32("bt4_rate", cfg.ble4_rate_hz);
    nvs.set_f32("bt4_pwr", cfg.ble4_power_dbm);
    nvs.set_f32("bt5_rate", cfg.ble5_rate_hz);
    nvs.set_f32("bt5_pwr", cfg.ble5_power_dbm);

    nvs.set_f64("op_lat", cfg.operator_lat);
    nvs.set_f64("op_lon", cfg.operator_lon);
    nvs.set_f32("op_alt", cfg.operator_alt);

    for (i, key) in cfg.public_keys.iter().enumerate() {
        let k = pubkey_key(i);
        nvs.set_str(core::str::from_utf8(&k).unwrap(), cstr(key));
    }
}

/// Port of `nvs_storage_load()`.
pub fn load(cfg: &mut BspConfig, nvs: &mut impl NvsStore) {
    load_str(nvs, "uas_id", &mut cfg.uas_id, "");
    load_str(nvs, "op_id", &mut cfg.operator_id, "");
    load_str(nvs, "self_id", &mut cfg.self_id_text, "");
    load_str(nvs, "uas_id2", &mut cfg.uas_id_2, "");
    load_str(nvs, "wifi_ssid", &mut cfg.wifi_ssid, "ESP-RID");
    load_str(nvs, "wifi_pass", &mut cfg.wifi_password, "");

    if let Some(v) = nvs.get_u8("ua_type") {
        cfg.ua_type = v;
    }
    if let Some(v) = nvs.get_u8("id_type") {
        cfg.id_type = v;
    }
    if let Some(v) = nvs.get_u8("ua_type2") {
        cfg.ua_type_2 = v;
    }
    if let Some(v) = nvs.get_u8("id_type2") {
        cfg.id_type_2 = v;
    }
    if let Some(v) = nvs.get_u8("wifi_ch") {
        cfg.wifi_channel = v;
    }
    if let Some(v) = nvs.get_u8("websrv_en") {
        cfg.webserver_en = v;
    }
    if let Some(v) = nvs.get_u8("mav_sysid") {
        cfg.mavlink_sysid = v;
    }
    if let Some(v) = nvs.get_u8("bcast_pwr") {
        cfg.bcast_powerup = v;
    }
    if let Some(v) = nvs.get_u8("tx_modes") {
        cfg.tx_modes = v;
    }
    {
        let r = nvs.get_u8("region").unwrap_or(cfg.region as u8);
        cfg.region = clamp_region(r);
    }
    if let Some(v) = nvs.get_u32("options") {
        cfg.options = v as u16;
    }
    if let Some(v) = nvs.get_i8("lock_lvl") {
        cfg.lock_level = v;
    }
    if let Some(v) = nvs.get_i8("led_r") {
        cfg.led_r_gpio = v;
    }
    if let Some(v) = nvs.get_i8("led_g") {
        cfg.led_g_gpio = v;
    }
    if let Some(v) = nvs.get_i8("led_b") {
        cfg.led_b_gpio = v;
    }

    // Input transport.
    if let Some(v) = nvs.get_u8("protocol") {
        // The C maps 1..=4 to MAVLINK/MSP/NMEA/NONE, everything else to AUTO.
        cfg.protocol = match v {
            1 => rid_interface::Protocol::Mavlink,
            2 => rid_interface::Protocol::Msp,
            3 => rid_interface::Protocol::Nmea,
            4 => rid_interface::Protocol::None,
            _ => rid_interface::Protocol::Auto,
        };
    }
    if let Some(v) = nvs.get_u8("uart_port") {
        cfg.uart_port = v;
    }
    if let Some(v) = nvs.get_u8("tx_pin") {
        cfg.tx_pin = v;
    }
    if let Some(v) = nvs.get_u8("rx_pin") {
        cfg.rx_pin = v;
    }

    // WS2812.
    if let Some(v) = nvs.get_i8("ws2812_gpio") {
        cfg.ws2812_gpio = v;
    }
    if let Some(v) = nvs.get_u8("ws2812_br") {
        cfg.ws2812_brightness = v;
    }

    // External lighting.
    {
        let mut buf = [0u8; NUM_LIGHTING_PINS];
        if nvs.get_blob("light_pins", &mut buf) {
            cfg.lighting_pins.copy_from_slice(bytes_i8(&buf));
        }
    }
    {
        let mut buf = [0u8; NUM_LIGHTING_PINS];
        if nvs.get_blob("light_pat", &mut buf) {
            cfg.lighting_patterns = buf;
        }
    }
    load_blob_i16(nvs, "light_phase", &mut cfg.lighting_phase_offsets);

    // DroneCAN.
    if let Some(v) = nvs.get_i8("dronecan_rx") {
        cfg.dronecan_rx_gpio = v;
    }
    if let Some(v) = nvs.get_i8("dronecan_tx") {
        cfg.dronecan_tx_gpio = v;
    }
    if let Some(v) = nvs.get_u32("dronecan_br") {
        cfg.dronecan_bitrate = v;
    }

    // MAVLink USB transport.
    if let Some(v) = nvs.get_u8("mav_usb") {
        cfg.mavlink_usb_enable = v != 0;
    }

    // OTA trigger GPIO.
    if let Some(v) = nvs.get_i8("ota_trig") {
        cfg.ota_trigger_gpio = v;
    }

    if let Some(v) = nvs.get_u32("start_delay") {
        cfg.start_delay_ms = v;
    }

    if let Some(v) = nvs.get_u32("baud") {
        cfg.baud_rate = v;
    }

    if let Some(v) = nvs.get_f32("wifi_pwr") {
        cfg.wifi_power_dbm = v;
    }
    if let Some(v) = nvs.get_f32("wifi_bcn") {
        cfg.wifi_bcn_rate_hz = v;
    }
    if let Some(v) = nvs.get_f32("wifi_nan") {
        cfg.wifi_nan_rate_hz = v;
    }
    if let Some(v) = nvs.get_f32("bt4_rate") {
        cfg.ble4_rate_hz = v;
    }
    if let Some(v) = nvs.get_f32("bt4_pwr") {
        cfg.ble4_power_dbm = v;
    }
    if let Some(v) = nvs.get_f32("bt5_rate") {
        cfg.ble5_rate_hz = v;
    }
    if let Some(v) = nvs.get_f32("bt5_pwr") {
        cfg.ble5_power_dbm = v;
    }

    if let Some(v) = nvs.get_f64("op_lat") {
        cfg.operator_lat = v;
    }
    if let Some(v) = nvs.get_f64("op_lon") {
        cfg.operator_lon = v;
    }
    if let Some(v) = nvs.get_f32("op_alt") {
        cfg.operator_alt = v;
    }

    // Auth private key (persisted for the auth lifecycle).
    nvs.get_blob("auth_key", &mut cfg.auth_private_key);

    for (i, key) in cfg.public_keys.iter_mut().enumerate() {
        let k = pubkey_key(i);
        load_str(nvs, core::str::from_utf8(&k).unwrap(), key, "");
    }
}

/// Port of `nvs_storage_erase()`.
pub fn erase(nvs: &mut impl NvsStore) {
    nvs.erase_all();
}

/// Port of `nvs_storage_reset_preserve_keys()`: wipes the store but restores
/// the public keys that were present.
pub fn reset_preserve_keys(nvs: &mut impl NvsStore) {
    let mut keys = [[0u8; MAX_KEY_LEN + 1]; NUM_KEYS];

    for (i, key) in keys.iter_mut().enumerate() {
        let k = pubkey_key(i);
        load_str(nvs, core::str::from_utf8(&k).unwrap(), key, "");
    }

    nvs.erase_all();

    for (i, key) in keys.iter().enumerate() {
        if key[0] != 0 {
            let k = pubkey_key(i);
            nvs.set_str(core::str::from_utf8(&k).unwrap(), cstr(key));
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests mirror the C flow of mutate-then-save, so mutating a Default
    // instance field-by-field is intentional.
    #![allow(clippy::field_reassign_with_default)]
    extern crate std;

    use super::*;
    use crate::config::BspConfig;
    use std::collections::HashMap;
    use std::string::{String, ToString};
    use std::vec::Vec;

    enum Value {
        Str(String),
        U8(u8),
        I8(i8),
        U32(u32),
        F32(f32),
        F64(f64),
        Blob(Vec<u8>),
    }

    /// In-memory `NvsStore` for tests, with the ESP-IDF string semantics
    /// (NUL-terminated reads, truncation to the destination buffer).
    struct MemNvs(HashMap<String, Value>);

    impl MemNvs {
        fn new() -> Self {
            Self(HashMap::new())
        }
    }

    impl NvsStore for MemNvs {
        fn get_str(&mut self, key: &str, out: &mut [u8]) -> bool {
            match self.0.get(key) {
                Some(Value::Str(s)) => {
                    copy_cstr(out, s);
                    true
                }
                _ => false,
            }
        }
        fn set_str(&mut self, key: &str, value: &str) {
            self.0
                .insert(key.to_string(), Value::Str(value.to_string()));
        }
        fn get_u8(&mut self, key: &str) -> Option<u8> {
            match self.0.get(key) {
                Some(Value::U8(v)) => Some(*v),
                _ => None,
            }
        }
        fn set_u8(&mut self, key: &str, value: u8) {
            self.0.insert(key.to_string(), Value::U8(value));
        }
        fn get_i8(&mut self, key: &str) -> Option<i8> {
            match self.0.get(key) {
                Some(Value::I8(v)) => Some(*v),
                _ => None,
            }
        }
        fn set_i8(&mut self, key: &str, value: i8) {
            self.0.insert(key.to_string(), Value::I8(value));
        }
        fn get_u32(&mut self, key: &str) -> Option<u32> {
            match self.0.get(key) {
                Some(Value::U32(v)) => Some(*v),
                _ => None,
            }
        }
        fn set_u32(&mut self, key: &str, value: u32) {
            self.0.insert(key.to_string(), Value::U32(value));
        }
        fn get_f32(&mut self, key: &str) -> Option<f32> {
            match self.0.get(key) {
                Some(Value::F32(v)) => Some(*v),
                _ => None,
            }
        }
        fn set_f32(&mut self, key: &str, value: f32) {
            self.0.insert(key.to_string(), Value::F32(value));
        }
        fn get_f64(&mut self, key: &str) -> Option<f64> {
            match self.0.get(key) {
                Some(Value::F64(v)) => Some(*v),
                _ => None,
            }
        }
        fn set_f64(&mut self, key: &str, value: f64) {
            self.0.insert(key.to_string(), Value::F64(value));
        }
        fn get_blob(&mut self, key: &str, out: &mut [u8]) -> bool {
            match self.0.get(key) {
                Some(Value::Blob(b)) if b.len() <= out.len() => {
                    out[..b.len()].copy_from_slice(b);
                    true
                }
                _ => false,
            }
        }
        fn set_blob(&mut self, key: &str, value: &[u8]) {
            self.0.insert(key.to_string(), Value::Blob(value.to_vec()));
        }
        fn erase_all(&mut self) {
            self.0.clear();
        }
    }

    #[test]
    fn save_then_load_roundtrips() {
        let mut cfg = BspConfig::default();
        cfg.region = rid_interface::Region::Kor;
        cfg.ua_type = 3;
        cfg.id_type = 2;
        cfg.uas_id = rid_interface::fixed_str("SN-123456789");
        cfg.operator_id = rid_interface::fixed_str("OP-HOME-42");
        cfg.self_id_text = rid_interface::fixed_str("test drone");
        cfg.uas_id_2 = rid_interface::fixed_str("SN-2nd");
        cfg.wifi_ssid = rid_interface::fixed_str("RID-AP");
        cfg.wifi_password = rid_interface::fixed_str("secret");
        cfg.wifi_channel = 11;
        cfg.webserver_en = 0;
        cfg.mavlink_sysid = 2;
        cfg.bcast_powerup = 0;
        cfg.tx_modes = 0x0F;
        cfg.options = 0x180;
        cfg.lock_level = 1;
        cfg.led_r_gpio = 2;
        cfg.led_g_gpio = -1;
        cfg.led_b_gpio = 4;
        cfg.baud_rate = 115200;
        cfg.wifi_power_dbm = 17.5;
        cfg.wifi_bcn_rate_hz = 2.0;
        cfg.wifi_nan_rate_hz = 0.5;
        cfg.ble4_rate_hz = 3.0;
        cfg.ble4_power_dbm = 8.0;
        cfg.ble5_rate_hz = 1.0;
        cfg.ble5_power_dbm = 9.0;
        cfg.operator_lat = 45.304;
        cfg.operator_lon = 11.9537;
        cfg.operator_alt = 100.5;

        // Input transport.
        cfg.protocol = rid_interface::Protocol::Msp;
        cfg.uart_port = 2;
        cfg.tx_pin = 21;
        cfg.rx_pin = 22;

        // WS2812.
        cfg.ws2812_gpio = 8;
        cfg.ws2812_brightness = 42;

        // External lighting.
        cfg.lighting_pins = [3, -1, 5, -1, 7];
        cfg.lighting_patterns = [1, 2, 3, 4, 5];
        cfg.lighting_phase_offsets = [100, 200, -50, 400, 500];

        // DroneCAN.
        cfg.dronecan_rx_gpio = 13;
        cfg.dronecan_tx_gpio = 14;
        cfg.dronecan_bitrate = 250000;

        // MAVLink USB transport.
        cfg.mavlink_usb_enable = true;

        // OTA trigger GPIO.
        cfg.ota_trigger_gpio = 9;

        // Startup delay.
        cfg.start_delay_ms = 15000;

        // Auth private key.
        cfg.auth_private_key[0] = b'M';
        cfg.auth_private_key[1] = b'Y';
        cfg.auth_private_key[2] = b'K';

        cfg.public_keys[0] = rid_interface::key_str("AB");

        let mut nvs = MemNvs::new();
        save(&cfg, &mut nvs);

        let mut out = BspConfig::default();
        load(&mut out, &mut nvs);
        assert_eq!(out, cfg);
    }

    #[test]
    fn persisted_arrays_and_blobs_roundtrip_fully() {
        // Guards the newly-added BSP-only persistence (issue #25): every field
        // that used to silently revert on reboot must survive a save/load.
        let mut cfg = BspConfig::default();
        cfg.protocol = rid_interface::Protocol::Nmea;
        cfg.uart_port = 3;
        cfg.tx_pin = 30;
        cfg.rx_pin = 31;
        cfg.ws2812_gpio = 4;
        cfg.ws2812_brightness = 99;
        cfg.lighting_pins = [1, 2, 3, 4, 5];
        cfg.lighting_patterns = [9, 8, 7, 6, 5];
        cfg.lighting_phase_offsets = [-1, -2, -3, -4, -5];
        cfg.dronecan_rx_gpio = 15;
        cfg.dronecan_tx_gpio = 16;
        cfg.dronecan_bitrate = 500000;
        cfg.mavlink_usb_enable = false;
        cfg.ota_trigger_gpio = -1;
        cfg.start_delay_ms = 0;
        cfg.auth_private_key[..4].copy_from_slice(b"abcd");

        let mut nvs = MemNvs::new();
        save(&cfg, &mut nvs);
        let mut out = BspConfig::default();
        load(&mut out, &mut nvs);
        assert_eq!(out, cfg);
    }

    #[test]
    fn missing_new_fields_keep_defaults_on_load() {
        // A store written by the legacy C doesn't contain the new keys; load
        // must leave the BSP-only defaults intact.
        let mut cfg = BspConfig::default();
        // Overwrite some defaults so we can tell they are kept.
        cfg.protocol = rid_interface::Protocol::Msp;
        cfg.uart_port = 2;
        cfg.ws2812_gpio = 8;
        cfg.lighting_pins = [3, 3, 3, 3, 3];
        cfg.dronecan_bitrate = 999;
        cfg.start_delay_ms = 12345;

        let mut nvs = MemNvs::new();
        load(&mut cfg, &mut nvs);

        assert_eq!(cfg.protocol, rid_interface::Protocol::Msp);
        assert_eq!(cfg.uart_port, 2);
        assert_eq!(cfg.ws2812_gpio, 8);
        assert_eq!(cfg.lighting_pins, [3, 3, 3, 3, 3]);
        assert_eq!(cfg.dronecan_bitrate, 999);
        assert_eq!(cfg.start_delay_ms, 12345);
        // Fields not touched keep their pre-load defaults.
        assert_eq!(cfg.lighting_patterns, [0; 5]);
        assert_eq!(cfg.lighting_phase_offsets, [0; 5]);
    }

    #[test]
    fn empty_store_behaves_like_factory_nvs() {
        // The C `load_str` copies the default with `strncpy` even when the key
        // is missing, so defaults of "" zero the buffer (the hub config is
        // expected to be empty until the user fills it in).
        let mut cfg = BspConfig::default();
        cfg.region = rid_interface::Region::Faa;
        let mut nvs = MemNvs::new();
        load(&mut cfg, &mut nvs);

        assert_eq!(cfg.uas_id, [0; rid_interface::MAX_STR_LEN + 1]);
        assert_eq!(cfg.operator_id, [0; rid_interface::MAX_STR_LEN + 1]);
        // wifi_ssid has a non-empty default.
        assert_eq!(cstr(&cfg.wifi_ssid), "ESP-RID");
        // Numeric/region fields keep the pre-load values.
        assert_eq!(cfg.region, rid_interface::Region::Faa);
        assert_eq!(cfg.ua_type, 1);
        assert_eq!(cfg.wifi_channel, 6);
    }

    #[test]
    fn out_of_range_region_clamps_to_auto() {
        let mut cfg = BspConfig::default();
        let mut nvs = MemNvs::new();
        nvs.set_u8("region", 255);
        load(&mut cfg, &mut nvs);
        assert_eq!(cfg.region, rid_interface::Region::Auto);

        let mut nvs = MemNvs::new();
        nvs.set_u8("region", 11); // NZL + 1
        load(&mut cfg, &mut nvs);
        assert_eq!(cfg.region, rid_interface::Region::Auto);

        let mut nvs = MemNvs::new();
        nvs.set_u8("region", 5);
        load(&mut cfg, &mut nvs);
        assert_eq!(cfg.region, rid_interface::Region::Kor);
    }

    #[test]
    fn options_truncated_to_u16_like_c() {
        let mut cfg = BspConfig::default();
        let mut nvs = MemNvs::new();
        nvs.set_u32("options", 0x0001_1234);
        load(&mut cfg, &mut nvs);
        assert_eq!(cfg.options, 0x1234);
    }

    #[test]
    fn wifi_ssid_default_is_esp_rid() {
        let mut cfg = BspConfig::default();
        cfg.wifi_ssid = rid_interface::fixed_str("overwritten");
        let mut nvs = MemNvs::new();
        load(&mut cfg, &mut nvs);
        assert_eq!(cstr(&cfg.wifi_ssid), "ESP-RID");
    }

    #[test]
    fn reset_preserves_public_keys_only() {
        let mut cfg = BspConfig::default();
        cfg.ua_type = 9;
        cfg.public_keys[2] = rid_interface::key_str("KEEP-ME");
        let mut nvs = MemNvs::new();
        save(&cfg, &mut nvs);

        reset_preserve_keys(&mut nvs);

        let mut out = BspConfig::default();
        load(&mut out, &mut nvs);
        // Other settings erased.
        assert_eq!(out.ua_type, 1);
        // Third public key restored.
        assert_eq!(cstr(&out.public_keys[2]), "KEEP-ME");
        // Empty keys were not written back.
        assert_eq!(cstr(&out.public_keys[0]), "");
    }

    #[test]
    fn erase_clears_everything() {
        let mut cfg = BspConfig::default();
        cfg.ua_type = 5;
        cfg.region = rid_interface::Region::Chn;
        cfg.wifi_ssid = rid_interface::fixed_str("PERSISTED");
        let mut nvs = MemNvs::new();
        save(&cfg, &mut nvs);
        erase(&mut nvs);

        let mut out = BspConfig::default();
        load(&mut out, &mut nvs);
        // Erased: stored values are gone, wifi_ssid falls back to its default.
        assert_eq!(out.ua_type, 1);
        assert_eq!(out.region, rid_interface::Region::Auto);
        assert_eq!(cstr(&out.wifi_ssid), "ESP-RID");
        assert_eq!(out.uas_id, [0; rid_interface::MAX_STR_LEN + 1]);
    }

    #[test]
    fn long_string_truncated_like_strncpy() {
        let mut nvs = MemNvs::new();
        nvs.set_str("uas_id", &"X".repeat(100));
        let mut cfg = BspConfig::default();
        load(&mut cfg, &mut nvs);
        // MAX_STR_LEN bytes + NUL.
        assert_eq!(cfg.uas_id.len(), rid_interface::MAX_STR_LEN + 1);
        assert_eq!(&cfg.uas_id[..32], b"XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX");
        assert_eq!(cfg.uas_id[32], 0);
    }
}
