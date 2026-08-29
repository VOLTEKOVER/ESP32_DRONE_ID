//! Status RGB LED, port of the pure logic of `led_status.c`: the state table
//! and pattern generators (blink/pulse/rainbow) plus the TX-flash override.
//! LEDC/PWM plumbing is BSP concern.
//!
//! Like the C, the blink patterns are driven by a tick counter (each
//! `tick()` call advances it) while `Pulse`/`Rainbow` use the real
//! millisecond clock passed to `tick`.

use crate::led_status::LedState::*;

/// `TX_FLASH_US` in milliseconds.
pub const TX_FLASH_MS: u32 = 80;

/// RGB color, `struct rgb` from `led_status.c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0 };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
    };
}

/// `rid_led_state_t`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedState {
    Boot,
    NoGps,
    GpsOk,
    Demo,
    Locked,
    Ota,
    Error,
}

/// `pattern_t` from `led_status.c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedPattern {
    Solid,
    Blink1Hz,
    Blink4Hz,
    BlinkDouble,
    Pulse,
    Rainbow,
}

/// One entry of the C `state_table`: color, pattern and log name.
pub fn led_state_entry(state: LedState) -> (Rgb, LedPattern, &'static str) {
    match state {
        Boot => (
            Rgb {
                r: 40,
                g: 80,
                b: 255,
            },
            LedPattern::Pulse,
            "BOOT",
        ),
        NoGps => (
            Rgb {
                r: 255,
                g: 200,
                b: 0,
            },
            LedPattern::Blink1Hz,
            "NO_GPS",
        ),
        GpsOk => (Rgb { r: 0, g: 255, b: 0 }, LedPattern::Solid, "GPS_OK"),
        Demo => (
            Rgb {
                r: 180,
                g: 40,
                b: 255,
            },
            LedPattern::Pulse,
            "DEMO",
        ),
        Locked => (
            Rgb { r: 255, g: 0, b: 0 },
            LedPattern::BlinkDouble,
            "LOCKED",
        ),
        Ota => (
            Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            LedPattern::Rainbow,
            "OTA",
        ),
        Error => (Rgb { r: 255, g: 0, b: 0 }, LedPattern::Blink4Hz, "ERROR"),
    }
}

/// `solid()` generator.
pub fn solid(c: Rgb) -> Rgb {
    c
}

/// Shared blink generator: `phase = (tick_count * 100) % period`, on for the
/// first `on_ms` of each period (matches `blink_1hz`/`blink_4hz`).
fn blink(c: Rgb, tick_count: u64, period_ms: u32, on_ms: u32) -> Rgb {
    let phase = (tick_count * 100) % period_ms as u64;
    if phase < on_ms as u64 {
        c
    } else {
        Rgb::BLACK
    }
}

/// `blink_1hz()` generator.
pub fn blink_1hz(c: Rgb, tick_count: u64) -> Rgb {
    blink(c, tick_count, 1000, 500)
}

/// `blink_4hz()` generator.
pub fn blink_4hz(c: Rgb, tick_count: u64) -> Rgb {
    blink(c, tick_count, 250, 125)
}

/// `blink_double()` generator.
pub fn blink_double(c: Rgb, tick_count: u64) -> Rgb {
    let phase = (tick_count * 100) % 1400;
    if phase < 200 {
        c
    } else if phase < 400 {
        Rgb::BLACK
    } else if phase < 600 {
        c
    } else {
        Rgb::BLACK
    }
}

/// `pulse()` generator (real milliseconds).
pub fn pulse(c: Rgb, now_ms: u32) -> Rgb {
    let phase = now_ms % 2000;
    let half = if phase < 1000 { phase } else { 2000 - phase };
    let bright = (half * 255 / 1000) as u8;
    Rgb {
        r: (c.r as u16 * bright as u16 / 255) as u8,
        g: (c.g as u16 * bright as u16 / 255) as u8,
        b: (c.b as u16 * bright as u16 / 255) as u8,
    }
}

