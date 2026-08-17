//! 3x 1D Kalman filter for GPS smoothing, port of `rid_kalman.c` with 100%
//! equivalent logic. Uses only `core` float math (`f32::cos`, `f32::sqrt`,
//! `f32::atan2`), so it stays `no_std`.

/// `RID_KALMAN_TIMEOUT_US` from the C header.
pub const KALMAN_TIMEOUT_US: u64 = 3_000_000;

const DEG2M_LAT: f32 = 111320.0;

/// 1D Kalman state, port of `rid_kalman_1d_t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kalman1d {
    pub x: f32,
    pub v: f32,
    pub p00: f32,
    pub p01: f32,
    pub p11: f32,
    pub q_pos: f32,
    pub q_vel: f32,
    pub r: f32,
    pub t_us: u64,
    pub valid: bool,
}

impl Kalman1d {
    /// Port of `rid_kalman_init_1d()`.
    pub fn init(&mut self, q_pos: f32, q_vel: f32, r: f32) {
        self.x = 0.0;
        self.v = 0.0;
        self.p00 = 0.0;
        self.p01 = 0.0;
        self.p11 = 0.0;
        self.q_pos = q_pos;
        self.q_vel = q_vel;
        self.r = r;
        self.t_us = 0;
        self.valid = false;
    }

    /// Port of `rid_kalman_predict_1d()`.
    pub fn predict(&mut self, now_us: u64) {
        if !self.valid {
            return;
        }

        let dt_us = now_us.wrapping_sub(self.t_us) as i64;
        if dt_us <= 0 {
            return;
        }

        let dt = dt_us as f32 * 1e-6;

        self.x += self.v * dt;
        let p00 = self.p00 + 2.0 * self.p01 * dt + self.p11 * dt * dt + self.q_pos * dt;
        let p01 = self.p01 + self.p11 * dt;
        let p11 = self.p11 + self.q_vel * dt;
        self.p00 = p00;
        self.p01 = p01;
        self.p11 = p11;

        self.t_us = now_us;
    }

    /// Port of `rid_kalman_update_1d()`.
    pub fn update(&mut self, meas: f32, now_us: u64) {
        if !self.valid {
            self.x = meas;
            self.v = 0.0;
            self.p00 = self.r;
            self.p01 = 0.0;
            self.p11 = 100.0 * self.r;
            self.t_us = now_us;
            self.valid = true;
            return;
        }

        self.predict(now_us);

        let y = meas - self.x;
        let s = self.p00 + self.r;
        let k0 = self.p00 / s;
        let k1 = self.p01 / s;

        self.x += k0 * y;
        self.v += k1 * y;

        let p00_new = (1.0 - k0) * self.p00;
        let p01_new = (1.0 - k0) * self.p01;
        let p11_new = self.p11 - k1 * self.p01;
        self.p00 = p00_new;
        self.p01 = p01_new;
        self.p11 = p11_new;

        self.t_us = now_us;
    }
}

/// 3D Kalman state, port of `rid_kalman_3d_t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kalman3d {
    pub lat: Kalman1d,
    pub lon: Kalman1d,
    pub alt: Kalman1d,
}

/// Smoothed position/velocity output, port of the `rid_kalman_get()` out
/// parameters. The speed/climb/heading fields are always computed (the C
/// code skips the trig only when all three output pointers are NULL; the
/// computed values are identical).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KalmanOut {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f32,
    pub speed: f32,
    pub climb: f32,
    pub heading: i16,
}

impl Kalman3d {
    /// Port of `rid_kalman_init()`.
    pub fn init(&mut self) {
        self.lat.init(1e-9, 1e-8, 1e-9);
        self.lon.init(1.5e-9, 1.5e-8, 1.5e-9);
        self.alt.init(1.0, 10.0, 25.0);
    }

    /// Port of `rid_kalman_predict()`.
    pub fn predict(&mut self, now_us: u64) {
        self.lat.predict(now_us);
        self.lon.predict(now_us);
        self.alt.predict(now_us);
    }

    /// Port of `rid_kalman_update()`.
    pub fn update(&mut self, latitude: f64, longitude: f64, altitude: f32, now_us: u64) {
        self.lat.update(latitude as f32, now_us);
        self.lon.update(longitude as f32, now_us);
        self.alt.update(altitude, now_us);
    }

    /// Port of `rid_kalman_get()`.
    pub fn get(&self) -> KalmanOut {
        let lat_rad = self.lat.x * (core::f32::consts::PI / 180.0);
        let mut cos_lat = libm::cosf(lat_rad);
        if cos_lat < 0.01 {
            cos_lat = 0.01;
        }

        let vn = self.lat.v * DEG2M_LAT;
        let ve = self.lon.v * DEG2M_LAT * cos_lat;

        let speed = libm::sqrtf(vn * vn + ve * ve);
        let climb = self.alt.v;

        let mut h = libm::atan2f(ve, vn) * (180.0 / core::f32::consts::PI);
        if h < 0.0 {
            h += 360.0;
        }
        if h >= 360.0 {
            h -= 360.0;
        }
        let heading = (h + 0.5) as i16;

        KalmanOut {
            latitude: self.lat.x as f64,
            longitude: self.lon.x as f64,
            altitude: self.alt.x,
            speed,
            climb,
            heading,
        }
    }

    /// Port of `rid_kalman_valid()`.
    pub fn valid(&self) -> bool {
        self.lat.valid || self.lon.valid || self.alt.valid
    }

