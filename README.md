<p align="center">
  <img src="docs/images/logo%20con%20scritta.svg" alt="OmniRID" width="420">
</p>

<p align="center">
  <a href="https://github.com/VOLTEKOVER/OmniRID-Universal-Drone-ID/actions/workflows/rid-rust-ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/VOLTEKOVER/OmniRID-Universal-Drone-ID/rid-rust-ci.yml?logo=github" alt="CI"></a>
  <a href="https://VOLTEKOVER.github.io/OmniRID-Universal-Drone-ID/"><img src="https://img.shields.io/badge/BETA-000?logo=esphome&color=f9a825" alt="BETA"></a>
  <a href="https://github.com/VOLTEKOVER/OmniRID-Universal-Drone-ID/releases"><img src="https://img.shields.io/github/v/release/VOLTEKOVER/OmniRID-Universal-Drone-ID?include_prereleases&logo=github&label=version" alt="Release"></a>
  <a href="https://www.espressif.com/"><img src="https://img.shields.io/badge/ESP32%20|%20S3%20|%20C6-000?logo=espressif" alt="Platform"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/VOLTEKOVER/OmniRID-Universal-Drone-ID?color=blue" alt="License"></a>
</p>

<p align="center">
  <img src="docs/images/ardupilot_logo.webp" height="28" alt="ArduPilot">&nbsp;&nbsp;
  <img src="docs/images/betaflight_logo.svg" height="28" alt="Betaflight">&nbsp;&nbsp;
  <img src="docs/images/inav_logo.png" height="28" alt="INAV">
</p>

<p>
  Omnirid is a Open Source Drone ID transmitter compatible with all protocols for now supporting 
  
  <b>ASTM F3411-22a</b> / <b>ASD-STAN prEN 4709-002</b> / <b>GB 42590-2023</b> in output .<br>
  and in input <b>MAVLink · MSP · NMEA · DroneCAN</b> from any flight controller.<br>

  Broadcasts via <b>WiFi Beacon + NAN + BLE 4.0/5.0</b>,

  Written in <b>Rust🦀</b>
</p>

<p align="center">
  <a href="https://VOLTEKOVER.github.io/OmniRID-Universal-Drone-ID/"><b>Wiki & Demo</b></a>&nbsp;&nbsp;
  <a href="https://VOLTEKOVER.github.io/OmniRID-Universal-Drone-ID/config(demo).html"><b>Live Demo</b></a>
</p>

---

> [!CAUTION]
> ## ⚠️ No release until the security audit is complete
>
> **This firmware has not been security tested yet.**
> No official release will be published until a full security audit
> (penetration testing, firmware analysis, protocol fuzzing) has been
> performed and all critical/high findings resolved.
>
> **Do not use on production aircraft.** Development and ground testing only.
> Details in [SECURITY.md](SECURITY.md).

---

## Table of Contents

