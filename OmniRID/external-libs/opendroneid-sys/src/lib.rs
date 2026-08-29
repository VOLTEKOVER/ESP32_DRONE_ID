//! Raw bindings to the vendored official Open Drone ID C library.
//!
//! The C source is compiled unchanged by `build.rs` (see `vendor/`); this crate
//! only mirrors its `opendroneid.h` ABI. The normative (non-packed) data
//! structures are mirrored as `repr(C)` structs with the same field order; the
//! packed encoded structures have bitfields in C so they are exposed as
//! `MESSAGE_SIZE`-byte buffers that the encode functions write into.
//!
//! Only `opendroneid.c` (the message encoder/decoder) is compiled here. The
//! WiFi pack assembly (`odid_message_build_pack`/`odid_message_process_pack`
//! from `wifi.c`) is ported in `outputs/out-astm`.
//!
//! Layout parity with the C compiler is asserted in the tests against
//! `layout_probe.c` (compiled into the same static library).

#![cfg_attr(not(test), no_std)]

use core::ffi::{c_char, c_double, c_float, c_int};
use core::mem::MaybeUninit;

/// `ODID_MESSAGE_SIZE`
pub const ODID_MESSAGE_SIZE: usize = 25;
/// `ODID_ID_SIZE`
pub const ODID_ID_SIZE: usize = 20;
/// `ODID_STR_SIZE`
pub const ODID_STR_SIZE: usize = 23;
/// `ODID_PROTOCOL_VERSION`
pub const ODID_PROTOCOL_VERSION: u8 = 2;
/// `ODID_AUTH_MAX_PAGES`
pub const ODID_AUTH_MAX_PAGES: usize = 16;
/// `ODID_AUTH_PAGE_ZERO_DATA_SIZE`
pub const ODID_AUTH_PAGE_ZERO_DATA_SIZE: usize = 17;
/// `ODID_AUTH_PAGE_NONZERO_DATA_SIZE`
pub const ODID_AUTH_PAGE_NONZERO_DATA_SIZE: usize = 23;
/// `MAX_AUTH_LENGTH`
pub const MAX_AUTH_LENGTH: usize =
    ODID_AUTH_PAGE_ZERO_DATA_SIZE + ODID_AUTH_PAGE_NONZERO_DATA_SIZE * (ODID_AUTH_MAX_PAGES - 1);
/// `ODID_BASIC_ID_MAX_MESSAGES`
pub const ODID_BASIC_ID_MAX_MESSAGES: usize = 2;
/// `ODID_PACK_MAX_MESSAGES`
pub const ODID_PACK_MAX_MESSAGES: usize = 9;

/// `ODID_SUCCESS`
pub const ODID_SUCCESS: c_int = 0;
/// `ODID_FAIL`
pub const ODID_FAIL: c_int = 1;

/// `MIN_DIR`
pub const MIN_DIR: f32 = 0.0;
/// `MAX_DIR`
pub const MAX_DIR: f32 = 360.0;
/// `INV_DIR`
pub const INV_DIR: f32 = 361.0;
/// `MIN_SPEED_H`
pub const MIN_SPEED_H: f32 = 0.0;
/// `MAX_SPEED_H`
pub const MAX_SPEED_H: f32 = 254.25;
/// `INV_SPEED_H`
pub const INV_SPEED_H: f32 = 255.0;
/// `MIN_SPEED_V`
pub const MIN_SPEED_V: f32 = -62.0;
/// `MAX_SPEED_V`
pub const MAX_SPEED_V: f32 = 62.0;
/// `INV_SPEED_V`
pub const INV_SPEED_V: f32 = 63.0;
/// `MIN_LAT`
pub const MIN_LAT: f64 = -90.0;
/// `MAX_LAT`
pub const MAX_LAT: f64 = 90.0;
/// `MIN_LON`
pub const MIN_LON: f64 = -180.0;
/// `MAX_LON`
pub const MAX_LON: f64 = 180.0;
/// `MIN_ALT`
pub const MIN_ALT: f32 = -1000.0;
/// `MAX_ALT`
pub const MAX_ALT: f32 = 31767.5;
/// `INV_ALT`
pub const INV_ALT: f32 = MIN_ALT;
/// `MAX_TIMESTAMP`
pub const MAX_TIMESTAMP: u32 = 60 * 60;
/// `INV_TIMESTAMP`
pub const INV_TIMESTAMP: u16 = 0xFFFF;
/// `MAX_AREA_RADIUS`
pub const MAX_AREA_RADIUS: u16 = 2550;

/// `SPEED_DIV` (encoder quantization constants).
pub const SPEED_DIV: [f32; 2] = [0.25, 0.75];
/// `VSPEED_DIV`
pub const VSPEED_DIV: f32 = 0.5;
/// `LATLON_MULT`
pub const LATLON_MULT: i32 = 10_000_000;
/// `ALT_DIV`
pub const ALT_DIV: f32 = 0.5;
/// `ALT_ADDER`
pub const ALT_ADDER: c_int = 1000;

// -- Enum values (ODID_* constants from the header) --------------------------

/// `ODID_IDTYPE_NONE`
pub const ODID_IDTYPE_NONE: c_int = 0;
/// `ODID_IDTYPE_SERIAL_NUMBER`
pub const ODID_IDTYPE_SERIAL_NUMBER: c_int = 1;
/// `ODID_IDTYPE_CAA_REGISTRATION_ID`
pub const ODID_IDTYPE_CAA_REGISTRATION_ID: c_int = 2;
/// `ODID_IDTYPE_UTM_ASSIGNED_UUID`
pub const ODID_IDTYPE_UTM_ASSIGNED_UUID: c_int = 3;
/// `ODID_IDTYPE_SPECIFIC_SESSION_ID`
pub const ODID_IDTYPE_SPECIFIC_SESSION_ID: c_int = 4;

