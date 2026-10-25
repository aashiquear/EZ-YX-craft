//! Application shell.
//!
//! The main window is the **pipeline** (① Stock → ② Design → ③ Toolpaths → ④ Output). The
//! **Image Studio**, **Solid Modeler** and **Machine Control** are separate modules that open in
//! their own native windows (so they can sit side by side with the pipeline) or, if the user
//! prefers / the platform can't do multiple windows, as full-window tabs inside the main window.
//! Modules never mutate the pipeline behind its back: they send results through
//! `ezyx_project::Command`s (`ImportFeature` / `ReimportFeature`), which gives undo and merge
//! reports for free.

use crate::ribbon::{self, Action, Align, DrawTool, Module, OpTemplate, PassFilter, Step};
use crate::tasks::{TaskResult, Tasks};
use crate::viewport::{Camera, ViewPreset};
use crate::{docshots, image_studio, machine_control, panels, pipeline, solid, theme};
use egui::RichText;
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
    pipeline::{Conflict, FeatureSource, StockShape},
    History, OpKind, OperationSpec, Project,
};
use ezyx_tooldb::{Operation, ToolLibrary};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Default)]
pub struct DrawState {
    pub start: Option<Vec3>,
    pub pts: Vec<Vec3>,
    pub box_start: Option<egui::Pos2>,
    /// Quick-key entry while drawing ("120,60" + Enter).
    pub typed: String,
    /// Measure tool: first point and last measured segment.
    pub measure: Option<(Vec3, Option<Vec3>)>,
}

pub struct Dialogs {
    pub about: bool,
    pub settings: bool,
    pub text_at: Option<Vec3>,
    pub text: String,
    pub text_height: f64,
    pub font_path: String,
    pub offset: f64,
    pub merge_report: Option<(String, bool)>,
}

/// Preferences persisted in the user's config dir.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Open modules as separate OS windows (else as tabs in the main window).
    pub separate_windows: bool,
    pub dark: bool,
    pub snap_grid: bool,
    pub grid: f64,
    pub snap_objects: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self { separate_windows: true, dark: false, snap_grid: true, grid: 0.5, snap_objects: true }
    }
}

impl Prefs {
    fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("ezyx-craft").join("prefs.json"))
    }
    pub fn load() -> Self {
        Self::path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    pub fn save(&self) {
        if let Some(p) = Self::path() {
            let _ = std::fs::create_dir_all(p.parent().unwrap_or(&p));
            let _ = std::fs::write(p, serde_json::to_string_pretty(self).unwrap_or_default());
        }
    }
}

pub struct EzApp {
    pub project: Project,
    pub history: History,
    pub library: ToolLibrary,
    pub path: Option<PathBuf>,
    pub step: Step,
    pub tool: DrawTool,
    pub selection: Vec<u64>,
    pub selected_feature: Option<u64>,
    pub selected_op: Option<u64>,
    pub cam: Camera,
    pub mode2d: bool,
    pub prefs: Prefs,
    pub show_ai: bool,
    pub status: String,
    pub rev: u64,
    pub results: Vec<OpResult>,
    pub results_rev: u64,
    pub conflicts: Vec<Conflict>,
    conflicts_rev: u64,
    pub passes: PassFilter,
    pub preview: Option<TriMesh>,
    pub preview_info: String,
    pub display: HashMap<String, (u64, TriMesh)>,
    pub tasks: Tasks,
    pub gpu: Option<ezyx_gpu::GpuInfo>,
    pub perf: ezyx_gpu::PerfDefaults,
    pub draw: DrawState,
    pub dialogs: Dialogs,
    pub image: image_studio::Studio,
    pub solid: solid::Modeler,
    pub machine: panels::machine::MachineState,
    pub ai: panels::ai::AiState,
    /// Module windows currently open.
    pub open: HashSet<Module>,
    /// Embedded mode: the module shown in the main window instead of the pipeline.
    pub focus: Option<Module>,
    pub show_toolpaths: bool,
    pub show_rapids: bool,
    pub tool_filter: String,
    pub cursor_world: Option<Vec3>,
    pub docs: Option<docshots::DocShots>,
    pending_preview: bool,
    /// Frames until the module windows opened by `EZYX_OPEN_MODULES` are tiled (0 = done).
    tile_in: u32,
}

