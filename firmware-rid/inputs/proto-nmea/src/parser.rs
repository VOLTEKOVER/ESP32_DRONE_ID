//! Streaming NMEA parser, port of `nmea_parser.c`.
//!
//! Behavioural parity notes:
//! - The line buffer is `[u8; 256]`; a sentence is parsed on `'\n'` (dropped)
//!   or when the buffer would overflow (the overflowing byte is dropped).
//! - A line is parsed only when it starts with `'$'` and has > 5 chars.
//! - Fields are split on `','` with `strtok` semantics: runs of commas are a
//!   single separator, so empty fields are skipped (and shift the indices).
//! - The sentence is truncated at `'*'` (checksum) before tokenizing.
//! - Only GGA, RMC and VTG are handled; the last valid GPS snapshot is kept
//!   and returned while `fix_type >= 2 && latitude != 0.0`.
//! - `lat_dir == 0` (missing longitude direction, would be a NULL deref in C)
//!   is treated as north/east.

use rid_interface::GpsData;

/// `NMEA_BUF_SIZE` from the C source.
pub const NMEA_BUF_SIZE: usize = 256;
/// `sizeof(work)` in `parse_nmea_line`.
const LINE_MAX: usize = 128;
/// Maximum token count (`char *fields[16]`).
const MAX_FIELDS: usize = 16;

/// `nmea_to_decimal()`: `ddmm.mmmm` + direction to decimal degrees.
///
/// Returns 0.0 for empty/too-short input or when the dot sits before index 4
/// (mirrors the C early-outs).
fn nmea_to_decimal(s: &[u8], dir: u8) -> f64 {
    if s.len() < 4 {
        return 0.0;
    }
    let dot = s.iter().position(|&b| b == b'.');
    let dot_pos = match dot {
        Some(p) => p,
        None => return 0.0,
    };
    if dot_pos < 4 {
        return 0.0;
    }
    let mut deg_len = dot_pos - 2;
    if deg_len >= 4 {
        deg_len = 3;
    }
    let degrees = c_atoi(&s[..deg_len]);
    let minutes = c_atof(&s[deg_len..]);
    let mut decimal = degrees as f64 + minutes / 60.0;
    if dir == b'S' || dir == b'W' {
        decimal = -decimal;
    }
    decimal
}

/// `atof()`-style parse: leading whitespace, sign, digits, fraction, exponent.
/// Stops at the first unsupported char (like the C runtime).
fn c_atof(s: &[u8]) -> f64 {
    let mut i = 0;
    while i < s.len() && (s[i] == b' ' || s[i] == b'\t') {
        i += 1;
    }
    let mut neg = false;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        neg = s[i] == b'-';
        i += 1;
    }
    let mut int: i64 = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        int = int.saturating_mul(10).saturating_add((s[i] - b'0') as i64);
        i += 1;
    }
    let mut frac: f64 = 0.0;
    let mut frac_digits: i32 = 0;
    if i < s.len() && s[i] == b'.' {
        i += 1;
        while i < s.len() && s[i].is_ascii_digit() {
            frac = frac * 10.0 + (s[i] - b'0') as f64;
            frac_digits += 1;
            i += 1;
        }
    }
    // `10f64.powi` is not available in `no_std` core: compute with a loop.
    let mut scale = 1.0f64;
    for _ in 0..frac_digits {
        scale *= 10.0;
    }
    let mut value = int as f64 + frac / scale;
    if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
        i += 1;
        let mut e_neg = false;
        if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
            e_neg = s[i] == b'-';
            i += 1;
        }
        let mut exp: i32 = 0;
        let mut exp_seen = false;
        while i < s.len() && s[i].is_ascii_digit() {
            exp = exp.saturating_mul(10).saturating_add((s[i] - b'0') as i32);
            exp_seen = true;
            i += 1;
        }
        if exp_seen {
            let mut factor = 1.0f64;
            for _ in 0..exp {
                factor *= 10.0;
            }
            if e_neg {
                value /= factor;
            } else {
                value *= factor;
            }
        }
    }
    if neg {
        -value
    } else {
        value
    }
}

/// `atoi()`-style parse: leading whitespace, sign, digits, stop at first
/// non-digit. Overflow saturates instead of invoking UB.
fn c_atoi(s: &[u8]) -> i32 {
    let mut i = 0;
    while i < s.len() && (s[i] == b' ' || s[i] == b'\t') {
        i += 1;
    }
    let mut neg = false;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        neg = s[i] == b'-';
        i += 1;
    }
    let mut v: i32 = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        v = v.saturating_mul(10).saturating_add((s[i] - b'0') as i32);
        i += 1;
    }
    if neg {
        -v
    } else {
        v
    }
}

