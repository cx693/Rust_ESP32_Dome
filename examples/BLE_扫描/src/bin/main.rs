#![no_std]
#![no_main]

use core::cell::RefCell;

use bt_hci::{cmd::le::LeSetScanParams, controller::ControllerCmdSync};
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_time::{Duration, Timer};
use esp_alloc as _;
use esp_hal::{
    interrupt::software::SoftwareInterruptControl,
    timer::timg::TimerGroup,
};
use esp_radio::ble::controller::BleConnector;
use heapless::Deque;
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};
use trouble_host::prelude::*;

esp_bootloader_esp_idf::esp_app_desc!();

const LOCAL_NAME: &str = "ESP32S3-BLE-Scanner";

#[esp_rtos::main]
async fn main(_s: Spawner) {
    rtt_init_print!();
    rprintln!("BLE Scanner 启动中...");

    let peripherals = esp_hal::init(esp_hal::Config::default());

    esp_alloc::heap_allocator!(size: 72 * 1024);

    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    rprintln!("本机设备名: {}", LOCAL_NAME);

    let bluetooth = peripherals.BT;
    let connector = BleConnector::new(bluetooth, Default::default()).unwrap();
    let controller: ExternalController<_, 1> = ExternalController::new(connector);

    ble_scanner_run(controller).await;
}

const CONNECTIONS_MAX: usize = 1;
const L2CAP_CHANNELS_MAX: usize = 1;

async fn ble_scanner_run<C>(controller: C)
where
    C: Controller + ControllerCmdSync<LeSetScanParams>,
{
    let address: Address = Address::random([0xff, 0x8f, 0x1b, 0x05, 0xe4, 0xff]);
    rprintln!("本机地址: {:?}", address);

    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();

    let stack = trouble_host::new(controller, &mut resources).set_random_address(address);

    let Host {
        central,
        mut runner,
        ..
    } = stack.build();

    let printer = Printer {
        seen: RefCell::new(Deque::new()),
    };

    let mut scanner = Scanner::new(central);

    let _ = join(runner.run_with_handler(&printer), async {
        let mut config = ScanConfig::default();
        config.active = true;
        config.phys = PhySet::M1;
        config.interval = Duration::from_secs(1);
        config.window = Duration::from_secs(1);

        rprintln!("开始扫描...");
        let mut _session = scanner.scan(&config).await.unwrap();

        loop {
            Timer::after(Duration::from_secs(1)).await;
        }
    })
    .await;
}

fn parse_device_name(data: &[u8]) -> Option<&str> {
    let mut i = 0;
    while i < data.len() {
        let len = data[i] as usize;
        if len == 0 || i + len >= data.len() {
            break;
        }
        let ad_type = data[i + 1];
        let ad_data = &data[i + 2..i + 1 + len];
        match ad_type {
            0x09 | 0x08 => {
                if let Ok(name) = core::str::from_utf8(ad_data) {
                    return Some(name);
                }
            }
            _ => {}
        }
        i += 1 + len;
    }
    None
}

fn adv_event_kind_str(kind: &bt_hci::param::LeAdvEventKind) -> &'static str {
    match kind {
        bt_hci::param::LeAdvEventKind::AdvInd => "ADV_IND",
        bt_hci::param::LeAdvEventKind::AdvDirectInd => "ADV_DIRECT_IND",
        bt_hci::param::LeAdvEventKind::AdvScanInd => "ADV_SCAN_IND",
        bt_hci::param::LeAdvEventKind::AdvNonconnInd => "ADV_NONCONN_IND",
        bt_hci::param::LeAdvEventKind::ScanRsp => "SCAN_RSP",
    }
}

struct Printer {
    seen: RefCell<Deque<BdAddr, 128>>,
}

impl EventHandler for Printer {
    fn on_adv_reports(&self, mut it: LeAdvReportsIter<'_>) {
        let mut seen = self.seen.borrow_mut();
        while let Some(Ok(report)) = it.next() {
            let kind = report.event_kind;
            let name = parse_device_name(report.data);

            if seen.iter().find(|b| b.raw() == report.addr.raw()).is_none() {
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
                if seen.is_full() {
                    seen.pop_front();
                }
                seen.push_back(report.addr).unwrap();
            } else if matches!(kind, bt_hci::param::LeAdvEventKind::ScanRsp) {
                if let Some(n) = name {
                    rprintln!("[SCAN_RSP] {:?} [{}]", report.addr, n);
                }
            }
        }
    }
}
