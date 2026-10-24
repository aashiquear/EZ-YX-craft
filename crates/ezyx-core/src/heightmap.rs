//! Regular grid height field. Used for:
//! * 2.5D / 3D reliefs (image → relief, mesh → relief),
//! * 3D raster toolpaths (drop-cutter runs on a heightmap),
//! * stock simulation (the dexel/Z-map in `ezyx-sim`).
//!
//! `data[y * width + x]` is the surface Z (mm) at cell centre
//! `origin + ((x + 0.5) * cell, (y + 0.5) * cell)`.

use crate::{Bounds2, DVec2};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Heightmap {
    pub width: usize,
    pub height: usize,
    /// Cell size in mm (square cells).
    pub cell: f64,
    /// World XY of the grid's lower-left corner.
    pub origin: DVec2,
    pub data: Vec<f32>,
}

impl Heightmap {
    pub fn new(width: usize, height: usize, cell: f64, origin: DVec2, fill: f32) -> Self {
        Self { width, height, cell, origin, data: vec![fill; width * height] }
    }

    /// Grid covering `size` mm at resolution `cell` mm.
    pub fn covering(origin: DVec2, size: DVec2, cell: f64, fill: f32) -> Self {
        let w = ((size.x / cell).ceil() as usize).max(1);
        let h = ((size.y / cell).ceil() as usize).max(1);
        Self::new(w, h, cell, origin, fill)
    }

    #[inline]
    pub fn idx(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }
    #[inline]
    pub fn get(&self, x: usize, y: usize) -> f32 {
        self.data[self.idx(x, y)]
    }
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, v: f32) {
        let i = self.idx(x, y);
        self.data[i] = v;
    }

    pub fn cell_center(&self, x: usize, y: usize) -> DVec2 {
        self.origin + DVec2::new((x as f64 + 0.5) * self.cell, (y as f64 + 0.5) * self.cell)
    }

    /// Continuous grid coordinates of a world point.
    pub fn to_grid(&self, p: DVec2) -> DVec2 {
        (p - self.origin) / self.cell - DVec2::splat(0.5)
    }

    pub fn bounds(&self) -> Bounds2 {
        Bounds2 { min: self.origin, max: self.origin + DVec2::new(self.width as f64, self.height as f64) * self.cell }
    }

    /// Bilinear sample; points outside the grid are clamped to the border.
    pub fn sample(&self, p: DVec2) -> f32 {
        let g = self.to_grid(p);
        let x0 = g.x.floor().clamp(0.0, (self.width - 1) as f64);
        let y0 = g.y.floor().clamp(0.0, (self.height - 1) as f64);
        let x1 = (x0 + 1.0).min((self.width - 1) as f64);
        let y1 = (y0 + 1.0).min((self.height - 1) as f64);
        let tx = (g.x - x0).clamp(0.0, 1.0) as f32;
        let ty = (g.y - y0).clamp(0.0, 1.0) as f32;
        let (x0, y0, x1, y1) = (x0 as usize, y0 as usize, x1 as usize, y1 as usize);
        let a = self.get(x0, y0) * (1.0 - tx) + self.get(x1, y0) * tx;
        let b = self.get(x0, y1) * (1.0 - tx) + self.get(x1, y1) * tx;
        a * (1.0 - ty) + b * ty
    }

    pub fn min_max(&self) -> (f32, f32) {
        self.data.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| (lo.min(*v), hi.max(*v)))
    }

    pub fn map(&mut self, f: impl Fn(f32) -> f32 + Sync) {
        self.data.par_iter_mut().for_each(|v| *v = f(*v));
    }

    /// Rescale values linearly into `[lo, hi]`.
    pub fn normalize(&mut self, lo: f32, hi: f32) {
        let (a, b) = self.min_max();
        let span = (b - a).max(1e-9);
        self.map(|v| lo + (v - a) / span * (hi - lo));
    }

    /// Separable box blur, `radius` in cells. Run 2–3 times for a near-Gaussian.
    pub fn box_blur(&mut self, radius: usize) {
        if radius == 0 {
            return;
        }
        let (w, h) = (self.width, self.height);
        let r = radius as isize;
        let mut tmp = vec![0f32; w * h];
        tmp.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, out) in row.iter_mut().enumerate() {
                let mut s = 0.0;
                let mut n = 0.0;
                for dx in -r..=r {
                    let xx = (x as isize + dx).clamp(0, w as isize - 1) as usize;
                    s += self.data[y * w + xx];
                    n += 1.0;
                }
                *out = s / n;
            }
        });
        let src = tmp;
        self.data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, out) in row.iter_mut().enumerate() {
                let mut s = 0.0;
                let mut n = 0.0;
                for dy in -r..=r {
                    let yy = (y as isize + dy).clamp(0, h as isize - 1) as usize;
                    s += src[yy * w + x];
                    n += 1.0;
                }
                *out = s / n;
            }
        });
    }

    /// Combine with another heightmap of identical dimensions.
    pub fn combine(&mut self, other: &Heightmap, mode: CombineMode) {
        assert_eq!(self.data.len(), other.data.len(), "heightmap size mismatch");
        self.data.par_iter_mut().zip(other.data.par_iter()).for_each(|(a, b)| {
            *a = match mode {
                CombineMode::Add => *a + *b,
                CombineMode::Subtract => *a - *b,
                CombineMode::Max => a.max(*b),
                CombineMode::Min => a.min(*b),
                CombineMode::Replace => *b,
            }
        });
    }

    /// Volume between the surface and `base_z` (mm³), counting only material above base.
    pub fn volume_above(&self, base_z: f32) -> f64 {
        let a = self.cell * self.cell;
        self.data.iter().map(|v| ((v - base_z).max(0.0)) as f64 * a).sum()
    }

    /// Surface normal via central differences (for shading).
    pub fn normal(&self, x: usize, y: usize) -> [f32; 3] {
        let xl = x.saturating_sub(1);
        let xr = (x + 1).min(self.width - 1);
        let yd = y.saturating_sub(1);
        let yu = (y + 1).min(self.height - 1);
        let dx = (self.get(xr, y) - self.get(xl, y)) / (((xr - xl).max(1)) as f64 * self.cell) as f32;
        let dy = (self.get(x, yu) - self.get(x, yd)) / (((yu - yd).max(1)) as f64 * self.cell) as f32;
        let n = [-dx, -dy, 1.0];
        let l = (n[0] * n[0] + n[1] * n[1] + 1.0).sqrt();
        [n[0] / l, n[1] / l, n[2] / l]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CombineMode {
    Add,
    Subtract,
    Max,
    Min,
    Replace,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_bilinear() {
        let mut h = Heightmap::new(2, 1, 1.0, DVec2::ZERO, 0.0);
        h.set(1, 0, 10.0);
        assert!((h.sample(DVec2::new(1.0, 0.5)) - 5.0).abs() < 1e-5);
    }
    #[test]
    fn volume() {
        let h = Heightmap::new(10, 10, 0.5, DVec2::ZERO, 2.0);
        assert!((h.volume_above(0.0) - 50.0).abs() < 1e-9);
    }
}
