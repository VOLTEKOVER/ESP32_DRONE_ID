# ESP DRONE REMOTEID — Data Verification (field by field)

Last updated: 2026-08-03
Method: static code audit, one data item at a time. For each field we trace the full chain:
**producer (parser) → core (rid_task) → consumer (TX / status / LED)** and mark it OK / BUG / DEAD / RISK.

Verdict legend:
- ✅ **OK** — chain complete and correct
- 🔴 **BUG** — data is produced but routed/lost incorrectly (functional defect)
- ⚫ **DEAD** — produced but never consumed (or consumed but never produced)
- 🟡 **RISK** — works but fragile / depends on external convention / precision loss

---

## 1) `rid_gps_data_t` — position & dynamics

### 1.1 `latitude` / `longitude`
| Producer | Where | Consumer | Verdict |
|---|---|---|---|
| MSP `MSP_RAW_GPS` (deg1e7 /1e7) | `msp_parser.c:48-49` | `g_state.gps` `esp_remote_id.c:455` → WiFi `wifi_tx.c:183-184`, BLE `ble_tx.c:55-56` | ✅ OK |
| NMEA `$GPGGA/$GPRMC` | `nmea_parser.c:54,69` | same | ✅ OK |
| MAVLink `GLOBAL_POSITION_INT` / `GPS_RAW_INT` / `ODID_LOCATION` | `mavlink_parser.c:107,122,164` | same | ✅ OK |
| MAVLink `OPEN_DRONE_ID_SYSTEM` | **`mavlink_parser.c:226-227`** | same | 🔴 **BUG A** (operator position overwrites drone position — confirmed) |
| MAVLink `MESSAGE_PACK` submsg | `mavlink_parser.c:271-272` | same | ✅ OK |
| DroneCAN `Fix2` | `rid_dronecan.c:37-38` | same | ✅ OK |
| Demo patrol | `rid_patrol.c:15-16` | same | ✅ OK |
| Kalman (float) | `rid_kalman.c:93-94` cast `(float)lat/lon` | `g_state.gps` `esp_remote_id.c:562-563` | 🟡 RISK — double→float loses ~0.5 m resolution (≈1e-7 deg); acceptable for RID, not for survey |

### 1.2 `altitude_msl`
| Producer | Where | Consumer | Verdict |
|---|---|---|---|
| MSP (dm → /10) | `msp_parser.c:50` | WiFi `wifi_tx.c:185`, BLE `ble_tx.c:57` | ✅ OK |
| NMEA `$GPGGA` | `nmea_parser.c:59` | same | ✅ OK |
| MAVLink (4 msgs) | `mavlink_parser.c:109,124,136,166` | same | ✅ OK |
| DroneCAN `Fix2` (mm → /1000) | `rid_dronecan.c:41` | same | ✅ OK |
| Kalman | `rid_kalman.c:564` | same | ✅ OK |

### 1.3 `altitude_relative` (→ ODID `Height`)
| Producer | Where | Consumer | Verdict |
|---|---|---|---|
| MAVLink (GPI / ODID_LOC / PACK) | `mavlink_parser.c:110,167,274` | WiFi `wifi_tx.c:186`, BLE `ble_tx.c:58` | ✅ OK |
| DroneCAN `Fix2` (ellipsoid mm) | `rid_dronecan.c:44` | same | 🟡 RISK — ellipsoid height ≠ height-above-takeoff (ODID `Height` semantics); value semantically wrong |
| Demo patrol | `rid_patrol.c:19` | same | ✅ OK |
| **MSP** | — never set | stays 0 | 🔴 **GAP** — MSP mode transmits `Height=0` always |
| **NMEA** | — never set | stays 0 | 🔴 **GAP** — NMEA mode transmits `Height=0` always |
| **Takeoff capture** | `esp_remote_id.c:484-490` | only `/api/status` | ⚫ **DEAD** — `takeoff_alt` captured but never used to compute ODID `Height` (ASTM F3411 wants height-above-takeoff) |

