//! MAVLink v2 frame packing (port of the packing side of `mavlink_helpers.h`
//! and the `mavlink_msg_*_pack` / `mavlink_msg_to_send_buffer` helpers used
//! by `rid_mavlink_tx.c`).
//!
//! The firmware uses the MAVLink v2 dialect (`MAVLINK_VERSION == 3`,
//! `MAVLINK_STX == 0xFD`), so frames are emitted byte-exact as v2. CRC-16 is
//! the MAVLink X.25 (MCRF4XX) checksum from `checksum.h`.

/// `MAVLINK_STX` (MAVLink 2).
pub const STX: u8 = 0xfd;
/// `X25_INIT_CRC` from `checksum.h`.
pub const X25_INIT_CRC: u16 = 0xffff;
/// `MAVLINK_MSG_ID_HEARTBEAT_CRC`.
pub const HEARTBEAT_CRC_EXTRA: u8 = 50;
/// `MAVLINK_MSG_ID_12904_CRC` (OPEN_DRONE_ID_SYSTEM).
pub const SYSTEM_CRC_EXTRA: u8 = 77;
/// `MAVLINK_MSG_ID_HEARTBEAT`.
pub const MSG_ID_HEARTBEAT: u32 = 0;
/// `MAVLINK_MSG_ID_OPEN_DRONE_ID_SYSTEM`.
pub const MSG_ID_OPEN_DRONE_ID_SYSTEM: u32 = 12904;
/// `MAVLINK_MSG_ID_HEARTBEAT_LEN`.
pub const HEARTBEAT_LEN: usize = 9;
/// `MAVLINK_MSG_ID_OPEN_DRONE_ID_SYSTEM_LEN`.
pub const SYSTEM_LEN: usize = 54;
/// `MAV_TYPE_ODID` from `minimal.h`.
pub const MAV_TYPE_ODID: u8 = 34;
/// `MAV_AUTOPILOT_INVALID` from `minimal.h`.
pub const MAV_AUTOPILOT_INVALID: u8 = 8;
/// `MAV_MODE_FLAG_CUSTOM_MODE_ENABLED` from `minimal.h`.
pub const MAV_MODE_FLAG_CUSTOM_MODE_ENABLED: u8 = 1;
/// `MAV_STATE_ACTIVE` from `minimal.h`.
pub const MAV_STATE_ACTIVE: u8 = 4;

/// Largest frame this crate packs: 10-byte v2 header + 54-byte payload +
/// 2-byte checksum.
pub const MAX_FRAME_LEN: usize = 10 + SYSTEM_LEN + 2;

/// `crc_accumulate()` from `checksum.h` (CRC-16/MCRF4XX).
pub fn crc_accumulate(data: u8, crc: u16) -> u16 {
    let mut tmp = data ^ (crc & 0xff) as u8;
    tmp = tmp ^ tmp.wrapping_shl(4);
    (crc >> 8) ^ ((tmp as u16) << 8) ^ ((tmp as u16) << 3) ^ ((tmp as u16) >> 4)
}

/// `_mav_trim_payload()` from `mavlink_helpers.h`: strips trailing zero
/// bytes, keeping at least one.
fn trim_payload(payload: &[u8]) -> usize {
    let mut len = payload.len();
    while len > 1 && payload[len - 1] == 0 {
        len -= 1;
    }
    len
}

/// Finalizes a MAVLink v2 frame into `buf`, returning its length. Writes the
/// 10-byte header, the (zero-trimmed) payload and the X.25 checksum computed
/// over the header (from `len`), payload and `crc_extra` — mirroring
/// `mavlink_finalize_message_buffer()` + `mavlink_msg_to_send_buffer()`.
pub fn finalize(
    buf: &mut [u8; MAX_FRAME_LEN],
    seq: u8,
    sysid: u8,
    compid: u8,
    msgid: u32,
    payload: &[u8],
    crc_extra: u8,
) -> usize {
    let len = trim_payload(payload);
    buf[0] = STX;
    buf[1] = len as u8;
    buf[2] = 0; // incompat_flags
    buf[3] = 0; // compat_flags
    buf[4] = seq;
    buf[5] = sysid;
    buf[6] = compid;
    buf[7] = (msgid & 0xff) as u8;
    buf[8] = ((msgid >> 8) & 0xff) as u8;
    buf[9] = ((msgid >> 16) & 0xff) as u8;
    buf[10..10 + len].copy_from_slice(&payload[..len]);

    let mut crc = X25_INIT_CRC;
    for &b in &buf[1..10 + len] {
        crc = crc_accumulate(b, crc);
    }
    crc = crc_accumulate(crc_extra, crc);
    buf[10 + len] = (crc & 0xff) as u8;
    buf[11 + len] = (crc >> 8) as u8;
    10 + len + 2
}

