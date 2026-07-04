// ============================================================================
// ESP32-S3 WiFi 热点 (AP 模式) 示例
// 功能：创建 WiFi 热点 → DHCP 自动分配 IP → Web 仪表盘展示已连接设备
// 框架：embassy (异步运行时) + esp-hal (硬件抽象层)
// ============================================================================

#![no_main] // 禁用标准 main 入口，由 esp_rtos 接管启动流程
#![no_std]  // 禁用标准库（嵌入式环境没有 OS，不支持 std）

extern crate alloc; // 使用堆分配（esp_alloc 提供）

// ---------- 核心库 ----------
use alloc::string::String;
use core::cell::RefCell;
use core::fmt::Write;
use critical_section::Mutex; // 互斥锁：保护共享数据的并发访问

// ---------- Embassy 异步运行时组件 ----------
use embassy_executor::Spawner;                       // 任务生成器，用于派生异步任务
use embassy_net::udp::{PacketMetadata, UdpSocket};   // UDP 网络通信
use embassy_net::{tcp::TcpSocket, Ipv4Address, Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4};
use embassy_time::{Duration, Instant};               // 异步定时器

use embedded_io_async::Write as _; // 异步写 trait（为 TcpSocket 提供 write_all）

// ---------- ESP 硬件相关 ----------
use esp_alloc as _;                  // 堆内存分配器（必须引入才能使用堆）
use esp_bootloader_esp_idf;         // ESP-IDF 引导加载程序兼容层
use esp_hal::{clock::CpuClock, interrupt::software::SoftwareInterruptControl, rng::Rng, timer::timg::TimerGroup};
use esp_radio::wifi::{ap::AccessPointConfig, AuthenticationMethod, Config, ControllerConfig, Interface, WifiController};
use esp_rtos as _;                   // ESP 实时操作系统（提供异步调度）

// ---------- 工具库 ----------
use heapless::Vec as HVec;           // 固定容量的 Vec（无需堆分配，适合嵌入式）
use panic_rtt_target as _;           // panic 时通过 RTT 输出错误信息
use rtt_target::{rprintln, rtt_init_print}; // RTT 调试打印（类似 println，通过调试器输出）

// 引入 ESP-IDF 应用描述符（固件元信息，烧录时使用）
esp_bootloader_esp_idf::esp_app_desc!();

// 编译时将 web/index.html 嵌入固件，避免文件系统依赖
const HTML: &str = include_str!("../../web/index.html");

// ========== 数据结构 ==========

/// 已连接的 WiFi 客户端信息
#[derive(Clone, Debug)]
struct Client {
    mac: [u8; 6],                      // MAC 地址（硬件唯一标识）
    ip_octet: u8,                      // IP 最后一段，如 192.168.4.{octet}
    hostname: heapless::String<32>,    // 客户端主机名（最长 32 字节）
    connected_at: Instant,             // 连接时间戳（用于计算在线时长）
}

/// 全局客户端列表（最多 16 个设备）
/// Mutex + RefCell 是嵌入式中保护共享状态的标准模式：
///   - Mutex：确保同一时刻只有一个线程能访问
///   - RefCell：运行时借用检查（因为 no_std 环境没有 Arc<Mutex<T>>）
static CLIENTS: Mutex<RefCell<HVec<Client, 16>>> = Mutex::new(RefCell::new(HVec::new()));

/// 在静态内存中分配空间的宏
/// embassy 的任务和网络栈需要 'static 生命周期，此宏将值放入全局 StaticCell 中
macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        CELL.uninit().write($val)
    }};
}

// ========== 程序入口 ==========

