//! ST7789 SPI LCD — 5 小球物理模拟
//!
//! 物理模型:
//!   - 重力加速度 g = 1 px/frame²
//!   - 抛物线运动: y = y0 + vy*t + 0.5*g*t²
//!   - 弹性碰撞: 恢复系数 e (动能按 e² 衰减)
//!   - 动量守恒: m1*v1 + m2*v2 = m1*v1' + m2*v2'
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
    COLOR_RGB565_BLACK, FB_SIZE, HEIGHT, WIDTH, St7789,
    fb_clear, fb_fill_circle,
};
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

esp_bootloader_esp_idf::esp_app_desc!();

// ── 物理常量 ─────────────────────────────────────────────────
const BALL_COUNT: usize = 5;
const BALL_RADIUS: i16 = 12;
const GRAVITY: i16 = 1;                 // 重力加速度 (px/frame²)
const MAX_VY: i16 = 16;                 // 最大速度 (防止穿透)
const GROUND_RESTITUTION: i16 = 75;     // 地面恢复系数 (75% 速度保留)
const BALL_RESTITUTION: i16 = 50;       // 球间恢复系数 (50% 速度保留)
const FLOOR_Y: i16 = HEIGHT as i16 - BALL_RADIUS;
const SETTLE_VY: i16 = 1;               // 速度 ≤ 此值视为静止

// ── 伪随机数 ─────────────────────────────────────────────────
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

// ── 小球 ────────────────────────────────────────────────────
struct Ball {
    x: i16,
    y: i16,
    vy: i16,        // 垂直速度 (正=向下)
    color: u16,
    settled: bool,
}

#[main]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("5 小球物理模拟");

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

    static mut FB: [u8; FB_SIZE] = [0u8; FB_SIZE];
    let fb = unsafe { &mut *(&raw mut FB) };

    let mut rng = Rng(12345);
    let mut round: u32 = 0;

    loop {
        round += 1;
        rprintln!("── 第 {} 轮 ──", round);

        let colors = [0xF800, 0xFFE0, 0x07E0, 0x07FF, 0x001F];
        let mut balls: [Ball; BALL_COUNT] = core::array::from_fn(|i| {
            Ball {
                x: (rng.range(WIDTH as u32 - 40) + 20) as i16,
                y: -(rng.range(60) as i16) - (i as i16 * 20),
                vy: 0,
                color: colors[i],
                settled: false,
            }
        });

        loop {
            fb_clear(fb, COLOR_RGB565_BLACK);

            // ── 球间碰撞 (动量守恒 + 恢复系数) ──────────────
            for i in 0..BALL_COUNT {
                for j in (i + 1)..BALL_COUNT {
                    // 两球都静止 → 跳过碰撞
                    if balls[i].settled && balls[j].settled { continue; }

                    let dx = balls[i].x - balls[j].x;
                    let dy = balls[i].y - balls[j].y;
                    let dist_sq = dx * dx + dy * dy;
                    let min_dist = BALL_RADIUS * 2;

                    if dist_sq < min_dist * min_dist && dist_sq > 0 {
                        let vi = balls[i].vy;
                        let vj = balls[j].vy;

                        // 只有相对速度足够大才碰撞
                        let rel_speed = (vi - vj).abs();
                        if rel_speed < SETTLE_VY * 2 {
                            // 速度太小, 直接让快的那个静止
                            if !balls[i].settled && vi.abs() <= SETTLE_VY {
                                balls[i].settled = true;
                                balls[i].vy = 0;
                            }
                            if !balls[j].settled && vj.abs() <= SETTLE_VY {
                                balls[j].settled = true;
                                balls[j].vy = 0;
                            }
                            // 分开位置但不触发碰撞
                            let dist = isqrt(dist_sq as i32);
                            if dist > 0 && dist < min_dist {
                                let overlap = min_dist - dist + 1;
                                let nx = dx * overlap / dist / 2;
                                let ny = dy * overlap / dist / 2;
                                balls[i].x += nx;
                                balls[i].y += ny;
                                balls[j].x -= nx;
                                balls[j].y -= ny;
                            }
                            continue;
                        }

                        // 弹性碰撞
                        balls[i].vy = vj * BALL_RESTITUTION / 100;
                        balls[j].vy = vi * BALL_RESTITUTION / 100;

                        balls[i].settled = false;
                        balls[j].settled = false;

                        // 分开两球 (沿连线方向)
                        let dist = isqrt(dist_sq as i32);
                        if dist > 0 {
                            let overlap = min_dist - dist + 1;
                            let nx = dx * overlap / dist / 2;
                            let ny = dy * overlap / dist / 2;
                            balls[i].x += nx;
                            balls[i].y += ny;
                            balls[j].x -= nx;
                            balls[j].y -= ny;
                        }
                    }
                }
            }

            // ── 更新每个球 (运动学) ─────────────────────────
            for ball in balls.iter_mut() {
                if ball.settled { continue; }

                // v = v + g*t (重力加速)
                ball.vy = (ball.vy + GRAVITY).min(MAX_VY);

                // y = y + v*t (位置更新)
                ball.y += ball.vy;

                // ── 地面碰撞 (弹性势能 → 动能) ─────────────
                if ball.y >= FLOOR_Y {
                    ball.y = FLOOR_Y;

                    // 弹性势能: E = 0.5 * k * x²
                    // 恢复系数 e: v' = e * v
                    // 能量衰减: E' = e² * E
                    if ball.vy.abs() > SETTLE_VY {
                        ball.vy = -(ball.vy * GROUND_RESTITUTION / 100);
                    } else {
                        // 弹性势能耗尽, 静止
                        ball.vy = 0;
                        ball.settled = true;
                        rprintln!("球静止: x={}", ball.x);
                    }
                }

                // 左右环绕
                if ball.x < -BALL_RADIUS { ball.x = WIDTH as i16 + BALL_RADIUS - 1; }
                if ball.x >= WIDTH as i16 + BALL_RADIUS { ball.x = -BALL_RADIUS + 1; }
            }

            // 画球
            for ball in balls.iter() {
                fb_fill_circle(fb, ball.x, ball.y, BALL_RADIUS, ball.color);
            }

            display.flush(fb);

            // 全部静止 → 新一轮
            if balls.iter().all(|b| b.settled) {
                rprintln!("全部静止");
                display.delay.delay_millis(1000);

                // 渐变清屏: 从上到下逐行擦除
                for y in 0..HEIGHT as usize {
                    let row_start = y * WIDTH as usize * 2;
                    for x in 0..WIDTH as usize * 2 {
                        fb[row_start + x] = 0;
                    }
                    if y % 4 == 3 {
                        display.flush(fb);
                    }
                }
                fb_clear(fb, COLOR_RGB565_BLACK);
                display.flush(fb);
                display.delay.delay_millis(300);
                break;
            }
        }
    }
}

/// 整数平方根 (用于距离计算)
fn isqrt(n: i32) -> i16 {
    if n <= 0 { return 0; }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x as i16
}
