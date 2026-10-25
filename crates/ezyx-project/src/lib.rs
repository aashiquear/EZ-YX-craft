//! # ezyx-project — the document and everything that operates on it
//!
//! The GUI, the CLI and the AI assistant all manipulate a [`Project`] through the same
//! [`commands::Command`] API, which gives undo/redo and keeps the three front-ends consistent.
//!
//! File format: `.ezyx` = pretty JSON of [`Project`] (versioned via `version`). Human-diffable,
//! git-friendly, easy for an LLM to read.

pub mod agent_host;
pub mod commands;
pub mod demo;
pub mod img3d_remote;
pub mod import;
pub mod jobsheet;
pub mod machines;
pub mod ops;
pub mod pipeline;
pub mod registry;
pub mod text;

use ezyx_cad::Part;
use ezyx_core::{region::Region, Bounds2, Heightmap, JobSetup, Polyline};
use ezyx_mesh::TriMesh;
use serde::{Deserialize, Serialize};

pub use commands::{Command, History};
pub use machines::MachineProfile;
pub use ops::{OpKind, OperationSpec};
pub use pipeline::{CutStrategy, DesignFeature, FeatureKind, FeatureSource, Payload, PlanSettings, StockShape};

/// 2: pipeline features + stock shape (v1 files load unchanged; their operations become manual ones).
pub const FILE_VERSION: u32 = 2;

pub type Id = u64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    pub color: [u8; 3],
    /// Locked layers can't be selected or edited in the viewport.
    #[serde(default)]
    pub locked: bool,
}

