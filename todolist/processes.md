# ESP DRONE REMOTEID — Every Process Running (Debug Reference)

Last updated: 2026-08-14 (verified against `components/esp_remote_id/src` and `main/`)
Scope: every concurrent context, task, callback, logical process and data path in `ESP32_DRONE_REMOTE_ID_Firmware`.
Use this file when a feature "does not work correctly": find the process, check its gates, then its output.
Companion files: `todolist/dataflow.md` (field-by-field chains), `todolist/softwarestatus.md` (open todos).

---

## 1) Runtime model (FreeRTOS / ESP-IDF)

- **No app software timers, no timer callbacks, no event-handler registrations**: `esp_timer_create`, `xTimerCreate` and `esp_event_handler_register` are **not used** in `components/esp_remote_id/src`. Everything periodic is driven by the `rid_task` 100 ms loop, the `rid_mavlink_tx` 100 ms loop, or IDF stacks (WiFi / BLE / HTTP).
- **Drivers polled from tasks** (no event queues): UART1 RX, TWAI (CAN), all parser reads use `uart_read_bytes(..., timeout=0)`.
- **One global mutex** `g_lock` (`esp_remote_id.c:43`) protects `g_config` / `g_state`. Known weaknesses (unlocked reads in `rid_task`, `portMAX_DELAY`) are tracked in `softwarestatus.md`.
- **Only 3 application tasks exist** (`xTaskCreate` appears 3 times): `rid_task`, `cli_task`, `rid_mavlink_tx`. Nothing else is an app task.

---

## 2) Boot sequence — every step, in order

`app_main()` (`main/main.c:101-109`): `psa_crypto_init()` → `fix_mac_if_needed()` → `esp_rid_init()` → `esp_rid_start()` → `print_splash()`.

Inside `esp_rid_init()` (`esp_remote_id.c:133-224`):

| # | Step | Where | Note |
|---|------|-------|------|
| 1 | `nvs_storage_init` (erase+reinit if corrupted) | `nvs_storage.c:10-18` | `ESP_ERROR_CHECK`; crash here = NVS partition broken |
| 2 | `default_config(&g_config)` | `esp_remote_id.c:138` | defaults: proto AUTO, baud 57600, pins 17/18, TX=WiFi beacon, bcast_powerup=1, start_delay 10 s |
| 3 | `nvs_storage_load(&g_config)` | `esp_remote_id.c:139` | persisted fields only (see §6.12) |
| 4 | `rid_ota_check_and_run(&g_config)` | `esp_remote_id.c:142`, `rid_ota.c:304-338` | **if OTA GPIO pulled low → enters OTA mode and loops forever; the rest of boot never runs** |
| 5 | `g_lock = xSemaphoreCreateMutex()` | `esp_remote_id.c:144` | |
| 6 | `memset(&g_state, 0)` | `esp_remote_id.c:146` | |
| 7 | startup delay `start_delay_ms` (default 10 s) | `esp_remote_id.c:149-152` | `vTaskDelay`, blocks main |
| 8 | boot baud = 115200 (AUTO) else `baud_rate` | `esp_remote_id.c:155-156` | |
| 9 | `protocol_detect_init` → UART driver (RX 256 B) | `esp_remote_id.c:157`, `protocol_detect.c:18-40` | |
| 10 | `nmea/msp/mavlink_parser_init` + `mavlink_parser_set_sysid_filter` | `esp_remote_id.c:158-161` | all three share UART1 |
| 11 | `esp_netif_init()` | `esp_remote_id.c:163` | |
| 12 | `wifi_tx_init` (event loop, AP, power, MAC) | `esp_remote_id.c:165`, `wifi_tx.c:66-124` | |
| 13 | `ble_tx_init` (BT controller + Bluedroid) | `esp_remote_id.c:166`, `ble_tx.c:124-141` | |
| 14 | `ble_tx_set_power(9)` | `esp_remote_id.c:169` | |
| 15 | if `options & (bit6\|bit7)` or `mavlink_usb_enable` → `rid_mavlink_tx_init` + create task | `esp_remote_id.c:172-176` | |
| 16 | if `options & AUTH_ED25519` → `rid_auth_init(auth_private_key)`; `g_state.auth_enabled = rid_auth_enabled()` | `esp_remote_id.c:179-182` | key only parsed here, at boot |
| 17 | `led_ws2812_init` | `esp_remote_id.c:185` | RMT |
| 18 | if any `lighting_pins[i] >= 0` → `rid_lighting_init` | `esp_remote_id.c:188-194` | runs once, not per pin |
| 19 | if both dronecan pins set → `rid_dronecan_init` | `esp_remote_id.c:197-200` | TWAI install+start |
| 20 | if `mavlink_usb_enable` → `rid_mavlink_usb_init` | `esp_remote_id.c:203-205` | may fail if console owns the UART |
| 21 | if `uas_id=="ESP32-RID-001"` or `operator_id=="OP-UNKNOWN"` → MAC-based IDs + NVS save | `esp_remote_id.c:207-214` | |
| 22 | `led_status_reconfigure` | `esp_remote_id.c:216` | first LED init (LEDC) |
| 23 | `web_config_init(webserver_en)` | `esp_remote_id.c:217`, `web_config.c:727-755` | starts HTTP server, 10 handlers |
| 24 | `cli_init()` → creates `cli_task` | `esp_remote_id.c:219`, `cli.c:391-395` | |
| 25 | `rid_kalman_init(&g_kalman)` | `esp_remote_id.c:221` | |
| 26 | **`esp_rid_start()`** (separate call from main) creates `rid_task` | `esp_remote_id.c:660-666`, `main/main.c:106` | |

