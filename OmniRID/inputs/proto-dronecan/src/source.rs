//! `DronecanSource`: wires the pure parser to a CAN reader and implements the
//! `rid-interface` `GpsSource` contract (port of the `rid_dronecan_get`
//! polling). DroneCAN is a secondary input: the sample fills `dronecan` and
//! leaves `proto`/`gps` untouched.

use rid_interface::input::{CanFrame, CanRead, InputSample};
use rid_interface::{Config, GpsSource};

use crate::parser::DronecanParser;

/// Frames drained per poll (`rx_queue_len` from `rid_dronecan_init`).
const READ_FRAMES: usize = 10;

/// DroneCAN input source: owns a parser and a CAN reader.
pub struct DronecanSource<C: CanRead> {
    parser: DronecanParser,
    can: C,
}

impl<C: CanRead> DronecanSource<C> {
    /// `rid_dronecan_init`: wraps the reader, resets the parser.
    pub fn new(can: C) -> Self {
        Self {
            parser: DronecanParser::new(),
            can,
        }
    }
}

impl<C: CanRead> GpsSource for DronecanSource<C> {
    fn sample(&mut self, _config: &Config, now_ms: u32, now_us: u64) -> InputSample {
        let mut frames = [CanFrame::default(); READ_FRAMES];
        let n = self.can.read(&mut frames);
        for f in &frames[..n] {
            self.parser.feed(f, now_ms);
        }

        let mut sample = InputSample::new(now_ms, now_us);
        sample.dronecan = self.parser.get(now_ms);
        sample
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::test_support::{multi_frame, pack_fix2, Fix2Fields};
    use rid_interface::Protocol;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::vec::Vec;

    #[derive(Clone)]
    struct MockCan(Rc<RefCell<Vec<CanFrame>>>);

    impl MockCan {
        fn from(frames: Vec<CanFrame>) -> Self {
            Self(Rc::new(RefCell::new(frames)))
        }
    }

    impl CanRead for MockCan {
        fn read(&mut self, frames: &mut [CanFrame]) -> usize {
            let mut data = self.0.borrow_mut();
            if data.is_empty() {
                return 0;
            }
            let n = data.len().min(frames.len());
            frames[..n].copy_from_slice(&data[..n]);
            data.drain(..n);
            n
        }
    }

    fn fix2_transfer() -> Vec<CanFrame> {
        let payload = pack_fix2(&Fix2Fields::default());
        multi_frame(1, 0, &payload, &[])
    }

    #[test]
    fn source_polls_and_fills_dronecan_field() {
        let mut src = DronecanSource::new(MockCan::from(fix2_transfer()));
        let s = src.sample(&Config::default(), 1000, 1_000_000);
        assert_eq!(s.proto, Protocol::Unknown);
        assert!(s.gps.is_none());
        let gps = s.dronecan.expect("dronecan gps after a valid transfer");
        assert!((gps.latitude - 45.30405).abs() < 1e-9);
        assert!((gps.longitude - 9.3875).abs() < 1e-9);
        assert_eq!(gps.fix_type, 3);
        assert_eq!(gps.satellites, 12);
        assert_eq!(gps.heading, 346);
        assert!((gps.speed - 10.307764).abs() < 1e-3);
        assert!((gps.altitude_msl - 1050.0).abs() < 1e-3);
        assert!((gps.altitude_relative - 1234.0).abs() < 1e-3);
    }

    #[test]
    fn source_empty_until_transfer() {
        let mut src = DronecanSource::new(MockCan::from(Vec::new()));
        let s = src.sample(&Config::default(), 1000, 1_000_000);
        assert!(s.dronecan.is_none());
    }

    #[test]
    fn source_frames_split_across_polls() {
        let frames = fix2_transfer();
        let buf = MockCan::from(frames[..3].to_vec());
        let mut src = DronecanSource::new(buf.clone());
        assert!(src
            .sample(&Config::default(), 1000, 1000)
            .dronecan
            .is_none());
        buf.0.borrow_mut().extend_from_slice(&frames[3..]);
        let s = src.sample(&Config::default(), 1200, 1200);
        assert!(s.dronecan.is_some());
    }
}
