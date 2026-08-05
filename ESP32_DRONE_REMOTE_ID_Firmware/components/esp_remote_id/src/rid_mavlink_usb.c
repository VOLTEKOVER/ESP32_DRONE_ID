#include <string.h>
#include "esp_log.h"
#include "driver/uart.h"
#include "rid_mavlink_usb.h"

#define TAG "MAVLINK_USB"

#define USB_UART_PORT CONFIG_ESP_CONSOLE_UART_NUM

static bool g_usb_ready = false;

bool rid_mavlink_usb_init(void)
{
    uart_config_t uart_cfg = {
        .baud_rate = 115200,
        .data_bits = UART_DATA_8_BITS,
        .parity = UART_PARITY_DISABLE,
        .stop_bits = UART_STOP_BITS_1,
        .flow_ctrl = UART_HW_FLOWCTRL_DISABLE,
        .source_clk = UART_SCLK_DEFAULT,
    };

    esp_err_t err = uart_driver_install(USB_UART_PORT, 256, 0, 0, NULL, 0);
    if (err != ESP_OK) {
        ESP_LOGW(TAG, "USB UART install failed: %s", esp_err_to_name(err));
        return false;
    }

    err = uart_param_config(USB_UART_PORT, &uart_cfg);
    if (err != ESP_OK) {
        ESP_LOGW(TAG, "USB UART param config failed: %s", esp_err_to_name(err));
        return false;
    }

    err = uart_set_pin(USB_UART_PORT, UART_PIN_NO_CHANGE, UART_PIN_NO_CHANGE,
                       UART_PIN_NO_CHANGE, UART_PIN_NO_CHANGE);
    if (err != ESP_OK) {
        ESP_LOGW(TAG, "USB UART set pin failed: %s", esp_err_to_name(err));
        return false;
    }

    g_usb_ready = true;
    ESP_LOGI(TAG, "USB Serial/JTAG MAVLink transport on UART%d", USB_UART_PORT);
    return true;
}

bool rid_mavlink_usb_write(const uint8_t *buf, size_t len)
{
    if (!g_usb_ready || !buf || len == 0) return false;
    return uart_write_bytes((uart_port_t)USB_UART_PORT, buf, len) >= 0;
}
