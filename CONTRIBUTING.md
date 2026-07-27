# Contributing to ESP Remote ID

Thanks for your interest in contributing! This guide will help you get started.

## Development Setup

### Prerequisites

- [ESP-IDF v6.0.1](https://docs.espressif.com/projects/esp-idf/en/v6.0.1/esp32/get-started/)
- Git with submodules
- (Optional) Node.js 22+ for RID Hub desktop app

### Building Firmware

```bash
cd ESP32_DRONE_REMOTE_ID_Firmware
idf.py set-target esp32    # or esp32s3, esp32c6
idf.py build
idf.py -p /dev/ttyUSB0 flash monitor
```

### Building RID Hub (Desktop App)

```bash
cd RID_Hub
npm ci
npm start                    # run in dev mode
npx electron-builder --dir   # pack without installer
```

## Project Structure

```
ESP32_DRONE_REMOTE_ID_Firmware/
├── components/esp_remote_id/    # Core Remote ID component
│   ├── src/                     # Source files
│   ├── include/                 # Public headers
│   ├── mavlink/                 # Vendored MAVLink c_library_v2
│   └── webui/                   # Embedded web UI (config.html)
├── main/                        # App entry point
└── sdkconfig.defaults           # Default Kconfig values

RID_Hub/                         # Electron desktop app
docs/                            # GitHub Pages documentation
```

## Making Changes

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-change`
3. Make your changes
4. Build for all3 targets to verify no regressions:
   ```bash
   idf.py set-target esp32 && idf.py build
   idf.py set-target esp32s3 && idf.py build
   idf.py set-target esp32c6 && idf.py build
   ```
5. Test on real hardware if possible
6. Commit with a clear message (see below)
7. Push and open a Pull Request

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): short description

Longer explanation if needed.
```

Types: `fix`, `feat`, `docs`, `ci`, `refactor`, `test`, `chore`

Examples:
- `fix(wifi): correct AP reconfiguration on SSID change`
- `feat(auth): add Ed25519 signature verification`
- `docs: update quick start guide`

## Code Style

- C: follow ESP-IDF coding standards
- No Kconfig configurations — all settings via web interface
- JavaScript: keep web UI lightweight (embedded in flash, ~16KB budget)
- Add `Wno-error` flags only when necessary (document why)

## Reporting Bugs

Use the [Bug Report template](https://github.com/VOLTEKOVER/ESP_DRONE_REMOTEID/issues/new?template=bug_report.yml) with:
- Clear description and reproduction steps
- Hardware details (ESP32 model, board, antenna)
- Firmware version and build target
- Serial output if applicable

## License

By contributing, you agree that your contributions will be licensed under the
[Apache License 2.0](LICENSE).
