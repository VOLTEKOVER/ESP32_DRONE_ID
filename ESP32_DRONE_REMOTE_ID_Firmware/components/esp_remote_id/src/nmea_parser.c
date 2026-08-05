#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include "esp_log.h"
#include "driver/uart.h"
#include "nmea_parser.h"

#define TAG "NMEA"
#define NMEA_BUF_SIZE 256

static char g_nmea_buf[NMEA_BUF_SIZE];
static int g_buf_idx = 0;
static rid_gps_data_t g_last_gps;
static uint8_t g_uart_port = UART_NUM_1;

void nmea_parser_init(uint8_t uart_port)
{
    g_uart_port = uart_port;
    memset(&g_last_gps, 0, sizeof(rid_gps_data_t));
    memset(g_nmea_buf, 0, NMEA_BUF_SIZE);
}

static double nmea_to_decimal(const char *nmea_str, char dir)
{
    if (!nmea_str || strlen(nmea_str) < 4) return 0.0;

    int dot_pos = -1;
    for (int i = 0; nmea_str[i]; i++) {
        if (nmea_str[i] == '.') { dot_pos = i; break; }
    }
    if (dot_pos < 4) return 0.0;

    int deg_len = dot_pos - 2;
    char deg_str[4] = {0};
    if (deg_len >= (int)sizeof(deg_str)) deg_len = sizeof(deg_str) - 1;
    memcpy(deg_str, nmea_str, deg_len);
    deg_str[deg_len] = '\0';
    double degrees = atoi(deg_str);

    double minutes = atof(nmea_str + deg_len);
    double decimal = degrees + minutes / 60.0;

    if (dir == 'S' || dir == 'W') decimal = -decimal;
    return decimal;
}

static void parse_gga(char *fields[])
{
    if (!fields[2] || !fields[3] || !fields[4] || !fields[5] || !fields[6]) return;

    int fix = atoi(fields[6]);
    if (fix < 1) return;

    g_last_gps.fix_type = (fix >= 2) ? 3 : 1;
    g_last_gps.latitude = nmea_to_decimal(fields[2], fields[3][0]);
    g_last_gps.longitude = nmea_to_decimal(fields[4], fields[5][0]);

    if (fields[7]) g_last_gps.satellites = atoi(fields[7]);
    if (fields[9]) {
        g_last_gps.altitude_msl = (float)atof(fields[9]);
        g_last_gps.altitude_baro = g_last_gps.altitude_msl;
    }
}

static void parse_rmc(char *fields[])
{
    if (!fields[3] || !fields[4] || !fields[5]) return;
    if (fields[2] && fields[2][0] != 'A') return;

    g_last_gps.latitude = nmea_to_decimal(fields[3], fields[4][0]);
    g_last_gps.longitude = nmea_to_decimal(fields[5], fields[6][0]);

    if (fields[7]) {
        g_last_gps.speed = (float)(atof(fields[7]) * 0.514444);
    }
}

static void parse_vtg(char *fields[])
{
    if (fields[1]) {
        g_last_gps.heading = (int16_t)atof(fields[1]);
    }
    if (fields[5]) {
        g_last_gps.speed = (float)(atof(fields[5]) * 0.514444);
    }
}

static void parse_nmea_line(const char *line)
{
    char work[128];
    strncpy(work, line, sizeof(work) - 1);
    work[sizeof(work) - 1] = '\0';

    char *star = strchr(work, '*');
    if (star) *star = '\0';

    char *fields[16];
    int count = 0;
    char *token = strtok(work, ",");
    while (token && count < 16) {
        fields[count++] = token;
        token = strtok(NULL, ",");
    }
    if (count < 2) return;

    if (strcmp(fields[0], "$GPGGA") == 0 || strcmp(fields[0], "$GNGGA") == 0) {
        parse_gga(fields);
    } else if (strcmp(fields[0], "$GPRMC") == 0 || strcmp(fields[0], "$GNRMC") == 0) {
        parse_rmc(fields);
    } else if (strcmp(fields[0], "$GPVTG") == 0 || strcmp(fields[0], "$GNVTG") == 0) {
        parse_vtg(fields);
    }
}

bool nmea_parser_get(rid_gps_data_t *gps)
{
    uint8_t buf[64];
    int len = uart_read_bytes((uart_port_t)g_uart_port, buf, sizeof(buf), 0);
    for (int i = 0; i < len; i++) {
        uint8_t c = buf[i];
        if (c == '\n' || g_buf_idx >= NMEA_BUF_SIZE - 1) {
            if (g_nmea_buf[0] == '$' && g_buf_idx > 5) {
                g_nmea_buf[g_buf_idx] = '\0';
                parse_nmea_line(g_nmea_buf);
            }
            g_buf_idx = 0;
            memset(g_nmea_buf, 0, NMEA_BUF_SIZE);
        } else {
            g_nmea_buf[g_buf_idx++] = c;
        }
    }

    if (g_last_gps.fix_type >= 2 && g_last_gps.latitude != 0.0) {
        memcpy(gps, &g_last_gps, sizeof(rid_gps_data_t));
        return true;
    }
    return false;
}
