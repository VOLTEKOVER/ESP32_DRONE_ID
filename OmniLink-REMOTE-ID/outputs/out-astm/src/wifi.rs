//! IEEE 802.11 framing for the WiFi broadcast transports.
//!
//! Ports of the frame builders from `wifi.c`:
//! - `build_beacon_frame` → `odid_wifi_build_message_pack_beacon_frame`
//!   (used by `wifi_tx_transmit`);
//! - `build_nan_action_frame` → `odid_wifi_build_message_pack_nan_action_frame`
//!   (used by `wifi_tx_transmit_nan`).
//!
//! The packed structs of `odid_wifi.h` are assembled byte-by-byte (no_std, no
//! alignment, endianness-safe). The beacon timestamp comes from the caller
//! (the BSP provides the monotonic clock) so the frames are deterministic.

use crate::pack::{build_pack, PackError};
use opendroneid_sys::UasData;

/// Size of the IEEE 802.11 management header (`struct ieee80211_mgmt`).
const MGMT_HDR_SIZE: usize = 24;
/// Size of the beacon fixed fields (`struct ieee80211_beacon`).
const BEACON_SIZE: usize = 12;

/// IEEE 802.11 management frame subtype BEACON (bit 4 set).
const STYPE_BEACON: u16 = 0x0080;
/// IEEE 802.11 management frame subtype ACTION (bits 4-7 = 0xD).
const STYPE_ACTION: u16 = 0x00D0;

/// `IEEE80211_CAPINFO_SHORT_SLOTTIME | IEEE80211_CAPINFO_SHORT_PREAMBLE`.
const CAPABILITY: u16 = 0x0420;

/// Return codes of the `odid_wifi_build_*` functions (negative in C).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FrameError {
    /// Output buffer too small (-ENOMEM).
    NoMem,
    /// Invalid argument, e.g. empty/oversized SSID (-EINVAL).
    Invalid,
    /// The embedded message pack could not be built.
    Pack(PackError),
}

impl From<PackError> for FrameError {
    fn from(e: PackError) -> Self {
        match e {
            PackError::TooManyMessages | PackError::NoMessages => FrameError::Invalid,
            PackError::BufferTooSmall => FrameError::NoMem,
        }
    }
}

/// Writes the 24-byte management header at `buf[offset..]`, port of
/// `buf_fill_ieee80211_mgmt` (FTYPE_MGMT is 0, so `frame_control` is the
/// subtype itself; all little-endian fields are written explicitly).
fn fill_ieee80211_mgmt(
    buf: &mut [u8],
    offset: usize,
    subtype: u16,
    da: &[u8; 6],
    sa: &[u8; 6],
    bssid: &[u8; 6],
) -> Result<(), FrameError> {
    if offset + MGMT_HDR_SIZE > buf.len() {
        return Err(FrameError::NoMem);
    }
    buf[offset..offset + 2].copy_from_slice(&subtype.to_le_bytes());
    buf[offset + 2..offset + 4].fill(0); // duration
    buf[offset + 4..offset + 10].copy_from_slice(da);
    buf[offset + 10..offset + 16].copy_from_slice(sa);
    buf[offset + 16..offset + 22].copy_from_slice(bssid);
    buf[offset + 22..offset + 24].fill(0); // seq_ctrl
    Ok(())
}

/// Writes the 12-byte beacon fixed fields at `buf[offset..]`, port of
/// `buf_fill_ieee80211_beacon`. The monotonic timestamp is injected by the
/// caller instead of `clock_gettime` so the frame is deterministic.
fn fill_ieee80211_beacon(
    buf: &mut [u8],
    offset: usize,
    interval_tu: u16,
    timestamp_us: u64,
) -> Result<(), FrameError> {
    if offset + BEACON_SIZE > buf.len() {
        return Err(FrameError::NoMem);
    }
    buf[offset..offset + 8].copy_from_slice(&timestamp_us.to_le_bytes());
    buf[offset + 8..offset + 10].copy_from_slice(&interval_tu.to_le_bytes());
    buf[offset + 10..offset + 12].copy_from_slice(&CAPABILITY.to_le_bytes());
    Ok(())
}

