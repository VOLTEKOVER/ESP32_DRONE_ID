//! External GPIO lighting, port of the pure logic of `rid_lighting.c`: the
//! per-channel on/off decision from the configured pattern and phase offset.
//! GPIO writes are BSP concern.

use crate::config::NUM_LIGHTING_PINS;

/// `lighting_pattern_t` from `rid_lighting.c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightingPattern {
    Off = 0,
    Solid = 1,
    BlinkSlow = 2,
    BlinkFast = 3,
    BlinkArmed = 4,
    FlashOnGps = 5,
}

impl LightingPattern {
    /// Cast of the C `(lighting_pattern_t)patterns[i]`; unknown values map to
    /// `Off` (which `pattern_active` answers `false`, like the C default).
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Solid,
            2 => Self::BlinkSlow,
            3 => Self::BlinkFast,
            4 => Self::BlinkArmed,
            5 => Self::FlashOnGps,
            _ => Self::Off,
        }
    }
}

/// Port of `pattern_active()` in `rid_lighting.c` (with `armed`/`gps_valid`
/// passed instead of stored globals).
pub fn pattern_active(pattern: LightingPattern, now_ms: u32, armed: bool, gps_valid: bool) -> bool {
    let phase = now_ms % 2000;
    match pattern {
        LightingPattern::Off => false,
        LightingPattern::Solid => true,
        LightingPattern::BlinkSlow => (phase % 2000) < 1000,
        LightingPattern::BlinkFast => (phase % 500) < 250,
        LightingPattern::BlinkArmed => armed && ((phase % 1000) < 500),
        LightingPattern::FlashOnGps => gps_valid,
    }
}

/// One configured lighting output (`lighting_channel_t` without the GPIO).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightingChannel {
    pub pattern: LightingPattern,
    pub phase_offset_ms: i16,
}

/// `pattern_active(pattern, now_ms + phase_offset_ms)` — the negative offset
/// wraps like the C `uint32_t + int16_t` arithmetic.
pub fn channel_active(ch: &LightingChannel, now_ms: u32, armed: bool, gps_valid: bool) -> bool {
    let t = now_ms.wrapping_add(ch.phase_offset_ms as u32);
    pattern_active(ch.pattern, t, armed, gps_valid)
}

/// Port of `rid_lighting_init()` channel building: channels with a negative
/// pin are skipped. The result is indexed like the config arrays, so the BSP
/// keeps the matching pin array for the GPIO writes.
pub fn channels_from_config(
    pins: &[i8; NUM_LIGHTING_PINS],
    patterns: &[u8; NUM_LIGHTING_PINS],
    phase_offsets: &[i16; NUM_LIGHTING_PINS],
) -> [Option<LightingChannel>; NUM_LIGHTING_PINS] {
    let mut out = [None; NUM_LIGHTING_PINS];
    for i in 0..NUM_LIGHTING_PINS {
        if pins[i] < 0 {
            continue;
        }
        out[i] = Some(LightingChannel {
            pattern: LightingPattern::from_u8(patterns[i]),
            phase_offset_ms: phase_offsets[i],
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(pattern: LightingPattern, now_ms: u32) -> bool {
        pattern_active(pattern, now_ms, false, false)
    }

    #[test]
    fn off_never_active() {
        assert!(!on(LightingPattern::Off, 0));
        assert!(!on(LightingPattern::Off, 1500));
    }

    #[test]
    fn solid_always_active() {
        assert!(on(LightingPattern::Solid, 0));
        assert!(on(LightingPattern::Solid, 1234567));
    }

    #[test]
    fn blink_slow_half_period() {
        assert!(on(LightingPattern::BlinkSlow, 0));
        assert!(on(LightingPattern::BlinkSlow, 999));
        assert!(!on(LightingPattern::BlinkSlow, 1000));
        assert!(!on(LightingPattern::BlinkSlow, 1999));
        // Wraps every 2000 ms.
        assert!(on(LightingPattern::BlinkSlow, 2500));
    }

    #[test]
    fn blink_fast_quarter_period() {
        assert!(on(LightingPattern::BlinkFast, 0));
        assert!(!on(LightingPattern::BlinkFast, 250));
        assert!(!on(LightingPattern::BlinkFast, 499));
        assert!(on(LightingPattern::BlinkFast, 600));
    }

    #[test]
    fn blink_armed_requires_armed() {
        assert!(!on(LightingPattern::BlinkArmed, 0));
        assert!(pattern_active(LightingPattern::BlinkArmed, 0, true, false));
        assert!(!pattern_active(
            LightingPattern::BlinkArmed,
            500,
            true,
            false
        ));
        assert!(pattern_active(
            LightingPattern::BlinkArmed,
            1000,
            true,
            false
        ));
    }

    #[test]
    fn flash_on_gps_requires_fix() {
        assert!(!on(LightingPattern::FlashOnGps, 0));
        assert!(pattern_active(
            LightingPattern::FlashOnGps,
            9999,
            false,
            true
        ));
    }

    #[test]
    fn unknown_pattern_value_is_off() {
        assert_eq!(LightingPattern::from_u8(99), LightingPattern::Off);
        assert!(!on(LightingPattern::from_u8(99), 0));
    }

    #[test]
    fn positive_phase_offset_shifts_the_pattern() {
        let ch = LightingChannel {
            pattern: LightingPattern::BlinkSlow,
            phase_offset_ms: 1000,
        };
        // t = 1000 -> off; t = 2500 -> 500 < 1000 -> on.
        assert!(!channel_active(&ch, 0, false, false));
        assert!(channel_active(&ch, 1500, false, false));
    }

    #[test]
    fn negative_phase_offset_wraps_unsigned() {
        // now + (-1000) in uint32 arithmetic lands on an "on" phase for
        // BlinkSlow at now = 0, whereas +1000 would be "off" (see above).
        let ch = LightingChannel {
            pattern: LightingPattern::BlinkSlow,
            phase_offset_ms: -1000,
        };
        assert!(channel_active(&ch, 0, false, false));
    }

    #[test]
    fn channels_skip_negative_pins() {
        let pins = [1i8, -1, 2, -1, -1];
        let patterns = [0u8, 3, 1, 0, 0];
        let offsets = [0i16, 0, 100, 0, 0];
        let chans = channels_from_config(&pins, &patterns, &offsets);
        assert_eq!(
            chans[0],
            Some(LightingChannel {
                pattern: LightingPattern::Off,
                phase_offset_ms: 0
            })
        );
        assert_eq!(chans[1], None);
        assert_eq!(
            chans[2],
            Some(LightingChannel {
                pattern: LightingPattern::Solid,
                phase_offset_ms: 100
            })
        );
        assert_eq!(chans[3], None);
        assert_eq!(chans[4], None);
    }

    #[test]
    fn pattern_byte_is_cast_verbatim() {
        let chans =
            channels_from_config(&[3, -1, -1, -1, -1], &[4, 0, 0, 0, 0], &[-45, 0, 0, 0, 0]);
        assert_eq!(chans[0].unwrap().pattern, LightingPattern::BlinkArmed);
        assert_eq!(chans[0].unwrap().phase_offset_ms, -45);
    }
}
