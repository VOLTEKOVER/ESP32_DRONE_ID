//! Pure-Rust DroneCAN/UAVCAN v0 parser.
//!
//! Port of `rid_dronecan.c` (ESP32_DRONE_REMOTE_ID_Firmware). The C source is
//! a stub: `decode_fix2` requires a single frame of at least 32 bytes, which a
//! classic-CAN frame (DLC <= 8) can never carry, and no multi-frame transfer
//! reassembly exists. This parser implements the transport protocol properly,
//! following libuavcan v0 (`uc_transfer_receiver.cpp`, commit d31c6923):
//!
//! - CAN id decode: `priority` (bits 24..28), `data_type_id` (bits 8..23),
//!   service-not-message bit 7 (ignored), `source_node_id` (bits 0..6);
//! - tail byte framing: bit 7 SoT, bit 6 EoT, bit 5 toggle, bits 0..4 TID;
//! - transfer reassembly per (source node, data type id), with the
//!   `TransferReceiver` FSM: restart on `not_initialized` / TID timeout
//!   (1 s) / unexpected first frame TID, toggle + TID validation, transfer
//!   CRC check on completion;
//! - CRC-16/CCITT-FALSE (init 0xFFFF, poly 0x1021, no reflection) over
//!   `[signature (8 bytes, little-endian)] ++ [transfer payload]`, compared
//!   against the 2 little-endian bytes prepended to the transfer;
//! - decode of `uavcan.equipment.gnss.Fix2` (data type id 1063, signature
//!   `0xca41e7000f37435f`) from its bit-packed DSDL layout;
//! - the 5 s freshness window of `rid_dronecan_get` and the `g_active` flag.
//!
//! The `uavcan.equipment.ahrs.Solution` (1000) and `org.drone_id.Identity`
//! (8192) cases of the C switch stay no-ops. `no_std`, allocation-free.

use rid_interface::{CanFrame, GpsData};

/// `uavcan.equipment.gnss.Fix2` data type id.
pub const FIX2_DTID: u16 = 1063;
/// `uavcan.equipment.gnss.Fix2` data type signature (CRC-64-WE of the DSDL).
const FIX2_SIGNATURE: u64 = 0xca41e7000f37435f;

/// Extended-id bit layout (UAVCAN v0): priority in bits 24..28 (not decoded),
/// `data_type_id` in bits 8..23, `source_node_id` in bits 0..6.
const DTID_SHIFT: u8 = 8;
/// `source_node_id` mask (bits 0..6).
const SRC_NODE_MASK: u32 = 0x7F;

/// Tail byte flags.
const FLAG_SOT: u8 = 0x80;
const FLAG_EOT: u8 = 0x40;
const FLAG_TOGGLE: u8 = 0x20;
const TID_MASK: u8 = 0x1F;

/// Transfer CRC is prepended to every multi-frame transfer (2 bytes, LE).
const TRANSFER_CRC_BYTES: usize = 2;
/// `TransferReceiver::DefaultTidTimeoutMSec` (libuavcan).
const TID_TIMEOUT_MS: u32 = 1000;
/// Freshness window of `rid_dronecan_get` (5 s).
const FRESHNESS_MS: u32 = 5000;
/// Reassembly buffer size (a full Fix2 with covariance + ECEF fits).
const MAX_TRANSFER_SIZE: usize = 256;
/// Reassembly contexts kept per (source node, data type id).
const MAX_TRANSFERS: usize = 4;

/// Fix2 fixed-offset section (48 bytes); the variable covariance/ECEF tail is
/// not decoded. Bit offsets come from the packed DSDL:
/// `timestamp`(56) + `gnss_timestamp`(56) + `gnss_time_standard`(3) +
/// `void13` + `num_leap_seconds`(8) + then the fields below.
const LON_OFFSET: usize = 136; // int37 longitude_deg_1e8
const LAT_OFFSET: usize = 173; // int37 latitude_deg_1e8
const ELL_OFFSET: usize = 210; // int27 height_ellipsoid_mm
const MSL_OFFSET: usize = 237; // int27 height_msl_mm
/// `float32[3] ned_velocity` starts at byte 33 (little-endian).
const NED_VEL_BYTE: usize = 33;
/// Byte 45: `uint6 sats_used` (bits 0..5), `uint2 status` (bits 6..7).
const SATS_STATUS_BYTE: usize = 45;
/// Minimum payload a Fix2 transfer can carry to be decodable.
const FIX2_FIXED_BYTES: usize = 48;

