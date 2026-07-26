#include <string.h>
#include "esp_log.h"
#include "driver/gpio.h"
#include "esp_timer.h"
#include "rid_lighting.h"

#define TAG "LIGHTING"

typedef enum {
    LIGHT_OFF = 0,
    LIGHT_SOLID,
    LIGHT_BLINK_SLOW,
    LIGHT_BLINK_FAST,
    LIGHT_BLINK_ARMED,
    LIGHT_FLASH_ON_GPS,
} lighting_pattern_t;

typedef struct {
    bool active;
    int8_t gpio;
    lighting_pattern_t pattern;
    int16_t phase_offset_ms;
} lighting_channel_t;

static lighting_channel_t g_channels[RID_LIGHTING_MAX_OUTPUTS];
static bool g_armed = false;
static bool g_gps_valid = false;
static int g_num_channels = 0;
static bool g_initialized = false;

static bool pattern_active(lighting_pattern_t pat, uint32_t now_ms)
{
    uint32_t phase = now_ms % 2000;

    switch (pat) {
    case LIGHT_OFF:
        return false;
    case LIGHT_SOLID:
        return true;
    case LIGHT_BLINK_SLOW:
        return (phase % 2000) < 1000;
    case LIGHT_BLINK_FAST:
        return (phase % 500) < 250;
    case LIGHT_BLINK_ARMED:
        return g_armed && ((phase % 1000) < 500);
    case LIGHT_FLASH_ON_GPS:
        return g_gps_valid;
    default:
        return false;
    }
}

void rid_lighting_init(const int8_t pins[RID_LIGHTING_MAX_OUTPUTS],
                       const uint8_t patterns[RID_LIGHTING_MAX_OUTPUTS],
                       const int16_t phase_offsets[RID_LIGHTING_MAX_OUTPUTS])
{
    g_num_channels = 0;

    for (int i = 0; i < RID_LIGHTING_MAX_OUTPUTS; i++) {
        if (pins[i] < 0) continue;

        g_channels[g_num_channels].gpio = pins[i];
        g_channels[g_num_channels].pattern = (lighting_pattern_t)patterns[i];
        g_channels[g_num_channels].phase_offset_ms = phase_offsets[i];
        g_channels[g_num_channels].active = false;

        gpio_config_t io_conf = {
            .pin_bit_mask = (1ULL << pins[i]),
            .mode = GPIO_MODE_OUTPUT,
            .pull_down_en = false,
            .pull_up_en = false,
            .intr_type = GPIO_INTR_DISABLE,
        };
        gpio_config(&io_conf);
        gpio_set_level(pins[i], 0);

        g_num_channels++;
    }

    g_initialized = true;
    ESP_LOGI(TAG, "Lighting init: %d channels", g_num_channels);
}

void rid_lighting_tick(void)
{
    if (!g_initialized || g_num_channels == 0) return;

    uint32_t now_ms = (uint32_t)(esp_timer_get_time() / 1000);

    for (int i = 0; i < g_num_channels; i++) {
        uint32_t offset_ms = now_ms + g_channels[i].phase_offset_ms;
        bool on = pattern_active(g_channels[i].pattern, offset_ms);
        gpio_set_level(g_channels[i].gpio, on ? 1 : 0);
    }
}

void rid_lighting_set_state(bool armed, bool gps_valid)
{
    g_armed = armed;
    g_gps_valid = gps_valid;
}
