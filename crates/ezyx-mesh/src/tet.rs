//! Volumetric tetrahedral meshes.
//!
//! Two generators:
//! * [`relief_solid`] – structured mesh of a relief (heightmap over a flat base): every grid column
//!   becomes a stack of hexahedra split 6-ways (Kuhn), so the mesh is conforming by construction
//!   and quality is controlled by keeping layer thickness ≈ cell size.
//! * [`from_sdf`] – lattice "stuffing" of any signed-distance solid: Kuhn-split background grid,
//!   boundary nodes snapped onto the surface along the SDF gradient, inverted/sliver elements
//!   rejected. Based on the ideas of isosurface stuffing (Labelle & Shewchuk 2007) in simplified
//!   form; a full BCC implementation with guaranteed dihedral bounds is on the roadmap.
//!
//! Exports: VTK legacy (ParaView), Gmsh MSH 2.2 (CalculiX / FEniCS / MOOSE via conversion).

use crate::{mt::KUHN_TETS, TriMesh};
use ezyx_core::Heightmap;
use glam::Vec3;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fmt::Write;

#[derive(Debug, Clone, Default)]
pub struct TetMesh {
    pub nodes: Vec<Vec3>,
    pub tets: Vec<[u32; 4]>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Quality {
    pub count: usize,
    /// Minimum dihedral angle (degrees) over the mesh.
    pub min_dihedral: f32,
    pub max_dihedral: f32,
    /// Mean normalised radius ratio (1 = regular tet).
    pub mean_radius_ratio: f32,
    pub min_radius_ratio: f32,
    pub volume: f32,
}

pub fn tet_volume(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> f32 {
    (b - a).dot((c - a).cross(d - a)) / 6.0
}

/// Normalised radius ratio 3·r_in / R_circ (1 for a regular tetrahedron, 0 for degenerate).
pub fn radius_ratio(p: [Vec3; 4]) -> f32 {
    let v = tet_volume(p[0], p[1], p[2], p[3]).abs();
    if v < 1e-20 {
        return 0.0;
    }
    let area = |a: Vec3, b: Vec3, c: Vec3| (b - a).cross(c - a).length() * 0.5;
    let s = area(p[0], p[1], p[2]) + area(p[0], p[1], p[3]) + area(p[0], p[2], p[3]) + area(p[1], p[2], p[3]);
    let r_in = 3.0 * v / s;
    // Circumradius via the standard formula.
    let a = p[1] - p[0];
    let b = p[2] - p[0];
    let c = p[3] - p[0];
    let num = a.length_squared() * b.cross(c) + b.length_squared() * c.cross(a) + c.length_squared() * a.cross(b);
    let r_circ = num.length() / (12.0 * v);
    (3.0 * r_in / r_circ).clamp(0.0, 1.0)
}

/// The six dihedral angles (degrees).
pub fn dihedrals(p: [Vec3; 4]) -> [f32; 6] {
    let edges = [(0, 1, 2, 3), (0, 2, 1, 3), (0, 3, 1, 2), (1, 2, 0, 3), (1, 3, 0, 2), (2, 3, 0, 1)];
    edges.map(|(i, j, k, l)| {
        let e = (p[j] - p[i]).normalize_or_zero();
        let u = p[k] - p[i];
        let w = p[l] - p[i];
        let u = (u - e * u.dot(e)).normalize_or_zero();
        let w = (w - e * w.dot(e)).normalize_or_zero();
        u.dot(w).clamp(-1.0, 1.0).acos().to_degrees()
    })
}

impl TetMesh {
    pub fn element(&self, i: usize) -> [Vec3; 4] {
        self.tets[i].map(|n| self.nodes[n as usize])
    }

    pub fn quality(&self) -> Quality {
        let per: Vec<(f32, f32, f32, f32)> = (0..self.tets.len())
            .into_par_iter()
            .map(|i| {
                let p = self.element(i);
                let d = dihedrals(p);
                let mn = d.iter().cloned().fold(180.0, f32::min);
                let mx = d.iter().cloned().fold(0.0, f32::max);
                (mn, mx, radius_ratio(p), tet_volume(p[0], p[1], p[2], p[3]))
            })
            .collect();
        let n = per.len().max(1) as f32;
        Quality {
            count: per.len(),
            min_dihedral: per.iter().map(|q| q.0).fold(180.0, f32::min),
            max_dihedral: per.iter().map(|q| q.1).fold(0.0, f32::max),
            mean_radius_ratio: per.iter().map(|q| q.2).sum::<f32>() / n,
            min_radius_ratio: per.iter().map(|q| q.2).fold(1.0, f32::min),
            volume: per.iter().map(|q| q.3).sum(),
        }
    }

    /// Ensure positive orientation for every element.
    pub fn orient(&mut self) {
        let nodes = &self.nodes;
        self.tets.par_iter_mut().for_each(|t| {
            let p = t.map(|n| nodes[n as usize]);
            if tet_volume(p[0], p[1], p[2], p[3]) < 0.0 {
                t.swap(2, 3);
            }
        });
    }

    /// Boundary faces (appearing in exactly one tet) as an outward triangle mesh.
    pub fn boundary(&self) -> TriMesh {
        let mut faces: HashMap<[u32; 3], ([u32; 3], usize)> = HashMap::new();
        for t in &self.tets {
            // Faces opposite each vertex, oriented outward for positively oriented tets.
            let fs = [[t[1], t[2], t[3]], [t[0], t[3], t[2]], [t[0], t[1], t[3]], [t[0], t[2], t[1]]];
            for f in fs {
                let mut k = f;
                k.sort_unstable();
                faces.entry(k).and_modify(|e| e.1 += 1).or_insert((f, 1));
            }
        }
        TriMesh { positions: self.nodes.clone(), indices: faces.into_values().filter(|(_, c)| *c == 1).map(|(f, _)| f).collect(), colors: vec![] }
    }

    pub fn to_vtk(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "# vtk DataFile Version 3.0\nEZ-YX-craft tet mesh\nASCII\nDATASET UNSTRUCTURED_GRID");
        let _ = writeln!(s, "POINTS {} float", self.nodes.len());
        for p in &self.nodes {
            let _ = writeln!(s, "{} {} {}", p.x, p.y, p.z);
        }
        let _ = writeln!(s, "CELLS {} {}", self.tets.len(), self.tets.len() * 5);
        for t in &self.tets {
            let _ = writeln!(s, "4 {} {} {} {}", t[0], t[1], t[2], t[3]);
        }
        let _ = writeln!(s, "CELL_TYPES {}", self.tets.len());
        for _ in &self.tets {
            let _ = writeln!(s, "10");
        }
        s
    }

    pub fn to_gmsh(&self) -> String {
        let mut s = String::from("$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$Nodes\n");
        let _ = writeln!(s, "{}", self.nodes.len());
        for (i, p) in self.nodes.iter().enumerate() {
            let _ = writeln!(s, "{} {} {} {}", i + 1, p.x, p.y, p.z);
        }
        let _ = writeln!(s, "$EndNodes\n$Elements\n{}", self.tets.len());
        for (i, t) in self.tets.iter().enumerate() {
            let _ = writeln!(s, "{} 4 2 1 1 {} {} {} {}", i + 1, t[0] + 1, t[1] + 1, t[2] + 1, t[3] + 1);
        }
        s.push_str("$EndElements\n");
        s
    }
}

/// Structured tet mesh of a relief: heightmap top, flat bottom at `base_z`.
/// `layers` = number of element layers through the thickness (graded linearly per column).
pub fn relief_solid(hm: &Heightmap, base_z: f32, layers: usize) -> TetMesh {
    let layers = layers.max(1);
    let (w, h) = (hm.width, hm.height);
    let mut nodes = Vec::with_capacity(w * h * (layers + 1));
    for k in 0..=layers {
        let t = k as f32 / layers as f32;
        for y in 0..h {
            for x in 0..w {
                let c = hm.cell_center(x, y);
                let top = hm.get(x, y).max(base_z + 1e-3);
                nodes.push(Vec3::new(c.x as f32, c.y as f32, base_z + (top - base_z) * t));
            }
        }
    }
    let id = |x: usize, y: usize, k: usize| (k * h * w + y * w + x) as u32;
    let mut tets = Vec::with_capacity((w - 1) * (h - 1) * layers * 6);
    for k in 0..layers {
        for y in 0..h - 1 {
            for x in 0..w - 1 {
                let c: [u32; 8] = std::array::from_fn(|b| id(x + (b & 1), y + ((b >> 1) & 1), k + ((b >> 2) & 1)));
                for t in KUHN_TETS {
                    tets.push([c[t[0]], c[t[1]], c[t[2]], c[t[3]]]);
                }
            }
        }
    }
    let mut m = TetMesh { nodes, tets };
    m.orient();
    m
}

/// Tet-mesh any SDF solid (negative inside) on a lattice of spacing `h` over [min, max].
pub fn from_sdf(min: Vec3, max: Vec3, h: f32, sdf: impl Fn(Vec3) -> f32 + Sync) -> TetMesh {
    let g = crate::mt::Grid::sample(min, max, h, &sdf);
    let grad = |p: Vec3| {
        let e = h * 0.05;
        Vec3::new(sdf(p + Vec3::X * e) - sdf(p - Vec3::X * e), sdf(p + Vec3::Y * e) - sdf(p - Vec3::Y * e), sdf(p + Vec3::Z * e) - sdf(p - Vec3::Z * e))
            .normalize_or_zero()
    };
    // Snap boundary nodes onto the surface (Newton steps along the gradient): inside nodes within
    // alpha·h, and outside nodes within one cell (they become the boundary layer).
    let alpha = 0.3;
    let mut pos: Vec<Vec3> = (0..g.values.len()).map(|i| g.pos(i)).collect();
    let mut val = g.values.clone();
    pos.par_iter_mut().zip(val.par_iter_mut()).for_each(|(p, v)| {
        if (*v <= 0.0 && -*v < alpha * h) || (*v > 0.0 && *v < h) {
            for _ in 0..3 {
                let d = sdf(*p);
                *p -= grad(*p) * d;
            }
            *v = 0.0;
        }
    });
    let mut used: HashMap<usize, u32> = HashMap::new();
    let mut nodes = Vec::new();
    let mut tets = Vec::new();
    for z in 0..g.nz - 1 {
        for y in 0..g.ny - 1 {
            for x in 0..g.nx - 1 {
                let c: [usize; 8] = std::array::from_fn(|b| g.idx(x + (b & 1), y + ((b >> 1) & 1), z + ((b >> 2) & 1)));
                for t in KUHN_TETS {
                    let v = [c[t[0]], c[t[1]], c[t[2]], c[t[3]]];
                    if v.iter().any(|i| val[*i] > 0.0) {
                        continue;
                    }
                    let p = v.map(|i| pos[i]);
                    let vol = tet_volume(p[0], p[1], p[2], p[3]).abs();
                    if vol < h * h * h * 1e-3 || radius_ratio(p) < 0.05 {
                        continue; // sliver from snapping
                    }
                    let ids = v.map(|i| {
                        *used.entry(i).or_insert_with(|| {
                            nodes.push(pos[i]);
                            (nodes.len() - 1) as u32
                        })
                    });
                    tets.push(ids);
                }
            }
        }
    }
    let mut m = TetMesh { nodes, tets };
    m.orient();
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use ezyx_core::DVec2;

    #[test]
    fn relief_block_volume_and_quality() {
        let hm = Heightmap::new(11, 11, 1.0, DVec2::ZERO, 0.0);
        let m = relief_solid(&hm, -5.0, 5);
        let q = m.quality();
        assert!((q.volume - 10.0 * 10.0 * 5.0).abs() < 0.05, "{}", q.volume);
        assert!(q.min_dihedral > 30.0, "{:?}", q);
        assert!(q.min_radius_ratio > 0.5);
        assert_eq!(m.boundary().boundary_edge_count(), 0);
        assert!(m.to_vtk().contains("CELL_TYPES"));
        assert!(m.to_gmsh().starts_with("$MeshFormat"));
    }

    #[test]
    fn sdf_sphere_tets() {
        let m = from_sdf(Vec3::splat(-6.0), Vec3::splat(6.0), 0.6, |p| p.length() - 5.0);
        let q = m.quality();
        let exact = 4.0 / 3.0 * std::f32::consts::PI * 125.0;
        assert!((q.volume - exact).abs() / exact < 0.08, "{} vs {}", q.volume, exact);
        assert!(q.volume > 0.0);
        assert!(q.min_radius_ratio >= 0.05);
    }
}
