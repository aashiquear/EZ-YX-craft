//! Right panel: context-sensitive properties.

use super::len_field;
use crate::app::EzApp;
use crate::ribbon::Tab;
use crate::theme;
use egui::{ComboBox, DragValue, RichText, Ui};
use ezyx_cad::{BooleanOp, Modifier, Shape};
use ezyx_cam::ops::{pocket::PocketStyle, profile::ProfileSide, raster3d::RasterDir};
use ezyx_core::{JobKind, Units, XyDatum, ZZero};
use ezyx_project::{commands::Command, ops::ReliefSource, OpKind};
use ezyx_tooldb::Operation;

fn checkpoint_once(app: &mut EzApp, tag: String) {
    if app.history.log.last() != Some(&tag) {
        app.history.checkpoint(&app.project, tag);
    }
}

pub fn show(app: &mut EzApp, ui: &mut Ui) {
    if app.tab == Tab::Simulate {
        preview_panel(app, ui);
        ui.separator();
    }
    if app.tab == Tab::Model3D {
        if let Some(id) = app.selected_part {
            part_editor(app, ui, id);
            return;
        }
        ui.label("Create a part with New Part / Demo, or extrude selected vectors.");
        return;
    }
    if let Some(id) = app.selected_op {
        if app.project.operations.iter().any(|o| o.id == id) {
            op_editor(app, ui, id);
            return;
        }
        app.selected_op = None;
    }
    if !app.selection.is_empty() {
        selection_panel(app, ui);
        return;
    }
    job_setup(app, ui);
    ui.separator();
    getting_started(app, ui);
}

fn getting_started(app: &mut EzApp, ui: &mut Ui) {
    ui.label(RichText::new("Getting started").strong());
    let p = &app.project;
    let steps = [
        ("1", "Set stock size & material (above)", true),
        ("2", "Design: draw, import SVG/DXF, trace an image", !p.vectors.is_empty() || !p.reliefs.is_empty() || !p.meshes.is_empty() || !p.parts.is_empty()),
        ("3", "Add toolpaths (Toolpaths tab)", !p.operations.is_empty()),
        ("4", "Preview the finished piece", app.preview.is_some()),
        ("5", "Save G-code or send via USB / Wi-Fi", false),
    ];
    for (n, s, done) in steps {
        ui.horizontal(|ui| {
            ui.label(RichText::new(if done { "✔" } else { n }).color(if done { theme::OK } else { theme::ACCENT }).strong());
            ui.label(s);
        });
    }
}

