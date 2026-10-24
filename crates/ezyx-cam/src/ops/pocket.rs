//! Pocketing: offset (contour-parallel) or raster (zig-zag) clearing of closed regions.

use crate::link::{order_loops, PathBuilder};
use crate::{CutDirection, CutParams, Pt, Toolpath};
use ezyx_core::{region::Region, DVec2, DVec3, Polyline};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PocketStyle {
    #[default]
    Offset,
    Raster,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PocketOptions {
    pub style: PocketStyle,
    /// Raster angle in degrees (0 = along X).
    pub raster_angle: f64,
    /// Wall allowance.
    pub allowance: f64,
    pub ramp_length: f64,
    /// For raster pockets: finish with a boundary pass.
    pub boundary_pass: bool,
}

impl Default for PocketOptions {
    fn default() -> Self {
        Self { style: PocketStyle::Offset, raster_angle: 0.0, allowance: 0.0, ramp_length: 8.0, boundary_pass: true }
    }
}

/// Offset rings from innermost to outermost (clearing outward, finishing on the wall).
pub fn offset_rings(region: &Region, tool_r: f64, stepover: f64, allowance: f64, dir: CutDirection) -> Vec<Vec<Polyline>> {
    let mut levels = Vec::new();
    let mut d = tool_r + allowance;
    loop {
        let off = region.offset(-d);
        if off.is_empty() {
            break;
        }
        let mut rings = off.rings();
        // Pocket walls: material is outside the ring → reverse for climb.
        if dir == CutDirection::Climb {
            rings = rings.into_iter().map(|r| r.reversed()).collect();
        }
        levels.push(rings);
        d += stepover.max(0.01);
        if levels.len() > 10_000 {
            break;
        }
    }
    levels.reverse();
    levels
}

/// Raster scanlines clipped to `region` (already offset by tool radius), zig-zag ordered.
pub fn raster_lines(region: &Region, stepover: f64, angle_deg: f64) -> Vec<(DVec2, DVec2)> {
    let rot = |p: DVec2, a: f64| {
        let (s, c) = a.sin_cos();
        DVec2::new(p.x * c - p.y * s, p.x * s + p.y * c)
    };
    let a = angle_deg.to_radians();
    let rings: Vec<Polyline> = region.rings().into_iter().map(|r| Polyline::closed(r.pts.iter().map(|p| rot(*p, -a)).collect())).collect();
    let b = rings.iter().fold(ezyx_core::Bounds2::EMPTY, |acc, r| acc.union(&r.bounds()));
    if b.is_empty() {
        return vec![];
    }
    let mut out = Vec::new();
    let mut y = b.min.y + stepover * 0.5;
    let mut flip = false;
    while y < b.max.y {
        let mut xs: Vec<f64> = Vec::new();
        for r in &rings {
            for (p, q) in r.segments() {
                if (p.y > y) != (q.y > y) {
                    xs.push(p.x + (y - p.y) / (q.y - p.y) * (q.x - p.x));
                }
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut segs: Vec<(DVec2, DVec2)> = xs.chunks_exact(2).map(|c| (rot(DVec2::new(c[0], y), a), rot(DVec2::new(c[1], y), a))).collect();
        if flip {
            segs.reverse();
            segs = segs.into_iter().map(|(s, e)| (e, s)).collect();
        }
        out.extend(segs);
        flip = !flip;
        y += stepover.max(0.01);
    }
    out
}

pub fn generate(name: &str, vectors: &[Polyline], p: &CutParams, opt: &PocketOptions) -> Toolpath {
    let region = Region::from_polylines(vectors.iter().filter(|v| v.closed));
    generate_region(name, &region, p, opt)
}

pub fn generate_region(name: &str, region: &Region, p: &CutParams, opt: &PocketOptions) -> Toolpath {
    let mut b = PathBuilder::new(&p.tool, &p.cutting, p.safe_z);
    b.comment(format!("Pocket {:?}: depth {:.3} mm", opt.style, p.cut_depth));
    let r = p.tool.radius();
    let surface = -p.start_depth;
    let cleared = region.offset(-(r + opt.allowance) + 1e-3);
    let stay_down_ok = |a: DVec2, c: DVec2| a.distance(c) < p.cutting.stepover * 1.6 && cleared.contains(a.lerp(c, 0.5));
    let mut prev_z = surface;
    for z in p.pass_levels() {
        match opt.style {
            PocketStyle::Offset => {
                let levels = offset_rings(region, r, p.cutting.stepover, opt.allowance, p.direction);
                let mut first = true;
                for rings in levels {
                    let start = b.current().map(|c| DVec2::new(c.x, c.y)).unwrap_or(DVec2::ZERO);
                    for ring in order_loops(rings, start) {
                        if let Some(c) = b.current() {
                            let cp = DVec2::new(c.x, c.y);
                            if !first && (c.z - z).abs() < 1e-9 && stay_down_ok(cp, ring.pts[0]) {
                                b.link_direct(Pt::new(ring.pts[0].x, ring.pts[0].y, z));
                                b.cut_loop_ramped(&ring, z, z, 0.0, surface);
                                continue;
                            }
                        }
                        b.retract();
                        b.cut_loop_ramped(&ring, z, prev_z, opt.ramp_length, surface);
                        first = false;
                    }
                }
            }
            PocketStyle::Raster => {
                let inner = region.offset(-(r + opt.allowance));
                let lines = raster_lines(&inner, p.cutting.stepover, opt.raster_angle);
                let mut last: Option<DVec2> = None;
                for (s, e) in lines {
                    match last {
                        Some(l) if stay_down_ok(l, s) => b.link_direct(Pt::new(s.x, s.y, z)),
                        _ => {
                            b.retract();
                            b.plunge_to(Pt::new(s.x, s.y, z), surface);
                        }
                    }
                    b.feed(Pt::new(e.x, e.y, z), p.cutting.feed);
                    last = Some(e);
                }
                if opt.boundary_pass {
                    for ring in inner.rings() {
                        let ring = if p.direction == CutDirection::Climb { ring.reversed() } else { ring };
                        b.retract();
                        b.cut_loop_ramped(&ring, z, z, 0.0, surface);
                    }
                }
            }
        }
        prev_z = z;
    }
    let _ = DVec3::ZERO;
    b.finish(name, &p.tool, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::testutil::params;
    use ezyx_core::path::shapes::{circle, rectangle};

    #[test]
    fn offset_pocket_covers_area() {
        let p = params("generic.flat_1_8_2f", 2.0);
        let r = rectangle(DVec2::ZERO, 40.0, 20.0, 0.0);
        let tp = generate("p", &[r], &p, &PocketOptions::default());
        let st = tp.stats(3000.0);
        assert!(st.cut_length > 200.0);
        assert!(st.min[0] >= p.tool.radius() - 0.05);
        assert!(st.max[0] <= 40.0 - p.tool.radius() + 0.05);
    }

    #[test]
    fn raster_pocket_with_hole() {
        let p = params("generic.flat_1_8_2f", 1.0);
        let outer = rectangle(DVec2::ZERO, 40.0, 40.0, 0.0);
        let hole = circle(DVec2::new(20.0, 20.0), 8.0);
        let opt = PocketOptions { style: PocketStyle::Raster, ..Default::default() };
        let tp = generate("p", &[outer, hole], &p, &opt);
        // No cutting point may fall inside the island.
        for (q, rapid) in tp.points() {
            if !rapid && q.z < 0.0 {
                assert!(DVec2::new(q.x, q.y).distance(DVec2::new(20.0, 20.0)) > 8.0 + p.tool.radius() - 0.1);
            }
        }
    }
}
