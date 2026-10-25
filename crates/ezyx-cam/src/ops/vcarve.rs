//! V-carving.
//!
//! **Method (offset-contour V-carve):** a V-bit whose tip sits at depth `z` cuts a cone that meets
//! the surface at radius `tip_r + z·tan(α)`. Cutting the contour offset inward by `d` at depth
//! `z(d) = (d − tip_r) / tan(α)` therefore produces walls that touch the original vector exactly.
//! Stepping `d` from the tip radius to the region's inradius, the union of all cones reproduces the
//! ideal V-carved surface (each contour's outward flank already has the correct slope), leaving
//! at most a ridge of `step / (2·tan α)` along the medial axis.
//!
//! When `flat_depth` is set the depth is capped and the remaining flat floor is cleared either by
//! the V-bit itself (offset passes at V-width stepover) or by a separate clearance tool (returned
//! as a separate first toolpath so the user can see/run it separately — exactly like other carving suites).
//!
//! A medial-axis (Voronoi) tracer that emits only the centreline passes is on the roadmap; it cuts
//! the same surface faster. This method was chosen first because it is robust on any input,
//! including self-touching fonts and traced bitmaps.

use crate::link::{order_loops, PathBuilder};
use crate::ops::pocket::{self, PocketOptions};
use crate::{CutParams, Toolpath};
use ezyx_core::{region::Region, DVec2, Polyline};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VCarveOptions {
    /// Maximum depth; `None` = full V (depth limited only by geometry).
    pub flat_depth: Option<f64>,
    /// Offset step (mm). Smaller = smoother centre ridge, longer runtime.
    pub step: f64,
    /// Clearance tool for flat areas (pocket at `flat_depth`).
    pub clearance: Option<CutParams>,
}

impl Default for VCarveOptions {
    fn default() -> Self {
        Self { flat_depth: None, step: 0.2, clearance: None }
    }
}

pub fn generate(name: &str, vectors: &[Polyline], p: &CutParams, opt: &VCarveOptions) -> Vec<Toolpath> {
    let region = Region::from_polylines(vectors.iter().filter(|v| v.closed));
    let ha = p.tool.half_angle().unwrap_or(45f64.to_radians());
    let tan = ha.tan().max(1e-3);
    let tip_r = match p.tool.shape {
        ezyx_tooldb::ToolShape::VBit { tip_diameter, .. } => tip_diameter * 0.5,
        _ => 0.0,
    };
    let max_depth_by_tool = (p.tool.radius() - tip_r) / tan;
    let limit = opt.flat_depth.unwrap_or(f64::INFINITY).min(max_depth_by_tool);
    let surface = -p.start_depth;

    let mut b = PathBuilder::new(&p.tool, &p.cutting, p.safe_z);
    b.comment(format!("V-carve {:.0}° step {:.3}", ha.to_degrees() * 2.0, opt.step));

    let mut d = tip_r.max(opt.step * 0.5);
    let mut flat_d: Option<f64> = None;
    let mut guard = 0;
    loop {
        guard += 1;
        if guard > 20_000 {
            break;
        }
        let z_ideal = (d - tip_r) / tan;
        let z = z_ideal.min(limit);
        let off = region.offset(-d);
        if off.is_empty() {
            break;
        }
        if z_ideal >= limit && flat_d.is_none() {
            flat_d = Some(d);
            if opt.clearance.is_some() {
                // Final wall contour at the flat depth, clearance tool does the rest.
                emit_rings(&mut b, &off, -(z + p.start_depth), surface);
                break;
            }
        }
        emit_rings(&mut b, &off, -(z + p.start_depth), surface);
        d += if flat_d.is_some() {
            // Floor clearing with the V-bit: stepover = 40% of the cut width at flat depth.
            (0.4 * 2.0 * (tip_r + limit * tan)).max(opt.step)
        } else {
            opt.step
        };
    }
    let mut out = vec![b.finish(name, &p.tool, false)];
    if let (Some(fd), Some(cp)) = (flat_d, &opt.clearance) {
        let floor = region.offset(-fd);
        let mut cp = cp.clone();
        cp.cut_depth = limit + p.start_depth;
        let tp = pocket::generate_region(&format!("{name} (clearance)"), &floor.offset(cp.tool.radius() * 0.9), &cp, &PocketOptions::default());
        // Clear the floor first (roughing), then the V-bit finishes walls and corners.
        out.insert(0, tp);
    }
    out
}

fn emit_rings(b: &mut PathBuilder, off: &Region, z: f64, surface: f64) {
    let start = b.current().map(|c| DVec2::new(c.x, c.y)).unwrap_or(DVec2::ZERO);
    for ring in order_loops(off.rings(), start) {
        b.retract();
        b.cut_loop_ramped(&ring, z, z, 0.0, surface);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::testutil::params;
    use ezyx_core::path::shapes::rectangle;

    #[test]
    fn v90_strip_depth_equals_half_width() {
        // A 6 mm wide strip with a 90° V-bit: max depth ≈ 3 mm.
        let p = params("generic.v90", 10.0);
        let r = rectangle(DVec2::ZERO, 60.0, 6.0, 0.0);
        let tps = generate("v", &[r], &p, &VCarveOptions { step: 0.1, ..Default::default() });
        let st = tps[0].stats(3000.0);
        assert!((st.min[2] + 3.0).abs() < 0.15, "min z {}", st.min[2]);
    }

    #[test]
    fn flat_depth_limits() {
        let p = params("generic.v90", 10.0);
        let r = rectangle(DVec2::ZERO, 60.0, 20.0, 0.0);
        let tps = generate("v", &[r], &p, &VCarveOptions { flat_depth: Some(2.0), step: 0.2, clearance: None });
        let st = tps[0].stats(3000.0);
        assert!((st.min[2] + 2.0).abs() < 1e-9);
    }
}