/// `rainbow()` generator (real milliseconds).
pub fn rainbow(now_ms: u32) -> Rgb {
    let phase = (now_ms / 20) % 256;
    let (r, g, b) = if phase < 85 {
        (phase * 3, 255 - phase * 3, 0)
    } else if phase < 170 {
        let p = phase - 85;
        (255 - p * 3, 0, p * 3)
    } else {
        let p = phase - 170;
        (0, p * 3, 255 - p * 3)
    };
    Rgb {
        r: r as u8,
        g: g as u8,
        b: b as u8,
    }
}

/// Port of the `led_status.c` state machine globals (`current_state`,
/// `last_tx_flash_us`, `tick_count`) and `led_status_tick()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedStateMachine {
    state: LedState,
    last_tx_flash_ms: Option<u32>,
    tick_count: u64,
}

impl LedStateMachine {
    pub const fn new() -> Self {
        Self {
            state: Boot,
            last_tx_flash_ms: None,
            tick_count: 0,
        }
    }

    pub fn state(&self) -> LedState {
        self.state
    }

    /// `led_status_set_state()`.
    pub fn set_state(&mut self, state: LedState) {
        self.state = state;
    }

    /// `led_status_tx_flash()`.
    pub fn tx_flash(&mut self, now_ms: u32) {
        self.last_tx_flash_ms = Some(now_ms);
    }

    /// `led_status_tick()`: returns the RGB output for this tick. The TX
    /// flash override (white for `TX_FLASH_MS` after `tx_flash`) wins over
    /// the state pattern.
    pub fn tick(&mut self, now_ms: u32) -> Rgb {
        self.tick_count = self.tick_count.wrapping_add(1);
        if let Some(t) = self.last_tx_flash_ms {
            if now_ms.wrapping_sub(t) < TX_FLASH_MS {
                return Rgb::WHITE;
            }
        }
        let (color, pat, _name) = led_state_entry(self.state);
        match pat {
            LedPattern::Solid => solid(color),
            LedPattern::Blink1Hz => blink_1hz(color, self.tick_count),
            LedPattern::Blink4Hz => blink_4hz(color, self.tick_count),
            LedPattern::BlinkDouble => blink_double(color, self.tick_count),
            LedPattern::Pulse => pulse(color, now_ms),
            LedPattern::Rainbow => rainbow(now_ms),
        }
    }
}

