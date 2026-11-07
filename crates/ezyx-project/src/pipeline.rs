//! The carving **pipeline**: Stock → Features → Toolpaths → Output.
//!
//! * **Stock** is the [`JobSetup`](ezyx_core::JobSetup) plus a [`StockShape`].
//! * **Features** are the timeline of things to carve ([`DesignFeature`]): 2D cuts drawn in the
//!   pipeline, images converted in the Image Studio (2D cut / 2.5D relief / photo V-carve) and solid
//!   parts from the Solid Modeler (2.5D relief or 3D model). Each feature remembers its
//!   [`FeatureSource`] so the user can go back to the editor that made it and re-import a new
//!   revision; [`reimport`] then merges it with everything downstream and reports conflicts.
//! * **Toolpaths** are planned automatically per feature ([`plan_feature`]): the best strategy and
//!   bits for a roughing + finishing cycle. The user can override bits / skip passes in the
//!   feature's [`PlanSettings`]; those choices survive re-planning and re-imports. Operations the
//!   user adds by hand (`OperationSpec::auto == false`) are never touched by the planner.
//! * **Output** is G-code for the selected machine post (`ops::export_gcode`) or a job handed to the
//!   Machine Control module.
//!
//! Everything here is headless and deterministic so the GUI, CLI and AI assistant share it.

use crate::ops::{OpKind, OperationSpec, ReliefSource};
use crate::{Id, MeshObject, Project, ReliefObject, VectorSource};
use ezyx_cam::ops::{pocket::PocketOptions, profile::ProfileOptions, profile::ProfileSide, raster3d::Raster3dOptions};
use ezyx_core::{region::Region, Bounds2, DVec2, DVec3, Heightmap, Polyline};
use ezyx_mesh::TriMesh;
use ezyx_tooldb::{Operation, ToolLibrary, ToolShape};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Outline of the blank the machine works from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StockShape {
    /// Rectangular board `width × height`.
    #[default]
    Rectangle,
    /// Round blank (disc) of diameter `width`, centred in the `width × height` box.
    Round,
}

impl StockShape {
    pub const ALL: [StockShape; 2] = [StockShape::Rectangle, StockShape::Round];
    pub fn label(self) -> &'static str {
        match self {
            StockShape::Rectangle => "Rectangle",
            StockShape::Round => "Round blank",
        }
    }
}

/// How 2D vectors are machined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum CutStrategy {
    /// Profile outside the closed shapes through the stock (with tabs).
    CutOut,
    /// Profile inside the closed shapes to a depth.
    InsideCut,
    /// Follow the line (open or closed vectors).
    OnLine,
    /// Clear the area inside closed shapes.
    Pocket,
    /// V-carve closed shapes (text, signs); deep areas get a flat floor cleared by an end mill.
    VCarve,
    /// Engrave along the vectors with a V-bit.
    Engrave,
    /// Drill at circle centres.
    Drill,
}

impl CutStrategy {
    pub const ALL: [CutStrategy; 7] =
        [CutStrategy::CutOut, CutStrategy::InsideCut, CutStrategy::OnLine, CutStrategy::Pocket, CutStrategy::VCarve, CutStrategy::Engrave, CutStrategy::Drill];
    pub fn label(self) -> &'static str {
        match self {
            CutStrategy::CutOut => "Cut out",
            CutStrategy::InsideCut => "Inside cut",
            CutStrategy::OnLine => "On the line",
            CutStrategy::Pocket => "Pocket",
            CutStrategy::VCarve => "V-carve",
            CutStrategy::Engrave => "Engrave",
            CutStrategy::Drill => "Drill",
        }
    }
    /// Sensible default depth for a new feature on stock of `thickness`.
    pub fn default_depth(self, thickness: f64) -> f64 {
        match self {
            CutStrategy::CutOut | CutStrategy::Drill => thickness,
            CutStrategy::InsideCut | CutStrategy::Pocket => (thickness * 0.25).clamp(1.0, 6.0),
            CutStrategy::VCarve => (thickness * 0.3).clamp(1.0, 6.0),
            CutStrategy::OnLine | CutStrategy::Engrave => 1.0,
        }
    }
    pub fn needs_closed(self) -> bool {
        matches!(self, CutStrategy::CutOut | CutStrategy::InsideCut | CutStrategy::Pocket | CutStrategy::VCarve)
    }
}

/// Which 3D object a model feature carves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelRef {
    Mesh(Id),
    Relief(Id),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FeatureKind {
    /// 2D vectors machined with a 2D strategy to `depth` (mm below the stock top).
    Cut2D { vector_ids: Vec<Id>, strategy: CutStrategy, depth: f64 },
    /// 2.5D relief (heightmap object), carved with 3D raster roughing + finishing.
    Relief { relief_id: Id },
    /// 3D model (mesh object in stock coordinates, top at Z = 0) carved top-down.
    Model3D { mesh_id: Id },
    /// Explicit 3D groove paths (photo V-carve) cut with a V-bit.
    Grooves { paths: Vec<Vec<DVec3>>, tool_id: String, max_depth: f64 },
}

impl FeatureKind {
    pub fn label(&self) -> String {
        match self {
            FeatureKind::Cut2D { strategy, .. } => format!("2D · {}", strategy.label()),
            FeatureKind::Relief { .. } => "2.5D relief".into(),
            FeatureKind::Model3D { .. } => "3D model".into(),
            FeatureKind::Grooves { .. } => "Photo V-carve".into(),
        }
    }
    pub fn icon(&self) -> &'static str {
        match self {
            FeatureKind::Cut2D { strategy: CutStrategy::VCarve, .. } => "∨",
            FeatureKind::Cut2D { strategy: CutStrategy::Drill, .. } => "⚲",
            FeatureKind::Cut2D { strategy: CutStrategy::Pocket, .. } => "▤",
            FeatureKind::Cut2D { .. } => "✂",
            FeatureKind::Relief { .. } => "⛰",
            FeatureKind::Model3D { .. } => "🧊",
            FeatureKind::Grooves { .. } => "📷",
        }
    }
}

/// Where a feature came from — and therefore which editor re-opens it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum FeatureSource {
    /// Drawn with the pipeline's own drawing tools.
    #[default]
    Drawn,
    /// Imported from a file (SVG, DXF, STL, OBJ…).
    File { path: String },
    /// Converted in the Image Studio. `mode` is the conversion (outline / centreline / relief /
    /// photo_vcarve), `settings` its options as JSON so the studio can reopen them.
    ImageStudio { image: String, mode: String, settings: serde_json::Value },
    /// Exported from the Solid Modeler (`part_id` indexes `Project::parts`).
    SolidModeler { part_id: Id, part_revision: u64, as_relief: bool },
    /// Created by the AI assistant.
    Assistant,
}

impl FeatureSource {
    pub fn label(&self) -> String {
        match self {
            FeatureSource::Drawn => "Drawn".into(),
            FeatureSource::File { path } => format!("File {}", short_name(path)),
            FeatureSource::ImageStudio { image, mode, .. } => format!("Image Studio · {mode} · {}", short_name(image)),
            FeatureSource::SolidModeler { part_id, part_revision, .. } => format!("Solid Modeler · part #{part_id} rev {part_revision}"),
            FeatureSource::Assistant => "Assistant".into(),
        }
    }
    /// The feature can be re-opened in another module's editor.
    pub fn editable_elsewhere(&self) -> bool {
        matches!(self, FeatureSource::ImageStudio { .. } | FeatureSource::SolidModeler { .. })
    }
}

fn short_name(p: &str) -> String {
    std::path::Path::new(p).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| p.to_string())
}

/// The user's choices for a feature's automatic toolpaths. `None` = let the planner pick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanSettings {
    pub roughing: bool,
    pub finishing: bool,
    pub rough_tool: Option<String>,
    pub finish_tool: Option<String>,
    /// Material left by roughing for the finishing pass (mm).
    pub allowance: f64,
    /// Holding tabs on cut-outs.
    pub tabs: bool,
    /// The roughing / finishing operation was hand-tuned by the user (it became a manual
    /// operation); the planner no longer generates that pass but still plans around it.
    pub hand_rough: bool,
    pub hand_finish: bool,
}

