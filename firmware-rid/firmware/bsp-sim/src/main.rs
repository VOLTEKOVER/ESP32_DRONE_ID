//! Host simulator: runs the central processing on a PC without any
//! hardware, exercising the hourglass hub, the identity readiness gate and
//! the full scheduler loop exactly as the firmware would.

use rid_core::hub;
use rid_core::kalman::Kalman3d;
use rid_core::readiness;
use rid_core::scheduler::{LedState, Scheduler, GPS_STALE_TIMEOUT_MS};
use rid_interface::{
    fixed_str, Config, CStr, GpsData, Identity, InputSample, OperatorLocation, Protocol, Region,
    Transmitter, OPT_IDENTITY_READY_GATE, OPT_KALMAN_FILTER,
};

/// Simulated WiFi interface MAC (like `wifi_tx_init` reads from eFuse).
const MAC: [u8; 6] = [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01];

fn build_identity() -> Identity {    Identity {
        uas_id: fixed_str("TEST-UAS-123"),
        operator_id: fixed_str("OP-001234"),
        self_id_text: fixed_str("Test flight"),
        uas_id_2: fixed_str("TEST-UAS-2"),
        ..Identity::default()
    }
}fn build_gps() -> GpsData {
    GpsData {
        latitude: 45.4642,
        longitude: 9.19,
        altitude_msl: 150.0,
        altitude_relative: 130.0,
        speed: 12.5,
        speed_vertical: 0.4,
        heading: 90,
        fix_type: 4,
        satellites: 16,
        armed: true,
        ..GpsData::default()
    }
}

fn run_region(region: Region) {
    let cfg = Config {
        region,
        ..Config::default()
    };
    let id = build_identity();
    let gps = build_gps();

    println!(
        "region={:<4} standard={}  fallback={}",
        hub::region_name(region),
        hub::standard_name(hub::active_standard(region)),
        !hub::has_encoder(hub::active_standard(region)),
    );

    let b = hub::build_uas(&gps, &id, region);
    println!(
        "  -> gated: op_id={} self_id={} uas_id_2={}",
        !b.gated_identity.operator_id.c_is_empty(),
        !b.gated_identity.self_id_text.c_is_empty(),
        !b.gated_identity.uas_id_2.c_is_empty(),
    );

    // Readiness gate on the placeholder config identity.
    let mut state = rid_interface::State {
        identity: Identity {
            uas_id: cfg.uas_id,
            operator_id: cfg.operator_id,
            ..Identity::default()
        },
        gps,
        ..rid_interface::State::default()
    };
    readiness::update_identity_ready(&mut state, OPT_IDENTITY_READY_GATE, region);
    println!("  -> identity_ready={}", state.identity_ready);
}

/// Prints a one-line summary per transport (like the BSP would TX).
struct PrintTx;

impl Transmitter for PrintTx {
    fn wifi_bcn(&mut self, gps: &GpsData, identity: &Identity, _c: &Config) {
        println!(
            "    [WIFI_BCN] uas={:?} lat={:.5} lon={:.5} alt={:.1}",
            &identity.uas_id[..identity.uas_id.c_len()],
            gps.latitude,
            gps.longitude,
            gps.altitude_msl,
        );
    }
    fn wifi_nan(&mut self, gps: &GpsData, identity: &Identity, _c: &Config, counter: u8) {
        println!(
            "    [WIFI_NAN] n={} uas={:?} lat={:.5}",
            counter,
            &identity.uas_id[..identity.uas_id.c_len()],
            gps.latitude,
        );
    }
    fn ble4(&mut self, gps: &GpsData, identity: &Identity, _c: &Config) {
        println!(
            "    [BLE4] uas={:?} lon={:.5}",
            &identity.uas_id[..identity.uas_id.c_len()],
            gps.longitude,
        );
    }
    fn ble5(&mut self, gps: &GpsData, identity: &Identity, _c: &Config) {
        println!(
            "    [BLE5] uas={:?} hdg={}",
            &identity.uas_id[..identity.uas_id.c_len()],
            gps.heading,
        );
    }
}

/// Transmitter that runs the real ASTM encode chain via `out-astm`
/// (encode side of `wifi_tx_transmit` / `ble_tx_transmit_legacy`).
struct AstmTx {
    ble4_rotation: u8,
    wifi_counter: u8,
    mono_us: u64,
}

