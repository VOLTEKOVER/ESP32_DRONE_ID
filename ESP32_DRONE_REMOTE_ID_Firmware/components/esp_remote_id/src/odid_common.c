#include <string.h>
#include "odid_common.h"
#include "rid_auth.h"

static ODID_Horizontal_accuracy_t horiz_acc(uint8_t fix_type, uint8_t satellites)
{
    if (fix_type >= 4 && satellites >= 15) return ODID_HOR_ACC_1_METER;
    if (fix_type >= 4 && satellites >= 10) return ODID_HOR_ACC_3_METER;
    if (fix_type >= 4) return ODID_HOR_ACC_10_METER;
    if (fix_type >= 3) return ODID_HOR_ACC_10_METER;
    return ODID_HOR_ACC_30_METER;
}

static ODID_Vertical_accuracy_t vert_acc(uint8_t fix_type, uint8_t satellites)
{
    if (fix_type >= 4 && satellites >= 15) return ODID_VER_ACC_3_METER;
    if (fix_type >= 4 && satellites >= 10) return ODID_VER_ACC_10_METER;
    if (fix_type >= 4) return ODID_VER_ACC_25_METER;
    if (fix_type >= 3) return ODID_VER_ACC_25_METER;
    return ODID_VER_ACC_45_METER;
}

void odid_common_build_uas_data(ODID_UAS_Data *d, const rid_gps_data_t *gps,
                                const rid_identity_t *identity)
{
    memset(d, 0, sizeof(ODID_UAS_Data));

    d->BasicIDValid[0] = 1;
    d->BasicID[0].IDType = (ODID_idtype_t)identity->id_type;
    d->BasicID[0].UAType = (ODID_uatype_t)identity->ua_type;
    strncpy((char *)d->BasicID[0].UASID, identity->uas_id, ODID_ID_SIZE);

    if (identity->uas_id_2[0] != '\0') {
        d->BasicIDValid[1] = 1;
        d->BasicID[1].IDType = (ODID_idtype_t)identity->id_type_2;
        d->BasicID[1].UAType = (ODID_uatype_t)identity->ua_type_2;
        strncpy((char *)d->BasicID[1].UASID, identity->uas_id_2, ODID_ID_SIZE);
    }

    d->LocationValid = 1;
    d->Location.Latitude = gps->latitude;
    d->Location.Longitude = gps->longitude;
    d->Location.AltitudeGeo = gps->altitude_msl;
    d->Location.Height = gps->altitude_relative;
    d->Location.AltitudeBaro = gps->altitude_baro;
    d->Location.SpeedHorizontal = gps->speed;
    d->Location.Direction = gps->heading;
    d->Location.SpeedVertical = gps->speed_vertical;
    d->Location.HorizAccuracy = horiz_acc(gps->fix_type, gps->satellites);
    d->Location.VertAccuracy = vert_acc(gps->fix_type, gps->satellites);

    d->SystemValid = 1;
    d->System.OperatorLatitude = gps->operator_lat;
    d->System.OperatorLongitude = gps->operator_lon;
    d->System.OperatorAltitudeGeo = gps->operator_alt;
    d->System.AreaCount = 0;
    d->System.AreaRadius = 0;

    if (identity->self_id_text[0] != '\0') {
        d->SelfIDValid = 1;
        d->SelfID.DescType = identity->has_self_id
                                 ? (ODID_desctype_t)identity->self_id_desc_type
                                 : ODID_DESC_TYPE_TEXT;
        strncpy((char *)d->SelfID.Desc, identity->self_id_text, ODID_STR_SIZE);
    }

    d->OperatorIDValid = 1;
    strncpy((char *)d->OperatorID.OperatorId, identity->operator_id, ODID_ID_SIZE);

    /* Authentication: MAVLink-relayed pages take priority, otherwise sign locally */
    uint8_t auth_pages = 0;
    ODID_Auth_data auth[ODID_AUTH_MAX_PAGES];

    if (identity->has_ext_auth && identity->ext_auth_last_page < ODID_AUTH_MAX_PAGES) {
        uint16_t last = identity->ext_auth_last_page;
        uint16_t need = (uint16_t)((1u << (last + 1)) - 1);
        if ((identity->ext_auth_pages_received & need) == need) {
            for (uint16_t p = 0; p <= last; p++) {
                memset(&auth[p], 0, sizeof(ODID_Auth_data));
                auth[p].DataPage = (uint8_t)p;
                auth[p].AuthType = (ODID_authtype_t)identity->ext_auth_type;
                auth[p].LastPageIndex = identity->ext_auth_last_page;
                auth[p].Length = identity->ext_auth_length;
                memcpy(auth[p].AuthData, identity->ext_auth_pages[p],
                       ODID_AUTH_PAGE_NONZERO_DATA_SIZE);
            }
            auth_pages = (uint8_t)(last + 1);
        }
    } else if (rid_auth_enabled()) {
        (void)rid_auth_sign_identity(identity->uas_id, auth, &auth_pages);
    }

    if (auth_pages > 0) {
        uint8_t fixed = 1 + (identity->uas_id_2[0] ? 1 : 0) + 1 +
                        (identity->self_id_text[0] ? 1 : 0) + 1 + 1;
        if (auth_pages <= ODID_PACK_MAX_MESSAGES - fixed) {
            for (uint8_t p = 0; p < auth_pages; p++) {
                d->Auth[p] = auth[p];
                d->AuthValid[p] = 1;
            }
        }
    }
}
