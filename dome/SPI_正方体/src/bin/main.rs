#![no_main]
#![no_std]

use core::cell::UnsafeCell;
use esp_bootloader_esp_idf;
use esp_hal::{
    gpio::{Level, Output, OutputConfig},
    main,
    spi::{
        Mode,
        master::{Config as SpiConfig, Spi},
    },
    time::Rate,
};
use hello::st7789::{
    COLOR_RGB565_BLACK, FB_SIZE, St7789,
    fb_draw_line, fb_fill_rect, fb_fill_triangle,
};
use libm::{cosf, sinf};
use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

esp_bootloader_esp_idf::esp_app_desc!();

struct FrameBuffer(UnsafeCell<[u8; FB_SIZE]>);
unsafe impl Sync for FrameBuffer {}

static FB: FrameBuffer = FrameBuffer(UnsafeCell::new([0u8; FB_SIZE]));

const VERTS: [[f32; 3]; 8] = [
    [-1.0, -1.0, -1.0],
    [ 1.0, -1.0, -1.0],
    [ 1.0,  1.0, -1.0],
    [-1.0,  1.0, -1.0],
    [-1.0, -1.0,  1.0],
    [ 1.0, -1.0,  1.0],
    [ 1.0,  1.0,  1.0],
    [-1.0,  1.0,  1.0],
];

const FACES: [[usize; 4]; 6] = [
    [4, 5, 6, 7],
    [1, 0, 3, 2],
    [0, 4, 7, 3],
    [5, 1, 2, 6],
    [0, 1, 5, 4],
    [3, 7, 6, 2],
];

const FACE_COLORS: [u16; 6] = [
    0xF800,
    0x07FF,
    0x07E0,
    0x001F,
    0xFFE0,
    0xF81F,
];

const FACE_NORMALS: [[f32; 3]; 6] = [
    [ 0.0,  0.0,  1.0],
    [ 0.0,  0.0, -1.0],
    [-1.0,  0.0,  0.0],
    [ 1.0,  0.0,  0.0],
    [ 0.0, -1.0,  0.0],
    [ 0.0,  1.0,  0.0],
];

fn rotate(p: [f32; 3], ax: f32, ay: f32) -> [f32; 3] {
    let (sx, cx) = (sinf(ax), cosf(ax));
    let (sy, cy) = (sinf(ay), cosf(ay));
    let y1 = p[1] * cx - p[2] * sx;
    let z1 = p[1] * sx + p[2] * cx;
    let x2 = p[0] * cy + z1 * sy;
    let z2 = -p[0] * sy + z1 * cy;
    [x2, y1, z2]
}

fn project(p: [f32; 3], dist: f32, scale: f32) -> (i16, i16) {
    let z = p[2] + dist;
    if z < 1.0 {
        return (120, 120);
    }
    let x = (p[0] * scale / z) as i16 + 120;
    let y = (p[1] * scale / z) as i16 + 120;
    (x, y)
}

fn bbox(pts: &[(i16, i16); 8]) -> (u16, u16, u16, u16) {
    let (mut x0, mut x1) = (pts[0].0, pts[0].0);
    let (mut y0, mut y1) = (pts[0].1, pts[0].1);
    for p in pts.iter().skip(1) {
        if p.0 < x0 { x0 = p.0; }
        if p.0 > x1 { x1 = p.0; }
        if p.1 < y0 { y0 = p.1; }
        if p.1 > y1 { y1 = p.1; }
    }
    (
        (x0 - 3).max(0) as u16,
        (y0 - 3).max(0) as u16,
        ((x1 + 3).min(239)) as u16,
        ((y1 + 3).min(239)) as u16,
    )
}

#[main]
fn main() -> ! {
    rtt_init_print!();
    rprintln!("Rotating Cube - Filled Faces");

    let peripherals = esp_hal::init(esp_hal::Config::default());

    let dc = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());
    let res = Output::new(peripherals.GPIO1, Level::High, OutputConfig::default());
    let blk = Output::new(peripherals.GPIO0, Level::High, OutputConfig::default());

    let spi = Spi::new(
        peripherals.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_khz(80000))
            .with_mode(Mode::_3),
    )
    .unwrap()
    .with_sck(peripherals.GPIO48)
    .with_mosi(peripherals.GPIO47);

    let mut display = St7789::new(spi, dc, res).with_blk(blk);
    display.init();
    rprintln!("Display ready");

    let fb = unsafe { &mut *FB.0.get() };
    let size: f32 = 70.0;
    let dist: f32 = 250.0;
    let focal: f32 = 200.0;
    let mut ax: f32 = 0.0;
    let mut ay: f32 = 0.0;
    let mut prev_bb: Option<(u16, u16, u16, u16)> = None;

    loop {
        let mut rv = [[0.0f32; 3]; 8];
        let mut rn = [[0.0f32; 3]; 6];
        for (i, v) in VERTS.iter().enumerate() {
            rv[i] = rotate([v[0] * size, v[1] * size, v[2] * size], ax, ay);
        }
        for (i, n) in FACE_NORMALS.iter().enumerate() {
            rn[i] = rotate(*n, ax, ay);
        }

        let mut proj = [(0i16, 0i16); 8];
        for (i, v) in rv.iter().enumerate() {
            proj[i] = project(*v, dist, focal);
        }

        let bb = bbox(&proj);
        let region = if let Some(prev) = prev_bb {
            (
                prev.0.min(bb.0),
                prev.1.min(bb.1),
                prev.2.max(bb.2),
                prev.3.max(bb.3),
            )
        } else {
            (0, 0, 239, 239)
        };

        fb_fill_rect(fb, region.0, region.1, region.2, region.3, COLOR_RGB565_BLACK);

        let mut vis: [(usize, f32); 6] = [(0, 0.0); 6];
        let mut cnt = 0usize;
        for i in 0..6 {
            if rn[i][2] >= 0.0 {
                continue;
            }
            let f = &FACES[i];
            let depth = rv[f[0]][2] + rv[f[1]][2] + rv[f[2]][2] + rv[f[3]][2];
            vis[cnt] = (i, depth);
            cnt += 1;
        }
        for i in 0..cnt {
            for j in i + 1..cnt {
                if vis[j].1 > vis[i].1 {
                    vis.swap(i, j);
                }
            }
        }

        for k in 0..cnt {
            let fi = vis[k].0;
            let f = FACES[fi];
            let c = FACE_COLORS[fi];
            let (ax, ay) = proj[f[0]];
            let (bx, by) = proj[f[1]];
            let (cx, cy) = proj[f[2]];
            let (dx, dy) = proj[f[3]];
            fb_fill_triangle(fb, ax, ay, bx, by, cx, cy, c);
            fb_fill_triangle(fb, ax, ay, cx, cy, dx, dy, c);
            fb_draw_line(fb, ax, ay, bx, by, COLOR_RGB565_BLACK);
            fb_draw_line(fb, bx, by, cx, cy, COLOR_RGB565_BLACK);
            fb_draw_line(fb, cx, cy, dx, dy, COLOR_RGB565_BLACK);
            fb_draw_line(fb, dx, dy, ax, ay, COLOR_RGB565_BLACK);
        }

        display.flush_region(fb, region.0, region.1, region.2, region.3);

        prev_bb = Some(bb);

        ay += 0.04;
        ax += 0.03;
        if ay > 6.2832 {
            ay -= 6.2832;
        }
        if ax > 6.2832 {
            ax -= 6.2832;
        }
    }
}