/// Streaming NMEA parser with the same buffer semantics as the C module.
pub struct NmeaParser {
    buf: [u8; NMEA_BUF_SIZE],
    idx: usize,
    last_gps: GpsData,
}

impl Default for NmeaParser {
    fn default() -> Self {
        Self::new()
    }
}

impl NmeaParser {
    /// `nmea_parser_init()`: empty buffer, zeroed GPS snapshot.
    pub fn new() -> Self {
        Self {
            buf: [0; NMEA_BUF_SIZE],
            idx: 0,
            last_gps: GpsData::default(),
        }
    }

    /// Port of the byte loop in `nmea_parser_get()`.
    pub fn feed(&mut self, bytes: &[u8]) {
        for &c in bytes {
            if c == b'\n' || self.idx >= NMEA_BUF_SIZE - 1 {
                if self.buf[0] == b'$' && self.idx > 5 {
                    // Copy the line out to release the borrow on `self.buf`.
                    let mut line = [0u8; NMEA_BUF_SIZE];
                    line[..self.idx].copy_from_slice(&self.buf[..self.idx]);
                    self.parse_line(&line[..self.idx]);
                }
                self.idx = 0;
                self.buf = [0; NMEA_BUF_SIZE];
            } else {
                self.buf[self.idx] = c;
                self.idx += 1;
            }
        }
    }

    /// Port of the trailing check in `nmea_parser_get()`: returns the last
    /// valid snapshot (3D fix with non-zero latitude) or `None`.
    pub fn get(&self) -> Option<GpsData> {
        if self.last_gps.fix_type >= 2 && self.last_gps.latitude != 0.0 {
            Some(self.last_gps)
        } else {
            None
        }
    }

    /// Port of `parse_nmea_line()`.
    fn parse_line(&mut self, line: &[u8]) {
        let n = line.len().min(LINE_MAX - 1);
        let mut work = [0u8; LINE_MAX];
        work[..n].copy_from_slice(&line[..n]);
        let work = &work[..n];

        // Strip the checksum: everything from '*' on is dropped.
        let work = match work.iter().position(|&b| b == b'*') {
            Some(p) => &work[..p],
            None => work,
        };

        // `strtok(work, ",")`: comma runs are one separator (empties skipped).
        let mut fields = [&b""[..]; MAX_FIELDS];
        let mut count = 0;
        let mut start = 0;
        let mut i = 0;
        while i < work.len() {
            if work[i] == b',' {
                if i > start && count < MAX_FIELDS {
                    fields[count] = &work[start..i];
                    count += 1;
                }
                start = i + 1;
            }
            i += 1;
        }
        if start < work.len() && count < MAX_FIELDS {
            fields[count] = &work[start..];
            count += 1;
        }
        if count < 2 {
            return;
        }

        let f0 = fields[0];
        if f0 == b"$GPGGA" || f0 == b"$GNGGA" {
            self.parse_gga(&fields, count);
        } else if f0 == b"$GPRMC" || f0 == b"$GNRMC" {
            self.parse_rmc(&fields, count);
        } else if f0 == b"$GPVTG" || f0 == b"$GNVTG" {
            self.parse_vtg(&fields, count);
        }
    }

    /// Port of `parse_gga()`.
    fn parse_gga(&mut self, fields: &[&[u8]], count: usize) {
        if fields[2].is_empty() || fields[3].is_empty() || fields[4].is_empty() || fields[5].is_empty()
            || fields[6].is_empty()
        {
            return;
        }
        let fix = c_atoi(fields[6]);
        if fix < 1 {
            return;
        }
        self.last_gps.fix_type = if fix >= 2 { 3 } else { 1 };
        self.last_gps.latitude = nmea_to_decimal(fields[2], fields[3][0]);
        self.last_gps.longitude = nmea_to_decimal(fields[4], fields[5][0]);
        if count > 7 {
            self.last_gps.satellites = c_atoi(fields[7]) as u8;
        }
        if count > 9 {
            self.last_gps.altitude_msl = c_atof(fields[9]) as f32;
            self.last_gps.altitude_baro = self.last_gps.altitude_msl;
        }
    }

    /// Port of `parse_rmc()`.
    fn parse_rmc(&mut self, fields: &[&[u8]], count: usize) {
        if count <= 3 || count <= 4 || count <= 5 || fields[3].is_empty() || fields[4].is_empty()
            || fields[5].is_empty()
        {
            return;
        }
        if fields[2][0] != b'A' {
            return;
        }
        self.last_gps.latitude = nmea_to_decimal(fields[3], fields[4][0]);
        let lon_dir = if count > 6 { fields[6][0] } else { 0 };
        self.last_gps.longitude = nmea_to_decimal(fields[5], lon_dir);
        if count > 7 {
            self.last_gps.speed = (c_atof(fields[7]) * 0.514444) as f32;
        }
    }

