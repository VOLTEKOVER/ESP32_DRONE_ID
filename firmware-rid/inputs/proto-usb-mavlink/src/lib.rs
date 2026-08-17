//! MAVLink v2 TX telemetry mirror (port of `rid_mavlink_tx.c` +
//! `rid_mavlink_usb.c`): packs HEARTBEAT and OPEN_DRONE_ID_SYSTEM frames and
//! writes them to a byte sink (USB Serial/JTAG on the real hardware, a mock
//! on the host).
//!
//! The C firmware sends these two messages over the flight-controller UART
//! and mirrors them to the USB console port when `mavlink_usb_enable` is set.
//! This crate implements the USB mirror; the message packing in [`pack`] is
//! shared logic a future FC-UART mirror can reuse. Unlike the input `proto-*`
//! crates this one does not implement `GpsSource`: it is telemetry out.
#![no_std]

#[cfg(test)]
extern crate std;

pub mod pack;
pub mod tx;

pub use pack::{pack_heartbeat, pack_open_drone_id_system, MAX_FRAME_LEN};
pub use tx::{MavlinkUsbTx, HEARTBEAT_PERIOD_US, OP_ALT_UNKNOWN, SYSTEM_PERIOD_US};