    /// Port of `rid_kalman_valid_age()`.
    pub fn valid_age(&self, now_us: u64) -> bool {
        if !self.lat.valid && !self.lon.valid && !self.alt.valid {
            return false;
        }
        if now_us.wrapping_sub(self.lat.t_us) > KALMAN_TIMEOUT_US
            || now_us.wrapping_sub(self.lon.t_us) > KALMAN_TIMEOUT_US
            || now_us.wrapping_sub(self.alt.t_us) > KALMAN_TIMEOUT_US
        {
            return false;
        }
        true
    }

    /// Port of `rid_kalman_reset()`.
    pub fn reset(&mut self) {
        self.lat.valid = false;
        self.lon.valid = false;
        self.alt.valid = false;
    }
}

impl Default for Kalman3d {
    /// Matches `rid_kalman_init()` tuning (lat: 1e-9/1e-8/1e-9,
    /// lon: 1.5e-9/1.5e-8/1.5e-9, alt: 1.0/10.0/25.0).
    fn default() -> Self {
        let mut k = Kalman3d {
            lat: Kalman1d {
                x: 0.0,
                v: 0.0,
                p00: 0.0,
                p01: 0.0,
                p11: 0.0,
                q_pos: 0.0,
                q_vel: 0.0,
                r: 0.0,
                t_us: 0,
                valid: false,
            },
            lon: Kalman1d {
                x: 0.0,
                v: 0.0,
                p00: 0.0,
                p01: 0.0,
                p11: 0.0,
                q_pos: 0.0,
                q_vel: 0.0,
                r: 0.0,
                t_us: 0,
                valid: false,
            },
            alt: Kalman1d {
                x: 0.0,
                v: 0.0,
                p00: 0.0,
                p01: 0.0,
                p11: 0.0,
                q_pos: 0.0,
                q_vel: 0.0,
                r: 0.0,
                t_us: 0,
                valid: false,
            },
        };
        k.init();
        k
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(a: f32, b: f32, eps: f32) {
        assert!((a - b).abs() <= eps, "|{a} - {b}| > {eps}");
    }

    #[test]
    fn first_update_initializes_state() {
        let mut k = Kalman1d {
            x: 0.0,
            v: 0.0,
            p00: 0.0,
            p01: 0.0,
            p11: 0.0,
            q_pos: 1e-9,
            q_vel: 1e-8,
            r: 1e-9,
            t_us: 0,
            valid: false,
        };
        k.update(10.0, 1_000_000);
        assert!(k.valid);
        assert_close(k.x, 10.0, 1e-6);
        assert_close(k.v, 0.0, 1e-9);
        assert_close(k.p00, 1e-9, 1e-12);
        assert_close(k.p11, 100.0 * 1e-9, 1e-9);
    }

    #[test]
    fn predict_advances_position() {
        let mut k = Kalman1d {
            x: 10.0,
            v: 2.0,
            p00: 1.0,
            p01: 0.0,
            p11: 1.0,
            q_pos: 0.0,
            q_vel: 0.0,
            r: 1.0,
            t_us: 0,
            valid: true,
        };
        k.predict(1_000_000);
        assert_close(k.x, 12.0, 1e-5);
        assert_close(k.p01, 1.0, 1e-6);
        assert_eq!(k.t_us, 1_000_000);
    }

    #[test]
    fn predict_ignores_non_positive_dt() {
        let mut k = Kalman1d {
            x: 10.0,
            v: 2.0,
            p00: 1.0,
            p01: 0.0,
            p11: 1.0,
            q_pos: 0.0,
            q_vel: 0.0,
            r: 1.0,
            t_us: 5_000_000,
            valid: true,
        };
        // Same timestamp -> no advance.
        k.predict(5_000_000);
        assert_close(k.x, 10.0, 1e-6);
        // Backwards timestamp (wraps, as in C) -> no advance.
        k.predict(4_000_000);
        assert_close(k.x, 10.0, 1e-6);
    }

    #[test]
    fn update_converges_to_measurement() {
        let mut k = Kalman3d::default();
        k.update(45.4642, 9.1900, 150.0, 100_000);
        k.update(45.4643, 9.1901, 151.0, 200_000);
        k.update(45.4644, 9.1902, 152.0, 300_000);
        assert!(k.valid());
        let o = k.get();
        assert_close(o.latitude as f32, 45.4643, 0.01);
        assert_close(o.longitude as f32, 9.1901, 0.01);
        assert!(o.speed > 0.0);
        assert!(o.heading >= 0 && o.heading < 360);
    }

    #[test]
    fn valid_age_expires() {
        let mut k = Kalman3d::default();
        k.update(45.0, 9.0, 100.0, 0);
        assert!(k.valid_age(1_000_000));
        assert!(!k.valid_age(KALMAN_TIMEOUT_US + 1));
    }

    #[test]
    fn reset_clears_validity() {
        let mut k = Kalman3d::default();
        k.update(45.0, 9.0, 100.0, 0);
        assert!(k.valid());
        k.reset();
        assert!(!k.valid());
        assert!(!k.valid_age(0));
    }

    #[test]
    fn heading_from_velocity() {
        // Pure north velocity -> heading 0, speed = vn.
        let mut k = Kalman3d::default();
        k.update(45.0, 9.0, 100.0, 0);
        k.update(45.0, 9.0, 100.0, 1_000_000);
        // Force a north-only velocity by resetting lon velocity.
        k.lon.v = 0.0;
        let o = k.get();
        assert_close(o.speed, k.lat.v * DEG2M_LAT, 1e-3);
        assert!(o.heading == 0 || o.heading == 360);
    }
}
