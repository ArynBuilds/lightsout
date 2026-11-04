//! CPU rendering into a plain pixel buffer: no GPU work, nothing drawn when
//! nothing changes. Colors are 0x00RRGGBB.

use fontdue::{Font, FontSettings, Metrics};
use std::collections::HashMap;

static REGULAR: &[u8] = include_bytes!("../assets/JetBrainsMono-Regular.ttf");
static BOLD: &[u8] = include_bytes!("../assets/JetBrainsMono-Bold.ttf");

/// System fonts asked, in order, for a character the built-in font doesn't have
/// (symbols, Braille spinners, box pieces, emoji outlines...). Any program's
/// characters are covered the same way; nothing is tied to one program.
fn fallback_files() -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    let mut v: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        let dir = PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into())).join("Fonts");
        for f in ["seguisym.ttf", "consola.ttf", "segoeui.ttf", "seguiemj.ttf", "cambria.ttc", "msgothic.ttc", "malgun.ttf", "simsun.ttc"] {
            v.push(dir.join(f));
        }
    } else if cfg!(target_os = "macos") {
        for f in [
            "/System/Library/Fonts/Menlo.ttc",
            "/System/Library/Fonts/Apple Symbols.ttf",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
        ] {
            v.push(PathBuf::from(f));
        }
    } else {
        // the usual places first, then every other font on the system
        for f in [
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/ancient-scripts/Symbola_hint.ttf",
            "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSansSymbols-Regular.ttf",
        ] {
            v.push(PathBuf::from(f));
        }
        let mut more = Vec::new();
        let mut dirs = vec![PathBuf::from("/usr/share/fonts"), PathBuf::from("/usr/local/share/fonts")];
        while let Some(d) = dirs.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    dirs.push(p);
                } else if p.extension().is_some_and(|x| x == "ttf" || x == "otf") && !v.contains(&p) {
                    more.push(p);
                }
            }
        }
        more.sort();
        v.extend(more);
    }
    v
}

pub struct Fonts {
    regular: Font,
    bold: Font,
    pub px: f32,
    pub cell_w: u32,
    pub cell_h: u32,
    pub ascent: f32,
    cache: HashMap<(char, bool), (Metrics, Vec<u8>)>,
    /// Fallback fonts: the files, and each one once it was needed (None = can't be read).
    fallback_files: Vec<std::path::PathBuf>,
    fallback_fonts: Vec<Option<Option<Font>>>,
}

impl Fonts {
    pub fn new(px: f32) -> Self {
        let mut f = Fonts {
            regular: Font::from_bytes(REGULAR, FontSettings::default()).expect("font"),
            bold: Font::from_bytes(BOLD, FontSettings::default()).expect("font"),
            px,
            cell_w: 1,
            cell_h: 1,
            ascent: 0.0,
            cache: HashMap::new(),
            fallback_files: Vec::new(),
            fallback_fonts: Vec::new(),
        };
        f.set_size(px);
        f
    }

    pub fn set_size(&mut self, px: f32) {
        self.px = px.clamp(8.0, 64.0);
        self.cache.clear();
        let m = self.regular.metrics('M', self.px);
        self.cell_w = m.advance_width.ceil().max(1.0) as u32;
        let lm = self.regular.horizontal_line_metrics(self.px).expect("line metrics");
        self.ascent = lm.ascent.ceil();
        self.cell_h = (lm.ascent - lm.descent + lm.line_gap).ceil().max(1.0) as u32 + 2;
    }

    fn glyph(&mut self, c: char, bold: bool) -> &(Metrics, Vec<u8>) {
        if !self.cache.contains_key(&(c, bold)) {
            let g = self.rasterize(c, bold);
            self.cache.insert((c, bold), g);
        }
        &self.cache[&(c, bold)]
    }

