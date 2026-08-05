#include <string.h>
#include "esp_log.h"
#include "driver/uart.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "esp_timer.h"
#include "rid_mavlink_tx.h"
#include "rid_mavlink_usb.h"
#include "mavlink_parser.h"
#include "ardupilotmega/mavlink.h"

#define TAG "RID_MAVLINK_TX"
#define TX_SYSID 0x41
#define TX_COMPID 0x38

static uint8_t g_uart_port = UART_NUM_1;

void rid_mavlink_tx_init(uint8_t uart_port)
{
    g_uart_port = uart_port;
}

static void send_mavlink_message(mavlink_message_t *msg)
{
    uint8_t buf[MAVLINK_MAX_PACKET_LEN];
    int len = mavlink_msg_to_send_buffer(buf, msg);
    if (len > 0) {
        uart_write_bytes((uart_port_t)g_uart_port, buf, len);
        rid_mavlink_usb_write(buf, (size_t)len);
    }
}

void rid_mavlink_tx_task(void *pvParameters)
{
    (void)pvParameters;
    int64_t last_heartbeat = 0;
    int64_t last_op_loc = 0;

    while (1) {
        int64_t now = esp_timer_get_time();

        if (now - last_heartbeat >= 1000000) {
            last_heartbeat = now;
            mavlink_message_t msg;
            mavlink_msg_heartbeat_pack(TX_SYSID, TX_COMPID, &msg,
                MAV_TYPE_ODID, MAV_AUTOPILOT_INVALID,
                MAV_MODE_FLAG_CUSTOM_MODE_ENABLED, 0, MAV_STATE_ACTIVE);
            send_mavlink_message(&msg);
        }

        /* Operator-location freshness: republish SYSTEM msg every 6s */
        if (now - last_op_loc >= 6000000) {
            last_op_loc = now;
            mavlink_message_t msg;
            uint8_t id_or_mac[20] = {0};

            double op_lat = 0.0, op_lon = 0.0;
            float op_alt = -1000.0f;
            uint8_t op_loc_type = 0;
            if (mavlink_parser_get_operator_location(&op_lat, &op_lon, &op_alt)) {
                op_loc_type = 1; /* MAV_ODID_OPERATOR_LOCATION_TYPE_FIXED */
            }

            mavlink_msg_open_drone_id_system_pack(TX_SYSID, TX_COMPID, &msg,
                0, 0, id_or_mac, op_loc_type, 0,
                (int32_t)(op_lat * 1e7), (int32_t)(op_lon * 1e7),
                1, 0, -1000.0f, -1000.0f, 0, 0, op_alt, 0);
            send_mavlink_message(&msg);
        }

        vTaskDelay(pdMS_TO_TICKS(100));
    }
}
