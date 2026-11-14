//! Pictures inside a terminal: a plugin's frame shown where `lightsout` runs in a terminal
//! that can draw pictures. kitty, WezTerm and Ghostty take the kitty protocol; iTerm2 its own;
//! Windows Terminal (and foot, mlterm, xterm with it) sixel. Which one: what the terminal is
//! known to be, or LIGHTSOUT_GRAPHICS (kitty, iterm, sixel, none).

use crate::parts::Image;

/// What the terminal `lightsout` runs in can draw: (kitty | iterm | sixel | none, the size of
/// a cell in pixels).
pub fn detect() -> (String, (u32, u32)) {
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let kind = match env("LIGHTSOUT_GRAPHICS").as_str() {
        k @ ("kitty" | "iterm" | "sixel" | "none") => k.to_string(),
        _ => {
            let prog = env("TERM_PROGRAM").to_lowercase();
            if !env("KITTY_WINDOW_ID").is_empty()
                || env("TERM") == "xterm-kitty"
                || prog == "ghostty"
                || prog == "wezterm"
            {
                "kitty".into()
            } else if prog == "iterm.app" {
                "iterm".into()
            } else if !env("WT_SESSION").is_empty() {
                // Windows Terminal draws sixel (1.22 and newer)
                "sixel".into()
            } else {
                "none".into()
            }
        }
    };
    // the cell size, when the terminal says it; else the usual one (Windows Terminal draws
    // sixel on a 10 × 20 grid of its own)
    let cell = crossterm::terminal::window_size()
        .ok()
        .filter(|s| s.width > 0 && s.height > 0 && s.columns > 0 && s.rows > 0)
        .map(|s| ((s.width / s.columns) as u32, (s.height / s.rows) as u32))
        .filter(|_| kind != "sixel" || std::env::var_os("WT_SESSION").is_none())
        .unwrap_or(if kind == "sixel" { (10, 20) } else { (9, 18) });
    (kind, cell)
}

/// The part of a picture that shows: `w` × `h` pixels from its row `top` (smaller if the
/// picture is). Answers (width, height, pixels).
pub fn crop(img: &Image, top: u32, w: u32, h: u32) -> (u32, u32, Vec<u32>) {
    let top = top.min(img.h);
    let (cw, ch) = (w.min(img.w), h.min(img.h - top));
    let mut px = Vec::with_capacity((cw * ch) as usize);
    for y in 0..ch {
        let row = ((top + y) * img.w) as usize;
        px.extend_from_slice(&img.px[row..row + cw as usize]);
    }
    (cw, ch, px)
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        s.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    s
}

fn rgb_bytes(px: &[u32]) -> Vec<u8> {
    px.iter()
        .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8])
        .collect()
}

/// kitty: the picture as image `id`, at the cursor, `cols` × `rows` cells, under the text
/// (so a menu drawn over it stays readable); the one shown before with this id goes first.
pub fn kitty(id: u32, w: u32, h: u32, px: &[u32], cols: u32, rows: u32) -> Vec<u8> {
    let mut out = format!("\x1b_Ga=d,d=I,i={id},q=2\x1b\\").into_bytes();
    let data = base64(&rgb_bytes(px));
    let chunks: Vec<&str> = data
        .as_bytes()
        .chunks(4096)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();
    for (n, c) in chunks.iter().enumerate() {
        let more = u8::from(n + 1 < chunks.len());
        if n == 0 {
            out.extend(
                format!("\x1b_Ga=T,f=24,s={w},v={h},i={id},c={cols},r={rows},C=1,q=2,z=-1,m={more};{c}\x1b\\")
                    .as_bytes(),
            );
        } else {
            out.extend(format!("\x1b_Gm={more};{c}\x1b\\").as_bytes());
        }
    }
    out
}

/// kitty: image `id` no longer shows.
pub fn kitty_delete(id: u32) -> Vec<u8> {
    format!("\x1b_Ga=d,d=I,i={id},q=2\x1b\\").into_bytes()
}

