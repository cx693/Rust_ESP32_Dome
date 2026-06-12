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

esp_bootloader_esp_idf::esp_app_desc!();

static BUTTON: Mutex<RefCell<Option<Input>>> = Mutex::new(RefCell::new(None));
static LED: Mutex<RefCell<Option<Output>>> = Mutex::new(RefCell::new(None));

#[main]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("外部中断 -- 点灯");

    let peripherals = esp_hal::init(esp_hal::Config::default());

    let mut io = Io::new(peripherals.IO_MUX);
    io.set_interrupt_handler(handler);

    let led = Output::new(peripherals.GPIO48, Level::Low, OutputConfig::default());

    let config = InputConfig::default().with_pull(Pull::Up);
    let mut button = Input::new(peripherals.GPIO0, config);

    critical_section::with(|cs| {
        button.listen(Event::FallingEdge);
        BUTTON.borrow_ref_mut(cs).replace(button);
        LED.borrow_ref_mut(cs).replace(led);
    });

    loop {}
}

#[handler]
#[ram]
fn handler() {
    let is_button = critical_section::with(|cs| {
        BUTTON
            .borrow_ref_mut(cs)
            .as_mut()
            .unwrap()
            .is_interrupt_set()
    });

    if is_button {
        rprintln!("按键中断触发！");
        critical_section::with(|cs| {
            LED.borrow_ref_mut(cs).as_mut().unwrap().toggle();
        })
    }
    critical_section::with(|cs| {
        BUTTON
            .borrow_ref_mut(cs)
            .as_mut()
            .unwrap()
            .clear_interrupt();
    })
}
