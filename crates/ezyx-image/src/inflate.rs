//! Image → 3D baseline: silhouette inflation ("balloon" modelling).
//!
//! The silhouette's Euclidean distance transform `d` is turned into a rounded profile
//! `h = R·sqrt(1 − (1 − min(d, R)/R)²)` (a circular bulge with rim radius `R`) and luminance
//! detail is optionally embossed on top. Front and back heightmaps are returned; `ezyx-mesh`
//! stitches them into a watertight mesh (`ezyx_mesh::img3d`).
//!
//! This runs offline in milliseconds and is a good fit for "cookie-cutter" style 3D carvings,
//! toys, and rotary blanks. Photo-real single-image reconstruction (Tripo3D / TripoSR-style)
//! needs a neural model; `ezyx-mesh::img3d::Backend` defines the interface for plugging in a
//! local ONNX model or a remote API.

use crate::{Bitmap, GrayImage};
use ezyx_core::{DVec2, Heightmap};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InflateOptions {
    pub width_mm: f64,
    /// Thickness of the front half at the thickest point (mm).
    pub thickness: f64,
    /// Rim rounding radius as a fraction of the max inscribed radius (0..1].
    pub roundness: f64,
    /// Emboss luminance detail (mm, 0 = off).
    pub detail_depth: f64,
    /// Back side mirrors the front (true) or is flat (false = relief-like plaque).
    pub symmetric: bool,
    pub resolution: f64,
}

impl Default for InflateOptions {
    fn default() -> Self {
        Self { width_mm: 100.0, thickness: 12.0, roundness: 0.6, detail_depth: 0.8, symmetric: true, resolution: 0.4 }
    }
}

pub struct Inflated {
    /// Front surface heights (≥ 0 inside the silhouette, 0 outside).
    pub front: Heightmap,
    /// Back surface depths (≤ 0 inside the silhouette, 0 outside).
    pub back: Heightmap,
    /// Silhouette resampled to the heightmap grid.
    pub inside: Vec<bool>,
}

pub fn inflate(img: &GrayImage, silhouette: &Bitmap, opt: &InflateOptions) -> Inflated {
    let aspect = img.height as f64 / img.width.max(1) as f64;
    let size = DVec2::new(opt.width_mm, opt.width_mm * aspect);
    let mut front = Heightmap::covering(DVec2::ZERO, size, opt.resolution, 0.0);
    let (gw, gh) = (front.width, front.height);
    // Resample silhouette to grid.
    let mut grid = Bitmap::new(gw, gh);
    let (sx, sy) = (img.width as f64 / size.x, img.height as f64 / size.y);
    for y in 0..gh {
        for x in 0..gw {
            let p = front.cell_center(x, y);
            let ix = ((p.x * sx) as isize).clamp(0, img.width as isize - 1);
            let iy = (((size.y - p.y) * sy) as isize).clamp(0, img.height as isize - 1);
            grid.set(x, y, silhouette.get(ix, iy));
        }
    }
    let dt = grid.distance_transform();
    let dmax = dt.iter().cloned().fold(0.0, f32::max).max(1.0) as f64;
    let r = (dmax * opt.roundness.clamp(0.05, 1.0)).max(1.0);
    let mut g = img.clone();
    g.blur(1.0);
    for y in 0..gh {
        for x in 0..gw {
            let i = y * gw + x;
            if !grid.data[i] {
                continue;
            }
            let d = (dt[i] as f64).min(r);
            let t = 1.0 - d / r;
            let bulge = (1.0 - t * t).max(0.0).sqrt();
            let mut h = bulge * opt.thickness;
            if opt.detail_depth > 0.0 {
                let p = front.cell_center(x, y);
                let lum = g.sample((p.x * sx) as f32, ((size.y - p.y) * sy) as f32) as f64;
                h += (lum - 0.5) * opt.detail_depth * bulge;
            }
            front.data[i] = h.max(0.01) as f32;
        }
    }
    let mut back = front.clone();
    if opt.symmetric {
        back.map(|v| -v);
    } else {
        back.map(|v| if v > 0.0 { -0.01 } else { 0.0 });
    }
    Inflated { front, back, inside: grid.data }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disc_inflates_to_dome() {
        let n = 64;
        let mut g = GrayImage::new(n, n, 1.0);
        let mut b = Bitmap::new(n, n);
        for y in 0..n {
            for x in 0..n {
                if ((x as f32 - 32.0).powi(2) + (y as f32 - 32.0).powi(2)).sqrt() < 25.0 {
                    b.set(x, y, true);
                    g.data[y * n + x] = 0.5;
                }
            }
        }
        let inf = inflate(&g, &b, &InflateOptions { width_mm: 64.0, resolution: 1.0, detail_depth: 0.0, roundness: 1.0, ..Default::default() });
        let (_, hi) = inf.front.min_max();
        assert!((hi as f64 - 12.0).abs() < 0.6, "{hi}");
        assert_eq!(inf.back.min_max().0, -hi);
    }
}
