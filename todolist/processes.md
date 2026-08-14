# ESP DRONE REMOTEID — Every Process Running (Debug Reference)

Last updated: 2026-08-14
Scope: every concurrent context, task, event, callback and logical process in `ESP32_DRONE_REMOTE_ID_Firmware`. Use this file when a feature "does not work correctly": find the process, check its inputs/gates, then its output.
Companion files: `todolist/dataflow_map.md` + `todolist/dataflow_verification.md` (field-by-field data chains; to be merged into `dataflow.md`), `todolist/softwarestatus.md` (open todos).

---

## 1) Runtime model (FreeRTOS / ESP-IDF)

- **No software timers, no timer callbacks, no event-loop registrations** in the application: `esp_timer_create`/`xTimerCreate`/`esp_event_handler_register` are **not used anywhere** in `components/esp_remote_id/src`. Everything periodic is driven by the `rid_task` 100 ms loop or by IDF stacks (WiFi/BLE/HTTP).
- **Drivers with ISRs** (all configured with `NULL` event-queue, i.e. polled): UART1 RX (`driver/uart.h`), TWAI (CAN RX queue), RMT (WS2812 TX), LEDC (status LED PWM), GPIO (only read once at boot for OTA trigger).
- **Synchronization**: one global mutex `g_lock` (`esp_remote_id.c:43`) guards `g_config`/`g_state`. Known weaknesses are tracked in `softwarestatus.md` (unlocked reads in `rid_task`, `portMAX_DELAY` usage).

---

## 2) Boot sequence — every step, in order

All inside `esp_rid_init()` (`esp_remote_id.c:133-224`), runs in the **main task** (`app_main`):

| # | Step | Where | Note |
|---|------|-------|------|
| 1 | `nvs_flash_init` + erase/reinit if corrupted | `nvs_storage.c:10-18` | `ESP_ERROR_CHECK` — a crash here = NVS partition broken |
| 2 | `default_config(&g_config)` then `nvs_storage_load` | `esp_remote_id.c:46-115, 139` | loaded values override defaults field-by-field |
| 3 | `rid_ota_check_and_run(&g_config)` | `esp_remote_id.c:142`, `rid_ota.c:304-338` | **if OTA GPIO pulled low → enters OTA mode and loops forever here** (rest of boot never runs) |
| 4 | `g_lock = xSemaphoreCreateMutex()` | `esp_remote_id.c:144` | |
| 5 | `memset(&g_state,0)` | `esp_remote_id.c:146` | |
| 6 | startup delay `g_config.start_delay_ms` (default 10 s) | `esp_remote_id.c:149-152` | `vTaskDelay` blocks main |
| 7 | compute boot baud (115200 in AUTO, else configured) | `esp_remote_id.c:154-156` | |
| 8 | `protocol_detect_init` → UART1 driver install (RX 256 B) | `esp_remote_id.c:157`, `protocol_detect.c:18-40` | configures pins 17/18 by default |
| 9 | `nmea_parser_init` / `msp_parser_init` / `mavlink_parser_init` + sysid filter | `esp_remote_id.c:158-161` | all three share UART1 |
| 10 | `esp_netif_init()` | `esp_remote_id.c:163` | network stack |
| 11 | `wifi_tx_init(&g_config)` | `esp_remote_id.c:165`, `wifi_tx.c:66-...` | `esp_wifi_init`, max TX power, AP config |
| 12 | `ble_tx_init()` | `esp_remote_id.c:166`, `ble_tx.c:195-207` | BT controller + Bluedroid init |
| 13 | `ble_tx_set_power(9)` | `esp_remote_id.c:169` | |
| 14 | if options bit 6/7 or `mavlink_usb_enable` → `rid_mavlink_tx_init` + create `rid_mavlink_tx` task | `esp_remote_id.c:172-176` | |
| 15 | if `RID_OPT_AUTH_ED25519` → `rid_auth_init(key)` | `esp_remote_id.c:178-181` | parses PEM key |
| 16 | `led_ws2812_init(gpio,brightness)` | `esp_remote_id.c:185` | RMT |
| 17 | if `lighting_pins[0]>=0` → `rid_lighting_init` | `esp_remote_id.c:187-194` | |
| 18 | if `dronecan_rx/tx_gpio>=0` → `rid_dronecan_init` (TWAI install+start) | `esp_remote_id.c:196-200` | |
| 19 | if `mavlink_usb_enable` → `rid_mavlink_usb_init` | `esp_remote_id.c:202-205` | |
| 20 | MAC-based ID fix-up (default IDs) + NVS save | `esp_remote_id.c:207-214` | only if still default |
| 21 | `led_status_reconfigure` | `esp_remote_id.c:216` | LEDC |
| 22 | `web_config_init(webserver_en)` | `esp_remote_id.c:217`, `web_config.c:727-755` | starts HTTP server (10 handlers) |
| 23 | `cli_init()` → creates `cli_task` | `esp_remote_id.c:219`, `cli.c:391-395` | |
| 24 | `rid_kalman_init(&g_kalman)` | `esp_remote_id.c:221` | |
| 25 | `esp_rid_start()` → creates `rid_task` | `esp_remote_id.c:660-666` | the core loop |