impl Default for LedStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };

    #[test]
    fn solid_returns_color_unchanged() {
        assert_eq!(solid(RED), RED);
    }

    #[test]
    fn blink_1hz_toggle_points() {
        assert_eq!(blink_1hz(RED, 0), RED);
        assert_eq!(blink_1hz(RED, 5), Rgb::BLACK);
        assert_eq!(blink_1hz(RED, 9), Rgb::BLACK);
        assert_eq!(blink_1hz(RED, 10), RED);
    }

    #[test]
    fn blink_4hz_toggle_points() {
        assert_eq!(blink_4hz(RED, 0), RED);
        assert_eq!(blink_4hz(RED, 2), Rgb::BLACK); // 200 % 250 = 200
        assert_eq!(blink_4hz(RED, 3), RED); // 300 % 250 = 50
    }

    #[test]
    fn blink_double_sequence() {
        assert_eq!(blink_double(RED, 0), RED); // 0
        assert_eq!(blink_double(RED, 3), Rgb::BLACK); // 300
        assert_eq!(blink_double(RED, 5), RED); // 500
        assert_eq!(blink_double(RED, 7), Rgb::BLACK); // 700
        assert_eq!(blink_double(RED, 14), RED); // 1400 % 1400 = 0
    }

    #[test]
    fn pulse_ramps_and_falls() {
        let c = Rgb {
            r: 40,
            g: 80,
            b: 255,
        };
        assert_eq!(pulse(c, 0), Rgb::BLACK);
        assert_eq!(pulse(c, 1000), c); // full brightness mid-cycle
                                       // Symmetric at +500 ms: bright = 500 * 255 / 1000 = 127.
        let half = Rgb {
            r: (40u16 * 127 / 255) as u8,
            g: (80u16 * 127 / 255) as u8,
            b: (255u16 * 127 / 255) as u8,
        };
        assert_eq!(pulse(c, 500), half);
        assert_eq!(pulse(c, 1500), half);
        assert_eq!(pulse(c, 2000), Rgb::BLACK);
    }

    #[test]
    fn rainbow_region_boundaries() {
        assert_eq!(rainbow(0), Rgb { r: 0, g: 255, b: 0 });
        assert_eq!(rainbow(1700), Rgb { r: 255, g: 0, b: 0 }); // 85
        assert_eq!(rainbow(3400), Rgb { r: 0, g: 0, b: 255 }); // 170
        assert_eq!(
            rainbow(1000),
            Rgb {
                r: 150,
                g: 105,
                b: 0
            }
        ); // 50
    }

    #[test]
    fn state_table_matches_c() {
        assert_eq!(
            led_state_entry(LedState::Boot),
            (
                Rgb {
                    r: 40,
                    g: 80,
                    b: 255
                },
                LedPattern::Pulse,
                "BOOT"
            )
        );
        assert_eq!(
            led_state_entry(LedState::NoGps),
            (
                Rgb {
                    r: 255,
                    g: 200,
                    b: 0
                },
                LedPattern::Blink1Hz,
                "NO_GPS"
            )
        );
        assert_eq!(
            led_state_entry(LedState::GpsOk),
            (Rgb { r: 0, g: 255, b: 0 }, LedPattern::Solid, "GPS_OK")
        );
        assert_eq!(
            led_state_entry(LedState::Locked),
            (
                Rgb { r: 255, g: 0, b: 0 },
                LedPattern::BlinkDouble,
                "LOCKED"
            )
        );
        assert_eq!(led_state_entry(LedState::Ota).2, "OTA");
        assert_eq!(led_state_entry(LedState::Error).1, LedPattern::Blink4Hz);
    }

    #[test]
    fn solid_state_outputs_color_every_tick() {
        let mut sm = LedStateMachine::new();
        sm.set_state(LedState::GpsOk);
        assert_eq!(sm.tick(0), Rgb { r: 0, g: 255, b: 0 });
        assert_eq!(sm.tick(5000), Rgb { r: 0, g: 255, b: 0 });
    }

    #[test]
    fn tx_flash_overrides_then_expires() {
        let mut sm = LedStateMachine::new();
        sm.set_state(LedState::GpsOk);
        sm.tx_flash(1000);
        assert_eq!(sm.tick(1010), Rgb::WHITE);
        assert_eq!(sm.tick(1079), Rgb::WHITE);
        // 1080 - 1000 = 80, not < 80 -> back to the state color.
        assert_eq!(sm.tick(1080), Rgb { r: 0, g: 255, b: 0 });
    }

    #[test]
    fn blink_state_advances_with_tick_count() {
        let mut sm = LedStateMachine::new();
        sm.set_state(LedState::NoGps);
        // First tick -> tick_count = 1 -> phase 100 < 500 -> on.
        assert_eq!(
            sm.tick(0),
            Rgb {
                r: 255,
                g: 200,
                b: 0
            }
        );
        // Fifth tick -> tick_count = 5 -> phase 500 -> off.
        for _ in 0..4 {
            sm.tick(0);
        }
        assert_eq!(sm.tick(0), Rgb::BLACK);
    }

    #[test]
    fn set_state_switches_output() {
        let mut sm = LedStateMachine::new();
        sm.set_state(LedState::GpsOk);
        assert_eq!(sm.tick(0), Rgb { r: 0, g: 255, b: 0 });
        sm.set_state(LedState::Error);
        // tick 2: blink_4hz phase = 200 -> off; tick 3: phase 50 -> on.
        assert_eq!(sm.tick(0), Rgb::BLACK);
        assert_eq!(sm.tick(0), RED);
        assert_eq!(sm.state(), LedState::Error);
    }
}
