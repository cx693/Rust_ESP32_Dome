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
use embassy_time::{Duration, Instant, Timer};
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
use esp_radio::wifi::{ap::AccessPointConfig, AuthenticationMethod, Config, ControllerConfig, Interface, WifiController};
use heapless::String as HString;
use heapless::Vec as HVec;
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

esp_bootloader_esp_idf::esp_app_desc!();

const AP_SSID: &str = "ESP32-AP";
const AP_PASSWORD: &str = "12345678";
const HTML: &str = include_str!("../../web/index.html");

const DHCP_START_OCTET: u8 = 10;
const DHCP_END_OCTET: u8 = 50;

#[derive(Clone, Copy, Debug)]
struct DhcpLease {
    mac: [u8; 6],
    ip_last_octet: u8,
}

#[derive(Clone, Debug)]
struct ClientInfo {
    mac: [u8; 6],
    ip: [u8; 4],
    hostname: HString<32>,
    connected_at: Instant,
}

struct ClientState {
    leases: HVec<DhcpLease, 32>,
    clients: HVec<ClientInfo, 16>,
    next_ip: u8,
}

static CLIENT_STATE: Mutex<RefCell<ClientState>> = Mutex::new(RefCell::new(ClientState {
    leases: HVec::new(),
    clients: HVec::new(),
    next_ip: DHCP_START_OCTET,
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
    rprintln!("ESP32 WiFi AP Mode");

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    let _led = Output::new(peripherals.GPIO48, Level::Low, OutputConfig::default());

    let ap_config = Config::AccessPoint(
        AccessPointConfig::default()
            .with_ssid(AP_SSID)
            .with_password(String::from(AP_PASSWORD))
            .with_auth_method(AuthenticationMethod::Wpa2Personal),
    );
    rprintln!("Starting AP: {}", AP_SSID);

    let (controller, interfaces) = esp_radio::wifi::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(ap_config),
    )
    .unwrap();

    let rng = Rng::new();
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

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
    if let Some(cfg) = stack.config_v4() {
        rprintln!("AP IP: {}", cfg.address);
    }

    rprintln!("HTTP server on port 80");
    let mut rx_buf = [0u8; 4096];
    let mut tx_buf = [0u8; 4096];

    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
        socket.set_timeout(Some(Duration::from_secs(10)));

        if let Err(e) = socket.accept(80).await {
            rprintln!("Accept error: {:?}", e);
            continue;
        }

        let req = read_request(&mut socket).await;
        if req.is_empty() {
            socket.close();
            continue;
        }

        let (method, path) = parse_request(&req);
        rprintln!("{} {}", method, path);

        match (method, path) {
            ("GET", "/") => {
                respond(&mut socket, "200 OK", "text/html; charset=utf-8", HTML.as_bytes()).await;
                socket.flush().await.ok();
                socket.close();
            }
            ("GET", "/api/clients") => {
                let json = clients_json();
                respond(&mut socket, "200 OK", "application/json", json.as_bytes()).await;
                socket.flush().await.ok();
                socket.close();
            }
            _ => {
                respond(&mut socket, "404 Not Found", "text/plain", b"404 Not Found" as &[u8]).await;
                socket.flush().await.ok();
                socket.close();
            }
        }
    }
}

