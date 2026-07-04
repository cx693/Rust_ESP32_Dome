//! ST7789 SPI LCD — GIF 动画播放 (月薪猫)
//!
//! 硬件 (ESP32-S3):
//!   GPIO48→SCK  GPIO47→MOSI  GPIO2→DC  GPIO1→RES  GPIO0→BLK

#![no_main]
#![no_std]

use core::cell::UnsafeCell;
use esp_bootloader_esp_idf;
use esp_hal::{
    delay::Delay,
    dma::{DmaRxBuf, DmaTxBuf},
    dma_buffers,
    gpio::{Level, Output, OutputConfig},
    main,
    spi::{Mode, master::{Config as SpiConfig, Spi}},
    time::Rate,
};
use hello::gif::GifPlayer;
use hello::st7789::{COLOR_RGB565_BLACK, FB_SIZE, St7789, fb_clear};
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

esp_bootloader_esp_idf::esp_app_desc!();

struct FrameBuffer(UnsafeCell<[u8; FB_SIZE]>);
unsafe impl Sync for FrameBuffer {}

static FB: FrameBuffer = FrameBuffer(UnsafeCell::new([0u8; FB_SIZE]));

#[main]
fn main() -> ! {
    esp_alloc::heap_allocator!(size: 96 * 1024);

    rtt_init_print!();
    rprintln!("GIF Player (DMA)");

    let peripherals = esp_hal::init(esp_hal::Config::default());

    let dc = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());
    let res = Output::new(peripherals.GPIO1, Level::High, OutputConfig::default());
    let blk = Output::new(peripherals.GPIO0, Level::High, OutputConfig::default());

    let (rx_buf, rx_desc, tx_buf, tx_desc) = dma_buffers!(32768);
    let spi = Spi::new(
        peripherals.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_khz(80000))
            .with_mode(Mode::_3),
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
    fb_clear(fb, COLOR_RGB565_BLACK);

    let delay = Delay::new();

    let gif_data = include_bytes!("../../img/J1.gif");
    let mut player = GifPlayer::new(gif_data);
    rprintln!("GIF has {} frames", player.frame_count());

    loop {
        player.reset();
        loop {
            match player.decode_next_frame(fb) {
                Some(delay_ms) => {
                    display.flush(fb);
                    if delay_ms > 0 {
                        delay.delay_millis(delay_ms as u32);
                    }
                }
                None => break,
            }
        }
    }
}
