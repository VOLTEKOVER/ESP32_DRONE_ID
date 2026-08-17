//! Per-chip capability matrix, grounded in the ESP-IDF firmware configuration
//! (`sdkconfig`: `CONFIG_BT_BLE_50_EXTEND_ADV_EN` on esp32s3/esp32c6, native
//! USB-Serial-JTAG on esp32c6, USB OTG on esp32s3).

use rid_interface::region::Standard;

/// Compile-time capabilities of the selected chip. Choosing a chip is a
/// feature flag: exactly one of `esp32`, `esp32s3`, `esp32c6` must be active.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Capabilities {
    /// 2.4 GHz Wi-Fi (802.11 beacon / NAN injection).
    pub wifi: bool,
    /// BLE 4.x legacy advertising.
    pub ble: bool,
    /// BLE 5 extended advertising (esp32s3/esp32c6 only).
    pub ble5: bool,
    /// Native USB.
    pub usb: bool,
    /// NVS key/value storage.
    pub nvs: bool,
    /// Status LED.
    pub led: bool,
    /// HTTP config web page.
    pub web: bool,
    /// OTA update over Wi-Fi.
    pub ota: bool,
    /// Broadcast standards the radio stack can transmit.
    pub standards: &'static [Standard],
}

impl Capabilities {
    /// Capabilities of the chip selected via the `esp32`/`esp32s3`/`esp32c6`
    /// features.
    pub fn current() -> Self {
        #[cfg(feature = "esp32")]
        return Self {
            wifi: true,
            ble: true,
            ble5: false,
            usb: false,
            nvs: true,
            led: true,
            web: true,
            ota: true,
            standards: &[Standard::Astm],
        };

        #[cfg(feature = "esp32s3")]
        return Self {
            wifi: true,
            ble: true,
            ble5: true,
            usb: true,
            nvs: true,
            led: true,
            web: true,
            ota: true,
            standards: &[Standard::Astm],
        };

        #[cfg(feature = "esp32c6")]
        return Self {
            wifi: true,
            ble: true,
            ble5: true,
            usb: true,
            nvs: true,
            led: true,
            web: true,
            ota: true,
            standards: &[Standard::Astm, Standard::ChnGb, Standard::Frdid],
        };

        #[cfg(not(any(feature = "esp32", feature = "esp32s3", feature = "esp32c6")))]
        {
            compile_error!("bsp-esp32: select exactly one chip feature: esp32, esp32s3 or esp32c6");
        }
    }

    /// Whether the radio stack can broadcast `standard`.
    pub fn supports(&self, standard: Standard) -> bool {
        self.standards.contains(&standard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_chip_has_wifi_and_ble() {
        let c = Capabilities::current();
        assert!(c.wifi);
        assert!(c.ble);
    }

    #[test]
    fn esp32c6_supports_all_three_standards() {
        let c = Capabilities::current();
        assert!(c.supports(Standard::Astm));
        assert!(c.supports(Standard::ChnGb));
        assert!(c.supports(Standard::Frdid));
    }
}
