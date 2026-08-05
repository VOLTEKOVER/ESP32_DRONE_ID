# ESP DRONE REMOTEID — Data Flow / Mega Graph

Last updated: 2026-08-05
Scope: all protocols, variables and data traversed by the firmware (`components/esp_remote_id/src/*.c`, `main/main.c`).

---

## 🧠 Quick answer: drone-to-drone sharing / forwarding

**THERE IS NO active drone-to-drone sharing or connection-forwarding code.**

What exists (and what does NOT):

| Feature | Status | Where |
|---|---|---|
| Promiscuous WiFi RX / sniffer (ODID RX) | ❌ **Defined but NEVER called** (dead library code) | `wifi.c:520`, `wifi.c:535` |
| BLE scanner RX | ❌ Absent (`esp_ble_gap_start_scanning` never used) | `ble_tx.c` is TX only |
| Mesh relay / ESP-NOW | ❌ Absent (only a roadmap TODO) | `softwarestatus.md` "ESP-NOW mesh relay" |
| MAVLink outbound forwarding to GCS | ⚠️ Only heartbeat + operator location TX | `rid_mavlink_tx.c:30-60` (UART1) |
| Re-broadcast of other drones' RID | ❌ Absent | — |

The RX parsers (`mavlink_parser`, `msp_parser`, `nmea_parser`, `rid_dronecan`) receive data ONLY from the flight controller over UART/CAN, consume it and re-transmit it as the RID **of the drone itself**. There is no mechanism that receives another drone's RID (WiFi or BLE) and forwards it.

---

## 🕸️ Mega Graph (Mermaid)

```mermaid
flowchart TB
    subgraph INPUT["INPUT — External sources"]
        NMEA["NMEA GPS\n(GGA/RMC via UART1)"]
        MAV["MAVLink\n(GLOBAL_POSITION_INT / GPS_RAW_INT\nVFR_HUD / ODID_* via UART1)"]
        MSP["MSP Betaflight\n(MSP_RAW_GPS 106 / ATTITUDE 108\nSTATUS 101 via UART1)"]
        DC["DroneCAN\n(Fix2 2000 / AHRS 1000\nIdentity 8192 via TWAI)"]
        DEMO["Demo Patrol\n(synthetic rid_patrol)"]
        NVS["NVS / Web Config\n(persisted config)"]
        CLIIN["CLI UART0\n(user commands)"]
    end

    subgraph PARSERS["PARSER + DETECTION"]
        DET["protocol_detect_auto\n(MSP>NMEA>MAV>NMEA default)"]
        PN["nmea_parser.c\n→ g_last_gps"]
        PM["msp_parser.c\n→ g_last_gps"]
        PV["mavlink_parser.c\n→ g_last_gps + g_last_identity\n+ g_operator_*"]
        PDC["rid_dronecan.c\n→ g_last_gps"]
    end

    subgraph CORE["CORE — rid_task 100ms (esp_remote_id.c)"]
        G["g_state.gps (rid_gps_data_t)\nlat lon alt_msl alt_rel speed\nspd_v heading fix sat armed\noperator_lat/lon/alt"]
        I["g_state.identity (rid_identity_t)\nuas_id operator_id self_id_text\nid_type ua_type uas_id_2\next_auth_pages[16]"]
        O["g_state.operator_* ⚠️\n(MAVLink op loc — write-only)"]
        K["Kalman (rid_kalman.c)\nfiltered lat/lon/alt"]
        T["takeoff_lat/lon/alt\n(captured at first 3D fix)"]
    end

    subgraph TX["OUTPUT — Transmissions"]
        WB["WiFi Beacon (IE 221)\nwifi_tx.c / wifi.c"]
        WN["WiFi NAN Action\nwifi_tx.c / wifi.c"]
        B4["BLE 4.x Legacy Adv\nble_tx.c"]
        B5["BLE 5.0 Long Range Adv\nble_tx.c"]
        WT["MAVLink TX UART1\n(heartbeat + ODID_SYSTEM)\nrid_mavlink_tx.c"]
        WBUS["MAVLink USB CDC\nrid_mavlink_usb.c"]
    end

    subgraph CONTROL["CONTROL / DIAGNOSTICS"]
        WEB["Web UI 192.168.4.1\n/api/config GET/POST\n/api/status /api/logs\n/api/reset /ota\nweb_config.c"]
        CLI["CLI UART0\ncli.c"]
        LED["LED status 7 states\nled_status.c"]
        WS["WS2812 RMT\nled_ws2812.c"]
        LIT["GPIO lighting 5ch\nrid_lighting.c"]
        OTA["OTA update\nrid_ota.c"]
        AUTH["Auth Ed25519\nrid_auth.c / rid_security.c"]
    end

    NMEA --> DET
    MAV --> DET
    MSP --> DET
    DET --> PN
    DET --> PM
    DET --> PV
    DC --> PDC
    DEMO --> G
    NVS --> G
    NVS --> I

    PN --> G
    PM --> G
    PV --> G
    PV --> I
    PV --> O
    PDC --> G
    O -.->|"⚠️ NEVER read (bug B)"| G
    G --> K --> G

    G --> WB
    I --> WB
    G --> WN
    I --> WN
    G --> B4
    I --> B4
    G --> B5
    I --> B5
    G --> WT
    I --> WT
    G --> WBUS

    G --> WEB
    I --> WEB
    T --> WEB
    CLIIN --> CLI
    G --> CLI
    CLI --> G
    CLI --> I
    G --> LED
    G --> WS
    G --> LIT
    WEB --> NVS
    OTA --> NVS
    AUTH --> I
```