---

## 3) Complete task list (application)

| Task | Created | Stack | Prio | Role |
|---|---|---|---|---|
| `main` | IDF | — | — | boot sequence above, then idle |
| `rid_task` | `esp_remote_id.c:664` | 4096 | 5 | the core 100 ms loop (see §5); subscribed to WDT |
| `cli_task` | `cli.c:393` | 4096 | 5 | reads UART0 stdin line-by-line, executes commands |
| `rid_mavlink_tx` | `esp_remote_id.c:175` | 2048 | 3 | **only if** options bit 6/7 or `mavlink_usb_enable`; heartbeat + ODID_SYSTEM on UART1 (+USB mirror) |
| `httpd` (web) | `web_config.c:740` | IDF default | 5 | serves the config UI + `/api/*`; one task per socket connection |
| `httpd` (OTA) | `rid_ota.c:274` | IDF default | 5 | only in OTA mode; `/`, `/update`, `/factory_reset`, `/rollback` |
| WiFi / BLE / sys_evt / esp_timer / ipc / idle | IDF | IDF | — | internal stacks (do not touch) |

`xTaskCreate` return values are **never checked** (`esp_remote_id.c:175,664`, `cli.c:393`) — see todo.

---

## 4) ISR / driver contexts

| Peripheral | Driver | Interrupt does | App polling |
|---|---|---|---|
| UART1 RX | `driver/uart` | RX FIFO → ring buffer (256 B) | parsers + detect call `uart_read_bytes` |
| UART0 (console) | console | stdin ring | `cli_task` `fgets` |
| TWAI/CAN | `driver/twai` | RX → queue (len 10) | `rid_dronecan_get` `twai_receive(0)` |
| RMT | `driver/rmt` | WS2812 TX | `led_ws2812_set_rgb` (blocking-ish) |
| LEDC | `driver/ledc` | PWM hardware | `led_status_tick` |
| GPIO (OTA trigger) | `driver/gpio` | none | read once at boot, `rid_ota.c:309-318` |
| USB Serial/JTAG | console | console RX | `rid_mavlink_usb` writes |

---

## 5) `rid_task` main loop — every phase (esp_remote_id.c:411-658)

Runs forever while `g_running`; period 100 ms (`vTaskDelay` at `:653`), WDT reset at `:654`.

