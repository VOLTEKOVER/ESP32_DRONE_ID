//! Ed25519 authentication signer, port of `rid_auth.c` (mbedtls -> dalek).
//! The UAS ID is signed with the configured Ed25519 key and the signature is
//! split into ODID Authentication data pages (AuthType =
//! `ODID_AUTH_UAS_ID_SIGNATURE`).

use ed25519_dalek::{Signer, SigningKey};
use pkcs8::DecodePrivateKey;
use rid_interface::odid::{
    AuthPack, AuthPage, AUTH_MAX_PAGES, AUTH_PAGE_NONZERO_DATA_SIZE, AUTH_PAGE_ZERO_DATA_SIZE,
    AUTH_UAS_ID_SIGNATURE,
};

/// `RID_AUTH_KEY_SIZE`
pub const AUTH_KEY_SIZE: usize = 32;
/// `RID_AUTH_SIG_SIZE` (Ed25519 signature length)
pub const AUTH_SIG_SIZE: usize = 64;

/// Port of the global `g_pk`/`g_auth_*` state of `rid_auth.c`.
#[derive(Default)]
pub struct AuthSigner {
    signing_key: Option<SigningKey>,
}

impl AuthSigner {
    /// Port of `rid_auth_init()`. Returns true when the PEM private key was
    /// parsed and is an Ed25519 key (mbedtls rejects any key whose bit length
    /// is not 256; dalek's Ed25519 parser rejects every other algorithm).
    pub fn init(&mut self, pem_key: Option<&str>) -> bool {
        let Some(pem) = pem_key else {
            self.signing_key = None;
            return false;
        };
        if pem.is_empty() {
            self.signing_key = None;
            return false;
        }
        match SigningKey::from_pkcs8_pem(pem) {
            Ok(key) => {
                self.signing_key = Some(key);
                true
            }
            Err(_) => {
                self.signing_key = None;
                false
            }
        }
    }

    /// Port of `rid_auth_enabled()`.
    pub fn enabled(&self) -> bool {
        self.signing_key.is_some()
    }

    /// Port of `rid_auth_sign_identity()`. Returns `None` when auth is
    /// disabled, the UAS ID is empty, or the signature needs more pages than
    /// `AUTH_MAX_PAGES`.
    ///
    /// Page 0 carries `AUTH_PAGE_ZERO_DATA_SIZE` bytes, later pages carry
    /// `AUTH_PAGE_NONZERO_DATA_SIZE` bytes each.
    pub fn sign_identity(&self, uas_id: &[u8]) -> Option<AuthPack> {
        let key = self.signing_key.as_ref()?;
        if uas_id.is_empty() || uas_id[0] == 0 {
            return None;
        }

        let sig = key.sign(uas_id).to_bytes();
        let sig_len = sig.len();

        let mut pages = 1u8;
        if sig_len > AUTH_PAGE_ZERO_DATA_SIZE {
            pages +=
                (sig_len - AUTH_PAGE_ZERO_DATA_SIZE).div_ceil(AUTH_PAGE_NONZERO_DATA_SIZE) as u8;
        }
        if pages as usize > AUTH_MAX_PAGES {
            return None;
        }

        let mut pack = AuthPack {
            pages: [AuthPage::default(); AUTH_MAX_PAGES],
            count: pages,
        };

        let mut offset = 0usize;
        for p in 0..pages as usize {
            let page = &mut pack.pages[p];
            page.data_page = p as u8;
            page.auth_type = AUTH_UAS_ID_SIGNATURE;
            page.last_page_index = pages - 1;
            page.length = sig_len as u8;

            let cap = if p == 0 {
                AUTH_PAGE_ZERO_DATA_SIZE
            } else {
                AUTH_PAGE_NONZERO_DATA_SIZE
            };
            let chunk = (sig_len - offset).min(cap);
            page.auth_data[..chunk].copy_from_slice(&sig[offset..offset + chunk]);
            offset += chunk;
        }

        Some(pack)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use pem_rfc7468::LineEnding;
    use pkcs8::EncodePrivateKey;

    fn valid_pem() -> String {
        // Real Ed25519 PKCS#8 PEM, generated at runtime from a fixed seed.
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        sk.to_pkcs8_pem(LineEnding::LF).unwrap().to_string()
    }

    #[test]
    fn init_rejects_missing_or_empty_key() {
        let mut s = AuthSigner::default();
        assert!(!s.init(None));
        assert!(!s.init(Some("")));
        assert!(!s.enabled());
    }

    #[test]
    fn init_accepts_ed25519_pem() {
        let pem = valid_pem();
        let mut s = AuthSigner::default();
        assert!(s.init(Some(&pem)));
        assert!(s.enabled());
    }

    #[test]
    fn init_rejects_non_ed25519_pem() {
        let mut s = AuthSigner::default();
        assert!(!s.init(Some(
            "-----BEGIN PRIVATE KEY-----\nnot a key\n-----END PRIVATE KEY-----\n"
        )));
        assert!(!s.enabled());
    }

    #[test]
    fn sign_identity_produces_4_pages_for_64_byte_sig() {
        let pem = valid_pem();
        let mut s = AuthSigner::default();
        assert!(s.init(Some(&pem)));
        let pack = s.sign_identity(b"TEST-UAS-123").expect("sign");
        // 64-byte Ed25519 sig: page0 17 + ceil(47/23)=3 -> 4 pages.
        assert_eq!(pack.count, 4);
        assert_eq!(pack.pages[0].data_page, 0);
        assert_eq!(pack.pages[0].auth_type, AUTH_UAS_ID_SIGNATURE);
        assert_eq!(pack.pages[0].last_page_index, 3);
        assert_eq!(pack.pages[0].length, AUTH_SIG_SIZE as u8);
        assert_eq!(pack.pages[3].last_page_index, 3);
        // Every byte of the signature is in the pages.
        let mut sig_rebuilt = [0u8; AUTH_SIG_SIZE];
        let mut offset = 0;
        for p in 0..pack.count as usize {
            let page = &pack.pages[p];
            let cap = if p == 0 {
                AUTH_PAGE_ZERO_DATA_SIZE
            } else {
                AUTH_PAGE_NONZERO_DATA_SIZE
            };
            let n = (AUTH_SIG_SIZE - offset).min(cap);
            sig_rebuilt[offset..offset + n].copy_from_slice(&page.auth_data[..n]);
            offset += n;
        }
        assert_eq!(offset, AUTH_SIG_SIZE);
        assert_ne!(sig_rebuilt, [0u8; AUTH_SIG_SIZE]);
    }

    #[test]
    fn sign_identity_empty_id_is_rejected() {
        let pem = valid_pem();
        let mut s = AuthSigner::default();
        assert!(s.init(Some(&pem)));
        assert!(s.sign_identity(b"").is_none());
        assert!(s.sign_identity(&[0u8]).is_none());
    }

    #[test]
    fn disabled_signer_returns_none() {
        let s = AuthSigner::default();
        assert!(s.sign_identity(b"TEST-UAS-123").is_none());
    }
}
