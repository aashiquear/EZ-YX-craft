//! **Solid Modeler** — a separate, SolidWorks-style module window for designing parts.
//!
//! Layout: CommandManager tabs on top (Sketch · Features · Evaluate · Export), the FeatureManager
//! tree / Parameters on the left, the 3D view (or the sketch canvas while sketching) in the middle
//! and the PropertyManager on the right. Parts live in `Project::parts`; every committed edit goes
//! through `Command::SetPart` (undo + revision bump). The finished part is sent to the pipeline
//! as a 2.5D relief or a 3D model feature, and re-sending updates that feature (merge report).

mod props;
mod sketch_ui;

use crate::app::EzApp;
use crate::ribbon::{big_button, group, pill, strip, tip};
use crate::tasks::TaskResult;
use crate::theme;
use crate::viewport::{self, Camera, Draw, Projector, ViewPreset};
use egui::{RichText, Stroke, Ui};
use ezyx_cad::{BooleanOp, EdgeTreatment, EndCondition, Feature, HoleKind, LoftSection, MassProperties, Modifier, Part, Shape, Sketch};
use ezyx_core::DVec2;
use ezyx_mesh::TriMesh;
use ezyx_project::pipeline::{FeatureSource, Payload};
use ezyx_project::Command;
use glam::Vec3;

