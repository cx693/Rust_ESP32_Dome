#![no_main]
#![no_std]

// ==================== ESP32 UART 中断接收示例 ====================
// 功能：通过 UART1 中断接收串口数据，存入缓冲区后在主循环中处理
// 引脚：TX = GPIO17, RX = GPIO18
// 波特率：115200
// ================================================================

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

// -------------------- 全局共享资源 --------------------
// Mutex<RefCell<T>> 是嵌入式 Rust 中跨中断/主循环共享状态的标准模式
// - Mutex: 临界区保护，防止中断和主循环同时访问
// - RefCell: 运行时借用检查，允许内部可变性

/// UART 实例（全局共享，中断和主循环都需要访问）
static SERIAL: Mutex<RefCell<Option<Uart<'_, esp_hal::Blocking>>>> = Mutex::new(RefCell::new(None));

/// 接收数据缓冲区（128 字节）
static RX_BUF: Mutex<RefCell<[u8; 128]>> = Mutex::new(RefCell::new([0u8; 128]));

/// 接收到的数据长度（0 表示无新数据）
static RX_LEN: Mutex<RefCell<usize>> = Mutex::new(RefCell::new(0));

// -------------------- 主函数 --------------------
#[main]
fn main() -> ! {
    // 初始化 RTT 调试输出（替代串口打印，通过 J-Link/调试器查看）
    rtt_init_print!();
    rprintln!("中断 串口");

    // 初始化 ESP32 HAL（时钟、GPIO 等底层外设）
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // UART 接收配置
    // - fifo_full_threshold: FIFO 满 30 字节时触发中断
    // - timeout: 接收超时 10 个字符时间后触发中断（处理不完整数据包）
    let rx_config = RxConfig::default()
        .with_fifo_full_threshold(30)
        .with_timeout(10);

    // UART 总配置：波特率 115200
    let config = Config::default().with_baudrate(115200).with_rx(rx_config);

    // 创建 UART1 实例，绑定 TX/RX 引脚
    let mut uart = Uart::new(peripherals.UART1, config)
        .unwrap()
        .with_rx(peripherals.GPIO18)  // 接收引脚
        .with_tx(peripherals.GPIO17); // 发送引脚

    // 注册中断处理函数
    uart.set_interrupt_handler(uart_interrupt_handler);

    // 在临界区中开启中断并保存 UART 实例到全局变量
    critical_section::with(|cs| {
        // 启用两种中断：FIFO 满 和 接收超时
        uart.listen(UartInterrupt::RxFifoFull | UartInterrupt::RxTimeout);
        // 将 UART 实例移入全局变量（所有权转移）
        SERIAL.borrow_ref_mut(cs).replace(uart);
    });

    rprintln!("UART1 已就绪 (TX=GPIO17, RX=GPIO18, 115200)");
    rprintln!("等待接收数据...");

    // -------------------- 主循环 --------------------
    // 轮询检查是否有新数据到达（由中断设置 RX_LEN）
    loop {
        let len = critical_section::with(|cs| {
            let len = *RX_LEN.borrow_ref(cs);
            if len > 0 {
                // 读取缓冲区数据
                let buf = RX_BUF.borrow_ref(cs);
                let strdata = core::str::from_utf8(&buf[..len]).unwrap_or("");
                rprintln!("main 收到 {} 字节: {:?}", len, &buf[..len]);
                rprintln!("{}", strdata);
            }
            len
        });

        // 处理完毕，清零长度标记（通知中断可覆盖缓冲区）
        if len > 0 {
            critical_section::with(|cs| {
                *RX_LEN.borrow_ref_mut(cs) = 0;
            });
        }
    }
}

// -------------------- UART 中断处理函数 --------------------
// #[handler] 标记为中断处理函数
// #[ram] 将函数放入 RAM 执行（比 Flash 快，中断对延迟敏感）
#[handler]
#[ram]
fn uart_interrupt_handler() {
    critical_section::with(|cs| {
        let mut serial = SERIAL.borrow_ref_mut(cs);
        if let Some(serial) = serial.as_mut() {
            let mut buf = [0u8; 128];

            // 从 UART FIFO 读取数据到临时缓冲区
            if let Ok(cnt) = serial.read_buffered(&buf) && cnt > 0 {
                // 将数据复制到全局缓冲区
                let mut rx_buf = RX_BUF.borrow_ref_mut(cs);
                rx_buf[..cnt].copy_from_slice(&buf[..cnt]);
                // 设置数据长度（主循环会检查此值）
                *RX_LEN.borrow_ref_mut(cs) = cnt;
            }

            // 清除中断标志（必须！否则中断会持续触发）
            serial.clear_interrupts(UartInterrupt::RxFifoFull | UartInterrupt::RxTimeout);
        }
    })
}
