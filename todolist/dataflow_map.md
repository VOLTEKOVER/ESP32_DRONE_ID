# ESP DRONE REMOTEID — Data Flow / Mega Graph

Last updated: 2026-08-02
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
| GPS timeout | 574-579 | `gps_valid=false` after 10s |
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

## ⚠️ Risk notes / dead ends

1. **`g_state.operator_*` is write-only** → only treats the symptom; bug B is in the routing.
2. **DroneCAN AHRS/Identity are stubs** → operator position and identity from CAN never populated.
3. **ODID RX (WiFi) is dead library code** (`wifi.c:520/535` never called) → for a drone-to-drone relay you'll need `esp_wifi_set_promiscuous` + callback.
4. **MAVLink TX does not send Location** → only heartbeat + System (GCS cannot see full position over MAVLink).
5. **`BLE 5 LR` and `RID_OPT_PRINT_RID_MAVLINK`** depend on config/option flags.