/// One parsed CAN frame (port of libuavcan `RxFrame`).
#[derive(Clone, Copy, Debug)]
struct Frame {
    dtid: u16,
    src_node: u8,
    sot: bool,
    eot: bool,
    toggle: bool,
    tid: u8,
    /// Payload bytes before the tail byte (`dlc - 1`, <= 7).
    payload_len: usize,
    data: [u8; 8],
}

impl Frame {
    fn parse(can: &CanFrame) -> Option<Frame> {
        let dlc = can.dlc as usize;
        if dlc == 0 {
            return None; // No tail byte.
        }
        let tail = can.data[dlc - 1];
        Some(Frame {
            dtid: ((can.id >> DTID_SHIFT) & 0xFFFF) as u16,
            src_node: (can.id & SRC_NODE_MASK) as u8,
            sot: tail & FLAG_SOT != 0,
            eot: tail & FLAG_EOT != 0,
            toggle: tail & FLAG_TOGGLE != 0,
            tid: tail & TID_MASK,
            payload_len: dlc - 1,
            data: can.data,
        })
    }
}

/// One byte of CRC-16/CCITT-FALSE (init 0xFFFF, poly 0x1021, no reflection),
/// exactly like `crc_accumulate()` in libuavcan's `crc.hpp`.
fn crc_accumulate(data: u8, crc: u16) -> u16 {
    let mut crc = crc ^ ((data as u16) << 8);
    for _ in 0..8 {
        crc = if crc & 0x8000 != 0 {
            (crc << 1) ^ 0x1021
        } else {
            crc << 1
        };
    }
    crc
}

/// CRC seed for a data type: CRC-16/CCITT-FALSE over the 8 signature bytes
/// in little-endian order (`Signature::toTransferCRC()`).
fn transfer_crc_seed(signature: u64) -> u16 {
    let mut crc = 0xFFFFu16;
    for i in 0..8 {
        crc = crc_accumulate(((signature >> (8 * i)) & 0xFF) as u8, crc);
    }
    crc
}

/// Checks the received transfer CRC against
/// `CRC16(signature_le ++ payload)`.
fn check_payload_crc(payload: &[u8], received: u16, signature: u64) -> bool {
    let mut crc = transfer_crc_seed(signature);
    for &b in payload {
        crc = crc_accumulate(b, crc);
    }
    crc == received
}

/// `TransferID::computeForwardDistance` for the 5-bit TID space.
fn forward_distance(a: u8, b: u8) -> u8 {
    b.wrapping_sub(a) & TID_MASK
}

/// Wrap-safe `now >= ref` for the monotonic ms clock (rejects timestamps more
/// than half the u32 range behind the reference).
fn is_after(now: u32, reference: u32) -> bool {
    now.wrapping_sub(reference) <= (u32::MAX / 2)
}

/// Reads `width` (<= 64) little-endian bits at `offset`, as in the DSDL
/// serialization (least significant bit of the field at `offset`).
fn read_bits(data: &[u8], offset: usize, width: usize) -> u64 {
    let mut value: u64 = 0;
    for i in 0..width {
        let bit = offset + i;
        if (data[bit >> 3] >> (bit & 7)) & 1 == 1 {
            value |= 1u64 << i;
        }
    }
    value
}

/// Reads a signed field (two's complement, sign-extended).
fn read_signed(data: &[u8], offset: usize, width: usize) -> i64 {
    let value = read_bits(data, offset, width);
    let sign = 1u64 << (width - 1);
    if value & sign != 0 {
        (value | !(sign - 1)) as i64
    } else {
        value as i64
    }
}

/// One transfer reassembly context (port of libuavcan `TransferReceiver`).
#[derive(Clone)]
struct TransferState {
    key: (u8, u16),
    initialized: bool,
    tid: u8,
    next_toggle: bool,
    buffer_write_pos: usize,
    this_transfer_crc: u16,
    prev_transfer_ms: u32,
    this_transfer_ms: u32,
    buf: [u8; MAX_TRANSFER_SIZE],
}

impl Default for TransferState {
    fn default() -> Self {
        Self {
            key: (0, 0),
            initialized: false,
            tid: 0,
            next_toggle: false,
            buffer_write_pos: 0,
            this_transfer_crc: 0,
            prev_transfer_ms: 0,
            this_transfer_ms: 0,
            buf: [0; MAX_TRANSFER_SIZE],
        }
    }
}

