#include <stdio.h>
#include <string.h>
#include "esp_log.h"
#include "driver/uart.h"
#include "msp_parser.h"

#define TAG "MSP"
#define MSP_BUF_SIZE 256

#define MSP_RAW_GPS 106
#define MSP_ATTITUDE 108
#define MSP_STATUS 101

static uint8_t g_msp_buf[MSP_BUF_SIZE];
static int g_buf_idx = 0;
static bool g_in_message = false;
static rid_gps_data_t g_last_gps;
static uint8_t g_uart_port = UART_NUM_1;

void msp_parser_init(uint8_t uart_port)
{
    g_uart_port = uart_port;
    memset(&g_last_gps, 0, sizeof(rid_gps_data_t));
}

static uint8_t msp_crc(uint8_t *data, int len)
{
    uint8_t c = 0;
    for (int i = 0; i < len; i++) {
        c ^= data[i];
    }
    return c;
}

static void parse_msp_gps(uint8_t *data, int len)
{
    if (len < 16) return;

    uint8_t fix = data[0];
    uint8_t sat = data[1];
    int32_t lat = (int32_t)(data[2] | (data[3] << 8) | (data[4] << 16) | (data[5] << 24));
    int32_t lon = (int32_t)(data[6] | (data[7] << 8) | (data[8] << 16) | (data[9] << 24));
    int16_t alt = (int16_t)(data[10] | (data[11] << 8));
    int16_t speed = (int16_t)(data[12] | (data[13] << 8));
    int16_t ground_course = (int16_t)(data[14] | (data[15] << 8));

    g_last_gps.fix_type = fix;
    g_last_gps.satellites = sat;
    g_last_gps.latitude = lat / 10000000.0;
    g_last_gps.longitude = lon / 10000000.0;
    g_last_gps.altitude_msl = alt / 10.0f;
    g_last_gps.altitude_baro = g_last_gps.altitude_msl;
    g_last_gps.speed = speed / 100.0f;
    g_last_gps.heading = ground_course / 10;
}

static void parse_msp_attitude(uint8_t *data, int len)
{
    if (len < 6) return;
    int16_t yaw = (int16_t)(data[4] | (data[5] << 8));
    g_last_gps.heading = yaw / 10;
}

static void parse_msp_status(uint8_t *data, int len)
{
    if (len < 10) return;
    uint32_t flag = (uint32_t)(data[6] | (data[7] << 8) | (data[8] << 16) | (data[9] << 24));
    g_last_gps.armed = (flag & 1) != 0;
}

static void parse_msp(uint8_t *buf, int len)
{
    if (len < 6) return;
    if (buf[0] != '$' || buf[1] != 'M' || buf[2] != '<') return;

    uint8_t msp_size = buf[4];
    uint8_t msp_type = buf[5];

    int payload_offset = 6;
    if (payload_offset + msp_size > len - 1) return;

    uint8_t crc_received = buf[len - 1];
    uint8_t crc_calc = msp_crc(&buf[3], buf[4] + 2);
    if (crc_calc != crc_received) {
        return;
    }

    switch (msp_type) {
    case MSP_RAW_GPS:
        parse_msp_gps(buf + payload_offset, msp_size);
        break;
    case MSP_ATTITUDE:
        parse_msp_attitude(buf + payload_offset, msp_size);
        break;
    case MSP_STATUS:
        parse_msp_status(buf + payload_offset, msp_size);
        break;
    }
}

bool msp_parser_get(rid_gps_data_t *gps)
{
    uint8_t buf[64];
    int len = uart_read_bytes((uart_port_t)g_uart_port, buf, sizeof(buf), 0);
    for (int i = 0; i < len; i++) {
        uint8_t c = buf[i];
        if (c == '$') {
            g_buf_idx = 0;
            g_in_message = true;
        }
        if (g_in_message) {
            if (g_buf_idx < MSP_BUF_SIZE) {
                g_msp_buf[g_buf_idx++] = c;
            }
            if (g_buf_idx >= 3 && g_buf_idx >= (6 + g_msp_buf[4] + 1)) {
                parse_msp(g_msp_buf, g_buf_idx);
                g_in_message = false;
                g_buf_idx = 0;
            }
        }
    }

    if (g_last_gps.fix_type >= 2 && g_last_gps.latitude != 0.0) {
        memcpy(gps, &g_last_gps, sizeof(rid_gps_data_t));
        return true;
    }
    return false;
}