| Phase | Lines | What happens | If it goes wrong → |
|---|---|---|---|
| A | 417 | `esp_task_wdt_add(NULL)` — subscribe to WDT | **never checked**; if it fails the task is unsupervised |
| B | 425-428 | brief lock: copy `protocol`+`options` to locals | |
| C | 430-433 | protocol selection: AUTO → `protocol_detect_auto()` (blocks up to 50 ms, **consumes UART bytes**) else configured | AUTO starvation bug (see todo); protocol flaps |
| D | 435 | `g_state.active_protocol = proto` (unlocked write) | |
| E | 439-451 | dispatch active parser `*_get()` → reads UART, parses, fills `gps_data` | wrong protocol → no data |
| F | 453-457 | DroneCAN fallback if `!have_data` | |
| G | 461-469 | gate `force_tx || fix>=2`; under lock: copy gps→`g_state`, `gps_valid=true`, `last_update_ms` | too-strict/loose gate |
| H | 471-479 | MAVLink only: armed, sysid | |
| I | 481-498 | identity: MAVLink if present else from `g_config` (**unlocked reads**) | race with web/CLI |
| J | 500-506 | takeoff capture (first 3D fix) | |
| K | 508-512 | MSP/NMEA: derive `altitude_relative` from takeoff | |
| L | 514-531 | operator location: MAVLink → `gps.operator_*` else `g_config.operator_*` | stale MAVLink when proto≠MAVLink |
| M | 533 | unlock `g_lock` | |
| N | 535-547 | `DONT_SAVE_BASIC_ID` + identity readiness gate | |
| O | 549-570 | demo mode: `rid_patrol_tick`, identity from config, `active_protocol=NONE` | |
| P | 572-598 | Kalman (if enabled & !demo): update/predict/overwrite `g_state.gps` | |
| Q | 600-607 | **absolute 10 s GPS timeout** clears `gps_valid` + WARN | timeouts too short/long |
| R | 609-612 | if `had_gps` → `update_transmissions()` (TX decision, §8) | |
| S | 614-623 | LED state machine + tick | |
| T | 626-630 | WS2812 RGB | |
| U | 632-634 | lighting set_state + tick | |
| V | 636-643 | periodic RID log line (if `PRINT_RID_MAVLINK`) | |
| W | 645-651 | status box every 100 loops, system box every 500 | |
| X | 653-654 | `vTaskDelay(100ms)` + `esp_task_wdt_reset()` | |

---

## 6) Subsystem processes

### 6.1 Protocol detection — `protocol_detect.c`
- `protocol_detect_init` installs UART driver (RX 256 B) at boot baud.
- `protocol_detect_auto()` (`:42-69`): `uart_read_bytes(...,50ms)`, classifies `$M<` → MSP, `$G/$N` → NMEA, `0xFE/0xFD` header → MAVLink, else defaults NMEA. **Consumes the bytes** (known issue).
- `protocol_detect_reinit` deletes + reinstalls UART at new baud (called on config save when baud changed).

### 6.2 NMEA parser — `nmea_parser.c`
- Static buffer `g_nmea_buf[256]` + index; `$GPGGA` (lat/lon/alt/fix/sats/baro) + `$GPRMC` (lat/lon/speed). Checksum validated.
- `nmea_parser_get()` (`:118`) reads UART, appends to buffer, parses on newline; gate `fix>=2 && lat!=0`.

### 6.3 MSP parser — `msp_parser.c`
- `MSP_RAW_GPS(106)`, `MSP_ATTITUDE(108)`, `MSP_STATUS(101)`; checksum XOR.
- `msp_parser_get()` (`:104`) reads UART, decodes `$M<` frames; gate `fix>=3 && lat!=0` (fixed to `>=2`).

### 6.4 MAVLink parser — `mavlink_parser.c`
- `mavlink_parse_char` on every byte from UART (512 B buffer).
- Messages: GLOBAL_POSITION_INT, GPS_RAW_INT, VFR_HUD, ATTITUDE, AHRS2, HEARTBEAT, ODID_LOCATION/BASIC_ID/OPERATOR_ID/SELF_ID/AUTHENTICATION/SYSTEM/MESSAGE_PACK.
- **SYSTEM (op-loc) does NOT overwrite position** (fixed); operator loc freshness 30 s; identity freshness 10 s; GPS freshness 5 s.
- `mavlink_parser_get_identity` only if <10 s old.

### 6.5 DroneCAN — `rid_dronecan.c`
- TWAI polled from `rid_task` fallback. Fix2(2000) decoded (unreachable, `len<32`), AHRS(1000)/Identity(8192) are **stubs**. Freshness 5 s.

### 6.6 Demo patrol — `rid_patrol.c`
- Synthetic circle around Rome (41.9028,12.4964), radius 0.003°, speed 6 m/s, fix 2..4.

