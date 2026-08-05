#ifndef PROTOCOL_DETECT_H
#define PROTOCOL_DETECT_H

#include "esp_remote_id.h"

void protocol_detect_init(uint8_t uart_port, uint8_t tx_pin, uint8_t rx_pin, uint32_t baud);
void protocol_detect_reinit(uint8_t uart_port, uint8_t tx_pin, uint8_t rx_pin, uint32_t baud);
rid_protocol_t protocol_detect_auto(void);

#endif
