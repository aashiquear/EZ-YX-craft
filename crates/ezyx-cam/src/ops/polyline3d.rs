//! Toolpaths from explicit 3D polylines (photo V-carve grooves, AI-generated paths, imports).

use crate::link::PathBuilder;
use crate::{CutParams, Toolpath};
use ezyx_core::DVec3;

/// Remove nearly collinear points (3D) to keep G-code small.
pub fn simplify3(pts: &[DVec3], tol: f64) -> Vec<DVec3> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    for i in 1..pts.len() - 1 {
        let a = *out.last().unwrap();
        let c = pts[i + 1];
        let b = pts[i];
        let ac = c - a;
        let l2 = ac.length_squared();
        let d = if l2 < 1e-18 { b.distance(a) } else { (b - a).cross(ac).length() / l2.sqrt() };
        if d > tol {
            out.push(b);
        }
    }
    out.push(*pts.last().unwrap());
    out
}

pub fn generate(name: &str, paths: &[Vec<DVec3>], p: &CutParams, tol: f64) -> Toolpath {
    let mut b = PathBuilder::new(&p.tool, &p.cutting, p.safe_z);
    b.comment(format!("{} 3D paths", paths.len()));
    for path in paths {
        let s = simplify3(path, tol);
        b.cut_polyline3(&s, -p.start_depth);
        b.retract();
    }
    b.finish(name, &p.tool, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn simplify_line() {
        let pts: Vec<_> = (0..100).map(|i| DVec3::new(i as f64, 0.0, -(i as f64) * 0.01)).collect();
        assert_eq!(simplify3(&pts, 1e-6).len(), 2);
    }
}
