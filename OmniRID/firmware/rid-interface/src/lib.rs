//! Neutral contracts for the modular Remote ID firmware.
//!
//! This crate defines the "common language" between the central processing
//! (`rid-core`), the input protocols, the output standards and the hardware
//! BSP. It mirrors the types of the legacy C firmware
//! (`ESP32_DRONE_REMOTE_ID_Firmware`) so the ported logic stays 100%
//! equivalent. No hardware dependencies; usable in `no_std`.
#![no_std]

pub mod input;
pub mod odid;
pub mod region;
pub mod types;

pub use input::{
    CanFrame, CanRead, GpsSource, InputSample, OperatorLocation, Transmitter, UartRead, UartWrite,
};
pub use odid::{
    AuthPack, AuthPage, AUTH_PAGE_NONZERO_DATA_SIZE, AUTH_PAGE_ZERO_DATA_SIZE,
    AUTH_UAS_ID_SIGNATURE, MAX_AUTH_LENGTH,
};
pub use region::{Region, RegionRules, Standard};
pub use types::{
    fixed_str, key_str, CStr, Config, FixedKeyStr, FixedStr, GpsData, Identity, Protocol, State,
    Stats, UasBuild, AUTH_MAX_PAGES, INV_ALT, INV_DIR, INV_SPEED_H, INV_SPEED_V, MAX_KEY_LEN,
    MAX_STR_LEN, MAX_TIMESTAMP, MESSAGE_SIZE, NUM_KEYS, OPT_AUTH_ED25519, OPT_DEMO_MODE,
    OPT_DONT_SAVE_BASIC_ID, OPT_FORCE_ARM_OK, OPT_IDENTITY_READY_GATE, OPT_KALMAN_FILTER,
    OPT_MAVLINK_ARM_STATUS, OPT_MAVLINK_OP_LOC_LOOP, OPT_PRINT_RID_MAVLINK, TRANSMIT_BLE4,
    TRANSMIT_BLE5, TRANSMIT_WIFI_BCN, TRANSMIT_WIFI_NAN,
};
