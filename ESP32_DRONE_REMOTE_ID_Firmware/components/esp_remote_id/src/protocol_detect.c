#include <stdio.h>
#include <string.h>
#include "esp_log.h"
#include "driver/uart.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "protocol_detect.h"

#define TAG "PROTO_DETECT"
#define PROTO_BUF_SIZE 256
#define DETECT_TIMEOUT_MS 1000
#define DETECT_READ_MS 50

static uint8_t g_uart_port = UART_NUM_1;
static uint8_t g_tx_pin = 17;
static uint8_t g_rx_pin = 18;

static void uart_configure(uint8_t uart_port, uint8_t tx_pin, uint8_t rx_pin, uint32_t baud)
{
    uart_config_t uart_cfg = {
        .baud_rate = (int)baud,
        .data_bits = UART_DATA_8_BITS,
        .parity = UART_PARITY_DISABLE,
        .stop_bits = UART_STOP_BITS_1,
        .flow_ctrl = UART_HW_FLOWCTRL_DISABLE,
        .source_clk = UART_SCLK_DEFAULT,
    };

    uart_param_config((uart_port_t)uart_port, &uart_cfg);
    uart_set_pin((uart_port_t)uart_port, tx_pin, rx_pin, UART_PIN_NO_CHANGE, UART_PIN_NO_CHANGE);
    uart_driver_install((uart_port_t)uart_port, PROTO_BUF_SIZE, 0, 0, NULL, 0);
}

void protocol_detect_init(uint8_t uart_port, uint8_t tx_pin, uint8_t rx_pin, uint32_t baud)
{
    g_uart_port = uart_port;
    g_tx_pin = tx_pin;
    g_rx_pin = rx_pin;
    uart_configure(uart_port, tx_pin, rx_pin, baud);
}

rid_protocol_t protocol_detect_auto(void)
{
    uint8_t buf[PROTO_BUF_SIZE];
    int len = uart_read_bytes((uart_port_t)g_uart_port, buf, PROTO_BUF_SIZE, pdMS_TO_TICKS(DETECT_READ_MS));

    if (len <= 0) {
        return RID_PROTOCOL_UNKNOWN;
    }

    if (len >= 3 && buf[0] == '$' && buf[1] == 'M' && buf[2] == '<') {
        return RID_PROTOCOL_MSP;
    }

    if (len >= 3 && buf[0] == '$' && (buf[1] == 'G' || buf[1] == 'N')) {
        return RID_PROTOCOL_NMEA;
    }

    for (int i = 0; i < len - 1; i++) {
        if (buf[i] == 0xFE || buf[i] == 0xFD) {
            uint8_t msg_len = buf[i + 1];
            if (msg_len > 0 && msg_len < 255 && (i + msg_len + 6) <= len) {
                return RID_PROTOCOL_MAVLINK;
            }
        }
    }

    return RID_PROTOCOL_NMEA;
}

void protocol_detect_reinit(uint8_t uart_port, uint8_t tx_pin, uint8_t rx_pin, uint32_t baud)
{
    ESP_LOGI(TAG, "Reconfiguring UART to %lu baud", (unsigned long)baud);
    uart_driver_delete((uart_port_t)uart_port);
    g_uart_port = uart_port;
    g_tx_pin = tx_pin;
    g_rx_pin = rx_pin;
    uart_configure(uart_port, tx_pin, rx_pin, baud);
    ESP_LOGI(TAG, "UART reconfigured to %lu baud", (unsigned long)baud);
}
