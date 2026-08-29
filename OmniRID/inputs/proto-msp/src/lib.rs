//! MSP (MultiWii Serial Protocol) input.
//!
//! Port of `msp_parser.c` (ESP32_DRONE_REMOTE_ID_Firmware). Like
//! `proto-nmea`, the C parser reads a UART inside `msp_parser_get`; this crate
//! splits that into a pure streaming parser (`MspParser::feed`) plus a
//! `MspSource` that owns a `UartRead` and implements the `rid-interface`
//! `GpsSource` contract. `no_std`, allocation-free.

#![no_std]

#[cfg(test)]
extern crate std;

pub mod parser;
pub mod source;

pub use parser::{MspParser, MSP_BUF_SIZE};
pub use source::MspSource;
