//! 共享状态与类型定义 —— 所有任务共用的常量、全局变量、数据结构

use core::cell::RefCell;
use core::sync::atomic::AtomicU8;

use critical_section::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use heapless::String as HString;
use heapless::Vec as HVec;

// ─── 热点配置 ───────────────────────────────────────────────
pub const AP_SSID: &str = "ESP32-Setup";
pub const AP_PASSWORD: &str = "12345678";
pub const HTML: &str = include_str!("../web/index.html"); // 编译时嵌入，不占运行时开销

// ─── 设备状态（原子变量，无锁读写）──────────────────────────
pub const STATE_AP: u8 = 0;         // 等待配网
pub const STATE_CONNECTING: u8 = 1; // 正在连接路由器
pub const STATE_CONNECTED: u8 = 2;  // 已连接
pub static APP_STATE: AtomicU8 = AtomicU8::new(STATE_AP);

// 当前连接的 SSID 和获取到的 IP（Mutex<RefCell<T>> 跨任务共享可变数据）
pub static CONNECTED_SSID: Mutex<RefCell<HString<32>>> = Mutex::new(RefCell::new(HString::new()));
pub static STA_IP: Mutex<RefCell<HString<32>>> = Mutex::new(RefCell::new(HString::new()));

// ─── WiFi 命令/响应通道（容量 1，生产者-消费者）──────────────
pub enum WifiCmd {
    Scan,
    Connect(HString<32>, HString<64>), // (SSID, 密码)
    Disconnect,
}

pub struct ScanAp {
    pub ssid: HString<32>,
    pub rssi: i8,          // 信号强度 dBm（-30 比 -70 强）
    pub channel: u8,
    pub auth: HString<16>, // 加密方式
}

pub enum WifiResp {
    ScanDone(HVec<ScanAp, 20>),
    Connecting,
    ConnectFail(HString<64>),
    Disconnected,
}

pub static CMD: Channel<CriticalSectionRawMutex, WifiCmd, 1> = Channel::new();
pub static RESP: Channel<CriticalSectionRawMutex, WifiResp, 1> = Channel::new();

// ─── DHCP 地址池（192.168.4.10 ~ 192.168.4.50）─────────────
pub const DHCP_START: u8 = 10;
pub const DHCP_END: u8 = 50;

/// DHCP 租约：MAC → IP 映射
#[derive(Clone, Copy, Debug)]
pub struct DhcpLease {
    pub mac: [u8; 6],
    pub ip_last_octet: u8,
}

/// 全局 DHCP 状态（在 critical_section 中访问）
pub struct ClientState {
    pub leases: HVec<DhcpLease, 32>,
    pub next_ip: u8, // 下一个待分配的 IP 末位
}

pub static CLIENT_STATE: Mutex<RefCell<ClientState>> = Mutex::new(RefCell::new(ClientState {
    leases: HVec::new(),
    next_ip: DHCP_START,
}));

/// 创建 'static 全局对象
///
/// embassy 任务需要 'static 引用，此宏在 BSS 段分配空间（不在栈上）。
#[macro_export]
macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        CELL.uninit().write($val)
    }};
}