---

## 📦 Per-bus data (field-by-field detail)

### 1) NMEA GPS — `nmea_parser.c`

| Source | Function | Field written | Destination struct |
|---|---|---|---|
| `$GPRMC` (r.54-57) | `nmea_to_decimal()` | `latitude`, `longitude` | `g_last_gps` |
| `$GPGGA` (r.69-70) | `nmea_to_decimal()` | `latitude`, `longitude` | `g_last_gps` |
| `$GPGGA` (r.59-60) | `atof()` | `altitude_msl`, `altitude_baro` | `g_last_gps` |
| `$GPGGA` (r.63-67) | — | `fix_type`, `satellites` | `g_last_gps` |
| `nmea_parser_get()` (r.132) | — | copies everything (gate: fix≥2 && lat≠0) | `gps` → `g_state.gps` |

**No operator data from NMEA** — `operator_lat/lon/alt` stay at 0.

### 2) MSP Betaflight — `msp_parser.c`

| Message | Fields written |
|---|---|
| `MSP_RAW_GPS (106)` | `fix_type, satellites, latitude, longitude, altitude_msl, altitude_baro, speed, heading` |
| `MSP_ATTITUDE (108)` | `heading` (yaw/10) |
| `MSP_STATUS (101)` | `armed` (flag bit 0) |

Gate in `msp_parser_get()` (r.122): fix≥3 && lat≠0.

### 3) MAVLink — `mavlink_parser.c`

| Msg ID | Fields written | Destination |
|---|---|---|
| `GLOBAL_POSITION_INT` (r.104) | `lat, lon, alt` | `g_last_gps` |
| `GPS_RAW_INT` (r.119) | `lat, lon, alt` | `g_last_gps` |
| `VFR_HUD` (r.131) | `altitude_msl, speed` | `g_last_gps` |
| `ATTITUDE` / `AHRS2` (r.139/146) | `heading` | `g_last_gps` |
| `HEARTBEAT` (r.153) | `mavlink_armed` | `g_state` (via getter) |
| `OPEN_DRONE_ID_LOCATION` (r.161) | lat/lon/alt/speed/hdg/fix | `g_last_gps` |
| `OPEN_DRONE_ID_BASIC_ID` (r.175) | `uas_id, id_type, ua_type` | `g_last_identity` |
| `OPEN_DRONE_ID_OPERATOR_ID` (r.187) | `operator_id` | `g_last_identity` |
| `OPEN_DRONE_ID_SELF_ID` (r.198) | `self_id_text, self_id_desc_type` | `g_last_identity` |
| `OPEN_DRONE_ID_AUTHENTICATION` (r.210) | `ext_auth_pages[16]` | `g_last_identity` |
| `OPEN_DRONE_ID_SYSTEM` (r.223) | **⚠️ operator lat/lon → `g_last_gps.latitude/longitude` (BUG A)** + `g_operator_*` | `g_last_gps` + `g_operator_*` |
| `OPEN_DRONE_ID_MESSAGE_PACK` (r.235) | decodes sub-msg 0..5 → everything | `g_last_gps` + `g_last_identity` |

