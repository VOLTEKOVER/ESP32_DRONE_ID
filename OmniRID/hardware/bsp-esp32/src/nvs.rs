//! ESP-IDF NVS implementation of the [`NvsStore`] trait.
//!
//! Port of `nvs_storage.c` hardware glue: wraps `esp_idf_svc::nvs` to provide
//! the abstract key/value store that `rid_app::nvs::{save,load,erase}` uses.

use esp_idf_svc as _;
use esp_idf_svc::nvs::{EspNvsPartition, NvsDefault};
use rid_app::nvs::NvsStore;

const NS: &str = "esp_rid";

/// ESP-IDF NVS implementation of `NvsStore`.
pub struct EspNvsStorage {
    partition: EspNvsPartition<NvsDefault>,
}

impl EspNvsStorage {
    /// Opens the `esp_rid` namespace on the default NVS partition.
    pub fn new() -> Self {
        let partition = EspNvsPartition::<NvsDefault>::new();
        Self { partition }
    }

    fn open(&self) -> Result<esp_idf_svc::nvs::EspNvs, esp_idf_svc::sys::EspError> {
        esp_idf_svc::nvs::EspNvs::new(self.partition.clone(), NS, true)
    }
}

impl Default for EspNvsStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl NvsStore for EspNvsStorage {
    fn get_str(&mut self, key: &str, out: &mut [u8]) -> bool {
        let Ok(h) = self.open() else {
            return false;
        };
        match h.get_str(key, out) {
            Ok(Some(s)) => {
                let n = s.len().min(out.len().saturating_sub(1));
                out[..n].copy_from_slice(s.as_bytes());
                out[n..].fill(0);
                true
            }
            _ => false,
        }
    }

    fn set_str(&mut self, key: &str, value: &str) {
        if let Ok(h) = self.open() {
            let _ = h.set_str(key, value);
            let _ = h.commit();
        }
    }

    fn get_u8(&mut self, key: &str) -> Option<u8> {
        let h = self.open().ok()?;
        h.get_u8(key).ok().flatten()
    }

    fn set_u8(&mut self, key: &str, value: u8) {
        if let Ok(h) = self.open() {
            let _ = h.set_u8(key, value);
            let _ = h.commit();
        }
    }

    fn get_i8(&mut self, key: &str) -> Option<i8> {
        let h = self.open().ok()?;
        h.get_i8(key).ok().flatten()
    }

    fn set_i8(&mut self, key: &str, value: i8) {
        if let Ok(h) = self.open() {
            let _ = h.set_i8(key, value);
            let _ = h.commit();
        }
    }

    fn get_u32(&mut self, key: &str) -> Option<u32> {
        let h = self.open().ok()?;
        h.get_u32(key).ok().flatten()
    }

    fn set_u32(&mut self, key: &str, value: u32) {
        if let Ok(h) = self.open() {
            let _ = h.set_u32(key, value);
            let _ = h.commit();
        }
    }

    fn get_f32(&mut self, key: &str) -> Option<f32> {
        let h = self.open().ok()?;
        let mut buf = [0u8; 4];
        h.get_blob(key, &mut buf).ok()?;
        Some(f32::from_le_bytes(buf))
    }

    fn set_f32(&mut self, key: &str, value: f32) {
        if let Ok(h) = self.open() {
            let _ = h.set_blob(key, &value.to_le_bytes());
            let _ = h.commit();
        }
    }

    fn get_f64(&mut self, key: &str) -> Option<f64> {
        let h = self.open().ok()?;
        let mut buf = [0u8; 8];
        h.get_blob(key, &mut buf).ok()?;
        Some(f64::from_le_bytes(buf))
    }

    fn set_f64(&mut self, key: &str, value: f64) {
        if let Ok(h) = self.open() {
            let _ = h.set_blob(key, &value.to_le_bytes());
            let _ = h.commit();
        }
    }

    fn erase_all(&mut self) {
        if let Ok(h) = self.open() {
            let _ = h.erase_all();
            let _ = h.commit();
        }
    }
}

/// Initialise the NVS flash partition.  Port of `nvs_storage_init()`.
pub fn init() {
    esp_idf_svc::sys::esp!(unsafe {
        esp_idf_sys::nvs_flash_init()
    })
    .or_else(|_| unsafe {
        esp_idf_sys::nvs_flash_erase();
        esp_idf_sys::nvs_flash_init();
        Ok(())
    })
    .expect("NVS init failed");
}
