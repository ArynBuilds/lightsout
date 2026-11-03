//! CPU rendering into a plain pixel buffer: no GPU work, nothing drawn when
//! nothing changes. Colors are 0x00RRGGBB.

use fontdue::{Font, FontSettings, Metrics};
use std::collections::HashMap;

static REGULAR: &[u8] = include_bytes!("../assets/JetBrainsMono-Regular.ttf");
static BOLD: &[u8] = include_bytes!("../assets/JetBrainsMono-Bold.ttf");

pub struct Fonts {
    regular: Font,
    bold: Font,
    pub px: f32,
    pub cell_w: u32,
    pub cell_h: u32,
    pub ascent: f32,
    cache: HashMap<(char, bool), (Metrics, Vec<u8>)>,
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
        let (regular, bold_font, px) = (&self.regular, &self.bold, self.px);
        self.cache.entry((c, bold)).or_insert_with(|| {
            let font = if bold { bold_font } else { regular };
            font.rasterize(c, px)
        })
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
