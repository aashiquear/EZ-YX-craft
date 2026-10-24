//! Text → outline vectors using any TrueType/OpenType font (ttf-parser, pure Rust).

use ezyx_core::{path::shapes::cubic_bezier, DVec2, Polyline};

struct Builder {
    loops: Vec<Polyline>,
    cur: Vec<DVec2>,
    scale: f64,
    origin: DVec2,
    pen: DVec2,
    tol: f64,
}

impl Builder {
    fn p(&self, x: f32, y: f32) -> DVec2 {
        self.origin + DVec2::new(x as f64, y as f64) * self.scale
    }
    fn flush(&mut self) {
        if self.cur.len() >= 3 {
            self.loops.push(Polyline::closed(std::mem::take(&mut self.cur)));
        }
        self.cur.clear();
    }
}

impl ttf_parser::OutlineBuilder for Builder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.flush();
        let p = self.p(x, y);
        self.cur.push(p);
        self.pen = p;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.p(x, y);
        self.cur.push(p);
        self.pen = p;
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (p0, c, p3) = (self.pen, self.p(x1, y1), self.p(x, y));
        // Elevate quadratic to cubic.
        let c1 = p0 + (c - p0) * (2.0 / 3.0);
        let c2 = p3 + (c - p3) * (2.0 / 3.0);
        self.cur.extend(cubic_bezier(p0, c1, c2, p3, self.tol));
        self.pen = p3;
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (p0, c1, c2, p3) = (self.pen, self.p(x1, y1), self.p(x2, y2), self.p(x, y));
        self.cur.extend(cubic_bezier(p0, c1, c2, p3, self.tol));
        self.pen = p3;
    }
    fn close(&mut self) {
        self.flush();
    }
}

/// Lay out a single line of `text` with cap height ≈ `height` mm, baseline-left at `origin`.
/// `letter_spacing` is extra mm between glyphs.
pub fn text_outlines(font_data: &[u8], text: &str, height: f64, origin: DVec2, letter_spacing: f64) -> Result<Vec<Polyline>, String> {
    let face = ttf_parser::Face::parse(font_data, 0).map_err(|e| format!("font: {e}"))?;
    let cap = face.capital_height().map(|c| c as f64).unwrap_or(face.ascender() as f64 * 0.7);
    let scale = height / cap.max(1.0);
    let mut b = Builder { loops: vec![], cur: vec![], scale, origin, pen: origin, tol: 0.01 };
    let mut x = 0.0f64;
    let mut prev: Option<ttf_parser::GlyphId> = None;
    for ch in text.chars() {
        if ch == '\n' {
            continue;
        }
        let Some(gid) = face.glyph_index(ch) else {
            x += height * 0.5;
            continue;
        };
        if let (Some(pg), Some(kern)) = (prev, face.tables().kern) {
            for sub in kern.subtables {
                if let Some(k) = sub.glyphs_kerning(pg, gid) {
                    x += k as f64 * scale;
                    break;
                }
            }
        }
        b.origin = origin + DVec2::new(x, 0.0);
        face.outline_glyph(gid, &mut b);
        b.flush();
        x += face.glyph_hor_advance(gid).unwrap_or(0) as f64 * scale + letter_spacing;
        prev = Some(gid);
    }
    Ok(b.loops)
}

/// Try common system font locations (Linux / macOS / Windows).
pub fn find_system_font() -> Option<std::path::PathBuf> {
    let candidates = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/Library/Fonts/Arial.ttf",
        "C:\\Windows\\Fonts\\arialbd.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
    ];
    candidates.iter().map(std::path::PathBuf::from).find(|p| p.exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn renders_with_system_font_if_available() {
        let Some(f) = find_system_font() else { return };
        let data = std::fs::read(f).unwrap();
        let loops = text_outlines(&data, "OAK", 20.0, DVec2::ZERO, 0.0).unwrap();
        // O has 2 loops, A has 2, K has 1.
        assert!(loops.len() >= 5, "{}", loops.len());
        let b = loops.iter().fold(ezyx_core::Bounds2::EMPTY, |a, l| a.union(&l.bounds()));
        assert!(b.size().y > 18.0 && b.size().y < 25.0, "{:?}", b.size());
    }
}
