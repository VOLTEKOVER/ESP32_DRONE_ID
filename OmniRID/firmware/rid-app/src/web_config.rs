//! HTTP endpoint decision logic, port of the host-testable parts of
//! `web_config.c`: the signed-action gate shared by `handle_post_config` and
//! `handle_factory_reset` (lock level + signature rate limiter) and the
//! `/api/command` parsing/dispatch. HTTP plumbing and state/JSON rendering
//! live in the BSP and the `web`/`json`/`state` modules.
//!
//! The C `get_lock_level()` (eFuse magic -> 2, else `cfg.lock_level`) is
//! evaluated by the caller and passed in as `lock_level`.

use rid_core::security::verify_signed_body;
use rid_interface::{FixedKeyStr, NUM_KEYS};

use crate::web::SigRate;

/// Outcome of a signed config write or factory reset, in C decision order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigWrite {
    /// Accepted (and, for config, to be applied and saved).
    Ok,
    /// Too many signature failures in the window.
    RateLimited,
    /// Signature missing or invalid.
    InvalidSignature,
}

/// Decision shared by `handle_post_config` and `handle_factory_reset`:
/// at `lock_level >= 1` a valid `X-Signature` over `body` is required, gated
/// by the `SigRate` window; below that the action always passes.
///
/// Mirrors the C order: rate check first, then signature verification (a
/// failure records the timestamp for the rate limiter).
pub fn signed_action_decision(
    lock_level: u8,
    body: &[u8],
    signature: Option<&[u8]>,
    keys: &[FixedKeyStr; NUM_KEYS],
    rate: &mut SigRate,
    now_ms: u32,
) -> ConfigWrite {
    if lock_level >= 1 {
        if !rate.check(now_ms) {
            return ConfigWrite::RateLimited;
        }
        // The C body is a NUL-terminated string; a signature over a shorter
        // (NUL-truncated) body must still verify.
        let body = match body.iter().position(|&b| b == 0) {
            Some(i) => &body[..i],
            None => body,
        };
        if !verify_signed_body(body, signature.unwrap_or(&[]), keys) {
            rate.record_fail(now_ms);
            return ConfigWrite::InvalidSignature;
        }
    }
    ConfigWrite::Ok
}

/// A normalized `/api/command` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    /// `restart` or `reboot`.
    Restart,
    /// `reset` or `factory`.
    FactoryReset,
    /// `status` (answers with the state JSON).
    Status,
    /// Anything else; logged and answered with "unknown command".
    Unknown,
}

/// Outcome of `handle_command` auth gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandOutcome {
    /// Auth passed (or not required); the BSP dispatches on the kind.
    Ok(CommandKind),
    /// The action needed a signature and the check failed.
    InvalidSignature,
}

/// Normalizes a command body like the C `handle_post_command`: strips leading
/// spaces/tabs, unwraps a surrounding `"..."` pair and cuts at the first NUL.
pub fn normalize_command(raw: &[u8]) -> &[u8] {
    let mut b = raw;
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    b = &b[..end];
    while matches!(b.first(), Some(&b' ' | &b'\t')) {
        b = &b[1..];
    }
    if b.first() == Some(&b'"') {
        b = &b[1..];
        if let Some(i) = b.iter().position(|&c| c == b'"') {
            b = &b[..i];
        }
    }
    b
}

/// Classifies a normalized command. Mirrors the C `strcmp` chain.
pub fn command_kind(cmd: &[u8]) -> CommandKind {
    match cmd {
        b"restart" | b"reboot" => CommandKind::Restart,
        b"reset" | b"factory" => CommandKind::FactoryReset,
        b"status" => CommandKind::Status,
        _ => CommandKind::Unknown,
    }
}

/// Whether the command requires a signature when the device is locked
/// (`strcmp` check in `handle_post_command`).
pub fn command_needs_auth(kind: CommandKind) -> bool {
    matches!(kind, CommandKind::Restart | CommandKind::FactoryReset)
}

