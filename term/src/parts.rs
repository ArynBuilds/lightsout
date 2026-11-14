//! What a plugin's part shows: rows of text pieces, each with a colour, bold or not, and a
//! command that a click runs. A plugin sends it as JSON (`lightsout part set ID JSON`), in
//! any of these shapes:
//!
//! - `"one line\nanother"`: plain lines
//! - `["a line", ["two", "pieces"]]`: rows; a row is a line or a list of pieces
//! - `{"bg": "#101014", "rows": [...]}`: with a background
//!
//! A piece is a string or `{"text": "...", "color": "accent", "bold": true, "command":
//! "room.go 2"}`. Colours are `#rrggbb` or a name that follows your settings' colours: `fg`,
//! `dim`, `accent`, `alert`, `done`, `bg`.
//!
//! Or a part shows a picture the plugin drew (a frame): `lightsout part image ID FILE`, a PNG
//! or a PPM (P6) file, shown as it is from the part's top left corner (the plugin draws it at
//! the part's size, which it gets with `part.resized`).

use serde_json::Value;
use std::path::Path;

/// A picture a plugin drew: its size and its pixels (0xRRGGBB, row by row).
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u32>,
}

/// The largest picture a part takes (bigger than any screen).
const MAX_SIDE: u32 = 8192;

/// A PNG or PPM (P6) file as a picture. Transparent parts are shown over black.
pub fn load_image(path: &Path) -> Result<Image, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.starts_with(b"P6") {
        return ppm(&bytes);
    }
    let mut d = png::Decoder::new(std::io::Cursor::new(&bytes));
    d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = d
        .read_info()
        .map_err(|e| format!("not a PNG or PPM picture ({e})"))?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).map_err(|e| e.to_string())?;
    if info.width > MAX_SIDE || info.height > MAX_SIDE {
        return Err(format!("the picture is bigger than {MAX_SIDE}×{MAX_SIDE}"));
    }
    let n = (info.width * info.height) as usize;
    let over_black = |c: u8, a: u8| (c as u32 * a as u32 / 255) as u8;
    let rgb = |r: u8, g: u8, b: u8| (r as u32) << 16 | (g as u32) << 8 | b as u32;
    let px = match info.color_type {
        png::ColorType::Rgb => buf
            .chunks(3)
            .take(n)
            .map(|c| rgb(c[0], c[1], c[2]))
            .collect(),
        png::ColorType::Rgba => buf
            .chunks(4)
            .take(n)
            .map(|c| {
                let a = c[3];
                rgb(
                    over_black(c[0], a),
                    over_black(c[1], a),
                    over_black(c[2], a),
                )
            })
            .collect(),
        png::ColorType::Grayscale => buf.iter().take(n).map(|&g| rgb(g, g, g)).collect(),
        png::ColorType::GrayscaleAlpha => buf
            .chunks(2)
            .take(n)
            .map(|c| {
                let g = over_black(c[0], c[1]);
                rgb(g, g, g)
            })
            .collect(),
        png::ColorType::Indexed => return Err("an indexed PNG wasn't expanded".into()),
    };
    Ok(Image {
        w: info.width,
        h: info.height,
        px,
    })
}

