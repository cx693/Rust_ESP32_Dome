// ============================================================
// ESP32-S3 LED 闪烁示例
// 功能: 每隔 500ms 切换一次板载 LED（GPIO48）的亮/灭状态
// 硬件: ESP32-S3 开发板，板载 LED 为低电平点亮（active-low）
// ============================================================

#![no_main] // 告诉编译器不使用标准 main 入口，由 esp_hal 提供
#![no_std]  // 裸机环境不使用标准库（std），只用核心库（core）

use esp_bootloader_esp_idf;
use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main,
};
use rtt_target::{rprintln, rtt_init_print};
use panic_rtt_target as _;

// 注册 ESP-IDF bootloader 的应用描述符（固件元信息，必须调用）
esp_bootloader_esp_idf::esp_app_desc!();

/// 程序入口
/// `#[main]` 由 esp_hal 提供，替代标准库的 main，完成芯片初始化后调用此函数
/// 返回类型 `-> !` 表示永不返回（嵌入式程序通常是一个无限循环）
#[main]
fn main() -> ! {
    // 初始化 RTT 调试输出（通过 J-Link/ESP-Prog 等调试器查看）
    rtt_init_print!();
    rprintln!("LED 闪烁示例启动");

    // 初始化 HAL（硬件抽象层），获取所有外设的控制权
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // 配置 GPIO48 为推挽输出，初始电平为 Low
    // ESP32-S3 许多开发板的板载 LED 是 active-low（低电平点亮）
    // 所以 Level::Low = LED 亮，Level::High = LED 灭
    let mut led = Output::new(peripherals.GPIO48, Level::Low, OutputConfig::default());

    // 创建延时器（基于硬件定时器，非忙等，CPU 可以低功耗等待）
    let delay = Delay::new();

    loop {
        // toggle() 翻转引脚电平：High → Low，Low → High
        // 比手动 set_level(if ...) 更简洁直观
        led.toggle();

        // 读取当前引脚状态并打印
        // is_set_high() 返回 true 表示引脚输出高电平
        // 由于 active-low：高电平 = LED 灭，低电平 = LED 亮
        if led.is_set_high() {
            rprintln!("LED 灭");
        } else {
            rprintln!("LED 亮");
        }

        // 延时 500 毫秒（非忙等，比 Instant::now() 的忙等循环更高效）
        delay.delay_millis(500);
    }
}
