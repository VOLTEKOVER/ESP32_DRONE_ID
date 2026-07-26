#ifndef LED_STATUS_H
#define LED_STATUS_H

#include <stdint.h>
#include <stdbool.h>

#define RID_LED_PIN_NC  -1

typedef enum {
    RID_LED_BOOT,
    RID_LED_NO_GPS,
    RID_LED_GPS_OK,
    RID_LED_DEMO,
    RID_LED_LOCKED,
    RID_LED_OTA,
    RID_LED_ERROR,
} rid_led_state_t;

void led_status_init(void);
void led_status_reconfigure(int r_pin, int g_pin, int b_pin);
void led_status_set_state(rid_led_state_t state);
void led_status_tx_flash(void);
void led_status_tick(void);

#endif
