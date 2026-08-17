//! `esp_rid_*` glue, port of the non-hardware parts of `esp_remote_id.c`:
//! the config lifecycle (`set_config`, `factory_reset` with public-key
//! preservation), the MAC-based default ID derivation from `esp_rid_init`
//! and the MAVLink TX task enable condition. The loop itself runs through
//! `rid_core::scheduler::Scheduler`.

use rid_app::config::{BspConfig, cstr};
use rid_core::scheduler::{Scheduler, TickOutcome};
use rid_interface::{Config, InputSample, Standard, State, Transmitter, fixed_str,
    OPT_MAVLINK_ARM_STATUS, OPT_MAVLINK_OP_LOC_LOOP};

/// Derives the hub-facing `Config` from the full BSP config (the subset the
/// scheduler/hub use; transport/pin/lighting fields stay BSP-side).
pub fn core_config(bsp: &BspConfig) -> Config {
    Config {
        region: bsp.region,
        ua_type: bsp.ua_type,
        id_type: bsp.id_type,
        uas_id: bsp.uas_id,
        operator_id: bsp.operator_id,
        ua_type_2: bsp.ua_type_2,
        id_type_2: bsp.id_type_2,
        uas_id_2: bsp.uas_id_2,
        self_id_text: bsp.self_id_text,
        options: bsp.options,
        operator_lat: bsp.operator_lat,
        operator_lon: bsp.operator_lon,
        operator_alt: bsp.operator_alt,
        tx_modes: bsp.tx_modes,
        wifi_bcn_rate_hz: bsp.wifi_bcn_rate_hz,
        wifi_nan_rate_hz: bsp.wifi_nan_rate_hz,
        ble4_rate_hz: bsp.ble4_rate_hz,
        ble5_rate_hz: bsp.ble5_rate_hz,
        bcast_powerup: bsp.bcast_powerup != 0,
        mavlink_sysid: bsp.mavlink_sysid,
        lock_level: bsp.lock_level,
    }
}

/// Port of the MAVLink TX task enable condition in `esp_rid_init`.
pub fn mavlink_tx_enabled(config: &BspConfig) -> bool {
    (config.options & (OPT_MAVLINK_ARM_STATUS | OPT_MAVLINK_OP_LOC_LOOP) != 0)
        || config.mavlink_usb_enable
}

/// Port of the placeholder check in `esp_rid_init`: either ID is still a
/// factory default.
pub fn is_placeholder_id(config: &BspConfig) -> bool {
    cstr(&config.uas_id) == "ESP32-RID-001" || cstr(&config.operator_id) == "OP-UNKNOWN"
}

/// Port of the MAC-based ID derivation in `esp_rid_init`: when a placeholder
/// ID is present, both IDs are replaced with the last two MAC bytes
/// (`ESP32-RID-<hex>` / `ESP32-OP-<hex>`). Returns true when it changed.
pub fn derive_ids_from_mac(config: &mut BspConfig, mac: &[u8; 6]) -> bool {
    if is_placeholder_id(config) {
        config.uas_id = fixed_str(&alloc::format!("ESP32-RID-{:02X}{:02X}", mac[4], mac[5]));
        config.operator_id = fixed_str(&alloc::format!("ESP32-OP-{:02X}{:02X}", mac[4], mac[5]));
        true
    } else {
        false
    }
}

/// Result of `Controller::set_config` (port of `esp_rid_set_config`): the
/// rebound standard/fallback and whether the UART must be re-initialized.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SetConfigOutcome {
    pub active_standard: Standard,
    pub standard_fallback: bool,
    /// `baud_rate` changed to a nonzero value (`protocol_detect_reinit`).
    pub protocol_reinit_required: bool,
}

/// Application controller, port of the `g_config`/`g_state` globals plus the
/// `esp_rid_*` API. The scheduler owns the runtime state.
#[derive(Debug)]
pub struct Controller {
    pub bsp_config: BspConfig,
    core: Config,
    pub scheduler: Scheduler,
}

impl Controller {
    /// Port of the config load + standard binding in `esp_rid_init`.
    pub fn new() -> Self {
        let bsp_config = BspConfig::default();
        let core = core_config(&bsp_config);
        let mut scheduler = Scheduler::new();
        scheduler.apply_config(&core);
        Self {
            bsp_config,
            core,
            scheduler,
        }
    }

    /// Port of `esp_rid_set_config` (minus the NVS save and the BSP
    /// reconfiguration, which are hardware side).
    pub fn set_config(&mut self, new: &BspConfig) -> SetConfigOutcome {
        let old_baud = self.bsp_config.baud_rate;
        self.bsp_config = new.clone();
        self.core = core_config(&self.bsp_config);
        self.scheduler.apply_config(&self.core);
        SetConfigOutcome {
            active_standard: self.scheduler.state.active_standard,
            standard_fallback: self.scheduler.state.standard_fallback,
            protocol_reinit_required: self.bsp_config.baud_rate != old_baud
                && self.bsp_config.baud_rate > 0,
        }
    }

