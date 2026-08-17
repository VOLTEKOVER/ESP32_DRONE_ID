# OmniRID — Web UI & Desktop App Status

> Embedded web UI served by `bsp-esp32` from flash (`include_str!`).
> Desktop app: Electron + React 19 + Ant Design 6 + Vite 8.
> Last updated: 2026-08-17

---

## 🗂️ Tab Structure (15 tabs)

### Monitor
| Tab | ID | Description |
|-----|----|-------------|
| Dashboard | `dashboard` | Live telemetry: GPS, protocol, TX status, battery, RSSI |

### Configuration
| Tab | ID | Description |
|-----|----|-------------|
| Identity | `identity` | UAS ID, ID type, UA type, operator ID, self-ID, operator location, second BasicID |
| Transmission | `transmission` | WiFi BCN/NAN, BLE4/5 toggles, channel, power, rates |
| Access Point | `ap` | WiFi SSID/password, web server toggle |
| Flight Controller | `fc` | Protocol (AUTO/MAVLink/MSP/NMEA), baud, SysID, broadcast on power |
| Hardware | `hardware` | LED GPIO (R/G/B), WS2812 pin/brightness, UART, TX/RX pins, DroneCAN pins/bitrate, lighting channels (5x pin/pattern/phase), OTA trigger GPIO, USB MAVLink, startup delay |

### Compliance & Security
| Tab | ID | Description |
|-----|----|-------------|
| Compliance | `compliance` | Region selector, per-region requirement checklist, standard status |
| Security | `security` | Password, timeout, lock level, 5 public keys, option flags (9 checkboxes) |

### Advanced
| Tab | ID | Description |
|-----|----|-------------|
| System | `system` | Save, Factory Reset, Restart |
| Firmware | `firmware` | OTA upload (.bin file) |

### Tools
| Tab | ID | Description |
|-----|----|-------------|
| Console | `console` | Serial terminal, CLI commands, signed commands |
| Presets | `presets` | Save/load/import/export config presets |
| Logging | `logging` | Start/stop telemetry log, download CSV |
| Sensors | `sensors` | Sensor readings (placeholder) |

### Other
| Tab | ID | Description |
|-----|----|-------------|
| Help | `help` | Documentation links, version info |

---

## 🔘 All Buttons

### Header Actions
| Button | Action | Status |
|--------|--------|--------|
| ☰ Menu | `toggleMobileMenu()` — mobile sidebar toggle | ✅ |
| 💾 Save All | `saveAll()` — save all dirty fields | ✅ |
| 📥 Export | `exportCfg()` — download config JSON | ✅ |
| 📤 Import | `importCfg()` — upload config JSON | ✅ |
| 🔄 Reconnect | `getCfg()` — refresh from device | ✅ |
| ☀/☾ Dark | `toggleDark()` — theme toggle | ✅ |

### System
| Button | Action | Status |
|--------|--------|--------|
| 🔄 Restart | `sendSigCmd('restart')` — restart device | ✅ Factory Reset |
| ↻ Factory Reset | `showModal('reset')` — confirm dialog | ✅ |

### Console
| Button | Action | Status |
|--------|--------|--------|
| 📋 Copy | `copyTerm()` — copy terminal output | ✅ |
| 📋 All | `copyAllLogs()` — copy all logs | ✅ |
| 🗑 Clear | `clearTerm()` — clear terminal | ✅ |
| ⬇ Auto | `toggleTermScroll()` — auto-scroll toggle | ✅ |
| ⚡ CLI | `showCliDialog()` — open CLI dialog | ✅ |
| Send | `sendCmd()` — send command | ✅ |

### Firmware
| Button | Action | Status |
|--------|--------|--------|
| Choose .bin | File picker for OTA | ✅ |
| Upload | OTA upload with progress | ✅ |

### Presets
| Button | Action | Status |
|--------|--------|--------|
| 💾 Save | `savePreset()` — save to localStorage | ✅ |
| 📥 Export All | `exportAllPresets()` — download all | ✅ |
| 📤 Import | Import presets from file | ✅ |

### Logging
| Button | Action | Status |
|--------|--------|--------|
| ▶ Start | `startLogging()` — begin telemetry capture | ✅ |
| ⏹ Stop | `stopLogging()` — stop capture | ✅ |
| 📥 Download | `downloadLog()` — download CSV | ✅ |
| 🗑 Clear | `clearLog()` — clear data | ✅ |

### Compliance
| Button | Action | Status |
|--------|--------|--------|
| 🔄 Update | `updateCompliance()` — re-evaluate checklist | ✅ |