impl Default for PlanSettings {
    fn default() -> Self {
        Self { roughing: true, finishing: true, rough_tool: None, finish_tool: None, allowance: 0.3, tabs: true, hand_rough: false, hand_finish: false }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignFeature {
    pub id: Id,
    pub name: String,
    pub kind: FeatureKind,
    #[serde(default)]
    pub source: FeatureSource,
    /// Bumped on every re-import / geometry change.
    #[serde(default)]
    pub revision: u32,
    #[serde(default)]
    pub plan: PlanSettings,
    #[serde(default)]
    pub suppressed: bool,
}

/// Geometry delivered by an editor (pipeline drawing, Image Studio, Solid Modeler, file import).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Payload {
    /// 2D vectors in the editor's own coordinates; placed at the stock centre on first import.
    Vectors {
        paths: Vec<Polyline>,
        strategy: Option<CutStrategy>,
        depth: Option<f64>,
    },
    /// Heightmap with its top at Z = 0 (values ≤ 0).
    Relief {
        heightmap: Heightmap,
    },
    /// Mesh in any position; placed centred on the stock with its top at Z = 0.
    Mesh {
        mesh: TriMesh,
    },
    Grooves {
        paths: Vec<Vec<DVec3>>,
        tool_id: String,
        max_depth: f64,
    },
}

impl PartialEq for Payload {
    // Meshes have no `PartialEq`; compare the serialized form (only used for command equality).
    fn eq(&self, o: &Self) -> bool {
        serde_json::to_value(self).ok() == serde_json::to_value(o).ok()
    }
}

impl Payload {
    pub fn kind_label(&self) -> &'static str {
        match self {
            Payload::Vectors { .. } => "2D vectors",
            Payload::Relief { .. } => "2.5D relief",
            Payload::Mesh { .. } => "3D model",
            Payload::Grooves { .. } => "photo V-carve grooves",
        }
    }
    fn bounds(&self) -> Bounds2 {
        match self {
            Payload::Vectors { paths, .. } => paths.iter().fold(Bounds2::EMPTY, |b, p| b.union(&p.bounds())),
            Payload::Relief { heightmap } => heightmap.bounds(),
            Payload::Mesh { mesh } => {
                let (lo, hi) = mesh.bounds();
                Bounds2 { min: DVec2::new(lo.x as f64, lo.y as f64), max: DVec2::new(hi.x as f64, hi.y as f64) }
            }
            Payload::Grooves { paths, .. } => {
                let mut b = Bounds2::EMPTY;
                for p in paths.iter().flatten() {
                    b.include(DVec2::new(p.x, p.y));
                }
                b
            }
        }
    }
    fn translate(&mut self, d: DVec2) {
        match self {
            Payload::Vectors { paths, .. } => {
                let t = ezyx_core::DAffine2::from_translation(d);
                for p in paths.iter_mut() {
                    *p = p.transform(&t);
                }
            }
            Payload::Relief { heightmap } => heightmap.origin += d,
            Payload::Mesh { mesh } => mesh.transform(&glam::Mat4::from_translation(glam::Vec3::new(d.x as f32, d.y as f32, 0.0))),
            Payload::Grooves { paths, .. } => {
                for p in paths.iter_mut().flatten() {
                    p.x += d.x;
                    p.y += d.y;
                }
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Tool selection

/// Bundled library used when a caller has no library at hand (commands, tests).
pub fn default_library() -> &'static ToolLibrary {
    static LIB: OnceLock<ToolLibrary> = OnceLock::new();
    LIB.get_or_init(ToolLibrary::bundled)
}

fn by_ids<'a>(lib: &'a ToolLibrary, ids: &[&str]) -> Vec<&'a ezyx_tooldb::Tool> {
    ids.iter().filter_map(|id| lib.get(id)).collect()
}

/// Flat end mills, largest first (generic templates preferred so feeds come from chip load).
fn flats(lib: &ToolLibrary, metal: bool) -> Vec<&ezyx_tooldb::Tool> {
    let pref: &[&str] = if metal {
        &["generic.flat_6mm_1f_alu", "generic.flat_3mm_1f_alu", "generic.flat_1_16_2f"]
    } else {
        &["generic.flat_1_4_2f", "generic.flat_1_8_2f", "generic.flat_1_16_2f"]
    };
    let mut v = by_ids(lib, pref);
    if v.is_empty() {
        v = lib.tools.iter().filter(|t| t.shape == ToolShape::FlatEnd).collect();
        v.sort_by(|a, b| b.diameter.total_cmp(&a.diameter));
    }
    v
}

fn balls(lib: &ToolLibrary) -> Vec<&ezyx_tooldb::Tool> {
    let mut v = by_ids(lib, &["generic.ball_1_4", "generic.ball_1_8", "generic.ball_1_16"]);
    if v.is_empty() {
        v = lib.tools.iter().filter(|t| t.shape == ToolShape::BallNose).collect();
        v.sort_by(|a, b| b.diameter.total_cmp(&a.diameter));
    }
    v
}

fn vbit(lib: &ToolLibrary, angle: f64) -> Option<&ezyx_tooldb::Tool> {
    let id = format!("generic.v{}", angle.round() as i64);
    lib.get(&id).or_else(|| lib.tools.iter().find(|t| matches!(t.shape, ToolShape::VBit { .. })))
}

/// Radius of the largest circle that fits inside the region (sampled; ≥ 0).
pub fn inscribed_radius(region: &Region) -> f64 {
    let b = region.bounds();
    if b.is_empty() {
        return 0.0;
    }
    let size = b.size();
    let step = (size.x.max(size.y) / 48.0).max(0.1);
    let rings = region.rings();
    let clearance = |p: DVec2| -> f64 {
        if !region.contains(p) {
            return 0.0;
        }
        rings.iter().flat_map(|r| r.segments()).map(|(a, c)| ezyx_core::path::dist_point_segment(p, a, c)).fold(f64::INFINITY, f64::min)
    };
    let (mut best, mut at) = (0.0f64, b.center());
    let mut y = b.min.y + step * 0.5;
    while y < b.max.y {
        let mut x = b.min.x + step * 0.5;
        while x < b.max.x {
            let p = DVec2::new(x, y);
            let d = clearance(p);
            if d > best {
                (best, at) = (d, p);
            }
            x += step;
        }
        y += step;
    }
    // Pattern search around the best sample to get below the grid resolution.
    let mut h = step * 0.5;
    while h > step * 0.01 {
        let mut moved = false;
        for d in [DVec2::X, -DVec2::X, DVec2::Y, -DVec2::Y, DVec2::ONE, -DVec2::ONE, DVec2::new(1.0, -1.0), DVec2::new(-1.0, 1.0)] {
            let q = at + d * h;
            let c = clearance(q);
            if c > best {
                (best, at, moved) = (c, q, true);
            }
        }
        if !moved {
            h *= 0.5;
        }
    }
    best
}

/// Largest tool whose diameter is at most `max_d` and that still cuts something in `region`.
fn largest_fitting<'a>(cands: &[&'a ezyx_tooldb::Tool], region: &Region, max_d: f64) -> Option<&'a ezyx_tooldb::Tool> {
    cands.iter().copied().find(|t| t.diameter <= max_d + 1e-9 && !region.offset(-t.radius()).is_empty()).or_else(|| cands.last().copied())
}

// ------------------------------------------------------------------------------------------------
// Planning

fn op(name: impl Into<String>, kind: OpKind, tool: &str, pass: Operation, depth: f64, ids: Vec<Id>, f: &DesignFeature) -> OperationSpec {
    OperationSpec {
        id: 0,
        name: name.into(),
        enabled: true,
        kind,
        tool_id: tool.into(),
        pass,
        start_depth: 0.0,
        cut_depth: depth,
        vector_ids: ids,
        direction: Default::default(),
        cutting_override: None,
        feature_id: Some(f.id),
        auto: true,
        feature_rev: f.revision,
    }
}

/// The best roughing + finishing cycle for one feature. Operation ids are left at 0.
pub fn plan_feature(p: &Project, lib: &ToolLibrary, f: &DesignFeature) -> Result<Vec<OperationSpec>, String> {
    let mut ops = plan_feature_all(p, lib, f)?;
    // Passes the user replaced with a hand-tuned operation are not generated again.
    ops.retain(|o| !(o.pass == Operation::Roughing && f.plan.hand_rough) && !(o.pass == Operation::Finishing && f.plan.hand_finish));
    Ok(ops)
}