pub use sketch_ui::SketchEdit;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolidTab {
    Sketch,
    Features,
    Evaluate,
    Export,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeftTab {
    Tree,
    Parameters,
}

pub enum SolidResult {
    Mesh { hash: u64, mesh: TriMesh },
    Mass { hash: u64, props: MassProperties },
    Interference { hash: u64, other: u64, volume: f64 },
}

pub struct Modeler {
    pub part_id: Option<u64>,
    /// Working copy, committed with `Command::SetPart` when an edit gesture ends.
    pub draft: Option<Part>,
    pub sel: Option<usize>,
    pub tab: SolidTab,
    pub left: LeftTab,
    pub cam: Camera,
    pub resolution: f64,
    mesh: Option<(u64, TriMesh)>,
    building: Option<u64>,
    pub sketch: Option<SketchEdit>,
    pub mass: Option<(u64, MassProperties)>,
    mass_pending: Option<u64>,
    pub interference: Option<(u64, u64, f64)>,
    pub interference_with: Option<u64>,
    pub section_z: f64,
    /// Pipeline feature made from this part (Send becomes Update).
    pub target_feature: Option<u64>,
    pub as_relief: bool,
    pub status: String,
    pub json_edit: Option<String>,
    /// The draft has edits not yet committed to the project.
    dirty: bool,
}

impl Default for Modeler {
    fn default() -> Self {
        let mut cam = Camera { ortho: false, ..Default::default() };
        cam.preset(ViewPreset::Iso);
        cam.fit(Vec3::splat(-50.0), Vec3::splat(50.0));
        Self {
            part_id: None,
            draft: None,
            sel: None,
            tab: SolidTab::Features,
            left: LeftTab::Tree,
            cam,
            resolution: 0.8,
            mesh: None,
            building: None,
            sketch: None,
            mass: None,
            mass_pending: None,
            interference: None,
            interference_with: None,
            section_z: 0.0,
            target_feature: None,
            as_relief: false,
            status: String::new(),
            json_edit: None,
            dirty: false,
        }
    }
}

impl Modeler {
    pub fn edit_feature(&mut self, feature: u64, part_id: u64, as_relief: bool) {
        self.part_id = Some(part_id);
        self.draft = None;
        self.target_feature = Some(feature);
        self.as_relief = as_relief;
        self.sel = None;
        self.sketch = None;
        self.tab = SolidTab::Features;
    }

    /// Drop the cached display mesh (rebuilt within the current GPU budget).
    pub fn clear_display(&mut self) {
        self.mesh = None;
        self.building = None;
    }

    pub fn on_result(&mut self, r: SolidResult) {
        match r {
            SolidResult::Mesh { hash, mesh } => {
                if self.building == Some(hash) {
                    self.building = None;
                }
                let first = self.mesh.is_none();
                if !mesh.is_empty() && first {
                    let (lo, hi) = mesh.bounds();
                    self.cam.fit(lo, hi);
                }
                self.mesh = Some((hash, mesh));
            }
            SolidResult::Mass { hash, props } => {
                self.mass_pending = None;
                self.mass = Some((hash, props));
            }
            SolidResult::Interference { hash, other, volume } => self.interference = Some((hash, other, volume)),
        }
    }
}

/// Ensure the modeler has a part to work on and a fresh draft of it.
fn sync_draft(app: &mut EzApp) {
    let m = &mut app.solid;
    if m.part_id.map(|id| !app.project.parts.iter().any(|p| p.id == id)).unwrap_or(true) {
        m.part_id = app.project.parts.first().map(|p| p.id);
        m.draft = None;
        m.sel = None;
    }
    let Some(id) = m.part_id else {
        m.draft = None;
        return;
    };
    let current = app.project.parts.iter().find(|p| p.id == id).map(|p| p.part.clone());
    // Pick up external changes (undo/redo, assistant) when no edit is in progress.
    if m.draft.is_none() || (!m.dirty && m.draft.as_ref() != current.as_ref()) {
        m.draft = current;
    }
}

/// Commit the draft to the project if it changed (one undo step per gesture).
fn commit(app: &mut EzApp, ui: &Ui) {
    let (Some(id), Some(draft)) = (app.solid.part_id, app.solid.draft.clone()) else { return };
    if !app.solid.dirty {
        return;
    }
    let current = app.project.parts.iter().find(|p| p.id == id).map(|p| &p.part);
    if current == Some(&draft) {
        app.solid.dirty = false;
        return;
    }
    // Wait for drags / typing to finish so one gesture is one undo step.
    if ui.input(|i| i.pointer.any_down())
        || ui.ctx().egui_wants_keyboard_input() && ui.ctx().memory(|m| m.focused().is_some()) && !ui.input(|i| i.key_pressed(egui::Key::Enter))
    {
        return;
    }
    app.exec(Command::SetPart { id, part: draft });
    app.solid.dirty = false;
}

pub fn new_part(app: &mut EzApp, part: Part) {
    if app.exec(Command::AddPart { part }) {
        app.solid.part_id = app.project.parts.last().map(|p| p.id);
        app.solid.draft = None;
        app.solid.sel = None;
        app.solid.mesh = None;
        app.solid.target_feature = None;
        app.solid.sketch = None;
    }
}

pub fn show(app: &mut EzApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    sync_draft(app);
    let before = app.solid.draft.clone();
    let before_id = app.solid.part_id;
    egui::Panel::top("solid_cmd").show(ui, |ui| command_manager(app, ui));
    egui::Panel::bottom("solid_status").show(ui, |ui| {
        ui.horizontal(|ui| {
            if app.solid.building.is_some() {
                ui.spinner();
            }
            let txt = if app.solid.status.is_empty() { "Ready" } else { app.solid.status.as_str() };
            ui.label(txt);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::Slider::new(&mut app.solid.resolution, 0.15..=3.0).logarithmic(true).text("preview mm"));
            });
        });
    });
    egui::Panel::left("solid_tree").resizable(true).default_size(250.0).show(ui, |ui| {
        ui.horizontal(|ui| {
            if pill(ui, "🌲 Features", app.solid.left == LeftTab::Tree, "FeatureManager design tree") {
                app.solid.left = LeftTab::Tree;
            }
            if pill(ui, "ƒ Parameters", app.solid.left == LeftTab::Parameters, &tip("solid.parameters")) {
                app.solid.left = LeftTab::Parameters;
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| match app.solid.left {
            LeftTab::Tree => props::tree(app, ui),
            LeftTab::Parameters => props::parameters(app, ui),
        });
    });
    egui::Panel::right("solid_props").resizable(true).default_size(330.0).show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            if app.solid.sketch.is_some() {
                sketch_ui::props(app, ui);
            } else {
                match app.solid.tab {
                    SolidTab::Evaluate => evaluate(app, ui),
                    SolidTab::Export => export_panel(app, ui),
                    _ => props::property_manager(app, ui),
                }
            }
        });
    });
    egui::CentralPanel::no_frame().show(ui, |ui| {
        if app.solid.sketch.is_some() {
            sketch_ui::canvas(app, ui);
        } else {
            viewer(app, ui);
        }
    });
    if before.is_some() && app.solid.part_id == before_id && app.solid.draft != before {
        app.solid.dirty = true;
    }
    commit(app, ui);
    ensure_mesh(app, &ctx);
}

fn ensure_mesh(app: &mut EzApp, ctx: &egui::Context) {
    let Some(part) = app.solid.draft.clone() else { return };
    let hash = part.revision_hash() ^ (app.solid.resolution * 1000.0) as u64;
    let fresh = app.solid.mesh.as_ref().map(|(h, _)| *h == hash).unwrap_or(false);
    if fresh || app.solid.building.is_some() {
        return;
    }
    app.solid.building = Some(hash);
    let res = app.solid.resolution;
    // Fine previews of big parts are simplified to the GPU budget (exports use full resolution).
    let max_tris = app.budget.mesh_triangles;
    app.tasks.spawn(ctx, move || TaskResult::Solid(SolidResult::Mesh { hash, mesh: part.mesh(res).decimated(max_tris) }));
}