fn job_setup(app: &mut EzApp, ui: &mut Ui) {
    ui.heading("Job setup");
    let mut changed = false;
    let units = app.project.job.display_units;
    ui.horizontal(|ui| {
        ui.label("Units");
        changed |= ui.selectable_value(&mut app.project.job.display_units, Units::Millimeters, "mm").changed();
        changed |= ui.selectable_value(&mut app.project.job.display_units, Units::Inches, "inch").changed();
    });
    ui.horizontal(|ui| {
        let flat = !app.project.job.is_rotary();
        if ui.selectable_label(flat, "▭ Flat").clicked() && !flat {
            app.exec(Command::SetFlat);
        }
        if ui.selectable_label(!flat, "🔄 Rotary").on_hover_text("4th-axis job: design is the unwrapped surface").clicked() && flat {
            app.exec(Command::SetRotary { diameter: 50.0, length: 200.0 });
            app.fit_view();
        }
    });
    egui::Grid::new("job").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        let j = &mut app.project.job;
        if let JobKind::Rotary { diameter, length, .. } = &mut j.kind {
            ui.label("Diameter");
            if len_field(ui, diameter, units, 1.0..=1000.0).changed() {
                j.height = std::f64::consts::PI * *diameter;
                j.thickness = *diameter * 0.5;
                changed = true;
            }
            ui.end_row();
            ui.label("Length");
            if len_field(ui, length, units, 1.0..=3000.0).changed() {
                j.width = *length;
                changed = true;
            }
            ui.end_row();
        } else {
            ui.label("Width (X)");
            changed |= len_field(ui, &mut j.width, units, 1.0..=5000.0).changed();
            ui.end_row();
            ui.label("Height (Y)");
            changed |= len_field(ui, &mut j.height, units, 1.0..=5000.0).changed();
            ui.end_row();
            ui.label("Thickness (Z)");
            changed |= len_field(ui, &mut j.thickness, units, 0.1..=500.0).changed();
            ui.end_row();
        }
        ui.label("Safe Z");
        changed |= len_field(ui, &mut j.safe_z, units, 0.5..=100.0).changed();
        ui.end_row();
        ui.label("Z zero");
        ui.horizontal(|ui| {
            changed |= ui.selectable_value(&mut j.z_zero, ZZero::MaterialSurface, "Top").changed();
            changed |= ui.selectable_value(&mut j.z_zero, ZZero::MachineBed, "Bed").changed();
        });
        ui.end_row();
        ui.label("XY zero");
        egui::Grid::new("datum").spacing([2.0, 2.0]).show(ui, |ui| {
            for (i, d) in XyDatum::ALL.iter().enumerate() {
                let sel = j.xy_datum == *d;
                if ui.add(egui::Button::new(if sel { "●" } else { "○" }).small()).clicked() {
                    j.xy_datum = *d;
                    changed = true;
                }
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
        });
        ui.end_row();
    });
    ui.add_space(4.0);
    let cur_mat = app.project.job.material.clone();
    let mat_name = ezyx_materials::find(&cur_mat).map(|m| m.name).unwrap_or(cur_mat.clone());
    ComboBox::from_label("Material").selected_text(mat_name).width(200.0).show_ui(ui, |ui| {
        for m in ezyx_materials::library() {
            if ui.selectable_label(m.id == cur_mat, format!("{} ({:?})", m.name, m.group)).on_hover_text(&m.notes).clicked() {
                app.project.job.material = m.id.clone();
                changed = true;
            }
        }
    });
    let cur_machine = app.project.machine.id.clone();
    let mut set_machine = None;
    ComboBox::from_label("Machine").selected_text(app.project.machine.name.clone()).width(200.0).show_ui(ui, |ui| {
        for m in ezyx_project::machines::builtin() {
            if ui.selectable_label(m.id == cur_machine, &m.name).clicked() {
                set_machine = Some(m.id.clone());
            }
        }
    });
    if let Some(id) = set_machine {
        app.exec(Command::SetMachine { id });
    }
    if app.project.machine.verify {
        ui.label(RichText::new("Profile values are typical — connect the machine to read its real travel ($130–$132).").small().color(theme::WARN));
    }
    let cur_post = app.project.post_id.clone();
    let mut set_post = None;
    ComboBox::from_label("Post-processor").selected_text(cur_post.clone()).width(200.0).show_ui(ui, |ui| {
        for p in ezyx_gcode::builtin_posts() {
            if ui.selectable_label(p.id == cur_post, &p.name).on_hover_text(&p.description).clicked() {
                set_post = Some(p.id.clone());
            }
        }
    });
    if let Some(id) = set_post {
        app.exec(Command::SetPost { id });
    }
    if changed {
        checkpoint_once(app, "edit job".into());
        app.touch();
    }
}

