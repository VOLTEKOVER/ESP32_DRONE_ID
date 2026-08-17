//! Streaming MSP parser, port of `msp_parser.c`.
//!
//! Behavioural parity notes:
//! - Frame layout as decoded by the C code is `$M<` + one extra byte + size +
//!   type + payload + crc (total `7 + size` bytes, crc at `6 + size`). The
//!   extra byte at index 3 is *included* in the XOR checksum, so a real
//!   Betaflight/iNav `$M<` + size + type + payload + crc stream is not
//!   decoded byte-for-byte. This quirk is replicated 1:1.
//! - The frame is parsed as soon as the byte after the checksum arrives
//!   (`g_buf_idx >= 7 + size`); the checksum is the last byte read.
//! - A `$` byte re-syncs: buffer reset and `g_in_message = true`.
//! - Only MSP_RAW_GPS (106), MSP_ATTITUDE (108) and MSP_STATUS (101) are
//!   handled; RAW_GPS requires >= 16 payload bytes, ATTITUDE >= 6, STATUS
//!   >= 10.
//! - Multi-byte fields are little-endian; coordinates are `1e-7` degrees,
//!   altitudes `1e-1` m, speed `1e-2` m/s, heading `1e-1` deg (integer
//!   division).

use rid_interface::GpsData;

/// `MSP_BUF_SIZE` from the C source.
pub const MSP_BUF_SIZE: usize = 256;
/// `MSP_RAW_GPS` from the C source.
const MSP_RAW_GPS: u8 = 106;
/// `MSP_ATTITUDE` from the C source.
const MSP_ATTITUDE: u8 = 108;
/// `MSP_STATUS` from the C source.
const MSP_STATUS: u8 = 101;

/// `msp_crc()`: XOR checksum over `data[0..len]`.
fn msp_crc(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |c, &b| c ^ b)
}

/// `parse_msp_gps()`: RAW_GPS payload (>= 16 bytes).
fn parse_msp_gps(gps: &mut GpsData, data: &[u8]) {
    if data.len() < 16 {
        return;
    }
    let fix = data[0];
    let sat = data[1];
    let lat = i32::from_le_bytes([data[2], data[3], data[4], data[5]]);
    let lon = i32::from_le_bytes([data[6], data[7], data[8], data[9]]);
    let alt = i16::from_le_bytes([data[10], data[11]]);
    let speed = i16::from_le_bytes([data[12], data[13]]);
    let ground_course = i16::from_le_bytes([data[14], data[15]]);

    gps.fix_type = fix;
    gps.satellites = sat;
    gps.latitude = lat as f64 / 10_000_000.0;
    gps.longitude = lon as f64 / 10_000_000.0;
    gps.altitude_msl = alt as f32 / 10.0;
    gps.altitude_baro = gps.altitude_msl;
    gps.speed = speed as f32 / 100.0;
    gps.heading = ground_course / 10;
}

/// `parse_msp_attitude()`: ATTITUDE payload (>= 6 bytes).
fn parse_msp_attitude(gps: &mut GpsData, data: &[u8]) {
    if data.len() < 6 {
        return;
    }
    let yaw = i16::from_le_bytes([data[4], data[5]]);
    gps.heading = yaw / 10;
}

/// `parse_msp_status()`: STATUS payload (>= 10 bytes).
fn parse_msp_status(gps: &mut GpsData, data: &[u8]) {
    if data.len() < 10 {
        return;
    }
    let flag = u32::from_le_bytes([data[6], data[7], data[8], data[9]]);
    gps.armed = (flag & 1) != 0;
}

/// `parse_msp()`: validate the frame and dispatch on the message type.
fn parse_msp(gps: &mut GpsData, buf: &[u8]) {
    if buf.len() < 6 {
        return;
    }
    if buf[0] != b'$' || buf[1] != b'M' || buf[2] != b'<' {
        return;
    }
    let msp_size = buf[4];
    let msp_type = buf[5];

    let payload_offset = 6;
    if payload_offset + msp_size as usize > buf.len() - 1 {
        return;
    }

    let crc_received = buf[buf.len() - 1];
    // XOR over `buf[3]` for `size + 2` bytes (the extra byte, size, type and
    // the payload).
    let crc_calc = msp_crc(&buf[3..payload_offset + msp_size as usize]);
    if crc_calc != crc_received {
        return;
    }

    let data = &buf[payload_offset..payload_offset + msp_size as usize];
    match msp_type {
        MSP_RAW_GPS => parse_msp_gps(gps, data),
        MSP_ATTITUDE => parse_msp_attitude(gps, data),
        MSP_STATUS => parse_msp_status(gps, data),
        _ => {}
    }
}

