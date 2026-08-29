# OmniRID — Every Process Running (Rust Firmware)

> **Rust firmware architecture** — `OmniRID/` workspace.
> Replaces legacy C firmware `ESP32_DRONE_REMOTE_ID_Firmware/` (deleted).
> Same FreeRTOS task architecture (rid_task loop, CLI task, web server) but organized as Rust crates.
> The same logical processes and gate chains apply.

Last updated: 2026-08-28 (audited against `OmniRID/firmware/` and `OmniRID/inputs/outputs/` Rust sources).
Scope: every concurrent context, task, callback, logical process and data path in the Rust firmware.
Use this file when a feature "does not work correctly": find the process, check its gates, then its output.
Companion files: `todolist/dataflow.md` (field-by-field chains), `todolist/softwarestatus.md` (open todos).

---

## Crate mapping (C → Rust)

| Legacy C file | Rust crate | Rust file(s) |
|---|---|---|
| `main/main.c` | `app` | `src/main.rs`, `src/controller.rs` |
| `esp_remote_id.c` | `app` + `rid-core` | `controller.rs`, `rid-core/src/hub.rs`, `rid-core/src/scheduler.rs` |
| `esp_remote_id.h` | `rid-interface` | `src/types.rs` |
| `protocol_detect.c` | `rid-core` | `src/protocol_detect.rs` |
| `nmea_parser.c` | `proto-nmea` | `inputs/proto-nmea/src/parser.rs` |
| `msp_parser.c` | `proto-msp` | `inputs/proto-msp/src/parser.rs` |
| `mavlink_parser.c` | `proto-mavlink` | `inputs/proto-mavlink/src/parser.rs` |
| `rid_dronecan.c` | `proto-dronecan` | `inputs/proto-dronecan/src/parser.rs` |
| `rid_patrol.c` | `rid-core` | `src/patrol.rs` |
| `wifi_tx.c` / `wifi.c` | `bsp-esp32` + `out-astm` | `bsp-esp32/src/wifi.rs`, `out-astm/src/wifi.rs` |
| `ble_tx.c` | `bsp-esp32` + `rid-app` | `bsp-esp32/src/ble.rs`, `rid-app/src/ble4.rs` |
| `rid_output.c/h` | `rid-core` + `out-astm` | `rid-core/src/hub.rs`, `out-astm/src/lib.rs` |
| `web_config.c` | `rid-app` + `bsp-esp32` | `rid-app/src/web_config.rs`, `bsp-esp32/src/web.rs` |
| `rid_ota.c` | `rid-app` + `bsp-esp32` | `rid-app/src/ota.rs`, `bsp-esp32/src/ota.rs` |
| `cli.c` | `rid-app` | `src/cli.rs` |
| `nvs_storage.c` | `rid-app` + `bsp-esp32` | `rid-app/src/nvs.rs`, `bsp-esp32/src/nvs.rs` |
| `rid_auth.c` | `rid-core` | `src/auth.rs` |
| `rid_security.c` | `rid-core` | `src/security.rs` |
| `rid_kalman.c` | `rid-core` | `src/kalman.rs` |
| `led_status.c` | `rid-app` | `src/led_status.rs` |
| `led_ws2812.c` | `bsp-esp32` + `rid-app` | `bsp-esp32/src/led.rs`, `rid-app/src/led_ws2812.rs` |
| `rid_lighting.c` | `rid-app` | `src/lighting.rs` |
| `rid_mavlink_tx.c` | `proto-usb-mavlink` | `inputs/proto-usb-mavlink/src/tx.rs` |
| `rid_mavlink_usb.c` | `proto-usb-mavlink` | `inputs/proto-usb-mavlink/src/lib.rs` |
| `opendroneid.c` + `mav2odid.c` | `opendroneid-sys` | `external-libs/opendroneid-sys/src/lib.rs` (FFI bindings) |

---

## 1) Runtime model (FreeRTOS / ESP-IDF)

