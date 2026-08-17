//! Input protocol auto-detection, port of `protocol_detect_auto()` from
//! `protocol_detect.c`. The UART plumbing (`init`/`reinit`) is hardware and
//! stays in the BSP; this module only maps a byte buffer to the protocol that
//! should parse it. The default fallback is NMEA, exactly like the C.

use rid_interface::Protocol;

/// Port of `protocol_detect_auto()`: inspects the bytes read from the UART
/// and returns the detected protocol.
///
/// Order of checks (kept identical to the C):
/// 1. empty read -> `Unknown`;
/// 2. `$M<` prefix -> MSP;
/// 3. `$G`/`$N` prefix -> NMEA;
/// 4. a MAVLink header (`0xFE` v1 or `0xFD` v2) whose payload length fits in
///    the buffer -> MAVLink;
/// 5. anything else -> NMEA.
pub fn detect_protocol(buf: &[u8]) -> Protocol {
    if buf.is_empty() {
        return Protocol::Unknown;
    }

    if buf.len() >= 3 && buf[0] == b'$' && buf[1] == b'M' && buf[2] == b'<' {
        return Protocol::Msp;
    }

    if buf.len() >= 3 && buf[0] == b'$' && (buf[1] == b'G' || buf[1] == b'N') {
        return Protocol::Nmea;
    }

    for i in 0..buf.len().saturating_sub(1) {
        if buf[i] == 0xFE || buf[i] == 0xFD {
            let msg_len = buf[i + 1];
            if msg_len > 0 && msg_len < 255 && (i + msg_len as usize + 6) <= buf.len() {
                return Protocol::Mavlink;
            }
        }
    }

    Protocol::Nmea
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_read_is_unknown() {
        assert_eq!(detect_protocol(b""), Protocol::Unknown);
    }

    #[test]
    fn msp_prefix() {
        assert_eq!(detect_protocol(b"$M<\x08\x00\x00"), Protocol::Msp);
        assert_eq!(detect_protocol(b"$M<"), Protocol::Msp);
    }

    #[test]
    fn nmea_prefix() {
        assert_eq!(detect_protocol(b"$GPGGA,1234.5,..."), Protocol::Nmea);
        assert_eq!(detect_protocol(b"$NRMCNMV,0.5,..."), Protocol::Nmea);
    }

    #[test]
    fn mavlink_header_detected_anywhere() {
        // 0xFE (v1) at offset 0: needs i + len + 6 <= buf.len().
        let mut buf = [0u8; 26]; // 26 >= 20 + 6
        buf[0] = 0xFE;
        buf[1] = 20;
        assert_eq!(detect_protocol(&buf), Protocol::Mavlink);
    }

    #[test]
    fn mavlink_v2_header_detected() {
        let mut buf = [0u8; 34];
        buf[0] = 0xFD;
        buf[1] = 25;
        assert_eq!(detect_protocol(&buf), Protocol::Mavlink);
    }

    #[test]
    fn mavlink_rejected_when_payload_does_not_fit() {
        // Header present but i + len + 6 > buf.len(): the C falls through.
        let buf = [0xFE, 20, 0, 0, 0, 0];
        assert_eq!(detect_protocol(&buf), Protocol::Nmea);
    }

    #[test]
    fn mavlink_rejected_for_zero_or_max_length() {
        // msg_len 0 and 255 are explicitly rejected by the C check.
        let buf = [0xFE, 0x00];
        assert_eq!(detect_protocol(&buf), Protocol::Nmea);
        let buf = [0xFD, 0xFF];
        assert_eq!(detect_protocol(&buf), Protocol::Nmea);
    }

    #[test]
    fn tiny_buffer_with_marker_is_not_mavlink() {
        // len 2, marker + len=1: i + 1 + 6 = 7 > 2.
        let buf = [0xFE, 0x01];
        assert_eq!(detect_protocol(&buf), Protocol::Nmea);
    }

    #[test]
    fn msp_nmea_take_priority_over_mavlink_marker() {
        // Even though 0xFE appears later, the $M< prefix wins (C order).
        let buf = b"$M<\x00\x00\xFE";
        assert_eq!(detect_protocol(buf), Protocol::Msp);
    }

    #[test]
    fn garbage_falls_back_to_nmea() {
        assert_eq!(detect_protocol(b"hello world"), Protocol::Nmea);
        assert_eq!(detect_protocol(b"\x01\x02\x03"), Protocol::Nmea);
    }
}
