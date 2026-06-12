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
}
