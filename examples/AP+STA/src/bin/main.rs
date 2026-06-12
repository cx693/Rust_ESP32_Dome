#![no_main]
#![no_std]

extern crate alloc;

use alloc::string::String;
use core::cell::RefCell;
use core::fmt::Write;
use core::sync::atomic::{AtomicU8, Ordering};

use critical_section::Mutex;
use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{Ipv4Address, Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};
use embedded_io_async::Write as _;
use esp_alloc as _;
use esp_bootloader_esp_idf;
use esp_hal::{
    clock::CpuClock,
    gpio::{Level, Output, OutputConfig},
    interrupt::software::SoftwareInterruptControl,
    rng::Rng,
    timer::timg::TimerGroup,
};
use esp_radio::wifi::{
    ap::AccessPointConfig,
    scan::ScanConfig,
    sta::StationConfig,
    AuthenticationMethod,
    Config,
    ControllerConfig,
    Interface,
    WifiController,
};
use heapless::String as HString;
use heapless::Vec as HVec;
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

esp_bootloader_esp_idf::esp_app_desc!();

const AP_SSID: &str = "ESP32-Setup";
const AP_PASSWORD: &str = "12345678";
const HTML: &str = include_str!("../../web/index.html");

const STATE_AP: u8 = 0;
const STATE_CONNECTING: u8 = 1;
const STATE_CONNECTED: u8 = 2;

static APP_STATE: AtomicU8 = AtomicU8::new(STATE_AP);
static CONNECTED_SSID: Mutex<RefCell<HString<32>>> = Mutex::new(RefCell::new(HString::new()));
static STA_IP: Mutex<RefCell<HString<32>>> = Mutex::new(RefCell::new(HString::new()));

enum WifiCmd {
    Scan,
    Connect(HString<32>, HString<64>),
    Disconnect,
}

struct ScanAp {
    ssid: HString<32>,
    rssi: i8,
    channel: u8,
    auth: HString<16>,
}

enum WifiResp {
    ScanDone(HVec<ScanAp, 20>),
    Connecting,
    ConnectOk,
    ConnectFail(HString<64>),
    Disconnected,
}

static CMD: Channel<CriticalSectionRawMutex, WifiCmd, 1> = Channel::new();
static RESP: Channel<CriticalSectionRawMutex, WifiResp, 1> = Channel::new();

const DHCP_START: u8 = 10;
const DHCP_END: u8 = 50;

#[derive(Clone, Copy, Debug)]
struct DhcpLease {
    mac: [u8; 6],
    ip_last_octet: u8,
}

#[derive(Clone, Debug)]
struct ClientInfo {
    mac: [u8; 6],
    ip_octet: u8,
    hostname: HString<32>,
    connected_at: embassy_time::Instant,
}

struct ClientState {
    leases: HVec<DhcpLease, 32>,
    clients: HVec<ClientInfo, 16>,
    next_ip: u8,
}

static CLIENT_STATE: Mutex<RefCell<ClientState>> = Mutex::new(RefCell::new(ClientState {
    leases: HVec::new(),
    clients: HVec::new(),
    next_ip: DHCP_START,
}));

macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        CELL.uninit().write($val)
    }};
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    rtt_init_print!();
    rprintln!("AP+STA 配网模式启动");

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    esp_alloc::heap_allocator!(size: 128 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    let led = Output::new(peripherals.GPIO48, Level::Low, OutputConfig::default());

    let ap_config = AccessPointConfig::default()
        .with_ssid(AP_SSID)
        .with_password(String::from(AP_PASSWORD))
        .with_auth_method(AuthenticationMethod::Wpa2Personal);

    let sta_config = StationConfig::default();

    let config = Config::AccessPointStation(sta_config, ap_config);

    rprintln!("WiFi 初始化: AP+STA 模式");
    let (controller, interfaces) = esp_radio::wifi::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(config),
    )
    .unwrap();

    let seed = {
        let r = Rng::new();
        (r.random() as u64) << 32 | r.random() as u64
    };

    let (ap_stack, ap_runner) = embassy_net::new(
        interfaces.access_point,
        embassy_net::Config::ipv4_static(StaticConfigV4 {
            address: Ipv4Cidr::new(Ipv4Address::new(192, 168, 4, 1), 24),
            gateway: None,
            dns_servers: Default::default(),
        }),
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        seed,
    );

    let (sta_stack, sta_runner) = embassy_net::new(
        interfaces.station,
        embassy_net::Config::dhcpv4(Default::default()),
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        seed ^ 0xDEADBEEF,
    );

    spawner.spawn(ap_net_task(ap_runner).unwrap());
    spawner.spawn(sta_net_task(sta_runner).unwrap());
    spawner.spawn(dhcp_task(ap_stack).unwrap());
    spawner.spawn(led_task(led).unwrap());
    spawner.spawn(wifi_control_task(controller, sta_stack).unwrap());
    spawner.spawn(http_server_task(ap_stack, "AP").unwrap());
    spawner.spawn(http_server_task(sta_stack, "STA").unwrap());

    rprintln!("所有任务已启动");
    loop {
        Timer::after(Duration::from_secs(3600)).await;
    }
}

