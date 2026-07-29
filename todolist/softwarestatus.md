# ESP DRONE REMOTEID — Software Status

Last updated: 2026-07-29
Scope: all 80+ source files (excluding build artifacts)

---

## 🎯 Priority TODO

### 🔴 CRITICAL — Security & Correctness
- [x] **JSON parser → cJSON** (`web_config.c:102-139`) — naive `strstr()` exploited by crafted JSON
- [x] **Message Pack submessage decode** (`mavlink_parser.c:235-247`) — switch on submsg[0] is empty
- [x] **Task watchdog** (`esp_remote_id.c:627`) — no recovery if rid_task hangs
- [x] **Rate limiting on signature** (`web_config.c:306-375`) — brute-force on eFuse sigs
- [ ] **Base64 strict padding** (`web_config.c:275-304`) — accepts malformed base64
- [ ] **OTA timeout** (`rid_ota.c:129-157`) — infinite loop if upload stalled

### 🟡 HIGH — Quality & Robustness
- [ ] **Absolute GPS timeout** (`esp_remote_id.c:577-580`) — log stale GPS regardless of Kalman
- [ ] **CLI config set** (`cli.c`) — add `config set <key> <value>` write command
- [ ] **Differential factory reset** (`rid_ota.c:105`) — preserve auth keys, erase only config
- [ ] **populate_uas_data dedup** (`wifi_tx.c`) — shared function → `odid_common.c`
- [ ] **Dual-core pinning** — Core 0 (WiFi TX) / Core 1 (BLE+UI)
- [ ] **BLE 5.0 LR runtime check** (`ble_tx.c`) — detect Coded PHY capability

### 🟢 MEDIUM — Features & Polish
- [ ] **ESP-NOW mesh relay** — multi-hop range extension (4d effort)
- [ ] **LoRa backup SX1262** — 10+ km emergency link (6d)
- [ ] **SD Card + geofence logging** — flight log recovery (4d)
- [ ] **CLI command history** — circular buffer last 10 cmds
- [ ] **Kalman covariance export** — diagnostic API
- [ ] **Stats tracking** — TX failures, parse errors, signatures

### ✅ DONE — Recently Completed
- Kalman predictor (1D×3), WS2812 via RMT, Ed25519 auth pages
- OTA server, DroneCAN/TWAI, MAVLink USB + ARM_STATUS
- GPIO lighting (5-ch, 6 patterns), identity readiness gate
- Self-ID + Auth relay, MESSAGE_PACK unpack, BLE TX API
- Demo GPS patrol, dark mode web UI, CI all 3 targets
- Startup delay (`start_delay_ms`) — web-configurable via `/api/config`
- BLE TX power — `ble_tx_set_power()` now respects `dbm` param
- Full documentation sync — docs/index.html (~969L), guide.html (~2066L)
- README, CONTRIBUTING, SECURITY synced with current project state
- CI fix — rid-hub-ci.yml setup-node@v7 → v4
- Issue/PR template overhaul — bug_report.yml, PULL_REQUEST_TEMPLATE.md

---

## 📋 File Status Summary

### `ESP32_DRONE_REMOTE_ID_Firmware/`

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `main.c` | 109 | ✅ OK | Entry point, splash |
| `CMakeLists.txt` (root) | 18 | ✅ OK | Project name, components |
| `partitions.csv` | 7 | ✅ OK | 4MB OTA dual-slot |
| `sdkconfig.defaults` | 5 | ✅ OK | BT + SHA-256 defaults |

**Component: `esp_remote_id/`**

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `CMakeLists.txt` | 52 | ✅ OK | 24 src files, REQUIRES |
| `esp_remote_id.h` | 200 | ✅ OK | Full config+state structs |
| `opendroneid.h` | 762 | ✅ OK | Upstream Intel ODID lib |
| `odid_wifi.h` | 106 | ✅ OK | 802.11 packed structs |
| `esp_remote_id.c` | 543 | 🟡 ABSOLUTE GPS | Needs absolute GPS timeout |
| `web_config.c` | 735 | 🟡 SECURITY | Base64 strict padding |
| `cli.c` | 317 | 🟡 NEEDS | Missing `config set` command |
| `wifi_tx.c` | 204 | 🟡 DEDUP | Dedup populate_uas_data with common lib |
| `wifi.c` | 614 | ✅ OK | Intel ODID frame builder |
| `ble_tx.c` | 239 | ✅ OK | ble_tx_set_power() respects dbm |
| `mavlink_parser.c` | 276 | ✅ OK | MESSAGE_PACK unpack |
| `mav2odid.c` | 636 | ✅ OK | Upstream Intel lib |
| `opendroneid.c` | 1477 | ✅ OK | Upstream Intel lib |
| `nmea_parser.c` | 136 | ✅ OK | GGA+RMC parser |
| `msp_parser.c` | 126 | ✅ OK | MSP v1 parser |
| `protocol_detect.c` | 75 | ✅ OK | Auto-detect UART protocol |
| `nvs_storage.c` | 190 | ✅ OK | Config persistence |
| `led_status.c` | 211 | ✅ OK | 7-state RGB via LEDC|
| `led_ws2812.c` | 110 | ✅ OK | RMT-driven addressable LED|
| `rid_kalman.c` | 146 | ✅ OK | 1D×3 filter|
| `rid_auth.c` | 107 | ✅ OK | Ed25519 via mbedTLS|
| `rid_ota.c` | 329 | 🟡 TIMEOUT | Add OTA upload timeout|
| `rid_patrol.c` | 31 | ✅ OK | Demo GPS patrol|
| `rid_security.c` | 158 | ✅ OK | SHA-256, Ed25519, base64, hex|
| `rid_mavlink_tx.c` | 59 | ✅ OK | HEARTBEAT out|
| `rid_lighting.c` | 101 | ✅ OK | 5-ch GPIO lighting|
| `rid_dronecan.c` | 142 | ✅ OK | TWAI Fix2 decode|
| `rid_mavlink_usb.c` | 42 | ✅ OK | USB CDC transport|

