//! LED 状态指示 —— 不同状态不同闪烁频率
//!   AP 模式 → 快闪(100ms)  连接中 → 中闪(250ms)  已连接 → 慢闪(1s)

use core::sync::atomic::Ordering;
use embassy_time::{Duration, Timer};
use esp_hal::gpio::{Level, Output};
use super::state::*;

pub async fn led_task(mut led: Output<'static>) {
    let mut on = false;
    loop {
        let ms = match APP_STATE.load(Ordering::Relaxed) {
            STATE_AP => 100u64,
            STATE_CONNECTING => 250,
            STATE_CONNECTED => 1000,
            _ => 100,
        };
        on = !on;
        led.set_level(if on { Level::Low } else { Level::High }); // ESP32 低电平点亮
        Timer::after(Duration::from_millis(ms)).await;
    }
}
