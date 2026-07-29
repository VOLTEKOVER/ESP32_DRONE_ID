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

This project implements a **WiFi AP + REST API** Remote ID transmitter:

- **Web config server** runs on `192.168.4.1` (AP network) — no inbound internet exposure
- **Authentication pages** available via Ed25519 signing (ASTM F3411-22a compliant)
- **3-tier lock system**: Level 0 (open), Level 1 (Ed25519 signed commands), Level 2 (eFuse permanent)
- **OTA updates** with SHA-256 integrity + optional Ed25519 signature verification
- **Key management** — Ed25519 keys stored in NVS (encrypted at rest if flash encryption enabled)

### Web Configuration

The web configuration interface is served on the AP network (192.168.4.1) with
optional WPA2-PSK password protection. At lock level 0, no authentication is
required for config changes. At level ≥ 1, all POST requests to `/api/config`
and `/ota` require an `X-Signature` header (Ed25519 signature over the SHA-256
hash of the body).

### Firmware Updates

OTA updates are uploaded via the web UI as HTTP POST to `/ota`. The browser
computes a SHA-256 hash (Web Crypto API) sent as `X-Expected-SHA256` header.
The server independently verifies the hash. At lock level ≥ 1, an Ed25519
signature (`X-Signature` header) is also required. A dual-OTA partition scheme
ensures automatic rollback on failure.

### Security Hardening

- **cJSON parser** — all JSON inputs parsed with cJSON instead of naive `strstr()`
- **Rate limiting** — 10 failed signature attempts per 60s sliding window, then lockout
- **strncpy null-termination** — all string fields explicitly null-terminated
- **psa_crypto_init()** — PSA Crypto API initialized at boot for ESP-IDF v5+
- **Shared verification module** — `rid_security.c/h` used by both web config and OTA