### 6.7 WiFi TX — `wifi_tx.c`
- `wifi_tx_init`: `esp_wifi_init`, `esp_wifi_set_max_tx_power`, AP config (SSID/pass/channel).
- `wifi_tx_transmit`: builds beacon with `odid_wifi_build_message_pack_beacon_frame`, sends via `esp_wifi_80211_tx` with 4-attempt `{AP,STA,AP,STA}` fallback.
- `wifi_tx_transmit_nan`: NAN action frame.
- `wifi_tx_reconfigure_ap`: re-apply AP config on config change.

### 6.8 BLE TX — `ble_tx.c`
- `ble_tx_init`: `esp_bt_controller_init` + `esp_bluedroid_init`.
- `ble_tx_transmit_legacy`: legacy 31 B ADV, one rotated 25 B message + counter (S3/C6 use ext adv instance 2 with `LEGACY_NONCONN`).
- `ble_tx_transmit_lr`: ext adv instances 0/1 (254 B), full pack.
- `ble_tx_set_power`: applies TX power.

### 6.9 Web server — `web_config.c`
- Handlers: `/`(HTML), `/style.css`, `/app.js`, `GET /api/config`, `POST /api/config` (cJSON parse, validate, NVS save; Ed25519 `X-Signature` + rate limit 10 fails/60 s when `lock_level>=1`; eFuse lock when `>=2`), `GET /api/status`, `POST /api/reset`, `POST /ota` (SHA-256 + `X-Expected-SHA256` + optional signature), `GET /api/logs` (ring 64×240 B), `POST /api/command`.
- `log_init` installs a `vprintf` hook feeding the log ring.

### 6.10 OTA — `rid_ota.c`
- Boot: GPIO pull-low enters OTA mode → AP "RemoteID-OTA" + httpd (4 handlers) → **infinite loop** (`:333-335`).
- `/update`: lock check, SHA-256 streaming + `esp_ota_write`, `esp_ota_end`, `esp_ota_set_boot_partition`, `esp_restart`; `OTA_MAX_IDLE_STALLS=12` (~60 s) abort.
- `/factory_reset`: `nvs_storage_reset_preserve_keys` + restart.
- `/rollback`: `esp_ota_set_boot_partition(running)` + restart.

### 6.11 CLI — `cli.c`
- Commands: `help, status, config [get/set], restart, reboot, reset, factory, protocol, heap, log_level, patrol, transmit <x> <on|off>, mac, uptime, kalman`.
- Uses `esp_rid_get/set_config`/`get_state` (lock-protected).

### 6.12 NVS — `nvs_storage.c`
- Namespace `esp_rid`; typed helpers. `save`/`load`/`erase`/`reset_preserve_keys`. **Known gap**: many fields not persisted (see todo).

### 6.13 Auth / security — `rid_auth.c`, `rid_security.c`
- `rid_auth_init` (mbedTLS parse Ed25519 PEM), `rid_auth_sign_identity` (page-0 wire format). `rid_security`: strict base64, hex, SHA-256, signed-body verify against `public_keys[5]`.
- **Known gap**: key not persisted, init only at boot.

### 6.14 Kalman — `rid_kalman.c`
- 3×1D filters; timeout 3 s; derived speed/climb/heading.

### 6.15 LEDs — `led_status.c`, `led_ws2812.c`, `rid_lighting.c`
- Status LED: 7 states (BOOT/NO_GPS/GPS_OK/DEMO/LOCKED/OTA/ERROR) + TX flash (80 ms). WS2812: green/amber. Lighting: 5 patterns, inputs `armed`+`gps_valid`.

### 6.16 MAVLink TX / USB — `rid_mavlink_tx.c`, `rid_mavlink_usb.c`
- Heartbeat 1 s + ODID_SYSTEM every 6 s (from real operator location); USB mirror on console UART.

---

## 7) Shared data — who writes / who reads

Full field-by-field chains in `todolist/dataflow.md`. Highlights (all under `g_lock` except where noted):

