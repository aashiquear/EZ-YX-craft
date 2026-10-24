//! # ezyx-cad — part design inside the carving app
//!
//! A SolidWorks/FreeCAD-style **feature tree** (sketch → extrude/revolve → fillet → pattern …)
//! evaluated on a **signed-distance-field kernel**. SDFs were chosen for the first version because:
//! * booleans, fillets (smooth min), shells and offsets are exact one-liners and never fail
//!   topologically — important for a beginner-friendly tool where "it just works" matters;
//! * the output we need for carving is a heightmap or a mesh, both of which SDFs produce directly
//!   (`ezyx-mesh::mt` for meshes, ray-marching for heightmaps);
//! * the same kernel feeds the tetrahedral mesher (`ezyx-mesh::tet::from_sdf`).
//!
//! Trade-off: no exact B-rep edges/faces for STEP export. A B-rep backend (e.g. the pure-Rust
//! `truck` kernel or OpenCascade via FFI) can implement the same [`Feature`] tree later; the tree is
//! kernel-agnostic data (serde) for exactly that reason. 2D sketch *constraints* (SolveSpace-style)
//! are on the roadmap; today sketches are fully-defined polylines from the 2D design module.

pub mod sdf;

use ezyx_core::{DVec2, DVec3, Heightmap, Polyline};
use ezyx_mesh::{mt, TriMesh};
use sdf::Sdf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BooleanOp {
    #[default]
    Add,
    Cut,
    Intersect,
}