/// 主函数 —— 整个程序从这里开始
/// #[esp_rtos::main] 标记这是异步入口，由 ESP-RTOS 调度器驱动
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    rtt_init_print!(); // 初始化 RTT 调试输出

    // ---- 1. 初始化硬件 ----
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    esp_alloc::heap_allocator!(size: 64 * 1024); // 分配 64KB 堆内存

    // ---- 2. 启动 RTOS 调度器 ----
    // TimerGroup 提供时钟源，SoftwareInterrupt 用于任务间唤醒
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    // ---- 3. 创建 WiFi AP ----
    // 配置热点名称 (SSID)、密码和加密方式
    let (controller, interfaces) = esp_radio::wifi::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(Config::AccessPoint(
            AccessPointConfig::default()
                .with_ssid("ESP32-AP")                        // 热点名称
                .with_password(String::from("12345678"))       // 密码（至少 8 位）
                .with_auth_method(AuthenticationMethod::Wpa2Personal), // WPA2 加密
        )),
    )
    .unwrap();

    // ---- 4. 创建 TCP/IP 网络栈 ----
    // 使用随机数种子初始化网络栈，配置静态 IP 192.168.4.1/24
    let seed = { let r = Rng::new(); (r.random() as u64) << 32 | r.random() as u64 };
    let (stack, runner) = embassy_net::new(
        interfaces.access_point, // 使用 AP 接口
        embassy_net::Config::ipv4_static(StaticConfigV4 {
            address: Ipv4Cidr::new(Ipv4Address::new(192, 168, 4, 1), 24), // 本机 IP 和子网
            gateway: None,                               // AP 模式无需网关
            dns_servers: Default::default(),              // 无 DNS 服务
        }),
        mk_static!(StackResources<3>, StackResources::<3>::new()), // 网络栈资源（支持 3 个并发连接）
        seed,
    );

    // ---- 5. 派生三个异步任务 ----
    spawner.spawn(ap_task(controller).unwrap());   // WiFi AP 保活任务
    spawner.spawn(net_task(runner).unwrap());      // 网络栈运行任务
    spawner.spawn(dhcp_task(stack).unwrap());      // DHCP 服务器任务

    // 等待网络栈就绪（获取到 IP 配置）
    stack.wait_config_up().await;
    rprintln!("AP ready: 192.168.4.1");

    // ---- 6. HTTP 服务器主循环 ----
    // 在 80 端口监听 TCP 连接，处理 HTTP 请求
    let (mut rx_buf, mut tx_buf) = ([0u8; 4096], [0u8; 4096]);
    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
        socket.set_timeout(Some(Duration::from_secs(10))); // 10 秒超时
        if socket.accept(80).await.is_err() { continue; }  // 等待客户端连接

        // 读取并解析 HTTP 请求
        let req = read_request(&mut socket).await;
        if req.is_empty() { socket.close(); continue; }
        let (method, path) = parse_request(&req);
        rprintln!("{} {}", method, path);

        // 路由分发：根据请求路径返回不同内容
        let (status, ct, body) = match (method, path) {
            ("GET", "/") => ("200 OK", "text/html; charset=utf-8", HTML.as_bytes()),
            ("GET", "/api/clients") => {
                // 返回已连接设备的 JSON 数据（前端每 2 秒轮询此接口）
                let json = clients_json();
                respond(&mut socket, "200 OK", "application/json", json.as_bytes()).await;
                socket.flush().await.ok();
                socket.close();
                continue;
            }
            _ => ("404 Not Found", "text/plain", b"404" as &[u8]),
        };

        respond(&mut socket, status, ct, body).await;
        socket.flush().await.ok();
        socket.close();
    }
}

// ========== 异步任务 ==========

/// WiFi AP 保活任务 —— 保持热点持续运行
/// 使用 pending() 让任务永久挂起（控制器由 esp_radio 内部管理）
#[embassy_executor::task]
async fn ap_task(_c: WifiController<'static>) {
    core::future::pending::<()>().await;
}

/// 网络栈运行任务 —— 驱动底层数据包收发
/// embassy-net 的 Runner 必须持续 poll 才能处理网络事件
#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface<'static>>) {
    runner.run().await
}

/// DHCP 服务器任务 —— 为连接的设备自动分配 IP 地址
/// DHCP 流程：客户端广播 Discover → 服务器回复 Offer → 客户端 Request → 服务器 ACK
#[embassy_executor::task]
async fn dhcp_task(stack: Stack<'static>) {
    // 创建 UDP socket 并绑定到 67 端口（DHCP 服务器标准端口）
    let (mut rx_meta, mut tx_meta) = ([PacketMetadata::EMPTY; 4], [PacketMetadata::EMPTY; 4]);
    let (mut rx_buf, mut tx_buf) = ([0u8; 1500], [0u8; 1500]);
    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx_buf, &mut tx_meta, &mut tx_buf);
    socket.bind(67).unwrap();
    rprintln!("DHCP on port 67");

    loop {
        // 接收 DHCP 请求包
        let mut buf = [0u8; 1024];
        let (n, _) = match socket.recv_from(&mut buf).await { Ok(v) => v, Err(_) => continue };
        let pkt = &buf[..n];

        // 校验：最小长度 + BOOTREQUEST(op=1) + DHCP 魔数 (99.130.83.99)
        if pkt.len() < 240 || pkt[0] != 1 || pkt[236..240] != [99, 130, 83, 99] { continue; }

        // 从 DHCP 包中提取关键字段
        let msg_type = match dhcp_opt(pkt, 53) { Some(&[t, ..]) => t, _ => continue }; // 消息类型
        let mac: [u8; 6] = pkt[28..34].try_into().unwrap();  // 客户端 MAC
        let xid: [u8; 4] = pkt[4..8].try_into().unwrap();    // 事务 ID（匹配请求和响应）
        let hostname = dhcp_opt(pkt, 12)                      // 客户端主机名
            .and_then(|b| core::str::from_utf8(b).ok())
            .unwrap_or("");

        // 根据消息类型分配或确认 IP
        let ip_octet = match msg_type {
            1 => assign_or_get(mac, hostname),    // Discover：首次分配 IP
            3 => {                                // Request：确认或更新 IP
                let req_octet = dhcp_opt(pkt, 50).filter(|o| o.len() >= 4).map(|o| o[3]);
                register_client(mac, req_octet, hostname)
            }
            _ => continue,
        };

        // 构建 DHCP 响应：Discover → Offer(2)，Request → ACK(5)
        let code = if msg_type == 1 { 2 } else { 5 };
        rprintln!("DHCP {} -> 192.168.4.{}", if code == 2 { "Offer" } else { "ACK" }, ip_octet);

        // 发送 DHCP 响应到广播地址 255.255.255.255:68
        let ip = [192, 168, 4, ip_octet];
        let mut resp = [0u8; 300];
        let len = build_dhcp_resp(&mut resp, code, &xid, &mac, &ip);
        let dest = embassy_net::IpEndpoint::new(
            embassy_net::IpAddress::Ipv4(embassy_net::Ipv4Address::new(255, 255, 255, 255)), 68);
        socket.send_to(&resp[..len], dest).await.ok();
    }
}

