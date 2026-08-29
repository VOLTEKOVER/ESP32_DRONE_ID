//! OTA upload validation, port of the host-testable logic of `rid_ota.c`
//! (the `ota_update_handler` state machine). Hardware concerns (partition
//! writes, WiFi AP, rebooting) are left to the BSP; this module mirrors the C
//! decision order exactly:
//!
//! 1. lock level >= 2 rejects unconditionally;
//! 2. lock level >= 1 requires the `X-Signature` header;
//! 3. the body is streamed chunk by chunk: running SHA-256, buffering for the
//!    later signature check and a pluggable flash-write sink;
//! 4. the `X-Expected-SHA256` header is mandatory at every lock level;
//! 5. the computed hash is compared (mismatch -> `HashMismatch`);
//! 6. at lock level >= 1 the buffered body is signature-verified;
//! 7. the upload must be complete (`remaining == 0`).

use alloc::vec::Vec;
use rid_interface::{FixedKeyStr, NUM_KEYS};
use sha2::{Digest, Sha256};

use rid_core::security::{hex_to_bytes, verify_signed_body};

/// Abort the upload if the client stalls for this many consecutive socket
/// timeouts (each `httpd_req_recv` timeout is ~5 s, so ~60 s idle).
pub const OTA_MAX_IDLE_STALLS: usize = 12;
/// Body buffer size used when no content length is known (`req->content_len`
/// is 0), matching `OTA_BODY_CAP_DEFAULT` in `rid_ota.c`.
pub const OTA_BODY_CAP_DEFAULT: usize = 512 * 1024;

/// One receive outcome in the upload loop (mirrors `httpd_req_recv`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecvChunk<'a> {
    /// A chunk of the body.
    Data(&'a [u8]),
    /// Socket read timed out (idle); counts toward `OTA_MAX_IDLE_STALLS`.
    Timeout,
    /// Hard socket error; aborts the upload immediately.
    Fatal,
}

/// Failure modes of `validate_ota_upload`, in C decision order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaError {
    /// `lock_level >= 2`: the device is permanently locked.
    PermanentlyLocked,
    /// `lock_level >= 1` but the `X-Signature` header is missing.
    MissingSignature,
    /// The flash-write sink rejected a chunk.
    WriteFailed,
    /// The `X-Expected-SHA256` header is missing (mandatory at every level).
    MissingExpectedHash,
    /// The computed SHA-256 does not match the expected value.
    HashMismatch,
    /// The signature check at `lock_level >= 1` failed.
    InvalidSignature,
    /// The stream ended before `content_len` bytes were received.
    UploadIncomplete,
}

/// Buffer size for the signature body, as in C: the content length when known,
/// otherwise `OTA_BODY_CAP_DEFAULT`. The caller may clamp it further.
pub fn ota_body_cap(content_len: usize) -> usize {
    if content_len > 0 {
        content_len
    } else {
        OTA_BODY_CAP_DEFAULT
    }
}

/// Inputs for one OTA upload, assembled by the BSP from the HTTP request
/// (lock level, headers, content length and the configured public keys).
#[derive(Clone, Copy)]
pub struct OtaUpload<'a> {
    /// Clamped 0..=2 configuration value.
    pub lock_level: u8,
    /// Declared body length; the loop stops once this many bytes have been
    /// received (the caller must not hand out more).
    pub content_len: usize,
    /// The `X-Expected-SHA256` header, `None` if it is absent or longer than
    /// 64 characters (the C code treats those as missing).
    pub expected_sha256: Option<&'a [u8]>,
    /// The `X-Signature` header, `None` if absent or >= 512 bytes.
    pub signature: Option<&'a [u8]>,
    /// Configured keys checked by `verify_signed_body`.
    pub public_keys: &'a [FixedKeyStr; NUM_KEYS],
    /// Buffering limit for the signature check (see `ota_body_cap`).
    pub body_cap: usize,
}

