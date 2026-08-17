//! Pure-Rust MAVLink v1/v2 parser.
//!
//! Port of `mavlink_parser.c` (ESP32_DRONE_REMOTE_ID_Firmware). The framing
//! state machine mirrors the C-MAVLink helpers (`mavlink_helpers.h`,
//! `checksum.h`, `mavlink_types.h`); the per-message handling mirrors the
//! `switch (msg.msgid)` in `mavlink_parser.c`.
//!
//! Only the messages the firmware consumes are in the CRC table; any other
//! message id is treated as unknown (bad CRC, frame discarded), which cannot
//! affect the firmware's outputs. Signed frames are accepted after consuming
//! the 13-byte signature block, exactly like the firmware with `signing` left
//! NULL (`mavlink_signature_check` returns true for a NULL signing state).
//! `MESSAGE_PACK` frames pass the CRC check but are not decoded yet (needs
//! the `opendroneid` C library, planned as `opendroneid-sys`).
//!
//! `no_std`, allocation-free.

use rid_interface::{GpsData, Identity, AUTH_MAX_PAGES, MAX_STR_LEN};

/// `MAVLINK_STX` (v2 magic byte).
pub const STX_V2: u8 = 0xfd;
/// `MAVLINK_STX_MAVLINK1` (v1 magic byte).
pub const STX_V1: u8 = 0xfe;
/// `MAVLINK_IFLAG_SIGNED`: frame carries a 13-byte signature block.
const IFLAG_SIGNED: u8 = 0x01;
/// `MAVLINK_IFLAG_MASK`: mask of all understood incompatibility flags.
const IFLAG_MASK: u8 = 0x01;
/// `MAVLINK_SIGNATURE_BLOCK_LEN`.
const SIGNATURE_BLOCK_LEN: usize = 13;
/// `X25_INIT_CRC`.
const X25_INIT_CRC: u16 = 0xffff;
/// `MAVLINK_MAX_PAYLOAD_LEN`.
const MAX_PAYLOAD_LEN: usize = 255;
/// `MAV_MODE_FLAG_SAFETY_ARMED` (bit 7 of the heartbeat `base_mode`).
const MODE_FLAG_SAFETY_ARMED: u8 = 0x80;
/// `ODID_AUTH_PAGE_NONZERO_DATA_SIZE`: bytes copied from the authentication
/// message (`sizeof(authentication_data)` == 23).
const AUTH_PAGE_DATA_SIZE: usize = 23;

/// `MAVLINK_MSG_ID_HEARTBEAT`.
const MSG_ID_HEARTBEAT: u32 = 0;
/// `MAVLINK_MSG_ID_GPS_RAW_INT`.
const MSG_ID_GPS_RAW_INT: u32 = 24;
/// `MAVLINK_MSG_ID_ATTITUDE`.
const MSG_ID_ATTITUDE: u32 = 30;
/// `MAVLINK_MSG_ID_GLOBAL_POSITION_INT`.
const MSG_ID_GLOBAL_POSITION_INT: u32 = 33;
/// `MAVLINK_MSG_ID_VFR_HUD`.
const MSG_ID_VFR_HUD: u32 = 74;
/// `MAVLINK_MSG_ID_AHRS2`.
const MSG_ID_AHRS2: u32 = 178;
/// `MAVLINK_MSG_ID_OPEN_DRONE_ID_BASIC_ID`.
const MSG_ID_OPEN_DRONE_ID_BASIC_ID: u32 = 12900;
/// `MAVLINK_MSG_ID_OPEN_DRONE_ID_LOCATION`.
const MSG_ID_OPEN_DRONE_ID_LOCATION: u32 = 12901;
/// `MAVLINK_MSG_ID_OPEN_DRONE_ID_AUTHENTICATION`.
const MSG_ID_OPEN_DRONE_ID_AUTHENTICATION: u32 = 12902;
/// `MAVLINK_MSG_ID_OPEN_DRONE_ID_SELF_ID`.
const MSG_ID_OPEN_DRONE_ID_SELF_ID: u32 = 12903;
/// `MAVLINK_MSG_ID_OPEN_DRONE_ID_SYSTEM`.
const MSG_ID_OPEN_DRONE_ID_SYSTEM: u32 = 12904;
/// `MAVLINK_MSG_ID_OPEN_DRONE_ID_OPERATOR_ID`.
const MSG_ID_OPEN_DRONE_ID_OPERATOR_ID: u32 = 12905;
/// `MAVLINK_MSG_ID_OPEN_DRONE_ID_MESSAGE_PACK`.
const MSG_ID_OPEN_DRONE_ID_MESSAGE_PACK: u32 = 12915;

/// Result of feeding one byte into the framing state machine
/// (port of the `MAVLINK_FRAMING_*` returns of `mavlink_parse_char`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Framing {
    Incomplete,
    Ok,
    BadCrc,
    BadSignature,
}

/// Framing state (port of `MAVLINK_PARSE_STATE_*`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ParseState {
    Idle,
    GotStx,
    GotLength,
    GotIncompatFlags,
    GotCompatFlags,
    GotSeq,
    GotSysid,
    GotCompid,
    GotMsgid1,
    GotMsgid2,
    GotMsgid3,
    GotPayload,
    GotCrc1,
    GotBadCrc1,
    SignatureWait,
    SignatureWaitBadCrc,
}

/// One CRC-table entry: `(crc_extra, max_msg_len)`.
/// `crc_extra`/`max_msg_len` come from the `MAVLINK_MSG_ID_*_CRC`/`_LEN`
/// defines of the message headers the firmware includes.
fn msg_entry(msgid: u32) -> Option<(u8, usize)> {
    match msgid {
        MSG_ID_HEARTBEAT => Some((50, 9)),
        MSG_ID_GPS_RAW_INT => Some((24, 52)),
        MSG_ID_ATTITUDE => Some((39, 28)),
        MSG_ID_GLOBAL_POSITION_INT => Some((104, 28)),
        MSG_ID_VFR_HUD => Some((20, 20)),
        MSG_ID_AHRS2 => Some((47, 24)),
        MSG_ID_OPEN_DRONE_ID_BASIC_ID => Some((114, 44)),
        MSG_ID_OPEN_DRONE_ID_LOCATION => Some((254, 59)),
        MSG_ID_OPEN_DRONE_ID_AUTHENTICATION => Some((140, 53)),
        MSG_ID_OPEN_DRONE_ID_SELF_ID => Some((249, 46)),
        MSG_ID_OPEN_DRONE_ID_SYSTEM => Some((77, 54)),
        MSG_ID_OPEN_DRONE_ID_OPERATOR_ID => Some((49, 43)),
        MSG_ID_OPEN_DRONE_ID_MESSAGE_PACK => Some((94, 249)),
        _ => None,
    }
}