// ========== 客户端管理 ==========

/// Discover 阶段：查找已知客户端，不存在则分配新 IP
fn assign_or_get(mac: [u8; 6], _hostname: &str) -> u8 {
    critical_section::with(|cs| {
        let clients = &mut *CLIENTS.borrow(cs).borrow_mut();
        // 已有记录则直接返回之前的 IP
        if let Some(c) = clients.iter().find(|c| c.mac == mac) {
            return c.ip_octet;
        }
        // 新客户端：分配一个未使用的 IP
        let octet = next_ip(clients);
        clients.push(Client { mac, ip_octet: octet, hostname: heapless::String::new(), connected_at: Instant::now() }).ok();
        octet
    })
}

/// Request 阶段：注册或确认客户端，记录主机名
fn register_client(mac: [u8; 6], req_octet: Option<u8>, hostname: &str) -> u8 {
    critical_section::with(|cs| {
        let clients = &mut *CLIENTS.borrow(cs).borrow_mut();
        if let Some(c) = clients.iter().find(|c| c.mac == mac) {
            return c.ip_octet;
        }
        // 优先使用客户端请求的 IP，否则自动分配
        let octet = req_octet.unwrap_or_else(|| next_ip(clients));
        let mut h = heapless::String::<32>::new();
        h.push_str(hostname).ok();
        clients.push(Client { mac, ip_octet: octet, hostname: h, connected_at: Instant::now() }).ok();
        rprintln!("Client: 192.168.4.{}", octet);
        octet
    })
}

/// 在 10~50 范围内找一个未被占用的 IP 尾号
fn next_ip(clients: &HVec<Client, 16>) -> u8 {
    for octet in 10..=50u8 {
        if !clients.iter().any(|c| c.ip_octet == octet) { return octet; }
    }
    10 // 地址池耗尽时回退到 10
}

// ========== DHCP 协议工具函数 ==========

