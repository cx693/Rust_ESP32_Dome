//! # I2C 总线扫描器 (ESP32)
//!
//! 本程序扫描 I2C 总线上的所有设备，并打印出发现的设备地址。
//! 这是学习 I2C 通信的基础示例。
//!
//! ## I2C 地址说明
//! - 7 位地址：0x08 - 0x77（标准范围）
//! - 8 位地址 = 7 位地址 << 1 | R/W 位
//!   - 写地址：7 位地址 << 1 | 0
//!   - 读地址：7 位地址 << 1 | 1
//!
//! ## 硬件连接
//! - SDA: GPIO4（数据线）
//! - SCL: GPIO5（时钟线）

#![no_main]
#![no_std]

use esp_hal::{delay::Delay, i2c::master::I2c, main};
use panic_rtt_target as _;
use rtt_target::{rprint, rprintln, rtt_init_print};

/// I2C 扫描的起始地址（0x08 是有效地址的起始）
const SCAN_START: u8 = 0x08;

/// I2C 扫描的结束地址（0x78 是保留地址的起始）
const SCAN_END: u8 = 0x78;

/// 根据 I2C 地址返回常见设备名称
///
/// 这个函数帮助识别常见的 I2C 设备，方便学习者理解扫描结果。
/// 注意：同一地址可能对应多种设备，这里只列出常见的。
///
/// # 参数
/// - `addr`: 7 位 I2C 地址
///
/// # 返回
/// - `Some(&str)`: 设备名称
/// - `None`: 未知设备
fn device_name(addr: u8) -> Option<&'static str> {
    match addr {
        // 特殊地址
        0x03 => Some("General Call"),
        0x08..=0x0F => Some("HS-mode Master"),

        // LED 驱动
        0x10..=0x11 => Some("LED Driver (IS31FL3731)"),

        // 加速度传感器
        0x1C => Some("Accel (MMA845xQ/LIS3DH)"),
        0x1D => Some("Accel (ADXL345/MMA845xQ)"),

        // 磁力计
        0x1E => Some("Magnetometer (HMC5883L)"),

        // I/O 扩展器
        0x20..=0x27 => Some("I/O Expander (PCF8574)"),

        // IMU 惯性测量单元
        0x28 => Some("IMU (MPU6050/BNO055)"),

        // 飞行时间传感器
        0x29 => Some("ToF (VL53L0X/VL53L1X)"),

        // DAC 数模转换器
        0x2A..=0x2F => Some("DAC (MCP4725)"),

        // 传感器
        0x38 => Some("Sensor (BH1750/AHT20)"),
        0x39 => Some("Light (APDS-9960/TSL2561)"),

        // OLED 显示屏
        0x3C => Some("OLED SSD1306 (128x32)"),
        0x3D => Some("OLED SSD1306 (128x64)"),

        // ADC/温度传感器
        0x40..=0x43 => Some("ADC/Temp (INA219/TMP102)"),

        // 温湿度传感器
        0x44..=0x45 => Some("TH Sensor (SHT30/SHT31)"),

        // ADC 模数转换器
        0x48..=0x4B => Some("ADC (ADS1115/ADS1015)"),

        // EEPROM 存储器
        0x50..=0x57 => Some("EEPROM (AT24Cxx)"),

        // 红外温度传感器
        0x5A => Some("IR Thermo (MLX90614)"),

        // 温湿度传感器
        0x5C => Some("TH Sensor (AM2320/DHT12)"),

        // 气体/光线传感器
        0x60 => Some("Sensor (SGP30/SI1145)"),

        // CO2 传感器
        0x62 => Some("CO2 Sensor (SCD40/SCD41)"),

        // RTC/IMU
        0x68 => Some("RTC/IMU (DS3231/MPU6050)"),
        0x69 => Some("IMU (MPU6050 alt addr)"),

        // IMU
        0x6A..=0x6B => Some("IMU (LSM6DS3/LSM9DS1)"),

        // 环境传感器
        0x76..=0x77 => Some("Env Sensor (BME280/BMP280)"),

        // 未知设备
        _ => None,
    }
}

// ESP-IDF bootloader 初始化宏
esp_bootloader_esp_idf::esp_app_desc!();

