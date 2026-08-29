//! Stubs for the not-yet-implemented broadcast standards.
//!
//! GB 42590-2023 (China) and FRDID have no encoder yet; the hourglass hub
//! already reports `has_encoder == false` for them so the firmware falls back
//! to ASTM. These functions are the future home of `out-china-gb42590` and
//! `out-frdid`, kept 1:1 with the current firmware behaviour (fallback).

use opendroneid_sys::UasData;

/// Encoder errors for the stub standards.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EncodeError {
    /// The encoder is not implemented yet.
    NotImplemented,
}

/// Stub: GB 42590-2023 pack encoder (China). Always fails today.
pub fn encode_gb42590(_uas: &UasData, _buf: &mut [u8]) -> Result<usize, EncodeError> {
    Err(EncodeError::NotImplemented)
}

/// Stub: FRDID pack encoder (FAA Remote ID, CTA-2063-A). Always fails today.
pub fn encode_frdid(_uas: &UasData, _buf: &mut [u8]) -> Result<usize, EncodeError> {
    Err(EncodeError::NotImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stubs_report_not_implemented() {
        let uas = opendroneid_sys::init_uas_data();
        let mut buf = [0u8; 256];
        assert_eq!(
            encode_gb42590(&uas, &mut buf),
            Err(EncodeError::NotImplemented)
        );
        assert_eq!(
            encode_frdid(&uas, &mut buf),
            Err(EncodeError::NotImplemented)
        );
    }
}
