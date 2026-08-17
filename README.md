<p align="center">
  <img src="docs/images/logo%20con%20scritta.svg" alt="OmniRID" width="420">
</p>

<p align="center">
  <a href="https://github.com/VOLTEKOVER/ESP_DRONE_REMOTEID/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/VOLTEKOVER/ESP_DRONE_REMOTEID/ci.yml?logo=github" alt="CI"></a>
  <a href="https://VOLTEKOVER.github.io/ESP_DRONE_REMOTEID/"><img src="https://img.shields.io/badge/BETA-000?logo=esphome&color=f9a825" alt="BETA"></a>
  <a href="https://github.com/VOLTEKOVER/ESP_DRONE_REMOTEID/releases"><img src="https://img.shields.io/github/v/release/VOLTEKOVER/ESP_DRONE_REMOTEID?include_prereleases&logo=github&label=version" alt="Release"></a>
  <a href="https://www.espressif.com/"><img src="https://img.shields.io/badge/ESP32%20|%20S3%20|%20C6-000?logo=espressif" alt="Platform"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/VOLTEKOVER/ESP_DRONE_REMOTEID?color=blue" alt="License"></a>
</p>

<p align="center">
  <img src="docs/images/ardupilot_logo.webp" height="28" alt="ArduPilot">&nbsp;&nbsp;
  <img src="docs/images/betaflight_logo.svg" height="28" alt="Betaflight">&nbsp;&nbsp;
  <img src="docs/images/inav_logo.png" height="28" alt="INAV">
</p>

<p align="center">
  <b>ASTM F3411-22a / ASD-STAN prEN 4709-002 / GB 42590-2023</b> Open DroneID transmitter.<br>
  Parses <b>MAVLink &middot; MSP &middot; NMEA &middot; DroneCAN</b> from any flight controller.<br>
  Broadcasts via <b>WiFi Beacon + NAN + BLE 4.0/5.0</b> with Kalman filter, Ed25519 auth, SHA-256 OTA verification, and security hardening.<br>
  Written in <b>Rust</b> (<code>no_std</code> core, <code>esp-idf-sys</code> glue for hardware).
</p>

<p align="center">
  <b>Vendored Dependencies</b><br>
  <a href="https://github.com/mavlink/c_library_v2"><code>MAVLink c_library_v2</code></a> Jul 2026 &middot;
  <a href="https://github.com/opendroneid/core-c"><code>OpenDroneID core-c</code></a> Protocol v2 &middot;
  <a href="https://github.com/espressif/esp-idf"><code>ESP-IDF</code></a> v6.0.1 &middot;
  <a href="https://github.com/Mbed-TLS/mbedtls"><code>mbedTLS</code></a>
</p>

<p align="center">
  Supports: <b>ESP32, ESP32-S3, ESP32-C6</b> &nbsp;&middot;&nbsp; Includes: <b>RID Hub</b> ground station (Electron + React 19)
</p>

<p align="center">
  <a href="https://VOLTEKOVER.github.io/ESP_DRONE_REMOTEID/"><b>Wiki & Demo</b></a>&nbsp;&nbsp;
  <a href="https://VOLTEKOVER.github.io/ESP_DRONE_REMOTEID/config(demo).html"><b>Live Demo</b></a>
</p>

---

> [!CAUTION]
> ## NO RELEASE UNTIL SECURITY AUDIT IS COMPLETE
>
> **This firmware has NOT been security tested yet.**
> No official release will be published until a full security audit
> (penetration testing, firmware analysis, protocol fuzzing) has been
> performed and all critical/high findings are resolved.
>
> **Do NOT use this in production aircraft.**
> Use only for development and testing on the ground.
>
> See [SECURITY.md](SECURITY.md) for details.

---

## Table of Contents