/// `ODID_UATYPE_NONE`
pub const ODID_UATYPE_NONE: c_int = 0;
/// `ODID_UATYPE_AEROPLANE`
pub const ODID_UATYPE_AEROPLANE: c_int = 1;
/// `ODID_UATYPE_HELICOPTER_OR_MULTIROTOR`
pub const ODID_UATYPE_HELICOPTER_OR_MULTIROTOR: c_int = 2;
/// `ODID_UATYPE_GYROPLANE`
pub const ODID_UATYPE_GYROPLANE: c_int = 3;
/// `ODID_UATYPE_HYBRID_LIFT`
pub const ODID_UATYPE_HYBRID_LIFT: c_int = 4;
/// `ODID_UATYPE_ORNITHOPTER`
pub const ODID_UATYPE_ORNITHOPTER: c_int = 5;
/// `ODID_UATYPE_GLIDER`
pub const ODID_UATYPE_GLIDER: c_int = 6;
/// `ODID_UATYPE_KITE`
pub const ODID_UATYPE_KITE: c_int = 7;
/// `ODID_UATYPE_FREE_BALLOON`
pub const ODID_UATYPE_FREE_BALLOON: c_int = 8;
/// `ODID_UATYPE_CAPTIVE_BALLOON`
pub const ODID_UATYPE_CAPTIVE_BALLOON: c_int = 9;
/// `ODID_UATYPE_AIRSHIP`
pub const ODID_UATYPE_AIRSHIP: c_int = 10;
/// `ODID_UATYPE_FREE_FALL_PARACHUTE`
pub const ODID_UATYPE_FREE_FALL_PARACHUTE: c_int = 11;
/// `ODID_UATYPE_ROCKET`
pub const ODID_UATYPE_ROCKET: c_int = 12;
/// `ODID_UATYPE_TETHERED_POWERED_AIRCRAFT`
pub const ODID_UATYPE_TETHERED_POWERED_AIRCRAFT: c_int = 13;
/// `ODID_UATYPE_GROUND_OBSTACLE`
pub const ODID_UATYPE_GROUND_OBSTACLE: c_int = 14;
/// `ODID_UATYPE_OTHER`
pub const ODID_UATYPE_OTHER: c_int = 15;

/// `ODID_STATUS_UNDECLARED`
pub const ODID_STATUS_UNDECLARED: c_int = 0;
/// `ODID_STATUS_GROUND`
pub const ODID_STATUS_GROUND: c_int = 1;
/// `ODID_STATUS_AIRBORNE`
pub const ODID_STATUS_AIRBORNE: c_int = 2;
/// `ODID_STATUS_EMERGENCY`
pub const ODID_STATUS_EMERGENCY: c_int = 3;
/// `ODID_STATUS_REMOTE_ID_SYSTEM_FAILURE`
pub const ODID_STATUS_REMOTE_ID_SYSTEM_FAILURE: c_int = 4;

/// `ODID_HEIGHT_REF_OVER_TAKEOFF`
pub const ODID_HEIGHT_REF_OVER_TAKEOFF: c_int = 0;
/// `ODID_HEIGHT_REF_OVER_GROUND`
pub const ODID_HEIGHT_REF_OVER_GROUND: c_int = 1;

/// `ODID_HOR_ACC_30_METER`
pub const ODID_HOR_ACC_30_METER: c_int = 9;
/// `ODID_HOR_ACC_10_METER`
pub const ODID_HOR_ACC_10_METER: c_int = 10;
/// `ODID_HOR_ACC_3_METER`
pub const ODID_HOR_ACC_3_METER: c_int = 11;
/// `ODID_HOR_ACC_1_METER`
pub const ODID_HOR_ACC_1_METER: c_int = 12;

/// `ODID_VER_ACC_45_METER`
pub const ODID_VER_ACC_45_METER: c_int = 2;
/// `ODID_VER_ACC_25_METER`
pub const ODID_VER_ACC_25_METER: c_int = 3;
/// `ODID_VER_ACC_10_METER`
pub const ODID_VER_ACC_10_METER: c_int = 4;
/// `ODID_VER_ACC_3_METER`
pub const ODID_VER_ACC_3_METER: c_int = 5;
/// `ODID_VER_ACC_1_METER`
pub const ODID_VER_ACC_1_METER: c_int = 6;

/// `ODID_AUTH_NONE`
pub const ODID_AUTH_NONE: c_int = 0;
/// `ODID_AUTH_UAS_ID_SIGNATURE`
pub const ODID_AUTH_UAS_ID_SIGNATURE: c_int = 1;

/// `ODID_DESC_TYPE_TEXT`
pub const ODID_DESC_TYPE_TEXT: c_int = 0;
/// `ODID_DESC_TYPE_EMERGENCY`
pub const ODID_DESC_TYPE_EMERGENCY: c_int = 1;
/// `ODID_DESC_TYPE_EXTENDED_STATUS`
pub const ODID_DESC_TYPE_EXTENDED_STATUS: c_int = 2;

/// `ODID_OPERATOR_ID`
pub const ODID_OPERATOR_ID: c_int = 0;

/// `ODID_OPERATOR_LOCATION_TYPE_TAKEOFF`
pub const ODID_OPERATOR_LOCATION_TYPE_TAKEOFF: c_int = 0;
/// `ODID_OPERATOR_LOCATION_TYPE_LIVE_GNSS`
pub const ODID_OPERATOR_LOCATION_TYPE_LIVE_GNSS: c_int = 1;
/// `ODID_OPERATOR_LOCATION_TYPE_FIXED`
pub const ODID_OPERATOR_LOCATION_TYPE_FIXED: c_int = 2;

