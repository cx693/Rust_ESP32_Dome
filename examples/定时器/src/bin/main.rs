//! # ESP32-S3 定时器中断示例
//!
//! 功能：每 1 秒触发一次定时器中断，翻转板载 LED（GPIO48）。
//!
//! ## 核心思路
//!
//! 1. 在 main 中初始化 LED 和定时器
//! 2. 将它们放入 `static` 全局变量，使中断处理函数也能访问
//! 3. 中断触发时：清标志 → 翻转 LED
//!
//! ## 为什么需要 `Mutex<RefCell<Option<T>>>` ？
//!
//! ESP32 的中断会打断 main 执行。如果 main 和中断同时操作同一个外设，
//! 可能产生数据竞争。`critical_section::Mutex` 保证同一时刻只有一个能访问。

#![no_main]
#![no_std]

use core::cell::RefCell;
use critical_section::Mutex;
use esp_hal::{
    gpio::{Level, Output, OutputConfig},
    handler, main,
    time::Duration,
    timer::{Timer as _, timg::TimerGroup}, // Timer trait 提供 .start() 等方法
};
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

// ── 全局静态资源 ──────────────────────────────────────────
// 用 Mutex<RefCell<Option<T>>> 包裹，以便在中断和 main 之间安全共享。
// Option：初始为 None，main 初始化后再 replace 为 Some(外设)。
static LED: Mutex<RefCell<Option<Output<'static>>>> = Mutex::new(RefCell::new(None));
static TIMER: Mutex<RefCell<Option<esp_hal::timer::timg::Timer<'static>>>> =
    Mutex::new(RefCell::new(None));

// IDF bootloader 要求的应用描述宏（固定写法）
esp_bootloader_esp_idf::esp_app_desc!();

// ── 中断处理函数 ──────────────────────────────────────────
// 标记为 #[handler]，esp-hal 会自动将其注册到中断向量表。
#[handler]
fn timer_handler() {
    // critical_section 临时关中断，保证下面的操作不会被嵌套打断
    critical_section::with(|cs| {
        // ① 清除中断标志（必须！否则中断会一直重复触发）
        if let Some(timer) = TIMER.borrow(cs).borrow().as_ref() {
            timer.clear_interrupt();
        }
        // ② 翻转 LED 电平
        if let Some(led) = LED.borrow(cs).borrow_mut().as_mut() {
            led.toggle();
        }
    });
}

// ── 主函数 ────────────────────────────────────────────────
// #[main] 宏负责完成芯片初始化栈，然后调用此函数。
// 返回 `!` 表示永不返回（裸机程序没有操作系统可以退出）。
#[main]
fn main() -> ! {
    // RTT 调试输出初始化（替代串口 println）
    rtt_init_print!();
    rprintln!("定时器中断示例 - 每1秒翻转LED");

    // 初始化 esp-hal（时钟、GPIO 复用等底层配置）
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // ── 第一步：初始化外设 ──
    // 这些操作发生在中断启用之前，所以不需要互斥保护。
    let led = Output::new(peripherals.GPIO48, Level::Low, OutputConfig::default());

    let timg0 = TimerGroup::new(peripherals.TIMG0); // TimerGroup 0 包含多个定时器
    let timer = timg0.timer0;                        // 取其中的 timer0

    // ── 第二步：配置定时器参数 ──
    // 自动重载：计时到期后自动重新开始（否则只触发一次）
    timer.enable_auto_reload(true);
    // 装载值：1 秒（定时器从该值递减到 0 时触发中断）
    timer.load_value(Duration::from_secs(1)).unwrap();
    // 绑定中断处理函数
    timer.set_interrupt_handler(timer_handler);

    // ── 第三步：将外设移入全局变量，然后启动定时器 ──
    // 整个 block 在 critical_section 内完成，避免「启动了中断但资源还没就绪」的竞态。
    critical_section::with(|cs| {
        LED.borrow(cs).replace(Some(led));
        TIMER.borrow(cs).replace(Some(timer));

        // 现在资源已就绪，安全地开启中断并启动定时器
        if let Some(timer) = TIMER.borrow(cs).borrow().as_ref() {
            timer.enable_interrupt(true);
            timer.start();
        }
    });

    rprintln!("定时器已启动，LED 每秒翻转一次");

    // 主循环：在裸机中 main 不能返回，CPU 在此空转等待中断
    loop {}
}
