//! 外部中断示例 —— 按下 BOOT 按键(GPIO0)，切换板载 LED(GPIO48) 亮灭
//!
//! 原理：配置 GPIO0 下降沿触发中断 → 中断处理函数中翻转 LED 电平

#![no_main]
#![no_std]

use core::cell::RefCell;
use critical_section::Mutex;
use esp_bootloader_esp_idf;
use esp_hal::{
    gpio::{Event, Input, InputConfig, Io, Level, Output, OutputConfig, Pull},
    handler, main, ram,
};
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

// 必须：声明 IDF 应用描述符，bootloader 据此加载固件
esp_bootloader_esp_idf::esp_app_desc!();

// Mutex + RefCell：跨线程/中断安全地共享外所有权
// Option：外设初始化后才放入，初始为 None
static BUTTON: Mutex<RefCell<Option<Input>>> = Mutex::new(RefCell::new(None));
static LED: Mutex<RefCell<Option<Output>>> = Mutex::new(RefCell::new(None));

#[main]
fn main() -> ! {
    // 初始化 RTT 日志通道（芯片复位后需重新初始化）
    rtt_init_print!();
    rprintln!("外部中断 -- 点灯");

    // 初始化 HAL，获取所有外设句柄
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // IO_MUX 负责 GPIO 引脚复用配置，同时管理 GPIO 中断
    let mut io = Io::new(peripherals.IO_MUX);
    io.set_interrupt_handler(gpio_isr); // 注册中断处理函数

    // 板载 LED：GPIO48，默认低电平（灭）
    let led = Output::new(peripherals.GPIO48, Level::Low, OutputConfig::default());

    // BOOT 按键：GPIO0，内部上拉（空闲为高电平）
    let config = InputConfig::default().with_pull(Pull::Up);
    let mut button = Input::new(peripherals.GPIO0, config);

    // 临界区：配置中断监听 + 将外设移入全局静态变量
    // 中断处理函数需要通过这些全局变量访问外设
    critical_section::with(|cs| {
        button.listen(Event::FallingEdge); // 检测下降沿（按键按下）
        BUTTON.borrow_ref_mut(cs).replace(button);
        LED.borrow_ref_mut(cs).replace(led);
    });

    // 主循环空转，所有逻辑由中断驱动
    loop {}
}

/// GPIO 中断服务程序（ISR）
///
/// `#[handler]` — 标记为中断处理函数
/// `#[ram]` — 代码放入 RAM 执行，避免 Flash 访问延迟
///
/// 注意：ISR 中应尽快完成工作并返回，避免长时间阻塞
#[handler]
#[ram]
fn gpio_isr() {
    critical_section::with(|cs| {
        // 拆成两行：RefMut 临时值必须先绑定到变量，否则会在语句末被释放
        let mut btn_ref = BUTTON.borrow_ref_mut(cs);
        let btn = btn_ref.as_mut().unwrap();

        // 仅处理按键触发的中断（同一向量可能有多个 GPIO 源）
        if btn.is_interrupt_set() {
            rprintln!("按键中断触发！");
            LED.borrow_ref_mut(cs).as_mut().unwrap().toggle();
            btn.clear_interrupt(); // 必须手动清除中断标志，否则会反复触发
        }
    });
}
