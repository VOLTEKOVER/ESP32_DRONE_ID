#include <stdio.h>
#include <string.h>
#include <math.h>
#include "esp_log.h"
#include "driver/uart.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "mavlink_parser.h"
#include "ardupilotmega/mavlink.h"
#include "mav2odid.h"

#define TAG "MAVLINK"
#define MAV_RX_BUF 512

static rid_gps_data_t g_last_gps;
static rid_identity_t g_last_identity;
static uint32_t g_last_update = 0;
static uint32_t g_last_identity_update = 0;
static mavlink_status_t g_mav_status;
static uint8_t g_mav_buf[MAV_RX_BUF];
static uint8_t g_sysid_filter = 0;
static uint8_t g_uart_port = UART_NUM_1;

/* Extended state */
static bool g_has_armed = false;
static bool g_armed = false;
static uint8_t g_mav_sysid = 0;
static double g_operator_lat = 0.0;
static double g_operator_lon = 0.0;
static float g_operator_alt = 0.0f;
static uint32_t g_operator_location_update = 0;

void mavlink_parser_init(uint8_t uart_port)
{
    g_uart_port = uart_port;
    memset(&g_last_gps, 0, sizeof(rid_gps_data_t));
    memset(&g_last_identity, 0, sizeof(rid_identity_t));
    memset(&g_mav_status, 0, sizeof(g_mav_status));
    g_has_armed = false;
    g_armed = false;
    g_mav_sysid = 0;
    g_operator_lat = 0.0;
    g_operator_lon = 0.0;
    g_operator_alt = 0.0f;
    g_operator_location_update = 0;
}

void mavlink_parser_set_sysid_filter(uint8_t sysid)
{
    g_sysid_filter = sysid;
}

bool mavlink_parser_get_sysid(uint8_t *sysid)
{
    if (g_mav_sysid != 0) {
        *sysid = g_mav_sysid;
        return true;
    }
    return false;
}

bool mavlink_parser_get_armed(bool *armed)
{
    if (g_has_armed) {
        *armed = g_armed;
        return true;
    }
    return false;
}

bool mavlink_parser_get_operator_location(double *lat, double *lon, float *alt)
{
    if (g_operator_location_update != 0 &&
        (xTaskGetTickCount() * portTICK_PERIOD_MS - g_operator_location_update) < 30000) {
        *lat = g_operator_lat;
        *lon = g_operator_lon;
        *alt = g_operator_alt;
        return true;
    }
    return false;
}

void mavlink_parser_set_operator_location(double lat, double lon, float alt)
{
    g_operator_lat = lat;
    g_operator_lon = lon;
    g_operator_alt = alt;
    g_operator_location_update = xTaskGetTickCount() * portTICK_PERIOD_MS;
}