#[embassy_executor::task]
async fn ap_net_task(mut runner: Runner<'static, Interface<'static>>) {
    runner.run().await
}

#[embassy_executor::task]
async fn sta_net_task(mut runner: Runner<'static, Interface<'static>>) {
    runner.run().await
}

#[embassy_executor::task]
async fn wifi_control_task(mut controller: WifiController<'static>, sta_stack: Stack<'static>) {
    loop {
        let cmd = CMD.receive().await;
        match cmd {
            WifiCmd::Scan => {
                rprintln!("扫描 Wi-Fi...");
                match controller.scan_async(&ScanConfig::default()).await {
                    Ok(results) => {
                        let mut list: HVec<ScanAp, 20> = HVec::new();
                        for ap in results.iter() {
                            let mut ssid = HString::<32>::new();
                            ssid.push_str(ap.ssid.as_str()).ok();
                            let auth_str = match ap.auth_method {
                                Some(AuthenticationMethod::None) => "Open",
                                Some(AuthenticationMethod::Wpa) => "WPA",
                                Some(AuthenticationMethod::Wpa2Personal) => "WPA2",
                                Some(AuthenticationMethod::WpaWpa2Personal) => "WPA/WPA2",
                                Some(AuthenticationMethod::Wpa3Personal) => "WPA3",
                                Some(AuthenticationMethod::Wpa2Wpa3Personal) => "WPA2/WPA3",
                                _ => "Other",
                            };
                            let mut auth = HString::<16>::new();
                            auth.push_str(auth_str).ok();
                            list.push(ScanAp {
                                ssid,
                                rssi: ap.signal_strength,
                                channel: ap.channel,
                                auth,
                            })
                            .ok();
                        }
                        rprintln!("扫描完成: {} 个网络", list.len());
                        RESP.send(WifiResp::ScanDone(list)).await;
                    }
                    Err(e) => {
                        rprintln!("扫描失败: {:?}", e);
                        RESP.send(WifiResp::ScanDone(HVec::new())).await;
                    }
                }
            }
            WifiCmd::Connect(ssid, password) => {
                rprintln!("连接: {}", ssid.as_str());
                critical_section::with(|cs| {
                    let mut s = CONNECTED_SSID.borrow(cs).borrow_mut();
                    s.clear();
                    write!(s, "{}", ssid.as_str()).ok();
                });
                APP_STATE.store(STATE_CONNECTING, Ordering::Relaxed);

                RESP.send(WifiResp::Connecting).await;

                let new_sta = StationConfig::default()
                    .with_ssid(ssid.as_str())
                    .with_password(String::from(password.as_str()));

                let ap_cfg = AccessPointConfig::default()
                    .with_ssid(AP_SSID)
                    .with_password(String::from(AP_PASSWORD))
                    .with_auth_method(AuthenticationMethod::Wpa2Personal);

                let new_config = Config::AccessPointStation(new_sta, ap_cfg);

                if let Err(e) = controller.set_config(&new_config) {
                    rprintln!("配置失败: {:?}", e);
                    APP_STATE.store(STATE_AP, Ordering::Relaxed);
                    continue;
                }

                let connect_result = embassy_time::with_timeout(
                    Duration::from_secs(20),
                    controller.connect_async(),
                )
                .await;

                match connect_result {
                    Ok(Ok(info)) => {
                        rprintln!("Wi-Fi 已连接: {:?}", info);
                        let ip_str = match embassy_time::with_timeout(
                            Duration::from_secs(15),
                            sta_stack.wait_config_up(),
                        )
                        .await
                        {
                            Ok(()) => sta_stack.config_v4().map(|cfg| {
                                let mut s = HString::<32>::new();
                                write!(s, "{}", cfg.address.address()).ok();
                                s
                            }),
                            Err(_) => None,
                        };
                        if let Some(ref ip) = ip_str {
                            rprintln!("STA IP: {}", ip.as_str());
                            critical_section::with(|cs| {
                                let mut s = STA_IP.borrow(cs).borrow_mut();
                                s.clear();
                                s.push_str(ip.as_str()).ok();
                            });
                        }
                        APP_STATE.store(STATE_CONNECTED, Ordering::Relaxed);
                    }
                    Ok(Err(e)) => {
                        rprintln!("连接失败: {:?}", e);
                        APP_STATE.store(STATE_AP, Ordering::Relaxed);
                    }
                    Err(_) => {
                        rprintln!("连接超时");
                        APP_STATE.store(STATE_AP, Ordering::Relaxed);
                    }
                }
            }
            WifiCmd::Disconnect => {
                rprintln!("断开 STA");
                let ap_cfg = AccessPointConfig::default()
                    .with_ssid(AP_SSID)
                    .with_password(String::from(AP_PASSWORD))
                    .with_auth_method(AuthenticationMethod::Wpa2Personal);
                let new_config = Config::AccessPointStation(StationConfig::default(), ap_cfg);
                controller.set_config(&new_config).ok();
                APP_STATE.store(STATE_AP, Ordering::Relaxed);
                critical_section::with(|cs| {
                    CONNECTED_SSID.borrow(cs).borrow_mut().clear();
                    STA_IP.borrow(cs).borrow_mut().clear();
                });
                RESP.send(WifiResp::Disconnected).await;
            }
        }
    }
}

