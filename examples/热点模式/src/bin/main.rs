#![no_main]
#![no_std]

extern crate alloc;

use alloc::string::String;
use core::cell::RefCell;
use core::fmt::Write;
use critical_section::Mutex;
use embassy_executor::Spawner;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{tcp::TcpSocket, Ipv4Address, Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4};
use embassy_time::{Duration, Instant};
use embedded_io_async::Write as _;
use esp_alloc as _;
use esp_bootloader_esp_idf;
use esp_hal::{clock::CpuClock, interrupt::software::SoftwareInterruptControl, rng::Rng, timer::timg::TimerGroup};
use esp_radio::wifi::{ap::AccessPointConfig, AuthenticationMethod, Config, ControllerConfig, Interface, WifiController};
use heapless::Vec as HVec;
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

esp_bootloader_esp_idf::esp_app_desc!();

const HTML: &str = include_str!("../../web/index.html");

#[derive(Clone, Debug)]
struct Client {
    mac: [u8; 6],
    ip_octet: u8,
    hostname: heapless::String<32>,
    connected_at: Instant,
}

static CLIENTS: Mutex<RefCell<HVec<Client, 16>>> = Mutex::new(RefCell::new(HVec::new()));

macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        CELL.uninit().write($val)
    }};
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    rtt_init_print!();
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    let (controller, interfaces) = esp_radio::wifi::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(Config::AccessPoint(
            AccessPointConfig::default()
                .with_ssid("ESP32-AP")
                .with_password(String::from("12345678"))
                .with_auth_method(AuthenticationMethod::Wpa2Personal),
        )),
    )
    .unwrap();

    let seed = { let r = Rng::new(); (r.random() as u64) << 32 | r.random() as u64 };
    let (stack, runner) = embassy_net::new(
        interfaces.access_point,
        embassy_net::Config::ipv4_static(StaticConfigV4 {
            address: Ipv4Cidr::new(Ipv4Address::new(192, 168, 4, 1), 24),
            gateway: None,
            dns_servers: Default::default(),
        }),
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        seed,
    );

    spawner.spawn(ap_task(controller).unwrap());
    spawner.spawn(net_task(runner).unwrap());
    spawner.spawn(dhcp_task(stack).unwrap());

    stack.wait_config_up().await;
    rprintln!("AP ready: 192.168.4.1");

    let (mut rx_buf, mut tx_buf) = ([0u8; 4096], [0u8; 4096]);
    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
        socket.set_timeout(Some(Duration::from_secs(10)));
        if socket.accept(80).await.is_err() { continue; }

        let req = read_request(&mut socket).await;
        if req.is_empty() { socket.close(); continue; }

        let (method, path) = parse_request(&req);
        rprintln!("{} {}", method, path);

        let (status, ct, body) = match (method, path) {
            ("GET", "/") => ("200 OK", "text/html; charset=utf-8", HTML.as_bytes()),
            ("GET", "/api/clients") => {
                let json = clients_json();
                respond(&mut socket, "200 OK", "application/json", json.as_bytes()).await;
                socket.flush().await.ok(); socket.close(); continue;
            }
            _ => ("404 Not Found", "text/plain", b"404" as &[u8]),
        };
        respond(&mut socket, status, ct, body).await;
        socket.flush().await.ok(); socket.close();
    }
}

#[embassy_executor::task]
async fn ap_task(_c: WifiController<'static>) {
    core::future::pending::<()>().await;
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface<'static>>) {
    runner.run().await
}

#[embassy_executor::task]
async fn dhcp_task(stack: Stack<'static>) {
    let (mut rx_meta, mut tx_meta) = ([PacketMetadata::EMPTY; 4], [PacketMetadata::EMPTY; 4]);
    let (mut rx_buf, mut tx_buf) = ([0u8; 1500], [0u8; 1500]);
    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx_buf, &mut tx_meta, &mut tx_buf);
    socket.bind(67).unwrap();
    rprintln!("DHCP on port 67");

    loop {
        let mut buf = [0u8; 1024];
        let (n, _) = match socket.recv_from(&mut buf).await { Ok(v) => v, Err(_) => continue };
        let pkt = &buf[..n];

        if pkt.len() < 240 || pkt[0] != 1 || pkt[236..240] != [99, 130, 83, 99] { continue; }

        let msg_type = match dhcp_opt(pkt, 53) { Some(&[t, ..]) => t, _ => continue };
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
        rprintln!("DHCP {} -> 192.168.4.{}", if code == 2 { "Offer" } else { "ACK" }, ip_octet);

        let ip = [192, 168, 4, ip_octet];
        let mut resp = [0u8; 300];
        let len = build_dhcp_resp(&mut resp, code, &xid, &mac, &ip);
        let dest = embassy_net::IpEndpoint::new(
            embassy_net::IpAddress::Ipv4(embassy_net::Ipv4Address::new(255, 255, 255, 255)), 68);
        socket.send_to(&resp[..len], dest).await.ok();
    }
}

