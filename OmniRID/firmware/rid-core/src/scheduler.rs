//! Central scheduler loop, port of `rid_task()` in `esp_remote_id.c` with
//! 100% equivalent logic.
//!
//! One `tick()` processes one loop iteration: ingest the input sample (from a
//! `GpsSource`), merge it into the state (takeoff capture, relative altitude,
//! operator location, identity readiness), run the Kalman filter, apply the
//! absolute GPS timeout, decide which transports transmit (via a
//! `Transmitter`) and pick the LED status. The hardware clocks (`now_ms` /
//! `now_us`), the parsers and the transports are injected, so the whole loop
//! is host-testable.

use rid_interface::input::{InputSample, Transmitter};
use rid_interface::{
    CStr, Config, Protocol, State, OPT_DEMO_MODE, OPT_DONT_SAVE_BASIC_ID, OPT_FORCE_ARM_OK,
    OPT_IDENTITY_READY_GATE, OPT_KALMAN_FILTER, TRANSMIT_BLE4, TRANSMIT_BLE5, TRANSMIT_WIFI_BCN,
    TRANSMIT_WIFI_NAN,
};

use crate::kalman::Kalman3d;
use crate::patrol::Patrol;
use crate::{hub, readiness};

/// Absolute GPS timeout, port of the hardcoded 10 s in `rid_task`.
pub const GPS_STALE_TIMEOUT_MS: u32 = 10_000;

/// LED status selected by the scheduler (port of the `led_status_set_state`
/// choices). The BSP maps this to its hardware.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LedState {
    Locked,
    Demo,
    GpsOk,
    NoGps,
}

/// Result of one scheduler tick; the BSP drives its LEDs/prints from here.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TickOutcome {
    /// True when a fresh fix (or demo mode) was available this tick.
    pub had_gps: bool,
    /// True when at least one transport transmitted this tick.
    pub tx_fired: bool,
    /// True when the absolute GPS timeout just expired.
    pub gps_stale: bool,
    pub led: LedState,
    /// WS2812 color: green when `gps_valid`, amber otherwise.
    pub ws2812_green: bool,
    /// External lighting armed flag (`rid_lighting_set_state`).
    pub lighting_armed: bool,
    /// `log_cycle % 100 == 0` (status box).
    pub periodic_status: bool,
    /// `log_cycle % 500 == 0` (system box).
    pub periodic_system: bool,
}

/// Central scheduler, port of the `g_state`/`g_kalman` globals plus the
/// static TX rate timers of `esp_remote_id.c`.
#[derive(Debug)]
pub struct Scheduler {
    pub state: State,
    kalman: Kalman3d,
    last_tx_wifi_bcn: u64,
    last_tx_wifi_nan: u64,
    last_tx_ble4: u64,
    last_tx_ble5: u64,
    nan_counter: u8,
    pub log_cycle: u32,
    patrol: Patrol,
}