/// Validates an OTA upload against the same rules as the C
/// `ota_update_handler`.
///
/// * `write` — per-chunk sink (e.g. `esp_ota_write`); returning `false` aborts
///   with `WriteFailed`.
/// * `chunks` — the body as receive outcomes.
///
/// Returns `Ok(())` when the hash and (when required) the signature are valid
/// and the upload is complete; the caller then commits the partition.
pub fn validate_ota_upload<'a, F>(
    req: OtaUpload<'a>,
    mut write: F,
    chunks: impl IntoIterator<Item = RecvChunk<'a>>,
) -> Result<(), OtaError>
where
    F: FnMut(&[u8]) -> bool,
{
    if req.lock_level >= 2 {
        return Err(OtaError::PermanentlyLocked);
    }
    if req.lock_level >= 1 && req.signature.is_none() {
        return Err(OtaError::MissingSignature);
    }

    let mut hasher = Sha256::new();
    let mut body_buf: Option<Vec<u8>> =
        (req.lock_level >= 1).then(|| Vec::with_capacity(req.body_cap));
    let mut remaining = req.content_len;
    let mut idle_stalls = 0usize;

    for chunk in chunks {
        if remaining == 0 {
            break;
        }
        match chunk {
            RecvChunk::Timeout => {
                idle_stalls += 1;
                if idle_stalls >= OTA_MAX_IDLE_STALLS {
                    break;
                }
            }
            RecvChunk::Fatal => break,
            RecvChunk::Data(buf) => {
                idle_stalls = 0;
                let take = buf.len().min(remaining);
                let buf = &buf[..take];
                hasher.update(buf);
                if let Some(v) = body_buf.as_mut() {
                    if v.len() + buf.len() <= req.body_cap {
                        v.extend_from_slice(buf);
                    }
                }
                if !write(buf) {
                    return Err(OtaError::WriteFailed);
                }
                remaining -= take;
            }
        }
    }

    if req.expected_sha256.is_none() {
        return Err(OtaError::MissingExpectedHash);
    }
    let digest = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&digest);
    let mut expected = [0u8; 32];
    if !hex_to_bytes(req.expected_sha256.unwrap(), &mut expected) || hash != expected {
        return Err(OtaError::HashMismatch);
    }

    if req.lock_level >= 1 {
        let body_bytes = body_buf.as_deref().unwrap_or(&[]);
        // The C code NUL-terminates the buffered body before the signature
        // check; mirror that truncation.
        let body_str = match body_bytes.iter().position(|&b| b == 0) {
            Some(i) => &body_bytes[..i],
            None => body_bytes,
        };
        if !verify_signed_body(body_str, req.signature.unwrap(), req.public_keys) {
            return Err(OtaError::InvalidSignature);
        }
    }

    if remaining > 0 {
        return Err(OtaError::UploadIncomplete);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use alloc::format;
    use alloc::string::String;
    use alloc::vec;
    use ed25519_dalek::{Signer, SigningKey};
    use rid_interface::key_str;

    const SEED: [u8; 32] = [7u8; 32];

    fn keys() -> &'static [FixedKeyStr; NUM_KEYS] {
        let sk = SigningKey::from_bytes(&SEED);
        let mut keys = [[0u8; rid_interface::MAX_KEY_LEN + 1]; NUM_KEYS];
        keys[0] = key_str(&format!(
            "PUBLIC_KEYV1:{}",
            b64_encode(sk.verifying_key().to_bytes().as_ref())
        ));
        Box::leak(Box::new(keys))
    }

    fn req<'a>(
        lock_level: u8,
        content_len: usize,
        expected_sha256: Option<&'a [u8]>,
        signature: Option<&'a [u8]>,
        body_cap: usize,
    ) -> OtaUpload<'a> {
        OtaUpload {
            lock_level,
            content_len,
            expected_sha256,
            signature,
            public_keys: keys(),
            body_cap,
        }
    }

    fn sign(body: &[u8]) -> String {
        let sk = SigningKey::from_bytes(&SEED);
        let hash = Sha256::digest(body);
        b64_encode(sk.sign(&hash).to_bytes().as_ref())
    }

    fn expected_hex(body: &[u8]) -> String {
        let hash = Sha256::digest(body);
        let mut out = String::new();
        for b in hash {
            out.push_str(&alloc::format!("{:02x}", b));
        }
        out
    }

    fn chunks<'a>(body: &'a [u8]) -> impl IntoIterator<Item = RecvChunk<'a>> + 'a {
        body.chunks(7).map(RecvChunk::Data)
    }

    #[test]
    fn permanently_locked_rejects_before_headers() {
        let err = validate_ota_upload(
            req(2, 0, Some(b"abc"), Some(b"x"), 16),
            |_| true,
            [RecvChunk::Data(b"data")],
        );
        assert_eq!(err, Err(OtaError::PermanentlyLocked));
    }

    #[test]
    fn lock_level_1_requires_signature_header() {
        let body = b"firmware";
        let err = validate_ota_upload(
            req(1, body.len(), Some(expected_hex(body).as_bytes()), None, 16),
            |_| true,
            chunks(body),
        );
        assert_eq!(err, Err(OtaError::MissingSignature));
    }

    #[test]
    fn happy_path_lock_0() {
        let body = b"firmware bytes";
        assert_eq!(
            validate_ota_upload(
                req(0, body.len(), Some(expected_hex(body).as_bytes()), None, 16),
                |_| true,
                chunks(body),
            ),
            Ok(())
        );
    }

    #[test]
    fn zero_length_body_allowed_at_lock_0() {
        assert_eq!(
            validate_ota_upload(
                req(0, 0, Some(expected_hex(b"").as_bytes()), None, 16),
                |_| true,
                [],
            ),
            Ok(())
        );
    }

    #[test]
    fn hash_mismatch_rejected() {
        let body = b"firmware";
        let err = validate_ota_upload(
            req(
                0,
                body.len(),
                Some(expected_hex(b"other").as_bytes()),
                None,
                16,
            ),
            |_| true,
            chunks(body),
        );
        assert_eq!(err, Err(OtaError::HashMismatch));
    }

    #[test]
    fn malformed_expected_header_is_a_mismatch() {
        let body = b"firmware";
        // Odd-length hex (cannot decode to 32 bytes).
        let err = validate_ota_upload(
            req(0, body.len(), Some(b"abc"), None, 16),
            |_| true,
            chunks(body),
        );
        assert_eq!(err, Err(OtaError::HashMismatch));
        // Non-hex character.
        let err = validate_ota_upload(
            req(
                0,
                body.len(),
                Some(b"zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"),
                None,
                16,
            ),
            |_| true,
            chunks(body),
        );
        assert_eq!(err, Err(OtaError::HashMismatch));
    }

    #[test]
    fn missing_expected_hash_rejected() {
        let body = b"firmware";
        let err = validate_ota_upload(req(0, body.len(), None, None, 16), |_| true, chunks(body));
        assert_eq!(err, Err(OtaError::MissingExpectedHash));
    }

    #[test]
    fn lock_level_1_valid_signature_passes() {
        let body = b"{\"locked\":1}";
        let sig = sign(body);
        assert_eq!(
            validate_ota_upload(
                req(
                    1,
                    body.len(),
                    Some(expected_hex(body).as_bytes()),
                    Some(sig.as_bytes()),
                    64
                ),
                |_| true,
                chunks(body),
            ),
            Ok(())
        );
    }

    #[test]
    fn lock_level_1_invalid_signature_rejected() {
        let body = b"firmware";
        let bad_sig = sign(b"other");
        let err = validate_ota_upload(
            req(
                1,
                body.len(),
                Some(expected_hex(body).as_bytes()),
                Some(bad_sig.as_bytes()),
                64,
            ),
            |_| true,
            chunks(body),
        );
        assert_eq!(err, Err(OtaError::InvalidSignature));
    }

    #[test]
    fn signature_verified_over_nul_truncated_body() {
        // The C code NUL-terminates the buffered body; the signature must be
        // over the truncated content.
        let body = b"abc\0def";
        let truncated = b"abc";
        let sig_truncated = sign(truncated);
        assert_eq!(
            validate_ota_upload(
                req(
                    1,
                    body.len(),
                    Some(expected_hex(body).as_bytes()),
                    Some(sig_truncated.as_bytes()),
                    64,
                ),
                |_| true,
                chunks(body),
            ),
            Ok(())
        );
        let sig_full = sign(body);
        let err = validate_ota_upload(
            req(
                1,
                body.len(),
                Some(expected_hex(body).as_bytes()),
                Some(sig_full.as_bytes()),
                64,
            ),
            |_| true,
            chunks(body),
        );
        assert_eq!(err, Err(OtaError::InvalidSignature));
    }

    #[test]
    fn signature_ignored_at_lock_level_0() {
        let body = b"firmware";
        assert_eq!(
            validate_ota_upload(
                req(
                    0,
                    body.len(),
                    Some(expected_hex(body).as_bytes()),
                    Some(sign(b"unused").as_bytes()),
                    16,
                ),
                |_| true,
                chunks(body),
            ),
            Ok(())
        );
    }

    #[test]
    fn incomplete_upload_rejected() {
        let body = b"firmware bytes";
        let fed = &body[..5];
        let err = validate_ota_upload(
            req(0, body.len(), Some(expected_hex(fed).as_bytes()), None, 16),
            |_| true,
            chunks(fed),
        );
        assert_eq!(err, Err(OtaError::UploadIncomplete));
    }

    #[test]
    fn too_many_idle_stalls_abort_as_incomplete() {
        // Hash check happens before the completeness gate, so the received
        // bytes must hash to the expected value for `UploadIncomplete` to be
        // reported (as in C, where a stalled loop reports the mismatch first).
        let fed = b"partial";
        let mut seq = vec![RecvChunk::Data(fed)];
        for _ in 0..OTA_MAX_IDLE_STALLS {
            seq.push(RecvChunk::Timeout);
        }
        let err = validate_ota_upload(
            req(
                0,
                fed.len() + 10,
                Some(expected_hex(fed).as_bytes()),
                None,
                16,
            ),
            |_| true,
            seq,
        );
        assert_eq!(err, Err(OtaError::UploadIncomplete));
    }

    #[test]
    fn fatal_socket_error_aborts_as_incomplete() {
        let fed = b"partial";
        let err = validate_ota_upload(
            req(
                0,
                fed.len() + 10,
                Some(expected_hex(fed).as_bytes()),
                None,
                16,
            ),
            |_| true,
            vec![RecvChunk::Data(fed), RecvChunk::Fatal],
        );
        assert_eq!(err, Err(OtaError::UploadIncomplete));
    }

    #[test]
    fn write_failure_aborts_immediately() {
        let body = b"firmware";
        let mut writes = 0;
        let err = validate_ota_upload(
            req(0, body.len(), Some(expected_hex(body).as_bytes()), None, 16),
            |_| {
                writes += 1;
                writes < 2
            },
            chunks(body),
        );
        assert_eq!(err, Err(OtaError::WriteFailed));
    }

    #[test]
    fn idle_stall_resets_on_data() {
        let body = b"firmware";
        let seq: Vec<RecvChunk<'_>> = vec![
            RecvChunk::Timeout,
            RecvChunk::Timeout,
            RecvChunk::Data(&body[..4]),
            // After data the stall counter resets; three more stalls are fine.
            RecvChunk::Timeout,
            RecvChunk::Timeout,
            RecvChunk::Timeout,
            RecvChunk::Data(&body[4..]),
        ];
        assert_eq!(
            validate_ota_upload(
                req(0, body.len(), Some(expected_hex(body).as_bytes()), None, 16),
                |_| true,
                seq,
            ),
            Ok(())
        );
    }

    #[test]
    fn body_buffering_caps_at_body_cap() {
        // lock >= 1: with body_cap smaller than the body, the buffered bytes
        // are truncated; a signature over the (invalid) truncated content fails.
        let body = b"0123456789";
        let err = validate_ota_upload(
            req(
                1,
                body.len(),
                Some(expected_hex(body).as_bytes()),
                Some(sign(body).as_bytes()),
                4,
            ),
            |_| true,
            chunks(body),
        );
        assert_eq!(err, Err(OtaError::InvalidSignature));
    }

    fn b64_encode(data: &[u8]) -> String {
        const TAB: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b0 = chunk[0];
            let b1 = chunk.get(1).copied().unwrap_or(0);
            let b2 = chunk.get(2).copied().unwrap_or(0);
            out.push(TAB[(b0 >> 2) as usize] as char);
            out.push(TAB[(((b0 & 0x3) << 4) | (b1 >> 4)) as usize] as char);
            out.push(if chunk.len() > 1 {
                TAB[(((b1 & 0xF) << 2) | (b2 >> 6)) as usize] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                TAB[(b2 & 0x3F) as usize] as char
            } else {
                '='
            });
        }
        out
    }
}
