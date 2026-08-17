//! NVS persistence, port of `nvs_storage.c`. The key/value semantics of the
//! ESP-IDF NVS are abstracted behind [`NvsStore`], so the exact field/key
//! handling, defaults, clamping and the reset-preserving-keys logic are
//! host-testable. The ESP32 NVS implementation of the trait lands with the
//! hardware phase.

use rid_interface::{MAX_KEY_LEN, NUM_KEYS};

use crate::config::{BspConfig, clamp_region, cstr};

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
    fn erase_all(&mut self);
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

    nvs.set_u32("baud", cfg.baud_rate);

    nvs.set_f32("wifi_pwr", cfg.wifi_power_dbm);
    nvs.set_f32("wifi_bcn", cfg.wifi_bcn_rate_hz);
    nvs.set_f32("wifi_nan", cfg.wifi_nan_rate_hz);
    nvs.set_f32("bt4_rate", cfg.ble4_rate_hz);
    nvs.set_f32("bt4_pwr", cfg.ble4_power_dbm);
    nvs.set_f32("bt5_rate", cfg.ble5_rate_hz);
    nvs.set_f32("bt5_pwr", cfg.ble5_power_dbm);

    nvs.set_f32("op_lat", cfg.operator_lat as f32);
    nvs.set_f32("op_lon", cfg.operator_lon as f32);
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

    if let Some(v) = nvs.get_f32("op_lat") {
        cfg.operator_lat = v as f64;
    }
    if let Some(v) = nvs.get_f32("op_lon") {
        cfg.operator_lon = v as f64;
    }
    if let Some(v) = nvs.get_f32("op_alt") {
        cfg.operator_alt = v;
    }

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

    enum Value {
        Str(String),
        U8(u8),
        I8(i8),
        U32(u32),
        F32(f32),
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
            self.0.insert(key.to_string(), Value::Str(value.to_string()));
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
        cfg.public_keys[0] = rid_interface::key_str("AB");

        let mut nvs = MemNvs::new();
        save(&cfg, &mut nvs);

        let mut out = BspConfig::default();
        load(&mut out, &mut nvs);

        // operator_* round-trip through f32 exactly like the C store.
        cfg.operator_lat = cfg.operator_lat as f32 as f64;
        cfg.operator_lon = cfg.operator_lon as f32 as f64;
        assert_eq!(out, cfg);
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
        assert_eq!(&cfg.uas_id[..20], b"XXXXXXXXXXXXXXXXXXXX");
        assert_eq!(cfg.uas_id[20], 0);
    }
}