### Security
| Button | Action | Status |
|--------|--------|--------|
| 👁 Show/Hide | `toggleSecPwd()` — password visibility | ✅ |
| Save Security | Save password/timeout/keys | ✅ |

---

## 📊 Form Fields (50+)

### Identity Tab
| Field | ID | Type | Max | Notes |
|-------|-----|------|-----|-------|
| UAS ID | `uas_id` | text | 20 | e.g. RID-12345678 |
| ID Type | `id_type` | select | — | Enum |
| UA Type | `ua_type` | select | — | Enum |
| Operator ID | `op_id` | text | 20 | |
| Self-ID | `self_id_text` | text | 23 | e.g. Camera drone |
| Operator Lat | `op_lat` | number | — | GPS decimal |
| Operator Lon | `op_lon` | number | — | GPS decimal |
| Operator Alt | `op_alt` | number | — | meters |
| UAS ID 2 | `uas_id2` | text | 20 | Second BasicID |
| ID Type 2 | `id_type2` | select | — | |
| UA Type 2 | `ua_type2` | select | — | |

### Transmission Tab
| Field | ID | Type | Range | Notes |
|-------|-----|------|-------|-------|
| WiFi BCN | `tx_wifi_bcn` | checkbox | — | |
| WiFi NAN | `tx_wifi_nan` | checkbox | — | |
| BLE 4.0 | `tx_ble4` | checkbox | — | |
| BLE 5.0 LR | `tx_ble5` | checkbox | — | |
| WiFi Channel | `wifi_ch` | select | — | |
| WiFi Power | `wifi_pwr` | number | 2-20 | step 0.5 |
| WiFi BCN Rate | `wifi_bcn` | number | — | Hz |
| WiFi NAN Rate | `wifi_nan` | number | — | Hz |
| BLE4 Rate | `bt4_rate` | number | — | Hz |
| BLE4 Power | `bt4_pwr` | number | — | dBm |
| BLE5 Rate | `bt5_rate` | number | — | Hz |
| BLE5 Power | `bt5_pwr` | number | — | dBm |

### Access Point Tab
| Field | ID | Type | Notes |
|-------|-----|------|-------|
| WiFi SSID | `wifi_ssid` | text | max 20 (⚠ spec allows 32) |
| WiFi Password | `wifi_pass` | password | max 20 (⚠ spec allows 63) |
| Web Server | `websrv_en` | checkbox | |

### Flight Controller Tab
| Field | ID | Type | Notes |
|-------|-----|------|-------|
| Protocol | `protocol` | select | AUTO/MAVLink/MSP/NMEA |
| Baud | `baud` | select | |
| MAVLink SysID | `mav_sysid` | number | |
| Broadcast Power | `bcast_pwr` | checkbox | |

### Hardware Tab
| Field | ID | Type | Notes |
|-------|-----|------|-------|
| LED R GPIO | `led_r_gpio` | number | |
| LED G GPIO | `led_g_gpio` | number | |
| LED B GPIO | `led_b_gpio` | number | |
| WS2812 GPIO | `ws2812_gpio` | number | |
| WS2812 Brightness | `ws2812_brightness` | number | |
| UART Port | `uart_port` | select | |
| TX Pin | `tx_pin` | number | |
| RX Pin | `rx_pin` | number | |
| DroneCAN RX | `dronecan_rx_gpio` | number | |
| DroneCAN TX | `dronecan_tx_gpio` | number | |
| DroneCAN Bitrate | `dronecan_bitrate` | select | |
| Lighting Pin 0-4 | `lighting_pin_N` | number | dynamic |
| Lighting Pattern 0-4 | `lighting_pattern_N` | select | dynamic |
| Lighting Phase 0-4 | `lighting_phase_N` | number | dynamic |
| OTA Trigger GPIO | `ota_trigger_gpio` | number | |
| USB MAVLink | `mavlink_usb_enable` | checkbox | |
| Startup Delay | `start_delay_ms` | number | |

### Security Tab
| Field | ID | Type | Notes |
|-------|-----|------|-------|
| Lock Level | `lock_lvl` | select | |
| Password | `sec-password` | password | |
| Timeout | `sec-timeout` | select | |
| Public Key 1-5 | `pubkey1`-`pubkey5` | text | PEM/DER/base64 |
| Option: Arm | `opt_arm` | checkbox | |
| Option: No Save | `opt_nosave` | checkbox | |
| Option: Print | `opt_print` | checkbox | |
| Option: Demo | `opt_demo` | checkbox | |
| Option: Kalman | `opt_kalman` | checkbox | |
| Option: Auth | `opt_auth` | checkbox | |
| Option: MAVLink Arm | `opt_mavlink_arm` | checkbox | |
| Option: MAVLink OpLoc | `opt_mavlink_op_loc` | checkbox | |
| Option: Identity Gate | `opt_identity_gate` | checkbox | |

