#![no_main]
#![no_std]
#![allow(unused_imports)]

use esp_bootloader_esp_idf;
use esp_hal::{
    analog::adc::{Adc, AdcCalCurve, AdcConfig, Attenuation},
    delay::Delay,
    main,
    peripherals::SENS,
};
use panic_rtt_target as _;

esp_bootloader_esp_idf::esp_app_desc!();
use rtt_target::{rprintln, rtt_init_print};

#[main]
fn main() -> ! {
    rtt_init_print!();
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let delay = Delay::new();

    let _pin = peripherals.GPIO1;
    let _adc = peripherals.ADC1;

    let regs = SENS::regs();
    regs.sar_peri_clk_gate_conf()
        .modify(|_, w| w.tsens_clk_en().set_bit());
    regs.sar_tsens_ctrl().modify(|_, w| {
        w.sar_tsens_power_up_force()
            .set_bit()
            .sar_tsens_power_up()
            .set_bit()
    });
    regs.sar_tsens_ctrl2()
        .modify(|_, w| unsafe { w.sar_tsens_xpd_force().bits(3) });
    delay.delay_micros(300);

    loop {
        regs.sar_tsens_ctrl()
            .modify(|_, w| w.sar_tsens_dump_out().set_bit());
        while !regs.sar_tsens_ctrl().read().sar_tsens_ready().bit_is_set() {}
        let raw = regs.sar_tsens_ctrl().read().sar_tsens_out().bits();
        regs.sar_tsens_ctrl()
            .modify(|_, w| w.sar_tsens_dump_out().clear_bit());

        rprintln!("{}°C", raw as f32 * 0.4386 - 20.52);
        delay.delay_millis(2000);
    }
}