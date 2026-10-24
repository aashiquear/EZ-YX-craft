//! Mesh → heightmaps.

use crate::TriMesh;
use ezyx_core::{DVec2, Heightmap};
use glam::Vec3;
use rayon::prelude::*;

/// Top-down Z-buffer of the mesh. Cells not covered get `background`.
/// Mesh coordinates are used as-is (place/scale the model before calling).
pub fn top_down(m: &TriMesh, origin: DVec2, size: DVec2, cell: f64, background: f32) -> Heightmap {
    let mut hm = Heightmap::covering(origin, size, cell, f32::NEG_INFINITY);
    let (w, h) = (hm.width, hm.height);
    // Bucket triangles into row bands for parallel rasterisation.
    let bands = rayon::current_num_threads().max(1) * 4;
    let band_h = h.div_ceil(bands).max(1);
    let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); h.div_ceil(band_h)];
    for i in 0..m.indices.len() {
        let [a, b, c] = m.triangle(i);
        let y0 = ((a.y.min(b.y).min(c.y) as f64 - origin.y) / cell - 0.5).floor().max(0.0) as usize;
        let y1 = ((a.y.max(b.y).max(c.y) as f64 - origin.y) / cell - 0.5).ceil().max(0.0) as usize;
        for bk in (y0 / band_h)..=(y1 / band_h).min(buckets.len() - 1) {
            buckets[bk].push(i);
        }
    }
    hm.data.par_chunks_mut(w * band_h).enumerate().for_each(|(bk, chunk)| {
        let y_start = bk * band_h;
        for &ti in &buckets[bk] {
            let [a, b, c] = m.triangle(ti);
            let to = |v: Vec3| DVec2::new((v.x as f64 - origin.x) / cell - 0.5, (v.y as f64 - origin.y) / cell - 0.5);
            let (pa, pb, pc) = (to(a), to(b), to(c));
            let den = (pb.y - pc.y) * (pa.x - pc.x) + (pc.x - pb.x) * (pa.y - pc.y);
            if den.abs() < 1e-12 {
                continue;
            }
            let x0 = pa.x.min(pb.x).min(pc.x).ceil().max(0.0) as usize;
            let x1 = (pa.x.max(pb.x).max(pc.x).floor() as isize).min(w as isize - 1);
            let yy0 = pa.y.min(pb.y).min(pc.y).ceil().max(y_start as f64) as usize;
            let yy1 = (pa.y.max(pb.y).max(pc.y).floor() as isize).min((y_start + chunk.len() / w) as isize - 1);
            if x1 < 0 || yy1 < 0 {
                continue;
            }
            for y in yy0..=yy1 as usize {
                for x in x0..=x1 as usize {
                    let (px, py) = (x as f64, y as f64);
                    let l1 = ((pb.y - pc.y) * (px - pc.x) + (pc.x - pb.x) * (py - pc.y)) / den;
                    let l2 = ((pc.y - pa.y) * (px - pc.x) + (pa.x - pc.x) * (py - pc.y)) / den;
                    let l3 = 1.0 - l1 - l2;
                    if l1 < -1e-9 || l2 < -1e-9 || l3 < -1e-9 {
                        continue;
                    }
                    let z = (l1 * a.z as f64 + l2 * b.z as f64 + l3 * c.z as f64) as f32;
                    let i = (y - y_start) * w + x;
                    if z > chunk[i] {
                        chunk[i] = z;
                    }
                }
            }
        }
    });
    hm.map(|v| if v.is_finite() { v } else { background });
    hm
}

/// Cylindrical radius map for rotary work: model axis along X.
/// Returns heightmap with x = model X, y = θ·r_stock (unwrapped arc length), z = r − r_stock (≤ 0).
pub fn cylindrical(m: &TriMesh, x_range: (f64, f64), r_stock: f64, cell: f64) -> Heightmap {
    let circ = std::f64::consts::TAU * r_stock;
    let mut hm = Heightmap::covering(DVec2::new(x_range.0, 0.0), DVec2::new(x_range.1 - x_range.0, circ), cell, -(r_stock as f32));
    let (w, h) = (hm.width, hm.height);
    // Bin triangles by X slab.
    let mut bins: Vec<Vec<usize>> = vec![Vec::new(); w];
    for i in 0..m.indices.len() {
        let [a, b, c] = m.triangle(i);
        let x0 = (((a.x.min(b.x).min(c.x) as f64) - x_range.0) / cell - 1.0).floor().max(0.0) as usize;
        let x1 = ((((a.x.max(b.x).max(c.x) as f64) - x_range.0) / cell + 1.0).ceil() as usize).min(w - 1);
        for bin in bins.iter_mut().take(x1 + 1).skip(x0) {
            bin.push(i);
        }
    }
    let far = (r_stock * 4.0 + 1000.0) as f32;
    let origin = hm.origin;
    hm.data.par_chunks_mut(w).enumerate().for_each(|(yi, row)| {
        let theta = (yi as f64 + 0.5) * cell / r_stock;
        let dir = Vec3::new(0.0, theta.cos() as f32, theta.sin() as f32);
        for (xi, out) in row.iter_mut().enumerate() {
            let x = (origin.x + (xi as f64 + 0.5) * cell) as f32;
            let o = Vec3::new(x, 0.0, 0.0) + dir * far;
            let d = -dir;
            let mut best: Option<f32> = None;
            for &ti in &bins[xi] {
                if let Some(t) = ray_tri(o, d, m.triangle(ti)) {
                    best = Some(best.map_or(t, |b: f32| b.min(t)));
                }
            }
            if let Some(t) = best {
                let r = (far - t) as f64;
                *out = (r.min(r_stock) - r_stock) as f32;
            }
        }
    });
    let _ = h;
    hm
}

/// Möller–Trumbore, two-sided.
pub fn ray_tri(o: Vec3, d: Vec3, [a, b, c]: [Vec3; 3]) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 0.0).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cube_top_down() {
        let c = TriMesh::cube(Vec3::new(2.0, 2.0, -5.0), Vec3::new(8.0, 8.0, -1.0));
        let hm = top_down(&c, DVec2::ZERO, DVec2::new(10.0, 10.0), 0.5, -10.0);
        assert!((hm.sample(DVec2::new(5.0, 5.0)) + 1.0).abs() < 1e-4);
        assert_eq!(hm.get(0, 0), -10.0);
    }
    #[test]
    fn cylinder_of_cube() {
        // Cube centred on X axis: half-size 5 → radius between 5 and 7.07.
        let c = TriMesh::cube(Vec3::new(0.0, -5.0, -5.0), Vec3::new(20.0, 5.0, 5.0));
        let hm = cylindrical(&c, (0.0, 20.0), 10.0, 0.5);
        let (lo, hi) = hm.min_max();
        assert!(lo >= -5.1 && hi <= -2.8, "{lo} {hi}");
    }
}