---

## 🔌 API Endpoints

| Endpoint | Method | Content | Status |
|----------|--------|---------|--------|
| `/` | GET | config.html | ✅ |
| `/config.html` | GET | config.html | ✅ |
| `/style.css` | GET | style.css | ✅ |
| `/app.js` | GET | app.js | ✅ |
| `/api/config` | GET | Full config JSON | ✅ |
| `/api/config` | POST | Update config fields | ✅ |
| `/api/status` | GET | Runtime status JSON | ✅ |
| `/api/capabilities` | GET | Build descriptor (inputs/regions/standards) | ✅ |
| `/api/command` | POST | Signed command dispatch | ✅ |
| `/api/reset` | POST | Factory reset | ✅ |
| `/api/logs` | GET | Log entries | ✅ |
| `/ota` | POST | Firmware upload (multipart) | ✅ |

---

## 🖥️ Desktop App (OmniRID-Desktop)

### Modules
| Module | File | Status |
|--------|------|--------|
| IPC Bridge | `main.js` + `preload.js` | ✅ |
| ASTM Decoder | `src/decoder.js` | ✅ |
| Per-MAC Tracker | `src/tracker.js` | ✅ |
| WiFi/BLE Capture | `src/capture.js` | ✅ |
| UI (6 tabs) | `renderer/app.js` | ✅ |

### Desktop Tabs
1. Dashboard — live device connection
2. Devices — tracked UAS
3. Map — device positions
4. Timeline — telemetry over time
5. Capture — WiFi/BLE/Serial sniffing
6. Settings — app configuration

---

## 🔧 TODO — UI Improvements

### 🔴 High Priority
- [ ] **WiFi SSID/password max length** — fields capped at 20, WiFi spec allows 32/63. Widen in HTML + config struct
- [ ] **Operator lat/lon precision** — stored as float (~1m error). Change to f64 in config
- [ ] **Demo public-safe hardening** — clear localStorage on load, disable password field in demo mode

### 🟡 Medium Priority
- [ ] **manifest.json version** — hardcoded, should auto-generate from CI
- [ ] **Settings search** — index built but verify works across all tabs
- [ ] **Export/import with auth keys** — currently exports config JSON only, not keys
- [ ] **Console CLI history** — add readline-style up/down arrow for last 10 commands
- [ ] **Sensors tab** — placeholder, needs real sensor data display
- [ ] **Mobile responsiveness** — verify sidebar/menu works on small screens
- [ ] **Tooltip consistency** — 63 real / 66 demo buttons have tooltips, verify all

### 🟢 Low Priority
- [ ] **Dark mode polish** — verify all elements have dark theme support
- [ ] **Keyboard shortcuts** — Ctrl+S save, Ctrl+E export, Escape close modals
- [ ] **Loading states** — show spinner during API calls
- [ ] **Error messages** — user-friendly toast notifications on API errors
- [ ] **Compliance region auto-detect** — detect from GPS coords instead of manual selection
- [ ] **OTA progress bar** — show upload progress with percentage
- [ ] **Config diff view** — show what changed before saving
- [ ] **Preset thumbnails** — visual preview of preset configs
- [ ] **Log graph** — plot telemetry data over time (like desktop app)
- [ ] **BLE 5.0 LR info** — explain when LR is available (S3/C6 only)
- [ ] **Lighting preview** — show LED pattern visualization
- [ ] **Compliance per-region requirements** — expandable details for each requirement

---

## 🎨 UI Design Notes

- **Framework**: Bootstrap 5.3.3 (vendored inline in app.js, works offline)
- **Theme**: Light/Dark with glassmorphism CSS
- **Layout**: Sidebar navigation + main content area
- **Responsive**: Mobile hamburger menu, collapsible sidebar
- **Accessibility**: Keyboard/focus accessible tooltips
- **Icons**: Unicode emoji (no icon library dependency)
- **Color scheme**: Primary blue, danger red, success green
- **Splash screen**: Logo + progress bar on initial load
- **Modals**: Factory reset, OTA upload confirmation
- **Toasts**: Center-positioned notifications
