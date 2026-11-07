//! Operation editor (Toolpaths step, for hand-made operations or to fine-tune a planned one).

use super::len_field;
use crate::app::EzApp;
use crate::theme;
use egui::{ComboBox, DragValue, RichText, Ui};
use ezyx_cam::ops::{pocket::PocketStyle, profile::ProfileSide, raster3d::RasterDir};
use ezyx_project::{ops::ReliefSource, OpKind};
use ezyx_tooldb::Operation;

pub fn checkpoint_once(app: &mut EzApp, tag: String) {
    if app.history.log.last() != Some(&tag) {
        app.history.checkpoint(&app.project, tag);
    }
}

pub fn op_editor(app: &mut EzApp, ui: &mut Ui, id: u64) {
    let lib = app.library.clone();
    let units = app.project.job.display_units;
    let group = ezyx_project::ops::material_group(&app.project);
    let machine = app.project.machine.limits.clone();
    let has_sel = !app.selection.is_empty();
    let selection = app.selection.clone();
    let parts: Vec<(u64, String)> = app.project.parts.iter().map(|p| (p.id, p.part.name.clone())).collect();
    let meshes: Vec<(u64, String)> = app.project.meshes.iter().map(|m| (m.id, m.name.clone())).collect();
    let reliefs: Vec<(u64, String)> = app.project.reliefs.iter().map(|r| (r.id, r.name.clone())).collect();
    let mut changed = false;
    let mut recalc = false;
    let filter = &mut app.tool_filter;
    let Some(op) = app.project.operations.iter_mut().find(|o| o.id == id) else { return };
    ui.horizontal(|ui| {
        ui.heading(op.kind.label());
        changed |= ui.text_edit_singleline(&mut op.name).changed();
    });
    // Tool picker.
    let tool = lib.get(&op.tool_id).cloned();
    ui.label(RichText::new("Tool").strong());
    ComboBox::from_id_salt("toolpick").width(290.0).selected_text(tool.as_ref().map(|t| t.name.clone()).unwrap_or_else(|| "— choose —".into())).show_ui(
        ui,
        |ui| {
            ui.text_edit_singleline(filter).on_hover_text("Filter, e.g. 'v 60', 'ball', '3.175'");
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for t in lib.search(filter).into_iter().take(150) {
                    if ui.selectable_label(t.id == op.tool_id, format!("{} — {}", t.name, t.shape.label())).clicked() {
                        op.tool_id = t.id.clone();
                        op.cutting_override = None;
                        changed = true;
                    }
                }
            });
        },
    );
    if let Some(t) = &tool {
        ui.label(RichText::new(format!("{} · Ø{} · {} flute(s) · {}", t.shape.label(), units.fmt(t.diameter), t.flutes, t.vendor)).small().color(theme::STEEL));
        for n in &t.notes {
            ui.label(RichText::new(format!("ℹ {n}")).small().color(theme::STEEL));
        }
        let base = ezyx_project::ops::resolve_cutting(t, group, op.pass, &machine);
        let mut over = op.cutting_override.is_some();
        ui.horizontal(|ui| {
            changed |= ui.selectable_value(&mut op.pass, Operation::Roughing, "Roughing").changed();
            changed |= ui.selectable_value(&mut op.pass, Operation::Finishing, "Finishing").changed();
            if ui.checkbox(&mut over, "Custom feeds").changed() {
                op.cutting_override = if over { Some(base.clone()) } else { None };
                changed = true;
            }
        });
        match &mut op.cutting_override {
            Some(c) => {
                egui::Grid::new("feeds").num_columns(2).show(ui, |ui| {
                    ui.label("Spindle");
                    changed |= ui.add(DragValue::new(&mut c.rpm).range(0.0..=60000.0).suffix(" rpm")).changed();
                    ui.end_row();
                    ui.label("Feed");
                    changed |= ui.add(DragValue::new(&mut c.feed).range(1.0..=20000.0).suffix(" mm/min")).changed();
                    ui.end_row();
                    ui.label("Plunge");
                    changed |= ui.add(DragValue::new(&mut c.plunge_feed).range(1.0..=10000.0).suffix(" mm/min")).changed();
                    ui.end_row();
                    ui.label("Depth/pass");
                    changed |= len_field(ui, &mut c.stepdown, units, 0.01..=50.0).changed();
                    ui.end_row();
                    ui.label("Stepover");
                    changed |= len_field(ui, &mut c.stepover, units, 0.005..=50.0).changed();
                    ui.end_row();
                });
            }
            None => {
                ui.label(RichText::new(format!(
                    "{:.0} rpm · {:.0} mm/min · plunge {:.0} · {} /pass · stepover {}",
                    base.rpm,
                    base.feed,
                    base.plunge_feed,
                    units.fmt(base.stepdown),
                    units.fmt(base.stepover)
                )));
                ui.label(RichText::new(&base.source).small().color(theme::STEEL));
            }
        }
    }
    ui.separator();
    egui::Grid::new("depths").num_columns(2).show(ui, |ui| {
        ui.label("Start depth");
        changed |= len_field(ui, &mut op.start_depth, units, 0.0..=500.0).changed();
        ui.end_row();
        ui.label("Cut depth");
        changed |= len_field(ui, &mut op.cut_depth, units, 0.01..=500.0).changed();
        ui.end_row();
    });
    ui.horizontal(|ui| {
        ui.label(if op.vector_ids.is_empty() { "Vectors: all".to_string() } else { format!("Vectors: {}", op.vector_ids.len()) });
        if ui.add_enabled(has_sel, egui::Button::new("Use selection")).clicked() {
            op.vector_ids = selection.clone();
            changed = true;
        }
        if ui.button("All").clicked() {
            op.vector_ids.clear();
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label("Direction");
        changed |= ui.selectable_value(&mut op.direction, ezyx_cam::CutDirection::Climb, "Climb").changed();
        changed |= ui.selectable_value(&mut op.direction, ezyx_cam::CutDirection::Conventional, "Conventional").changed();
    });
    ui.separator();
    match &mut op.kind {
        OpKind::Profile(o) => {
            ui.horizontal(|ui| {
                changed |= ui.selectable_value(&mut o.side, ProfileSide::Outside, "Outside").changed();
                changed |= ui.selectable_value(&mut o.side, ProfileSide::Inside, "Inside").changed();
                changed |= ui.selectable_value(&mut o.side, ProfileSide::On, "On").changed();
            });
            egui::Grid::new("prof").num_columns(2).show(ui, |ui| {
                ui.label("Allowance");
                changed |= len_field(ui, &mut o.allowance, units, -10.0..=10.0).changed();
                ui.end_row();
                ui.label("Ramp length");
                changed |= len_field(ui, &mut o.ramp_length, units, 0.0..=200.0).changed();
                ui.end_row();
                ui.label("Finish pass");
                changed |= len_field(ui, &mut o.finish_pass, units, 0.0..=5.0).changed();
                ui.end_row();
            });
            let mut tabs = o.tabs.is_some();
            if ui.checkbox(&mut tabs, "Tabs (hold the part)").changed() {
                o.tabs = tabs.then(Default::default);
                changed = true;
            }
            if let Some(t) = &mut o.tabs {
                egui::Grid::new("tabs").num_columns(2).show(ui, |ui| {
                    ui.label("Count");
                    changed |= ui.add(DragValue::new(&mut t.count_per_loop).range(1..=32)).changed();
                    ui.end_row();
                    ui.label("Length");
                    changed |= len_field(ui, &mut t.length, units, 0.5..=100.0).changed();
                    ui.end_row();
                    ui.label("Height");
                    changed |= len_field(ui, &mut t.height, units, 0.2..=50.0).changed();
                    ui.end_row();
                });
            }
        }
        OpKind::Pocket(o) => {
            ui.horizontal(|ui| {
                changed |= ui.selectable_value(&mut o.style, PocketStyle::Offset, "Offset").changed();
                changed |= ui.selectable_value(&mut o.style, PocketStyle::Raster, "Raster").changed();
            });
            if o.style == PocketStyle::Raster {
                changed |= ui.add(egui::Slider::new(&mut o.raster_angle, 0.0..=180.0).text("angle °")).changed();
                changed |= ui.checkbox(&mut o.boundary_pass, "Boundary pass").changed();
            }
            ui.horizontal(|ui| {
                ui.label("Allowance");
                changed |= len_field(ui, &mut o.allowance, units, 0.0..=10.0).changed();
            });
        }
        OpKind::VCarve { flat_depth, step, clearance_tool_id } => {
            let mut flat = flat_depth.is_some();
            if ui.checkbox(&mut flat, "Flat depth (limit depth, clear floor)").changed() {
                *flat_depth = flat.then_some(3.0);
                changed = true;
            }
            if let Some(fd) = flat_depth {
                ui.horizontal(|ui| {
                    ui.label("Flat depth");
                    changed |= len_field(ui, fd, units, 0.05..=50.0).changed();
                });
                let cur = clearance_tool_id.clone().unwrap_or_default();
                ComboBox::from_label("Clearance tool").selected_text(if cur.is_empty() { "V-bit only".to_string() } else { cur.clone() }).show_ui(ui, |ui| {
                    if ui.selectable_label(cur.is_empty(), "V-bit only").clicked() {
                        *clearance_tool_id = None;
                        changed = true;
                    }
                    for t in lib.tools.iter().filter(|t| matches!(t.shape, ezyx_tooldb::ToolShape::FlatEnd)).take(80) {
                        if ui.selectable_label(cur == t.id, &t.name).clicked() {
                            *clearance_tool_id = Some(t.id.clone());
                            changed = true;
                        }
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.label("Resolution");
                changed |= len_field(ui, step, units, 0.02..=2.0).on_hover_text("Smaller = smoother centre, longer runtime").changed();
            });
        }
        OpKind::Drill(o) => {
            egui::Grid::new("drill").num_columns(2).show(ui, |ui| {
                ui.label("Peck");
                changed |= len_field(ui, &mut o.peck, units, 0.0..=50.0).changed();
                ui.end_row();
                ui.label("Dwell");
                changed |= ui.add(DragValue::new(&mut o.dwell).range(0.0..=10.0).suffix(" s")).changed();
                ui.end_row();
            });
        }
        OpKind::Engrave => {
            ui.label("Follows the selected vectors at the cut depth.");
        }
        OpKind::Raster3d { source, options, resolution } => {
            let label = match source {
                ReliefSource::AllReliefs => "All reliefs".to_string(),
                ReliefSource::Relief(i) => format!("Relief #{i}"),
                ReliefSource::Part(i) => format!("Part #{i}"),
                ReliefSource::Mesh(i) => format!("Mesh #{i}"),
            };
            ComboBox::from_label("Model").selected_text(label).show_ui(ui, |ui| {
                if ui.selectable_label(*source == ReliefSource::AllReliefs, "All reliefs").clicked() {
                    *source = ReliefSource::AllReliefs;
                    changed = true;
                }
                for (i, n) in &reliefs {
                    if ui.selectable_label(*source == ReliefSource::Relief(*i), format!("Relief #{i} {n}")).clicked() {
                        *source = ReliefSource::Relief(*i);
                        changed = true;
                    }
                }
                for (i, n) in &parts {
                    if ui.selectable_label(*source == ReliefSource::Part(*i), format!("Part #{i} {n}")).clicked() {
                        *source = ReliefSource::Part(*i);
                        changed = true;
                    }
                }
                for (i, n) in &meshes {
                    if ui.selectable_label(*source == ReliefSource::Mesh(*i), format!("Mesh #{i} {n}")).clicked() {
                        *source = ReliefSource::Mesh(*i);
                        changed = true;
                    }
                }
            });
            changed |= ui.checkbox(&mut options.roughing, "Roughing (Z-levels)").changed();
            egui::Grid::new("r3d").num_columns(2).show(ui, |ui| {
                ui.label("Allowance");
                changed |= len_field(ui, &mut options.allowance, units, 0.0..=5.0).changed();
                ui.end_row();
                ui.label("Model resolution");
                changed |= len_field(ui, resolution, units, 0.03..=5.0).changed();
                ui.end_row();
            });
            ui.horizontal(|ui| {
                changed |= ui.selectable_value(&mut options.direction, RasterDir::X, "Along X").changed();
                changed |= ui.selectable_value(&mut options.direction, RasterDir::Y, "Along Y").changed();
                changed |= ui.selectable_value(&mut options.direction, RasterDir::XY, "Cross").changed();
            });
        }
        OpKind::Paths3d { paths, .. } => {
            ui.label(format!("{} grooves from the photo V-carve.", paths.len()));
        }
        OpKind::Laser { power, passes, focus_z } => {
            changed |= ui.add(egui::Slider::new(power, 0.0..=1.0).text("power")).changed();
            changed |= ui.add(egui::Slider::new(passes, 1..=20).text("passes")).changed();
            ui.horizontal(|ui| {
                ui.label("Focus Z");
                changed |= len_field(ui, focus_z, units, -50.0..=50.0).changed();
            });
        }
    }
    ui.separator();
    if let Some(r) = app.results.iter().find(|r| r.op_id == id) {
        if let Some(e) = &r.error {
            ui.colored_label(theme::DANGER, e);
        }
        let st: Vec<_> = r.toolpaths.iter().map(|t| t.stats(machine.max_feed)).collect();
        let secs: f64 = st.iter().map(|s| s.est_seconds).sum();
        let cut: f64 = st.iter().map(|s| s.cut_length).sum();
        ui.label(format!("≈ {:.1} min · {:.1} m of cutting", secs / 60.0, cut / 1000.0));
        for a in &r.advice {
            let c = match a.severity {
                ezyx_sim::Severity::Info => theme::STEEL,
                ezyx_sim::Severity::Warning => theme::WARN,
                ezyx_sim::Severity::Danger => theme::DANGER,
            };
            ui.colored_label(c, &a.message);
        }
    }
    ui.horizontal(|ui| {
        if ui.button("⚡ Recalculate").clicked() {
            recalc = true;
        }
        if ui.button("Close").clicked() {
            app.selected_op = None;
        }
    });
    if changed {
        checkpoint_once(app, format!("edit op #{id}"));
        // Editing a planned operation makes it hand-tuned: the planner stops regenerating that
        // pass for the feature (and won't overwrite the edit).
        if let Some(o) = app.project.operations.iter_mut().find(|o| o.id == id && o.auto) {
            o.auto = false;
            let (fid, pass) = (o.feature_id, o.pass);
            if let Some(f) = app.project.features.iter_mut().find(|f| Some(f.id) == fid) {
                match pass {
                    Operation::Roughing => f.plan.hand_rough = true,
                    Operation::Finishing => f.plan.hand_finish = true,
                }
            }
        }
        app.touch();
    }
    if recalc {
        let ctx = ui.ctx().clone();
        app.calculate(Some(&ctx));
    }
}