- **No app software timers, no timer callbacks, no event-handler registrations**: `esp_timer_create`, `xTimerCreate` and `esp_event_handler_register` are **not used** in the app crate. Everything periodic is driven by the `rid_task` 100 ms loop, the `rid_mavlink_tx` 100 ms loop, or IDF stacks (WiFi / BLE / HTTP).
- **Drivers polled from tasks** (no event queues): UART1 RX, TWAI (CAN), all parser reads use `uart_read_bytes(..., timeout=0)`.
- **One global mutex** `g_lock` protects `g_config` / `g_state`. Known weaknesses (unlocked reads in `rid_task`, `portMAX_DELAY`) are tracked in `softwarestatus.md`.
- **Only 3 application tasks exist** (`xTaskCreate` appears 3 times): `rid_task`, `cli_task`, `rid_mavlink_tx`. Nothing else is an app task.

---

## 2) Boot sequence — every step, in order

`app_main()` (`app/src/main.rs`): `psa_crypto_init()` → `fix_mac_if_needed()` → `esp_rid_init()` → `esp_rid_start()` → `print_splash()`.

Inside `esp_rid_init()` (`app/src/controller.rs`):

| # | Step | Where | Note |
|---|------|-------|------|
| 1 | `nvs_storage_init` (erase+reinit if corrupted) | `rid-app/src/nvs.rs` + `bsp-esp32/src/nvs.rs` | `ESP_ERROR_CHECK`; crash here = NVS partition broken |
| 2 | `default_config(&g_config)` | `rid-app/src/config.rs` | defaults: proto AUTO, baud 57600, pins 17/18, TX=WiFi beacon, bcast_powerup=1, start_delay 10 s, region AUTO |
| 3 | `nvs_storage_load(&g_config)` | `rid-app/src/nvs.rs` | persisted fields only (see §6.12); `region` key loaded here |
| 3b | `active_standard` / `standard_fallback` bound from region | `rid-core/src/hub.rs` | `hub::active_standard()` + `hub::has_encoder()`; also re-bound in `set_config` |
| 4 | `rid_ota_check_and_run(&g_config)` | `rid-app/src/ota.rs` + `bsp-esp32/src/ota.rs` | **if OTA GPIO pulled low → enters OTA mode and loops forever; the rest of boot never runs** |
| 5 | `g_lock = xSemaphoreCreateMutex()` | `app/src/controller.rs` | |
| 6 | `memset(&g_state, 0)` | `app/src/controller.rs` | |
| 7 | startup delay `start_delay_ms` (default 10 s) | `app/src/controller.rs` | `vTaskDelay`, blocks main |
| 8 | boot baud = 115200 (AUTO) else `baud_rate` | `app/src/controller.rs` | |
| 9 | `protocol_detect_init` → UART driver (RX 256 B) | `rid-core/src/protocol_detect.rs` | |
| 10 | `nmea/msp/mavlink_parser_init` + `mavlink_parser_set_sysid_filter` | `app/src/controller.rs` | all three share UART1 |
| 11 | `esp_netif_init()` | `app/src/controller.rs` | |
| 12 | `wifi_tx_init` (event loop, AP, power, MAC) | `bsp-esp32/src/wifi.rs` | |
| 13 | `ble_tx_init` (BT controller + Bluedroid) | `bsp-esp32/src/ble.rs` | |
| 14 | `ble_tx_set_power(9)` | `app/src/controller.rs` | |
| 15 | if `options & (bit6|bit7)` or `mavlink_usb_enable` → `rid_mavlink_tx_init` + create task | `app/src/controller.rs` | |
| 16 | if `options & AUTH_ED25519` → `rid_auth_init(auth_private_key)`; `g_state.auth_enabled = rid_auth_enabled()` | `rid-core/src/auth.rs` | key only parsed here, at boot |
| 17 | `led_ws2812_init` | `bsp-esp32/src/led.rs` | RMT |
| 18 | if any `lighting_pins[i] >= 0` → `rid_lighting_init` | `rid-app/src/lighting.rs` | runs once, not per pin |
| 19 | if both dronecan pins set → `rid_dronecan_init` | `inputs/proto-dronecan/src/parser.rs` | TWAI install+start |
| 20 | if `mavlink_usb_enable` → `rid_mavlink_usb_init` | `inputs/proto-usb-mavlink/src/lib.rs` | may fail if console owns the UART |
| 21 | if `uas_id=="ESP32-RID-001"` or `operator_id=="OP-UNKNOWN"` → MAC-based IDs + NVS save | `app/src/controller.rs` | |
| 22 | `led_status_reconfigure` | `rid-app/src/led_status.rs` | first LED init (LEDC) |
| 23 | `web_config_init(webserver_en)` | `rid-app/src/web_config.rs` + `bsp-esp32/src/web.rs` | starts HTTP server, 10 handlers |
| 24 | `cli_init()` → creates `cli_task` | `rid-app/src/cli.rs` | |
| 25 | `rid_kalman_init(&g_kalman)` | `rid-core/src/kalman.rs` | |
| 26 | **`esp_rid_start()`** (separate call from main) creates `rid_task` | `app/src/controller.rs` | |

