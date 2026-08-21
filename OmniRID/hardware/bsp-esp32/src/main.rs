//! ESP32 Remote ID firmware entry point.
//!
//! Port of `main.c` from the C firmware: NVS init, MAC fixup, config load,
//! WiFi/BLE/LED init, the scheduler loop, and (optionally) OTA mode.

#![cfg_attr(feature = "hardware", no_std)]
#![cfg_attr(feature = "hardware", no_main)]

#[cfg(feature = "hardware")]
extern crate alloc;

#[cfg(feature = "hardware")]
mod imp {
    use alloc::sync::Arc;
    use bsp_esp32::SharedState;

    /// Splash screen port of `print_splash()` from `main.c`.
    fn print_splash(mac: &[u8; 6]) {
        esp_println::println!();
        esp_println::println!("  OmniRID -- Open DroneID Transmitter");
        esp_println::println!("  WiFi AP    | ESP-RID");
        esp_println::println!("  Config URL | http://192.168.4.1");
        esp_println::println!(
            "  MAC AP     | {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        );
        esp_println::println!();
    }

    /// Fix MAC when eFuse is corrupted (common on ESP32-S0WD).
    /// Port of `fix_mac_if_needed()` from `main.c`.
    fn fix_mac() {
        unsafe {
            let mut mac = [0u8; 6];
            if esp_idf_sys::esp_efuse_mac_get_default(mac.as_mut_ptr()) != esp_idf_sys::ESP_OK
                || (mac[0] == 0 && mac[1] == 0 && mac[2] == 0)
            {
                esp_println::println!("[MAIN] eFuse MAC CRC error — using fallback MAC");
                let fallback = [0x24u8, 0x0A, 0xC4, 0x12, 0x34, 0x56];
                esp_idf_sys::esp_base_mac_addr_set(fallback.as_ptr());
            }
        }
    }

    /// Get the base MAC address for the device.
    fn get_mac() -> [u8; 6] {
        let mut mac = [0u8; 6];
        unsafe {
            esp_idf_sys::esp_wifi_get_mac(
                esp_idf_sys::wifi_interface_t_WIFI_IF_AP,
                mac.as_mut_ptr(),
            );
        }
        mac
    }

    /// Monotonic millisecond clock.
    fn tick_ms() -> u32 {
        unsafe { (esp_idf_sys::xTaskGetTickCount() * esp_idf_sys::portTICK_PERIOD_MS) as u32 }
    }

    /// Monotonic microsecond clock.
    fn tick_us() -> u64 {
        unsafe { esp_idf_sys::esp_timer_get_time() as u64 }
    }

    pub fn run() {
        // 1. Init NVS (must be first).
        bsp_esp32::nvs::init();

        // 2. Fix MAC if eFuse is corrupted.
        fix_mac();

        // 3. Load saved configuration.
        let state = SharedState::new();
        {
            let mut lock = state.ctl.lock();
            bsp_esp32::nvs_load(&mut lock.bsp_config);
            lock.derive_default_ids(&get_mac());
        }

        // 4. Check OTA trigger GPIO.
        {
            let lock = state.ctl.lock();
            let gpio = lock.bsp_config.ota_trigger_gpio;
            drop(lock);
            bsp_esp32::ota::check_and_run(gpio);
        }

        // 5. Init peripherals.
        {
            let lock = state.ctl.lock();
            let cfg = lock.bsp_config.clone();
            drop(lock);
            bsp_esp32::wifi::init(&cfg);
            bsp_esp32::ble::init();
            bsp_esp32::led::init(cfg.led_r_gpio, cfg.led_g_gpio, cfg.led_b_gpio);
            if cfg.mavlink_usb_enable {
                bsp_esp32::usb::init();
            }
        }

        // 6. Print splash.
        let mac = bsp_esp32::wifi::mac();
        print_splash(&mac);

        // 7. Start web server (if enabled).
        let _web_server = {
            let lock = state.ctl.lock();
            if lock.bsp_config.webserver_en != 0 {
                drop(lock);
                match bsp_esp32::web::start(&state) {
                    Ok(srv) => Some(srv),
                    Err(e) => {
                        esp_println::println!("[MAIN] Web server failed: {:?}", e);
                        None
                    }
                }
            } else {
                None
            }
        };

        // 8. Spawn the scheduler loop on Core 1 (Application Core).
        //
        // The scheduler loop handles GPS polling, output encoding, and
        // transmission scheduling.  Pinning it to Core 1 ensures that
        // WiFi beacon/NAN TX (which ESP-IDF routes to Core 0) and BLE
        // radio callbacks never block on scheduler work.
        {
            let state_clone = Arc::clone(&state);
            bsp_esp32::core::spawn_scheduler(move || {
                let mut tx = bsp_esp32::EspTx;
                let mut last_led_update_ms: u32 = 0;

                loop {
                    let now_ms = tick_ms();
                    let now_us = tick_us();

                    let input = rid_interface::InputSample::new(now_ms, now_us);

                    {
                        let mut lock = state_clone.ctl.lock();
                        let outcome = lock.ctl.step(&input, &mut tx);

                        if now_ms.wrapping_sub(last_led_update_ms) >= 200 {
                            last_led_update_ms = now_ms;
                            let color = match outcome.led {
                                rid_core::scheduler::LedState::GpsOk => [0u8, 255, 0],
                                rid_core::scheduler::LedState::NoGps => [255, 128, 0],
                                rid_core::scheduler::LedState::Locked => [255, 0, 0],
                                rid_core::scheduler::LedState::Demo => [0, 128, 255],
                            };
                            let max = 8191u32;
                            let r = (color[0] as u32 * max) / 255;
                            let g = (color[1] as u32 * max) / 255;
                            let b = (color[2] as u32 * max) / 255;
                            bsp_esp32::led::set_rgb(r, g, b);
                        }
                    }

                    unsafe { esp_idf_sys::vTaskDelay(1); }
                }
            });
        }

        // 9. app_main keeps running on Core 1 (lower priority).
        //    If the scheduler task is the only high-priority work,
        //    app_main idles and yields to FreeRTOS.
        loop {
            unsafe { esp_idf_sys::vTaskDelay(100); }
        }
    }
}

#[cfg(feature = "hardware")]
#[no_mangle]
pub extern "C" fn app_main() {
    imp::run();
}

#[cfg(not(feature = "hardware"))]
fn main() {}