fn plan_feature_all(p: &Project, lib: &ToolLibrary, f: &DesignFeature) -> Result<Vec<OperationSpec>, String> {
    let metal = crate::ops::material_group(p).is_metal();
    let s = &f.plan;
    let th = p.job.thickness;
    let mut out = Vec::new();
    let pick = |over: &Option<String>, auto: Option<&ezyx_tooldb::Tool>| -> Result<String, String> {
        match over {
            Some(id) if lib.get(id).is_some() => Ok(id.clone()),
            Some(id) => Err(format!("tool '{id}' is not in the library")),
            None => auto.map(|t| t.id.clone()).ok_or_else(|| "no suitable tool in the library".to_string()),
        }
    };
    match &f.kind {
        FeatureKind::Cut2D { vector_ids, strategy, depth } => {
            let paths = p.paths_for(vector_ids);
            if paths.is_empty() {
                return Err(format!("feature '{}' has no vectors", f.name));
            }
            let region = Region::from_polylines(paths.iter().filter(|v| v.closed));
            let r_in = inscribed_radius(&region);
            let fl = flats(lib, metal);
            let depth = depth.max(0.05);
            match strategy {
                CutStrategy::CutOut | CutStrategy::InsideCut => {
                    let side = if *strategy == CutStrategy::CutOut { ProfileSide::Outside } else { ProfileSide::Inside };
                    // Outside cuts are limited by the gaps between parts; inside cuts by the shape.
                    let max_d = if side == ProfileSide::Inside {
                        (r_in * 1.6).max(0.5)
                    } else if depth > 12.0 {
                        6.5
                    } else {
                        6.4
                    };
                    let rough = largest_fitting(&fl, &region, max_d);
                    let tabs = (*strategy == CutStrategy::CutOut && s.tabs && depth >= th - 1e-6).then(ezyx_cam::ops::profile::Tabs::default);
                    if s.roughing {
                        let t = pick(&s.rough_tool, rough)?;
                        let o = ProfileOptions { side, allowance: if s.finishing { s.allowance } else { 0.0 }, tabs: tabs.clone(), ..Default::default() };
                        out.push(op(format!("{} · rough", f.name), OpKind::Profile(o), &t, Operation::Roughing, depth, vector_ids.clone(), f));
                    }
                    if s.finishing {
                        // Finish with the same or the next smaller cutter (cleaner corners, less load).
                        let auto = rough.and_then(|r| fl.iter().copied().find(|t| t.diameter <= r.diameter && t.diameter >= r.diameter * 0.45)).or(rough);
                        let t = pick(&s.finish_tool, auto)?;
                        let o = ProfileOptions { side, allowance: 0.0, tabs, ramp_length: 5.0, finish_pass: 0.0 };
                        out.push(op(format!("{} · finish", f.name), OpKind::Profile(o), &t, Operation::Finishing, depth, vector_ids.clone(), f));
                    }
                }
                CutStrategy::Pocket => {
                    if region.is_empty() {
                        return Err(format!("'{}': pockets need closed vectors", f.name));
                    }
                    let rough = largest_fitting(&fl, &region, (r_in * 1.2).max(0.5));
                    if s.roughing {
                        let t = pick(&s.rough_tool, rough)?;
                        let o = PocketOptions { allowance: if s.finishing { s.allowance } else { 0.0 }, ..Default::default() };
                        out.push(op(format!("{} · rough", f.name), OpKind::Pocket(o), &t, Operation::Roughing, depth, vector_ids.clone(), f));
                    }
                    if s.finishing {
                        // Smaller cutter reaches the corners the roughing tool left.
                        let auto = rough.and_then(|r| fl.iter().copied().find(|t| t.diameter < r.diameter - 1e-9)).or(rough);
                        let t = pick(&s.finish_tool, auto)?;
                        let o = PocketOptions { allowance: 0.0, ..Default::default() };
                        out.push(op(format!("{} · finish", f.name), OpKind::Pocket(o), &t, Operation::Finishing, depth, vector_ids.clone(), f));
                    }
                }
                CutStrategy::VCarve => {
                    if region.is_empty() {
                        return Err(format!("'{}': V-carving needs closed vectors", f.name));
                    }
                    // Wide shapes get a 90° bit (shallower), fine lettering a 60° bit.
                    let angle = if r_in > 6.0 { 90.0 } else { 60.0 };
                    let v = vbit(lib, angle);
                    let t = pick(&s.finish_tool, v)?;
                    let tool = lib.get(&t).ok_or("V-bit missing")?;
                    let half = tool.half_angle().unwrap_or(45f64.to_radians());
                    let full_v = r_in / half.tan().max(1e-3);
                    let needs_floor = full_v > depth;
                    let clearance = if needs_floor && s.roughing {
                        let floor = region.offset(-(depth * half.tan()));
                        Some(pick(&s.rough_tool, largest_fitting(&fl, &floor, (inscribed_radius(&floor) * 1.4).max(0.5)))?)
                    } else {
                        None
                    };
                    let kind = OpKind::VCarve { flat_depth: needs_floor.then_some(depth), step: 0.2, clearance_tool_id: clearance };
                    out.push(op(format!("{} · V-carve", f.name), kind, &t, Operation::Finishing, depth.max(full_v.min(depth)), vector_ids.clone(), f));
                }
                CutStrategy::Engrave | CutStrategy::OnLine => {
                    let (kind, auto) = if *strategy == CutStrategy::Engrave {
                        (OpKind::Engrave, vbit(lib, 90.0))
                    } else {
                        (
                            OpKind::Profile(ProfileOptions { side: ProfileSide::On, ..Default::default() }),
                            fl.iter().copied().find(|t| t.diameter <= 3.2).or(fl.last().copied()),
                        )
                    };
                    let t = pick(&s.finish_tool, auto)?;
                    out.push(op(format!("{} · {}", f.name, strategy.label().to_lowercase()), kind, &t, Operation::Finishing, depth, vector_ids.clone(), f));
                }
                CutStrategy::Drill => {
                    // Pick the drill closest to the hole size; holes bigger than any drill are
                    // pocketed instead (helical-style clearing with an end mill).
                    let holes: Vec<f64> = paths.iter().filter(|v| v.closed).map(|v| v.bounds().size().min_element()).collect();
                    let hole_d = holes.iter().cloned().fold(0.0, f64::max);
                    let drills: Vec<&ezyx_tooldb::Tool> = lib.tools.iter().filter(|t| matches!(t.shape, ToolShape::Drill { .. })).collect();
                    let best = drills.iter().copied().min_by(|a, b| (a.diameter - hole_d).abs().total_cmp(&(b.diameter - hole_d).abs()));
                    if s.finish_tool.is_none() && hole_d > best.map(|d| d.diameter + 0.5).unwrap_or(0.0) && !region.is_empty() {
                        let t = pick(&s.rough_tool, largest_fitting(&fl, &region, (r_in * 1.2).max(0.5)))?;
                        out.push(op(
                            format!("{} · bore", f.name),
                            OpKind::Pocket(PocketOptions::default()),
                            &t,
                            Operation::Roughing,
                            depth,
                            vector_ids.clone(),
                            f,
                        ));
                    } else {
                        let t = pick(&s.finish_tool, best)?;
                        out.push(op(format!("{} · drill", f.name), OpKind::Drill(Default::default()), &t, Operation::Roughing, depth, vector_ids.clone(), f));
                    }
                }
            }
        }
        FeatureKind::Relief { relief_id } => plan_3d(p, lib, f, ReliefSource::Relief(*relief_id), metal, &mut out)?,
        FeatureKind::Model3D { mesh_id } => plan_3d(p, lib, f, ReliefSource::Mesh(*mesh_id), metal, &mut out)?,
        FeatureKind::Grooves { paths, tool_id, max_depth } => {
            let t = pick(&s.finish_tool, lib.get(tool_id))?;
            let kind = OpKind::Paths3d { paths: paths.clone(), tolerance: 0.01 };
            out.push(op(format!("{} · grooves", f.name), kind, &t, Operation::Finishing, *max_depth, vec![], f));
        }
    }
    Ok(out)
}

