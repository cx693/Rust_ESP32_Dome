//! ST7789 SPI LCD — 彩色雪花飘落 + 积雪
//!
//! 效果:
//!   1. 彩色雪花从顶部飘落, 落到底部或已有积雪上就停住
//!   2. 积雪逐渐向上堆叠
//!   3. 雪堆到屏幕顶部 → 清屏, 开始新一轮
//!
//! 硬件 (ESP32-S3):
//!   GPIO48→SCK  GPIO47→MOSI  GPIO2→DC  GPIO1→RES  GPIO0→BLK

#![no_main]
#![no_std]

use esp_bootloader_esp_idf;
use esp_hal::{
    dma::{DmaRxBuf, DmaTxBuf},
    dma_buffers,
    gpio::{Level, Output, OutputConfig},
    main,
    spi::{Mode, master::{Config as SpiConfig, Spi}},
    time::Rate,
};
use hello::st7789::{
    COLOR_RGB565_BLACK, FB_SIZE, HEIGHT, St7789, WIDTH, fb_clear, fb_set_pixel,
};
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

esp_bootloader_esp_idf::esp_app_desc!();

// ── 常量 ─────────────────────────────────────────────────────
const SNOW_COUNT: usize = 100;                      // 飘落中雪花数量

// ── 调色板 (12 色高饱和度) ──────────────────────────────────
const PALETTE: [u16; 12] = [
    0xF800, 0xFFE0, 0x07E0, 0x07FF, 0x001F, 0xF81F,
    0xFFFF, 0xFD20, 0xA800, 0x04FF, 0xAFE5, 0xFBE0,
];

// ── 伪随机数 (xorshift32) ──────────────────────────────────
struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    fn range(&mut self, n: u32) -> u32 { self.next() % n }
}

// ── 雪花 ────────────────────────────────────────────────────
struct Flake { x: i16, y: i16, color: u16, speed: u8, drift: i8 }

#[main]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("彩色雪花飘落");

    let peripherals = esp_hal::init(esp_hal::Config::default());

    let dc  = Output::new(peripherals.GPIO2,  Level::Low,  OutputConfig::default());
    let res = Output::new(peripherals.GPIO1,  Level::High, OutputConfig::default());
    let blk = Output::new(peripherals.GPIO0,  Level::High, OutputConfig::default());

    let (rx_buf, rx_desc, tx_buf, tx_desc) = dma_buffers!(32000);
    let spi = Spi::new(
        peripherals.SPI2,
        SpiConfig::default().with_frequency(Rate::from_khz(80000)).with_mode(Mode::_3),
    )
    .unwrap()
    .with_sck(peripherals.GPIO48)
    .with_mosi(peripherals.GPIO47)
    .with_dma(peripherals.DMA_CH0)
    .with_buffers(DmaRxBuf::new(rx_desc, rx_buf).unwrap(), DmaTxBuf::new(tx_desc, tx_buf).unwrap());

    let mut display = St7789::new(spi, dc, res).with_blk(blk);
    display.init();
    rprintln!("屏幕初始化完成");

    // ── 帧缓冲区 (静态分配) ─────────────────────────────────
    static mut FB: [u8; FB_SIZE] = [0u8; FB_SIZE];
    let fb = unsafe { &mut *(&raw mut FB) };

    // ── 积雪位图: 1 bit/像素 ────────────────────────────────
    const SNOW_MAP_LEN: usize = (WIDTH as usize) * (HEIGHT as usize) / 32;
    static mut SNOW_MAP: [u32; SNOW_MAP_LEN] = [0; SNOW_MAP_LEN];
    let snow_map = unsafe { &mut *(&raw mut SNOW_MAP) };

    let mut rng = Rng(54321);
    let mut round: u32 = 0;

    loop {
        round += 1;
        rprintln!("── 第 {} 轮 ──", round);

        // 重置积雪
        snow_map.fill(0);

        // 初始化雪花 (随机散布在下半屏)
        let mut flakes: [Flake; SNOW_COUNT] =
            core::array::from_fn(|_| new_flake(&mut rng, true));

        // ── 单轮主循环 ──────────────────────────────────────
        loop {
            // 1) 清屏 + 重绘积雪
            fb_clear(fb, COLOR_RGB565_BLACK);
            draw_snow_map(fb, snow_map);

            // 2) 更新雪花
            for flake in flakes.iter_mut() {
                // 画十字形 (3×3)
                fb_set_pixel(fb, flake.x, flake.y, flake.color);
                fb_set_pixel(fb, flake.x - 1, flake.y, flake.color);
                fb_set_pixel(fb, flake.x + 1, flake.y, flake.color);
                fb_set_pixel(fb, flake.x, flake.y - 1, flake.color);
                fb_set_pixel(fb, flake.x, flake.y + 1, flake.color);

                // 移动
                let ny = flake.y + flake.speed as i16;
                let nx = flake.x + flake.drift as i16;

                // 随机漂移
                if rng.range(15) == 0 {
                    flake.drift = match rng.range(3) { 0 => -1, 1 => 0, _ => 1 };
                }

                // 碰到积雪 → 停住
                if check_settle(snow_map, nx, ny) {
                    settle_flake(snow_map, flake.x, flake.y);
                    *flake = new_flake(&mut rng, false);
                } else {
                    flake.y = ny;
                    flake.x = nx;
                    // 左右环绕
                    if flake.x < -2 { flake.x = WIDTH as i16 + 1; }
                    if flake.x >= WIDTH as i16 + 2 { flake.x = -1; }
                    // 出底部 → 重新飘入
                    if flake.y >= HEIGHT as i16 + 3 {
                        *flake = new_flake(&mut rng, false);
                    }
                }
            }

            // 3) 刷新
            display.flush(fb);

            // 4) 积雪触顶 → 清屏重来
            if snow_reached_top(snow_map) {
                rprintln!("积雪触顶, 清屏");
                for y in 0..HEIGHT as usize {
                    for x in 0..WIDTH as usize {
                        let idx = (y * WIDTH as usize + x) * 2;
                        fb[idx] = 0;
                        fb[idx + 1] = 0;
                    }
                    if y % 4 == 3 { display.flush(fb); }
                }
                display.delay.delay_millis(500);
                break;
            }
        }
    }
}

