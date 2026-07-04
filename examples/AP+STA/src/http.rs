//! HTTP 服务器 —— 监听 80 端口，处理 Web 配网页面 API
//!
//! GET  /              → HTML 页面
//! GET  /api/scan      → 扫描 Wi-Fi
//! POST /api/connect   → 连接网络
//! GET  /api/status    → 查询状态
//! POST /api/disconnect → 断开

use core::fmt::Write;
use core::sync::atomic::Ordering;

use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use embedded_io_async::Write as _;
use heapless::String as HString;
use heapless::Vec as HVec;
use rtt_target::rprintln;

use super::state::*;

/// HTTP 服务器主任务（AP/STA 各一个实例）
pub async fn http_server_task(stack: Stack<'static>, label: &'static str) {
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
        if req.is_empty() { socket.close(); continue; }

        let (method, path) = parse_request(&req);
        rprintln!("HTTP [{}] {} {}", label, method, path);

        match (method, path) {
            ("GET", "/") => send(&mut socket, "200 OK", "text/html; charset=utf-8", HTML.as_bytes()).await,
            ("GET", "/api/scan") => api_scan(&mut socket).await,
            ("POST", "/api/connect") => api_connect(&mut socket, &req).await,
            ("GET", "/api/status") => api_status(&mut socket).await,
            ("POST", "/api/disconnect") => api_disconnect(&mut socket).await,
            _ => send(&mut socket, "404 Not Found", "text/plain", b"404").await,
        }
        socket.flush().await.ok();
        socket.close();
    }
}

// ─── API 处理 ───────────────────────────────────────────────

async fn api_scan(socket: &mut TcpSocket<'_>) {
    CMD.send(WifiCmd::Scan).await;
    match RESP.receive().await {
        WifiResp::ScanDone(list) => {
            let json = scan_json(&list);
            send(socket, "200 OK", "application/json", json.as_bytes()).await;
        }
        _ => send(socket, "200 OK", "application/json", b"{\"networks\":[]}").await,
    }
}

async fn api_connect(socket: &mut TcpSocket<'_>, req: &str) {
    let body = extract_body(req);
    if let Some((ssid, password)) = parse_connect_body(body) {
        CMD.send(WifiCmd::Connect(ssid, password)).await;
        match RESP.receive().await {
            WifiResp::Connecting => {
                send(socket, "200 OK", "application/json", b"{\"ok\":true,\"status\":\"connecting\"}").await;
            }
            WifiResp::ConnectFail(err) => {
                let mut s = HString::<128>::new();
                write!(s, "{{\"ok\":false,\"error\":\"{}\"}}", err.as_str()).ok();
                send(socket, "200 OK", "application/json", s.as_bytes()).await;
            }
            _ => {
                send(socket, "200 OK", "application/json", b"{\"ok\":false,\"error\":\"unknown\"}").await;
            }
        }
    } else {
        send(socket, "400 Bad Request", "application/json", b"{\"ok\":false,\"error\":\"invalid json\"}").await;
    }
}

async fn api_status(socket: &mut TcpSocket<'_>) {
    let json = status_json();
    send(socket, "200 OK", "application/json", json.as_bytes()).await;
}

async fn api_disconnect(socket: &mut TcpSocket<'_>) {
    CMD.send(WifiCmd::Disconnect).await;
    RESP.receive().await;
    send(socket, "200 OK", "application/json", b"{\"ok\":true}").await;
}

// ─── HTTP 解析 ──────────────────────────────────────────────

/// 读取完整 HTTP 请求（header + body）
async fn read_request(socket: &mut TcpSocket<'_>) -> HString<2048> {
    let mut buf = [0u8; 2048];
    let mut total = 0usize;
    loop {
        match socket.read(&mut buf[total..]).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                total += n;
                if total >= buf.len() { break; }
                // header 读完后，等 body 也读完
                if let Some(pos) = find_header_end(&buf[..total]) {
                    let hdr_end = pos + 4;
                    let cl = parse_content_length(&buf[..pos]);
                    if total >= hdr_end + cl { break; }
                }
            }
        }
    }
    let mut s = HString::new();
    if let Ok(text) = core::str::from_utf8(&buf[..total]) { s.push_str(text).ok(); }
    s
}

/// 查找 \r\n\r\n 位置（header 结尾标记）
fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

