/*
 * Layout probe compiled alongside opendroneid.c. Exports the exact
 * sizeof/offsetof values the C compiler produces so the Rust FFI bindings
 * (lib.rs) can assert byte-for-byte layout parity in their tests.
 *
 * Part of the ESP32_DRONE_ID port; not part of the upstream OpenDroneID lib.
 */
#include "opendroneid.h"
#include <stddef.h>
#include <stdint.h>

#define EXPORT_SZ(T) size_t odid_sz_##T(void) { return sizeof(T); }

EXPORT_SZ(ODID_UAS_Data)
EXPORT_SZ(ODID_BasicID_data)
EXPORT_SZ(ODID_Location_data)
EXPORT_SZ(ODID_Auth_data)
EXPORT_SZ(ODID_SelfID_data)
EXPORT_SZ(ODID_System_data)
EXPORT_SZ(ODID_OperatorID_data)
EXPORT_SZ(ODID_MessagePack_data)

#define EXPORT_OFF(S, F) size_t odid_off_##S##_##F(void) { return offsetof(S, F); }

EXPORT_OFF(ODID_Location_data, Status)
EXPORT_OFF(ODID_Location_data, Direction)
EXPORT_OFF(ODID_Location_data, Latitude)
EXPORT_OFF(ODID_Location_data, Longitude)
EXPORT_OFF(ODID_Location_data, AltitudeBaro)
EXPORT_OFF(ODID_Location_data, AltitudeGeo)
EXPORT_OFF(ODID_Location_data, HeightType)
EXPORT_OFF(ODID_Location_data, Height)
EXPORT_OFF(ODID_Auth_data, DataPage)
EXPORT_OFF(ODID_Auth_data, AuthType)
EXPORT_OFF(ODID_Auth_data, LastPageIndex)
EXPORT_OFF(ODID_Auth_data, Length)
EXPORT_OFF(ODID_Auth_data, Timestamp)
EXPORT_OFF(ODID_Auth_data, AuthData)
EXPORT_OFF(ODID_System_data, OperatorLatitude)
EXPORT_OFF(ODID_System_data, OperatorLongitude)
EXPORT_OFF(ODID_System_data, AreaCount)
EXPORT_OFF(ODID_System_data, AreaRadius)
EXPORT_OFF(ODID_System_data, AreaCeiling)
EXPORT_OFF(ODID_System_data, AreaFloor)
EXPORT_OFF(ODID_System_data, CategoryEU)
EXPORT_OFF(ODID_System_data, ClassEU)
EXPORT_OFF(ODID_System_data, OperatorAltitudeGeo)
EXPORT_OFF(ODID_System_data, Timestamp)
