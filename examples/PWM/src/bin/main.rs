// ============================================================
// ESP32-S3 PWM 三色 LED 控制示例
// 功能：7 色循环显示 + 呼吸灯效果
// 使用 LEDC (LED Control) 外设输出 PWM 信号，驱动 RGB LED
// 硬件连接: R→GPIO4, G→GPIO5, B→GPIO6
// ============================================================

#![no_std] // 嵌入式环境，不使用标准库
#![no_main] // 没有 main 入口，由 esp_hal 接管启动流程

// --- 依赖导入 ---
use esp_bootloader_esp_idf;
use esp_hal::{
    delay::Delay,
    gpio::DriveMode,
    ledc::{
        LSGlobalClkSource, Ledc, LowSpeed,
        channel::{self, ChannelIFace},
        timer::{self, TimerIFace},
    },
    main,
    time::Rate,
};
use panic_rtt_target as _; // panic 时通过 RTT 输出信息
use rtt_target::{rprintln, rtt_init_print}; // RTT 打印宏

// 声明 ESP-IDF bootloader 应用描述符（启动所需）
esp_bootloader_esp_idf::esp_app_desc!();

/// PWM 占空比百分比上限（duty_pct 范围 0~100）
const DUTY_MAX: u8 = 100;

/// 每种纯色的显示时长（毫秒）
const COLOR_HOLD_MS: u32 = 1000;

/// 呼吸灯：渐亮/渐暗的步数（越大越平滑，但耗时越长）
const BREATH_STEPS: u8 = 50;

/// 呼吸灯：每一步的间隔（毫秒），控制呼吸速度
const BREATH_STEP_MS: u32 = 20;

/// 要循环显示的颜色列表 (R%, G%, B%)
/// 每个分量范围 0~100，对应占空比百分比
const COLORS: [(u8, u8, u8); 7] = [
    (DUTY_MAX, 0, 0),                 // 红
    (0, DUTY_MAX, 0),                 // 绿
    (0, 0, DUTY_MAX),                 // 蓝
    (DUTY_MAX, DUTY_MAX, 0),          // 黄 (红+绿)
    (0, DUTY_MAX, DUTY_MAX),          // 青 (绿+蓝)
    (DUTY_MAX, 0, DUTY_MAX),          // 品红 (红+蓝)
    (DUTY_MAX, DUTY_MAX, DUTY_MAX),   // 白 (红+绿+蓝)
];

/// 呼吸灯效果：对当前颜色做一次完整的 "渐亮 → 渐暗" 循环
///
/// 原理：用正弦曲线模拟自然的呼吸节奏
///   - sin(0) = 0.0 → 最暗
///   - sin(π/2) = 1.0 → 最亮
///   - sin(π) = 0.0 → 回到最暗
///
/// `step` 从 0 到 BREATH_STEPS，映射到 0..π 的弧度
/// 输出亮度 = sin(step/STEPS × π) × DUTY_MAX
fn breathe(
    ch_r: &mut channel::Channel<'_, LowSpeed>,
    ch_g: &mut channel::Channel<'_, LowSpeed>,
    ch_b: &mut channel::Channel<'_, LowSpeed>,
    color: (u8, u8, u8),
    delay: &Delay,
) {
    let (r_base, g_base, b_base) = color;

    for step in 0..=BREATH_STEPS {
        // 计算 sin 值：将 step 映射到 0.0 ~ π 的弧度
        // 用整数近似 sin：用查找表或泰勒展开太复杂，
        // 这里用一个简单的"抛物线呼吸曲线"替代：
        //   brightness = 4 × t × (1 - t)    其中 t = step / STEPS
        // 这条曲线在 t=0 时为 0，t=0.5 时为 1，t=1 时为 0，形似正弦半波
        let t = step as u32;
        let s = BREATH_STEPS as u32;
        // 4 * t * (s - t) / s / s  → 范围 0 ~ 1（整数运算，避免浮点）
        let brightness = (4 * t * (s - t)) / s; // 0 ~ s
        // 映射到占空比百分比：brightness 范围是 0~s，需要映射到 0~DUTY_MAX
        let pct = ((brightness * DUTY_MAX as u32) / s) as u8;

        // 按比例缩放各颜色分量
        let r = (r_base as u32 * pct as u32 / DUTY_MAX as u32) as u8;
        let g = (g_base as u32 * pct as u32 / DUTY_MAX as u32) as u8;
        let b = (b_base as u32 * pct as u32 / DUTY_MAX as u32) as u8;

        ch_r.set_duty(r).unwrap();
        ch_g.set_duty(g).unwrap();
        ch_b.set_duty(b).unwrap();

        delay.delay_millis(BREATH_STEP_MS);
    }
}

