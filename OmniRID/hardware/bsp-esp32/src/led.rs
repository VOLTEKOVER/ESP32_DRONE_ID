//! ESP-IDF LED status output: LEDC PWM for bi-colour RGB LEDs and
//! external GPIO lighting pins.
//!
//! Port of `led_status.c` / `led_ws2812.c` hardware glue.  The pure-logic
//! colour-mapping lives in `rid_app::led_status` / `rid_app::led_ws2812`.

use esp_idf_svc as _;
use esp_idf_svc::sys::{self as sys};

/// LEDC channel config for one pin (R, G, or B).
struct LedcChannel {
    gpio: i8,
    channel: i32,
    timer: i32,
}

static mut LED_R: LedcChannel = LedcChannel { gpio: -1, channel: 0, timer: 0 };
static mut LED_G: LedcChannel = LedcChannel { gpio: -1, channel: 1, timer: 0 };
static mut LED_B: LedcChannel = LedcChannel { gpio: -1, channel: 2, timer: 0 };

/// Initialise LEDC timers and channels for the RGB LED.
/// Port of the LED init from `esp_rid_init`.
pub fn init(r_gpio: i8, g_gpio: i8, b_gpio: i8) {
    unsafe {
        LED_R.gpio = r_gpio;
        LED_G.gpio = g_gpio;
        LED_B.gpio = b_gpio;
    }

    // Configure LEDC timer (13-bit, 5 kHz base).
    let mut timer_cfg: sys::ledc_timer_config_t = unsafe { core::mem::zeroed() };
    timer_cfg.speed_mode = sys::ledc_mode_t_LEDC_LOW_SPEED_MODE;
    timer_cfg.duty_resolution = sys::ledc_timer_bit_t_LEDC_TIMER_13_BIT;
    timer_cfg.timer_num = sys::ledc_timer_t_LEDC_TIMER_0;
    timer_cfg.freq_hz = 5000;
    // `LEDC_AUTO_CLK` has value 0 on every ESP32 family member, but its
    // bindgen constant name differs per chip (ledc_clk_cfg_t_* vs
    // ledc_clk_src_t_*).  Assigning the literal 0 compiles on all three.
    timer_cfg.clk_cfg = 0 as _;
    unsafe { sys::ledc_timer_config(&timer_cfg); }

    // Configure each channel.
    let leds: [(*mut LedcChannel, i8); 3] = unsafe {
        [
            (&mut LED_R as *mut LedcChannel, r_gpio),
            (&mut LED_G as *mut LedcChannel, g_gpio),
            (&mut LED_B as *mut LedcChannel, b_gpio),
        ]
    };
    for (ch, gpio) in leds {
        if gpio < 0 {
            continue;
        }
        let mut ch_cfg: sys::ledc_channel_config_t = unsafe { core::mem::zeroed() };
        ch_cfg.gpio_num = gpio as _;
        ch_cfg.speed_mode = sys::ledc_mode_t_LEDC_LOW_SPEED_MODE;
        ch_cfg.channel = unsafe { (*ch).channel as _ };
        ch_cfg.timer_sel = sys::ledc_timer_t_LEDC_TIMER_0;
        ch_cfg.duty = 0;
        ch_cfg.hpoint = 0;
        unsafe {
            sys::ledc_channel_config(&ch_cfg);
            (*ch).timer = sys::ledc_timer_t_LEDC_TIMER_0 as _;
        }
    }
}

/// Set the RGB duty cycle (each 0..=8191 for 13-bit resolution).
pub fn set_rgb(r: u32, g: u32, b: u32) {
    unsafe {
        if LED_R.gpio >= 0 {
            sys::ledc_set_duty(sys::ledc_mode_t_LEDC_LOW_SPEED_MODE, LED_R.channel as _, r);
            sys::ledc_update_duty(sys::ledc_mode_t_LEDC_LOW_SPEED_MODE, LED_R.channel as _);
        }
        if LED_G.gpio >= 0 {
            sys::ledc_set_duty(sys::ledc_mode_t_LEDC_LOW_SPEED_MODE, LED_G.channel as _, g);
            sys::ledc_update_duty(sys::ledc_mode_t_LEDC_LOW_SPEED_MODE, LED_G.channel as _);
        }
        if LED_B.gpio >= 0 {
            sys::ledc_set_duty(sys::ledc_mode_t_LEDC_LOW_SPEED_MODE, LED_B.channel as _, b);
            sys::ledc_update_duty(sys::ledc_mode_t_LEDC_LOW_SPEED_MODE, LED_B.channel as _);
        }
    }
}

/// Set a single external lighting GPIO on/off.
pub fn set_gpio(pin: i8, on: bool) {
    if pin < 0 {
        return;
    }
    unsafe {
        sys::gpio_set_level(pin as _, on as _);
    }
}

/// Turn off all LEDs.
pub fn all_off() {
    set_rgb(0, 0, 0);
}