impl AstmTx {
    fn report(&self, out: &out_astm::BuildOutcome) {
        println!(
            "      std={} fallback={} msgs: b2={} loc={} self={} sys={} op={} auth={}",
            hub::standard_name(out.standard),
            out.fallback,
            out.uas.basic_id_valid.iter().filter(|&&v| v != 0).count(),
            out.uas.location_valid,
            out.uas.self_id_valid,
            out.uas.system_valid,
            out.uas.operator_id_valid,
            out.uas.auth_valid.iter().filter(|&&v| v != 0).count(),
        );
    }
}

impl Transmitter for AstmTx {
    fn wifi_bcn(&mut self, gps: &GpsData, identity: &Identity, config: &Config) {
        let out = out_astm::build_uas(gps, identity, config, None);
        let mut buf = [0u8; 1024];
        match out_astm::wifi::build_beacon_frame(
            &out.uas,
            &MAC,
            b"ESP-RID",
            100,
            self.wifi_counter,
            self.mono_us,
            &mut buf,
        ) {
            Ok(len) => {
                println!(
                    "    [WIFI_BCN] len={} ctr={} frame={:02X?}",
                    len, self.wifi_counter, &buf[..len]
                );
                self.wifi_counter = self.wifi_counter.wrapping_add(1);
            }
            Err(e) => println!("    [WIFI_BCN] frame error: {e:?}"),
        }
        self.mono_us += 500_000;
    }
    fn wifi_nan(&mut self, gps: &GpsData, identity: &Identity, config: &Config, counter: u8) {
        let out = out_astm::build_uas(gps, identity, config, None);
        let mut buf = [0u8; 1024];
        match out_astm::wifi::build_nan_action_frame(&out.uas, &MAC, counter, &mut buf) {
            Ok(len) => {
                println!("    [WIFI_NAN] len={} ctr={} frame={:02X?}", len, counter, &buf[..len]);
            }
            Err(e) => println!("    [WIFI_NAN] frame error: {e:?}"),
        }
    }
    fn ble4(&mut self, gps: &GpsData, identity: &Identity, config: &Config) {
        let out = out_astm::build_uas(gps, identity, config, None);
        self.report(&out);
        match out_astm::ble4::next_message(&out.uas, &mut self.ble4_rotation) {
            Some(m) => println!(
                "    [BLE4] rot={} valid={} type={:#04x} {:02X?}",
                self.ble4_rotation.wrapping_sub(1),
                out_astm::ble4::count_valid(&out.uas),
                opendroneid_sys::decode_message_type(m.0[0]),
                m.as_bytes(),
            ),
            None => println!("    [BLE4] no valid message"),
        }
    }
    fn ble5(&mut self, gps: &GpsData, identity: &Identity, _c: &Config) {
        println!(
            "    [BLE5] uas={:?} hdg={}",
            &identity.uas_id[..identity.uas_id.c_len()],
            gps.heading,
        );
    }
}

/// End-to-end output demo: hub -> UAS data -> WiFi pack / BLE4 rotation,
/// then a decode roundtrip through the official C library.
fn run_output() {
    println!("\n--- Output: ASTM encode chain (port of odid_common/wifi.c/ble_tx.c) ---");

    let cfg = Config {
        region: Region::Eur,
        ..Config::default()
    };
    let id = build_identity();
    let gps = build_gps();

    let out = out_astm::build_uas(&gps, &id, &cfg, None);
    let mut buf = [0u8; out_astm::pack::MAX_PACK_LEN];
    let len = out_astm::pack::build_pack(&out.uas, &mut buf).unwrap();
    println!("std={} fallback={} pack len={} byte0={:#04x} n={}", hub::standard_name(out.standard), out.fallback, len, buf[0], buf[2]);

    // Decode roundtrip through the C library (odid_message_process_pack path).
    let mut enc = opendroneid_sys::MessagePackEncoded {
        proto_version_message_type: buf[0],
        single_message_size: buf[1],
        msg_pack_size: buf[2],
        messages: [opendroneid_sys::MessageEncoded([0; opendroneid_sys::ODID_MESSAGE_SIZE]);
            opendroneid_sys::ODID_PACK_MAX_MESSAGES],
    };
    for i in 0..opendroneid_sys::ODID_PACK_MAX_MESSAGES {
        let start = 3 + i * opendroneid_sys::ODID_MESSAGE_SIZE;
        enc.messages[i]
            .0
            .copy_from_slice(&buf[start..start + opendroneid_sys::ODID_MESSAGE_SIZE]);
    }
    let mut back = opendroneid_sys::init_uas_data();
    let ret = unsafe { opendroneid_sys::decodeMessagePack(&mut back, &enc) };
    let op_id: String = back
        .operator_id
        .operator_id
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8 as char)
        .collect();
    println!(
        "C decode: ret={} loc_valid={} lat={:.5} lon={:.5} alt={:.1} op_id={:?}",
        ret,
        back.location_valid,
        back.location.latitude,
        back.location.longitude,
        back.location.altitude_geo,
        op_id,
    );

    let mut tx = AstmTx { ble4_rotation: 0, wifi_counter: 0, mono_us: 0 };
    println!("wifi beacon frame + nan action frame (2 packets each):");
    tx.wifi_bcn(&gps, &id, &cfg);
    tx.wifi_nan(&gps, &id, &cfg, 0x00);
    tx.wifi_bcn(&gps, &id, &cfg);
    tx.wifi_nan(&gps, &id, &cfg, 0x01);

    println!("ble4 rotation (one 25-byte message per advertisement):");
    for _ in 0..5 {
        tx.ble4(&gps, &id, &cfg);
    }

    // GB 42590 has no encoder yet: the stub reports NotImplemented.
    match out_astm::stubs::encode_gb42590(&out.uas, &mut buf) {
        Err(out_astm::stubs::EncodeError::NotImplemented) => {
            println!("gb42590 stub: NotImplemented (hub falls back to ASTM)")
        }
        _ => unreachable!(),
    }
}