fn command_manager(app: &mut EzApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("🧊 Solid Modeler").strong().size(16.0).color(theme::ACCENT));
        for (t, l) in [(SolidTab::Sketch, "Sketch"), (SolidTab::Features, "Features"), (SolidTab::Evaluate, "Evaluate"), (SolidTab::Export, "Export")] {
            if pill(ui, l, app.solid.tab == t, "") {
                app.solid.tab = t;
            }
        }
        ui.separator();
        // Part picker.
        let parts: Vec<(u64, String)> = app.project.parts.iter().map(|p| (p.id, p.part.name.clone())).collect();
        let cur = app.solid.part_id.and_then(|id| parts.iter().find(|p| p.0 == id)).map(|p| p.1.clone()).unwrap_or_else(|| "— no part —".into());
        egui::ComboBox::from_id_salt("solid_part").selected_text(cur).show_ui(ui, |ui| {
            for (id, n) in &parts {
                if ui.selectable_label(app.solid.part_id == Some(*id), n).clicked() {
                    app.solid.part_id = Some(*id);
                    app.solid.draft = None;
                    app.solid.sel = None;
                    app.solid.target_feature =
                        app.project.features.iter().find(|f| matches!(f.source, FeatureSource::SolidModeler { part_id, .. } if part_id == *id)).map(|f| f.id);
                }
            }
        });
        ui.menu_button("🗋 New part", |ui| {
            ui.label(RichText::new(tip("solid.templates")).small().color(theme::STEEL));
            if ui.button("Empty part (base box)").clicked() {
                new_part(
                    app,
                    Part::new("Part").add(Feature::new("Base", BooleanOp::Add, Shape::Box { size: [60.0, 40.0, 10.0], corner_radius: 2.0 }).at(0.0, 0.0, 5.0)),
                );
                ui.close();
            }
            if ui.button("From a sketch").clicked() {
                new_part(app, Part::new("Part"));
                app.solid.sketch = Some(SketchEdit::new(None, Sketch::rectangle(60.0, 40.0)));
                ui.close();
            }
            ui.separator();
            for (name, p) in ezyx_cad::templates() {
                if ui.button(name).clicked() {
                    new_part(app, p);
                    ui.close();
                }
            }
        });
        if let Some(t) = app.solid.target_feature.and_then(|id| app.project.features.iter().find(|f| f.id == id)) {
            ui.label(RichText::new(format!("→ pipeline feature '{}'", t.name)).color(theme::STEEL));
        }
    });
    let mut out_feature: Option<Feature> = None;
    let mut finish: Option<Modifier> = None;
    strip(ui, "solid_strip", |ui| match app.solid.tab {
        SolidTab::Sketch => {
            group(ui, "Sketch", |ui| {
                if big_button(ui, "✏", "New sketch", &tip("solid.sketch"), false) {
                    app.solid.sketch = Some(SketchEdit::new(None, Sketch::new(ezyx_cad::SketchPlane::XY)));
                }
                if big_button(ui, "▭", "Rect sketch", "Start from a fully-defined rectangle sketch", false) {
                    app.solid.sketch = Some(SketchEdit::new(None, Sketch::rectangle(60.0, 40.0)));
                }
                if big_button(ui, "◯", "Circle sketch", "Start from a dimensioned circle sketch", false) {
                    app.solid.sketch = Some(SketchEdit::new(None, Sketch::circle(20.0)));
                }
                let editable = app
                    .solid
                    .sel
                    .and_then(|i| app.solid.draft.as_ref().and_then(|p| p.features.get(i)))
                    .map(|f| matches!(f.shape, Shape::SketchExtrude { .. } | Shape::SketchRevolve { .. }))
                    .unwrap_or(false);
                if editable && big_button(ui, "✎", "Edit sketch", "Edit the selected feature's sketch", false) {
                    let i = app.solid.sel.unwrap_or(0);
                    if let Some(sk) = app.solid.draft.as_mut().and_then(|p| p.features.get_mut(i)).and_then(|f| f.shape.sketch_mut()).cloned() {
                        app.solid.sketch = Some(SketchEdit::new(Some(i), sk));
                    }
                }
            });
            ui.label(RichText::new("Draw on a plane, constrain and dimension it, then finish as an extrude, cut or revolve.").small().color(theme::STEEL));
        }
        SolidTab::Features => {
            group(ui, "Sketch based", |ui| {
                let t = tip("solid.features");
                if big_button(ui, "⬆", "Extrude", &t, false) {
                    app.solid.sketch = Some(SketchEdit::new(None, Sketch::rectangle(40.0, 30.0)));
                }
                if big_button(ui, "🔄", "Revolve", &t, false) {
                    let mut sk = Sketch::polygon(&[[0.0, 0.0], [15.0, 0.0], [12.0, 10.0], [5.0, 16.0], [0.0, 17.0]]);
                    sk.plane = ezyx_cad::SketchPlane::XZ;
                    let mut e = SketchEdit::new(None, sk);
                    e.finish_as_revolve = true;
                    app.solid.sketch = Some(e);
                }
                if big_button(ui, "➰", "Sweep", &t, false) {
                    out_feature = Some(Feature::new(
                        "Sweep",
                        BooleanOp::Add,
                        Shape::Sweep {
                            profile: vec![ezyx_core::path::shapes::circle(DVec2::ZERO, 3.0)],
                            path: vec![[-30.0, 0.0, 5.0], [0.0, 0.0, 5.0], [0.0, 30.0, 5.0], [0.0, 30.0, 25.0]],
                            twist_deg: 0.0,
                        },
                    ));
                }
                if big_button(ui, "🏺", "Loft", &t, false) {
                    let sq = |s: f64| vec![ezyx_core::path::shapes::rectangle(DVec2::splat(-s / 2.0), s, s, 0.0)];
                    out_feature = Some(Feature::new(
                        "Loft",
                        BooleanOp::Add,
                        Shape::Loft { sections: vec![LoftSection { profile: sq(40.0), z: 0.0 }, LoftSection { profile: sq(20.0), z: 30.0 }] },
                    ));
                }
            });
            group(ui, "Primitives", |ui| {
                for (icon, l, s) in [
                    ("⬛", "Box", Shape::Box { size: [30.0, 30.0, 15.0], corner_radius: 1.0 }),
                    ("🛢", "Cylinder", Shape::Cylinder { radius: 10.0, height: 20.0 }),
                    ("⚪", "Sphere", Shape::Sphere { radius: 12.0 }),
                    ("🔻", "Cone", Shape::Cone { r_bottom: 12.0, r_top: 4.0, height: 20.0 }),
                    ("🍩", "Torus", Shape::Torus { major: 15.0, minor: 4.0 }),
                ] {
                    if big_button(ui, icon, l, "Add a primitive body", false) {
                        out_feature = Some(Feature::new(l, BooleanOp::Add, s));
                    }
                }
            });
            group(ui, "Holes & threads", |ui| {
                let t = tip("solid.holes");
                let top = app.solid.draft.as_ref().and_then(|p| p.sdf()).map(|s| ezyx_cad::sdf::top_z(&s)).unwrap_or(10.0);
                if big_button(ui, "⚲", "Hole", &t, false) {
                    let mut f = Feature::new(
                        "Hole",
                        BooleanOp::Cut,
                        Shape::Hole { kind: HoleKind::Counterbore { diameter: 10.0, depth: 3.0 }, diameter: 5.5, depth: 30.0 },
                    );
                    f.placement.translate = [0.0, 0.0, top];
                    out_feature = Some(f);
                }
                if big_button(ui, "🔩", "Thread", &t, false) {
                    out_feature = Some(Feature::new(
                        "Thread",
                        BooleanOp::Add,
                        Shape::Thread { major_diameter: 10.0, pitch: 1.5, length: 20.0, internal: false, depth: 0.0 },
                    ));
                }
            });
            group(ui, "Dress-up", |ui| {
                let t = tip("solid.dress");
                if big_button(ui, "◜", "Fillet all", &t, false) {
                    finish = Some(Modifier::Fillet { radius: 1.0 });
                }
                if big_button(ui, "▢", "Shell", &t, false) {
                    finish = Some(Modifier::ShellOpen { thickness: 2.0, open_top: true });
                }
            });
            group(ui, "Bodies", |ui| {
                let t = tip("solid.bodies");
                if big_button(ui, "✂", "Split", &t, false) {
                    finish = Some(Modifier::Split { plane: ezyx_cad::MirrorPlane::XY, offset: 0.0, keep_positive: true });
                }
                if big_button(ui, "⇔", "Mirror", &tip("solid.patterns"), false) {
                    finish = Some(Modifier::Mirror { plane: ezyx_cad::MirrorPlane::YZ });
                }
                let others: Vec<(u64, Part)> = app.project.parts.iter().filter(|p| Some(p.id) != app.solid.part_id).map(|p| (p.id, p.part.clone())).collect();
                ui.menu_button("⊕ Combine…", |ui| {
                    ui.label(RichText::new(tip("solid.bodies")).small().color(theme::STEEL));
                    if others.is_empty() {
                        ui.label("Create another part first.");
                    }
                    for (_, p) in &others {
                        for (l, op) in [("Add", BooleanOp::Add), ("Subtract", BooleanOp::Cut), ("Common", BooleanOp::Intersect)] {
                            if ui.button(format!("{l} '{}'", p.name)).clicked() {
                                out_feature = Some(Feature::new(format!("Combine {}", p.name), op, Shape::Body { part: Box::new(p.clone()) }));
                                ui.close();
                            }
                        }
                    }
                });
            });
        }
        SolidTab::Evaluate => {
            group(ui, "Evaluate", |ui| {
                if big_button(ui, "⚖", "Mass", &tip("solid.evaluate"), false) {
                    app.solid.mass = None;
                }
                for (icon, l, v) in
                    [("⬇", "Top", ViewPreset::Top), ("⬆", "Front", ViewPreset::Front), ("➡", "Right", ViewPreset::Right), ("◩", "Iso", ViewPreset::Iso)]
                {
                    if big_button(ui, icon, l, "View", false) {
                        app.solid.cam.preset(v);
                    }
                }
            });
        }
        SolidTab::Export => {
            group(ui, "To the pipeline", |ui| {
                let t = tip("solid.export");
                if big_button(ui, "⛰", "As 2.5D", &t, false) {
                    send_to_pipeline(app, true, false);
                }
                if big_button(ui, "🧊", "As 3D", &t, false) {
                    send_to_pipeline(app, false, false);
                }
                if app.solid.target_feature.is_some() && big_button(ui, "⟳", "Update", "Update the pipeline feature made from this part (merge report)", false)
                {
                    let r = app.solid.as_relief;
                    send_to_pipeline(app, r, true);
                }
            });
            group(ui, "Files", |ui| {
                if big_button(ui, "💾", "STL", "Export STL", false) {
                    export_mesh(app, "stl");
                }
                if big_button(ui, "💾", "OBJ", "Export OBJ", false) {
                    export_mesh(app, "obj");
                }
                if big_button(ui, "🔷", "Tet mesh", "Volumetric tet mesh (VTK + Gmsh) for analysis", false) {
                    export_tet(app);
                }
                if big_button(ui, "🔗", "SolidCraft", &tip("solid.solidcraft"), false) {
                    open_in_solidcraft(app);
                }
            });
        }
    });
    if let Some(f) = out_feature {
        add_feature(app, f);
    }
    if let Some(m) = finish {
        if let Some(p) = app.solid.draft.as_mut() {
            p.finish.retain(|x| std::mem::discriminant(x) != std::mem::discriminant(&m));
            p.finish.push(m);
            app.solid.sel = None;
        }
    }
}

