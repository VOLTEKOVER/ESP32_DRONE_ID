# OmniRID — Web UI, Desktop App & Documentation Status

> Embedded web UI served by `bsp-esp32` from flash (`include_str!`).
> Desktop app: Electron + React 19 + Ant Design 6 + Vite 8.
> Documentation: README.md + docs/index.html + docs/guide.html
> Last updated: 2026-08-17

---

## Part 1 — Web UI (Embedded)

### 🗂️ Tab Structure (15 tabs)

| Group | Tab | ID | Description |
|-------|-----|----|-------------|
| Monitor | Dashboard | `dashboard` | Live telemetry: GPS, protocol, TX status, battery, RSSI |
| Config | Identity | `identity` | UAS ID, ID/UA type, operator ID/self-ID, location, second BasicID |
| Config | Transmission | `transmission` | WiFi BCN/NAN, BLE4/5 toggles, channel, power, rates |
| Config | Access Point | `ap` | WiFi SSID/password, web server toggle |
| Config | Flight Controller | `fc` | Protocol, baud, SysID, broadcast on power |
| Config | Hardware | `hardware` | LED GPIO, WS2812, UART, TX/RX pins, DroneCAN, lighting, OTA GPIO, USB, startup delay |
| Compliance | Compliance | `compliance` | Region selector, per-region requirement checklist |
| Compliance | Security | `security` | Password, timeout, lock level, 5 public keys, 9 option flags |
| Advanced | System | `system` | Save, Factory Reset, Restart |
| Advanced | Firmware | `firmware` | OTA upload (.bin file) |
| Tools | Console | `console` | Serial terminal, CLI commands, signed commands |
| Tools | Presets | `presets` | Save/load/import/export config presets |
| Tools | Logging | `logging` | Start/stop telemetry log, download CSV |
| Tools | Sensors | `sensors` | Sensor readings (placeholder) |
| Other | Help | `help` | Documentation links, version info |

### 🔘 All Buttons

**Header Actions:** ☰ Menu, 💾 Save All, 📥 Export, 📤 Import, 🔄 Reconnect, ☀/☾ Dark
**System:** 🔄 Restart, ↻ Factory Reset
**Console:** 📋 Copy, 📋 All, 🗑 Clear, ⬇ Auto-scroll, ⚡ CLI dialog, Send
**Firmware:** Choose .bin, Upload
**Presets:** 💾 Save, 📥 Export All, 📤 Import
**Logging:** ▶ Start, ⏹ Stop, 📥 Download, 🗑 Clear
**Compliance:** 🔄 Update
**Security:** 👁 Show/Hide password, Save Security

### 📊 Form Fields (50+)

**Identity:** uas_id, id_type, ua_type, op_id, self_id_text, op_lat, op_lon, op_alt, uas_id2, id_type2, ua_type2
**Transmission:** tx_wifi_bcn, tx_wifi_nan, tx_ble4, tx_ble5, wifi_ch, wifi_pwr (2-20), wifi_bcn, wifi_nan, bt4_rate, bt4_pwr, bt5_rate, bt5_pwr
**AP:** wifi_ssid, wifi_pass, websrv_en
**FC:** protocol (AUTO/MAVLink/MSP/NMEA), baud, mav_sysid, bcast_pwr
**Hardware:** led_r/g/b_gpio, ws2812_gpio, ws2812_brightness, uart_port, tx/rx_pin, dronecan_rx/tx_gpio, dronecan_bitrate, lighting_pin/pattern/phase 0-4, ota_trigger_gpio, mavlink_usb_enable, start_delay_ms
**Security:** lock_lvl, sec-password, sec-timeout, pubkey1-5, opt_arm/nosave/print/demo/kalman/auth/mavlink_arm/mavlink_op_loc/identity_gate

### 🔌 API Endpoints