#[embassy_executor::task]
async fn ap_task(controller: WifiController<'static>) {
    loop {
        match controller.wait_for_access_point_connected_event_async().await {
            Ok(event) => {
                rprintln!("AP event: {:?}", event);
            }
            Err(e) => {
                rprintln!("AP event error: {:?}", e);
                Timer::after(Duration::from_secs(1)).await;
            }
        }
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface<'static>>) {
    runner.run().await
}

#[embassy_executor::task]
async fn dhcp_task(stack: Stack<'static>) {
    let mut rx_meta = [PacketMetadata::EMPTY; 4];
    let mut tx_meta = [PacketMetadata::EMPTY; 4];
    let mut rx_buf = [0u8; 1500];
    let mut tx_buf = [0u8; 1500];

    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx_buf, &mut tx_meta, &mut tx_buf);
    socket.bind(67).unwrap();
    rprintln!("DHCP server listening on port 67");

    loop {
        let mut recv_data = [0u8; 1024];
        let (n, _src) = match socket.recv_from(&mut recv_data).await {
            Ok(v) => v,
            Err(_) => continue,
        };

        if n < 240 {
            continue;
        }

        let pkt = &recv_data[..n];

        if pkt[0] != 1 {
            continue;
        }

        if pkt[236..240] != [99, 130, 83, 99] {
            continue;
        }

        let msg_type = match find_dhcp_option(pkt, 53) {
            Some(v) if !v.is_empty() => v[0],
            _ => continue,
        };

        let mut client_mac = [0u8; 6];
        client_mac.copy_from_slice(&pkt[28..34]);

        let mut hostname = HString::<32>::new();
        if let Some(name_bytes) = find_dhcp_option(pkt, 12) {
            if let Ok(name_str) = core::str::from_utf8(name_bytes) {
                hostname.push_str(name_str).ok();
            }
        }

        let requested_ip = find_dhcp_option(pkt, 50).and_then(|opt| {
            if opt.len() >= 4 {
                Some([opt[0], opt[1], opt[2], opt[3]])
            } else {
                None
            }
        });

        let xid = [pkt[4], pkt[5], pkt[6], pkt[7]];

        match msg_type {
            1 => {
                rprintln!(
                    "DHCP Discover from {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                    client_mac[0], client_mac[1], client_mac[2],
                    client_mac[3], client_mac[4], client_mac[5]
                );

                let ip_octet = critical_section::with(|cs| {
                    let state = &mut *CLIENT_STATE.borrow(cs).borrow_mut();
                    if let Some(lease) = state.leases.iter().find(|l| l.mac == client_mac) {
                        lease.ip_last_octet
                    } else {
                        let octet = state.next_ip;
                        state.leases.push(DhcpLease { mac: client_mac, ip_last_octet: octet }).ok();
                        state.next_ip = if state.next_ip >= DHCP_END_OCTET {
                            DHCP_START_OCTET
                        } else {
                            state.next_ip + 1
                        };
                        octet
                    }
                });

                let offer_ip = [192, 168, 4, ip_octet];
                let mut resp = [0u8; 300];
                let len = build_dhcp_response(&mut resp, 2, &xid, &client_mac, &offer_ip);

                let dest = embassy_net::IpEndpoint::new(
                    embassy_net::IpAddress::Ipv4(embassy_net::Ipv4Address::new(255, 255, 255, 255)),
                    68,
                );
                socket.send_to(&resp[..len], dest).await.ok();
            }
            3 => {
                rprintln!(
                    "DHCP Request from {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                    client_mac[0], client_mac[1], client_mac[2],
                    client_mac[3], client_mac[4], client_mac[5]
                );

                let ip_octet = critical_section::with(|cs| {
                    let state = &mut *CLIENT_STATE.borrow(cs).borrow_mut();

                    if let Some(req) = requested_ip {
                        let octet = req[3];
                        if let Some(lease) = state.leases.iter_mut().find(|l| l.mac == client_mac) {
                            lease.ip_last_octet = octet;
                        } else {
                            state.leases.push(DhcpLease { mac: client_mac, ip_last_octet: octet }).ok();
                        }
                        if !state.clients.iter().any(|c| c.mac == client_mac) {
                            state.clients.push(ClientInfo {
                                mac: client_mac,
                                ip: [192, 168, 4, octet],
                                hostname,
                                connected_at: Instant::now(),
                            }).ok();
                            rprintln!("Client connected: 192.168.4.{}", octet);
                        }
                        octet
                    } else if let Some(lease) = state.leases.iter().find(|l| l.mac == client_mac) {
                        let octet = lease.ip_last_octet;
                        if !state.clients.iter().any(|c| c.mac == client_mac) {
                            state.clients.push(ClientInfo {
                                mac: client_mac,
                                ip: [192, 168, 4, octet],
                                hostname,
                                connected_at: Instant::now(),
                            }).ok();
                            rprintln!("Client connected: 192.168.4.{}", octet);
                        }
                        octet
                    } else {
                        let octet = state.next_ip;
                        state.leases.push(DhcpLease { mac: client_mac, ip_last_octet: octet }).ok();
                        state.next_ip = if state.next_ip >= DHCP_END_OCTET {
                            DHCP_START_OCTET
                        } else {
                            state.next_ip + 1
                        };
                        state.clients.push(ClientInfo {
                            mac: client_mac,
                            ip: [192, 168, 4, octet],
                            hostname,
                            connected_at: Instant::now(),
                        }).ok();
                        rprintln!("Client connected: 192.168.4.{}", octet);
                        octet
                    }
                });

                let ack_ip = [192, 168, 4, ip_octet];
                let mut resp = [0u8; 300];
                let len = build_dhcp_response(&mut resp, 5, &xid, &client_mac, &ack_ip);

                let dest = embassy_net::IpEndpoint::new(
                    embassy_net::IpAddress::Ipv4(embassy_net::Ipv4Address::new(255, 255, 255, 255)),
                    68,
                );
                socket.send_to(&resp[..len], dest).await.ok();
            }
            _ => {}
        }
    }
}