/// One byte of the CRC16_MCRF4XX checksum, exactly as `crc_accumulate()` in
/// `checksum.h`.
pub(crate) fn crc_accumulate(data: u8, crc: u16) -> u16 {
    let mut tmp = data ^ (crc & 0xff) as u8;
    tmp = tmp ^ tmp.wrapping_shl(4);
    (crc >> 8) ^ ((tmp as u16) << 8) ^ ((tmp as u16) << 3) ^ ((tmp as u16) >> 4)
}

/// `crc_calculate()` in `checksum.h`: CRC over a buffer starting from
/// `X25_INIT_CRC`. Used by the test frame builders.
#[cfg(test)]
pub(crate) fn crc_calculate(buf: &[u8]) -> u16 {
    let mut crc = X25_INIT_CRC;
    for &b in buf {
        crc = crc_accumulate(b, crc);
    }
    crc
}

/// Payload field readers (the payload is zero-filled to the message's max
/// length on receipt, so reads past the received length yield 0 like the C
/// `mavlink_msg_*_decode` memset path).
fn rd_u8(p: &[u8], o: usize) -> u8 {
    p[o]
}
fn rd_u16(p: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([p[o], p[o + 1]])
}
fn rd_i16(p: &[u8], o: usize) -> i16 {
    i16::from_le_bytes([p[o], p[o + 1]])
}
fn rd_i32(p: &[u8], o: usize) -> i32 {
    i32::from_le_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]])
}
fn rd_f32(p: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]])
}

/// `att.yaw * 180.0f / 3.14159f` in mavlink_parser.c: degrees from radians,
/// truncated to integer (as the C `(int16_t)` cast), normalized to 0..360.
/// The literal matches the C exactly (f32 rounding parity with the firmware).
#[allow(clippy::approx_constant)]
fn heading_from_rad(yaw: f32) -> i16 {
    let mut heading = (yaw * 180.0 / 3.14159) as i16;
    if heading < 0 {
        heading += 360;
    }
    heading
}

/// MAVLink parser: framing state machine + the firmware's message handling.
///
/// `feed` consumes a byte stream (any framing/CRC error resyncs like the C);
/// the `get_*` methods reproduce `mavlink_parser_get`, `get_armed`,
/// `get_sysid`, `get_identity` and `get_operator_location` with their
/// freshness windows. `now_ms` is the monotonic millisecond clock of the
/// current poll (port of `xTaskGetTickCount() * portTICK_PERIOD_MS`).
#[derive(Debug)]
pub struct MavlinkParser {
    parse_state: ParseState,
    in_mavlink1: bool,
    len: u8,
    packet_idx: usize,
    incompat_flags: u8,
    compat_flags: u8,
    seq: u8,
    sysid: u8,
    compid: u8,
    msgid: u32,
    checksum: u16,
    ck: [u8; 2],
    payload: [u8; MAX_PAYLOAD_LEN],
    signature: [u8; SIGNATURE_BLOCK_LEN],
    signature_wait: usize,

    // Extended output state (static globals in the C parser).
    gps: GpsData,
    identity: Identity,
    last_update: u32,
    last_identity_update: u32,
    has_armed: bool,
    armed: bool,
    mav_sysid: u8,
    operator_lat: f64,
    operator_lon: f64,
    operator_alt: f32,
    operator_location_update: u32,
    sysid_filter: u8,
}

impl Default for MavlinkParser {
    fn default() -> Self {
        Self::new()
    }
}

impl MavlinkParser {
    /// `mavlink_parser_init(uart_port)`: reset parser and output state.
    pub fn new() -> Self {
        Self {
            parse_state: ParseState::Idle,
            in_mavlink1: false,
            len: 0,
            packet_idx: 0,
            incompat_flags: 0,
            compat_flags: 0,
            seq: 0,
            sysid: 0,
            compid: 0,
            msgid: 0,
            checksum: X25_INIT_CRC,
            ck: [0; 2],
            payload: [0; MAX_PAYLOAD_LEN],
            signature: [0; SIGNATURE_BLOCK_LEN],
            signature_wait: 0,

            gps: GpsData::default(),
            identity: Identity::default(),
            last_update: 0,
            last_identity_update: 0,
            has_armed: false,
            armed: false,
            mav_sysid: 0,
            operator_lat: 0.0,
            operator_lon: 0.0,
            operator_alt: 0.0,
            operator_location_update: 0,
            sysid_filter: 0,
        }
    }

    /// `mavlink_parser_set_sysid_filter(sysid)`: 0 accepts any system id.
    pub fn set_sysid_filter(&mut self, sysid: u8) {
        self.sysid_filter = sysid;
    }

    /// Feeds a chunk of received bytes. On every completed valid frame the
    /// firmware's message handling runs (with the current `now_ms` used for
    /// the freshness timestamps).
    pub fn feed(&mut self, bytes: &[u8], now_ms: u32) {
        for &c in bytes {
            if self.parse_char(c) {
                self.process(now_ms);
            }
        }
    }

    /// One byte through the parse_char wrapper (port of `mavlink_parse_char`):
    /// bad CRC/signature discards the frame and resyncs, returning false.
    fn parse_char(&mut self, c: u8) -> bool {
        let received = self.frame_char(c);
        if received == Framing::BadCrc || received == Framing::BadSignature {
            // _mav_parse_error(status) + msg_received = INCOMPLETE + IDLE.
            self.parse_state = ParseState::Idle;
            if c == STX_V2 {
                // Re-parse starts at the next (length) byte; len/checksum reset
                // exactly as in mavlink_parse_char().
                self.parse_state = ParseState::GotStx;
                self.len = 0;
                self.checksum = X25_INIT_CRC;
            }
            return false;
        }
        received == Framing::Ok
    }