### 1.4 `altitude_baro`
| Producer | Where | Consumer | Verdict |
|---|---|---|---|
| MSP | `msp_parser.c:51` | **none** — never read in TX | ⚫ **DEAD** |
| NMEA `$GPGGA` | `nmea_parser.c:60` | **none** | ⚫ **DEAD** |
| MAVLink `ODID_LOCATION` | `mavlink_parser.c:168` | **none** | ⚫ **DEAD** |
| DroneCAN | — never set | — | — |

### 1.5 `speed`
| Producer | Where | Consumer | Verdict |
|---|---|---|---|
| MSP (cm/s → /100) | `msp_parser.c:52` | WiFi `wifi_tx.c:187`, BLE `ble_tx.c:59` | ✅ OK |
| NMEA `$GPRMC/$GPVTG` (kt → ×0.5144) | `nmea_parser.c:73,83` | same | ✅ OK |
| MAVLink (4 msgs) | `mavlink_parser.c:115,127,134,169` | same | ✅ OK |
| DroneCAN `Fix2` (cm/s → /100) | `rid_dronecan.c:47` | same | ✅ OK |
| Kalman (from filter v) | `rid_kalman.c:113` | same | ✅ OK |

### 1.6 `speed_vertical`
| Producer | Where | Consumer | Verdict |
|---|---|---|---|
| MAVLink (3 msgs) | `mavlink_parser.c:116,170,277` | WiFi `wifi_tx.c:189`, BLE `ble_tx.c:60` | ✅ OK |
| DroneCAN | — never set | stays 0 | 🔴 **GAP** |
| **MSP / NMEA** | — never set | stays 0 | 🔴 **GAP** (vertical speed lost for the two most common FC protocols) |
| Kalman | `rid_kalman.c:114` | same | ✅ OK |

### 1.7 `heading`
| Producer | Where | Consumer | Verdict |
|---|---|---|---|
| MSP (RAW_GPS cdeg→/10, ATTITUDE yaw/10) | `msp_parser.c:53,60` | WiFi `wifi_tx.c:188`, BLE `ble_tx.c:61` | ✅ OK |
| NMEA `$GPVTG` | `nmea_parser.c:80` | same | ✅ OK |
| MAVLink (GPI cdeg, VFR_HUD, ATTITUDE/AHRS2 yaw, ODID_LOC) | `mavlink_parser.c:111,128,135,142,149,171` | same | ✅ OK |
| DroneCAN `Fix2` (deg1e2 → /100) | `rid_dronecan.c:50` | same | ✅ OK |
| Kalman (atan2 of velocities) | `rid_kalman.c:116-121` | same | ✅ OK |

### 1.8 `fix_type`
| Producer | Where | Gate that accepts it | Verdict |
|---|---|---|---|
| MSP (`MSP_RAW_GPS`) | `msp_parser.c:46` | `msp_parser_get` `fix>=3` `:122` | 🔴 **BUG G** — Betaflight/iNav `fixType` enum is 0/1/2 (3D fix = **2**) → gate `>=3` **never passes** → MSP data never reaches TX |
| NMEA `$GPGGA` (fix≥2 → 3) | `nmea_parser.c:53` | `nmea_parser_get` `fix>=2` `:132` | ✅ OK |
| MAVLink (GPI hardcoded 3 / GPS_RAW_INT / ODID_LOC) | `mavlink_parser.c:112,125,172` | rid_task `fix>=2` `esp_remote_id.c:452` | ✅ OK |
| DroneCAN `Fix2` (`>=2`) | `rid_dronecan.c:53-55` | rid_task `fix>=2` | ✅ OK |
| Demo patrol (2..4) | `rid_patrol.c:26` | always | ✅ OK |

