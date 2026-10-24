//! Photo V-carve: reproduce a shaded photo with parallel V-grooves whose depth (and therefore
//! width) follows image darkness. Dark → deep/wide groove, light → shallow or no groove.
//! With a 60° or 90° bit in a light hardwood (maple, basswood) or painted MDF this gives the
//! classic "engraved photo" look; in anodised aluminium with an engraving bit it gives a
//! metal photo plate.

use crate::GrayImage;
use ezyx_core::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GroovePattern {
    #[default]
    Lines,
    /// Sine-wave lines (more organic look).
    Waves,
    /// Concentric circles from the image centre.
    Circles,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhotoVCarveOptions {
    pub width_mm: f64,
    /// Distance between groove centrelines (mm).
    pub spacing: f64,
    /// V-bit included angle (degrees).
    pub bit_angle: f64,
    /// Fraction of spacing the darkest groove may occupy (0..1). 0.9 leaves thin ridges.
    pub max_coverage: f64,
    /// Grooves shallower than this are skipped (lifts over highlights).
    pub min_depth: f64,
    pub angle_deg: f64,
    pub pattern: GroovePattern,
    pub gamma: f32,
    pub contrast: f32,
    /// Sample spacing along the groove (mm).
    pub sample: f64,
    pub origin: DVec2,
}

impl Default for PhotoVCarveOptions {
    fn default() -> Self {
        Self {
            width_mm: 150.0,
            spacing: 1.2,
            bit_angle: 60.0,
            max_coverage: 0.92,
            min_depth: 0.03,
            angle_deg: 0.0,
            pattern: GroovePattern::Lines,
            gamma: 1.0,
            contrast: 1.0,
            sample: 0.15,
            origin: DVec2::ZERO,
        }
    }
}

impl PhotoVCarveOptions {
    /// Depth of the darkest groove.
    pub fn max_depth(&self) -> f64 {
        let half = (self.bit_angle * 0.5).to_radians().tan().max(1e-3);
        self.spacing * self.max_coverage * 0.5 / half
    }
}

/// Returns 3D polylines (Z ≤ 0) ready for `ezyx_cam::ops::polyline3d::generate`.
pub fn grooves(img: &GrayImage, opt: &PhotoVCarveOptions) -> Vec<Vec<DVec3>> {
    let aspect = img.height as f64 / img.width.max(1) as f64;
    let size = DVec2::new(opt.width_mm, opt.width_mm * aspect);
    let (sx, sy) = (img.width as f64 / size.x, img.height as f64 / size.y);
    let max_d = opt.max_depth();
    let g_inv = 1.0 / opt.gamma.max(0.01);
    let darkness = |p: DVec2| -> Option<f64> {
        if p.x < 0.0 || p.y < 0.0 || p.x > size.x || p.y > size.y {
            return None;
        }
        let v = img.sample((p.x * sx) as f32, ((size.y - p.y) * sy) as f32);
        let d = (((1.0 - v) - 0.5) * opt.contrast + 0.5).clamp(0.0, 1.0).powf(g_inv);
        Some(d as f64)
    };
    // Build centreline curves in local (unrotated) space covering the rotated bounds.
    let center = size * 0.5;
    let (s, c) = opt.angle_deg.to_radians().sin_cos();
    let rot = |p: DVec2| center + DVec2::new((p.x - center.x) * c - (p.y - center.y) * s, (p.x - center.x) * s + (p.y - center.y) * c);
    let diag = size.length();
    let mut curves: Vec<Vec<DVec2>> = Vec::new();
    match opt.pattern {
        GroovePattern::Lines | GroovePattern::Waves => {
            let n = (diag / opt.spacing).ceil() as i64;
            for i in -n / 2..=n / 2 {
                let y = center.y + i as f64 * opt.spacing;
                let steps = (diag / opt.sample).ceil() as usize;
                let mut line = Vec::with_capacity(steps + 1);
                for k in 0..=steps {
                    let x = center.x - diag / 2.0 + k as f64 * opt.sample;
                    let yy = if opt.pattern == GroovePattern::Waves { y + (x * 0.15).sin() * opt.spacing * 0.6 } else { y };
                    line.push(rot(DVec2::new(x, yy)));
                }
                curves.push(line);
            }
        }
        GroovePattern::Circles => {
            let n = (diag * 0.5 / opt.spacing).ceil() as usize;
            for i in 1..=n {
                let r = i as f64 * opt.spacing;
                let steps = ((std::f64::consts::TAU * r) / opt.sample).ceil().max(12.0) as usize;
                curves.push((0..=steps).map(|k| center + DVec2::from_angle(std::f64::consts::TAU * k as f64 / steps as f64) * r).collect());
            }
        }
    }
    // Sample depth, split where too shallow or outside the image, alternate direction.
    let mut out = Vec::new();
    for (i, curve) in curves.into_iter().enumerate() {
        let mut seg: Vec<DVec3> = Vec::new();
        let mut segs = Vec::new();
        for p in curve {
            match darkness(p) {
                Some(d) if d * max_d >= opt.min_depth => seg.push(DVec3::new(p.x + opt.origin.x, p.y + opt.origin.y, -d * max_d)),
                _ => {
                    if seg.len() > 1 {
                        segs.push(std::mem::take(&mut seg));
                    }
                    seg.clear();
                }
            }
        }
        if seg.len() > 1 {
            segs.push(seg);
        }
        if i % 2 == 1 {
            segs.reverse();
            for s in &mut segs {
                s.reverse();
            }
        }
        out.extend(segs);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gradient_depths() {
        // Left white → right black.
        let mut g = GrayImage::new(100, 50, 1.0);
        for y in 0..50 {
            for x in 0..100 {
                g.data[y * 100 + x] = 1.0 - x as f32 / 99.0;
            }
        }
        let opt = PhotoVCarveOptions { width_mm: 100.0, ..Default::default() };
        let gs = grooves(&g, &opt);
        assert!(!gs.is_empty());
        let deepest = gs.iter().flatten().map(|p| p.z).fold(0.0, f64::min);
        assert!((deepest + opt.max_depth()).abs() < 0.05);
        // Depth increases with x on a horizontal groove.
        let s = &gs[0];
        let (a, b) = (s.first().unwrap(), s.last().unwrap());
        let (left, right) = if a.x < b.x { (a, b) } else { (b, a) };
        assert!(right.z < left.z);
    }
}