fn selection_panel(app: &mut EzApp, ui: &mut Ui) {
    ui.heading(format!("{} selected", app.selection.len()));
    let b = app.selection.iter().filter_map(|id| app.project.vector(*id)).fold(ezyx_core::Bounds2::EMPTY, |a, v| a.union(&v.path.bounds()));
    let u = app.project.job.display_units;
    ui.label(format!("Size {} × {}", u.fmt(b.size().x), u.fmt(b.size().y)));
    ui.label(format!("At ({}, {})", u.fmt(b.min.x), u.fmt(b.min.y)));
    ui.separator();
    ui.label(RichText::new("Transform").strong());
    let ids = app.selection.clone();
    let mut cmd = None;
    ui.horizontal(|ui| {
        for (lbl, dx, dy) in [("←", -1.0, 0.0), ("→", 1.0, 0.0), ("↑", 0.0, 1.0), ("↓", 0.0, -1.0)] {
            if ui.button(lbl).on_hover_text("Nudge 1 mm (Shift: 10 mm)").clicked() {
                let k = if ui.input(|i| i.modifiers.shift) { 10.0 } else { 1.0 };
                cmd = Some(Command::MoveVectors { ids: ids.clone(), dx: dx * k, dy: dy * k });
            }
        }
        if ui.button("⊕ Center").clicked() {
            cmd = Some(Command::CenterOnStock { ids: ids.clone() });
        }
    });
    ui.horizontal(|ui| {
        for f in [0.5, 0.9, 1.1, 2.0] {
            if ui.button(format!("×{f}")).clicked() {
                cmd = Some(Command::ScaleVectors { ids: ids.clone(), factor: f });
            }
        }
        for r in [-90.0, 90.0] {
            if ui.button(format!("⟲{r}°")).clicked() {
                cmd = Some(Command::RotateVectors { ids: ids.clone(), degrees: r });
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Offset");
        len_field(ui, &mut app.dialogs.offset, u, -200.0..=200.0);
        if ui.button("Apply").clicked() {
            cmd = Some(Command::OffsetVectors { ids: ids.clone(), distance: app.dialogs.offset });
        }
    });
    if let Some(c) = cmd {
        app.exec(c);
    }
    ui.separator();
    ui.label(RichText::new("Make it").strong());
    ui.horizontal_wrapped(|ui| {
        use crate::ribbon::OpTemplate::*;
        for (t, l) in [(ProfileOutside, "Cut out"), (Pocket, "Pocket"), (VCarve, "V-carve"), (Engrave, "Engrave"), (Drill, "Drill")] {
            if ui.button(l).clicked() {
                app.add_op(t);
            }
        }
    });
}

fn op_editor(app: &mut EzApp, ui: &mut Ui, id: u64) {
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
        if ui.button("👁 Preview").clicked() {
            app.tab = Tab::Simulate;
        }
        if ui.button("Close").clicked() {
            app.selected_op = None;
        }
    });
    if changed {
        checkpoint_once(app, format!("edit op #{id}"));
        app.touch();
    }
    if recalc {
        let ctx = ui.ctx().clone();
        app.calculate(Some(&ctx));
    }
}

fn preview_panel(app: &mut EzApp, ui: &mut Ui) {
    ui.heading("Predicted result");
    let mat = ezyx_materials::find(&app.project.job.material);
    ui.label(format!("Material: {}", mat.map(|m| m.name).unwrap_or_default()));
    ui.label(RichText::new(format!("Simulation resolution {:.2} mm (from GPU tier)", app.perf.sim_cell_mm)).small().color(theme::STEEL));
    ui.add(egui::Slider::new(&mut app.perf.sim_cell_mm, 0.05..=1.0).logarithmic(true).text("cell mm"));
    if !app.preview_info.is_empty() {
        ui.label(&app.preview_info);
    }
    let secs: f64 = app.results.iter().flat_map(|r| r.toolpaths.iter()).map(|t| t.stats(app.project.machine.limits.max_feed).est_seconds).sum();
    if secs > 0.0 {
        ui.label(format!("Estimated machining time ≈ {:.0} min", secs / 60.0));
    }
}

