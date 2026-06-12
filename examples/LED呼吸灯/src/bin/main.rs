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

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    // 初始化 RTT（Real-Time Transfer）调试输出
    rtt_init_print!();
    // 打印调试信息
    rprintln!("呼吸灯 硬件 PWM");

    // 初始化 ESP32 的外设，使用默认配置
    let peripherals = esp_hal::init(esp_hal::Config::default());
    // 获取 LEDC（LED PWM 控制器）外设实例
    let mut ledc = Ledc::new(peripherals.LEDC);

    // 设置 LEDC 的全局低速时钟源为 APB 总线时钟
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

    // 创建一个低速 LEDC 定时器 0
    let mut lstimer0 = ledc.timer::<LowSpeed>(timer::Number::Timer0);

    // 配置定时器：占空比分辨率 10 位（0～1023），
    // 时钟源为 APB 时钟，PWM 频率为 5 kHz
    lstimer0
        .configure(timer::config::Config {
            duty: timer::config::Duty::Duty10Bit,
            clock_source: timer::LSClockSource::APBClk,
            frequency: Rate::from_khz(5),
        })
        .unwrap();

    // 创建一个 LEDC 通道 0，并绑定到 GPIO48 引脚
    let mut channel0 = ledc.channel(channel::Number::Channel0, peripherals.GPIO48);
    // 配置通道：使用上面配置的定时器，
    // 初始占空比为 0%，输出模式为推挽
    channel0
        .configure(channel::config::Config {
            timer: &lstimer0,
            duty_pct: 0,
            drive_mode: DriveMode::PushPull,
        })
        .unwrap();

    // 无限循环，实现呼吸灯效果
    loop {
        // 启动占空比渐变：从 0% 渐变到 100%，持续时间 1000 毫秒
        channel0.start_duty_fade(0, 100, 1000).unwrap();
        // 等待当前渐变完成（阻塞）
        while channel0.is_duty_fade_running() {}

        // 启动占空比渐变：从 100% 渐变到 0%，持续时间 1000 毫秒
        channel0.start_duty_fade(100, 0, 1000).unwrap();
        // 等待渐变完成
        while channel0.is_duty_fade_running() {}
    }
}