/// Packs a HEARTBEAT with the same arguments as the
/// `mavlink_msg_heartbeat_pack(TX_SYSID, TX_COMPID, ...)` call in
/// `rid_mavlink_tx.c` (MAV_TYPE_ODID, MAV_AUTOPILOT_INVALID,
/// MAV_MODE_FLAG_CUSTOM_MODE_ENABLED, custom_mode 0, MAV_STATE_ACTIVE).
pub fn pack_heartbeat(buf: &mut [u8; MAX_FRAME_LEN], seq: u8, sysid: u8, compid: u8) -> usize {
    let mut payload = [0u8; HEARTBEAT_LEN];
    payload[4] = MAV_TYPE_ODID;
    payload[5] = MAV_AUTOPILOT_INVALID;
    payload[6] = MAV_MODE_FLAG_CUSTOM_MODE_ENABLED;
    payload[7] = MAV_STATE_ACTIVE;
    payload[8] = 3; // mavlink_version
    finalize(
        buf,
        seq,
        sysid,
        compid,
        MSG_ID_HEARTBEAT,
        &payload,
        HEARTBEAT_CRC_EXTRA,
    )
}

/// Packs an OPEN_DRONE_ID_SYSTEM message with the same arguments as the
/// `mavlink_msg_open_drone_id_system_pack(TX_SYSID, TX_COMPID, ...)` call in
/// `rid_mavlink_tx.c`: broadcast targets, all-zero id_or_mac,
/// `area_count == 1`, radius 0, -1000 m area ceiling/floor, EU category/class
/// 0, timestamp 0, and the given operator location in degE7 (0/0 with
/// `op_loc_type == 0` when unknown, like the C).
/// `mavlink_msg_open_drone_id_system_pack()` takes 17 arguments, so this
/// mirror keeps the same shape (kept a flat parameter list on purpose).
#[allow(clippy::too_many_arguments)]
pub fn pack_open_drone_id_system(
    buf: &mut [u8; MAX_FRAME_LEN],
    seq: u8,
    sysid: u8,
    compid: u8,
    op_lat: f64,
    op_lon: f64,
    op_alt: f32,
    op_loc_type: u8,
) -> usize {
    let mut payload = [0u8; SYSTEM_LEN];
    payload[0..4].copy_from_slice(&((op_lat * 1e7) as i32).to_le_bytes());
    payload[4..8].copy_from_slice(&((op_lon * 1e7) as i32).to_le_bytes());
    payload[8..12].copy_from_slice(&(-1000.0f32).to_le_bytes()); // area_ceiling
    payload[12..16].copy_from_slice(&(-1000.0f32).to_le_bytes()); // area_floor
    payload[16..20].copy_from_slice(&op_alt.to_le_bytes()); // operator_altitude_geo
    payload[24..26].copy_from_slice(&1u16.to_le_bytes()); // area_count
    payload[50] = op_loc_type; // operator_location_type
    finalize(
        buf,
        seq,
        sysid,
        compid,
        MSG_ID_OPEN_DRONE_ID_SYSTEM,
        &payload,
        SYSTEM_CRC_EXTRA,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent bitwise CRC-16/MCRF4XX (reflected, poly 0x8408, init
    /// 0xFFFF, no final xor) used to cross-check `crc_accumulate`.
    fn crc_ref(data: &[u8]) -> u16 {
        let mut crc: u16 = 0xffff;
        for &b in data {
            crc ^= b as u16;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0x8408;
                } else {
                    crc >>= 1;
                }
            }
        }
        crc
    }

    /// Recomputes the frame checksum with the reference CRC and compares it
    /// to the two bytes `finalize` wrote.
    fn check_frame_crc(frame: &[u8], crc_extra: u8) {
        let mut crc = crc_ref(&frame[1..frame.len() - 2]);
        crc ^= crc_extra as u16;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0x8408;
            } else {
                crc >>= 1;
            }
        }
        assert_eq!(frame[frame.len() - 2], (crc & 0xff) as u8);
        assert_eq!(frame[frame.len() - 1], (crc >> 8) as u8);
    }

    #[test]
    fn crc_matches_mcrf4xx_check_value() {
        // CRC-16/MCRF4XX check value for "123456789".
        assert_eq!(crc_ref(b"123456789"), 0x6f91);
        let mut crc = 0xffffu16;
        for &b in b"123456789" {
            crc = crc_accumulate(b, crc);
        }
        assert_eq!(crc, 0x6f91);
    }

    #[test]
    fn heartbeat_frame_is_byte_exact() {
        let mut buf = [0u8; MAX_FRAME_LEN];
        let n = pack_heartbeat(&mut buf, 0, 0x41, 0x38);
        assert_eq!(n, 21);
        let f = &buf[..n];
        assert_eq!(
            &f[..19],
            &[
                0xfd, 9, 0, 0, 0, 0x41, 0x38, 0, 0, 0, // header (seq 0)
                0x00, 0x00, 0x00, 0x00, // custom_mode
                0x22, 0x08, 0x01, 0x04, 0x03, // type, autopilot, base_mode, status, version
            ]
        );
        check_frame_crc(f, HEARTBEAT_CRC_EXTRA);
    }

    #[test]
    fn heartbeat_seq_and_ids() {
        let mut buf = [0u8; MAX_FRAME_LEN];
        let n = pack_heartbeat(&mut buf, 7, 1, 2);
        assert_eq!(buf[4], 7);
        assert_eq!(buf[5], 1);
        assert_eq!(buf[6], 2);
        assert_eq!(&buf[7..10], &[0, 0, 0]); // msgid 0
        assert_eq!(n, 21);
        check_frame_crc(&buf[..n], HEARTBEAT_CRC_EXTRA);
    }

    #[test]
    fn system_frame_unknown_location_trims_to_25() {
        // op_loc_type 0: after payload byte 24 (area_count) everything is
        // zero, so _mav_trim_payload keeps len 25.
        let mut buf = [0u8; MAX_FRAME_LEN];
        let n = pack_open_drone_id_system(&mut buf, 0, 0x41, 0x38, 0.0, 0.0, -1000.0, 0);
        assert_eq!(n, 10 + 25 + 2);
        let f = &buf[..n];
        assert_eq!(f[1], 25); // trimmed length
        assert_eq!(&f[7..10], &[0x68, 0x32, 0]); // msgid 12904 LE
        assert_eq!(&f[10..14], &0i32.to_le_bytes()); // operator_latitude
        assert_eq!(&f[14..18], &0i32.to_le_bytes()); // operator_longitude
        assert_eq!(&f[18..22], &(-1000.0f32).to_le_bytes()); // area_ceiling
        assert_eq!(f[34], 1); // area_count low byte (high byte trimmed off)
    }

    #[test]
    fn system_frame_fresh_location_trims_to_51() {
        let mut buf = [0u8; MAX_FRAME_LEN];
        let n = pack_open_drone_id_system(&mut buf, 0, 0x41, 0x38, 45.30405, 9.3875, 1234.0, 1);
        assert_eq!(n, 10 + 51 + 2);
        let f = &buf[..n];
        assert_eq!(f[1], 51); // trimmed at the nonzero location type
        assert_eq!(&f[10..14], &((45.30405 * 1e7) as i32).to_le_bytes());
        assert_eq!(&f[14..18], &((9.3875 * 1e7) as i32).to_le_bytes());
        assert_eq!(&f[26..30], &1234.0f32.to_le_bytes()); // operator_altitude_geo
        assert_eq!(f[60], 1); // operator_location_type (classification_type 0 trimmed off)
        check_frame_crc(f, SYSTEM_CRC_EXTRA);
    }

    #[test]
    fn system_msgid_little_endian() {
        let mut buf = [0u8; MAX_FRAME_LEN];
        let n = pack_open_drone_id_system(&mut buf, 0, 0x41, 0x38, 0.0, 0.0, -1000.0, 0);
        assert_eq!(
            u32::from_le_bytes([buf[7], buf[8], buf[9], 0]),
            MSG_ID_OPEN_DRONE_ID_SYSTEM
        );
        assert!(n > 30);
    }
}