/// A binary PPM: `P6`, width, height, 255, then the RGB bytes (easy to write from any language).
fn ppm(bytes: &[u8]) -> Result<Image, String> {
    let mut fields = Vec::new();
    let mut i = 2;
    while fields.len() < 3 && i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.get(i) == Some(&b'#') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let n: u32 = std::str::from_utf8(&bytes[start..i])
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or("a PPM picture with a broken header")?;
        fields.push(n);
    }
    let (w, h, max) = match fields[..] {
        [w, h, m] => (w, h, m),
        _ => return Err("a PPM picture with a broken header".into()),
    };
    if max != 255 || w > MAX_SIDE || h > MAX_SIDE {
        return Err("a PPM picture must be 8-bit (255) and at most 8192×8192".into());
    }
    let data = &bytes[(i + 1).min(bytes.len())..];
    let n = (w * h) as usize;
    if data.len() < n * 3 {
        return Err("a PPM picture with fewer pixels than its size".into());
    }
    let px = data
        .chunks(3)
        .take(n)
        .map(|c| (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32)
        .collect();
    Ok(Image { w, h, px })
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Span {
    pub text: String,
    pub color: Option<String>,
    pub bold: bool,
    pub command: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Content {
    pub rows: Vec<Vec<Span>>,
    pub bg: Option<String>,
}

fn span(v: &Value) -> Option<Span> {
    match v {
        Value::String(s) => Some(Span {
            text: s.clone(),
            ..Default::default()
        }),
        Value::Object(o) => Some(Span {
            text: o
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string(),
            color: o.get("color").and_then(|t| t.as_str()).map(str::to_string),
            bold: o.get("bold").and_then(|t| t.as_bool()).unwrap_or(false),
            command: o
                .get("command")
                .and_then(|t| t.as_str())
                .filter(|c| !c.trim().is_empty())
                .map(str::to_string),
        }),
        Value::Number(n) => Some(Span {
            text: n.to_string(),
            ..Default::default()
        }),
        _ => None,
    }
}

fn rows(v: &Value) -> Vec<Vec<Span>> {
    match v {
        Value::String(s) => s
            .lines()
            .map(|l| {
                vec![Span {
                    text: l.to_string(),
                    ..Default::default()
                }]
            })
            .collect(),
        Value::Array(a) => a
            .iter()
            .map(|row| match row {
                Value::Array(pieces) => pieces.iter().filter_map(span).collect(),
                other => span(other).into_iter().collect(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// What a plugin sent, in any of the shapes above (anything else shows nothing).
pub fn parse(v: &Value) -> Content {
    match v {
        Value::Object(o) if o.contains_key("rows") => Content {
            rows: rows(&o["rows"]),
            bg: o.get("bg").and_then(|b| b.as_str()).map(str::to_string),
        },
        Value::Object(_) => Content {
            rows: span(v).into_iter().map(|s| vec![s]).collect(),
            bg: None,
        },
        other => Content {
            rows: rows(other),
            bg: None,
        },
    }
}

/// The colours a part can name, from your settings (and Lights Out's own for the rest).
#[derive(Clone, Copy)]
pub struct Palette {
    pub fg: u32,
    pub bg: u32,
    pub accent: u32,
    pub dim: u32,
    pub alert: u32,
    pub done: u32,
}

/// A colour by name or `#rrggbb`; `or` when there is none or it isn't understood.
pub fn color(name: Option<&str>, p: Palette, or: u32) -> u32 {
    match name.map(str::trim) {
        Some("fg") => p.fg,
        Some("bg") => p.bg,
        Some("accent") => p.accent,
        Some("dim") => p.dim,
        Some("alert") => p.alert,
        Some("done") => p.done,
        Some(hex) if hex.starts_with('#') && hex.len() == 7 => {
            u32::from_str_radix(&hex[1..], 16).unwrap_or(or)
        }
        _ => or,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_shape_a_plugin_may_send() {
        let c = parse(&json!("one\ntwo"));
        assert_eq!(c.rows.len(), 2);
        assert_eq!(c.rows[1][0].text, "two");
        let c = parse(&json!({"bg": "#101014", "rows": [
            "title",
            ["● ", {"text": "claude", "color": "accent", "bold": true, "command": "agent.jump"}],
        ]}));
        assert_eq!(c.bg.as_deref(), Some("#101014"));
        assert_eq!(c.rows[1].len(), 2);
        let s = &c.rows[1][1];
        assert_eq!((s.text.as_str(), s.bold), ("claude", true));
        assert_eq!(s.command.as_deref(), Some("agent.jump"));
        assert!(parse(&json!(null)).rows.is_empty());
    }

    #[test]
    fn pictures_in_png_and_ppm() {
        let dir = std::env::temp_dir().join(format!("lo-image-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // a 2×1 PPM: red, blue
        let ppm = dir.join("a.ppm");
        let mut bytes = b"P6
# made by a test
2 1
255
"
        .to_vec();
        bytes.extend([255, 0, 0, 0, 0, 255]);
        std::fs::write(&ppm, bytes).unwrap();
        let i = load_image(&ppm).unwrap();
        assert_eq!((i.w, i.h, i.px.clone()), (2, 1, vec![0xff0000, 0x0000ff]));
        // the same as a PNG, with a half-transparent green third pixel
        let png_file = dir.join("a.png");
        {
            let f = std::fs::File::create(&png_file).unwrap();
            let mut e = png::Encoder::new(std::io::BufWriter::new(f), 3, 1);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            let mut w = e.write_header().unwrap();
            w.write_image_data(&[255, 0, 0, 255, 0, 0, 255, 255, 0, 255, 0, 128])
                .unwrap();
        }
        let i = load_image(&png_file).unwrap();
        assert_eq!(i.px, vec![0xff0000, 0x0000ff, 0x008000]);
        std::fs::write(dir.join("b.txt"), "hello").unwrap();
        assert!(load_image(&dir.join("b.txt")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn colours_follow_the_settings() {
        let p = Palette {
            fg: 1,
            bg: 2,
            accent: 3,
            dim: 4,
            alert: 5,
            done: 6,
        };
        assert_eq!(color(Some("accent"), p, 0), 3);
        assert_eq!(color(Some("#ff0000"), p, 0), 0xff0000);
        assert_eq!(color(Some("nope"), p, 9), 9);
        assert_eq!(color(None, p, 9), 9);
    }
}
