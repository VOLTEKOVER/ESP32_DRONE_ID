//! ESP32 Remote ID firmware entry point.
//!
//! The `app_main` function is the ESP-IDF entry point (equivalent to `main`
//! on a regular OS). It initializes the hardware and starts the scheduler
//! loop.

// Only compiled when building for real ESP32 hardware.
#![cfg_attr(not(feature = "hardware"), no_std)]
#![cfg_attr(feature = "hardware", feature(alloc_error_handler))]

#[cfg(feature = "hardware")]
fn main() {
    // ESP-IDF link patches (required by esp-idf-sys).
    esp_idf_sys::link_patches();

    println!("Remote ID firmware starting (ESP-IDF)...");

    // TODO: init NVS, load BspConfig, init WiFi/BLE, start web server,
    // run Controller::new() + loop.

    bsp_esp32::caps::Capabilities::current();
}