/// 程序入口
/// 返回类型 `!` 表示永不返回（嵌入式系统永远运行）
#[main]
fn main() -> ! {
    // 初始化 RTT 调试输出（通过 probe-rs 在终端查看）
    rtt_init_print!();
    rprintln!("=== ESP32-S3 PWM RGB LED Demo ===");

    // --- 第 1 步：初始化 HAL（获取外设句柄） ---
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // --- 第 2 步：初始化 LEDC 外设 ---
    // LEDC 是 ESP32 的 LED 控制器，可生成多路独立的 PWM 信号
    let mut ledc = Ledc::new(peripherals.LEDC);
    // 使用 APB 总线时钟 (80MHz) 作为低速定时器的时钟源
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

    // --- 第 3 步：配置 PWM 定时器 ---
    // 定时器决定 PWM 的频率和分辨率
    let mut timer0 = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    timer0
        .configure(timer::config::Config {
            duty: timer::config::Duty::Duty10Bit, // 10 位分辨率 (0~1023)
            clock_source: timer::LSClockSource::APBClk,
            frequency: Rate::from_khz(5), // PWM 频率 5kHz（人眼不可察觉闪烁）
        })
        .unwrap();

    // --- 第 4 步：配置 3 个 PWM 通道，分别驱动 R/G/B ---
    // Channel0 → GPIO4 (红)
    let mut ch_r = ledc.channel(channel::Number::Channel0, peripherals.GPIO4);
    ch_r.configure(channel::config::Config {
        timer: &timer0,
        duty_pct: 0,                    // 初始占空比 0%（灭）
        drive_mode: DriveMode::PushPull, // 推挽输出
    })
    .unwrap();

    // Channel1 → GPIO5 (绿)
    let mut ch_g = ledc.channel(channel::Number::Channel1, peripherals.GPIO5);
    ch_g.configure(channel::config::Config {
        timer: &timer0,
        duty_pct: 0,
        drive_mode: DriveMode::PushPull,
    })
    .unwrap();

    // Channel2 → GPIO6 (蓝)
    let mut ch_b = ledc.channel(channel::Number::Channel2, peripherals.GPIO6);
    ch_b.configure(channel::config::Config {
        timer: &timer0,
        duty_pct: 0,
        drive_mode: DriveMode::PushPull,
    })
    .unwrap();

    // 创建 Delay 实例（在循环外创建，避免重复初始化）
    let delay = Delay::new();

    // --- 第 5 步：循环显示颜色 ---
    rprintln!("开始循环：纯色显示 + 呼吸灯效果");
    loop {
        for &(r, g, b) in &COLORS {
            // --- 阶段 A：纯色常亮 ---
            rprintln!("  [纯色] R={:>3}%  G={:>3}%  B={:>3}%", r, g, b);
            ch_r.set_duty(r).unwrap();
            ch_g.set_duty(g).unwrap();
            ch_b.set_duty(b).unwrap();
            delay.delay_millis(COLOR_HOLD_MS);

            // --- 阶段 B：呼吸灯 ---
            // 当前颜色做一次 "渐亮 → 渐暗" 循环
            rprintln!("  [呼吸] R={:>3}%  G={:>3}%  B={:>3}%", r, g, b);
            breathe(&mut ch_r, &mut ch_g, &mut ch_b, (r, g, b), &delay);
        }
    }
}
