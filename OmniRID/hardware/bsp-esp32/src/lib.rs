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
pub mod core;

// ESP-IDF glue modules — only compiled when building for real hardware.
#[cfg(feature = "hardware")]
pub mod ble;
#[cfg(feature = "hardware")]
pub mod led;
#[cfg(feature = "hardware")]
pub mod nvs;
#[cfg(feature = "hardware")]
pub mod ota;
#[cfg(feature = "hardware")]
pub mod usb;
#[cfg(feature = "hardware")]
pub mod web;
#[cfg(feature = "hardware")]
pub mod wifi;

pub use rid_app::*;

// Re-export the controller from the `app` crate so `bsp-esp32` users get it
// without depending on `app` directly.
pub use app::controller::Controller;
pub use app::capabilities;

// ---------------------------------------------------------------------------
// ESP-IDF glue: shared state and Transmitter implementation
// ---------------------------------------------------------------------------

#[cfg(feature = "hardware")]
use alloc::sync::Arc;
#[cfg(feature = "hardware")]
use spin::Mutex;

#[cfg(feature = "hardware")]
use rid_interface::{Config, GpsData, Identity, Transmitter};

/// Shared application state, accessible from both the HTTP server and the
/// main loop.
#[cfg(feature = "hardware")]
pub struct SharedState {
    pub ctl: Mutex<Controller>,
    pub log_ring: Mutex<web::LogRing>,
    pub sig_rate: Mutex<web::SigRate>,
}

#[cfg(feature = "hardware")]
impl SharedState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            ctl: Mutex::new(Controller::new()),
            log_ring: Mutex::new(web::LogRing::new()),
            sig_rate: Mutex::new(web::SigRate::new()),
        })
    }
}

/// Save configuration to NVS (hardware side of `nvs_storage_save`).
#[cfg(feature = "hardware")]
pub fn nvs_save(cfg: &rid_app::config::BspConfig) {
    let mut store = nvs::EspNvsStorage::new();
    rid_app::nvs::save(cfg, &mut store);
}

/// Erase all NVS data.
#[cfg(feature = "hardware")]
pub fn nvs_erase() {
    let mut store = nvs::EspNvsStorage::new();
    rid_app::nvs::erase(&mut store);
}

/// Load configuration from NVS.
#[cfg(feature = "hardware")]
pub fn nvs_load(cfg: &mut rid_app::config::BspConfig) {
    let mut store = nvs::EspNvsStorage::new();
    rid_app::nvs::load(cfg, &mut store);
}

/// Hardware `Transmitter` implementation that delegates to WiFi and BLE.
#[cfg(feature = "hardware")]
pub struct EspTx;

#[cfg(feature = "hardware")]
impl Transmitter for EspTx {
    fn wifi_bcn(&mut self, gps: &GpsData, identity: &Identity, config: &Config) {
        wifi::transmit_wifi_beacon(gps, identity, config);
    }

    fn wifi_nan(&mut self, gps: &GpsData, identity: &Identity, config: &Config, counter: u8) {
        wifi::transmit_wifi_nan(gps, identity, config, counter);
    }

    fn ble4(&mut self, gps: &GpsData, identity: &Identity, config: &Config) {
        let outcome = out_astm::build_uas(gps, identity, config, None);
        let mut rotation: u8 = 0;
        if let Some(msg) = out_astm::ble4::next_message(&outcome.uas, &mut rotation) {
            ble::transmit_legacy(&msg.0, rotation);
        }
    }

    fn ble5(&mut self, gps: &GpsData, identity: &Identity, config: &Config) {
        let outcome = out_astm::build_uas(gps, identity, config, None);
        let mut pack_buf = [0u8; 1024];
        if let Ok(pack_len) = out_astm::pack::build_pack(&outcome.uas, &mut pack_buf) {
            ble::transmit_extended(&pack_buf[..pack_len]);
        }
    }
}