### 1.9 `satellites`
| Producer | Where | Consumer | Verdict |
|---|---|---|---|
| MSP / NMEA / DroneCAN / MAV GLOBAL_POSITION / GPS_RAW_INT | `msp_parser.c:47`, `nmea_parser.c:57`, `rid_dronecan.c:59`, `mavlink_parser.c:126` | accuracy estimate `wifi_tx.c:190-191`, `ble_tx.c:62-63`; status | ✅ OK |
| MAVLink `OPEN_DRONE_ID_SYSTEM` | **`mavlink_parser.c:228`** writes `odid_sys.area_count` into `satellites` | accuracy estimate | 🟡 RISK — `area_count` (flight-area count) misused as satellite count; only affects accuracy rounding |
| MAVLink `MESSAGE_PACK` submsg `System` | `mavlink_parser.c:318` same issue | same | 🟡 RISK |

### 1.10 `armed`
| Producer | Where | Consumer | Verdict |
|---|---|---|---|
| MSP `MSP_STATUS` (flag bit0) | `msp_parser.c:67` → `gps_data.armed` | `FORCE_ARM_OK` gate uses fresh value `esp_remote_id.c:450` | ✅ OK (gate) |
| MAVLink `HEARTBEAT` | `mavlink_parser.c:156-158` → `g_state.mavlink_armed` `:461` | lighting `:605`, `gps.armed` | ✅ OK |
| **MSP/NMEA armed → `g_state.gps.armed`** | `:455` copies it, then **`:463` overwrites with `g_state.mavlink_armed` (false)** | lighting BLINK_ARMED | 🔴 **BUG C** — armed status is always lost for non-MAVLink protocols (`g_state.gps.armed` forced false every loop) |
| Demo patrol (armed=true) | `rid_patrol.c:30` | — | 🔴 same overwrite |

### 1.11 `operator_lat` / `operator_lon` / `operator_alt` (within `gps`)
| Producer | Where | Consumer | Verdict |
|---|---|---|---|
| Static config (fallback) | `esp_remote_id.c:502-504` (and demo `:537-539`) | ODID `System` `wifi_tx.c:194-195`, `ble_tx.c:72-73` | ✅ OK |
| MAVLink op-loc → **`g_state.operator_*`** | `esp_remote_id.c:496-498` | **no consumer** | 🔴 **BUG B** — MAVLink operator location is written to a write-only field; TX always uses static config. Confirmed. |
| DroneCAN `Identity` (8192) | — | — | 🔴 **stub** `rid_dronecan.c:67-70` — never decoded |
| `OperatorAltitudeGeo` | — | — | ⚫ **DEAD** — `wifi_tx.c`/`ble_tx.c` set Lat/Lon but never `OperatorAltitudeGeo` (stays 0) |

---

## 2) `rid_identity_t`

| Field | Producer | Consumer | Verdict |
|---|---|---|---|
| `uas_id` | config fallback `:474` / MAVLink `:180,260` / demo `:531` | WiFi `wifi_tx.c:173`, BLE `ble_tx.c:45` | ✅ OK |
| `operator_id` | config `:475` / MAVLink `:192,326` / demo `:532` | WiFi `wifi_tx.c:206`, BLE `ble_tx.c:76` | ✅ OK |
| `self_id_text` | config `:476` / MAVLink `:203,302` / demo `:533` | WiFi `wifi_tx.c:200-202`, BLE `ble_tx.c:65-68` | ✅ OK |
| `id_type`, `ua_type` | config `:477-478` / MAVLink `:182-183,262-263` | WiFi `wifi_tx.c:171-172`, BLE `ble_tx.c:43-44` | ✅ OK |
| `uas_id_2`, `id_type_2`, `ua_type_2` | config `:479-481` **only** (never from MAVLink) | WiFi `wifi_tx.c:175-180`, BLE `ble_tx.c:47-52` | ✅ OK |
| `has_self_id`, `self_id_desc_type` | MAVLink `:205-206,305` | **none** — TX hardcodes `ODID_DESC_TYPE_TEXT` | ⚫ **DEAD** |
| `ext_auth_pages[]`, `has_ext_auth`, `ext_auth_last_page` | MAVLink RX `:214-218,288-292` | **none** — no TX path reads them | 🔴 **BUG D** |
| Auth signing (`rid_auth_sign_message`) | defined `rid_auth.c:63-102` | **never called anywhere** | ⚫ **DEAD** — `RID_OPT_AUTH_ED25519` only calls `rid_auth_init` (`esp_remote_id.c:175-177`); `AuthValid` never set in `wifi_tx.c`/`ble_tx.c`; ODID `Auth` message is never transmitted |
| `identity_ready` gate sanity | `identity_is_sane` `:117-124` | TX gate `:289-292` | ✅ OK (by design rejects `ESP32-RID-*` / `OP-UNKNOWN`) |