fn plan_3d(p: &Project, lib: &ToolLibrary, f: &DesignFeature, src: ReliefSource, metal: bool, out: &mut Vec<OperationSpec>) -> Result<(), String> {
    let s = &f.plan;
    let (b, depth) = feature_extent(p, f).ok_or_else(|| format!("'{}': its 3D object is missing", f.name))?;
    let small = b.size().max_element() < 80.0;
    let depth = depth.min(p.job.thickness).max(0.1);
    let pick = |over: &Option<String>, auto: Option<&ezyx_tooldb::Tool>| -> Result<String, String> {
        match over {
            Some(id) if lib.get(id).is_some() => Ok(id.clone()),
            Some(id) => Err(format!("tool '{id}' is not in the library")),
            None => auto.map(|t| t.id.clone()).ok_or_else(|| "no suitable tool in the library".to_string()),
        }
    };
    if s.roughing {
        let fl = flats(lib, metal);
        let auto = if small { fl.iter().copied().find(|t| t.diameter <= 3.2).or(fl.first().copied()) } else { fl.first().copied() };
        let t = pick(&s.rough_tool, auto)?;
        let options = Raster3dOptions { roughing: true, allowance: if s.finishing { s.allowance } else { 0.0 }, boundary_only: true, ..Default::default() };
        let kind = OpKind::Raster3d { source: src.clone(), options, resolution: if small { 0.25 } else { 0.4 } };
        out.push(op(format!("{} · 3D rough", f.name), kind, &t, Operation::Roughing, depth, vec![], f));
    }
    if s.finishing {
        let bl = balls(lib);
        let auto = if small {
            bl.iter().copied().find(|t| t.diameter <= 1.6).or(bl.last().copied())
        } else {
            bl.iter().copied().find(|t| t.diameter <= 3.2).or(bl.first().copied())
        };
        let t = pick(&s.finish_tool, auto)?;
        let options = Raster3dOptions { boundary_only: true, ..Default::default() };
        let kind = OpKind::Raster3d { source: src, options, resolution: if small { 0.1 } else { 0.2 } };
        out.push(op(format!("{} · 3D finish", f.name), kind, &t, Operation::Finishing, depth, vec![], f));
    }
    Ok(())
}

/// Machining order: drilling, then area clearing / 3D, finishing, and through-cuts last so the
/// part stays held. Ties keep the timeline order.
fn stage(o: &OperationSpec) -> u8 {
    let through = matches!(&o.kind, OpKind::Profile(po) if po.side == ProfileSide::Outside);
    match (&o.kind, o.pass, through) {
        (OpKind::Drill(_), ..) => 0,
        (_, Operation::Roughing, false) => 1,
        (_, Operation::Finishing, false) => 2,
        (_, Operation::Roughing, true) => 3,
        (_, Operation::Finishing, true) => 4,
    }
}

/// Re-plan the automatic toolpaths of `ids` (all features when empty). Manual operations are kept.
/// Returns the number of operations generated and any planning errors.
pub fn replan(p: &mut Project, lib: &ToolLibrary, ids: &[Id]) -> (usize, Vec<String>) {
    let targets: Vec<Id> = p.features.iter().filter(|f| ids.is_empty() || ids.contains(&f.id)).map(|f| f.id).collect();
    p.operations.retain(|o| !(o.auto && o.feature_id.map(|f| targets.contains(&f)).unwrap_or(false)));
    // Auto ops of features that no longer exist go too.
    let live: Vec<Id> = p.features.iter().map(|f| f.id).collect();
    p.operations.retain(|o| !(o.auto && o.feature_id.map(|f| !live.contains(&f)).unwrap_or(false)));
    let mut errors = Vec::new();
    let mut n = 0;
    let feats = p.features.clone();
    for f in feats.iter().filter(|f| targets.contains(&f.id) && !f.suppressed) {
        match plan_feature(p, lib, f) {
            Ok(ops) => {
                for mut o in ops {
                    o.id = p.alloc_id();
                    p.operations.push(o);
                    n += 1;
                }
            }
            Err(e) => errors.push(e),
        }
    }
    // Suppressed features keep no automatic toolpaths.
    let suppressed: Vec<Id> = feats.iter().filter(|f| f.suppressed).map(|f| f.id).collect();
    p.operations.retain(|o| !(o.auto && o.feature_id.map(|f| suppressed.contains(&f)).unwrap_or(false)));
    sort_operations(p);
    (n, errors)
}

/// Stable machining order for automatic operations; manual ones stay after them in user order.
pub fn sort_operations(p: &mut Project) {
    let order: Vec<Id> = p.features.iter().map(|f| f.id).collect();
    let fpos = |o: &OperationSpec| o.feature_id.and_then(|f| order.iter().position(|x| *x == f)).unwrap_or(usize::MAX);
    let (mut auto, manual): (Vec<_>, Vec<_>) = std::mem::take(&mut p.operations).into_iter().partition(|o| o.auto);
    auto.sort_by_key(|o| (stage(o), fpos(o)));
    // Within a stage, keep operations that share a bit together (fewer tool changes); otherwise
    // the timeline order stands.
    let mut grouped: Vec<OperationSpec> = Vec::with_capacity(auto.len());
    let mut rest = auto;
    while !rest.is_empty() {
        let first = rest.remove(0);
        let (st, tool) = (stage(&first), first.tool_id.clone());
        grouped.push(first);
        let (same, others): (Vec<_>, Vec<_>) = rest.into_iter().partition(|o| stage(o) == st && o.tool_id == tool);
        grouped.extend(same);
        rest = others;
    }
    p.operations = grouped;
    p.operations.extend(manual);
}

/// Classify a computed toolpath as roughing or finishing for preview filtering (a V-carve's
/// clearance pass, cut by a different tool, counts as roughing).
pub fn toolpath_pass(op: &OperationSpec, tp: &ezyx_cam::Toolpath) -> Operation {
    if tp.tool.id != op.tool_id {
        Operation::Roughing
    } else {
        op.pass
    }
}

// ------------------------------------------------------------------------------------------------
// Feature creation / import / merge

/// Suggest a 2D strategy for freshly imported or traced vectors.
pub fn suggest_strategy(paths: &[Polyline], stock: &ezyx_core::JobSetup) -> CutStrategy {
    let closed: Vec<&Polyline> = paths.iter().filter(|p| p.closed).collect();
    if closed.is_empty() {
        return CutStrategy::Engrave;
    }
    if closed.len() * 2 < paths.len() {
        return CutStrategy::Engrave;
    }
    let b = closed.iter().fold(Bounds2::EMPTY, |a, p| a.union(&p.bounds()));
    let stock_area = stock.width * stock.height;
    let sz = b.size();
    // One big outline covering most of the stock: that's the part to cut out.
    if closed.len() <= 2 && sz.x * sz.y > 0.3 * stock_area {
        return CutStrategy::CutOut;
    }
    // Many small closed loops (text, logos): V-carve.
    let region = Region::from_polylines(closed.iter().copied());
    if closed.len() >= 3 && inscribed_radius(&region) < 8.0 {
        return CutStrategy::VCarve;
    }
    CutStrategy::Pocket
}

fn stock_center(p: &Project) -> DVec2 {
    DVec2::new(p.job.width, p.job.height) * 0.5
}

fn place_mesh_top(mesh: &mut TriMesh) {
    let (_, hi) = mesh.bounds();
    mesh.transform(&glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.0, -hi.z)));
}

/// Store the payload's geometry in the project and return the feature kind referencing it.
fn store_payload(p: &mut Project, name: &str, payload: Payload, source: &FeatureSource) -> FeatureKind {
    let vsrc = match source {
        FeatureSource::File { path } => VectorSource::Imported { file: path.clone() },
        FeatureSource::ImageStudio { image, .. } => VectorSource::Traced { image: image.clone() },
        _ => VectorSource::Drawn,
    };
    match payload {
        Payload::Vectors { paths, strategy, depth } => {
            let strategy = strategy.unwrap_or_else(|| suggest_strategy(&paths, &p.job));
            let ids = paths.into_iter().map(|pl| p.add_vector(name, pl, vsrc.clone())).collect();
            FeatureKind::Cut2D { vector_ids: ids, strategy, depth: depth.unwrap_or_else(|| strategy.default_depth(p.job.thickness)) }
        }
        Payload::Relief { heightmap } => {
            let id = p.alloc_id();
            p.reliefs.push(ReliefObject { id, name: name.into(), heightmap, visible: true });
            FeatureKind::Relief { relief_id: id }
        }
        Payload::Mesh { mut mesh } => {
            place_mesh_top(&mut mesh);
            let id = p.alloc_id();
            p.meshes.push(MeshObject { id, name: name.into(), mesh, visible: true });
            FeatureKind::Model3D { mesh_id: id }
        }
        Payload::Grooves { paths, tool_id, max_depth } => FeatureKind::Grooves { paths, tool_id, max_depth },
    }
}