fn build_dhcp_response(
    buf: &mut [u8],
    msg_type: u8,
    xid: &[u8; 4],
    client_mac: &[u8; 6],
    your_ip: &[u8; 4],
) -> usize {
    buf.fill(0);

    buf[0] = 2;
    buf[1] = 1;
    buf[2] = 6;
    buf[4..8].copy_from_slice(xid);
    buf[10] = 0x80;
    buf[12..16].copy_from_slice(your_ip);
    buf[20..24].copy_from_slice(&[192, 168, 4, 1]);
    buf[28..34].copy_from_slice(client_mac);
    buf[236..240].copy_from_slice(&[99, 130, 83, 99]);

    let mut pos = 240;

    buf[pos] = 53;
    buf[pos + 1] = 1;
    buf[pos + 2] = msg_type;
    pos += 3;

    buf[pos] = 51;
    buf[pos + 1] = 4;
    buf[pos + 2..pos + 6].copy_from_slice(&3600u32.to_be_bytes());
    pos += 6;

    buf[pos] = 1;
    buf[pos + 1] = 4;
    buf[pos + 2..pos + 6].copy_from_slice(&[255, 255, 255, 0]);
    pos += 6;

    buf[pos] = 3;
    buf[pos + 1] = 4;
    buf[pos + 2..pos + 6].copy_from_slice(&[192, 168, 4, 1]);
    pos += 6;

    buf[pos] = 6;
    buf[pos + 1] = 4;
    buf[pos + 2..pos + 6].copy_from_slice(&[192, 168, 4, 1]);
    pos += 6;

    buf[pos] = 54;
    buf[pos + 1] = 4;
    buf[pos + 2..pos + 6].copy_from_slice(&[192, 168, 4, 1]);
    pos += 6;

    buf[pos] = 255;
    pos += 1;

    pos
}

fn find_dhcp_option(pkt: &[u8], code: u8) -> Option<&[u8]> {
    let mut pos = 240;
    while pos + 2 <= pkt.len() {
        let opt_code = pkt[pos];
        if opt_code == 255 {
            return None;
        }
        if opt_code == 0 {
            pos += 1;
            continue;
        }
        let opt_len = pkt[pos + 1] as usize;
        if pos + 2 + opt_len > pkt.len() {
            return None;
        }
        if opt_code == code {
            return Some(&pkt[pos + 2..pos + 2 + opt_len]);
        }
        pos += 2 + opt_len;
    }
    None
}

async fn read_request(socket: &mut TcpSocket<'_>) -> HString<2048> {
    let mut buf = [0u8; 2048];
    let mut total = 0usize;
    loop {
        match socket.read(&mut buf[total..]).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                total += n;
                if total >= 4 && buf[total - 4..total] == *b"\r\n\r\n" || total >= buf.len() {
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

async fn respond(socket: &mut TcpSocket<'_>, status: &str, content_type: &str, body: &[u8]) {
    let mut header = HString::<256>::new();
    write!(
        header,
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status,
        content_type,
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

fn clients_json() -> HString<2048> {
    let mut s = HString::<2048>::new();
    write!(
        s,
        "{{\"ssid\":\"{}\",\"ip\":\"192.168.4.1\",\"gateway\":\"192.168.4.1\",\"clients\":[",
        AP_SSID
    )
    .ok();

    let clients_snapshot: HVec<(HString<32>, [u8; 6], [u8; 4], Instant), 16> = critical_section::with(|cs| {
        let state = &*CLIENT_STATE.borrow(cs).borrow();
        let mut out = HVec::new();
        for c in state.clients.iter() {
            out.push((c.hostname.clone(), c.mac, c.ip, c.connected_at)).ok();
        }
        out
    });

    for (i, (hostname, mac, ip, connected_at)) in clients_snapshot.iter().enumerate() {
        if i > 0 {
            s.push(',').ok();
        }
        let hostname_str = if hostname.is_empty() {
            ""
        } else {
            hostname.as_str()
        };
        let client_uptime = (Instant::now() - *connected_at).as_secs();
        write!(
            s,
            "{{\"mac\":\"{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\",\"ip\":\"{}.{}.{}.{}\",\"hostname\":\"{}\",\"uptime\":{}}}",
            mac[0], mac[1], mac[2],
            mac[3], mac[4], mac[5],
            ip[0], ip[1], ip[2], ip[3],
            hostname_str,
            client_uptime,
        )
        .ok();
    }

    s.push_str("]}").ok();
    s
}
