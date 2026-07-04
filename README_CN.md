[English](README.md) | [中文](README_CN.md)

<img src="https://cdn.nlark.com/yuque/0/2026/png/67055297/1780842853453-2f7c908d-8ee3-443d-b967-ae06d26315c0.png" width="767" title="" crop="0,0,1,1" id="YMKvO" class="ne-image">

# Rust on ESP32-S3 裸机开发示例

> **作者：CXi** | **开发板：嘉立创 ESP32-S3R8N8**

一套完整的 ESP32-S3 Rust 裸机开发示例，涵盖 GPIO、PWM、UART、I2C、SPI、ADC、WiFi、BLE 及 LCD 显示屏项目。每个项目都配有详细的中文教程文档。

---

## 📁 项目结构

```
Rust_ESP32_Dome/
├── examples/                  ← 基础示例（20 个项目）
│   ├── hello/                 → Hello World（RTT 打印）
│   ├── LED/                   → LED 闪烁（GPIO 输出）
│   ├── LED呼吸灯/             → 呼吸灯（硬件 PWM）
│   ├── PWM/                   → RGB 三色 LED 7 色循环
│   ├── 按键LED/               → 按键控制 LED（GPIO 输入）
│   ├── 外部中断/              → 外部中断（GPIO ISR）
│   ├── 定时器/                → 定时器中断（1 秒翻转）
│   ├── 串口/                  → 串口接收（中断驱动）
│   ├── I2C/                   → I2C 总线扫描器
│   ├── SPI/                   → SPI LCD 雪花动画
│   ├── ADC/                   → 内部温度传感器
│   ├── ADC_电压/              → 外部电压测量
│   ├── 监听模式/              → WiFi 抓包（Beacon 扫描）
│   ├── WIFI-客户端模式/       → WiFi STA + HTTP 服务器
│   ├── 热点模式/              → WiFi AP + DHCP + Web 仪表盘
│   ├── AP+STA/                → WiFi AP+STA 配网
│   ├── BLE_扫描/              → BLE 设备扫描器
│   ├── BIL_peripheral/        → BLE 电池服务外设
│   └── 模版/                  → 项目模板
│
├── dome/                      ← 进阶显示项目（4 个项目）
│   ├── SPI_中文显示/          → 中文字体渲染（抗锯齿）
│   ├── SPI_小球下落/          → 小球物理模拟（碰撞检测）
│   ├── SPI_正方体/            → 3D 正方体旋转（透视投影）
│   └── SPI_月薪猫/            → GIF 动画播放器（LZW 解码）
│
├── README.md                  ← 英文文档
└── README_CN.md               ← 中文文档
```

---

## 🚀 快速开始

### 1. 安装 Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. 安装 ESP32 工具链

```bash
cargo install espup --locked
espup install
cargo install espflash --locked
cargo install probe-rs-tools --locked
```

### 3. 配置环境变量

```bash
# Linux / macOS
source $HOME/export-esp.sh

# 添加到 ~/.zshrc 或 ~/.bashrc 实现自动加载
echo "source $HOME/export-esp.sh" >> ~/.zshrc
```

### 4. 编译烧录

```bash
cd examples/hello
cargo run
```

---

## 📚 示例列表

### GPIO 基础

| 项目 | 说明 | 教程 |
|------|------|------|
| [hello](examples/hello/) | Hello World - 每 500ms 打印计数 | [教程.md](examples/hello/教程.md) |
| [LED](examples/LED/) | GPIO48 LED 闪烁 | [教程.md](examples/LED/教程.md) |
| [按键LED](examples/按键LED/) | GPIO0 按键控制 GPIO48 LED | [教程.md](examples/按键LED/教程.md) |

### PWM 输出

| 项目 | 说明 | 教程 |
|------|------|------|
| [LED呼吸灯](examples/LED呼吸灯/) | 硬件 PWM 呼吸灯 | [教程.md](examples/LED呼吸灯/教程.md) |
| [PWM](examples/PWM/) | RGB LED 7 色循环 + 呼吸灯 | [教程.md](examples/PWM/教程.md) |

### 中断

| 项目 | 说明 | 教程 |
|------|------|------|
| [外部中断](examples/外部中断/) | GPIO 下降沿中断 | [教程.md](examples/外部中断/教程.md) |
| [定时器](examples/定时器/) | 定时器中断（每 1 秒） | [教程.md](examples/定时器/教程.md) |

### 通信外设

| 项目 | 说明 | 教程 |
|------|------|------|
| [串口](examples/串口/) | UART1 中断接收（115200 波特率） | [教程.md](examples/串口/教程.md) |
| [I2C](examples/I2C/) | I2C 总线扫描器（0x08~0x77） | [教程.md](examples/I2C/教程.md) |
| [SPI](examples/SPI/) | ST7789 LCD 雪花动画（DMA） | [教程.md](examples/SPI/教程.md) |

### 模拟采集

| 项目 | 说明 | 教程 |
|------|------|------|
| [ADC](examples/ADC/) | 内部温度传感器 | [教程.md](examples/ADC/教程.md) |
| [ADC_电压](examples/ADC_电压/) | 外部电压测量（GPIO1, 0~3.1V） | [教程.md](examples/ADC_电压/教程.md) |

### WiFi