/// `ODID_CLASSIFICATION_TYPE_UNDECLARED`
pub const ODID_CLASSIFICATION_TYPE_UNDECLARED: c_int = 0;
/// `ODID_CLASSIFICATION_TYPE_EU`
pub const ODID_CLASSIFICATION_TYPE_EU: c_int = 1;

/// `ODID_CATEGORY_EU_UNDECLARED`
pub const ODID_CATEGORY_EU_UNDECLARED: c_int = 0;
/// `ODID_CATEGORY_EU_OPEN`
pub const ODID_CATEGORY_EU_OPEN: c_int = 1;
/// `ODID_CATEGORY_EU_SPECIFIC`
pub const ODID_CATEGORY_EU_SPECIFIC: c_int = 2;
/// `ODID_CATEGORY_EU_CERTIFIED`
pub const ODID_CATEGORY_EU_CERTIFIED: c_int = 3;

/// `ODID_CLASS_EU_UNDECLARED`
pub const ODID_CLASS_EU_UNDECLARED: c_int = 0;
/// `ODID_CLASS_EU_CLASS_0`
pub const ODID_CLASS_EU_CLASS_0: c_int = 1;
/// `ODID_CLASS_EU_CLASS_1`
pub const ODID_CLASS_EU_CLASS_1: c_int = 2;
/// `ODID_CLASS_EU_CLASS_2`
pub const ODID_CLASS_EU_CLASS_2: c_int = 3;
/// `ODID_CLASS_EU_CLASS_3`
pub const ODID_CLASS_EU_CLASS_3: c_int = 4;
/// `ODID_CLASS_EU_CLASS_4`
pub const ODID_CLASS_EU_CLASS_4: c_int = 5;
/// `ODID_CLASS_EU_CLASS_5`
pub const ODID_CLASS_EU_CLASS_5: c_int = 6;
/// `ODID_CLASS_EU_CLASS_6`
pub const ODID_CLASS_EU_CLASS_6: c_int = 7;

// -- Message types (ODID_messagetype_t) ------------------------------------

pub const ODID_MESSAGETYPE_BASIC_ID: c_int = 0;
pub const ODID_MESSAGETYPE_LOCATION: c_int = 1;
pub const ODID_MESSAGETYPE_AUTH: c_int = 2;
pub const ODID_MESSAGETYPE_SELF_ID: c_int = 3;
pub const ODID_MESSAGETYPE_SYSTEM: c_int = 4;
pub const ODID_MESSAGETYPE_OPERATOR_ID: c_int = 5;
pub const ODID_MESSAGETYPE_PACKED: c_int = 0xF;
pub const ODID_MESSAGETYPE_INVALID: c_int = 0xFF;

// -- Normative data structs (exact mirror of opendroneid.h) ----------------

/// `ODID_BasicID_data`
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BasicIdData {
    /// `ODID_uatype_t UAType`
    pub ua_type: c_int,
    /// `ODID_idtype_t IDType`
    pub id_type: c_int,
    /// `char UASID[ODID_ID_SIZE+1]`
    pub uas_id: [c_char; ODID_ID_SIZE + 1],
}

/// `ODID_Location_data`
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocationData {
    /// `ODID_status_t Status`
    pub status: c_int,
    /// `float Direction`
    pub direction: c_float,
    /// `float SpeedHorizontal`
    pub speed_horizontal: c_float,
    /// `float SpeedVertical`
    pub speed_vertical: c_float,
    /// `double Latitude`
    pub latitude: c_double,
    /// `double Longitude`
    pub longitude: c_double,
    /// `float AltitudeBaro`
    pub altitude_baro: c_float,
    /// `float AltitudeGeo`
    pub altitude_geo: c_float,
    /// `ODID_Height_reference_t HeightType`
    pub height_type: c_int,
    /// `float Height`
    pub height: c_float,
    /// `ODID_Horizontal_accuracy_t HorizAccuracy`
    pub horiz_accuracy: c_int,
    /// `ODID_Vertical_accuracy_t VertAccuracy`
    pub vert_accuracy: c_int,
    /// `ODID_Vertical_accuracy_t BaroAccuracy`
    pub baro_accuracy: c_int,
    /// `ODID_Speed_accuracy_t SpeedAccuracy`
    pub speed_accuracy: c_int,
    /// `ODID_Timestamp_accuracy_t TSAccuracy`
    pub ts_accuracy: c_int,
    /// `float TimeStamp`
    pub time_stamp: c_float,
}

/// `ODID_Auth_data`
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthData {
    /// `uint8_t DataPage`
    pub data_page: u8,
    /// `ODID_authtype_t AuthType`
    pub auth_type: c_int,
    /// `uint8_t LastPageIndex`
    pub last_page_index: u8,
    /// `uint8_t Length`
    pub length: u8,
    /// `uint32_t Timestamp`
    pub timestamp: u32,
    /// `uint8_t AuthData[ODID_AUTH_PAGE_NONZERO_DATA_SIZE+1]`
    pub auth_data: [u8; ODID_AUTH_PAGE_NONZERO_DATA_SIZE + 1],
}

/// `ODID_SelfID_data`
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelfIdData {
    /// `ODID_desctype_t DescType`
    pub desc_type: c_int,
    /// `char Desc[ODID_STR_SIZE+1]`
    pub desc: [c_char; ODID_STR_SIZE + 1],
}

/// `ODID_System_data`
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SystemData {
    /// `ODID_operator_location_type_t OperatorLocationType`
    pub operator_location_type: c_int,
    /// `ODID_classification_type_t ClassificationType`
    pub classification_type: c_int,
    /// `double OperatorLatitude`
    pub operator_latitude: c_double,
    /// `double OperatorLongitude`
    pub operator_longitude: c_double,
    /// `uint16_t AreaCount`
    pub area_count: u16,
    /// `uint16_t AreaRadius`
    pub area_radius: u16,
    /// `float AreaCeiling`
    pub area_ceiling: c_float,
    /// `float AreaFloor`
    pub area_floor: c_float,
    /// `ODID_category_EU_t CategoryEU`
    pub category_eu: c_int,
    /// `ODID_class_EU_t ClassEU`
    pub class_eu: c_int,
    /// `float OperatorAltitudeGeo`
    pub operator_altitude_geo: c_float,
    /// `uint32_t Timestamp`
    pub timestamp: u32,
}

