//! Application state, layout and action dispatch.

use crate::panels;
use crate::ribbon::{self, Action, DrawTool, OpTemplate, Prim, Tab};
use crate::tasks::{TaskResult, Tasks};
use crate::theme;
use crate::viewport::{self, Camera, Draw, Projector, ViewPreset};
use egui::{Color32, Pos2, RichText, Sense, Stroke};
use ezyx_cad::{BooleanOp, Feature, Part, Shape};
use ezyx_cam::ops::{
    pocket::PocketOptions,
    profile::{ProfileOptions, ProfileSide},
    raster3d::Raster3dOptions,
};
use ezyx_core::{DVec2, JobKind};
use ezyx_mesh::TriMesh;
use ezyx_project::{
    commands::Command,
    ops::{OpResult, ReliefSource},
    History, OpKind, OperationSpec, PartObject, Project, ReliefObject,
};
use ezyx_tooldb::{Operation, ToolLibrary};
use glam::Vec3;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Default)]
pub struct DrawState {
    pub start: Option<Vec3>,
    pub pts: Vec<Vec3>,
    pub box_start: Option<Pos2>,
}

pub struct Dialogs {
    pub about: bool,
    pub text_at: Option<Vec3>,
    pub text: String,
    pub text_height: f64,
    pub font_path: String,
    pub offset: f64,
    pub tet_report: Option<String>,
}

pub struct EzApp {
    pub project: Project,
    pub history: History,
    pub library: ToolLibrary,
    pub path: Option<PathBuf>,
    pub tab: Tab,
    pub tool: DrawTool,
    pub selection: Vec<u64>,
    pub selected_op: Option<u64>,
    pub selected_part: Option<u64>,
    pub selected_mesh: Option<u64>,
    pub cam: Camera,
    pub mode2d: bool,
    pub dark: bool,
    pub show_ai: bool,
    pub status: String,
    pub rev: u64,
    pub results: Vec<OpResult>,
    pub results_rev: u64,
    pub preview: Option<TriMesh>,
    pub preview_info: String,
    pub display: HashMap<String, (u64, TriMesh)>,
    pub part_building: HashSet<u64>,
    pub tasks: Tasks,
    pub gpu: Option<ezyx_gpu::GpuInfo>,
    pub perf: ezyx_gpu::PerfDefaults,
    pub draw: DrawState,
    pub dialogs: Dialogs,
    pub image: panels::image::ImageState,
    pub machine: panels::machine::MachineState,
    pub ai: panels::ai::AiState,
    pub show_toolpaths: bool,
    pub show_rapids: bool,
    pub tool_filter: String,
    pub cursor_world: Option<Vec3>,
    /// Set by `EZYX_START_PREVIEW=1` (screenshots / smoke tests).
    pub pending_preview: bool,
}

