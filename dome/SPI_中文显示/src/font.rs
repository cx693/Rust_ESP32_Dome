#![allow(dead_code)]

#[derive(Clone, Copy, PartialEq)]
pub enum FontFamily {
    Cxi,
    AliMaMa,
}

include!(concat!(env!("OUT_DIR"), "/glyphs.rs"));

pub fn get_glyph(family: FontFamily, ch: char, size: u16) -> Option<(&'static [u8], u16, u16, i16)> {
    lookup_glyph(family, ch, size)
}

pub fn blend_rgb565(fg: u16, bg: u16, alpha: u8) -> u16 {
    if alpha == 0 {
        return bg;
    }
    if alpha == 255 {
        return fg;
    }
    let a = alpha as u32;
    let inv = 255 - a;
    let fr = ((fg >> 11) & 0x1F) as u32;
    let fg_g = ((fg >> 5) & 0x3F) as u32;
    let fb = (fg & 0x1F) as u32;
    let br = ((bg >> 11) & 0x1F) as u32;
    let bg_g = ((bg >> 5) & 0x3F) as u32;
    let bb = (bg & 0x1F) as u32;
    let r = (fr * a + br * inv) / 255;
    let g = (fg_g * a + bg_g * inv) / 255;
    let b = (fb * a + bb * inv) / 255;
    ((r << 11) | (g << 5) | b) as u16
}

pub fn char_width(family: FontFamily, ch: char, size: u16) -> u16 {
    get_glyph(family, ch, size).map(|(_, w, _, _)| w).unwrap_or(0)
}

pub fn draw_char(
    fb: &mut [u8],
    x: i16,
    y: i16,
    family: FontFamily,
    ch: char,
    size: u16,
    fg: u16,
    bg: u16,
    screen_w: u16,
    screen_h: u16,
) -> u16 {
    let (cov, gw, gh, y_off) = match get_glyph(family, ch, size) {
        Some(v) => v,
        None => return 0,
    };
    for row in 0..gh {
        for col in 0..gw {
            let sx = x + col as i16;
            let sy = y + row as i16 + y_off;
            if sx < 0 || sy < 0 || sx >= screen_w as i16 || sy >= screen_h as i16 {
                continue;
            }
            let ci = row as usize * gw as usize + col as usize;
            let alpha = if ci < cov.len() { cov[ci] } else { 0 };
            let color = blend_rgb565(fg, bg, alpha);
            let idx = (sy as usize * screen_w as usize + sx as usize) * 2;
            fb[idx] = (color >> 8) as u8;
            fb[idx + 1] = (color & 0xFF) as u8;
        }
    }
    gw
}

pub fn draw_str(
    fb: &mut [u8],
    mut x: i16,
    y: i16,
    text: &str,
    family: FontFamily,
    size: u16,
    fg: u16,
    bg: u16,
    screen_w: u16,
    screen_h: u16,
) {
    for ch in text.chars() {
        if ch == '\n' {
            continue;
        }
        let w = draw_char(fb, x, y, family, ch, size, fg, bg, screen_w, screen_h);
        x += w as i16;
    }
}
