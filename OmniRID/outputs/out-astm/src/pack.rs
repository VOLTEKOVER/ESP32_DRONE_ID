//! WiFi message-pack assembly, port of `odid_message_build_pack` from
//! `wifi.c`. The per-message encoding is done by the official C library
//! through `opendroneid-sys`.

use opendroneid_sys::{
    encode_auth, encode_basic_id, encode_location, encode_operator_id, encode_self_id,
    encode_system, AuthEncoded, BasicIdEncoded, LocationEncoded, MessageEncoded, OperatorIdEncoded,
    SelfIdEncoded, SystemEncoded, UasData, ODID_AUTH_MAX_PAGES, ODID_BASIC_ID_MAX_MESSAGES,
    ODID_MESSAGETYPE_PACKED, ODID_MESSAGE_SIZE, ODID_PACK_MAX_MESSAGES, ODID_PROTOCOL_VERSION,
    ODID_SUCCESS,
};

/// Maximum message-pack size in bytes (3 header bytes + 9 * 25).
pub const MAX_PACK_LEN: usize = 3 + ODID_PACK_MAX_MESSAGES * ODID_MESSAGE_SIZE;

/// `odid_message_build_pack` return codes (< 0 in C).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PackError {
    /// More valid messages than `ODID_PACK_MAX_MESSAGES` (-EINVAL).
    TooManyMessages,
    /// No valid message at all (-EINVAL).
    NoMessages,
    /// Output buffer too small (-ENOMEM).
    BufferTooSmall,
}

