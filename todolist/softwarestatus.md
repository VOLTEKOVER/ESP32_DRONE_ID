# ESP DRONE REMOTEID — Software Status

Last updated: 2026-08-14
Scope: all 80+ source files (excluding build artifacts)

---

## 🎯 Priority TODO

### 🔴 CRITICAL — Security & Correctness
- [x] **JSON parser → cJSON** (`web_config.c:102-139`) — naive `strstr()` exploited by crafted JSON
- [x] **Message Pack submessage decode** (`mavlink_parser.c:235-247`) — switch on submsg[0] is empty
- [x] **Task watchdog** (`esp_remote_id.c:627`) — no recovery if rid_task hangs
- [x] **Rate limiting on signature** (`web_config.c:306-375`) — brute-force on eFuse sigs
- [x] **Base64 strict padding** (`rid_security.c:12-41`) — accepts malformed base64
- [x] **OTA timeout** (`rid_ota.c:129-157`) — abort upload after N consecutive idle socket timeouts (`OTA_MAX_IDLE_STALLS=12`, ~60 s)

### 🟡 HIGH — Quality & Robustness
- [x] **Absolute GPS timeout** (`esp_remote_id.c:586-593`) — `gps_valid` cleared at 10 s regardless of Kalman predictions + WARN log
- [x] **CLI config set** (`cli.c`) — `config set <field> <value>` for 20+ fields
- [x] **Differential factory reset** (`nvs_storage.c`) — new `nvs_storage_reset_preserve_keys()` keeps pubkey1..5; wired into OTA handler + `esp_rid_factory_reset`
- [x] **populate_uas_data dedup** (`wifi_tx.c` + `ble_tx.c`) — shared function → `odid_common.c` (`odid_common_build_uas_data`, both transports) 2026-08-15
- [ ] **Dual-core pinning** — Core 0 (WiFi TX) / Core 1 (BLE+UI)
- [x] **BLE 5.0 LR gate verified** (`ble_tx.c:246`) — `CONFIG_BT_BLE_50_EXTEND_ADV_EN` is the correct Bluedroid Kconfig symbol in IDF v5/v6, already gated by `SOC_BLE_50_SUPPORTED` (compiles only on S3/C6). Remaining: check return codes of `esp_ble_gap_ext_adv_*` calls at runtime

