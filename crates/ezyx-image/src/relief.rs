//! Image → 2.5D relief heightmap.

use crate::GrayImage;
use ezyx_core::{DVec2, Heightmap};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReliefOptions {
    /// Physical size (mm).
    pub width_mm: f64,
    /// Max relief depth below the stock top (mm).
    pub depth: f64,
    /// Bright = high (true) or dark = high (false). Photos usually want bright = high;
    /// lithophane-style carving wants dark = deep.
    pub bright_is_high: bool,
    pub gamma: f32,
    pub blur: f32,
    /// Heightmap cell size (mm). Smaller = more detail, slower toolpaths.
    pub resolution: f64,
    /// Flatten the background: values within this fraction of the background level become flat.
    pub background_cutoff: f32,
    pub origin: DVec2,
}

impl Default for ReliefOptions {
    fn default() -> Self {
        Self { width_mm: 150.0, depth: 6.0, bright_is_high: true, gamma: 1.0, blur: 1.0, resolution: 0.2, background_cutoff: 0.0, origin: DVec2::ZERO }
    }
}

/// Heights are ≤ 0 (0 = stock top) so the result can be fed straight to the 3D raster toolpath.
pub fn image_to_relief(img: &GrayImage, opt: &ReliefOptions) -> Heightmap {
    let mut g = img.clone();
    g.blur(opt.blur);
    let aspect = img.height as f64 / img.width.max(1) as f64;
    let size = DVec2::new(opt.width_mm, opt.width_mm * aspect);
    let mut hm = Heightmap::covering(opt.origin, size, opt.resolution, 0.0);
    let (sx, sy) = (img.width as f64 / size.x, img.height as f64 / size.y);
    let g_inv = 1.0 / opt.gamma.max(0.01);
    for y in 0..hm.height {
        for x in 0..hm.width {
            let p = hm.cell_center(x, y) - opt.origin;
            let v = g.sample((p.x * sx) as f32, ((size.y - p.y) * sy) as f32);
            let mut t = if opt.bright_is_high { v } else { 1.0 - v };
            if t < opt.background_cutoff {
                t = 0.0;
            }
            t = t.clamp(0.0, 1.0).powf(g_inv);
            hm.set(x, y, ((t as f64 - 1.0) * opt.depth) as f32);
        }
    }
    hm
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn range() {
        let mut g = GrayImage::new(20, 10, 0.0);
        g.data[5] = 1.0;
        let hm = image_to_relief(&g, &ReliefOptions { width_mm: 20.0, resolution: 1.0, blur: 0.0, ..Default::default() });
        let (lo, hi) = hm.min_max();
        assert!(lo >= -6.0 - 1e-5 && hi <= 1e-5);
        assert_eq!(hm.width, 20);
        assert_eq!(hm.height, 10);
    }
}
