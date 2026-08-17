# Contributing to OmniRID

Thanks for your interest in contributing! This guide will help you get started.

## Development Setup

### Prerequisites

- Rust stable toolchain (`rustup`)
- (Optional) `espup` for ESP32 cross-compilation
- (Optional) Node.js 22+ for RID Hub desktop app

### Building the Workspace

```bash
cd OmniRID
cargo build --workspace           # host build
cargo test --workspace            # run all tests (312)
cargo clippy --workspace -- -D warnings   # lint
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
OmniRID/
├── firmware/              # Core Rust crates
│   ├── app/               # Application logic (lib + bin)
│   ├── rid-core/          # Kalman filter, security, TX pipeline
│   ├── rid-app/           # Feature logic (NVS, OTA, web, LED, config)
│   └── rid-interface/     # Transport abstraction (WiFi/BLE/USB)
├── inputs/                # Protocol parsers (MAVLink, MSP, NMEA, DroneCAN)
├── outputs/               # Protocol encoders (ASTM, NAN, BLE4, pack)
├── external-libs/         # Vendored C dependencies (opendroneid-sys)
└── hardware/bsp-esp32/    # ESP-IDF glue (standalone workspace)

RID_Hub/                   # Electron desktop app
docs/                      # GitHub Pages documentation
```

## Making Changes

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-change`
3. Make your changes
4. Verify no regressions:
   ```bash
   cargo test --workspace
   cargo clippy --workspace -- -D warnings
   ```
5. Test on real hardware if possible
6. Update [`todolist/softwarestatus.md`](todolist/softwarestatus.md) for file-level changes
7. Commit with a clear message (see below)
8. Push and open a Pull Request

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

- Rust: `cargo fmt` (edition 2024), `cargo clippy -D warnings`
- `no_std` for firmware crates; `std` only in `bsp-esp32`
- Keep `unsafe` out of core crates (only in hardware glue)
- Update [`todolist/softwarestatus.md`](todolist/softwarestatus.md) with any new/changed source files

## Reporting Bugs

Use the [Bug Report template](https://github.com/VOLTEKOVER/ESP_DRONE_REMOTEID/issues/new?template=bug_report.yml) with:
- Clear description and reproduction steps
- Hardware details (ESP32 model, board, antenna)
- Firmware version and build target
- Serial output if applicable

## License

By contributing, you agree that your contributions will be licensed under the
[Apache License 2.0](LICENSE).
