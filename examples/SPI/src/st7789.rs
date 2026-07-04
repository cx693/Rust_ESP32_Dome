//! ST7789 SPI LCD 显示驱动
//!
//! 硬件: ST7789V 控制器, 240×240 分辨率, SPI 接口
//! 像素: RGB565 (16bit/pixel, 高字节在前)
//! 数据手册: https://www.displayfuture.com/Display/datasheet/controller/ST7789V.pdf

use esp_hal::{delay::Delay, gpio::Output, spi::master::SpiDmaBus};

// ── 屏幕参数 ─────────────────────────────────────────────────

pub const WIDTH: u16 = 240;
pub const HEIGHT: u16 = 240;

/// 显示区域偏移 (部分 ST7789 模块可见区 ≠ 240×240, 需调整)
const X_OFFSET: u16 = 0;
const Y_OFFSET: u16 = 0;

/// DMA 单次传输上限 (esp32s3 DMA 描述符 15bit 长度)
const DMA_MAX_SIZE: usize = 32736;

/// 帧缓冲区字节数: 宽 × 高 × 2字节/像素
pub const FB_SIZE: usize = (WIDTH as usize) * (HEIGHT as usize) * 2;

// ── ST7789 指令 ──────────────────────────────────────────────

const ST7789_SWRESET: u8 = 0x01; // 软件复位
const ST7789_SLPOUT: u8 = 0x11;  // 退出休眠
const ST7789_NORON: u8 = 0x13;   // 正常显示模式
const ST7789_INVON: u8 = 0x21;   // 颜色反转开
#[allow(dead_code)]
const ST7789_INV_OFF: u8 = 0x20; // 颜色反转关 (备用)
const ST7789_DISPON: u8 = 0x29;  // 开启显示
const ST7789_CASET: u8 = 0x2A;   // 列地址设置
const ST7789_RASET: u8 = 0x2B;   // 行地址设置
const ST7789_RAMWR: u8 = 0x2C;   // 写入显存
const ST7789_MADCTL: u8 = 0x36;  // 显示方向
const ST7789_COLMOD: u8 = 0x3A;  // 像素格式
const ST7789_PORCTRL: u8 = 0xB2; // Porch 控制
const ST7789_GCTRL: u8 = 0xB7;   // Gate 控制
const ST7789_VCOMS: u8 = 0xBB;   // VCOM 电压
const ST7789_LCMCTRL: u8 = 0xC0; // LCM 控制
const ST7789_VDVVRHEN: u8 = 0xC2;// VDV/VRH 使能
const ST7789_VRHS: u8 = 0xC3;    // VRH 电压
const ST7789_VDVS: u8 = 0xC4;    // VDV 电压
const ST7789_FRCTRL2: u8 = 0xC6; // 帧率控制
const ST7789_PWCTRL1: u8 = 0xD0; // 电源控制
const ST7789_PVGAMCTRL: u8 = 0xE0; // 正伽马
const ST7789_NVGAMCTRL: u8 = 0xE1; // 负伽马

// ── RGB565 颜色 ──────────────────────────────────────────────
// 格式: RRRRRGGGGGGBBBBB

pub const COLOR_RGB565_RED: u16 = 0xF800;
pub const COLOR_RGB565_GREEN: u16 = 0x07E0;
pub const COLOR_RGB565_BLUE: u16 = 0x001F;
pub const COLOR_RGB565_YELLOW: u16 = 0xFFE0;
pub const COLOR_RGB565_PURPLE: u16 = 0xF81F;
pub const COLOR_RGB565_BLACK: u16 = 0x0000;
pub const COLOR_RGB565_WHITE: u16 = 0xFFFF;

// ── 驱动结构体 ───────────────────────────────────────────────

/// ST7789 LCD 驱动
///
/// ```no_run
/// let mut display = St7789::new(spi, dc, res).with_blk(blk);
/// display.init();
/// display.flush(&framebuffer); // 刷一帧
/// ```
pub struct St7789<'d> {
    spi: SpiDmaBus<'d, esp_hal::Blocking>,
    dc: Output<'d>,               // 数据/命令选择
    res: Output<'d>,              // 硬件复位
    blk: Option<Output<'d>>,      // 背光 (可选)
    pub delay: Delay,
}

