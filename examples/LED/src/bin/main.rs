#![no_main]
#![no_std]

use esp_bootloader_esp_idf;
use esp_hal::{
    gpio::{Level, Output, OutputConfig},
    main,
    time::Instant,
};
use rtt_target::{rprintln, rtt_init_print};
use panic_rtt_target as _;

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("LED 闪烁");
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let mut led = Output::new(peripherals.GPIO48,Level::Low,OutputConfig::default());
    let mut i: u32 = 0;
    loop {
        let on = i % 2 == 0;
        led.set_level(if on { Level::Low } else { Level::High });
        rprintln!("当前状态: {}", if on { "亮" } else { "灭" });
        i += 1;
        let now = Instant::now();
        while now.elapsed().as_millis() < 5000 {}
    }
}
