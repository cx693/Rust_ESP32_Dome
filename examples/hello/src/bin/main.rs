// ESP32-S3 Hello World —— 每隔 500ms 打印一次计数

// 嵌入式程序没有操作系统，不用标准库和默认 main
#![no_main]
#![no_std]

// esp_hal:     ESP32 硬件抽象层
// rtt_target:  RTT 调试输出（类似 println!）
// panic_rtt_target: panic 时通过 RTT 输出错误信息
use esp_bootloader_esp_idf;
use esp_hal::{main, time::Instant};
use rtt_target::{rprintln, rtt_init_print};
use panic_rtt_target as _;

// ESP-IDF 引导加载程序要求的应用描述，必须有
esp_bootloader_esp_idf::esp_app_desc!();

// #[main] 标记入口，`-> !` 表示永不返回（嵌入式程序永远在跑）
#[main]
fn main() -> ! {
    rtt_init_print!(); // 初始化 RTT，之后才能用 rprintln!

    // 初始化硬件，peripherals 包含所有外设控制权
    // 下划线前缀 _ 表示"暂时不用这个变量"
    let _peripherals = esp_hal::init(esp_hal::Config::default());

    let mut count: u32 = 0;

    // 嵌入式程序必须有无限循环
    loop {
        rprintln!("你好，第 {} 次", count);
        count += 1;

        // 忙等待延时 500ms（简单但占 CPU，实际项目建议用定时器）
        let start = Instant::now();
        while start.elapsed().as_millis() < 500 {}
    }
}
