# OmniRID — Software Status

> OmniRID is a Rust modular firmware for Remote ID broadcasting on ESP32.
> All legacy C firmware (`ESP32_DRONE_REMOTE_ID_Firmware/`) has been deleted.
> The C firmware audit and fix campaign findings (A–P, §9 of `dataflow.md`)
> remain as historical reference for the port.

Last updated: 2026-08-28
Tests: 319 passing | Clippy: clean | Edition: 2024

> Session fixes (2026-08-28): [#18] MSP framing off-by-one → MSP v1 standard;
> [#24] operator-location gate (MAVLink op location ignored by other protocols);
> [#25] NVS persistence for full config + auth lifecycle;
> [#26] xTaskCreatePinnedToCore return check + hardware-gated module;
> [#34] `speed_vertical` derived from altitude deltas for MSP/NMEA (Kalman off).
>
> Audit correction: [#19] DroneCAN reassembly, [#20] OTA signature at lock≥1 and
> [#21] `bcast_powerup` were already implemented in the Rust code (the
> `dataflow.md`/`processes.md` §10 "open" rows described the old C port). They
> were closed again on GitHub with pointers to the code.

---

## Workspace Structure

```
OmniRID/
├── firmware/          # Core (no_std, agnostic)
│   ├── rid-interface/   # Trait contracts (0 tests, pure types)
│   ├── rid-core/        # Hub, scheduler, kalman, auth, security (56 tests)
│   ├── rid-app/         # CLI, web, NVS, OTA, LED logic (~120 tests)
│   ├── app/             # Controller assembly + /api/capabilities (12 tests)
│   └── bsp-sim/         # Host simulator (binary demo)
├── inputs/            # Protocol parsers
│   ├── proto-mavlink/   # MAVLink v1/v2 (27 tests)
│   ├── proto-nmea/      # NMEA GGA/RMC/VTG (19 tests)
│   ├── proto-msp/       # MSP v1 (14 tests)
│   ├── proto-dronecan/  # UAVCAN v0 Fix2 (16 tests)
│   └── proto-usb-mavlink/ # USB CDC MAVLink (11 tests)
├── outputs/           # Broadcast standards
│   └── out-astm/        # ASTM F3411-22a WiFi/BLE (26 tests)
├── external-libs/     # Vendored C wrapped via FFI
│   └── opendroneid-sys/ # Intel OpenDroneID (11 tests)
└── hardware/          # Real BSP (excluded from workspace)
    └── bsp-esp32/       # ESP32 WiFi/BLE/NVS/LED/USB/OTA (2 tests caps)
```

---

## 🎯 TODO — Rust Firmware

### 🔴 CRITICAL — Security & Correctness

- [x] JSON parsing (serde_json replaces naive strstr/C)
- [x] Ed25519 verify_signed_body (ed25519-dalek + pkcs8)
- [x] OTA upload timeout + signature verification
- [x] Base64 strict padding (rid-core::security)
- [x] Rate limiting on signed commands (SigRate in rid-app::web)

### 🟡 HIGH — Quality & Robustness

- [x] Config race conditions (Rust ownership eliminates g_config data race)
- [x] Parser isolation (each proto-* is a separate crate, no shared UART)
- [x] Populate UAS data dedup (out-astm::build_uas)
- [x] GPS staleness detection (rid-core::scheduler)
- [x] **Dual-core pinning** — Core 0 (WiFi TX) / Core 1 (Scheduler+BLE+UI) via `bsp_esp32::core`
- [ ] **Release binary / build matrix ESP32** — CI workflow for cross-compilation when bsp-esp32 is complete

### 🟠 FEATURES — Future

- [ ] **Non-ASTM output encoders** — China GB 42590 (stub in out-astm::stubs), FRDID (US FRIAs)
- [ ] **ESP-NOW mesh relay** — multi-hop range extension (~4d)
- [ ] **LoRa SX1262 backup** — 10+ km emergency link (~6d)
- [ ] **SD Card + geofence logging** — flight log recovery (~4d)
- [ ] **CLI command history** — circular buffer last 10 cmds
- [ ] **Kalman covariance export** — diagnostic API
- [x] **Stats tracking** — ticks, GPS updates/discards, parse errors, signatures, TX fails, OTA count (`rid_interface::Stats`, serialized in `/api/status`)

### 🔵 CI/CD & Auto-Update

- [ ] **Auto-update protocols from GitHub** — path-dependency only today; CI protocol-updates.yml already checks upstream, but decide if git `rev` deps are needed or workspace + cargo update suffices
- [ ] **Flash encryption** — eFuse AES-256 (port from peinser)

---

## ✅ DONE — Completed Work

### Workspace & Architecture
- Single workspace with glob members, `exclude = ["hardware"]`
- `rid-interface` trait contracts (no_std, zero deps)
- `bsp-esp32` isolated in `hardware/`, standalone workspace, host-compilable
 - 319 tests passing across all crates
- Clippy clean (`-D warnings`)

### Firmware Core (Fase 0–5)
- rid-core: hub (region dispatch, standard selection), scheduler (tick, GPS staleness, Kalman), auth (Ed25519), security (base64/hex/SHA-256/Ed25519), protocol_detect (auto-detect), readiness (per-region gate), patrol (demo GPS)
- rid-app: CLI (config set/get), JSON API, web_config (signed commands, rate limit), NVS (save/load/reset_preserve_keys), OTA (timeout, signature, SHA-256 streaming), BLE 4.x legacy adv, LED status (7-state RGB), WS2812 (HSV/GRB), lighting (5-ch patterns), webui (embedded assets)
- app: Controller (assemble BSP + input + output), /api/capabilities, derive_ids_from_mac

### Input Protocols
- proto-mavlink: MAVLink v1/v2 framing, CRC, MESSAGE_PACK, GPS decode
- proto-nmea: streaming GGA/RMC/VTG parser
- proto-msp: MSP v1 RAW_GPS/ATTITUDE/STATUS
- proto-dronecan: full UAVCAN v0 transfer reassembly, Fix2 DSDL
- proto-usb-mavlink: USB CDC heartbeat + OPEN_DRONE_ID_SYSTEM frames

### Output Standards
- out-astm: ASTM F3411-22a, WiFi beacon + NAN, BLE 4.x rotation, MESSAGE_PACK encode
- opendroneid-sys: vendored Intel C lib with Rust FFI, auto-update weekly

### Hardware (bsp-esp32)
- WiFi beacon/NAN injection, BLE 4.x/5.0 LR, NVS, USB CDC, LED (LEDC/RMT), web server, OTA
- Capability matrix per chip (esp32/s3/c6)
- Dual-core pinning: Core 0 (WiFi TX) / Core 1 (Scheduler+BLE+UI)
- Feature-gated: `#[cfg(feature = "hardware")]`

### CI/CD (7 workflows)
- OmniRID CI (build/test/clippy), ESP32 Cross-Build, OmniRID Desktop CI
- Weekly checks (OpenDroneID, MAVLink, DroneCAN, ESP-IDF, regulatory pages)
- Security Audit (cargo-audit + cargo-deny), Release, Deploy Pages

### Documentation & Desktop
- guide.html, index.html, config(demo).html — all updated for Rust
- OmniRID-Desktop: Electron + React 19 + Ant Design 6 + Vite 8

### Audit Fixes (session 2026-08-28)
- [#18] MSP framing corrected: replaced the replicated C off-by-one quirk with standard MSP v1 framing (`buf[3]=size, buf[4]=type, payload=buf[5..]`). Unblocks §10.1 / dataflow section-2 "blocked by §10.1" rows.
- [#24] Operator-location gate: non-MAVLink protocols ignore MAVLink operator location unless explicitly selected (`non_mavlink_protocol_ignores_mavlink_operator_location`).
- [#25] NVS persistence: full config now persisted (protocol, uart_port, tx_pin, rx_pin, ws2812_*, lighting_*, dronecan_*, mavlink_usb_enable, ota_trigger_gpio, auth_private_key, start_delay_ms) via get_blob/set_blob + auth lifecycle. Closes dataflow §5/§10.7/§10.8 and processes §6.12 gaps.
- [#26] xTaskCreatePinnedToCore return-checked; task module hardware-gated.

---

## 📋 C → Rust Port Summary

| C File(s) | Rust Crate | Status |
|---|---|---|
| `esp_remote_id.h` | `rid-interface` | ✅ |
| `esp_remote_id.c` | `rid-core` + `app` | ✅ |
| `rid_kalman.c` | `rid-core::kalman` | ✅ |
| `rid_output.c/h` | `rid-core::hub` | ✅ |
| `rid_auth.c` | `rid-core::auth` | ✅ |
| `rid_security.c` | `rid-core::security` | ✅ |
| `protocol_detect.c` | `rid-core::protocol_detect` | ✅ |
| `mavlink_parser.c` | `inputs/proto-mavlink` | ✅ |
| `nmea_parser.c` | `inputs/proto-nmea` | ✅ |
| `msp_parser.c` | `inputs/proto-msp` | ✅ |
| `rid_dronecan.c` | `inputs/proto-dronecan` | ✅ |
| `rid_mavlink_usb.c` + `rid_mavlink_tx.c` | `inputs/proto-usb-mavlink` | ✅ |
| `odid_common.c` + `wifi.c` + `ble_tx.c` | `outputs/out-astm` | ✅ |
| `opendroneid.c` + `mav2odid.c` | `external-libs/opendroneid-sys` | ✅ |
| `web_config.c` (pure) | `rid-app::web_config` | ✅ |
| `cli.c` (pure) | `rid-app::cli` | ✅ |
| `nvs_storage.c` | `rid-app::nvs` + `bsp-esp32::nvs` | ✅ |
| `led_status.c` | `rid-app::led_status` | ✅ |
| `led_ws2812.c` | `rid-app::led_ws2812` | ✅ |
| `rid_lighting.c` | `rid-app::lighting` | ✅ |
| `rid_ota.c` (validation) | `rid-app::ota` + `bsp-esp32::ota` | ✅ |
| `ble_tx.c` (framing) | `rid-app::ble4` + `bsp-esp32::ble` | ✅ |
| ESP-IDF HW (WiFi/BLE/NVS/LED/USB) | `hardware/bsp-esp32` | ✅ |

---

## 🛩️ Firmware Roadmap

| Prio | Feature | Effort | Status |
|------|---------|--------|--------|
| P0 | Non-ASTM encoders (CN 42590, FRDID) | ~5d | Future |
| P1 | Release binary / ESP32 build matrix CI | ~1d | Future |
| P1 | ESP-NOW mesh relay | ~4d | Future |
| P1 | LoRa SX1262 backup | ~6d | Future |
| P2 | Flash encryption (eFuse AES-256) | ~2d | Future |
| P2 | SD Card + geofence logging | ~4d | Future |
| P3 | Kalman covariance export | ~1d | Future |

## 🖥️ Ground Tools Roadmap

| Tool | Effort | Status |
|------|--------|--------|
| Timing Analysis | ~1d | Future |
| NVS Provisioning | ~1d | Future |
| Meshtastic Bridge | ~2d | Future |
| Mesh Mapper | ~3d | Future |

---

## Port Sources

- **peinser/esp-remoteid** — Ed25519, OTA, DroneCAN, MAVLink, flash encryption, WS2812, GPIO lighting. Most ported; missing flash encryption.
- **colonelpanichacks/Sky-Spy** — WiFi promiscuous + BLE RID receiver, dual-core pinning, mesh mapper.
- **JimZGChow/wifi-rid-to-mesh** — RID→LoRa Meshtastic bridge, French RID format.
- **PeterJBurke/esp32-c3-remote-id** — Arduino RID with timing analysis.
