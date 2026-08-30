//! ESP-IDF BLE glue: Bluedroid BLE controller init and advertising.
//!
//! Port of `ble_tx.c` from the C firmware.  Two transmission modes:
//! - Legacy advertising (BLE 4.x): 31-byte Service Data on UUID 0xFFFA
//! - Extended advertising (BLE 5.0): full ODID message pack on 1M + Coded PHY
//!
//! This module is only compiled when the `hardware` feature is active.

use esp_idf_svc as _;
use esp_idf_svc::sys::{self as sys};

/// RID Service UUID (ASTM F3411-22a §6.4.1.1).
const RID_SERVICE_UUID: u16 = 0xFFFA;

/// ASTM Open Drone ID application code for the Service Data header.
const ODID_APP_CODE: u8 = 0x0D;

/// Global BLE state.
struct BleState {
    initialized: bool,
}

static mut BLE_STATE: BleState = BleState { initialized: false };

/// Initialise the Bluedroid BLE stack.  Port of `ble_tx_init()`.
///
/// On ESP32-C6 (BLE 5 only) or targets without Bluedroid, this is a no-op
/// and the transmit functions return `false`.
pub fn init() {
    unsafe {
        if BLE_STATE.initialized {
            return;
        }

        // Release classic BT memory (we only need BLE).
        sys::esp_bt_controller_mem_release(sys::esp_bt_mode_t_ESP_BT_MODE_CLASSIC_BT);

        // The `BT_CONTROLLER_INIT_CONFIG_DEFAULT()` C macro is not bound by
        // bindgen, so build the config from zeroed memory.  A zero stack size
        // makes `esp_bt_controller_init` fall back to its own default.
        let mut cfg: sys::esp_bt_controller_config_t = core::mem::zeroed();
        if sys::esp_bt_controller_init(&mut cfg) != sys::ESP_OK {
            return;
        }
        if sys::esp_bt_controller_enable(sys::esp_bt_mode_t_ESP_BT_MODE_BLE) != sys::ESP_OK {
            return;
        }
        if sys::esp_bluedroid_init() != sys::ESP_OK {
            return;
        }
        if sys::esp_bluedroid_enable() != sys::ESP_OK {
            return;
        }

        BLE_STATE.initialized = true;
    }
}

/// Is BLE available on this target?
pub fn is_available() -> bool {
    unsafe { BLE_STATE.initialized }
}

/// Set the BLE TX power (clamped to -12..+9 dBm).
pub fn set_power(dbm: i8) {
    if !is_available() {
        return;
    }
    let clamped = dbm.clamp(-12, 9);
    // ESP-IDF encodes TX power levels as small integers; cast to the binding's
    // power-level type (name differs across IDF versions) via `as _`.
    let level = (clamped + 12) / 3;
    unsafe {
        sys::esp_ble_tx_power_set(
            sys::esp_ble_power_type_t_ESP_BLE_PWR_TYPE_DEFAULT as _,
            level as _,
        );
        sys::esp_ble_tx_power_set(sys::esp_ble_power_type_t_ESP_BLE_PWR_TYPE_ADV as _, level as _);
        sys::esp_ble_tx_power_set(
            sys::esp_ble_power_type_t_ESP_BLE_PWR_TYPE_SCAN as _,
            level as _,
        );
    }
}

// ---------------------------------------------------------------------------
// BLE 4.x Legacy advertising (port of `build_legacy_adv` + transmit)
// ---------------------------------------------------------------------------

/// Build a 31-byte legacy advertising PDU with a single 25-byte ODID message
/// in the Service Data AD structure on UUID 0xFFFA.
///
/// Returns `(adv_data, len)`.  `rotation` is the message counter.
fn build_legacy_adv(
    msg_data: &[u8], // 25-byte encoded ODID message
    rotation: u8,
) -> ([u8; 31], usize) {
    let mut buf = [0u8; 31];
    // Service Data AD structure: length=30, type=0x16 (Service Data -16bit)
    buf[0] = 0x1E;
    buf[1] = 0x16;
    // UUID 0xFFFA little-endian
    buf[2] = (RID_SERVICE_UUID & 0xFF) as u8;
    buf[3] = (RID_SERVICE_UUID >> 8) as u8;
    buf[4] = ODID_APP_CODE;
    buf[5] = rotation;
    let n = msg_data.len().min(25);
    buf[6..6 + n].copy_from_slice(&msg_data[..n]);
    (buf, 31)
}