1. [Quick Start](#-quick-start)
2. [Features](#-features)
3. [Communication Overview](#-communication-overview)
4. [Hardware](#-hardware)
5. [Build](#️-build)
6. [Project Structure](#-project-structure)
7. [Development Status](#-development-status)
8. [Testing](#-testing)
9. [Documentation](#-documentation)
10. [Security Notes](#-security-notes)
11. [Contributing](#-contributing)
12. [License & Ecosystem](#-license)
13. [Next Steps](#-next-steps)

---

## Quick Start

> No hardware yet? Try the **[offline demo](https://VOLTEKOVER.github.io/ESP_DRONE_REMOTEID/config(demo).html)** first — full simulation, no ESP32 required.

### Host (Linux / macOS / Windows)

```bash
git clone https://github.com/VOLTEKOVER/ESP_DRONE_REMOTEID.git
cd ESP_DRONE_REMOTEID
cargo build --workspace          # builds all crates for host
cargo test --workspace           # runs 312 tests
```

### ESP32 Cross-Build

```bash
# Requires Rust nightly with the ESP32 target + ESP-IDF SDK
cargo +esp build --target xtensa-esp32-none-elf    --manifest-path firmware/Cargo.toml
cargo +esp build --target xtensa-esp32s3-none-elf  --manifest-path firmware/Cargo.toml
cargo +esp build --target riscv32imc-esp-none-elf   --manifest-path firmware/Cargo.toml
```

### Flash & Connect

| Step | Action |
|:---:|---|
| 1 | Flash the firmware to your ESP32 via USB |
| 2 | Connect to WiFi **ESP-RID** |
| 3 | Open `http://192.168.4.1` to configure |
| 4 | Run `rid-hub` for the ground station dashboard |

---

## Features

### Radio & Protocols

| Feature | Details |
|---|---|
| **Broadcast** | WiFi Beacon (802.11 mgmt) + WiFi NAN + BLE 4.0 Legacy + BLE 5.0 Long Range (dual instances, S3/C6) |
| **Input protocols** | MAVLink v2 (ArduPilot/PX4), MSP (Betaflight/iNAV), NMEA, DroneCAN/CAN bus — auto-detect |
| **GPS source** | From flight controller (MAVLink/MSP/NMEA/CAN) **or** direct GPS module, takeoff location capture |
| **Position filter** | 1D x 3 Kalman (lat/lon/alt) with velocity prediction, 3s timeout |

### Security & Compliance

| Feature | Details |
|---|---|
| **Authentication** | Ed25519 signing (ASTM F3411-22a compliant), 4 pages per broadcast cycle |
| **Lock levels** | 3-tier: Normal / Ed25519 signed / eFuse permanent |
| **OTA updates** | WiFi AP with client-side SHA-256 (Web Crypto) + Ed25519 signature verification |
| **Security hardening** | Safe JSON parsing, rate limiting (10 fails/60s), bounded strings, `psa_crypto_init()` |
| **Configuration** | 70+ parameters: UAS ID, rates, power, public keys, auth, lock, lighting |

### User Interface

| Feature | Details |
|---|---|
| **Web UI** | Built-in WiFi AP at `192.168.4.1` + REST API + live telemetry |
| **CLI** | Raw UART REPL (14 commands: status, config, restart, patrol, transmit, etc.) |
| **Storage** | Persistent NVS configuration, eFuse tamper detection |
| **Dashboard** | Dark/light mode, responsive (mobile/tablet/desktop) |

<p align="center">
  <img src="docs/images/dashboard.png" alt="RID Hub Dashboard" width="820">
</p>

### Hardware & Lighting

| Feature | Details |
|---|---|
| **Status LED** | RGB PWM (LEDC) with 7 states + TX flash (configurable GPIO) |
| **Addressable LED** | WS2812 RGB (RMT driver) with HSV/RGB + brightness control |
| **GPIO Lighting** | 5-channel pattern outputs (OFF/SOLID/BLINK_SLOW/BLINK_FAST/ARMED/GPS_FLASH) with phase offsets |
| **Demo mode** | Simulated GPS patrol (Rome Colosseum, 200 m radius, 6 m/s) |

### Ground Station (RID Hub)

| Component | Stack |
|---|---|
| **Decoder** | Pure JavaScript, ASTM F3411-22a parser |
| **Tracker** | Device tracking with 500-point trail, CSV/KML export |
| **Capture** | WiFi monitor mode + BLE scan + Serial USB (optional npm modules) |
| **UI** | React 19 + Ant Design 6 + Vite 8 + Leaflet |

---

## Communication Overview

### Input (GPS from Flight Controller)

| Protocol | Format | Details |
|---|---|---|
| **MAVLink v2** | ArduPilot/PX4 | `GPS_RAW_INT`, `GLOBAL_POSITION_INT`, `HEARTBEAT`, `OPEN_DRONE_ID_*` |
| **MSP** | Betaflight/iNAV | `MSP_RAW_GPS` (106), `MSP_ATTITUDE` (108), `MSP_STATUS` (101) |
| **NMEA 0183** | Direct GPS module | `$GPGGA`, `$GNGGA`, `$GPRMC`, `$GNRMC`, `$GPVTG`, `$GNVTG` |
| **DroneCAN** | CAN bus | `uavcan.equipment.gnss.Fix2` (TWAI decode) |

### Output (RID Broadcast)

| Medium | Standard | Range |
|---|---|---|
| **WiFi Beacon** | IEEE 802.11 Mgmt | ~100 m (typical) |
| **WiFi NAN** | Service Discovery | ~100 m |
| **BLE 4.0** | Legacy advertising | ~50 m |
| **BLE 5.0** | Coded PHY (S3/C6) | ~200+ m (LR mode) |

### Configuration & Control

| Channel | Type | Access |
|---|---|---|
| **Web UI** | HTTP REST API | `192.168.4.1` (AP mode) |
| **CLI** | UART REPL | UART0 @ 115200 baud |
| **NVS** | Flash storage | Persistent config |
| **GPIO** | Lighting outputs | 5 configurable channels |

---

## Hardware

### Minimum Wiring (ESP32 + Flight Controller)

```
Flight Controller    ESP32 (or variant)
─────────────────    ─────────────────
TX (UART)       ->   GPIO16 (UART2 RX)
GND             ->   GND
5V (BEC)        ->   5V / VIN
```

> **NMEA tap**: add a **1 kohm series resistor** on the tap line to prevent backfeed.

### Recommended: Seeed XIAO ESP32-C6 (Zero-solder Kit)

| Feature | Benefit |
|---|---|
| **21 x 17.8 mm** | Fits in any enclosure |
| **USB-C + LiPo charger** | Battery-ready, no soldering |
| **WiFi 6 + BT 5.3** | Better range & throughput |
| **802.15.4 capable** | Future mesh support |
| **U.FL + ceramic antenna** | Switchable antenna, multi-band |
| **Stackable with L76K GNSS** | Direct GPS module option |
| **~$15 total cost** | Best value |

Full BOM & pinout: [`docs/prototype_bom.md`](docs/prototype_bom.md)

---

## Build

### Online (No Toolchain)

Push to `main` -> automatic CI for all targets via GitHub Actions.
See [Latest builds](https://github.com/VOLTEKOVER/ESP_DRONE_REMOTEID/actions/workflows/ci.yml)

### Host Build

```bash
cargo build --workspace
```

### ESP32 Cross-Build (nightly + ESP-IDF)

```bash
cargo +esp build --target xtensa-esp32-none-elf    --manifest-path firmware/Cargo.toml
cargo +esp build --target xtensa-esp32s3-none-elf  --manifest-path firmware/Cargo.toml
cargo +esp build --target riscv32imc-esp-none-elf   --manifest-path firmware/Cargo.toml
```

### Run Tests

```bash
cargo test --workspace       # 312 tests, all passing, clippy clean
```

---

## Project Structure

```
ESP_DRONE_REMOTEID/
├── OmniRID/              # Rust workspace root
│   ├── Cargo.toml                   # Workspace manifest
│   │
│   ├── firmware/                    # Core firmware (no_std, esp-idf-sys glue)
│   │   ├── Cargo.toml
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
│   ├── external-libs/               # Vendored C / unsafe glue
│   │   ├── opendroneid-core-c/
│   │   ├── mavlink/
│   │   └── mbedtls/
│   │
│   └── hardware/bsp-esp32/          # Board support packages (standalone crate)
│       ├── Cargo.toml
│       └── src/lib.rs
│
├── OmniRID-Desktop/                         # Ground station (Electron)
│   ├── main.js
│   ├── preload.js
│   ├── src/
│   │   ├── decoder.js               # ASTM F3411-22a decoder
│   │   ├── tracker.js               # Device tracking + CSV/KML
│   │   └── capture.js               # WiFi/BLE/Serial capture
│   ├── renderer/src/
│   │   ├── App.tsx
│   │   ├── components/
│   │   │   ├── DashboardTab.tsx
│   │   │   ├── DevicesTab.tsx
│   │   │   ├── MapTab.tsx
│   │   │   ├── TimelineTab.tsx
│   │   │   └── CaptureTab.tsx
│   │   └── hooks/useRidApi.ts
│   └── package.json
│
├── docs/                            # GitHub Pages
│   ├── index.html
│   ├── guide.html
│   ├── config(demo).html
│   ├── prototype_bom.md
│   └── images/
│
├── .github/workflows/
│   ├── ci.yml                       # Host test + ESP32-C6 cross-build
│   ├── release.yml
│   ├── rid-hub-ci.yml
│   └── dependabot.yml
├── SECURITY.md
├── LICENSE                          # Apache 2.0
└── README.md                        # This file
```

---

## Development Status

### Rust Port Progress

| Component | Status |
|---|---|
| **Workspace structure** | 4 boxes + standalone BSP crate |
| **Firmware core** | Main orchestrator, NVS, CLI, web server |
| **WiFi Beacon + NAN TX** | Implemented (esp-idf-sys bindings) |
| **BLE 4.0/5.0 TX** | Implemented (dual instances on S3/C6) |
| **MAVLink v2 parser** | Implemented (MESSAGE_PACK, ARM_STATUS, 6 ODID submessages) |
| **MSP parser** | Implemented |
| **NMEA parser** | Implemented |
| **DroneCAN (TWAI)** | Implemented |
| **Kalman filter** | 1D x 3 with velocity prediction |
| **Ed25519 auth** | ASTM F3411-22a compliant, 4 pages/cycle |
| **OTA updates** | SHA-256 + Ed25519 verification, dual-partition rollback |
| **Security hardening** | Safe JSON, rate limiting, bounded strings |
| **WS2812 RGB LED** | RMT driver, HSV/RGB |
| **GPIO lighting** | 5-channel patterns with phase offsets |
| **Demo patrol** | Simulated GPS (Rome Colosseum) |
| **CI** | Host tests + ESP32-C6 cross-build on GitHub Actions |

### Legacy C Firmware

The original C firmware (`ESP32_DRONE_REMOTE_ID_Firmware/`) has been **deleted**. All functionality has been ported to Rust. The old repository was fully replaced by this Rust workspace on 2026-08-17.

---

## Testing

### Firmware Tests (Rust)

```bash
cargo test --workspace       # 312 tests, all passing
cargo clippy --workspace -- -D warnings    # zero warnings
```

### CLI Commands (after flash)

```
> status              # Show GPS, TX rates, identity state
> config get uas_id   # Check config
> transmit 10         # Force 10 packets
> patrol              # Start demo GPS patrol
```

### Ground Station (RID Hub)

```bash
cd OmniRID-Desktop
npm install
npm run dev            # http://localhost:5173
npm run build          # Production build
npm start              # Launch Electron
```

### Web UI Demo (No Hardware)

Live demo at [VOLTEKOVER.github.io/ESP_DRONE_REMOTEID/config(demo).html](https://VOLTEKOVER.github.io/ESP_DRONE_REMOTEID/config(demo).html) with simulated GPS, battery, counters, logs.

---

## Documentation

| Resource | Link |
|---|---|
| **Quick Start** | [GitHub Pages](https://VOLTEKOVER.github.io/ESP_DRONE_REMOTEID/) |
| **Technical Wiki** | [Protocols, wiring, API, security](https://VOLTEKOVER.github.io/ESP_DRONE_REMOTEID/guide.html) |
| **Hardware BOM** | [`docs/prototype_bom.md`](docs/prototype_bom.md) |
| **API Reference** | Web UI `/api/*` endpoints (documented in guide) |

---

## Security Notes

### Authentication (Lock System)

| Level | Name | Behavior |
|---|---|---|
| **0** | Normal | No restrictions, read all config |
| **1** | Ed25519 Signed | Signature-based control for sensitive commands (restart, reset, OTA) |
| **2** | eFuse Permanent | Reads `EFUSE_BLK3` magic; irreversible (chip erase only) |

### OTA Updates

- **Client-side SHA-256** computed via Web Crypto API (`crypto.subtle.digest`)
- **Mandatory `X-Expected-SHA256`** header (hex-encoded SHA-256 of firmware body)
- **Optional `X-Signature`** header (Ed25519 signature when lock level >= 1)
- Server-side SHA-256 + Ed25519 verification via shared security module
- Rejects mismatched or missing hashes — prevents corrupt or malicious firmware
- Dual-OTA partition scheme with automatic rollback on failure

### Security Hardening

- **Safe JSON parsing** — all web API inputs parsed safely (no unsafe `cJSON` in Rust)
- **Rate limiting** — 10 failed signature attempts per 60 s sliding window
- **Bounded strings** — all string fields use stack-allocated or length-bounded types
- **`psa_crypto_init()`** — PSA Crypto API initialized at boot for ESP-IDF v5+
- **No unsafe in hot paths** — Rust type system enforces memory safety at compile time

### Key Storage

- Ed25519 **private key** stored in NVS (encrypted at rest if flash encryption enabled)
- **Public keys** (up to 5) for lock level command authentication
- Consider `CONFIG_SECURE_FLASH_ENC` on production builds

---

## Contributing

Contributions welcome! Please:

1. Use Rust edition 2024, follow `rustfmt` defaults
2. Run `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` before submitting
3. Test on at least one target (ESP32 / S3 / C6)
4. Update documentation for major changes
5. Create PR with description + hardware test notes

See [`PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md) for the full checklist.

---

## License

```
OmniRID — Open DroneID Transmitter for ESP32
Copyright (C) 2024-2026 VOLTEKOVER

Based on Intel Open Drone ID (https://github.com/opendroneid)
Copyright (C) 2019-2023 Intel Corporation

Licensed under the Apache License, Version 2.0.
See LICENSE file for details.
```

### Ecosystem

| Project | Purpose | Status |
|---|---|---|
| **opendroneid-core-c** | ASTM F3411 encoder/decoder | Vendored in repo |
| **ESP-IDF** | ESP32 SDK | v6.0.1+ |
| **MAVLink v2** | ArduPilot/PX4 protocol | ardupilotmega dialect, vendored |
| **mbedTLS** | Crypto (Ed25519, ECDSA) | ESP-IDF built-in |
| **RID Hub** | Ground station (Electron) | Standalone app |

---

## Next Steps

1. **Try the demo**: [offline config UI](https://VOLTEKOVER.github.io/ESP_DRONE_REMOTEID/config(demo).html)
2. **Build locally**: clone repo, `cargo build --workspace`, flash to ESP32
3. **Connect**: WiFi **ESP-RID** -> `192.168.4.1`
4. **Check documentation**: [Wiki](https://VOLTEKOVER.github.io/ESP_DRONE_REMOTEID/guide.html)
5. **Report issues**: [GitHub Issues](https://github.com/VOLTEKOVER/ESP_DRONE_REMOTEID/issues)

---

<p align="center">
  Made with care by <a href="https://github.com/VOLTEKOVER">VOLTEKOVER</a>
</p>