impl TransferState {
    fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// `(now - this_transfer_ts_) > DefaultTidTimeoutMSec`.
    fn is_timed_out(&self, now_ms: u32) -> bool {
        self.this_transfer_ms != 0 && now_ms.wrapping_sub(self.this_transfer_ms) > TID_TIMEOUT_MS
    }

    /// `prepareForNextTransfer()`: TID advances, toggle/buffer reset.
    fn prepare_for_next(&mut self) {
        self.tid = (self.tid + 1) & TID_MASK;
        self.next_toggle = false;
        self.buffer_write_pos = 0;
    }

    /// `TransferReceiver::validate()` minus the interface check.
    fn validate(&self, f: &Frame) -> bool {
        if f.sot && !f.eot && f.payload_len < TRANSFER_CRC_BYTES {
            return false; // CRC expected.
        }
        if f.sot && f.toggle {
            return false; // Toggle bit is not cleared.
        }
        if f.toggle != self.next_toggle {
            return false;
        }
        if f.tid != self.tid {
            return false;
        }
        true
    }

    /// `TransferReceiver::writePayload()`; returns false on buffer overflow.
    fn write_payload(&mut self, f: &Frame) -> bool {
        if f.sot {
            // The first frame carries the transfer CRC (2 bytes, LE).
            self.this_transfer_crc =
                (f.data[0] as u16) | ((f.data[1] as u16) << 8);
            let n = f.payload_len - TRANSFER_CRC_BYTES;
            if n > MAX_TRANSFER_SIZE {
                return false;
            }
            self.buf[..n].copy_from_slice(&f.data[2..2 + n]);
            self.buffer_write_pos = n;
        } else {
            let n = f.payload_len;
            if self.buffer_write_pos + n > MAX_TRANSFER_SIZE {
                return false;
            }
            self.buf[self.buffer_write_pos..self.buffer_write_pos + n]
                .copy_from_slice(&f.data[..n]);
            self.buffer_write_pos += n;
        }
        true
    }
}

/// DroneCAN input parser: feeds CAN frames, produces the last decoded Fix2
/// within its freshness window.
#[derive(Clone, Default)]
pub struct DronecanParser {
    transfers: [TransferState; MAX_TRANSFERS],
    gps: GpsData,
    last_update_ms: u32,
    active: bool,
}

