//! Firmware configuration, port of `rid_config_t` from `esp_remote_id.h` and
//! of `default_config()` from `esp_remote_id.c`.
//!
//! This is the full BSP-side configuration. The hub-facing subset (`Config`
//! in `rid-interface`) is derived from it; the transport/pin/lighting fields
//! live only here.

use rid_interface::{
    fixed_str, FixedKeyStr, FixedStr, Protocol, Region, MAX_KEY_LEN, MAX_STR_LEN, NUM_KEYS,
    TRANSMIT_WIFI_BCN,
};

/// `auth_private_key[512]` from the C header.
pub const AUTH_KEY_MAX_LEN: usize = 512;
/// Number of external lighting outputs.
pub const NUM_LIGHTING_PINS: usize = 5;

/// Full BSP configuration, mirror of `rid_config_t`.
#[derive(Clone, PartialEq, Debug)]
pub struct BspConfig {
    pub protocol: Protocol,
    pub uart_port: u8,
    pub baud_rate: u32,
    pub tx_pin: u8,
    pub rx_pin: u8,

    pub region: Region,

    pub ua_type: u8,
    pub id_type: u8,
    pub uas_id: FixedStr,
    pub operator_id: FixedStr,

    pub ua_type_2: u8,
    pub id_type_2: u8,
    pub uas_id_2: FixedStr,

    pub tx_modes: u8,
    pub wifi_channel: u8,
    pub wifi_power_dbm: f32,
    pub wifi_bcn_rate_hz: f32,
    pub wifi_nan_rate_hz: f32,
    pub ble4_rate_hz: f32,
    pub ble4_power_dbm: f32,
    pub ble5_rate_hz: f32,
    pub ble5_power_dbm: f32,

    pub wifi_ssid: FixedStr,
    pub wifi_password: FixedStr,
    pub webserver_en: u8,

    pub mavlink_sysid: u8,
    pub bcast_powerup: u8,

    pub operator_lat: f64,
    pub operator_lon: f64,
    pub operator_alt: f32,
    pub self_id_text: FixedStr,

    pub options: u16,
    pub lock_level: i8,

    pub led_r_gpio: i8,
    pub led_g_gpio: i8,
    pub led_b_gpio: i8,

    /// WS2812 addressable RGB LED.
    pub ws2812_gpio: i8,
    pub ws2812_brightness: u8,

    /// External GPIO lighting outputs.
    pub lighting_pins: [i8; NUM_LIGHTING_PINS],
    pub lighting_patterns: [u8; NUM_LIGHTING_PINS],
    pub lighting_phase_offsets: [i16; NUM_LIGHTING_PINS],

    /// DroneCAN.
    pub dronecan_rx_gpio: i8,
    pub dronecan_tx_gpio: i8,
    pub dronecan_bitrate: u32,

    /// MAVLink USB transport.
    pub mavlink_usb_enable: bool,

    /// OTA trigger GPIO (-1 = disabled).
    pub ota_trigger_gpio: i8,

    /// Authentication private key PEM.
    pub auth_private_key: [u8; AUTH_KEY_MAX_LEN],

    /// Startup delay before transmission (ms).
    pub start_delay_ms: u32,

    pub public_keys: [FixedKeyStr; NUM_KEYS],
}