### 4) DroneCAN — `rid_dronecan.c`

| CAN ID | Message | Fields written |
|---|---|---|
| `2000` | `uavcan.equipment.gnss.Fix2` | `lat, lon, altitude_msl, altitude_relative, speed, heading, fix_type, satellites` |
| `1000` | `uavcan.equipment.ahrs.Solution` | **not decoded** (stub r.62-65) |
| `8192` | `org.drone_id.Identity` | **not decoded** (stub r.67-70) |

⚠️ AHRS and Identity are empty stubs → no operator position via DroneCAN.

### 5) Demo Patrol — `rid_patrol.c` (r.17-19)

Writes only: `altitude_msl/baro/relative` (sinusoidal) + `latitude/longitude` (circle). Enters `g_state.gps` when `RID_OPT_DEMO_MODE`.

### 6) Core assembly — `esp_remote_id.c` (rid_task r.399-630)

| Step | Line | Action |
|---|---|---|
| Protocol detect | 418-421 | `protocol_detect_auto()` or configured protocol |
| Read parser | 427-445 | `nmea_parser_get` / `msp_parser_get` / `mavlink_parser_get` / `rid_dronecan_get` |
| GPS gate | 452 | `force_tx (FORCE_ARM_OK) || fix_type>=2` |
| Copy state | 455-456 | `g_state.gps = gps_data; gps_valid=true` |
| Identity | 465-482 | MAVLink (if present) otherwise from `g_config` |
| Takeoff | 484-490 | first 3D fix → `takeoff_lat/lon/alt` |
| Operator | 492-505 | MAVLink → `g_state.operator_*` **⚠️** otherwise `g_state.gps.operator_* = g_config.operator_*` |
| Identity gate | 514-521 | `RID_OPT_IDENTITY_READY_GATE` |
| Demo | 523-544 | `rid_patrol_tick` + identity from config |
| Kalman | 546-572 | update+predict+overwrite `g_state.gps` |
| GPS timeout | 586-593 | `gps_valid=false` after 10 s **regardless of Kalman** (absolute timeout on `last_update_ms`) + WARN log |
| TX | 581-584 | `update_transmissions()` → WiFi/BLE |

---

## 🐞 BUGS FOUND (drone/operator swap suspicion → CONFIRMED)

### BUG A — `mavlink_parser.c:226-227` (CRITICAL: operator position overwrites drone position)
```c
g_last_gps.latitude  = odid_sys.operator_latitude  / 1e7;   // ← writes OPERATOR position into DRONE position
g_last_gps.longitude = odid_sys.operator_longitude / 1e7;
```
When the flight controller sends an `OPEN_DRONE_ID_SYSTEM` (ArduPilot does this for the operator position), the parser copies the **operator's** coordinates into the **drone position** fields. Since r.341 forces `g_last_update` if lat/lon≠0, the transmitted position becomes the operator's → **this is exactly the symptom you observed**. Fix: remove the two lines (keep only `g_operator_*`).

### BUG B — `esp_remote_id.c:496-505` (HIGH: MAVLink operator location never transmitted)
- TX reads `g_state.gps.operator_*` → `wifi_tx.c:194-195`, `ble_tx.c:72-73`.
- MAVLink writes `g_state.operator_*` (r.496-498), which is **write-only** (no consumer).
- Therefore the transmitted operator position is ALWAYS the static `g_config.operator_*` (r.502-504), never the one received from MAVLink.
- Fix: after r.496-498 also copy into `g_state.gps.operator_lat/lon/alt`.

### Note BLE `System.OperatorAltitudeGeo`
`ble_tx.c`/`wifi_tx.c` set `OperatorLatitude/Longitude` but **never `OperatorAltitudeGeo`** (stays 0).

---

## 📤 Output — ODID message map