impl<'d> St7789<'d> {
    /// 创建驱动 (spi 需已配置好 DMA)
    pub fn new(spi: SpiDmaBus<'d, esp_hal::Blocking>, dc: Output<'d>, res: Output<'d>) -> Self {
        Self { spi, dc, res, blk: None, delay: Delay::new() }
    }

    /// 链式设置背光引脚
    pub fn with_blk(mut self, blk: Output<'d>) -> Self {
        self.blk = Some(blk);
        self
    }

    // ── SPI 通信 ─────────────────────────────────────────────

    /// 发送命令 (DC=低)
    fn write_cmd(&mut self, cmd: u8) {
        self.dc.set_low();
        self.spi.write(&[cmd]).unwrap();
    }

    /// 发送数据 (DC=高)
    fn write_data(&mut self, data: &[u8]) {
        self.dc.set_high();
        self.spi.write(data).unwrap();
    }

    /// 发送 16bit (大端)
    fn write_word(&mut self, word: u16) {
        self.write_data(&[(word >> 8) as u8, (word & 0xFF) as u8]);
    }

    /// 硬复位: 拉低 RES 20ms → 拉高等待 150ms
    fn hard_reset(&mut self) {
        self.res.set_low();
        self.delay.delay_millis(20);
        self.res.set_high();
        self.delay.delay_millis(150);
    }

    // ── 初始化 ───────────────────────────────────────────────

    /// 初始化屏幕 (使用前必须调用)
    ///
    /// 流程: 背光 → 硬复位 → 软复位 → 退出休眠 → 参数配置 → 开显示
    pub fn init(&mut self) {
        // 背光
        if let Some(ref mut blk) = self.blk {
            blk.set_high();
        }

        self.hard_reset();

        self.write_cmd(ST7789_SWRESET);
        self.delay.delay_millis(200);

        self.write_cmd(ST7789_SLPOUT);
        self.delay.delay_millis(120);

        // 显示方向: 0x00=正常, 0x60=横屏
        self.write_cmd(ST7789_MADCTL);
        self.write_data(&[0x00]);

        // 像素格式: 0x55=RGB565 (16bit)
        self.write_cmd(ST7789_COLMOD);
        self.write_data(&[0x55]);
        self.delay.delay_millis(10);

        // ── 电气参数 (一般不需要改) ──

        self.write_cmd(ST7789_PORCTRL);
        self.write_data(&[0x0C, 0x0C, 0x00, 0x33, 0x33]);

        self.write_cmd(ST7789_GCTRL);
        self.write_data(&[0x35]);

        self.write_cmd(ST7789_VCOMS);
        self.write_data(&[0x19]);

        self.write_cmd(ST7789_LCMCTRL);
        self.write_data(&[0x2C]);

        self.write_cmd(ST7789_VDVVRHEN);
        self.write_data(&[0x01]);

        self.write_cmd(ST7789_VRHS);
        self.write_data(&[0x12]);

        self.write_cmd(ST7789_VDVS);
        self.write_data(&[0x20]);

        // 帧率 ≈60fps
        self.write_cmd(ST7789_FRCTRL2);
        self.write_data(&[0x0F]);

        self.write_cmd(ST7789_PWCTRL1);
        self.write_data(&[0xA4, 0xA1]);

        // 伽马校正
        self.write_cmd(ST7789_PVGAMCTRL);
        self.write_data(&[
            0xD0, 0x04, 0x0D, 0x11, 0x13, 0x2B, 0x3F, 0x54,
            0x4C, 0x18, 0x0D, 0x0B, 0x1F, 0x23,
        ]);
        self.write_cmd(ST7789_NVGAMCTRL);
        self.write_data(&[
            0xD0, 0x04, 0x0C, 0x11, 0x13, 0x2C, 0x3F, 0x44,
            0x51, 0x2F, 0x1F, 0x1F, 0x20, 0x23,
        ]);

        // 颜色反转 (某些屏需要, 不需要可改 INV_OFF)
        self.write_cmd(ST7789_INVON);
        self.delay.delay_millis(10);

        self.write_cmd(ST7789_NORON);
        self.delay.delay_millis(10);

        self.write_cmd(ST7789_DISPON);
        self.delay.delay_millis(120);
    }

