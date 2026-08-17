//! BLE 4.0 legacy message rotation, port of the encode part of
//! `build_legacy_adv` from `ble_tx.c`.
//!
//! One 25-byte ODID message fits per 31-byte advertisement, so the valid
//! messages are rotated across advertising cycles. The 31-byte Service Data AD
//! framing (`0x1E 0x16 0xFA 0xFF 0x0D <counter> <message>`) is transport-side
//! and lives in the BSP.

use opendroneid_sys::{
    encode_auth, encode_basic_id, encode_location, encode_operator_id, encode_self_id,
    encode_system, AuthEncoded, BasicIdEncoded, LocationEncoded, MessageEncoded,
    OperatorIdEncoded, SelfIdEncoded, SystemEncoded, UasData, ODID_AUTH_MAX_PAGES,
    ODID_BASIC_ID_MAX_MESSAGES, ODID_MESSAGE_SIZE, ODID_SUCCESS,
};

/// Number of currently valid messages, counted in the same order as the C loop.
pub fn count_valid(uas: &UasData) -> u8 {
    let mut total = 0u8;
    for i in 0..ODID_BASIC_ID_MAX_MESSAGES {
        if uas.basic_id_valid[i] != 0 {
            total += 1;
        }
    }
    if uas.location_valid != 0 {
        total += 1;
    }
    for i in 0..ODID_AUTH_MAX_PAGES {
        if uas.auth_valid[i] != 0 {
            total += 1;
        }
    }
    if uas.self_id_valid != 0 {
        total += 1;
    }
    if uas.system_valid != 0 {
        total += 1;
    }
    if uas.operator_id_valid != 0 {
        total += 1;
    }
    total
}