/// Streaming MSP parser with the same buffer semantics as the C module.
pub struct MspParser {
    buf: [u8; MSP_BUF_SIZE],
    idx: usize,
    in_message: bool,
    last_gps: GpsData,
}

impl Default for MspParser {
    fn default() -> Self {
        Self::new()
    }
}

impl MspParser {
    /// `msp_parser_init()`: zeroed GPS snapshot (the buffer needs no reset:
    /// re-sync happens on `$`).
    pub fn new() -> Self {
        Self {
            buf: [0; MSP_BUF_SIZE],
            idx: 0,
            in_message: false,
            last_gps: GpsData::default(),
        }
    }

    /// Port of the byte loop in `msp_parser_get()`.
    pub fn feed(&mut self, bytes: &[u8]) {
        for &c in bytes {
            if c == b'$' {
                self.idx = 0;
                self.in_message = true;
            }
            if self.in_message {
                if self.idx < MSP_BUF_SIZE {
                    self.buf[self.idx] = c;
                    self.idx += 1;
                }
                if self.idx >= 3 && self.idx >= 7 + self.buf[4] as usize {
                    parse_msp(&mut self.last_gps, &self.buf[..self.idx]);
                    self.in_message = false;
                    self.idx = 0;
                }
            }
        }
    }

    /// Port of the trailing check in `msp_parser_get()`: returns the last
    /// valid snapshot (3D fix with non-zero latitude) or `None`.
    pub fn get(&self) -> Option<GpsData> {
        if self.last_gps.fix_type >= 2 && self.last_gps.latitude != 0.0 {
            Some(self.last_gps)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    /// Builds a frame in the layout the C parser decodes:
    /// `$M<` + x + size + type + payload + crc, with the crc over
    /// `buf[3..6+size)` (extra byte, size, type and payload).
    fn frame(x: u8, msp_type: u8, payload: &[u8]) -> Vec<u8> {
        let mut f = [b'$', b'M', b'<', x, payload.len() as u8, msp_type].to_vec();
        f.extend_from_slice(payload);
        let crc = f[3..].iter().fold(0u8, |c, &b| c ^ b);
        f.push(crc);
        f
    }

    fn parse(stream: &[u8]) -> Option<GpsData> {
        let mut p = MspParser::new();
        p.feed(stream);
        p.get()
    }

    #[test]
    fn raw_gps_fix() {
        // fix=3, sats=10, lat=453040500 (45.30405), lon=-9387500 (-0.93875),
        // alt=1234 (123.4 m), speed=5432 (54.32 m/s), course=1800 (180 deg).
        let mut payload = [0u8; 16];
        payload[0] = 3;
        payload[1] = 10;
        payload[2..6].copy_from_slice(&453040500i32.to_le_bytes());
        payload[6..10].copy_from_slice(&(-9387500i32).to_le_bytes());
        payload[10..12].copy_from_slice(&1234i16.to_le_bytes());
        payload[12..14].copy_from_slice(&5432i16.to_le_bytes());
        payload[14..16].copy_from_slice(&1800i16.to_le_bytes());

        let gps = parse(&frame(0x00, 106, &payload)).expect("valid fix");
        assert_eq!(gps.fix_type, 3);
        assert_eq!(gps.satellites, 10);
        assert!((gps.latitude - 45.30405).abs() < 1e-9);
        assert!((gps.longitude + 0.93875).abs() < 1e-9);
        assert!((gps.altitude_msl - 123.4).abs() < 1e-5);
        assert!((gps.altitude_baro - 123.4).abs() < 1e-5);
        assert!((gps.speed - 54.32).abs() < 1e-5);
        assert_eq!(gps.heading, 180); // 1800 / 10 (integer div)
    }

    #[test]
    fn raw_gps_too_short_ignored() {
        // 8-byte payload: `parse_msp_gps` early-out, snapshot untouched.
        let mut payload = [0u8; 8];
        payload[0] = 3;
        assert!(parse(&frame(0x00, 106, &payload)).is_none());
    }

    #[test]
    fn attitude_and_status_merge() {
        // GGA-style RAW_GPS first to get a 3D fix.
        let mut payload = [0u8; 16];
        payload[0] = 3;
        payload[1] = 5;
        payload[2..6].copy_from_slice(&(453040500i32).to_le_bytes());
        payload[6..10].copy_from_slice(&9387500i32.to_le_bytes());
        payload[10..12].copy_from_slice(&1234i16.to_le_bytes());

        let mut att = [0u8; 6];
        att[4..6].copy_from_slice(&2700i16.to_le_bytes()); // yaw = 270.0 deg

        let mut status = [0u8; 10];
        status[6..10].copy_from_slice(&1u32.to_le_bytes()); // armed bit 0

        let mut p = MspParser::new();
        p.feed(&frame(0x00, 106, &payload));
        p.feed(&frame(0x00, 108, &att));
        p.feed(&frame(0x00, 101, &status));
        let gps = p.get().expect("valid fix");
        assert!((gps.latitude - 45.30405).abs() < 1e-9);
        assert_eq!(gps.heading, 270); // ATTITUDE overwrote RAW_GPS course
        assert!(gps.armed);
    }

    #[test]
    fn status_flag_bit0_maps_to_armed() {
        let mut status = [0u8; 10];
        status[6..10].copy_from_slice(&3u32.to_le_bytes()); // armed | box[0]
        let mut p = MspParser::new();
        p.feed(&frame(0x00, 101, &status));
        assert!(p.get().is_none()); // no fix yet
        let mut payload = [0u8; 16];
        payload[0] = 2;
        payload[1] = 7;
        payload[2..6].copy_from_slice(&453040500i32.to_le_bytes());
        payload[6..10].copy_from_slice(&9387500i32.to_le_bytes());
        p.feed(&frame(0x00, 106, &payload));
        let gps = p.get().expect("valid fix");
        assert!(gps.armed); // STATUS was stored before the fix arrived
    }

    #[test]
    fn wrong_crc_ignored() {
        let mut payload = [0u8; 16];
        payload[0] = 3;
        payload[2..6].copy_from_slice(&453040500i32.to_le_bytes());
        payload[6..10].copy_from_slice(&9387500i32.to_le_bytes());
        let mut f = frame(0x00, 106, &payload);
        let n = f.len();
        f[n - 1] ^= 0xFF;
        assert!(parse(&f).is_none());
    }

    #[test]
    fn bad_header_ignored() {
        let mut payload = [0u8; 16];
        payload[0] = 3;
        let mut f = frame(0x00, 106, &payload);
        f[0] = b'X';
        assert!(parse(&f).is_none());
    }

    #[test]
    fn unknown_type_ignored() {
        // Type 109 is not handled: the frame is valid (crc ok) but stores
        // nothing, so `get()` stays None.
        assert!(parse(&frame(0x00, 109, &[1, 2, 3, 4, 5, 6])).is_none());
    }

    #[test]
    fn dollar_resync_mid_stream() {
        // A '$' inside what should be a frame re-syncs the parser; the rest
        // no longer forms a valid frame, so no fix is stored.
        let mut payload = [0u8; 16];
        payload[0] = 3;
        payload[2..6].copy_from_slice(&453040500i32.to_le_bytes());
        payload[6..10].copy_from_slice(&9387500i32.to_le_bytes());
        let mut f = frame(0x00, 106, &payload);
        f[5] = b'$';
        assert!(parse(&f).is_none());
    }

    #[test]
    fn back_to_back_frames() {
        let mut payload = [0u8; 16];
        payload[0] = 3;
        payload[1] = 4;
        payload[2..6].copy_from_slice(&453040500i32.to_le_bytes());
        payload[6..10].copy_from_slice(&9387500i32.to_le_bytes());
        let mut stream = Vec::new();
        for _ in 0..3 {
            stream.extend_from_slice(&frame(0x00, 106, &payload));
        }
        let mut p = MspParser::new();
        p.feed(&stream);
        let gps = p.get().expect("valid fix");
        assert_eq!(gps.satellites, 4);
    }

    #[test]
    fn partial_frame_across_feed_calls() {
        let mut payload = [0u8; 16];
        payload[0] = 3;
        payload[2..6].copy_from_slice(&453040500i32.to_le_bytes());
        payload[6..10].copy_from_slice(&9387500i32.to_le_bytes());
        let f = frame(0x00, 106, &payload);
        let mut p = MspParser::new();
        p.feed(&f[..8]);
        assert!(p.get().is_none());
        p.feed(&f[8..]);
        assert!(p.get().is_some());
    }

    #[test]
    fn extra_byte_part_of_crc() {
        // The x byte at index 3 is covered by the checksum: toggling it after
        // the frame is built makes the crc mismatch, so nothing parses.
        let mut payload = [0u8; 16];
        payload[0] = 3;
        payload[2..6].copy_from_slice(&453040500i32.to_le_bytes());
        payload[6..10].copy_from_slice(&9387500i32.to_le_bytes());
        let mut f = frame(0x00, 106, &payload);
        assert!(parse(&f).is_some());
        f[3] ^= 0xFF;
        assert!(parse(&f).is_none());
    }
}