/// 主函数 - I2C 扫描器入口
///
/// 程序流程：
/// 1. 初始化 RTT 调试输出
/// 2. 配置 I2C 外设（100kHz 标准模式）
/// 3. 扫描 0x08-0x77 范围内的所有地址
/// 4. 打印扫描结果和设备信息
#[main]
fn main() -> ! {
    // 初始化 RTT 调试输出（用于串口调试）
    rtt_init_print!();

    // 初始化 ESP32 外设
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // 创建延时对象
    let delay = Delay::new();

    // 配置 I2C 外设
    // - I2C0: 使用第一个 I2C 控制器
    // - GPIO4: SDA（数据线）
    // - GPIO5: SCL（时钟线）
    // - 默认配置：100kHz 标准模式
    let mut i2c = I2c::new(peripherals.I2C0, esp_hal::i2c::master::Config::default())
        .unwrap()
        .with_sda(peripherals.GPIO4)
        .with_scl(peripherals.GPIO5);

    // 等待 I2C 总线稳定
    delay.delay_millis(500);

    // 打印扫描信息头部
    rprint!("\r\n");
    rprintln!("=============================");
    rprintln!("  I2C Bus Scanner (ESP32)");
    rprintln!("  Range: 0x{:02X} - 0x{:02X}", SCAN_START, SCAN_END - 1);
    rprintln!("  Speed: 100kHz (Standard)");
    rprintln!("=============================");
    rprintln!();

    // 打印列标题（0-F）
    rprint!("     ");
    for col in 0..16u8 {
        rprint!("{:02X} ", col);
    }
    rprintln!();

    // 打印分隔线
    rprint!("    ");
    for _ in 0..16 {
        rprint!("---");
    }
    rprintln!();

    // 存储发现的设备地址
    let mut found_count: u8 = 0;
    let mut found_addrs: [u8; 32] = [0u8; 32];

    // 扫描 I2C 总线
    // 按行扫描，每行 16 个地址
    for row in 0..8u8 {
        // 打印行标题（地址高 4 位）
        rprint!("{:02X}: ", row * 16);

        // 扫描当前行的 16 个地址
        for col in 0..16u8 {
            let addr = row * 16 + col;

            // 检查地址是否在扫描范围内
            if !(SCAN_START..SCAN_END).contains(&addr) {
                rprint!("   ");  // 跳过无效地址
                continue;
            }

            // 尝试向设备发送数据来检测设备是否存在
            // 发送 1 字节数据（0x00），如果设备应答则表示存在
            let data: [u8; 1] = [0];
            match i2c.write(addr, &data) {
                Ok(_) => {
                    // 设备应答，打印地址
                    rprint!("{:02X}", addr);

                    // 保存发现的地址（防止数组越界）
                    if (found_count as usize) < found_addrs.len() {
                        found_addrs[found_count as usize] = addr;
                    }
                    found_count += 1;
                }
                Err(_) => {
                    // 设备未应答，打印占位符
                    rprint!("..");
                }
            }

            rprint!(" ");  // 地址间空格
        }
        rprintln!();  // 换行
    }

    // 打印扫描结果摘要
    rprintln!();
    rprintln!("=============================");
    rprintln!("  Scan Done: {} device(s)", found_count);
    rprintln!("=============================");

    // 如果发现设备，打印详细信息
    if found_count > 0 {
        rprintln!();
        for i in 0..found_count.min(32) {
            let addr = found_addrs[i as usize];

            // 打印设备序号和 7 位地址
            rprint!("  [{}] 0x{:02X}", i + 1, addr);

            // 打印 8 位读写地址
            rprint!(" (W:0x{:02X} R:0x{:02X})", addr << 1, (addr << 1) | 1);

            // 打印设备名称（如果已知）
            if let Some(name) = device_name(addr) {
                rprint!(" {}", name);
            }
            rprintln!();
        }
    }

    // 打印地址格式说明
    rprintln!();
    rprintln!("Tip: 8-bit addr = 7-bit << 1 | R/W bit");

    // 主循环（程序不会退出）
    loop {
        delay.delay_millis(1000);
    }
}