    /// Port of `esp_rid_factory_reset`: the config is reset to defaults
    /// preserving the public keys (and the scheduler is rebound).
    pub fn factory_reset(&mut self) {
        let keys = self.bsp_config.public_keys;
        self.bsp_config = BspConfig::default();
        self.bsp_config.public_keys = keys;
        self.core = core_config(&self.bsp_config);
        self.scheduler.apply_config(&self.core);
    }

    /// Port of the MAC-based default ID derivation in `esp_rid_init`.
    pub fn derive_default_ids(&mut self, mac: &[u8; 6]) -> bool {
        if derive_ids_from_mac(&mut self.bsp_config, mac) {
            self.core = core_config(&self.bsp_config);
            self.scheduler.apply_config(&self.core);
            true
        } else {
            false
        }
    }

    /// One scheduler loop iteration (port of one `rid_task` iteration).
    pub fn step(&mut self, input: &InputSample, out: &mut impl Transmitter) -> TickOutcome {
        self.scheduler.tick(input, &self.core, out)
    }

    pub fn state(&self) -> &State {
        &self.scheduler.state
    }

    pub fn config(&self) -> &BspConfig {
        &self.bsp_config
    }

    /// `/api/config` payload.
    pub fn config_json(&self) -> alloc::string::String {
        rid_app::json::config_to_json(&self.bsp_config)
    }

    /// `/api/status` payload.
    pub fn status_json(&self) -> alloc::string::String {
        rid_app::state::state_to_json(&state_snapshot(&self.scheduler.state))
    }

    /// `/api/capabilities` payload.
    pub fn capabilities_json(&self) -> alloc::string::String {
        crate::capabilities::capabilities_json()
    }
}

impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}