| ODID Message | WiFi Beacon | WiFi NAN | BLE4 | BLE5 | MAVLink TX |
|---|---|---|---|---|---|
| BasicID (0) | ✅ `wifi_tx.c:170-180` | ✅ | ✅ `ble_tx.c:42-52` | ✅ | — |
| Location (1) | ✅ `:182-191` | ✅ | ✅ `:54-63` | ✅ | — |
| System (4) | ✅ `:193-197` | ✅ | ✅ `:71-73` | ✅ | ✅ `rid_mavlink_tx.c:53` |
| SelfID (3) | ✅ `:199-203` | ✅ | ✅ `:65-69` | ✅ | — |
| OperatorID (5) | ✅ `:205-206` | ✅ | ✅ `:75-76` | ✅ | — |
| Auth (2) | ⚠️ (from `ext_auth_pages`, if `AUTH_ED25519`) | — | — | — | — |

---

## 🖥️ Web API / Status — exposed fields

`/api/status` (web_config.c:347-369): `fw_version, protocol, gps_valid, lat, lon, alt, speed, heading, satellites, fix_type, tx_total, tx_wifi_bcn, tx_wifi_nan, tx_ble4, tx_ble5, takeoff_captured, takeoff_lat, takeoff_lon, takeoff_alt, uptime_ms`.

`/api/config` GET (r.300-345): all `rid_config_t` fields (protocol, uart, ua_type, id_type, uas_id, operator_id, tx_modes, wifi_*, ble4_*, ble5_*, operator_lat/lon/alt, options, led_*, ws2812_*, lighting_*, dronecan_*, mavlink_usb_enable, ota_trigger_gpio, auth_private_key, start_delay_ms).

`/api/config` POST: verifies `X-Signature` (Ed25519) if `lock_level>=1` + rate limiting.

---

## 📋 FULL DATA INVENTORY — all data processed in EVERY configuration

### A) Complete `rid_config_t` field inventory (default → NVS → Web/CLI)

Sources: `default_config()` `esp_remote_id.c:46-115`, persisted in NVS namespace `"esp_rid"` (`nvs_storage.c`), edited via Web `POST /api/config` (`web_config.c:136-293`) and CLI `cli.c`.

