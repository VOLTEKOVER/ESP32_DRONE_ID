# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| latest main | Yes |
| < latest | No |

## Reporting a Vulnerability

If you discover a security vulnerability in ESP Remote ID, please report it
responsibly. **Do not open a public GitHub issue for security vulnerabilities.**

Instead, please email the maintainers or use
[GitHub's private vulnerability reporting](https://github.com/VOLTEKOVER/ESP_DRONE_REMOTEID/security/advisories/new).

### What to include

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

### What to expect

- Acknowledgment within 48 hours
- Assessment within 1 week
- Fix or mitigation plan communicated to you

## Security Considerations

This project implements a **broadcast-only** Remote ID transmitter:

- **No inbound connections** — the device only broadcasts WiFi BLE/NaN frames
- **No authentication bypass** — web config requires physical network access (AP mode)
- **OTA updates** — firmware updates are delivered over HTTPS from GitHub Releases
- **Key management** — Ed25519 auth keys are stored in NVS (not flash filesystem)

### Web Configuration

The web configuration interface is served on the AP network (192.168.4.1). It is
intended for local configuration only. There is no authentication on the web
interface — anyone connected to the AP can modify settings.

### Firmware Updates

OTA updates fetch `manifest.json` from GitHub Releases over HTTPS. The firmware
binaries are not signed by this project (ESP-IDF Secure Boot is not enabled by
default). Users should verify firmware integrity when possible.