/// 从 header 中提取 Content-Length
fn parse_content_length(headers: &[u8]) -> usize {
    let text = match core::str::from_utf8(headers) { Ok(s) => s, Err(_) => return 0 };
    for line in text.split("\r\n") {
        if line.len() > 15 && line[..15].eq_ignore_ascii_case("content-length:") {
            if let Ok(n) = line[15..].trim().parse::<usize>() { return n; }
        }
    }
    0
}

/// 提取 body（header 之后的部分）
fn extract_body(req: &str) -> &str {
    req.find("\r\n\r\n").map_or("", |p| &req[p + 4..])
}

/// 解析请求行 → (方法, 路径)
fn parse_request(req: &str) -> (&str, &str) {
    let mut parts = req.lines().next().unwrap_or("").splitn(3, ' ');
    (parts.next().unwrap_or(""), parts.next().unwrap_or("/"))
}

// ─── JSON ───────────────────────────────────────────────────

/// 扫描结果 → JSON（转义 SSID 中的 " 和 \）
fn scan_json(results: &HVec<ScanAp, 20>) -> HString<4096> {
    let mut s = HString::<4096>::new();
    write!(s, "{{\"networks\":[").ok();
    for (i, ap) in results.iter().enumerate() {
        if i > 0 { s.push(',').ok(); }
        write!(s, "{{\"ssid\":\"").ok();
        for ch in ap.ssid.as_str().chars() {
            match ch {
                '"' => s.push_str("\\\"").ok(),
                '\\' => s.push_str("\\\\").ok(),
                _ => s.push(ch).ok(),
            };
        }
        write!(s, "\",\"rssi\":{},\"channel\":{},\"auth\":\"{}\"}}", ap.rssi, ap.channel, ap.auth.as_str()).ok();
    }
    write!(s, "]}}").ok();
    s
}

/// 设备状态 → JSON（一次临界区读取 SSID + IP，保证一致）
fn status_json() -> HString<256> {
    let mut s = HString::<256>::new();
    match APP_STATE.load(Ordering::Relaxed) {
        STATE_AP => {
            write!(s, "{{\"state\":\"ap\",\"ssid\":\"{}\",\"gateway\":\"192.168.4.1\"}}", AP_SSID).ok();
        }
        STATE_CONNECTING => {
            let ssid = critical_section::with(|cs| CONNECTED_SSID.borrow(cs).borrow().clone());
            write!(s, "{{\"state\":\"connecting\",\"ssid\":\"{}\"}}", ssid.as_str()).ok();
        }
        STATE_CONNECTED => {
            let (ssid, ip) = critical_section::with(|cs| {
                (CONNECTED_SSID.borrow(cs).borrow().clone(), STA_IP.borrow(cs).borrow().clone())
            });
            write!(s, "{{\"state\":\"connected\",\"ssid\":\"{}\",\"ip\":\"{}\",\"gateway\":\"192.168.4.1\"}}",
                ssid.as_str(), ip.as_str()).ok();
        }
        _ => { write!(s, "{{\"state\":\"ap\"}}").ok(); }
    }
    s
}

/// 解析 {"ssid":"...","password":"..."} → (SSID, 密码)
fn parse_connect_body(body: &str) -> Option<(HString<32>, HString<64>)> {
    let ssid = find_json_str(body, "ssid")?;
    let password = find_json_str(body, "password")?;
    let mut s = HString::<32>::new(); s.push_str(ssid).ok()?;
    let mut p = HString::<64>::new(); p.push_str(password).ok()?;
    Some((s, p))
}

/// 从 JSON 中提取 "key":"value" 的 value 部分
fn find_json_str<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    // 构建搜索模式 "key":" 然后找到结束引号
    let needle: alloc::string::String = ["\"", key, "\":\""].concat();
    let start = json.find(needle.as_str())? + needle.len();
    let end = json[start..].find('"')?;
    Some(&json[start..start + end])
}

/// 发送 HTTP 响应
async fn send(socket: &mut TcpSocket<'_>, status: &str, ct: &str, body: &[u8]) {
    let mut hdr = HString::<256>::new();
    write!(hdr, "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status, ct, body.len()).ok();
    socket.write_all(hdr.as_bytes()).await.ok();
    socket.write_all(body).await.ok();
}
