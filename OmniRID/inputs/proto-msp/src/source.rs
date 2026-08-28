//! `MspSource`: wires the pure parser to a byte reader and implements the
//! `rid-interface` `GpsSource` contract (port of the polling in `rid_task`).

use rid_interface::input::InputSample;
use rid_interface::{Config, GpsSource, Protocol, UartRead};

use crate::parser::MspParser;

/// `uart_read_bytes` chunk used by `msp_parser_get`.
const READ_CHUNK: usize = 64;

/// MSP input source: owns a parser and a byte reader.
pub struct MspSource<U: UartRead> {
    parser: MspParser,
    uart: U,
}

impl<U: UartRead> MspSource<U> {
    /// `msp_parser_init(uart_port)`: wraps the reader, resets the parser.
    pub fn new(uart: U) -> Self {
        Self {
            parser: MspParser::new(),
            uart,
        }
    }
}

impl<U: UartRead> GpsSource for MspSource<U> {
    fn sample(&mut self, _config: &Config, now_ms: u32, now_us: u64) -> InputSample {
        let mut buf = [0u8; READ_CHUNK];
        let n = self.uart.read(&mut buf);
        if n > 0 {
            self.parser.feed(&buf[..n]);
        }
        let mut sample = InputSample::new(now_ms, now_us);
        sample.proto = Protocol::Msp;
        sample.gps = self.parser.get();
        sample
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn raw_gps_frame() -> Vec<u8> {
        let mut payload = [0u8; 16];
        payload[0] = 3;
        payload[1] = 10;
        payload[2..6].copy_from_slice(&453040500i32.to_le_bytes());
        payload[6..10].copy_from_slice(&9387500i32.to_le_bytes());
        payload[10..12].copy_from_slice(&1234i16.to_le_bytes());
        payload[12..14].copy_from_slice(&5432i16.to_le_bytes());
        payload[14..16].copy_from_slice(&1800i16.to_le_bytes());
        let mut f = [b'$', b'M', b'<', 16, 106].to_vec();
        f.extend_from_slice(&payload);
        let crc = f[5..].iter().fold(0u8, |c, &b| c ^ b);
        f.push(crc);
        f
    }

    #[test]
    fn source_polls_and_resolves_protocol() {
        let mut src = MspSource::new(MockUart::from(&raw_gps_frame()));
        let cfg = Config::default();

        let s1 = src.sample(&cfg, 1000, 1_000_000);
        assert_eq!(s1.proto, Protocol::Msp);
        assert_eq!(s1.now_ms, 1000);
        assert_eq!(s1.now_us, 1_000_000);
        let gps = s1.gps.expect("parsed in the first poll");
        assert!((gps.latitude - 45.30405).abs() < 1e-9);
        assert_eq!(gps.fix_type, 3);
        assert_eq!(gps.satellites, 10);

        // Second poll: no new bytes, last snapshot still returned.
        let s2 = src.sample(&cfg, 2000, 2_000_000);
        assert_eq!(s2.proto, Protocol::Msp);
        assert!(s2.gps.is_some());
    }

    #[test]
    fn source_empty_until_fix() {
        let mut src = MspSource::new(MockUart::from(&[0x24, 0x4D, 0x3C, 0x01, 0x02, 0x03]));
        let s = src.sample(&Config::default(), 0, 0);
        assert_eq!(s.proto, Protocol::Msp);
        assert!(s.gps.is_none());
    }

    #[test]
    fn source_feed_split_across_polls() {
        let data = raw_gps_frame();
        let buf = MockUart::from(&data[..10]);
        let mut src = MspSource::new(buf.clone());
        assert!(src.sample(&Config::default(), 0, 0).gps.is_none());
        buf.0.borrow_mut().extend_from_slice(&data[10..]);
        let s = src.sample(&Config::default(), 1000, 1000);
        assert!(s.gps.is_some());
    }
}
