//! 简易 DHCP 服务器（AP 接口，192.168.4.0/24 网段）
//!
//! DHCP 流程：Discover → Offer → Request → ACK

use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::Stack;
use rtt_target::rprintln;

use super::state::*;

/// DHCP 服务器主任务：监听 UDP 67 端口
pub async fn dhcp_task(stack: Stack<'static>) {
    let (mut rx_meta, mut tx_meta) = ([PacketMetadata::EMPTY; 4], [PacketMetadata::EMPTY; 4]);
    let (mut rx_buf, mut tx_buf) = ([0u8; 1500], [0u8; 1500]);
    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx_buf, &mut tx_meta, &mut tx_buf);
    socket.bind(67).unwrap();
    rprintln!("DHCP 服务器 (端口 67)");

    loop {
        let mut buf = [0u8; 1024];
        let (n, _) = match socket.recv_from(&mut buf).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        let pkt = &buf[..n];

        // 校验：最小长度、BOOTREQUEST(op=1)、Magic Cookie
        if pkt.len() < 240 || pkt[0] != 1 || pkt[236..240] != [99, 130, 83, 99] {
            continue;
        }

        let msg_type = match dhcp_opt(pkt, 53) {
            Some(&[t, ..]) => t,
            _ => continue,
        };
        let mac: [u8; 6] = pkt[28..34].try_into().unwrap();
        let xid: [u8; 4] = pkt[4..8].try_into().unwrap();

        // 1=Discover→分配IP回复Offer, 3=Request→确认回复ACK
        let ip_octet = match msg_type {
            1 => assign_or_get(mac),
            3 => register_client(mac, dhcp_opt(pkt, 50).filter(|o| o.len() >= 4).map(|o| o[3])),
            _ => continue,
        };

        let code = if msg_type == 1 { 2 } else { 5 }; // 2=Offer, 5=ACK
        rprintln!("DHCP {} -> 192.168.4.{}", if code == 2 { "Offer" } else { "ACK" }, ip_octet);

        let ip = [192, 168, 4, ip_octet];
        let mut resp = [0u8; 300];
        let len = build_dhcp_resp(&mut resp, code, &xid, &mac, &ip);
        let dest = embassy_net::IpEndpoint::new(
            embassy_net::IpAddress::Ipv4(embassy_net::Ipv4Address::new(255, 255, 255, 255)), 68,
        );
        socket.send_to(&resp[..len], dest).await.ok();
    }
}

// ─── IP 分配 ────────────────────────────────────────────────

/// 查找已有租约，没有则从地址池分配新 IP
fn assign_or_get(mac: [u8; 6]) -> u8 {
    critical_section::with(|cs| {
        let state = &mut *CLIENT_STATE.borrow(cs).borrow_mut();
        if let Some(c) = state.leases.iter().find(|l| l.mac == mac) {
            return c.ip_last_octet;
        }
        let octet = state.next_ip;
        state.leases.push(DhcpLease { mac, ip_last_octet: octet }).ok();
        state.next_ip = if state.next_ip >= DHCP_END { DHCP_START } else { state.next_ip + 1 };
        octet
    })
}

/// Request 阶段：确认或分配 IP（优先使用客户端请求的地址）
fn register_client(mac: [u8; 6], req_octet: Option<u8>) -> u8 {
    critical_section::with(|cs| {
        let state = &mut *CLIENT_STATE.borrow(cs).borrow_mut();
        if let Some(c) = state.leases.iter().find(|l| l.mac == mac) {
            return c.ip_last_octet;
        }
        // 优先用客户端请求的 IP（必须在合法范围内）
        let use_pool = req_octet.is_none_or(|o| o < DHCP_START || o > DHCP_END);
        let octet = if use_pool { state.next_ip } else { req_octet.unwrap() };
        state.leases.push(DhcpLease { mac, ip_last_octet: octet }).ok();
        if use_pool {
            state.next_ip = if state.next_ip >= DHCP_END { DHCP_START } else { state.next_ip + 1 };
        }
        rprintln!("Client: 192.168.4.{}", octet);
        octet
    })
}

// ─── DHCP 报文 ──────────────────────────────────────────────

/// 构建 DHCP 响应报文
///
/// 报文格式：[0]op [1]htype [2]hlen [4..8]xid [10]flags [12..16]yiaddr
/// [20..24]siaddr [28..34]chaddr [236..240]cookie [240+]options
fn build_dhcp_resp(buf: &mut [u8], msg_type: u8, xid: &[u8; 4], mac: &[u8; 6], ip: &[u8; 4]) -> usize {
    buf.fill(0);
    buf[0] = 2;    // BOOTREPLY
    buf[1] = 1;    // 以太网
    buf[2] = 6;    // MAC 长度
    buf[4..8].copy_from_slice(xid);
    buf[10] = 0x80; // 广播
    buf[12..16].copy_from_slice(ip);
    buf[20..24].copy_from_slice(&[192, 168, 4, 1]); // 网关
    buf[28..34].copy_from_slice(mac);
    buf[236..240].copy_from_slice(&[99, 130, 83, 99]); // Magic Cookie

    // DHCP Options（TLV：Type + Length + Value）
    let gw = [192u8, 168, 4, 1];
    let opts: &[(u8, &[u8])] = &[
        (53, &[msg_type]),            // 消息类型
        (51, &3600u32.to_be_bytes()), // 租约 3600s
        (1, &[255, 255, 255, 0]),     // 子网掩码
        (3, &gw),                     // 网关
        (6, &gw),                     // DNS
        (54, &gw),                    // 服务器标识
    ];
    let mut pos = 240;
    for (code, data) in opts {
        buf[pos] = *code;
        buf[pos + 1] = data.len() as u8;
        buf[pos + 2..pos + 2 + data.len()].copy_from_slice(data);
        pos += 2 + data.len();
    }
    buf[pos] = 255; // End
    pos + 1
}

/// 提取 DHCP option 值（TLV 格式：[code:1][len:1][data:len]）
fn dhcp_opt(pkt: &[u8], code: u8) -> Option<&[u8]> {
    let mut pos = 240;
    while pos + 2 <= pkt.len() {
        match pkt[pos] {
            255 => return None,
            0 => pos += 1,
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
