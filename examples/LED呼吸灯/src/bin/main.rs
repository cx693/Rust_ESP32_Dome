//! ESP32-S3 LED 呼吸灯示例
//!
//! 原理：利用 LEDC（LED PWM 控制器）硬件实现 PWM 输出，
//!       通过渐变占空比（duty）模拟呼吸效果。
//!
//! 硬件连接：GPIO48 → LED（开发板内置，通常低电平点亮）

#![no_main]
#![no_std]

use esp_bootloader_esp_idf;
use esp_hal::{
    gpio::DriveMode,
    ledc::{
        LSGlobalClkSource, Ledc, LowSpeed,
        channel::{self, ChannelIFace},
        timer::{self, TimerIFace},
    },
    main,
    time::Rate,
};
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

// IDF 引导加载程序要求的应用描述符宏
esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    // -------- 调试初始化 --------
    rtt_init_print!();
    rprintln!("呼吸灯 硬件 PWM");

    // -------- 外设初始化 --------
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // -------- LEDC 控制器配置 --------
    // LEDC = LED Control，ESP32 内置的 PWM 控制器
    let mut ledc = Ledc::new(peripherals.LEDC);
    // 低速通道使用 APB 时钟（80MHz）
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

    // -------- 定时器配置 --------
    // 定时器决定 PWM 的频率和分辨率
    let mut lstimer0 = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    lstimer0
        .configure(timer::config::Config {
            duty: timer::config::Duty::Duty10Bit, // 10 位分辨率 → 占空比范围 0~1023
            clock_source: timer::LSClockSource::APBClk,
            frequency: Rate::from_khz(5),         // PWM 频率 5kHz
        })
        .unwrap();

    // -------- 通道配置 --------
    // 通道将定时器的 PWM 信号输出到指定 GPIO
    let mut channel0 = ledc.channel(channel::Number::Channel0, peripherals.GPIO48);
    channel0
        .configure(channel::config::Config {
            timer: &lstimer0,       // 绑定上面的定时器
            duty_pct: 0,            // 初始占空比 0%
            drive_mode: DriveMode::PushPull, // 推挽输出
        })
        .unwrap();

    // -------- 呼吸灯主循环 --------
    // 渐变占空比：0% → 100% → 0%，周而复始
    loop {
        // 渐亮：0% → 100%，耗时 1000ms
        channel0.start_duty_fade(0, 100, 1000).unwrap();
        while channel0.is_duty_fade_running() {} // 阻塞等待渐变完成

        // 渐暗：100% → 0%，耗时 1000ms
        channel0.start_duty_fade(100, 0, 1000).unwrap();
        while channel0.is_duty_fade_running() {}
    }
}
