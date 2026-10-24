//! SDF expression tree. Distances in mm; negative inside.

use crate::{MirrorPlane, Modifier, Shape};
use ezyx_core::{DVec2, DVec3, Heightmap, Polyline};
use glam::{DMat3, DQuat, EulerRot};

#[derive(Debug, Clone)]
pub enum Sdf {
    Box { half: DVec3, r: f64 },
    Sphere { r: f64 },
    Cylinder { r: f64, h: f64 },
    Cone { r0: f64, r1: f64, h: f64 },
    Torus { major: f64, minor: f64 },
    Extrude { loops: Vec<Polyline>, h: f64, draft: f64 },
    Revolve { loops: Vec<Polyline>, angle: f64 },
    Relief { hm: Heightmap, base: f64 },
    Union(Box<Sdf>, Box<Sdf>, f64),
    Subtract(Box<Sdf>, Box<Sdf>, f64),
    Intersect(Box<Sdf>, Box<Sdf>, f64),
    Offset(Box<Sdf>, f64),
    Shell(Box<Sdf>, f64),
    Transform { inner: Box<Sdf>, translate: [f64; 3], rotate_deg: [f64; 3] },
    Mirror(Box<Sdf>, MirrorPlane),
    Repeat { inner: Box<Sdf>, count: usize, step: DVec3 },
    Polar { inner: Box<Sdf>, count: usize },
}

pub fn shape(s: &Shape) -> Sdf {
    match s {
        Shape::Box { size, corner_radius } => Sdf::Box { half: DVec3::from_array(*size) * 0.5, r: *corner_radius },
        Shape::Sphere { radius } => Sdf::Sphere { r: *radius },
        Shape::Cylinder { radius, height } => Sdf::Cylinder { r: *radius, h: *height },
        Shape::Cone { r_bottom, r_top, height } => Sdf::Cone { r0: *r_bottom, r1: *r_top, h: *height },
        Shape::Torus { major, minor } => Sdf::Torus { major: *major, minor: *minor },
        Shape::Extrude { sketch, height, draft_deg } => Sdf::Extrude { loops: sketch.clone(), h: *height, draft: draft_deg.to_radians().tan() },
        Shape::Revolve { profile, angle_deg } => Sdf::Revolve { loops: profile.clone(), angle: angle_deg.to_radians() },
        Shape::Relief { heightmap, base } => Sdf::Relief { hm: heightmap.clone(), base: *base },
    }
}

pub fn modify(s: Sdf, m: &Modifier) -> Sdf {
    match m {
        // Fillet all edges: shrink then grow (rounds convex edges by r).
        Modifier::Fillet { radius } => Sdf::Offset(Box::new(Sdf::Offset(Box::new(s), -*radius)), *radius),
        Modifier::Shell { thickness } => Sdf::Shell(Box::new(s), *thickness),
        Modifier::LinearPattern { count, step } => Sdf::Repeat { inner: Box::new(s), count: *count, step: DVec3::from_array(*step) },
        Modifier::CircularPattern { count, .. } => Sdf::Polar { inner: Box::new(s), count: *count },
        Modifier::Mirror { plane } => Sdf::Mirror(Box::new(s), *plane),
    }
}

fn smin(a: f64, b: f64, k: f64) -> f64 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b + (a - b) * h - k * h * (1.0 - h)
}
fn smax(a: f64, b: f64, k: f64) -> f64 {
    -smin(-a, -b, k)
}

/// Signed distance to even-odd polygon set.
pub fn polygon_sdf(loops: &[Polyline], p: DVec2) -> f64 {
    let mut d = f64::INFINITY;
    let mut inside = false;
    for l in loops {
        d = d.min(l.segments().map(|(a, b)| ezyx_core::path::dist_point_segment(p, a, b)).fold(f64::INFINITY, f64::min));
        if l.contains(p) {
            inside = !inside;
        }
    }
    if inside {
        -d
    } else {
        d
    }
}

fn rot(deg: [f64; 3]) -> DMat3 {
    DMat3::from_quat(DQuat::from_euler(EulerRot::XYZ, deg[0].to_radians(), deg[1].to_radians(), deg[2].to_radians()))
}

