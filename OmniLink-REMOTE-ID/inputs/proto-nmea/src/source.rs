//! `NmeaSource`: wires the pure parser to a byte reader and implements the
//! `rid-interface` `GpsSource` contract (port of the polling in `rid_task`).

use rid_interface::input::InputSample;
use rid_interface::{Config, GpsSource, Protocol, UartRead};

use crate::parser::NmeaParser;

/// `uart_read_bytes` chunk used by `nmea_parser_get`.
const READ_CHUNK: usize = 64;

/// NMEA input source: owns a parser and a byte reader.
pub struct NmeaSource<U: UartRead> {
    parser: NmeaParser,
    uart: U,
}

impl<U: UartRead> NmeaSource<U> {
    /// `nmea_parser_init(uart_port)`: wraps the reader, resets the parser.
    pub fn new(uart: U) -> Self {
        Self {
            parser: NmeaParser::new(),
            uart,
        }
    }
}

impl<U: UartRead> GpsSource for NmeaSource<U> {
    fn sample(&mut self, _config: &Config, now_ms: u32, now_us: u64) -> InputSample {
        let mut buf = [0u8; READ_CHUNK];
        let n = self.uart.read(&mut buf);
        if n > 0 {
            self.parser.feed(&buf[..n]);
        }
        let mut sample = InputSample::new(now_ms, now_us);
        sample.proto = Protocol::Nmea;
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

    #[test]
    fn source_polls_and_resolves_protocol() {
        // Short GGA (59 bytes incl. CRLF) fits one 64-byte read chunk.
        let mut src = NmeaSource::new(MockUart::from(
            b"$GPGGA,123519,4807.038,N,01131.000,E,3,08,0.9,545.4,M*47\r\n",
        ));
        let cfg = Config::default();

        let s1 = src.sample(&cfg, 1000, 1_000_000);
        assert_eq!(s1.proto, Protocol::Nmea);
        assert_eq!(s1.now_ms, 1000);
        assert_eq!(s1.now_us, 1_000_000);
        let gps = s1.gps.expect("parsed in the first poll");
        assert!((gps.latitude - 48.1173).abs() < 1e-9);
        assert_eq!(gps.fix_type, 3);

        // Second poll: no new bytes, last snapshot still returned.
        let s2 = src.sample(&cfg, 2000, 2_000_000);
        assert_eq!(s2.proto, Protocol::Nmea);
        assert!(s2.gps.is_some());
        assert_eq!(s2.now_ms, 2000);
    }

    #[test]
    fn source_empty_until_fix() {
        let mut src = NmeaSource::new(MockUart::from(
            b"$GPRMC,1,V,0,0,0,0,000.0,000.0,230394,,*0A\r\n",
        ));
        let s = src.sample(&Config::default(), 0, 0);
        assert_eq!(s.proto, Protocol::Nmea);
        assert!(s.gps.is_none());
    }

    #[test]
    fn source_feed_split_across_polls() {
        let data = b"$GPGGA,123519,4807.038,N,01131.000,E,3,08,0.9,545.4,M,46.9,M,,*47\r\n";
        let buf = MockUart::from(&data[..20]);
        let mut src = NmeaSource::new(buf.clone());
        assert!(src.sample(&Config::default(), 0, 0).gps.is_none());
        buf.0.borrow_mut().extend_from_slice(&data[20..]);
        let s = src.sample(&Config::default(), 1000, 1000);
        assert!(s.gps.is_some());
    }
}