| Data | Writers | Readers (may be unlocked) |
|---|---|---|
| `g_config` | NVS load, web POST, CLI, boot | `rid_task` (unlocked at `:308-338,490-497,528-530,557-565,614`) |
| `g_state.gps` | parsers→`rid_task` (locked), Kalman (unlocked), demo (locked) | TX builders (unlocked, `:310-334`), web/CLI (locked) |
| `g_state.identity` | `rid_task` | TX builders, gate, web |
| `active_protocol` | `rid_task` | TX gate, web, CLI |
| parser statics (`g_last_gps` etc.) | only the active parser | only `rid_task` |
| `ext_auth_pages` | MAVLink relay / `rid_auth` | TX builders |

---

## 8) TX decision process — `update_transmissions()` (`esp_remote_id.c:296-341`)

Gate chain (all must pass):
1. `gps_valid || bcast_powerup`
2. `active_protocol != UNKNOWN`
3. identity gate (if `RID_OPT_IDENTITY_READY_GATE` and not sane → block)
4. per-channel `rate_allowed(last_us, rate_hz)` → WiFi beacon, WiFi NAN, BLE4, BLE5
Each successful TX increments its counter + `transmissions_count` + LED flash.

Debug: check which link of the gate blocks — that is almost always the answer.

---

## 9) Debugging guide — symptom → where to look

| Symptom | Likely process / gate | Check first |
|---|---|---|
| No transmission at all | §8 gate chain | `/api/status`: `gps_valid`, `active_protocol`, `identity_ready`, counters; `tx_modes` |
| `active_protocol = UNKNOWN` in AUTO | detect returns UNKNOWN (`protocol_detect.c:47-49`) | is the FC actually sending? baud 115200 probe |
| Protocol flaps / data lost in AUTO | detect consumes UART bytes (todo: AUTO starvation) | set a fixed protocol to confirm |
| GPS stale after 10 s | §5 phase Q + parser freshness (5 s) | parser buffer overflow? wrong baud/pins? |
| MSP configured but no GPS | `msp_parser_get` gate `fix>=3` | gate `>=2`; baud; `$M<` frames on wire |
| MAVLink configured but no data | sysid filter (`mavlink_sysid`) | set 0 = any |
| BLE4 nothing on receiver | legacy ADV 31 B limit / counter rotation | check `ble4_count` increments; power |
| BLE5 nothing on receiver | ext adv only compiled on S3/C6 (`CONFIG_BT_BLE_50_EXTEND_ADV_EN`) | target support; `ble5_count` |
| Web UI unreachable | `webserver_en=0` or `lock_level>=2` (eFuse) | boot log; AP SSID visible? |
| Config reverts after reboot | §6.12 NVS gaps (todo) | `nvs_storage_save/load` missing fields |
| Auth pages missing | `rid_auth_init` only at boot + key not persisted | re-enter key, reboot |
| OTA page loads but upload stalls | OTA idle counter (`rid_ota.c:134-145`) | `OTA_MAX_IDLE_STALLS=12`, ~60 s |
| Watchdog reset loop | `rid_task` blocked on `g_lock` `portMAX_DELAY` (todo) | who holds the lock (web/CLI/NVS)? |
| LED always NO_GPS | `gps_valid` never set | parser data flow (§5 E/G) |
| Random reboot / garbage on UART0 | stack overflow, heap | `print_system_box` heap free; IDF backtrace |
| TX works but position is wrong | parser unit conversion (see `dataflow.md` 1.1-1.11) | verify raw values vs transmitted |

---

## 10) Known open issues (live list → `softwarestatus.md`)

- AUTO-mode UART starvation (urgent)
- `g_config` / `g_state` data races, `portMAX_DELAY` in `rid_task`
- NVS persistence gaps (auth key, ws2812, lighting, dronecan, start_delay_ms, protocol, pins)
- Parser split needed (pure `process_bytes` vs UART) → enables fuzz + host tests
- `xTaskCreate` / `esp_task_wdt_add` return codes unchecked
- BLE4/5 legacy on S3/C6 uses ext-adv instance 2 (`LEGACY_NONCONN`) — verified green in CI
