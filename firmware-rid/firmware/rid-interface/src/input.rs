//! Hourglass input/output contracts.
//!
//! The scheduler (in `rid-core`) is driven by a `GpsSource` (input port) and
//! pushes to a `Transmitter` (output port). The `proto-*` crates implement
//! `GpsSource` for their parser; the `bsp-*`/`out-*` crates implement
//! `Transmitter` for their transports. Adding a protocol or a transport never
//! touches the core logic.

use crate::types::{Config, GpsData, Identity, Protocol};

/// Fresh MAVLink operator location (mirrors the `mavlink_parser_get_operator_location`
/// output). Only present when the location is inside its freshness window.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct OperatorLocation {
    pub lat: f64,
    pub lon: f64,
    pub alt: f32,
}

/// One loop iteration of input data, as produced by the active input source.
///
/// Port of the parser polling + auto-detect in `rid_task`: the source resolves
/// the configured protocol (AUTO -> detected) and returns the resolved
/// `proto` plus the parser's results.
#[derive(Clone, Copy, Debug)]
pub struct InputSample {
    /// Protocol that produced `gps` (NMEA/MSP/MAVLINK, or `Unknown` when
    /// nothing was available). Never `Auto`: the source resolves it.
    pub proto: Protocol,
    /// Primary parser result; `None` when the parser had no data.
    pub gps: Option<GpsData>,
    /// DroneCAN secondary source; `None` when inactive.
    pub dronecan: Option<GpsData>,
    /// MAVLink arm status (only meaningful when `proto == Mavlink`).
    pub mavlink_armed: Option<bool>,
    /// MAVLink system id (only meaningful when `proto == Mavlink`).
    pub mavlink_sysid: Option<u32>,
    /// MAVLink identity relay (only meaningful when `proto == Mavlink`).
    pub mavlink_identity: Option<Identity>,
    /// Fresh MAVLink operator location (only meaningful when `proto == Mavlink`).
    pub mavlink_operator_location: Option<OperatorLocation>,
    /// Monotonic millisecond clock (port of `xTaskGetTickCount() * portTICK_PERIOD_MS`).
    pub now_ms: u32,
    /// Monotonic microsecond clock (port of `esp_timer_get_time()`).
    pub now_us: u64,
}

impl InputSample {
    /// Empty sample with just the clocks set.
    pub fn new(now_ms: u32, now_us: u64) -> Self {
        Self {
            proto: Protocol::Unknown,
            gps: None,
            dronecan: None,
            mavlink_armed: None,
            mavlink_sysid: None,
            mavlink_identity: None,
            mavlink_operator_location: None,
            now_ms,
            now_us,
        }
    }
}

/// Input contract: one poll per scheduler tick.
/// Implemented by the `proto-*` parser crates.
///
/// `now_ms`/`now_us` are the monotonic clocks of the current tick (port of
/// `xTaskGetTickCount() * portTICK_PERIOD_MS` / `esp_timer_get_time()`); they
/// are passed in so the source stays free of hardware dependencies.
pub trait GpsSource {
    fn sample(&mut self, config: &Config, now_ms: u32, now_us: u64) -> InputSample;
}

/// Byte source for the parsers (port of the `uart_read_bytes` side of the
/// `*_parser_get` functions). On the real hardware this is a UART; on the
/// host it can be a mock/stream.
pub trait UartRead {
    /// Reads up to `buf.len()` bytes, returning how many were read (0 = none
    /// right now).
    fn read(&mut self, buf: &mut [u8]) -> usize;
}

impl<U: UartRead + ?Sized> UartRead for &mut U {
    fn read(&mut self, buf: &mut [u8]) -> usize {
        (**self).read(buf)
    }
}

/// Byte sink for the TX protocols (port of the `uart_write_bytes` side used
/// by `rid_mavlink_usb_write`). On the real hardware this is a UART; on the
/// host it can be a mock.
pub trait UartWrite {
    /// Writes `buf`, returning how many bytes were accepted (`== buf.len()`
    /// on success, 0 = nothing written right now).
    fn write(&mut self, buf: &[u8]) -> usize;
}

impl<W: UartWrite + ?Sized> UartWrite for &mut W {
    fn write(&mut self, buf: &[u8]) -> usize {
        (**self).write(buf)
    }
}

/// One CAN message as delivered by the TWAI driver
/// (port of `twai_message_t`: `identifier`, `data_length_code`, `data`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct CanFrame {
    /// CAN identifier (29-bit extended for DroneCAN).
    pub id: u32,
    /// `data_length_code` (0..=8 for classic CAN).
    pub dlc: u8,
    /// `data`.
    pub data: [u8; 8],
}

/// CAN frame source for the DroneCAN parser (port of the `twai_receive` side
/// of `rid_dronecan_get`). On the real hardware this is the TWAI controller;
/// on the host it can be a mock/stream.
pub trait CanRead {
    /// Reads up to `frames.len()` frames, returning how many were read
    /// (0 = none right now).
    fn read(&mut self, frames: &mut [CanFrame]) -> usize;
}

impl<C: CanRead + ?Sized> CanRead for &mut C {
    fn read(&mut self, frames: &mut [CanFrame]) -> usize {
        (**self).read(frames)
    }
}

/// Output contract: the four broadcast transports.
/// Implemented by the BSP/output crates (which encode via the hub).
pub trait Transmitter {
    /// Port of `wifi_tx_transmit()`.
    fn wifi_bcn(&mut self, gps: &GpsData, identity: &Identity, config: &Config);
    /// Port of `wifi_tx_transmit_nan()`.
    fn wifi_nan(&mut self, gps: &GpsData, identity: &Identity, config: &Config, counter: u8);
    /// Port of `ble_tx_transmit_legacy()`.
    fn ble4(&mut self, gps: &GpsData, identity: &Identity, config: &Config);
    /// Port of `ble_tx_transmit_lr()`.
    fn ble5(&mut self, gps: &GpsData, identity: &Identity, config: &Config);
}
