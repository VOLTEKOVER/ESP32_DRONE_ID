# DIY RID Prototype — Zero-Solder BOM

## Main Board

| Option | Chip | BLE | WiFi | Extra | Price | Where |
|---|---|---|---|---|---|---|
| **ESP32-DevKitC V4** (with IPEX) | ESP32 | 4.2 LR | 4 | Cheapest, mature | ~5€ | AliExpress, Amazon |
| **ESP32-S3-DevKitC-1** (with IPEX) | ESP32-S3 | 5.0 LR | 4 + 5 GHz | ESP-NN (ML), USB OTG | ~12€ | AliExpress, Amazon |
| **ESP32-C6-DevKitC-1** (with IPEX) | ESP32-C6 | 5.4 **Channel Sounding** | 6 (OFDMA) | 802.15.4 (Thread/Zigbee) | ~15€ | AliExpress, Amazon |

All have versions with **IPEX/U.FL connector** for external antenna. If not available, get the PCB antenna version — it works fine.

## External Antenna (optional, no soldering)

| Item | Connector | Price |
|---|---|---|
| **Pigtail U.FL → SMA** | Plug & play | ~1€ |
| **2.4GHz SMA antenna** (3-5 dBi) | Screw-on | ~2€ |

Connects in 5 seconds, zero soldering.

## Additional Hardware (Dupont only, zero soldering)

| Component | Connection | Price | Optional? |
|---|---|---|---|
| **WS2812B RGB LED** (Neopixel) | 5V, GND, DATA (3 pin Dupont) | ~1€ | No (visual status) |
| **GPS BN-880 / NEO-6M / NEO-8M** | VCC, GND, TX, RX (4 pin Dupont) | ~6€ | Not recommended (RID without GPS is pointless) |
| **MPU6050 (6-axis IMU)** | 3.3V, GND, SCL, SDA (4 pin Dupont) | ~2€ | Yes (inertial anti-spoofing) |
| **Push button** | 3.3V, GPIO (2 wires) | ~0.5€ | Yes (manual start/stop flight) |

## Power

| How | What you need |
|---|---|
| **USB-C** (test/development) | Power bank or phone charger |
| **From drone** (BEC / PDB) | USB cable or wires to 5V/GND pins |

## Total BOM

| Component | Price |
|---|---|
| ESP32 dev board (with IPEX) | 5-15€ |
| Pigtail U.FL → SMA | 1€ |
| SMA 2.4GHz antenna | 2€ |
| WS2812B LED | 1€ |
| GPS BN-880 | 6€ |
| MPU6050 (optional) | 2€ |
| Dupont cable kit (40x) | 2€ |
| **Total** | **18-29€** |

Everything connects with Dupont cables. **No soldering required.** Anyone can replicate this.

## Pin Configuration (via web UI)

```
GPS TX  → GPIO 18
GPS RX  → GPIO 17
LED DATA → GPIO 48 (WS2812B) or R/G/B on separate GPIOs
IMU SCL → GPIO 9  (MPU6050, optional)
IMU SDA → GPIO 8  (MPU6050, optional)
BUTTON  → GPIO 0  (optional)
```

## Innovation (all software)

| Feature | What's needed | Feasibility |
|---|---|---|
| ESP-NOW Mesh RID | Software only | Today |
| HW Attestation (Digital Signature) | Software only (ESP32 built-in) | Today |
| BT Channel Sounding (ranging) | ESP32-C6 | Today |
| Trajectory Prediction (Kalman) | Software only | Today |
| Anti-spoofing (GPS + IMU) | MPU6050 + software | Today |
| WiFi 6 OFDMA | ESP32-C6 | Today |
| WiFi 5 GHz | ESP32-S3 or C6 | Today |