/// Add a new feature from an editor's payload, centred on the stock, and plan its toolpaths.
pub fn import_feature(p: &mut Project, lib: &ToolLibrary, name: &str, mut payload: Payload, source: FeatureSource) -> Result<Id, String> {
    let b = payload.bounds();
    if b.is_empty() {
        return Err(format!("nothing to import: the {} are empty", payload.kind_label()));
    }
    // Vectors drawn in the pipeline already sit in stock coordinates; everything else is centred.
    if !matches!(source, FeatureSource::Drawn | FeatureSource::Assistant) || !matches!(payload, Payload::Vectors { .. }) {
        payload.translate(stock_center(p) - b.center());
    }
    let kind = store_payload(p, name, payload, &source);
    let id = p.alloc_id();
    p.features.push(DesignFeature { id, name: name.into(), kind, source, revision: 1, plan: PlanSettings::default(), suppressed: false });
    let (_, errs) = replan(p, lib, &[id]);
    if let Some(e) = errs.first() {
        return Err(e.clone());
    }
    Ok(id)
}

/// Turn existing vectors into a 2D feature (the pipeline's "carve this" buttons).
pub fn feature_from_vectors(
    p: &mut Project,
    lib: &ToolLibrary,
    ids: &[Id],
    strategy: CutStrategy,
    depth: Option<f64>,
    name: Option<String>,
) -> Result<Id, String> {
    if ids.is_empty() {
        return Err("select the vectors to carve first".into());
    }
    let paths = p.paths_for(ids);
    if strategy.needs_closed() && !paths.iter().any(|v| v.closed) {
        return Err(format!("{} needs closed vectors", strategy.label()));
    }
    // A vector belongs to one feature: take it away from any previous one.
    for f in p.features.iter_mut() {
        if let FeatureKind::Cut2D { vector_ids, .. } = &mut f.kind {
            vector_ids.retain(|v| !ids.contains(v));
        }
    }
    p.features.retain(|f| !matches!(&f.kind, FeatureKind::Cut2D { vector_ids, .. } if vector_ids.is_empty()));
    let id = p.alloc_id();
    let name = name.unwrap_or_else(|| format!("{} {}", strategy.label(), p.features.len() + 1));
    let depth = depth.unwrap_or_else(|| strategy.default_depth(p.job.thickness));
    p.features.push(DesignFeature {
        id,
        name,
        kind: FeatureKind::Cut2D { vector_ids: ids.to_vec(), strategy, depth },
        source: FeatureSource::Drawn,
        revision: 1,
        plan: PlanSettings::default(),
        suppressed: false,
    });
    let (_, errs) = replan(p, lib, &[]);
    match errs.first() {
        Some(e) => Err(e.clone()),
        None => Ok(id),
    }
}

/// Delete a feature, its automatic toolpaths and (optionally) its geometry.
pub fn remove_feature(p: &mut Project, id: Id, delete_geometry: bool) -> Result<String, String> {
    let i = p.features.iter().position(|f| f.id == id).ok_or_else(|| format!("no feature #{id}"))?;
    let f = p.features.remove(i);
    p.operations.retain(|o| !(o.auto && o.feature_id == Some(id)));
    for o in p.operations.iter_mut().filter(|o| o.feature_id == Some(id)) {
        o.feature_id = None;
    }
    if delete_geometry {
        match &f.kind {
            FeatureKind::Cut2D { vector_ids, .. } => p.vectors.retain(|v| !vector_ids.contains(&v.id)),
            FeatureKind::Relief { relief_id } => p.reliefs.retain(|r| r.id != *relief_id),
            FeatureKind::Model3D { mesh_id } => p.meshes.retain(|m| m.id != *mesh_id),
            FeatureKind::Grooves { .. } => {}
        }
    }
    Ok(format!("Removed feature '{}'", f.name))
}

/// What happened when a new revision of a feature came back from its editor.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MergeReport {
    pub feature: String,
    pub old_revision: u32,
    pub new_revision: u32,
    /// Footprint before → after (mm).
    pub old_size: DVec2,
    pub new_size: DVec2,
    pub regenerated_ops: usize,
    /// Hand-made operations that pointed at the old geometry and were re-targeted.
    pub retargeted_ops: Vec<String>,
    /// Conflicts that appeared with this revision.
    pub new_conflicts: Vec<Conflict>,
    /// Conflicts this revision resolved.
    pub resolved_conflicts: Vec<Conflict>,
    pub errors: Vec<String>,
}

impl MergeReport {
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty() && !self.new_conflicts.iter().any(|c| c.severity == Severity::Error)
    }
    pub fn summary(&self) -> String {
        let mut s = format!(
            "Merged '{}' rev {} → {}: footprint {:.1}×{:.1} → {:.1}×{:.1} mm, {} toolpath(s) regenerated",
            self.feature, self.old_revision, self.new_revision, self.old_size.x, self.old_size.y, self.new_size.x, self.new_size.y, self.regenerated_ops
        );
        if !self.retargeted_ops.is_empty() {
            s += &format!("; re-targeted manual operations: {}", self.retargeted_ops.join(", "));
        }
        for c in &self.new_conflicts {
            s += &format!("\n⚠ {}", c.message);
        }
        for c in &self.resolved_conflicts {
            s += &format!("\n✔ resolved: {}", c.message);
        }
        for e in &self.errors {
            s += &format!("\n✖ {e}");
        }
        s
    }
}

/// Replace a feature's geometry with a new revision from its editor, keep its placement on the
/// stock and the user's plan settings, re-plan its toolpaths and report conflicts.
pub fn reimport(p: &mut Project, lib: &ToolLibrary, id: Id, mut payload: Payload, source: Option<FeatureSource>) -> Result<MergeReport, String> {
    let before = analyze(p, lib);
    let fi = p.features.iter().position(|f| f.id == id).ok_or_else(|| format!("no feature #{id}"))?;
    let old = p.features[fi].clone();
    let (old_b, _) = feature_extent(p, &old).unwrap_or((Bounds2 { min: stock_center(p), max: stock_center(p) }, 0.0));
    let nb = payload.bounds();
    if nb.is_empty() {
        return Err("the new revision is empty".into());
    }
    // Keep the feature where the user placed it: align the new footprint's centre with the old.
    payload.translate(old_b.center() - nb.center());
    let mut report = MergeReport {
        feature: old.name.clone(),
        old_revision: old.revision,
        new_revision: old.revision + 1,
        old_size: old_b.size(),
        new_size: nb.size(),
        ..Default::default()
    };

    let new_kind = match (&old.kind, payload) {
        (FeatureKind::Cut2D { vector_ids, strategy, depth }, Payload::Vectors { paths, strategy: ns, depth: nd }) => {
            let layer = p.vectors.iter().find(|v| vector_ids.contains(&v.id)).map(|v| v.layer).unwrap_or(0);
            p.vectors.retain(|v| !vector_ids.contains(&v.id));
            let vsrc = match source.as_ref().unwrap_or(&old.source) {
                FeatureSource::ImageStudio { image, .. } => VectorSource::Traced { image: image.clone() },
                FeatureSource::File { path } => VectorSource::Imported { file: path.clone() },
                _ => VectorSource::Drawn,
            };
            let new_ids: Vec<Id> = paths
                .into_iter()
                .map(|pl| {
                    let i = p.add_vector(old.name.clone(), pl, vsrc.clone());
                    if let Some(v) = p.vectors.iter_mut().find(|v| v.id == i) {
                        v.layer = layer;
                    }
                    i
                })
                .collect();
            // Manual operations that used any of the old vectors now use the new revision.
            for o in p.operations.iter_mut().filter(|o| !o.auto) {
                if o.vector_ids.iter().any(|v| vector_ids.contains(v)) {
                    o.vector_ids.retain(|v| !vector_ids.contains(v));
                    o.vector_ids.extend(new_ids.iter().copied());
                    o.feature_rev = old.revision + 1;
                    report.retargeted_ops.push(o.name.clone());
                }
            }
            FeatureKind::Cut2D { vector_ids: new_ids, strategy: ns.unwrap_or(*strategy), depth: nd.unwrap_or(*depth) }
        }
        (FeatureKind::Relief { relief_id }, Payload::Relief { heightmap }) => {
            let r = p.reliefs.iter_mut().find(|r| r.id == *relief_id).ok_or("relief object missing")?;
            r.heightmap = heightmap;
            FeatureKind::Relief { relief_id: *relief_id }
        }
        (FeatureKind::Model3D { mesh_id }, Payload::Mesh { mut mesh }) => {
            place_mesh_top(&mut mesh);
            let m = p.meshes.iter_mut().find(|m| m.id == *mesh_id).ok_or("mesh object missing")?;
            m.mesh = mesh;
            FeatureKind::Model3D { mesh_id: *mesh_id }
        }
        (FeatureKind::Grooves { .. }, Payload::Grooves { paths, tool_id, max_depth }) => FeatureKind::Grooves { paths, tool_id, max_depth },
        // The editor now sends a different kind (e.g. a part switched from 2.5D to 3D): replace.
        (_, payload) => {
            let name = old.name.clone();
            remove_geometry(p, &old.kind);
            store_payload(p, &name, payload, source.as_ref().unwrap_or(&old.source))
        }
    };
    {
        let f = &mut p.features[fi];
        f.kind = new_kind;
        f.revision += 1;
        if let Some(s) = source {
            f.source = s;
        }
    }
    let (n, errs) = replan(p, lib, &[id]);
    report.regenerated_ops = n;
    report.errors = errs;
    let after = analyze(p, lib);
    report.new_conflicts = after.iter().filter(|c| !before.contains(c)).cloned().collect();
    report.resolved_conflicts = before.iter().filter(|c| !after.contains(c)).cloned().collect();
    Ok(report)
}

