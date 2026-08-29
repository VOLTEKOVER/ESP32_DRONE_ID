//! WS2812 LED strip, port of the pure logic of `led_ws2812.c`: brightness
//! scaling, GRB frame layout and HSV->RGB conversion. The RMT transmit
//! plumbing is BSP concern.

use crate::led_status::Rgb;

/// `g_brightness = pct * 255 / 100` (clamped to 255 by the C u8 assignment).
pub fn brightness_scalar(pct: u8) -> u8 {
    (pct as u16 * 255 / 100) as u8
}

/// `set_rgb(r, g, b)` scaling: each channel `c * brightness / 255`.
pub fn scale_rgb(rgb: Rgb, pct: u8) -> Rgb {
    let br = brightness_scalar(pct);
    Rgb {
        r: (rgb.r as u16 * br as u16 / 255) as u8,
        g: (rgb.g as u16 * br as u16 / 255) as u8,
        b: (rgb.b as u16 * br as u16 / 255) as u8,
    }
}

/// WS2812 data order is G, R, B.
pub fn rgb_to_grb(rgb: Rgb) -> [u8; 3] {
    [rgb.g, rgb.r, rgb.b]
}

/// The full frame written to the strip for a color at a brightness percent.
pub fn ws2812_frame(rgb: Rgb, pct: u8) -> [u8; 3] {
    rgb_to_grb(scale_rgb(rgb, pct))
}

/// `set_hsv(h, s, v)` conversion. `hue` is uint16 like the C. The `region`
/// is the C `uint8_t region = hue / 43` (truncating cast), `remainder` wraps
/// like the C uint16 assignment; normal hues (< 1024) match the classic 6
/// segment wheel.
pub fn hsv_to_rgb(hue: u16, sat: u8, val: u8) -> Rgb {
    let sat = sat as u16;
    let val = val as u16;
    let region = (hue / 43) as u8;
    let remainder = ((hue as i32 - region as i32 * 43) as u16).wrapping_mul(6);
    let p = val * (255 - sat) / 255;
    let q = val * (255 - sat.wrapping_mul(remainder) / 255) / 255;
    let t = val * (255 - sat.wrapping_mul(255u16.wrapping_sub(remainder)) / 255) / 255;
    let (r, g, b) = match region {
        0 => (val, t, p),
        1 => (q, val, p),
        2 => (p, val, t),
        3 => (p, q, val),
        4 => (t, p, val),
        _ => (val, p, q),
    };
    Rgb {
        r: r as u8,
        g: g as u8,
        b: b as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_scalar_percent() {
        assert_eq!(brightness_scalar(0), 0);
        assert_eq!(brightness_scalar(100), 255);
        assert_eq!(brightness_scalar(50), 127);
        assert_eq!(brightness_scalar(16), 40);
    }

    #[test]
    fn scale_rgb_uses_scalar() {
        let rgb = Rgb {
            r: 255,
            g: 128,
            b: 0,
        };
        assert_eq!(scale_rgb(rgb, 100), rgb);
        // g_brightness = 50 * 255 / 100 = 127.
        assert_eq!(
            scale_rgb(rgb, 50),
            Rgb {
                r: (255u16 * 127 / 255) as u8,
                g: (128u16 * 127 / 255) as u8,
                b: 0,
            }
        );
        assert_eq!(scale_rgb(rgb, 0), Rgb::BLACK);
    }

    #[test]
    fn frame_is_grb() {
        let rgb = Rgb {
            r: 255,
            g: 128,
            b: 64,
        };
        assert_eq!(rgb_to_grb(rgb), [128, 255, 64]);
        assert_eq!(ws2812_frame(rgb, 100), [128, 255, 64]);
    }

    #[test]
    fn hsv_primaries() {
        // hue 0 -> red.
        assert_eq!(hsv_to_rgb(0, 255, 255), Rgb { r: 255, g: 0, b: 0 });
        // hue 43 -> region 1 -> yellow.
        assert_eq!(
            hsv_to_rgb(43, 255, 255),
            Rgb {
                r: 255,
                g: 255,
                b: 0
            }
        );
        // hue 85 -> region 1 end: sat 255, remainder 252.
        assert_eq!(hsv_to_rgb(85, 255, 255), Rgb { r: 3, g: 255, b: 0 });
        // hue 127 (region 2) -> cyan-ish: remainder = (127-86)*6 = 246.
        // p = 0, t = 255 - 255*(255-246)/255 = 246 -> (0, 255, 246).
        assert_eq!(
            hsv_to_rgb(127, 255, 255),
            Rgb {
                r: 0,
                g: 255,
                b: 246
            }
        );
    }

    #[test]
    fn hsv_low_saturation_is_gray() {
        assert_eq!(
            hsv_to_rgb(1023, 0, 255),
            Rgb {
                r: 255,
                g: 255,
                b: 255
            }
        );
        // sat 0 at any hue -> white at full value.
        assert_eq!(
            hsv_to_rgb(512, 0, 100),
            Rgb {
                r: 100,
                g: 100,
                b: 100
            }
        );
    }

    #[test]
    fn hsv_default_region_is_magenta_family() {
        // hue 512 -> region 11 -> default (val, p, q).
        // remainder = (512-473)*6 = 234, q = 255*(255-234)/255 = 21.
        assert_eq!(
            hsv_to_rgb(512, 255, 255),
            Rgb {
                r: 255,
                g: 0,
                b: 21
            }
        );
        // hue 1000 -> region 23 -> default; remainder = 66, q = 189.
        assert_eq!(
            hsv_to_rgb(1000, 255, 255),
            Rgb {
                r: 255,
                g: 0,
                b: 189
            }
        );
    }
}