/// Encodes the message selected by the rotation counter (the C static
/// `rotation++ % total`). Returns `None` when nothing is valid or the selected
/// message fails to encode.
pub fn next_message(uas: &UasData, rotation: &mut u8) -> Option<MessageEncoded> {
    let total = count_valid(uas);
    if total == 0 {
        return None;
    }

    let target = *rotation;
    *rotation = rotation.wrapping_add(1);
    let target = target % total;

    let mut n = 0u8;
    let mut msg = MessageEncoded([0; ODID_MESSAGE_SIZE]);
    let mut found = false;

    for i in 0..ODID_BASIC_ID_MAX_MESSAGES {
        if uas.basic_id_valid[i] == 0 {
            continue;
        }
        if n == target {
            let mut enc = BasicIdEncoded([0; ODID_MESSAGE_SIZE]);
            found = encode_basic_id(&mut enc, &uas.basic_id[i]) == ODID_SUCCESS;
            if found {
                msg = enc.into();
            }
            break;
        }
        n += 1;
    }

    if !found && uas.location_valid != 0 {
        if n == target {
            let mut enc = LocationEncoded([0; ODID_MESSAGE_SIZE]);
            found = encode_location(&mut enc, &uas.location) == ODID_SUCCESS;
            if found {
                msg = enc.into();
            }
        }
        n += 1;
    }

    for i in 0..ODID_AUTH_MAX_PAGES {
        if found || uas.auth_valid[i] == 0 {
            continue;
        }
        if n == target {
            let mut enc = AuthEncoded([0; ODID_MESSAGE_SIZE]);
            found = encode_auth(&mut enc, &uas.auth[i]) == ODID_SUCCESS;
            if found {
                msg = enc.into();
            }
            break;
        }
        n += 1;
    }

    if !found && uas.self_id_valid != 0 {
        if n == target {
            let mut enc = SelfIdEncoded([0; ODID_MESSAGE_SIZE]);
            found = encode_self_id(&mut enc, &uas.self_id) == ODID_SUCCESS;
            if found {
                msg = enc.into();
            }
        }
        n += 1;
    }

    if !found && uas.system_valid != 0 {
        if n == target {
            let mut enc = SystemEncoded([0; ODID_MESSAGE_SIZE]);
            found = encode_system(&mut enc, &uas.system) == ODID_SUCCESS;
            if found {
                msg = enc.into();
            }
        }
        n += 1;
    }

    if !found && uas.operator_id_valid != 0 && n == target {
        let mut enc = OperatorIdEncoded([0; ODID_MESSAGE_SIZE]);
        found = encode_operator_id(&mut enc, &uas.operator_id) == ODID_SUCCESS;
        if found {
            msg = enc.into();
        }
    }

    if !found {
        None
    } else {
        Some(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ffi::c_char;
    use opendroneid_sys::decode_message_type;

    /// Copies ASCII bytes into a C `char` array.
    fn copy_chars(dst: &mut [c_char], src: &[u8]) {
        for (d, s) in dst.iter_mut().zip(src) {
            *d = *s as c_char;
        }
    }

    fn uas_with_three_messages() -> UasData {
        let mut d = opendroneid_sys::init_uas_data();
        d.basic_id_valid[0] = 1;
        d.basic_id[0].id_type = opendroneid_sys::ODID_IDTYPE_SERIAL_NUMBER;
        d.basic_id[0].ua_type = opendroneid_sys::ODID_UATYPE_AEROPLANE;
        let s = b"ESP32-RID-001";
        copy_chars(&mut d.basic_id[0].uas_id[..s.len()], s);

        d.location_valid = 1;
        d.location.latitude = 45.30405;
        d.location.longitude = 11.95375;
        d.location.altitude_geo = 100.0;
        d.location.altitude_baro = 100.0;
        d.location.height = 50.0;
        d.location.direction = 0.0;
        d.location.speed_horizontal = 0.0;
        d.location.speed_vertical = 0.0;

        d.system_valid = 1;
        d.system.operator_latitude = 45.30;
        d.system.operator_longitude = 11.95;
        d.system.operator_altitude_geo = 90.0;
        d
    }

    #[test]
    fn count_valid_messages() {
        let d = uas_with_three_messages();
        assert_eq!(count_valid(&d), 3);
        let mut e = opendroneid_sys::init_uas_data();
        assert_eq!(count_valid(&e), 0);
        e.basic_id_valid[1] = 1;
        assert_eq!(count_valid(&e), 1);
    }

    #[test]
    fn rotation_cycles_through_all_messages() {
        let d = uas_with_three_messages();
        let mut rotation = 0u8;
        let m0 = next_message(&d, &mut rotation).unwrap();
        let m1 = next_message(&d, &mut rotation).unwrap();
        let m2 = next_message(&d, &mut rotation).unwrap();
        let m3 = next_message(&d, &mut rotation).unwrap();
        assert_eq!(decode_message_type(m0.0[0]), opendroneid_sys::ODID_MESSAGETYPE_BASIC_ID);
        assert_eq!(decode_message_type(m1.0[0]), opendroneid_sys::ODID_MESSAGETYPE_LOCATION);
        assert_eq!(decode_message_type(m2.0[0]), opendroneid_sys::ODID_MESSAGETYPE_SYSTEM);
        // Wraps back to the first message.
        assert_eq!(m3.0, m0.0);
        assert_eq!(rotation, 4);
    }

    #[test]
    fn rotation_wraps_u8() {
        let d = uas_with_three_messages();
        let mut rotation = 0u8;
        // Advance to 255.
        for _ in 0..255 {
            let _ = next_message(&d, &mut rotation).unwrap();
        }
        assert_eq!(rotation, 255);
        // 255 % 3 = 0 -> basic id again.
        let m = next_message(&d, &mut rotation).unwrap();
        assert_eq!(rotation, 0, "u8 wraps around");
        assert_eq!(decode_message_type(m.0[0]), opendroneid_sys::ODID_MESSAGETYPE_BASIC_ID);
    }

    #[test]
    fn no_messages_returns_none() {
        let d = opendroneid_sys::init_uas_data();
        let mut rotation = 0u8;
        assert_eq!(next_message(&d, &mut rotation), None);
        assert_eq!(rotation, 0, "rotation unchanged when nothing valid");
    }

    #[test]
    fn second_basic_id_counts_as_separate_slot() {
        let mut d = opendroneid_sys::init_uas_data();
        d.basic_id_valid[0] = 1;
        d.basic_id[0].id_type = opendroneid_sys::ODID_IDTYPE_SERIAL_NUMBER;
        d.basic_id[0].ua_type = opendroneid_sys::ODID_UATYPE_AEROPLANE;
        copy_chars(&mut d.basic_id[0].uas_id[..3], b"ID1");
        d.basic_id_valid[1] = 1;
        d.basic_id[1].id_type = opendroneid_sys::ODID_IDTYPE_SERIAL_NUMBER;
        d.basic_id[1].ua_type = opendroneid_sys::ODID_UATYPE_AEROPLANE;
        copy_chars(&mut d.basic_id[1].uas_id[..3], b"ID2");
        assert_eq!(count_valid(&d), 2);

        let mut rotation = 0u8;
        let m0 = next_message(&d, &mut rotation).unwrap();
        let m1 = next_message(&d, &mut rotation).unwrap();
        // Both are basic IDs but carry different UASIDs.
        assert_ne!(m0.0[2..5], m1.0[2..5]);
        assert_eq!(&m0.0[2..5], b"ID1");
        assert_eq!(&m1.0[2..5], b"ID2");
    }
}

