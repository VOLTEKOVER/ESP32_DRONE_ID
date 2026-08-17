//! `MavlinkUsbTx`: periodic MAVLink telemetry writer (port of the
//! `rid_mavlink_tx_task` loop in `rid_mavlink_tx.c`, USB mirror only).

use rid_interface::input::OperatorLocation;
use rid_interface::UartWrite;

use crate::pack::{pack_heartbeat, pack_open_drone_id_system, MAX_FRAME_LEN};

/// `TX_SYSID` from `rid_mavlink_tx.c`.
pub const TX_SYSID: u8 = 0x41;
/// `TX_COMPID` from `rid_mavlink_tx.c`.
pub const TX_COMPID: u8 = 0x38;
/// Heartbeat period: `now - last_heartbeat >= 1000000` in `rid_mavlink_tx.c`.
pub const HEARTBEAT_PERIOD_US: u64 = 1_000_000;
/// Operator-location republish period: `>= 6000000`.
pub const SYSTEM_PERIOD_US: u64 = 6_000_000;
/// Unknown operator altitude, the `op_alt = -1000.0f` default.
pub const OP_ALT_UNKNOWN: f32 = -1000.0;
/// The C firmware sends 1 (MAV_ODID_OPERATOR_LOCATION_TYPE_LIVE_GNSS) for a
/// fresh operator location although its comment claims
/// `MAV_ODID_OPERATOR_LOCATION_TYPE_FIXED` (2); kept bit-exact with the C.
pub const OP_LOC_TYPE_FRESH: u8 = 1;

/// Periodic MAVLink TX writer: sends the HEARTBEAT every second and the
/// OPEN_DRONE_ID_SYSTEM every six seconds through a `UartWrite` sink.
pub struct MavlinkUsbTx<W: UartWrite> {
    transport: W,
    last_heartbeat_us: u64,
    last_system_us: u64,
    tx_seq: u8,
}

impl<W: UartWrite> MavlinkUsbTx<W> {
    /// `rid_mavlink_usb_init()` + the start state of `rid_mavlink_tx_task`
    /// (last sends at 0, TX sequence 0).
    pub fn new(transport: W) -> Self {
        Self {
            transport,
            last_heartbeat_us: 0,
            last_system_us: 0,
            tx_seq: 0,
        }
    }

    /// One pass of the `rid_mavlink_tx_task` loop: HEARTBEAT every
    /// `HEARTBEAT_PERIOD_US`, OPEN_DRONE_ID_SYSTEM every `SYSTEM_PERIOD_US`.
    /// `op_loc` is the fresh MAVLink operator location (port of
    /// `mavlink_parser_get_operator_location()`); when `None` the system
    /// message carries 0/0/-1000 with `op_loc_type == 0`, like the C.
    /// Returns how many frames were accepted by the transport (0/1/2).
    pub fn tick(&mut self, now_us: u64, op_loc: Option<&OperatorLocation>) -> usize {
        let mut wrote = 0;
        let mut buf = [0u8; MAX_FRAME_LEN];

        if now_us.wrapping_sub(self.last_heartbeat_us) >= HEARTBEAT_PERIOD_US {
            self.last_heartbeat_us = now_us;
            let n = pack_heartbeat(&mut buf, self.tx_seq, TX_SYSID, TX_COMPID);
            self.tx_seq = self.tx_seq.wrapping_add(1);
            if self.transport.write(&buf[..n]) == n {
                wrote += 1;
            }
        }

        if now_us.wrapping_sub(self.last_system_us) >= SYSTEM_PERIOD_US {
            self.last_system_us = now_us;
            let (lat, lon, alt, loc_type) = match op_loc {
                Some(loc) => (loc.lat, loc.lon, loc.alt, OP_LOC_TYPE_FRESH),
                None => (0.0, 0.0, OP_ALT_UNKNOWN, 0),
            };
            let n = pack_open_drone_id_system(
                &mut buf, self.tx_seq, TX_SYSID, TX_COMPID, lat, lon, alt, loc_type,
            );
            self.tx_seq = self.tx_seq.wrapping_add(1);
            if self.transport.write(&buf[..n]) == n {
                wrote += 1;
            }
        }

        wrote
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::vec::Vec;

    #[derive(Clone)]
    struct MockWriter(Rc<RefCell<Vec<u8>>>);

    impl MockWriter {
        fn new() -> Self {
            Self(Rc::new(RefCell::new(Vec::new())))
        }
        fn bytes(&self) -> Vec<u8> {
            self.0.borrow().clone()
        }
        fn clear(&self) {
            self.0.borrow_mut().clear();
        }
    }

    impl UartWrite for MockWriter {
        fn write(&mut self, buf: &[u8]) -> usize {
            self.0.borrow_mut().extend_from_slice(buf);
            buf.len()
        }
    }

    #[derive(Clone)]
    struct FailingWriter;

    impl UartWrite for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> usize {
            0
        }
    }

