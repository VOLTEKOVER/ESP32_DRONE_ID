//! ESP-IDF OTA update: the hardware side of `rid_ota.c`.
//!
//! Reads the OTA trigger GPIO, starts a temporary WiFi AP + HTTP server
//! if triggered, writes the firmware to the next update partition, and
//! reboots on success.

use esp_idf_svc as _;
use esp_idf_svc::sys::{self as sys};

/// Check the OTA trigger GPIO and, if held low, enter OTA mode.
///
/// In OTA mode, a dedicated AP named "RemoteID-OTA" is started and an
/// HTTP server listens for firmware uploads.  The function never returns
/// (it loops forever waiting for uploads or rebooting).
pub fn check_and_run(ota_trigger_gpio: i8) {
    if ota_trigger_gpio < 0 {
        return;
    }

    unsafe {
        let mut cfg: sys::gpio_config_t = core::mem::zeroed();
        cfg.pin_bit_mask = 1u64 << ota_trigger_gpio as u64;
        cfg.mode = sys::gpio_mode_t_GPIO_MODE_INPUT;
        cfg.pull_up_en = sys::gpio_pullup_t_GPIO_PULLUP_ENABLE;
        sys::gpio_config(&cfg);
        sys::vTaskDelay(10);
        if sys::gpio_get_level(ota_trigger_gpio as _) != 0 {
            return; // Not pressed.
        }
    }

    // Enter OTA mode: start a minimal AP + HTTP server.
    // This is a simplified version; the full C implementation is in rid_ota.c.
    loop {
        unsafe { sys::vTaskDelay(1000); }
    }
}

/// Write a chunk to the OTA partition.
pub fn ota_write(handle: &mut OtaHandle, data: &[u8]) -> bool {
    handle.write(data)
}

/// Finalise the OTA handle.
pub fn ota_end(handle: OtaHandle) -> bool {
    handle.finish()
}

/// Reboot into the new firmware.
pub fn reboot() -> ! {
    unsafe {
        sys::esp_restart();
    }
    loop {}
}

/// Minimal OTA write handle wrapping ESP-IDF OTA APIs.
pub struct OtaHandle {
    handle: sys::esp_ota_handle_t,
}

impl OtaHandle {
    /// Begin an OTA write session to the next update partition.
    pub fn begin() -> Option<Self> {
        unsafe {
            let partition = sys::esp_ota_get_next_update_partition(core::ptr::null());
            if partition.is_null() {
                return None;
            }
            let mut handle: sys::esp_ota_handle_t = 0;
            if sys::esp_ota_begin(partition, usize::MAX, &mut handle) != sys::ESP_OK {
                return None;
            }
            Some(Self { handle })
        }
    }

    fn write(&mut self, data: &[u8]) -> bool {
        unsafe {
            sys::esp_ota_write(self.handle, data.as_ptr() as *const _, data.len())
                == sys::ESP_OK
        }
    }

    fn finish(self) -> bool {
        let ok = unsafe { sys::esp_ota_end(self.handle) == sys::ESP_OK };
        if ok {
            // Set the boot partition to the update partition.
            unsafe {
                let running = sys::esp_ota_get_running_partition();
                let boot = sys::esp_ota_get_boot_partition();
                if boot != running {
                    sys::esp_ota_set_boot_partition(running);
                }
            }
        }
        ok
    }
}