/// `ODID_OperatorID_data`
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperatorIdData {
    /// `ODID_operatorIdType_t OperatorIdType`
    pub operator_id_type: c_int,
    /// `char OperatorId[ODID_ID_SIZE+1]`
    pub operator_id: [c_char; ODID_ID_SIZE + 1],
}

/// `ODID_UAS_Data`
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UasData {
    /// `ODID_BasicID_data BasicID[ODID_BASIC_ID_MAX_MESSAGES]`
    pub basic_id: [BasicIdData; ODID_BASIC_ID_MAX_MESSAGES],
    /// `ODID_Location_data Location`
    pub location: LocationData,
    /// `ODID_Auth_data Auth[ODID_AUTH_MAX_PAGES]`
    pub auth: [AuthData; ODID_AUTH_MAX_PAGES],
    /// `ODID_SelfID_data SelfID`
    pub self_id: SelfIdData,
    /// `ODID_System_data System`
    pub system: SystemData,
    /// `ODID_OperatorID_data OperatorID`
    pub operator_id: OperatorIdData,
    /// `uint8_t BasicIDValid[ODID_BASIC_ID_MAX_MESSAGES]`
    pub basic_id_valid: [u8; ODID_BASIC_ID_MAX_MESSAGES],
    /// `uint8_t LocationValid`
    pub location_valid: u8,
    /// `uint8_t AuthValid[ODID_AUTH_MAX_PAGES]`
    pub auth_valid: [u8; ODID_AUTH_MAX_PAGES],
    /// `uint8_t SelfIDValid`
    pub self_id_valid: u8,
    /// `uint8_t SystemValid`
    pub system_valid: u8,
    /// `uint8_t OperatorIDValid`
    pub operator_id_valid: u8,
}

// -- Packed encoded structs (25-byte buffers; C uses bitfields) ------------

/// `ODID_BasicID_encoded`
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BasicIdEncoded(pub [u8; ODID_MESSAGE_SIZE]);

/// `ODID_Location_encoded`
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocationEncoded(pub [u8; ODID_MESSAGE_SIZE]);

/// `ODID_Auth_encoded`
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthEncoded(pub [u8; ODID_MESSAGE_SIZE]);

/// `ODID_SelfID_encoded`
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelfIdEncoded(pub [u8; ODID_MESSAGE_SIZE]);

/// `ODID_System_encoded`
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemEncoded(pub [u8; ODID_MESSAGE_SIZE]);

/// `ODID_OperatorID_encoded`
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperatorIdEncoded(pub [u8; ODID_MESSAGE_SIZE]);

/// `ODID_Message_encoded`
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MessageEncoded(pub [u8; ODID_MESSAGE_SIZE]);

impl MessageEncoded {
    /// Returns the raw bytes.
    pub fn as_bytes(&self) -> &[u8; ODID_MESSAGE_SIZE] {
        &self.0
    }
}

impl From<BasicIdEncoded> for MessageEncoded {
    fn from(m: BasicIdEncoded) -> Self {
        MessageEncoded(m.0)
    }
}
impl From<LocationEncoded> for MessageEncoded {
    fn from(m: LocationEncoded) -> Self {
        MessageEncoded(m.0)
    }
}
impl From<AuthEncoded> for MessageEncoded {
    fn from(m: AuthEncoded) -> Self {
        MessageEncoded(m.0)
    }
}
impl From<SelfIdEncoded> for MessageEncoded {
    fn from(m: SelfIdEncoded) -> Self {
        MessageEncoded(m.0)
    }
}
impl From<SystemEncoded> for MessageEncoded {
    fn from(m: SystemEncoded) -> Self {
        MessageEncoded(m.0)
    }
}
impl From<OperatorIdEncoded> for MessageEncoded {
    fn from(m: OperatorIdEncoded) -> Self {
        MessageEncoded(m.0)
    }
}

/// `ODID_MessagePack_data` (normative form, used by `encodeMessagePack`).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MessagePackData {
    /// `uint8_t SingleMessageSize`
    pub single_message_size: u8,
    /// `uint8_t MsgPackSize`
    pub msg_pack_size: u8,
    /// `ODID_Message_encoded Messages[ODID_PACK_MAX_MESSAGES]`
    pub messages: [MessageEncoded; ODID_PACK_MAX_MESSAGES],
}

/// `ODID_MessagePack_encoded` (packed byte stream: 3 header bytes + messages).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MessagePackEncoded {
    /// Byte 0: `ProtoVersion` (LSb 4 bits) | `MessageType` (bits 4-7).
    pub proto_version_message_type: u8,
    /// Byte 1: `SingleMessageSize`.
    pub single_message_size: u8,
    /// Byte 2: `MsgPackSize`.
    pub msg_pack_size: u8,
    /// Bytes 3+: `Messages`.
    pub messages: [MessageEncoded; ODID_PACK_MAX_MESSAGES],
}

impl MessagePackEncoded {
    /// Size in bytes: 3 header bytes + `n` 25-byte messages.
    pub fn len_for(n: usize) -> usize {
        3 + n * ODID_MESSAGE_SIZE
    }
}

// -- Raw C bindings ----------------------------------------------------------

