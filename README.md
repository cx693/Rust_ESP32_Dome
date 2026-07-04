[English](README.md) | [中文](README_CN.md)

<img src="https://cdn.nlark.com/yuque/0/2026/png/67055297/1780842853453-2f7c908d-8ee3-443d-b967-ae06d26315c0.png" width="767" title="" crop="0,0,1,1" id="YMKvO" class="ne-image">

# Rust on ESP32-S3

> **Author: CXi** | **Board: ESP32-S3R8N8 (Lichuang)**

A collection of Rust bare-metal examples for ESP32-S3, covering GPIO, PWM, UART, I2C, SPI, ADC, WiFi, BLE, and LCD display projects.

---

## 📁 Project Structure

```
Rust_ESP32_Dome/
├── examples/                  ← Basic examples (20 projects)
│   ├── hello/                 → Hello World (RTT printing)
│   ├── LED/                   → LED blink (GPIO output)
│   ├── LED呼吸灯/             → Breathing LED (hardware PWM)
│   ├── PWM/                   → RGB LED 7-color + breathing
│   ├── 按键LED/               → Button + LED (GPIO input)
│   ├── 外部中断/              → External interrupt (GPIO ISR)
│   ├── 定时器/                → Timer interrupt (1s toggle)
│   ├── 串口/                  → UART receive (interrupt-driven)
│   ├── I2C/                   → I2C bus scanner
│   ├── SPI/                   → SPI LCD snow animation
│   ├── ADC/                   → Internal temperature sensor
│   ├── ADC_电压/              → External voltage measurement
│   ├── 监听模式/              → WiFi Sniffer (Beacon scan)
│   ├── WIFI-客户端模式/       → WiFi STA + HTTP server
│   ├── 热点模式/              → WiFi AP + DHCP + Web dashboard
│   ├── 热点模式_副本/         → WiFi AP (backup copy)
│   ├── AP+STA/                → WiFi AP+STA provisioning
│   ├── BLE_扫描/              → BLE device scanner
│   ├── BIL_peripheral/        → BLE battery service
│   └── 模版/                  → Project template
│
├── dome/                      ← Advanced display projects (4 projects)
│   ├── SPI_中文显示/          → Chinese font rendering
│   ├── SPI_小球下落/          → Ball physics simulation
│   ├── SPI_正方体/            → 3D rotating cube
│   └── SPI_月薪猫/            → GIF animation player
│
├── README.md                  ← English documentation
└── README_CN.md               ← Chinese documentation
```

---

## 🚀 Quick Start

### 1. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. Install ESP32 Toolchain

```bash
cargo install espup --locked
espup install
cargo install espflash --locked
cargo install probe-rs-tools --locked
```

### 3. Setup Environment Variables

```bash
# Linux / macOS
source $HOME/export-esp.sh

# Add to ~/.zshrc or ~/.bashrc for auto-loading
echo "source $HOME/export-esp.sh" >> ~/.zshrc
```

### 4. Build & Flash

```bash
cd examples/hello
cargo run
```

---

## 📚 Examples List

### GPIO Basics

| Project | Description | Tutorial |
|---------|-------------|----------|
| [hello](examples/hello/) | Hello World - RTT printing every 500ms | [教程.md](examples/hello/教程.md) |
| [LED](examples/LED/) | LED blink on GPIO48 | [教程.md](examples/LED/教程.md) |
| [按键LED](examples/按键LED/) | Button (GPIO0) controls LED (GPIO48) | [教程.md](examples/按键LED/教程.md) |

### PWM

| Project | Description | Tutorial |
|---------|-------------|----------|
| [LED呼吸灯](examples/LED呼吸灯/) | Hardware PWM breathing LED | [教程.md](examples/LED呼吸灯/教程.md) |
| [PWM](examples/PWM/) | RGB LED 7-color cycle + breathing | [教程.md](examples/PWM/教程.md) |

### Interrupts

| Project | Description | Tutorial |
|---------|-------------|----------|
| [外部中断](examples/外部中断/) | GPIO falling edge interrupt | [教程.md](examples/外部中断/教程.md) |
| [定时器](examples/定时器/) | Timer interrupt every 1 second | [教程.md](examples/定时器/教程.md) |

### Communication

| Project | Description | Tutorial |
|---------|-------------|----------|
| [串口](examples/串口/) | UART1 interrupt receive (115200 baud) | [教程.md](examples/串口/教程.md) |
| [I2C](examples/I2C/) | I2C bus scanner (0x08~0x77) | [教程.md](examples/I2C/教程.md) |
| [SPI](examples/SPI/) | ST7789 LCD snow animation (DMA) | [教程.md](examples/SPI/教程.md) |

### ADC

| Project | Description | Tutorial |
|---------|-------------|----------|
| [ADC](examples/ADC/) | Internal temperature sensor | [教程.md](examples/ADC/教程.md) |
| [ADC_电压](examples/ADC_电压/) | External voltage (GPIO1, 0~3.1V) | [教程.md](examples/ADC_电压/教程.md) |

### WiFi