| # | Field | Type | Default | Writable (Web JSON key) | Consumers |
|---|---|---|---|---|---|
| 1 | `protocol` | enum | `AUTO` | `protocol` (1..4 else AUTO) | rid_task dispatch `:418-439` |
| 2 | `uart_port` | u8 | 1 | `uart_port` | UART1 |
| 3 | `baud_rate` | u32 | 57600 | `baud_rate` (>0) | UART reinit `:227-229` + parsers |
| 4 | `tx_pin` | u8 | 17 | `tx_pin` | UART1 pin |
| 5 | `rx_pin` | u8 | 18 | `rx_pin` | UART1 pin |
| 6 | `ua_type` | u8 | 1 | `ua_type` | BasicID UAType |
| 7 | `id_type` | u8 | 1 | `id_type` | BasicID IDType |
| 8 | `uas_id` | str[21] | `"ESP32-RID-001"` | `uas_id` | BasicID + identity fallback `:474` |
| 9 | `operator_id` | str[21] | `"OP-UNKNOWN"` | `operator_id` | OperatorID + fallback `:475` |
| 10 | `self_id_text` | str[21] | `""` | `self_id_text` | SelfID + fallback `:476` |
| 11 | `operator_lat/lon/alt` | f64/f64/f32 | 0 | `operator_lat/lon/alt` | `g_state.gps.operator_*` `:502-504`, demo `:537-539` |
| 12 | `ua_type_2` | u8 | 0 | `ua_type_2` | BasicID[1] `wifi_tx.c:178` |
| 13 | `id_type_2` | u8 | 0 | `id_type_2` | BasicID[1] `wifi_tx.c:177` |
| 14 | `uas_id_2` | str[21] | `""` | `uas_id_2` | BasicID[1] (enabled if non-empty) |
| 15 | `tx_modes` | u8 bitmask | `WIFI_BCN` | `tx_modes` | `update_transmissions()` `:296-326` |
| 16 | `wifi_channel` | u8 | 6 | `wifi_channel` (1..13) | AP config + beacon `wifi_tx.c:91` |
| 17 | `wifi_power_dbm` | f32 | 20.0 | `wifi_power_dbm` (2..20) | `esp_wifi_set_max_tx_power` `wifi_tx.c:111` |
| 18 | `wifi_bcn_rate_hz` | f32 | 1.0 | (0..5) | rate_allowed beacon `:296-302` |
| 19 | `wifi_nan_rate_hz` | f32 | 0.0 | (0..5) | rate_allowed NAN `:304-310` |
| 20 | `ble4_rate_hz` | f32 | 1.0 | (0..5) | rate_allowed BLE4 `:312-318` |
| 21 | `ble4_power_dbm` | f32 | 18.0 | (−27..18) | `ble_tx_set_power` `:233`, `esp_rid_set_config:233` |
| 22 | `ble5_rate_hz` | f32 | 1.0 | (0..5) | rate_allowed BLE5 `:320-326` |
| 23 | `ble5_power_dbm` | f32 | 18.0 | (−27..18) | `ble_tx_set_power` |
| 24 | `wifi_ssid` | str[21] | `"ESP-RID"` | `wifi_ssid` | AP SSID `wifi_tx.c:86-90,135-139` |
| 25 | `wifi_password` | str[21] | `""` | `wifi_password` | AP auth mode `wifi_tx.c:93-100` |
| 26 | `webserver_en` | u8 | 1 | `webserver_en` | (accepted, AP always runs) |
| 27 | `mavlink_sysid` | u8 | 0 (=any) | `mavlink_sysid` | `mavlink_parser_set_sysid_filter` `:158,230` |
| 28 | `bcast_powerup` | u8 | 1 | `bcast_powerup` | TX gate `:286` (transmit w/o GPS) |
| 29 | `options` | u16 bitmask | 0 | `options` | see matrix C |
| 30 | `lock_level` | i8 | 0 | `lock_level` (0/1/2; ≥2 burns eFuse) | `get_lock_level()` `web_config.c:67-78` |
| 31 | `led_r_gpio` | i8 | −1 | `led_r_gpio` | RGB status LED |
| 32 | `led_g_gpio` | i8 | −1 | `led_g_gpio` | RGB status LED |
| 33 | `led_b_gpio` | i8 | −1 | `led_b_gpio` | RGB status LED |
| 34 | `ws2812_gpio` | i8 | −1 | `ws2812_gpio` | WS2812 (RMT) |
| 35 | `ws2812_brightness` | u8 | 16 (%) | `ws2812_brightness` | RMT brightness scale |
| 36 | `lighting_pins[5]` | i8[5] | −1 | `lighting_pin_0..4` | GPIO outputs |
| 37 | `lighting_patterns[5]` | u8[5] | 0 | `lighting_pattern_0..4` | pattern selector |
| 38 | `lighting_phase_offsets[5]` | i16[5] | 0 | `lighting_phase_0..4` | phase shift ms |
| 39 | `dronecan_rx_gpio` | i8 | −1 | `dronecan_rx_gpio` | TWAI RX |
| 40 | `dronecan_tx_gpio` | i8 | −1 | `dronecan_tx_gpio` | TWAI TX |
| 41 | `dronecan_bitrate` | u32 | 1000000 | `dronecan_bitrate` | TWAI bitrate |
| 42 | `mavlink_usb_enable` | bool | false | `mavlink_usb_enable` | USB Serial/JTAG MAVLink out `:198-200` |
| 43 | `ota_trigger_gpio` | i8 | −1 | `ota_trigger_gpio` | boot-time OTA trigger `rid_ota.c` |
| 44 | `auth_private_key` | str[512] | `""` | `auth_private_key` | Ed25519 signing `rid_auth.c` |
| 45 | `start_delay_ms` | u32 | 10000 | `start_delay_ms` (≥0) | startup delay `:149-152` |
| 46 | `public_keys[5]` | str[5][257] | `""` | `public_key_1..5` | Ed25519 verify (lock≥1) `rid_security.c` |

**Boot ID fix-up** (`esp_remote_id.c:202-209`): if `uas_id=="ESP32-RID-001"` or `operator_id=="OP-UNKNOWN"`, both are regenerated from the eFuse MAC suffix (`ESP32-RID-`+MAC[4,5], `ESP32-OP-`+MAC[4,5]) and re-saved to NVS.