---

## 3) Complete task list

| Task | Created | Stack | Prio | Role |
|---|---|---|---|---|
| `main` | IDF | — | — | boot sequence (§2), then idle |
| `rid_task` | `esp_remote_id.c:670` | 4096 | 5 | core 100 ms loop (§5), subscribed to WDT |
| `cli_task` | `cli.c:393` | 4096 | 5 | reads UART0 stdin, executes commands (§6.11) |
| `rid_mavlink_tx` | `esp_remote_id.c:175` | 2048 | 3 | heartbeat 1 s + ODID_SYSTEM 6 s on UART1 + USB mirror (§6.16) — **only if** options bit6/bit7 or `mavlink_usb_enable` |
| `httpd` (web) | `web_config.c:740` | IDF default (4096) | 5 | config UI + `/api/*`; one task per open socket |
| `httpd` (OTA) | `rid_ota.c:274` | IDF default | 5 | only in OTA (GPIO) mode: `/`, `/update`, `/factory_reset`, `/rollback` |
| WiFi / BLE / sys_evt / esp_timer / ipc / idle | IDF | IDF | — | internal stacks |

Note: `xTaskCreate` / `esp_task_wdt_add` return values are **now checked** (error logged on failure) at `esp_remote_id.c:175,420,670`, `cli.c:393` — tracked in `softwarestatus.md`.

---

## 4) ISR / driver contexts

| Peripheral | Driver | Interrupt does | App polling |
|---|---|---|---|
| UART1 RX | `driver/uart` | RX FIFO → ring buffer (256 B) | parsers + detect: `uart_read_bytes(..., 0)` |
| UART0 (console) | console driver | stdin ring | `cli_task` `fgets` |
| TWAI/CAN | `driver/twai` | RX → queue (len 10) | `rid_dronecan_get` `twai_receive(0)` |
| RMT | `driver/rmt` | WS2812 TX | `led_ws2812_set_rgb` |
| LEDC | `driver/ledc` | PWM hardware | `led_status_tick` |
| GPIO (OTA trigger) | `driver/gpio` | none (polled once at boot) | `rid_ota.c:309-318` |
| USB Serial/JTAG | console | console RX | `rid_mavlink_usb_write` |

---

