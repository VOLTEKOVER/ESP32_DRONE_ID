//! `MavlinkSource`: wires the pure parser to a byte reader and implements the
//! `rid-interface` `GpsSource` contract (port of the polling in `rid_task`).

use rid_interface::input::{InputSample, OperatorLocation};
use rid_interface::{Config, GpsSource, Protocol, UartRead};

use crate::parser::MavlinkParser;

/// `MAV_RX_BUF` from `mavlink_parser.c`: bytes read per poll.
const READ_CHUNK: usize = 512;

/// MAVLink input source: owns a parser and a byte reader.
pub struct MavlinkSource<U: UartRead> {
    parser: MavlinkParser,
    uart: U,
}

impl<U: UartRead> MavlinkSource<U> {
    /// `mavlink_parser_init(uart_port)`: wraps the reader, resets the parser.
    pub fn new(uart: U) -> Self {
        Self {
            parser: MavlinkParser::new(),
            uart,
        }
    }
}

impl<U: UartRead> GpsSource for MavlinkSource<U> {
    fn sample(&mut self, config: &Config, now_ms: u32, now_us: u64) -> InputSample {
        // `mavlink_parser_set_sysid_filter(g_config.mavlink_sysid)`.
        self.parser.set_sysid_filter(config.mavlink_sysid);

        let mut buf = [0u8; READ_CHUNK];
        let n = self.uart.read(&mut buf);
        if n > 0 {
            self.parser.feed(&buf[..n], now_ms);
        }

        let mut sample = InputSample::new(now_ms, now_us);
        sample.proto = Protocol::Mavlink;
        sample.gps = self.parser.get(now_ms);
        sample.mavlink_armed = self.parser.get_armed();
        sample.mavlink_sysid = self.parser.get_sysid().map(u32::from);
        sample.mavlink_identity = self.parser.get_identity(now_ms);
        sample.mavlink_operator_location = self
            .parser
            .get_operator_location(now_ms)
            .map(|(lat, lon, alt)| OperatorLocation { lat, lon, alt });
        sample
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{crc_calculate, crc_accumulate};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::vec::Vec;

    #[derive(Clone)]
    struct MockUart(Rc<RefCell<Vec<u8>>>);

    impl MockUart {
        fn from(data: &[u8]) -> Self {
            Self(Rc::new(RefCell::new(data.to_vec())))
        }
    }

    impl UartRead for MockUart {
        fn read(&mut self, buf: &mut [u8]) -> usize {
            let mut data = self.0.borrow_mut();
            if data.is_empty() {
                return 0;
            }
            let n = data.len().min(buf.len());
            buf[..n].copy_from_slice(&data[..n]);
            data.drain(..n);
            n
        }
    }

    fn gps_raw_int_frame() -> Vec<u8> {
        let mut payload = [0u8; 52];
        payload[8..12].copy_from_slice(&453040500i32.to_le_bytes());
        payload[12..16].copy_from_slice(&9387500i32.to_le_bytes());
        payload[16..20].copy_from_slice(&1234000i32.to_le_bytes());
        payload[24..26].copy_from_slice(&1050u16.to_le_bytes());
        payload[26..28].copy_from_slice(&12340u16.to_le_bytes());
        payload[28] = 3;
        payload[29] = 9;
        let mut f = [0xfd, payload.len() as u8, 0, 0, 0, 1, 1, 24, 0, 0].to_vec();
        f.extend_from_slice(&payload);
        let mut crc = 0xffffu16;
        for &b in &f[1..] {
            crc = crc_accumulate(b, crc);
        }
        crc = crc_accumulate(24, crc);
        f.push((crc & 0xff) as u8);
        f.push((crc >> 8) as u8);
        f
    }

    fn heartbeat_frame(base_mode: u8) -> Vec<u8> {
        let mut payload = [0u8; 9];
        payload[6] = base_mode;
        let mut f = [0xfd, 9, 0, 0, 1, 1, 1, 0, 0, 0].to_vec();
        f.extend_from_slice(&payload);
        let crc = crc_calculate(&f[1..]);
        let crc = crc_accumulate(50, crc);
        f.push((crc & 0xff) as u8);
        f.push((crc >> 8) as u8);
        f
    }

    #[test]
    fn source_polls_and_fills_mavlink_fields() {
        let mut stream = heartbeat_frame(0x80);
        stream.extend_from_slice(&gps_raw_int_frame());
        let mut src = MavlinkSource::new(MockUart::from(&stream));

        let cfg = Config {
            mavlink_sysid: 1,
            ..Config::default()
        };

        let s1 = src.sample(&cfg, 1000, 1_000_000);
        assert_eq!(s1.proto, Protocol::Mavlink);
        assert_eq!(s1.now_ms, 1000);
        assert_eq!(s1.now_us, 1_000_000);
        assert_eq!(s1.mavlink_armed, Some(true));
        assert_eq!(s1.mavlink_sysid, Some(1));
        let gps = s1.gps.expect("gps parsed in the first poll");
        assert!((gps.latitude - 45.30405).abs() < 1e-9);
        assert_eq!(gps.fix_type, 3);
        assert_eq!(gps.satellites, 9);
        assert!(gps.armed);
        assert!(s1.mavlink_identity.is_none());

        // Second poll: no new bytes, last snapshot still returned.
        let s2 = src.sample(&cfg, 2000, 2_000_000);
        assert_eq!(s2.proto, Protocol::Mavlink);
        assert!(s2.gps.is_some());
    }

    #[test]
    fn source_empty_until_fix() {
        let mut src = MavlinkSource::new(MockUart::from(&[0xfd, 0x09]));
        let s = src.sample(&Config::default(), 0, 0);
        assert_eq!(s.proto, Protocol::Mavlink);
        assert!(s.gps.is_none());
        assert!(s.mavlink_armed.is_none());
    }

    #[test]
    fn source_sysid_filter_from_config() {
        let stream = gps_raw_int_frame();
        let mut src = MavlinkSource::new(MockUart::from(&stream));
        let cfg = Config {
            mavlink_sysid: 9, // frame is sysid 1 -> filtered out
            ..Config::default()
        };
        let s = src.sample(&cfg, 1000, 1000);
        assert!(s.gps.is_none());
        assert!(s.mavlink_sysid.is_none());
    }

    #[test]
    fn source_feed_split_across_polls() {
        let data = gps_raw_int_frame();
        let buf = MockUart::from(&data[..30]);
        let mut src = MavlinkSource::new(buf.clone());
        assert!(src.sample(&Config::default(), 0, 0).gps.is_none());
        buf.0.borrow_mut().extend_from_slice(&data[30..]);
        let s = src.sample(&Config::default(), 1000, 1000);
        assert!(s.gps.is_some());
    }
}
