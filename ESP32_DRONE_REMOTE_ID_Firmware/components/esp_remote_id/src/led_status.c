#include "led_status.h"
#include "driver/ledc.h"
#include "esp_log.h"
#include "esp_timer.h"

#define TAG "LED"

#define LEDC_MODE    LEDC_LOW_SPEED_MODE
#define LEDC_TIMER   LEDC_TIMER_0
#define LEDC_FREQ_HZ 5000
#define LEDC_RES     LEDC_TIMER_8_BIT

#define TX_FLASH_US  80000

/* ---------- color presets (RGB 0-255) ---------- */
struct rgb { uint8_t r, g, b; };

/* ---------- state machine ---------- */
typedef enum {
    PAT_SOLID,
    PAT_BLINK_1HZ,
    PAT_BLINK_4HZ,
    PAT_BLINK_DOUBLE,
    PAT_PULSE,
    PAT_RAINBOW,
} pattern_t;

static const struct state_entry {
    struct rgb   color;
    pattern_t    pat;
    const char  *name;
} state_table[] = {
    [RID_LED_BOOT]   = { {  40,  80, 255 }, PAT_PULSE,       "BOOT"    },
    [RID_LED_NO_GPS] = { { 255, 200,   0 }, PAT_BLINK_1HZ,   "NO_GPS"  },
    [RID_LED_GPS_OK] = { {   0, 255,   0 }, PAT_SOLID,       "GPS_OK"  },
    [RID_LED_DEMO]   = { { 180,  40, 255 }, PAT_PULSE,       "DEMO"    },
    [RID_LED_LOCKED] = { { 255,   0,   0 }, PAT_BLINK_DOUBLE,"LOCKED"  },
    [RID_LED_OTA]    = { { 255, 255, 255 }, PAT_RAINBOW,     "OTA"     },
    [RID_LED_ERROR]  = { { 255,   0,   0 }, PAT_BLINK_4HZ,   "ERROR"   },
};

static int r_pin = RID_LED_PIN_NC;
static int g_pin = RID_LED_PIN_NC;
static int b_pin = RID_LED_PIN_NC;
static bool initialized = false;

static rid_led_state_t current_state = RID_LED_BOOT;
static int64_t last_tx_flash_us = 0;
static int64_t tick_count = 0;

/* ---------- LEDC helpers ---------- */
static void set_rgb(uint8_t r, uint8_t g, uint8_t b)
{
    if (r_pin >= 0) {
        ledc_set_duty(LEDC_MODE, LEDC_CHANNEL_0, r);
        ledc_update_duty(LEDC_MODE, LEDC_CHANNEL_0);
    }
    if (g_pin >= 0) {
        ledc_set_duty(LEDC_MODE, LEDC_CHANNEL_1, g);
        ledc_update_duty(LEDC_MODE, LEDC_CHANNEL_1);
    }
    if (b_pin >= 0) {
        ledc_set_duty(LEDC_MODE, LEDC_CHANNEL_2, b);
        ledc_update_duty(LEDC_MODE, LEDC_CHANNEL_2);
    }
}

static void config_channel(int pin, ledc_channel_t chan)
{
    if (pin < 0) return;
    ledc_channel_config_t ch = {
        .gpio_num   = pin,
        .speed_mode = LEDC_MODE,
        .channel    = chan,
        .timer_sel  = LEDC_TIMER,
        .duty       = 0,
        .hpoint     = 0,
    };
    ledc_channel_config(&ch);
}

/* ---------- pattern generators ---------- */
static struct rgb solid(struct rgb c) { return c; }

static struct rgb blink_1hz(struct rgb c)
{
    uint32_t phase = (uint32_t)((tick_count * 100) % 1000);
    return (phase < 500) ? c : (struct rgb){0,0,0};
}

static struct rgb blink_4hz(struct rgb c)
{
    uint32_t phase = (uint32_t)((tick_count * 100) % 250);
    return (phase < 125) ? c : (struct rgb){0,0,0};
}