/// Transmit BLE 4.x legacy advertising.  Port of `ble_tx_transmit_legacy`.
pub fn transmit_legacy(msg_data: &[u8], rotation: u8) -> bool {
    if !is_available() {
        return false;
    }

    let (adv_data, len) = build_legacy_adv(msg_data, rotation);

    unsafe {
        // Configure advertising data.
        sys::esp_ble_gap_config_adv_data_raw(
            adv_data.as_ptr() as *mut u8,
            len as _,
        );

        // Start legacy advertising.
        let mut params: sys::esp_ble_adv_params_t = core::mem::zeroed();
        params.adv_int_min = 0x100;
        params.adv_int_max = 0x100;
        params.adv_type = sys::esp_ble_adv_type_t_ADV_TYPE_SCAN_IND;
        params.own_addr_type = sys::esp_ble_addr_type_t_BLE_ADDR_TYPE_RANDOM;
        params.channel_map = sys::esp_ble_adv_channel_t_ADV_CHNL_ALL as _;
        params.adv_filter_policy = sys::esp_ble_adv_filter_t_ADV_FILTER_ALLOW_SCAN_ANY_CON_ANY;

        sys::esp_ble_gap_start_advertising(&mut params as *mut _);
    }

    true
}

/// Transmit BLE 5.0 extended advertising with the full ODID message pack.
/// Port of `ble_tx_transmit_lr`.
///
/// Extended advertising (`esp_ble_gap_ext_adv_*`) only exists on BLE-5 SoCs
/// (ESP32-S3/C6).  On classic ESP32 these bindings are absent, so this falls
/// back to legacy advertising with the first 25 bytes.
pub fn transmit_extended(pack: &[u8]) -> bool {
    if !is_available() {
        return false;
    }

    // Classic ESP32 has no extended advertising; fall back to legacy with the
    // first 25 bytes.  (The application already avoids `ble5` there via `caps`,
    // so this path is effectively unused.)
    #[cfg(not(any(feature = "esp32s3", feature = "esp32c6")))]
    {
        let _ = pack;
        return false;
    }

    // Extended advertising (`esp_ble_gap_ext_adv_*`) only exists on BLE-5 SoCs
    // (ESP32-S3/C6).  The body below is gated so it is never compiled on
    // classic ESP32, whose bindings lack the BLE-5 PHY enum constants.
    #[cfg(any(feature = "esp32s3", feature = "esp32c6"))]
    {
        unsafe {
            // Instance 0: 1M PHY (visible to BLE 4.2 scanners).
            let mut params: sys::esp_ble_gap_ext_adv_params_t = core::mem::zeroed();
            // Numeric literals for the BLE-5 enum constants; the bindgen names
            // are not always emitted on every IDF/sdkconfig combination, but
            // the values are stable across IDF 5.x:
            //   EXT_ADV_PROP_NONCONN_NONSCANNABLE_UNDIRECTED = 0x10
            //   esp_ble_gap_phy_t:  BLE_GAP_PHY_1M = 0x01, BLE_GAP_PHY_CODED = 0x04
            params.type_ = 0x10;
            params.interval_min = 0x100;
            params.interval_max = 0x100;
            params.channel_map = sys::esp_ble_adv_channel_t_ADV_CHNL_ALL as _;
            params.own_addr_type = sys::esp_ble_addr_type_t_BLE_ADDR_TYPE_RANDOM;
            params.primary_phy = 0x01;
            params.secondary_phy = 0x01;

            if sys::esp_ble_gap_ext_adv_set_params(0, &params) != sys::ESP_OK {
                return false;
            }
            if sys::esp_ble_gap_config_ext_adv_data_raw(0, pack.len() as _, pack.as_ptr())
                != sys::ESP_OK
            {
                return false;
            }

            let mut adv: sys::esp_ble_gap_ext_adv_t = core::mem::zeroed();
            adv.instance = 0;
            adv.duration = 0;
            adv.max_events = 0;
            sys::esp_ble_gap_ext_adv_start(1, &adv);

            // Instance 1: Coded PHY (long-range, 200+ m range).
            params.primary_phy = 0x04;
            params.secondary_phy = 0x04;

            if sys::esp_ble_gap_ext_adv_set_params(1, &params) == sys::ESP_OK {
                let _ = sys::esp_ble_gap_config_ext_adv_data_raw(
                    1,
                    pack.len() as _,
                    pack.as_ptr(),
                );
                adv.instance = 1;
                let _ = sys::esp_ble_gap_ext_adv_start(1, &adv);
            }
        }
    }

    true
}

/// Shut down BLE.
pub fn deinit() {
    if is_available() {
        unsafe {
            sys::esp_bluedroid_disable();
            sys::esp_bluedroid_deinit();
            sys::esp_bt_controller_disable();
            sys::esp_bt_controller_deinit();
        }
        unsafe { BLE_STATE.initialized = false; }
    }
}
