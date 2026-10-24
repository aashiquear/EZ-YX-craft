//! Floating-point greyscale image (0 = black, 1 = white) with the filters carving needs.

use crate::Bitmap;
use rayon::prelude::*;

#[derive(Debug, Clone)]
pub struct GrayImage {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl GrayImage {
    pub fn new(width: usize, height: usize, fill: f32) -> Self {
        Self { width, height, data: vec![fill; width * height] }
    }

    pub fn from_dynamic(img: &image::DynamicImage) -> Self {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let data = rgba
            .pixels()
            .map(|p| {
                let a = p[3] as f32 / 255.0;
                // Rec. 709 luma, composited on white.
                let l = (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0;
                l * a + (1.0 - a)
            })
            .collect();
        Self { width: w as usize, height: h as usize, data }
    }

    /// Alpha channel as a mask (true = opaque) – used for silhouettes from PNGs with transparency.
    pub fn alpha_mask(img: &image::DynamicImage, threshold: u8) -> Bitmap {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        Bitmap { width: w as usize, height: h as usize, data: rgba.pixels().map(|p| p[3] > threshold).collect() }
    }

    /// Resize (Lanczos) so the longest side is at most `max_side` pixels.
    pub fn from_dynamic_limited(img: &image::DynamicImage, max_side: u32) -> Self {
        let (w, h) = (img.width(), img.height());
        if w.max(h) <= max_side {
            return Self::from_dynamic(img);
        }
        let s = max_side as f32 / w.max(h) as f32;
        let r = img.resize((w as f32 * s) as u32, (h as f32 * s) as u32, image::imageops::FilterType::Lanczos3);
        Self::from_dynamic(&r)
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> f32 {
        self.data[y * self.width + x]
    }
    #[inline]
    pub fn get_clamped(&self, x: isize, y: isize) -> f32 {
        let x = x.clamp(0, self.width as isize - 1) as usize;
        let y = y.clamp(0, self.height as isize - 1) as usize;
        self.get(x, y)
    }

    /// Bilinear sample in pixel coordinates.
    pub fn sample(&self, x: f32, y: f32) -> f32 {
        let x0 = x.floor();
        let y0 = y.floor();
        let (tx, ty) = (x - x0, y - y0);
        let (xi, yi) = (x0 as isize, y0 as isize);
        let a = self.get_clamped(xi, yi) * (1.0 - tx) + self.get_clamped(xi + 1, yi) * tx;
        let b = self.get_clamped(xi, yi + 1) * (1.0 - tx) + self.get_clamped(xi + 1, yi + 1) * tx;
        a * (1.0 - ty) + b * ty
    }

    pub fn invert(&mut self) {
        self.data.par_iter_mut().for_each(|v| *v = 1.0 - *v);
    }

    /// Levels: map [black, white] → [0, 1], then gamma.
    pub fn levels(&mut self, black: f32, white: f32, gamma: f32) {
        let span = (white - black).max(1e-4);
        let g = 1.0 / gamma.max(0.01);
        self.data.par_iter_mut().for_each(|v| *v = ((*v - black) / span).clamp(0.0, 1.0).powf(g));
    }

    /// Approximate Gaussian blur (3 box passes), sigma in pixels.
    pub fn blur(&mut self, sigma: f32) {
        if sigma <= 0.0 {
            return;
        }
        // Box radius for 3 passes approximating sigma.
        let r = ((sigma * sigma * 4.0 + 1.0).sqrt() * 0.5).round().max(1.0) as isize;
        for _ in 0..3 {
            self.box_pass(r, true);
            self.box_pass(r, false);
        }
    }

    fn box_pass(&mut self, r: isize, horizontal: bool) {
        let (w, h) = (self.width, self.height);
        let src = self.data.clone();
        let n = (2 * r + 1) as f32;
        if horizontal {
            self.data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                let line = &src[y * w..(y + 1) * w];
                let at = |i: isize| line[i.clamp(0, w as isize - 1) as usize];
                let mut s: f32 = (-r..=r).map(at).sum();
                for (x, out) in row.iter_mut().enumerate() {
                    *out = s / n;
                    s += at(x as isize + r + 1) - at(x as isize - r);
                }
            });
        } else {
            let cols: Vec<Vec<f32>> = (0..w)
                .into_par_iter()
                .map(|x| {
                    let at = |i: isize| src[i.clamp(0, h as isize - 1) as usize * w + x];
                    let mut s: f32 = (-r..=r).map(at).sum();
                    (0..h)
                        .map(|y| {
                            let v = s / n;
                            s += at(y as isize + r + 1) - at(y as isize - r);
                            v
                        })
                        .collect()
                })
                .collect();
            for (x, col) in cols.into_iter().enumerate() {
                for (y, v) in col.into_iter().enumerate() {
                    self.data[y * w + x] = v;
                }
            }
        }
    }

