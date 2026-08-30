//! ESP-IDF USB CDC ACM transport for MAVLink.
//!
//! Port of `rid_mavlink_usb.c`: provides a byte sink (`UsbCdcTx`) backed by
//! the ESP-IDF USB Serial/JTAG driver.  Also provides a byte source (`UsbCdcRx`)
//! for incoming MAVLink data from the flight controller.
//!
//! The USB Serial/JTAG peripheral only exists on ESP32-S3/C6 (not classic
//! ESP32), so the whole module is only compiled for those targets.

#![cfg(any(feature = "esp32s3", feature = "esp32c6"))]

use esp_idf_svc as _;
use esp_idf_svc::sys::{self as sys};
use rid_interface::{UartRead, UartWrite};

/// USB CDC ACM byte sink (TX to flight controller).
pub struct UsbCdcTx;

/// USB CDC ACM byte source (RX from flight controller).
pub struct UsbCdcRx;

impl UartWrite for UsbCdcTx {
    fn write(&mut self, buf: &[u8]) -> usize {
        if buf.is_empty() {
            return 0;
        }
        unsafe {
            let n = sys::usb_serial_jtag_write_bytes(
                buf.as_ptr() as *const _,
                buf.len(),
                100, // ticks timeout
            );
            if n > 0 { n as usize } else { 0 }
        }
    }
}

impl UartRead for UsbCdcRx {
    fn read(&mut self, buf: &mut [u8]) -> usize {
        if buf.is_empty() {
            return 0;
        }
        unsafe {
            let n = sys::usb_serial_jtag_read_bytes(
                buf.as_mut_ptr() as *mut _,
                buf.len(),
                0, // non-blocking
            );
            if n > 0 { n as usize } else { 0 }
        }
    }
}

/// Initialise the USB CDC ACM peripheral.
pub fn init() {
    unsafe {
        let mut cfg: sys::usb_serial_jtag_driver_config_t = core::mem::zeroed();
        cfg.tx_buffer_size = 256;
        cfg.rx_buffer_size = 256;
        sys::usb_serial_jtag_driver_install(&mut cfg);
    }
}
