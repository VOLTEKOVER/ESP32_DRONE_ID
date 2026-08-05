#ifndef RID_MAVLINK_TX_H
#define RID_MAVLINK_TX_H

#include <stdbool.h>
#include <stdint.h>

void rid_mavlink_tx_init(uint8_t uart_port);
void rid_mavlink_tx_task(void *pvParameters);

#endif