    /// One byte through the framing state machine (port of
    /// `mavlink_frame_char_buffer`). Returns the framing result.
    fn frame_char(&mut self, c: u8) -> Framing {
        let st = self.parse_state;
        match st {
            ParseState::Idle => {
                if c == STX_V2 {
                    self.parse_state = ParseState::GotStx;
                    self.len = 0;
                    self.in_mavlink1 = false;
                    self.checksum = X25_INIT_CRC;
                } else if c == STX_V1 {
                    self.parse_state = ParseState::GotStx;
                    self.len = 0;
                    self.in_mavlink1 = true;
                    self.checksum = X25_INIT_CRC;
                }
            }
            ParseState::GotStx => {
                // The C's `status->msg_received` overrun guard and the
                // `MAVLINK_MAX_PAYLOAD_LEN < 255` length cap never trigger
                // here (msg_received is reset to INCOMPLETE at function entry
                // and MAX_PAYLOAD_LEN is 255), so only the accepting branch
                // is ported.
                self.len = c;
                self.packet_idx = 0;
                self.checksum = crc_accumulate(c, self.checksum);
                if self.in_mavlink1 {
                    self.incompat_flags = 0;
                    self.compat_flags = 0;
                    self.parse_state = ParseState::GotCompatFlags;
                } else {
                    self.parse_state = ParseState::GotLength;
                }
            }
            ParseState::GotLength => {
                self.incompat_flags = c;
                if self.incompat_flags & !IFLAG_MASK != 0 {
                    // Unknown incompatibility flag: discard and resync.
                    self.parse_state = ParseState::Idle;
                } else {
                    self.checksum = crc_accumulate(c, self.checksum);
                    self.parse_state = ParseState::GotIncompatFlags;
                }
            }
            ParseState::GotIncompatFlags => {
                self.compat_flags = c;
                self.checksum = crc_accumulate(c, self.checksum);
                self.parse_state = ParseState::GotCompatFlags;
            }
            ParseState::GotCompatFlags => {
                self.seq = c;
                self.checksum = crc_accumulate(c, self.checksum);
                self.parse_state = ParseState::GotSeq;
            }
            ParseState::GotSeq => {
                self.sysid = c;
                self.checksum = crc_accumulate(c, self.checksum);
                self.parse_state = ParseState::GotSysid;
            }
            ParseState::GotSysid => {
                self.compid = c;
                self.checksum = crc_accumulate(c, self.checksum);
                self.parse_state = ParseState::GotCompid;
            }
            ParseState::GotCompid => {
                self.msgid = c as u32;
                self.checksum = crc_accumulate(c, self.checksum);
                if self.in_mavlink1 {
                    if self.len > 0 {
                        self.parse_state = ParseState::GotMsgid3;
                    } else {
                        self.parse_state = ParseState::GotPayload;
                    }
                } else {
                    self.parse_state = ParseState::GotMsgid1;
                }
            }
            ParseState::GotMsgid1 => {
                self.msgid |= (c as u32) << 8;
                self.checksum = crc_accumulate(c, self.checksum);
                self.parse_state = ParseState::GotMsgid2;
            }
            ParseState::GotMsgid2 => {
                self.msgid |= (c as u32) << 16;
                self.checksum = crc_accumulate(c, self.checksum);
                if self.len > 0 {
                    self.parse_state = ParseState::GotMsgid3;
                } else {
                    self.parse_state = ParseState::GotPayload;
                }
            }
            ParseState::GotMsgid3 => {
                self.payload[self.packet_idx] = c;
                self.packet_idx += 1;
                self.checksum = crc_accumulate(c, self.checksum);
                if self.packet_idx == self.len as usize {
                    self.parse_state = ParseState::GotPayload;
                }
            }
            ParseState::GotPayload => {
                if let Some((crc_extra, max_len)) = msg_entry(self.msgid) {
                    self.checksum = crc_accumulate(crc_extra, self.checksum);
                    if c != (self.checksum & 0xff) as u8 {
                        self.parse_state = ParseState::GotBadCrc1;
                    } else {
                        self.parse_state = ParseState::GotCrc1;
                    }
                    self.ck[0] = c;
                    // Zero-fill to cope with short incoming packets.
                    if self.packet_idx < max_len {
                        let p = &mut self.payload[self.packet_idx..max_len];
                        p.fill(0);
                    }
                } else {
                    // Message not in the CRC table: bad CRC path.
                    self.parse_state = ParseState::GotBadCrc1;
                    self.ck[0] = c;
                }
            }
            ParseState::GotCrc1 | ParseState::GotBadCrc1 => {
                let bad_crc = st == ParseState::GotBadCrc1 || (c as u16) != (self.checksum >> 8);
                self.ck[1] = c;
                if self.incompat_flags & IFLAG_SIGNED != 0 {
                    if bad_crc {
                        self.parse_state = ParseState::SignatureWaitBadCrc;
                    } else {
                        self.parse_state = ParseState::SignatureWait;
                    }
                    self.signature_wait = SIGNATURE_BLOCK_LEN;
                    return Framing::Incomplete;
                }
                self.parse_state = ParseState::Idle;
                return if bad_crc { Framing::BadCrc } else { Framing::Ok };
            }
            ParseState::SignatureWait | ParseState::SignatureWaitBadCrc => {
                self.signature[SIGNATURE_BLOCK_LEN - self.signature_wait] = c;
                self.signature_wait -= 1;
                if self.signature_wait == 0 {
                    // sig_ok: the firmware never configures signing, so
                    // `mavlink_signature_check(NULL, ...)` returns true.
                    let was_bad = st == ParseState::SignatureWaitBadCrc;
                    self.parse_state = ParseState::Idle;
                    return if was_bad {
                        Framing::BadCrc
                    } else {
                        Framing::Ok
                    };
                }
            }
        }
        Framing::Incomplete
    }