---

## 3) Complete task list

| Task | Created | Stack | Prio | Role |
|---|---|---|---|---|
| `main` | IDF | — | — | boot sequence (§2), then idle |
| `rid_task` | `app/src/controller.rs` | 4096 | 5 | core 100 ms loop (§5), subscribed to WDT |
| `cli_task` | `rid-app/src/cli.rs` | 4096 | 5 | reads UART0 stdin, executes commands (§6.11) |
| `rid_mavlink_tx` | `inputs/proto-usb-mavlink/src/tx.rs` | 2048 | 3 | heartbeat 1 s + ODID_SYSTEM 6 s on UART1 + USB mirror (§6.16) — **only if** options bit6/bit7 or `mavlink_usb_enable` |
| `httpd` (web) | `rid-app/src/web_config.rs` | IDF default (4096) | 5 | config UI + `/api/*`; one task per open socket |
| `httpd` (OTA) | `rid-app/src/ota.rs` | IDF default | 5 | only in OTA (GPIO) mode: `/`, `/update`, `/factory_reset`, `/rollback` |
| WiFi / BLE / sys_evt / esp_timer / ipc / idle | IDF | IDF | — | internal stacks |

Note: `xTaskCreate` / `esp_task_wdt_add` return values are **now checked** (error logged on failure) — tracked in `softwarestatus.md`.

---

## 4) ISR / driver contexts

| Peripheral | Driver | Interrupt does | App polling |
|---|---|---|---|
| UART1 RX | `driver/uart` | RX FIFO → ring buffer (256 B) | parsers + detect: `uart_read_bytes(..., 0)` |
| UART0 (console) | console driver | stdin ring | `cli_task` `fgets` |
| TWAI/CAN | `driver/twai` | RX → queue (len 10) | `rid_dronecan_get` `twai_receive(0)` |
| RMT | `driver/rmt` | WS2812 TX | `led_ws2812_set_rgb` |
| LEDC | `driver/ledc` | PWM hardware | `led_status_tick` |
| GPIO (OTA trigger) | `driver/gpio` | none (polled once at boot) | `rid_ota_check_and_run` |
| USB Serial/JTAG | console | console RX | `rid_mavlink_usb_write` |

---

## 5) `rid_task` main loop — every phase (`app/src/controller.rs`)

Period 100 ms (`vTaskDelay`), WDT reset. `g_running` toggled by `esp_rid_start/stop`.