impl EzApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx, false);
        let gpu = ezyx_gpu::best();
        let perf = ezyx_gpu::defaults_for(gpu.as_ref().map(|g| g.tier).unwrap_or(ezyx_gpu::Tier::Capable));
        let mut app = Self {
            project: Project::default(),
            history: History::default(),
            library: ToolLibrary::bundled(),
            path: None,
            tab: Tab::Home,
            tool: DrawTool::Select,
            selection: vec![],
            selected_op: None,
            selected_part: None,
            selected_mesh: None,
            cam: Camera::default(),
            mode2d: true,
            dark: false,
            show_ai: false,
            status: "Welcome! Set up your stock on the right, then draw or import a design.".into(),
            rev: 1,
            results: vec![],
            results_rev: 0,
            preview: None,
            preview_info: String::new(),
            display: HashMap::new(),
            part_building: HashSet::new(),
            tasks: Tasks::default(),
            gpu,
            perf,
            draw: DrawState::default(),
            dialogs: Dialogs { about: false, text_at: None, text: "Hello".into(), text_height: 30.0, font_path: String::new(), offset: 3.0, tet_report: None },
            image: Default::default(),
            machine: Default::default(),
            ai: panels::ai::AiState::load(),
            show_toolpaths: true,
            show_rapids: true,
            tool_filter: String::new(),
            cursor_world: None,
            pending_preview: std::env::var("EZYX_START_PREVIEW").is_ok(),
        };
        // Open a project passed on the command line (also used by OS file associations).
        if let Some(path) = std::env::args().nth(1).map(PathBuf::from) {
            match Project::load(&path) {
                Ok(p) => {
                    app.project = p;
                    app.path = Some(path);
                    app.calculate(None);
                }
                Err(e) => app.status = format!("⚠ could not open {}: {e}", path.display()),
            }
        }
        app.fit_view();
        app
    }

    // ------------------------------------------------------------------ helpers

    pub fn exec(&mut self, c: Command) -> bool {
        match self.history.execute(&mut self.project, &c) {
            Ok(m) => {
                self.status = m;
                self.rev += 1;
                true
            }
            Err(e) => {
                self.status = format!("⚠ {e}");
                false
            }
        }
    }

    pub fn touch(&mut self) {
        self.rev += 1;
    }

    pub fn fit_view(&mut self) {
        let j = &self.project.job;
        let (lo, hi) = match j.kind {
            JobKind::Rotary { diameter, length, .. } if !self.mode2d => {
                let r = diameter as f32 * 0.5;
                (Vec3::new(0.0, -r, -r), Vec3::new(length as f32, r, r))
            }
            _ => (Vec3::new(0.0, 0.0, -j.thickness as f32), Vec3::new(j.width as f32, j.height as f32, 0.0)),
        };
        self.cam.fit(lo, hi);
    }

    pub fn set_mode2d(&mut self, on: bool) {
        self.mode2d = on;
        if on {
            self.cam.ortho = true;
            self.cam.preset(ViewPreset::Top);
        } else {
            self.cam.ortho = false;
            self.cam.preset(ViewPreset::Iso);
        }
        self.fit_view();
    }

    fn file_dialog() -> rfd::FileDialog {
        rfd::FileDialog::new()
    }

    pub fn save(&mut self, as_new: bool) {
        let path = if as_new || self.path.is_none() {
            Self::file_dialog().add_filter("EZ-YX-craft project", &["ezyx"]).set_file_name(format!("{}.ezyx", self.project.name)).save_file()
        } else {
            self.path.clone()
        };
        if let Some(p) = path {
            match self.project.save(&p) {
                Ok(()) => {
                    self.status = format!("Saved {}", p.display());
                    self.path = Some(p);
                }
                Err(e) => self.status = format!("⚠ save failed: {e}"),
            }
        }
    }

    pub fn selected_or_all_closed(&self) -> Vec<u64> {
        if self.selection.is_empty() {
            self.project.vectors.iter().filter(|v| v.path.closed).map(|v| v.id).collect()
        } else {
            self.selection.clone()
        }
    }

    fn default_tool_for(&self, t: OpTemplate) -> &'static str {
        match t {
            OpTemplate::VCarve => "generic.v60",
            OpTemplate::Engrave => "generic.v90",
            OpTemplate::Drill => "generic.drill_3",
            OpTemplate::Finish3d => "generic.ball_1_8",
            OpTemplate::Rough3d => "generic.flat_1_4_2f",
            OpTemplate::Laser => "generic.laser_5w",
            _ => "generic.flat_1_8_2f",
        }
    }

    pub fn add_op(&mut self, t: OpTemplate) {
        let src = if let Some(id) = self.selected_part {
            ReliefSource::Part(id)
        } else if let Some(id) = self.selected_mesh {
            ReliefSource::Mesh(id)
        } else {
            ReliefSource::AllReliefs
        };
        let th = self.project.job.thickness;
        let (kind, depth, pass, name) = match t {
            OpTemplate::ProfileOutside => (
                OpKind::Profile(ProfileOptions { side: ProfileSide::Outside, tabs: Some(Default::default()), ..Default::default() }),
                th,
                Operation::Roughing,
                "Cut out",
            ),
            OpTemplate::ProfileInside => {
                (OpKind::Profile(ProfileOptions { side: ProfileSide::Inside, ..Default::default() }), 3.0, Operation::Roughing, "Inside profile")
            }
            OpTemplate::ProfileOn => (OpKind::Profile(ProfileOptions { side: ProfileSide::On, ..Default::default() }), 1.0, Operation::Roughing, "On-line"),
            OpTemplate::Pocket => (OpKind::Pocket(PocketOptions::default()), 3.0, Operation::Roughing, "Pocket"),
            OpTemplate::VCarve => (OpKind::VCarve { flat_depth: None, step: 0.2, clearance_tool_id: None }, 10.0, Operation::Finishing, "V-carve"),
            OpTemplate::Drill => (OpKind::Drill(Default::default()), th, Operation::Roughing, "Drill"),
            OpTemplate::Engrave => (OpKind::Engrave, 1.0, Operation::Finishing, "Engrave"),
            OpTemplate::Finish3d => (
                OpKind::Raster3d { source: src, options: Raster3dOptions::default(), resolution: self.perf.sim_cell_mm.max(0.1) },
                th,
                Operation::Finishing,
                "3D finish",
            ),
            OpTemplate::Rough3d => (
                OpKind::Raster3d {
                    source: src,
                    options: Raster3dOptions { roughing: true, allowance: 0.3, ..Default::default() },
                    resolution: (self.perf.sim_cell_mm * 2.0).max(0.25),
                },
                th,
                Operation::Roughing,
                "3D rough",
            ),
            OpTemplate::Laser => (OpKind::Laser { power: 0.6, passes: 1, focus_z: 0.0 }, 0.1, Operation::Finishing, "Laser"),
        };
        let ids = match t {
            OpTemplate::Finish3d | OpTemplate::Rough3d => vec![],
            OpTemplate::ProfileOn | OpTemplate::Engrave | OpTemplate::Laser => self.selection.clone(),
            _ => self.selected_or_all_closed(),
        };
        let spec = OperationSpec {
            id: 0,
            name: name.into(),
            enabled: true,
            kind,
            tool_id: self.default_tool_for(t).into(),
            pass,
            start_depth: 0.0,
            cut_depth: depth,
            vector_ids: ids,
            direction: Default::default(),
            cutting_override: None,
        };
        if self.exec(Command::AddOperation { spec }) {
            self.selected_op = self.project.operations.last().map(|o| o.id);
            self.tab = Tab::Toolpaths;
            self.calculate(None);
        }
    }

    pub fn calculate(&mut self, ctx: Option<&egui::Context>) {
        let p = self.project.clone();
        let lib = self.library.clone();
        self.results_rev = self.rev;
        match ctx {
            Some(ctx) => self.tasks.spawn(ctx, move || TaskResult::Toolpaths(ezyx_project::ops::compute_all(&p, &lib))),
            None => {
                self.results = ezyx_project::ops::compute_all(&p, &lib);
                self.status = format!("Calculated {} operation(s)", self.results.len());
            }
        }
    }

    pub fn export_gcode(&mut self) {
        match ezyx_project::ops::export_gcode(&self.project, &self.library) {
            Ok((g, warnings)) => {
                let ext = ezyx_gcode::post::find_post(&self.project.post_id).map(|p| p.extension).unwrap_or_else(|| "nc".into());
                if let Some(path) = Self::file_dialog()
                    .add_filter("G-code", &[ext.as_str(), "nc", "gcode", "tap", "ngc"])
                    .set_file_name(format!("{}.{ext}", self.project.name))
                    .save_file()
                {
                    match std::fs::write(&path, g) {
                        Ok(()) => {
                            self.status = format!(
                                "G-code saved to {}{}",
                                path.display(),
                                if warnings.is_empty() { String::new() } else { format!(" — ⚠ {}", warnings.join("; ")) }
                            )
                        }
                        Err(e) => self.status = format!("⚠ {e}"),
                    }
                }
            }
            Err(e) => self.status = format!("⚠ {e}"),
        }
    }

    pub fn start_preview(&mut self, ctx: &egui::Context) {
        let p = self.project.clone();
        let lib = self.library.clone();
        let cell = self.perf.sim_cell_mm.max(0.08);
        let lod = self.perf.viewport_lod;
        self.status = format!("Simulating at {cell:.2} mm resolution…");
        self.tasks.spawn(ctx, move || {
            let t0 = std::time::Instant::now();
            let results = ezyx_project::ops::compute_all(&p, &lib);
            let rotary = match p.job.kind {
                JobKind::Rotary { diameter, .. } => Some(diameter),
                _ => None,
            };
            let mut sim = ezyx_sim::StockSim::new(&p.job, cell);
            for r in &results {
                for tp in &r.toolpaths {
                    let flat = if let Some(d) = rotary { unwrap_rotary(tp, d) } else { tp.clone() };
                    sim.apply(&flat);
                }
            }
            let mat = ezyx_materials::find(&p.job.material).unwrap_or_else(|| ezyx_materials::library()[0].clone());
            let lod = lod.max((sim.hm.width.max(sim.hm.height) / 900).max(1));
            let mut mesh = sim.render_mesh(&mat, lod);
            if let Some(d) = rotary {
                ezyx_sim::wrap_mesh_cylinder(&mut mesh, d as f32 * 0.5);
            }
            TaskResult::Simulation { mesh, removed_cm3: sim.removed_volume() / 1000.0, warnings: sim.warnings.clone(), seconds: t0.elapsed().as_secs_f64() }
        });
    }

    fn poll_tasks(&mut self) {
        while let Ok(r) = self.tasks.rx.try_recv() {
            self.tasks.running = self.tasks.running.saturating_sub(1);
            match r {
                TaskResult::Toolpaths(res) => {
                    let errs: Vec<String> = res.iter().filter_map(|r| r.error.clone()).collect();
                    let secs: f64 = res.iter().flat_map(|r| r.toolpaths.iter()).map(|t| t.stats(self.project.machine.limits.max_feed).est_seconds).sum();
                    self.status =
                        if errs.is_empty() { format!("Toolpaths ready — estimated {:.1} min", secs / 60.0) } else { format!("⚠ {}", errs.join("; ")) };
                    self.results = res;
                }
                TaskResult::Simulation { mesh, removed_cm3, warnings, seconds } => {
                    self.preview_info = format!("Removed {removed_cm3:.1} cm³ · simulated in {seconds:.1} s · {} triangles", mesh.indices.len());
                    if !warnings.is_empty() {
                        self.preview_info += &format!("\n⚠ {}", warnings.join("\n⚠ "));
                    }
                    self.preview = Some(mesh);
                    self.status = "Preview ready — orbit with the left mouse button.".into();
                    if self.mode2d {
                        self.set_mode2d(false);
                    }
                }
                TaskResult::PartMesh { id, mesh } => {
                    self.part_building.remove(&id);
                    self.display.insert(format!("part{id}"), (self.rev, mesh));
                }
                TaskResult::Error(e) => self.status = format!("⚠ {e}"),
            }
        }
    }

    // ------------------------------------------------------------------ actions

    pub fn handle(&mut self, a: Action, ctx: &egui::Context) {
        let center = DVec2::new(self.project.job.width, self.project.job.height) * 0.5;
        match a {
            Action::New => {
                self.project = Project::default();
                self.history = History::default();
                self.path = None;
                self.selection.clear();
                self.selected_op = None;
                self.results.clear();
                self.preview = None;
                self.touch();
                self.fit_view();
                self.status = "New project — set your stock size on the right.".into();
            }
            Action::Open => {
                if let Some(p) = Self::file_dialog().add_filter("EZ-YX-craft project", &["ezyx"]).pick_file() {
                    match Project::load(&p) {
                        Ok(pr) => {
                            self.project = pr;
                            self.history = History::default();
                            self.path = Some(p);
                            self.selection.clear();
                            self.results.clear();
                            self.preview = None;
                            self.touch();
                            self.fit_view();
                            self.status = "Project opened".into();
                        }
                        Err(e) => self.status = format!("⚠ {e}"),
                    }
                }
            }
            Action::Save => self.save(false),
            Action::SaveAs => self.save(true),
            Action::ImportVectors => {
                if let Some(p) = Self::file_dialog().add_filter("Vectors", &["svg", "dxf"]).pick_file() {
                    self.exec(Command::ImportVectors { path: p.display().to_string() });
                }
            }
            Action::ImportMesh => {
                if let Some(p) = Self::file_dialog().add_filter("3D model", &["stl", "obj"]).pick_file() {
                    if self.exec(Command::ImportMesh { path: p.display().to_string(), fit_to_stock: true, depth: self.project.job.thickness * 0.6 }) {
                        self.selected_mesh = self.project.meshes.last().map(|m| m.id);
                        self.set_mode2d(false);
                    }
                }
            }
            Action::Undo => {
                if self.history.undo(&mut self.project) {
                    self.touch();
                    self.status = "Undo".into();
                }
            }
            Action::Redo => {
                if self.history.redo(&mut self.project) {
                    self.touch();
                    self.status = "Redo".into();
                }
            }
            Action::Tool(t) => {
                self.tool = t;
                self.draw = DrawState::default();
                if t != DrawTool::Select && !self.mode2d {
                    self.set_mode2d(true);
                }
                self.tab = Tab::Design2D;
            }
            Action::AddStar => {
                self.exec(Command::AddStar { cx: center.x, cy: center.y, r_outer: 40.0, r_inner: 18.0, points: 5 });
            }
            Action::AddPolygon => {
                self.exec(Command::AddPolygon { cx: center.x, cy: center.y, r: 35.0, sides: 6 });
            }
            Action::Offset => {
                let ids = self.selection.clone();
                self.exec(Command::OffsetVectors { ids, distance: self.dialogs.offset });
            }
            Action::Union | Action::Subtract | Action::Intersect => {
                let op = match a {
                    Action::Union => ezyx_project::commands::BoolOp::Union,
                    Action::Subtract => ezyx_project::commands::BoolOp::Subtract,
                    _ => ezyx_project::commands::BoolOp::Intersect,
                };
                let ids = self.selection.clone();
                if self.exec(Command::Boolean { op, ids }) {
                    self.selection.clear();
                }
            }
            Action::Center => {
                let ids = self.selection.clone();
                self.exec(Command::CenterOnStock { ids });
            }
            Action::Delete => {
                let ids = std::mem::take(&mut self.selection);
                self.exec(Command::DeleteVectors { ids });
            }
            Action::ReliefFromImage | Action::ImageRelief => {
                if self.image.ensure_loaded(ctx) {
                    let mut o = self.image.relief.clone();
                    o.origin = center - DVec2::new(o.width_mm, o.width_mm * self.image.aspect()) * 0.5;
                    let path = self.image.path.clone().unwrap_or_default();
                    if self.exec(Command::ImageRelief { path, options: o }) {
                        self.set_mode2d(false);
                    }
                }
            }
            Action::DomeFromVectors => self.dome_from_selection(),
            Action::DemoPart => {
                if self.exec(Command::AddDemoPart) {
                    self.selected_part = self.project.parts.last().map(|p| p.id);
                    self.tab = Tab::Model3D;
                    self.set_mode2d(false);
                }
            }
            Action::NewPart => {
                let part =
                    Part::new("Part").add(Feature::new("Base", BooleanOp::Add, Shape::Box { size: [60.0, 40.0, 10.0], corner_radius: 2.0 }).at(0.0, 0.0, 5.0));
                if self.exec(Command::AddPart { part }) {
                    self.selected_part = self.project.parts.last().map(|p| p.id);
                    self.set_mode2d(false);
                }
            }
            Action::Primitive(prim) => {
                let shape = match prim {
                    Prim::Box => Shape::Box { size: [30.0, 30.0, 15.0], corner_radius: 1.0 },
                    Prim::Cylinder => Shape::Cylinder { radius: 10.0, height: 20.0 },
                    Prim::Sphere => Shape::Sphere { radius: 12.0 },
                    Prim::Cone => Shape::Cone { r_bottom: 12.0, r_top: 4.0, height: 20.0 },
                    Prim::Torus => Shape::Torus { major: 15.0, minor: 4.0 },
                };
                self.add_feature(Feature::new(format!("{prim:?}"), BooleanOp::Add, shape));
            }
            Action::Extrude | Action::Revolve => {
                let sketch: Vec<_> = self.project.paths_for(&self.selection).into_iter().filter(|p| p.closed).collect();
                if sketch.is_empty() {
                    self.status = "Select closed vectors first (2D Design tab).".into();
                } else if a == Action::Extrude {
                    // Centre the sketch on the part origin.
                    let b = sketch.iter().fold(ezyx_core::Bounds2::EMPTY, |acc, l| acc.union(&l.bounds()));
                    let t = ezyx_core::DAffine2::from_translation(-b.center());
                    let sketch = sketch.iter().map(|l| l.transform(&t)).collect();
                    self.add_feature(Feature::new("Extrude", BooleanOp::Add, Shape::Extrude { sketch, height: 10.0, draft_deg: 0.0 }));
                } else {
                    let b = sketch.iter().fold(ezyx_core::Bounds2::EMPTY, |acc, l| acc.union(&l.bounds()));
                    let t = ezyx_core::DAffine2::from_translation(-DVec2::new(b.min.x, b.min.y));
                    let profile = sketch.iter().map(|l| l.transform(&t)).collect();
                    self.add_feature(Feature::new("Revolve", BooleanOp::Add, Shape::Revolve { profile, angle_deg: 360.0 }));
                }
            }
            Action::PartToRelief => {
                if self.selected_part.is_some() {
                    self.add_op(OpTemplate::Rough3d);
                    self.add_op(OpTemplate::Finish3d);
                } else {
                    self.status = "Select a part in the project tree first.".into();
                }
            }
            Action::ExportPartStl => {
                if let Some(po) = self.selected_part.and_then(|id| self.project.parts.iter().find(|p| p.id == id)) {
                    let mesh = po.part.mesh(0.4);
                    if let Some(path) = Self::file_dialog().add_filter("STL", &["stl"]).set_file_name("part.stl").save_file() {
                        self.status = match ezyx_mesh::io::write_stl(&mesh, &path) {
                            Ok(()) => format!("Exported {}", path.display()),
                            Err(e) => format!("⚠ {e}"),
                        };
                    }
                }
            }
            Action::PartTetMesh => {
                if let Some(po) = self.selected_part.and_then(|id| self.project.parts.iter().find(|p| p.id == id)) {
                    let tm = po.part.tet_mesh(1.0);
                    self.save_tet(tm);
                }
            }
            Action::MeshFit => {
                if let Some(id) = self.selected_mesh {
                    let depth = self.project.job.thickness * 0.6;
                    let job = self.project.clone();
                    if let Some(m) = self.project.meshes.iter_mut().find(|m| m.id == id) {
                        ezyx_project::commands::place_mesh(&mut m.mesh, &job, true, depth);
                        self.touch();
                    }
                }
            }
            Action::MeshSmooth => {
                if let Some(m) = self.selected_mesh.and_then(|id| self.project.meshes.iter_mut().find(|m| m.id == id)) {
                    m.mesh.smooth_taubin(3, 0.5, -0.53);
                    self.touch();
                }
            }
            Action::ExportMeshStl => {
                if let Some(m) = self.selected_mesh.and_then(|id| self.project.meshes.iter().find(|m| m.id == id)) {
                    if let Some(path) = Self::file_dialog().add_filter("STL", &["stl"]).set_file_name("mesh.stl").save_file() {
                        self.status = match ezyx_mesh::io::write_stl(&m.mesh, &path) {
                            Ok(()) => format!("Exported {}", path.display()),
                            Err(e) => format!("⚠ {e}"),
                        };
                    }
                }
            }
            Action::MeshToRelief => {
                if let Some(id) = self.selected_mesh {
                    match ezyx_project::ops::relief_heightmap(&self.project, &ReliefSource::Mesh(id), 0.3) {
                        Ok(hm) => {
                            let rid = self.project.alloc_id();
                            self.history.checkpoint(&self.project, "mesh → relief");
                            self.project.reliefs.push(ReliefObject { id: rid, name: format!("Relief of mesh #{id}"), heightmap: hm, visible: true });
                            self.touch();
                        }
                        Err(e) => self.status = format!("⚠ {e}"),
                    }
                } else {
                    self.status = "Select a mesh in the project tree.".into();
                }
            }
            Action::MeshTet => {
                if let Some(r) = self.project.reliefs.last() {
                    let tm = ezyx_mesh::tet::relief_solid(&r.heightmap, -(self.project.job.thickness as f32), 6);
                    self.save_tet(tm);
                } else {
                    self.status = "Create a relief first (2.5D tab or Mesh → To Relief).".into();
                }
            }
            Action::LoadImage => {
                self.tab = Tab::Image;
                self.image.pick(ctx);
            }
            Action::TraceOutline | Action::TraceCenterline => {
                if self.image.ensure_loaded(ctx) {
                    let mut o = self.image.trace.clone();
                    o.origin = center - DVec2::new(o.width_mm, o.width_mm * self.image.aspect()) * 0.5;
                    let path = self.image.path.clone().unwrap_or_default();
                    self.exec(Command::TraceImage { path, options: o, centerline: a == Action::TraceCenterline });
                }
            }
            Action::PhotoVCarve => {
                if self.image.ensure_loaded(ctx) {
                    let mut o = self.image.photo.clone();
                    o.origin = center - DVec2::new(o.width_mm, o.width_mm * self.image.aspect()) * 0.5;
                    let tool = if o.bit_angle <= 45.0 {
                        "generic.v30"
                    } else if o.bit_angle <= 75.0 {
                        "generic.v60"
                    } else {
                        "generic.v90"
                    };
                    o.bit_angle = match tool {
                        "generic.v30" => 30.0,
                        "generic.v60" => 60.0,
                        _ => 90.0,
                    };
                    let path = self.image.path.clone().unwrap_or_default();
                    if self.exec(Command::PhotoVCarve { path, options: o, tool_id: tool.into() }) {
                        self.selected_op = self.project.operations.last().map(|o| o.id);
                        self.calculate(Some(ctx));
                    }
                }
            }
            Action::ImageTo3d => {
                if self.image.ensure_loaded(ctx) {
                    let path = self.image.path.clone().unwrap_or_default();
                    if self.exec(Command::ImageTo3d { path, width_mm: self.image.relief.width_mm, thickness: self.image.thickness_3d }) {
                        self.selected_mesh = self.project.meshes.last().map(|m| m.id);
                        self.set_mode2d(false);
                    }
                }
            }
            Action::AddOp(t) => self.add_op(t),
            Action::Calculate => self.calculate(Some(ctx)),
            Action::ExportGcode => self.export_gcode(),
            Action::Preview => self.start_preview(ctx),
            Action::ClearPreview => self.preview = None,
            Action::View(v) => {
                if self.mode2d && v != ViewPreset::Top {
                    self.mode2d = false;
                    self.cam.ortho = false;
                }
                self.cam.preset(v);
            }
            Action::Toggle2D => {
                let m = !self.mode2d;
                self.set_mode2d(m);
            }
            Action::Fit => self.fit_view(),
            Action::ToggleAi => self.show_ai = !self.show_ai,
            Action::ToggleDark => {
                self.dark = !self.dark;
                theme::apply(ctx, self.dark);
            }
            Action::About => self.dialogs.about = true,
        }
    }

    fn save_tet(&mut self, tm: ezyx_mesh::TetMesh) {
        let q = tm.quality();
        let report = format!(
            "{} tetrahedra · volume {:.1} mm³\nDihedral angles {:.1}° – {:.1}°\nRadius ratio min {:.2}, mean {:.2}",
            q.count, q.volume, q.min_dihedral, q.max_dihedral, q.min_radius_ratio, q.mean_radius_ratio
        );
        if let Some(path) = Self::file_dialog().add_filter("VTK", &["vtk"]).set_file_name("mesh.vtk").save_file() {
            let _ = std::fs::write(&path, tm.to_vtk());
            let _ = std::fs::write(path.with_extension("msh"), tm.to_gmsh());
            self.dialogs.tet_report = Some(format!("{report}\n\nSaved {} and .msh", path.display()));
        } else {
            self.dialogs.tet_report = Some(report);
        }
    }

    fn add_feature(&mut self, f: Feature) {
        if self.selected_part.is_none() {
            let part = Part::new("Part");
            self.exec(Command::AddPart { part });
            self.selected_part = self.project.parts.last().map(|p| p.id);
        }
        let id = self.selected_part.unwrap();
        self.history.checkpoint(&self.project, "add feature");
        if let Some(po) = self.project.parts.iter_mut().find(|p| p.id == id) {
            po.part.features.push(f);
        }
        self.touch();
        self.tab = Tab::Model3D;
        if self.mode2d {
            self.set_mode2d(false);
        }
    }

    fn dome_from_selection(&mut self) {
        let region = self.project.region_for(&self.selected_or_all_closed());
        if region.is_empty() {
            self.status = "Select closed vectors to inflate.".into();
            return;
        }
        let b = region.bounds();
        let cell = 0.4;
        let pad = 2.0;
        let w = ((b.size().x + 2.0 * pad) / cell).ceil() as usize;
        let h = ((b.size().y + 2.0 * pad) / cell).ceil() as usize;
        let origin = b.min - DVec2::splat(pad);
        let mut bm = ezyx_image::Bitmap::new(w, h);
        let mut gray = ezyx_image::GrayImage::new(w, h, 0.5);
        for y in 0..h {
            for x in 0..w {
                // Image rows go top→down; world Y goes up.
                let p = origin + DVec2::new((x as f64 + 0.5) * cell, ((h - 1 - y) as f64 + 0.5) * cell);
                if region.contains(p) {
                    bm.set(x, y, true);
                    gray.data[y * w + x] = 0.5;
                }
            }
        }
        let thickness = (self.project.job.thickness * 0.4).min(12.0);
        let opt =
            ezyx_image::inflate::InflateOptions { width_mm: w as f64 * cell, thickness, roundness: 0.8, detail_depth: 0.0, symmetric: false, resolution: cell };
        let inf = ezyx_image::inflate::inflate(&gray, &bm, &opt);
        let mut hm = inf.front;
        hm.origin = origin;
        // Relief convention: top of dome at Z = 0, background at the dome height below.
        let top = hm.min_max().1;
        hm.map(|v| v - top);
        let id = self.project.alloc_id();
        self.history.checkpoint(&self.project, "dome");
        self.project.reliefs.push(ReliefObject { id, name: "Dome".into(), heightmap: hm, visible: true });
        self.touch();
        self.status = format!("Dome relief #{id} created ({thickness:.1} mm high)");
        self.set_mode2d(false);
    }

    fn ensure_part_meshes(&mut self, ctx: &egui::Context) {
        let parts: Vec<PartObject> = self.project.parts.iter().filter(|p| p.visible).cloned().collect();
        for po in parts {
            let key = format!("part{}", po.id);
            let fresh = self.display.get(&key).map(|(r, _)| *r == self.rev).unwrap_or(false);
            if !fresh && !self.part_building.contains(&po.id) {
                self.part_building.insert(po.id);
                let id = po.id;
                let res = if self.perf.gpu_compute { 0.6 } else { 1.0 };
                self.tasks.spawn(ctx, move || TaskResult::PartMesh { id, mesh: po.part.mesh(res) });
            }
        }
    }

    fn relief_mesh(&mut self, r: &ReliefObject) -> TriMesh {
        let key = format!("relief{}", r.id);
        if let Some((rev, m)) = self.display.get(&key) {
            if *rev == self.rev {
                return m.clone();
            }
        }
        let lod = (r.heightmap.width.max(r.heightmap.height) / 400).max(1);
        let m = ezyx_mesh::hm::surface(&r.heightmap, lod);
        self.display.insert(key, (self.rev, m.clone()));
        m
    }

    // ------------------------------------------------------------------ viewport

    fn viewport(&mut self, ui: &mut egui::Ui) {
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = resp.rect;
        painter.rect_filled(rect, 0.0, theme::viewport_bg(self.dark));
        let drawing = self.tool != DrawTool::Select;
        let lmb_reserved = self.mode2d || drawing;
        viewport::navigate(ui, &resp, &mut self.cam, rect, lmb_reserved);
        let pr = Projector::new(&self.cam, rect);
        let d = Draw { painter: &painter, pr: &pr };
        let job = self.project.job.clone();
        let rotary = match job.kind {
            JobKind::Rotary { diameter, length, .. } => Some((diameter as f32, length as f32)),
            _ => None,
        };

        // Stock.
        let (w, h, t) = (job.width as f32, job.height as f32, job.thickness as f32);
        let stock_col = if self.dark { Color32::from_gray(150) } else { Color32::from_rgb(120, 100, 80) };
        if let (Some((dia, len)), false) = (rotary, self.mode2d) {
            let r = dia * 0.5;
            let n = 48;
            for k in 0..n {
                let a0 = std::f32::consts::TAU * k as f32 / n as f32;
                let a1 = std::f32::consts::TAU * (k + 1) as f32 / n as f32;
                for x in [0.0, len] {
                    d.line(Vec3::new(x, r * a0.sin(), r * a0.cos()), Vec3::new(x, r * a1.sin(), r * a1.cos()), Stroke::new(1.0, stock_col));
                }
                if k % 6 == 0 {
                    d.line(Vec3::new(0.0, r * a0.sin(), r * a0.cos()), Vec3::new(len, r * a0.sin(), r * a0.cos()), Stroke::new(0.5, stock_col));
                }
            }
            d.line(Vec3::new(-10.0, 0.0, 0.0), Vec3::new(len + 10.0, 0.0, 0.0), Stroke::new(1.0, theme::ACCENT));
        } else {
            if self.preview.is_none() {
                // Stock top surface.
                let corners = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(w, 0.0, 0.0), Vec3::new(w, h, 0.0), Vec3::new(0.0, h, 0.0)];
                let pts: Vec<Pos2> = corners.iter().filter_map(|c| pr.to_screen(*c)).collect();
                if pts.len() == 4 {
                    let fill = if self.dark {
                        Color32::from_rgba_unmultiplied(90, 80, 70, 120)
                    } else {
                        Color32::from_rgba_unmultiplied(theme::MAPLE.r(), theme::MAPLE.g(), theme::MAPLE.b(), 235)
                    };
                    painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::NONE));
                }
                // Grid.
                let step = if self.cam.dist > 1500.0 { 50.0 } else { 10.0 };
                let gcol = if self.dark { Color32::from_gray(70) } else { Color32::from_rgba_unmultiplied(0, 0, 0, 22) };
                let mut x = 0.0;
                while x <= w + 0.01 {
                    d.line(Vec3::new(x, 0.0, 0.0), Vec3::new(x, h, 0.0), Stroke::new(if (x as i32) % 50 == 0 { 1.0 } else { 0.5 }, gcol));
                    x += step;
                }
                let mut y = 0.0;
                while y <= h + 0.01 {
                    d.line(Vec3::new(0.0, y, 0.0), Vec3::new(w, y, 0.0), Stroke::new(if (y as i32) % 50 == 0 { 1.0 } else { 0.5 }, gcol));
                    y += step;
                }
            }
            let s = Stroke::new(1.2, stock_col);
            for z in [0.0, -t] {
                d.polyline(&[Vec3::new(0.0, 0.0, z), Vec3::new(w, 0.0, z), Vec3::new(w, h, z), Vec3::new(0.0, h, z)], true, s);
            }
            for (x, y) in [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)] {
                d.line(Vec3::new(x, y, 0.0), Vec3::new(x, y, -t), s);
            }
            // Datum marker.
            let o = job.xy_datum.fraction();
            let dp = Vec3::new(o.x as f32 * w, o.y as f32 * h, 0.0);
            if let Some(p) = pr.to_screen(dp) {
                painter.circle_filled(p, 4.0, theme::ACCENT);
            }
        }

        // Solid content.
        if let Some(m) = &self.preview {
            d.mesh(m, [0.8, 0.7, 0.55], 255, false);
        } else {
            let reliefs: Vec<ReliefObject> = self.project.reliefs.iter().filter(|r| r.visible).cloned().collect();
            for r in &reliefs {
                let m = self.relief_mesh(r);
                d.mesh(&m, [0.85, 0.75, 0.6], 255, false);
            }
            for m in self.project.meshes.iter().filter(|m| m.visible) {
                let sel = Some(m.id) == self.selected_mesh;
                d.mesh(&m.mesh, if sel { [0.95, 0.7, 0.45] } else { [0.75, 0.78, 0.82] }, 255, true);
            }
            for po in self.project.parts.iter().filter(|p| p.visible) {
                if let Some((_, m)) = self.display.get(&format!("part{}", po.id)) {
                    let sel = Some(po.id) == self.selected_part;
                    // Parts are modelled around the origin; show them centred on the stock top.
                    let mut mm = m.clone();
                    let (lo, hi) = mm.bounds();
                    let c = (lo + hi) * 0.5;
                    mm.transform(&glam::Mat4::from_translation(Vec3::new(w * 0.5 - c.x, h * 0.5 - c.y, -hi.z)));
                    d.mesh(&mm, if sel { [0.95, 0.72, 0.45] } else { [0.72, 0.76, 0.84] }, 255, true);
                }
            }
        }

        // Vectors (hidden under the predicted-result preview).
        for v in self.project.vectors.iter().filter(|_| self.preview.is_none()) {
            let sel = self.selection.contains(&v.id);
            let col = if sel {
                theme::VECTOR_SEL
            } else if self.dark {
                Color32::from_gray(220)
            } else {
                theme::VECTOR
            };
            let pts: Vec<Vec3> = v.path.pts.iter().map(|p| Vec3::new(p.x as f32, p.y as f32, 0.02)).collect();
            d.polyline(&pts, v.path.closed, Stroke::new(if sel { 2.2 } else { 1.3 }, col));
        }

        // Toolpaths.
        if self.show_toolpaths && self.preview.is_none() {
            let maxd = self.results.iter().flat_map(|r| r.toolpaths.iter()).flat_map(|t| t.points()).map(|(p, _)| -p.z).fold(0.1f64, f64::max) as f32;
            let to3 = |p: &ezyx_cam::Pt, rot: bool| -> Vec3 {
                if rot && !self.mode2d {
                    let r = rotary.map(|(d, _)| d * 0.5).unwrap_or(0.0) + p.z as f32;
                    let a = (p.a as f32).to_radians();
                    Vec3::new(p.x as f32, r * a.sin(), r * a.cos())
                } else if rot {
                    let circ = rotary.map(|(d, _)| std::f32::consts::PI * d).unwrap_or(1.0);
                    Vec3::new(p.x as f32, p.a as f32 / 360.0 * circ, p.z as f32)
                } else {
                    Vec3::new(p.x as f32, p.y as f32, p.z as f32)
                }
            };
            let total: usize = self.results.iter().flat_map(|r| r.toolpaths.iter()).map(|t| t.moves.len()).sum();
            let stride = (total / 250_000).max(1);
            for r in &self.results {
                let highlighted = self.selected_op == Some(r.op_id);
                for tp in &r.toolpaths {
                    let mut prev: Option<Vec3> = None;
                    for (i, (p, rapid)) in tp.points().enumerate() {
                        let q = to3(&p, tp.rotary);
                        if let Some(a) = prev {
                            if rapid {
                                if self.show_rapids && i % stride == 0 {
                                    d.dashed(a, q, Stroke::new(0.7, theme::RAPID));
                                }
                            } else if i % stride == 0 || stride == 1 {
                                let c = theme::depth_color(-(p.z as f32) / maxd);
                                d.line(a, q, Stroke::new(if highlighted { 1.8 } else { 1.0 }, c));
                            }
                        }
                        prev = Some(q);
                    }
                }
            }
        }

        // Interaction.
        let hover = resp.hover_pos();
        let snap = |v: Vec3, ui: &egui::Ui| {
            if ui.input(|i| i.modifiers.alt) {
                v
            } else {
                Vec3::new((v.x * 2.0).round() / 2.0, (v.y * 2.0).round() / 2.0, 0.0)
            }
        };
        self.cursor_world = hover.and_then(|s| pr.ground(s, 0.0)).map(|g| snap(g, ui));
        match self.tool {
            DrawTool::Select => {
                if resp.clicked() {
                    if let Some(s) = hover {
                        let hit = self.pick_vector(&pr, s);
                        let shift = ui.input(|i| i.modifiers.shift || i.modifiers.command);
                        match (hit, shift) {
                            (Some(id), true) => {
                                if let Some(i) = self.selection.iter().position(|x| *x == id) {
                                    self.selection.remove(i);
                                } else {
                                    self.selection.push(id);
                                }
                            }
                            (Some(id), false) => self.selection = vec![id],
                            (None, false) => self.selection.clear(),
                            _ => {}
                        }
                        if hit.is_some() {
                            self.selected_op = None;
                        }
                    }
                }
                if self.mode2d {
                    if resp.drag_started_by(egui::PointerButton::Primary) {
                        self.draw.box_start = hover;
                    }
                    if let (Some(a), Some(b)) = (self.draw.box_start, hover) {
                        if resp.dragged_by(egui::PointerButton::Primary) {
                            painter.rect_stroke(egui::Rect::from_two_pos(a, b), 0.0, Stroke::new(1.0, theme::ACCENT), egui::StrokeKind::Middle);
                        }
                        if resp.drag_stopped() {
                            let r = egui::Rect::from_two_pos(a, b);
                            let ids: Vec<u64> = self
                                .project
                                .vectors
                                .iter()
                                .filter(|v| {
                                    v.path.pts.iter().all(|p| pr.to_screen(Vec3::new(p.x as f32, p.y as f32, 0.0)).map(|s| r.contains(s)).unwrap_or(false))
                                })
                                .map(|v| v.id)
                                .collect();
                            if ui.input(|i| i.modifiers.shift) {
                                self.selection.extend(ids);
                            } else {
                                self.selection = ids;
                            }
                            self.draw.box_start = None;
                        }
                    }
                }
            }
            DrawTool::Rect | DrawTool::Circle => {
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    self.draw.start = self.cursor_world;
                }
                if let (Some(a), Some(b)) = (self.draw.start, self.cursor_world) {
                    let stroke = Stroke::new(1.5, theme::ACCENT);
                    if self.tool == DrawTool::Rect {
                        d.polyline(&[a, Vec3::new(b.x, a.y, 0.0), b, Vec3::new(a.x, b.y, 0.0)], true, stroke);
                    } else {
                        let r = (b - a).length();
                        let pts: Vec<Vec3> = (0..64)
                            .map(|k| a + Vec3::new((k as f32 / 64.0 * std::f32::consts::TAU).cos(), (k as f32 / 64.0 * std::f32::consts::TAU).sin(), 0.0) * r)
                            .collect();
                        d.polyline(&pts, true, stroke);
                    }
                    if resp.drag_stopped() {
                        let cmd = if self.tool == DrawTool::Rect {
                            let (x0, y0) = (a.x.min(b.x) as f64, a.y.min(b.y) as f64);
                            let (ww, hh) = ((a.x - b.x).abs() as f64, (a.y - b.y).abs() as f64);
                            (ww > 0.2 && hh > 0.2).then_some(Command::AddRectangle { x: x0, y: y0, w: ww, h: hh, corner_radius: 0.0 })
                        } else {
                            let r = (b - a).length() as f64;
                            (r > 0.2).then_some(Command::AddCircle { cx: a.x as f64, cy: a.y as f64, r })
                        };
                        if let Some(c) = cmd {
                            self.exec(c);
                            self.selection = self.project.vectors.last().map(|v| vec![v.id]).unwrap_or_default();
                        }
                        self.draw.start = None;
                    }
                }
            }
            DrawTool::Polyline => {
                if let Some(p) = self.cursor_world {
                    if resp.double_clicked() {
                        self.finish_polyline();
                    } else if resp.clicked() {
                        self.draw.pts.push(p);
                    }
                    if !self.draw.pts.is_empty() {
                        let mut pts = self.draw.pts.clone();
                        pts.push(p);
                        d.polyline(&pts, false, Stroke::new(1.5, theme::ACCENT));
                    }
                }
            }
            DrawTool::Text => {
                if resp.clicked() {
                    self.dialogs.text_at = self.cursor_world;
                }
            }
        }

        d.gizmo();
        self.viewport_overlay(ui, rect);
        if let Some(c) = self.cursor_world {
            let u = job.display_units;
            painter.text(
                rect.right_bottom() + egui::vec2(-10.0, -8.0),
                egui::Align2::RIGHT_BOTTOM,
                format!("X {}  Y {}", u.fmt(c.x as f64), u.fmt(c.y as f64)),
                egui::FontId::monospace(12.0),
                theme::STEEL,
            );
        }
        if self.project.vectors.is_empty() && self.project.reliefs.is_empty() && self.project.meshes.is_empty() && self.project.parts.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Draw with the 2D Design tab · import SVG/DXF/STL · or load an image\nAsk the 🤖 Assistant: \"make a 200×100 walnut sign that says Workshop\"",
                egui::FontId::proportional(15.0),
                theme::STEEL,
            );
        }
    }

    fn finish_polyline(&mut self) {
        let mut pts = std::mem::take(&mut self.draw.pts);
        pts.dedup_by(|a, b| (*a - *b).length() < 1e-3);
        if pts.len() >= 2 {
            let closed = pts.len() > 2 && (pts[0] - *pts.last().unwrap()).length() < 2.0;
            if closed {
                pts.pop();
            }
            let points = pts.iter().map(|p| [p.x as f64, p.y as f64]).collect();
            self.exec(Command::AddPolyline { points, closed });
        }
    }

    fn pick_vector(&self, pr: &Projector, s: Pos2) -> Option<u64> {
        let mut best: Option<(f32, u64)> = None;
        for v in &self.project.vectors {
            let pts: Vec<Pos2> = v.path.pts.iter().filter_map(|p| pr.to_screen(Vec3::new(p.x as f32, p.y as f32, 0.0))).collect();
            let n = pts.len();
            if n < 2 {
                continue;
            }
            let segs = if v.path.closed { n } else { n - 1 };
            for i in 0..segs {
                let (a, b) = (pts[i], pts[(i + 1) % n]);
                let ab = b - a;
                let t = ((s - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                let dist = (a + ab * t - s).length();
                if dist < 8.0 && best.map(|(bd, _)| dist < bd).unwrap_or(true) {
                    best = Some((dist, v.id));
                }
            }
            // Clicking inside a closed shape also selects it (beginner friendly).
            if v.path.closed {
                if let Some(g) = pr.ground(s, 0.0) {
                    if v.path.contains(DVec2::new(g.x as f64, g.y as f64)) && best.is_none() {
                        best = Some((7.9, v.id));
                    }
                }
            }
        }
        best.map(|(_, id)| id)
    }

    fn viewport_overlay(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        egui::Area::new(egui::Id::unique("vp_overlay")).fixed_pos(rect.right_top() + egui::vec2(-400.0, 8.0)).order(egui::Order::Foreground).show(
            ui.ctx(),
            |ui| {
                egui::Frame::NONE.fill(ui.visuals().window_fill.gamma_multiply(0.92)).corner_radius(8).inner_margin(6).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let label = if self.mode2d { "2D" } else { "3D" };
                        if ui
                            .add(egui::Button::new(RichText::new(label).strong()).fill(theme::ACCENT_SOFT))
                            .on_hover_text("Toggle 2D drafting / 3D orbit")
                            .clicked()
                        {
                            let m = !self.mode2d;
                            self.set_mode2d(m);
                        }
                        for (txt, v) in [("Top", ViewPreset::Top), ("Front", ViewPreset::Front), ("Right", ViewPreset::Right), ("Iso", ViewPreset::Iso)] {
                            if ui.small_button(txt).clicked() {
                                if self.mode2d && v != ViewPreset::Top {
                                    self.mode2d = false;
                                    self.cam.ortho = false;
                                }
                                self.cam.preset(v);
                            }
                        }
                        if ui.small_button("⛶").on_hover_text("Fit").clicked() {
                            self.fit_view();
                        }
                        ui.toggle_value(&mut self.show_toolpaths, "🛠").on_hover_text("Show toolpaths");
                        if !self.mode2d {
                            ui.toggle_value(&mut self.cam.ortho, "▦").on_hover_text("Orthographic");
                        }
                    });
                    let hint = if self.mode2d { "LMB select/draw · RMB/MMB pan · wheel zoom" } else { "LMB orbit · RMB/MMB/Shift pan · wheel zoom" };
                    ui.label(RichText::new(hint).small().color(theme::STEEL));
                });
            },
        );
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let mut acts = Vec::new();
        ctx.input(|i| {
            let cmd = i.modifiers.command;
            if cmd && i.key_pressed(egui::Key::Z) {
                acts.push(Action::Undo);
            }
            if cmd && i.key_pressed(egui::Key::Y) {
                acts.push(Action::Redo);
            }
            if cmd && i.key_pressed(egui::Key::S) {
                acts.push(Action::Save);
            }
            if i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace) {
                acts.push(Action::Delete);
            }
            if i.key_pressed(egui::Key::F) && !cmd {
                acts.push(Action::Fit);
            }
            if i.key_pressed(egui::Key::Tab) {
                acts.push(Action::Toggle2D);
            }
            if i.key_pressed(egui::Key::Escape) {
                acts.push(Action::Tool(DrawTool::Select));
            }
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) && self.tool == DrawTool::Polyline {
            self.finish_polyline();
        }
        for a in acts {
            if a == Action::Delete && self.selection.is_empty() {
                continue;
            }
            self.handle(a, ctx);
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.dialogs.about {
            egui::Window::new("About EZ-YX-craft").collapsible(false).resizable(false).open(&mut self.dialogs.about).show(ctx, |ui| {
                ui.heading(RichText::new("EZ-YX-craft").color(theme::ACCENT));
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.label("Open-source design, CAM, simulation and machine control for wood & metal carving.");
                ui.label("License: MIT OR Apache-2.0");
                ui.separator();
                ui.label("Bundled bit data: SainSmart Genmitsu bit table V6.0 (Graham Bland, community).");
                if let Some(g) = &self.gpu {
                    ui.label(format!("GPU: {} ({} · {:?})", g.name, g.backend, g.tier));
                    for n in &g.notes {
                        ui.label(RichText::new(n).small());
                    }
                } else {
                    ui.label("GPU: none detected (CPU paths)");
                }
            });
        }
        if let Some(at) = self.dialogs.text_at {
            let mut open = true;
            let mut add = false;
            egui::Window::new("Add text").collapsible(false).open(&mut open).show(ctx, |ui| {
                ui.text_edit_singleline(&mut self.dialogs.text);
                ui.add(egui::DragValue::new(&mut self.dialogs.text_height).range(1.0..=1000.0).suffix(" mm cap height"));
                ui.horizontal(|ui| {
                    ui.label("Font file (optional):");
                    ui.text_edit_singleline(&mut self.dialogs.font_path);
                    if ui.button("…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("Fonts", &["ttf", "otf"]).pick_file() {
                            self.dialogs.font_path = p.display().to_string();
                        }
                    }
                });
                if ui.button(RichText::new("Add").strong()).clicked() {
                    add = true;
                }
            });
            if add {
                let font = (!self.dialogs.font_path.is_empty()).then(|| self.dialogs.font_path.clone());
                self.exec(Command::AddText {
                    text: self.dialogs.text.clone(),
                    height: self.dialogs.text_height,
                    x: at.x as f64,
                    y: at.y as f64,
                    font_path: font,
                });
                self.dialogs.text_at = None;
            } else if !open {
                self.dialogs.text_at = None;
            }
        }
        if let Some(r) = self.dialogs.tet_report.clone() {
            let mut open = true;
            egui::Window::new("Tetrahedral mesh").open(&mut open).show(ctx, |ui| {
                ui.label(r);
            });
            if !open {
                self.dialogs.tet_report = None;
            }
        }
    }
}