fn remove_geometry(p: &mut Project, k: &FeatureKind) {
    match k {
        FeatureKind::Cut2D { vector_ids, .. } => p.vectors.retain(|v| !vector_ids.contains(&v.id)),
        FeatureKind::Relief { relief_id } => p.reliefs.retain(|r| r.id != *relief_id),
        FeatureKind::Model3D { mesh_id } => p.meshes.retain(|m| m.id != *mesh_id),
        FeatureKind::Grooves { .. } => {}
    }
}

// ------------------------------------------------------------------------------------------------
// Conflict analysis

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conflict {
    pub severity: Severity,
    pub features: Vec<Id>,
    pub message: String,
}

/// XY footprint and depth (mm below the stock top) of a feature.
pub fn feature_extent(p: &Project, f: &DesignFeature) -> Option<(Bounds2, f64)> {
    match &f.kind {
        FeatureKind::Cut2D { vector_ids, depth, .. } => {
            let b = p.vectors.iter().filter(|v| vector_ids.contains(&v.id)).fold(Bounds2::EMPTY, |a, v| a.union(&v.path.bounds()));
            (!b.is_empty()).then_some((b, *depth))
        }
        FeatureKind::Relief { relief_id } => p.reliefs.iter().find(|r| r.id == *relief_id).map(|r| {
            let (lo, _) = r.heightmap.min_max();
            (relief_footprint(&r.heightmap), -lo as f64)
        }),
        FeatureKind::Model3D { mesh_id } => p.meshes.iter().find(|m| m.id == *mesh_id).map(|m| {
            let (lo, hi) = m.mesh.bounds();
            (Bounds2 { min: DVec2::new(lo.x as f64, lo.y as f64), max: DVec2::new(hi.x as f64, hi.y as f64) }, (hi.z - lo.z) as f64)
        }),
        FeatureKind::Grooves { paths, max_depth, .. } => {
            let mut b = Bounds2::EMPTY;
            for q in paths.iter().flatten() {
                b.include(DVec2::new(q.x, q.y));
            }
            (!b.is_empty()).then_some((b, *max_depth))
        }
    }
}

/// Bounds of the cells that actually carry relief (above the background level).
fn relief_footprint(hm: &Heightmap) -> Bounds2 {
    let (lo, _) = hm.min_max();
    let mut b = Bounds2::EMPTY;
    for y in 0..hm.height {
        for x in 0..hm.width {
            if hm.get(x, y) > lo + 1e-3 {
                b.include(hm.cell_center(x, y));
            }
        }
    }
    if b.is_empty() {
        hm.bounds()
    } else {
        b
    }
}

fn footprint_region(p: &Project, f: &DesignFeature) -> Region {
    match &f.kind {
        FeatureKind::Cut2D { vector_ids, strategy, .. } => {
            let paths = p.paths_for(vector_ids);
            let closed = Region::from_polylines(paths.iter().filter(|v| v.closed));
            if !matches!(strategy, CutStrategy::Pocket | CutStrategy::VCarve) || closed.is_empty() {
                // Profiles, engraving and lines only touch a narrow band along the vectors.
                let band: Vec<Polyline> = paths.clone();
                let mut r = Region::default();
                for b in band {
                    let ring = if b.closed { b } else { Polyline::closed(b.pts.iter().chain(b.pts.iter().rev()).copied().collect()) };
                    let one = Region::from_polylines([&ring]);
                    let outer = one.offset(1.6);
                    let inner = one.offset(-1.6);
                    r = r.union(&outer.difference(&inner));
                }
                r
            } else {
                closed
            }
        }
        _ => match feature_extent(p, f) {
            Some((b, _)) => Region::from_polylines([&ezyx_core::path::shapes::rectangle(b.min, b.size().x, b.size().y, 0.0)]),
            None => Region::default(),
        },
    }
}

/// Every problem the pipeline can see: features off the stock, too deep, overlapping other
/// features, missing geometry, tools that don't fit, stale hand-made operations.
pub fn analyze(p: &Project, lib: &ToolLibrary) -> Vec<Conflict> {
    let mut out = Vec::new();
    let th = p.job.thickness;
    let stock = p.job.stock_bounds();
    let mk = |sev, ids: Vec<Id>, msg: String| Conflict { severity: sev, features: ids, message: msg };
    for f in p.features.iter().filter(|f| !f.suppressed) {
        let Some((b, depth)) = feature_extent(p, f) else {
            out.push(mk(Severity::Error, vec![f.id], format!("'{}' lost its geometry — re-import it or delete the feature", f.name)));
            continue;
        };
        let eps = 1e-6;
        let outside = match p.stock_shape {
            StockShape::Rectangle => b.min.x < stock.min.x - eps || b.min.y < stock.min.y - eps || b.max.x > stock.max.x + eps || b.max.y > stock.max.y + eps,
            StockShape::Round => {
                let c = stock_center(p);
                let r = p.job.width.min(p.job.height) * 0.5;
                [b.min, b.max, DVec2::new(b.min.x, b.max.y), DVec2::new(b.max.x, b.min.y)].iter().any(|q| (*q - c).length() > r + eps)
            }
        };
        if outside {
            out.push(mk(Severity::Error, vec![f.id], format!("'{}' extends past the stock edge", f.name)));
        }
        let through = matches!(&f.kind, FeatureKind::Cut2D { strategy: CutStrategy::CutOut | CutStrategy::Drill, .. });
        if depth > th + 1e-6 && !through {
            out.push(mk(Severity::Error, vec![f.id], format!("'{}' is {depth:.1} mm deep but the stock is only {th:.1} mm thick", f.name)));
        }
        if let FeatureKind::Cut2D { vector_ids, strategy, .. } = &f.kind {
            let paths = p.paths_for(vector_ids);
            if strategy.needs_closed() && paths.iter().any(|v| !v.closed) {
                out.push(mk(Severity::Warning, vec![f.id], format!("'{}' has open vectors that {} will ignore", f.name, strategy.label())));
            }
            if *strategy == CutStrategy::CutOut && !f.plan.tabs && p.job.kind == ezyx_core::JobKind::Flat {
                out.push(mk(Severity::Info, vec![f.id], format!("'{}' cuts through without tabs — clamp or tape the part", f.name)));
            }
        }
        // Tool fit for automatic ops of this feature.
        if let FeatureKind::Cut2D { vector_ids, strategy: CutStrategy::Pocket | CutStrategy::InsideCut | CutStrategy::VCarve, .. } = &f.kind {
            let region = p.region_for(vector_ids);
            for o in p.operations.iter().filter(|o| o.feature_id == Some(f.id)) {
                if let Some(t) = lib.get(&o.tool_id) {
                    if matches!(t.shape, ToolShape::FlatEnd | ToolShape::BallNose) && region.offset(-t.radius()).is_empty() {
                        out.push(mk(
                            Severity::Error,
                            vec![f.id],
                            format!("'{}': {} ({:.2} mm) is too big to cut anything in '{}'", o.name, t.name, t.diameter, f.name),
                        ));
                    }
                }
            }
        }
    }
    // Overlaps between features (the deeper cut wins — usually not what the user wanted).
    let feats: Vec<&DesignFeature> = p.features.iter().filter(|f| !f.suppressed).collect();
    let regions: Vec<Region> = feats.iter().map(|f| footprint_region(p, f)).collect();
    for i in 0..feats.len() {
        for j in i + 1..feats.len() {
            let both_cutouts = [feats[i], feats[j]].iter().all(|f| matches!(f.kind, FeatureKind::Cut2D { strategy: CutStrategy::CutOut, .. }));
            if both_cutouts || regions[i].is_empty() || regions[j].is_empty() {
                continue;
            }
            let a = regions[i].intersection(&regions[j]).area();
            if a > 1.0 {
                out.push(mk(
                    Severity::Warning,
                    vec![feats[i].id, feats[j].id],
                    format!("'{}' overlaps '{}' by {a:.0} mm² — the deeper cut wins there", feats[i].name, feats[j].name),
                ));
            }
        }
    }
    // Operations still pointing at vanished or stale geometry.
    for o in p.operations.iter().filter(|o| !o.auto) {
        if o.vector_ids.iter().any(|id| p.vector(*id).is_none()) {
            out.push(mk(Severity::Warning, vec![], format!("operation '{}' references deleted vectors", o.name)));
        }
        if let Some(f) = o.feature_id.and_then(|id| p.features.iter().find(|f| f.id == id)) {
            if o.feature_rev != 0 && o.feature_rev != f.revision {
                out.push(mk(
                    Severity::Info,
                    vec![f.id],
                    format!("'{}' was made for revision {} of '{}' (now {}) — check it", o.name, o.feature_rev, f.name, f.revision),
                ));
            }
        }
    }
    out.sort_by_key(|c| std::cmp::Reverse(c.severity));
    out
}