static struct rgb blink_double(struct rgb c)
{
    uint32_t phase = (uint32_t)((tick_count * 100) % 1400);
    if (phase < 200) return c;
    if (phase < 400) return (struct rgb){0,0,0};
    if (phase < 600) return c;
    return (struct rgb){0,0,0};
}

static struct rgb pulse(struct rgb c)
{
    uint32_t ms = (uint32_t)(esp_timer_get_time() / 1000);
    uint32_t phase = ms % 2000;
    uint32_t half = phase < 1000 ? phase : (2000 - phase);
    uint8_t bright = (uint8_t)((uint32_t)half * 255 / 1000);
    uint8_t r = (uint16_t)c.r * bright / 255;
    uint8_t g = (uint16_t)c.g * bright / 255;
    uint8_t b = (uint16_t)c.b * bright / 255;
    return (struct rgb){r, g, b};
}

static struct rgb rainbow(void)
{
    uint32_t ms = (uint32_t)(esp_timer_get_time() / 1000);
    uint32_t phase = (ms / 20) % 256;
    uint8_t r, g, bl;
    if (phase < 85) {
        r = phase * 3; g = 255 - phase * 3; bl = 0;
    } else if (phase < 170) {
        phase -= 85;
        r = 255 - phase * 3; g = 0; bl = phase * 3;
    } else {
        phase -= 170;
        r = 0; g = phase * 3; bl = 255 - phase * 3;
    }
    return (struct rgb){r, g, bl};
}

/* ---------- public API ---------- */
void led_status_init(void)
{
    if (r_pin < 0 && g_pin < 0 && b_pin < 0) {
        ESP_LOGI(TAG, "No LED pins configured");
        initialized = true;
        return;
    }

    if (!initialized) {
        ledc_timer_config_t timer = {
            .speed_mode      = LEDC_MODE,
            .duty_resolution = LEDC_RES,
            .timer_num       = LEDC_TIMER,
            .freq_hz         = LEDC_FREQ_HZ,
        };
        ledc_timer_config(&timer);

        config_channel(r_pin, LEDC_CHANNEL_0);
        config_channel(g_pin, LEDC_CHANNEL_1);
        config_channel(b_pin, LEDC_CHANNEL_2);
        initialized = true;
    }

    set_rgb(0, 0, 0);
    current_state = RID_LED_BOOT;
    ESP_LOGI(TAG, "LED PWM init R=%d G=%d B=%d", r_pin, g_pin, b_pin);
}

void led_status_reconfigure(int r, int g, int b)
{
    r_pin = r;
    g_pin = g;
    b_pin = b;
    initialized = false;
    led_status_init();
}

void led_status_set_state(rid_led_state_t state)
{
    if (state < sizeof(state_table) / sizeof(state_table[0])) {
        current_state = state;
    }
}

void led_status_tx_flash(void)
{
    last_tx_flash_us = esp_timer_get_time();
}

void led_status_tick(void)
{
    if (!initialized) return;
    if (r_pin < 0 && g_pin < 0 && b_pin < 0) return;

    tick_count++;

    /* TX flash overrides everything */
    if (last_tx_flash_us > 0 && esp_timer_get_time() - last_tx_flash_us < TX_FLASH_US) {
        set_rgb(255, 255, 255);
        return;
    }

    const struct state_entry *st = &state_table[current_state];

    struct rgb out = {0,0,0};
    switch (st->pat) {
    case PAT_SOLID:       out = solid(st->color); break;
    case PAT_BLINK_1HZ:   out = blink_1hz(st->color); break;
    case PAT_BLINK_4HZ:   out = blink_4hz(st->color); break;
    case PAT_BLINK_DOUBLE:out = blink_double(st->color); break;
    case PAT_PULSE:       out = pulse(st->color); break;
    case PAT_RAINBOW:     out = rainbow(); break;
    }

    set_rgb(out.r, out.g, out.b);
}
