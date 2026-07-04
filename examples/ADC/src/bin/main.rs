// ESP32-S3 内部温度传感器 — 直接操作 SENS 寄存器读取芯片温度
// 参考文档: docs/temperature_sensor.md
#![no_main]
#![no_std]

use esp_bootloader_esp_idf;
use esp_hal::{delay::Delay, main, peripherals::SENS};
use panic_rtt_target as _;

// IDF bootloader 要求的 app 描述符（包含版本、大小等元数据）
esp_bootloader_esp_idf::esp_app_desc!();
use rtt_target::{rprintln, rtt_init_print};

#[main]
fn main() -> ! {
    // 初始化 RTT 日志（host 端通过 probe-rs 查看输出）
    rtt_init_print!();
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let delay = Delay::new();

    // ── 初始化温度传感器 ─────────────────────────────────
    let regs = SENS::regs();

    // ① 开启温度传感器外设时钟（不开时钟则寄存器读写无效）
    regs.sar_peri_clk_gate_conf()
        .modify(|_, w| w.tsens_clk_en().set_bit());

    // ② 强制上电：软件直接控制电源，不依赖硬件自动管理
    regs.sar_tsens_ctrl().modify(|_, w| {
        w.sar_tsens_power_up_force() // 绕过硬件自动上下电
            .set_bit()
            .sar_tsens_power_up() // 给传感器上电
            .set_bit()
    });

    // ③ 设置上电等待时间 = 0b11（最大值），确保模拟电路稳定
    regs.sar_tsens_ctrl2()
        .modify(|_, w| unsafe { w.sar_tsens_xpd_force().bits(3) });

    // ④ 等待 300µs，让传感器内部模拟电路稳定
    delay.delay_micros(300);

    // ── 循环读取温度 ────────────────────────────────────
    loop {
        // 触发一次转换：将内部 ADC 结果锁存到输出寄存器
        regs.sar_tsens_ctrl()
            .modify(|_, w| w.sar_tsens_dump_out().set_bit());

        // 忙等待：直到转换完成（ready 位变为 1）
        while !regs.sar_tsens_ctrl().read().sar_tsens_ready().bit_is_set() {}

        // 读取原始 8 位 ADC 值（范围 0~255）
        let raw = regs.sar_tsens_ctrl().read().sar_tsens_out().bits();

        // 清除触发信号，为下次转换做准备
        regs.sar_tsens_ctrl()
            .modify(|_, w| w.sar_tsens_dump_out().clear_bit());

        // 换算公式（无校准 / DAC=0 时的简化版）：
        //   °C = raw × 0.4386 - 20.52
        // 例: raw=120 → 120 × 0.4386 - 20.52 ≈ 32.1°C
        let temp = raw as f32 * 0.4386 - 20.52;
        rprintln!("{:.1}°C (raw={})", temp, raw);

        delay.delay_millis(2000);
    }
}