## 5) `rid_task` main loop — every phase (`esp_remote_id.c:414-663`)

Period 100 ms (`vTaskDelay` at `:659`), WDT reset at `:660`. `g_running` toggled by `esp_rid_start/stop`.

| Phase | Lines | What happens | If it goes wrong → |
|---|---|---|---|
| A | 416-420 | `active_protocol=UNKNOWN`; `esp_task_wdt_add(NULL)` (return checked, logs on failure) | no WDT coverage if add fails |
| B | 425-428 | brief lock: copy `protocol` + `options` to locals | |
| C | 430-433 | AUTO → `protocol_detect_auto()` (**consumes UART bytes, blocks up to 50 ms**); else `proto = cfg_proto` | AUTO starvation/misclassification (todo) |
| D | 435 | `g_state.active_protocol = proto` (unlocked write) | |
| E | 439-451 | dispatch active parser `*_get()` → UART read + parse + fill `gps_data` | wrong protocol → no data |
| F | 453-457 | if `!have_data && rid_dronecan_is_active()` → `rid_dronecan_get`; on success `active_protocol=NONE` | DroneCAN is non-functional today (see §6.5) |
| G | 459-469 | gate `have_data && lat!=0`; `force_tx = FORCE_ARM_OK && armed`; if `force_tx || fix>=2`: copy `gps_data`→`g_state.gps`, `gps_valid=true`, `last_update_ms` (all under lock) | |
| H | 471-479 | MAVLink only: `mavlink_parser_get_armed`/`get_sysid` → `g_state.mavlink_armed/sysid`; `gps.armed` overwritten | |
| I | 481-498 | identity: MAVLink if fresh & non-empty, else from `g_config` (**unlocked reads** at `:490-497`) | race with web/CLI |
| J | 500-506 | takeoff capture (first `fix>=3` with lat/lon ≠ 0) | |
| K | 508-512 | MSP/NMEA: `altitude_relative = altitude_msl - takeoff_alt` (if captured) | |
| L | 514-531 | operator loc: MAVLink if fresh (<30 s) → `operator_*` + `gps.operator_*`; else from `g_config` | stale MAVLink when proto≠MAVLink |
| M | 533 | give lock | |
| N | 535-547 | if `DONT_SAVE_BASIC_ID`: clear `uas_id`/`uas_id_2`; identity gate: `identity_ready=true` unless (`IDENTITY_READY_GATE` set AND `identity_is_sane()` or `position_is_sane()` false) | |
| O | 549-570 | **`else if DEMO_MODE`** (only when NO valid GPS this loop): under lock `rid_patrol_tick`, `gps_valid=true`, `had_gps=true`, `active_protocol=NONE`, identity/operator from config; `identity_ready=true` | demo stops as soon as a real fix arrives |
| P | 572-598 | Kalman (`KALMAN && !DEMO`): `rid_kalman_update` from **raw** `gps_data`, then `predict`, then if valid age (<3 s) **unlocked** overwrite of `g_state.gps` (lat/lon/alt/speed/climb/heading), `gps_valid=true`; else if `!had_gps` `gps_valid=false` | race: kalman writes without lock |
| Q | 600-607 | **absolute 10 s timeout** on `last_update_ms` → `gps_valid=false` + WARN | timeout too short/long |
| R | 609-612 | **only if `had_gps`** → `update_transmissions()` (§8); on success `led_status_tx_flash` | bcast_powerup is ineffective when no parser data (see §8) |
| S | 614-623 | LED state (LOCKED>DEMO>GPS_OK>NO_GPS) + `led_status_tick` | |
| T | 626-630 | WS2812 green (GPS) / amber (no GPS) | |
| U | 632-634 | `rid_lighting_set_state(armed, gps_valid)` + `tick` | |
| V | 636-643 | optional RID log line (`PRINT_RID_MAVLINK`) | |
| W | 645-651 | status box every 100 loops, system box every 500 | |
| X | 653-654 | `vTaskDelay(100 ms)` + `esp_task_wdt_reset()` | |