extern "C" {
    pub fn odid_initUasData(data: *mut UasData);

    pub fn encodeBasicIDMessage(
        out_encoded: *mut BasicIdEncoded,
        in_data: *const BasicIdData,
    ) -> c_int;
    pub fn encodeLocationMessage(
        out_encoded: *mut LocationEncoded,
        in_data: *const LocationData,
    ) -> c_int;
    pub fn encodeAuthMessage(out_encoded: *mut AuthEncoded, in_data: *const AuthData) -> c_int;
    pub fn encodeSelfIDMessage(
        out_encoded: *mut SelfIdEncoded,
        in_data: *const SelfIdData,
    ) -> c_int;
    pub fn encodeSystemMessage(
        out_encoded: *mut SystemEncoded,
        in_data: *const SystemData,
    ) -> c_int;
    pub fn encodeOperatorIDMessage(
        out_encoded: *mut OperatorIdEncoded,
        in_data: *const OperatorIdData,
    ) -> c_int;
    pub fn encodeMessagePack(
        out_encoded: *mut MessagePackEncoded,
        in_data: *const MessagePackData,
    ) -> c_int;

    /// `ODID_messagetype_t decodeMessageType(uint8_t byte)`.
    pub fn decodeMessageType(byte: u8) -> c_int;

    pub fn decodeBasicIDMessage(
        out_data: *mut BasicIdData,
        in_encoded: *const BasicIdEncoded,
    ) -> c_int;
    pub fn decodeLocationMessage(
        out_data: *mut LocationData,
        in_encoded: *const LocationEncoded,
    ) -> c_int;
    pub fn decodeAuthMessage(out_data: *mut AuthData, in_encoded: *const AuthEncoded) -> c_int;
    pub fn decodeSelfIDMessage(
        out_data: *mut SelfIdData,
        in_encoded: *const SelfIdEncoded,
    ) -> c_int;
    pub fn decodeSystemMessage(
        out_data: *mut SystemData,
        in_encoded: *const SystemEncoded,
    ) -> c_int;
    pub fn decodeOperatorIDMessage(
        out_data: *mut OperatorIdData,
        in_encoded: *const OperatorIdEncoded,
    ) -> c_int;
    pub fn decodeMessagePack(uas_data: *mut UasData, pack: *const MessagePackEncoded) -> c_int;

    /// `ODID_messagetype_t decodeOpenDroneID(ODID_UAS_Data *, const uint8_t *)`.
    pub fn decodeOpenDroneID(uas_data: *mut UasData, msg_data: *const u8) -> c_int;
}

// -- Safe wrappers ------------------------------------------------------------

/// `odid_initUasData` (zeroed + invalid-field defaults).
pub fn init_uas_data() -> UasData {
    let mut data = MaybeUninit::<UasData>::uninit();
    unsafe { odid_initUasData(data.as_mut_ptr()) };
    unsafe { data.assume_init() }
}

/// `encodeBasicIDMessage`, returns `ODID_SUCCESS`/`ODID_FAIL`.
pub fn encode_basic_id(out: &mut BasicIdEncoded, data: &BasicIdData) -> c_int {
    unsafe { encodeBasicIDMessage(out as *mut _, data as *const _) }
}

/// `encodeLocationMessage`.
pub fn encode_location(out: &mut LocationEncoded, data: &LocationData) -> c_int {
    unsafe { encodeLocationMessage(out as *mut _, data as *const _) }
}

/// `encodeAuthMessage`.
pub fn encode_auth(out: &mut AuthEncoded, data: &AuthData) -> c_int {
    unsafe { encodeAuthMessage(out as *mut _, data as *const _) }
}

/// `encodeSelfIDMessage`.
pub fn encode_self_id(out: &mut SelfIdEncoded, data: &SelfIdData) -> c_int {
    unsafe { encodeSelfIDMessage(out as *mut _, data as *const _) }
}

/// `encodeSystemMessage`.
pub fn encode_system(out: &mut SystemEncoded, data: &SystemData) -> c_int {
    unsafe { encodeSystemMessage(out as *mut _, data as *const _) }
}

/// `encodeOperatorIDMessage`.
pub fn encode_operator_id(out: &mut OperatorIdEncoded, data: &OperatorIdData) -> c_int {
    unsafe { encodeOperatorIDMessage(out as *mut _, data as *const _) }
}

/// `encodeMessagePack`.
pub fn encode_message_pack(out: &mut MessagePackEncoded, data: &MessagePackData) -> c_int {
    unsafe { encodeMessagePack(out as *mut _, data as *const _) }
}

/// `decodeMessageType` (upper nibble of the message byte).
pub fn decode_message_type(byte: u8) -> c_int {
    unsafe { decodeMessageType(byte) }
}

// -- Layout probe (test-only) ------------------------------------------------

#[cfg(test)]
mod layout {
    use super::*;
    use core::mem::offset_of;