---

## 3) `rid_state_t`

| Field | Writer | Reader | Verdict |
|---|---|---|---|
| `gps_valid` | `:456,526,568`; cleared `:570,577` | TX gate, LED, status | ✅ OK |
| `identity_ready` | `:518,520,543` | TX gate `:291` | ✅ OK |
| `mavlink_armed` | `:461` | `:463`, lighting `:605` | ✅ OK (MAVLink only) |
| `mavlink_sysid` (state) | **none** (`mavlink_parser_get_sysid` never called) | — | ⚫ **DEAD** |
| `operator_lat/lon/alt` (state) | `:496-498` | **none** | 🔴 **BUG B** (write-only) |
| `operator_position_updated_ms`, `operator_location_type` | `:499-500` | **none** | ⚫ **DEAD** (part of BUG B) |
| `auth_enabled` (state) | **none** | — | ⚫ **DEAD** |
| `takeoff_lat/lon/alt`, `takeoff_captured` | `:484-490` | `/api/status` only | 🟡 **UNDERUSED** — captured but never used in TX `Height` |
| `transmissions_count`, `wifi_bcn_count`, `wifi_nan_count`, `ble4_count`, `ble5_count` | `update_transmissions` `:299-324` | `/api/status` | ✅ OK |
| `last_update_ms` | `:457,528` | 10 s timeout `:576` | ✅ OK |

---

## 4) `rid_config_t` — is each setting actually honored?

| Setting | Applied? | Evidence | Verdict |
|---|---|---|---|
| `protocol`, `options`, `tx_modes`, `lock_level` | yes | rid_task / web_config / eFuse | ✅ OK |
| `wifi_ssid`, `wifi_password`, `wifi_channel`, `wifi_power_dbm` | yes | `wifi_tx.c:86-111,135-156` | ✅ OK |
| `wifi_bcn_rate_hz`, `wifi_nan_rate_hz`, `ble4_rate_hz`, `ble5_rate_hz` | yes | `rate_allowed` `:296-326` | ✅ OK |
| `ble4_power_dbm`, `ble5_power_dbm` | yes | `ble_tx_set_power` `ble_tx.c:232-238` | ✅ OK |
| `mavlink_sysid` | yes | filter `:158,230` | ✅ OK |
| `bcast_powerup` | yes | TX gate `:286` | ✅ OK |
| `start_delay_ms` | yes | `:149-152` | ✅ OK |
| `baud_rate` | **partially** | `protocol_detect_init` sets **115200** hardcoded `protocol_detect.c:17`; configured baud applied **only** on `protocol_detect_reinit` (after a config save `:228`), **not at boot** | 🟡 **RISK/BUG** — boot in AUTO mode probes at 115200 regardless of configured 57600 → misdetect or garbage |
| `uart_port`, `tx_pin`, `rx_pin` | **no** | shown in UI (`webui/app.js:431-433`, `config.html:425-428`) but UART1/pins 17/18 are hardcoded in `protocol_detect.c:26,72` | ⚫ **DEAD config** — changing them does nothing |
| `webserver_en` | **no** | saved/loaded/displayed but never checked; web server always starts | ⚫ **DEAD config** |
| `ws2812_gpio`, `ws2812_brightness` | yes | `esp_remote_id.c:180,232` | ✅ OK |
| `led_r/g/b_gpio` | yes | `led_status_reconfigure` `:211,231` | ✅ OK |
| `lighting_*[5]` | yes | `rid_lighting_init` `:183-189` | ✅ OK |
| `dronecan_rx/tx_gpio`, `dronecan_bitrate` | yes | `rid_dronecan_init` `:192-195` | ✅ OK |
| `mavlink_usb_enable` | **partial** | init only `:198-200` — nothing writes MAVLink to the USB UART | ⚫ **DEAD feature** |
| `ota_trigger_gpio` | yes | `rid_ota_check_and_run` | ✅ OK |
| `auth_private_key` | **partial** | parsed in `rid_auth_init` but never used for signing | 🔴 **BUG D** |
| `public_keys[5]` | yes | signature verify `rid_security.c:118-160` | ✅ OK |

