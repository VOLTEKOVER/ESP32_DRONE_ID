//! Operational regions and broadcast standards (exclusive binding).
//!
//! Port of `rid_region_t` and `rid_standard_t` from `esp_remote_id.h`.

/// Operational region. Selects the (exclusive) broadcast standard and the
/// message gating rules via the output hub.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Region {
    #[default]
    Auto = 0,
    /// Europe (EASA/EEA)
    Eur = 1,
    /// USA (FAA Part 89)
    Faa = 2,
    /// Japan (MLIT)
    Jpn = 3,
    /// Singapore (CAAS)
    Sgp = 4,
    /// South Korea (KASA)
    Kor = 5,
    /// China (GB 42590)
    Chn = 6,
    /// Canada (Transport Canada)
    Can = 7,
    /// Australia (CASA)
    Aus = 8,
    /// Brazil (ANAC)
    Bra = 9,
    /// New Zealand (CAA NZ)
    Nzl = 10,
}

impl Region {
    /// Number of valid regions (matches the C `g_region_rules` table size).
    pub const COUNT: usize = 11;

    pub const fn from_raw(v: u8) -> Option<Self> {
        match v {
            0 => Some(Region::Auto),
            1 => Some(Region::Eur),
            2 => Some(Region::Faa),
            3 => Some(Region::Jpn),
            4 => Some(Region::Sgp),
            5 => Some(Region::Kor),
            6 => Some(Region::Chn),
            7 => Some(Region::Can),
            8 => Some(Region::Aus),
            9 => Some(Region::Bra),
            10 => Some(Region::Nzl),
            _ => None,
        }
    }

    /// Maps a raw value to a `Region`, resolving out-of-range values to
    /// `Auto`. The C hub returns the AUTO row (ASTM standard, all messages
    /// allowed, both identity fields required) for any out-of-range index,
    /// so this preserves the exact behaviour for unreachable inputs.
    pub const fn from_raw_or_auto(v: u8) -> Self {
        match Self::from_raw(v) {
            Some(r) => r,
            None => Region::Auto,
        }
    }
}

/// Remote ID broadcast standards. Only one is active at a time.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Standard {
    /// ASTM F3411-22a (OpenDroneID)
    #[default]
    Astm = 0,
    /// China GB 42590-2023
    ChnGb = 1,
    /// US FAA FRDID
    Frdid = 2,
}

impl Standard {
    pub const COUNT: usize = 3;

    pub const fn from_raw(v: u8) -> Option<Self> {
        match v {
            0 => Some(Standard::Astm),
            1 => Some(Standard::ChnGb),
            2 => Some(Standard::Frdid),
            _ => None,
        }
    }

    /// Out-of-range standard resolves to ASTM (same default as the C hub).
    pub const fn from_raw_or_astm(v: u8) -> Self {
        match Self::from_raw(v) {
            Some(s) => s,
            None => Standard::Astm,
        }
    }
}

/// Message gating rules for a region: which ODID messages are allowed to be
/// broadcast and which identity fields are mandatory for readiness.
/// Port of `rid_region_rules_t`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RegionRules {
    /// Exclusive standard selected by the region.
    pub standard: Standard,
    /// Operator ID message allowed on air.
    pub operator_id_en: bool,
    /// Self-ID message allowed on air.
    pub self_id_en: bool,
    /// Second Basic ID (uas_id_2) allowed on air.
    pub basic_id_2_en: bool,
    /// Operator ID mandatory for identity readiness.
    pub require_operator_id: bool,
    /// UAS ID mandatory for identity readiness.
    pub require_uas_id: bool,
}
