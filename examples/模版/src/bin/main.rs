#![no_main]
#![no_std]

use esp_bootloader_esp_idf;
use esp_hal::{
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    main,
};
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("按键 LED");
    let peripherals = esp_hal::init(esp_hal::Config::default());
    // led 灯的 引脚
    let mut led = Output::new(peripherals.GPIO48, Level::Low, OutputConfig::default());
    // 按键的引脚
    let key = Input::new(
        peripherals.GPIO0,
        InputConfig::default().with_pull(Pull::Up)
    );

    let mut status: bool = false;
    let delay = Delay::new();
    loop {
        if key.is_low() {
            status = !status;
            rprintln!("当前状态是{}", status);
            led.set_level(if status { Level::Low } else { Level::High });
            delay.delay_millis(120);
            while key.is_low() {}
        }
    }
}
