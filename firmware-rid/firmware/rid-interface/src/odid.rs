//! ODID message structures shared between the auth signer (`rid-core`),
//! the input parsers and the `out-*` encoders. Port of the relevant types
//! from `opendroneid.h`.

pub use crate::types::AUTH_MAX_PAGES;

/// `ODID_AUTH_PAGE_ZERO_DATA_SIZE`
pub const AUTH_PAGE_ZERO_DATA_SIZE: usize = 17;
/// `ODID_AUTH_PAGE_NONZERO_DATA_SIZE`
pub const AUTH_PAGE_NONZERO_DATA_SIZE: usize = 23;
/// `MAX_AUTH_LENGTH`
pub const MAX_AUTH_LENGTH: usize =
    AUTH_PAGE_ZERO_DATA_SIZE + AUTH_PAGE_NONZERO_DATA_SIZE * (AUTH_MAX_PAGES - 1);
/// `ODID_AUTH_UAS_ID_SIGNATURE`
pub const AUTH_UAS_ID_SIGNATURE: u8 = 1;

/// `ODID_Auth_data` (field order and sizes from `opendroneid.h`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AuthPage {
    pub data_page: u8,
    pub auth_type: u8,
    pub last_page_index: u8,
    pub length: u8,
    pub timestamp: u32,
    pub auth_data: [u8; AUTH_PAGE_NONZERO_DATA_SIZE + 1],
}

impl Default for AuthPage {
    fn default() -> Self {
        Self {
            data_page: 0,
            auth_type: 0,
            last_page_index: 0,
            length: 0,
            timestamp: 0,
            auth_data: [0; AUTH_PAGE_NONZERO_DATA_SIZE + 1],
        }
    }
}

/// A fully built Authentication message: up to `AUTH_MAX_PAGES` pages plus
/// the page count (port of the C `ODID_Auth_data auth[]` + page count).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AuthPack {
    pub pages: [AuthPage; AUTH_MAX_PAGES],
    pub count: u8,
}