pub fn add_feature(app: &mut EzApp, f: Feature) {
    if app.solid.draft.is_none() {
        new_part(app, Part::new("Part"));
        sync_draft(app);
    }
    if let Some(p) = app.solid.draft.as_mut() {
        p.features.push(f);
        app.solid.sel = Some(p.features.len() - 1);
    }
}

fn viewer(app: &mut EzApp, ui: &mut Ui) {
    let (resp, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
    let rect = resp.rect;
    painter.rect_filled(rect, 0.0, theme::viewport_bg(app.prefs.dark));
    viewport::navigate(ui, &resp, &mut app.solid.cam, rect, false);
    let pr = Projector::new(&app.solid.cam, rect);
    let d = Draw { painter: &painter, pr: &pr };
    // Ground grid on XY.
    let gcol = if app.prefs.dark { egui::Color32::from_gray(70) } else { egui::Color32::from_rgba_unmultiplied(0, 0, 0, 28) };
    for k in -10..=10 {
        let v = k as f32 * 10.0;
        d.line(Vec3::new(v, -100.0, 0.0), Vec3::new(v, 100.0, 0.0), Stroke::new(if k == 0 { 1.2 } else { 0.5 }, gcol));
        d.line(Vec3::new(-100.0, v, 0.0), Vec3::new(100.0, v, 0.0), Stroke::new(if k == 0 { 1.2 } else { 0.5 }, gcol));
    }
    if let Some((_, m)) = &app.solid.mesh {
        d.mesh(m, [0.72, 0.76, 0.84], 255, true);
    }
    // Selected feature's placement origin.
    if let Some(f) = app.solid.sel.and_then(|i| app.solid.draft.as_ref().and_then(|p| p.features.get(i))) {
        let t = f.placement.translate;
        let o = Vec3::new(t[0] as f32, t[1] as f32, t[2] as f32);
        for (ax, c) in
            [(Vec3::X, egui::Color32::from_rgb(220, 60, 60)), (Vec3::Y, egui::Color32::from_rgb(60, 170, 70)), (Vec3::Z, egui::Color32::from_rgb(60, 110, 230))]
        {
            d.line(o, o + ax * 12.0, Stroke::new(2.0, c));
        }
    }
    d.gizmo();
    if app.solid.draft.is_none() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "🗋 New part → start from a template, an empty part or a sketch",
            egui::FontId::proportional(16.0),
            theme::STEEL,
        );
    } else if let Some((_, m)) = &app.solid.mesh {
        let (lo, hi) = m.bounds();
        let s = hi - lo;
        painter.text(
            rect.left_top() + egui::vec2(10.0, 8.0),
            egui::Align2::LEFT_TOP,
            format!("{:.1} × {:.1} × {:.1} mm · {} triangles", s.x, s.y, s.z, m.indices.len()),
            egui::FontId::monospace(12.0),
            theme::STEEL,
        );
    }
    painter.text(
        rect.right_bottom() + egui::vec2(-10.0, -8.0),
        egui::Align2::RIGHT_BOTTOM,
        "LMB orbit · RMB/MMB pan · wheel zoom",
        egui::FontId::proportional(11.0),
        theme::STEEL,
    );
}