| Project | Description | Tutorial |
|---------|-------------|----------|
| [监听模式](examples/监听模式/) | WiFi Sniffer - Beacon frame capture | [教程.md](examples/监听模式/教程.md) |
| [WIFI-客户端模式](examples/WIFI-客户端模式/) | WiFi STA + HTTP server + NTP | [教程.md](examples/WIFI-客户端模式/教程.md) |
| [热点模式](examples/热点模式/) | WiFi AP + DHCP + Web dashboard | [教程.md](examples/热点模式/教程.md) |
| [AP+STA](examples/AP+STA/) | AP+STA dual-mode provisioning | [教程.md](examples/AP+STA/教程.md) |

### BLE

| Project | Description | Tutorial |
|---------|-------------|----------|
| [BLE_扫描](examples/BLE_扫描/) | BLE device scanner | [教程.md](examples/BLE_扫描/教程.md) |
| [BIL_peripheral](examples/BIL_peripheral/) | BLE battery service peripheral | [教程.md](examples/BIL_peripheral/教程.md) |

### Template

| Project | Description | Tutorial |
|---------|-------------|----------|
| [模版](examples/模版/) | Minimal project template | [教程.md](examples/模版/教程.md) |

---

## 🖥️ Display Projects (dome/)

| Project | Description | Tutorial |
|---------|-------------|----------|
| [SPI_中文显示](dome/SPI_中文显示/) | Chinese font rendering (anti-aliased) | [教程.md](dome/SPI_中文显示/教程.md) |
| [SPI_小球下落](dome/SPI_小球下落/) | 5-ball physics simulation (collision) | [教程.md](dome/SPI_小球下落/教程.md) |
| [SPI_正方体](dome/SPI_正方体/) | 3D rotating cube (perspective projection) | [教程.md](dome/SPI_正方体/教程.md) |
| [SPI_月薪猫](dome/SPI_月薪猫/) | GIF animation player (LZW decode) | [教程.md](dome/SPI_月薪猫/教程.md) |

### Display Project Series (Recommended Order)

```
1. SPI_中文显示    → SPI basics, DMA, framebuffer, font rendering
2. SPI_小球下落    → Frame animation, physics, collision detection
3. SPI_正方体      → 3D projection, back-face culling, depth sorting
4. SPI_月薪猫      → GIF format, LZW compression, heap allocation
```

---

## 🔧 Hardware Requirements

| Component | Model | Notes |
|-----------|-------|-------|
| Board | ESP32-S3R8N8 (Lichuang) | 8MB Flash + 8MB PSRAM |
| USB Cable | Type-C | For programming and power |
| LCD (dome/) | ST7789 240×240 IPS | SPI interface, 1.3 inch |
| RGB LED (PWM/) | Common cathode | R→GPIO4, G→GPIO5, B→GPIO6 |

### Pin Mapping (Display Projects)

```
ESP32-S3          ST7789 LCD
─────────────────────────────
GPIO48 (SCK)  ──→ SCK
GPIO47 (MOSI) ──→ SDA/MOSI
GPIO2         ──→ DC
GPIO1         ──→ RES
GPIO0         ──→ BLK
3.3V          ──→ VCC
GND           ──→ GND
```

---

## ⚙️ Tool Installation

### Install espflash / probe-rs

```bash
cargo install espflash --locked
cargo install probe-rs-tools --locked
```

### Check chip version

```bash
espflash board-info
```

### Verify debug interface (JTAG)

```bash
probe-rs info
```

---

## 🔨 Build & Flash

```bash
# Build only
cargo build

# Build and flash
cargo run

# Build release (recommended for embedded)
cargo build --release
cargo run --release
```

### Entering Download Mode (if flash fails)

1. **Hold BOOT button**
2. **Press RST button**
3. **Release BOOT button**
4. Re-run `cargo run`

---

## 🛠️ VSCode Settings

Replace `.vscode/settings.json` for syntax highlighting:

```json
{
    "rust-analyzer.cargo.allTargets": false,
    "rust-analyzer.cargo.target": null,
    "rust-analyzer.check.onSave.command": "clippy",
    "rust-analyzer.cargo.features": "default",
    "rust-analyzer.server.extraEnv": {
        "RUSTUP_TOOLCHAIN": "stable"
    }
}
```

---

## 📖 WSL/Linux Setup

WSL needs [wsl-usb-manager](https://github.com/nickbeth/wsl-usb-manager/releases)

```bash
# Download probe-rs udev rules
sudo wget -O /etc/udev/rules.d/69-probe-rs.rules https://probe.rs/files/69-probe-rs.rules

# Reload udev rules
sudo udevadm control --reload
sudo udevadm trigger

# Add user to groups
sudo usermod -a -G dialout $USER
sudo usermod -a -G plugdev $USER
```

---

## 📚 References

| Resource | Link |
|----------|------|
| Rust on ESP Book | https://esp-rs.github.io/book/ |
| esp-hal Repository | https://github.com/esp-rs/esp-hal |
| esp-hal Examples | https://github.com/esp-rs/esp-hal/tree/main/examples |
| ESP32-S3 Datasheet | https://www.espressif.com/sites/default/files/documentation/esp32-s3_datasheet_cn.pdf |
| probe-rs | https://probe.rs/ |
| Embassy (Async) | https://embassy.dev/ |

---

## 📄 License

MIT License

---

> **Author: CXi** | Feel free to ⭐ Star / 🍴 Fork / 💬 Issues
