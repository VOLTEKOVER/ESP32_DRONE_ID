//! Web/HTTP support logic, ported from `web_config.c`: the signature-failure
//! rate limiter and the log ring buffer with JSON rendering. Both are pure and
//! host-testable; the HTTP server, eFuse and vprintf plumbing land with the
//! hardware phase.

/// `SIG_RATE_MAX_FAILS` from the C.
pub const SIG_RATE_MAX_FAILS: usize = 10;
/// `SIG_RATE_WINDOW_MS` from the C.
pub const SIG_RATE_WINDOW_MS: u32 = 60_000;

/// Port of `sig_rate_check`/`sig_rate_record_fail`: a sliding window of
/// signature verification failures (used to rate-limit web config changes when
/// the device is locked). Time is `u32` ms; the elapsed-time comparison uses
/// the same unsigned wraparound arithmetic as the C.
#[derive(Clone, Copy)]
pub struct SigRate {
    fail_times: [u32; SIG_RATE_MAX_FAILS],
    count: usize,
}

impl SigRate {
    pub const fn new() -> Self {
        Self {
            fail_times: [0; SIG_RATE_MAX_FAILS],
            count: 0,
        }
    }

    /// Port of `sig_rate_check()`: drops expired failures from the window and
    /// returns `true` while fewer than `SIG_RATE_MAX_FAILS` are recent.
    pub fn check(&mut self, now_ms: u32) -> bool {
        let mut valid = 0usize;
        for i in 0..self.count {
            if now_ms.wrapping_sub(self.fail_times[i]) < SIG_RATE_WINDOW_MS {
                self.fail_times[valid] = self.fail_times[i];
                valid += 1;
            }
        }
        self.count = valid;
        self.count < SIG_RATE_MAX_FAILS
    }

    /// Port of `sig_rate_record_fail()`: records a failure when the window is
    /// not already full.
    pub fn record_fail(&mut self, now_ms: u32) {
        if self.count < SIG_RATE_MAX_FAILS {
            self.fail_times[self.count] = now_ms;
            self.count += 1;
        }
    }

    pub fn count(&self) -> usize {
        self.count
    }
}

impl Default for LogRing {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for SigRate {
    fn default() -> Self {
        Self::new()
    }
}

/// `LOG_RING_MAX` from the C.
pub const LOG_RING_MAX: usize = 64;
/// `LOG_MSG_MAX` from the C.
pub const LOG_MSG_MAX: usize = 240;
/// Ring entry count and the size of the `/api/logs` response buffer.
pub const LOG_BUF_SIZE: usize = 4096;

/// Port of `log_entry_t`.
#[derive(Clone, Copy)]
pub struct LogEntry {
    pub time_ms: u32,
    pub level: u8,
    pub msg: [u8; LOG_MSG_MAX],
}

/// Port of the `s_log_ring` ring buffer and `log_push()`.
pub struct LogRing {
    ring: [LogEntry; LOG_RING_MAX],
    head: usize,
    count: usize,
}

impl LogRing {
    pub const fn new() -> Self {
        Self {
            ring: [LogEntry {
                time_ms: 0,
                level: 0,
                msg: [0; LOG_MSG_MAX],
            }; LOG_RING_MAX],
            head: 0,
            count: 0,
        }
    }

    /// Port of `log_push()`: stores the entry at `(head + count) % MAX` and
    /// advances head once the ring is full (dropping the oldest).
    pub fn push(&mut self, level: u8, msg: &[u8], now_ms: u32) {
        let i = (self.head + self.count) % LOG_RING_MAX;
        let e = &mut self.ring[i];
        e.time_ms = now_ms;
        e.level = level;
        copy_trunc(msg, &mut e.msg);
        if self.count < LOG_RING_MAX {
            self.count += 1;
        } else {
            self.head = (self.head + 1) % LOG_RING_MAX;
        }
    }

    pub fn count(&self) -> usize {
        self.count
    }