fn evaluate(app: &mut EzApp, ui: &mut Ui) {
    ui.heading("Evaluate");
    ui.label(RichText::new(tip("solid.evaluate")).small().color(theme::STEEL));
    ui.separator();
    let Some(part) = app.solid.draft.clone() else { return };
    let hash = part.revision_hash();
    let mat = ezyx_materials::find(&app.project.job.material);
    let density = mat.as_ref().map(|m| m.density / 1000.0).unwrap_or(1.0);
    ui.label(format!("Material: {} ({density:.2} g/cm³) — set in the pipeline's ① Stock", mat.map(|m| m.name).unwrap_or_default()));
    let fresh = app.solid.mass.as_ref().map(|(h, _)| *h == hash).unwrap_or(false);
    if !fresh && app.solid.mass_pending != Some(hash) {
        app.solid.mass_pending = Some(hash);
        let res = app.solid.resolution.min(0.8);
        let p = part.clone();
        app.tasks.spawn(ui.ctx(), move || TaskResult::Solid(SolidResult::Mass { hash, props: p.mass_properties(res, density) }));
    }
    match &app.solid.mass {
        Some((h, m)) if *h == hash => {
            egui::Grid::new("mass").num_columns(2).striped(true).show(ui, |ui| {
                ui.label("Volume");
                ui.label(format!("{:.1} cm³", m.volume_mm3 / 1000.0));
                ui.end_row();
                ui.label("Surface area");
                ui.label(format!("{:.1} cm²", m.area_mm2 / 100.0));
                ui.end_row();
                ui.label("Mass");
                ui.label(format!("{:.1} g", m.mass_g));
                ui.end_row();
                ui.label("Centre of mass");
                ui.label(format!("({:.2}, {:.2}, {:.2}) mm", m.centroid[0], m.centroid[1], m.centroid[2]));
                ui.end_row();
                ui.label("Bounding box");
                ui.label(format!("{:.1} × {:.1} × {:.1} mm", m.bbox_max[0] - m.bbox_min[0], m.bbox_max[1] - m.bbox_min[1], m.bbox_max[2] - m.bbox_min[2]));
                ui.end_row();
            });
        }
        _ => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Computing mass properties…");
            });
        }
    }
    ui.separator();
    ui.label(RichText::new("Section").strong());
    ui.add(egui::Slider::new(&mut app.solid.section_z, -50.0..=100.0).text("Z mm"));
    let a = part.section_area_at_z(app.solid.section_z, 0.5);
    ui.label(format!("Section area at Z {:.1}: {:.1} mm²", app.solid.section_z, a));
    ui.separator();
    ui.label(RichText::new("Interference").strong());
    let others: Vec<(u64, String)> = app.project.parts.iter().filter(|p| Some(p.id) != app.solid.part_id).map(|p| (p.id, p.part.name.clone())).collect();
    egui::ComboBox::from_id_salt("interf")
        .selected_text(
            app.solid.interference_with.and_then(|id| others.iter().find(|o| o.0 == id)).map(|o| o.1.clone()).unwrap_or_else(|| "pick a part".into()),
        )
        .show_ui(ui, |ui| {
            for (id, n) in &others {
                if ui.selectable_label(app.solid.interference_with == Some(*id), n).clicked() {
                    app.solid.interference_with = Some(*id);
                }
            }
        });
    if let Some(other) = app.solid.interference_with {
        if ui.button("Check interference").clicked() {
            if let Some(op) = app.project.parts.iter().find(|p| p.id == other).map(|p| p.part.clone()) {
                let p = part.clone();
                app.tasks
                    .spawn(ui.ctx(), move || TaskResult::Solid(SolidResult::Interference { hash, other, volume: ezyx_cad::interference_volume(&p, &op, 0.5) }));
            }
        }
        if let Some((h, o, v)) = app.solid.interference {
            if h == hash && o == other {
                if v > 0.01 {
                    ui.colored_label(theme::WARN, format!("Parts overlap by {v:.1} mm³"));
                } else {
                    ui.colored_label(theme::OK, "No interference");
                }
            }
        }
    }
}