#[embassy_executor::task]
async fn led_task(mut led: Output<'static>) {
    let mut on = false;
    loop {
        let half_period = match APP_STATE.load(Ordering::Relaxed) {
            STATE_AP => 100u64,
            STATE_CONNECTING => 250,
            STATE_CONNECTED => 1000,
            _ => 100,
        };
        on = !on;
        led.set_level(if on { Level::Low } else { Level::High });
        Timer::after(Duration::from_millis(half_period)).await;
    }
}

#[embassy_executor::task(pool_size = 2)]
async fn http_server_task(stack: Stack<'static>, label: &'static str) {
    stack.wait_config_up().await;
    rprintln!("HTTP [{}] 就绪", label);

    let mut rx_buf = [0u8; 4096];
    let mut tx_buf = [0u8; 4096];

    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
        socket.set_timeout(Some(Duration::from_secs(10)));

        if let Err(e) = socket.accept(80).await {
            rprintln!("HTTP [{}] accept err: {:?}", label, e);
            Timer::after(Duration::from_millis(100)).await;
            continue;
        }

        let req = read_request(&mut socket).await;
        if req.is_empty() {
            socket.close();
            continue;
        }

        let (method, path) = parse_request(&req);
        rprintln!("HTTP [{}] {} {}", label, method, path);

        match (method, path) {
            ("GET", "/") => {
                respond(&mut socket, "200 OK", "text/html; charset=utf-8", HTML.as_bytes()).await;
                socket.flush().await.ok();
                socket.close();
            }
            ("GET", "/api/scan") => {
                CMD.send(WifiCmd::Scan).await;
                let resp = RESP.receive().await;
                match resp {
                    WifiResp::ScanDone(list) => {
                        let json = scan_json(&list);
                        respond(&mut socket, "200 OK", "application/json", json.as_bytes()).await;
                    }
                    _ => {
                        respond(
                            &mut socket,
                            "200 OK",
                            "application/json",
                            b"{\"networks\":[]}",
                        )
                        .await;
                    }
                }
                socket.flush().await.ok();
                socket.close();
            }
            ("POST", "/api/connect") => {
                let body = extract_body(&req);
                if let Some((ssid, password)) = parse_connect_body(body) {
                    CMD.send(WifiCmd::Connect(ssid, password)).await;
                    let resp = RESP.receive().await;
                    match resp {
                        WifiResp::Connecting => {
                            respond(
                                &mut socket,
                                "200 OK",
                                "application/json",
                                b"{\"ok\":true,\"status\":\"connecting\"}",
                            )
                            .await;
                        }
                        WifiResp::ConnectFail(err) => {
                            let mut json = HString::<128>::new();
                            write!(json, "{{\"ok\":false,\"error\":\"{}\"}}", err.as_str()).ok();
                            respond(
                                &mut socket,
                                "200 OK",
                                "application/json",
                                json.as_bytes(),
                            )
                            .await;
                        }
                        _ => {
                            respond(
                                &mut socket,
                                "200 OK",
                                "application/json",
                                b"{\"ok\":false,\"error\":\"unknown\"}",
                            )
                            .await;
                        }
                    }
                } else {
                    respond(
                        &mut socket,
                        "400 Bad Request",
                        "application/json",
                        b"{\"ok\":false,\"error\":\"invalid json\"}",
                    )
                    .await;
                }
                socket.flush().await.ok();
                socket.close();
            }
            ("GET", "/api/status") => {
                let json = status_json();
                respond(&mut socket, "200 OK", "application/json", json.as_bytes()).await;
                socket.flush().await.ok();
                socket.close();
            }
            ("POST", "/api/disconnect") => {
                CMD.send(WifiCmd::Disconnect).await;
                RESP.receive().await;
                respond(
                    &mut socket,
                    "200 OK",
                    "application/json",
                    b"{\"ok\":true}",
                )
                .await;
                socket.flush().await.ok();
                socket.close();
            }
            _ => {
                respond(&mut socket, "404 Not Found", "text/plain", b"404").await;
                socket.flush().await.ok();
                socket.close();
            }
        }
    }
}