    fn rasterize(&mut self, c: char, bold: bool) -> (Metrics, Vec<u8>) {
        let font = if bold { &self.bold } else { &self.regular };
        if c.is_ascii() || font.lookup_glyph_index(c) != 0 {
            return font.rasterize(c, self.px);
        }
        // not in the built-in font: the first system font that has it
        if self.fallback_files.is_empty() {
            self.fallback_files = fallback_files();
            self.fallback_fonts = vec![None; self.fallback_files.len()];
        }
        for i in 0..self.fallback_files.len() {
            if self.fallback_fonts[i].is_none() {
                let loaded = std::fs::read(&self.fallback_files[i]).ok().and_then(|bytes| Font::from_bytes(bytes, FontSettings::default()).ok());
                self.fallback_fonts[i] = Some(loaded);
            }
            let Some(Some(f)) = &self.fallback_fonts[i] else { continue };
            if f.lookup_glyph_index(c) == 0 {
                continue;
            }
            // keep it inside two cells (wide characters) and the line height
            let mut px = self.px;
            let m = f.metrics(c, px);
            let max_w = (self.cell_w * 2) as f32;
            let max_h = self.cell_h as f32;
            if m.width as f32 > max_w || m.height as f32 > max_h {
                let k = (max_w / m.width.max(1) as f32).min(max_h / m.height.max(1) as f32);
                px *= k.clamp(0.3, 1.0);
            }
            return f.rasterize(c, px);
        }
        font.rasterize(c, self.px)
    }
}

pub struct Canvas<'a> {
    pub buf: &'a mut [u32],
    pub w: u32,
    pub h: u32,
}

#[inline]
fn blend(dst: u32, src: u32, a: u32) -> u32 {
    if a == 255 {
        return src;
    }
    let ch = |shift: u32| {
        let d = (dst >> shift) & 0xff;
        let s = (src >> shift) & 0xff;
        ((s * a + d * (255 - a)) / 255) << shift
    };
    ch(16) | ch(8) | ch(0)
}

impl Canvas<'_> {
    pub fn fill(&mut self, color: u32) {
        self.buf.fill(color);
    }

    pub fn rect(&mut self, x: i32, y: i32, w: u32, h: u32, color: u32) {
        let x0 = x.max(0) as u32;
        let y0 = y.max(0) as u32;
        let x1 = ((x + w as i32).max(0) as u32).min(self.w);
        let y1 = ((y + h as i32).max(0) as u32).min(self.h);
        for yy in y0..y1 {
            let row = (yy * self.w) as usize;
            self.buf[row + x0 as usize..row + x1 as usize].fill(color);
        }
    }

    /// Draws one character with its top-left cell corner at (x, y).
    pub fn ch(&mut self, f: &mut Fonts, x: i32, y: i32, c: char, bold: bool, color: u32) {
        if c == ' ' || c == '\0' {
            return;
        }
        let ascent = f.ascent as i32;
        let (m, bitmap) = f.glyph(c, bold);
        let gx = x + m.xmin;
        let gy = y + 1 + ascent - (m.height as i32 + m.ymin);
        for row in 0..m.height {
            let py = gy + row as i32;
            if py < 0 || py >= self.h as i32 {
                continue;
            }
            for col in 0..m.width {
                let a = bitmap[row * m.width + col] as u32;
                if a == 0 {
                    continue;
                }
                let px = gx + col as i32;
                if px < 0 || px >= self.w as i32 {
                    continue;
                }
                let i = (py as u32 * self.w + px as u32) as usize;
                self.buf[i] = blend(self.buf[i], color, a);
            }
        }
    }

    /// Draws a string, one cell per character. Returns the x after the text.
    pub fn text(&mut self, f: &mut Fonts, x: i32, y: i32, s: &str, bold: bool, color: u32) -> i32 {
        let mut cx = x;
        for c in s.chars() {
            self.ch(f, cx, y, c, bold, color);
            cx += f.cell_w as i32;
        }
        cx
    }
}

#[cfg(all(test, windows))]
mod tests {
    /// Characters agents draw that the built-in font lacks get a real glyph from the
    /// system's fonts (not an empty box).
    #[test]
    fn missing_characters_come_from_system_fonts() {
        let mut f = super::Fonts::new(16.0);
        for c in ['⏺', '⎿', '✻', '✢', '⠋', '✓', '❯', '●', '🚀'] {
            let (m, bitmap) = f.glyph(c, false).clone();
            assert!(m.width > 0 && bitmap.iter().any(|&a| a > 0), "no glyph for {c:?}");
        }
    }
}