/// Port of `odid_wifi_build_message_pack_beacon_frame`: builds the IEEE 802.11
/// beacon carrying the ASTM message pack in the vendor-specific IE 221, with
/// `mac` as SA/BSSID, `ssid` as the network name (1-32 bytes) and the pack
/// preceded by the OpenDroneID service info counter.
pub fn build_beacon_frame(
    uas: &UasData,
    mac: &[u8; 6],
    ssid: &[u8],
    interval_tu: u16,
    send_counter: u8,
    timestamp_us: u64,
    buf: &mut [u8],
) -> Result<usize, FrameError> {
    if ssid.is_empty() || ssid.len() > 32 {
        return Err(FrameError::Invalid);
    }

    const BROADCAST: [u8; 6] = [0xFF; 6];
    const ASD_STAN_OUI: [u8; 3] = [0xFA, 0x0B, 0xBC];

    let mut len = 0usize;

    fill_ieee80211_mgmt(buf, len, STYPE_BEACON, &BROADCAST, mac, mac)?;
    len += MGMT_HDR_SIZE;

    fill_ieee80211_beacon(buf, len, interval_tu, timestamp_us)?;
    len += BEACON_SIZE;

    // SSID information element (0x00).
    if len + 2 + ssid.len() > buf.len() {
        return Err(FrameError::NoMem);
    }
    buf[len] = 0x00;
    buf[len + 1] = ssid.len() as u8;
    buf[len + 2..len + 2 + ssid.len()].copy_from_slice(ssid);
    len += 2 + ssid.len();

    // Supported rates (0x01): a single 6 Mbps rate.
    if len + 3 > buf.len() {
        return Err(FrameError::NoMem);
    }
    buf[len] = 0x01;
    buf[len + 1] = 0x01;
    buf[len + 2] = 0x8C;
    len += 3;

    // Vendor-specific information element 221 (0xDD); length patched at the end.
    if len + 6 > buf.len() {
        return Err(FrameError::NoMem);
    }
    let vendor_len_off = len + 1;
    buf[len] = 0xDD;
    buf[len + 1] = 0x00;
    buf[len + 2..len + 5].copy_from_slice(&ASD_STAN_OUI);
    buf[len + 5] = 0x0D;
    len += 6;

    // OpenDroneID service info: message counter.
    if len + 1 > buf.len() {
        return Err(FrameError::NoMem);
    }
    buf[len] = send_counter;
    len += 1;

    // Message pack (`odid_message_build_pack`).
    let pack_len = build_pack(uas, &mut buf[len..])?;
    len += pack_len;

    // vendor.length = OUI(3) + OUI type(1) + service info(1) + pack.
    buf[vendor_len_off] = (3 + 1 + 1 + pack_len) as u8;
    Ok(len)
}