impl DronecanParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// `rid_dronecan_is_active()`: true once any CAN frame was seen.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// `rid_dronecan_get()`: the last Fix2, only while it is inside the 5 s
    /// window following a fix with `status >= 2`.
    pub fn get(&self, now_ms: u32) -> Option<GpsData> {
        if self.last_update_ms != 0 && now_ms.wrapping_sub(self.last_update_ms) < FRESHNESS_MS {
            Some(self.gps)
        } else {
            None
        }
    }

    /// Feeds one CAN frame (`rid_dronecan_get`'s `twai_receive` loop).
    pub fn feed(&mut self, can: &CanFrame, now_ms: u32) {
        self.active = true;

        let Some(f) = Frame::parse(can) else {
            return;
        };
        if f.dtid != FIX2_DTID {
            return; // AHRS/Identity and anything else are no-ops.
        }

        let idx = self.get_or_create_context((f.src_node, f.dtid), now_ms);
        let t = &mut self.transfers[idx];

        // addFrame() timestamp guards: zero or out-of-order frames are dropped.
        if now_ms == 0
            || !is_after(now_ms, t.prev_transfer_ms)
            || !is_after(now_ms, t.this_transfer_ms)
        {
            return;
        }

        // TransferReceiver::addFrame() FSM.
        let not_initialized = !t.is_initialized();
        let tid_timed_out = t.is_timed_out(now_ms);
        let first_frame = f.sot;
        let not_previous_tid = forward_distance(f.tid, t.tid) > 1;
        let need_restart =
            not_initialized || tid_timed_out || (first_frame && not_previous_tid);

        if need_restart {
            // tba.remove() + restart(frame).
            t.tid = f.tid;
            t.next_toggle = false;
            t.buffer_write_pos = 0;
            t.this_transfer_crc = 0;
            t.initialized = true;
            if !first_frame {
                // Tail of a previous transfer; wait for the next start frame.
                t.tid = (t.tid + 1) & TID_MASK;
                return;
            }
        }

        if !t.validate(&f) {
            return;
        }

        // receive(): this_transfer_ts_ is derived from the first frame.
        if f.sot {
            t.this_transfer_ms = now_ms;
        }

        // Single-frame transfer: no CRC, cannot carry a Fix2 (48+ bytes).
        if f.sot && f.eot {
            t.prev_transfer_ms = t.this_transfer_ms; // updateTransferTimings()
            t.prepare_for_next();
            return;
        }

        if !t.write_payload(&f) {
            // Buffer overflow: drop the transfer, prepare for the next one.
            t.prepare_for_next();
            return;
        }
        t.next_toggle = !t.next_toggle;

        if f.eot {
            t.prev_transfer_ms = t.this_transfer_ms; // updateTransferTimings()
            let complete = t.buffer_write_pos;
            let crc_ok = check_payload_crc(
                &t.buf[..complete],
                t.this_transfer_crc,
                FIX2_SIGNATURE,
            );
            t.prepare_for_next();
            if crc_ok {
                Self::decode_fix2(
                    &t.buf[..complete],
                    now_ms,
                    &mut self.gps,
                    &mut self.last_update_ms,
                );
            }
        }
    }

    /// Picks the context for `key`, reusing the least-recently-active slot
    /// when the pool is exhausted (replaces libuavcan's buffer pool).
    fn get_or_create_context(&mut self, key: (u8, u16), now_ms: u32) -> usize {
        for (i, t) in self.transfers.iter().enumerate() {
            if t.initialized && t.key == key {
                return i;
            }
        }
        for (i, t) in self.transfers.iter().enumerate() {
            if !t.initialized {
                self.transfers[i].key = key;
                return i;
            }
        }
        let mut idx = 0;
        let mut oldest = u32::MAX;
        for (i, t) in self.transfers.iter().enumerate() {
            let last = if t.this_transfer_ms != 0 {
                t.this_transfer_ms
            } else {
                now_ms
            };
            if last < oldest {
                oldest = last;
                idx = i;
            }
        }
        let t = &mut self.transfers[idx];
        *t = TransferState::default();
        t.key = key;
        idx
    }

    /// `decode_fix2()` on the reassembled transfer (the C body, re-mapped to
    /// the standard Fix2 DSDL layout). `last_update_ms` advances only for
    /// `status >= 2`, like the C `if (gnss_fix >= 2)`.
    fn decode_fix2(payload: &[u8], now_ms: u32, gps: &mut GpsData, last_update_ms: &mut u32) {
        if payload.len() < FIX2_FIXED_BYTES {
            return;
        }

        let lon_deg_1e8 = read_signed(payload, LON_OFFSET, 37);
        let lat_deg_1e8 = read_signed(payload, LAT_OFFSET, 37);
        let height_ell_mm = read_signed(payload, ELL_OFFSET, 27);
        let height_msl_mm = read_signed(payload, MSL_OFFSET, 27);

        let p = NED_VEL_BYTE;
        let vn = f32::from_le_bytes([payload[p], payload[p + 1], payload[p + 2], payload[p + 3]]);
        let ve = f32::from_le_bytes([payload[p + 4], payload[p + 5], payload[p + 6], payload[p + 7]]);

        let sats = payload[SATS_STATUS_BYTE] & 0x3F;
        let status = (payload[SATS_STATUS_BYTE] >> 6) & 0x3;

        gps.latitude = lat_deg_1e8 as f64 / 1.0e8;
        gps.longitude = lon_deg_1e8 as f64 / 1.0e8;
        gps.altitude_msl = height_msl_mm as f32 / 1000.0;
        gps.altitude_relative = height_ell_mm as f32 / 1000.0;

        // Horizontal speed from the NED velocity.
        gps.speed = libm::sqrtf(vn * vn + ve * ve);

        // Heading from the horizontal velocity vector (C has no equivalent).
        let deg = libm::atan2f(ve, vn) * (180.0 / core::f32::consts::PI);
        let deg = if deg < 0.0 { deg + 360.0 } else { deg };
        gps.heading = ((deg + 0.5) as i16) % 360;

        gps.satellites = sats;
        if status >= 2 {
            gps.fix_type = status;
            *last_update_ms = now_ms;
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Test helpers: bit-packing a Fix2 payload and slicing it into CAN
    //! frames. Tests use `std` (this crate is `no_std`).

    use crate::parser::{
        crc_accumulate, transfer_crc_seed, ELL_OFFSET, FIX2_DTID, FIX2_SIGNATURE, LAT_OFFSET,
        LON_OFFSET, MSL_OFFSET, NED_VEL_BYTE, SATS_STATUS_BYTE, TID_MASK, TRANSFER_CRC_BYTES,
    };
    use rid_interface::CanFrame;
    use std::vec::Vec;

    /// Packed Fix2 fixed-offset section (48 bytes), fields set as given.
    pub struct Fix2Fields {
        pub lon_deg_1e8: i64,
        pub lat_deg_1e8: i64,
        pub height_ell_mm: i64,
        pub height_msl_mm: i64,
        pub vn: f32,
        pub ve: f32,
        pub vd: f32,
        pub sats: u8,
        pub status: u8,
        pub mode: u8,
        pub sub_mode: u8,
    }

    impl Default for Fix2Fields {
        fn default() -> Self {
            Self {
                lon_deg_1e8: 938_750_000,
                lat_deg_1e8: 4_530_405_000,
                height_ell_mm: 1_234_000,
                height_msl_mm: 1_050_000,
                vn: 10.0,
                ve: -2.5,
                vd: 0.5,
                sats: 12,
                status: 3,
                mode: 0,
                sub_mode: 0,
            }
        }
    }

    /// Packs the fields into the 48-byte DSDL bit layout.
    pub fn pack_fix2(f: &Fix2Fields) -> [u8; 48] {
        let mut data = [0u8; 48];
        set_bits(&mut data, LON_OFFSET, 37, f.lon_deg_1e8 as u64);
        set_bits(&mut data, LAT_OFFSET, 37, f.lat_deg_1e8 as u64);
        set_bits(&mut data, ELL_OFFSET, 27, f.height_ell_mm as u64);
        set_bits(&mut data, MSL_OFFSET, 27, f.height_msl_mm as u64);
        let p = NED_VEL_BYTE;
        data[p..p + 4].copy_from_slice(&f.vn.to_le_bytes());
        data[p + 4..p + 8].copy_from_slice(&f.ve.to_le_bytes());
        data[p + 8..p + 12].copy_from_slice(&f.vd.to_le_bytes());
        data[SATS_STATUS_BYTE] = (f.sats & 0x3F) | ((f.status & 0x3) << 6);
        // mode (byte 46, bits 0..3) + sub_mode (bits 4..7 + byte 47 bits 0..1).
        data[SATS_STATUS_BYTE + 1] = (f.mode & 0xF) | ((f.sub_mode & 0xF) << 4);
        data[SATS_STATUS_BYTE + 2] = (f.sub_mode >> 4) & 0x3;
        data
    }

    fn set_bits(data: &mut [u8], offset: usize, width: usize, value: u64) {
        for i in 0..width {
            let bit = offset + i;
            if (value >> i) & 1 == 1 {
                data[bit >> 3] |= 1 << (bit & 7);
            }
        }
    }

    /// DroneCAN broadcast CAN id for a Fix2 frame (priority 4, dtid 1063).
    /// The transfer id lives in the tail byte, not in the CAN id.
    pub fn fix2_can_id(src_node: u8) -> u32 {
        (4u32 << 24) | ((FIX2_DTID as u32) << 8) | (src_node as u32)
    }

    /// Transfer CRC over signature + payload, as the sender computes it.
    pub fn transfer_crc(payload: &[u8]) -> u16 {
        let mut crc = transfer_crc_seed(FIX2_SIGNATURE);
        for &b in payload {
            crc = crc_accumulate(b, crc);
        }
        crc
    }

    /// A frame mutation for corruption tests (non-capturing closures coerce
    /// to `&'static`).
    pub type FrameOverride = &'static dyn Fn(&mut CanFrame);

    /// Splits `payload` into multi-frame CAN frames (7 payload bytes per
    /// frame; the first carries the 2-byte CRC, little-endian).
    ///
    /// `overrides` lets a test corrupt a specific frame (e.g. toggle or TID
    /// or payload bytes); each override is applied on top of the defaults.
    pub fn multi_frame(
        src_node: u8,
        tid: u8,
        payload: &[u8],
        overrides: &[(usize, FrameOverride)],
    ) -> Vec<CanFrame> {
        let total = payload.len() + TRANSFER_CRC_BYTES;
        let nframes = if total <= 7 {
            1
        } else {
            1 + (total - 7).div_ceil(7)
        };
        let mut frames = Vec::with_capacity(nframes);

        let mut off = 0usize;
        for i in 0..nframes {
            let mut data = [0u8; 8];
            let count;
        if i == 0 {
                // First frame: CRC first, then payload.
                let crc = transfer_crc(payload);
                data[0] = crc as u8;
                data[1] = (crc >> 8) as u8;
                let n = (payload.len()).min(5);
                data[2..2 + n].copy_from_slice(&payload[..n]);
                count = 2 + n;
                off = n;
            } else {
                let n = (payload.len() - off).min(7);
                data[..n].copy_from_slice(&payload[off..off + n]);
                count = n;
                off += n;
            }

            let sot = i == 0;
            let eot = i == nframes - 1;
            let toggle = i != 0 && i % 2 == 1;
            let mut tail = tid & TID_MASK;
            if sot {
                tail |= 0x80;
            }
            if eot {
                tail |= 0x40;
            }
            if toggle {
                tail |= 0x20;
            }
            data[count] = tail;

            let mut f = CanFrame {
                id: fix2_can_id(src_node),
                dlc: (count + 1) as u8,
                data,
            };
            for (idx, apply) in overrides {
                if *idx == i {
                    apply(&mut f);
                }
            }
            frames.push(f);
        }
        frames
    }

    /// A single-frame transfer (payload <= 7 bytes, no CRC).
    pub fn single_frame(src_node: u8, tid: u8, payload: &[u8]) -> CanFrame {
        let mut data = [0u8; 8];
        let n = payload.len().min(7);
        data[..n].copy_from_slice(&payload[..n]);
        data[n] = 0x80 | 0x40 | (tid & TID_MASK);
        CanFrame {
            id: fix2_can_id(src_node),
            dlc: (n + 1) as u8,
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::test_support::{
        fix2_can_id, multi_frame, pack_fix2, single_frame, Fix2Fields,
    };
    use rid_interface::CanFrame;

    fn default_payload() -> [u8; 48] {
        pack_fix2(&Fix2Fields::default())
    }

    fn feed_all(p: &mut DronecanParser, frames: &[CanFrame], now_ms: u32) {
        for f in frames {
            p.feed(f, now_ms);
        }
    }

    #[test]
    fn reassembles_multi_frame_transfer_and_decodes_fix2() {
        let mut p = DronecanParser::new();
        let frames = multi_frame(1, 0, &default_payload(), &[]);
        assert_eq!(frames.len(), 8);
        feed_all(&mut p, &frames, 1000);
        let gps = p.get(1000).expect("gps after a complete transfer");
        assert!((gps.latitude - 45.30405).abs() < 1e-9);
        assert!((gps.longitude - 9.3875).abs() < 1e-9);
        assert!((gps.altitude_msl - 1050.0).abs() < 1e-3);
        assert!((gps.altitude_relative - 1234.0).abs() < 1e-3);
        assert!((gps.speed - 10.307764).abs() < 1e-3);
        assert_eq!(gps.heading, 346);
        assert_eq!(gps.fix_type, 3);
        assert_eq!(gps.satellites, 12);
        assert!(p.is_active());
    }

    #[test]
    fn crc_mismatch_discards_the_transfer() {
        let mut p = DronecanParser::new();
        let frames = multi_frame(1, 0, &default_payload(), &[(
            3,
            &|f: &mut CanFrame| f.data[1] ^= 0xFF,
        )]);
        feed_all(&mut p, &frames, 1000);
        assert!(p.get(1000).is_none());
    }

    #[test]
    fn wrong_toggle_discards_the_transfer() {
        let mut p = DronecanParser::new();
        let frames = multi_frame(1, 0, &default_payload(), &[(
            1,
            &|f: &mut CanFrame| f.data[7] &= !0x20, // clear toggle on frame 1
        )]);
        feed_all(&mut p, &frames, 1000);
        assert!(p.get(1000).is_none());
    }

    #[test]
    fn truncated_transfer_never_completes() {
        let mut p = DronecanParser::new();
        let frames = multi_frame(1, 0, &default_payload(), &[]);
        feed_all(&mut p, &frames[..7], 1000);
        assert!(p.get(1000).is_none());
    }

    #[test]
    fn next_transfer_uses_next_tid() {
        let mut p = DronecanParser::new();
        let payload = default_payload();
        let t0 = multi_frame(1, 0, &payload, &[]);
        feed_all(&mut p, &t0, 1000);
        assert!(p.get(1000).is_some());

        // TID advances to 1 for the next transfer; the parser must accept it.
        let t1 = multi_frame(1, 1, &payload, &[]);
        feed_all(&mut p, &t1, 2000);
        assert!(p.get(2000).is_some());
    }

    #[test]
    fn interleaved_nodes_do_not_cross_contaminate() {
        let mut p = DronecanParser::new();
        let a = multi_frame(1, 0, &default_payload(), &[]);
        let b_fields = Fix2Fields {
            lat_deg_1e8: 40_000_000_000, // 40.0 deg
            status: 2,
            ..Fix2Fields::default()
        };
        let b = multi_frame(2, 0, &pack_fix2(&b_fields), &[]);

        // Interleave frame by frame.
        for i in 0..a.len().max(b.len()) {
            if let Some(f) = a.get(i) {
                p.feed(f, 1500);
            }
            if let Some(f) = b.get(i) {
                p.feed(f, 1500);
            }
        }
        let gps = p.get(2000).expect("gps from the interleaved transfers");
        // Either node completed last; values must be self-consistent.
        assert!(gps.fix_type == 3 || gps.fix_type == 2);
    }

    #[test]
    fn freshness_window_expires() {
        let mut p = DronecanParser::new();
        let frames = multi_frame(1, 0, &default_payload(), &[]);
        feed_all(&mut p, &frames, 1000);
        assert!(p.get(1000).is_some());
        // 4 s later (wrap-safe) still fresh.
        assert!(p.get(5000).is_some());
        // Past the 5 s window.
        assert!(p.get(7000).is_none());
    }

    #[test]
    fn no_fix_status_keeps_old_timestamp() {
        let mut p = DronecanParser::new();
        let fields = Fix2Fields {
            status: 1, // TIME_ONLY
            ..Fix2Fields::default()
        };
        let frames = multi_frame(1, 0, &pack_fix2(&fields), &[]);
        feed_all(&mut p, &frames, 1000);
        assert!(p.get(1000).is_none(), "no fix -> not fresh");
        assert_eq!(p.gps.satellites, 12, "satellites still decoded");
    }

    #[test]
    fn non_fix2_messages_are_ignored() {
        let mut p = DronecanParser::new();
        // A Fix2 CAN id is required; use a bogus dtid instead.
        let mut f = single_frame(1, 0, &[0u8; 7]);
        f.id = (4u32 << 24) | (1000u32 << 8) | 1; // uavcan.equipment.ahrs.Solution
        p.feed(&f, 1000);
        assert!(p.get(1000).is_none());
    }

    #[test]
    fn single_frame_fix2_cannot_carry_a_payload() {
        let mut p = DronecanParser::new();
        let f = single_frame(1, 0, &[0u8; 7]);
        p.feed(&f, 1000);
        assert!(p.get(1000).is_none());
    }

    #[test]
    fn id_parse_rejects_dlc_zero() {
        let mut p = DronecanParser::new();
        let f = CanFrame {
            id: fix2_can_id(1),
            dlc: 0,
            data: [0; 8],
        };
        p.feed(&f, 1000);
        assert!(!p.get(1000).is_some());
    }

    #[test]
    fn negative_coordinates_sign_extend() {
        let mut p = DronecanParser::new();
        let fields = Fix2Fields {
            lat_deg_1e8: -4_530_405_000, // -45.30405 deg
            lon_deg_1e8: -938_750_000, // -9.3875 deg
            ..Fix2Fields::default()
        };
        let frames = multi_frame(1, 0, &pack_fix2(&fields), &[]);
        feed_all(&mut p, &frames, 1000);
        let gps = p.get(1000).unwrap();
        assert!((gps.latitude + 45.30405).abs() < 1e-9);
        assert!((gps.longitude + 9.3875).abs() < 1e-9);
    }

    #[test]
    fn crc_accumulate_matches_ccitt_false_reference() {
        // CRC-16/CCITT-FALSE("123456789") == 0x29B1 (known check value).
        let mut crc = 0xFFFFu16;
        for b in b"123456789" {
            crc = crc_accumulate(*b, crc);
        }
        assert_eq!(crc, 0x29B1);
    }
}