| 项目 | 说明 | 教程 |
|------|------|------|
| [监听模式](examples/监听模式/) | WiFi Sniffer - Beacon 帧抓包 | [教程.md](examples/监听模式/教程.md) |
| [WIFI-客户端模式](examples/WIFI-客户端模式/) | WiFi STA + HTTP 服务器 + NTP | [教程.md](examples/WIFI-客户端模式/教程.md) |
| [热点模式](examples/热点模式/) | WiFi AP + DHCP + Web 仪表盘 | [教程.md](examples/热点模式/教程.md) |
| [AP+STA](examples/AP+STA/) | AP+STA 双模式配网 | [教程.md](examples/AP+STA/教程.md) |

### BLE 蓝牙

| 项目 | 说明 | 教程 |
|------|------|------|
| [BLE_扫描](examples/BLE_扫描/) | BLE 设备扫描器 | [教程.md](examples/BLE_扫描/教程.md) |
| [BIL_peripheral](examples/BIL_peripheral/) | BLE 电池服务外设 | [教程.md](examples/BIL_peripheral/教程.md) |

### 模板

| 项目 | 说明 | 教程 |
|------|------|------|
| [模版](examples/模版/) | 最小化项目模板 | [教程.md](examples/模版/教程.md) |

---

## 🖥️ 显示屏项目（dome/）

| 项目 | 说明 | 教程 |
|------|------|------|
| [SPI_中文显示](dome/SPI_中文显示/) | 中文字体渲染（抗锯齿） | [教程.md](dome/SPI_中文显示/教程.md) |
| [SPI_小球下落](dome/SPI_小球下落/) | 5 小球物理模拟（碰撞检测） | [教程.md](dome/SPI_小球下落/教程.md) |
| [SPI_正方体](dome/SPI_正方体/) | 3D 正方体旋转（透视投影） | [教程.md](dome/SPI_正方体/教程.md) |
| [SPI_月薪猫](dome/SPI_月薪猫/) | GIF 动画播放器（LZW 解码） | [教程.md](dome/SPI_月薪猫/教程.md) |

### 显示屏项目系列（推荐学习顺序）

```
1. SPI_中文显示    → SPI 基础、DMA 传输、帧缓冲区、字体渲染
2. SPI_小球下落    → 帧动画、物理引擎、碰撞检测
3. SPI_正方体      → 3D 投影、背面剔除、深度排序
4. SPI_月薪猫      → GIF 格式、LZW 压缩、堆内存管理
```

---

## 🔧 硬件准备

| 物料 | 型号 | 说明 |
|------|------|------|
| 开发板 | 嘉立创 ESP32-S3R8N8 | 8MB Flash + 8MB PSRAM |
| USB 数据线 | Type-C | 用于烧录和供电 |
| LCD 屏幕（dome/） | ST7789 240×240 IPS | SPI 接口，1.3 寸 |
| RGB LED（PWM/） | 共阴极 | R→GPIO4, G→GPIO5, B→GPIO6 |

### 引脚接线（显示屏项目）

```
ESP32-S3          ST7789 LCD
─────────────────────────────
GPIO48 (SCK)  ──→ SCK
GPIO47 (MOSI) ──→ SDA/MOSI
GPIO2         ──→ DC  (数据/命令选择)
GPIO1         ──→ RES (复位)
GPIO0         ──→ BLK (背光)
3.3V          ──→ VCC
GND           ──→ GND
```

---

## ⚙️ 工具安装

### 安装 espflash / probe-rs

```bash
cargo install espflash --locked
cargo install probe-rs-tools --locked
```

### 查看芯片版本

```bash
espflash board-info
```

### 验证调试接口（JTAG）

```bash
probe-rs info
```

---

## 🔨 编译与烧录

```bash
# 仅编译
cargo build

# 编译并烧录
cargo run

# Release 模式（推荐嵌入式使用）
cargo build --release
cargo run --release
```

### 进入下载模式（烧录失败时）

1. **按住 BOOT 按键不放**
2. **按一下 RST 按键**
3. **松开 BOOT 按键**
4. 重新执行 `cargo run`

---

## 🛠️ VSCode 配置

替换 `.vscode/settings.json` 获得语法提示：

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

## 📖 WSL/Linux 配置

WSL 需要安装 [wsl-usb-manager](https://github.com/nickbeth/wsl-usb-manager/releases)

```bash
# 下载 probe-rs udev 规则
sudo wget -O /etc/udev/rules.d/69-probe-rs.rules https://probe.rs/files/69-probe-rs.rules

# 重新加载 udev 规则
sudo udevadm control --reload
sudo udevadm trigger

# 添加用户到串口组
sudo usermod -a -G dialout $USER
sudo usermod -a -G plugdev $USER
```

---

## 📚 参考资料

| 资源 | 链接 |
|------|------|
| Rust on ESP 官方教程 | https://esp-rs.github.io/book/ |
| esp-hal 仓库 | https://github.com/esp-rs/esp-hal |
| esp-hal 官方示例 | https://github.com/esp-rs/esp-hal/tree/main/examples |
| ESP32-S3 数据手册 | https://www.espressif.com/sites/default/files/documentation/esp32-s3_datasheet_cn.pdf |
| probe-rs 调试工具 | https://probe.rs/ |
| Embassy 异步运行时 | https://embassy.dev/ |

---

## 📄 许可证

MIT License

---

> **作者：CXi** | 欢迎 ⭐ Star / 🍴 Fork / 💬 Issues