/// Convert a wrapped rotary toolpath back to flat (unwrapped) coordinates for simulation.
pub fn unwrap_rotary(tp: &ezyx_cam::Toolpath, diameter: f64) -> ezyx_cam::Toolpath {
    use ezyx_cam::{Move, Pt};
    if !tp.rotary {
        return tp.clone();
    }
    let circ = std::f64::consts::PI * diameter;
    let f = |p: &Pt| Pt { x: p.x, y: p.a / 360.0 * circ, z: p.z, a: 0.0 };
    let mut out = tp.clone();
    out.rotary = false;
    out.moves = tp
        .moves
        .iter()
        .map(|m| match m {
            Move::Rapid(p) => Move::Rapid(f(p)),
            Move::Feed { to, feed } => Move::Feed { to: f(to), feed: *feed },
            o => o.clone(),
        })
        .collect();
    out
}

impl eframe::App for EzApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_tasks();
        if std::mem::take(&mut self.pending_preview) {
            self.tab = Tab::Simulate;
            self.start_preview(&ctx);
        }
        self.machine.poll(&ctx);
        self.ai.poll(&mut self.project, &mut self.history, &mut self.rev);
        if self.results_rev != self.rev && !self.results.is_empty() && self.tasks.running == 0 {
            // Keep toolpaths in sync with edits.
            self.calculate(Some(&ctx));
        }
        self.ensure_part_meshes(&ctx);
        self.shortcuts(&ctx);

        let mut actions = Vec::new();
        egui::Panel::top("tabs").show(ui, |ui| ribbon::tabs(ui, &mut self.tab, self.show_ai, &mut actions));
        egui::Panel::top("ribbon").show(ui, |ui| ribbon::ribbon(ui, self.tab, &mut actions));
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if self.tasks.running > 0 {
                    ui.spinner();
                }
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(self.machine.badge()).color(theme::STEEL));
                    ui.separator();
                    ui.label(RichText::new(format!("{} · {}", self.project.machine.name, self.project.job.material)).color(theme::STEEL));
                });
            });
        });
        egui::Panel::left("tree").resizable(true).default_size(250.0).show(ui, |ui| panels::tree::show(self, ui));
        if self.show_ai {
            egui::Panel::right("ai").resizable(true).default_size(360.0).show(ui, |ui| panels::ai::show(self, ui));
        }
        egui::Panel::right("inspector").resizable(true).default_size(330.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                Tab::Machine => panels::machine::show(self, ui),
                Tab::Image => panels::image::show(self, ui, &mut actions),
                _ => panels::inspector::show(self, ui),
            });
        });
        egui::CentralPanel::no_frame().show(ui, |ui| self.viewport(ui));
        self.dialogs(&ctx);
        panels::machine::confirm_dialog(self, &ctx);
        panels::ai::confirm_dialog(self, &ctx);
        for a in actions {
            self.handle(a, &ctx);
        }
        if self.tasks.running > 0 || self.machine.connected() || self.ai.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}
