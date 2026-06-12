#![no_main]
#![no_std]

use esp_bootloader_esp_idf;
use esp_hal::{
    DriverMode,
    delay::Delay,
    gpio::{DriveMode, Input, InputConfig, Level, Output, OutputConfig, Pull},
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
    rtt_init_print!();
    rprintln!("PWM 3Color LED");

    let peripherals = esp_hal::init(esp_hal::Config::default());
    let mut ledc = Ledc::new(peripherals.LEDC);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

    let mut lstimer0 = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    lstimer0
        .configure(timer::config::Config {
            duty: timer::config::Duty::Duty10Bit,
            clock_source: timer::LSClockSource::APBClk,
            frequency: Rate::from_khz(5),
        })
        .unwrap();

    let mut ch_r = ledc.channel(channel::Number::Channel0, peripherals.GPIO4);
    ch_r.configure(channel::config::Config {
        timer: &lstimer0,
        duty_pct: 0,
        drive_mode: DriveMode::PushPull,
    })
    .unwrap();

    let mut ch_g = ledc.channel(channel::Number::Channel1, peripherals.GPIO5);
    ch_g.configure(channel::config::Config {
        timer: &lstimer0,
        duty_pct: 0,
        drive_mode: DriveMode::PushPull,
    })
    .unwrap();

    let mut ch_b = ledc.channel(channel::Number::Channel2, peripherals.GPIO6);
    ch_b.configure(channel::config::Config {
        timer: &lstimer0,
        duty_pct: 0,
        drive_mode: DriveMode::PushPull,
    })
    .unwrap();

    let colors: [(u8, u8, u8); 7] = [
        (100, 0, 0),
        (0, 100, 0),
        (0, 0, 100),
        (100, 100, 0),
        (0, 100, 100),
        (100, 0, 100),
        (100, 100, 100),
    ];

    loop {
        for &(r, g, b) in &colors {
            rprintln!("R={} G={} B={}", r, g, b);
            ch_r.set_duty(r).unwrap();
            ch_g.set_duty(g).unwrap();
            ch_b.set_duty(b).unwrap();

            esp_hal::delay::Delay::new().delay_millis(1000);
        }
    }
}
