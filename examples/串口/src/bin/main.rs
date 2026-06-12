#![no_main]
#![no_std]

use core::{cell::RefCell, fmt::Write};

use critical_section::Mutex;
use esp_bootloader_esp_idf as _;
use esp_hal::{
    handler,
    main, ram,
    uart::{Config, RxConfig, Uart, UartInterrupt},
};
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

esp_bootloader_esp_idf::esp_app_desc!();

static SERIAL: Mutex<RefCell<Option<Uart<'_, esp_hal::Blocking>>>> = Mutex::new(RefCell::new(None));
static RX_BUF: Mutex<RefCell<[u8; 128]>> = Mutex::new(RefCell::new([0u8; 128]));
static RX_LEN: Mutex<RefCell<usize>> = Mutex::new(RefCell::new(0));

#[main]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("中断 串口");

    let peripherals = esp_hal::init(esp_hal::Config::default());

    let rx_config = RxConfig::default()
        .with_fifo_full_threshold(30)
        .with_timeout(10);

    let config = Config::default().with_baudrate(115200).with_rx(rx_config);

    let mut uart = Uart::new(peripherals.UART1, config)
        .unwrap()
        .with_rx(peripherals.GPIO18)
        .with_tx(peripherals.GPIO17);

    uart.set_interrupt_handler(uart_interrupt_handler);

    critical_section::with(|cs| {
        uart.listen(UartInterrupt::RxFifoFull | UartInterrupt::RxTimeout);
        SERIAL.borrow_ref_mut(cs).replace(uart);
    });

    rprintln!("UART1 已就绪 (TX=GPIO17, RX=GPIO18, 115200)");
    rprintln!("等待接收数据...");

    loop {
        let len = critical_section::with(|cs|{
            let len = *RX_LEN.borrow_ref(cs);
            if len > 0{
                let buf = RX_BUF.borrow_ref(cs);
                let strdata = core::str::from_utf8(&buf[..len]).unwrap_or("");
                rprintln!("main 收到 {} 字节: {:?}", len, &buf[..len]);
                rprintln!("{}",strdata);
            }
            len
        });
        if len > 0{
            critical_section::with(|cs|{
                *RX_LEN.borrow_ref_mut(cs) = 0;
            });
        }
    }
}

#[handler]
#[ram]
fn uart_interrupt_handler() {
    critical_section::with(|cs|{
        let mut serial = SERIAL.borrow_ref_mut(cs);
        if let Some(serial) = serial.as_mut() {
            let mut buf = [0u8;128];
            if let Ok(cnt) = serial.read_buffered(&mut buf) && cnt > 0{
                let mut rx_buf = RX_BUF.borrow_ref_mut(cs);
                rx_buf[..cnt].copy_from_slice(&buf[..cnt]);
                *RX_LEN.borrow_ref_mut(cs) = cnt;
            }
            serial.clear_interrupts(UartInterrupt::RxFifoFull | UartInterrupt::RxTimeout);
        } 
    })
}