| Endpoint | Method | Content |
|----------|--------|---------|
| `/` `/config.html` | GET | HTML page |
| `/style.css` | GET | CSS |
| `/app.js` | GET | JavaScript |
| `/api/config` | GET/POST | Config JSON |
| `/api/status` | GET | Runtime status |
| `/api/capabilities` | GET | Build descriptor |
| `/api/command` | POST | Signed commands |
| `/api/reset` | GET/POST | Factory reset |
| `/api/logs` | GET | Log entries |
| `/ota` | POST | Firmware upload |

### 🔧 TODO — UI Improvements

**High:**
- [ ] WiFi SSID/password maxlength 20 → 32/63
- [ ] Operator lat/lon precision float → f64
- [ ] Demo public-safe hardening

**Medium:**
- [ ] manifest.json version auto-gen
- [ ] Settings search across all tabs
- [ ] Export/import with auth keys
- [ ] Console CLI history (up/down arrows)
- [ ] Sensors tab real data
- [ ] Mobile responsiveness verify

**Low:**
- [ ] Dark mode polish
- [ ] Keyboard shortcuts (Ctrl+S, Ctrl+E)
- [ ] Loading states / error toasts
- [ ] OTA progress bar
- [ ] Compliance region auto-detect from GPS
- [ ] Config diff view before save
- [ ] Lighting pattern preview

---

## Part 2 — Desktop App (OmniRID-Desktop)

| Module | File | Status |
|--------|------|--------|
| IPC Bridge | `main.js` + `preload.js` | ✅ |
| ASTM Decoder | `src/decoder.js` | ✅ |
| Per-MAC Tracker | `src/tracker.js` | ✅ |
| WiFi/BLE Capture | `src/capture.js` | ✅ |
| UI (6 tabs) | `renderer/app.js` | ✅ |

Tabs: Dashboard, Devices, Map, Timeline, Capture, Settings

---

## Part 3 — Documentation Restructuring (C → Rust)

### C → Rust Reference Mapping

| Old C Reference | New Rust Reference |
|---|---|
| `main.c` | `firmware/app/src/main.rs` |
| `web_config.c` / `web_server.c` | `firmware/rid-app/src/web_config.rs` + `bsp-esp32/src/web.rs` |
| `rid_security.c` | `firmware/rid-core/src/security.rs` |
| `rid_ota.c` | `firmware/rid-app/src/ota.rs` + `bsp-esp32/src/ota.rs` |
| `rid_auth.c` | `firmware/rid-core/src/auth.rs` |
| `rid_kalman.c` | `firmware/rid-core/src/kalman.rs` |
| `rid_patrol.c` | `firmware/rid-core/src/patrol.rs` |
| `rid_output.c/h` | `firmware/rid-core/src/hub.rs` |
| `protocol_detect.c` | `firmware/rid-core/src/protocol_detect.rs` |
| `mavlink_parser.c` | `inputs/proto-mavlink/src/parser.rs` |
| `nmea_parser.c` | `inputs/proto-nmea/src/parser.rs` |
| `msp_parser.c` | `inputs/proto-msp/src/parser.rs` |
| `rid_dronecan.c` | `inputs/proto-dronecan/src/parser.rs` |
| `rid_mavlink_usb.c` + `rid_mavlink_tx.c` | `inputs/proto-usb-mavlink/src/{tx,pack}.rs` |
| `odid_common.c` + `wifi.c` + `ble_tx.c` | `outputs/out-astm/src/{lib,wifi,ble4}.rs` |
| `opendroneid.c` + `mav2odid.c` | `external-libs/opendroneid-sys/` (FFI only) |
| `cli.c` | `firmware/rid-app/src/cli.rs` |
| `nvs_storage.c` | `firmware/rid-app/src/nvs.rs` + `bsp-esp32/src/nvs.rs` |
| `led_status.c` | `firmware/rid-app/src/led_status.rs` |
| `led_ws2812.c` | `firmware/rid-app/src/led_ws2812.rs` |
| `rid_lighting.c` | `firmware/rid-app/src/lighting.rs` |
| `ble_tx.c` | `firmware/rid-app/src/ble4.rs` + `bsp-esp32/src/ble.rs` |
| `wifi_tx.c` | `bsp-esp32/src/wifi.rs` |
| `esp_remote_id.h` | `firmware/rid-interface/src/{types,input,region,odid}.rs` |
| `MAVLink c_library_v2` | `inputs/proto-mavlink` (pure Rust) |
| `Vendored C` | `opendroneid-sys` FFI bindings only |