// ── 积雪位图操作 ────────────────────────────────────────────

fn is_snow_at(snow_map: &[u32], x: i16, y: i16) -> bool {
    if x < 0 || y < 0 || x >= WIDTH as i16 || y >= HEIGHT as i16 {
        return y >= HEIGHT as i16; // 底部边界视为"有积雪"
    }
    let idx = y as usize * WIDTH as usize + x as usize;
    (snow_map[idx / 32] >> (idx % 32)) & 1 != 0
}

/// 雪堆到最上面 5 行 → 触顶
fn snow_reached_top(snow_map: &[u32]) -> bool {
    for y in 0..5i16 {
        for x in (0..WIDTH as i16).step_by(4) {
            if is_snow_at(snow_map, x, y) { return true; }
        }
    }
    false
}

/// 检查下方是否有积雪 (十字形底部 3 点)
fn check_settle(snow_map: &[u32], x: i16, y: i16) -> bool {
    is_snow_at(snow_map, x - 1, y + 2)
        || is_snow_at(snow_map, x, y + 2)
        || is_snow_at(snow_map, x + 1, y + 2)
}

/// 将雪花的十字形标记到积雪位图
fn settle_flake(snow_map: &mut [u32], x: i16, y: i16) {
    for (px, py) in [(x, y), (x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
        if px >= 0 && py >= 0 && (px as u16) < WIDTH && (py as u16) < HEIGHT {
            let idx = py as usize * WIDTH as usize + px as usize;
            snow_map[idx / 32] |= 1 << (idx % 32);
        }
    }
}

/// 绘制积雪 (4 色分层, 产生彩色效果)
fn draw_snow_map(fb: &mut [u8], snow_map: &[u32]) {
    let w = WIDTH as usize;
    let colors: [u16; 4] = [0xFFFF, 0xFD20, 0x07FF, 0xF81F]; // 白/橙/青/紫
    for word_idx in 0..snow_map.len() {
        let mut word = snow_map[word_idx];
        if word == 0 { continue; }
        let base = word_idx * 32;
        while word != 0 {
            let bit = word.trailing_zeros() as usize;
            word &= !(1 << bit);
            let pixel = base + bit;
            let x = pixel % w;
            let y = pixel / w;
            let color = colors[(y / 8) % colors.len()];
            fb_set_pixel(fb, x as i16, y as i16, color);
        }
    }
}

// ── 雪花生成 ────────────────────────────────────────────────

fn new_flake(rng: &mut Rng, random_y: bool) -> Flake {
    Flake {
        x: rng.range(WIDTH as u32) as i16,
        y: if random_y {
            (rng.range(HEIGHT as u32 / 2) + HEIGHT as u32 / 2) as i16 // 下半屏
        } else {
            -(rng.range(20) as i16) // 屏幕上方飘入
        },
        color: PALETTE[rng.range(PALETTE.len() as u32) as usize],
        speed: (rng.range(3) + 3) as u8, // 3~5 像素/帧
        drift: 0,
    }
}