    /// Port of the `switch (msg.msgid)` + trailing `g_last_update` refresh in
    /// `mavlink_parser_get` (runs only for accepted frames, after the sysid
    /// filter).
    fn process(&mut self, now_ms: u32) {
        if self.sysid_filter != 0 && self.sysid != self.sysid_filter {
            return;
        }
        self.mav_sysid = self.sysid;

        match self.msgid {
            MSG_ID_GLOBAL_POSITION_INT => {
                self.gps.latitude = rd_i32(&self.payload, 4) as f64 / 1e7;
                self.gps.longitude = rd_i32(&self.payload, 8) as f64 / 1e7;
                self.gps.altitude_msl = rd_i32(&self.payload, 12) as f32 / 1000.0;
                self.gps.altitude_relative = rd_i32(&self.payload, 16) as f32 / 1000.0;
                self.gps.heading = (rd_u16(&self.payload, 26) / 100) as i16;
                self.gps.fix_type = 3;
                let vx = rd_i16(&self.payload, 20) as f32 / 100.0;
                let vy = rd_i16(&self.payload, 22) as f32 / 100.0;
                self.gps.speed = libm::sqrtf(vx * vx + vy * vy);
                self.gps.speed_vertical = -(rd_i16(&self.payload, 24) as f32) / 100.0;
            }
            MSG_ID_GPS_RAW_INT => {
                self.gps.latitude = rd_i32(&self.payload, 8) as f64 / 1e7;
                self.gps.longitude = rd_i32(&self.payload, 12) as f64 / 1e7;
                self.gps.altitude_msl = rd_i32(&self.payload, 16) as f32 / 1000.0;
                self.gps.fix_type = rd_u8(&self.payload, 28);
                self.gps.satellites = rd_u8(&self.payload, 29);
                self.gps.speed = rd_u16(&self.payload, 24) as f32 / 100.0;
                self.gps.heading = (rd_u16(&self.payload, 26) / 100) as i16;
            }
            MSG_ID_VFR_HUD => {
                self.gps.speed = rd_f32(&self.payload, 4);
                self.gps.heading = rd_i16(&self.payload, 16);
                self.gps.altitude_msl = rd_f32(&self.payload, 8);
            }
            MSG_ID_ATTITUDE => {
                // ATTITUDE: time_boot_ms@0, roll@4, pitch@8, yaw@12.
                let yaw = rd_f32(&self.payload, 12);
                self.gps.heading = heading_from_rad(yaw);
            }
            MSG_ID_AHRS2 => {
                // AHRS2: roll@0, pitch@4, yaw@8.
                let yaw = rd_f32(&self.payload, 8);
                self.gps.heading = heading_from_rad(yaw);
            }
            MSG_ID_HEARTBEAT => {
                self.armed = (rd_u8(&self.payload, 6) & MODE_FLAG_SAFETY_ARMED) != 0;
                self.has_armed = true;
                self.gps.armed = self.armed;
            }
            MSG_ID_OPEN_DRONE_ID_LOCATION => {
                self.gps.latitude = rd_i32(&self.payload, 0) as f64 / 1e7;
                self.gps.longitude = rd_i32(&self.payload, 4) as f64 / 1e7;
                self.gps.altitude_msl = rd_f32(&self.payload, 12);
                self.gps.altitude_relative = rd_f32(&self.payload, 16);
                self.gps.altitude_baro = rd_f32(&self.payload, 8);
                self.gps.speed = rd_u16(&self.payload, 26) as f32 / 100.0;
                self.gps.speed_vertical = rd_i16(&self.payload, 28) as f32 / 100.0;
                self.gps.heading = (rd_u16(&self.payload, 24) / 100) as i16;
                self.gps.fix_type = 3;
            }
            MSG_ID_OPEN_DRONE_ID_BASIC_ID => {
                self.identity.uas_id[..MAX_STR_LEN].copy_from_slice(&self.payload[24..24 + MAX_STR_LEN]);
                self.identity.uas_id[MAX_STR_LEN] = 0;
                self.identity.id_type = rd_u8(&self.payload, 22);
                self.identity.ua_type = rd_u8(&self.payload, 23);
                self.last_identity_update = now_ms;
            }
            MSG_ID_OPEN_DRONE_ID_OPERATOR_ID => {
                self.identity.operator_id[..MAX_STR_LEN]
                    .copy_from_slice(&self.payload[23..23 + MAX_STR_LEN]);
                self.identity.operator_id[MAX_STR_LEN] = 0;
                self.last_identity_update = now_ms;
            }
            MSG_ID_OPEN_DRONE_ID_SELF_ID => {
                self.identity.self_id_text[..MAX_STR_LEN]
                    .copy_from_slice(&self.payload[23..23 + MAX_STR_LEN]);
                self.identity.self_id_text[MAX_STR_LEN] = 0;
                self.identity.self_id_desc_type = rd_u8(&self.payload, 22);
                self.identity.has_self_id = true;
                self.last_identity_update = now_ms;
            }
            MSG_ID_OPEN_DRONE_ID_AUTHENTICATION => {
                let page = rd_u8(&self.payload, 27) as usize;
                if page < AUTH_MAX_PAGES {
                    self.identity.ext_auth_pages[page][..AUTH_PAGE_DATA_SIZE]
                        .copy_from_slice(&self.payload[30..30 + AUTH_PAGE_DATA_SIZE]);
                    self.identity.ext_auth_last_page = rd_u8(&self.payload, 28);
                    self.identity.ext_auth_type = rd_u8(&self.payload, 26);
                    self.identity.ext_auth_length = rd_u8(&self.payload, 29);
                    self.identity.ext_auth_pages_received |= 1 << page;
                    self.identity.has_ext_auth = true;
                }
                self.last_identity_update = now_ms;
            }
            MSG_ID_OPEN_DRONE_ID_SYSTEM => {
                // This message carries the operator location, not the UA
                // position: gps.latitude/longitude stay untouched.
                self.operator_lat = rd_i32(&self.payload, 0) as f64 / 1e7;
                self.operator_lon = rd_i32(&self.payload, 4) as f64 / 1e7;
                self.operator_alt = rd_f32(&self.payload, 16);
                self.operator_location_update = now_ms;
            }
            MSG_ID_OPEN_DRONE_ID_MESSAGE_PACK => {
                // Accepted at framing level (valid CRC), but the payload
                // decode needs the OpenDroneID C library (`opendroneid-sys`,
                // Fase 3) and is not implemented yet.
            }
            _ => {}
        }

        if self.gps.latitude != 0.0 || self.gps.longitude != 0.0 {
            self.last_update = now_ms;
        }
    }

    /// `mavlink_parser_get(gps)`: last position, fresh within 5000 ms.
    pub fn get(&self, now_ms: u32) -> Option<GpsData> {
        if self.last_update != 0 && now_ms.wrapping_sub(self.last_update) < 5000 {
            Some(self.gps)
        } else {
            None
        }
    }

    /// `mavlink_parser_get_armed(armed)`.
    pub fn get_armed(&self) -> Option<bool> {
        if self.has_armed {
            Some(self.armed)
        } else {
            None
        }
    }

    /// `mavlink_parser_get_sysid(sysid)`: only when a frame was accepted.
    pub fn get_sysid(&self) -> Option<u8> {
        if self.mav_sysid != 0 {
            Some(self.mav_sysid)
        } else {
            None
        }
    }

