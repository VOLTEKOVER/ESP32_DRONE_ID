//! Security helpers, port of `rid_security.c`: base64 decode, hex
//! encode/decode, SHA-256 checksum and Ed25519 signed-body verification
//! against the configured public keys (mbedtls/PSA -> dalek/sha2).

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use pkcs8::DecodePublicKey;
use rid_interface::{CStr, FixedKeyStr, NUM_KEYS};
use sha2::{Digest, Sha256};

const B64_TAB: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Port of the static `b64_decode()` in `rid_security.c`. Returns `None` for
/// the same inputs the C code rejects (length 0 or not % 4, more than two
/// `=` pads, pad with no payload, invalid character).
pub fn b64_decode(input: &[u8]) -> Option<Vec<u8>> {
    let in_len = input.len();
    if in_len == 0 || !in_len.is_multiple_of(4) {
        return None;
    }
    let mut pad = 0usize;
    while pad < in_len && input[in_len - 1 - pad] == b'=' {
        pad += 1;
    }
    if pad > 2 {
        return None;
    }
    let valid_len = in_len - pad;
    if pad > 0 && valid_len.is_multiple_of(4) {
        return None;
    }

    let mut out = Vec::with_capacity((valid_len * 3) / 4);
    let mut buf = [0u8; 4];
    let mut buf_i = 0usize;
    for &c in &input[..valid_len] {
        let p = B64_TAB.iter().position(|&t| t == c)?;
        buf[buf_i] = p as u8;
        buf_i += 1;
        if buf_i == 4 {
            out.push((buf[0] << 2) | (buf[1] >> 4));
            out.push((buf[1] << 4) | (buf[2] >> 2));
            out.push((buf[2] << 6) | buf[3]);
            buf_i = 0;
        }
    }
    if buf_i >= 2 {
        out.push((buf[0] << 2) | (buf[1] >> 4));
    }
    if buf_i >= 3 {
        out.push((buf[1] << 4) | (buf[2] >> 2));
    }
    Some(out)
}

/// Port of `rid_security_bytes_to_hex()`. Writes two lowercase hex digits per
/// byte into `out`; returns `false` when `out` is too small (needs
/// `2 * len`).
pub fn bytes_to_hex(bytes: &[u8], out: &mut [u8]) -> bool {
    if out.len() < bytes.len() * 2 {
        return false;
    }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for (i, &b) in bytes.iter().enumerate() {
        out[i * 2] = HEX[(b >> 4) as usize];
        out[i * 2 + 1] = HEX[(b & 0xF) as usize];
    }
    true
}

/// Convenience wrapper returning an owned hex string.
pub fn to_hex(bytes: &[u8]) -> String {
    let mut out = vec![0u8; bytes.len() * 2];
    let _ = bytes_to_hex(bytes, &mut out);
    // SAFETY-free: the bytes are guaranteed ASCII hex digits.
    String::from_utf8(out).expect("hex is ASCII")
}

/// Port of `rid_security_hex_to_bytes()`. Returns `false` on length mismatch
/// or non-hex characters (same as C).
pub fn hex_to_bytes(hex: &[u8], out: &mut [u8]) -> bool {
    if hex.len() != out.len() * 2 {
        return false;
    }
    for (i, chunk) in hex.chunks_exact(2).enumerate() {
        let hi = chunk[0];
        let lo = chunk[1];
        let hi_v = match hi {
            b'0'..=b'9' => hi - b'0',
            b'a'..=b'f' => hi - b'a' + 10,
            b'A'..=b'F' => hi - b'A' + 10,
            _ => return false,
        };
        let lo_v = match lo {
            b'0'..=b'9' => lo - b'0',
            b'a'..=b'f' => lo - b'a' + 10,
            b'A'..=b'F' => lo - b'A' + 10,
            _ => return false,
        };
        out[i] = (hi_v << 4) | lo_v;
    }
    true
}

/// SHA-256 over `data` (port of the `psa_hash_compute(PSA_ALG_SHA_256)` calls).
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// Port of `rid_security_verify_sha256()`.
pub fn verify_sha256(data: &[u8], expected_hex: &[u8]) -> bool {
    if expected_hex.is_empty() || expected_hex[0] == 0 {
        return false;
    }
    let mut expected = [0u8; 32];
    if !hex_to_bytes(expected_hex, &mut expected) {
        return false;
    }
    sha256(data) == expected
}

/// Parses a configured public key. Mirrors mbedtls: the buffer is first tried
/// as an encoded public key (PEM text or raw DER); if that fails and the
/// string starts with `PUBLIC_KEYV1:` (case-insensitive, as `strncasecmp`),
/// the base64 payload is decoded and used as a raw key.
fn parse_public_key(key: &FixedKeyStr) -> Option<VerifyingKey> {
    let bytes = &key[..key.c_len()];

    if let Ok(pem) = alloc::str::from_utf8(bytes) {
        if let Ok(vk) = VerifyingKey::from_public_key_pem(pem) {
            return Some(vk);
        }
    }
    if let Ok(vk) = VerifyingKey::from_public_key_der(bytes) {
        return Some(vk);
    }

    const PREFIX: &[u8] = b"PUBLIC_KEYV1:";
    if bytes.len() > PREFIX.len() && bytes[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
        let payload = &bytes[PREFIX.len()..];
        if let Some(key_bin) = b64_decode(payload) {
            let arr: [u8; 32] = key_bin.try_into().ok()?;
            if let Ok(vk) = VerifyingKey::from_bytes(&arr) {
                return Some(vk);
            }
        }
    }
    None
}