    extern "C" {
        pub fn odid_sz_ODID_UAS_Data() -> usize;
        pub fn odid_sz_ODID_BasicID_data() -> usize;
        pub fn odid_sz_ODID_Location_data() -> usize;
        pub fn odid_sz_ODID_Auth_data() -> usize;
        pub fn odid_sz_ODID_SelfID_data() -> usize;
        pub fn odid_sz_ODID_System_data() -> usize;
        pub fn odid_sz_ODID_OperatorID_data() -> usize;
        pub fn odid_sz_ODID_MessagePack_data() -> usize;

        pub fn odid_off_ODID_Location_data_Status() -> usize;
        pub fn odid_off_ODID_Location_data_Direction() -> usize;
        pub fn odid_off_ODID_Location_data_Latitude() -> usize;
        pub fn odid_off_ODID_Location_data_Longitude() -> usize;
        pub fn odid_off_ODID_Location_data_AltitudeBaro() -> usize;
        pub fn odid_off_ODID_Location_data_AltitudeGeo() -> usize;
        pub fn odid_off_ODID_Location_data_HeightType() -> usize;
        pub fn odid_off_ODID_Location_data_Height() -> usize;
        pub fn odid_off_ODID_Auth_data_DataPage() -> usize;
        pub fn odid_off_ODID_Auth_data_AuthType() -> usize;
        pub fn odid_off_ODID_Auth_data_LastPageIndex() -> usize;
        pub fn odid_off_ODID_Auth_data_Length() -> usize;
        pub fn odid_off_ODID_Auth_data_Timestamp() -> usize;
        pub fn odid_off_ODID_Auth_data_AuthData() -> usize;
        pub fn odid_off_ODID_System_data_OperatorLatitude() -> usize;
        pub fn odid_off_ODID_System_data_OperatorLongitude() -> usize;
        pub fn odid_off_ODID_System_data_AreaCount() -> usize;
        pub fn odid_off_ODID_System_data_AreaRadius() -> usize;
        pub fn odid_off_ODID_System_data_AreaCeiling() -> usize;
        pub fn odid_off_ODID_System_data_AreaFloor() -> usize;
        pub fn odid_off_ODID_System_data_CategoryEU() -> usize;
        pub fn odid_off_ODID_System_data_ClassEU() -> usize;
        pub fn odid_off_ODID_System_data_OperatorAltitudeGeo() -> usize;
        pub fn odid_off_ODID_System_data_Timestamp() -> usize;
    }

    #[test]
    fn data_struct_sizes_match_c() {
        assert_eq!(unsafe { odid_sz_ODID_UAS_Data() }, size_of::<UasData>());
        assert_eq!(
            unsafe { odid_sz_ODID_BasicID_data() },
            size_of::<BasicIdData>()
        );
        assert_eq!(
            unsafe { odid_sz_ODID_Location_data() },
            size_of::<LocationData>()
        );
        assert_eq!(unsafe { odid_sz_ODID_Auth_data() }, size_of::<AuthData>());
        assert_eq!(
            unsafe { odid_sz_ODID_SelfID_data() },
            size_of::<SelfIdData>()
        );
        assert_eq!(
            unsafe { odid_sz_ODID_System_data() },
            size_of::<SystemData>()
        );
        assert_eq!(
            unsafe { odid_sz_ODID_OperatorID_data() },
            size_of::<OperatorIdData>()
        );
        assert_eq!(
            unsafe { odid_sz_ODID_MessagePack_data() },
            size_of::<MessagePackData>()
        );
    }

    #[test]
    fn data_struct_offsets_match_c() {
        assert_eq!(offset_of!(LocationData, status), unsafe {
            odid_off_ODID_Location_data_Status()
        });
        assert_eq!(offset_of!(LocationData, direction), unsafe {
            odid_off_ODID_Location_data_Direction()
        });
        assert_eq!(offset_of!(LocationData, latitude), unsafe {
            odid_off_ODID_Location_data_Latitude()
        });
        assert_eq!(offset_of!(LocationData, longitude), unsafe {
            odid_off_ODID_Location_data_Longitude()
        });
        assert_eq!(offset_of!(LocationData, altitude_baro), unsafe {
            odid_off_ODID_Location_data_AltitudeBaro()
        });
        assert_eq!(offset_of!(LocationData, altitude_geo), unsafe {
            odid_off_ODID_Location_data_AltitudeGeo()
        });
        assert_eq!(offset_of!(LocationData, height_type), unsafe {
            odid_off_ODID_Location_data_HeightType()
        });
        assert_eq!(offset_of!(LocationData, height), unsafe {
            odid_off_ODID_Location_data_Height()
        });

        assert_eq!(offset_of!(AuthData, data_page), unsafe {
            odid_off_ODID_Auth_data_DataPage()
        });
        assert_eq!(offset_of!(AuthData, auth_type), unsafe {
            odid_off_ODID_Auth_data_AuthType()
        });
        assert_eq!(offset_of!(AuthData, last_page_index), unsafe {
            odid_off_ODID_Auth_data_LastPageIndex()
        });
        assert_eq!(offset_of!(AuthData, length), unsafe {
            odid_off_ODID_Auth_data_Length()
        });
        assert_eq!(offset_of!(AuthData, timestamp), unsafe {
            odid_off_ODID_Auth_data_Timestamp()
        });
        assert_eq!(offset_of!(AuthData, auth_data), unsafe {
            odid_off_ODID_Auth_data_AuthData()
        });

        assert_eq!(offset_of!(SystemData, operator_latitude), unsafe {
            odid_off_ODID_System_data_OperatorLatitude()
        });
        assert_eq!(offset_of!(SystemData, operator_longitude), unsafe {
            odid_off_ODID_System_data_OperatorLongitude()
        });
        assert_eq!(offset_of!(SystemData, area_count), unsafe {
            odid_off_ODID_System_data_AreaCount()
        });
        assert_eq!(offset_of!(SystemData, area_radius), unsafe {
            odid_off_ODID_System_data_AreaRadius()
        });
        assert_eq!(offset_of!(SystemData, area_ceiling), unsafe {
            odid_off_ODID_System_data_AreaCeiling()
        });
        assert_eq!(offset_of!(SystemData, area_floor), unsafe {
            odid_off_ODID_System_data_AreaFloor()
        });
        assert_eq!(offset_of!(SystemData, category_eu), unsafe {
            odid_off_ODID_System_data_CategoryEU()
        });
        assert_eq!(offset_of!(SystemData, class_eu), unsafe {
            odid_off_ODID_System_data_ClassEU()
        });
        assert_eq!(offset_of!(SystemData, operator_altitude_geo), unsafe {
            odid_off_ODID_System_data_OperatorAltitudeGeo()
        });
        assert_eq!(offset_of!(SystemData, timestamp), unsafe {
            odid_off_ODID_System_data_Timestamp()
        });
    }