/// How a vector was made (kept so shapes stay editable / regenerable).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VectorSource {
    Drawn,
    Rectangle { x: f64, y: f64, w: f64, h: f64, r: f64 },
    Circle { cx: f64, cy: f64, r: f64 },
    Ellipse { cx: f64, cy: f64, rx: f64, ry: f64 },
    Polygon { cx: f64, cy: f64, r: f64, sides: usize },
    Star { cx: f64, cy: f64, r_outer: f64, r_inner: f64, points: usize },
    Text { text: String, height: f64 },
    Traced { image: String },
    Imported { file: String },
    Derived { op: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorObject {
    pub id: Id,
    pub name: String,
    pub layer: usize,
    pub path: Polyline,
    pub source: VectorSource,
}

/// A 2.5D/3D relief component (from image, mesh, CAD part, or sculpting).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReliefObject {
    pub id: Id,
    pub name: String,
    pub heightmap: Heightmap,
    pub visible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshObject {
    pub id: Id,
    pub name: String,
    pub mesh: TriMesh,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartObject {
    pub id: Id,
    pub part: Part,
    pub visible: bool,
    /// Bumped on every edit in the Solid Modeler (pipeline features record the revision they used).
    #[serde(default)]
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub version: u32,
    pub name: String,
    pub job: JobSetup,
    pub machine: MachineProfile,
    pub layers: Vec<Layer>,
    pub vectors: Vec<VectorObject>,
    pub reliefs: Vec<ReliefObject>,
    pub meshes: Vec<MeshObject>,
    pub parts: Vec<PartObject>,
    pub operations: Vec<OperationSpec>,
    /// The pipeline timeline (order = design order).
    #[serde(default)]
    pub features: Vec<DesignFeature>,
    #[serde(default)]
    pub stock_shape: StockShape,
    pub post_id: String,
    next_id: Id,
}

impl Default for Project {
    fn default() -> Self {
        let machine = machines::builtin().into_iter().next().expect("profiles");
        Self {
            version: FILE_VERSION,
            name: "Untitled".into(),
            job: JobSetup::default(),
            post_id: machine.post_id.clone(),
            machine,
            layers: vec![Layer { name: "Layer 1".into(), visible: true, color: [30, 30, 30], locked: false }],
            vectors: vec![],
            reliefs: vec![],
            meshes: vec![],
            parts: vec![],
            operations: vec![],
            features: vec![],
            stock_shape: StockShape::Rectangle,
            next_id: 1,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

impl Project {
    pub fn alloc_id(&mut self) -> Id {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn add_vector(&mut self, name: impl Into<String>, path: Polyline, source: VectorSource) -> Id {
        let id = self.alloc_id();
        self.vectors.push(VectorObject { id, name: name.into(), layer: 0, path, source });
        id
    }

    pub fn vector(&self, id: Id) -> Option<&VectorObject> {
        self.vectors.iter().find(|v| v.id == id)
    }

    pub fn paths_for(&self, ids: &[Id]) -> Vec<Polyline> {
        if ids.is_empty() {
            return self.vectors.iter().map(|v| v.path.clone()).collect();
        }
        ids.iter().filter_map(|id| self.vector(*id)).map(|v| v.path.clone()).collect()
    }

    pub fn region_for(&self, ids: &[Id]) -> Region {
        let p = self.paths_for(ids);
        Region::from_polylines(p.iter())
    }

    pub fn design_bounds(&self) -> Bounds2 {
        let mut b = self.vectors.iter().fold(Bounds2::EMPTY, |a, v| a.union(&v.path.bounds()));
        for r in &self.reliefs {
            b = b.union(&r.heightmap.bounds());
        }
        b
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), ProjectError> {
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn load(path: &std::path::Path) -> Result<Self, ProjectError> {
        let p: Project = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        if p.version > FILE_VERSION {
            return Err(ProjectError::Other(format!("file version {} is newer than this app ({FILE_VERSION})", p.version)));
        }
        Ok(p)
    }

    /// Compact text summary (for the AI assistant and the status bar).
    pub fn summary(&self) -> String {
        let mut s = format!(
            "Project '{}': stock {:.1} x {:.1} x {:.1} mm ({:?}), material {}, machine {}, post {}.\n",
            self.name, self.job.width, self.job.height, self.job.thickness, self.job.kind, self.job.material, self.machine.name, self.post_id
        );
        s += &format!("{} vectors:\n", self.vectors.len());
        for v in self.vectors.iter().take(60) {
            let b = v.path.bounds();
            s += &format!(
                "  #{} '{}' {} pts {} bbox ({:.1},{:.1})-({:.1},{:.1})\n",
                v.id,
                v.name,
                v.path.len(),
                if v.path.closed { "closed" } else { "open" },
                b.min.x,
                b.min.y,
                b.max.x,
                b.max.y
            );
        }
        s += &format!("{} reliefs, {} meshes, {} parts.\n", self.reliefs.len(), self.meshes.len(), self.parts.len());
        s += &format!("{} pipeline features:\n", self.features.len());
        for f in &self.features {
            s += &format!(
                "  #{} '{}' {} from {} rev {}{}\n",
                f.id,
                f.name,
                f.kind.label(),
                f.source.label(),
                f.revision,
                if f.suppressed { " (suppressed)" } else { "" }
            );
        }
        s += &format!("{} operations:\n", self.operations.len());
        for o in &self.operations {
            s += &format!(
                "  #{} '{}' {} tool={} enabled={}{}\n",
                o.id,
                o.name,
                o.kind.label(),
                o.tool_id,
                o.enabled,
                o.feature_id.map(|f| format!(" feature={f}{}", if o.auto { " (auto)" } else { "" })).unwrap_or_default()
            );
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn save_load_roundtrip() {
        let mut p = Project::default();
        p.add_vector("sq", ezyx_core::path::shapes::rectangle(ezyx_core::DVec2::ZERO, 10.0, 10.0, 0.0), VectorSource::Drawn);
        let dir = std::env::temp_dir().join(format!("ezyx_proj_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("t.ezyx");
        p.save(&f).unwrap();
        let q = Project::load(&f).unwrap();
        assert_eq!(q.vectors.len(), 1);
        assert!(q.summary().contains("sq"));
    }
}
