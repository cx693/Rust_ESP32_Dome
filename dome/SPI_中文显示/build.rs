use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use std::collections::HashSet;
use std::{env, fs, path::Path};

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let src_dir = Path::new(&manifest_dir).join("src");
    let fonts_dir = Path::new(&manifest_dir).join("fonts");

    watch_dir(&src_dir);
    println!("cargo:rerun-if-changed=fonts/CXI.ttf");
    println!("cargo:rerun-if-changed=fonts/阿里妈妈方圆体.ttf");

    linker_be_nice();
    println!("cargo:rustc-link-arg=-Tlinkall.x");

    let calls = scan_draw_str_calls(&src_dir);

    let cxi_ttf = fs::read(fonts_dir.join("CXI.ttf")).expect("Failed to read CXI.ttf");
    let ali_ttf =
        fs::read(fonts_dir.join("阿里妈妈方圆体.ttf")).expect("Failed to read 阿里妈妈方圆体.ttf");

    let mut glyphs: Vec<(&str, char, u16, u16, u16, i16, Vec<u8>)> = Vec::new();
    let mut seen = HashSet::new();

    for (text, family, size) in &calls {
        let font_data = match family.as_str() {
            "FontFamily::Cxi" => &cxi_ttf,
            "FontFamily::AliMaMa" => &ali_ttf,
            _ => continue,
        };
        for ch in text.chars() {
            if ch == '\n' {
                continue;
            }
            let key = (family.as_str(), ch, *size);
            if !seen.insert(key) {
                continue;
            }
            if let Some((w, h, y_off, data)) = rasterize(font_data, ch, *size) {
                glyphs.push((family.as_str(), ch, *size, w, h, y_off, data));
            }
        }
    }

    let mut code = String::new();

    for (i, (_, _, _, _, _, _, data)) in glyphs.iter().enumerate() {
        code.push_str(&format!("const G{}: [u8; {}] = [", i, data.len()));
        for (j, byte) in data.iter().enumerate() {
            if j > 0 {
                code.push(',');
            }
            code.push_str(&format!("0x{:02X}", byte));
        }
        code.push_str("];\n");
    }

    code.push_str(
        "\npub fn lookup_glyph(family: FontFamily, ch: char, size: u16) -> Option<(&'static [u8], u16, u16, i16)> {\n",
    );
    code.push_str("    match (family, ch, size) {\n");

    for (i, (family, ch, size, w, h, y_off, _)) in glyphs.iter().enumerate() {
        let fam = match *family {
            "FontFamily::Cxi" => "FontFamily::Cxi",
            "FontFamily::AliMaMa" => "FontFamily::AliMaMa",
            _ => continue,
        };
        let ch_lit = char_literal(*ch);
        code.push_str(&format!(
            "        ({}, {}, {}) => Some((&G{}, {}, {}, {})),\n",
            fam, ch_lit, size, i, w, h, y_off
        ));
    }

    code.push_str("        _ => None,\n");
    code.push_str("    }\n");
    code.push_str("}\n");

    fs::write(Path::new(&out_dir).join("glyphs.rs"), code).unwrap();
}

fn watch_dir(dir: &Path) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            watch_dir(&path);
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

fn scan_draw_str_calls(dir: &Path) -> Vec<(String, String, u16)> {
    let mut calls = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            calls.extend(scan_draw_str_calls(&path));
        } else if path.extension().map_or(false, |e| e == "rs") {
            let source = fs::read_to_string(&path).unwrap();
            calls.extend(parse_draw_str(&source));
        }
    }
    calls
}