    #[test]
    fn encoded_structs_are_25_bytes() {
        assert_eq!(size_of::<BasicIdEncoded>(), ODID_MESSAGE_SIZE);
        assert_eq!(size_of::<LocationEncoded>(), ODID_MESSAGE_SIZE);
        assert_eq!(size_of::<AuthEncoded>(), ODID_MESSAGE_SIZE);
        assert_eq!(size_of::<SelfIdEncoded>(), ODID_MESSAGE_SIZE);
        assert_eq!(size_of::<SystemEncoded>(), ODID_MESSAGE_SIZE);
        assert_eq!(size_of::<OperatorIdEncoded>(), ODID_MESSAGE_SIZE);
        assert_eq!(size_of::<MessageEncoded>(), ODID_MESSAGE_SIZE);
        assert_eq!(
            size_of::<MessagePackEncoded>(),
            3 + ODID_PACK_MAX_MESSAGES * ODID_MESSAGE_SIZE
        );
    }

    #[test]
    fn init_uas_data_matches_odid_init() {
        // odid_initUasData = memset(0) + odid_init*Data defaults.
        let mut d = init_uas_data();
        assert_eq!(
            d.system.area_count, 1,
            "odid_initSystemData sets AreaCount=1"
        );
        assert_eq!(d.system.area_ceiling, INV_ALT);
        assert_eq!(d.system.area_floor, INV_ALT);
        assert_eq!(d.system.operator_altitude_geo, INV_ALT);
        assert_eq!(d.location.direction, INV_DIR);
        assert_eq!(d.location.speed_horizontal, INV_SPEED_H);
        assert_eq!(d.location.speed_vertical, INV_SPEED_V);
        assert_eq!(d.location.altitude_baro, INV_ALT);
        assert_eq!(d.location.altitude_geo, INV_ALT);
        assert_eq!(d.location.height, INV_ALT);

        // Zero everything back out and compare byte-for-byte with a zeroed struct.
        d.system.area_count = 0;
        d.system.area_ceiling = 0.0;
        d.system.area_floor = 0.0;
        d.system.operator_altitude_geo = 0.0;
        d.location.direction = 0.0;
        d.location.speed_horizontal = 0.0;
        d.location.speed_vertical = 0.0;
        d.location.altitude_baro = 0.0;
        d.location.altitude_geo = 0.0;
        d.location.height = 0.0;
        let zeroed = unsafe { core::mem::zeroed::<UasData>() };
        assert_eq!(d.as_bytes(), zeroed.as_bytes());
    }
}

impl UasData {
    /// Byte view of the struct (used by the tests).
    #[cfg(test)]
    fn as_bytes(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts((self as *const UasData).cast::<u8>(), size_of::<UasData>())
        }
    }
}

// -- FFI behaviour tests --------------------------------------------------------

#[cfg(test)]
mod ffi_tests {
    use super::*;

    /// ASCII comparison of a C `char` array against a byte literal.
    fn eq_chars(dst: &[c_char], src: &[u8]) -> bool {
        dst.iter().zip(src).all(|(&a, &b)| a as u8 == b)
    }

    fn uas_data_with_basic_id() -> UasData {
        let mut d = init_uas_data();
        d.basic_id_valid[0] = 1;
        d.basic_id[0].id_type = ODID_IDTYPE_SERIAL_NUMBER;
        d.basic_id[0].ua_type = ODID_UATYPE_HELICOPTER_OR_MULTIROTOR;
        let s = b"ESP32-RID-001";
        for (dst, src) in d.basic_id[0].uas_id.iter_mut().zip(s) {
            *dst = *src as c_char;
        }
        d
    }

    #[test]
    fn message_type_nibble() {
        assert_eq!(decode_message_type(0x02), ODID_MESSAGETYPE_BASIC_ID);
        assert_eq!(decode_message_type(0x12), ODID_MESSAGETYPE_LOCATION);
        assert_eq!(decode_message_type(0xF2), ODID_MESSAGETYPE_PACKED);
        assert_eq!(decode_message_type(0xA2), ODID_MESSAGETYPE_INVALID);
    }

    #[test]
    fn encode_basic_id_success_and_bytes() {
        let d = uas_data_with_basic_id();
        let mut enc = BasicIdEncoded([0; ODID_MESSAGE_SIZE]);
        assert_eq!(encode_basic_id(&mut enc, &d.basic_id[0]), ODID_SUCCESS);
        // Byte 0: ProtoVersion=2, MessageType=0 -> 0x02
        assert_eq!(enc.0[0], ODID_PROTOCOL_VERSION);
        // Byte 1: IDType (hi nibble) | UAType (lo nibble): 1 | 2 -> 0x12
        assert_eq!(enc.0[1], 0x12);
        // Bytes 2-21: UASID, NUL-terminated at byte 15.
        assert_eq!(&enc.0[2..15], b"ESP32-RID-001");
        assert_eq!(enc.0[15], 0);
        // Bytes 22-24 reserved, zero.
        assert_eq!(&enc.0[22..25], &[0, 0, 0]);
    }

    #[test]
    fn encode_rejects_out_of_range() {
        let mut d = uas_data_with_basic_id();
        d.basic_id[0].id_type = 16;
        let mut enc = BasicIdEncoded([0; ODID_MESSAGE_SIZE]);
        assert_eq!(encode_basic_id(&mut enc, &d.basic_id[0]), ODID_FAIL);
    }

    #[test]
    fn decode_roundtrip_basic_id() {
        let d = uas_data_with_basic_id();
        let mut enc = BasicIdEncoded([0; ODID_MESSAGE_SIZE]);
        assert_eq!(encode_basic_id(&mut enc, &d.basic_id[0]), ODID_SUCCESS);
        let mut back = BasicIdData {
            ua_type: 0,
            id_type: 0,
            uas_id: [0; ODID_ID_SIZE + 1],
        };
        assert_eq!(
            unsafe { decodeBasicIDMessage(&mut back, &enc) },
            ODID_SUCCESS
        );
        assert_eq!(back.id_type, d.basic_id[0].id_type);
        assert_eq!(back.ua_type, d.basic_id[0].ua_type);
        assert!(eq_chars(&back.uas_id, b"ESP32-RID-001"));
    }