#[embassy_executor::task]
async fn dhcp_task(stack: Stack<'static>) {
    let (mut rx_meta, mut tx_meta) = (
        [PacketMetadata::EMPTY; 4],
        [PacketMetadata::EMPTY; 4],
    );
    let (mut rx_buf, mut tx_buf) = ([0u8; 1500], [0u8; 1500]);
    let mut socket =
        UdpSocket::new(stack, &mut rx_meta, &mut rx_buf, &mut tx_meta, &mut tx_buf);
    socket.bind(67).unwrap();
    rprintln!("DHCP 服务器 (端口 67)");

    loop {
        let mut buf = [0u8; 1024];
        let (n, _) = match socket.recv_from(&mut buf).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        let pkt = &buf[..n];
        if pkt.len() < 240 || pkt[0] != 1 || pkt[236..240] != [99, 130, 83, 99] {
            continue;
        }

        let msg_type = match dhcp_opt(pkt, 53) {
            Some(&[t, ..]) => t,
            _ => continue,
        };
        let mac: [u8; 6] = pkt[28..34].try_into().unwrap();
        let xid: [u8; 4] = pkt[4..8].try_into().unwrap();
        let hostname = dhcp_opt(pkt, 12)
            .and_then(|b| core::str::from_utf8(b).ok())
            .unwrap_or("");

        let ip_octet = match msg_type {
            1 => assign_or_get(mac, hostname),
            3 => {
                let req_octet = dhcp_opt(pkt, 50).filter(|o| o.len() >= 4).map(|o| o[3]);
                register_client(mac, req_octet, hostname)
            }
            _ => continue,
        };

        let code = if msg_type == 1 { 2 } else { 5 };
        rprintln!(
            "DHCP {} -> 192.168.4.{}",
            if code == 2 { "Offer" } else { "ACK" },
            ip_octet
        );

        let ip = [192, 168, 4, ip_octet];
        let mut resp = [0u8; 300];
        let len = build_dhcp_resp(&mut resp, code, &xid, &mac, &ip);
        let dest = embassy_net::IpEndpoint::new(
            embassy_net::IpAddress::Ipv4(embassy_net::Ipv4Address::new(255, 255, 255, 255)),
            68,
        );
        socket.send_to(&resp[..len], dest).await.ok();
    }
}