impl Sdf {
    pub fn eval(&self, p: DVec3) -> f64 {
        match self {
            Sdf::Box { half, r } => {
                let q = p.abs() - (*half - DVec3::splat(*r));
                q.max(DVec3::ZERO).length() + q.x.max(q.y.max(q.z)).min(0.0) - r
            }
            Sdf::Sphere { r } => p.length() - r,
            Sdf::Cylinder { r, h } => {
                // Base at z = 0, top at z = h.
                let d = DVec2::new(p.truncate().length() - r, (p.z - h * 0.5).abs() - h * 0.5);
                d.x.max(d.y).min(0.0) + d.max(DVec2::ZERO).length()
            }
            Sdf::Cone { r0, r1, h } => {
                let t = (p.z / h).clamp(0.0, 1.0);
                let r = r0 + (r1 - r0) * t;
                let slope = ((r1 - r0) / h).atan().cos();
                let radial = (p.truncate().length() - r) * slope;
                radial.max(-p.z).max(p.z - h)
            }
            Sdf::Torus { major, minor } => DVec2::new(p.truncate().length() - major, p.z).length() - minor,
            Sdf::Extrude { loops, h, draft } => {
                let d2 = polygon_sdf(loops, p.truncate()) + p.z.clamp(0.0, *h) * draft;
                let dz = (p.z - h * 0.5).abs() - h * 0.5;
                let w = DVec2::new(d2, dz);
                w.x.max(w.y).min(0.0) + w.max(DVec2::ZERO).length()
            }
            Sdf::Revolve { loops, angle } => {
                let r = p.truncate().length();
                let mut d = polygon_sdf(loops, DVec2::new(r, p.z));
                if *angle < std::f64::consts::TAU - 1e-6 {
                    let a = p.y.atan2(p.x).rem_euclid(std::f64::consts::TAU);
                    if a > *angle {
                        let to_start = (a - angle).min(std::f64::consts::TAU - a);
                        d = d.max(to_start * r.max(1e-6) * 0.5);
                    }
                }
                d
            }
            Sdf::Relief { hm, base } => {
                let b = hm.bounds();
                let q = p.truncate();
                let inside_xy = q.x >= b.min.x && q.y >= b.min.y && q.x <= b.max.x && q.y <= b.max.y;
                let top = hm.sample(q) as f64;
                let dz = (p.z - top).max(base - p.z);
                let dxy = (b.min.x - q.x).max(q.x - b.max.x).max(b.min.y - q.y).max(q.y - b.max.y);
                if inside_xy {
                    dz.max(dxy) * 0.8
                } else {
                    dxy.max(dz)
                }
            }
            Sdf::Union(a, b, k) => smin(a.eval(p), b.eval(p), *k),
            Sdf::Subtract(a, b, k) => smax(a.eval(p), -b.eval(p), *k),
            Sdf::Intersect(a, b, k) => smax(a.eval(p), b.eval(p), *k),
            Sdf::Offset(a, d) => a.eval(p) - d,
            Sdf::Shell(a, t) => a.eval(p).abs() - t * 0.5,
            Sdf::Transform { inner, translate, rotate_deg } => {
                let q = rot(*rotate_deg).transpose() * (p - DVec3::from_array(*translate));
                inner.eval(q)
            }
            Sdf::Mirror(a, plane) => {
                let q = match plane {
                    MirrorPlane::YZ => DVec3::new(p.x.abs(), p.y, p.z),
                    MirrorPlane::XZ => DVec3::new(p.x, p.y.abs(), p.z),
                    MirrorPlane::XY => DVec3::new(p.x, p.y, p.z.abs()),
                };
                a.eval(q)
            }
            Sdf::Repeat { inner, count, step } => (0..(*count).max(1)).map(|i| inner.eval(p - *step * i as f64)).fold(f64::INFINITY, f64::min),
            Sdf::Polar { inner, count } => {
                let n = (*count).max(1) as f64;
                let seg = std::f64::consts::TAU / n;
                let a = p.y.atan2(p.x);
                let a = (a + seg * 0.5).rem_euclid(seg) - seg * 0.5;
                let r = p.truncate().length();
                inner.eval(DVec3::new(r * a.cos(), r * a.sin(), p.z))
            }
        }
    }