pub const STOCK_PRESETS: [(&str, f64, f64, f64, &str, StockShape); 6] = [
    ("Walnut sign board 300 × 140 × 18", 300.0, 140.0, 18.0, "hardwood.walnut", StockShape::Rectangle),
    ("Pine 1×6 board 600 × 140 × 19", 600.0, 140.0, 19.0, "softwood.pine", StockShape::Rectangle),
    ("MDF sheet 600 × 400 × 12", 600.0, 400.0, 12.0, "engineered.mdf", StockShape::Rectangle),
    ("Round maple blank Ø200 × 20", 200.0, 200.0, 20.0, "hardwood.maple", StockShape::Round),
    ("Aluminium plate 150 × 100 × 6", 150.0, 100.0, 6.0, "metal.al6061", StockShape::Rectangle),
    ("HDU sign foam 400 × 300 × 25", 400.0, 300.0, 25.0, "foam.hdu", StockShape::Rectangle),
];

impl EzApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let prefs = Prefs::load();
        theme::apply(&cc.egui_ctx, prefs.dark);
        let gpu = ezyx_gpu::best();
        let perf = ezyx_gpu::defaults_for(gpu.as_ref().map(|g| g.tier).unwrap_or(ezyx_gpu::Tier::Capable));
        let docs = docshots::DocShots::from_env();
        let mut app = Self {
            project: Project::default(),
            history: History::default(),
            library: ToolLibrary::bundled(),
            path: None,
            step: Step::Stock,
            tool: DrawTool::Select,
            selection: vec![],
            selected_feature: None,
            selected_op: None,
            cam: Camera::default(),
            mode2d: true,
            prefs,
            show_ai: false,
            status: "Welcome! ① Choose your stock, ② design or import features, ③ check the toolpaths, ④ get G-code.".into(),
            rev: 1,
            results: vec![],
            results_rev: 0,
            conflicts: vec![],
            conflicts_rev: 0,
            passes: PassFilter::Both,
            preview: None,
            preview_info: String::new(),
            display: HashMap::new(),
            tasks: Tasks::default(),
            gpu,
            perf,
            draw: DrawState::default(),
            dialogs: Dialogs {
                about: false,
                settings: false,
                text_at: None,
                text: "Hello".into(),
                text_height: 30.0,
                font_path: String::new(),
                offset: 3.0,
                merge_report: None,
            },
            image: Default::default(),
            solid: Default::default(),
            machine: Default::default(),
            ai: panels::ai::AiState::load(),
            open: HashSet::new(),
            focus: None,
            show_toolpaths: true,
            show_rapids: true,
            tool_filter: String::new(),
            cursor_world: None,
            docs,
            pending_preview: std::env::var("EZYX_START_PREVIEW").is_ok(),
            tile_in: 0,
        };
        if app.docs.is_some() {
            // Screenshots are taken of the main window: show modules embedded, fixed look.
            app.prefs = Prefs { separate_windows: false, ..Prefs::default() };
            theme::apply(&cc.egui_ctx, false);
            app.perf.sim_cell_mm = 0.25;
            app.perf.viewport_lod = 1;
        }
        // Open a project passed on the command line (also used by OS file associations).
        if let Some(path) = std::env::args().nth(1).filter(|a| !a.starts_with('-')).map(PathBuf::from) {
            app.load_project(&path);
        }
        // `EZYX_OPEN_MODULES=image,solid,machine` opens module windows at start (testing / kiosks).
        for m in std::env::var("EZYX_OPEN_MODULES").unwrap_or_default().split(',') {
            match m.trim() {
                "image" => app.open_module(Module::Image),
                "solid" => app.open_module(Module::Solid),
                "machine" => app.open_module(Module::Machine),
                _ => {}
            }
        }
        if !app.open.is_empty() {
            app.tile_in = 20;
        }
        app.fit_view();
        app
    }

    // ------------------------------------------------------------------ helpers

    pub fn exec(&mut self, c: Command) -> bool {
        let reimport = matches!(c, Command::ReimportFeature { .. });
        match self.history.execute(&mut self.project, &c) {
            Ok(m) => {
                if reimport {
                    let clean = !m.contains('✖') && !m.contains('⚠');
                    self.dialogs.merge_report = Some((m.clone(), clean));
                }
                self.status = m.lines().next().unwrap_or_default().to_string();
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

    pub fn load_project(&mut self, path: &std::path::Path) {
        match Project::load(path) {
            Ok(p) => {
                self.project = p;
                self.history = History::default();
                self.path = Some(path.to_path_buf());
                self.selection.clear();
                self.selected_feature = None;
                self.selected_op = None;
                self.results.clear();
                self.preview = None;
                self.step = if self.project.features.is_empty() && self.project.operations.is_empty() { Step::Stock } else { Step::Design };
                self.touch();
                self.fit_view();
                self.calculate(None);
                self.status = format!("Opened {}", path.display());
            }
            Err(e) => self.status = format!("⚠ could not open {}: {e}", path.display()),
        }
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
        self.cam.ortho = on;
        self.cam.preset(if on { ViewPreset::Top } else { ViewPreset::Iso });
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

    /// Hand-made operation (Toolpaths → Advanced). Never touched by the planner.
    pub fn add_op(&mut self, t: OpTemplate) {
        let src = match self.selected_feature.and_then(|id| self.project.features.iter().find(|f| f.id == id)).map(|f| &f.kind) {
            Some(ezyx_project::FeatureKind::Relief { relief_id }) => ReliefSource::Relief(*relief_id),
            Some(ezyx_project::FeatureKind::Model3D { mesh_id }) => ReliefSource::Mesh(*mesh_id),
            _ => ReliefSource::AllReliefs,
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
            feature_id: self.selected_feature,
            auto: false,
            feature_rev: self.selected_feature.and_then(|id| self.project.features.iter().find(|f| f.id == id)).map(|f| f.revision).unwrap_or(0),
        };
        if self.exec(Command::AddOperation { spec }) {
            self.selected_op = self.project.operations.last().map(|o| o.id);
            self.step = Step::Toolpaths;
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

    /// Simulate material removal for the passes the filter shows (`final_render` = everything,
    /// switch to the 3D view).
    pub fn simulate(&mut self, ctx: &egui::Context, final_render: bool) {
        let p = self.project.clone();
        let lib = self.library.clone();
        let cell = self.perf.sim_cell_mm.max(0.08);
        let lod = self.perf.viewport_lod;
        let filter = if final_render { PassFilter::Both } else { self.passes };
        self.status = format!("Simulating {} at {cell:.2} mm resolution…", if final_render { "the finished piece" } else { "the shown passes" });
        self.tasks.spawn(ctx, move || {
            let t0 = std::time::Instant::now();
            let results = ezyx_project::ops::compute_all(&p, &lib);
            let rotary = match p.job.kind {
                JobKind::Rotary { diameter, .. } => Some(diameter),
                _ => None,
            };
            let mut sim = ezyx_sim::StockSim::new(&p.job, cell);
            if p.stock_shape == StockShape::Round {
                ezyx_project::pipeline::mask_round_stock(&mut sim, &p.job);
            }
            for r in &results {
                let Some(op) = p.operations.iter().find(|o| o.id == r.op_id) else { continue };
                for tp in r.toolpaths.iter().filter(|tp| filter.shows(ezyx_project::pipeline::toolpath_pass(op, tp))) {
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
            TaskResult::Simulation {
                mesh,
                removed_cm3: sim.removed_volume() / 1000.0,
                warnings: sim.warnings.clone(),
                seconds: t0.elapsed().as_secs_f64(),
                final_render,
            }
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
                TaskResult::Simulation { mesh, removed_cm3, warnings, seconds, final_render } => {
                    // Docs screenshots must be deterministic: no timings.
                    let timing = if self.docs.is_some() { String::new() } else { format!(" · simulated in {seconds:.1} s") };
                    self.preview_info = format!("Removed {removed_cm3:.1} cm³{timing} · {} triangles", mesh.indices.len());
                    if !warnings.is_empty() {
                        self.preview_info += &format!("\n⚠ {}", warnings.join("\n⚠ "));
                    }
                    self.preview = Some(mesh);
                    self.status = "Preview ready — orbit with the left mouse button.".into();
                    if self.mode2d || final_render {
                        self.set_mode2d(false);
                    }
                }
                TaskResult::Solid(r) => self.solid.on_result(r),
                TaskResult::Studio(r) => self.image.on_result(r),
                TaskResult::Error(e) => self.status = format!("⚠ {e}"),
            }
        }
    }

    fn refresh_conflicts(&mut self) {
        if self.conflicts_rev != self.rev {
            self.conflicts_rev = self.rev;
            self.conflicts = ezyx_project::pipeline::analyze(&self.project, &self.library);
        }
    }

    // ------------------------------------------------------------------ modules

    pub fn open_module(&mut self, m: Module) {
        if self.prefs.separate_windows {
            self.open.insert(m);
            self.focus = None;
        } else {
            self.focus = Some(m);
        }
    }

    /// Reopen the editor that produced a feature, primed with that feature's source.
    pub fn edit_feature(&mut self, id: u64, ctx: &egui::Context) {
        let Some(f) = self.project.features.iter().find(|f| f.id == id).cloned() else { return };
        match &f.source {
            FeatureSource::ImageStudio { .. } => {
                self.image.edit_feature(&f, ctx);
                self.open_module(Module::Image);
            }
            FeatureSource::SolidModeler { part_id, as_relief, .. } => {
                self.solid.edit_feature(f.id, *part_id, *as_relief);
                self.open_module(Module::Solid);
            }
            _ => {
                self.step = Step::Design;
                if let ezyx_project::FeatureKind::Cut2D { vector_ids, .. } = &f.kind {
                    self.selection = vector_ids.clone();
                }
                self.status = format!("'{}' was made here — edit its vectors in ② Design", f.name);
            }
        }
    }

    /// Place the pipeline on the left half of the screen and stack the open module windows on the
    /// right half (one module gets the whole right half).
    fn tile_windows(&mut self, ctx: &egui::Context) {
        let monitor = ctx.input(|i| i.viewport().monitor_size).unwrap_or(egui::vec2(1920.0, 1080.0));
        let mods: Vec<Module> = Module::ALL.iter().copied().filter(|m| self.open.contains(m)).collect();
        if mods.is_empty() {
            self.status = "Open a module (Image Studio, Solid Modeler, Machine Control) to tile it next to the pipeline.".into();
            return;
        }
        let half = (monitor.x * 0.5).floor();
        let h = monitor.y - 60.0;
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(0.0, 0.0)));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(half - 8.0, h)));
        let each = h / mods.len() as f32;
        for (i, m) in mods.iter().enumerate() {
            let id = module_viewport(*m);
            ctx.send_viewport_cmd_to(id, egui::ViewportCommand::OuterPosition(egui::pos2(half, each * i as f32)));
            ctx.send_viewport_cmd_to(id, egui::ViewportCommand::InnerSize(egui::vec2(half - 8.0, each - 40.0)));
        }
        self.fit_view();
    }

    fn module_ui(&mut self, m: Module, ui: &mut egui::Ui) {
        match m {
            Module::Image => image_studio::show(self, ui),
            Module::Solid => solid::show(self, ui),
            Module::Machine => machine_control::show(self, ui),
        }
    }

    fn show_module_windows(&mut self, ctx: &egui::Context) {
        for m in Module::ALL {
            if !self.open.contains(&m) {
                continue;
            }
            let builder = egui::ViewportBuilder::default().with_title(m.title()).with_inner_size([1180.0, 800.0]).with_min_inner_size([720.0, 480.0]);
            let close = ctx.show_viewport_immediate(module_viewport(m), builder, |ui, class| {
                if class == egui::ViewportClass::EmbeddedWindow {
                    ui.set_min_size(egui::vec2(900.0, 600.0));
                }
                self.module_ui(m, ui);
                ui.ctx().input(|i| i.viewport().close_requested())
            });
            if close {
                self.open.remove(&m);
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
                self.selected_feature = None;
                self.selected_op = None;
                self.results.clear();
                self.preview = None;
                self.step = Step::Stock;
                self.touch();
                self.fit_view();
                self.status = "New project — choose your stock.".into();
            }
            Action::Open => {
                if let Some(p) = Self::file_dialog().add_filter("EZ-YX-craft project", &["ezyx"]).pick_file() {
                    self.load_project(&p);
                }
            }
            Action::Save => self.save(false),
            Action::SaveAs => self.save(true),
            Action::ImportVectors => {
                if let Some(p) = Self::file_dialog().add_filter("Vectors", &["svg", "dxf"]).pick_file() {
                    self.import_vectors(&p);
                }
            }
            Action::ImportMesh => {
                if let Some(p) = Self::file_dialog().add_filter("3D model", &["stl", "obj"]).pick_file() {
                    self.import_mesh(&p);
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
            Action::GoTo(s) => self.go_to(s, ctx),
            Action::NextStep => {
                if let Some(n) = self.step.next() {
                    self.go_to(n, ctx);
                }
            }
            Action::StockPreset(i) => {
                let (name, w, h, t, mat, shape) = STOCK_PRESETS[i.min(STOCK_PRESETS.len() - 1)];
                self.exec(Command::SetFlat);
                let material = ezyx_materials::find(mat).map(|m| m.id);
                self.exec(Command::SetJob { width: Some(w), height: Some(h), thickness: Some(t), material, safe_z: None });
                self.exec(Command::SetStockShape { shape });
                self.fit_view();
                self.status = format!("Stock: {name}");
            }
            Action::Tool(t) => {
                self.tool = t;
                self.draw = DrawState::default();
                if t != DrawTool::Select && !self.mode2d {
                    self.set_mode2d(true);
                }
                if self.step != Step::Design {
                    self.step = Step::Design;
                }
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
            Action::Align(al) => self.align(al),
            Action::Distribute { horizontal } => self.distribute(horizontal),
            Action::Mirror { horizontal } => {
                let ids = self.selection.clone();
                let m = if horizontal { [-1.0, 0.0, 0.0, 1.0, 0.0, 0.0] } else { [1.0, 0.0, 0.0, -1.0, 0.0, 0.0] };
                self.exec(Command::TransformVectors { ids, matrix: m, about_center: true });
            }
            Action::Delete => {
                let ids = std::mem::take(&mut self.selection);
                if !ids.is_empty() {
                    self.exec(Command::DeleteVectors { ids });
                }
            }
            Action::SelectAll => {
                let locked: Vec<usize> = self.project.layers.iter().enumerate().filter(|(_, l)| l.locked || !l.visible).map(|(i, _)| i).collect();
                self.selection = self.project.vectors.iter().filter(|v| !locked.contains(&v.layer)).map(|v| v.id).collect();
            }
            Action::Carve(strategy) => {
                let ids = self.selection.clone();
                if self.exec(Command::CarveVectors { ids, strategy, depth: None, name: None }) {
                    self.selected_feature = self.project.features.last().map(|f| f.id);
                    self.selection.clear();
                    self.calculate(Some(ctx));
                }
            }
            Action::OpenModule(m) => self.open_module(m),
            Action::EditFeature(id) => self.edit_feature(id, ctx),
            Action::AutoPlanAll => {
                // Back to the planner's defaults for every feature (keeps hand-made operations).
                let ids: Vec<u64> = self.project.features.iter().map(|f| f.id).collect();
                self.history.checkpoint(&self.project, "auto-plan all");
                // Hand-tuned copies of planned passes go; operations added via Advanced stay.
                let tuned: Vec<(u64, bool, bool)> = self.project.features.iter().map(|f| (f.id, f.plan.hand_rough, f.plan.hand_finish)).collect();
                self.project.operations.retain(|o| {
                    !tuned.iter().any(|(fid, r, fi)| {
                        o.feature_id == Some(*fid) && !o.auto && ((*r && o.pass == Operation::Roughing) || (*fi && o.pass == Operation::Finishing))
                    })
                });
                for f in self.project.features.iter_mut() {
                    f.plan = Default::default();
                }
                let (n, errs) = ezyx_project::pipeline::replan(&mut self.project, &self.library, &ids);
                self.status =
                    if errs.is_empty() { format!("🪄 Planned {n} toolpaths with the recommended bits") } else { format!("⚠ {}", errs.join("; ")) };
                self.touch();
                self.calculate(Some(ctx));
            }
            Action::Calculate => {
                self.exec(Command::PlanToolpaths { feature_ids: vec![] });
                self.calculate(Some(ctx));
            }
            Action::AddOp(t) => self.add_op(t),
            Action::ShowPasses(f) => {
                self.passes = f;
                self.preview = None;
            }
            Action::Simulate => self.simulate(ctx, false),
            Action::FinalRender => self.simulate(ctx, true),
            Action::ClearPreview => self.preview = None,
            Action::ExportGcode => self.export_gcode(),
            Action::JobSheet => self.export_job_sheet(),
            Action::SendToMachine => self.send_to_machine(),
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
                self.prefs.dark = !self.prefs.dark;
                theme::apply(ctx, self.prefs.dark);
                self.prefs.save();
            }
            Action::TileWindows => self.tile_windows(ctx),
            Action::Settings => self.dialogs.settings = true,
            Action::About => self.dialogs.about = true,
        }
    }

    fn go_to(&mut self, s: Step, ctx: &egui::Context) {
        self.step = s;
        match s {
            Step::Stock => {
                self.tool = DrawTool::Select;
                self.preview = None;
            }
            Step::Design => self.preview = None,
            Step::Toolpaths | Step::Output => {
                self.tool = DrawTool::Select;
                // Arriving here always shows an up-to-date plan.
                if self.results_rev != self.rev || self.results.is_empty() {
                    self.calculate(Some(ctx));
                }
            }
        }
    }

    pub fn import_vectors(&mut self, p: &std::path::Path) {
        let path = p.display().to_string();
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        let polys = match ext.as_str() {
            "svg" => std::fs::read(p).map_err(|e| e.to_string()).and_then(|b| ezyx_project::import::import_svg(&b)),
            "dxf" => ezyx_project::import::import_dxf(p),
            other => Err(format!("unsupported vector format '.{other}'")),
        };
        match polys {
            Ok(paths) => {
                let name = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Import".into());
                let payload = ezyx_project::Payload::Vectors { paths, strategy: None, depth: None };
                if self.exec(Command::ImportFeature { name, payload, source: FeatureSource::File { path } }) {
                    self.selected_feature = self.project.features.last().map(|f| f.id);
                    self.step = Step::Design;
                }
            }
            Err(e) => self.status = format!("⚠ {e}"),
        }
    }

    pub fn import_mesh(&mut self, p: &std::path::Path) {
        match ezyx_mesh::io::load(p) {
            Ok(mut mesh) => {
                // Fit big models into the stock (90 % of XY, 60 % of the thickness).
                ezyx_project::commands::place_mesh(&mut mesh, &self.project, true, self.project.job.thickness * 0.6);
                let name = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Model".into());
                let payload = ezyx_project::Payload::Mesh { mesh };
                if self.exec(Command::ImportFeature { name, payload, source: FeatureSource::File { path: p.display().to_string() } }) {
                    self.selected_feature = self.project.features.last().map(|f| f.id);
                    self.set_mode2d(false);
                }
            }
            Err(e) => self.status = format!("⚠ {e}"),
        }
    }

    fn sel_bounds(&self, ids: &[u64]) -> ezyx_core::Bounds2 {
        ids.iter().filter_map(|id| self.project.vector(*id)).fold(ezyx_core::Bounds2::EMPTY, |a, v| a.union(&v.path.bounds()))
    }

    fn align(&mut self, al: Align) {
        if self.selection.is_empty() {
            return;
        }
        // One shape aligns to the stock; several align to their common bounds.
        let all = if self.selection.len() == 1 { self.project.job.stock_bounds() } else { self.sel_bounds(&self.selection) };
        self.history.checkpoint(&self.project, "align");
        for id in self.selection.clone() {
            let b = self.sel_bounds(&[id]);
            let d = match al {
                Align::Left => DVec2::new(all.min.x - b.min.x, 0.0),
                Align::Right => DVec2::new(all.max.x - b.max.x, 0.0),
                Align::CenterX => DVec2::new(all.center().x - b.center().x, 0.0),
                Align::Bottom => DVec2::new(0.0, all.min.y - b.min.y),
                Align::Top => DVec2::new(0.0, all.max.y - b.max.y),
                Align::CenterY => DVec2::new(0.0, all.center().y - b.center().y),
            };
            let _ = Command::MoveVectors { ids: vec![id], dx: d.x, dy: d.y }.apply(&mut self.project);
        }
        self.touch();
        self.status = format!("Aligned {} shape(s)", self.selection.len());
    }

    fn distribute(&mut self, horizontal: bool) {
        if self.selection.len() < 3 {
            self.status = "Select three or more shapes to distribute.".into();
            return;
        }
        let axis = |v: DVec2| if horizontal { v.x } else { v.y };
        let mut items: Vec<(u64, f64)> = self.selection.iter().map(|id| (*id, axis(self.sel_bounds(&[*id]).center()))).collect();
        items.sort_by(|a, b| a.1.total_cmp(&b.1));
        let (first, last) = (items[0].1, items[items.len() - 1].1);
        let step = (last - first) / (items.len() - 1) as f64;
        self.history.checkpoint(&self.project, "distribute");
        for (i, (id, c)) in items.iter().enumerate() {
            let d = first + step * i as f64 - c;
            let (dx, dy) = if horizontal { (d, 0.0) } else { (0.0, d) };
            let _ = Command::MoveVectors { ids: vec![*id], dx, dy }.apply(&mut self.project);
        }
        self.touch();
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

    pub fn export_job_sheet(&mut self) {
        let s = ezyx_project::jobsheet::summarize(&self.project, &self.library, &self.results);
        let html = ezyx_project::jobsheet::html(&self.project, &s, &self.conflicts);
        if let Some(path) = Self::file_dialog().add_filter("HTML", &["html"]).set_file_name(format!("{} job sheet.html", self.project.name)).save_file() {
            self.status = match std::fs::write(&path, html) {
                Ok(()) => format!("Job sheet saved to {}", path.display()),
                Err(e) => format!("⚠ {e}"),
            };
        }
    }

    pub fn send_to_machine(&mut self) {
        match ezyx_project::ops::export_gcode(&self.project, &self.library) {
            Ok((g, warnings)) => {
                let name = format!("{} (pipeline)", self.project.name);
                self.machine.add_job(name, g, "pipeline");
                self.open_module(Module::Machine);
                self.status = if warnings.is_empty() { "Job sent to Machine Control".into() } else { format!("Job sent — ⚠ {}", warnings.join("; ")) };
            }
            Err(e) => self.status = format!("⚠ {e}"),
        }
    }

    // ------------------------------------------------------------------ chrome

    fn top_bar(&mut self, ui: &mut egui::Ui, out: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("EZ-YX").strong().color(theme::ACCENT).size(16.0));
            ui.label(RichText::new("craft").color(theme::STEEL).size(16.0));
            ui.menu_button("File", |ui| {
                for (l, a) in [("🗋 New", Action::New), ("📂 Open…", Action::Open), ("💾 Save", Action::Save), ("💾 Save as…", Action::SaveAs)] {
                    if ui.button(l).clicked() {
                        out.push(a);
                        ui.close();
                    }
                }
                ui.separator();
                for (l, a) in [("📐 Import SVG/DXF…", Action::ImportVectors), ("🔺 Import STL/OBJ…", Action::ImportMesh)] {
                    if ui.button(l).clicked() {
                        out.push(a);
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("⚙ Settings").clicked() {
                    out.push(Action::Settings);
                    ui.close();
                }
                if ui.button("ℹ About").clicked() {
                    out.push(Action::About);
                    ui.close();
                }
            });
            ui.separator();
            // Pipeline + modules. In embedded mode these act as tabs.
            let pipeline_sel = self.focus.is_none();
            if ribbon::pill(ui, "🛠 Pipeline", pipeline_sel, ezyx_project::registry::modules()[0].summary) {
                self.focus = None;
            }
            for m in Module::ALL {
                let sel = self.focus == Some(m) || self.open.contains(&m);
                let label = if self.prefs.separate_windows { format!("{} ↗", m.label()) } else { m.label().to_string() };
                if ribbon::pill(ui, &label, sel, &ribbon::tip(m.feature_id())) {
                    out.push(Action::OpenModule(m));
                }
            }
            if self.prefs.separate_windows && ui.button("⧉ Tile").on_hover_text(ribbon::tip("settings.windows")).clicked() {
                out.push(Action::TileWindows);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ai =
                    egui::Button::new(RichText::new("🤖 Assistant").strong()).fill(if self.show_ai { theme::ACCENT_SOFT } else { egui::Color32::TRANSPARENT });
                if ui.add(ai).on_hover_text(ribbon::tip("assistant.chat")).clicked() {
                    out.push(Action::ToggleAi);
                }
                if ui.button("🌓").on_hover_text("Light / dark").clicked() {
                    out.push(Action::ToggleDark);
                }
                if ui.add_enabled(self.history.can_redo(), egui::Button::new("↪")).on_hover_text("Redo (Ctrl+Y)").clicked() {
                    out.push(Action::Redo);
                }
                if ui.add_enabled(self.history.can_undo(), egui::Button::new("↩")).on_hover_text("Undo (Ctrl+Z)").clicked() {
                    out.push(Action::Undo);
                }
            });
        });
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let mut acts = Vec::new();
        let drawing = matches!(self.tool, DrawTool::Rect | DrawTool::Circle | DrawTool::Ellipse) && self.draw.start.is_some();
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
            if cmd && i.key_pressed(egui::Key::A) {
                acts.push(Action::SelectAll);
            }
            for (k, s) in [(egui::Key::Num1, Step::Stock), (egui::Key::Num2, Step::Design), (egui::Key::Num3, Step::Toolpaths), (egui::Key::Num4, Step::Output)]
            {
                if cmd && i.key_pressed(k) {
                    acts.push(Action::GoTo(s));
                }
            }
            if !cmd && !drawing {
                if i.key_pressed(egui::Key::Delete) {
                    acts.push(Action::Delete);
                }
                if i.key_pressed(egui::Key::F) {
                    acts.push(Action::Fit);
                }
                for (k, t) in [
                    (egui::Key::R, DrawTool::Rect),
                    (egui::Key::C, DrawTool::Circle),
                    (egui::Key::P, DrawTool::Polyline),
                    (egui::Key::T, DrawTool::Text),
                    (egui::Key::M, DrawTool::Measure),
                ] {
                    if self.step == Step::Design && i.key_pressed(k) {
                        acts.push(Action::Tool(t));
                    }
                }
            }
            if i.key_pressed(egui::Key::Tab) {
                acts.push(Action::Toggle2D);
            }
            if i.key_pressed(egui::Key::Escape) {
                acts.push(Action::Tool(DrawTool::Select));
            }
        });
        for a in acts {
            if a == Action::Delete && self.selection.is_empty() {
                continue;
            }
            if self.focus.is_some() && matches!(a, Action::Tool(_) | Action::Delete | Action::Toggle2D | Action::Fit | Action::SelectAll) {
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
        if self.dialogs.settings {
            let mut open = true;
            let mut changed = false;
            egui::Window::new("Settings").collapsible(false).open(&mut open).show(ctx, |ui| {
                ui.label(RichText::new("Windows").strong());
                changed |= ui.radio_value(&mut self.prefs.separate_windows, true, "Open modules in separate windows (side by side)").changed();
                changed |= ui.radio_value(&mut self.prefs.separate_windows, false, "Show modules as tabs inside the main window").changed();
                ui.separator();
                ui.label(RichText::new("Drawing").strong());
                changed |= ui.checkbox(&mut self.prefs.snap_grid, "Snap to grid").changed();
                ui.horizontal(|ui| {
                    ui.label("Grid");
                    changed |= ui.add(egui::DragValue::new(&mut self.prefs.grid).range(0.01..=100.0).suffix(" mm")).changed();
                });
                changed |= ui.checkbox(&mut self.prefs.snap_objects, "Snap to shape points and stock edges").changed();
                ui.separator();
                ui.label(RichText::new("Performance").strong());
                ui.add(egui::Slider::new(&mut self.perf.sim_cell_mm, 0.05..=1.0).logarithmic(true).text("simulation cell (mm)"));
                ui.label(RichText::new(ribbon::tip("settings.general")).small().color(theme::STEEL));
            });
            if changed {
                self.prefs.save();
                if !self.prefs.separate_windows {
                    self.open.clear();
                }
            }
            if !open {
                self.dialogs.settings = false;
            }
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
                let before = self.project.vectors.len();
                self.exec(Command::AddText {
                    text: self.dialogs.text.clone(),
                    height: self.dialogs.text_height,
                    x: at.x as f64,
                    y: at.y as f64,
                    font_path: font,
                });
                self.selection = self.project.vectors[before.min(self.project.vectors.len())..].iter().map(|v| v.id).collect();
                self.dialogs.text_at = None;
            } else if !open {
                self.dialogs.text_at = None;
            }
        }
        if let Some((report, clean)) = self.dialogs.merge_report.clone() {
            let mut open = true;
            egui::Window::new(if clean { "✔ Feature updated" } else { "⚠ Feature updated — check the conflicts" })
                .collapsible(false)
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(RichText::new(ribbon::tip("pipeline.merge")).small().color(theme::STEEL));
                    ui.separator();
                    for l in report.lines() {
                        let col = if l.starts_with('⚠') {
                            theme::WARN
                        } else if l.starts_with('✖') {
                            theme::DANGER
                        } else if l.starts_with('✔') {
                            theme::OK
                        } else {
                            ui.visuals().text_color()
                        };
                        ui.colored_label(col, l);
                    }
                    if ui.button("OK").clicked() {
                        self.dialogs.merge_report = None;
                    }
                });
            if !open {
                self.dialogs.merge_report = None;
            }
        }
    }
}

pub fn module_viewport(m: Module) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("ezyx-module", m as u8))
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
            self.step = Step::Toolpaths;
            self.simulate(&ctx, true);
        }
        if let Some(mut d) = self.docs.take() {
            let keep = d.tick(self, &ctx);
            if keep {
                self.docs = Some(d);
            }
        }
        self.machine.poll(&ctx);
        self.ai.poll(&mut self.project, &mut self.history, &mut self.rev);
        if self.results_rev != self.rev && !self.results.is_empty() && self.tasks.running == 0 {
            // Keep toolpaths in sync with edits.
            self.calculate(Some(&ctx));
        }
        self.refresh_conflicts();
        self.shortcuts(&ctx);

        let mut actions = Vec::new();
        egui::Panel::top("topbar").show(ui, |ui| self.top_bar(ui, &mut actions));
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
        if self.show_ai {
            egui::Panel::right("ai").resizable(true).default_size(360.0).show(ui, |ui| panels::ai::show(self, ui));
        }
        match self.focus {
            Some(m) => {
                egui::CentralPanel::default().show(ui, |ui| self.module_ui(m, ui));
            }
            None => pipeline::show(self, ui, &mut actions),
        }
        self.show_module_windows(&ctx);
        if self.tile_in > 0 {
            self.tile_in -= 1;
            if self.tile_in == 0 {
                self.tile_windows(&ctx);
            }
            ctx.request_repaint();
        }
        self.dialogs(&ctx);
        panels::machine::confirm_dialog(self, &ctx);
        panels::ai::confirm_dialog(self, &ctx);
        for a in actions {
            self.handle(a, &ctx);
        }
        if self.tasks.running > 0 || self.machine.connected() || self.ai.busy || self.docs.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}