| Phase | What happens | If it goes wrong → |
|---|---|---|
| A | `active_protocol=UNKNOWN`; `esp_task_wdt_add(NULL)` (return checked, logs on failure) | no WDT coverage if add fails |
| B | brief lock: copy `protocol` + `options` to locals | |
| C | AUTO → `protocol_detect_auto()` (**consumes UART bytes, blocks up to 50 ms**); else `proto = cfg_proto` | AUTO starvation/misclassification (todo) |
| D | `g_state.active_protocol = proto` (unlocked write) | |
| E | dispatch active parser `*_get()` → UART read + parse + fill `gps_data` | wrong protocol → no data |
| F | if `!have_data && rid_dronecan_is_active()` → `rid_dronecan_get`; on success `active_protocol=NONE` | DroneCAN is non-functional today (see §6.5) |
| G | gate `have_data && lat!=0`; `force_tx = FORCE_ARM_OK && armed`; if `force_tx || fix>=2`: copy `gps_data`→`g_state.gps`, `gps_valid=true`, `last_update_ms` (all under lock) | |
| H | MAVLink only: `mavlink_parser_get_armed`/`get_sysid` → `g_state.mavlink_armed/sysid`; `gps.armed` overwritten | |
| I | identity: MAVLink if fresh & non-empty, else from `g_config` (**unlocked reads**) | race with web/CLI |
| J | takeoff capture (first `fix>=3` with lat/lon ≠ 0) | |
| K | MSP/NMEA: `altitude_relative = altitude_msl - takeoff_alt` (if captured) | |
| L | operator loc: MAVLink if fresh (<30 s) → `operator_*` + `gps.operator_*`; else from `g_config` | stale MAVLink when proto≠MAVLink |
| M | give lock | |
| N | if `DONT_SAVE_BASIC_ID`: clear `uas_id`/`uas_id_2`; identity gate: `identity_ready=true` unless (`IDENTITY_READY_GATE` set AND `identity_is_sane(identity, region_rules)` or `position_is_sane()` false); `region_rules = rid_output_region_rules(g_config.region)` | |
| O | **else if DEMO_MODE** (only when NO valid GPS this loop): under lock `rid_patrol_tick`, `gps_valid=true`, `had_gps=true`, `active_protocol=NONE`, identity/operator from config; `identity_ready=true` | demo stops as soon as a real fix arrives |
| P | Kalman (`KALMAN && !DEMO`): `rid_kalman_update` from **raw** `gps_data`, then `predict`, then if valid age (<3 s) **unlocked** overwrite of `g_state.gps` (lat/lon/alt/speed/climb/heading), `gps_valid=true`; else if `!had_gps` `gps_valid=false` | race: kalman writes without lock |
| Q | **absolute 10 s timeout** on `last_update_ms` → `gps_valid=false` + WARN | timeout too short/long |
| R | **only if `had_gps`** → `update_transmissions()` (§8); on success `led_status_tx_flash` | bcast_powerup is ineffective when no parser data (see §8) |
| S | LED state (LOCKED>DEMO>GPS_OK>NO_GPS) + `led_status_tick` | |
| T | WS2812 green (GPS) / amber (no GPS) | |
| U | `rid_lighting_set_state(armed, gps_valid)` + `tick` | |
| V | optional RID log line (`PRINT_RID_MAVLINK`) | |
| W | status box every 100 loops, system box every 500 | |
| X | `vTaskDelay(100 ms)` + `esp_task_wdt_reset()` | |

---

## 6) Subsystem processes

### 6.1 Protocol detection — `rid-core/src/protocol_detect.rs`
- `protocol_detect_init` installs UART driver (RX 256 B, no event queue) at boot baud.
- `protocol_detect_auto`: reads up to 256 B with **50 ms block**; classifies `$M<` → MSP, `$G/$N` → NMEA, `0xFE/0xFD` header (length-plausible) → MAVLink, **no data → UNKNOWN, everything else → NMEA**. The bytes read are **consumed**; the active parser then reads only what remains → in AUTO, a busy MAVLink/NMEA stream is regularly eaten by the detector and can be misclassified (tracked in `softwarestatus.md`).
- `protocol_detect_reinit`: `uart_driver_delete` + reinstall (called when baud changes via config).

### 6.2 NMEA parser — `inputs/proto-nmea/src/parser.rs`
- State: `g_nmea_buf[256]` + index; reads 64 B/call non-blocking.
- Sentences: `$GPGGA/$GNGGA` (lat/lon/alt/baro/sats/fix), `$GPRMC/$GNRMC` (lat/lon/speed), `$GPVTG/$GNVTG` (heading, speed). Checksum validated.
- Mapping: GGA fix 1 → `fix_type 1`, fix ≥2 → `fix_type 3`; alt → `altitude_msl` + `altitude_baro`; RMC/VTG speed in knots × 0.514444 → m/s.
- Gate in `nmea_parser_get`: `fix_type >= 2 && lat != 0` (no freshness here — handled by the 10 s timeout in `rid_task`).

### 6.3 MSP parser — `inputs/proto-msp/src/parser.rs`
- State: `g_msp_buf[256]`, frame complete when `idx >= 6 + size + 1`.
- Messages: `MSP_RAW_GPS(106)` (fix/sats/lat/lon/alt/speed/ground course), `MSP_ATTITUDE(108)` (heading=yaw/10), `MSP_STATUS(101)` (armed=flag&1). CRC = XOR.
- Gate: `fix_type >= 2 && lat != 0`.