**`webui/`**

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `config.html` | ~2546 | ✅ OK | Full UI (inline CSS/JS)|

### `mavlink/` — Auto-generated v2 dialect headers
- **Active**: ardupilotmega, common, minimal, protocol, types
- **Unused**: standard, development, ASLUAV, matrixpilot, test, helpers

### `RID_Hub/` — Electron + React 19 + Ant Design 6 + Vite 8

| Module | Status | Notes |
|--------|--------|-------|
| `main.js` / `preload.js` | ✅ OK | IPC bridge, no Python |
| `src/decoder.js` | ✅ OK | ASTM decoder port from Python |
| `src/tracker.js` | ✅ OK | Per-MAC tracking + export |
| `src/capture.js` | ✅ OK | WiFi/BLE/Serial wrappers |
| `renderer/src/` | ✅ OK | 6 tabs: Dashboard, Devices, Map, Timeline, Capture, Settings |

### `.github/` — CI/CD
- `build.yml` (66) ✅ 3-target matrix (esp32/s3/c6)
- `codeql.yml` (48) ✅ CodeQL security analysis
- `deploy-pages.yml` (21) ✅ Manual Pages deployment
- `release.yml` (308) ✅ Tag-triggered release + Pages deploy
- `rid-hub-ci.yml` (42) ✅ RID Hub build + test
- `dependabot.yml` (13) ✅ Weekly GHA updates
- Templates ✅ bug_report.yml, config.yml, feature_request.yml, PULL_REQUEST_TEMPLATE.md

### `docs/` — GitHub Pages

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `index.html` | ~969 | ✅ OK | Landing + Quick Start |
| `guide.html` | ~2066 | ✅ OK | Technical wiki |
| `config(demo).html` | ~2546 | ✅ OK | Offline demo simulation |
| `manifest.json` | 57 | 🟡 VERSION | Hardcoded version — should be auto-generated |
| `prototype_bom.md` | 68 | ✅ OK | XIAO C6 + L76K BOM |

### Root Files
- `README.md` (460) ✅ Full feature table, project structure, protocol listing
- `.gitignore` ✅ Covers build/, sdkconfig, __pycache__
- `LICENSE` ✅ Apache 2.0

---

## 🛩️ Firmware Roadmap

| Prio | Feature | Effort | Status |
|------|---------|--------|--------|
| P0 | Security fixes (base64 strict padding, OTA timeout) | ~1d | 🔜 Next |
| P1 | ESP-NOW mesh relay | ~4d | 🔜 Next |
| P1 | CLI config set + history | ~2d | Future |
| P1 | LoRa SX1262 backup | ~6d | Future |
| P2 | SD Card + geofence | ~4d | Future |
| P2 | Flash encryption (eFuse AES-256) | ~2d | Port from peinser |
| P2 | Dual-core pinning + BLE 5.0 LR runtime | ~2d | Future |

## 🖥️ Ground Tools Roadmap

| Tool | Effort | Status |
|------|--------|--------|
| Timing Analysis | ~1d | Future |
| NVS Provisioning | ~1d | Future |
| Meshtastic Bridge | ~2d | Future |
| Mesh Mapper | ~3d | Future |

---

## Port Sources
- **peinser/esp-remoteid** — Ed25519, OTA, DroneCAN, MAVLink features, flash encryption, WS2812, GPIO lighting, devcontainer, startup delay. Most ported; still missing flash encryption, devcontainer.
- **colonelpanichacks/Sky-Spy** — WiFi promiscuous + BLE RID receiver, dual-core pinning, mesh mapper. Ground tools integrated in RID Hub.
- **JimZGChow/wifi-rid-to-mesh** — RID→LoRa Meshtastic bridge, French RID format.
- **PeterJBurke/esp32-c3-remote-id** — Arduino RID with timing analysis.
