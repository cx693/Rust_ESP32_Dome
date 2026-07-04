//! WiFi 控制任务 —— 通过 Channel 接收命令，执行扫描/连接/断开

use alloc::string::String;
use core::fmt::Write;
use core::sync::atomic::Ordering;

use embassy_net::Stack;
use embassy_time::Duration;
use esp_radio::wifi::{
    ap::AccessPointConfig, scan::ScanConfig, sta::StationConfig,
    AuthenticationMethod, Config, WifiController,
};
use heapless::String as HString;
use heapless::Vec as HVec;
use rtt_target::rprintln;

use super::state::*;

/// WiFi 控制主任务：循环接收命令并执行
pub async fn wifi_control_task(mut controller: WifiController<'static>, sta_stack: Stack<'static>) {
    loop {
        match CMD.receive().await {
            WifiCmd::Scan => handle_scan(&mut controller).await,
            WifiCmd::Connect(ssid, pw) => handle_connect(&mut controller, &sta_stack, ssid, pw).await,
            WifiCmd::Disconnect => handle_disconnect(&mut controller).await,
        }
    }
}

/// 构建 AP 配置（多处复用）
fn make_ap_config() -> AccessPointConfig {
    AccessPointConfig::default()
        .with_ssid(AP_SSID)
        .with_password(String::from(AP_PASSWORD))
        .with_auth_method(AuthenticationMethod::Wpa2Personal)
}

/// 扫描附近 Wi-Fi
async fn handle_scan(controller: &mut WifiController<'static>) {
    rprintln!("扫描 Wi-Fi...");
    let list = match controller.scan_async(&ScanConfig::default()).await {
        Ok(results) => {
            let mut list: HVec<ScanAp, 20> = HVec::new();
            for ap in results.iter() {
                let mut ssid = HString::<32>::new();
                ssid.push_str(ap.ssid.as_str()).ok();
                let mut auth = HString::<16>::new();
                auth.push_str(match ap.auth_method {
                    Some(AuthenticationMethod::None) => "Open",
                    Some(AuthenticationMethod::Wpa) => "WPA",
                    Some(AuthenticationMethod::Wpa2Personal) => "WPA2",
                    Some(AuthenticationMethod::WpaWpa2Personal) => "WPA/WPA2",
                    Some(AuthenticationMethod::Wpa3Personal) => "WPA3",
                    Some(AuthenticationMethod::Wpa2Wpa3Personal) => "WPA2/WPA3",
                    _ => "Other",
                }).ok();
                list.push(ScanAp { ssid, rssi: ap.signal_strength, channel: ap.channel, auth }).ok();
            }
            rprintln!("扫描完成: {} 个网络", list.len());
            list
        }
        Err(e) => { rprintln!("扫描失败: {:?}", e); HVec::new() }
    };
    RESP.send(WifiResp::ScanDone(list)).await;
}

/// 连接到指定 Wi-Fi（保存 SSID → 配置 STA → 连接 → 等 DHCP IP）
async fn handle_connect(
    controller: &mut WifiController<'static>,
    sta_stack: &Stack<'static>,
    ssid: HString<32>,
    password: HString<64>,
) {
    rprintln!("连接: {}", ssid.as_str());
    // 保存 SSID 到全局状态
    critical_section::with(|cs| {
        let mut s = CONNECTED_SSID.borrow(cs).borrow_mut();
        s.clear();
        write!(s, "{}", ssid.as_str()).ok();
    });
    APP_STATE.store(STATE_CONNECTING, Ordering::Relaxed);
    RESP.send(WifiResp::Connecting).await;

    // 配置 STA + AP
    let sta = StationConfig::default()
        .with_ssid(ssid.as_str())
        .with_password(String::from(password.as_str()));
    let config = Config::AccessPointStation(sta, make_ap_config());

    if let Err(e) = controller.set_config(&config) {
        rprintln!("配置失败: {:?}", e);
        APP_STATE.store(STATE_AP, Ordering::Relaxed);
        return;
    }

    // 连接（20s 超时）
    match embassy_time::with_timeout(Duration::from_secs(20), controller.connect_async()).await {
        Ok(Ok(info)) => {
            rprintln!("Wi-Fi 已连接: {:?}", info);
            // 等 DHCP 分配 IP（15s 超时）
            let ip = match embassy_time::with_timeout(Duration::from_secs(15), sta_stack.wait_config_up()).await {
                Ok(()) => sta_stack.config_v4().map(|cfg| {
                    let mut s = HString::<32>::new();
                    write!(s, "{}", cfg.address.address()).ok();
                    s
                }),
                Err(_) => None,
            };
            if let Some(ref ip) = ip {
                rprintln!("STA IP: {}", ip.as_str());
                critical_section::with(|cs| {
                    let mut s = STA_IP.borrow(cs).borrow_mut();
                    s.clear();
                    s.push_str(ip.as_str()).ok();
                });
            }
            APP_STATE.store(STATE_CONNECTED, Ordering::Relaxed);
        }
        Ok(Err(e)) => { rprintln!("连接失败: {:?}", e); APP_STATE.store(STATE_AP, Ordering::Relaxed); }
        Err(_) => { rprintln!("连接超时"); APP_STATE.store(STATE_AP, Ordering::Relaxed); }
    }
}

/// 断开 STA，恢复为仅 AP 模式
async fn handle_disconnect(controller: &mut WifiController<'static>) {
    rprintln!("断开 STA");
    let config = Config::AccessPointStation(StationConfig::default(), make_ap_config());
    controller.set_config(&config).ok();
    APP_STATE.store(STATE_AP, Ordering::Relaxed);
    critical_section::with(|cs| {
        CONNECTED_SSID.borrow(cs).borrow_mut().clear();
        STA_IP.borrow(cs).borrow_mut().clear();
    });
    RESP.send(WifiResp::Disconnected).await;
}