impl Default for BspConfig {
    /// Port of `default_config()`: the struct is zeroed first, then the
    /// documented defaults are applied.
    fn default() -> Self {
        Self {
            protocol: Protocol::Auto,
            uart_port: 1,
            baud_rate: 57600,
            tx_pin: 17,
            rx_pin: 18,

            region: Region::Auto,

            ua_type: 1,
            id_type: 1,
            uas_id: fixed_str("ESP32-RID-001"),
            operator_id: fixed_str("OP-UNKNOWN"),

            ua_type_2: 0,
            id_type_2: 0,
            uas_id_2: [0; MAX_STR_LEN + 1],

            tx_modes: TRANSMIT_WIFI_BCN,
            wifi_channel: 6,
            wifi_power_dbm: 20.0,
            wifi_bcn_rate_hz: 1.0,
            wifi_nan_rate_hz: 0.0,
            ble4_rate_hz: 1.0,
            ble4_power_dbm: 18.0,
            ble5_rate_hz: 1.0,
            ble5_power_dbm: 18.0,

            wifi_ssid: fixed_str("ESP-RID"),
            wifi_password: [0; MAX_STR_LEN + 1],
            webserver_en: 1,

            mavlink_sysid: 0,
            bcast_powerup: 1,

            operator_lat: 0.0,
            operator_lon: 0.0,
            operator_alt: 0.0,
            self_id_text: [0; MAX_STR_LEN + 1],

            options: 0,
            lock_level: 0,

            led_r_gpio: -1,
            led_g_gpio: -1,
            led_b_gpio: -1,

            ws2812_gpio: -1,
            ws2812_brightness: 16,

            lighting_pins: [-1; NUM_LIGHTING_PINS],
            lighting_patterns: [0; NUM_LIGHTING_PINS],
            lighting_phase_offsets: [0; NUM_LIGHTING_PINS],

            dronecan_rx_gpio: -1,
            dronecan_tx_gpio: -1,
            dronecan_bitrate: 1000000,

            mavlink_usb_enable: false,

            ota_trigger_gpio: -1,

            auth_private_key: [0; AUTH_KEY_MAX_LEN],

            start_delay_ms: 10000,

            public_keys: [[0; MAX_KEY_LEN + 1]; NUM_KEYS],
        }
    }
}

/// Reads a NUL-terminated C string buffer as `&str` (empty when malformed).
pub fn cstr(buf: &[u8]) -> &str {
    let n = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    core::str::from_utf8(&buf[..n]).unwrap_or("")
}

/// Clamps a raw region value like the C `nvs_storage_load`:
/// values above `RID_REGION_NZL` resolve to `RID_REGION_AUTO`.
pub fn clamp_region(r: u8) -> Region {
    if r <= Region::Nzl as u8 {
        Region::from_raw(r).unwrap_or(Region::Auto)
    } else {
        Region::Auto
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_c() {
        let c = BspConfig::default();
        assert_eq!(c.protocol, Protocol::Auto);
        assert_eq!(c.uart_port, 1);
        assert_eq!(c.baud_rate, 57600);
        assert_eq!(c.tx_pin, 17);
        assert_eq!(c.rx_pin, 18);
        assert_eq!(c.region, Region::Auto);
        assert_eq!(c.ua_type, 1);
        assert_eq!(c.id_type, 1);
        assert_eq!(cstr(&c.uas_id), "ESP32-RID-001");
        assert_eq!(cstr(&c.operator_id), "OP-UNKNOWN");
        assert_eq!(c.tx_modes, TRANSMIT_WIFI_BCN);
        assert_eq!(c.wifi_channel, 6);
        assert_eq!(c.wifi_power_dbm, 20.0);
        assert_eq!(c.wifi_bcn_rate_hz, 1.0);
        assert_eq!(c.wifi_nan_rate_hz, 0.0);
        assert_eq!(c.ble4_rate_hz, 1.0);
        assert_eq!(c.ble4_power_dbm, 18.0);
        assert_eq!(c.ble5_rate_hz, 1.0);
        assert_eq!(c.ble5_power_dbm, 18.0);
        assert_eq!(cstr(&c.wifi_ssid), "ESP-RID");
        assert_eq!(c.webserver_en, 1);
        assert_eq!(c.bcast_powerup, 1);
        assert_eq!(c.ws2812_brightness, 16);
        assert_eq!(c.lighting_pins, [-1; 5]);
        assert_eq!(c.dronecan_bitrate, 1000000);
        assert_eq!(c.start_delay_ms, 10000);
        for p in &c.public_keys {
            assert_eq!(*p, [0; MAX_KEY_LEN + 1]);
        }
    }

    #[test]
    fn clamp_region_matches_c() {
        assert_eq!(clamp_region(0), Region::Auto);
        assert_eq!(clamp_region(5), Region::Kor);
        assert_eq!(clamp_region(10), Region::Nzl);
        // Anything above NZL -> AUTO (the C ternary).
        assert_eq!(clamp_region(11), Region::Auto);
        assert_eq!(clamp_region(255), Region::Auto);
    }
}
