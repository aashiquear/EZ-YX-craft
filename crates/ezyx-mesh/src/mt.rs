//! Marching tetrahedra over a regular scalar grid. Negative = inside.

use crate::TriMesh;
use glam::Vec3;
use rayon::prelude::*;
use std::collections::HashMap;

/// Scalar field sampled on (nx × ny × nz) nodes.
pub struct Grid {
    pub nx: usize,
    pub ny: usize,
    pub nz: usize,
    pub origin: Vec3,
    pub h: f32,
    pub values: Vec<f32>,
}

impl Grid {
    /// Sample `f` on a grid covering [min, max] with spacing `h` (parallel).
    pub fn sample(min: Vec3, max: Vec3, h: f32, f: impl Fn(Vec3) -> f32 + Sync) -> Self {
        let pad = h;
        let min = min - Vec3::splat(pad);
        let max = max + Vec3::splat(pad);
        let n = ((max - min) / h).ceil().as_uvec3() + glam::UVec3::ONE;
        let (nx, ny, nz) = (n.x as usize, n.y as usize, n.z as usize);
        let values = (0..nx * ny * nz)
            .into_par_iter()
            .map(|i| {
                let x = i % nx;
                let y = (i / nx) % ny;
                let z = i / (nx * ny);
                f(min + Vec3::new(x as f32, y as f32, z as f32) * h)
            })
            .collect();
        Self { nx, ny, nz, origin: min, h, values }
    }
    #[inline]
    pub fn idx(&self, x: usize, y: usize, z: usize) -> usize {
        (z * self.ny + y) * self.nx + x
    }
    pub fn pos(&self, i: usize) -> Vec3 {
        let x = i % self.nx;
        let y = (i / self.nx) % self.ny;
        let z = i / (self.nx * self.ny);
        self.origin + Vec3::new(x as f32, y as f32, z as f32) * self.h
    }
}

/// Kuhn/Freudenthal split of the unit cube into 6 tetrahedra sharing the main diagonal 0–6.
/// Cube corner numbering: bit0 = x, bit1 = y, bit2 = z.
pub const KUHN_TETS: [[usize; 4]; 6] = [[0, 1, 3, 7], [0, 1, 5, 7], [0, 2, 3, 7], [0, 2, 6, 7], [0, 4, 5, 7], [0, 4, 6, 7]];

pub fn extract(g: &Grid, iso: f32) -> TriMesh {
    let mut m = TriMesh::default();
    let mut edge_vert: HashMap<(usize, usize), u32> = HashMap::new();
    let mut vert = |a: usize, b: usize, m: &mut TriMesh| -> u32 {
        let key = if a < b { (a, b) } else { (b, a) };
        *edge_vert.entry(key).or_insert_with(|| {
            let (va, vb) = (g.values[a] - iso, g.values[b] - iso);
            let t = (va / (va - vb)).clamp(0.0, 1.0);
            m.positions.push(g.pos(a).lerp(g.pos(b), t));
            (m.positions.len() - 1) as u32
        })
    };
    for z in 0..g.nz - 1 {
        for y in 0..g.ny - 1 {
            for x in 0..g.nx - 1 {
                let c: [usize; 8] = std::array::from_fn(|k| g.idx(x + (k & 1), y + ((k >> 1) & 1), z + ((k >> 2) & 1)));
                // Quick reject: all same sign.
                let s0 = g.values[c[0]] < iso;
                if c.iter().all(|i| (g.values[*i] < iso) == s0) {
                    continue;
                }
                for t in KUHN_TETS {
                    let v = [c[t[0]], c[t[1]], c[t[2]], c[t[3]]];
                    let inside: Vec<usize> = (0..4).filter(|k| g.values[v[*k]] < iso).collect();
                    let outside: Vec<usize> = (0..4).filter(|k| g.values[v[*k]] >= iso).collect();
                    let emit = |tri: [u32; 3], m: &mut TriMesh, inside_pt: Vec3| {
                        // Orient so the normal points away from the inside.
                        let [a, b, cc] = [m.positions[tri[0] as usize], m.positions[tri[1] as usize], m.positions[tri[2] as usize]];
                        let n = (b - a).cross(cc - a);
                        if n.dot(a - inside_pt) < 0.0 {
                            m.indices.push([tri[0], tri[2], tri[1]]);
                        } else {
                            m.indices.push(tri);
                        }
                    };
                    match inside.len() {
                        1 | 3 => {
                            let (lone, others, lone_inside) = if inside.len() == 1 { (inside[0], outside, true) } else { (outside[0], inside, false) };
                            let tri = [vert(v[lone], v[others[0]], &mut m), vert(v[lone], v[others[1]], &mut m), vert(v[lone], v[others[2]], &mut m)];
                            let ip = if lone_inside { g.pos(v[lone]) } else { g.pos(v[others[0]]) };
                            emit(tri, &mut m, ip);
                        }
                        2 => {
                            let (i0, i1, o0, o1) = (inside[0], inside[1], outside[0], outside[1]);
                            let a = vert(v[i0], v[o0], &mut m);
                            let b = vert(v[i0], v[o1], &mut m);
                            let cq = vert(v[i1], v[o1], &mut m);
                            let d = vert(v[i1], v[o0], &mut m);
                            let ip = (g.pos(v[i0]) + g.pos(v[i1])) * 0.5;
                            emit([a, b, cq], &mut m, ip);
                            emit([a, cq, d], &mut m, ip);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    // Nodes lying exactly on the iso-surface create coincident vertices on different edges;
    // weld them so the mesh is a proper manifold for smoothing.
    m.weld(g.h * 1e-4);
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sphere() {
        let g = Grid::sample(Vec3::splat(-10.0), Vec3::splat(10.0), 0.5, |p| p.length() - 8.0);
        let m = extract(&g, 0.0);
        assert_eq!(m.boundary_edge_count(), 0, "watertight");
        let v = m.volume();
        let exact = 4.0 / 3.0 * std::f32::consts::PI * 512.0;
        assert!((v - exact).abs() / exact < 0.03, "{v} vs {exact}");
    }
}