fn part_editor(app: &mut EzApp, ui: &mut Ui, id: u64) {
    let units = app.project.job.display_units;
    let mut changed = false;
    let Some(po) = app.project.parts.iter_mut().find(|p| p.id == id) else { return };
    ui.horizontal(|ui| {
        ui.heading("Part");
        changed |= ui.text_edit_singleline(&mut po.part.name).changed();
    });
    ui.label(RichText::new("Feature tree (top to bottom). Cut/Intersect apply to everything above.").small().color(theme::STEEL));
    let mut delete = None;
    let mut move_up = None;
    let n = po.part.features.len();
    for (i, f) in po.part.features.iter_mut().enumerate() {
        egui::CollapsingHeader::new(format!("{} {} {}", if f.suppressed { "⊘" } else { "▸" }, i + 1, f.name))
            .id_salt(("feat", id, i))
            .default_open(i + 1 == n)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    changed |= ui.text_edit_singleline(&mut f.name).changed();
                    changed |= ui.checkbox(&mut f.suppressed, "off").changed();
                });
                ui.horizontal(|ui| {
                    changed |= ui.selectable_value(&mut f.op, BooleanOp::Add, "Add").changed();
                    changed |= ui.selectable_value(&mut f.op, BooleanOp::Cut, "Cut").changed();
                    changed |= ui.selectable_value(&mut f.op, BooleanOp::Intersect, "Intersect").changed();
                });
                egui::Grid::new(("shape", id, i)).num_columns(2).show(ui, |ui| {
                    match &mut f.shape {
                        Shape::Box { size, corner_radius } => {
                            for (k, l) in ["X", "Y", "Z"].iter().enumerate() {
                                ui.label(*l);
                                changed |= len_field(ui, &mut size[k], units, 0.1..=2000.0).changed();
                                ui.end_row();
                            }
                            ui.label("Corner R");
                            changed |= len_field(ui, corner_radius, units, 0.0..=100.0).changed();
                            ui.end_row();
                        }
                        Shape::Sphere { radius } => {
                            ui.label("Radius");
                            changed |= len_field(ui, radius, units, 0.1..=1000.0).changed();
                            ui.end_row();
                        }
                        Shape::Cylinder { radius, height } => {
                            ui.label("Radius");
                            changed |= len_field(ui, radius, units, 0.1..=1000.0).changed();
                            ui.end_row();
                            ui.label("Height");
                            changed |= len_field(ui, height, units, 0.1..=2000.0).changed();
                            ui.end_row();
                        }
                        Shape::Cone { r_bottom, r_top, height } => {
                            ui.label("R bottom");
                            changed |= len_field(ui, r_bottom, units, 0.0..=1000.0).changed();
                            ui.end_row();
                            ui.label("R top");
                            changed |= len_field(ui, r_top, units, 0.0..=1000.0).changed();
                            ui.end_row();
                            ui.label("Height");
                            changed |= len_field(ui, height, units, 0.1..=2000.0).changed();
                            ui.end_row();
                        }
                        Shape::Torus { major, minor } => {
                            ui.label("Major R");
                            changed |= len_field(ui, major, units, 0.1..=1000.0).changed();
                            ui.end_row();
                            ui.label("Minor R");
                            changed |= len_field(ui, minor, units, 0.1..=500.0).changed();
                            ui.end_row();
                        }
                        Shape::Extrude { height, draft_deg, sketch } => {
                            ui.label(format!("Sketch: {} loops", sketch.len()));
                            ui.end_row();
                            ui.label("Height");
                            changed |= len_field(ui, height, units, 0.1..=2000.0).changed();
                            ui.end_row();
                            ui.label("Draft °");
                            changed |= ui.add(DragValue::new(draft_deg).range(-45.0..=45.0)).changed();
                            ui.end_row();
                        }
                        Shape::Revolve { angle_deg, profile } => {
                            ui.label(format!("Profile: {} loops", profile.len()));
                            ui.end_row();
                            ui.label("Angle °");
                            changed |= ui.add(DragValue::new(angle_deg).range(1.0..=360.0)).changed();
                            ui.end_row();
                        }
                        Shape::Relief { base, .. } => {
                            ui.label("Base Z");
                            changed |= len_field(ui, base, units, -500.0..=0.0).changed();
                            ui.end_row();
                        }
                    }
                    ui.label("Position");
                    ui.horizontal(|ui| {
                        for k in 0..3 {
                            changed |= ui.add(DragValue::new(&mut f.placement.translate[k]).speed(0.5).max_decimals(2)).changed();
                        }
                    });
                    ui.end_row();
                    ui.label("Rotation °");
                    ui.horizontal(|ui| {
                        for k in 0..3 {
                            changed |= ui.add(DragValue::new(&mut f.placement.rotate_deg[k]).speed(1.0).max_decimals(1)).changed();
                        }
                    });
                    ui.end_row();
                    ui.label("Blend R");
                    changed |= len_field(ui, &mut f.blend, units, 0.0..=50.0).on_hover_text("Smooth fillet where this feature meets the body").changed();
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    if ui.small_button("+ Linear pattern").clicked() {
                        f.modifiers.push(Modifier::LinearPattern { count: 3, step: [20.0, 0.0, 0.0] });
                        changed = true;
                    }
                    if ui.small_button("+ Circular").clicked() {
                        f.modifiers.push(Modifier::CircularPattern { count: 6, axis_z: true });
                        changed = true;
                    }
                    if ui.small_button("+ Shell").clicked() {
                        f.modifiers.push(Modifier::Shell { thickness: 2.0 });
                        changed = true;
                    }
                });
                let mut rm = None;
                for (k, m) in f.modifiers.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        match m {
                            Modifier::LinearPattern { count, step } => {
                                ui.label("Linear ×");
                                changed |= ui.add(DragValue::new(count).range(1..=100)).changed();
                                ui.label("step");
                                for s in step.iter_mut() {
                                    changed |= ui.add(DragValue::new(s).speed(0.5)).changed();
                                }
                            }
                            Modifier::CircularPattern { count, .. } => {
                                ui.label("Circular ×");
                                changed |= ui.add(DragValue::new(count).range(1..=100)).changed();
                            }
                            Modifier::Shell { thickness } => {
                                ui.label("Shell");
                                changed |= len_field(ui, thickness, units, 0.1..=50.0).changed();
                            }
                            Modifier::Fillet { radius } => {
                                ui.label("Fillet");
                                changed |= len_field(ui, radius, units, 0.0..=50.0).changed();
                            }
                            Modifier::Mirror { .. } => {
                                ui.label("Mirror");
                            }
                        }
                        if ui.small_button("✖").clicked() {
                            rm = Some(k);
                        }
                    });
                }
                if let Some(k) = rm {
                    f.modifiers.remove(k);
                    changed = true;
                }
                ui.horizontal(|ui| {
                    if i > 0 && ui.small_button("⬆ Move up").clicked() {
                        move_up = Some(i);
                    }
                    if ui.small_button("🗑 Delete").clicked() {
                        delete = Some(i);
                    }
                });
            });
    }
    if let Some(i) = move_up {
        po.part.features.swap(i, i - 1);
        changed = true;
    }
    if let Some(i) = delete {
        po.part.features.remove(i);
        changed = true;
    }
    ui.separator();
    let mut fillet = po.part.finish.iter().find_map(|m| if let Modifier::Fillet { radius } = m { Some(*radius) } else { None }).unwrap_or(0.0);
    ui.horizontal(|ui| {
        ui.label("Round all edges");
        if len_field(ui, &mut fillet, units, 0.0..=20.0).changed() {
            po.part.finish.retain(|m| !matches!(m, Modifier::Fillet { .. }));
            if fillet > 0.0 {
                po.part.finish.push(Modifier::Fillet { radius: fillet });
            }
            changed = true;
        }
    });
    if app.part_building.contains(&id) {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Rebuilding preview…");
        });
    }
    if changed {
        checkpoint_once(app, format!("edit part #{id}"));
        app.touch();
    }
}