    // ── 显示操作 ─────────────────────────────────────────────

    /// 设置绘图窗口, 后续数据自动写入此区域
    pub fn set_window(&mut self, x0: u16, y0: u16, x1: u16, y1: u16) {
        self.write_cmd(ST7789_CASET);
        self.write_word(x0 + X_OFFSET);
        self.write_word(x1 + X_OFFSET);

        self.write_cmd(ST7789_RASET);
        self.write_word(y0 + Y_OFFSET);
        self.write_word(y1 + Y_OFFSET);

        self.write_cmd(ST7789_RAMWR);
    }

    /// 全屏填充单色 (每次写 10 行, 减少 SPI 调用)
    pub fn fill_screen(&mut self, color: u16) {
        self.set_window(0, 0, WIDTH - 1, HEIGHT - 1);
        self.dc.set_high();

        let hi = (color >> 8) as u8;
        let lo = (color & 0xFF) as u8;
        const LINES: usize = 10;
        let mut buf = [0u8; WIDTH as usize * 2 * LINES];
        let mut i = 0;
        for _ in 0..WIDTH as usize * LINES {
            buf[i] = hi;
            buf[i + 1] = lo;
            i += 2;
        }
        for _ in 0..HEIGHT as usize / LINES {
            self.spi.write(&buf).unwrap();
        }
    }

    /// 将帧缓冲区写入屏幕 (按 DMA_MAX_SIZE 分块)
    pub fn flush(&mut self, fb: &[u8]) {
        self.set_window(0, 0, WIDTH - 1, HEIGHT - 1);
        self.dc.set_high();
        for chunk in fb.chunks(DMA_MAX_SIZE) {
            self.spi.write(chunk).unwrap();
        }
    }

    /// 局部刷新: 只写入指定矩形区域
    pub fn flush_region(&mut self, fb: &[u8], x0: u16, y0: u16, x1: u16, y1: u16) {
        if x0 > x1 || y0 > y1 || x1 >= WIDTH || y1 >= HEIGHT {
            return;
        }
        self.set_window(x0, y0, x1, y1);
        self.dc.set_high();
        let row_bytes = (x1 - x0 + 1) as usize * 2;
        for row in y0..=y1 {
            let start = (row as usize * WIDTH as usize + x0 as usize) * 2;
            self.spi.write(&fb[start..start + row_bytes]).unwrap();
        }
    }
}

// ── 帧缓冲区工具 ─────────────────────────────────────────────
// 格式: RGB565, 每像素 2 字节, 大端, 按行排列
// 内存: [px0_hi, px0_lo, px1_hi, px1_lo, ...]

/// 清屏: 用指定颜色填充帧缓冲区
pub fn fb_clear(fb: &mut [u8], color: u16) {
    let hi = (color >> 8) as u8;
    let lo = (color & 0xFF) as u8;
    for chunk in fb.chunks_exact_mut(2) {
        chunk[0] = hi;
        chunk[1] = lo;
    }
}

/// 画点 (越界自动忽略)
pub fn fb_set_pixel(fb: &mut [u8], x: i16, y: i16, color: u16) {
    if x < 0 || y < 0 || x >= WIDTH as i16 || y >= HEIGHT as i16 {
        return;
    }
    let idx = ((y as usize) * (WIDTH as usize) + (x as usize)) * 2;
    fb[idx] = (color >> 8) as u8;
    fb[idx + 1] = (color & 0xFF) as u8;
}

