//! ESP32 board glue: the thin adapter between `rid-app` (the
//! board-agnostic application layer) and the ESP-IDF hardware (WiFi/BLE
//! injection, real NVS, LED, USB, web server, OTA).
//!
//! All pure application logic lives in `rid-app`; this crate only contains the
//! per-chip pieces: the compile-time capability matrix (`caps`) and, when the
//! `hardware` feature is active, the ESP-IDF glue modules. Hardware code is
//! gated behind `#[cfg(feature = "hardware")]` and needs the ESP32 Rust
//! toolchain (espup); `caps` builds and tests on any host.
#![cfg_attr(not(feature = "hardware"), no_std)]

extern crate alloc;

pub mod caps;

// ESP-IDF glue modules — only compiled when building for real hardware.
#[cfg(feature = "hardware")]
pub mod wifi;

pub use rid_app::*;
