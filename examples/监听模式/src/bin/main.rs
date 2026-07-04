//! # WiFi 监听模式 (Sniffer) 示例
//!
//! 功能：将 ESP32-S3 的 WiFi 网卡置于混杂模式（promiscuous mode），
//!       捕获空中的 802.11 Beacon 帧，提取并打印新发现的 AP（接入点）SSID。
//!
//! ## 运行流程概览
//! 1. 初始化硬件 → 分配堆内存 → 启动 Embassy 异步运行时
//! 2. 创建 WiFi 驱动 → 获取 sniffer 接口 → 开启混杂模式
//! 3. 注册回调：每收到一帧 → 解析为 Beacon → 提取 SSID → 去重后打印
//! 4. 主任务进入空循环，所有实际工作在回调中完成

//% CHIP_FILTER: wifi_driver_supported

// ─── 嵌入式必备属性 ───────────────────────────────────────────────
#![no_std] // 不使用标准库（嵌入式环境没有 OS，用 alloc 提供堆分配）
#![no_main] // 不使用标准 main 入口，由 esp_rtos 接管启动流程

// 需要显式引入 alloc crate，因为 no_std 环境下 alloc 不自动可用
extern crate alloc;

// ─── 标准库替代：堆上集合与字符串 ─────────────────────────────────
use alloc::{
    collections::btree_set::BTreeSet, // 有序集合，用于去重存储已发现的 SSID
    string::{String, ToString},
};
use core::cell::RefCell;

// ─── 依赖导入 ─────────────────────────────────────────────────────
use critical_section::Mutex; // 临界区互斥锁，保护中断回调与主任务共享的数据
use panic_rtt_target as _; // panic 时通过 RTT 通道输出信息（仅引入，不直接调用）
use esp_hal::{
    clock::CpuClock,
    interrupt::software::SoftwareInterruptControl,
    timer::timg::TimerGroup, // 定时器组，为 Embassy 运行时提供时钟源
};
use esp_println::println; // 串口打印宏（替代标准库的 println!）
use ieee80211::{match_frames, mgmt_frame::BeaconFrame}; // 802.11 帧解析

// ─── 固件描述符 ───────────────────────────────────────────────────
// ESP-IDF 引导程序要求的固件元数据（版本、名称等），烧录时使用
esp_bootloader_esp_idf::esp_app_desc!();

// ─── 全局共享状态：已发现的 SSID 集合 ─────────────────────────────
// 为什么需要 Mutex + RefCell？
//   - Mutex：提供中断安全的临界区（critical_section），确保同一时刻只有一个执行流访问数据
//   - RefCell：在已获得锁的前提下，提供运行时借用检查（内部可变性模式）
//   - BTreeSet：有序且自动去重，插入已存在的 SSID 会返回 false
static KNOWN_SSIDS: Mutex<RefCell<BTreeSet<String>>> = Mutex::new(RefCell::new(BTreeSet::new()));

// ─── 程序入口 ─────────────────────────────────────────────────────
// #[esp_rtos::main] 标记异步入口，由 Embassy 运行时调度执行
// 返回 `-> !` 表示永不返回（嵌入式程序通常不会正常退出）
#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) -> ! {
    // 初始化日志系统（通过环境变量控制日志级别）
    esp_println::logger::init_logger_from_env();

    // ── 步骤 1：初始化 ESP32-S3 硬件 ─────────────────────────────
    // 设置 CPU 为最高频率（ESP32-S3 默认 240MHz），获取所有外设的控制权
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // ── 步骤 2：分配堆内存 ───────────────────────────────────────
    // 嵌入式没有操作系统管理内存，必须手动分配堆空间
    // 72KB 用于存储 String 等堆上分配的数据
    esp_alloc::heap_allocator!(size: 72 * 1024);

    // ── 步骤 3：启动 Embassy 异步运行时 ─────────────────────────
    // Embassy 需要一个硬件定时器作为任务调度的时钟源
    // TIMG0.timer0 提供定时，软件中断用于任务唤醒
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    // ── 步骤 4：初始化 WiFi 驱动 ────────────────────────────────
    // esp_radio::wifi::new 返回控制器（用于配置 WiFi）和接口集合
    // 这里只需要 sniffer 接口，所以用 _ 忽略 controller
    let (_controller, interfaces) =
        esp_radio::wifi::new(peripherals.WIFI, Default::default()).unwrap();

    // ── 步骤 5：开启混杂模式并注册回调 ──────────────────────────
    let mut sniffer = interfaces.sniffer;

    // 混杂模式 = 接收所有 WiFi 帧（不仅是发给自己的）
    sniffer.set_promiscuous_mode(true).unwrap();

    // set_receive_cb 注册一个回调闭包，每收到一帧 WiFi 数据就会被调用
    // ⚠️ 回调在中断上下文中执行，不能做耗时操作，不能阻塞
    sniffer.set_receive_cb(|packet| {
        // match_frames! 宏：尝试将原始字节解析为 BeaconFrame
        // 如果不是 Beacon 帧则直接跳过（不做任何处理）
        let _ = match_frames! {
            packet.data,
            beacon = BeaconFrame => {
                // 提取 SSID（WiFi 名称），某些帧可能没有 SSID，直接跳过
                let Some(ssid) = beacon.ssid() else {
                    return;
                };

                // critical_section::with 进入临界区（关中断），安全访问全局数据
                // insert() 返回 true 表示是新 SSID，false 表示已存在
                if critical_section::with(|cs| {
                    KNOWN_SSIDS.borrow_ref_mut(cs).insert(ssid.to_string())
                }) {
                    // 只有新发现的 AP 才打印，避免重复输出
                    println!("Found new AP with SSID: {ssid}");
                }
            }
        };
    });

    // ── 步骤 6：主任务空循环 ────────────────────────────────────
    // 所有 WiFi 监听工作都在回调中完成，主任务无需做任何事
    // loop {} 让程序保持运行，不会退出
    loop {}
}