fn assign_or_get(mac: [u8; 6], _hostname: &str) -> u8 {
    critical_section::with(|cs| {
        let clients = &mut *CLIENTS.borrow(cs).borrow_mut();
        if let Some(c) = clients.iter().find(|c| c.mac == mac) {
            return c.ip_octet;
        }
        let octet = next_ip(clients);
        clients.push(Client { mac, ip_octet: octet, hostname: heapless::String::new(), connected_at: Instant::now() }).ok();
        octet
    })
}

fn register_client(mac: [u8; 6], req_octet: Option<u8>, hostname: &str) -> u8 {
    critical_section::with(|cs| {
        let clients = &mut *CLIENTS.borrow(cs).borrow_mut();
        if let Some(c) = clients.iter().find(|c| c.mac == mac) {
            return c.ip_octet;
        }
        let octet = req_octet.unwrap_or_else(|| next_ip(clients));
        let mut h = heapless::String::<32>::new();
        h.push_str(hostname).ok();
        clients.push(Client { mac, ip_octet: octet, hostname: h, connected_at: Instant::now() }).ok();
        rprintln!("Client: 192.168.4.{}", octet);
        octet
    })
}

fn next_ip(clients: &HVec<Client, 16>) -> u8 {
    for octet in 10..=50u8 {
        if !clients.iter().any(|c| c.ip_octet == octet) { return octet; }
    }
    10
}

fn build_dhcp_resp(buf: &mut [u8], msg_type: u8, xid: &[u8; 4], mac: &[u8; 6], ip: &[u8; 4]) -> usize {
    buf.fill(0);
    buf[0] = 2; buf[1] = 1; buf[2] = 6;
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
        (3, &gw), (6, &gw), (54, &gw),
    ];
    let mut pos = 240;
    for (code, data) in opts {
        buf[pos] = *code; buf[pos + 1] = data.len() as u8;
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
            0 => { pos += 1; }
            c => {
                let len = pkt[pos + 1] as usize;
                if pos + 2 + len > pkt.len() { return None; }
                if c == code { return Some(&pkt[pos + 2..pos + 2 + len]); }
                pos += 2 + len;
            }
        }
    }
    None
}

async fn read_request(socket: &mut TcpSocket<'_>) -> heapless::String<2048> {
    let mut buf = [0u8; 2048];
    let mut total = 0;
    loop {
        match socket.read(&mut buf[total..]).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                total += n;
                if (total >= 4 && buf[total - 4..total] == *b"\r\n\r\n") || total >= buf.len() { break; }
            }
        }
    }
    let mut s = heapless::String::new();
    if let Ok(text) = core::str::from_utf8(&buf[..total]) { s.push_str(text).ok(); }
    s
}

async fn respond(socket: &mut TcpSocket<'_>, status: &str, ct: &str, body: &[u8]) {
    let mut h = heapless::String::<256>::new();
    write!(h, "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", status, ct, body.len()).ok();
    socket.write_all(h.as_bytes()).await.ok();
    socket.write_all(body).await.ok();
}

fn parse_request(req: &str) -> (&str, &str) {
    let mut p = req.lines().next().unwrap_or("").splitn(3, ' ');
    (p.next().unwrap_or(""), p.next().unwrap_or("/"))
}

fn clients_json() -> heapless::String<2048> {
    let mut s = heapless::String::<2048>::new();
    write!(s, "{{\"ssid\":\"ESP32-AP\",\"ip\":\"192.168.4.1\",\"gateway\":\"192.168.4.1\",\"clients\":[").ok();

    critical_section::with(|cs| {
        let clients = &*CLIENTS.borrow(cs).borrow();
        for (i, c) in clients.iter().enumerate() {
            if i > 0 { s.push(',').ok(); }
            let uptime = (Instant::now() - c.connected_at).as_secs();
            let hn = if c.hostname.is_empty() { "" } else { c.hostname.as_str() };
            write!(s,
                "{{\"mac\":\"{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\",\"ip\":\"192.168.4.{}\",\"hostname\":\"{}\",\"uptime\":{}}}",
                c.mac[0], c.mac[1], c.mac[2], c.mac[3], c.mac[4], c.mac[5], c.ip_octet, hn, uptime
            ).ok();
        }
    });

    s.push_str("]}").ok();
    s
}