fn run_scheduler() {
    println!("\n--- Scheduler (port of rid_task) ---");

    let mut sched = Scheduler::new();
    let cfg = Config {
        options: OPT_KALMAN_FILTER,
        tx_modes: rid_interface::TRANSMIT_WIFI_BCN | rid_interface::TRANSMIT_BLE5,
        wifi_bcn_rate_hz: 2.0,
        ble5_rate_hz: 1.0,
        region: Region::Eur,
        ..Config::default()
    };
    sched.apply_config(&cfg);

    let id = build_identity();
    sched.state.identity = id;
    let mut tx = PrintTx;

    for ms in (0..20).map(|i| i * 500) {
        let mut gps = build_gps();
        gps.latitude += ms as f64 * 1e-5;
        gps.longitude += ms as f64 * 5e-6;

        // Gap after 6 s to exercise the Kalman + stale path.
        let has_fix = ms <= 6000;
        let mut input = InputSample::new(ms, ms as u64 * 1000);
        input.proto = Protocol::Nmea;
        input.gps = if has_fix { Some(gps) } else { None };
        input.mavlink_operator_location = Some(OperatorLocation {
            lat: 41.9,
            lon: 12.5,
            alt: 0.0,
        });

        let out = sched.tick(&input, &cfg, &mut tx);
        let led = match out.led {
            LedState::Locked => "LOCKED",
            LedState::Demo => "DEMO",
            LedState::GpsOk => "GPS_OK",
            LedState::NoGps => "NO_GPS",
        };
        println!(
            "  t={:>4}ms  valid={:<5} stale={:<5} tx={:<5} led={:<7} proto={:?} std={}",
            ms,
            sched.state.gps_valid,
            out.gps_stale,
            out.tx_fired,
            led,
            sched.state.active_protocol,
            hub::standard_name(sched.state.active_standard),
        );
        if ms == GPS_STALE_TIMEOUT_MS + 1000 {
            println!("    (absolute GPS timeout fired)");
        }
    }

    println!(
        "  totals: transmissions={} bcn={} nan={} ble4={} ble5={}",
        sched.state.transmissions_count,
        sched.state.wifi_bcn_count,
        sched.state.wifi_nan_count,
        sched.state.ble4_count,
        sched.state.ble5_count,
    );
}

fn main() {
    println!("=== Remote ID hub simulator (port of rid_output.c / esp_remote_id.c) ===\n");
    run_region(Region::Eur);
    run_region(Region::Chn);
    run_region(Region::Faa);

    // Kalman demo, mirroring the main-loop usage (update -> predict -> get).
    println!("\n--- Kalman (port of rid_kalman.c) ---");
    let mut k = Kalman3d::default();
    for i in 0..10u64 {
        let now_us = i * 100_000;
        k.update(
            45.4642 + i as f64 * 1e-4,
            9.1900 + i as f64 * 1e-4,
            150.0 + i as f32,
            now_us,
        );
        k.predict(now_us + 1);
    }
    let o = k.get();
    println!(
        "valid={} lat={:.6} lon={:.6} alt={:.1} speed={:.2} m/s climb={:.2} heading={}",
        k.valid_age(1_000_000),
        o.latitude,
        o.longitude,
        o.altitude,
        o.speed,
        o.climb,
        o.heading,
    );

    run_scheduler();
    run_output();
}
