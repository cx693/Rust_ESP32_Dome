//! BLE 电池服务外设示例（Battery Service Peripheral）
//!
//! 功能概述：
//! 1. 初始化 ESP32-S3 硬件和 BLE 控制器
//! 2. 以随机地址广播，等待中央设备（Central）连接
//! 3. 连接后提供 GATT 电池服务（电量读取/通知 + 自定义状态特征）
//! 4. 每 2 秒向已连接的中央设备推送电量值，并读取 RSSI 信号强度
//!
//! 技术栈：Embassy（异步运行时） + trouBLE（BLE 协议栈） + esp-hal（硬件抽象层）

#![no_std] // 嵌入式环境：不使用标准库
#![no_main] // 入口函数由 esp_rtos 管理，不需要标准 main

// ========== 导入 ==========

use embassy_executor::Spawner;
use embassy_futures::{join::join, select::select};
use embassy_time::Timer;
use esp_alloc as _; // 引入堆分配器（虽不直接使用，但必须存在）
use esp_backtrace as _; // panic 时的回溯处理
use esp_hal::{
    clock::CpuClock,
    interrupt::software::SoftwareInterruptControl,
    timer::timg::TimerGroup,
};
use esp_radio::ble::controller::BleConnector;
use rtt_target::{rprintln, rtt_init_print}; // RTT 调试输出（probe-rs 可读取）
use trouble_host::prelude::*; // trouBLE BLE 协议栈的核心类型

// ========== 常量 ==========

/// 最大同时连接数（外设通常只需 1 个）
const CONNECTIONS_MAX: usize = 1;
/// L2CAP 通道数：1 个信令通道 + 1 个 ATT 数据通道
const L2CAP_CHANNELS_MAX: usize = 2;

// ========== GATT 服务定义 ==========
// GATT（Generic Attribute Profile）是 BLE 数据交换的核心协议
// 服务 → 特征 → 描述符，层层嵌套

/// GATT 服务器：包含所有服务的集合
#[gatt_server]
struct Server {
    battery_service: BatteryService,
}

/// 电池服务（UUID: 0x180F —— BLE 标准电池服务）
#[gatt_service(uuid = service::BATTERY)]
struct BatteryService {
    /// 电量特征：可读 + 可通知，范围 0~100，初始值 10
    #[descriptor(uuid = descriptors::VALID_RANGE, read, value = [0, 100])]
    #[descriptor(uuid = descriptors::MEASUREMENT_DESCRIPTION, read, value = "Battery Level")]
    #[characteristic(uuid = characteristic::BATTERY_LEVEL, read, notify, value = 10)]
    level: u8,

    /// 自定义状态特征：可读 + 可写 + 可通知（自定义 UUID）
    #[characteristic(uuid = "408813df-5dd4-1f87-ec11-cab001100000", write, read, notify)]
    status: bool,
}

// ========== 入口函数 ==========

// ESP-IDF 应用描述符（固件元数据，烧录工具需要）
esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main] // embassy 异步入口宏
async fn main(_s: Spawner) {
    // 1) 初始化 RTT 调试输出（通过 probe-rs 在电脑端查看）
    rtt_init_print!();
    rprintln!("=== BLE 电池外设启动 ===");

    // 2) 初始化硬件：外设、CPU 时钟拉满
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    // 3) 分配堆内存（BLE 协议栈需要动态内存，72KB）
    esp_alloc::heap_allocator!(size: 72 * 1024);

    // 4) 启动 esp-rtos 异步运行时（需要定时器 + 软件中断）
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    // 5) 创建 BLE 控制器
    //    BleConnector: 连接 esp-radio（ESP32 内置蓝牙硬件）与 trouBLE（BLE 协议栈）
    let connector = BleConnector::new(peripherals.BT, Default::default()).unwrap();
    //    ExternalController: 将硬件控制器适配为 trouBLE 的通用 Controller 接口
    //    泛型参数 1 = HCI 命令管道深度（同时最多缓存 1 条 HCI 命令）
    let controller: ExternalController<_, 1> = ExternalController::new(connector);

    // 6) 进入 BLE 主循环
    ble_bas_peripheral_run(controller).await;
}

// ========== BLE 主逻辑 ==========

/// BLE 电池外设主函数：初始化协议栈 → 广播 → 等待连接 → 处理事件 → 断开后重新广播
async fn ble_bas_peripheral_run<C: Controller>(controller: C) {
    // 使用固定随机地址（实际产品中应从硬件 MAC 地址生成）
    let address = Address::random([0xff, 0x8f, 0x1a, 0x05, 0xe4, 0xff]);
    rprintln!("BLE 地址 = {:?}", address);

    // 创建协议栈资源（连接池 + L2CAP 通道池）
    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();

    // 构建 BLE 协议栈，设置随机地址
    let stack = trouble_host::new(controller, &mut resources).set_random_address(address);

    // 拆分出 peripheral（广播/连接管理器）和 runner（底层事件循环驱动）
    let Host {
        mut peripheral,
        runner,
        ..
    } = stack.build();

    // 创建 GATT 服务器，配置 GAP（Generic Access Profile）设备名称和外观
    let server = Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: "BLE-ESP32", // 设备广播名称
        appearance: &appearance::power_device::GENERIC_POWER_DEVICE, // 外观图标：通用电源设备
    }))
    .unwrap();

    // join: 同时运行两个任务，任一结束则全部结束
    //   - ble_task:     底层 BLE 事件循环（必须一直运行）
    //   - 广播循环:     广播 → 连接 → 处理事件 → 断开 → 重新广播
    let _ = join(ble_task(runner), async {
        loop {
            // 发起广播，等待中央设备连接
            match advertise(&mut peripheral, &server).await {
                Ok(conn) => {
                    rprintln!("中央设备已连接");

                    // 连接成功后，同时运行两个任务（select: 任一结束则全部结束）
                    //   - gatt_events_task: 处理 GATT 读写事件
                    //   - custom_task:      定时推送电量通知
                    select(
                        gatt_events_task(&server, &conn),
                        custom_task(&server, &conn, &stack),
                    )
                    .await;

                    rprintln!("连接已断开，重新广播...");
                }
                Err(e) => {
                    panic!("[广播] 错误: {:?}", e);
                }
            }
        }
    })
    .await;
}