fn assign_or_get(mac: [u8; 6], _hostname: &str) -> u8 {
    critical_section::with(|cs| {
        let state = &mut *CLIENT_STATE.borrow(cs).borrow_mut();
        if let Some(c) = state.leases.iter().find(|l| l.mac == mac) {
            return c.ip_last_octet;
        }
        let octet = state.next_ip;
        state
            .leases
            .push(DhcpLease {
                mac,
                ip_last_octet: octet,
            })
            .ok();
        state.next_ip = if state.next_ip >= DHCP_END {
            DHCP_START
        } else {
            state.next_ip + 1
        };
        octet
    })
}

fn register_client(mac: [u8; 6], req_octet: Option<u8>, hostname: &str) -> u8 {
    critical_section::with(|cs| {
        let state = &mut *CLIENT_STATE.borrow(cs).borrow_mut();
        if let Some(c) = state.leases.iter().find(|l| l.mac == mac) {
            return c.ip_last_octet;
        }
        let octet = req_octet.unwrap_or(state.next_ip);
        state
            .leases
            .push(DhcpLease {
                mac,
                ip_last_octet: octet,
            })
            .ok();
        let mut h = HString::<32>::new();
        h.push_str(hostname).ok();
        state
            .clients
            .push(ClientInfo {
                mac,
                ip_octet: octet,
                hostname: h,
                connected_at: embassy_time::Instant::now(),
            })
            .ok();
        state.next_ip = if state.next_ip >= DHCP_END {
            DHCP_START
        } else {
            state.next_ip + 1
        };
        rprintln!("Client: 192.168.4.{}", octet);
        octet
    })
}

fn build_dhcp_resp(
    buf: &mut [u8],
    msg_type: u8,
    xid: &[u8; 4],
    mac: &[u8; 6],
    ip: &[u8; 4],
) -> usize {
    buf.fill(0);
    buf[0] = 2;
    buf[1] = 1;
    buf[2] = 6;
    buf[4..8].copy_from_slice(xid);
    buf[10] = 0x80;
    buf[12..16].copy_from_slice(ip);
    buf[20..24].copy_from_slice(&[192, 168, 4, 1]);
    buf[28..34].copy_from_slice(mac);
    buf[236..240].copy_from_slice(&[99, 130, 83, 99]);

    let gw = [192u8, 168, 4, 1];
    let opts: &[(u8, &[u8])] = &[
        (53, &[msg_type]),
        (51, &3600u32.to_be_bytes()),
        (1, &[255, 255, 255, 0]),
        (3, &gw),
        (6, &gw),
        (54, &gw),
    ];
    let mut pos = 240;
    for (code, data) in opts {
        buf[pos] = *code;
        buf[pos + 1] = data.len() as u8;
        buf[pos + 2..pos + 2 + data.len()].copy_from_slice(data);
        pos += 2 + data.len();
    }
    buf[pos] = 255;
    pos + 1
}

fn dhcp_opt(pkt: &[u8], code: u8) -> Option<&[u8]> {
    let mut pos = 240;
    while pos + 2 <= pkt.len() {
        match pkt[pos] {
            255 => return None,
            0 => pos += 1,
            c => {
                let len = pkt[pos + 1] as usize;
                if pos + 2 + len > pkt.len() {
                    return None;
                }
                if c == code {
                    return Some(&pkt[pos + 2..pos + 2 + len]);
                }
                pos += 2 + len;
            }
        }
    }
    None
}

