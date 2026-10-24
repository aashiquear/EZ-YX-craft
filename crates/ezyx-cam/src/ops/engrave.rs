//! Engrave / "quick engrave": follow vectors on-line at a fixed depth (multi-pass).
//! Also used for laser line engraving (with `laser_power`).

use crate::link::{order_loops, PathBuilder};
use crate::{CutParams, Move, Pt, Toolpath};
use ezyx_core::{DVec2, DVec3, Polyline};

pub fn generate(name: &str, vectors: &[Polyline], p: &CutParams) -> Toolpath {
    let mut b = PathBuilder::new(&p.tool, &p.cutting, p.safe_z);
    b.comment(format!("Engrave on vectors, depth {:.3} mm", p.cut_depth));
    let surface = -p.start_depth;
    let ordered = order_loops(vectors.to_vec(), DVec2::ZERO);
    for z in p.pass_levels() {
        for v in &ordered {
            let mut pts: Vec<DVec3> = v.pts.iter().map(|q| DVec3::new(q.x, q.y, z)).collect();
            if v.closed {
                pts.push(pts[0]);
            }
            b.cut_polyline3(&pts, surface);
            b.retract();
        }
    }
    b.finish(name, &p.tool, false)
}

/// Laser vector engraving: Z stays at focus height, power toggled per path (M4 dynamic mode).
pub fn laser(name: &str, vectors: &[Polyline], p: &CutParams, power: f64, focus_z: f64, passes: usize) -> Toolpath {
    let mut b = PathBuilder::new(&p.tool, &p.cutting, focus_z);
    b.comment(format!("Laser engrave, power {:.0}% × {passes} passes", power * 100.0));
    let ordered = order_loops(vectors.to_vec(), DVec2::ZERO);
    for _ in 0..passes.max(1) {
        for v in &ordered {
            let Some(first) = v.pts.first() else { continue };
            b.moves.push(Move::Laser { power: 0.0 });
            b.rapid(Pt::new(first.x, first.y, focus_z));
            b.moves.push(Move::Laser { power });
            for q in v.pts.iter().skip(1).chain(if v.closed { Some(first) } else { None }) {
                b.feed(Pt::new(q.x, q.y, focus_z), p.cutting.feed);
            }
        }
    }
    b.moves.push(Move::Laser { power: 0.0 });
    let mut tp = b.finish(name, &p.tool, false);
    tp.moves.retain(|m| !matches!(m, Move::Spindle { .. }));
    tp
}
