//! BLE 4.x legacy advertisement framing, port of the framing part of
//! `build_legacy_adv` from `ble_tx.c`.
//!
//! Legacy advertising data is limited to 31 bytes, so exactly one 25-byte ODID
//! message fits per advertisement, sent as a Service Data AD structure on the
//! Remote ID UUID 0xFFFA:
//!
//! `0x1E 0x16 0xFA 0xFF | 0x0D (app code) | counter | 25-byte message`
//!
//! Message selection/rotation is handled by `out_astm::ble4::next_message`;
//! this module only wraps the selected message into the 31-byte AD structure.

use opendroneid_sys::{UasData, ODID_MESSAGE_SIZE};
use out_astm::ble4::next_message;

/// Legacy advertisement length in bytes.
pub const LEGACY_ADV_LEN: usize = 31;

/// Header bytes, as written by the C:
/// `0x1E` length, `0x16` Service Data 16-bit UUID, `0xFA 0xFF` UUID 0xFFFA LE,
/// `0x0D` ASTM Open Drone ID application code.
const AD_HEADER: [u8; 6] = [0x1E, 0x16, 0xFA, 0xFF, 0x0D, 0x00];

/// Rotates one valid message out of `uas` and writes the 31-byte legacy
/// advertisement into `buf`. The counter byte matches the C: it is the
/// pre-increment rotation value (the C writes `rotation - 1` after the
/// post-increment, wrapping in u8).
///
/// Returns `false` when no message is valid or the selected message fails to
/// encode, mirroring `build_legacy_adv`.
pub fn build_legacy_adv(uas: &UasData, rotation: &mut u8, buf: &mut [u8; LEGACY_ADV_LEN]) -> bool {
    let counter = *rotation;
    let Some(msg) = next_message(uas, rotation) else {
        return false;
    };

    buf[..6].copy_from_slice(&AD_HEADER);
    buf[5] = counter;
    buf[6..LEGACY_ADV_LEN].copy_from_slice(&msg.0[..ODID_MESSAGE_SIZE]);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ffi::c_char;
    use opendroneid_sys::decode_message_type;

    fn copy_chars(dst: &mut [c_char], src: &[u8]) {
        for (d, s) in dst.iter_mut().zip(src) {
            *d = *s as c_char;
        }
    }

    fn uas_with_two_messages() -> UasData {
        let mut d = opendroneid_sys::init_uas_data();
        d.basic_id_valid[0] = 1;
        d.basic_id[0].id_type = opendroneid_sys::ODID_IDTYPE_SERIAL_NUMBER;
        d.basic_id[0].ua_type = opendroneid_sys::ODID_UATYPE_AEROPLANE;
        copy_chars(&mut d.basic_id[0].uas_id[..3], b"ID1");
        d.location_valid = 1;
        d.location.latitude = 45.30405;
        d.location.longitude = 11.95375;
        d
    }

    #[test]
    fn legacy_adv_layout() {
        let d = uas_with_two_messages();
        let mut rotation = 0u8;
        let mut buf = [0u8; LEGACY_ADV_LEN];
        assert!(build_legacy_adv(&d, &mut rotation, &mut buf));

        // AD header: 0x1E length, 0x16 Service Data, UUID 0xFFFA LE, app code.
        assert_eq!(&buf[..2], &[0x1E, 0x16]);
        assert_eq!(&buf[2..4], &[0xFA, 0xFF]);
        assert_eq!(buf[4], 0x0D);
        // Counter = pre-increment rotation (0 for the first frame).
        assert_eq!(buf[5], 0);
        // The message itself is the first rotated message: a Basic ID.
        assert_eq!(
            decode_message_type(buf[6]),
            opendroneid_sys::ODID_MESSAGETYPE_BASIC_ID
        );
    }

    #[test]
    fn counter_advances_and_wraps() {
        let d = uas_with_two_messages();
        let mut rotation = 0u8;
        let mut buf = [0u8; LEGACY_ADV_LEN];
        assert!(build_legacy_adv(&d, &mut rotation, &mut buf));
        assert_eq!(buf[5], 0);
        assert!(build_legacy_adv(&d, &mut rotation, &mut buf));
        assert_eq!(buf[5], 1);
        // Second message in the rotation is the Location.
        assert_eq!(
            decode_message_type(buf[6]),
            opendroneid_sys::ODID_MESSAGETYPE_LOCATION
        );
        assert!(build_legacy_adv(&d, &mut rotation, &mut buf));
        assert_eq!(buf[5], 2);
    }

    #[test]
    fn counter_is_pre_increment_not_modulo() {
        let d = uas_with_two_messages();
        let mut rotation = 0u8;
        let mut buf = [0u8; LEGACY_ADV_LEN];
        // Advance 253 frames so rotation is 253 pre-increment.
        for _ in 0..253 {
            assert!(build_legacy_adv(&d, &mut rotation, &mut buf));
        }
        // rotation == 253 now; the frame counter is 253 (raw, before % total).
        assert!(build_legacy_adv(&d, &mut rotation, &mut buf));
        assert_eq!(buf[5], 253);
    }

    #[test]
    fn no_valid_messages_returns_false() {
        let d = opendroneid_sys::init_uas_data();
        let mut rotation = 0u8;
        let mut buf = [0u8; LEGACY_ADV_LEN];
        assert!(!build_legacy_adv(&d, &mut rotation, &mut buf));
        assert_eq!(rotation, 0, "rotation unchanged when nothing valid");
    }

    #[test]
    fn message_bytes_match_the_encoded_message() {
        let d = uas_with_two_messages();
        let mut rotation = 0u8;
        let mut buf = [0u8; LEGACY_ADV_LEN];
        assert!(build_legacy_adv(&d, &mut rotation, &mut buf));

        // The same rotation produced a message via next_message: bytes 6..31
        // must equal its raw data.
        let mut rotation2 = 0u8;
        let msg = next_message(&d, &mut rotation2).unwrap();
        assert_eq!(buf[6..LEGACY_ADV_LEN], msg.0[..ODID_MESSAGE_SIZE]);
    }
}
