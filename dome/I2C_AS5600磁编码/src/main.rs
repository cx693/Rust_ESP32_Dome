#![no_std]
#![no_main]
#![allow(non_snake_case, reason = "AS5600 matches chip name")]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::Duration;
use esp_alloc;
use esp_hal::clock::CpuClock;
use esp_hal::i2c::master::Config as I2cConfig;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::{Config as UartConfig, Uart};

mod AS5600;
use AS5600::r#async::AsyncAs5600;
use AS5600::raw_to_degrees;

use {esp_backtrace as _, esp_println as _};
extern crate alloc;
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let p = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);

    let psram = esp_hal::psram::Psram::new(p.PSRAM, Default::default());
    let (_psram_start, psram_size) = psram.raw_parts();
    info!("检测到 PSRAM: {} MB", psram_size / (1024 * 1024));
    // esp_alloc::psram_allocator!(&psram);
    // info!("已经将 PSRAM 加入到全局");

    let timg0 = TimerGroup::new(p.TIMG0);
    let sw_interrupt = esp_hal::interrupt::software::SoftwareInterruptControl::new(p.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);
    embassy_time::Timer::after(Duration::from_millis(500)).await;

    // ---- LiDAR 初始化 ----
    let _uart1 = Uart::new(
        p.UART1,
        UartConfig::default().with_baudrate(230400),
    )
    .unwrap()
    .with_rx(p.GPIO2)
    .with_tx(p.GPIO1);


    let i2c = esp_hal::i2c::master::I2c::new(p.I2C0, I2cConfig::default())
        .unwrap()
        .with_scl(p.GPIO9)
        .with_sda(p.GPIO8)
        .into_async();

    let mut encoder = AsyncAs5600::new(i2c);

    // 等待 AS5600 上电稳定
    embassy_time::Timer::after(Duration::from_millis(5)).await;

    info!("AS5600 初始化完成");

    loop {
        match encoder.angle().await {
            Ok(raw) => {
                let degrees = raw_to_degrees(raw);
                info!("角度: {}° (raw: {})", degrees, raw);
            }
            Err(_e) => {
                info!("AS5600 读取错误");
            }
        }

        // // 检查磁铁状态
        // if let Ok(status) = encoder.magnet_status().await {
        //     match status {
        //         AS5600::MagnetStatus::Detected => {}
        //         AS5600::MagnetStatus::TooWeak => info!("警告: 磁铁信号过弱"),
        //         AS5600::MagnetStatus::TooStrong => info!("警告: 磁铁信号过强"),
        //         AS5600::MagnetStatus::NotDetected => info!("警告: 未检测到磁铁"),
        //     }
        // }

        // embassy_time::Timer::after(Duration::from_millis(100)).await;
    }
}