---

## 6) Subsystem processes

### 6.1 Protocol detection — `protocol_detect.c`
- `protocol_detect_init` (`:34-40`) installs UART driver (RX 256 B, no event queue) at boot baud.
- `protocol_detect_auto` (`:42-69`): reads up to 256 B with **50 ms block**; classifies `$M<` → MSP, `$G/$N` → NMEA, `0xFE/0xFD` header (length-plausible) → MAVLink, **no data → UNKNOWN, everything else → NMEA**. The bytes read are **consumed**; the active parser then reads only what remains → in AUTO, a busy MAVLink/NMEA stream is regularly eaten by the detector and can be misclassified (tracked in `softwarestatus.md`).
- `protocol_detect_reinit` (`:71-80`): `uart_driver_delete` + reinstall (called when baud changes via config).
- `DETECT_TIMEOUT_MS 1000` is defined but **unused** (dead code).

### 6.2 NMEA parser — `nmea_parser.c`
- State: `g_nmea_buf[256]` + index; reads 64 B/call non-blocking.
- Sentences: `$GPGGA/$GNGGA` (lat/lon/alt/baro/sats/fix), `$GPRMC/$GNRMC` (lat/lon/speed), `$GPVTG/$GNVTG` (heading, speed). Checksum **not** validated (only `*` stripped).
- Mapping: GGA fix 1 → `fix_type 1`, fix ≥2 → `fix_type 3`; alt → `altitude_msl` + `altitude_baro`; RMC/VTG speed in knots × 0.514444 → m/s.
- Gate in `nmea_parser_get` (`:115-137`): `fix_type >= 2 && lat != 0` (no freshness here — handled by the 10 s timeout in `rid_task`).
- Robustness: `parse_rmc` reads `fields[6][0]` without null-checking `fields[6]` (`:71`) — a truncated RMC sentence can crash the task.

### 6.3 MSP parser — `msp_parser.c`
- State: `g_msp_buf[256]`, frame complete when `idx >= 6 + size + 1`.
- Messages: `MSP_RAW_GPS(106)` (fix/sats/lat/lon/alt/speed/ground course), `MSP_ATTITUDE(108)` (heading=yaw/10), `MSP_STATUS(101)` (armed=flag&1). CRC = XOR.
- Gate: `fix_type >= 2 && lat != 0`.
- **⚠ SUSPECTED FRAMING BUG**: parser reads `msp_size = buf[4]`, `msp_type = buf[5]`, payload at 6, and CRC = XOR of `buf[3]` for `buf[4]+2` bytes (`:76-84`). Standard MSP v1 framing is `$M<` `size` `cmd` `payload` `crc` → size at **buf[3]**, cmd at **buf[4]**. Against a standard FC the CRC check almost always fails and real frames are rejected. **Verify against real captured traffic** (Betaflight spec: `$M< size type payload crc`).

### 6.4 MAVLink parser — `mavlink_parser.c`
- State: `g_mav_buf[512]`, one shared `mavlink_status_t`; reads 512 B/call non-blocking.
- `mavlink_parse_char` per byte; if `sysid_filter != 0` skip others; `g_mav_sysid` = last seen sysid.
- Handled messages: GLOBAL_POSITION_INT, GPS_RAW_INT, VFR_HUD, ATTITUDE, AHRS2, HEARTBEAT (armed), OPEN_DRONE_ID_* (LOCATION, BASIC_ID, OPERATOR_ID, SELF_ID, AUTHENTICATION, SYSTEM → operator location only, MESSAGE_PACK → decodes packed 0..5 types via `mav2odid`).
- Freshness: GPS position **5 s** (`:354`), identity **10 s**, operator location **30 s**.
- Note: `OPEN_DRONE_ID_SYSTEM` stores operator location only (never position — fixed) and `MESSAGE_PACK` location updates position only if lat/lon ≠ 0.

