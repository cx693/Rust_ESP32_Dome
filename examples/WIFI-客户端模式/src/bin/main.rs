#![no_main]
#![no_std]

extern crate alloc;

use alloc::string::String;
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, Ordering};

use embassy_executor::Spawner;
use embassy_net::{tcp::TcpSocket, Runner, Stack, StackResources};
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
use esp_radio::wifi::{Config, ControllerConfig, Interface, WifiController, sta::StationConfig};
use heapless::String as HString;
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

esp_bootloader_esp_idf::esp_app_desc!();

const SSID: &str = "Cudy-AC89";
const PASSWORD: &str = "123513as";
const HTML: &str = include_str!("../../web/index.html");
const TZ: u32 = 8 * 3600;

static LED_ON: AtomicBool = AtomicBool::new(false);

macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        CELL.uninit().write($val)
    }};
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    rtt_init_print!();
    rprintln!("ESP32 WiFi STA Mode");

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    let mut led = Output::new(peripherals.GPIO48, Level::Low, OutputConfig::default());

    let station_config = Config::Station(
        StationConfig::default()
            .with_ssid(SSID)
            .with_password(String::from(PASSWORD)),
    );
    rprintln!("Connecting to {}...", SSID);

    let (controller, interfaces) = esp_radio::wifi::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(station_config),
    )
    .unwrap();

    let rng = Rng::new();
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

    let (stack, runner) = embassy_net::new(
        interfaces.station,
        embassy_net::Config::dhcpv4(Default::default()),
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        seed,
    );

    spawner.spawn(connection_task(controller).unwrap());
    spawner.spawn(net_task(runner).unwrap());

    stack.wait_config_up().await;
    if let Some(cfg) = stack.config_v4() {
        rprintln!("Got IP: {}", cfg.address);
    }

    let ntp_ts = sync_ntp(stack).await.unwrap_or(0);
    let sync_instant = Instant::now();
    rprintln!("NTP {}", if ntp_ts > 0 { "ok" } else { "failed" });

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

        let now_ts = ntp_ts.wrapping_add((Instant::now() - sync_instant).as_secs() as u32);

        let (status, content_type, body) = match (method, path) {
            ("GET", "/") => ("200 OK", "text/html; charset=utf-8", HTML.as_bytes()),
            ("GET", "/api/status") => {
                let json = status_json(LED_ON.load(Ordering::Relaxed), now_ts);
                respond(&mut socket, "200 OK", "application/json", json.as_bytes()).await;
                socket.flush().await.ok();
                socket.close();
                continue;
            }
            ("POST", "/api/toggle") => {
                let new = !LED_ON.load(Ordering::Relaxed);
                LED_ON.store(new, Ordering::Relaxed);
                led.set_level(if new { Level::Low } else { Level::High });
                rprintln!("LED {}", if new { "ON" } else { "OFF" });
                let json = status_json(new, now_ts);
                respond(&mut socket, "200 OK", "application/json", json.as_bytes()).await;
                socket.flush().await.ok();
                socket.close();
                continue;
            }
            _ => ("404 Not Found", "text/plain", b"404 Not Found" as &[u8]),
        };

        respond(&mut socket, status, content_type, body).await;
        socket.flush().await.ok();
        socket.close();
    }
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

async fn respond(
    socket: &mut TcpSocket<'_>,
    status: &str,
    content_type: &str,
    body: &[u8],
) {
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

#[embassy_executor::task]
async fn connection_task(mut controller: WifiController<'static>) {
    loop {
        match controller.connect_async().await {
            Ok(info) => {
                rprintln!("Connected to {:?}", info);
                controller.wait_for_disconnect_async().await.ok();
                rprintln!("Disconnected");
            }
            Err(e) => rprintln!("Connection error: {:?}", e),
        }
        Timer::after(Duration::from_secs(5)).await;
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface<'static>>) {
    runner.run().await
}

async fn sync_ntp(stack: Stack<'static>) -> Option<u32> {
    use embassy_net::udp::{PacketMetadata, UdpSocket};

    rprintln!("Syncing NTP...");
    let mut rx_meta = [PacketMetadata::EMPTY; 1];
    let mut tx_meta = [PacketMetadata::EMPTY; 1];
    let mut rx_buf = [0u8; 256];
    let mut tx_buf = [0u8; 256];

    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx_buf, &mut tx_meta, &mut tx_buf);
    socket.bind(0).ok()?;

    let mut pkt = [0u8; 48];
    pkt[0] = 0x1b;

    let server = embassy_net::IpEndpoint::new(
        embassy_net::IpAddress::Ipv4(embassy_net::Ipv4Address::new(162, 159, 200, 123)),
        123,
    );
    socket.send_to(&pkt, server).await.ok()?;

    let mut resp = [0u8; 128];
    match embassy_time::with_timeout(Duration::from_secs(5), socket.recv_from(&mut resp)).await {
        Ok(Ok((n, _))) if n >= 48 => {
            let ts = u32::from_be_bytes([resp[40], resp[41], resp[42], resp[43]])
                .wrapping_sub(2_208_988_800);
            rprintln!("NTP unix: {}", ts);
            Some(ts)
        }
        _ => None,
    }
}

fn parse_request(req: &str) -> (&str, &str) {
    let mut parts = req.lines().next().unwrap_or("").splitn(3, ' ');
    (parts.next().unwrap_or(""), parts.next().unwrap_or("/"))
}

fn status_json(led: bool, ts: u32) -> HString<128> {
    let mut s = HString::<128>::new();
    let led_str = if led { "true" } else { "false" };
    if ts > 0 {
        let local = ts + TZ;
        let (y, m, d) = days_to_ymd(local / 86400);
        let h = (local % 86400) / 3600;
        let min = (local % 3600) / 60;
        let sec = local % 60;
        write!(
            s,
            "{{\"led\":{},\"time\":\"{:02}:{:02}:{:02}\",\"date\":\"{:04}/{:02}/{:02}\"}}",
            led_str, h, min, sec, y, m, d
        )
        .ok();
    } else {
        write!(s, "{{\"led\":{},\"time\":\"--:--:--\",\"date\":\"----/--/--\"}}", led_str).ok();
    }
    s
}

fn days_to_ymd(mut days: u32) -> (u32, u32, u32) {
    let mut year = 1970u32;
    loop {
        let dy = if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 { 366 } else { 365 };
        if days < dy { break; }
        days -= dy;
        year += 1;
    }
    let mdays: [u32; 12] = if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u32;
    for &dm in &mdays {
        if days < dm { break; }
        days -= dm;
        month += 1;
    }
    (year, month, days + 1)
}
