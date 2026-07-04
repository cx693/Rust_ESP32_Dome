//! ESP32-S3 AP+STA 共存配网示例
//!
//! 架构：AP 热点(192.168.4.1) + STA 客户端，Web 页面配网
//! 流程：手机连热点 → 浏览器选 Wi-Fi → ESP32 连路由器 → 显示 IP
//!
//! 模块：state(共享状态) / wifi(控制) / dhcp(地址分配) / http(Web服务) / led(指示灯)

#![no_main]
#![no_std]

extern crate alloc;

use alloc::string::String;

use embassy_executor::Spawner;
use embassy_net::{Ipv4Address, Ipv4Cidr, StackResources, StaticConfigV4};
use embassy_time::{Duration, Timer};
use esp_alloc as _;
use esp_bootloader_esp_idf;
use esp_hal::{clock::CpuClock, gpio::{Level, Output, OutputConfig}, interrupt::software::SoftwareInterruptControl, rng::Rng, timer::timg::TimerGroup};
use esp_radio::wifi::{ap::AccessPointConfig, sta::StationConfig, AuthenticationMethod, Config, ControllerConfig, WifiController};
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

use ap_sta_coex::state::*;
use ap_sta_coex::{dhcp, http, led, wifi};
use ap_sta_coex::mk_static;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    rtt_init_print!();
    rprintln!("AP+STA 配网模式启动");

    let p = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    esp_alloc::heap_allocator!(size: 128 * 1024);

    let timg0 = TimerGroup::new(p.TIMG0);
    let sw_int = SoftwareInterruptControl::new(p.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    // AP+STA 双模式配置
    let ap_cfg = AccessPointConfig::default()
        .with_ssid(AP_SSID).with_password(String::from(AP_PASSWORD))
        .with_auth_method(AuthenticationMethod::Wpa2Personal);
    let (controller, ifaces) = esp_radio::wifi::new(
        p.WIFI,
        ControllerConfig::default().with_initial_config(Config::AccessPointStation(StationConfig::default(), ap_cfg)),
    ).unwrap();

    // 硬件随机种子
    let seed = { let r = Rng::new(); (r.random() as u64) << 32 | r.random() as u64 };

    // AP 网络栈（静态 IP 192.168.4.1）
    let (ap_stack, ap_runner) = embassy_net::new(
        ifaces.access_point,
        embassy_net::Config::ipv4_static(StaticConfigV4 {
            address: Ipv4Cidr::new(Ipv4Address::new(192, 168, 4, 1), 24),
            gateway: None, dns_servers: Default::default(),
        }),
        mk_static!(StackResources<3>, StackResources::<3>::new()), seed,
    );
    // STA 网络栈（DHCP 自动获取 IP）
    let (sta_stack, sta_runner) = embassy_net::new(
        ifaces.station, embassy_net::Config::dhcpv4(Default::default()),
        mk_static!(StackResources<3>, StackResources::<3>::new()), seed ^ 0xDEADBEEF,
    );

    // 启动所有任务（#[embassy_executor::task] 返回 Result<SpawnToken, _>，需 unwrap）
    spawner.spawn(net_runner(ap_runner).unwrap());
    spawner.spawn(net_runner(sta_runner).unwrap());
    spawner.spawn(dhcp_task(ap_stack).unwrap());
    spawner.spawn(led_task(Output::new(p.GPIO48, Level::Low, OutputConfig::default())).unwrap());
    spawner.spawn(wifi_task(controller, sta_stack).unwrap());
    spawner.spawn(http_task_ap(ap_stack).unwrap());
    spawner.spawn(http_task_sta(sta_stack).unwrap());

    rprintln!("所有任务已启动");
    loop { Timer::after(Duration::from_secs(3600)).await; }
}

// ─── 任务包装层（库 async fn → embassy task）────────────────

#[embassy_executor::task(pool_size = 2)] // AP + STA 各一个
async fn net_runner(mut r: embassy_net::Runner<'static, esp_radio::wifi::Interface<'static>>) { r.run().await }

#[embassy_executor::task]
async fn dhcp_task(s: embassy_net::Stack<'static>) { dhcp::dhcp_task(s).await }

#[embassy_executor::task]
async fn led_task(l: Output<'static>) { led::led_task(l).await }

#[embassy_executor::task]
async fn wifi_task(c: WifiController<'static>, s: embassy_net::Stack<'static>) { wifi::wifi_control_task(c, s).await }

#[embassy_executor::task]
async fn http_task_ap(s: embassy_net::Stack<'static>) { http::http_server_task(s, "AP").await }

#[embassy_executor::task]
async fn http_task_sta(s: embassy_net::Stack<'static>) { http::http_server_task(s, "STA").await }