### 6.5 DroneCAN — `rid_dronecan.c`
- Init (`:79-112`): TWAI install/start, RX queue 10, bitrate 1M/500k/250k.
- `rid_dronecan_get` (`:114-143`): drains queue; sets `g_active=true` on **any** received message.
- **Effectively non-functional**: `decode_fix2` requires `len >= 32` but classic CAN DLC ≤ 8 (`:38`, documented in-code as unreachable); AHRS/Identity decoders are empty stubs (`:69-77`). So DroneCAN never yields GPS. `g_active` only toggles whether the fallback is attempted at all.

### 6.6 Demo patrol — `rid_patrol.c`
- Synthetic circle: home 41.9028/12.4964, radius 0.003°, `angle += 0.018 rad`/tick (~35 s lap), alt 50±20 m, speed 6±2 m/s, fix 2-4, sats 6-16, `armed=true`. Only active in DEMO mode (phase O).

### 6.7 WiFi TX — `wifi_tx.c`
- Init (`:66-124`): event loop, `esp_netif_create_default_wifi_ap`, `esp_wifi_init`, eFuse MAC (or random if invalid) + `esp_base_mac_addr_set`, `WIFI_STORAGE_RAM`, AP mode, SSID/password/channel from config (SSID ≤ 32, WPA2 if password else OPEN, max_conn 4, beacon 100 ms), `esp_wifi_start`, bandwidth 20 MHz, TX power = `wifi_power_dbm` in **quarter-dBm units** (value ×4).
- `wifi_tx_transmit` (`:248-274`): builds beacon pack via `odid_wifi_build_message_pack_beacon_frame` (RAW 6-byte MAC is fine — the lib uses it as a binary 802.11 MAC), 4-attempt fallback `{AP,STA,AP,STA} × {no-seq,seq}`; returns true on first success.
- `wifi_tx_transmit_nan` (`:276-297`): NAN action frame, **single** attempt on AP.
- `wifi_tx_reconfigure_ap` (`:131-165`): re-applies AP config + power (called on config change).
- `populate_uas_data` (`:167-246`): BasicID[0..1], Location, System(operator), SelfID, OperatorID, Auth (MAVLink-relayed pages first, else local Ed25519 sign; pages skipped if pack would overflow).

### 6.8 BLE TX — `ble_tx.c`
- Init (`:124-141`): BT controller (release classic), enable BLE, Bluedroid init+enable.
- `ble_tx_transmit_legacy` (`:143-187`): one 25 B ODID message per 31 B Service-Data adv (UUID 0xFFFA, app code 0x0D, counter), messages **rotated** per cycle. On S3/C6 (`CONFIG_BT_BLE_50_EXTEND_ADV_EN`): ext-adv **instance 2**, `LEGACY_NONCONN`, 1M PHY; on ESP32 classic: `config_adv_data_raw` + `start_advertising`, `ADV_TYPE_SCAN_IND`.
- `ble_tx_transmit_lr` (`:189-235`): full pack (≤254 B) on **instance 0** (1M, legacy-compatible) + **instance 1** (Coded PHY). Only if ext-adv enabled.
- `ble_tx_set_power` (`:237-248`): clamps to [-12..9] dBm, level = `(dbm+12)/3`.
- `ext_adv_instance` helper (`:100-120`): `set_params` + `config_data` + `start` with **return checks** — any failure logs an error and the TX returns `false`. (Classic-ESP32 `config_adv_data_raw`/`start_advertising` calls at `:179-180` still unchecked.)

