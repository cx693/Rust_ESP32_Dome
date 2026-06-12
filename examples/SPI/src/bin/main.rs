#![no_main]
#![no_std]

use esp_bootloader_esp_idf;
use esp_hal::{
    gpio::{Level, Output, OutputConfig},
    main,
    spi::{
        Mode,
        master::{Config as SpiConfig, Spi},
    },
    time::Rate,
};
use hello::st7789::{
    COLOR_RGB565_BLUE, COLOR_RGB565_GREEN, COLOR_RGB565_PURPLE, COLOR_RGB565_RED,
    COLOR_RGB565_YELLOW, St7789,
};
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("ST7789 SPI 屏幕驱动");

    let peripherals = esp_hal::init(esp_hal::Config::default());

    let dc = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());
    let res = Output::new(peripherals.GPIO1, Level::High, OutputConfig::default());
    let blk = Output::new(peripherals.GPIO0, Level::High, OutputConfig::default());

    let spi = Spi::new(
        peripherals.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_khz(80000))
            .with_mode(Mode::_3),
    )
    .unwrap()
    .with_sck(peripherals.GPIO48)
    .with_mosi(peripherals.GPIO47);

    let mut display = St7789::new(spi, dc, res)
        .with_blk(blk);
    display.init();
    rprintln!("初始化完成，开始刷屏");

    let colors: [(&str, u16); 5] = [
        ("红色", COLOR_RGB565_RED),
        ("蓝色", COLOR_RGB565_BLUE),
        ("绿色", COLOR_RGB565_GREEN),
        ("黄色", COLOR_RGB565_YELLOW),
        ("紫色", COLOR_RGB565_PURPLE),
    ];

    loop {
        for (name, color) in &colors {
            rprintln!("显示: {}", name);
            display.fill_screen(*color);
            display.delay.delay_millis(1000);
        }
    }
}