/// Port of `rid_security_verify_signed_body()`.
///
/// The body is hashed with SHA-256 and the signature (base64) is verified
/// with Ed25519 against every configured public key. Like mbedtls's
/// `pk_verify` for Ed25519, the digest is the message being signed.
pub fn verify_signed_body(
    body: &[u8],
    sig_b64: &[u8],
    public_keys: &[FixedKeyStr; NUM_KEYS],
) -> bool {
    if body.is_empty() || sig_b64.is_empty() || sig_b64[0] == 0 {
        return false;
    }

    let sig = match b64_decode(sig_b64) {
        Some(s) if !s.is_empty() => s,
        _ => return false,
    };
    let Ok(sig) = <[u8; 64]>::try_from(sig.as_slice()) else {
        return false;
    };

    let hash = sha256(body);

    for key in public_keys {
        if key.c_is_empty() {
            continue;
        }
        let Some(vk) = parse_public_key(key) else {
            continue;
        };
        if vk.verify(&hash, &Signature::from_bytes(&sig)).is_ok() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use ed25519_dalek::Signer;
    use pem_rfc7468::LineEnding;
    use pkcs8::EncodePublicKey;

    #[test]
    fn b64_decode_roundtrip() {
        let data = b"hello world";
        let enc = b"aGVsbG8gd29ybGQ=";
        assert_eq!(b64_decode(enc).unwrap(), data);

        // Unpadded input with length % 4 != 0 is rejected.
        assert!(b64_decode(b"aGVsbG8gd29ybGQ").is_none());
        // Odd length rejected.
        assert!(b64_decode(b"abc").is_none());
        // Invalid char rejected.
        assert!(b64_decode(b"aG*b*zdA==").is_none());
        // Empty rejected.
        assert!(b64_decode(b"").is_none());

        // More than two pads rejected.
        assert!(b64_decode(b"A===").is_none());
        // Only pads rejected.
        assert!(b64_decode(b"==").is_none());
        // Pad inside the data rejected.
        assert!(b64_decode(b"A=A=").is_none());
        // Standard vector (padded).
        assert_eq!(b64_decode(b"aGVsbG8=").unwrap(), b"hello");
        assert_eq!(b64_decode(b"AA==").unwrap(), [0]);
        assert_eq!(b64_decode(b"AAA=").unwrap(), [0, 0]);
        assert_eq!(b64_decode(b"AAAA").unwrap(), [0, 0, 0]);
    }

    #[test]
    fn hex_roundtrip() {
        let data = [0xABu8, 0x12, 0xFF, 0x00];
        let hex = to_hex(&data);
        assert_eq!(hex, "ab12ff00");

        let mut out = [0u8; 4];
        assert!(hex_to_bytes(hex.as_bytes(), &mut out));
        assert_eq!(out, data);

        assert!(!hex_to_bytes(b"ab12ff0", &mut out)); // odd length
        assert!(!hex_to_bytes(b"gg12ff00", &mut out)); // bad char
        assert!(!hex_to_bytes(b"AB12FF00", &mut [0u8; 3])); // length mismatch
        assert!(!hex_to_bytes(b"0x", &mut [0u8; 1])); // non-hex digit
                                                      // Uppercase input decodes.
        let mut up = [0u8; 4];
        assert!(hex_to_bytes(b"AB12FF00", &mut up));
        assert_eq!(up, [0xAB, 0x12, 0xFF, 0x00]);
    }

    #[test]
    fn verify_sha256_known_vector() {
        // SHA-256 of "abc" is well known.
        assert!(verify_sha256(
            b"abc",
            b"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        ));
        assert!(!verify_sha256(
            b"abd",
            b"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        ));
        assert!(!verify_sha256(b"abc", b""));
    }

    #[test]
    fn signed_body_verification_with_pem_and_raw_prefix() {
        let seed = [9u8; 32];
        let sk = ed25519_dalek::SigningKey::from_bytes(&seed);
        let vk = sk.verifying_key();

        let body = b"{\"locked\":1}";
        // Like mbedtls pk_verify for Ed25519: the SHA-256 digest is the signed message.
        let hash = sha256(body);
        let sig = sk.sign(&hash);
        let sig_b64 = b64_encode(&sig.to_bytes());

        // Key as SPKI PEM.
        let pem = vk.to_public_key_pem(LineEnding::LF).unwrap();
        let mut keys_pem = [[0u8; rid_interface::MAX_KEY_LEN + 1]; rid_interface::NUM_KEYS];
        keys_pem[0] = rid_interface::key_str(&pem);

        let pk = parse_public_key(&keys_pem[0]).expect("PEM key parses");
        assert!(pk.verify(&hash, &sig).is_ok(), "direct verify failed");
        assert!(verify_signed_body(body, sig_b64.as_bytes(), &keys_pem));

        // Key as PUBLIC_KEYV1:<base64 32 bytes>.
        let mut keys_v1 = [[0u8; rid_interface::MAX_KEY_LEN + 1]; rid_interface::NUM_KEYS];
        keys_v1[0] = rid_interface::key_str(&format!(
            "PUBLIC_KEYV1:{}",
            b64_encode(vk.to_bytes().as_ref())
        ));
        assert!(verify_signed_body(body, sig_b64.as_bytes(), &keys_v1));

        // Tampered body must fail.
        assert!(!verify_signed_body(
            b"{\"locked\":0}",
            sig_b64.as_bytes(),
            &keys_pem
        ));
        // Empty signature must fail.
        assert!(!verify_signed_body(body, b"", &keys_pem));
    }

    fn b64_encode(data: &[u8]) -> String {
        base64ish::encode(data)
    }
}

#[cfg(test)]
mod base64ish {
    use alloc::string::String;

    /// Minimal standard base64 encoder for tests only.
    const TAB: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    pub fn encode(data: &[u8]) -> String {
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