    /// `mavlink_parser_get_identity(identity)`: fresh within 10000 ms.
    pub fn get_identity(&self, now_ms: u32) -> Option<Identity> {
        if self.last_identity_update != 0 && now_ms.wrapping_sub(self.last_identity_update) < 10000 {
            Some(self.identity)
        } else {
            None
        }
    }

    /// `mavlink_parser_get_operator_location(lat, lon, alt)`: fresh within
    /// 30000 ms and only after a SYSTEM frame was received.
    pub fn get_operator_location(&self, now_ms: u32) -> Option<(f64, f64, f32)> {
        if self.operator_location_update != 0
            && now_ms.wrapping_sub(self.operator_location_update) < 30000
        {
            Some((self.operator_lat, self.operator_lon, self.operator_alt))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    /// Reference CRC-16/MCRF4XX (reflected poly 0x1021) used to validate the
    /// `crc_accumulate` port against the standard algorithm.
    fn crc_mcrf4xx(buf: &[u8]) -> u16 {
        let mut crc = 0xffffu16;
        for &b in buf {
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

    /// Builds a v2 frame: magic, len, incompat, compat, seq, sysid, compid,
    /// 24-bit msgid, payload, CRC (over length..payload + crc_extra).
    pub(crate) fn pack_v2(
        msgid: u32,
        payload: &[u8],
        sysid: u8,
        compid: u8,
        seq: u8,
        crc_extra: u8,
    ) -> Vec<u8> {
        let mut f = [
            STX_V2,
            payload.len() as u8,
            0,
            0,
            seq,
            sysid,
            compid,
            (msgid & 0xff) as u8,
            ((msgid >> 8) & 0xff) as u8,
            ((msgid >> 16) & 0xff) as u8,
        ]
        .to_vec();
        f.extend_from_slice(payload);
        let crc = crc_calculate(&f[1..]);
        let crc = crc_accumulate(crc_extra, crc);
        f.push((crc & 0xff) as u8);
        f.push((crc >> 8) as u8);
        f
    }

    /// Builds a v1 frame: magic, len, seq, sysid, compid, 8-bit msgid,
    /// payload, CRC.
    pub(crate) fn pack_v1(
        msgid: u32,
        payload: &[u8],
        sysid: u8,
        compid: u8,
        seq: u8,
        crc_extra: u8,
    ) -> Vec<u8> {
        assert!(msgid <= 0xff);
        let mut f = [STX_V1, payload.len() as u8, seq, sysid, compid, msgid as u8].to_vec();
        f.extend_from_slice(payload);
        let crc = crc_calculate(&f[1..]);
        let crc = crc_accumulate(crc_extra, crc);
        f.push((crc & 0xff) as u8);
        f.push((crc >> 8) as u8);
        f
    }

    /// Builds a signed v2 frame: incompat = SIGNED, then a 13-byte signature
    /// block. The CRC is computed over the header *with* the flag byte set.
    pub(crate) fn pack_v2_signed(
        msgid: u32,
        payload: &[u8],
        sysid: u8,
        compid: u8,
        seq: u8,
        crc_extra: u8,
    ) -> Vec<u8> {
        let mut f = [
            STX_V2,
            payload.len() as u8,
            IFLAG_SIGNED,
            0,
            seq,
            sysid,
            compid,
            (msgid & 0xff) as u8,
            ((msgid >> 8) & 0xff) as u8,
            ((msgid >> 16) & 0xff) as u8,
        ]
        .to_vec();
        f.extend_from_slice(payload);
        let crc = crc_calculate(&f[1..]);
        let crc = crc_accumulate(crc_extra, crc);
        f.push((crc & 0xff) as u8);
        f.push((crc >> 8) as u8);
        f.extend_from_slice(&[0x11; SIGNATURE_BLOCK_LEN]);
        f
    }

    fn heartbeat(sysid: u8, base_mode: u8, crc_extra: u8) -> Vec<u8> {
        let mut payload = [0u8; 9];
        payload[6] = base_mode;
        payload[8] = 3;
        pack_v2(MSG_ID_HEARTBEAT, &payload, sysid, 1, 0, crc_extra)
    }

    #[test]
    fn crc_matches_mcrf4xx_reference() {
        let samples: [&[u8]; 4] = [
            &[0x09, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x51, 0x06, 0x08, 0x00, 0x00, 0x00, 0x04, 0x03],
            &[0x54, 0x68, 0x65, 0x20, 0x71, 0x75, 0x69, 0x63, 0x6b, 0x20, 0x62, 0x72, 0x6f, 0x77, 0x6e],
            &[0x00],
            &[],
        ];
        for s in samples {
            assert_eq!(crc_calculate(s), crc_mcrf4xx(s), "crc over {s:02x?}");
        }
    }

    #[test]
    fn heartbeat_sets_armed_and_sysid_no_gps() {
        let mut p = MavlinkParser::new();
        let frame = heartbeat(1, MODE_FLAG_SAFETY_ARMED, 50);
        p.feed(&frame, 1000);
        assert_eq!(p.get_armed(), Some(true));
        assert_eq!(p.get_sysid(), Some(1));
        assert_eq!(p.get(1000), None);
        assert_eq!(p.get_identity(1000), None);
    }

    #[test]
    fn disarmed_heartbeat() {
        let mut p = MavlinkParser::new();
        let frame = heartbeat(1, 0, 50);
        p.feed(&frame, 1000);
        assert_eq!(p.get_armed(), Some(false));
    }

    #[test]
    fn gps_raw_int_fills_position() {
        let mut p = MavlinkParser::new();
        let mut payload = [0u8; 52];
        payload[8..12].copy_from_slice(&453040500i32.to_le_bytes());
        payload[12..16].copy_from_slice(&9387500i32.to_le_bytes());
        payload[16..20].copy_from_slice(&1234000i32.to_le_bytes());
        payload[24..26].copy_from_slice(&1050u16.to_le_bytes());
        payload[26..28].copy_from_slice(&12340u16.to_le_bytes());
        payload[28] = 3;
        payload[29] = 11;
        let frame = pack_v2(MSG_ID_GPS_RAW_INT, &payload, 1, 1, 0, 24);
        p.feed(&frame, 1000);
        let g = p.get(1000).expect("gps present");
        assert!((g.latitude - 45.30405).abs() < 1e-9);
        assert!((g.longitude - 0.93875).abs() < 1e-9);
        assert!((g.altitude_msl - 1234.0).abs() < 1e-3);
        assert!((g.speed - 10.5).abs() < 1e-4);
        assert_eq!(g.heading, 123);
        assert_eq!(g.fix_type, 3);
        assert_eq!(g.satellites, 11);
    }

    #[test]
    fn global_position_int_v1() {
        let mut p = MavlinkParser::new();
        let mut payload = [0u8; 28];
        payload[4..8].copy_from_slice(&453000000i32.to_le_bytes());
        payload[8..12].copy_from_slice(&938000000i32.to_le_bytes());
        payload[12..16].copy_from_slice(&100000i32.to_le_bytes());
        payload[16..20].copy_from_slice(&50000i32.to_le_bytes());
        payload[20..22].copy_from_slice(&1500i16.to_le_bytes());
        payload[22..24].copy_from_slice(&(-400i16).to_le_bytes());
        payload[24..26].copy_from_slice(&(-300i16).to_le_bytes());
        payload[26..28].copy_from_slice(&9000u16.to_le_bytes());
        let frame = pack_v1(MSG_ID_GLOBAL_POSITION_INT, &payload, 3, 1, 7, 104);
        p.feed(&frame, 1000);
        let g = p.get(1000).expect("gps present");
        assert!((g.latitude - 45.3).abs() < 1e-9);
        assert!((g.longitude - 93.8).abs() < 1e-9);
        assert!((g.altitude_msl - 100.0).abs() < 1e-3);
        assert!((g.altitude_relative - 50.0).abs() < 1e-3);
        assert!((g.speed - 15.52417).abs() < 1e-3); // sqrt(15^2 + 4^2)
        assert!((g.speed_vertical - 3.0).abs() < 1e-4);
        assert_eq!(g.heading, 90);
        assert_eq!(g.fix_type, 3);
        assert_eq!(p.get_sysid(), Some(3));
    }

    #[test]
    fn vfr_hud_and_attitude_and_ahrs2() {
        let mut p = MavlinkParser::new();

        // Position first: VFR_HUD / ATTITUDE / AHRS2 only refresh the last
        // update, they never produce a fix on their own (lat/lon stay 0).
        let mut pos = [0u8; 52];
        pos[8..12].copy_from_slice(&453000000i32.to_le_bytes());
        pos[12..16].copy_from_slice(&938000000i32.to_le_bytes());
        p.feed(&pack_v2(MSG_ID_GPS_RAW_INT, &pos, 1, 1, 0, 24), 1000);

        let mut hud = [0u8; 20];
        hud[4..8].copy_from_slice(&12.5f32.to_le_bytes());
        hud[8..12].copy_from_slice(&105.25f32.to_le_bytes());
        hud[16..18].copy_from_slice(&250i16.to_le_bytes());
        p.feed(&pack_v2(MSG_ID_VFR_HUD, &hud, 1, 1, 1, 20), 2000);
        let g = p.get(2000).expect("hud gps");
        assert!((g.speed - 12.5).abs() < 1e-4);
        assert!((g.altitude_msl - 105.25).abs() < 1e-3);
        assert_eq!(g.heading, 250);

        let mut att = [0u8; 28];
        att[12..16].copy_from_slice(&(-0.5f32).to_le_bytes());
        p.feed(&pack_v2(MSG_ID_ATTITUDE, &att, 1, 1, 2, 39), 3000);
        let g = p.get(3000).expect("att gps");
        assert_eq!(g.heading, 332); // -0.5 rad * 180/pi = -28.65 -> truncated -28 + 360

        let mut ahrs = [0u8; 24];
        ahrs[8..12].copy_from_slice(&0.5f32.to_le_bytes());
        p.feed(&pack_v2(MSG_ID_AHRS2, &ahrs, 1, 1, 3, 47), 4000);
        let g = p.get(4000).expect("ahrs2 gps");
        assert_eq!(g.heading, 28);
    }

    #[test]
    fn odid_location_sets_gps() {
        let mut p = MavlinkParser::new();
        let mut payload = [0u8; 59];
        payload[0..4].copy_from_slice(&453000000i32.to_le_bytes());
        payload[4..8].copy_from_slice(&938000000i32.to_le_bytes());
        payload[8..12].copy_from_slice(&50.0f32.to_le_bytes());
        payload[12..16].copy_from_slice(&150.0f32.to_le_bytes());
        payload[16..20].copy_from_slice(&20.0f32.to_le_bytes());
        payload[24..26].copy_from_slice(&12300u16.to_le_bytes());
        payload[26..28].copy_from_slice(&1050u16.to_le_bytes());
        payload[28..30].copy_from_slice(&(-50i16).to_le_bytes());
        let frame = pack_v2(MSG_ID_OPEN_DRONE_ID_LOCATION, &payload, 1, 1, 0, 254);
        p.feed(&frame, 1000);
        let g = p.get(1000).expect("gps present");
        assert!((g.latitude - 45.3).abs() < 1e-9);
        assert!((g.longitude - 93.8).abs() < 1e-9);
        assert!((g.altitude_msl - 150.0).abs() < 1e-4);
        assert!((g.altitude_relative - 20.0).abs() < 1e-4);
        assert!((g.altitude_baro - 50.0).abs() < 1e-4);
        assert!((g.speed - 10.5).abs() < 1e-4);
        assert!((g.speed_vertical + 0.5).abs() < 1e-4);
        assert_eq!(g.heading, 123);
        assert_eq!(g.fix_type, 3);
    }

    #[test]
    fn basic_operator_self_auth_messages() {
        let mut p = MavlinkParser::new();

        let mut basic = [0u8; 44];
        basic[22] = 1;
        basic[23] = 2;
        basic[24..44].copy_from_slice(b"ABCDEFGHIJ0123456789");
        p.feed(&pack_v2(MSG_ID_OPEN_DRONE_ID_BASIC_ID, &basic, 1, 1, 0, 114), 1000);
        let id = p.get_identity(1000).expect("identity present");
        assert_eq!(&id.uas_id[..20], b"ABCDEFGHIJ0123456789");
        assert_eq!(id.uas_id[20], 0);
        assert_eq!(id.id_type, 1);
        assert_eq!(id.ua_type, 2);

        let mut op = [0u8; 43];
        op[23..43].copy_from_slice(b"OP-12345678901234567");
        p.feed(&pack_v2(MSG_ID_OPEN_DRONE_ID_OPERATOR_ID, &op, 1, 1, 1, 49), 2000);
        let id = p.get_identity(2000).expect("identity present");
        assert_eq!(&id.operator_id[..20], b"OP-12345678901234567");
        assert_eq!(id.operator_id[20], 0);

        let mut self_id = [0u8; 46];
        self_id[22] = 5;
        self_id[23..43].copy_from_slice(b"HELLO WORLD         ");
        p.feed(&pack_v2(MSG_ID_OPEN_DRONE_ID_SELF_ID, &self_id, 1, 1, 2, 249), 3000);
        let id = p.get_identity(3000).expect("identity present");
        assert_eq!(&id.self_id_text[..20], b"HELLO WORLD         ");
        assert_eq!(id.self_id_desc_type, 5);
        assert!(id.has_self_id);
    }

    #[test]
    fn authentication_pages_accumulate() {
        let mut p = MavlinkParser::new();
        let mut a0 = [0u8; 53];
        a0[26] = 2; // authentication_type
        a0[27] = 0; // data_page
        a0[28] = 3; // last_page_index
        a0[29] = 45; // length
        a0[30..53].copy_from_slice(b"PAGE0AAAAAAAAAAAAAAAAAA");
        p.feed(&pack_v2(MSG_ID_OPEN_DRONE_ID_AUTHENTICATION, &a0, 1, 1, 0, 140), 1000);
        let id = p.get_identity(1000).expect("identity present");
        assert!(id.has_ext_auth);
        assert_eq!(id.ext_auth_type, 2);
        assert_eq!(id.ext_auth_last_page, 3);
        assert_eq!(id.ext_auth_length, 45);
        assert_eq!(id.ext_auth_pages_received, 1 << 0);
        assert_eq!(&id.ext_auth_pages[0][..23], b"PAGE0AAAAAAAAAAAAAAAAAA");
        assert_eq!(id.ext_auth_pages[0][23], 0);

        let mut a1 = [0u8; 53];
        a1[26] = 2;
        a1[27] = 1;
        a1[28] = 3;
        a1[30..53].copy_from_slice(b"PAGE1AAAAAAAAAAAAAAAAAA");
        p.feed(&pack_v2(MSG_ID_OPEN_DRONE_ID_AUTHENTICATION, &a1, 1, 1, 1, 140), 2000);
        let id = p.get_identity(2000).expect("identity present");
        assert_eq!(id.ext_auth_pages_received, 0b11);
        assert_eq!(&id.ext_auth_pages[1][..23], b"PAGE1AAAAAAAAAAAAAAAAAA");
    }

    #[test]
    fn auth_page_beyond_max_is_ignored_but_refreshes_identity() {
        let mut p = MavlinkParser::new();
        let mut a = [0u8; 53];
        a[27] = AUTH_MAX_PAGES as u8;
        p.feed(&pack_v2(MSG_ID_OPEN_DRONE_ID_AUTHENTICATION, &a, 1, 1, 0, 140), 1000);
        let id = p.get_identity(1000).expect("identity present");
        assert!(!id.has_ext_auth);
        assert_eq!(id.ext_auth_pages_received, 0);
    }

    #[test]
    fn system_sets_operator_location() {
        let mut p = MavlinkParser::new();
        let mut payload = [0u8; 54];
        payload[0..4].copy_from_slice(&450000000i32.to_le_bytes());
        payload[4..8].copy_from_slice(&90000000i32.to_le_bytes());
        payload[16..20].copy_from_slice(&300.5f32.to_le_bytes());
        let frame = pack_v2(MSG_ID_OPEN_DRONE_ID_SYSTEM, &payload, 1, 1, 0, 77);
        p.feed(&frame, 1000);
        let (lat, lon, alt) = p.get_operator_location(1000).expect("operator location");
        assert!((lat - 45.0).abs() < 1e-9);
        assert!((lon - 9.0).abs() < 1e-9);
        assert!((alt - 300.5).abs() < 1e-4);
        // SYSTEM must not touch the UA position.
        assert_eq!(p.get(1000), None);
    }

    #[test]
    fn freshness_windows() {
        let mut p = MavlinkParser::new();
        let mut payload = [0u8; 52];
        payload[8..12].copy_from_slice(&453000000i32.to_le_bytes());
        payload[12..16].copy_from_slice(&938000000i32.to_le_bytes());
        // now_ms must be nonzero: g_last_update==0 means "never updated".
        p.feed(&pack_v2(MSG_ID_GPS_RAW_INT, &payload, 1, 1, 0, 24), 1000);
        assert!(p.get(5999).is_some());
        assert!(p.get(6000).is_none());

        let mut basic = [0u8; 44];
        basic[24] = b'X';
        p.feed(&pack_v2(MSG_ID_OPEN_DRONE_ID_BASIC_ID, &basic, 1, 1, 1, 114), 1000);
        assert!(p.get_identity(10999).is_some());
        assert!(p.get_identity(11000).is_none());

        let mut sys = [0u8; 54];
        sys[0..4].copy_from_slice(&450000000i32.to_le_bytes());
        p.feed(&pack_v2(MSG_ID_OPEN_DRONE_ID_SYSTEM, &sys, 1, 1, 2, 77), 1000);
        assert!(p.get_operator_location(30999).is_some());
        assert!(p.get_operator_location(31000).is_none());
    }

    #[test]
    fn freshness_refreshed_by_any_position_frame() {
        let mut p = MavlinkParser::new();
        let mut payload = [0u8; 52];
        payload[8..12].copy_from_slice(&453000000i32.to_le_bytes());
        payload[12..16].copy_from_slice(&938000000i32.to_le_bytes());
        p.feed(&pack_v2(MSG_ID_GPS_RAW_INT, &payload, 1, 1, 0, 24), 0);
        // At 4000 ms a heartbeat refreshes g_last_update (lat/lon nonzero).
        p.feed(&heartbeat(1, 0, 50), 4000);
        assert!(p.get(8000).is_some());
        assert!(p.get(9000).is_none());
    }

    #[test]
    fn sysid_filter_skips_other_systems() {
        let mut p = MavlinkParser::new();
        p.set_sysid_filter(1);
        let mut payload = [0u8; 52];
        payload[8..12].copy_from_slice(&453000000i32.to_le_bytes());
        payload[12..16].copy_from_slice(&938000000i32.to_le_bytes());
        p.feed(&pack_v2(MSG_ID_GPS_RAW_INT, &payload, 2, 1, 0, 24), 1000);
        assert_eq!(p.get(1000), None);
        assert_eq!(p.get_sysid(), None);
        p.feed(&pack_v2(MSG_ID_GPS_RAW_INT, &payload, 1, 1, 1, 24), 2000);
        assert!(p.get(2000).is_some());
        assert_eq!(p.get_sysid(), Some(1));
    }

    #[test]
    fn bad_crc_frame_is_discarded_and_stream_resyncs() {
        let mut p = MavlinkParser::new();
        let mut good = [0u8; 52];
        good[8..12].copy_from_slice(&453000000i32.to_le_bytes());
        good[12..16].copy_from_slice(&938000000i32.to_le_bytes());
        let good_frame = pack_v2(MSG_ID_GPS_RAW_INT, &good, 1, 1, 0, 24);

        // Corrupt a payload byte, then append the good frame back to back.
        let mut bad = good_frame.clone();
        bad[20] ^= 0xff;
        let mut stream = bad;
        stream.extend_from_slice(&good_frame);
        p.feed(&stream, 1000);
        let g = p.get(1000).expect("good frame after bad crc parsed");
        assert!((g.latitude - 45.3).abs() < 1e-9);
    }

    #[test]
    fn unknown_message_is_ignored() {
        let mut p = MavlinkParser::new();
        // BATTERY_STATUS (msgid 147) is not in the CRC table.
        let payload = [0u8; 36];
        let frame = pack_v2(147, &payload, 1, 1, 0, 154);
        p.feed(&frame, 1000);
        assert_eq!(p.get(1000), None);
        assert_eq!(p.get_sysid(), None);

        let mut good = [0u8; 52];
        good[8..12].copy_from_slice(&453000000i32.to_le_bytes());
        good[12..16].copy_from_slice(&938000000i32.to_le_bytes());
        let good_frame = pack_v2(MSG_ID_GPS_RAW_INT, &good, 1, 1, 1, 24);
        let mut stream = frame;
        stream.extend_from_slice(&good_frame);
        p.feed(&stream, 2000);
        assert!(p.get(2000).is_some());
    }

    #[test]
    fn signed_frame_is_accepted() {
        let mut p = MavlinkParser::new();
        let mut payload = [0u8; 52];
        payload[8..12].copy_from_slice(&453000000i32.to_le_bytes());
        payload[12..16].copy_from_slice(&938000000i32.to_le_bytes());
        let frame = pack_v2_signed(MSG_ID_GPS_RAW_INT, &payload, 1, 1, 0, 24);
        assert_eq!(frame.len(), 10 + 52 + 2 + 13);
        p.feed(&frame, 1000);
        assert!(p.get(1000).is_some());
    }

    #[test]
    fn signed_bad_crc_is_discarded() {
        let mut p = MavlinkParser::new();
        let mut payload = [0u8; 52];
        payload[8..12].copy_from_slice(&453000000i32.to_le_bytes());
        payload[12..16].copy_from_slice(&938000000i32.to_le_bytes());
        let mut frame = pack_v2_signed(MSG_ID_GPS_RAW_INT, &payload, 1, 1, 0, 24);
        frame[20] ^= 0xff;
        p.feed(&frame, 1000);
        assert_eq!(p.get(1000), None);
    }

    #[test]
    fn incompatible_incompat_flags_discards_frame() {
        let mut p = MavlinkParser::new();
        let mut payload = [0u8; 52];
        payload[8..12].copy_from_slice(&453000000i32.to_le_bytes());
        payload[12..16].copy_from_slice(&938000000i32.to_le_bytes());
        let mut frame = pack_v2(MSG_ID_GPS_RAW_INT, &payload, 1, 1, 0, 24);
        frame[2] = 0x80; // unknown incompatibility flag
        p.feed(&frame, 1000);
        assert_eq!(p.get(1000), None);
    }

    #[test]
    fn message_pack_accepted_but_not_decoded() {
        let mut p = MavlinkParser::new();
        let mut payload = [0u8; 249];
        payload[22] = 25; // single_message_size
        payload[23] = 1; // msg_pack_size
        let frame = pack_v2(MSG_ID_OPEN_DRONE_ID_MESSAGE_PACK, &payload, 1, 1, 0, 94);
        p.feed(&frame, 1000);
        // CRC passes (no panic), no gps/identity produced yet.
        assert_eq!(p.get(1000), None);
        assert_eq!(p.get_identity(1000), None);
    }

    #[test]
    fn short_frame_extension_fields_read_as_zero() {
        let mut p = MavlinkParser::new();
        // GPS_RAW_INT sent with only the 12-byte core: time_usec (8 bytes,
        // all zero) + lat. lon@12 is an extension field -> zero-filled.
        let mut payload = [0u8; 12];
        payload[8..12].copy_from_slice(&453000000i32.to_le_bytes());
        let frame = pack_v2(MSG_ID_GPS_RAW_INT, &payload, 1, 1, 0, 24);
        p.feed(&frame, 1000);
        let g = p.get(1000).expect("gps present");
        assert!((g.latitude - 45.3).abs() < 1e-9);
        assert_eq!(g.longitude, 0.0);
        assert_eq!(g.altitude_msl, 0.0);
        assert_eq!(g.fix_type, 0);
        assert_eq!(g.satellites, 0);
        assert_eq!(g.speed, 0.0);
    }

    #[test]
    fn frame_split_across_feed_calls() {
        let mut p = MavlinkParser::new();
        let mut payload = [0u8; 52];
        payload[8..12].copy_from_slice(&453000000i32.to_le_bytes());
        payload[12..16].copy_from_slice(&938000000i32.to_le_bytes());
        let frame = pack_v2(MSG_ID_GPS_RAW_INT, &payload, 1, 1, 0, 24);
        for (i, &b) in frame.iter().enumerate() {
            p.feed(&[b], 1000);
            if i + 1 < frame.len() {
                assert_eq!(p.get(1000), None);
            }
        }
        assert!(p.get(1000).is_some());
    }

    #[test]
    fn zero_length_payload_v2() {
        let mut p = MavlinkParser::new();
        let frame = pack_v2(MSG_ID_HEARTBEAT, &[], 1, 1, 0, 50);
        p.feed(&frame, 1000);
        assert_eq!(p.get_armed(), Some(false));
        assert_eq!(p.get_sysid(), Some(1));
    }
}