    #[test]
    fn encode_system_roundtrip() {
        let mut d = init_uas_data();
        d.system_valid = 1;
        d.system.operator_location_type = ODID_OPERATOR_LOCATION_TYPE_TAKEOFF;
        d.system.operator_latitude = 45.30405;
        d.system.operator_longitude = 11.95375;
        d.system.area_count = 1;
        d.system.area_radius = 0;
        d.system.operator_altitude_geo = 123.4;

        let mut enc = SystemEncoded([0; ODID_MESSAGE_SIZE]);
        assert_eq!(encode_system(&mut enc, &d.system), ODID_SUCCESS);
        // Byte 0: 0xF? no - SYSTEM type = 4 -> 0x42
        assert_eq!(enc.0[0], 0x42);
        // Byte 1: OperatorLocationType=0 (2 bits) | ClassificationType=0 (3) | Reserved=0 (3)
        assert_eq!(enc.0[1], 0x00);

        let mut back = SystemData {
            operator_location_type: 0,
            classification_type: 0,
            operator_latitude: 0.0,
            operator_longitude: 0.0,
            area_count: 0,
            area_radius: 0,
            area_ceiling: 0.0,
            area_floor: 0.0,
            category_eu: 0,
            class_eu: 0,
            operator_altitude_geo: 0.0,
            timestamp: 0,
        };
        assert_eq!(
            unsafe { decodeSystemMessage(&mut back, &enc) },
            ODID_SUCCESS
        );
        assert_eq!(back.operator_latitude, 45.30405);
        assert_eq!(back.operator_longitude, 11.95375);
        assert_eq!(back.area_count, 1);
    }

    #[test]
    fn encode_auth_page_zero_and_nonzero() {
        let mut page0 = init_uas_data().auth[0];
        page0.data_page = 0;
        page0.auth_type = ODID_AUTH_UAS_ID_SIGNATURE;
        page0.last_page_index = 2;
        page0.length = 63;
        page0.timestamp = 1234;
        let mut enc = AuthEncoded([0; ODID_MESSAGE_SIZE]);
        assert_eq!(encode_auth(&mut enc, &page0), ODID_SUCCESS);
        assert_eq!(enc.0[0], 0x22, "AUTH (2) | proto 2");
        assert_eq!(enc.0[1], 0x10, "DataPage=0 (lo) | AuthType=1 (hi)");
        assert_eq!(enc.0[2], 2);
        assert_eq!(enc.0[3], 63);
        assert_eq!(u16::from_le_bytes([enc.0[4], enc.0[5]]), 1234);

        let mut page1 = init_uas_data().auth[0];
        page1.data_page = 1;
        page1.auth_type = ODID_AUTH_UAS_ID_SIGNATURE;
        page1.auth_data[0..3].copy_from_slice(&[0xAA, 0xBB, 0xCC]);
        let mut enc1 = AuthEncoded([0; ODID_MESSAGE_SIZE]);
        assert_eq!(encode_auth(&mut enc1, &page1), ODID_SUCCESS);
        assert_eq!(enc1.0[0], 0x22);
        assert_eq!(enc1.0[1], 0x11, "DataPage=1 (lo) | AuthType=1 (hi)");
        assert_eq!(&enc1.0[2..5], &[0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn encode_message_pack_basic_and_location() {
        let d = uas_data_with_basic_id();
        let mut enc = BasicIdEncoded([0; ODID_MESSAGE_SIZE]);
        assert_eq!(encode_basic_id(&mut enc, &d.basic_id[0]), ODID_SUCCESS);
        let m1 = enc.into();

        let mut loc = d.location;
        loc.status = ODID_STATUS_UNDECLARED;
        loc.latitude = 45.30405;
        loc.longitude = 11.95375;
        loc.altitude_geo = 123.4;
        loc.altitude_baro = 122.0;
        loc.height = 60.0;
        loc.height_type = ODID_HEIGHT_REF_OVER_TAKEOFF;
        loc.direction = 90.0;
        loc.speed_horizontal = 12.0;
        loc.speed_vertical = 0.0;
        let mut loc_enc = LocationEncoded([0; ODID_MESSAGE_SIZE]);
        assert_eq!(encode_location(&mut loc_enc, &loc), ODID_SUCCESS);
        let m2 = loc_enc.into();

        let mut msgs = [MessageEncoded([0; ODID_MESSAGE_SIZE]); 9];
        msgs[0] = m1;
        msgs[1] = m2;
        let data = MessagePackData {
            single_message_size: ODID_MESSAGE_SIZE as u8,
            msg_pack_size: 2,
            messages: msgs,
        };
        let mut out = MessagePackEncoded {
            proto_version_message_type: 0,
            single_message_size: 0,
            msg_pack_size: 0,
            messages: [MessageEncoded([0; ODID_MESSAGE_SIZE]); 9],
        };
        assert_eq!(encode_message_pack(&mut out, &data), ODID_SUCCESS);
        assert_eq!(out.proto_version_message_type, 0xF2);
        assert_eq!(out.single_message_size, ODID_MESSAGE_SIZE as u8);
        assert_eq!(out.msg_pack_size, 2);
        assert_eq!(
            decode_message_type(out.messages[0].0[0]),
            ODID_MESSAGETYPE_BASIC_ID
        );
        assert_eq!(
            decode_message_type(out.messages[1].0[0]),
            ODID_MESSAGETYPE_LOCATION
        );
    }
}
