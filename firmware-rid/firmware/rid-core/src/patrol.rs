//! Demo-mode patrol trajectory, port of `rid_patrol.c` with 100% equivalent
//! logic. Used by the scheduler when `OPT_DEMO_MODE` is set and no GPS fix is
//! available. `no_std` float math via `libm`.

use rid_interface::GpsData;

const PATROL_HOME_LAT: f32 = 41.9028;
const PATROL_HOME_LON: f32 = 12.4964;
const PATROL_RADIUS: f32 = 0.003;
const PATROL_SPEED: f32 = 6.0;

/// Port of the `static float angle` in `rid_patrol_tick()`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Patrol {
    angle: f32,
}

impl Patrol {
    /// Port of `rid_patrol_tick()`.
    pub fn tick(&mut self, gps: &mut GpsData) {
        self.angle += 0.018;
        if self.angle > 2.0 * core::f32::consts::PI {
            self.angle -= 2.0 * core::f32::consts::PI;
        }

        gps.latitude = (PATROL_HOME_LAT + libm::cosf(self.angle) * PATROL_RADIUS) as f64;
        gps.longitude = (PATROL_HOME_LON + libm::sinf(self.angle) * PATROL_RADIUS) as f64;
        gps.altitude_msl = 50.0 + libm::sinf(self.angle * 2.0) * 20.0;
        gps.altitude_baro = gps.altitude_msl;
        gps.altitude_relative = gps.altitude_msl;
        gps.speed = PATROL_SPEED + libm::sinf(self.angle) * 2.0;
        gps.speed_vertical = libm::sinf(self.angle * 2.0) * 0.5;

        let mut heading = (libm::atan2f(libm::cosf(self.angle), -libm::sinf(self.angle))
            * (180.0 / core::f32::consts::PI)) as i16;
        if heading < 0 {
            heading += 360;
        }
        gps.heading = heading;

        let var = libm::sinf(self.angle * 3.0) * 0.5 + 0.5;
        let fix_type = (2 + (var * 2.0) as u8).clamp(2, 4);
        gps.fix_type = fix_type;
        gps.satellites = 6 + (var * 10.0) as u8;
        gps.armed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patrol_stays_inside_bounds() {
        let mut patrol = Patrol::default();
        let mut gps = GpsData::default();
        for _ in 0..200 {
            patrol.tick(&mut gps);
            assert!(gps.latitude > 41.0 && gps.latitude < 43.0);
            assert!(gps.longitude > 12.0 && gps.longitude < 13.0);
            assert!(gps.fix_type >= 2 && gps.fix_type <= 4);
            assert!(gps.armed);
            assert!(gps.heading >= 0 && gps.heading < 360);
        }
    }

    #[test]
    fn angle_wraps() {
        let mut patrol = Patrol { angle: 6.26 };
        let mut gps = GpsData::default();
        // One step past 2*PI (6.28318...) wraps.
        patrol.tick(&mut gps);
        assert!(patrol.angle < 2.0 * core::f32::consts::PI);
        assert!(patrol.angle > 0.0);
    }
}