### Search & Replace Commands

```bash
# Find all C references
git grep -n "rid_security.c\|rid_ota.c\|web_config.c\|rid_auth.c\|main.c\|opendroneid-core-c\|Vendored C"

# Replace file names
git grep -l "rid_security.c" | xargs sed -i 's/rid_security.c/firmware\/rid-core\/src\/security.rs/g'
git grep -l "rid_ota.c" | xargs sed -i 's/rid_ota.c/firmware\/rid-app\/src\/ota.rs/g'
git grep -l "web_config.c" | xargs sed -i 's/web_config.c/firmware\/rid-app\/src\/web_config.rs/g'
git grep -l "rid_auth.c" | xargs sed -i 's/rid_auth.c/firmware\/rid-core\/src\/auth.rs/g'
git grep -l "main.c" | xargs sed -i 's/main.c/firmware\/app\/src\/main.rs/g'
git grep -l "rid_kalman.c" | xargs sed -i 's/rid_kalman.c/firmware\/rid-core\/src\/kalman.rs/g'
git grep -l "nvs_storage.c" | xargs sed -i 's/nvs_storage.c/firmware\/rid-app\/src\/nvs.rs/g'
git grep -l "cli.c" | xargs sed -i 's/cli.c/firmware\/rid-app\/src\/cli.rs/g'

# Generic replacements
git grep -l "Vendored C" | xargs sed -i 's/Vendored C/FFI bindings (opendroneid-sys)/g'
git grep -l "rid_security.c/h" | xargs sed -i 's/rid_security.c\/h/rid-core::security/g'
```

### Files to Verify Before Deletion

```bash
# List all C/H files still in repo
git ls-files '*.c' '*.h'

# Check if these directories exist and are referenced
git grep -n "opendroneid-core-c\|mavlink-sys\|mbedtls"
```

Candidate deletions (after confirming no references):
- `external-libs/opendroneid-core-c/` — if fully replaced by `opendroneid-sys`
- `external-libs/mavlink/` — if replaced by `proto-mavlink` crate
- `external-libs/mbedtls/` — if provided by ESP-IDF

### Specific Text Changes by File

**README.md:**
- One-liner: "OmniRID — hardware-agnostic Open DroneID transmitter (Rust). Any flight-controller input → any RID standard output."
- Quickstart: `git clone` → `cd OmniRID` → `cargo build --workspace && cargo test --workspace`
- Remove duplicated content already in docs/
- Update badges to correct repo/branch
- Replace "Vendored Dependencies" section with Rust crate list

**docs/index.html:**
- Keep hero + WebSerial button + 3-step flash
- Remove technical deep-dives (move to guide.html)
- Add links to guide sections (Protocols, Config, OTA, Security, Hardware)

**docs/guide.html:**
- Security section: replace C file refs with Rust paths
- OTA section: update to describe Rust implementation
- Build section: remove CMake/make steps, use cargo only
- "Legacy C Firmware" section: mark as "completed port, C deleted"
- Security Hardening table: replace C file column with Rust crate paths
- "Vendored Dependencies" block: list Rust crates + opendroneid-sys only

### Post-Change Verification

```bash
# Build
cargo build --workspace

# Test
cargo test --workspace

# Check no C references remain
git grep -n "\.c\b" -- '*.md' '*.html' | grep -v "opendroneid-sys\|vendor\|changelog\|LICENSE"

# Verify links in docs
# Open docs/index.html locally, check all links resolve
```