// ========== 子任务 ==========

/// BLE 底层事件循环（必须持续运行，驱动 HCI 数据收发）
async fn ble_task<C: Controller, P: PacketPool>(mut runner: Runner<'_, C, P>) {
    loop {
        if let Err(e) = runner.run().await {
            panic!("[BLE 事件循环] 错误: {:?}", e);
        }
    }
}

/// GATT 事件处理任务：响应中央设备的读/写请求
async fn gatt_events_task<P: PacketPool>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
) -> Result<(), Error> {
    let level = server.battery_service.level;
    let status = server.battery_service.status;

    let reason = loop {
        match conn.next().await {
            // 连接断开事件
            GattConnectionEvent::Disconnected { reason } => break reason,

            // GATT 读写事件
            GattConnectionEvent::Gatt { event } => {
                match &event {
                    // 读取事件：中央设备读取特征值
                    GattEvent::Read(event) => {
                        if event.handle() == level.handle {
                            let value = server.get(&level);
                            rprintln!("[GATT] 读取电量特征: {:?}", value);
                        } else if event.handle() == status.handle {
                            let value = server.get(&status);
                            rprintln!("[GATT] 读取状态特征: {:?}", value);
                        }
                    }
                    // 写入事件：中央设备写入特征值
                    GattEvent::Write(event) => {
                        if event.handle() == level.handle {
                            rprintln!("[GATT] 写入电量特征: {:?}", event.data());
                        } else if event.handle() == status.handle {
                            rprintln!("[GATT] 写入状态特征: {:?}", event.data());
                        }
                    }
                    _ => {}
                };
                // 必须显式接受事件并发送响应（否则中央设备会超时）
                match event.accept() {
                    Ok(reply) => reply.send().await,
                    Err(e) => rprintln!("[GATT] 响应发送失败: {:?}", e),
                };
            }
            _ => {} // 忽略其他连接事件
        }
    };

    rprintln!("[GATT] 断开原因: {:?}", reason);
    Ok(())
}

/// 广播任务：发送广播数据，等待中央设备连接
async fn advertise<'a, 's, C: Controller>(
    peripheral: &mut Peripheral<'a, C, DefaultPacketPool>,
    server: &'s Server<'a>,
) -> Result<GattConnection<'a, 's, DefaultPacketPool>, BleHostError<C::Error>> {
    // 构造广播数据（BLE 广播包最大 31 字节）
    let mut advertiser_data = [0; 31];
    let len = AdStructure::encode_slice(
        &[
            // Flags: 可被发现 + 仅 BLE（不兼容经典蓝牙）
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            // 电池服务 UUID（0x180F）
            AdStructure::ServiceUuids16(&[[0x0f, 0x18]]),
            // 设备名称
            AdStructure::CompleteLocalName(b"BLE-ESP32"),
        ],
        &mut advertiser_data[..],
    )?;

    // 开始广播（可连接 + 可扫描 + 定向广播）
    let advertiser = peripheral
        .advertise(
            &Default::default(),
            Advertisement::ConnectableScannableUndirected {
                adv_data: &advertiser_data[..len],
                scan_data: &[],
            },
        )
        .await?;

    rprintln!("[广播] 正在广播，等待中央设备连接...");

    // 阻塞等待中央设备连接，并绑定 GATT 服务器
    let conn = advertiser.accept().await?.with_attribute_server(server)?;

    rprintln!("[广播] 连接已建立");
    Ok(conn)
}

/// 自定义任务：每 2 秒向中央设备推送电量通知，并读取 RSSI 信号强度
async fn custom_task<C: Controller, P: PacketPool>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
    stack: &Stack<'_, C, P>,
) {
    let mut battery_level: u8 = 100; // 初始电量 100%
    let level = server.battery_service.level;

    loop {
        // 模拟电池放电：每 2 秒减少 1%，到 0% 后回到 100%（循环演示）
        if battery_level == 0 {
            battery_level = 100;
        } else {
            battery_level -= 1;
        }

        // 通过 notify 向中央设备推送新值（无需中央设备轮询）
        rprintln!("[通知] 电量 = {}%", battery_level);
        if level.notify(conn, &battery_level).await.is_err() {
            rprintln!("[通知] 推送失败，连接可能已断开");
            break;
        }

        // 读取 RSSI（Received Signal Strength Indicator，信号强度，单位 dBm）
        // 值越大（越接近 0）信号越强，典型范围：-30（极近）~-90（很远）
        match conn.raw().rssi(stack).await {
            Ok(rssi) => rprintln!("[RSSI] 信号强度 = {} dBm", rssi),
            Err(_) => {
                rprintln!("[RSSI] 读取失败");
                break;
            }
        }

        // 等待 2 秒后再次推送
        Timer::after_secs(2).await;
    }
}
