#![no_main]
#![no_std]

use esp_bootloader_esp_idf;
use esp_hal::{main, time::Instant};
use rtt_target::{rprintln, rtt_init_print};
use panic_rtt_target as _;

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("你好");
    let _peripherals = esp_hal::init(esp_hal::Config::default());
    let mut i: u32 = 0;
    loop {
        rprintln!("你好{}", i);
        i += 1;
        let now = Instant::now();
        while now.elapsed().as_millis() < 500 {}
    }
}