### B) Per-protocol configuration matrix

| Config | Active parser | UART baud | Gate (data accepted) | Extra data produced | Default `active_protocol` |
|---|---|---|---|---|---|
| `MAVLINK` (1) | `mavlink_parser_get` `:435` | config baud | lat≠0 (incl. force_tx) | identity, armed, operator loc | `MAVLINK` |
| `MSP` (2) | `msp_parser_get` `:432` | config baud | fix≥3 && lat≠0 | armed (flag bit 0) | `MSP` |
| `NMEA` (3) | `nmea_parser_get` `:429` | config baud | fix≥2 && lat≠0 | — | `NMEA` |
| `NONE` (4) | none `:437-439` | — | — | — | `NONE` |
| `AUTO` (255) | `protocol_detect_auto()` `:418-419` then dispatch | probe 115200 → config baud after first config write | per detected parser | — | detected (`MSP`>`NMEA`>MAVLink sniff>`NMEA` default, `protocol_detect.c:39-56`) |
| **Fallback** (any) | DroneCAN `:442-445` (if `!have_data` && DroneCAN active) | CAN 1 Mbit | lat≠0 | — | `NONE` (overwrites) |
| **Demo** (`RID_OPT_DEMO_MODE` and no GPS) | `rid_patrol_tick` `:523-544` | — | always | synthetic circle around Rome | `NONE` (overwrites) |

⚠️ **Ordering note**: DroneCAN fallback runs only when the primary parser returned no data, and its success **overwrites** `g_state.active_protocol = NONE` (`:444`).

### C) `options` bitfield matrix (`esp_remote_id.h:12-20`)

| Bit | Flag | Effect |
|---|---|---|
| 0 | `RID_OPT_FORCE_ARM_OK` | GPS gate bypassed when `armed==true` (`:450`) |
| 1 | `RID_OPT_DONT_SAVE_BASIC_ID` | clears `uas_id`/`uas_id_2` after identity assembly (`:509-512`) → transmits empty BasicID |
| 2 | `RID_OPT_PRINT_RID_MAVLINK` | logs RID summary line each loop (`:608-615`) |
| 3 | `RID_OPT_DEMO_MODE` | synthetic patrol when no GPS (`:523-544`); LED DEMO; Kalman auto-off (`:546`) |
| 4 | `RID_OPT_KALMAN_FILTER` | 3×1D Kalman on lat/lon/alt, overwrites `g_state.gps` (`:546-572`) |
| 5 | `RID_OPT_AUTH_ED25519` | `rid_auth_init(privkey)` (`:175-177`) → ODID Auth message |
| 6 | `RID_OPT_MAVLINK_ARM_STATUS` | enables MAVLink TX task (heartbeat, armed) `:169-172` |
| 7 | `RID_OPT_MAVLINK_OP_LOC_LOOP` | enables MAVLink TX operator-location loop `:169-172` |
| 8 | `RID_OPT_IDENTITY_READY_GATE` | TX blocked until identity sane + position sane (`:289-292`, `:514-521`) |

`sane()` rules (`esp_remote_id.c:117-131`): `uas_id` non-empty && ≠ `ESP32-RID-*`; `operator_id` non-empty && ≠ `OP-UNKNOWN`; lat ∈ [−90,90], lon ∈ [−180,180].

### D) `tx_modes` bitfield matrix (`esp_remote_id.h:49-54`)

| Bit | Flag | Builder | Rate source | Counters incremented |
|---|---|---|---|---|
| 0 | `RID_TRANSMIT_WIFI_BCN` | `wifi_tx_transmit` (`wifi_tx.c:209-235`) | `wifi_bcn_rate_hz` | `wifi_bcn_count`, `transmissions_count` |
| 1 | `RID_TRANSMIT_WIFI_NAN` | `wifi_tx_transmit_nan` (`wifi_tx.c:237-258`) | `wifi_nan_rate_hz` | `wifi_nan_count`, `transmissions_count` |
| 2 | `RID_TRANSMIT_BLE4` | `ble_tx_transmit_legacy` (`ble_tx.c:150-176`) | `ble4_rate_hz` | `ble4_count`, `transmissions_count` |
| 3 | `RID_TRANSMIT_BLE5` | `ble_tx_transmit_lr` (`ble_tx.c:178-230`) | `ble5_rate_hz` | `ble5_count`, `transmissions_count` |