bool mavlink_parser_get(rid_gps_data_t *gps)
{
    int len = uart_read_bytes((uart_port_t)g_uart_port, g_mav_buf, sizeof(g_mav_buf), 0);
    if (len > 0) {
        mavlink_message_t msg;
        mavlink_status_t status;

        for (int i = 0; i < len; i++) {
            if (mavlink_parse_char(MAVLINK_COMM_0, g_mav_buf[i], &msg, &status)) {
                if (g_sysid_filter != 0 && msg.sysid != g_sysid_filter) continue;

                g_mav_sysid = msg.sysid;

                switch (msg.msgid) {
                case MAVLINK_MSG_ID_GLOBAL_POSITION_INT: {
                    mavlink_global_position_int_t pos;
                    mavlink_msg_global_position_int_decode(&msg, &pos);
                    g_last_gps.latitude = pos.lat / 1e7;
                    g_last_gps.longitude = pos.lon / 1e7;
                    g_last_gps.altitude_msl = pos.alt / 1000.0f;
                    g_last_gps.altitude_relative = pos.relative_alt / 1000.0f;
                    g_last_gps.heading = pos.hdg / 100;
                    g_last_gps.fix_type = 3;
                    float vx = pos.vx / 100.0f;
                    float vy = pos.vy / 100.0f;
                    g_last_gps.speed = sqrtf(vx * vx + vy * vy);
                    g_last_gps.speed_vertical = -pos.vz / 100.0f;
                    break;
                }
                case MAVLINK_MSG_ID_GPS_RAW_INT: {
                    mavlink_gps_raw_int_t raw;
                    mavlink_msg_gps_raw_int_decode(&msg, &raw);
                    g_last_gps.latitude = raw.lat / 1e7;
                    g_last_gps.longitude = raw.lon / 1e7;
                    g_last_gps.altitude_msl = raw.alt / 1000.0f;
                    g_last_gps.fix_type = raw.fix_type;
                    g_last_gps.satellites = raw.satellites_visible;
                    g_last_gps.speed = raw.vel / 100.0f;
                    g_last_gps.heading = raw.cog / 100;
                    break;
                }
                case MAVLINK_MSG_ID_VFR_HUD: {
                    mavlink_vfr_hud_t hud;
                    mavlink_msg_vfr_hud_decode(&msg, &hud);
                    g_last_gps.speed = hud.groundspeed;
                    g_last_gps.heading = hud.heading;
                    g_last_gps.altitude_msl = hud.alt;
                    break;
                }
                case MAVLINK_MSG_ID_ATTITUDE: {
                    mavlink_attitude_t att;
                    mavlink_msg_attitude_decode(&msg, &att);
                    g_last_gps.heading = (int16_t)(att.yaw * 180.0f / 3.14159f);
                    if (g_last_gps.heading < 0) g_last_gps.heading += 360;
                    break;
                }
                case MAVLINK_MSG_ID_AHRS2: {
                    mavlink_ahrs2_t ahrs;
                    mavlink_msg_ahrs2_decode(&msg, &ahrs);
                    g_last_gps.heading = (int16_t)(ahrs.yaw * 180.0f / 3.14159f);
                    if (g_last_gps.heading < 0) g_last_gps.heading += 360;
                    break;
                }
                case MAVLINK_MSG_ID_HEARTBEAT: {
                    mavlink_heartbeat_t hb;
                    mavlink_msg_heartbeat_decode(&msg, &hb);
                    g_armed = (hb.base_mode & MAV_MODE_FLAG_SAFETY_ARMED) != 0;
                    g_has_armed = true;
                    g_last_gps.armed = g_armed;
                    break;
                }
                case MAVLINK_MSG_ID_OPEN_DRONE_ID_LOCATION: {
                    mavlink_open_drone_id_location_t odid_loc;
                    mavlink_msg_open_drone_id_location_decode(&msg, &odid_loc);
                    g_last_gps.latitude = odid_loc.latitude / 1e7;
                    g_last_gps.longitude = odid_loc.longitude / 1e7;
                    g_last_gps.altitude_msl = odid_loc.altitude_geodetic;
                    g_last_gps.altitude_relative = odid_loc.height;
                    g_last_gps.altitude_baro = odid_loc.altitude_barometric;
                    g_last_gps.speed = odid_loc.speed_horizontal / 100.0f;
                    g_last_gps.speed_vertical = odid_loc.speed_vertical / 100.0f;
                    g_last_gps.heading = odid_loc.direction / 100;
                    g_last_gps.fix_type = 3;
                    break;
                }
                case MAVLINK_MSG_ID_OPEN_DRONE_ID_BASIC_ID: {
                    mavlink_open_drone_id_basic_id_t odid_basic;
                    mavlink_msg_open_drone_id_basic_id_decode(&msg, &odid_basic);
                    int id_len = ODID_ID_SIZE;
                    if (id_len > ESP_RID_MAX_STR_LEN) id_len = ESP_RID_MAX_STR_LEN;
                    memcpy(g_last_identity.uas_id, odid_basic.uas_id, id_len);
                    g_last_identity.uas_id[id_len] = '\0';
                    g_last_identity.id_type = odid_basic.id_type;
                    g_last_identity.ua_type = odid_basic.ua_type;
                    g_last_identity_update = xTaskGetTickCount() * portTICK_PERIOD_MS;
                    break;
                }
                case MAVLINK_MSG_ID_OPEN_DRONE_ID_OPERATOR_ID: {
                    mavlink_open_drone_id_operator_id_t odid_op;
                    mavlink_msg_open_drone_id_operator_id_decode(&msg, &odid_op);
                    int op_len = ODID_ID_SIZE;
                    if (op_len > ESP_RID_MAX_STR_LEN) op_len = ESP_RID_MAX_STR_LEN;
                    memcpy(g_last_identity.operator_id, odid_op.operator_id, op_len);
                    g_last_identity.operator_id[op_len] = '\0';
                    g_last_identity_update = xTaskGetTickCount() * portTICK_PERIOD_MS;
                    break;
                }

                case MAVLINK_MSG_ID_OPEN_DRONE_ID_SELF_ID: {
                    mavlink_open_drone_id_self_id_t odid_self;
                    mavlink_msg_open_drone_id_self_id_decode(&msg, &odid_self);
                    int text_len = ODID_ID_SIZE;
                    if (text_len > ESP_RID_MAX_STR_LEN) text_len = ESP_RID_MAX_STR_LEN;
                    memcpy(g_last_identity.self_id_text, odid_self.description, text_len);
                    g_last_identity.self_id_text[text_len] = '\0';
                    g_last_identity.self_id_desc_type = odid_self.description_type;
                    g_last_identity.has_self_id = true;
                    g_last_identity_update = xTaskGetTickCount() * portTICK_PERIOD_MS;
                    break;
                }
                case MAVLINK_MSG_ID_OPEN_DRONE_ID_AUTHENTICATION: {
                    mavlink_open_drone_id_authentication_t odid_auth;
                    mavlink_msg_open_drone_id_authentication_decode(&msg, &odid_auth);
                    if (odid_auth.data_page < ODID_AUTH_MAX_PAGES) {
                        memcpy(g_last_identity.ext_auth_pages[odid_auth.data_page],
                               odid_auth.authentication_data, sizeof(odid_auth.authentication_data));
                        g_last_identity.ext_auth_last_page = odid_auth.last_page_index;
                        g_last_identity.ext_auth_type = odid_auth.authentication_type;
                        g_last_identity.ext_auth_length = odid_auth.length;
                        g_last_identity.ext_auth_pages_received |= (1 << odid_auth.data_page);
                        g_last_identity.has_ext_auth = true;
                    }
                    g_last_identity_update = xTaskGetTickCount() * portTICK_PERIOD_MS;
                    break;
                }
                case MAVLINK_MSG_ID_OPEN_DRONE_ID_SYSTEM: {
                    mavlink_open_drone_id_system_t odid_sys;
                    mavlink_msg_open_drone_id_system_decode(&msg, &odid_sys);
                    /* This message carries the operator location, not the UA
                     * position: do NOT overwrite g_last_gps.latitude/longitude. */
                    g_operator_lat = odid_sys.operator_latitude / 1e7;
                    g_operator_lon = odid_sys.operator_longitude / 1e7;
                    g_operator_alt = odid_sys.operator_altitude_geo;
                    g_operator_location_update = xTaskGetTickCount() * portTICK_PERIOD_MS;
                    break;
                }
                case MAVLINK_MSG_ID_OPEN_DRONE_ID_MESSAGE_PACK: {
                    mavlink_open_drone_id_message_pack_t pack;
                    mavlink_msg_open_drone_id_message_pack_decode(&msg, &pack);
                    if (pack.single_message_size == 0 || pack.single_message_size != ODID_MESSAGE_SIZE) break;
                    if (pack.msg_pack_size == 0 || pack.msg_pack_size > ODID_PACK_MAX_MESSAGES) break;

                    ODID_MessagePack_encoded encoded;
                    memset(&encoded, 0, sizeof(encoded));
                    encoded.SingleMessageSize = pack.single_message_size;
                    encoded.MsgPackSize = pack.msg_pack_size;
                    memcpy(encoded.Messages, pack.messages,
                           pack.msg_pack_size * ODID_MESSAGE_SIZE);

                    ODID_UAS_Data uas;
                    memset(&uas, 0, sizeof(uas));
                    if (decodeMessagePack(&uas, &encoded) != 0) break;

                    for (int p = 0; p < pack.msg_pack_size && p < ODID_PACK_MAX_MESSAGES; p++) {
                        ODID_Message_encoded *m = &encoded.Messages[p];
                        switch (m->rawData[0]) {
                        case 0: {
                            ODID_BasicID_data basic;
                            decodeBasicIDMessage(&basic, &m->basicId);
                            int id_len = strlen(basic.UASID);
                            if (id_len > ESP_RID_MAX_STR_LEN) id_len = ESP_RID_MAX_STR_LEN;
                            memcpy(g_last_identity.uas_id, basic.UASID, id_len);
                            g_last_identity.uas_id[id_len] = '\0';
                            g_last_identity.id_type = basic.IDType;
                            g_last_identity.ua_type = basic.UAType;
                            g_last_identity_update = xTaskGetTickCount() * portTICK_PERIOD_MS;
                            break;
                        }
                        case 1: {
                            ODID_Location_data loc;
                            decodeLocationMessage(&loc, &m->location);
                            if (loc.Latitude != 0 || loc.Longitude != 0) {
                                g_last_gps.latitude = loc.Latitude;
                                g_last_gps.longitude = loc.Longitude;
                                g_last_gps.altitude_msl = loc.AltitudeGeo;
                                g_last_gps.altitude_relative = loc.Height;
                                g_last_gps.altitude_baro = loc.AltitudeBaro;
                                g_last_gps.speed = loc.SpeedHorizontal;
                                g_last_gps.speed_vertical = loc.SpeedVertical;
                                g_last_gps.heading = (int16_t)loc.Direction;
                                g_last_gps.fix_type = 3;
                                g_last_update = xTaskGetTickCount() * portTICK_PERIOD_MS;
                            }
                            break;
                        }
                        case 2: {
                            ODID_Auth_data auth;
                            memset(&auth, 0, sizeof(auth));
                            decodeAuthMessage(&auth, &m->auth);
                            if (auth.DataPage < ODID_AUTH_MAX_PAGES) {
                                memcpy(g_last_identity.ext_auth_pages[auth.DataPage],
                                       auth.AuthData, ODID_AUTH_PAGE_NONZERO_DATA_SIZE);
                                if (auth.DataPage == 0) {
                                    g_last_identity.ext_auth_last_page = auth.LastPageIndex;
                                    g_last_identity.ext_auth_length = auth.Length;
                                }
                                g_last_identity.ext_auth_type = auth.AuthType;
                                g_last_identity.ext_auth_pages_received |= (1 << auth.DataPage);
                                g_last_identity.has_ext_auth = true;
                                g_last_identity_update = xTaskGetTickCount() * portTICK_PERIOD_MS;
                            }
                            break;
                        }
                        case 3: {
                            ODID_SelfID_data selfid;
                            decodeSelfIDMessage(&selfid, &m->selfId);
                            int text_len = strlen(selfid.Desc);
                            if (text_len > ESP_RID_MAX_STR_LEN) text_len = ESP_RID_MAX_STR_LEN;
                            memcpy(g_last_identity.self_id_text, selfid.Desc, text_len);
                            g_last_identity.self_id_text[text_len] = '\0';
                            g_last_identity.self_id_desc_type = selfid.DescType;
                            g_last_identity.has_self_id = true;
                            g_last_identity_update = xTaskGetTickCount() * portTICK_PERIOD_MS;
                            break;
                        }
                        case 4: {
                            ODID_System_data sys;
                            decodeSystemMessage(&sys, &m->system);
                            if (sys.OperatorLatitude != 0 || sys.OperatorLongitude != 0) {
                                g_operator_lat = sys.OperatorLatitude;
                                g_operator_lon = sys.OperatorLongitude;
                                g_operator_alt = sys.OperatorAltitudeGeo;
                                g_operator_location_update = xTaskGetTickCount() * portTICK_PERIOD_MS;
                            }
                            break;
                        }
                        case 5: {
                            ODID_OperatorID_data op;
                            decodeOperatorIDMessage(&op, &m->operatorId);
                            int op_len = strlen(op.OperatorId);
                            if (op_len > ESP_RID_MAX_STR_LEN) op_len = ESP_RID_MAX_STR_LEN;
                            memcpy(g_last_identity.operator_id, op.OperatorId, op_len);
                            g_last_identity.operator_id[op_len] = '\0';
                            g_last_identity_update = xTaskGetTickCount() * portTICK_PERIOD_MS;
                            break;
                        }
                        default:
                            break;
                        }
                    }
                    break;
                }
                default:
                    break;
                }

                if (g_last_gps.latitude != 0.0 || g_last_gps.longitude != 0.0) {
                    g_last_update = xTaskGetTickCount() * portTICK_PERIOD_MS;
                }
            }
        }
    }

    if (g_last_update != 0 && (xTaskGetTickCount() * portTICK_PERIOD_MS - g_last_update) < 5000) {
        memcpy(gps, &g_last_gps, sizeof(rid_gps_data_t));
        return true;
    }

    return false;
}

bool mavlink_parser_get_identity(rid_identity_t *identity)
{
    if (g_last_identity_update != 0 &&
        (xTaskGetTickCount() * portTICK_PERIOD_MS - g_last_identity_update) < 10000) {
        memcpy(identity, &g_last_identity, sizeof(rid_identity_t));
        return true;
    }
    return false;
}