---

## 5) TX chain

| Path | Pack | Result | Verdict |
|---|---|---|---|
| WiFi Beacon | `odid_wifi_build_message_pack_beacon_frame` (`wifi.c:438`) via `esp_wifi_80211_tx` 4-attempt fallback | BasicID+Location+System+SelfID+OperatorID | ✅ OK (subject to BUG A/B) |
| WiFi NAN | `odid_wifi_build_message_pack_nan_action_frame` (`wifi.c:357`) | same | ✅ OK |
| **BLE 4.x legacy** | `build_legacy_adv` `ble_tx.c:96-128` | comment says "one message/cycle" but code copies **the whole pack** (`pack_len`, no rotation) into `g_adv_data`; total adv = 11 B header + pack → **>31 B legacy limit** → `esp_ble_gap_config_adv_data_raw` rejected/truncated | 🔴 **BUG F** — legacy BLE advertising is broken (whole pack in 31-byte ADV; no per-message rotation) |
| BLE 5.0 long-range | ext adv instances 0/1 (`ble_tx.c:178-230`) | full pack, 254 B OK | ✅ OK |
| **MAVLink TX** (UART1) | heartbeat every 1 s + `OPEN_DRONE_ID_SYSTEM` every 6 s | **hardcoded dummy payload** `rid_mavlink_tx.c:52-54`: `id_or_mac={0}`, `operator_lat/lon=-1000.0f`, all zeros — reads **no** `g_state` | 🔴 **BUG E** — the "operator location loop" transmits garbage; real state never read |
| MAVLink USB | `rid_mavlink_usb_init` | installs UART on console port only; **no writer** | ⚫ **DEAD feature** |
| ODID `Auth` message | — | `AuthValid` never set anywhere | ⚫ **DEAD** (BUG D) |

---

## 6) Summary of issues found (new, beyond the known A/B)

| # | Severity | Issue | Location |
|---|---|---|---|
| C | HIGH | `armed` always overwritten to false for non-MAVLink protocols | `esp_remote_id.c:463` |
| D | HIGH | Ed25519 auth configured but **never transmitted**; MAVLink-relayed auth pages also never re-broadcast | `rid_auth.c:63` (uncalled), `wifi_tx.c:166-207`, `ble_tx.c:38-77` |
| E | HIGH | MAVLink TX sends hardcoded zero/−1000 SYSTEM payload, never real operator/state | `rid_mavlink_tx.c:52-54` |
| F | HIGH | BLE 4.x legacy advertising broken: whole pack (>31 B) in one legacy ADV, no message rotation | `ble_tx.c:96-128` |
| G | HIGH | MSP `fix_type>=3` gate never passes on Betaflight/iNav (max 2) → MSP mode dead in practice | `msp_parser.c:122` |
| H | MED | Boot ignores configured `baud_rate` (AUTO probes at hardcoded 115200) | `protocol_detect.c:17`, `esp_remote_id.c:149-152` |
| I | MED | `uart_port`/`tx_pin`/`rx_pin`/`webserver_en` config fields are dead (never honored) | `protocol_detect.c:26,72`, `web_config.c` |
| J | MED | NMEA/MSP never set `altitude_relative` → ODID `Height=0` | `nmea_parser.c`, `msp_parser.c` |
| K | LOW | `takeoff_*` captured but never used in TX; `altitude_baro` never transmitted; state `mavlink_sysid`/`auth_enabled` never written; `area_count` misused as `satellites` | various |