fn parse_draw_str(source: &str) -> Vec<(String, String, u16)> {
    let mut calls = Vec::new();
    let mut search_from = 0;
    while let Some(idx) = source[search_from..].find("draw_str(") {
        let call_start = search_from + idx;
        let args_start = call_start + "draw_str(".len();

        let mut depth = 1i32;
        let mut args_end = args_start;
        let mut in_str = false;
        let mut escape = false;
        for (i, c) in source[args_start..].char_indices() {
            if escape {
                escape = false;
                continue;
            }
            if c == '\\' && in_str {
                escape = true;
                continue;
            }
            if c == '"' {
                in_str = !in_str;
                continue;
            }
            if in_str {
                continue;
            }
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => {
                    depth -= 1;
                    if depth == 0 {
                        args_end = args_start + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        if depth != 0 {
            search_from = args_start;
            continue;
        }

        let args_str = &source[args_start..args_end];
        let parts = split_args(args_str);

        if parts.len() >= 6 {
            let text_raw = parts[3].trim();
            let family_raw = parts[4].trim();
            let size_raw = parts[5].trim();

            if text_raw.starts_with('"') && text_raw.ends_with('"') && text_raw.len() >= 2 {
                let text = &text_raw[1..text_raw.len() - 1];
                if let Ok(size) = size_raw.parse::<u16>() {
                    calls.push((text.to_string(), family_raw.to_string(), size));
                }
            }
        }
        search_from = args_end + 1;
    }
    calls
}

fn split_args(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    let mut last = 0;

    for (i, c) in s.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if c == '\\' && in_str {
            escape = true;
            continue;
        }
        if c == '"' {
            in_str = !in_str;
            continue;
        }
        if in_str {
            continue;
        }
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&s[last..i]);
                last = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[last..]);
    parts
}

fn char_literal(ch: char) -> String {
    match ch {
        '\'' => "'\\''".to_string(),
        '\\' => "'\\\\'".to_string(),
        '\n' => "'\\n'".to_string(),
        '\r' => "'\\r'".to_string(),
        '\t' => "'\\t'".to_string(),
        '\0' => "'\\0'".to_string(),
        c if c.is_ascii_graphic() || c == ' ' || !c.is_ascii() => format!("'{}'", c),
        c => format!("'\\u{{{:x}}}'", c as u32),
    }
}

fn rasterize(font_data: &[u8], ch: char, px: u16) -> Option<(u16, u16, i16, Vec<u8>)> {
    let font = FontRef::try_from_slice(font_data).ok()?;
    let scale = PxScale {
        x: px as f32,
        y: px as f32,
    };
    let scaled = font.as_scaled(scale);
    let glyph_id = font.glyph_id(ch);
    if glyph_id == font.glyph_id('\0') && ch != '\0' {
        return None;
    }
    let glyph = glyph_id.with_scale_and_position(
        scale,
        ab_glyph::Point {
            x: 0.0,
            y: scaled.ascent(),
        },
    );
    let outline = match font.outline_glyph(glyph) {
        Some(o) => o,
        None => {
            let advance = scaled.h_advance(glyph_id).ceil() as u16;
            if advance == 0 {
                return None;
            }
            return Some((advance, 1, 0, vec![0u8; advance as usize]));
        }
    };
    let bounds = outline.px_bounds();
    let gw = bounds.width().ceil() as u16;
    let gh = bounds.height().ceil() as u16;
    if gw == 0 || gh == 0 || gw as usize * gh as usize > 1024 {
        return None;
    }
    let w = gw as usize;
    let h = gh as usize;
    let mut coverage = vec![0.0f32; w * h];
    outline.draw(|x, y, c| {
        let px_i = x as usize;
        let py_i = y as usize;
        if px_i < w && py_i < h {
            let idx = py_i * w + px_i;
            if c > coverage[idx] {
                coverage[idx] = c;
            }
        }
    });
    let mut result = vec![0u8; w * h];
    for i in 0..(w * h) {
        result[i] = (coverage[i].clamp(0.0, 1.0) * 255.0) as u8;
    }
    Some((gw, gh, bounds.min.y as i16, result))
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                what if what.starts_with("_defmt_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`"
                    );
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                what if what.starts_with("esp_rtos_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `esp-radio` has no scheduler enabled. Make sure you have initialized `esp-rtos` or provided an external scheduler."
                    );
                    eprintln!();
                }
                "embedded_test_linker_file_not_added_to_rustflags" => {
                    eprintln!();
                    eprintln!(
                        "💡 `embedded-test` not found - make sure `embedded-test.x` is added as a linker script for tests"
                    );
                    eprintln!();
                }
                "free"
                | "malloc"
                | "calloc"
                | "get_free_internal_heap_size"
                | "malloc_internal"
                | "realloc_internal"
                | "calloc_internal"
                | "free_internal" => {
                    eprintln!();
                    eprintln!(
                        "💡 Did you forget the `esp-alloc` dependency or didn't enable the `compat` feature on it?"
                    );
                    eprintln!();
                }
                _ => (),
            },
            _ => {
                std::process::exit(1);
            }
        }

        std::process::exit(0);
    }

    println!(
        "cargo:rustc-link-arg=-Wl,--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}
