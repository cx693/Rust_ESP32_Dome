use esp_hal::{
    delay::Delay,
    gpio::Output,
    spi::master::Spi,
};

pub const WIDTH: u16 = 240;
pub const HEIGHT: u16 = 240;

const X_OFFSET: u16 = 0;
const Y_OFFSET: u16 = 0;

const ST7789_SWRESET: u8 = 0x01;
const ST7789_SLPOUT: u8 = 0x11;
const ST7789_NORON: u8 = 0x13;
const ST7789_INVON: u8 = 0x21;
const ST7789_INV_OFF: u8 = 0x20;
const ST7789_DISPON: u8 = 0x29;
const ST7789_CASET: u8 = 0x2A;
const ST7789_RASET: u8 = 0x2B;
const ST7789_RAMWR: u8 = 0x2C;
const ST7789_MADCTL: u8 = 0x36;
const ST7789_COLMOD: u8 = 0x3A;
const ST7789_PORCTRL: u8 = 0xB2;
const ST7789_GCTRL: u8 = 0xB7;
const ST7789_VCOMS: u8 = 0xBB;
const ST7789_LCMCTRL: u8 = 0xC0;
const ST7789_VDVVRHEN: u8 = 0xC2;
const ST7789_VRHS: u8 = 0xC3;
const ST7789_VDVS: u8 = 0xC4;
const ST7789_FRCTRL2: u8 = 0xC6;
const ST7789_PWCTRL1: u8 = 0xD0;
const ST7789_PVGAMCTRL: u8 = 0xE0;
const ST7789_NVGAMCTRL: u8 = 0xE1;

pub const COLOR_RGB565_RED: u16 = 0xF800;
pub const COLOR_RGB565_GREEN: u16 = 0x07E0;
pub const COLOR_RGB565_BLUE: u16 = 0x001F;
pub const COLOR_RGB565_YELLOW: u16 = 0xFFE0;
pub const COLOR_RGB565_PURPLE: u16 = 0xF81F;
pub const COLOR_RGB565_BLACK: u16 = 0x0000;
pub const COLOR_RGB565_WHITE: u16 = 0xFFFF;

pub struct St7789<'d> {
    spi: Spi<'d, esp_hal::Blocking>,
    dc: Output<'d>,
    res: Output<'d>,
    blk: Option<Output<'d>>,
    pub delay: Delay,
}

impl<'d> St7789<'d> {
    pub fn new(
        spi: Spi<'d, esp_hal::Blocking>,
        dc: Output<'d>,
        res: Output<'d>,
    ) -> Self {
        Self {
            spi,
            dc,
            res,
            blk: None,
            delay: Delay::new(),
        }
    }

    pub fn with_blk(mut self, blk: Output<'d>) -> Self {
        self.blk = Some(blk);
        self
    }

    fn write_cmd(&mut self, cmd: u8) {
        self.dc.set_low();
        self.spi.write(&[cmd]).unwrap();
    }

    fn write_data(&mut self, data: &[u8]) {
        self.dc.set_high();
        self.spi.write(data).unwrap();
    }

    fn write_word(&mut self, word: u16) {
        self.write_data(&[(word >> 8) as u8, (word & 0xFF) as u8]);
    }

    fn hard_reset(&mut self) {
        self.res.set_low();
        self.delay.delay_millis(20);
        self.res.set_high();
        self.delay.delay_millis(150);
    }

    pub fn init(&mut self) {
        if let Some(ref mut blk) = self.blk {
            blk.set_high();
        }

        self.hard_reset();

        self.write_cmd(ST7789_SWRESET);
        self.delay.delay_millis(200);

        self.write_cmd(ST7789_SLPOUT);
        self.delay.delay_millis(120);

        self.write_cmd(ST7789_MADCTL);
        self.write_data(&[0x00]);

        self.write_cmd(ST7789_COLMOD);
        self.write_data(&[0x55]);
        self.delay.delay_millis(10);

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

        self.write_cmd(ST7789_FRCTRL2);
        self.write_data(&[0x0F]);

        self.write_cmd(ST7789_PWCTRL1);
        self.write_data(&[0xA4, 0xA1]);

        self.write_cmd(ST7789_PVGAMCTRL);
        self.write_data(&[
            0xD0, 0x04, 0x0D, 0x11, 0x13, 0x2B, 0x3F, 0x54, 0x4C, 0x18,
            0x0D, 0x0B, 0x1F, 0x23,
        ]);

        self.write_cmd(ST7789_NVGAMCTRL);
        self.write_data(&[
            0xD0, 0x04, 0x0C, 0x11, 0x13, 0x2C, 0x3F, 0x44, 0x51, 0x2F,
            0x1F, 0x1F, 0x20, 0x23,
        ]);

        self.write_cmd(ST7789_INVON);
        self.delay.delay_millis(10);

        self.write_cmd(ST7789_NORON);
        self.delay.delay_millis(10);

        self.write_cmd(ST7789_DISPON);
        self.delay.delay_millis(120);
    }

    pub fn set_window(&mut self, x0: u16, y0: u16, x1: u16, y1: u16) {
        self.write_cmd(ST7789_CASET);
        self.write_word(x0 + X_OFFSET);
        self.write_word(x1 + X_OFFSET);

        self.write_cmd(ST7789_RASET);
        self.write_word(y0 + Y_OFFSET);
        self.write_word(y1 + Y_OFFSET);

        self.write_cmd(ST7789_RAMWR);
    }

