#ifndef MAVLINK_PARSER_H
#define MAVLINK_PARSER_H

#include "esp_remote_id.h"

void mavlink_parser_init(uint8_t uart_port);
bool mavlink_parser_get(rid_gps_data_t *gps);
bool mavlink_parser_get_identity(rid_identity_t *identity);
void mavlink_parser_set_sysid_filter(uint8_t sysid);

/* Expanded API */
bool mavlink_parser_get_sysid(uint8_t *sysid);
bool mavlink_parser_get_armed(bool *armed);
bool mavlink_parser_get_operator_location(double *lat, double *lon, float *alt);
void mavlink_parser_set_operator_location(double lat, double lon, float alt);

#endif
