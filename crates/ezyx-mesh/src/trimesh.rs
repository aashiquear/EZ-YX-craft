use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TriMesh {
    pub positions: Vec<Vec3>,
    pub indices: Vec<[u32; 3]>,
    /// Optional per-vertex colours (sRGB 0..1) — used by the simulation renderer.
    #[serde(default)]
    pub colors: Vec<[f32; 3]>,
}

impl TriMesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
    pub fn triangle(&self, i: usize) -> [Vec3; 3] {
        let t = self.indices[i];
        [self.positions[t[0] as usize], self.positions[t[1] as usize], self.positions[t[2] as usize]]
    }
    pub fn bounds(&self) -> (Vec3, Vec3) {
        self.positions.iter().fold((Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)), |(lo, hi), p| (lo.min(*p), hi.max(*p)))
    }
    pub fn transform(&mut self, m: &Mat4) {
        for p in &mut self.positions {
            *p = m.transform_point3(*p);
        }
        if m.determinant() < 0.0 {
            for t in &mut self.indices {
                t.swap(1, 2);
            }
        }
    }
    pub fn append(&mut self, o: &TriMesh) {
        let base = self.positions.len() as u32;
        self.positions.extend_from_slice(&o.positions);
        self.indices.extend(o.indices.iter().map(|t| [t[0] + base, t[1] + base, t[2] + base]));
        if !o.colors.is_empty() || !self.colors.is_empty() {
            self.colors.resize(base as usize, [0.8; 3]);
            if o.colors.is_empty() {
                self.colors.resize(self.positions.len(), [0.8; 3]);
            } else {
                self.colors.extend_from_slice(&o.colors);
            }
        }
    }
    /// Area-weighted vertex normals.
    pub fn vertex_normals(&self) -> Vec<Vec3> {
        let mut n = vec![Vec3::ZERO; self.positions.len()];
        for t in &self.indices {
            let [a, b, c] = [self.positions[t[0] as usize], self.positions[t[1] as usize], self.positions[t[2] as usize]];
            let f = (b - a).cross(c - a);
            for i in t {
                n[*i as usize] += f;
            }
        }
        n.into_iter().map(|v| v.normalize_or_zero()).collect()
    }
    /// Merge vertices closer than `eps` (STL files store unshared vertices).
    pub fn weld(&mut self, eps: f32) {
        let inv = 1.0 / eps.max(1e-9);
        let mut map: HashMap<(i64, i64, i64), u32> = HashMap::new();
        let mut remap = Vec::with_capacity(self.positions.len());
        let mut pos = Vec::new();
        for p in &self.positions {
            let k = ((p.x * inv).round() as i64, (p.y * inv).round() as i64, (p.z * inv).round() as i64);
            let id = *map.entry(k).or_insert_with(|| {
                pos.push(*p);
                (pos.len() - 1) as u32
            });
            remap.push(id);
        }
        self.positions = pos;
        self.colors.clear();
        self.indices = self
            .indices
            .iter()
            .map(|t| [remap[t[0] as usize], remap[t[1] as usize], remap[t[2] as usize]])
            .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
            .collect();
    }
    /// Signed volume (positive for outward-oriented closed meshes).
    pub fn volume(&self) -> f32 {
        self.indices
            .iter()
            .map(|t| {
                let [a, b, c] = [self.positions[t[0] as usize], self.positions[t[1] as usize], self.positions[t[2] as usize]];
                a.dot(b.cross(c)) / 6.0
            })
            .sum()
    }
    /// Count edges used by != 2 triangles (0 for a watertight manifold).
    pub fn boundary_edge_count(&self) -> usize {
        let mut m: HashMap<(u32, u32), i32> = HashMap::new();
        for t in &self.indices {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *m.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        m.values().filter(|c| **c != 2).count()
    }
    /// Taubin λ|μ smoothing (shrink-free) — cleans marching-tetrahedra stair steps.
    pub fn smooth_taubin(&mut self, iterations: usize, lambda: f32, mu: f32) {
        let n = self.positions.len();
        let mut nb: Vec<Vec<u32>> = vec![Vec::new(); n];
        for t in &self.indices {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                nb[a as usize].push(b);
                nb[b as usize].push(a);
            }
        }
        for v in &mut nb {
            v.sort_unstable();
            v.dedup();
        }
        for it in 0..iterations * 2 {
            let f = if it % 2 == 0 { lambda } else { mu };
            let old = self.positions.clone();
            for (i, p) in self.positions.iter_mut().enumerate() {
                if nb[i].is_empty() {
                    continue;
                }
                let avg = nb[i].iter().map(|j| old[*j as usize]).sum::<Vec3>() / nb[i].len() as f32;
                *p = old[i] + (avg - old[i]) * f;
            }
        }
    }

    pub fn cube(min: Vec3, max: Vec3) -> TriMesh {
        let p = |x: bool, y: bool, z: bool| Vec3::new(if x { max.x } else { min.x }, if y { max.y } else { min.y }, if z { max.z } else { min.z });
        let positions = vec![
            p(false, false, false),
            p(true, false, false),
            p(true, true, false),
            p(false, true, false),
            p(false, false, true),
            p(true, false, true),
            p(true, true, true),
            p(false, true, true),
        ];
        let indices = vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
        TriMesh { positions, indices, colors: vec![] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cube_volume_and_closed() {
        let c = TriMesh::cube(Vec3::ZERO, Vec3::new(2.0, 3.0, 4.0));
        assert!((c.volume() - 24.0).abs() < 1e-4);
        assert_eq!(c.boundary_edge_count(), 0);
    }
}