### 6.9 Web server — `web_config.c`
- Init (`:727-755`): `log_init` installs a `vprintf` hook feeding a 64×240 B log ring; httpd on port 80 (max 16 handlers, LRU purge) + 10 handlers.
- Endpoints: `/` (HTML from embedded files), `/style.css`, `/app.js` (cache 86400), `GET /api/config`, `POST /api/config` (cJSON parse → validate ranges → `esp_rid_set_config` → NVS save), `GET /api/status`, `POST /api/reset` (factory reset + restart), `POST /ota` (SHA-256 streaming + `X-Expected-SHA256`; rejected at lock≥2; **no signature check**), `GET /api/logs`, `POST /api/command` (restart/reboot/reset/factory/status/…).
- Locking (`get_lock_level` `:67-78`): lock_level 2 is also **burned into eFuse** (`EFUSE_BLK3`, magic `RID!`) at config write time; ≥1 requires `X-Signature` (Ed25519 over SHA-256 body, verified against 5 public keys) for config POST, factory reset and privileged commands. Rate limit (10 fails/60 s, `:43-65`) applies to config POST and factory reset **only** — `/api/command` requires the signature but has **no rate limit**.

### 6.10 OTA — `rid_ota.c`
- Boot (GPIO): low on `ota_trigger_gpio` → AP `RemoteID-OTA` (open) + httpd (`:269-302`) + **infinite loop** (`:333-335`).
- `/update` (`:61-239`): rejected at lock≥2; `X-Expected-SHA256` mandatory at every level; `X-Signature` **mandatory at lock≥1** (stronger than the web `/ota`); streams with `esp_ota_write` + SHA-256; `esp_ota_end`/`set_boot_partition`/`esp_restart`; abort after `OTA_MAX_IDLE_STALLS=12` (~60 s) idle.
- `/factory_reset` (`:241-252`): `nvs_storage_reset_preserve_keys` + restart (differential reset keeps public keys).
- `/rollback` (`:254-267`): boot to previous partition + restart.

### 6.11 CLI — `cli.c`
- Task (`:353-389`): `fgets` on UART0 stdin, `parse_line`, dispatch. Commands: `help, status, config [set <field> <value>], restart, reboot, reset, factory, protocol, heap, log_level, patrol, transmit, mac, uptime, kalman`.
- `config set` fields: uas_id, operator_id, self_id, wifi_ssid, wifi_password, ua_type, id_type, wifi_channel, mavlink_sysid, bcast_powerup, webserver, lock_level, baud_rate, wifi_power_dbm, wifi/wifi_nan/ble4/ble5 rates+power, operator_lat/lon/alt, start_delay_ms. **Cannot** set ws2812, lighting, dronecan, ota_trigger_gpio or auth key.
- `lock_level` via CLI has **no eFuse handling** and no signature requirement (unlike web).
- All changes go through `esp_rid_set_config` (NVS save + live re-init of UART/AP/LED/BLE).

### 6.12 NVS — `nvs_storage.c`
- Namespace `esp_rid`. Persisted: uas_id, op_id, self_id, uas_id_2, wifi_ssid, wifi_pass, ua_type, id_type, ua_type_2, id_type_2, wifi_ch, websrv_en, mav_sysid, bcast_pwr, tx_modes, options, lock_lvl, led_r/g/b, baud, wifi_pwr/bcn/nan, bt4_rate/pwr, bt5_rate/pwr, op_lat/lon/alt, pubkey1..5.
- **NOT persisted** (lost on reboot): `protocol`, `uart_port`, `tx_pin`, `rx_pin`, `ws2812_gpio`, `ws2812_brightness`, `lighting_*`, `dronecan_*`, `mavlink_usb_enable`, `ota_trigger_gpio`, `auth_private_key`, `start_delay_ms` (tracked in `softwarestatus.md`).
- `operator_lat/lon` stored as **float** (`nvs_storage.c:118-119`) although the fields are double → precision loss at ~1 cm.
- `nvs_storage_reset_preserve_keys` (`:192-217`): erase-all then re-write pubkey1..5.

