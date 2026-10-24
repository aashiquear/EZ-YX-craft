//! # ezyx-mesh
//!
//! * [`trimesh`] – indexed triangle meshes, normals, welding, transforms.
//! * [`io`] – STL (binary/ASCII) and OBJ read/write.
//! * [`hm`] – heightmap ⇄ mesh (relief surfaces, closed relief solids).
//! * [`raster`] – mesh → top-down heightmap (3D carving of imported models) and mesh →
//!   cylindrical radius map (rotary carving).
//! * [`mt`] – marching tetrahedra isosurface extraction (used by the SDF CAD kernel and
//!   image-to-3D). Tetrahedra instead of cubes: no 256-case tables, no ambiguous cases.
//! * [`tet`] – volumetric tetrahedral meshing (structured relief solids and SDF solids),
//!   quality metrics, VTK / Gmsh export for FEA hand-off.
//! * [`img3d`] – image → 3D mesh backends (offline inflation baseline + plug-in interface for
//!   neural single-image reconstruction).

pub mod hm;
pub mod img3d;
pub mod io;
pub mod mt;
pub mod raster;
pub mod tet;
pub mod trimesh;

pub use tet::TetMesh;
pub use trimesh::TriMesh;

#[derive(Debug, thiserror::Error)]
pub enum MeshError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(String),
}
