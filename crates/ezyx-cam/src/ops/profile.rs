//! Profile (contour) cutting: outside, inside or on the vector, with tabs and ramps.

use crate::link::{order_loops, PathBuilder};
use crate::{CutDirection, CutParams, Pt, Toolpath};
use ezyx_core::{region::Region, DVec2, Polyline};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ProfileSide {
    #[default]
    Outside,
    Inside,
    On,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tabs {
    pub count_per_loop: usize,
    /// Tab length along the path (mm), excluding tool diameter.
    pub length: f64,
    /// Tab height measured up from the cut bottom (mm).
    pub height: f64,
}

impl Default for Tabs {
    fn default() -> Self {
        Self { count_per_loop: 4, length: 6.0, height: 2.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileOptions {
    pub side: ProfileSide,
    /// Extra material left on the wall (mm), e.g. for a later finishing pass.
    pub allowance: f64,
    pub tabs: Option<Tabs>,
    /// Ramp length (mm); 0 = straight plunge.
    pub ramp_length: f64,
    /// Final light pass at full depth with this radial stock (0 = none).
    pub finish_pass: f64,
}

impl Default for ProfileOptions {
    fn default() -> Self {
        Self { side: ProfileSide::Outside, allowance: 0.0, tabs: None, ramp_length: 10.0, finish_pass: 0.0 }
    }
}

/// Generate the tool-centre loops for the profile.
pub fn tool_loops(vectors: &[Polyline], p: &CutParams, opt: &ProfileOptions, extra: f64) -> Vec<Polyline> {
    let r = p.tool.radius() + opt.allowance + extra;
    match opt.side {
        ProfileSide::On => vectors.to_vec(),
        side => {
            let region = Region::from_polylines(vectors.iter().filter(|v| v.closed));
            let off = region.offset(if side == ProfileSide::Outside { r } else { -r });
            let mut loops = off.rings();
            // Rings come out climb-oriented for an outside cut; inside cuts need reversing.
            let reverse = (side == ProfileSide::Inside) ^ (p.direction == CutDirection::Conventional);
            if reverse {
                loops = loops.into_iter().map(|l| l.reversed()).collect();
            }
            // Open vectors can't be offset to a side: cut them on-line.
            loops.extend(vectors.iter().filter(|v| !v.closed).cloned());
            loops
        }
    }
}

/// Positions (arc length start, end) of tabs on a loop.
fn tab_ranges(len: f64, tabs: &Tabs, tool_d: f64) -> Vec<(f64, f64)> {
    if tabs.count_per_loop == 0 || len < (tabs.length + tool_d) * tabs.count_per_loop as f64 * 2.0 {
        return vec![];
    }
    let span = tabs.length + tool_d;
    (0..tabs.count_per_loop)
        .map(|i| {
            let c = len * (i as f64 + 0.5) / tabs.count_per_loop as f64;
            (c - span / 2.0, c + span / 2.0)
        })
        .collect()
}

/// Emit points along a closed loop at depth `z`, lifting to `tab_z` inside tab ranges.
fn loop_with_tabs(path: &Polyline, z: f64, tab_z: f64, ranges: &[(f64, f64)]) -> Vec<ezyx_core::DVec3> {
    use ezyx_core::DVec3;
    let mut out = Vec::new();
    let in_tab = |s: f64| ranges.iter().any(|(a, b)| s > *a + 1e-9 && s < *b - 1e-9);
    let zat = |s: f64| if in_tab(s) { tab_z } else { z };
    let first = path.pts[0];
    out.push(DVec3::new(first.x, first.y, zat(0.0)));
    let mut s = 0.0;
    for (a, b) in path.segments() {
        let l = a.distance(b);
        // Split points inside this segment.
        let mut cuts: Vec<f64> = ranges.iter().flat_map(|(r0, r1)| [*r0, *r1]).filter(|c| *c > s && *c < s + l).collect();
        cuts.sort_by(|x, y| x.partial_cmp(y).unwrap());
        for c in cuts {
            let p = a.lerp(b, (c - s) / l);
            // Step: arrive at current level, then change level vertically.
            let before = zat(c - 1e-6);
            let after = zat(c + 1e-6);
            out.push(DVec3::new(p.x, p.y, before));
            out.push(DVec3::new(p.x, p.y, after));
        }
        s += l;
        out.push(DVec3::new(b.x, b.y, zat(s - 1e-6)));
    }
    out
}

pub fn generate(name: &str, vectors: &[Polyline], p: &CutParams, opt: &ProfileOptions) -> Toolpath {
    let mut b = PathBuilder::new(&p.tool, &p.cutting, p.safe_z);
    b.comment(format!("Profile {:?}: depth {:.3} mm", opt.side, p.cut_depth));
    let loops = order_loops(tool_loops(vectors, p, opt, opt.finish_pass), DVec2::ZERO);
    let levels = p.pass_levels();
    let tab_top = -p.cut_depth + opt.tabs.as_ref().map(|t| t.height).unwrap_or(0.0);
    let surface = -p.start_depth;
    for lp in &loops {
        let mut prev_z = surface;
        let ranges = opt.tabs.as_ref().map(|t| tab_ranges(lp.length(), t, p.tool.diameter)).unwrap_or_default();
        for &z in &levels {
            if !lp.closed {
                // Open path: zig-zag direction each level to avoid air moves.
                let pts: Vec<_> = lp.pts.iter().map(|q| ezyx_core::DVec3::new(q.x, q.y, z)).collect();
                b.cut_polyline3(&pts, surface);
                b.retract();
                continue;
            }
            if !ranges.is_empty() && z < tab_top {
                let pts = loop_with_tabs(lp, z, tab_top, &ranges);
                b.cut_polyline3(&pts, surface);
            } else {
                b.cut_loop_ramped(lp, z, prev_z, opt.ramp_length, surface);
            }
            prev_z = z;
        }
        b.retract();
    }
    if opt.finish_pass > 0.0 {
        b.comment("Finishing pass");
        let fin = order_loops(tool_loops(vectors, p, opt, 0.0), DVec2::ZERO);
        let z = -p.cut_depth;
        for lp in &fin {
            let ranges = opt.tabs.as_ref().map(|t| tab_ranges(lp.length(), t, p.tool.diameter)).unwrap_or_default();
            if lp.closed && !ranges.is_empty() {
                b.cut_polyline3(&loop_with_tabs(lp, z, tab_top, &ranges), surface);
            } else if lp.closed {
                b.cut_loop_ramped(lp, z, z, 0.0, surface);
            }
            b.retract();
        }
    }
    let _ = Pt::default();
    b.finish(name, &p.tool, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::testutil::params;
    use crate::Move;
    use ezyx_core::path::shapes::rectangle;

    #[test]
    fn outside_profile_depth_and_offset() {
        let p = params("generic.flat_1_8_2f", 6.0);
        let r = rectangle(DVec2::ZERO, 50.0, 30.0, 0.0);
        let tp = generate("t", &[r], &p, &ProfileOptions::default());
        let st = tp.stats(3000.0);
        assert!((st.min[2] + 6.0).abs() < 1e-9, "bottom {:?}", st.min);
        // Tool centre is outside by radius.
        assert!((st.min[0] + p.tool.radius()).abs() < 0.05, "{:?}", st.min);
        assert!(tp.moves.iter().any(|m| matches!(m, Move::Feed { .. })));
    }

    #[test]
    fn tabs_lift() {
        let p = params("generic.flat_1_8_2f", 6.0);
        let r = rectangle(DVec2::ZERO, 100.0, 100.0, 0.0);
        let opt = ProfileOptions { tabs: Some(Tabs::default()), ..Default::default() };
        let tp = generate("t", &[r], &p, &opt);
        let at_tab = tp.points().filter(|(q, rapid)| !rapid && (q.z + 4.0).abs() < 1e-9).count();
        assert!(at_tab > 0, "expected points at tab height");
    }

    #[test]
    fn inside_is_smaller() {
        let p = params("generic.flat_1_8_2f", 3.0);
        let r = rectangle(DVec2::ZERO, 50.0, 30.0, 0.0);
        let opt = ProfileOptions { side: ProfileSide::Inside, ..Default::default() };
        let st = generate("t", &[r], &p, &opt).stats(3000.0);
        assert!((st.min[0] - p.tool.radius()).abs() < 0.05);
    }
}
