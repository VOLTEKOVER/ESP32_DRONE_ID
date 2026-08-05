#ifndef RID_MAVLINK_USB_H
#define RID_MAVLINK_USB_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

bool rid_mavlink_usb_init(void);
bool rid_mavlink_usb_write(const uint8_t *buf, size_t len);

#endif
