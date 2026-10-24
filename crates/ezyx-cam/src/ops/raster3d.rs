//! 3D roughing and finishing on a heightmap using the drop-cutter algorithm.
//!
//! For a tool centred at `c`, the lowest tip height that does not gouge the model is
//! `max over cells q in the footprint of (h(q) − profile(|q − c|))`, where `profile` is the
//! cutter's height above its tip at radius r (works for flat, ball, V, tapered-ball, drill).
//! The kernel is precomputed once per tool and rows are processed in parallel with rayon.
//! A GPU compute version (same kernel, one invocation per sample) lives behind `ezyx-gpu`.

use crate::link::PathBuilder;
use crate::ops::polyline3d::simplify3;
use crate::{pass_levels, CutParams, Toolpath};
use ezyx_core::{DVec2, DVec3, Heightmap};
use ezyx_tooldb::Tool;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RasterDir {
    #[default]
    X,
    Y,
    /// X then Y (cross-hatch finishing).
    XY,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Raster3dOptions {
    pub direction: RasterDir,
    /// Material left for finishing (roughing only).
    pub allowance: f64,
    pub roughing: bool,
    /// Path point tolerance for simplification.
    pub tolerance: f64,
    /// Restrict machining to the model boundary (+ tool radius) instead of the whole grid.
    pub boundary_only: bool,
}

impl Default for Raster3dOptions {
    fn default() -> Self {
        Self { direction: RasterDir::X, allowance: 0.0, roughing: false, tolerance: 0.005, boundary_only: false }
    }
}

/// Precomputed drop-cutter kernel: (dx, dy, profile_height) in cells.
pub struct DropKernel {
    taps: Vec<(i32, i32, f32)>,
}

impl DropKernel {
    pub fn new(tool: &Tool, cell: f64) -> Self {
        let r = tool.radius();
        let n = (r / cell).ceil() as i32;
        let mut taps = Vec::new();
        for dy in -n..=n {
            for dx in -n..=n {
                let d = ((dx * dx + dy * dy) as f64).sqrt() * cell;
                if let Some(h) = tool.profile_height(d) {
                    taps.push((dx, dy, h as f32));
                }
            }
        }
        // Sort by profile height so the inner loop can early-out (low taps first matter most).
        taps.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
        Self { taps }
    }

    /// Tip Z for a tool centred over grid cell (cx, cy).
    pub fn drop_at(&self, hm: &Heightmap, cx: i64, cy: i64, floor: f32) -> f32 {
        let (w, h) = (hm.width as i64, hm.height as i64);
        let mut z = floor;
        for &(dx, dy, ph) in &self.taps {
            let x = cx + dx as i64;
            let y = cy + dy as i64;
            if x < 0 || y < 0 || x >= w || y >= h {
                continue;
            }
            let v = hm.data[(y * w + x) as usize] - ph;
            if v > z {
                z = v;
            }
        }
        z
    }
}

/// Tool-tip surface over the whole grid (the "offset surface"). Useful for simulation & GPU parity.
pub fn tip_surface(hm: &Heightmap, tool: &Tool, floor: f32) -> Heightmap {
    let k = DropKernel::new(tool, hm.cell);
    let mut out = hm.clone();
    out.data.par_chunks_mut(hm.width).enumerate().for_each(|(y, row)| {
        for (x, v) in row.iter_mut().enumerate() {
            *v = k.drop_at(hm, x as i64, y as i64, floor);
        }
    });
    out
}

/// Scan lines (in grid space) with tip Z, before linking.
fn scanlines(hm: &Heightmap, tool: &Tool, stepover: f64, dir: RasterDir, allowance: f64, floor: f32) -> Vec<Vec<DVec3>> {
    let k = DropKernel::new(tool, hm.cell);
    let step_cells = (stepover / hm.cell).max(1.0);
    let mut jobs: Vec<(bool, f64)> = Vec::new();
    let along_x = matches!(dir, RasterDir::X | RasterDir::XY);
    let along_y = matches!(dir, RasterDir::Y | RasterDir::XY);
    if along_x {
        let mut y = step_cells * 0.5;
        while y < hm.height as f64 {
            jobs.push((true, y));
            y += step_cells;
        }
    }
    if along_y {
        let mut x = step_cells * 0.5;
        while x < hm.width as f64 {
            jobs.push((false, x));
            x += step_cells;
        }
    }
    let mut lines: Vec<Vec<DVec3>> = jobs
        .par_iter()
        .map(|&(is_x, c)| {
            let n = if is_x { hm.width } else { hm.height };
            let ci = c.floor() as i64;
            (0..n)
                .map(|i| {
                    let (gx, gy) = if is_x { (i as i64, ci) } else { (ci, i as i64) };
                    let z = k.drop_at(hm, gx, gy, floor) as f64 + allowance;
                    let p = hm.cell_center(gx as usize, gy.clamp(0, hm.height as i64 - 1) as usize);
                    DVec3::new(p.x, p.y, z)
                })
                .collect()
        })
        .collect();
    // Zig-zag.
    for (i, l) in lines.iter_mut().enumerate() {
        if i % 2 == 1 {
            l.reverse();
        }
    }
    lines
}

/// 3D finishing (or single-level roughing when `opt.roughing == false`).
pub fn generate(name: &str, hm: &Heightmap, p: &CutParams, opt: &Raster3dOptions) -> Toolpath {
    let mut b = PathBuilder::new(&p.tool, &p.cutting, p.safe_z);
    let floor = -(p.cut_depth as f32);
    b.comment(format!("3D {} raster {:?}, stepover {:.3}", if opt.roughing { "roughing" } else { "finishing" }, opt.direction, p.cutting.stepover));
    let lines = scanlines(hm, &p.tool, p.cutting.stepover, opt.direction, opt.allowance, floor);
    let surface = -p.start_depth;
    if !opt.roughing {
        for (i, l) in lines.iter().enumerate() {
            let s = simplify3(l, opt.tolerance);
            // Stay down between adjacent zig-zag lines when the link is short.
            if i > 0 {
                if let (Some(c), Some(first)) = (b.current(), s.first()) {
                    if DVec2::new(c.x, c.y).distance(first.truncate()) < p.cutting.stepover * 1.5 {
                        b.link_direct(crate::Pt::from_v(DVec3::new(first.x, first.y, first.z.max(c.z))));
                    }
                }
            }
            b.cut_polyline3(&s, surface);
        }
        return b.finish(name, &p.tool, false);
    }
    // Roughing: z-level slicing of the offset surface. At each level cut wherever the target lies
    // below the previous level, at max(level, target).
    let mut prev_level = -p.start_depth;
    for level in pass_levels(p.start_depth, p.cut_depth, p.cutting.stepdown) {
        for l in &lines {
            let mut seg: Vec<DVec3> = Vec::new();
            let flush = |seg: &mut Vec<DVec3>, b: &mut PathBuilder| {
                if seg.len() > 1 {
                    let s = simplify3(seg, opt.tolerance);
                    b.cut_polyline3(&s, surface);
                    b.retract();
                }
                seg.clear();
            };
            for q in l {
                if q.z < prev_level - 1e-6 {
                    seg.push(DVec3::new(q.x, q.y, level.max(q.z)));
                } else {
                    flush(&mut seg, &mut b);
                }
            }
            flush(&mut seg, &mut b);
        }
        prev_level = level;
    }
    b.finish(name, &p.tool, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::testutil::params;

    fn dome() -> Heightmap {
        let mut hm = Heightmap::covering(DVec2::ZERO, DVec2::new(40.0, 40.0), 0.25, -10.0);
        for y in 0..hm.height {
            for x in 0..hm.width {
                let c = hm.cell_center(x, y) - DVec2::new(20.0, 20.0);
                let r2 = 225.0 - c.length_squared();
                if r2 > 0.0 {
                    hm.set(x, y, (r2.sqrt() - 15.0) as f32 * 0.6);
                }
            }
        }
        hm
    }

    #[test]
    fn ball_never_gouges() {
        let hm = dome();
        let p = params("generic.ball_1_8", 10.0);
        let tp = generate("f", &hm, &p, &Raster3dOptions { tolerance: 0.0, ..Default::default() });
        let r = p.tool.radius();
        let n = (r / hm.cell).ceil() as i64;
        for (q, rapid) in tp.points().step_by(5) {
            if rapid {
                continue;
            }
            let g = hm.to_grid(DVec2::new(q.x, q.y));
            let (cx, cy) = (g.x.round() as i64, g.y.round() as i64);
            for dy in -n..=n {
                for dx in -n..=n {
                    let (x, y) = (cx + dx, cy + dy);
                    if x < 0 || y < 0 || x >= hm.width as i64 || y >= hm.height as i64 {
                        continue;
                    }
                    let c = hm.cell_center(x as usize, y as usize);
                    if let Some(ph) = p.tool.profile_height(c.distance(DVec2::new(q.x, q.y))) {
                        let model = hm.get(x as usize, y as usize) as f64;
                        assert!(q.z + ph >= model - 0.06, "gouge {:.3} at {:?}", model - q.z - ph, q);
                    }
                }
            }
        }
        assert!(tp.stats(3000.0).cut_length > 100.0);
    }

    #[test]
    fn roughing_levels() {
        let hm = dome();
        let mut p = params("generic.flat_1_4_2f", 10.0);
        p.cutting.stepdown = 2.0;
        let tp = generate("r", &hm, &p, &Raster3dOptions { roughing: true, allowance: 0.3, ..Default::default() });
        let st = tp.stats(3000.0);
        assert!(st.min[2] >= -10.0 - 1e-6);
        assert!(st.cut_length > 50.0);
    }
}