Global gate (`esp_remote_id.c:284-292`): needs `gps_valid || bcast_powerup`, `active_protocol != UNKNOWN`, and identity gate if option set. Beacon TX uses 4-attempt fallback `{AP,STA,AP,STA} × {no-seq,no-seq,seq,seq}` (`wifi_tx.c:224-228`).

### E) `rid_state_t` inventory — every field, writer → reader

| Field | Written by | Read by |
|---|---|---|
| `gps.*` | parsers/demo/Kalman (`:455`, `:562-567`) | TX builders, `/api/status`, CLI status, LEDs |
| `identity.*` | MAVLink (`:472`) else config fallback (`:474-481`); demo (`:531-535`) | TX builders, gate, `/api/status` |
| `active_protocol` | rid_task (`:423`, `:444`, `:529`) | TX gate, `/api/status`, CLI |
| `last_update_ms` | on GPS accept (`:457`, `:528`) | 10 s GPS timeout (`:576`) |
| `transmissions_count` | `update_transmissions` | `/api/status` (tx_total) |
| `wifi_bcn_count` | `:299` | `/api/status` (tx_wifi_bcn) |
| `wifi_nan_count` | `:307` | `/api/status` (tx_wifi_nan) |
| `ble4_count` | `:315` | `/api/status` (tx_ble4) |
| `ble5_count` | `:323` | `/api/status` (tx_ble5) |
| `gps_valid` | `:456`, `:526`, `:568`; cleared `:570`,`:577` | TX gate, LEDs, WS2812, lighting, status |
| `identity_ready` | gate logic `:514-521`, demo `:543` | TX gate (`:291`), status |
| `mavlink_armed` | `:461` | armed LED/lighting, `gps.armed` |
| `mavlink_sysid` | (struct) | — |
| `operator_lat/lon/alt` | MAVLink `:496-498` ⚠️ (BUG B: write-only) | — (never consumed!) |
| `operator_position_updated_ms` | `:499` | — |
| `operator_location_type` | `:500` | — |
| `auth_enabled` | (struct) | — |
| `takeoff_lat/lon/alt` | first 3D fix `:484-490` | `/api/status` |
| `takeoff_captured` | `:489` | `/api/status` |

### F) Subsystem data (per configuration detail)

**Kalman** (`rid_kalman.c`): 3×1D filters, `q_pos/q_vel/r` per axis — lat `1e-9/1e-8/1e-9`, lon `1.5e-9/1.5e-8/1.5e-9`, alt `1.0/10.0/25.0` (`:26-28`); timeout `RID_KALMAN_TIMEOUT_US = 3 s` (`rid_kalman.h:7`); derived speed/climb/heading from filter velocities (`:105-122`); `valid_age` keeps `gps_valid=true` up to 3 s without new data.

**LED status** (`led_status.h:10-16`, `esp_remote_id.c:586-595`): 7 states — `BOOT, NO_GPS, GPS_OK, DEMO, LOCKED (lock_level≥2), OTA, ERROR`; TX flash on successful transmit (`:583`).

**WS2812** (`esp_remote_id.c:598-602`): green (GPS ok) / amber (no GPS), brightness % → `(pct*255/100)`.

**Lighting** (`rid_lighting.c:9-16`): 5 patterns — OFF, SOLID, BLINK_SLOW (2 s), BLINK_FAST (0.5 s), BLINK_ARMED (1 s × armed), FLASH_ON_GPS; inputs `armed`, `gps_valid` (`:605`).