fn export_panel(app: &mut EzApp, ui: &mut Ui) {
    ui.heading("Export");
    ui.label(RichText::new(tip("solid.export")).small().color(theme::STEEL));
    ui.separator();
    let Some(part) = app.solid.draft.as_ref() else {
        ui.label("Create a part first.");
        return;
    };
    let bounds = part.sdf().map(|s| s.bounds());
    if let Some((lo, hi)) = bounds {
        let s = hi - lo;
        let j = &app.project.job;
        ui.label(format!("Part {:.1} × {:.1} × {:.1} mm · stock {:.0} × {:.0} × {:.1} mm", s.x, s.y, s.z, j.width, j.height, j.thickness));
        if s.z > j.thickness {
            ui.colored_label(theme::WARN, "Taller than the stock is thick — the pipeline will flag it (choose thicker stock or scale the part).");
        }
        if s.x > j.width || s.y > j.height {
            ui.colored_label(theme::WARN, "Larger than the stock outline.");
        }
    }
    ui.add_space(6.0);
    ui.label(RichText::new("2.5D relief").strong());
    ui.label(RichText::new("Top-down height field — what a 3-axis router reaches from above. Undercuts become vertical walls.").small().color(theme::STEEL));
    if ui.button("⛰ Send as 2.5D relief").clicked() {
        send_to_pipeline(app, true, false);
    }
    ui.add_space(4.0);
    ui.label(RichText::new("3D model").strong());
    ui.label(
        RichText::new("The mesh is carved with 3D roughing + finishing (also top-down on a 3-axis machine; on rotary jobs it wraps).")
            .small()
            .color(theme::STEEL),
    );
    if ui.button("🧊 Send as 3D model").clicked() {
        send_to_pipeline(app, false, false);
    }
    if let Some(f) = app.solid.target_feature.and_then(|id| app.project.features.iter().find(|f| f.id == id)) {
        ui.add_space(6.0);
        ui.label(RichText::new(format!("Linked to pipeline feature '{}' (rev {}).", f.name, f.revision)).strong());
        let r = app.solid.as_relief;
        if ui
            .add(egui::Button::new(RichText::new("⟳ Update pipeline feature").strong()).fill(theme::ACCENT_SOFT))
            .on_hover_text(tip("pipeline.merge"))
            .clicked()
        {
            send_to_pipeline(app, r, true);
        }
    }
    ui.separator();
    ui.label(RichText::new("SolidCraft bridge").strong());
    ui.label(RichText::new(tip("solid.solidcraft")).small().color(theme::STEEL));
    let found = solidcraft_path();
    ui.label(match &found {
        Some(p) => format!("Found: {}", p.display()),
        None => "solidcraft is not on your PATH (https://github.com/tangency-labs/solidcraft)".into(),
    });
}

