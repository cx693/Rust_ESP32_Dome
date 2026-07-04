#![allow(dead_code)]
include!(concat!(env!("OUT_DIR"), "/alimamafont.rs"));

pub fn ascii_font(c: char) -> Option<&'static [u8]> {
    let idx = c as usize;
    if idx >= 32 && idx <= 126 {
        Some(&ASCII_FONT[idx - 32][..])
    } else {
        None
    }
}

pub fn cn_font(c: char) -> Option<&'static [u8]> {
    CN_CHARS.iter().position(|&ch| ch == c).map(|i| &CN_FONT[i][..])
}