    /// Port of `parse_vtg()`.
    fn parse_vtg(&mut self, fields: &[&[u8]], count: usize) {
        if count > 1 {
            self.last_gps.heading = c_atof(fields[1]) as i16;
        }
        if count > 5 {
            self.last_gps.speed = (c_atof(fields[5]) * 0.514444) as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rid_interface::CStr;

    fn parse(stream: &[u8]) -> Option<GpsData> {
        let mut p = NmeaParser::new();
        p.feed(stream);
        p.get()
    }

    #[test]
    fn gga_full_fix() {
        // $GPGGA: lat 48°07.038'N lon 011°31.000'E, fix=3, 8 sats, alt 545.4m
        let gps = parse(
            b"$GPGGA,123519,4807.038,N,01131.000,E,3,08,0.9,545.4,M,46.9,M,,*47\r\n",
        )
        .expect("valid fix");
        assert_eq!(gps.fix_type, 3);
        assert!((gps.latitude - 48.1173).abs() < 1e-9);
        assert!((gps.longitude - 11.5166666667).abs() < 1e-9);
        assert_eq!(gps.satellites, 8);
        assert!((gps.altitude_msl - 545.4).abs() < 1e-5);
        assert!((gps.altitude_baro - 545.4).abs() < 1e-5);
        // RMC not present yet: speed/heading stay at their defaults.
        assert_eq!(gps.speed, 0.0);
        assert_eq!(gps.heading, 0);
    }

    #[test]
    fn gga_fix_one_is_not_3d() {
        // fix=1 -> fix_type 1 -> `get()` stays None (needs fix_type >= 2).
        assert!(parse(b"$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M*47\r\n").is_none());
    }

    #[test]
    fn gga_fix_zero_ignored() {
        // fix=0: sentence accepted but the snapshot keeps previous values.
        let mut p = NmeaParser::new();
        p.feed(b"$GPGGA,1,4807.038,N,01131.000,E,3,08,0.9,545.4,M*47\r\n");
        p.feed(b"$GPGGA,2,0,0,0,0,0,0,0.0,0.0,M*47\r\n");
        let gps = p.get().expect("still valid after fix=0");
        assert!((gps.latitude - 48.1173).abs() < 1e-9);
        assert_eq!(gps.fix_type, 3);
    }

    #[test]
    fn rmc_sets_speed() {
        // RMC never sets `fix_type` (that is GGA's job), so a valid GGA must
        // come first for `get()` to report a 3D fix.
        let mut p = NmeaParser::new();
        p.feed(b"$GPGGA,123519,4807.038,N,01131.000,E,3,08,0.9,545.4,M*47\r\n");
        p.feed(b"$GPRMC,123519,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*6A\r\n");
        let gps = p.get().expect("valid rmc");
        assert!((gps.latitude - 48.1173).abs() < 1e-9);
        assert!((gps.longitude - 11.5166666667).abs() < 1e-9);
        assert!((gps.speed - 22.4 * 0.514444).abs() < 1e-5);
    }

    #[test]
    fn rmc_invalid_status_keeps_old_values() {
        let mut p = NmeaParser::new();
        p.feed(b"$GPGGA,123519,4807.038,N,01131.000,E,3,08,0.9,545.4,M*47\r\n");
        p.feed(b"$GPRMC,1,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*6A\r\n");
        p.feed(b"$GPRMC,2,V,0,0,0,0,000.0,000.0,230394,,*0A\r\n");
        let gps = p.get().expect("valid before void");
        assert!((gps.latitude - 48.1173).abs() < 1e-9);
        assert!((gps.speed - 22.4 * 0.514444).abs() < 1e-5); // RMC 'V' returned early
    }

    #[test]
    fn rmc_south_west_negates() {
        let mut p = NmeaParser::new();
        p.feed(b"$GPGGA,123519,4807.038,N,01131.000,E,3,08,0.9,545.4,M*47\r\n");
        p.feed(b"$GPRMC,123519,A,4807.038,S,01131.000,W,022.4,084.4,230394,003.1,E*6A\r\n");
        let gps = p.get().expect("valid rmc");
        assert!(gps.latitude < 0.0);
        assert!(gps.longitude < 0.0);
        assert!((gps.latitude + 48.1173).abs() < 1e-9);
        assert!((gps.longitude + 11.5166666667).abs() < 1e-9);
    }

    #[test]
    fn vtg_sets_heading_and_knots_speed() {
        // course 054.7 true, speed 005.5 knots -> 2.829442 m/s.
        let mut p = NmeaParser::new();
        p.feed(b"$GPGGA,123519,4807.038,N,01131.000,E,3,08,0.9,545.4,M*47\r\n");
        p.feed(b"$GPVTG,054.7,T,034.4,M,005.5,N,002.6,K*48\r\n");
        let gps = p.get().expect("valid vtg");
        assert_eq!(gps.fix_type, 3);
        assert_eq!(gps.heading, 54); // (int16_t)atof("054.7")
        assert!((gps.speed - 5.5 * 0.514444).abs() < 1e-5);
    }

    #[test]
    fn multi_sentence_stream() {
        let mut p = NmeaParser::new();
        p.feed(
            b"$GPGGA,123519,4807.038,N,01131.000,E,3,08,0.9,545.4,M,46.9,M,,*47\r\n\
              $GPRMC,123519,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*6A\r\n\
              $GPVTG,054.7,T,034.4,M,005.5,N,002.6,K*48\r\n",
        );
        let gps = p.get().expect("valid fix");
        assert_eq!(gps.fix_type, 3);
        assert_eq!(gps.satellites, 8);
        assert!((gps.speed - 5.5 * 0.514444).abs() < 1e-5); // VTG overwrote RMC
        assert_eq!(gps.heading, 54);
    }

    #[test]
    fn non_gps_sentences_ignored() {
        let mut p = NmeaParser::new();
        p.feed(
            b"$GPGGA,123519,4807.038,N,01131.000,E,3,08,0.9,545.4,M,46.9,M,,*47\r\n\
              $GPGSA,A,3,04,05,,09,12,,,24,,,,,2.5,1.3,2.1*39\r\n\
              $GPGSV,3,1,11,03,03,111,00,04,15,270,00,06,01,010,00,13,06,292,00*74\r\n",
        );
        let gps = p.get().expect("valid fix");
        assert_eq!(gps.satellites, 8); // GSA/GSV did not clobber it
    }

    #[test]
    fn no_dollar_ignored() {
        assert!(parse(b"hello world\r\n").is_none());
        assert!(parse(b"12345\r\n").is_none()); // not '$', no parse
        assert!(parse(b"$ABC\r\n").is_none()); // starts with '$' but <= 5 chars
    }

    #[test]
    fn feed_without_newline_does_not_parse() {
        // No '\n' yet: buffer holds the partial line, nothing parsed.
        let mut p = NmeaParser::new();
        p.feed(b"$GPGGA,123519,4807.038,N,01131.000,E,3,08,0.9,545.4,M,46.9,M,,*47");
        assert!(p.get().is_none());
        p.feed(b"\r\n");
        assert!(p.get().is_some());
    }

    #[test]
    fn long_garbage_line_no_panic() {
        let mut junk = [b'A'; 300];
        junk[299] = b'\n';
        let mut p = NmeaParser::new();
        p.feed(&junk);
        assert!(p.get().is_none());
        // Buffer is usable again afterwards.
        p.feed(b"$GPGGA,123519,4807.038,N,01131.000,E,3,08,0.9,545.4,M,46.9,M,,*47\r\n");
        assert!(p.get().is_some());
    }

    #[test]
    fn checksum_stripped_at_asterisk() {
        // Without the '*' handling, "*47" would shift/poison fields.
        let gps = parse(b"$GPGGA,123519,4807.038,N,01131.000,E,3,08,0.9,545.4,M*47\r\n")
            .expect("valid fix");
        assert_eq!(gps.fix_type, 3);
        assert_eq!(gps.satellites, 8);
    }

    #[test]
    fn gngga_gnrmc_gnvtg_accepted() {
        let gps = parse(
            b"$GNGGA,123519,4807.038,N,01131.000,E,3,08,0.9,545.4,M,46.9,M,,*4A\r\n\
              $GNRMC,123519,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*6B\r\n",
        )
        .expect("valid fix");
        assert_eq!(gps.fix_type, 3);
        assert_eq!(gps.satellites, 8);
    }

    #[test]
    fn empty_field_strtok_shift() {
        // `strtok` treats comma runs as one separator: the empty EW field is
        // skipped, so indices shift ("E" lands at fields[4], "3" at fields[5],
        // fix at fields[6]). The parser follows the C and accepts the line:
        // latitude parses, longitude direction is bogus so the coordinate is
        // 0.0 (nmea_to_decimal early-out on a 1-char input).
        let gps = parse(b"$GPGGA,123519,4807.038,N,,E,3,08,0.9,545.4,M*47\r\n").expect("parsed");
        assert_eq!(gps.fix_type, 3);
        assert!((gps.latitude - 48.1173).abs() < 1e-9);
        assert_eq!(gps.longitude, 0.0);
        assert_eq!(gps.satellites, 0); // atoi("0.9") -> 0
    }

    #[test]
    fn cstr_re_exports_still_work() {
        // Guard against accidental removal of the interface helpers we use.
        let s = rid_interface::fixed_str("4807.038");
        assert!(s.c_starts_with("4807"));
        assert!(s.c_contains("7.038"));
    }
}