/// Auth gate of `handle_post_command`: at `lock_level >= 1` restart/reset
/// commands need a valid `X-Signature` over the normalized command bytes.
/// Note: unlike config writes, the command path has no rate limiter in C.
pub fn handle_command(
    lock_level: u8,
    raw: &[u8],
    signature: Option<&[u8]>,
    keys: &[FixedKeyStr; NUM_KEYS],
) -> CommandOutcome {
    let cmd = normalize_command(raw);
    let kind = command_kind(cmd);
    if lock_level >= 1
        && command_needs_auth(kind)
        && !verify_signed_body(cmd, signature.unwrap_or(&[]), keys)
    {
        return CommandOutcome::InvalidSignature;
    }
    CommandOutcome::Ok(kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use alloc::format;
    use alloc::string::String;
    use ed25519_dalek::{Signer, SigningKey};
    use rid_interface::key_str;
    use sha2::{Digest, Sha256};

    const SEED: [u8; 32] = [11u8; 32];

    fn keys() -> &'static [FixedKeyStr; NUM_KEYS] {
        let sk = SigningKey::from_bytes(&SEED);
        let mut keys = [[0u8; rid_interface::MAX_KEY_LEN + 1]; NUM_KEYS];
        keys[0] = key_str(&format!(
            "PUBLIC_KEYV1:{}",
            b64_encode(sk.verifying_key().to_bytes().as_ref())
        ));
        Box::leak(Box::new(keys))
    }

    fn sign(body: &[u8]) -> String {
        let sk = SigningKey::from_bytes(&SEED);
        let hash = Sha256::digest(body);
        b64_encode(sk.sign(&hash).to_bytes().as_ref())
    }

    // --- signed_action_decision -------------------------------------------

    #[test]
    fn unlocked_config_write_always_passes() {
        let mut rate = SigRate::new();
        assert_eq!(
            signed_action_decision(0, b"{\"locked\":1}", None, keys(), &mut rate, 1000),
            ConfigWrite::Ok
        );
    }

    #[test]
    fn locked_config_write_with_valid_signature() {
        let mut rate = SigRate::new();
        let body = b"{\"locked\":1}";
        let sig = sign(body);
        assert_eq!(
            signed_action_decision(1, body, Some(sig.as_bytes()), keys(), &mut rate, 1000),
            ConfigWrite::Ok
        );
    }

    #[test]
    fn locked_config_write_missing_signature() {
        let mut rate = SigRate::new();
        let body = b"{\"locked\":1}";
        assert_eq!(
            signed_action_decision(1, body, None, keys(), &mut rate, 1000),
            ConfigWrite::InvalidSignature
        );
        assert_eq!(rate.count(), 1, "failure recorded for the rate limiter");
    }

    #[test]
    fn locked_config_write_bad_signature() {
        let mut rate = SigRate::new();
        let body = b"{\"locked\":1}";
        let bad = sign(b"other");
        assert_eq!(
            signed_action_decision(1, body, Some(bad.as_bytes()), keys(), &mut rate, 1000),
            ConfigWrite::InvalidSignature
        );
        assert_eq!(rate.count(), 1);
    }

    #[test]
    fn rate_limited_before_signature_check() {
        let mut rate = SigRate::new();
        for i in 0..10 {
            rate.record_fail(1000 + i);
        }
        // Even a valid signature is rejected while rate-limited.
        let body = b"{\"locked\":1}";
        let sig = sign(body);
        assert_eq!(
            signed_action_decision(1, body, Some(sig.as_bytes()), keys(), &mut rate, 5000),
            ConfigWrite::RateLimited
        );
    }

    #[test]
    fn rate_limiter_recovers_after_window() {
        let mut rate = SigRate::new();
        for i in 0..10 {
            rate.record_fail(1000 + i);
        }
        let body = b"{}";
        let sig = sign(body);
        let after = 1000 + crate::web::SIG_RATE_WINDOW_MS;
        // A valid signature is accepted once the window has elapsed.
        assert_eq!(
            signed_action_decision(1, body, Some(sig.as_bytes()), keys(), &mut rate, after),
            ConfigWrite::Ok
        );
    }

    #[test]
    fn factory_reset_signs_literal_command() {
        let mut rate = SigRate::new();
        let sig = sign(b"factory_reset");
        assert_eq!(
            signed_action_decision(
                1,
                b"factory_reset",
                Some(sig.as_bytes()),
                keys(),
                &mut rate,
                1000
            ),
            ConfigWrite::Ok
        );
        // A signature over anything else is rejected.
        let other = sign(b"reset");
        assert_eq!(
            signed_action_decision(
                1,
                b"factory_reset",
                Some(other.as_bytes()),
                keys(),
                &mut rate,
                1000
            ),
            ConfigWrite::InvalidSignature
        );
    }

    #[test]
    fn signature_checked_over_nul_truncated_body() {
        let mut rate = SigRate::new();
        let body = b"{\"locked\":1}\0junk";
        let sig = sign(b"{\"locked\":1}");
        assert_eq!(
            signed_action_decision(1, body, Some(sig.as_bytes()), keys(), &mut rate, 1000),
            ConfigWrite::Ok
        );
    }

    // --- command parsing ---------------------------------------------------

    #[test]
    fn command_kind_classification() {
        assert_eq!(command_kind(b"restart"), CommandKind::Restart);
        assert_eq!(command_kind(b"reboot"), CommandKind::Restart);
        assert_eq!(command_kind(b"reset"), CommandKind::FactoryReset);
        assert_eq!(command_kind(b"factory"), CommandKind::FactoryReset);
        assert_eq!(command_kind(b"status"), CommandKind::Status);
        assert_eq!(command_kind(b"status\n"), CommandKind::Unknown);
        assert_eq!(command_kind(b"garbage"), CommandKind::Unknown);
        assert_eq!(command_kind(b""), CommandKind::Unknown);
    }

    #[test]
    fn normalize_strips_quotes_and_leading_whitespace() {
        assert_eq!(normalize_command(b"restart"), b"restart");
        assert_eq!(normalize_command(b"  restart"), b"restart");
        assert_eq!(normalize_command(b"\trestart"), b"restart");
        assert_eq!(normalize_command(b"\"restart\""), b"restart");
        assert_eq!(normalize_command(b"\"reset\" extra"), b"reset");
        assert_eq!(normalize_command(b"\"\""), b"");
        assert_eq!(normalize_command(b"restart\0junk"), b"restart");
        // No trailing whitespace trimming (as in C).
        assert_eq!(normalize_command(b"restart\n"), b"restart\n");
    }

    #[test]
    fn needs_auth_only_for_restart_and_factory() {
        assert!(command_needs_auth(CommandKind::Restart));
        assert!(command_needs_auth(CommandKind::FactoryReset));
        assert!(!command_needs_auth(CommandKind::Status));
        assert!(!command_needs_auth(CommandKind::Unknown));
    }

    #[test]
    fn unlocked_commands_need_no_signature() {
        assert_eq!(
            handle_command(0, b"restart", None, keys()),
            CommandOutcome::Ok(CommandKind::Restart)
        );
        assert_eq!(
            handle_command(0, b"\"status\"", None, keys()),
            CommandOutcome::Ok(CommandKind::Status)
        );
    }

    #[test]
    fn locked_restart_requires_signature() {
        let sig = sign(b"restart");
        assert_eq!(
            handle_command(1, b"restart", Some(sig.as_bytes()), keys()),
            CommandOutcome::Ok(CommandKind::Restart)
        );
        assert_eq!(
            handle_command(1, b"restart", None, keys()),
            CommandOutcome::InvalidSignature
        );
        let bad = sign(b"reset");
        assert_eq!(
            handle_command(1, b"restart", Some(bad.as_bytes()), keys()),
            CommandOutcome::InvalidSignature
        );
    }

    #[test]
    fn locked_status_and_unknown_need_no_signature() {
        assert_eq!(
            handle_command(1, b"status", None, keys()),
            CommandOutcome::Ok(CommandKind::Status)
        );
        assert_eq!(
            handle_command(1, b"frobnicate", None, keys()),
            CommandOutcome::Ok(CommandKind::Unknown)
        );
    }

    #[test]
    fn quoted_command_signed_over_normalized_bytes() {
        // The client signs the normalized command ("restart"), and the quoted
        // body must verify.
        let sig = sign(b"restart");
        assert_eq!(
            handle_command(1, b"\"restart\"", Some(sig.as_bytes()), keys()),
            CommandOutcome::Ok(CommandKind::Restart)
        );
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