/// Port of `odid_wifi_build_message_pack_nan_action_frame`: NAN service
/// discovery action frame carrying the message pack as service info, with the
/// NAN cluster ID as BSSID and the fixed OpenDroneID service hash.
pub fn build_nan_action_frame(
    uas: &UasData,
    mac: &[u8; 6],
    send_counter: u8,
    buf: &mut [u8],
) -> Result<usize, FrameError> {
    const TARGET_ADDR: [u8; 6] = [0x51, 0x6F, 0x9A, 0x01, 0x00, 0x00];
    const WIFI_ALLIANCE_OUI: [u8; 3] = [0x50, 0x6F, 0x9A];
    const SERVICE_ID: [u8; 6] = [0x88, 0x69, 0x19, 0x9D, 0x92, 0x09];
    const CLUSTER_ID: [u8; 6] = [0x50, 0x6F, 0x9A, 0x01, 0x00, 0xFF];

    let mut len = 0usize;

    fill_ieee80211_mgmt(buf, len, STYPE_ACTION, &TARGET_ADDR, mac, &CLUSTER_ID)?;
    len += MGMT_HDR_SIZE;

    // NAN service discovery header (Public Action frame, vendor specific).
    if len + 6 > buf.len() {
        return Err(FrameError::NoMem);
    }
    buf[len] = 0x04; // category
    buf[len + 1] = 0x09; // action code
    buf[len + 2..len + 5].copy_from_slice(&WIFI_ALLIANCE_OUI);
    buf[len + 5] = 0x13; // oui_type
    len += 6;

    // NAN service descriptor attribute; lengths patched after the pack.
    if len + 13 > buf.len() {
        return Err(FrameError::NoMem);
    }
    let nsda_len_off = len + 1;
    let nsda_info_len_off = len + 12;
    buf[len] = 0x03; // attribute_id
    buf[len + 1] = 0x00; // header.length (LE, patched)
    buf[len + 2] = 0x00;
    buf[len + 3..len + 9].copy_from_slice(&SERVICE_ID);
    buf[len + 9] = 0x01; // instance_id (always 1)
    buf[len + 10] = 0x00; // requestor_instance_id
    buf[len + 11] = 0x10; // service_control: follow up
    buf[len + 12] = 0x00; // service_info_length (patched)
    len += 13;

    // OpenDroneID service info: message counter.
    if len + 1 > buf.len() {
        return Err(FrameError::NoMem);
    }
    buf[len] = send_counter;
    len += 1;

    let pack_len = build_pack(uas, &mut buf[len..])?;
    len += pack_len;

    // service_info_length = sizeof(si) + pack; header.length =
    // sizeof(nsda) - sizeof(header) + service_info_length.
    let service_info_length = 1 + pack_len;
    buf[nsda_len_off..nsda_len_off + 2]
        .copy_from_slice(&((13 - 3 + service_info_length) as u16).to_le_bytes());
    buf[nsda_info_len_off] = service_info_length as u8;

    // NAN service descriptor extension attribute.
    if len + 7 > buf.len() {
        return Err(FrameError::NoMem);
    }
    buf[len] = 0x0E; // attribute_id
    buf[len + 1..len + 3].copy_from_slice(&0x0004u16.to_le_bytes());
    buf[len + 3] = 0x01; // instance_id
    buf[len + 4..len + 6].copy_from_slice(&0x0200u16.to_le_bytes()); // control
    buf[len + 6] = send_counter; // service_update_indicator
    len += 7;

    Ok(len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendroneid_sys::{decode_message_type, ODID_MESSAGETYPE_BASIC_ID, ODID_MESSAGE_SIZE};
    use rid_interface::fixed_str;

    use crate::pack::{build_pack, MAX_PACK_LEN};
    use crate::build_uas_data;
    use rid_interface::types::{GpsData, Identity};

    /// UAS snapshot with a deterministic 4-message pack (basic/location/system/
    /// operator), like `rid_output_build_uas` produces.
    fn uas() -> UasData {
        let g = GpsData {
            latitude: 45.30405,
            longitude: 11.95375,
            altitude_msl: 123.4,
            altitude_relative: 60.0,
            speed: 12.0,
            speed_vertical: 0.0,
            heading: 90,
            fix_type: 4,
            satellites: 12,
            armed: true,
            ..GpsData::default()
        };
        let id = Identity {
            uas_id: fixed_str("ESP32-RID-001"),
            operator_id: fixed_str("OP-123456"),
            ..Identity::default()
        };
        build_uas_data(&g, &id, None)
    }

    fn pack_len(uas: &UasData) -> usize {
        let mut buf = [0u8; MAX_PACK_LEN];
        build_pack(uas, &mut buf).unwrap()
    }

    const MAC: [u8; 6] = [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01];

    #[test]
    fn beacon_frame_layout_is_exact() {
        let uas = uas();
        let plen = pack_len(&uas);
        let ssid = b"ESP-RID";
        let mut buf = [0u8; 1024];
        let len = build_beacon_frame(&uas, &MAC, ssid, 100, 0x42, 0x0123_4567_89AB_CDEF, &mut buf)
            .unwrap();

        // 24 (mgmt) + 12 (beacon) + 2+7 (ssid) + 3 (rates) + 6 (vendor) + 1 (si) + pack.
        assert_eq!(len, 55 + plen);

        // Management header: BEACON subtype, broadcast DA, MAC as SA/BSSID.
        assert_eq!(&buf[0..2], &[0x80, 0x00]);
        assert_eq!(&buf[2..4], &[0, 0], "duration");
        assert_eq!(&buf[4..10], &[0xFF; 6]);
        assert_eq!(&buf[10..16], &MAC);
        assert_eq!(&buf[16..22], &MAC);
        assert_eq!(&buf[22..24], &[0, 0], "seq_ctrl");

        // Beacon fixed fields: timestamp LE, interval, capability 0x0420.
        assert_eq!(
            u64::from_le_bytes(buf[24..32].try_into().unwrap()),
            0x0123_4567_89AB_CDEF
        );
        assert_eq!(u16::from_le_bytes(buf[32..34].try_into().unwrap()), 100);
        assert_eq!(u16::from_le_bytes(buf[34..36].try_into().unwrap()), 0x0420);

        // SSID IE, rates IE, vendor IE (221) with ASD-STAN OUI.
        assert_eq!(&buf[36..45], &[0x00, 0x07, b'E', b'S', b'P', b'-', b'R', b'I', b'D']);
        assert_eq!(&buf[45..48], &[0x01, 0x01, 0x8C]);
        assert_eq!(&buf[48..54], &[0xDD, (3 + 1 + 1 + plen) as u8, 0xFA, 0x0B, 0xBC, 0x0D]);

        // Service info counter then the pack header at offset 55.
        assert_eq!(buf[54], 0x42);
        assert_eq!(buf[55], 0xF2);
        assert_eq!(buf[56], ODID_MESSAGE_SIZE as u8);
        assert_eq!(buf[57], 4);
        assert_eq!(decode_message_type(buf[58]), ODID_MESSAGETYPE_BASIC_ID);
    }

    #[test]
    fn beacon_ssid_length_limits() {
        let uas = uas();
        let mut buf = [0u8; 1024];
        assert_eq!(
            build_beacon_frame(&uas, &MAC, b"", 100, 0, 0, &mut buf),
            Err(FrameError::Invalid)
        );
        assert_eq!(
            build_beacon_frame(&uas, &MAC, &[b'x'; 33], 100, 0, 0, &mut buf),
            Err(FrameError::Invalid)
        );
        assert_eq!(
            build_beacon_frame(&uas, &MAC, &[b'x'; 32], 100, 0, 0, &mut buf),
            Ok(24 + 12 + 2 + 32 + 3 + 6 + 1 + pack_len(&uas))
        );
    }

    #[test]
    fn beacon_frame_too_small_buffer() {
        let uas = uas();
        let mut buf = [0u8; 24];
        assert_eq!(
            build_beacon_frame(&uas, &MAC, b"ESP-RID", 100, 0, 0, &mut buf),
            Err(FrameError::NoMem)
        );
    }

    #[test]
    fn beacon_frame_propagates_pack_errors() {
        // No valid messages -> the pack fails with -EINVAL.
        let empty = opendroneid_sys::init_uas_data();
        let mut buf = [0u8; 1024];
        assert_eq!(
            build_beacon_frame(&empty, &MAC, b"ESP-RID", 100, 0, 0, &mut buf),
            Err(FrameError::Invalid)
        );
    }

    #[test]
    fn nan_action_frame_layout_is_exact() {
        let uas = uas();
        let plen = pack_len(&uas);
        let mut buf = [0u8; 1024];
        let len = build_nan_action_frame(&uas, &MAC, 0x07, &mut buf).unwrap();

        // 24 (mgmt) + 6 (nsd) + 13 (nsda) + 1 (si) + pack + 7 (nsdea).
        assert_eq!(len, 51 + plen);

        // Management header: ACTION subtype, NAN dest, MAC as SA, cluster BSSID.
        assert_eq!(&buf[0..2], &[0xD0, 0x00]);
        assert_eq!(&buf[4..10], &[0x51, 0x6F, 0x9A, 0x01, 0x00, 0x00]);
        assert_eq!(&buf[10..16], &MAC);
        assert_eq!(&buf[16..22], &[0x50, 0x6F, 0x9A, 0x01, 0x00, 0xFF]);

        // NAN service discovery header.
        assert_eq!(&buf[24..30], &[0x04, 0x09, 0x50, 0x6F, 0x9A, 0x13]);

        // Service descriptor attribute: id 0x3, length LE, service hash, 0x10.
        assert_eq!(&buf[30..33], &[0x03, (11 + plen) as u8, 0x00]);
        assert_eq!(&buf[33..39], &[0x88, 0x69, 0x19, 0x9D, 0x92, 0x09]);
        assert_eq!(&buf[39..43], &[0x01, 0x00, 0x10, (1 + plen) as u8]);

        // Service info counter then the pack header at offset 44.
        assert_eq!(buf[43], 0x07);
        assert_eq!(buf[44], 0xF2);
        assert_eq!(buf[45], ODID_MESSAGE_SIZE as u8);
        assert_eq!(buf[46], 4);

        // Extension attribute at 44 + plen.
        let e = 44 + plen;
        assert_eq!(buf[e], 0x0E);
        assert_eq!(&buf[e + 1..e + 3], &[0x04, 0x00]);
        assert_eq!(buf[e + 3], 0x01);
        assert_eq!(&buf[e + 4..e + 6], &[0x00, 0x02], "control 0x0200 LE");
        assert_eq!(buf[e + 6], 0x07, "service_update_indicator");
    }

    #[test]
    fn nan_action_frame_too_small_buffer() {
        let uas = uas();
        let mut buf = [0u8; 23];
        assert_eq!(
            build_nan_action_frame(&uas, &MAC, 0, &mut buf),
            Err(FrameError::NoMem)
        );
    }
}