fn part_payload(part: &Part, as_relief: bool, thickness: f64, res: f64) -> Result<Payload, String> {
    let s = part.sdf().ok_or("the part is empty")?;
    let (lo, hi) = s.bounds();
    if as_relief {
        let height = (hi.z - lo.z).min(thickness).max(0.1);
        let cell = (res * 0.5).clamp(0.1, 0.5);
        Ok(Payload::Relief { heightmap: part.heightmap(cell, -height) })
    } else {
        let mesh = part.mesh(res.min(0.6));
        if mesh.is_empty() {
            return Err("the part has no surface".into());
        }
        Ok(Payload::Mesh { mesh })
    }
}

fn send_to_pipeline(app: &mut EzApp, as_relief: bool, update: bool) {
    // Commit pending edits first so the feature records the right revision.
    if let (Some(id), Some(d)) = (app.solid.part_id, app.solid.draft.clone()) {
        if app.project.parts.iter().find(|p| p.id == id).map(|p| p.part != d).unwrap_or(false) {
            app.exec(Command::SetPart { id, part: d });
        }
        app.solid.dirty = false;
    }
    let Some(po) = app.solid.part_id.and_then(|id| app.project.parts.iter().find(|p| p.id == id)).cloned() else {
        app.solid.status = "Create a part first.".into();
        return;
    };
    let payload = match part_payload(&po.part, as_relief, app.project.job.thickness, app.solid.resolution) {
        Ok(p) => p,
        Err(e) => {
            app.solid.status = format!("⚠ {e}");
            return;
        }
    };
    let source = FeatureSource::SolidModeler { part_id: po.id, part_revision: po.revision, as_relief };
    let target = app.solid.target_feature.filter(|id| app.project.features.iter().any(|f| f.id == *id));
    let ok = match (update, target) {
        (true, Some(id)) | (false, Some(id)) if update || app.solid.as_relief == as_relief => {
            app.exec(Command::ReimportFeature { id, payload, source: Some(source) })
        }
        _ => {
            let ok = app.exec(Command::ImportFeature { name: po.part.name.clone(), payload, source });
            if ok {
                app.solid.target_feature = app.project.features.last().map(|f| f.id);
                app.selected_feature = app.solid.target_feature;
            }
            ok
        }
    };
    if ok {
        app.solid.as_relief = as_relief;
        app.solid.status = format!("Sent '{}' to the pipeline as {}", po.part.name, if as_relief { "a 2.5D relief" } else { "a 3D model" });
        if app.step == crate::ribbon::Step::Stock {
            app.step = crate::ribbon::Step::Design;
        }
    }
}