/// 构建 DHCP 响应包（Offer 或 ACK）
///
/// DHCP 包格式（简化）：
///   [0]     op:      操作码 (2=响应)
///   [1]     htype:   硬件类型 (1=以太网)
///   [2]     hlen:    硬件地址长度 (6)
///   [4..8]  xid:     事务 ID（原样回传）
///   [10]    flags:   标志位 (0x80=广播)
///   [12..16] yiaddr: 分配给客户端的 IP
///   [20..24] siaddr: 服务器 IP
///   [28..34] chaddr: 客户端 MAC
///   [236..240] magic: DHCP 魔数
///   [240+]  options: 可变长选项列表
fn build_dhcp_resp(buf: &mut [u8], msg_type: u8, xid: &[u8; 4], mac: &[u8; 6], ip: &[u8; 4]) -> usize {
    buf.fill(0);
    // 固定头部字段
    buf[0] = 2;                          // op: BOOTREPLY
    buf[1] = 1;                          // htype: Ethernet
    buf[2] = 6;                          // hlen: MAC 地址长度
    buf[4..8].copy_from_slice(xid);      // 原样回传事务 ID
    buf[10] = 0x80;                      // flags: 广播响应
    buf[12..16].copy_from_slice(ip);     // yiaddr: 分配的 IP
    buf[20..24].copy_from_slice(&[192, 168, 4, 1]); // siaddr: 服务器 IP
    buf[28..34].copy_from_slice(mac);    // chaddr: 客户端 MAC
    buf[236..240].copy_from_slice(&[99, 130, 83, 99]); // DHCP 魔数

    // DHCP 选项（TLV 格式：Type + Length + Value）
    let gw = [192u8, 168, 4, 1];
    let opts: &[(u8, &[u8])] = &[
        (53, &[msg_type]),               // 消息类型：Offer(2) 或 ACK(5)
        (51, &3600u32.to_be_bytes()),    // 租约时间：3600 秒
        (1, &[255, 255, 255, 0]),        // 子网掩码
        (3, &gw),                        // 默认网关
        (6, &gw),                        // DNS 服务器（用网关地址代替）
        (54, &gw),                       // DHCP 服务器标识
    ];
    let mut pos = 240;
    for (code, data) in opts {
        buf[pos] = *code;
        buf[pos + 1] = data.len() as u8;
        buf[pos + 2..pos + 2 + data.len()].copy_from_slice(data);
        pos += 2 + data.len();
    }
    buf[pos] = 255; // 选项结束标记
    pos + 1         // 返回包总长度
}

/// 从 DHCP 包中提取指定选项的值
///
/// DHCP 选项从偏移 240 开始，TLV 格式：
///   code=255 表示结束
///   code=0   表示填充字节（跳过）
///   其他     len 字节后跟 value
fn dhcp_opt(pkt: &[u8], code: u8) -> Option<&[u8]> {
    let mut pos = 240;
    while pos + 2 <= pkt.len() {
        match pkt[pos] {
            255 => return None,           // 选项结束
            0 => { pos += 1; }           // 填充，跳过 1 字节
            c => {
                let len = pkt[pos + 1] as usize;
                if pos + 2 + len > pkt.len() { return None; }
                if c == code { return Some(&pkt[pos + 2..pos + 2 + len]); } // 找到目标选项
                pos += 2 + len;
            }
        }
    }
    None
}

// ========== HTTP 工具函数 ==========

/// 从 TCP 连接中读取完整的 HTTP 请求头
/// 持续读取直到遇到 \r\n\r\n（HTTP 请求头结束标志）
async fn read_request(socket: &mut TcpSocket<'_>) -> heapless::String<2048> {
    let mut buf = [0u8; 2048];
    let mut total = 0;
    loop {
        match socket.read(&mut buf[total..]).await {
            Ok(0) | Err(_) => break,     // 连接关闭或出错
            Ok(n) => {
                total += n;
                // 检测请求头结束：\r\n\r\n 或缓冲区满
                if (total >= 4 && buf[total - 4..total] == *b"\r\n\r\n") || total >= buf.len() { break; }
            }
        }
    }
    // 将字节转为 UTF-8 字符串
    let mut s = heapless::String::new();
    if let Ok(text) = core::str::from_utf8(&buf[..total]) { s.push_str(text).ok(); }
    s
}

/// 发送 HTTP 响应（状态行 + 响应头 + 响应体）
async fn respond(socket: &mut TcpSocket<'_>, status: &str, ct: &str, body: &[u8]) {
    let mut h = heapless::String::<256>::new();
    // 拼接 HTTP 响应头
    write!(h, "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
           status, ct, body.len()).ok();
    socket.write_all(h.as_bytes()).await.ok(); // 先发头部
    socket.write_all(body).await.ok();         // 再发正文
}

/// 解析 HTTP 请求行，返回 (方法, 路径)
/// 例如 "GET /api/clients HTTP/1.1" → ("GET", "/api/clients")
fn parse_request(req: &str) -> (&str, &str) {
    let mut p = req.lines().next().unwrap_or("").splitn(3, ' ');
    (p.next().unwrap_or(""), p.next().unwrap_or("/"))
}

/// 将已连接客户端列表序列化为 JSON
/// 格式：{"ssid":"...","ip":"...","gateway":"...","clients":[{...},...]}
fn clients_json() -> heapless::String<2048> {
    let mut s = heapless::String::<2048>::new();
    write!(s, "{{\"ssid\":\"ESP32-AP\",\"ip\":\"192.168.4.1\",\"gateway\":\"192.168.4.1\",\"clients\":[").ok();

    critical_section::with(|cs| {
        let clients = &*CLIENTS.borrow(cs).borrow();
        for (i, c) in clients.iter().enumerate() {
            if i > 0 { s.push(',').ok(); }
            let uptime = (Instant::now() - c.connected_at).as_secs(); // 在线时长（秒）
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
