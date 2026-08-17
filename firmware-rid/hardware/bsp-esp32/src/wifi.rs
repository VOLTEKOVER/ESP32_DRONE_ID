//! ESP-IDF WiFi glue: beacon and NAN injection.
//!
//! Port of `wifi.c` / `wifi_tx.c` from the C firmware. Uses `esp-idf-svc` WiFi
//! for beacon frame injection and ESP-NOW / NAN for multicast NAN transport.
//!
//! This module is only compiled when the `hardware` feature is active.

use esp_idf_svc as _;

/// Initialize the WiFi driver in STA+AP mode for beacon/NAN injection.
pub fn init() -> Result<(), esp_idf_svc::sys::EspError> {
    // TODO: esp_netif_init, esp_event_loop_create_default, esp_wifi_init,
    // esp_wifi_set_mode(WIFI_MODE_APSTA), esp_wifi_start
    Ok(())
}

/// Transmit a raw 802.11 beacon frame (port of `wifi_tx_beacon`).
pub fn transmit_beacon(_frame: &[u8]) -> Result<(), esp_idf_svc::sys::EspError> {
    // TODO: esp_wifi_80211_tx
    Ok(())
}

/// Shut down WiFi.
pub fn deinit() -> Result<(), esp_idf_svc::sys::EspError> {
    // TODO: esp_wifi_stop, esp_netif_deinit
    Ok(())
}