    fn op_loc() -> OperatorLocation {
        OperatorLocation {
            lat: 45.30405,
            lon: 9.3875,
            alt: 1234.0,
        }
    }

    #[test]
    fn heartbeat_every_second_and_system_every_six() {
        let w = MockWriter::new();
        let mut tx = MavlinkUsbTx::new(w.clone());

        // t = 0: nothing (last = 0, delta < period).
        assert_eq!(tx.tick(0, None), 0);
        assert!(w.bytes().is_empty());

        // t = 1s: first heartbeat, seq 0.
        assert_eq!(tx.tick(1_000_000, None), 1);
        let b = w.bytes();
        assert_eq!(b.len(), 21);
        assert_eq!(b[0], 0xfd);
        assert_eq!(b[4], 0); // seq 0
        assert_eq!(b[7..10], [0, 0, 0]); // msgid HEARTBEAT
        w.clear();

        // t = 2s: heartbeat only, seq 1.
        assert_eq!(tx.tick(2_000_000, None), 1);
        assert_eq!(w.bytes()[4], 1);
        w.clear();

        // t = 6s: heartbeat (seq 2) + system (seq 3, unknown location).
        assert_eq!(tx.tick(6_000_000, None), 2);
        let b = w.bytes();
        assert_eq!(b[4], 2);
        assert_eq!(b.len(), 21 + 37);
        let sys = &b[21..];
        assert_eq!(sys[0], 0xfd);
        assert_eq!(sys[1], 25); // trimmed
        assert_eq!(sys[7..10], [0x68, 0x32, 0]); // msgid 12904
        assert_eq!(sys[4], 3); // seq
        w.clear();
    }

    #[test]
    fn system_reuses_operator_location_when_fresh() {
        let w = MockWriter::new();
        let mut tx = MavlinkUsbTx::new(w.clone());
        tx.tick(6_000_000, Some(&op_loc()));
        let b = w.bytes();
        let sys = &b[21..];
        assert_eq!(sys[1], 51);
        assert_eq!(sys[10..14], ((45.30405 * 1e7) as i32).to_le_bytes());
        assert_eq!(sys[14..18], ((9.3875 * 1e7) as i32).to_le_bytes());
        assert_eq!(sys[60], OP_LOC_TYPE_FRESH);
    }

    #[test]
    fn no_heartbeat_without_period_elapsed() {
        let w = MockWriter::new();
        let mut tx = MavlinkUsbTx::new(w.clone());
        tx.tick(1_000_000, None);
        assert_eq!(w.bytes().len(), 21);
        assert_eq!(tx.tick(1_500_000, None), 0); // 500 ms later
        assert_eq!(w.bytes().len(), 21);
    }

    #[test]
    fn heartbeat_still_scheduled_when_write_fails() {
        // The C increments the TX sequence in finalize regardless of the
        // write result (`uart_write_bytes` return ignored for scheduling).
        let mut tx = MavlinkUsbTx::new(FailingWriter);
        assert_eq!(tx.tick(1_000_000, None), 0);
        assert_eq!(tx.tick(2_000_000, None), 0); // period still elapsed
    }

    #[test]
    fn sequence_increments_across_messages() {
        let w = MockWriter::new();
        let mut tx = MavlinkUsbTx::new(w.clone());
        tx.tick(1_000_000, None); // heartbeat seq 0
        w.clear();
        tx.tick(6_000_000, None); // heartbeat seq 1 + system seq 2
        let b = w.bytes();
        assert_eq!(b[4], 1);
        assert_eq!(b[21 + 4], 2);
    }
}