    /// Conservative axis-aligned bounds (min, max).
    pub fn bounds(&self) -> (DVec3, DVec3) {
        match self {
            Sdf::Box { half, .. } => (-*half, *half),
            Sdf::Sphere { r } => (DVec3::splat(-r), DVec3::splat(*r)),
            Sdf::Cylinder { r, h } => (DVec3::new(-r, -r, 0.0), DVec3::new(*r, *r, *h)),
            Sdf::Cone { r0, r1, h } => {
                let r = r0.max(*r1);
                (DVec3::new(-r, -r, 0.0), DVec3::new(r, r, *h))
            }
            Sdf::Torus { major, minor } => (DVec3::new(-(major + minor), -(major + minor), -minor), DVec3::new(major + minor, major + minor, *minor)),
            Sdf::Extrude { loops, h, draft } => {
                let b = loops.iter().fold(ezyx_core::Bounds2::EMPTY, |a, l| a.union(&l.bounds()));
                let grow = (h * draft).max(0.0);
                (DVec3::new(b.min.x - grow, b.min.y - grow, 0.0), DVec3::new(b.max.x + grow, b.max.y + grow, *h))
            }
            Sdf::Revolve { loops, .. } => {
                let b = loops.iter().fold(ezyx_core::Bounds2::EMPTY, |a, l| a.union(&l.bounds()));
                let r = b.max.x.abs().max(b.min.x.abs());
                (DVec3::new(-r, -r, b.min.y), DVec3::new(r, r, b.max.y))
            }
            Sdf::Relief { hm, base } => {
                let b = hm.bounds();
                (DVec3::new(b.min.x, b.min.y, *base), DVec3::new(b.max.x, b.max.y, hm.min_max().1 as f64))
            }
            Sdf::Union(a, b, k) => {
                let (a0, a1) = a.bounds();
                let (b0, b1) = b.bounds();
                (a0.min(b0) - DVec3::splat(*k), a1.max(b1) + DVec3::splat(*k))
            }
            Sdf::Subtract(a, _, _) => a.bounds(),
            Sdf::Intersect(a, b, _) => {
                let (a0, a1) = a.bounds();
                let (b0, b1) = b.bounds();
                (a0.max(b0), a1.min(b1))
            }
            Sdf::Offset(a, d) => {
                let (l, h) = a.bounds();
                (l - DVec3::splat(d.max(0.0)), h + DVec3::splat(d.max(0.0)))
            }
            Sdf::Shell(a, t) => {
                let (l, h) = a.bounds();
                (l - DVec3::splat(*t), h + DVec3::splat(*t))
            }
            Sdf::Transform { inner, translate, rotate_deg } => {
                let (l, h) = inner.bounds();
                let r = rot(*rotate_deg);
                let t = DVec3::from_array(*translate);
                let mut lo = DVec3::splat(f64::INFINITY);
                let mut hi = DVec3::splat(f64::NEG_INFINITY);
                for i in 0..8 {
                    let c = DVec3::new(if i & 1 == 0 { l.x } else { h.x }, if i & 2 == 0 { l.y } else { h.y }, if i & 4 == 0 { l.z } else { h.z });
                    let w = r * c + t;
                    lo = lo.min(w);
                    hi = hi.max(w);
                }
                (lo, hi)
            }
            Sdf::Mirror(a, plane) => {
                let (l, h) = a.bounds();
                let m = l.abs().max(h.abs());
                match plane {
                    MirrorPlane::YZ => (DVec3::new(-m.x, l.y, l.z), DVec3::new(m.x, h.y, h.z)),
                    MirrorPlane::XZ => (DVec3::new(l.x, -m.y, l.z), DVec3::new(h.x, m.y, h.z)),
                    MirrorPlane::XY => (DVec3::new(l.x, l.y, -m.z), DVec3::new(h.x, h.y, m.z)),
                }
            }
            Sdf::Repeat { inner, count, step } => {
                let (l, h) = inner.bounds();
                let e = *step * ((*count).max(1) - 1) as f64;
                (l.min(l + e), h.max(h + e))
            }
            Sdf::Polar { inner, .. } => {
                let (l, h) = inner.bounds();
                let r = l.truncate().length().max(h.truncate().length());
                (DVec3::new(-r, -r, l.z), DVec3::new(r, r, h.z))
            }
        }
    }
}
