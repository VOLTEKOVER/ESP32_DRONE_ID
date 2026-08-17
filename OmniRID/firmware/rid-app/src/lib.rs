//! Board-agnostic Remote ID application layer, port of the pure parts of
//! `components/esp_remote_id` (BSP application logic that does not touch
//! hardware): firmware config (`rid_config_t`), the web JSON config API, the
//! console CLI, security primitives, the runtime state JSON, the NVS storage
//! contract and the BLE 4.x legacy advertisement framing.
//!
//! The crate is `no_std` + `alloc` and has no ESP-IDF dependency: it builds
//! and tests on any host. Board glues are thin adapters on top of it:
//! `bsp-esp32` (real hardware) and `bsp-sim` (Windows host simulation).
#![no_std]

extern crate alloc;

pub mod ble4;
pub mod cli;
pub mod config;
pub mod json;
pub mod led_status;
pub mod led_ws2812;
pub mod lighting;
pub mod nvs;
pub mod ota;
pub mod state;
pub mod web;
pub mod web_config;
pub mod webui;

pub use ble4::build_legacy_adv;
pub use cli::{
    MAX_ARGS, ConfigSetError, KalmanUsageError, TxModeError, apply_demo_mode, apply_kalman,
    config_set_field, parse_i64_base0, parse_line, parse_log_level, parse_protocol_name, proto_name,
    set_tx_mode,
};
pub use config::{BspConfig, AUTH_KEY_MAX_LEN, NUM_LIGHTING_PINS};
pub use json::{apply_json, config_to_json, parse_region_name, region_name};
pub use led_status::{
    LedPattern, LedState, LedStateMachine, Rgb, TX_FLASH_MS, blink_1hz, blink_4hz, blink_double,
    led_state_entry, pulse, rainbow, solid,
};
pub use led_ws2812::{brightness_scalar, hsv_to_rgb, rgb_to_grb, scale_rgb, ws2812_frame};
pub use lighting::{
    LightingChannel, LightingPattern, channel_active, channels_from_config, pattern_active,
};
pub use nvs::{NvsStore, erase, load, reset_preserve_keys, save};
pub use ota::{
    OTA_BODY_CAP_DEFAULT, OTA_MAX_IDLE_STALLS, OtaError, OtaUpload, RecvChunk, ota_body_cap,
    validate_ota_upload,
};
// Security lives in `rid_core::security` (complete port of `rid_security.c`
// including `verify_signed_body`, Ed25519 + PEM/DER/PUBLIC_KEYV1); re-exported
// here so the application layer exposes it.
pub use rid_core::security::{b64_decode, bytes_to_hex, hex_to_bytes, verify_sha256, verify_signed_body};
pub use state::{FW_VERSION, State, standard_name, state_to_json};
pub use web::{LogRing, SigRate, json_escape, level_from_line};
pub use web_config::{
    CommandKind, CommandOutcome, ConfigWrite, command_kind, command_needs_auth, handle_command,
    normalize_command, signed_action_decision,
};