fn export_mesh(app: &mut EzApp, ext: &str) {
    let Some(part) = app.solid.draft.clone() else { return };
    let mesh = part.mesh(app.solid.resolution.min(0.4));
    if let Some(path) = rfd::FileDialog::new().add_filter(ext.to_uppercase(), &[ext]).set_file_name(format!("{}.{ext}", part.name)).save_file() {
        let r = if ext == "obj" { ezyx_mesh::io::write_obj(&mesh, &path) } else { ezyx_mesh::io::write_stl(&mesh, &path) };
        app.solid.status = match r {
            Ok(()) => format!("Exported {}", path.display()),
            Err(e) => format!("⚠ {e}"),
        };
    }
}

fn export_tet(app: &mut EzApp) {
    let Some(part) = app.solid.draft.clone() else { return };
    let tm = part.tet_mesh(app.solid.resolution.max(0.8));
    let q = tm.quality();
    let report = format!(
        "{} tetrahedra · {:.1} mm³ · dihedral {:.1}°–{:.1}° · radius ratio min {:.2}",
        q.count, q.volume, q.min_dihedral, q.max_dihedral, q.min_radius_ratio
    );
    if let Some(path) = rfd::FileDialog::new().add_filter("VTK", &["vtk"]).set_file_name(format!("{}.vtk", part.name)).save_file() {
        let _ = std::fs::write(&path, tm.to_vtk());
        let _ = std::fs::write(path.with_extension("msh"), tm.to_gmsh());
        app.solid.status = format!("{report} — saved {} and .msh", path.display());
    } else {
        app.solid.status = report;
    }
}

fn solidcraft_path() -> Option<std::path::PathBuf> {
    let exe = if cfg!(windows) { "solidcraft.exe" } else { "solidcraft" };
    std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|d| d.join(exe)).find(|p| p.is_file()))
}

fn open_in_solidcraft(app: &mut EzApp) {
    let Some(part) = app.solid.draft.clone() else { return };
    let Some(exe) = solidcraft_path() else {
        app.solid.status =
            "SolidCraft is not installed (not on PATH). Export STL and open it there, then import its STL/OBJ back with Design → STL/OBJ.".into();
        return;
    };
    let path = std::env::temp_dir().join(format!("ezyx-{}.stl", part.name.replace(' ', "_")));
    match ezyx_mesh::io::write_stl(&part.mesh(app.solid.resolution.min(0.4)), &path) {
        Ok(()) => match std::process::Command::new(exe).arg(&path).spawn() {
            Ok(_) => app.solid.status = format!("Opened {} in SolidCraft — export STL/OBJ there and import it back in the pipeline.", path.display()),
            Err(e) => app.solid.status = format!("⚠ could not start SolidCraft: {e}"),
        },
        Err(e) => app.solid.status = format!("⚠ {e}"),
    }
}

/// Sketch profiles for a feature created from a finished sketch.
pub fn finish_sketch(app: &mut EzApp, edit: SketchEdit, revolve: bool, op: BooleanOp, depth: f64) {
    let SketchEdit { feature, sketch, .. } = edit;
    if let Some(i) = feature {
        if let Some(sk) = app.solid.draft.as_mut().and_then(|p| p.features.get_mut(i)).and_then(|f| f.shape.sketch_mut()) {
            *sk = sketch;
        }
        return;
    }
    let shape = if revolve {
        Shape::SketchRevolve { sketch, angle_deg: 360.0 }
    } else {
        Shape::SketchExtrude {
            sketch,
            depth,
            end: if op == BooleanOp::Cut { EndCondition::ThroughAll } else { EndCondition::Blind },
            draft_deg: 0.0,
            top_edge: EdgeTreatment::None,
            bottom_edge: EdgeTreatment::None,
            reverse: false,
        }
    };
    let name = match (revolve, op) {
        (true, _) => "Revolve",
        (false, BooleanOp::Cut) => "Cut-extrude",
        _ => "Boss-extrude",
    };
    add_feature(app, Feature::new(name, op, shape));
}