1. [Quick Start](#quick-start)
2. [Features](#features)
3. [Communication](#communication)
4. [Hardware](#hardware)
5. [Build](#build)
6. [Project Structure](#project-structure)
7. [Development Status](#development-status)
8. [Testing](#testing)
9. [Documentation](#documentation)
10. [Security](#security)
11. [Contributing](#contributing)
12. [License & Ecosystem](#license--ecosystem)
13. [Next Steps](#next-steps)

---

## Quick Start


## Features

### Input (GPS from flight controller)

| Protocol | Format | Details |
|---|---|---|
| **MAVLink v2** | ArduPilot/PX4 | `GPS_RAW_INT`, `GLOBAL_POSITION_INT`, `HEARTBEAT`, `OPEN_DRONE_ID_*` |
| **MSP** | Betaflight/iNAV | `MSP_RAW_GPS` (106), `MSP_ATTITUDE` (108), `MSP_STATUS` (101) |
| **NMEA 0183** | Direct GPS module | `$GPGGA`, `$GNGGA`, `$GPRMC`, `$GNRMC`, `$GPVTG`, `$GNVTG` |
| **DroneCAN** | CAN bus | `uavcan.equipment.gnss.Fix2` (TWAI decode) |

### Output (RID broadcast)

| Medium | Standard | Range |
|---|---|---|
| **WiFi Beacon** | IEEE 802.11 Mgmt | ~100 m (typical) |
| **WiFi NAN** | Service Discovery | ~100 m |
| **BLE 4.0** | Legacy advertising | ~50 m |
| **BLE 5.0** | Coded PHY (S3/C6) | ~200+ m (LR mode) |
|**LoRa**|(to-do)for advanced applications|~1000 m|

## WEB UI

<p align="center">
  <img src="docs/images/dashboard.png" alt="RID Hub Dashboard" width="820">
</p>

### CLI commands (after flashing)

```
> status              # Show GPS, TX rates, identity state
> config get uas_id   # Check config
> transmit 10         # Force 10 packets
> patrol              # Start demo GPS patrol
```

> No hardware yet? Try the **[offline demo](https://VOLTEKOVER.github.io/OmniRID-Universal-Drone-ID/config(demo).html)** first — full simulation, no ESP32 required.

### Host build (Linux / macOS / Windows)

```bash
git clone https://github.com/VOLTEKOVER/OmniRID-Universal-Drone-ID.git
cd OmniRID-Universal-Drone-ID
cargo build --workspace          # builds all crates for the host
cargo test --workspace           # runs 312 tests
```

### ESP32 cross-build

Requires Rust nightly with the ESP32 target + the ESP-IDF SDK:

```bash
cargo +esp build --target xtensa-esp32-none-elf    --manifest-path firmware/Cargo.toml
cargo +esp build --target xtensa-esp32s3-none-elf  --manifest-path firmware/Cargo.toml
cargo +esp build --target riscv32imc-esp-none-elf   --manifest-path firmware/Cargo.toml
```

### Flash & connect

| Step | Action |
|:---:|---|
| 1 | Flash the firmware to your ESP32 via USB |
| 2 | Connect to WiFi network **ESP-RID** |
| 3 | Open `http://192.168.4.1` to configure |
| 4 | Run `rid-hub` for the ground station dashboard |

---

## Features

### Radio & protocols

| Feature | Details |
|---|---|
| **Broadcast** | WiFi Beacon (802.11 mgmt) + WiFi NAN + BLE 4.0 Legacy + BLE 5.0 Long Range (dual instance on S3/C6) |
| **Input protocols** | MAVLink v2 (ArduPilot/PX4), MSP (Betaflight/iNAV), NMEA, DroneCAN/CAN bus — auto-detected |
| **GPS source** | From flight controller (MAVLink/MSP/NMEA/CAN) or a direct GPS module, with takeoff-location capture |
| **Position filter** | 1D × 3 Kalman filter (lat/lon/alt) with velocity prediction, 3 s timeout |

### Security & compliance

| Feature | Details |
|---|---|
| **Authentication** | Ed25519 signing (ASTM F3411-22a compliant), 4 pages per broadcast cycle |
| **Lock levels** | 3 tiers: Normal / Ed25519 signed / eFuse permanent |
| **OTA updates** | WiFi AP with client-side SHA-256 verification (Web Crypto) + Ed25519 signature |
| **Hardening** | Safe JSON parsing, rate limiting (10 failed attempts/60s), bounded strings, `psa_crypto_init()` |
| **Configuration** | 70+ parameters: UAS ID, rates, power, public keys, auth, lock, lighting |

# Ground station (RID Hub)

| Component | Stack |
|---|---|
| **Decoder** | Pure JavaScript, ASTM F3411-22a parser |
| **Tracker** | Device tracking with 500-point trail, CSV/KML export |
| **Capture** | WiFi monitor mode + BLE scan + Serial USB (optional npm modules) |
| **UI** | React 19 + Ant Design 6 + Vite 8 + Leaflet |

---

## Hardware

### Minimum wiring (ESP32 + flight controller)

```
Flight Controller    ESP32 (or variant)
─────────────────    ─────────────────
TX (UART)       ->   GPIO16 (UART2 RX)
GND             ->   GND
5V (BEC)        ->   5V / VIN
```

> **NMEA tap**: add a **1 kΩ series resistor** on the tap line to prevent backfeed.

### Recommended: Seeed XIAO ESP32-C6 (zero-solder kit)

| Feature | Benefit |
|---|---|
| **21 × 17.8 mm** | Fits in any enclosure |
| **USB-C + LiPo charger** | Battery-ready, no soldering |
| **WiFi 6 + BT 5.3** | Better range & throughput |
| **802.15.4 capable** | Future mesh support |
| **U.FL + ceramic antenna** | Switchable antenna, multi-band |
| **Stackable with L76K GNSS** | Direct GPS module option |
| **~$15 total cost** | Best value |

Full BOM & pinout: [`docs/prototype_bom.md`](docs/prototype_bom.md)

---

## Build

### Online (no toolchain)

Every push to `main` triggers automatic CI for all targets via GitHub Actions.
See [latest builds](https://github.com/VOLTEKOVER/OmniRID-Universal-Drone-ID/actions/workflows/rid-rust-ci.yml).

### Host build

```bash
cargo build --workspace
```

### ESP32 cross-build (nightly + ESP-IDF)

```bash
cargo +esp build --target xtensa-esp32-none-elf    --manifest-path firmware/Cargo.toml
cargo +esp build --target xtensa-esp32s3-none-elf  --manifest-path firmware/Cargo.toml
cargo +esp build --target riscv32imc-esp-none-elf   --manifest-path firmware/Cargo.toml
```

### Run tests

```bash
cargo test --workspace       # 312 tests, all passing, clippy clean
```

---

## Project Structure

```
OmniRID-Universal-Drone-ID/
├── OmniRID/                         # Rust workspace root
│   ├── Cargo.toml                   # Workspace manifest
│   │
│   ├── firmware/                    # Core firmware (no_std, esp-idf-sys glue)
│   │   └── src/
│   │       ├── main.rs              # Entry point
│   │       ├── beacon.rs            # WiFi Beacon + NAN TX
│   │       ├── ble.rs               # BLE 4.0/5.0 TX
│   │       ├── kalman.rs            # 1D x 3 Kalman filter
│   │       ├── auth.rs              # Ed25519 signing
│   │       ├── ota.rs               # OTA update (SHA-256 + Ed25519)
│   │       ├── security.rs          # Shared crypto module
│   │       ├── web_server.rs        # HTTP server + REST API
│   │       ├── cli.rs               # UART CLI REPL
│   │       ├── nvs.rs               # NVS persistence
│   │       ├── led.rs               # Status LED + WS2812 + GPIO
│   │       └── patrol.rs            # Demo GPS patrol
│   │
│   ├── inputs/                      # Protocol parsers
│   │   ├── mavlink/
│   │   ├── msp/
│   │   ├── nmea/
│   │   └── dronecan/
│   │
│   ├── outputs/                     # Broadcast backends
│   │   ├── wifi_beacon/
│   │   ├── wifi_nan/
│   │   └── ble_advertise/
│   │
│   ├── external-libs/
│   │   └── opendroneid-sys/         # OpenDroneID C FFI bindings (vendored)
│   │
│   └── hardware/bsp-esp32/          # Board support package (standalone crate)
│
├── OmniRID-Desktop/                 # Ground station (Electron)
│   ├── main.js
│   ├── preload.js
│   ├── src/
│   │   ├── decoder.js               # ASTM F3411-22a decoder
│   │   ├── tracker.js               # Device tracking + CSV/KML
│   │   └── capture.js               # WiFi/BLE/Serial capture
│   └── renderer/src/
│       ├── App.tsx
│       ├── components/
│       │   ├── DashboardTab.tsx
│       │   ├── DevicesTab.tsx
│       │   ├── MapTab.tsx
│       │   ├── TimelineTab.tsx
│       │   └── CaptureTab.tsx
│       └── hooks/useRidApi.ts
│
├── docs/                            # GitHub Pages
│   ├── index.html
│   ├── guide.html
│   ├── config(demo).html
│   ├── prototype_bom.md
│   └── images/
│
├── .github/workflows/
│   ├── rid-rust-ci.yml              # Host tests + ESP32-C6 cross-build
│   ├── release.yml
│   ├── rid-hub-ci.yml
│   └── dependabot.yml
│
├── SECURITY.md
├── LICENSE                          # Apache 2.0
└── README.md
```
---

### Ground station (RID Hub)

```bash
cd OmniRID-Desktop
npm install
npm run dev            # http://localhost:5173
npm run build           # Production build
npm start                # Launch Electron
```

### Web UI demo (no hardware)

Live demo at [config(demo).html](https://VOLTEKOVER.github.io/OmniRID-Universal-Drone-ID/config(demo).html), with simulated GPS, battery, counters, and logs.

---

## Documentation

| Resource | Link |
|---|---|
| **Quick Start** | [GitHub Pages](https://VOLTEKOVER.github.io/OmniRID-Universal-Drone-ID/) |
| **Technical Wiki** | [Protocols, wiring, API, security](https://VOLTEKOVER.github.io/OmniRID-Universal-Drone-ID/guide.html) |
| **Hardware BOM** | [`docs/prototype_bom.md`](docs/prototype_bom.md) |
| **API Reference** | Web UI `/api/*` endpoints (documented in the guide) |

---

## Security

### Authentication (lock system)

| Level | Name | Behavior |
|:---:|---|---|
| **0** | Normal | No restrictions, full read access to config |
| **1** | Ed25519 signed | Signature-based control for sensitive commands (restart, reset, OTA) |
| **2** | eFuse permanent | Reads the magic value in `EFUSE_BLK3`; irreversible (chip erase only) |

### OTA updates

- **Client-side SHA-256** computed via the Web Crypto API (`crypto.subtle.digest`)
- Mandatory **`X-Expected-SHA256`** header (hex-encoded SHA-256 of the firmware body)
- Optional **`X-Signature`** header (Ed25519 signature, when lock level ≥ 1)
- Server-side SHA-256 + Ed25519 verification via the shared security module
- Mismatched or missing hashes are rejected, preventing corrupt or malicious firmware
- Dual-OTA partition scheme with automatic rollback on failure

### Hardening

- **Safe JSON parsing** — all web API inputs are validated (no unsafe `cJSON` in Rust)
- **Rate limiting** — 10 failed signature attempts per 60 s sliding window
- **Bounded strings** — all string fields use stack-allocated or length-bounded types
- **`psa_crypto_init()`** — PSA Crypto API initialized at boot for ESP-IDF v5+
- **No unsafe in hot paths** — memory safety enforced by Rust's type system at compile time

### Key storage

- The Ed25519 **private key** is stored in NVS (encrypted at rest if flash encryption is enabled)
- Up to **5 public keys** are supported for command authentication at each lock level
- Consider enabling `CONFIG_SECURE_FLASH_ENC` on production builds

---

## Contributing

1. Use Rust edition 2024 and follow `rustfmt` defaults
2. Run `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` before submitting
3. Test on at least one target
4. Update documentation for significant changes
5. Open a PR with a description and hardware test notes

Full checklist: [`PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md)

---

## License & Ecosystem

```
OmniRID — Open Drone ID transmitter for ESP32
Copyright (C) 2024-2026 VOLTEKOVER

Based on Intel Open Drone ID (https://github.com/opendroneid)
Copyright (C) 2019-2023 Intel Corporation

Licensed under the Apache License, Version 2.0.
See the LICENSE file for details.
```
---

<p align="center">
  Made with ❤️ by <a href="https://github.com/VOLTEKOVER">VOLTEKOVER</a>
</p>