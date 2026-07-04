use crate::st7789::{COLOR_RGB565_BLACK, HEIGHT, WIDTH, fb_fill_rect, fb_set_pixel};

const GIF_MAGIC: &[u8] = b"GIF89a";
const GIF_MAGIC_87A: &[u8] = b"GIF87a";

pub struct GifPlayer<'a> {
    data: &'a [u8],
    pos: usize,
    global_ct: [u8; 768],
    frame_count: usize,
    data_start: usize,
    prev_disposal: u8,
    prev_left: i16,
    prev_top: i16,
    prev_width: usize,
    prev_height: usize,
}

impl<'a> GifPlayer<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        let mut player = Self {
            data,
            pos: 0,
            global_ct: [0u8; 768],
            frame_count: 0,
            data_start: 0,
            prev_disposal: 0,
            prev_left: 0,
            prev_top: 0,
            prev_width: 0,
            prev_height: 0,
        };
        player.init();
        player
    }

    fn read_u16_le(data: &[u8], offset: usize) -> u16 {
        u16::from_le_bytes([data[offset], data[offset + 1]])
    }

    fn init(&mut self) {
        if self.data.len() < 13 {
            return;
        }
        if &self.data[0..6] != GIF_MAGIC && &self.data[0..6] != GIF_MAGIC_87A {
            return;
        }
        self.pos = 6;

        let _width = Self::read_u16_le(self.data, self.pos) as usize;
        let _height = Self::read_u16_le(self.data, self.pos + 2) as usize;
        let flags = self.data[self.pos + 4];
        self.pos += 7;

        let has_gct = flags & 0x80 != 0;
        let gct_size = if has_gct { 1 << ((flags & 0x07) + 1) } else { 0 };

        if has_gct && self.pos + gct_size * 3 <= self.data.len() {
            self.global_ct[..gct_size * 3]
                .copy_from_slice(&self.data[self.pos..self.pos + gct_size * 3]);
            self.pos += gct_size * 3;
        }

        self.data_start = self.pos;
        self.frame_count = self.count_frames();
    }

    fn count_frames(&self) -> usize {
        let mut pos = self.data_start;
        let mut count = 0;
        loop {
            if pos >= self.data.len() {
                return count;
            }
            let bt = self.data[pos];
            pos += 1;
            match bt {
                0x2C => {
                    if pos + 9 > self.data.len() {
                        return count;
                    }
                    let img_flags = self.data[pos + 8];
                    pos += 9;
                    let has_lct = img_flags & 0x80 != 0;
                    let lct_size = if has_lct {
                        1 << ((img_flags & 0x07) + 1)
                    } else {
                        0
                    };
                    pos += lct_size * 3;
                    if pos >= self.data.len() {
                        return count;
                    }
                    pos += 1;
                    loop {
                        if pos >= self.data.len() {
                            return count;
                        }
                        let sz = self.data[pos] as usize;
                        pos += 1;
                        if sz == 0 {
                            break;
                        }
                        pos += sz;
                    }
                    count += 1;
                }
                0x21 => {
                    if pos >= self.data.len() {
                        return count;
                    }
                    pos += 1;
                    loop {
                        if pos >= self.data.len() {
                            return count;
                        }
                        let sz = self.data[pos] as usize;
                        pos += 1;
                        if sz == 0 {
                            break;
                        }
                        pos += sz;
                    }
                }
                0x3B => return count,
                _ => {}
            }
        }
    }

    pub fn frame_count(&self) -> usize {
        self.frame_count
    }

    pub fn reset(&mut self) {
        self.pos = self.data_start;
        self.prev_disposal = 0;
    }

    pub fn decode_next_frame(&mut self, fb: &mut [u8]) -> Option<u16> {
        if self.prev_disposal == 2 && self.prev_width > 0 && self.prev_height > 0 {
            let x0 = self.prev_left.max(0) as u16;
            let y0 = self.prev_top.max(0) as u16;
            let x1 = ((self.prev_left + self.prev_width as i16 - 1).min(WIDTH as i16 - 1)).max(0)
                as u16;
            let y1 = ((self.prev_top + self.prev_height as i16 - 1).min(HEIGHT as i16 - 1)).max(0)
                as u16;
            if x0 <= x1 && y0 <= y1 {
                fb_fill_rect(fb, x0, y0, x1, y1, COLOR_RGB565_BLACK);
            }
        }

        let mut delay_ms: u16 = 0;
        let mut disposal_method: u8 = 0;
        let mut transparent = false;
        let mut transparent_index: u8 = 0;

        loop {
            if self.pos >= self.data.len() {
                return None;
            }
            let block_type = self.data[self.pos];
            self.pos += 1;

            match block_type {
                0x2C => {
                    if self.pos + 9 > self.data.len() {
                        return None;
                    }
                    let left = Self::read_u16_le(self.data, self.pos) as i16;
                    let top = Self::read_u16_le(self.data, self.pos + 2) as i16;
                    let w = Self::read_u16_le(self.data, self.pos + 4) as usize;
                    let h = Self::read_u16_le(self.data, self.pos + 6) as usize;
                    let img_flags = self.data[self.pos + 8];
                    self.pos += 9;

                    let has_lct = img_flags & 0x80 != 0;
                    let lct_size = if has_lct {
                        1 << ((img_flags & 0x07) + 1)
                    } else {
                        0
                    };

                    let mut local_ct = [0u8; 768];
                    if has_lct {
                        if self.pos + lct_size * 3 > self.data.len() {
                            return None;
                        }
                        local_ct[..lct_size * 3]
                            .copy_from_slice(&self.data[self.pos..self.pos + lct_size * 3]);
                        self.pos += lct_size * 3;
                    }

                    let ct: &[u8; 768] = if has_lct { &local_ct } else { &self.global_ct };

                    if self.pos >= self.data.len() {
                        return None;
                    }
                    let min_code_size = self.data[self.pos] as usize;
                    self.pos += 1;

                    let mut compressed = alloc::vec::Vec::new();
                    loop {
                        if self.pos >= self.data.len() {
                            return None;
                        }
                        let sz = self.data[self.pos] as usize;
                        self.pos += 1;
                        if sz == 0 {
                            break;
                        }
                        if self.pos + sz > self.data.len() {
                            return None;
                        }
                        compressed.extend_from_slice(&self.data[self.pos..self.pos + sz]);
                        self.pos += sz;
                    }

                    let pixel_count = w * h;
                    let mut indexed_pixels = alloc::vec![0u8; pixel_count];
                    let decoded =
                        Self::decode_lzw(&compressed, min_code_size, &mut indexed_pixels, pixel_count);

                    for y in 0..h {
                        for x in 0..w {
                            let idx = y * w + x;
                            if idx >= decoded {
                                continue;
                            }
                            let color_idx = indexed_pixels[idx] as usize;
                            if transparent && color_idx == transparent_index as usize {
                                continue;
                            }
                            let pal_idx = color_idx * 3;
                            if pal_idx + 2 < ct.len() {
                                let r = ct[pal_idx];
                                let g = ct[pal_idx + 1];
                                let b = ct[pal_idx + 2];
                                let color = rgb_to_rgb565(r, g, b);
                                let px = left + x as i16;
                                let py = top + y as i16;
                                if px >= 0
                                    && py >= 0
                                    && (px as u16) < WIDTH
                                    && (py as u16) < HEIGHT
                                {
                                    fb_set_pixel(fb, px, py, color);
                                }
                            }
                        }
                    }

                    self.prev_disposal = disposal_method;
                    self.prev_left = left;
                    self.prev_top = top;
                    self.prev_width = w;
                    self.prev_height = h;

                    return Some(delay_ms);
                }
                0x21 => {
                    if self.pos >= self.data.len() {
                        return None;
                    }
                    let ext_type = self.data[self.pos];
                    self.pos += 1;

                    if ext_type == 0xF9 {
                        if self.pos + 6 > self.data.len() {
                            return None;
                        }
                        let packed = self.data[self.pos + 1];
                        disposal_method = (packed >> 2) & 0x07;
                        transparent = packed & 0x01 != 0;
                        transparent_index = self.data[self.pos + 4];
                        delay_ms = Self::read_u16_le(self.data, self.pos + 2) as u16 * 10;
                        self.pos += 6;
                    } else {
                        loop {
                            if self.pos >= self.data.len() {
                                return None;
                            }
                            let sz = self.data[self.pos] as usize;
                            self.pos += 1;
                            if sz == 0 {
                                break;
                            }
                            self.pos += sz;
                        }
                    }
                }
                0x3B => return None,
                _ => {}
            }
        }
    }

    fn decode_lzw(
        data: &[u8],
        min_code_size: usize,
        pixels: &mut [u8],
        pixel_count: usize,
    ) -> usize {
        let clear_code = 1 << min_code_size;
        let eoi_code = clear_code + 1;

        static mut PARENTS: [u16; 4096] = [0; 4096];
        static mut FIRST_BYTE: [u8; 4096] = [0u8; 4096];
        static mut LENGTHS: [u16; 4096] = [0u16; 4096];
        static mut STACK: [u8; 4096] = [0u8; 4096];

        let mut code_size = min_code_size + 1;
        let mut next_code = eoi_code + 1;
        let mut bit_buf: u32 = 0;
        let mut bits_in_buf: usize = 0;
        let mut byte_pos = 0;
        let mut pixel_idx = 0;
        let mut prev_code: Option<usize> = None;

        unsafe {
            for i in 0..clear_code {
                PARENTS[i] = i as u16;
                FIRST_BYTE[i] = i as u8;
                LENGTHS[i] = 1;
            }

            loop {
                while bits_in_buf < code_size {
                    if byte_pos >= data.len() {
                        return pixel_idx;
                    }
                    bit_buf |= (data[byte_pos] as u32) << bits_in_buf;
                    bits_in_buf += 8;
                    byte_pos += 1;
                }
                let code = (bit_buf & ((1u32 << code_size) - 1)) as usize;
                bit_buf >>= code_size;
                bits_in_buf -= code_size;

                if code == eoi_code {
                    break;
                }

                if code == clear_code {
                    code_size = min_code_size + 1;
                    next_code = eoi_code + 1;
                    for i in 0..clear_code {
                        PARENTS[i] = i as u16;
                        LENGTHS[i] = 1;
                    }
                    prev_code = None;
                    continue;
                }

                let len;
                let first_byte;
                let is_special = code == next_code;

                if is_special {
                    if let Some(prev) = prev_code {
                        len = LENGTHS[prev] as usize;
                        let mut c = prev;
                        let mut pos = len;
                        while pos > 0 {
                            pos -= 1;
                            STACK[pos] = FIRST_BYTE[c];
                            if c < clear_code {
                                break;
                            }
                            c = PARENTS[c] as usize;
                        }
                        first_byte = STACK[pos];
                        let copy_len = len.min(pixel_count.saturating_sub(pixel_idx));
                        for i in 0..copy_len {
                            pixels[pixel_idx + i] = STACK[pos + i];
                        }
                        pixel_idx += copy_len;
                        if pixel_idx < pixel_count {
                            pixels[pixel_idx] = first_byte;
                            pixel_idx += 1;
                        }
                    } else {
                        break;
                    }
                } else {
                    len = LENGTHS[code] as usize;
                    let mut c = code;
                    let mut pos = len;
                    while pos > 0 {
                        pos -= 1;
                        STACK[pos] = FIRST_BYTE[c];
                        if c < clear_code {
                            break;
                        }
                        c = PARENTS[c] as usize;
                    }
                    first_byte = STACK[pos];
                    let copy_len = len.min(pixel_count.saturating_sub(pixel_idx));
                    for i in 0..copy_len {
                        pixels[pixel_idx + i] = STACK[pos + i];
                    }
                    pixel_idx += copy_len;
                }

                if let Some(prev) = prev_code {
                    if next_code < 4096 {
                        PARENTS[next_code] = prev as u16;
                        FIRST_BYTE[next_code] = first_byte;
                        LENGTHS[next_code] = LENGTHS[prev] + 1;
                        next_code += 1;
                        if next_code >= (1 << code_size) && code_size < 12 {
                            code_size += 1;
                        }
                    }
                }

                prev_code = Some(code);

                if pixel_idx >= pixel_count {
                    break;
                }
            }
        }

        pixel_idx
    }
}

fn rgb_to_rgb565(r: u8, g: u8, b: u8) -> u16 {
    let r5 = (r >> 3) as u16;
    let g6 = (g >> 2) as u16;
    let b5 = (b >> 3) as u16;
    (r5 << 11) | (g6 << 5) | b5
}
