//! MAVLink v1/v2 input protocol.
//!
//! Port of `mavlink_parser.c` (ESP32_DRONE_REMOTE_ID_Firmware). The C parser
//! reads a UART inside `mavlink_parser_get`; this crate splits that into a
//! pure streaming parser (`MavlinkParser::feed`) plus a `MavlinkSource` that
//! owns a byte reader (`UartRead`) and implements the `rid-interface`
//! `GpsSource` contract for the scheduler. `no_std`, allocation-free.

#![no_std]

#[cfg(test)]
extern crate std;

pub mod parser;
pub mod source;

pub use parser::MavlinkParser;
pub use source::MavlinkSource;
