//! ESP32-S3 ADC 模拟电压采集示例
//!
//! 功能：通过 GPIO1 读取模拟电压，经 ADC 转换后输出毫伏值。
//! 硬件：将待测电压（0~3.1V）接到 GPIO1 引脚即可。
//!
//! 关键概念：
//! - ADC（模数转换器）：将连续的模拟电压信号转为离散的数字值。
//! - 校准（Calibration）：消除芯片个体差异，提高测量精度。
//! - 衰减（Attenuation）：扩大 ADC 可测量的输入电压范围。
//!     _0dB   → 0~1.1V   （精度最高）
//!     _2_5dB → 0~1.5V
//!     _6dB   → 0~2.2V
//!     _11dB  → 0~3.1V   （范围最广，本例使用）

#![no_main]
#![no_std]

use esp_bootloader_esp_idf;
use esp_hal::{
    analog::adc::{Adc, AdcCalCurve, AdcConfig, Attenuation},
    delay::Delay,
    main,
};
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

// 声明 ESP-IDF 应用描述符（启动引导所需）
esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    // 初始化 RTT 打印通道（用于向电脑输出调试信息）
    rtt_init_print!();
    rprintln!("ESP32-S3 ADC 校准采集示例");

    // 初始化外设（获取芯片所有外设的控制权）和延时器
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let delay = Delay::new();

    // ── 配置 ADC1 通道 ──────────────────────────────
    let mut adc1_config = AdcConfig::new();

    // 启用 GPIO1 的 ADC 功能，使用曲线校准，衰减设为 11dB（量程 0~3.1V）
    let mut pin_gpio1 =
        adc1_config.enable_pin_with_cal::<_, AdcCalCurve<_>>(peripherals.GPIO1, Attenuation::_11dB);

    // 用配置好的参数创建 ADC1 实例
    let mut adc1 = Adc::new(peripherals.ADC1, adc1_config);

    // ── 循环采集 ─────────────────────────────────────
    loop {
        // 阻塞式读取：等待转换完成并返回校准后的毫伏值
        let voltage_mv = adc1.read_blocking(&mut pin_gpio1);

        // 同时输出毫伏和伏特两种单位，方便对照
        rprintln!(
            "GPIO1: {} mV ({:.3} V)",
            voltage_mv,
            voltage_mv as f32 / 1000.0
        );

        // 每 100ms 采样一次
        delay.delay_millis(100);
    }
}