async fn read_request(socket: &mut TcpSocket<'_>) -> HString<2048> {
    let mut buf = [0u8; 2048];
    let mut total = 0usize;
    let mut headers_done = false;
    let mut content_length = 0usize;
    let mut header_end = 0usize;

    loop {
        match socket.read(&mut buf[total..]).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                total += n;
                if !headers_done {
                    if let Some(pos) = find_header_end(&buf[..total]) {
                        headers_done = true;
                        header_end = pos + 4;
                        content_length = parse_content_length(&buf[..pos]);
                    }
                }
                if headers_done && total >= header_end + content_length {
                    break;
                }
                if total >= buf.len() {
                    break;
                }
            }
        }
    }

    let mut s = HString::new();
    if let Ok(text) = core::str::from_utf8(&buf[..total]) {
        s.push_str(text).ok();
    }
    s
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    for i in 0..buf.len().saturating_sub(3) {
        if buf[i] == b'\r'
            && buf[i + 1] == b'\n'
            && buf[i + 2] == b'\r'
            && buf[i + 3] == b'\n'
        {
            return Some(i);
        }
    }
    None
}

fn parse_content_length(headers: &[u8]) -> usize {
    let text = match core::str::from_utf8(headers) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    for line in text.split("\r\n") {
        if line.len() > 15 && line[..15].eq_ignore_ascii_case("content-length:") {
            if let Ok(n) = line[15..].trim().parse::<usize>() {
                return n;
            }
        }
    }
    0
}

fn extract_body(request: &str) -> &str {
    if let Some(pos) = request.find("\r\n\r\n") {
        &request[pos + 4..]
    } else {
        ""
    }
}

fn parse_connect_body(body: &str) -> Option<(HString<32>, HString<64>)> {
    let ssid = find_json_string(body, "ssid")?;
    let password = find_json_string(body, "password")?;
    let mut s = HString::<32>::new();
    s.push_str(ssid).ok()?;
    let mut p = HString::<64>::new();
    p.push_str(password).ok()?;
    Some((s, p))
}

fn find_json_string<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle: alloc::string::String = ["\"", key, "\":\""].concat();
    let start = json.find(needle.as_str())? + needle.len();
    let rest = &json[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

async fn respond(socket: &mut TcpSocket<'_>, status: &str, ct: &str, body: &[u8]) {
    let mut header = HString::<256>::new();
    write!(
        header,
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status,
        ct,
        body.len()
    )
    .ok();
    socket.write_all(header.as_bytes()).await.ok();
    socket.write_all(body).await.ok();
}

fn parse_request(req: &str) -> (&str, &str) {
    let mut parts = req.lines().next().unwrap_or("").splitn(3, ' ');
    (parts.next().unwrap_or(""), parts.next().unwrap_or("/"))
}

fn scan_json(results: &HVec<ScanAp, 20>) -> HString<4096> {
    let mut s = HString::<4096>::new();
    write!(s, "{{\"networks\":[").ok();
    for (i, ap) in results.iter().enumerate() {
        if i > 0 {
            s.push(',').ok();
        }
        write!(
            s,
            "{{\"ssid\":\"{}\",\"rssi\":{},\"channel\":{},\"auth\":\"{}\"}}",
            ap.ssid.as_str(),
            ap.rssi,
            ap.channel,
            ap.auth.as_str()
        )
        .ok();
    }
    write!(s, "]}}").ok();
    s
}

fn status_json() -> HString<256> {
    let state = APP_STATE.load(Ordering::Relaxed);
    let mut s = HString::<256>::new();
    match state {
        STATE_AP => {
            write!(
                s,
                "{{\"state\":\"ap\",\"ssid\":\"{}\",\"gateway\":\"192.168.4.1\"}}",
                AP_SSID
            )
            .ok();
        }
        STATE_CONNECTING => {
            let ssid = critical_section::with(|cs| CONNECTED_SSID.borrow(cs).borrow().clone());
            write!(
                s,
                "{{\"state\":\"connecting\",\"ssid\":\"{}\"}}",
                ssid.as_str()
            )
            .ok();
        }
        STATE_CONNECTED => {
            let ssid = critical_section::with(|cs| CONNECTED_SSID.borrow(cs).borrow().clone());
            let ip = critical_section::with(|cs| STA_IP.borrow(cs).borrow().clone());
            write!(
                s,
                "{{\"state\":\"connected\",\"ssid\":\"{}\",\"ip\":\"{}\",\"gateway\":\"192.168.4.1\"}}",
                ssid.as_str(),
                ip.as_str()
            )
            .ok();
        }
        _ => {
            write!(s, "{{\"state\":\"ap\"}}").ok();
        }
    }
    s
}
