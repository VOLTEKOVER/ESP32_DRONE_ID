//! Build capabilities for the adaptive web UI (`/api/capabilities`, Fase 5).
//! Everything is compile-time: the protocols/inputs, regions, standards and
//! encoder support, transports and options this firmware build understands.
//! Pure and host-testable.

use alloc::string::String;
use rid_app::state::FW_VERSION;
use rid_core::hub;
use rid_interface::{
    Region, Standard, OPT_AUTH_ED25519, OPT_DEMO_MODE, OPT_DONT_SAVE_BASIC_ID, OPT_FORCE_ARM_OK,
    OPT_IDENTITY_READY_GATE, OPT_KALMAN_FILTER, OPT_MAVLINK_ARM_STATUS, OPT_MAVLINK_OP_LOC_LOOP,
    OPT_PRINT_RID_MAVLINK,
};
use serde_json::{Map, Value};

/// Input protocols the build supports (primary + secondary sources).
pub const INPUTS: [&str; 5] = ["AUTO", "NMEA", "MSP", "MAVLink", "DroneCAN"];

/// Broadcast transports (bitmask order of `RID_TRANSMIT_*`).
pub const TX_MODES: [&str; 4] = ["WIFI_BCN", "WIFI_NAN", "BLE4", "BLE5"];

/// Build options (bitmask order of `RID_OPT_*`).
pub const OPTIONS: [(u16, &str); 9] = [
    (OPT_FORCE_ARM_OK, "FORCE_ARM_OK"),
    (OPT_DONT_SAVE_BASIC_ID, "DONT_SAVE_BASIC_ID"),
    (OPT_PRINT_RID_MAVLINK, "PRINT_RID_MAVLINK"),
    (OPT_DEMO_MODE, "DEMO_MODE"),
    (OPT_KALMAN_FILTER, "KALMAN_FILTER"),
    (OPT_AUTH_ED25519, "AUTH_ED25519"),
    (OPT_MAVLINK_ARM_STATUS, "MAVLINK_ARM_STATUS"),
    (OPT_MAVLINK_OP_LOC_LOOP, "MAVLINK_OP_LOC_LOOP"),
    (OPT_IDENTITY_READY_GATE, "IDENTITY_READY_GATE"),
];

/// Region names in `rid_region_t` order (port of `g_region_names`).
pub fn region_names() -> [&'static str; Region::COUNT] {
    core::array::from_fn(|i| hub::region_name(Region::from_raw(i as u8).unwrap()))
}

/// Standard names in `rid_standard_t` order (port of `g_standard_names`).
pub fn standard_names() -> [&'static str; Standard::COUNT] {
    core::array::from_fn(|i| hub::standard_name(Standard::from_raw(i as u8).unwrap()))
}

/// Encoder availability per standard (`rid_output_has_encoder`).
pub fn encoder_support() -> [bool; Standard::COUNT] {
    core::array::from_fn(|i| hub::has_encoder(Standard::from_raw(i as u8).unwrap()))
}

/// The `/api/capabilities` payload.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Capabilities {
    pub fw_version: &'static str,
    pub inputs: [&'static str; 5],
    pub tx_modes: [&'static str; 4],
    pub options: [&'static str; 9],
    pub regions: [&'static str; Region::COUNT],
    pub standards: [&'static str; Standard::COUNT],
    pub encoder_support: [bool; Standard::COUNT],
}

impl Capabilities {
    pub fn build() -> Self {
        Self {
            fw_version: FW_VERSION,
            inputs: INPUTS,
            tx_modes: TX_MODES,
            options: OPTIONS.map(|(_, name)| name),
            regions: region_names(),
            standards: standard_names(),
            encoder_support: encoder_support(),
        }
    }

    /// Serializes to the JSON served by `/api/capabilities`.
    pub fn to_json(&self) -> String {
        let mut m = Map::new();
        m.insert("fw_version".into(), Value::from(self.fw_version));
        m.insert("inputs".into(), Value::from(self.inputs.as_slice()));
        m.insert("tx_modes".into(), Value::from(self.tx_modes.as_slice()));
        m.insert("options".into(), Value::from(self.options.as_slice()));
        m.insert("regions".into(), Value::from(self.regions.as_slice()));
        m.insert("standards".into(), Value::from(self.standards.as_slice()));

        let mut enc = Map::new();
        for (i, name) in self.standards.iter().enumerate() {
            enc.insert((*name).into(), Value::from(self.encoder_support[i]));
        }
        m.insert("has_encoder".into(), Value::Object(enc));

        serde_json::to_string(&Value::Object(m)).expect("Capabilities serialization")
    }
}

impl Default for Capabilities {
    fn default() -> Self {
        Self::build()
    }
}

/// Convenience: full `/api/capabilities` payload for the current build.
pub fn capabilities_json() -> String {
    Capabilities::build().to_json()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_match_the_c_ordering() {
        assert_eq!(INPUTS, ["AUTO", "NMEA", "MSP", "MAVLink", "DroneCAN"]);
        assert_eq!(TX_MODES, ["WIFI_BCN", "WIFI_NAN", "BLE4", "BLE5"]);
        assert_eq!(region_names().len(), 11);
        assert_eq!(region_names()[0], "AUTO");
        assert_eq!(region_names()[6], "CHN");
        assert_eq!(region_names()[10], "NZL");
        assert_eq!(standard_names()[0], "ASTM F3411-22a");
        assert_eq!(standard_names()[1], "China GB 42590");
        assert_eq!(standard_names()[2], "FRDID");
    }

    #[test]
    fn only_astm_has_an_encoder() {
        assert_eq!(encoder_support(), [true, false, false]);
    }

    #[test]
    fn option_bits_match_the_mask() {
        assert_eq!(OPTIONS[0].0, 1);
        assert_eq!(OPTIONS[1].0, 1 << 1);
        assert_eq!(OPTIONS[2].0, 1 << 2);
        assert_eq!(OPTIONS[3].0, 1 << 3);
        assert_eq!(OPTIONS[4].0, 1 << 4);
        assert_eq!(OPTIONS[5].0, 1 << 5);
        assert_eq!(OPTIONS[6].0, 1 << 6);
        assert_eq!(OPTIONS[7].0, 1 << 7);
        assert_eq!(OPTIONS[8].0, 1 << 8);
    }

    #[test]
    fn json_is_valid_and_complete() {
        let json = capabilities_json();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["fw_version"], FW_VERSION);
        assert_eq!(v["inputs"][0], "AUTO");
        assert_eq!(v["regions"].as_array().unwrap().len(), 11);
        assert_eq!(v["standards"].as_array().unwrap().len(), 3);
        assert_eq!(v["has_encoder"]["ASTM F3411-22a"], true);
        assert_eq!(v["has_encoder"]["China GB 42590"], false);
        assert_eq!(v["options"].as_array().unwrap().len(), 9);
        assert_eq!(v["tx_modes"].as_array().unwrap().len(), 4);
    }
}