/// Remove stock outside a round blank from a Z-map simulation (cells outside become bottom).
pub fn mask_round_stock(sim: &mut ezyx_sim::StockSim, job: &ezyx_core::JobSetup) {
    let c = DVec2::new(job.width, job.height) * 0.5;
    let r = job.width.min(job.height) * 0.5;
    let hm = &mut sim.hm;
    for y in 0..hm.height {
        for x in 0..hm.width {
            if (hm.cell_center(x, y) - c).length() > r {
                let i = hm.idx(x, y);
                hm.data[i] = sim.bottom;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ezyx_core::path::shapes::{circle, rectangle};

    fn lib() -> &'static ToolLibrary {
        default_library()
    }

    #[test]
    fn inscribed_radius_of_rectangle() {
        let r = Region::from_polylines([&rectangle(DVec2::ZERO, 40.0, 10.0, 0.0)]);
        let ri = inscribed_radius(&r);
        assert!((ri - 5.0).abs() < 0.3, "{ri}");
    }

    #[test]
    fn pocket_plan_picks_big_rough_small_finish() {
        let mut p = Project::default();
        let v = p.add_vector("big", rectangle(DVec2::new(50.0, 50.0), 80.0, 60.0, 0.0), VectorSource::Drawn);
        let id = feature_from_vectors(&mut p, lib(), &[v], CutStrategy::Pocket, Some(4.0), None).unwrap();
        let ops: Vec<&OperationSpec> = p.operations.iter().filter(|o| o.feature_id == Some(id)).collect();
        assert_eq!(ops.len(), 2);
        let rough = lib().get(&ops[0].tool_id).unwrap();
        let finish = lib().get(&ops[1].tool_id).unwrap();
        assert_eq!(ops[0].pass, Operation::Roughing);
        assert!((rough.diameter - 6.35).abs() < 1e-6, "{}", rough.diameter);
        assert!(finish.diameter < rough.diameter);
        assert!(matches!(&ops[0].kind, OpKind::Pocket(o) if (o.allowance - 0.3).abs() < 1e-9));
        assert!(matches!(&ops[1].kind, OpKind::Pocket(o) if o.allowance == 0.0));
        assert!((ops[0].cut_depth - 4.0).abs() < 1e-9);
    }

    #[test]
    fn narrow_slot_gets_small_tool() {
        let mut p = Project::default();
        let v = p.add_vector("slot", rectangle(DVec2::new(20.0, 20.0), 60.0, 2.4, 0.0), VectorSource::Drawn);
        feature_from_vectors(&mut p, lib(), &[v], CutStrategy::Pocket, Some(2.0), None).unwrap();
        let t = lib().get(&p.operations[0].tool_id).unwrap();
        assert!(t.diameter <= 1.6 + 1e-9, "slot 2.4 mm got a {} mm tool", t.diameter);
    }

    #[test]
    fn cutout_rough_and_finish_with_tabs_last_in_order() {
        let mut p = Project::default();
        let outline = p.add_vector("outline", rectangle(DVec2::new(10.0, 10.0), 200.0, 150.0, 5.0), VectorSource::Drawn);
        let hole = p.add_vector("hole", rectangle(DVec2::new(60.0, 60.0), 40.0, 40.0, 0.0), VectorSource::Drawn);
        feature_from_vectors(&mut p, lib(), &[outline], CutStrategy::CutOut, None, None).unwrap();
        feature_from_vectors(&mut p, lib(), &[hole], CutStrategy::Pocket, Some(5.0), None).unwrap();
        // Pocket ops come first, the through cut last.
        let kinds: Vec<&str> = p.operations.iter().map(|o| o.kind.label()).collect();
        assert_eq!(kinds, ["Pocket", "Pocket", "Profile", "Profile"]);
        let cut = &p.operations[2];
        assert!((cut.cut_depth - p.job.thickness).abs() < 1e-9);
        match &cut.kind {
            OpKind::Profile(o) => {
                assert!(o.tabs.is_some());
                assert!((o.allowance - 0.3).abs() < 1e-9);
            }
            _ => unreachable!(),
        }
        // User overrides survive re-planning.
        let fid = p.features[0].id;
        p.features[0].plan.rough_tool = Some("generic.flat_1_8_2f".into());
        p.features[0].plan.finishing = false;
        replan(&mut p, lib(), &[fid]);
        let cut_ops: Vec<&OperationSpec> = p.operations.iter().filter(|o| o.feature_id == Some(fid)).collect();
        assert_eq!(cut_ops.len(), 1);
        assert_eq!(cut_ops[0].tool_id, "generic.flat_1_8_2f");
        assert!(matches!(&cut_ops[0].kind, OpKind::Profile(o) if o.allowance == 0.0));
        // A hand-tuned pass is no longer generated, but the rest of the plan stays.
        p.features[0].plan = PlanSettings { hand_finish: true, ..Default::default() };
        replan(&mut p, lib(), &[fid]);
        let cut_ops: Vec<&OperationSpec> = p.operations.iter().filter(|o| o.feature_id == Some(fid)).collect();
        assert_eq!(cut_ops.len(), 1);
        assert_eq!(cut_ops[0].pass, Operation::Roughing);
        assert!(matches!(&cut_ops[0].kind, OpKind::Profile(o) if (o.allowance - 0.3).abs() < 1e-9));
    }

    #[test]
    fn vcarve_wide_shape_gets_flat_floor_and_clearance() {
        let mut p = Project::default();
        let v = p.add_vector("plaque", rectangle(DVec2::new(20.0, 20.0), 80.0, 40.0, 0.0), VectorSource::Drawn);
        feature_from_vectors(&mut p, lib(), &[v], CutStrategy::VCarve, Some(3.0), None).unwrap();
        match &p.operations[0].kind {
            OpKind::VCarve { flat_depth, clearance_tool_id, .. } => {
                assert_eq!(*flat_depth, Some(3.0));
                assert!(clearance_tool_id.is_some());
            }
            k => panic!("{k:?}"),
        }
        // The plan computes: clearance pass is classified as roughing in previews.
        let res = crate::ops::compute_all(&p, lib());
        let o = &p.operations[0];
        let passes: Vec<Operation> = res[0].toolpaths.iter().map(|t| toolpath_pass(o, t)).collect();
        assert_eq!(passes, [Operation::Roughing, Operation::Finishing]);
    }

    #[test]
    fn conflicts_outside_deep_overlap() {
        let mut p = Project::default();
        let a = p.add_vector("a", rectangle(DVec2::new(10.0, 10.0), 40.0, 40.0, 0.0), VectorSource::Drawn);
        let b = p.add_vector("b", rectangle(DVec2::new(30.0, 30.0), 40.0, 40.0, 0.0), VectorSource::Drawn);
        let c = p.add_vector("c", circle(DVec2::new(295.0, 100.0), 10.0), VectorSource::Drawn);
        feature_from_vectors(&mut p, lib(), &[a], CutStrategy::Pocket, Some(3.0), None).unwrap();
        feature_from_vectors(&mut p, lib(), &[b], CutStrategy::Pocket, Some(30.0), None).unwrap();
        feature_from_vectors(&mut p, lib(), &[c], CutStrategy::Pocket, Some(2.0), None).unwrap();
        let cs = analyze(&p, lib());
        let msg = |s: &str| cs.iter().any(|c| c.message.contains(s));
        assert!(msg("past the stock edge"), "{cs:?}");
        assert!(msg("30.0 mm deep"), "{cs:?}");
        // 20×20 overlap.
        let ov = cs.iter().find(|c| c.message.contains("overlaps")).expect("overlap");
        assert!(ov.message.contains("400 mm²"), "{}", ov.message);
        assert_eq!(cs[0].severity, Severity::Error);
    }

    #[test]
    fn engrave_border_around_lettering_is_no_overlap() {
        let p = crate::demo::sign("OAK").unwrap();
        let cs = analyze(&p, lib());
        assert!(cs.iter().all(|c| !c.message.contains("overlaps")), "{cs:?}");
    }

    #[test]
    fn round_stock_outside_detection() {
        let mut p = Project { stock_shape: StockShape::Round, ..Default::default() };
        p.job.width = 200.0;
        p.job.height = 200.0;
        let v = p.add_vector("corner", rectangle(DVec2::new(5.0, 5.0), 20.0, 20.0, 0.0), VectorSource::Drawn);
        feature_from_vectors(&mut p, lib(), &[v], CutStrategy::Pocket, Some(2.0), None).unwrap();
        assert!(analyze(&p, lib()).iter().any(|c| c.message.contains("stock edge")));
    }

    #[test]
    fn reimport_keeps_placement_and_retargets_manual_ops() {
        let mut p = Project::default();
        let src = FeatureSource::ImageStudio { image: "logo.png".into(), mode: "outline".into(), settings: serde_json::Value::Null };
        let pay = Payload::Vectors { paths: vec![rectangle(DVec2::ZERO, 40.0, 20.0, 0.0)], strategy: Some(CutStrategy::Pocket), depth: Some(3.0) };
        let id = import_feature(&mut p, lib(), "Logo", pay, src).unwrap();
        let (b0, _) = feature_extent(&p, &p.features[0]).unwrap();
        assert!((b0.center() - DVec2::new(150.0, 100.0)).length() < 1e-9);
        // The user moves the feature and adds a manual engraving on its vectors.
        let ids = match &p.features[0].kind {
            FeatureKind::Cut2D { vector_ids, .. } => vector_ids.clone(),
            _ => unreachable!(),
        };
        crate::Command::MoveVectors { ids: ids.clone(), dx: -60.0, dy: 30.0 }.apply(&mut p).unwrap();
        let mid = p.alloc_id();
        p.operations.push(OperationSpec { id: mid, name: "My engrave".into(), feature_id: Some(id), feature_rev: 1, ..plain_op(ids) });
        // New revision: twice as wide, drawn somewhere else in the editor.
        let pay = Payload::Vectors { paths: vec![rectangle(DVec2::new(500.0, 500.0), 80.0, 20.0, 0.0)], strategy: None, depth: None };
        let rep = reimport(&mut p, lib(), id, pay, None).unwrap();
        let (b1, d1) = feature_extent(&p, &p.features[0]).unwrap();
        assert!((b1.center() - DVec2::new(90.0, 130.0)).length() < 1e-9, "{:?}", b1.center());
        assert!((b1.size().x - 80.0).abs() < 1e-9);
        assert!((d1 - 3.0).abs() < 1e-9, "depth kept");
        assert_eq!(rep.new_revision, 2);
        assert_eq!(rep.retargeted_ops, ["My engrave"]);
        assert_eq!(rep.regenerated_ops, 2);
        assert!((rep.old_size.x - 40.0).abs() < 1e-9 && (rep.new_size.x - 80.0).abs() < 1e-9);
        let manual = p.operations.iter().find(|o| o.id == mid).unwrap();
        assert!(manual.vector_ids.iter().all(|v| p.vector(*v).is_some()));
        assert!(rep.is_clean(), "{}", rep.summary());
    }

    #[test]
    fn reimport_reports_new_conflict() {
        let mut p = Project::default();
        let src = FeatureSource::SolidModeler { part_id: 1, part_revision: 1, as_relief: true };
        let mut hm = Heightmap::new(50, 50, 1.0, DVec2::ZERO, -5.0);
        hm.set(25, 25, 0.0);
        let id = import_feature(&mut p, lib(), "Part", Payload::Relief { heightmap: hm }, src).unwrap();
        assert!(analyze(&p, lib()).iter().all(|c| c.severity != Severity::Error));
        // New revision is deeper than the stock.
        let mut hm2 = Heightmap::new(50, 50, 1.0, DVec2::ZERO, -25.0);
        hm2.set(25, 25, 0.0);
        let rep = reimport(&mut p, lib(), id, Payload::Relief { heightmap: hm2 }, None).unwrap();
        assert!(!rep.is_clean());
        assert!(rep.new_conflicts.iter().any(|c| c.message.contains("deep")), "{}", rep.summary());
    }

    #[test]
    fn model_feature_plans_rough_and_ball_finish() {
        let mut p = Project::default();
        let mesh = TriMesh::cube(glam::Vec3::new(0.0, 0.0, 0.0), glam::Vec3::new(120.0, 90.0, 8.0));
        let id = import_feature(&mut p, lib(), "Box", Payload::Mesh { mesh }, FeatureSource::File { path: "box.stl".into() }).unwrap();
        let ops: Vec<&OperationSpec> = p.operations.iter().filter(|o| o.feature_id == Some(id)).collect();
        assert_eq!(ops.len(), 2);
        assert!(matches!(lib().get(&ops[1].tool_id).unwrap().shape, ToolShape::BallNose));
        let (b, d) = feature_extent(&p, &p.features[0]).unwrap();
        assert!((d - 8.0).abs() < 1e-3);
        assert!((b.center() - DVec2::new(150.0, 100.0)).length() < 1e-3);
    }

    #[test]
    fn suggest_strategies() {
        let j = ezyx_core::JobSetup::default();
        assert_eq!(suggest_strategy(&[rectangle(DVec2::ZERO, 250.0, 180.0, 0.0)], &j), CutStrategy::CutOut);
        assert_eq!(suggest_strategy(&[Polyline::open(vec![DVec2::ZERO, DVec2::X * 10.0])], &j), CutStrategy::Engrave);
        let letters: Vec<Polyline> = (0..5).map(|i| rectangle(DVec2::new(i as f64 * 12.0, 0.0), 4.0, 15.0, 0.0)).collect();
        assert_eq!(suggest_strategy(&letters, &j), CutStrategy::VCarve);
    }

    fn plain_op(ids: Vec<Id>) -> OperationSpec {
        OperationSpec {
            id: 0,
            name: String::new(),
            enabled: true,
            kind: OpKind::Engrave,
            tool_id: "generic.v90".into(),
            pass: Operation::Finishing,
            start_depth: 0.0,
            cut_depth: 0.5,
            vector_ids: ids,
            direction: Default::default(),
            cutting_override: None,
            feature_id: None,
            auto: false,
            feature_rev: 0,
        }
    }
}