### 6.13 Auth / security — `rid_auth.c`, `rid_security.c`
- `rid_auth_init` (`rid_auth.c:13-52`): parses PEM Ed25519 key (must be 256 bit), called **only at boot**; `rid_auth_enabled()` = initialized && enabled.
- `rid_auth_sign_identity` (`:59-108`): pure Ed25519 sign of `uas_id` (`mbedtls_pk_sign`, `MBEDTLS_MD_NONE`), paginated (page 0 = zero-data size, others = nonzero size), `ODID_AUTH_UAS_ID_SIGNATURE`.
- `rid_security_verify_signed_body` (`rid_security.c:96-164`): SHA-256 of body, `mbedtls_pk_verify` with `MBEDTLS_MD_SHA256` (Ed25519-ph over SHA-256) against each of the 5 public keys (supports raw PEM or `PUBLIC_KEYV1:` base64).
- Note: local identity signing uses **pure Ed25519** while web verification uses **Ed25519-ph(SHA-256)** — two different schemes; each is internally consistent with its counterpart.
- `auth_private_key` not persisted in NVS → re-entering it is lost after reboot unless stored via web (web field exists but save misses it).

### 6.14 Kalman — `rid_kalman.c`
- 3×1D filters; `RID_KALMAN_TIMEOUT_US = 3 s` (`rid_kalman.h:7`); lat/lon in degrees with velocity in deg/s; speed/climb/heading derived from filter velocities (`rid_kalman_get` `:98-123`).

### 6.15 LEDs — `led_status.c`, `led_ws2812.c`, `rid_lighting.c`
- Status LED (LEDC, 5 kHz): states BOOT(blue pulse), NO_GPS(amber 1 Hz), GPS_OK(green solid), DEMO(purple pulse), LOCKED(red double), OTA(rainbow), ERROR(red 4 Hz); TX flash = white 80 ms override (`led_status.c:180-196`).
- WS2812 (RMT): `set_rgb` scales by brightness (`led_ws2812.c:69-84`), GRB order; green on GPS, amber otherwise.
- Lighting (GPIO): 6 patterns (off/solid/blink-slow/blink-fast/blink-armed/flash-on-gps), per-channel phase offset; inputs `armed` + `gps_valid`.

### 6.16 MAVLink TX / USB — `rid_mavlink_tx.c`, `rid_mavlink_usb.c`
- Task loop 100 ms (`rid_mavlink_tx.c:33-73`): HEARTBEAT (MAV_TYPE_ODID, state ACTIVE) every 1 s; ODID_SYSTEM every 6 s with operator location from parser if fresh, else 0/0/-1000. Writes to **the same UART1** used by the parser + mirrors to USB.
- USB (`rid_mavlink_usb.c:12-45`): installs UART driver on `CONFIG_ESP_CONSOLE_UART_NUM` at 115200. **Likely fails if the console already owns that UART** (returns false, logged) — and if it succeeds it shares the console port with CLI output.

---

## 7) Shared state — who writes / who reads

Full field-by-field chains in `todolist/dataflow.md`.

| Data | Writers | Readers (unlocked marked) |
|---|---|---|
| `g_config` | NVS load, web POST, CLI, boot MAC fix | `rid_task` (unlocked: `:308-338,490-497,528-530,557-565,614`), TX builders |
| `g_state.gps` | `rid_task` (locked G/L), Kalman (unlocked `:588-594`), demo (locked) | TX builders (unlocked `:310-334`), web/CLI (locked copy) |
| `g_state.identity` | `rid_task` (locked I) | TX builders, gate (unlocked), web |
| `active_protocol` | `rid_task` (`:435`) | TX gate, web, CLI |
| parser statics (`g_last_gps`, buffers) | active parser only | only `rid_task` |
| `g_kalman` | `rid_task` (unlocked) | status box (unlocked), CLI |
| log ring | `vprintf` hook (any task) | `/api/logs` |

---

## 8) TX decision chain — `update_transmissions()` (`esp_remote_id.c:296-341`)

Called **only when `had_gps`** (valid GPS this loop, or demo). Gates in order:

