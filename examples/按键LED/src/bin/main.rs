//! ============================================================
//! 按键控制 LED 示例
//! 功能：每按一次按键，LED 翻转一次（亮 → 灭 → 亮 → ...）
//! 硬件：ESP32-S3，GPIO0 接按键，GPIO48 接 LED
//! ============================================================

// ── 必须的 no_std / no_main 声明 ─────────────────────────────
// 嵌入式环境没有操作系统，所以需要禁用标准库和默认入口点
#![no_main]
#![no_std]

// ── 导入依赖 ─────────────────────────────────────────────────
// esp_bootloader_esp_idf: ESP-IDF 引导加载程序支持
use esp_bootloader_esp_idf;

// esp_hal: ESP32 硬件抽象层（HAL），提供 GPIO、延时等底层操作
use esp_hal::{
    delay::Delay,                                    // 软件延时器
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull}, // GPIO 输入/输出
    main,                                            // 入口宏
};

// panic_rtt_target: 发生 panic 时通过 RTT 输出错误信息
use panic_rtt_target as _;

// rtt_target: RTT（Real-Time Transfer）日志输出，用于替代串口打印
use rtt_target::{rprintln, rtt_init_print};

// ── 应用描述符（ESP-IDF 引导加载程序要求） ───────────────────
esp_bootloader_esp_idf::esp_app_desc!();

// ── 程序入口 ─────────────────────────────────────────────────
// #[main] 宏标记程序入口函数，替代标准的 fn main()
// 返回类型 `-> !` 表示函数永远不会返回（嵌入式程序是死循环）
#[main]
fn main() -> ! {
    // 初始化 RTT 日志，之后就可以用 rprintln! 打印信息了
    rtt_init_print!();
    rprintln!("=== 按键控制 LED 启动 ===");

    // ── 初始化外设 ───────────────────────────────────────────
    // esp_hal::init() 初始化芯片的所有外设，返回一个 peripherals 对象
    // 通过 peripherals.GPIOxx 来访问具体的引脚
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // ── 配置 LED 引脚（GPIO48）为输出模式 ────────────────────
    // 参数说明：
    //   peripherals.GPIO48 — 使用 GPIO48 引脚
    //   Level::Low         — 初始电平为低（LED 默认关闭）
    //   OutputConfig::default() — 使用默认输出配置
    let mut led = Output::new(peripherals.GPIO48, Level::Low, OutputConfig::default());

    // ── 配置按键引脚（GPIO0）为输入模式 ──────────────────────
    // 参数说明：
    //   peripherals.GPIO0 — 使用 GPIO0 引脚（ESP32-S3 的 BOOT 按键）
    //   InputConfig::default().with_pull(Pull::Up) — 启用内部上拉电阻
    //     上拉电阻的作用：按键未按下时，引脚保持高电平（is_high）
    //     按键按下时，引脚被拉低到 GND，变为低电平（is_low）
    let key = Input::new(peripherals.GPIO0, InputConfig::default().with_pull(Pull::Up));

    // ── LED 状态变量 ─────────────────────────────────────────
    // false = LED 关闭，true = LED 打开
    let mut led_on: bool = false;

    // ── 延时器 ───────────────────────────────────────────────
    // 用于按键消抖（按下后短暂等待，避免机械抖动导致多次触发）
    let delay = Delay::new();

    // ── 主循环 ───────────────────────────────────────────────
    // 嵌入式程序的核心：不断轮询按键状态
    loop {
        // 检测按键是否被按下（低电平 = 按下）
        if key.is_low() {
            // 1. 翻转 LED 状态
            led_on = !led_on;
            rprintln!("按键按下 → LED {}", if led_on { "开" } else { "关" });

            // 2. 根据状态设置 LED 电平
            //    led_on = true  → 输出 Low（低电平点亮，因为 LED 接法是低电平有效）
            //    led_on = false → 输出 High（高电平熄灭）
            led.set_level(if led_on { Level::Low } else { Level::High });

            // 3. 消抖延时（120ms）
            //    机械按键按下瞬间会产生电平抖动，等待一段时间后再检测
            delay.delay_millis(120);

            // 4. 等待按键松开
            //    松开前不退出，防止按住不放时连续触发
            while key.is_low() {}
        }
    }
}