**Web endpoints** (`web_config.c:716-725`): `/` (HTML UI), `/style.css`, `/app.js`, `GET /api/config`, `POST /api/config` (JSON apply `:136-293`), `GET /api/status`, `POST /api/reset`, `POST /ota` (SHA-256 + `X-Expected-SHA256` mandatory, `:507-604`), `GET /api/logs` (ring buffer 64×240 B), `POST /api/command` (`restart|reboot|reset|factory|status`, auth-gated when lock≥1 `:660-677`).
Auth gating: `lock_level≥1` → Ed25519 `X-Signature` + rate limit 10 fails/60 s (`web_config.c:43-65`); `lock_level≥2` → eFuse magic `0x52494421` "RID!" (`:27`, `:67-78`) → `/ota` and config locked.
Log ring: vprintf hook intercepts ESP_LOG (`web_config.c:96-134`), feeds `/api/logs`.

**CLI** (`cli.c:50-66`): `help, status, config [set <field> <value>], restart, reboot, reset, factory, protocol [auto|mavlink|msp|nmea|none], heap, log_level <tag> <level>, patrol [on|off], transmit <wifi_bcn|wifi_nan|ble4|ble5|all> <on|off>, mac, uptime, kalman [on|off]` (MAX_ARGS 16, MAX_LINE 256). `config set` writes uas_id/operator_id/self_id/wifi_*/rates/powers/operator_*/lock_level/start_delay_ms/etc.

**NVS** (`nvs_storage.c`): namespace `"esp_rid"`; typed helpers store/load str/u8/u32/f32/i8; `esp_rid_factory_reset` → **differential reset** (`nvs_storage_reset_preserve_keys`) keeps `pubkey1..5`, erases the rest, re-defaults + re-saves (`esp_remote_id.c:260-283`); OTA factory-reset form uses the same preserve-keys path.

**OTA** (`rid_ota.c`): `ota_trigger_gpio` boot check; three web forms — update, factory_reset, rollback.

**Auth** (`rid_auth.c` / `rid_security.c`): Ed25519 private key (256-bit PEM, `rid_auth.c`), base64 strict decode, hex encode/decode; verify against `public_keys[5]`.

**MAVLink TX** (`rid_mavlink_tx.c`): enabled by options bits 6|7 → UART1 task 2048 B; heartbeat + `OPEN_DRONE_ID_SYSTEM` operator loc loop.
**MAVLink USB** (`rid_mavlink_usb.c`): `mavlink_usb_enable` → USB Serial/JTAG console @ 115200 MAVLink out.

### G) Timeouts / rate constants

| Constant | Value | Where |
|---|---|---|
| Parser data freshness | GPS 5 s, identity 10 s | `mavlink_parser.c:348`, `:358` |
| Operator loc freshness | 30 s | `mavlink_parser.c:73` |
| GPS validity timeout | 10 s | `esp_remote_id.c:586` |
| Kalman timeout | 3 s | `rid_kalman.h:7` |
| Rate limit | 10 fails / 60 s | `web_config.c:43-44` |
| Startup delay default | 10 s | `esp_remote_id.c:111` |
| Loop period | 100 ms | `esp_remote_id.c:625` |
| Detect read window | 50 ms / 1 s timeout | `protocol_detect.c:11-12` |
| WiFi NAN counter | 8-bit increment | `esp_remote_id.c:282`, `:306` |

---

## ⚠️ Risk notes / dead ends

1. **`g_state.operator_*` is now consumed** — bug B fixed: MAVLink op-loc copied to `g_state.gps.operator_*` (`esp_remote_id.c:524-526`) and used as `System` in both TX builders. Historical: was write-only.
2. **DroneCAN AHRS/Identity are stubs** → operator position and identity from CAN never populated.
3. **ODID RX (WiFi) is dead library code** (`wifi.c:520/535` never called) → for a drone-to-drone relay you'll need `esp_wifi_set_promiscuous` + callback.
4. **MAVLink TX does not send Location** → only heartbeat + System (GCS cannot see full position over MAVLink).
5. **`BLE 5 LR` and `RID_OPT_PRINT_RID_MAVLINK`** depend on config/option flags; BLE 5 LR compiled only when `CONFIG_BT_BLE_50_EXTEND_ADV_EN` (S3/C6).
6. **OTA idle timeout** — `OTA_MAX_IDLE_STALLS=12` (~60 s) aborts stalled uploads.
7. **Differential factory reset** — public keys survive a reset (locked unit stays locked); full wipe requires explicit `nvs_flash_erase()`.