/// iTerm2: the picture as a PNG at the cursor, `cols` × `rows` cells.
pub fn iterm(w: u32, h: u32, px: &[u32], cols: u32, rows: u32) -> Vec<u8> {
    let mut png_bytes = Vec::new();
    {
        let mut e = png::Encoder::new(&mut png_bytes, w, h);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        if let Ok(mut wr) = e.write_header() {
            let _ = wr.write_image_data(&rgb_bytes(px));
        }
    }
    format!(
        "\x1b]1337;File=inline=1;width={cols};height={rows};preserveAspectRatio=0;doNotMoveCursor=1:{}\x07",
        base64(&png_bytes)
    )
    .into_bytes()
}

/// Sixel: the picture at the cursor, in 216 colours (6 levels of red, green and blue).
pub fn sixel(w: u32, h: u32, px: &[u32]) -> Vec<u8> {
    let level = |c: u32| (c * 5 + 127) / 255; // 0..=5
    let index: Vec<u8> = px
        .iter()
        .map(|p| (level(p >> 16 & 255) * 36 + level(p >> 8 & 255) * 6 + level(p & 255)) as u8)
        .collect();
    let mut out = format!("\x1bP0;1;0q\"1;1;{w};{h}").into_bytes();
    let mut used = [false; 216];
    for &i in &index {
        used[i as usize] = true;
    }
    for (i, _) in used.iter().enumerate().filter(|(_, u)| **u) {
        let (r, g, b) = (i / 36, i / 6 % 6, i % 6);
        out.extend(format!("#{i};2;{};{};{}", r * 20, g * 20, b * 20).as_bytes());
    }
    let (w, h) = (w as usize, h as usize);
    for band in (0..h).step_by(6) {
        let rows = (h - band).min(6);
        let mut in_band = [false; 216];
        for y in band..band + rows {
            for &i in &index[y * w..(y + 1) * w] {
                in_band[i as usize] = true;
            }
        }
        for color in (0..216).filter(|c| in_band[*c]) {
            out.extend(format!("#{color}").as_bytes());
            // each column's six pixels of this colour, as one character; runs shortened
            let mut run: (u8, usize) = (0, 0);
            let flush = |out: &mut Vec<u8>, (ch, n): (u8, usize)| match n {
                0 => {}
                1..=3 => out.extend(std::iter::repeat_n(ch, n)),
                _ => out.extend(format!("!{n}{}", ch as char).as_bytes()),
            };
            for x in 0..w {
                let mut bits = 0u8;
                for k in 0..rows {
                    if index[(band + k) * w + x] as usize == color {
                        bits |= 1 << k;
                    }
                }
                let ch = 63 + bits;
                if run.1 > 0 && run.0 == ch {
                    run.1 += 1;
                } else {
                    flush(&mut out, run);
                    run = (ch, 1);
                }
            }
            flush(&mut out, run);
            out.push(b'$');
        }
        out.push(b'-');
    }
    out.extend(b"\x1b\\");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_as_everyone_writes_it() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn the_visible_part_of_a_picture() {
        let img = Image {
            w: 3,
            h: 4,
            px: (0..12).collect(),
        };
        assert_eq!(crop(&img, 1, 2, 2), (2, 2, vec![3, 4, 6, 7]));
        assert_eq!(crop(&img, 3, 9, 9), (3, 1, vec![9, 10, 11]));
    }

    #[test]
    fn sixel_and_kitty_are_well_formed() {
        // 2 × 7: red on top, blue below (two bands of six rows)
        let mut px = vec![0xff0000; 2 * 6];
        px.extend([0x0000ff; 2]);
        let s = String::from_utf8(sixel(2, 7, &px)).unwrap();
        assert!(s.starts_with("\x1bP0;1;0q\"1;1;2;7"), "{s:?}");
        assert!(s.ends_with("\x1b\\"));
        // red is colour 5·36 = 180, blue 5; the first band is all red: "~~" (63 + 63)
        assert!(s.contains("#180;2;100;0;0") && s.contains("#5;2;0;0;100"));
        assert!(s.contains("#180~~$"), "{s:?}");
        let k = String::from_utf8(kitty(7, 2, 7, &px, 1, 1)).unwrap();
        assert!(k.contains("a=T,f=24,s=2,v=7,i=7,c=1,r=1"));
    }
}