/// 画线: Bresenham 算法 (纯整数运算, 嵌入式友好)
///
/// 参考: https://en.wikipedia.org/wiki/Bresenham%27s_line_algorithm
pub fn fb_draw_line(fb: &mut [u8], x0: i16, y0: i16, x1: i16, y1: i16, color: u16) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx: i16 = if x0 < x1 { 1 } else { -1 };
    let sy: i16 = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut x = x0;
    let mut y = y0;

    loop {
        fb_set_pixel(fb, x, y, color);
        if x == x1 && y == y1 { break; }
        let e2 = 2 * err;
        if e2 >= dy { err += dy; x += sx; }
        if e2 <= dx { err += dx; y += sy; }
    }
}

/// 画填充矩形 (坐标自动裁剪到屏幕范围)
pub fn fb_fill_rect(fb: &mut [u8], x0: u16, y0: u16, x1: u16, y1: u16, color: u16) {
    let hi = (color >> 8) as u8;
    let lo = (color & 0xFF) as u8;
    let w = WIDTH as usize;
    let x1 = x1.min(WIDTH - 1);
    let y1 = y1.min(HEIGHT - 1);

    for y in y0..=y1 {
        let row = y as usize * w;
        for x in x0..=x1 {
            let idx = (row + x as usize) * 2;
            fb[idx] = hi;
            fb[idx + 1] = lo;
        }
    }
}

/// 画填充三角形: 扫描线算法
///
/// 1. 按 Y 排序顶点 (A ≤ B ≤ C)
/// 2. 上半 (A→B) + 下半 (B→C) 分别逐行扫描
/// 3. 线性插值算每行左右边界
pub fn fb_fill_triangle(
    fb: &mut [u8],
    x0: i16, y0: i16, x1: i16, y1: i16, x2: i16, y2: i16, color: u16,
) {
    let (mut ax, mut ay) = (x0, y0);
    let (mut bx, mut by) = (x1, y1);
    let (mut cx, mut cy) = (x2, y2);

    // 冒泡排序: ay ≤ by ≤ cy
    if ay > by { core::mem::swap(&mut ax, &mut bx); core::mem::swap(&mut ay, &mut by); }
    if ay > cy { core::mem::swap(&mut ax, &mut cx); core::mem::swap(&mut ay, &mut cy); }
    if by > cy { core::mem::swap(&mut bx, &mut cx); core::mem::swap(&mut by, &mut cy); }

    if ay == cy { return; } // 三点共线

    let hi = (color >> 8) as u8;
    let lo = (color & 0xFF) as u8;
    let w = WIDTH as usize;
    let (ay32, by32, cy32) = (ay as i32, by as i32, cy as i32);
    let (ax32, bx32, cx32) = (ax as i32, bx as i32, cx as i32);
    let h_ac = cy32 - ay32;

    // 上半: A→B
    if ay < by {
        let h_ab = by32 - ay32;
        for y in ay..by {
            let t = y as i32 - ay32;
            let xl = ax32 + (bx32 - ax32) * t / h_ab;
            let xr = ax32 + (cx32 - ax32) * t / h_ac;
            let (xl, xr) = if xl <= xr { (xl as i16, xr as i16) } else { (xr as i16, xl as i16) };
            if y >= 0 && (y as u16) < HEIGHT {
                let row = y as usize * w;
                for x in xl.max(0) as u16..=(xr as u16).min(WIDTH - 1) {
                    let idx = (row + x as usize) * 2;
                    fb[idx] = hi; fb[idx + 1] = lo;
                }
            }
        }
    }

    // 下半: B→C
    if by < cy {
        let h_bc = cy32 - by32;
        for y in by..=cy {
            let t1 = y as i32 - by32;
            let t2 = y as i32 - ay32;
            let xl = bx32 + (cx32 - bx32) * t1 / h_bc;
            let xr = ax32 + (cx32 - ax32) * t2 / h_ac;
            let (xl, xr) = if xl <= xr { (xl as i16, xr as i16) } else { (xr as i16, xl as i16) };
            if y >= 0 && (y as u16) < HEIGHT {
                let row = y as usize * w;
                for x in xl.max(0) as u16..=(xr as u16).min(WIDTH - 1) {
                    let idx = (row + x as usize) * 2;
                    fb[idx] = hi; fb[idx + 1] = lo;
                }
            }
        }
    }
}