> Tracked in GitHub issue [#28](https://github.com/VOLTEKOVER/ESP_DRONE_REMOTE_ID/issues/28)

### 🟠 PRIORITY REFACTORS — 5 suggestions verified 2026-08-14
- [ ] **Copy `g_config` to a local struct at the start of the `rid_task` loop** — HIGH value, low cost. Real data race today: `rid_task` reads `g_config` without lock (`esp_remote_id.c:308-338`, `:490-497`, `:528-530`, `:557-565`, `:614`) while `esp_rid_set_config()` writes it under `g_lock`. Only `protocol`/`options` are currently copied locally (`:425-428`).
- [ ] **Return checks on `xTaskCreate` / `esp_task_wdt_add`** — low cost, robustness. Missing at `esp_remote_id.c:175` (rid_mavlink_tx_task), `:664` (rid_task), `cli.c:393` (cli_task), and `esp_task_wdt_add(NULL)` at `esp_remote_id.c:417`.
- [ ] **Parser → producer isolation (queue per parser + single-thread consumer)** — medium value, high cost. Parsers are polled synchronously from `rid_task` (`esp_remote_id.c:441-447`) and each `*_get()` reads the same UART inline (`mavlink_parser.c:93`, nmea/msp alike) → byte loss risk during WiFi/BLE TX in the same task. Cheap alternative first: larger UART RX buffer or interrupt-driven driver.
- [ ] **Fuzz harness for nmea/msp/mavlink parsers** — host libFuzzer. Blocked on parser refactor (separate byte-ingest from `uart_read_bytes` to make them host-testable). Would have caught the `decode_fix2` `-Warray-bounds` issue.
- [ ] **Configurable timeouts via web (`config.html`)** — low value. GPS stale is hardcoded 10 s (`esp_remote_id.c:603`), parser-internal 5 s, operator-location 30 s, identity 10 s, OTA idle stall `OTA_MAX_IDLE_STALLS` (`rid_ota.c:24`). Only session auth timeout is web-configurable today.

> Tracked in GitHub issue [#26](https://github.com/VOLTEKOVER/ESP_DRONE_REMOTE_ID/issues/26)

### 🔴 CONCURRENCY AUDIT — verified against code 2026-08-14
- [ ] **Fix AUTO-mode UART starvation (urgent)** — `protocol_detect_auto()` (`protocol_detect.c:42-69`) consumes up to 256 B from the same UART with a 50 ms blocking read on **every** `rid_task` loop, before the active parser reads → parser starved, framing broken in AUTO mode. Fix: lock the protocol after the first detection, or peek without consuming (`uart_get_buffered_data_len`), or detect only once at boot.
- [ ] **Data race on `g_state`** — written under `g_lock` (`esp_remote_id.c:466-533`) but read/written unlocked at `:435`, `:456`, `:588-597`, `:600-641`, and passed by reference to the TX paths (`:310-334`) while cli/web read it under lock. Fix: snapshot `gps`/`identity` under lock and pass copies to `wifi_tx_*`/`ble_tx_*`.
- [ ] **Replace `portMAX_DELAY` with timed waits** — `esp_remote_id.c:425`, `:466`, `:550` block forever on `g_lock`; a stuck holder (NVS write / httpd) hangs `rid_task` until WDT. Use `pdMS_TO_TICKS()` + log on timeout.
- [ ] **`rid_mavlink_tx` shares the same UART as the parsers** (`esp_remote_id.c:174`) — TX and parsing contend on one port when enabled together; document or gate the combination.
- [ ] **Stale MAVLink operator location** — `mavlink_parser_get_operator_location` (`esp_remote_id.c:517`) is used even when the active protocol is NMEA/MSP; its 30 s freshness window can feed stale data. Gate it on `proto == RID_PROTOCOL_MAVLINK`.

> Tracked in GitHub issue [#24](https://github.com/VOLTEKOVER/ESP_DRONE_REMOTE_ID/issues/24)

### 🟡 CONFIG PERSISTENCE — `esp_remote_id.h` verified 2026-08-14
- [ ] **NVS save/load gaps** (`nvs_storage.c`) — fields settable via web/CLI but never persisted, lost on reboot: `protocol`, `uart_port`, `tx_pin`, `rx_pin`, `ws2812_gpio`, `ws2812_brightness`, `lighting_pins/patterns/phase_offsets`, `dronecan_rx/tx_gpio`, `dronecan_bitrate`, `mavlink_usb_enable`, `ota_trigger_gpio`, `auth_private_key`, `start_delay_ms` (absent from `nvs_storage_save/load`). E.g. CLI `protocol` and `config set start_delay_ms` revert after reboot.
- [ ] **Auth lifecycle** — `rid_auth_init()` runs only at boot (`esp_remote_id.c:180`) and `auth_private_key` is not persisted → auth pages silently stop after reboot; a key change via web applies only after a reboot. Re-init on config change, or persist key (or document one-time provisioning).
- [ ] **`wifi_ssid`/`wifi_password` capped at `ESP_RID_MAX_STR_LEN` (20)** — WiFi spec allows 32/63 chars. Widen the field (NVS layout impact) or enforce the 20-char limit in the UI.
- [ ] **`operator_lat`/`operator_lon` precision** — stored as `float` in NVS (`nvs_storage.c:118-119`) while the struct fields are `double` (~1 m error at mid latitudes). Use a f64 blob or accept.

> Tracked in GitHub issue [#25](https://github.com/VOLTEKOVER/ESP_DRONE_REMOTE_ID/issues/25)

### 🟢 ARCHITECTURE — portability & dependency mgmt 2026-08-14
- [ ] **Split parsers: pure `process_bytes()` vs UART transport** — the key step for hardware independence. `nmea_parser_get()`/`msp_parser_get()`/`mavlink_parser_get()` read UART directly (`nmea_parser.c:118`, `msp_parser.c:104`, `mavlink_parser.c:93`). Split into a host-testable byte-ingest function (no ESP-IDF includes) + a thin transport port. Unblocks 3 existing items: fuzz harness, queue producer, host unit tests.
- [ ] **Formalize layered structure** — pure core (protocols, ODID encode, config) without ESP-IDF includes vs port/driver layer (UART, WiFi, BLE, TWAI, LED). `opendroneid.c`/`mav2odid.c` are already pure; parsers are the main coupling point left.
- [ ] **Migrate vendored libs to IDF Component Manager** — replace vendored `mavlink/` (ardupilotmega headers) and `opendroneid` with managed components (from the Espressif registry, if maintained upstream) using semver ranges in `idf_component.yml` (as already done for `espressif/cjson`). Libs then track upstream at build time.
- [ ] **Pin exact dependency versions for release** — `idf.py reconfigure` floats within `^range` in dev, but release/CI must pin exact versions (e.g. in `idf_component.lock`) for reproducible builds; note firmware OTA already covers runtime self-update of the whole image.
- [ ] **Commit `idf_component.lock`** — the Component Manager generates it on `idf.py reconfigure`; no lock file is committed today, so managed deps (`espressif/cjson`) can float and builds are not reproducible.
- [ ] **Add `version:` to `components/esp_remote_id/idf_component.yml`** — required only to publish the component to the registry; currently derived from git.
- [ ] **Use `PRIV_REQUIRES` for internal-only deps** (`CMakeLists.txt:30-48`) — mbedtls, cjson, nvs_flash, app_update, `esp_driver_*` are not part of the public API; moving them to `PRIV_REQUIRES` keeps the public interface minimal.
- [ ] **Drop legacy `driver` dep** (`CMakeLists.txt:45`) — redundant umbrella in IDF v5+ alongside the new `esp_driver_*` components.

> Tracked in GitHub issue [#27](https://github.com/VOLTEKOVER/ESP_DRONE_REMOTE_ID/issues/27)

### 🟢 MEDIUM — Features & Polish
- [ ] **ESP-NOW mesh relay** — multi-hop range extension (4d effort)
- [ ] **LoRa backup SX1262** — 10+ km emergency link (6d)
- [ ] **SD Card + geofence logging** — flight log recovery (4d)
- [ ] **CLI command history** — circular buffer last 10 cmds
- [ ] **Kalman covariance export** — diagnostic API
- [ ] **Stats tracking** — TX failures, parse errors, signatures

> Tracked in GitHub issue [#29](https://github.com/VOLTEKOVER/ESP_DRONE_REMOTE_ID/issues/29)

### ✅ DONE — Recently Completed
- **Fix campaign A–P** (static audit) — see `dataflow.md` §9; remaining open issues in §10
- **OTA upload timeout** — `rid_ota.c` `OTA_MAX_IDLE_STALLS=12` aborts stalled uploads (~60 s idle)
- **Absolute GPS timeout** — `gps_valid` cleared at 10 s without fresh parser data, independent of Kalman (`esp_remote_id.c`)
- **Differential factory reset** — `nvs_storage_reset_preserve_keys()` preserves provisioned public keys (device stays locked); used by OTA factory-reset form and `esp_rid_factory_reset`
- **CLI `config set`** — `config` now reads and writes (uas_id, operator_id, self_id, wifi_*, rates, power, operator_*, lock_level, start_delay_ms, …)
- **BLE 5.0 LR gate verified** — compile guard `CONFIG_BT_BLE_50_EXTEND_ADV_EN` confirmed correct for IDF v5/v6 Bluedroid (auto-excluded on ESP32)
- **Web UI split** — `config.html` + `style.css` (2374L) + `app.js` (1352L); `EMBED_FILES` + 2 new handlers `/style.css` `/app.js` in `web_config.c`
- **Bootstrap 5.3.3 vendored inline** in app.js (works offline) + heap overflow fix in `handle_get_logs` (clamp off)
- **Compliance Checklist** — 9 regions (auto/EUR/FAA/JPN/SGP/KOR/CHN/CAN/AUS/BRA/NZL), per-region `reqMap`, operator ID only for EUR
- **Tooltip system** — `data-tip` on all buttons (63 real / 66 demo), glassmorphism CSS, keyboard/focus accessible
- **Encoding repair** — fixed UTF-8 mojibake across webui + docs (theme icon ☀️/☾, em-dash, UAV favicon 🛸, BOM)
- **Password hashing** — `rid_sec_pwd` now SHA-256 (pure-JS, works over http) instead of base64; auto-migrates legacy base64 on login
- **Console persistence fix** — `rid_console` save `'1'/'0'` now matches restore check
- **Theme dialog "Remember my choice" disabled** — checkbox `disabled`, `pickTheme` won't persist `rid_theme_prompt`
- **Docs links fixed** — `config(demo).html` rename, dead `prototype_bom.md`/`shared.css` links removed, `#guide-installation` anchor repair
- Kalman predictor (1D×3), WS2812 via RMT, Ed25519 auth pages
- OTA server, DroneCAN/TWAI, MAVLink USB + ARM_STATUS
- GPIO lighting (5-ch, 6 patterns), identity readiness gate
- Self-ID + Auth relay, MESSAGE_PACK unpack, BLE TX API
- Demo GPS patrol, dark mode web UI, CI all 3 targets
- Startup delay (`start_delay_ms`) — web-configurable via `/api/config`
- BLE TX power — `ble_tx_set_power()` now respects `dbm` param
- Full documentation sync — docs/index.html, guide.html
- README, CONTRIBUTING, SECURITY synced with current project state
- CI fix — rid-hub-ci.yml setup-node@v7 → v4
- Issue/PR template overhaul — bug_report.yml, PULL_REQUEST_TEMPLATE.md
- Base64 strict padding — `b64_decode()` now validates padding and rejects invalid chars

---

## 🔍 Dead Code Audit (2026-08-05)

**Declared but never implemented** (header prototype with no definition anywhere → no link error, dead):

| Function | Declared in | Note |
|----------|-------------|------|
| `frdid_build` | `opendroneid.h:778` | Vendored ODID header; impl file `frdid.c` (FRDID / US standard) was never vendored into the project |
| `frdid_wifi_build_beacon_frame` | `opendroneid.h:775` | Same — FRDID WiFi beacon builder missing |

**Defined but never declared nor called** (dead code):

| Function | Defined in | Note |
|----------|-----------|------|
| `rid_ota_is_active` | `rid_ota.c:340` | No header declaration, no caller |

Action options: remove the 2 FRDID prototypes (or vendor `frdid.c`) and drop `rid_ota_is_active`.

**Non-functional feature exposed by `-Os`:** `decode_fix2` (`rid_dronecan.c:36`) reads `data[0..25]` from a classic-CAN frame (`twai_message_t.data[8]`); the `len < 32` guard is never satisfied (DLC ≤ 8), so DroneCAN Fix2 GPS decoding is unreachable until multi-frame transfer reassembly is implemented. `-Warray-bounds` suppressed for that function only (`-Werror` was tripping the whole build).

---

## 📋 File Status Summary

### `ESP32_DRONE_REMOTE_ID_Firmware/`

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `main.c` | 109 | ✅ OK | Entry point, splash |
| `CMakeLists.txt` (root) | 18 | ✅ OK | Project name, components |
| `partitions.csv` | 7 | ✅ OK | 4MB OTA dual-slot |
| `sdkconfig.defaults` | 20 | ✅ OK | Bluedroid BLE on all 3 targets (GATT/SMP off), size optimization, no assertions, err-to-name off, log WARN, partition + flash 4MB defaults |

**Component: `esp_remote_id/`**

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `CMakeLists.txt` | 55 | ✅ OK | 25 src files, REQUIRES |
| `esp_remote_id.h` | 200 | ✅ OK | Full config+state structs |
| `opendroneid.h` | 762 | ✅ OK | Upstream Intel ODID lib |
| `odid_wifi.h` | 106 | ✅ OK | 802.11 packed structs |
| `esp_remote_id.c` | 672 | ✅ OK | Absolute GPS timeout, differential reset, fix B/J/K |
| `web_config.c` | 750 | ✅ OK | cJSON, rate limiting, signature verify, /style.css + /app.js handlers |
| `cli.c` | 395 | ✅ OK | `config set <field> <value>` write command |
| `wifi_tx.c` | 198 | ✅ OK | Uses shared `odid_common_build_uas_data` |
| `wifi.c` | 614 | ✅ OK | Intel ODID frame builder |
| `ble_tx.c` | 233 | ✅ OK | ble_tx_set_power() respects dbm; LR gate verified; shared ODID builder |
| `odid_common.c` | 103 | ✅ OK | Shared `odid_common_build_uas_data` (WiFi + BLE pack builder) |
| `mavlink_parser.c` | 276 | ✅ OK | MESSAGE_PACK unpack |
| `mav2odid.c` | 636 | ✅ OK | Upstream Intel lib |
| `opendroneid.c` | 1477 | ✅ OK | Upstream Intel lib |
| `nmea_parser.c` | 136 | ✅ OK | GGA+RMC parser |
| `msp_parser.c` | 126 | ✅ OK | MSP v1 parser |
| `protocol_detect.c` | 75 | ✅ OK | Auto-detect UART protocol |
| `nvs_storage.c` | 218 | ✅ OK | Config persistence + `reset_preserve_keys` |
| `led_status.c` | 211 | ✅ OK | 7-state RGB via LEDC|
| `led_ws2812.c` | 110 | ✅ OK | RMT-driven addressable LED|
| `rid_kalman.c` | 146 | ✅ OK | 1D×3 filter|
| `rid_auth.c` | 107 | ✅ OK | Ed25519 via mbedTLS|
| `rid_ota.c` | 343 | ✅ OK | Upload timeout guard + differential factory reset|
| `rid_patrol.c` | 31 | ✅ OK | Demo GPS patrol|
| `rid_security.c` | 158 | ✅ OK | SHA-256, Ed25519, base64 strict, hex|
| `rid_mavlink_tx.c` | 59 | ✅ OK | HEARTBEAT out|
| `rid_lighting.c` | 101 | ✅ OK | 5-ch GPIO lighting|
| `rid_dronecan.c` | 142 | ✅ OK | TWAI Fix2 decode|
| `rid_mavlink_usb.c` | 42 | ✅ OK | USB CDC transport|

**`webui/`** (split — `EMBED_FILES`)

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `config.html` | 862 | ✅ OK | Markup-only (no inline JS/CSS except theme restore) |
| `style.css` | 2374 | ✅ OK | Full styling incl. tooltip system |
| `app.js` | 1352 | ✅ OK | All logic incl. vendored Bootstrap 5.3.3, SHA-256 |

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
| `index.html` | 634 | ✅ OK | Landing + Quick Start |
| `guide.html` | 1839 | ✅ OK | Technical wiki (regulatory 2026 norms) |
| `config(demo).html` | 2603 | ✅ OK | Offline demo simulation (public-safe, no password, auto-reset) |
| `bootstrap-theme.css` | 2176 | ✅ OK | Shared Bootstrap theme (replaces shared.css) |
| `manifest.json` | 57 | 🟡 VERSION | Hardcoded version — should be auto-generated |

### Root Files
- `README.md` (460) ✅ Full feature table, project structure, protocol listing
- `.gitignore` ✅ Covers build/, sdkconfig, __pycache__
- `LICENSE` ✅ Apache 2.0

---

## 🛩️ Firmware Roadmap

| Prio | Feature | Effort | Status |
|------|---------|--------|--------|
| P0 | OTA upload timeout | ~0.5d | ✅ Done |
| P1 | CLI config set | ~0.5d | ✅ Done |
| P1 | CLI command history | ~1d | Future |
| P1 | ESP-NOW mesh relay | ~4d | Future |
| P1 | LoRa SX1262 backup | ~6d | Future |
| P2 | SD Card + geofence | ~4d | Future |
| P2 | Flash encryption (eFuse AES-256) | ~2d | Port from peinser |
| P2 | Dual-core pinning | ~1d | Future |
| P2 | Absolute GPS timeout | ~1d | ✅ Done |
| P2 | Differential factory reset (keep auth keys) | ~1d | ✅ Done |
| P3 | Kalman covariance export + stats tracking | ~2d | Future |

## 🖥️ Ground Tools Roadmap

| Tool | Effort | Status |
|------|--------|--------|
| Timing Analysis | ~1d | Future |
| NVS Provisioning | ~1d | Future |
| Meshtastic Bridge | ~2d | Future |
| Mesh Mapper | ~3d | Future |

---

## 💡 Suggested Next Steps

**Web UI / Docs (zero-hardware, easy wins)**
- [ ] **Demo public-safe hardening** — hosted on GitHub Pages: clear `localStorage` on load, disable the Access Password field (it's public, password check is cosmetic in demo), reset state per visit so user A can't block user B
- [ ] Settings search: index is built but field may be missing in some tabs — verify search works across all sections
- [ ] `manifest.json` version auto-generated from CMake/CI instead of hardcoded
- [ ] Add export/import of full config incl. auth keys (currently config JSON only)
- [ ] Demo: visual "DEMO" banner + sample telemetry mode toggle (clearly public-facing)

**Firmware (needs ESP32 / IDF build)**
- [x] P0 OTA timeout — guard the upload loop, abort after N idle timeouts (~60 s)
- [x] Absolute GPS timeout — `gps_valid` cleared at 10 s regardless of Kalman state
- [x] `cli.c` `config set <field> <value>` write command (parity with web UI)
- [x] Differential factory reset — `nvs_storage_reset_preserve_keys()` keeps auth keys, erases only config
- [x] `populate_uas_data` dedup — shared ODID pack builder between `wifi_tx.c` and `ble_tx.c` (`odid_common_build_uas_data`) 2026-08-15
- [ ] BLE 5.0 LR — check return codes of `esp_ble_gap_ext_adv_set_params/config/start` at runtime

**Ground tools**
- [ ] NVS provisioning tool — flash a known-good config via CLI
- [ ] Timing analysis — measure beacon inter-transmission gaps against ASTM 3411-22a

## Port Sources
- **peinser/esp-remoteid** — Ed25519, OTA, DroneCAN, MAVLink features, flash encryption, WS2812, GPIO lighting, devcontainer, startup delay. Most ported; still missing flash encryption, devcontainer.
- **colonelpanichacks/Sky-Spy** — WiFi promiscuous + BLE RID receiver, dual-core pinning, mesh mapper. Ground tools integrated in RID Hub.
- **JimZGChow/wifi-rid-to-mesh** — RID→LoRa Meshtastic bridge, French RID format.
- **PeterJBurke/esp32-c3-remote-id** — Arduino RID with timing analysis.
