//! NMEA GPS input protocol.
//!
//! Port of `nmea_parser.c` (ESP32_DRONE_REMOTE_ID_Firmware). The C parser
//! reads a UART inside `nmea_parser_get`; this crate splits that into a pure
//! streaming parser (`NmeaParser::feed`) plus a `NmeaSource` that owns a
//! byte reader (`UartRead`) and implements the `rid-interface` `GpsSource`
//! contract for the scheduler. `no_std`, allocation-free.

#![no_std]

#[cfg(test)]
extern crate std;

pub mod parser;
pub mod source;

pub use parser::{NMEA_BUF_SIZE, NmeaParser};
pub use source::NmeaSource;
