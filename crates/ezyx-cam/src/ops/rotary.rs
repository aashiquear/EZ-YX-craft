//! Rotary (4th-axis) machining.
//!
//! **Wrapping** maps a flat design onto a cylinder: design Y (0 … π·D) becomes the A angle
//! (0 … 360°), Y is held at the axis centreline and Z stays "depth below the cylinder surface".
//! Because the mapping is linear, linear moves stay linear – no subdivision is needed.
//!
//! **Feed compensation:** GRBL-family controllers interpret F as "units per minute" over the
//! combined XYZA vector, treating degrees as if they were mm. We rescale F per move so the actual
//! surface speed equals the requested feed: `F' = F × machine_len / surface_len`.
//! (Posts that support inverse-time `G93` can use [`Move::Feed`] with `rotary_inverse_time`.)
//!
//! **Rotary 3D:** a mesh is sampled into a cylindrical height map `R(x, θ)` (see
//! `ezyx-mesh::raster::cylindrical`), unwrapped to a regular heightmap with
//! `y = θ·R_stock` and `z = R − R_stock`, machined with the standard drop-cutter raster, then
//! wrapped. The unwrap ignores cutter curvature effects – accurate when the tool is small
//! relative to the stock diameter, which is the normal hobby case.

use crate::{Move, Pt, Toolpath};
use std::f64::consts::PI;

pub fn wrap(tp: &Toolpath, diameter: f64) -> Toolpath {
    let circ = PI * diameter;
    let to_a = |y: f64| y / circ * 360.0;
    let mut out = tp.clone();
    out.rotary = true;
    out.name = format!("{} (wrapped Ø{:.1})", tp.name, diameter);
    let mut prev: Option<Pt> = None;
    out.moves = tp
        .moves
        .iter()
        .map(|m| match m {
            Move::Rapid(p) => {
                let q = Pt { x: p.x, y: 0.0, z: p.z, a: to_a(p.y) };
                prev = Some(*p);
                Move::Rapid(q)
            }
            Move::Feed { to, feed } => {
                let q = Pt { x: to.x, y: 0.0, z: to.z, a: to_a(to.y) };
                let f = if let Some(pp) = prev {
                    // True surface length (at the cut radius) vs machine "length" with A in degrees.
                    let r = (diameter * 0.5 + to.z.min(0.0)).max(0.1);
                    let dy_surface = (to.y - pp.y) / (diameter * 0.5) * r;
                    let surf = ((to.x - pp.x).powi(2) + dy_surface.powi(2) + (to.z - pp.z).powi(2)).sqrt();
                    let da = to_a(to.y) - to_a(pp.y);
                    let mach = ((to.x - pp.x).powi(2) + da.powi(2) + (to.z - pp.z).powi(2)).sqrt();
                    if surf > 1e-9 {
                        feed * mach / surf
                    } else {
                        *feed
                    }
                } else {
                    *feed
                };
                prev = Some(*to);
                Move::Feed { to: q, feed: f }
            }
            other => other.clone(),
        })
        .collect();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::{engrave, testutil::params};
    use ezyx_core::{DVec2, Polyline};

    #[test]
    fn full_circumference_is_360() {
        let p = params("generic.v60", 1.0);
        let d = 50.0;
        let line = Polyline::open(vec![DVec2::new(10.0, 0.0), DVec2::new(10.0, PI * d)]);
        let tp = wrap(&engrave::generate("e", &[line], &p), d);
        let max_a = tp.points().map(|(q, _)| q.a).fold(0.0, f64::max);
        assert!((max_a - 360.0).abs() < 1e-6);
        assert!(tp.points().all(|(q, _)| q.y == 0.0));
    }
}
