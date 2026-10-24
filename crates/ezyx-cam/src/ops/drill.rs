//! Drilling with optional peck. Expanded to plain G0/G1 so it works on GRBL (no canned cycles).

use crate::link::PathBuilder;
use crate::{CutParams, Pt, Toolpath};
use ezyx_core::{DVec2, Polyline};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrillOptions {
    /// Peck depth (0 = single plunge).
    pub peck: f64,
    /// Retract height above the hole between pecks.
    pub peck_retract: f64,
    pub dwell: f64,
}

impl Default for DrillOptions {
    fn default() -> Self {
        Self { peck: 2.0, peck_retract: 1.0, dwell: 0.0 }
    }
}

/// Drill centres: the centroid of every closed vector (circles), or every point of open vectors.
pub fn centers(vectors: &[Polyline]) -> Vec<DVec2> {
    let mut out = Vec::new();
    for v in vectors {
        if v.closed && !v.is_empty() {
            out.push(v.bounds().center());
        } else {
            out.extend(v.pts.iter().copied());
        }
    }
    out
}

pub fn generate(name: &str, points: &[DVec2], p: &CutParams, opt: &DrillOptions) -> Toolpath {
    let mut b = PathBuilder::new(&p.tool, &p.cutting, p.safe_z);
    b.comment(format!("Drill {} holes to {:.3} mm", points.len(), p.cut_depth));
    let surface = -p.start_depth;
    // Greedy order.
    let mut rest: Vec<DVec2> = points.to_vec();
    let mut cur = DVec2::ZERO;
    while !rest.is_empty() {
        let i = rest.iter().enumerate().min_by(|a, b| a.1.distance_squared(cur).partial_cmp(&b.1.distance_squared(cur)).unwrap()).map(|(i, _)| i).unwrap();
        let c = rest.swap_remove(i);
        cur = c;
        b.retract();
        b.rapid(Pt::new(c.x, c.y, p.safe_z));
        b.rapid(Pt::new(c.x, c.y, surface + 0.5));
        let target = -p.cut_depth;
        if opt.peck <= 0.0 {
            b.feed(Pt::new(c.x, c.y, target), p.cutting.plunge_feed);
        } else {
            let mut z = surface;
            while z > target + 1e-9 {
                let nz = (z - opt.peck).max(target);
                // Rapid back down to just above the previous depth.
                b.rapid(Pt::new(c.x, c.y, (z + 0.2).min(surface + 0.5)));
                b.feed(Pt::new(c.x, c.y, nz), p.cutting.plunge_feed);
                b.rapid(Pt::new(c.x, c.y, surface + opt.peck_retract));
                z = nz;
            }
        }
        if opt.dwell > 0.0 {
            b.moves.push(crate::Move::Dwell { seconds: opt.dwell });
        }
        b.rapid(Pt::new(c.x, c.y, p.safe_z));
    }
    b.finish(name, &p.tool, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::testutil::params;
    #[test]
    fn pecks_reach_depth() {
        let p = params("generic.drill_3", 7.0);
        let tp = generate("d", &[DVec2::new(10.0, 10.0), DVec2::new(30.0, 10.0)], &p, &DrillOptions::default());
        assert!((tp.stats(3000.0).min[2] + 7.0).abs() < 1e-9);
    }
}