    /// Sobel gradient magnitude (edge strength, normalised to 0..1).
    pub fn sobel(&self) -> GrayImage {
        let (w, h) = (self.width, self.height);
        let mut out = GrayImage::new(w, h, 0.0);
        out.data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, o) in row.iter_mut().enumerate() {
                let p = |dx: isize, dy: isize| self.get_clamped(x as isize + dx, y as isize + dy);
                let gx = p(1, -1) + 2.0 * p(1, 0) + p(1, 1) - p(-1, -1) - 2.0 * p(-1, 0) - p(-1, 1);
                let gy = p(-1, 1) + 2.0 * p(0, 1) + p(1, 1) - p(-1, -1) - 2.0 * p(0, -1) - p(1, -1);
                *o = (gx * gx + gy * gy).sqrt() / 5.657;
            }
        });
        out
    }

    /// Otsu's automatic threshold.
    pub fn otsu(&self) -> f32 {
        let mut hist = [0u64; 256];
        for v in &self.data {
            hist[(v.clamp(0.0, 1.0) * 255.0) as usize] += 1;
        }
        let total = self.data.len() as f64;
        let sum: f64 = hist.iter().enumerate().map(|(i, c)| i as f64 * *c as f64).sum();
        let (mut sum_b, mut w_b, mut best, mut thr) = (0.0, 0.0, 0.0, 128usize);
        for (i, c) in hist.iter().enumerate() {
            w_b += *c as f64;
            if w_b == 0.0 {
                continue;
            }
            let w_f = total - w_b;
            if w_f == 0.0 {
                break;
            }
            sum_b += i as f64 * *c as f64;
            let m_b = sum_b / w_b;
            let m_f = (sum - sum_b) / w_f;
            let between = w_b * w_f * (m_b - m_f) * (m_b - m_f);
            if between > best {
                best = between;
                thr = i;
            }
        }
        // Upper edge of the winning bin so values in that bin count as foreground.
        (thr as f32 + 1.0) / 255.0
    }

    /// Dark pixels (≤ threshold) become foreground.
    pub fn threshold(&self, t: f32) -> Bitmap {
        Bitmap { width: self.width, height: self.height, data: self.data.iter().map(|v| *v <= t).collect() }
    }

    /// Adaptive (local mean) threshold — robust for pencil sketches photographed under uneven light.
    /// Pixel is foreground when darker than the local mean by `offset`.
    pub fn adaptive_threshold(&self, radius: usize, offset: f32) -> Bitmap {
        let (w, h) = (self.width, self.height);
        // Integral image (f64 for precision on large images).
        let mut ii = vec![0f64; (w + 1) * (h + 1)];
        for y in 0..h {
            let mut row = 0.0;
            for x in 0..w {
                row += self.get(x, y) as f64;
                ii[(y + 1) * (w + 1) + x + 1] = ii[y * (w + 1) + x + 1] + row;
            }
        }
        let r = radius as isize;
        let data = (0..w * h)
            .into_par_iter()
            .map(|i| {
                let (x, y) = ((i % w) as isize, (i / w) as isize);
                let x0 = (x - r).max(0) as usize;
                let y0 = (y - r).max(0) as usize;
                let x1 = ((x + r + 1) as usize).min(w);
                let y1 = ((y + r + 1) as usize).min(h);
                let s = ii[y1 * (w + 1) + x1] - ii[y0 * (w + 1) + x1] - ii[y1 * (w + 1) + x0] + ii[y0 * (w + 1) + x0];
                let mean = s / ((x1 - x0) * (y1 - y0)) as f64;
                (self.data[i] as f64) < mean - offset as f64
            })
            .collect();
        Bitmap { width: w, height: h, data }
    }

    /// Posterise into `levels` grey levels (useful preview for multi-depth "layered" carving).
    pub fn posterize(&mut self, levels: u32) {
        let l = levels.max(2) as f32 - 1.0;
        self.data.par_iter_mut().for_each(|v| *v = (*v * l).round() / l);
    }

    pub fn to_image(&self) -> image::GrayImage {
        image::GrayImage::from_raw(self.width as u32, self.height as u32, self.data.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8).collect())
            .expect("dimensions")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn otsu_bimodal() {
        let mut g = GrayImage::new(10, 10, 0.1);
        for v in g.data.iter_mut().skip(50) {
            *v = 0.9;
        }
        let t = g.otsu();
        assert!(t > 0.09 && t < 0.9, "{t}");
        assert_eq!(g.threshold(t).count(), 50);
    }
    #[test]
    fn blur_preserves_mean() {
        let mut g = GrayImage::new(32, 32, 0.0);
        g.data[16 * 32 + 16] = 1.0;
        g.blur(2.0);
        let s: f32 = g.data.iter().sum();
        assert!((s - 1.0).abs() < 0.05, "{s}");
    }
}