    /// Port of the `/api/logs` JSON builder (`handle_get_logs` body): renders
    /// `[{"t":<ms>,"l":"<level>","m":"<msg>"},...]` into `buf` and returns the
    /// length. The oldest entries are dropped once the ring is full.
    pub fn render_log_json(&self, buf: &mut [u8]) -> usize {
        let mut off = 0usize;
        push_bytes(buf, &mut off, b"[");

        let n = self.count;
        let start = if n < LOG_RING_MAX { 0 } else { self.head };
        for i in 0..n {
            let idx = (start + i) % LOG_RING_MAX;
            let e = &self.ring[idx];

            let mut escaped = [0u8; LOG_MSG_MAX * 2];
            json_escape(&e.msg, &mut escaped);

            if i > 0 {
                push_bytes(buf, &mut off, b",");
            }
            push_bytes(buf, &mut off, b"{\"t\":");
            push_u32(buf, &mut off, e.time_ms);
            push_bytes(buf, &mut off, b",\"l\":\"");
            push_bytes(buf, &mut off, &[e.level]);
            push_bytes(buf, &mut off, b"\",\"m\":\"");
            push_cstr(buf, &mut off, &escaped);
            push_bytes(buf, &mut off, b"\"}");

            if off >= buf.len().saturating_sub(128) {
                off = buf.len().saturating_sub(128);
                break;
            }
        }
        push_bytes(buf, &mut off, b"]");
        off
    }
}

/// Port of the escaping loop in `handle_get_logs`: escapes `"` `\` `\n` `\r`
/// `\t`, drops other control characters, stops at NUL, and terminates with a
/// NUL.
pub fn json_escape(src: &[u8], dst: &mut [u8]) {
    let mut eo = 0usize;
    for &c in src {
        if c == 0 {
            break;
        }
        if eo >= dst.len().saturating_sub(4) {
            break;
        }
        match c {
            b'"' | b'\\' => {
                dst[eo] = b'\\';
                dst[eo + 1] = c;
                eo += 2;
            }
            b'\n' => {
                dst[eo] = b'\\';
                dst[eo + 1] = b'n';
                eo += 2;
            }
            b'\r' => {
                dst[eo] = b'\\';
                dst[eo + 1] = b'r';
                eo += 2;
            }
            b'\t' => {
                dst[eo] = b'\\';
                dst[eo + 1] = b't';
                eo += 2;
            }
            0x00..=0x1F => continue,
            _ => {
                dst[eo] = c;
                eo += 1;
            }
        }
    }
    dst[eo] = 0;
}

/// Port of the level detection in `log_vprintf`: `E`/`W`/`I`/`D`/`V` at the
/// start of the line, otherwise `I`.
pub fn level_from_line(line: &[u8]) -> u8 {
    match line.first() {
        Some(b'E') | Some(b'W') | Some(b'I') | Some(b'D') | Some(b'V') => line[0],
        _ => b'I',
    }
}

/// `strncpy(msg, LOG_MSG_MAX - 1)` + trailing NUL, stopping at an embedded NUL.
fn copy_trunc(src: &[u8], dst: &mut [u8; LOG_MSG_MAX]) {
    let end = src.iter().position(|&b| b == 0).unwrap_or(src.len());
    let n = end.min(LOG_MSG_MAX - 1);
    dst[..n].copy_from_slice(&src[..n]);
    dst[n..].fill(0);
}

fn push_bytes(buf: &mut [u8], off: &mut usize, s: &[u8]) {
    let n = s.len().min(buf.len().saturating_sub(*off));
    buf[*off..*off + n].copy_from_slice(&s[..n]);
    *off += n;
}

fn push_cstr(buf: &mut [u8], off: &mut usize, s: &[u8]) {
    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    push_bytes(buf, off, &s[..end]);
}

fn push_u32(buf: &mut [u8], off: &mut usize, mut v: u32) {
    let mut tmp = [0u8; 10];
    let mut n = 0usize;
    if v == 0 {
        tmp[n] = b'0';
        n += 1;
    }
    while v > 0 {
        tmp[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
    }
    while n > 0 {
        n -= 1;
        push_bytes(buf, off, &tmp[n..n + 1]);
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::string::String;

    #[test]
    fn sig_rate_allows_under_ten_failures() {
        let mut sr = SigRate::new();
        assert!(sr.check(1000));
        for i in 0..9 {
            sr.record_fail(1000 + i);
        }
        assert!(sr.check(2000));
        assert_eq!(sr.count(), 9);
    }

    #[test]
    fn sig_rate_blocks_at_ten_failures() {
        let mut sr = SigRate::new();
        for i in 0..10 {
            sr.record_fail(1000 + i);
        }
        assert!(!sr.check(2000));
        // record_fail is a no-op while full.
        sr.record_fail(3000);
        assert_eq!(sr.count(), 10);
    }

    #[test]
    fn sig_rate_window_expiry_frees_slots() {
        let mut sr = SigRate::new();
        for i in 0..10 {
            sr.record_fail(1000 + i);
        }
        assert!(!sr.check(2000));
        // All failures fall out of the 60s window.
        assert!(sr.check(2000 + SIG_RATE_WINDOW_MS));
        assert_eq!(sr.count(), 0);
    }

    #[test]
    fn sig_rate_partial_expiry() {
        let mut sr = SigRate::new();
        // 4 failures at t=1000, 6 at t=2000.
        for _ in 0..4 {
            sr.record_fail(1000);
        }
        for _ in 0..6 {
            sr.record_fail(2000);
        }
        assert!(!sr.check(2000));
        // At t=61000 the first four expired (61000 - 1000 == window, not < window).
        assert!(sr.check(61000));
        assert_eq!(sr.count(), 6);
    }

    #[test]
    fn sig_rate_wraparound_arithmetic() {
        // now wraps below the stored timestamp; elapsed < window -> still
        // counted (matches the C unsigned subtraction).
        let mut sr = SigRate::new();
        sr.record_fail(0xFFFF_FFF0);
        assert!(sr.check(0x0000_0010)); // elapsed 0x20 ms
        assert_eq!(sr.count(), 1);
    }

    #[test]
    fn log_push_truncates_to_msg_max() {
        let mut ring = LogRing::new();
        ring.push(b'I', &[b'x'; 300], 1000);
        assert_eq!(ring.count(), 1);
        let e = &ring.ring[0];
        assert_eq!(&e.msg[..239], &[b'x'; 239]);
        assert_eq!(e.msg[239], 0);
        assert_eq!(e.time_ms, 1000);
        assert_eq!(e.level, b'I');
    }

    #[test]
    fn log_ring_keeps_newest_when_full() {
        let mut ring = LogRing::new();
        for i in 0..(LOG_RING_MAX + 10) as u32 {
            let mut m = [0u8; 8];
            m[..3].copy_from_slice(b"msg");
            m[3] = b'0' + (i / 100) as u8;
            m[4] = b'0' + (i / 10 % 10) as u8;
            m[5] = b'0' + (i % 10) as u8;
            ring.push(b'I', &m[..6], i);
        }
        assert_eq!(ring.count(), LOG_RING_MAX);
        // The newest entry (i = LOG_RING_MAX + 9 = 73) is present.
        let mut buf = [0u8; LOG_BUF_SIZE];
        let n = ring.render_log_json(&mut buf);
        let out = String::from_utf8_lossy(&buf[..n]);
        assert!(out.contains("msg073"));
        // The oldest entry (i = 0) was dropped.
        assert!(!out.contains("msg000"));
    }

    #[test]
    fn json_escape_handles_specials() {
        let src = b"a\"b\\c\nd\re\tf\x01g";
        let mut dst = [0u8; 64];
        json_escape(src, &mut dst);
        // a " b \ c \n d \r e \t f (0x01 dropped) g NUL
        assert_eq!(dst[..18], *b"a\\\"b\\\\c\\nd\\re\\tfg\0");
    }

    #[test]
    fn json_escape_stops_at_nul() {
        let src = b"abc\x00def";
        let mut dst = [0u8; 16];
        json_escape(src, &mut dst);
        assert_eq!(dst[..6], *b"abc\0\0\0");
    }

    #[test]
    fn level_detection_matches_c() {
        assert_eq!(level_from_line(b"E..."), b'E');
        assert_eq!(level_from_line(b"W..."), b'W');
        assert_eq!(level_from_line(b"I..."), b'I');
        assert_eq!(level_from_line(b"D..."), b'D');
        assert_eq!(level_from_line(b"V..."), b'V');
        assert_eq!(level_from_line(b"garbage"), b'I');
        assert_eq!(level_from_line(b""), b'I');
    }

    #[test]
    fn render_log_json_shape() {
        let mut ring = LogRing::new();
        ring.push(b'E', b"boom", 42);
        ring.push(b'I', b"ok", 43);
        let mut buf = [0u8; LOG_BUF_SIZE];
        let n = ring.render_log_json(&mut buf);
        let out = String::from_utf8_lossy(&buf[..n]);
        assert_eq!(
            out,
            "[{\"t\":42,\"l\":\"E\",\"m\":\"boom\"},{\"t\":43,\"l\":\"I\",\"m\":\"ok\"}]"
        );
    }
}