/// Assembles the ASTM message pack exactly like `odid_message_build_pack`:
/// messages are appended in order (BasicID x2, Location, Auth, SelfID, System,
/// OperatorID), a message that fails to encode is skipped, and the pack is
/// rejected when the count exceeds `ODID_PACK_MAX_MESSAGES`.
pub fn build_pack(uas: &UasData, buf: &mut [u8]) -> Result<usize, PackError> {
    let mut msgs = [MessageEncoded([0; ODID_MESSAGE_SIZE]); ODID_PACK_MAX_MESSAGES];
    let mut n = 0usize;

    for i in 0..ODID_BASIC_ID_MAX_MESSAGES {
        if uas.basic_id_valid[i] != 0 {
            if n >= ODID_PACK_MAX_MESSAGES {
                return Err(PackError::TooManyMessages);
            }
            let mut enc = BasicIdEncoded([0; ODID_MESSAGE_SIZE]);
            if encode_basic_id(&mut enc, &uas.basic_id[i]) == ODID_SUCCESS {
                msgs[n] = enc.into();
                n += 1;
            }
        }
    }

    if uas.location_valid != 0 {
        if n >= ODID_PACK_MAX_MESSAGES {
            return Err(PackError::TooManyMessages);
        }
        let mut enc = LocationEncoded([0; ODID_MESSAGE_SIZE]);
        if encode_location(&mut enc, &uas.location) == ODID_SUCCESS {
            msgs[n] = enc.into();
            n += 1;
        }
    }

    for i in 0..ODID_AUTH_MAX_PAGES {
        if uas.auth_valid[i] != 0 {
            if n >= ODID_PACK_MAX_MESSAGES {
                return Err(PackError::TooManyMessages);
            }
            let mut enc = AuthEncoded([0; ODID_MESSAGE_SIZE]);
            if encode_auth(&mut enc, &uas.auth[i]) == ODID_SUCCESS {
                msgs[n] = enc.into();
                n += 1;
            }
        }
    }

    if uas.self_id_valid != 0 {
        if n >= ODID_PACK_MAX_MESSAGES {
            return Err(PackError::TooManyMessages);
        }
        let mut enc = SelfIdEncoded([0; ODID_MESSAGE_SIZE]);
        if encode_self_id(&mut enc, &uas.self_id) == ODID_SUCCESS {
            msgs[n] = enc.into();
            n += 1;
        }
    }

    if uas.system_valid != 0 {
        if n >= ODID_PACK_MAX_MESSAGES {
            return Err(PackError::TooManyMessages);
        }
        let mut enc = SystemEncoded([0; ODID_MESSAGE_SIZE]);
        if encode_system(&mut enc, &uas.system) == ODID_SUCCESS {
            msgs[n] = enc.into();
            n += 1;
        }
    }

    if uas.operator_id_valid != 0 {
        if n >= ODID_PACK_MAX_MESSAGES {
            return Err(PackError::TooManyMessages);
        }
        let mut enc = OperatorIdEncoded([0; ODID_MESSAGE_SIZE]);
        if encode_operator_id(&mut enc, &uas.operator_id) == ODID_SUCCESS {
            msgs[n] = enc.into();
            n += 1;
        }
    }

    if n == 0 {
        return Err(PackError::NoMessages);
    }

    let len = 3 + n * ODID_MESSAGE_SIZE;
    if len > buf.len() {
        return Err(PackError::BufferTooSmall);
    }

    buf[0] = ((ODID_MESSAGETYPE_PACKED as u8) << 4) | ODID_PROTOCOL_VERSION;
    buf[1] = ODID_MESSAGE_SIZE as u8;
    buf[2] = n as u8;
    for i in 0..n {
        buf[3 + i * ODID_MESSAGE_SIZE..3 + (i + 1) * ODID_MESSAGE_SIZE].copy_from_slice(&msgs[i].0);
    }
    Ok(len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ffi::c_char;
    use opendroneid_sys::{
        decode_message_type, ODID_MESSAGETYPE_BASIC_ID, ODID_MESSAGETYPE_LOCATION,
        ODID_MESSAGETYPE_OPERATOR_ID,
    };

    /// ASCII comparison of a C `char` array against a byte literal.
    fn eq_chars(dst: &[c_char], src: &[u8]) -> bool {
        dst.iter().zip(src).all(|(&a, &b)| a as u8 == b)
    }

    /// Copies ASCII bytes into a C `char` array.
    fn copy_chars(dst: &mut [c_char], src: &[u8]) {
        for (d, s) in dst.iter_mut().zip(src) {
            *d = *s as c_char;
        }
    }

    /// A full UAS snapshot: 2 basic IDs + location + self + system + operator.
    fn full_uas() -> UasData {
        let mut d = opendroneid_sys::init_uas_data();
        d.basic_id_valid[0] = 1;
        d.basic_id[0].id_type = opendroneid_sys::ODID_IDTYPE_SERIAL_NUMBER;
        d.basic_id[0].ua_type = opendroneid_sys::ODID_UATYPE_HELICOPTER_OR_MULTIROTOR;
        let s = b"ESP32-RID-001";
        copy_chars(&mut d.basic_id[0].uas_id[..s.len()], s);

        d.basic_id_valid[1] = 1;
        d.basic_id[1].id_type = opendroneid_sys::ODID_IDTYPE_CAA_REGISTRATION_ID;
        d.basic_id[1].ua_type = opendroneid_sys::ODID_UATYPE_AEROPLANE;
        let s2 = b"IT-REG-000001";
        copy_chars(&mut d.basic_id[1].uas_id[..s2.len()], s2);

        d.location_valid = 1;
        d.location.status = opendroneid_sys::ODID_STATUS_AIRBORNE;
        d.location.latitude = 45.30405;
        d.location.longitude = 11.95375;
        d.location.altitude_geo = 123.4;
        d.location.altitude_baro = 122.0;
        d.location.height = 60.0;
        d.location.height_type = opendroneid_sys::ODID_HEIGHT_REF_OVER_TAKEOFF;
        d.location.direction = 90.0;
        d.location.speed_horizontal = 12.0;
        d.location.speed_vertical = 0.0;
        d.location.horiz_accuracy = opendroneid_sys::ODID_HOR_ACC_3_METER;
        d.location.vert_accuracy = opendroneid_sys::ODID_VER_ACC_3_METER;

        d.self_id_valid = 1;
        d.self_id.desc_type = opendroneid_sys::ODID_DESC_TYPE_TEXT;
        let sd = b"DEMO";
        copy_chars(&mut d.self_id.desc[..sd.len()], sd);

        d.system_valid = 1;
        d.system.operator_location_type = opendroneid_sys::ODID_OPERATOR_LOCATION_TYPE_TAKEOFF;
        d.system.operator_latitude = 45.30;
        d.system.operator_longitude = 11.95;
        d.system.operator_altitude_geo = 110.0;
        d.system.area_count = 0;
        d.system.area_radius = 0;

        d.operator_id_valid = 1;
        d.operator_id.operator_id_type = opendroneid_sys::ODID_OPERATOR_ID;
        let od = b"OP-123456";
        copy_chars(&mut d.operator_id.operator_id[..od.len()], od);

        d
    }

    #[test]
    fn pack_header_and_message_order() {
        let uas = full_uas();
        let mut buf = [0u8; MAX_PACK_LEN];
        let len = build_pack(&uas, &mut buf).unwrap();
        assert_eq!(len, 3 + 6 * ODID_MESSAGE_SIZE);
        assert_eq!(buf[0], 0xF2, "PACKED (0xF) | proto 2");
        assert_eq!(buf[1], ODID_MESSAGE_SIZE as u8);
        assert_eq!(buf[2], 6);
        assert_eq!(decode_message_type(buf[3]), ODID_MESSAGETYPE_BASIC_ID);
        assert_eq!(
            decode_message_type(buf[3 + ODID_MESSAGE_SIZE]),
            ODID_MESSAGETYPE_BASIC_ID
        );
        assert_eq!(
            decode_message_type(buf[3 + 2 * ODID_MESSAGE_SIZE]),
            ODID_MESSAGETYPE_LOCATION
        );
        assert_eq!(
            decode_message_type(buf[3 + 5 * ODID_MESSAGE_SIZE]),
            ODID_MESSAGETYPE_OPERATOR_ID
        );
    }

    #[test]
    fn pack_roundtrip_through_c_decoder() {
        let uas = full_uas();
        let mut buf = [0u8; MAX_PACK_LEN];
        let len = build_pack(&uas, &mut buf).unwrap();
        assert_eq!(len, 3 + 6 * ODID_MESSAGE_SIZE);

        // Mirror the pack buffer into a MessagePackEncoded (same byte layout)
        // and decode with the official C library. This is what the C
        // odid_message_process_pack (wifi.c) does.
        let mut enc = opendroneid_sys::MessagePackEncoded {
            proto_version_message_type: buf[0],
            single_message_size: buf[1],
            msg_pack_size: buf[2],
            messages: [MessageEncoded([0; ODID_MESSAGE_SIZE]); ODID_PACK_MAX_MESSAGES],
        };
        for i in 0..ODID_PACK_MAX_MESSAGES {
            let start = 3 + i * ODID_MESSAGE_SIZE;
            enc.messages[i]
                .0
                .copy_from_slice(&buf[start..start + ODID_MESSAGE_SIZE]);
        }

        let mut back = opendroneid_sys::init_uas_data();
        let ret = unsafe { opendroneid_sys::decodeMessagePack(&mut back, &enc) };
        assert_eq!(ret, opendroneid_sys::ODID_SUCCESS);
        assert_eq!(back.basic_id_valid[0], 1);
        assert_eq!(
            back.basic_id[0].id_type,
            opendroneid_sys::ODID_IDTYPE_SERIAL_NUMBER
        );
        assert!(eq_chars(&back.basic_id[0].uas_id, b"ESP32-RID-001"));
        assert_eq!(back.basic_id_valid[1], 1);
        assert_eq!(back.location_valid, 1);
        assert_eq!(back.location.latitude, 45.30405);
        assert_eq!(back.location.longitude, 11.95375);
        // AltitudeGeo roundtrips through the 0.5 m quantization: 2247 * 0.5 - 1000.
        assert_eq!(back.location.altitude_geo, 123.5);
        assert_eq!(back.location.altitude_baro, 122.0);
        assert_eq!(back.location.height, 60.0);
        assert_eq!(back.location.direction, 90.0);
        assert_eq!(back.location.speed_horizontal, 12.0);
        assert_eq!(back.self_id_valid, 1);
        assert!(eq_chars(&back.self_id.desc, b"DEMO"));
        assert_eq!(back.system_valid, 1);
        assert_eq!(back.system.operator_latitude, 45.30);
        assert_eq!(back.operator_id_valid, 1);
        assert!(eq_chars(&back.operator_id.operator_id, b"OP-123456"));
    }

    #[test]
    fn pack_location_message_bytes_are_exact() {
        // Lock the Location encoding: verify the packed bytes against the
        // quantization formulas of the official encoder.
        let mut uas = opendroneid_sys::init_uas_data();
        uas.location_valid = 1;
        uas.location.status = opendroneid_sys::ODID_STATUS_AIRBORNE;
        uas.location.latitude = 45.30405;
        uas.location.longitude = 11.95375;
        uas.location.altitude_geo = 123.4;
        uas.location.altitude_baro = 122.0;
        uas.location.height = 60.0;
        uas.location.direction = 90.0;
        uas.location.speed_horizontal = 12.0;
        uas.location.speed_vertical = 0.0;
        uas.location.horiz_accuracy = opendroneid_sys::ODID_HOR_ACC_3_METER;
        uas.location.vert_accuracy = opendroneid_sys::ODID_VER_ACC_3_METER;

        let mut buf = [0u8; MAX_PACK_LEN];
        let len = build_pack(&uas, &mut buf).unwrap();
        assert_eq!(len, 3 + ODID_MESSAGE_SIZE);
        let m = &buf[3..3 + ODID_MESSAGE_SIZE];

        assert_eq!(m[0], 0x12, "LOCATION | proto 2");
        // Byte 1: Status (hi nibble) | Reserved | HeightType | EWDirection |
        // SpeedMult (lo nibble): AIRBORNE(2)<<4 -> 0x20.
        assert_eq!(m[1], 0x20, "Status airborne (hi nibble)");
        assert_eq!(m[2], 90, "Direction 90, E/W=0");
        assert_eq!(m[3], 48, "SpeedH 12 m/s / 0.25");
        assert_eq!(m[4], 0, "SpeedV 0");
        // Latitude = round(45.30405 * 1e7) = 453040500 (LE)
        assert_eq!(
            i32::from_le_bytes([m[5], m[6], m[7], m[8]]),
            (45.30405_f64 * 10_000_000.0).round() as i32
        );
        // Longitude = round(11.95375 * 1e7) = 119537500 (LE)
        assert_eq!(
            i32::from_le_bytes([m[9], m[10], m[11], m[12]]),
            (11.95375_f64 * 10_000_000.0).round() as i32
        );
        // AltitudeBaro = round((122.0 + 1000) / 0.5) = 2244
        assert_eq!(u16::from_le_bytes([m[13], m[14]]), 2244);
        // AltitudeGeo = round((123.4 + 1000) / 0.5) = 2247
        assert_eq!(u16::from_le_bytes([m[15], m[16]]), 2247);
        // Height = round((60.0 + 1000) / 0.5) = 2120
        assert_eq!(u16::from_le_bytes([m[17], m[18]]), 2120);
        // Byte 19: HorizAccuracy (lo) | VertAccuracy (hi): 11 | 5 -> 0x5B
        assert_eq!(m[19], 0x5B);
        assert_eq!(m[24], 0, "reserved3");
    }

    #[test]
    fn pack_rejects_no_messages() {
        let uas = opendroneid_sys::init_uas_data();
        let mut buf = [0u8; MAX_PACK_LEN];
        assert_eq!(build_pack(&uas, &mut buf), Err(PackError::NoMessages));
    }

    #[test]
    fn pack_rejects_small_buffer() {
        let uas = full_uas();
        let mut buf = [0u8; 3];
        assert_eq!(build_pack(&uas, &mut buf), Err(PackError::BufferTooSmall));
    }

    #[test]
    fn pack_skips_unencodable_message() {
        // A location with an out-of-range latitude fails to encode and is
        // skipped, leaving only the basic ID.
        let mut uas = opendroneid_sys::init_uas_data();
        uas.basic_id_valid[0] = 1;
        uas.basic_id[0].id_type = opendroneid_sys::ODID_IDTYPE_SERIAL_NUMBER;
        uas.basic_id[0].ua_type = opendroneid_sys::ODID_UATYPE_AEROPLANE;
        let s = b"ESP32-RID-001";
        copy_chars(&mut uas.basic_id[0].uas_id[..s.len()], s);
        uas.location_valid = 1;
        uas.location.latitude = 99.0; // > MAX_LAT
        uas.location.longitude = 0.0;
        uas.location.altitude_geo = 0.0;
        uas.location.altitude_baro = 0.0;
        uas.location.height = 0.0;

        let mut buf = [0u8; MAX_PACK_LEN];
        let len = build_pack(&uas, &mut buf).unwrap();
        assert_eq!(len, 3 + ODID_MESSAGE_SIZE);
        assert_eq!(decode_message_type(buf[3]), ODID_MESSAGETYPE_BASIC_ID);
    }
}