1. `g_state.gps_valid || g_config.bcast_powerup` — else return false (note: with no parser data `had_gps=false`, so `update_transmissions` is not called at all and `bcast_powerup` has no effect in practice).
2. `active_protocol != RID_PROTOCOL_UNKNOWN` — else return false.
3. if `IDENTITY_READY_GATE` option: `identity_ready` must be true.
4. per-mode `rate_allowed(last_us, rate_hz)` (`:282-292`, `esp_timer_get_time`, rate ≤ 0 disabled):
   - WiFi beacon → `wifi_tx_transmit` + `wifi_bcn_count`++
   - WiFi NAN → `wifi_tx_transmit_nan` (+ `g_nan_counter`)
   - BLE4 → `ble_tx_transmit_legacy`
   - BLE5 → `ble_tx_transmit_lr`
Each success increments the per-channel counter + `transmissions_count` + LED flash.

Debug: find which gate blocks — that is usually the answer.

---

## 9) Debugging guide — symptom → process → check

| Symptom | Likely process / gate | Check first |
|---|---|---|
| No transmission at all | §8 chain | `/api/status`: `gps_valid`, `active_protocol`, counters; `tx_modes` in config |
| `active_protocol = UNKNOWN` in AUTO | `protocol_detect_auto` returns UNKNOWN only on **no data** (`protocol_detect.c:47-49`); any other garbage → NMEA | is the FC actually sending? baud (probe 115200 in AUTO); wiring/pins |
| Protocol flaps / data lost in AUTO | detector consumes bytes + default-NMEA fallback (`protocol_detect.c:42-69`) | set a fixed protocol to confirm |
| GPS stale after 10 s | §5 Q + parser freshness (MAVLink 5 s) | parser buffer overflow? baud/pins? |
| MSP configured but no GPS | §6.3 **suspected framing off-by-one** | compare against real captured `$M<` frames |
| MAVLink configured but no data | `sysid_filter` (`mavlink_parser.c:48-51,100`) | set 0 = any sysid; freshness 5 s |
| BLE4 nothing received | legacy 31 B rotation (`ble_tx.c:143-187`) | `ble4_count` increments? power; ext-adv instance 2 |
| BLE5 nothing received | ext adv only compiled on S3/C6 (`CONFIG_BT_BLE_50_EXTEND_ADV_EN`) | target support; `ble5_count` |
| Web UI unreachable | `webserver_en=0` or eFuse lock | boot log; AP SSID visible? |
| Config reverts after reboot | §6.12 NVS gaps | `nvs_storage_save/load` missing fields |
| Auth pages missing | `rid_auth_init` at boot only + key not persisted | re-enter key, reboot; check bitlen 256 |
| OTA page loads but stalls | OTA idle counter (`rid_ota.c:134-145`) | ~60 s stall timeout |
| Watchdog reset loop | `rid_task` blocked on `g_lock` `portMAX_DELAY` (`esp_remote_id.c:425,466,550`) | who holds the lock (web/CLI/NVS)? |
| LED always NO_GPS | `gps_valid` never set | §5 phases E/G; parser gates |
| TX works but position wrong | unit conversion (see `dataflow.md` §1) | raw values vs transmitted |
| DroneCAN never gives position | §6.5 (reassembly missing) | not a config issue |

---

## 10) Known issues and suspects (→ `softwarestatus.md` for status)

- AUTO-mode UART starvation / default-NMEA misclassification (urgent).
- `g_config` / `g_state` races; `portMAX_DELAY` in `rid_task`; WDT add unchecked.
- NVS persistence gaps (protocol, pins, ws2812, lighting, dronecan, mavlink_usb, ota_gpio, auth key, start_delay_ms).
- **⚠ MSP framing off-by-one** (verify with real traffic).
- **⚠ `rid_mavlink_usb` likely conflicts with console UART** (verify at runtime).
- Web `/ota` has **no signature check** at lock=1 (unlike GPIO-mode `/update`).
- DroneCAN `decode_fix2` unreachable (no multi-frame reassembly).
- `bcast_powerup` effectively dead (TX only when `had_gps`).
- Parser split needed (pure `process_bytes` vs UART) → enables fuzz + host tests.
- `operator_lat/lon` stored as float in NVS (precision).
