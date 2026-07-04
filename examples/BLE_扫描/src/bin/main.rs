//! ESP32-S3 BLE 扫描器
//!
//! 功能：扫描附近的 BLE 设备，打印设备地址、名称和信号强度(RSSI)。
//! 使用 trouble-host 作为 BLE 协议栈，通过 RTT 输出调试信息。

#![no_std]
#![no_main]

use core::cell::RefCell;

use bt_hci::{cmd::le::LeSetScanParams, controller::ControllerCmdSync};
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_time::{Duration, Timer};
use esp_alloc as _;
use esp_hal::{interrupt::software::SoftwareInterruptControl, timer::timg::TimerGroup};
use esp_radio::ble::controller::BleConnector;
use heapless::Deque;
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};
use trouble_host::prelude::*;

// 声明 IDF 应用描述符（ESP-IDF bootloader 需要）
esp_bootloader_esp_idf::esp_app_desc!();

/// 本机 BLE 设备名称
const LOCAL_NAME: &str = "ESP32S3-BLE-Scanner";

/// 最大并发连接数（扫描器只需 1）
const CONNECTIONS_MAX: usize = 1;
/// 最大 L2CAP 通道数
const L2CAP_CHANNELS_MAX: usize = 1;

/// ============================================================
/// 入口函数
/// ============================================================
#[esp_rtos::main]
async fn main(_s: Spawner) {
    rtt_init_print!();
    rprintln!("BLE Scanner 启动中...");

    // 初始化 HAL 外设
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // 堆内存分配器（72 KiB，BLE 协议栈需要较大堆空间）
    esp_alloc::heap_allocator!(size: 72 * 1024);

    // 启动 embassy 异步运行时
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    rprintln!("本机设备名: {}", LOCAL_NAME);

    // 创建 BLE HCI 控制器（连接到底层蓝牙硬件）
    let connector = BleConnector::new(peripherals.BT, Default::default()).unwrap();
    let controller: ExternalController<_, 1> = ExternalController::new(connector);

    // 运行 BLE 扫描
    ble_scanner_run(controller).await;
}

/// ============================================================
/// BLE 扫描主逻辑
/// ============================================================
async fn ble_scanner_run<C>(controller: C)
where
    C: Controller + ControllerCmdSync<LeSetScanParams>,
{
    // 设置本机随机地址（BLE 隐私机制，防止被追踪）
    // 注意：最高两位为 11，表示 Static Random Address
    let address: Address = Address::random([0xff, 0x8f, 0x1b, 0x05, 0xe4, 0xff]);
    rprintln!("本机地址: {:?}", address);

    // 协议栈资源（连接槽 + L2CAP 通道）
    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();

    // 构建 trouble-host 协议栈，设置随机地址
    let stack = trouble_host::new(controller, &mut resources).set_random_address(address);

    // 解构出 central 角色和事件循环 runner
    let Host {
        central,
        mut runner,
        ..
    } = stack.build();

    // 事件处理器：维护已见设备列表，控制打印逻辑
    let printer = Printer {
        seen: RefCell::new(Deque::new()),
    };

    let mut scanner = Scanner::new(central);

    // join: 事件循环 和 扫描配置 并发运行
    // - runner: 负责收发 HCI 数据包，收到广播包时回调 printer
    // - 匿名 async: 配置扫描参数并保持扫描持续运行
    let _ = join(runner.run_with_handler(&printer), async {
        let config = ScanConfig {
            active: true,                         // 主动扫描（请求 SCAN_RSP 获取设备名）
            phys: PhySet::M1,                     // 仅使用 1M PHY
            interval: Duration::from_secs(1),     // 扫描间隔
            window: Duration::from_secs(1),       // 扫描窗口（= interval 表示连续扫描）
            ..ScanConfig::default()
        };

        rprintln!("开始扫描...");
        // _session 必须保持存活，drop 后扫描会停止
        let mut _session = scanner.scan(&config).await.unwrap();

        // 保持任务存活，扫描通过回调异步报告结果
        loop {
            Timer::after(Duration::from_secs(1)).await;
        }
    })
    .await;
}

/// ============================================================
/// 解析广播数据中的设备名称
/// ============================================================
///
/// BLE 广播数据格式：[长度][AD Type][AD Data...] 重复排列
///   - 0x09: Complete Local Name（完整名称）
///   - 0x08: Shortened Local Name（缩短名称）
fn parse_device_name(data: &[u8]) -> Option<&str> {
    let mut i = 0;
    while i < data.len() {
        let len = data[i] as usize;
        // len == 0 表示到达末尾的填充；len 溢出则数据损坏
        if len == 0 || i + len > data.len() {
            break;
        }
        let ad_type = data[i + 1];
        let ad_data = &data[i + 2..i + 1 + len];

        // AD Type 0x09 = Complete Local Name, 0x08 = Shortened Local Name
        if matches!(ad_type, 0x09 | 0x08) {
            if let Ok(name) = core::str::from_utf8(ad_data) {
                return Some(name);
            }
        }
        // 跳到下一个 AD 结构（当前长度字节 + len 字节数据）
        i += 1 + len;
    }
    None
}

/// 将广播事件类型转为可读字符串
fn adv_event_kind_str(kind: &bt_hci::param::LeAdvEventKind) -> &'static str {
    match kind {
        bt_hci::param::LeAdvEventKind::AdvInd => "ADV_IND",           // 可连接可扫描广播
        bt_hci::param::LeAdvEventKind::AdvDirectInd => "ADV_DIRECT_IND", // 定向广播（仅目标设备可连）
        bt_hci::param::LeAdvEventKind::AdvScanInd => "ADV_SCAN_IND",     // 仅可扫描广播
        bt_hci::param::LeAdvEventKind::AdvNonconnInd => "ADV_NONCONN_IND", // 不可连接广播
        bt_hci::param::LeAdvEventKind::ScanRsp => "SCAN_RSP",         // 扫描响应（主动扫描的回复）
    }
}

/// ============================================================
/// 广播事件处理器
/// ============================================================
///
/// trouble-host 在收到广播包时回调 `on_adv_reports`。
/// 使用环形缓冲区记录已见设备，避免重复打印。
struct Printer {
    /// 已发现设备的地址环形缓冲区（最多 128 个）
    seen: RefCell<Deque<BdAddr, 128>>,
}

impl EventHandler for Printer {
    fn on_adv_reports(&self, mut it: LeAdvReportsIter<'_>) {
        let mut seen = self.seen.borrow_mut();

        while let Some(Ok(report)) = it.next() {
            let kind = report.event_kind;
            let name = parse_device_name(report.data);
            let is_new = !seen.iter().any(|b| b.raw() == report.addr.raw());

            if is_new {
                // 新设备：打印完整信息
                match name {
                    Some(n) => rprintln!(
                        "[{}] {:?} [{}] rssi:{}",
                        adv_event_kind_str(&kind),
                        report.addr,
                        n,
                        report.rssi
                    ),
                    None => rprintln!(
                        "[{}] {:?} rssi:{} data_len:{}",
                        adv_event_kind_str(&kind),
                        report.addr,
                        report.rssi,
                        report.data.len()
                    ),
                }
                // 环形缓冲区满时淘汰最旧的记录
                if seen.is_full() {
                    seen.pop_front();
                }
                let _ = seen.push_back(report.addr);
            } else if matches!(kind, bt_hci::param::LeAdvEventKind::ScanRsp) {
                // 已知设备的 SCAN_RSP：补充打印设备名
                if let Some(n) = name {
                    rprintln!("[SCAN_RSP] {:?} [{}]", report.addr, n);
                }
            }
        }
    }
}