### 6.4 MAVLink parser — `inputs/proto-mavlink/src/parser.rs`
- State: `g_mav_buf[512]`, one shared `mavlink_status_t`; reads 512 B/call non-blocking.
- `mavlink_parse_char` per byte; if `sysid_filter != 0` skip others; `g_mav_sysid` = last seen sysid.
- Handled messages: GLOBAL_POSITION_INT, GPS_RAW_INT, VFR_HUD, ATTITUDE, AHRS2, HEARTBEAT (armed), OPEN_DRONE_ID_* (LOCATION, BASIC_ID, OPERATOR_ID, SELF_ID, AUTHENTICATION, SYSTEM → operator location only, MESSAGE_PACK → decodes packed 0..5 types via `mav2odid`).
- Freshness: GPS position **5 s**, identity **10 s**, operator location **30 s**.
- Note: `OPEN_DRONE_ID_SYSTEM` stores operator location only (never position) and `MESSAGE_PACK` location updates position only if lat/lon ≠ 0.

### 6.5 DroneCAN — `inputs/proto-dronecan/src/parser.rs`
- Init: TWAI install/start, RX queue 10, bitrate 1M/500k/250k.
- `rid_dronecan_get`: drains queue; sets `g_active=true` on **any** received message.
- **FIXED [#19]**: `decode_fix2` is now reachable thanks to full multi-frame (FT0/FT1) transfer reassembly (`TransferReceiver`, TID/toggle/timeout/CRC) in `inputs/proto-dronecan/src/parser.rs`; Fix2 position is decoded and tested. AHRS/Identity wire-format stubs remain a separate backlog item.

### 6.6 Demo patrol — `rid-core/src/patrol.rs`
- Synthetic circle: home 41.9028/12.4964, radius 0.003°, `angle += 0.018 rad`/tick (~35 s lap), alt 50±20 m, speed 6±2 m/s, fix 2-4, sats 6-16, `armed=true`. Only active in DEMO mode (phase O).

### 6.7 WiFi TX — `bsp-esp32/src/wifi.rs` + `out-astm/src/wifi.rs`
- Init: event loop, `esp_netif_create_default_wifi_ap`, `esp_wifi_init`, eFuse MAC (or random if invalid) + `esp_base_mac_addr_set`, `WIFI_STORAGE_RAM`, AP mode, SSID/password/channel from config (SSID ≤ 32, WPA2 if password else OPEN, max_conn 4, beacon 100 ms), `esp_wifi_start`, bandwidth 20 MHz, TX power = `wifi_power_dbm` in **quarter-dBm units** (value ×4).
- `wifi_tx_transmit`: builds the UAS pack via **`rid_core::hub::build_uas()`** (region-gated, exclusive standard), then `odid_wifi_build_message_pack_beacon_frame` (RAW 6-byte MAC is fine — the lib uses it as a binary 802.11 MAC), 4-attempt fallback `{AP,STA,AP,STA} × {no-seq,seq}`; returns true on first success.
- `wifi_tx_transmit_nan`: NAN action frame via `build_uas()`, **single** attempt on AP.

### 6.7a Output hub — `rid-core/src/hub.rs` + `out-astm/src/lib.rs`
- The single choke point every transport calls: `build_uas(gps, identity, cfg, signed_auth)` binds the neutral GPS+identity state to the **exclusive standard** selected by `cfg.region` and copies the identity fields that region allows.
- `hub::active_standard(cfg)`: AUTO→ASTM; EUR/FAA/JPN/SGP/KOR/CAN/AUS/BRA/NZL→ASTM today; CHN→CHN_GB (**no encoder yet** → `hub::has_encoder()` false → falls back to ASTM with warning).
- `hub::region_rules(region)` + `g_region_rules[]`: per-region gating — `require_operator_id` only for AUTO/EUR/FAA/CAN/AUS; CHN suppresses OperatorID, SelfID and the second BasicID (GB 42590 constraint).
- Adding a new standard = a new encoder function that plugs in behind `build_uas`; adding a new input = nothing here (inputs feed the neutral state).

### 6.8 BLE TX — `bsp-esp32/src/ble.rs` + `rid-app/src/ble4.rs`
- Init: BT controller (release classic), enable BLE, Bluedroid init+enable.
- `ble_tx_transmit_legacy`: builds the pack via `build_uas()`, one 25 B ODID message per 31 B Service-Data adv (UUID 0xFFFA, app code 0x0D, counter), messages **rotated** per cycle. On S3/C6: ext-adv **instance 2**, `LEGACY_NONCONN`, 1M PHY; on ESP32 classic: `config_adv_data_raw` + `start_advertising`, `ADV_TYPE_SCAN_IND`.
- `ble_tx_transmit_lr`: full pack (≤254 B) via `build_uas()` on **instance 0** (1M, legacy-compatible) + **instance 1** (Coded PHY). Only if ext-adv enabled.
- `ble_tx_set_power`: clamps to [-12..9] dBm, level = `(dbm+12)/3`.

### 6.9 Web server — `rid-app/src/web_config.rs` + `bsp-esp32/src/web.rs`
- Init: `log_init` installs a `vprintf` hook feeding a 64×240 B log ring; httpd on port 80 (max 16 handlers, LRU purge) + 10 handlers.
- Endpoints: `/` (HTML from embedded files), `/style.css`, `/app.js` (cache 86400), `GET /api/config`, `POST /api/config` (JSON parse → validate ranges → `esp_rid_set_config` → NVS save), `GET /api/status`, `POST /api/reset` (factory reset + restart), `POST /ota` (SHA-256 streaming + `X-Expected-SHA256`; rejected at lock≥2; **no signature check**), `GET /api/logs`, `POST /api/command` (restart/reboot/reset/factory/status/…).
- Locking (`get_lock_level`): lock_level 2 is also **burned into eFuse** (`EFUSE_BLK3`, magic `RID!`) at config write time; ≥1 requires `X-Signature` (Ed25519 over SHA-256 body, verified against 5 public keys) for config POST, factory reset and privileged commands. Rate limit (10 fails/60 s) applies to config POST and factory reset **only** — `/api/command` requires the signature but has **no rate limit**.

### 6.10 OTA — `rid-app/src/ota.rs` + `bsp-esp32/src/ota.rs`
- Boot (GPIO): low on `ota_trigger_gpio` → AP `RemoteID-OTA` (open) + httpd + **infinite loop**.
- `/update`: rejected at lock≥2; `X-Expected-SHA256` mandatory at every level; `X-Signature` **mandatory at lock≥1** (stronger than the web `/ota`); streams with `esp_ota_write` + SHA-256; `esp_ota_end`/`set_boot_partition`/`esp_restart`; abort after `OTA_MAX_IDLE_STALLS=12` (~60 s) idle.
- `/factory_reset`: `nvs_storage_reset_preserve_keys` + restart (differential reset keeps public keys).
- `/rollback`: boot to previous partition + restart.

### 6.11 CLI — `rid-app/src/cli.rs`
- Task: `fgets` on UART0 stdin, `parse_line`, dispatch. Commands: `help, status, config [set <field> <value>], restart, reboot, reset, factory, protocol, heap, log_level, patrol, transmit, mac, uptime, kalman`.
- `config set` fields: uas_id, operator_id, self_id, wifi_ssid, wifi_password, ua_type, id_type, wifi_channel, mavlink_sysid, bcast_powerup, webserver, lock_level, baud_rate, wifi_power_dbm, wifi/wifi_nan/ble4/ble5 rates+power, operator_lat/lon/alt, start_delay_ms, **region**. **Cannot** set ws2812, lighting, dronecan, ota_trigger_gpio or auth key.
- `lock_level` via CLI has **no eFuse handling** and no signature requirement (unlike web).
- All changes go through `esp_rid_set_config` (NVS save + live re-init of UART/AP/LED/BLE).

### 6.12 NVS — `rid-app/src/nvs.rs` + `bsp-esp32/src/nvs.rs`
- Namespace `esp_rid`. Persisted: uas_id, op_id, self_id, uas_id_2, wifi_ssid, wifi_pass, ua_type, id_type, ua_type_2, id_type_2, wifi_ch, websrv_en, mav_sysid, bcast_pwr, tx_modes, **region**, options, lock_lvl, led_r/g/b, baud, wifi_pwr/bcn/nan, bt4_rate/pwr, bt5_rate/pwr, op_lat/lon/alt, pubkey1..5.
- **FIXED [#25]** — `protocol`, `uart_port`, `tx_pin`, `rx_pin`, `ws2812_gpio`, `ws2812_brightness`, `lighting_*`, `dronecan_*`, `mavlink_usb_enable`, `ota_trigger_gpio`, `auth_private_key`, `start_delay_ms` are now persisted via get_blob/set_blob; no longer lost on reboot (see `softwarestatus.md`).
- `operator_lat/lon` stored as **float** in NVS although the fields are double → precision loss at ~1 cm.
- `nvs_storage_reset_preserve_keys`: erase-all then re-write pubkey1..5.

### 6.13 Auth / security — `rid-core/src/auth.rs` + `rid-core/src/security.rs`
- `rid_auth_init`: parses PEM Ed25519 key (must be 256 bit), called **only at boot**; `rid_auth_enabled()` = initialized && enabled.
- `rid_auth_sign_identity`: pure Ed25519 sign of `uas_id` (`mbedtls_pk_sign`, `MBEDTLS_MD_NONE`), paginated (page 0 = zero-data size, others = nonzero size), `ODID_AUTH_UAS_ID_SIGNATURE`.
- `rid_security_verify_signed_body`: SHA-256 of body, `mbedtls_pk_verify` with `MBEDTLS_MD_SHA256` (Ed25519-ph over SHA-256) against each of the 5 public keys (supports raw PEM or `PUBLIC_KEYV1:` base64).
- Note: local identity signing uses **pure Ed25519** while web verification uses **Ed25519-ph(SHA-256)** — two different schemes; each is internally consistent with its counterpart.
- `auth_private_key` not persisted in NVS → re-entering it is lost after reboot unless stored via web (web field exists but save misses it).

### 6.14 Kalman — `rid-core/src/kalman.rs`
- 3×1D filters; `RID_KALMAN_TIMEOUT_US = 3 s`; lat/lon in degrees with velocity in deg/s; speed/climb/heading derived from filter velocities (`rid_kalman_get`).

### 6.15 LEDs — `rid-app/src/led_status.rs`, `bsp-esp32/src/led.rs`, `rid-app/src/lighting.rs`
- Status LED (LEDC, 5 kHz): states BOOT(blue pulse), NO_GPS(amber 1 Hz), GPS_OK(green solid), DEMO(purple pulse), LOCKED(red double), OTA(rainbow), ERROR(red 4 Hz); TX flash = white 80 ms override.
- WS2812 (RMT): `set_rgb` scales by brightness, GRB order; green on GPS, amber otherwise.
- Lighting (GPIO): 6 patterns (off/solid/blink-slow/blink-fast/blink-armed/flash-on-gps), per-channel phase offset; inputs `armed` + `gps_valid`.

### 6.16 MAVLink TX / USB — `inputs/proto-usb-mavlink/src/tx.rs` + `inputs/proto-usb-mavlink/src/lib.rs`
- Task loop 100 ms: HEARTBEAT (MAV_TYPE_ODID, state ACTIVE) every 1 s; ODID_SYSTEM every 6 s with operator location from parser if fresh, else 0/0/-1000. Writes to **the same UART1** used by the parser + mirrors to USB.
- USB: installs UART driver on `CONFIG_ESP_CONSOLE_UART_NUM` at 115200. **Likely fails if the console already owns that UART** (returns false, logged) — and if it succeeds it shares the console port with CLI output.

---

## 7) Shared state — who writes / who reads

Full field-by-field chains in `todolist/dataflow.md`.

| Data | Writers | Readers (unlocked marked) |
|---|---|---|
| `g_config` | NVS load, web POST, CLI, boot MAC fix | `rid_task` (unlocked), TX builders (hub `build_uas` via cfg) |
| `g_state.gps` | `rid_task` (locked G/L), Kalman (unlocked), demo (locked) | TX builders (unlocked), web/CLI (locked copy) |
| `g_state.identity` | `rid_task` (locked I) | TX builders, gate (unlocked), web |
| `active_protocol` | `rid_task` | TX gate, web, CLI |
| parser statics (`g_last_gps`, buffers) | active parser only | only `rid_task` |
| `g_kalman` | `rid_task` (unlocked) | status box (unlocked), CLI |
| log ring | `vprintf` hook (any task) | `/api/logs` |

---

## 8) TX decision chain — `update_transmissions()` (`app/src/controller.rs`)

Called **only when `had_gps`** (valid GPS this loop, or demo). Gates in order:

1. `g_state.gps_valid || g_config.bcast_powerup` — else return false (note: with no parser data `had_gps=false`, so `update_transmissions` is not called at all and `bcast_powerup` has no effect in practice).
2. `active_protocol != RID_PROTOCOL_UNKNOWN` — else return false.
3. if `IDENTITY_READY_GATE` option: `identity_ready` must be true.
4. per-mode `rate_allowed(last_us, rate_hz)` (rate ≤ 0 disabled):
   - WiFi beacon → `wifi_tx_transmit` + `wifi_bcn_count`++
   - WiFi NAN → `wifi_tx_transmit_nan` (+ `g_nan_counter`)
   - BLE4 → `ble_tx_transmit_legacy`
   - BLE5 → `ble_tx_transmit_lr`

Each transport passes `&g_config` so the hub (`build_uas`) selects the region standard and gates the messages. Each success increments the per-channel counter + `transmissions_count` + LED flash.

Debug: find which gate blocks — that is usually the answer.

---

## 9) Debugging guide — symptom → process → check

| Symptom | Likely process / gate | Check first |
|---|---|---|
| No transmission at all | §8 chain | `/api/status`: `gps_valid`, `active_protocol`, counters; `tx_modes` in config |
| `active_protocol = UNKNOWN` in AUTO | `protocol_detect_auto` returns UNKNOWN only on **no data**; any other garbage → NMEA | is the FC actually sending? baud (probe 115200 in AUTO); wiring/pins |
| Protocol flaps / data lost in AUTO | detector consumes bytes + default-NMEA fallback | set a fixed protocol to confirm |
| GPS stale after 10 s | §5 Q + parser freshness (MAVLink 5 s) | parser buffer overflow? baud/pins? |
| MSP configured but no GPS | §6.3 framing | compare against real captured `$M<` frames |
| MAVLink configured but no data | `sysid_filter` | set 0 = any sysid; freshness 5 s |
| BLE4 nothing received | legacy 31 B rotation | `ble4_count` increments? power; ext-adv instance 2 |
| BLE5 nothing received | ext adv only compiled on S3/C6 | target support; `ble5_count` |
| Web UI unreachable | `webserver_en=0` or eFuse lock | boot log; AP SSID visible? |
| Config reverts after reboot | ~~§6.12 NVS gaps~~ → **FIXED #25** | all config fields now persisted via get_blob/set_blob |
| Auth pages missing | `rid_auth_init` at boot only + key not persisted (**FIXED #25**) | key persisted in NVS; reboot not needed; check bitlen 256 |
| OTA page loads but stalls | OTA idle counter | ~60 s stall timeout |
| Watchdog reset loop | `rid_task` blocked on `g_lock` `portMAX_DELAY` | who holds the lock (web/CLI/NVS)? |
| LED always NO_GPS | `gps_valid` never set | §5 phases E/G; parser gates |
| TX works but position wrong | unit conversion (see `dataflow.md` §1) | raw values vs transmitted |
| DroneCAN never gives position | §6.5 (reassembly missing) | not a config issue |

---

## 10) Known issues and suspects (→ `softwarestatus.md` for status)

- AUTO-mode UART starvation / default-NMEA misclassification (urgent).
- `g_config` / `g_state` races; `portMAX_DELAY` in `rid_task`; WDT add unchecked.
- ~~NVS persistence gaps (protocol, pins, ws2812, lighting, dronecan, mavlink_usb, ota_gpio, auth key, start_delay_ms)~~ → **FIXED [#25]** — full config persisted via get_blob/set_blob.
- ~~MSP framing off-by-one~~ → **FIXED [#18]** — standard MSP v1 framing.
- **⚠ `rid_mavlink_usb` vs console** — Rust port uses the USB-Serial/JTAG peripheral (`bsp-esp32/src/usb.rs`), not UART0 as in C, so the original UART0 clash is gone architecturally; on C3/C6 the same USB peripheral may carry console output — **verify at runtime/CI** — **OPEN #22**.
- ~~Web `/ota` has no signature check at lock=1~~ → **FIXED [#20]** — `ota.rs` requires `X-Signature` (Ed25519) at lock≥1, rejects at lock≥2.
- ~~DroneCAN `decode_fix2` unreachable (no multi-frame reassembly)~~ → **FIXED [#19]** — full `TransferReceiver` reassembly implemented + tested.
- ~~`bcast_powerup` effectively dead (TX only when `had_gps`)~~ → **FIXED [#21]** — `update_transmissions()` runs every tick.
- ~~`speed_vertical` never set for MSP/NMEA~~ → **FIXED [#34]** — derived from altitude deltas when Kalman off.
- Parser split needed (pure `process_bytes` vs UART) → enables fuzz + host tests.
- `operator_lat/lon` stored as float in NVS (precision).