/// Port of `rate_allowed()` in `esp_remote_id.c`.
pub fn rate_allowed(last_us: &mut u64, now_us: u64, rate_hz: f32) -> bool {
    if rate_hz <= 0.0 {
        return false;
    }
    let interval = (1_000_000.0 / rate_hz) as u64;
    if now_us.wrapping_sub(*last_us) >= interval {
        *last_us = now_us;
        return true;
    }
    false
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            state: State::default(),
            kalman: Kalman3d::default(),
            last_tx_wifi_bcn: 0,
            last_tx_wifi_nan: 0,
            last_tx_ble4: 0,
            last_tx_ble5: 0,
            nan_counter: 0,
            log_cycle: 0,
            patrol: Patrol::default(),
        }
    }

    /// Port of the standard/fallback binding in `esp_rid_set_config()`.
    pub fn apply_config(&mut self, config: &Config) {
        self.state.active_standard = hub::active_standard(config.region);
        self.state.standard_fallback = !hub::has_encoder(self.state.active_standard);
    }

    /// Port of `update_transmissions()` in `esp_remote_id.c`.
    fn update_transmissions(&mut self, config: &Config, now_us: u64, out: &mut impl Transmitter) -> bool {
        if !self.state.gps_valid && !config.bcast_powerup {
            return false;
        }
        if self.state.active_protocol == Protocol::Unknown {
            return false;
        }
        if config.options & OPT_IDENTITY_READY_GATE != 0 && !self.state.identity_ready {
            return false;
        }

        let mut tx = false;

        if config.tx_modes & TRANSMIT_WIFI_BCN != 0
            && rate_allowed(&mut self.last_tx_wifi_bcn, now_us, config.wifi_bcn_rate_hz)
        {
            out.wifi_bcn(&self.state.gps, &self.state.identity, config);
            self.state.wifi_bcn_count += 1;
            self.state.transmissions_count += 1;
            tx = true;
        }

        if config.tx_modes & TRANSMIT_WIFI_NAN != 0
            && rate_allowed(&mut self.last_tx_wifi_nan, now_us, config.wifi_nan_rate_hz)
        {
            out.wifi_nan(&self.state.gps, &self.state.identity, config, self.nan_counter);
            self.nan_counter = self.nan_counter.wrapping_add(1);
            self.state.wifi_nan_count += 1;
            self.state.transmissions_count += 1;
            tx = true;
        }

        if config.tx_modes & TRANSMIT_BLE4 != 0
            && rate_allowed(&mut self.last_tx_ble4, now_us, config.ble4_rate_hz)
        {
            out.ble4(&self.state.gps, &self.state.identity, config);
            self.state.ble4_count += 1;
            self.state.transmissions_count += 1;
            tx = true;
        }

        if config.tx_modes & TRANSMIT_BLE5 != 0
            && rate_allowed(&mut self.last_tx_ble5, now_us, config.ble5_rate_hz)
        {
            out.ble5(&self.state.gps, &self.state.identity, config);
            self.state.ble5_count += 1;
            self.state.transmissions_count += 1;
            tx = true;
        }

        tx
    }

    /// Port of one `rid_task` loop iteration.
    pub fn tick(&mut self, input: &InputSample, config: &Config, out: &mut impl Transmitter) -> TickOutcome {
        let cfg_opts = config.options;
        let proto = input.proto;

        self.state.active_protocol = proto;
        self.state.stats.ticks = self.state.stats.ticks.wrapping_add(1);

        // Primary parser, then DroneCAN as secondary input.
        let mut sample = input.gps;
        if sample.is_none() {
            if let Some(d) = input.dronecan {
                sample = Some(d);
                self.state.active_protocol = Protocol::None;
            }
        }

        let mut had_gps = false;

        if let Some(gd) = sample {
            if gd.latitude != 0.0 {
                let force_tx = (cfg_opts & OPT_FORCE_ARM_OK != 0) && gd.armed;

                if force_tx || gd.fix_type >= 2 {
                    had_gps = true;
                    self.state.gps = gd;
                    self.state.gps_valid = true;
                    self.state.last_update_ms = input.now_ms;
                    self.state.stats.gps_updates += 1;

                    // MAVLink arm status.
                    if proto == Protocol::Mavlink {
                        self.state.mavlink_armed = input.mavlink_armed.unwrap_or(false);
                        self.state.gps.armed = self.state.mavlink_armed;
                        if let Some(sysid) = input.mavlink_sysid {
                            self.state.mavlink_sysid = sysid;
                        }
                    }

                    // Identity: MAVLink relay, else build from config.
                    let have_mav_id =
                        proto == Protocol::Mavlink && input.mavlink_identity.is_some();
                    if have_mav_id {
                        let mav_id = input.mavlink_identity.unwrap();
                        if !mav_id.uas_id.c_is_empty() {
                            self.state.identity = mav_id;
                        } else {
                            self.build_identity_from_config(config, false);
                        }
                    } else {
                        self.build_identity_from_config(config, false);
                    }

                    // Takeoff capture (once at first 3D fix).
                    readiness::maybe_capture_takeoff(
                        &mut self.state,
                        gd.fix_type,
                        gd.latitude,
                        gd.longitude,
                        gd.altitude_msl,
                    );

                    // MSP/NMEA do not provide relative altitude.
                    if (proto == Protocol::Msp || proto == Protocol::Nmea)
                        && self.state.takeoff_captured
                    {
                        readiness::derive_relative_altitude(&mut self.state);
                    }

                    // Operator location: fresh MAVLink (only when MAVLink is
                    // the active protocol, issue #24), else config.
                    if proto == Protocol::Mavlink {
                        if let Some(op) = input.mavlink_operator_location {
                            self.state.operator_lat = op.lat;
                            self.state.operator_lon = op.lon;
                            self.state.operator_alt = op.alt;
                            self.state.operator_position_updated_ms = input.now_ms;
                            self.state.operator_location_type = 1;
                            self.state.gps.operator_lat = op.lat;
                            self.state.gps.operator_lon = op.lon;
                            self.state.gps.operator_alt = op.alt;
                        } else {
                            self.state.gps.operator_lat = config.operator_lat;
                            self.state.gps.operator_lon = config.operator_lon;
                            self.state.gps.operator_alt = config.operator_alt;
                        }
                    } else {
                        // Non-MAVLink protocols (MSP/NMEA/None/Auto): the
                        // 30 s MAVLink freshness window could feed stale
                        // operator coordinates, so only the configured
                        // operator position is used.
                        self.state.gps.operator_lat = config.operator_lat;
                        self.state.gps.operator_lon = config.operator_lon;
                        self.state.gps.operator_alt = config.operator_alt;
                    }

                    if cfg_opts & OPT_DONT_SAVE_BASIC_ID != 0 {
                        self.state.identity.uas_id[0] = 0;
                        self.state.identity.uas_id_2[0] = 0;
                    }

                    readiness::update_identity_ready(&mut self.state, cfg_opts, config.region);
                } else {
                    self.state.stats.gps_discarded += 1;
                }
            } else {
                self.state.stats.gps_discarded += 1;
            }
        } else if cfg_opts & OPT_DEMO_MODE != 0 {
            // Demo mode: synthesize a patrol trajectory.
            self.patrol.tick(&mut self.state.gps);
            self.state.gps_valid = true;
            had_gps = true;
            self.state.last_update_ms = input.now_ms;
            self.state.active_protocol = Protocol::None;

            self.build_identity_from_config(config, true);
            self.state.gps.operator_lat = config.operator_lat;
            self.state.gps.operator_lon = config.operator_lon;
            self.state.gps.operator_alt = config.operator_alt;

            self.state.identity_ready = true;
        }

        // Kalman filter.
        let kalman_en = (cfg_opts & OPT_KALMAN_FILTER != 0) && (cfg_opts & OPT_DEMO_MODE == 0);
        let now_us = if kalman_en { input.now_us } else { 0 };
        if kalman_en {
            if had_gps {
                if let Some(gd) = sample {
                    if gd.latitude != 0.0 && gd.fix_type >= 2 {
                        self.kalman
                            .update(gd.latitude, gd.longitude, gd.altitude_msl, now_us);
                    }
                }
            }

            self.kalman.predict(now_us);

            if self.kalman.valid_age(now_us) {
                let o = self.kalman.get();
                self.state.gps.latitude = o.latitude;
                self.state.gps.longitude = o.longitude;
                self.state.gps.altitude_msl = o.altitude;
                self.state.gps.speed = o.speed;
                self.state.gps.speed_vertical = o.climb;
                self.state.gps.heading = o.heading;
                self.state.gps_valid = true;
            } else if !had_gps {
                self.state.gps_valid = false;
            }
        }

        // Absolute GPS timeout (independent of Kalman predictions).
        let now_ms = input.now_ms;
        let mut gps_stale = false;
        if self.state.gps_valid && now_ms.wrapping_sub(self.state.last_update_ms) > GPS_STALE_TIMEOUT_MS {
            self.state.gps_valid = false;
            gps_stale = true;
        }

        // `update_transmissions()` runs unconditionally each tick: the
        // `bcast_powerup` gate inside decides whether to transmit without a
        // GPS fix. This fixes the C bug where `bcast_powerup` never fired
        // because the call was gated behind `had_gps`.
        let tx_fired = self.update_transmissions(config, input.now_us, out);

        let led = if config.lock_level >= 2 {
            LedState::Locked
        } else if cfg_opts & OPT_DEMO_MODE != 0 {
            LedState::Demo
        } else if self.state.gps_valid {
            LedState::GpsOk
        } else {
            LedState::NoGps
        };

        let ws2812_green = self.state.gps_valid;
        let lighting_armed = self.state.gps.armed;

        self.log_cycle += 1;

        TickOutcome {
            had_gps,
            tx_fired,
            gps_stale,
            led,
            ws2812_green,
            lighting_armed,
            periodic_status: self.log_cycle.is_multiple_of(100),
            periodic_system: self.log_cycle.is_multiple_of(500),
        }
    }

    /// Port of the identity-from-config build in `rid_task`. `demo` mirrors
    /// the demo branch which sets one fewer field (`uas_id_2` untouched).
    fn build_identity_from_config(&mut self, config: &Config, demo: bool) {
        let ident = &mut self.state.identity;
        ident.uas_id = config.uas_id;
        ident.operator_id = config.operator_id;
        ident.self_id_text = config.self_id_text;
        ident.id_type = config.id_type;
        ident.ua_type = config.ua_type;
        if !demo {
            ident.uas_id_2 = config.uas_id_2;
            ident.id_type_2 = config.id_type_2;
            ident.ua_type_2 = config.ua_type_2;
        }
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rid_interface::input::InputSample;
    use rid_interface::{
        fixed_str, CStr, GpsData, Identity, OperatorLocation, Region,
    };

    /// Recording transmitter: counts calls per channel.
    #[derive(Default)]
    struct Recorder {
        bcn: u32,
        nan: u32,
        ble4: u32,
        ble5: u32,
    }

    impl Transmitter for Recorder {
        fn wifi_bcn(&mut self, _g: &GpsData, _i: &Identity, _c: &Config) {
            self.bcn += 1;
        }
        fn wifi_nan(&mut self, _g: &GpsData, _i: &Identity, _c: &Config, _n: u8) {
            self.nan += 1;
        }
        fn ble4(&mut self, _g: &GpsData, _i: &Identity, _c: &Config) {
            self.ble4 += 1;
        }
        fn ble5(&mut self, _g: &GpsData, _i: &Identity, _c: &Config) {
            self.ble5 += 1;
        }
    }

    fn sample(ms: u32, gps: Option<GpsData>) -> InputSample {
        InputSample {
            proto: Protocol::Nmea,
            gps,
            ..InputSample::new(ms, (ms as u64) * 1000)
        }
    }

    fn fix() -> GpsData {
        GpsData {
            latitude: 45.5,
            longitude: 9.2,
            altitude_msl: 120.0,
            fix_type: 4,
            satellites: 12,
            armed: true,
            ..GpsData::default()
        }
    }

    #[test]
    fn gps_fix_sets_state_and_transmits() {
        let mut s = Scheduler::new();
        s.state.identity.uas_id = fixed_str("TEST-UAS-123");
        let cfg = Config::default();
        let mut rec = Recorder::default();
        let out = s.tick(&sample(1000, Some(fix())), &cfg, &mut rec);
        assert!(out.had_gps);
        assert!(out.tx_fired);
        assert!(s.state.gps_valid);
        assert_eq!(out.led, LedState::GpsOk);
        assert_eq!(s.state.gps.latitude, 45.5);
        assert_eq!(s.state.transmissions_count, 1);
        assert_eq!(rec.bcn, 1);
        assert_eq!(rec.ble4, 0); // only WIFI_BCN default
        // Default identity carries the config placeholder.
        assert!(s.state.identity.uas_id.c_starts_with("ESP32-RID-"));
        // Takeoff captured at 3D fix.
        assert!(s.state.takeoff_captured);
        assert_eq!(s.state.takeoff_lat, 45.5);
    }

    #[test]
    fn no_data_no_tx_no_gps() {
        let mut s = Scheduler::new();
        // `bcast_powerup` defaults to true; disable it to verify that without
        // GPS (and without the power-up gate) nothing is transmitted.
        let cfg = Config {
            bcast_powerup: false,
            ..Config::default()
        };
        let mut rec = Recorder::default();
        let out = s.tick(&sample(1000, None), &cfg, &mut rec);
        assert!(!out.had_gps);
        assert!(!out.tx_fired);
        assert!(!s.state.gps_valid);
        assert_eq!(out.led, LedState::NoGps);
    }

    #[test]
    fn bcast_powerup_transmits_without_gps() {
        // Regression test for the C bug (#21): `bcast_powerup` must fire even
        // with no GPS data because `update_transmissions()` now runs every
        // tick instead of only when `had_gps` is true.
        let mut s = Scheduler::new();
        let cfg = Config {
            bcast_powerup: true,
            ..Config::default()
        };
        let mut rec = Recorder::default();
        let out = s.tick(&sample(1000, None), &cfg, &mut rec);
        assert!(!out.had_gps);
        assert!(!s.state.gps_valid);
        assert!(out.tx_fired);
        assert_eq!(rec.bcn, 1);
    }

    #[test]
    fn demo_mode_synthesizes_gps() {
        let mut s = Scheduler::new();
        let cfg = Config {
            options: OPT_DEMO_MODE,
            ..Config::default()
        };
        let mut rec = Recorder::default();
        let out = s.tick(&sample(1000, None), &cfg, &mut rec);
        assert!(out.had_gps);
        assert!(out.tx_fired);
        assert!(s.state.gps_valid);
        assert_eq!(s.state.active_protocol, Protocol::None);
        assert_eq!(out.led, LedState::Demo);
        // The C demo branch does not capture takeoff.
        assert!(!s.state.takeoff_captured);
    }

    #[test]
    fn dronecan_is_secondary_input() {
        let mut s = Scheduler::new();
        let cfg = Config::default();
        let mut rec = Recorder::default();
        let mut inp = sample(1000, None);
        inp.dronecan = Some(fix());
        let out = s.tick(&inp, &cfg, &mut rec);
        assert!(out.had_gps);
        assert_eq!(s.state.active_protocol, Protocol::None);
        assert_eq!(s.state.gps.latitude, 45.5);
    }

    #[test]
    fn identity_gate_blocks_transmission() {
        let mut s = Scheduler::new();
        let cfg = Config {
            options: OPT_IDENTITY_READY_GATE,
            ..Config::default()
        };
        let mut rec = Recorder::default();
        // Placeholder identity -> not ready -> no TX despite fresh fix.
        let out = s.tick(&sample(1000, Some(fix())), &cfg, &mut rec);
        assert!(out.had_gps);
        assert!(!out.tx_fired);
        assert!(!s.state.identity_ready);
        assert_eq!(rec.bcn, 0);
    }

    #[test]
    fn absolute_gps_timeout_expires() {
        let mut s = Scheduler::new();
        let cfg = Config::default();
        let mut rec = Recorder::default();
        // Fresh fix at t=1000.
        s.tick(&sample(1000, Some(fix())), &cfg, &mut rec);
        assert!(s.state.gps_valid);
        // No new fix: after > 10 s the fix is stale.
        let out = s.tick(&sample(1000 + GPS_STALE_TIMEOUT_MS + 1, None), &cfg, &mut rec);
        assert!(out.gps_stale);
        assert!(!s.state.gps_valid);
        assert_eq!(out.led, LedState::NoGps);
    }

    #[test]
    fn kalman_smooths_and_validity_expires() {
        let mut s = Scheduler::new();
        let cfg = Config {
            options: OPT_KALMAN_FILTER,
            ..Config::default()
        };
        let mut rec = Recorder::default();
        let mut g1 = fix();
        g1.latitude = 45.50001;
        let mut g2 = fix();
        g2.latitude = 45.50002;
        s.tick(&sample(1000, Some(g1)), &cfg, &mut rec);
        s.tick(&sample(2000, Some(g2)), &cfg, &mut rec);
        assert!(s.state.gps_valid);
        // Now stop feeding: within Kalman timeout predictions keep it valid.
        s.tick(&sample(3000, None), &cfg, &mut rec);
        assert!(s.state.gps_valid);
        // Beyond the Kalman timeout (3 s) with no fix it drops.
        s.tick(&sample(3000 + 3_000_001, None), &cfg, &mut rec);
        assert!(!s.state.gps_valid);
    }

    #[test]
    fn mavlink_extras_merge() {
        let mut s = Scheduler::new();
        let cfg = Config::default();
        let mut rec = Recorder::default();
        let mut inp = sample(1000, Some(fix()));
        inp.proto = Protocol::Mavlink;
        inp.mavlink_armed = Some(true);
        inp.mavlink_sysid = Some(42);
        inp.mavlink_identity = Some(Identity {
            uas_id: fixed_str("MAV-UAS-1"),
            ..Identity::default()
        });
        inp.mavlink_operator_location = Some(OperatorLocation {
            lat: 44.0,
            lon: 8.0,
            alt: 5.0,
        });
        s.tick(&inp, &cfg, &mut rec);
        assert!(s.state.mavlink_armed);
        assert!(s.state.gps.armed);
        assert_eq!(s.state.mavlink_sysid, 42);
        assert!(s.state.identity.uas_id.c_starts_with("MAV-UAS-1"));
        assert_eq!(s.state.operator_location_type, 1);
        assert_eq!(s.state.gps.operator_lat, 44.0);
        assert_eq!(s.state.operator_position_updated_ms, 1000);
    }

    #[test]
    fn non_mavlink_protocol_ignores_mavlink_operator_location() {
        // Issue #24: the MAVLink operator location (30 s freshness window)
        // must only be applied when MAVLink is the active protocol. For
        // MSP/NMEA etc. the configured operator position is used instead,
        // so a stale MAVLink coordinate can't leak into the transmission.
        let mut s = Scheduler::new();
        let cfg = Config {
            operator_lat: 40.5,
            operator_lon: 9.25,
            operator_alt: 12.0,
            ..Config::default()
        };
        let mut rec = Recorder::default();
        let mut inp = sample(1000, Some(fix()));
        inp.proto = Protocol::Msp;
        inp.mavlink_operator_location = Some(OperatorLocation {
            lat: 44.0,
            lon: 8.0,
            alt: 5.0,
        });
        s.tick(&inp, &cfg, &mut rec);
        // Fresh fix so the operator branch above ran.
        assert!(s.state.gps_valid);
        // MAVLink location must NOT be applied for MSP.
        assert_eq!(s.state.gps.operator_lat, 40.5);
        assert_eq!(s.state.gps.operator_lon, 9.25);
        assert_eq!(s.state.operator_location_type, 0);
        assert_eq!(s.state.operator_position_updated_ms, 0);

        // Same input with MAVLink active: MAVLink location is applied.
        let mut s2 = Scheduler::new();
        let mut rec2 = Recorder::default();
        inp.proto = Protocol::Mavlink;
        s2.tick(&inp, &cfg, &mut rec2);
        assert_eq!(s2.state.gps.operator_lat, 44.0);
        assert_eq!(s2.state.operator_location_type, 1);
        assert_eq!(s2.state.operator_position_updated_ms, 1000);
    }

    #[test]
    fn rate_limiting_respects_rates() {
        let mut s = Scheduler::new();
        let cfg = Config {
            tx_modes: TRANSMIT_WIFI_BCN,
            wifi_bcn_rate_hz: 1.0,
            ..Config::default()
        };
        let mut rec = Recorder::default();
        s.tick(&sample(1000, Some(fix())), &cfg, &mut rec);
        assert_eq!(rec.bcn, 1);
        // 500 ms later: not yet due.
        s.tick(&sample(1500, Some(fix())), &cfg, &mut rec);
        assert_eq!(rec.bcn, 1);
        // 600 ms more: due again.
        s.tick(&sample(2100, Some(fix())), &cfg, &mut rec);
        assert_eq!(rec.bcn, 2);
    }

    #[test]
    fn locked_led_wins_over_demo() {
        let mut s = Scheduler::new();
        let cfg = Config {
            options: OPT_DEMO_MODE,
            lock_level: 2,
            ..Config::default()
        };
        let mut rec = Recorder::default();
        let out = s.tick(&sample(1000, None), &cfg, &mut rec);
        assert_eq!(out.led, LedState::Locked);
    }

    #[test]
    fn apply_config_binds_standard() {
        let mut s = Scheduler::new();
        let cfg = Config {
            region: Region::Chn,
            ..Config::default()
        };
        s.apply_config(&cfg);
        assert_eq!(s.state.active_standard, crate::hub::active_standard(Region::Chn));
        assert!(s.state.standard_fallback);
    }

    #[test]
    fn nmea_derives_relative_altitude() {
        let mut s = Scheduler::new();
        let cfg = Config::default();
        let mut rec = Recorder::default();
        let mut g = fix();
        g.altitude_msl = 200.0;
        // Capture takeoff at 100 m first.
        let mut g0 = fix();
        g0.altitude_msl = 100.0;
        s.tick(&sample(1000, Some(g0)), &cfg, &mut rec);
        assert!(s.state.takeoff_captured);
        s.tick(&sample(2000, Some(g)), &cfg, &mut rec);
        assert_eq!(s.state.gps.altitude_relative, 100.0);
    }

    #[test]
    fn dont_save_basic_id_clears_ids() {
        let mut s = Scheduler::new();
        let cfg = Config {
            options: OPT_DONT_SAVE_BASIC_ID,
            ..Config::default()
        };
        let mut rec = Recorder::default();
        s.tick(&sample(1000, Some(fix())), &cfg, &mut rec);
        assert!(s.state.identity.uas_id.c_is_empty());
        assert!(s.state.identity.uas_id_2.c_is_empty());
    }
}