/// Copies the scheduler runtime state into the `rid-app` JSON snapshot type.
fn state_snapshot(state: &State) -> rid_app::state::State {
    rid_app::state::State {
        gps: state.gps,
        identity: state.identity,
        active_protocol: state.active_protocol,
        last_update_ms: state.last_update_ms,
        transmissions_count: state.transmissions_count,
        wifi_bcn_count: state.wifi_bcn_count,
        wifi_nan_count: state.wifi_nan_count,
        ble4_count: state.ble4_count,
        ble5_count: state.ble5_count,
        gps_valid: state.gps_valid,
        identity_ready: state.identity_ready,
        mavlink_armed: state.mavlink_armed,
        mavlink_sysid: state.mavlink_sysid,
        operator_lat: state.operator_lat,
        operator_lon: state.operator_lon,
        operator_alt: state.operator_alt,
        operator_position_updated_ms: state.operator_position_updated_ms,
        operator_location_type: state.operator_location_type,
        auth_enabled: state.auth_enabled,
        active_standard: state.active_standard,
        standard_fallback: state.standard_fallback,
        takeoff_lat: state.takeoff_lat,
        takeoff_lon: state.takeoff_lon,
        takeoff_alt: state.takeoff_alt,
        takeoff_captured: state.takeoff_captured,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rid_app::config::{NUM_LIGHTING_PINS, cstr};
    use rid_interface::{
        CStr, GpsData, Identity, Protocol, Region, TRANSMIT_BLE4, MAX_KEY_LEN,
        NUM_KEYS,
    };

    #[derive(Default)]
    struct Recorder {
        bcn: u32,
        nan: u32,
        ble4: u32,
        ble5: u32,
    }

    impl Transmitter for Recorder {
        fn wifi_bcn(&mut self, _g: &GpsData, _i: &Identity, _c: &Config) {
            self.bcn += 1;
        }
        fn wifi_nan(&mut self, _g: &GpsData, _i: &Identity, _c: &Config, _n: u8) {
            self.nan += 1;
        }
        fn ble4(&mut self, _g: &GpsData, _i: &Identity, _c: &Config) {
            self.ble4 += 1;
        }
        fn ble5(&mut self, _g: &GpsData, _i: &Identity, _c: &Config) {
            self.ble5 += 1;
        }
    }

    fn sample(ms: u32, gps: Option<GpsData>) -> InputSample {
        InputSample {
            proto: Protocol::Nmea,
            gps,
            ..InputSample::new(ms, ms as u64 * 1000)
        }
    }

    fn fix() -> GpsData {
        GpsData {
            latitude: 45.5,
            longitude: 9.2,
            altitude_msl: 120.0,
            fix_type: 4,
            satellites: 12,
            armed: true,
            ..GpsData::default()
        }
    }

    #[test]
    fn core_config_maps_bsp_fields() {
        let bsp = BspConfig {
            region: Region::Chn,
            bcast_powerup: 0,
            tx_modes: TRANSMIT_BLE4,
            ble4_rate_hz: 2.5,
            options: 1,
            ..BspConfig::default()
        };
        let c = core_config(&bsp);
        assert_eq!(c.region, Region::Chn);
        assert!(!c.bcast_powerup);
        assert_eq!(c.tx_modes, TRANSMIT_BLE4);
        assert_eq!(c.ble4_rate_hz, 2.5);
        assert_eq!(c.options, 1);
        assert_eq!(c.lock_level, 0);
        // Default bcast_powerup = 1 -> true.
        let c = core_config(&BspConfig::default());
        assert!(c.bcast_powerup);
    }

    #[test]
    fn placeholder_detection_and_derive() {
        let mut bsp = BspConfig::default();
        assert!(is_placeholder_id(&bsp));
        let mac = [0xDE, 0xAD, 0xBE, 0xEF, 0xAB, 0xCD];
        assert!(derive_ids_from_mac(&mut bsp, &mac));
        assert_eq!(cstr(&bsp.uas_id), "ESP32-RID-ABCD");
        assert_eq!(cstr(&bsp.operator_id), "ESP32-OP-ABCD");
        assert!(!is_placeholder_id(&bsp));
        // Already derived -> no change.
        assert!(!derive_ids_from_mac(&mut bsp, &mac));
        assert_eq!(cstr(&bsp.uas_id), "ESP32-RID-ABCD");
    }

    #[test]
    fn set_config_binds_standard_and_detects_baud() {
        let mut ctl = Controller::new();
        // Same baud -> no reinit; standard binding for CHN -> GB with fallback.
        let mut cfg = BspConfig {
            region: Region::Chn,
            ..BspConfig::default()
        };
        let out = ctl.set_config(&cfg);
        assert_eq!(out.active_standard, Standard::ChnGb);
        assert!(out.standard_fallback);
        assert!(!out.protocol_reinit_required);
        assert_eq!(ctl.state().active_standard, Standard::ChnGb);
        // Baud change -> reinit required.
        cfg.baud_rate = 115200;
        let out = ctl.set_config(&cfg);
        assert!(out.protocol_reinit_required);
    }

    #[test]
    fn factory_reset_preserves_public_keys() {
        let mut ctl = Controller::new();
        let cfg = BspConfig {
            region: Region::Faa,
            public_keys: {
                let mut k = [rid_interface::key_str(""); NUM_KEYS];
                k[0] = rid_interface::key_str("ED25519KEY");
                k
            },
            ..BspConfig::default()
        };
        ctl.set_config(&cfg);
        ctl.factory_reset();
        assert_eq!(ctl.config().region, Region::Auto);
        assert_eq!(ctl.state().active_standard, Standard::Astm);
        assert_eq!(cstr(&ctl.config().public_keys[0]), "ED25519KEY");
        assert_eq!(ctl.config().public_keys[1], [0; MAX_KEY_LEN + 1]);
        assert_eq!(ctl.config().lighting_pins, [-1; NUM_LIGHTING_PINS]);
        assert_eq!(ctl.config().public_keys.len(), NUM_KEYS);
    }

    #[test]
    fn derive_default_ids_via_controller() {
        let mut ctl = Controller::new();
        assert!(ctl.derive_default_ids(&[0, 0, 0, 0, 0x12, 0x34]));
        // The hub-facing config follows the BSP config.
        assert_eq!(cstr(&ctl.core.uas_id), "ESP32-RID-1234");
    }

    #[test]
    fn mavlink_tx_condition() {
        let mut cfg = BspConfig::default();
        assert!(!mavlink_tx_enabled(&cfg));
        cfg.options |= OPT_MAVLINK_ARM_STATUS;
        assert!(mavlink_tx_enabled(&cfg));
        let cfg = BspConfig {
            options: 0,
            mavlink_usb_enable: true,
            ..BspConfig::default()
        };
        assert!(mavlink_tx_enabled(&cfg));
    }

    #[test]
    fn step_runs_the_scheduler_loop() {
        let mut ctl = Controller::new();
        ctl.bsp_config.region = Region::Eur;
        let _ = ctl.set_config(&ctl.bsp_config.clone());
        let mut rec = Recorder::default();
        let out = ctl.step(&sample(1000, Some(fix())), &mut rec);
        assert!(out.tx_fired);
        assert!(ctl.state().gps_valid);
        assert_eq!(rec.bcn, 1);
        assert!(ctl.state().identity.uas_id.c_starts_with("ESP32-RID-"));
    }

    #[test]
    fn json_endpoints_produce_valid_json() {
        let ctl = Controller::new();
        let cfg_json = ctl.config_json();
        let status_json = ctl.status_json();
        let caps_json = ctl.capabilities_json();
        assert!(serde_json::from_str::<serde_json::Value>(&cfg_json).is_ok());
        assert!(serde_json::from_str::<serde_json::Value>(&status_json).is_ok());
        assert!(serde_json::from_str::<serde_json::Value>(&caps_json).is_ok());
        assert!(cfg_json.contains("\"region\":"));
        assert!(status_json.contains("\"gps_valid\":false"));
        assert!(caps_json.contains("\"fw_version\":"));
    }
}
