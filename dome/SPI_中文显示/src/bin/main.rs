//! ST7789 SPI LCD — 中文字体显示示例
//!
//! 硬件 (ESP32-S3):
//!   GPIO48→SCK  GPIO47→MOSI  GPIO2→DC  GPIO1→RES  GPIO0→BLK

#![no_main]
#![no_std]

use core::cell::UnsafeCell;
use esp_bootloader_esp_idf;
use esp_hal::{
    dma::{DmaRxBuf, DmaTxBuf},
    dma_buffers,
    gpio::{Level, Output, OutputConfig},
    main,
    spi::{Mode, master::{Config as SpiConfig, Spi}},
    time::Rate,
};
use hello::st7789::{St7789, FB_SIZE, fb_clear};
use hello::font::{FontFamily, draw_str};
use esp_alloc as _;
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

esp_bootloader_esp_idf::esp_app_desc!();

struct FrameBuffer(UnsafeCell<[u8; FB_SIZE]>);
unsafe impl Sync for FrameBuffer {}
static FB: FrameBuffer = FrameBuffer(UnsafeCell::new([0u8; FB_SIZE]));

const BLACK: u16 = 0x0000;
const GREEN: u16 = 0x07E0;
const YELLOW: u16 = 0xFFE0;
const WHITE: u16 = 0xFFFF;
const CYAN: u16 = 0x07FF;
const RED: u16 = 0xF800;

const W: u16 = 240;
const H: u16 = 240;

#[main]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("Font Demo - any char, any size (DMA)");

    let peripherals = esp_hal::init(esp_hal::Config::default());

    let dc = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());
    let res = Output::new(peripherals.GPIO1, Level::High, OutputConfig::default());
    let blk = Output::new(peripherals.GPIO0, Level::High, OutputConfig::default());

    let (rx_buf, rx_desc, tx_buf, tx_desc) = dma_buffers!(32000);
    let spi = Spi::new(
        peripherals.SPI2,
        SpiConfig::default().with_frequency(Rate::from_khz(80000)).with_mode(Mode::_3),
    )
    .unwrap()
    .with_sck(peripherals.GPIO48)
    .with_mosi(peripherals.GPIO47)
    .with_dma(peripherals.DMA_CH0)
    .with_buffers(DmaRxBuf::new(rx_desc, rx_buf).unwrap(), DmaTxBuf::new(tx_desc, tx_buf).unwrap());

    let mut display = St7789::new(spi, dc, res).with_blk(blk);
    display.init();
    rprintln!("Display ready (DMA)");

    let fb = unsafe { &mut *FB.0.get() };
    fb_clear(fb, BLACK);

    let mut y = 4i16;

    draw_str(fb, 0, y, "Hello world", FontFamily::Cxi, 16, GREEN, BLACK, W, H);
    y += 20;
    draw_str(fb, 0, y, "你好世界", FontFamily::Cxi, 20, YELLOW, BLACK, W, H);
    y += 26;

    draw_str(fb, 0, y, "Hello world", FontFamily::AliMaMa, 24, WHITE, BLACK, W, H);
    y += 30;
    draw_str(fb, 0, y, "你好世界!", FontFamily::AliMaMa, 24, CYAN, BLACK, W, H);
    y += 30;

    draw_str(fb, 0, y, "ABC abc 123", FontFamily::Cxi, 20, RED, BLACK, W, H);
    y += 26;
    draw_str(fb, 0, y, "我叫陈晨", FontFamily::AliMaMa, 28, GREEN, BLACK, W, H);
    y += 34;
    draw_str(fb, 0, y, "我喜欢玩 Play", FontFamily::AliMaMa, 32, WHITE, BLACK, W, H);

    display.flush(fb);
    rprintln!("Done");

    loop {}
}