    pub fn fill_screen(&mut self, color: u16) {
        self.set_window(0, 0, WIDTH - 1, HEIGHT - 1);
        self.dc.set_high();

        let hi = (color >> 8) as u8;
        let lo = (color & 0xFF) as u8;
        let mut line_buf = [0u8; WIDTH as usize * 2];
        let mut i = 0;
        for _ in 0..WIDTH {
            line_buf[i] = hi;
            line_buf[i + 1] = lo;
            i += 2;
        }
        for _ in 0..HEIGHT {
            self.spi.write(&line_buf).unwrap();
        }
    }

    pub fn flush(&mut self, fb: &[u8]) {
        self.set_window(0, 0, WIDTH - 1, HEIGHT - 1);
        self.dc.set_high();
        for chunk in fb.chunks(WIDTH as usize * 2) {
            self.spi.write(chunk).unwrap();
        }
    }

    pub fn flush_region(&mut self, fb: &[u8], x0: u16, y0: u16, x1: u16, y1: u16) {
        if x0 > x1 || y0 > y1 {
            return;
        }
        self.set_window(x0, y0, x1, y1);
        self.dc.set_high();
        let w = (x1 - x0 + 1) as usize * 2;
        for row in y0..=y1 {
            let start = (row as usize * WIDTH as usize + x0 as usize) * 2;
            self.spi.write(&fb[start..start + w]).unwrap();
        }
    }
}

pub const FB_SIZE: usize = (WIDTH as usize) * (HEIGHT as usize) * 2;

pub fn fb_clear(fb: &mut [u8], color: u16) {
    let hi = (color >> 8) as u8;
    let lo = (color & 0xFF) as u8;
    for chunk in fb.chunks_exact_mut(2) {
        chunk[0] = hi;
        chunk[1] = lo;
    }
}

pub fn fb_set_pixel(fb: &mut [u8], x: i16, y: i16, color: u16) {
    if x < 0 || y < 0 || x >= WIDTH as i16 || y >= HEIGHT as i16 {
        return;
    }
    let idx = ((y as usize) * (WIDTH as usize) + (x as usize)) * 2;
    fb[idx] = (color >> 8) as u8;
    fb[idx + 1] = (color & 0xFF) as u8;
}

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
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

pub fn fb_fill_rect(fb: &mut [u8], x0: u16, y0: u16, x1: u16, y1: u16, color: u16) {
    let hi = (color >> 8) as u8;
    let lo = (color & 0xFF) as u8;
    let w = WIDTH as usize;
    for y in y0..=y1 {
        let row_start = y as usize * w;
        for x in x0..=x1 {
            let idx = (row_start + x as usize) * 2;
            fb[idx] = hi;
            fb[idx + 1] = lo;
        }
    }
}

pub fn fb_fill_triangle(
    fb: &mut [u8],
    x0: i16, y0: i16,
    x1: i16, y1: i16,
    x2: i16, y2: i16,
    color: u16,
) {
    let (mut ax, mut ay) = (x0, y0);
    let (mut bx, mut by) = (x1, y1);
    let (mut cx, mut cy) = (x2, y2);

    if ay > by {
        core::mem::swap(&mut ax, &mut bx);
        core::mem::swap(&mut ay, &mut by);
    }
    if ay > cy {
        core::mem::swap(&mut ax, &mut cx);
        core::mem::swap(&mut ay, &mut cy);
    }
    if by > cy {
        core::mem::swap(&mut bx, &mut cx);
        core::mem::swap(&mut by, &mut cy);
    }

    if ay == cy {
        return;
    }

    let hi = (color >> 8) as u8;
    let lo = (color & 0xFF) as u8;
    let w = WIDTH as usize;
    let ay32 = ay as i32;
    let by32 = by as i32;
    let cy32 = cy as i32;
    let ax32 = ax as i32;
    let bx32 = bx as i32;
    let cx32 = cx as i32;
    let h_ac = cy32 - ay32;

    if ay < by {
        let h_ab = by32 - ay32;
        for y in ay..by {
            let t = y as i32 - ay32;
            let x1 = ax32 + (bx32 - ax32) * t / h_ab;
            let x2 = ax32 + (cx32 - ax32) * t / h_ac;
            let (xl, xr) = if x1 <= x2 { (x1 as i16, x2 as i16) } else { (x2 as i16, x1 as i16) };
            if y >= 0 && (y as u16) < HEIGHT {
                let row = y as usize * w;
                let xs = xl.max(0) as u16;
                let xe = (xr as u16).min(WIDTH - 1);
                for x in xs..=xe {
                    let idx = (row + x as usize) * 2;
                    fb[idx] = hi;
                    fb[idx + 1] = lo;
                }
            }
        }
    }

    if by < cy {
        let h_bc = cy32 - by32;
        for y in by..=cy {
            let t1 = y as i32 - by32;
            let t2 = y as i32 - ay32;
            let x1 = bx32 + (cx32 - bx32) * t1 / h_bc;
            let x2 = ax32 + (cx32 - ax32) * t2 / h_ac;
            let (xl, xr) = if x1 <= x2 { (x1 as i16, x2 as i16) } else { (x2 as i16, x1 as i16) };
            if y >= 0 && (y as u16) < HEIGHT {
                let row = y as usize * w;
                let xs = xl.max(0) as u16;
                let xe = (xr as u16).min(WIDTH - 1);
                for x in xs..=xe {
                    let idx = (row + x as usize) * 2;
                    fb[idx] = hi;
                    fb[idx + 1] = lo;
                }
            }
        }
    }
}
