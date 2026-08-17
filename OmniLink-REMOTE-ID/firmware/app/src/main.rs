//! Host demo of the Fase 5 application assembly: wires a mock input source
//! (`GpsSource`) and a counting transmitter (`Transmitter`) into the
//! `Controller`, runs the loop and prints the three API endpoint payloads
//! plus the config-lifecycle operations. On real hardware `bsp-esp32` plugs
//! its parsers/radios into the same `Controller`.

use app::capabilities::Capabilities;
use app::controller::Controller;
use rid_core::scheduler::LedState;
use rid_interface::{Config, GpsData, GpsSource, Identity, InputSample, Protocol, Transmitter};

/// Scripted NMEA-like source: a fresh fix every 100 ms, with a 1 s gap every
/// 3 s to exercise the stale path.
struct MockSource {
    tick: u64,
}

impl GpsSource for MockSource {
    fn sample(&mut self, _config: &Config, now_ms: u32, now_us: u64) -> InputSample {
        let mut s = InputSample::new(now_ms, now_us);
        s.proto = Protocol::Nmea;
        let gap = self.tick % 30 >= 20;
        if !gap {
            s.gps = Some(GpsData {
                latitude: 45.4642 + (self.tick as f64) * 1e-5,
                longitude: 9.19,
                altitude_msl: 150.0,
                fix_type: 4,
                satellites: 16,
                armed: true,
                ..GpsData::default()
            });
        }
        self.tick += 1;
        s
    }
}

/// Counting transmitter (the BSP would encode + push to the radio).
#[derive(Default)]
struct CountTx {
    bcn: u32,
    nan: u32,
    ble4: u32,
    ble5: u32,
}

impl Transmitter for CountTx {
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

fn led_name(led: LedState) -> &'static str {
    match led {
        LedState::Locked => "LOCKED",
        LedState::Demo => "DEMO",
        LedState::GpsOk => "GPS_OK",
        LedState::NoGps => "NO_GPS",
    }
}

fn main() {
    println!("=== Remote ID app (Fase 5): port of esp_remote_id.c glue ===");

    let mut ctl = Controller::new();

    // esp_rid_init: placeholder IDs are replaced from the WiFi MAC.
    let mac = [0xDE, 0xAD, 0xBE, 0xEF, 0xAB, 0xCD];
    ctl.derive_default_ids(&mac);
    println!("\n[init] derived IDs from MAC {:02X}:{:02X}", mac[4], mac[5]);

    // esp_rid_set_config: switch region and watch the standard binding.
    let mut cfg = ctl.config().clone();
    cfg.region = rid_interface::Region::Eur;
    let out = ctl.set_config(&cfg);
    println!(
        "[set_config] region=EUR standard={} fallback={} reinit={}",
        out.active_standard as u8, out.standard_fallback, out.protocol_reinit_required
    );

    // Loop: 30 ticks at 100 ms.
    let mut source = MockSource { tick: 0 };
    let mut tx = CountTx::default();
    let mut last_led: Option<LedState> = None;
    println!("\n  t[ms]  valid  tx  led     proto  standard");
    for i in 0..30u32 {
        let now_ms = i * 100;
        let input = source.sample(&app::core_config(ctl.config()), now_ms, now_ms as u64 * 1000);
        let outcome = ctl.step(&input, &mut tx);
        if outcome.led != last_led.unwrap_or(LedState::NoGps) || i % 5 == 0 {
            println!(
                "  {:>4}  {:<5} {:<3} {:<7} {:?}  {}",
                now_ms,
                ctl.state().gps_valid,
                outcome.tx_fired,
                led_name(outcome.led),
                ctl.state().active_protocol,
                outcome.periodic_status,
            );
            last_led = Some(outcome.led);
        }
    }
    println!(
        "\n[loop] transmissions={} bcn={} nan={} ble4={} ble5={}",
        ctl.state().transmissions_count, tx.bcn, tx.nan, tx.ble4, tx.ble5
    );

    // /api/status
    println!("\n[GET /api/status]");
    println!("  {}", ctl.status_json());

    // /api/config
    println!("\n[GET /api/config]");
    println!("  {}", ctl.config_json());

    // /api/capabilities
    println!("\n[GET /api/capabilities]");
    println!("  {}", ctl.capabilities_json());
    let caps = Capabilities::build();
    println!(
        "  (inputs={} regions={} standards={} options={} tx_modes={})",
        caps.inputs.len(),
        caps.regions.len(),
        caps.standards.len(),
        caps.options.len(),
        caps.tx_modes.len(),
    );

    // esp_rid_factory_reset: keys survive, everything else resets.
    let mut locked = ctl.config().clone();
    locked.lock_level = 2;
    locked.public_keys[0] = rid_interface::key_str("ED25519KEY");
    ctl.set_config(&locked);
    ctl.factory_reset();
    println!(
        "\n[factory_reset] lock_level={} region={:?} key0_preserved={}",
        ctl.config().lock_level,
        ctl.config().region,
        rid_app::config::cstr(&ctl.config().public_keys[0]) == "ED25519KEY",
    );
}
