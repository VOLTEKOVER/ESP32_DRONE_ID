//! DroneCAN/UAVCAN v0 input protocol.
//!
//! Port of `rid_dronecan.c` (ESP32_DRONE_REMOTE_ID_Firmware). The C source is
//! a stub: `decode_fix2` is unreachable because a single classic-CAN frame can
//! never carry the 32 bytes it expects and no multi-frame transfer
//! reassembly exists. This crate implements the protocol properly:
//!
//! - 29-bit CAN identifier decode (priority, data type id, source node);
//! - tail-byte framing (SoT/EoT/toggle/transfer id);
//! - single-frame and multi-frame transfer reassembly per (source node,
//!   data type id), following libuavcan's `TransferReceiver` FSM, including
//!   the TID timeout and the transfer CRC check;
//! - decode of `uavcan.equipment.gnss.Fix2` (data type id 1063, signature
//!   `0xca41e7000f37435f`) from its bit-packed DSDL layout;
//! - the 5 s freshness window of the C `rid_dronecan_get`.
//!
//! The `org.drone_id.Identity` and `uavcan.equipment.ahrs.Solution` messages
//! of the C switch stay no-ops (no decode). `no_std`, allocation-free.

#![no_std]

#[cfg(test)]
extern crate std;

pub mod parser;
pub mod source;

pub use parser::DronecanParser;
pub use source::DronecanSource;
