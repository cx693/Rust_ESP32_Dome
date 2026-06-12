#![no_main]
#![no_std]

use core::cell::RefCell;
use critical_section::Mutex;
use esp_hal::{
    gpio::{Level, Output, OutputConfig},
    handler, main,
    time::Duration,
    timer::{Timer as TimerTrait, timg::TimerGroup},
};
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

// ======================== 静态资源 ========================

// 只需要在中断中访问 LED（翻转）和 Timer（清中断）
static LED: Mutex<RefCell<Option<Output<'static>>>> = Mutex::new(RefCell::new(None));
static TIMER: Mutex<RefCell<Option<esp_hal::timer::timg::Timer<'static>>>> =
    Mutex::new(RefCell::new(None));

esp_bootloader_esp_idf::esp_app_desc!();

// ======================== 中断处理 ========================

#[handler]
fn timer_handler() {
    // 在中断上下文中使用 critical_section 保证与 main 的互斥
    critical_section::with(|cs| {
        // 1. 清除定时器中断标志（必须做，否则会重复触发）
        if let Some(timer) = TIMER.borrow(cs).borrow().as_ref() {
            timer.clear_interrupt();
        }
        // 2. 翻转 LED
        if let Some(led) = LED.borrow(cs).borrow_mut().as_mut() {
            led.toggle();
        }
    });
}

// ======================== 主函数 ========================

#[main]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("定时器中断 - 每1秒翻转LED");

    let peripherals = esp_hal::init(esp_hal::Config::default());

    // --- 外设初始化（不需要 critical_section）---
    let led = Output::new(peripherals.GPIO48, Level::Low, OutputConfig::default());
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let timer = timg0.timer0;

    // --- 配置定时器（在中断启用前配置，不需要关中断）---
    // 注意：这些方法通过 &self 操作寄存器（内部可变性），配置阶段中断尚未启用
    timer.enable_auto_reload(true);
    timer.load_value(Duration::from_secs(1)).unwrap();
    timer.set_interrupt_handler(timer_handler);

    // --- 将资源移入静态变量 + 启用中断（一次性关中断完成）---
    critical_section::with(|cs| {
        LED.borrow(cs).replace(Some(led));
        TIMER.borrow(cs).replace(Some(timer));

        // 在所有资源就绪后再启用中断，避免竞态
        if let Some(timer) = TIMER.borrow(cs).borrow().as_ref() {
            timer.enable_interrupt(true);
            timer.start();
        }
    });

    rprintln!("定时器已启动，进入主循环等待");
    loop {

    }
}