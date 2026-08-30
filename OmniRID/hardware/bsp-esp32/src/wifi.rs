//! ESP-IDF WiFi glue: AP-mode init, 802.11 beacon/action-frame injection,
//! and the Wi-Fi side of the [`Transmitter`] trait.
//!
//! Port of `wifi.c` / `wifi_tx.c` from the C firmware.

use esp_idf_svc as _;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys::{self as sys};
use esp_idf_svc::wifi::{self as wifi_svc, AuthMethod, Configuration, EspWifi, Protocol};
use rid_interface::{Config, GpsData, Identity};
use rid_app::config::cstr;

/// Global WiFi state.
struct WifiState {
    mac: [u8; 6],
    ssid: [u8; 33],
    ssid_len: u8,
    channel: u8,
    message_counter: u8,
    initialized: bool,
}

static mut WIFI_STATE: WifiState = WifiState {
    mac: [0; 6],
    ssid: [0; 33],
    ssid_len: 7,
    channel: 6,
    message_counter: 0,
    initialized: false,
};

fn state() -> &'static mut WifiState {
    unsafe { &mut WIFI_STATE }
}

/// Initialise WiFi in AP mode.  Port of `wifi_tx_init()`.
pub fn init(cfg: &rid_app::config::BspConfig) {
    if state().initialized {
        return;
    }

    let _ = sys::esp!(unsafe { sys::esp_event_loop_create_default() });

    let peripherals = Peripherals::take().expect("peripherals");
    let sysloop = esp_idf_svc::eventloop::EspSystemEventLoop::take()
        .expect("failed to init sysloop");
    let nvs = EspDefaultNvsPartition::take().expect("failed to init NVS partition");

    let mut wifi = EspWifi::new(peripherals.modem, sysloop, Some(nvs)).expect("wifi");

    // Read MAC.
    let mut mac = [0u8; 6];
    unsafe { sys::esp_wifi_get_mac(sys::wifi_interface_t_WIFI_IF_AP, mac.as_mut_ptr()) };
    state().mac = mac;

    let ssid = cstr(&cfg.wifi_ssid);
    let ssid_bytes = ssid.as_bytes();
    let sl = ssid_bytes.len().min(32);
    state().ssid[..sl].copy_from_slice(&ssid_bytes[..sl]);
    state().ssid_len = sl as u8;
    state().channel = cfg.wifi_channel;

    let pass = cstr(&cfg.wifi_password);
    let method = if pass.is_empty() {
        AuthMethod::None
    } else {
        AuthMethod::WPA2Personal
    };

    let ap_config = Configuration::AccessPoint(wifi_svc::AccessPointConfiguration {
        ssid: ssid.try_into().unwrap(),
        password: pass.try_into().unwrap(),
        channel: cfg.wifi_channel as u8,
        ssid_hidden: false,
        protocols: Protocol::P802D11B.into(),
        auth_method: method,
        max_connections: 4,
        ..Default::default()
    });

    wifi.set_configuration(&ap_config).expect("wifi cfg");
    wifi.start().expect("wifi start");

    // Set TX power (ESP-IDF uses 0.25 dBm units). The binding takes i8.
    let power_quarter_dbm = (cfg.wifi_power_dbm * 4.0) as i8;
    unsafe { sys::esp_wifi_set_max_tx_power(power_quarter_dbm) };

    state().initialized = true;
}

/// Get the MAC address.
pub fn mac() -> [u8; 6] {
    state().mac
}

/// Transmit a raw 802.11 frame via `esp_wifi_80211_tx`.
/// Port of the 4-attempt TX fallback from `wifi_tx_transmit`.
fn tx_raw(buf: &[u8]) -> bool {
    unsafe {
        let ifaces = [
            sys::wifi_interface_t_WIFI_IF_AP,
            sys::wifi_interface_t_WIFI_IF_STA,
            sys::wifi_interface_t_WIFI_IF_AP,
            sys::wifi_interface_t_WIFI_IF_STA,
        ];
        let seqs = [false, false, true, true];
        for i in 0..4 {
            if sys::esp_wifi_80211_tx(
                ifaces[i],
                buf.as_ptr() as *const _,
                buf.len() as _,
                seqs[i] as _,
            ) == sys::ESP_OK
            {
                return true;
            }
        }
    }
    false
}

/// Transmit a WiFi beacon frame containing an ODID message pack.
/// Called by `Transmitter::wifi_bcn`.
pub fn transmit_wifi_beacon(gps: &GpsData, identity: &Identity, config: &Config) {
    if !state().initialized {
        return;
    }

    // Build UAS data via the out-astm crate (region gating + ODID encoding).
    let outcome = out_astm::build_uas(gps, identity, config, None);

    // Build the IEEE 802.11 beacon frame.
    let counter = state().message_counter;
    state().message_counter = state().message_counter.wrapping_add(1);
    let ssid = &state().ssid[..state().ssid_len as usize];

    let mut buf = [0u8; 1024];
    if let Ok(len) = out_astm::wifi::build_beacon_frame(
        &outcome.uas,
        &state().mac,
        ssid,
        100, // beacon interval TU
        counter,
        tick_us(),
        &mut buf,
    ) {
        tx_raw(&buf[..len]);
    }
}

/// Transmit a NAN action frame containing an ODID message pack.
/// Called by `Transmitter::wifi_nan`.
pub fn transmit_wifi_nan(gps: &GpsData, identity: &Identity, config: &Config, counter: u8) {
    if !state().initialized {
        return;
    }

    let outcome = out_astm::build_uas(gps, identity, config, None);

    let mut buf = [0u8; 1024];
    if let Ok(len) = out_astm::wifi::build_nan_action_frame(
        &outcome.uas,
        &state().mac,
        counter,
        &mut buf,
    ) {
        tx_raw(&buf[..len]);
    }
}

/// Monotonic microsecond clock (port of `esp_timer_get_time()`).
fn tick_us() -> u64 {
    unsafe { sys::esp_timer_get_time() as u64 }
}

/// Shut down WiFi.
pub fn deinit() {
    // Best-effort: no-op if not initialised.
}