/// Placement of a feature: translation (mm) and rotation (XYZ Euler, degrees).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Placement {
    pub translate: [f64; 3],
    pub rotate_deg: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Shape {
    Box {
        size: [f64; 3],
        corner_radius: f64,
    },
    Sphere {
        radius: f64,
    },
    Cylinder {
        radius: f64,
        height: f64,
    },
    Cone {
        r_bottom: f64,
        r_top: f64,
        height: f64,
    },
    Torus {
        major: f64,
        minor: f64,
    },
    /// Extrude closed sketch loops (even-odd) along +Z, with optional draft (degrees).
    Extrude {
        sketch: Vec<Polyline>,
        height: f64,
        draft_deg: f64,
    },
    /// Revolve a sketch drawn in the XZ half-plane (x ≥ 0 = radius) around Z.
    Revolve {
        profile: Vec<Polyline>,
        angle_deg: f64,
    },
    /// Heightmap relief as a solid (e.g. imported from the image module).
    Relief {
        heightmap: Heightmap,
        base: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Modifier {
    /// Round all edges of the result so far by radius (offset trick).
    Fillet {
        radius: f64,
    },
    /// Hollow with wall thickness.
    Shell {
        thickness: f64,
    },
    LinearPattern {
        count: usize,
        step: [f64; 3],
    },
    CircularPattern {
        count: usize,
        axis_z: bool,
    },
    Mirror {
        plane: MirrorPlane,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MirrorPlane {
    YZ,
    XZ,
    XY,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    pub name: String,
    pub op: BooleanOp,
    pub shape: Shape,
    pub placement: Placement,
    /// Blend radius with the existing body (0 = sharp boolean).
    pub blend: f64,
    pub modifiers: Vec<Modifier>,
    pub suppressed: bool,
}

impl Feature {
    pub fn new(name: impl Into<String>, op: BooleanOp, shape: Shape) -> Self {
        Self { name: name.into(), op, shape, placement: Placement::default(), blend: 0.0, modifiers: vec![], suppressed: false }
    }
    pub fn at(mut self, x: f64, y: f64, z: f64) -> Self {
        self.placement.translate = [x, y, z];
        self
    }
    pub fn rotated(mut self, rx: f64, ry: f64, rz: f64) -> Self {
        self.placement.rotate_deg = [rx, ry, rz];
        self
    }
    pub fn blended(mut self, r: f64) -> Self {
        self.blend = r;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Part {
    pub name: String,
    pub features: Vec<Feature>,
    /// Modifiers applied to the whole part (e.g. a global fillet).
    pub finish: Vec<Modifier>,
}

impl Part {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), ..Default::default() }
    }
    pub fn add(mut self, f: Feature) -> Self {
        self.features.push(f);
        self
    }

    /// Build the SDF tree.
    pub fn sdf(&self) -> Option<Sdf> {
        let mut body: Option<Sdf> = None;
        for f in self.features.iter().filter(|f| !f.suppressed) {
            let mut s = sdf::shape(&f.shape);
            for m in &f.modifiers {
                s = sdf::modify(s, m);
            }
            s = Sdf::Transform { inner: Box::new(s), translate: f.placement.translate, rotate_deg: f.placement.rotate_deg };
            body = match (body.take(), f.op) {
                (None, BooleanOp::Add) => Some(s),
                (None, _) => None, // cutting from nothing is a no-op
                (Some(b), BooleanOp::Add) => Some(Sdf::Union(Box::new(b), Box::new(s), f.blend)),
                (Some(b), BooleanOp::Cut) => Some(Sdf::Subtract(Box::new(b), Box::new(s), f.blend)),
                (Some(b), BooleanOp::Intersect) => Some(Sdf::Intersect(Box::new(b), Box::new(s), f.blend)),
            };
        }
        let mut b = body?;
        for m in &self.finish {
            b = sdf::modify(b, m);
        }
        Some(b)
    }

    /// Triangulate at resolution `h` (mm).
    pub fn mesh(&self, h: f64) -> TriMesh {
        let Some(s) = self.sdf() else { return TriMesh::default() };
        let (lo, hi) = s.bounds();
        let g = mt::Grid::sample(lo.as_vec3(), hi.as_vec3(), h as f32, |p| s.eval(p.as_dvec3()) as f32);
        let mut m = mt::extract(&g, 0.0);
        m.smooth_taubin(2, 0.4, -0.42);
        m
    }

    /// Tetrahedral mesh for analysis export.
    pub fn tet_mesh(&self, h: f64) -> ezyx_mesh::TetMesh {
        let Some(s) = self.sdf() else { return Default::default() };
        let (lo, hi) = s.bounds();
        ezyx_mesh::tet::from_sdf(lo.as_vec3(), hi.as_vec3(), h as f32, |p| s.eval(p.as_dvec3()) as f32)
    }

    /// Top-down heightmap of the part for 3D carving (sphere-traced from above).
    /// The part's top is placed at Z = 0 (stock top); returns heights ≤ 0, `floor` where empty.
    pub fn heightmap(&self, cell: f64, floor: f64) -> Heightmap {
        let Some(s) = self.sdf() else { return Heightmap::new(1, 1, cell, DVec2::ZERO, floor as f32) };
        let (lo, hi) = s.bounds();
        let mut hm = Heightmap::covering(DVec2::new(lo.x, lo.y), DVec2::new(hi.x - lo.x, hi.y - lo.y), cell, floor as f32);
        let top = hi.z;
        let mut hits: Vec<Option<f64>> = vec![None; hm.width * hm.height];
        for y in 0..hm.height {
            for x in 0..hm.width {
                let c = hm.cell_center(x, y);
                let mut z = top + cell;
                for _ in 0..512 {
                    let d = s.eval(DVec3::new(c.x, c.y, z));
                    if d < cell * 0.02 {
                        hits[y * hm.width + x] = Some(z);
                        break;
                    }
                    z -= d.max(cell * 0.05);
                    if z < lo.z - cell {
                        break;
                    }
                }
            }
        }
        // Put the part's highest point at Z = 0 (stock top).
        let zmax = hits.iter().flatten().cloned().fold(f64::NEG_INFINITY, f64::max);
        for (v, h) in hm.data.iter_mut().zip(hits) {
            if let Some(z) = h {
                *v = ((z - zmax) as f32).max(floor as f32);
            }
        }
        hm
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// A ready-made demo part (used by the "New 3D part" template and tests): a rounded plaque with a
/// revolved knob and four counterbored holes.
pub fn demo_part() -> Part {
    let mut p = Part::new("Demo knob plate")
        .add(Feature::new("Base", BooleanOp::Add, Shape::Box { size: [80.0, 50.0, 8.0], corner_radius: 3.0 }).at(0.0, 0.0, 4.0))
        .add(
            Feature::new(
                "Knob",
                BooleanOp::Add,
                Shape::Revolve {
                    profile: vec![Polyline::closed(vec![
                        DVec2::new(0.0, 0.0),
                        DVec2::new(15.0, 0.0),
                        DVec2::new(12.0, 10.0),
                        DVec2::new(6.0, 16.0),
                        DVec2::new(0.0, 17.0),
                    ])],
                    angle_deg: 360.0,
                },
            )
            .at(0.0, 0.0, 8.0)
            .blended(2.0),
        );
    let mut hole = Feature::new("Hole", BooleanOp::Cut, Shape::Cylinder { radius: 2.5, height: 30.0 }).at(-32.0, -17.0, 0.0);
    hole.modifiers.push(Modifier::LinearPattern { count: 2, step: [64.0, 0.0, 0.0] });
    let mut hole2 = hole.clone();
    hole2.name = "Hole row 2".into();
    hole2.placement.translate[1] = 17.0;
    p.features.push(hole);
    p.features.push(hole2);
    p.finish.push(Modifier::Fillet { radius: 0.8 });
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_volume() {
        let p = Part::new("b").add(Feature::new("b", BooleanOp::Add, Shape::Box { size: [10.0, 20.0, 5.0], corner_radius: 0.0 }));
        let m = p.mesh(0.5);
        assert_eq!(m.boundary_edge_count(), 0);
        assert!((m.volume() - 1000.0).abs() / 1000.0 < 0.03, "{}", m.volume());
    }

    #[test]
    fn cut_reduces_volume() {
        let base = Part::new("b").add(Feature::new("b", BooleanOp::Add, Shape::Box { size: [20.0, 20.0, 10.0], corner_radius: 0.0 }));
        let cut = base.clone().add(Feature::new("h", BooleanOp::Cut, Shape::Cylinder { radius: 4.0, height: 30.0 }).at(0.0, 0.0, -15.0));
        let v0 = base.mesh(0.5).volume();
        let v1 = cut.mesh(0.5).volume();
        let hole = std::f32::consts::PI * 16.0 * 10.0;
        assert!(((v0 - v1) - hole).abs() / hole < 0.1, "{} vs {}", v0 - v1, hole);
    }

    #[test]
    fn demo_part_meshes_and_heightmap() {
        let p = demo_part();
        let m = p.mesh(1.0);
        assert!(!m.is_empty());
        let hm = p.heightmap(1.0, -30.0);
        let (lo, hi) = hm.min_max();
        assert!(hi <= 0.01 && lo >= -30.0);
        // Knob top should be the highest point (≈ 0) near the centre.
        assert!(hm.sample(DVec2::ZERO) > -1.5);
        let json = p.to_json();
        let back: Part = serde_json::from_str(&json).unwrap();
        assert_eq!(back.features.len(), p.features.len());
    }

    #[test]
    fn extrude_sketch() {
        let sq = ezyx_core::path::shapes::rectangle(DVec2::new(-5.0, -5.0), 10.0, 10.0, 0.0);
        let p = Part::new("e").add(Feature::new("e", BooleanOp::Add, Shape::Extrude { sketch: vec![sq], height: 4.0, draft_deg: 0.0 }));
        let v = p.mesh(0.4).volume();
        assert!((v - 400.0).abs() / 400.0 < 0.05, "{v}");
    }
}
