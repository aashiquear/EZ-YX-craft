//! Right panel of the pipeline window, one layout per step.

use crate::app::EzApp;
use crate::panels::{inspector, len_field};
use crate::ribbon::{tip, Action, Module, PassFilter, Step};
use crate::theme;
use egui::{ComboBox, RichText, Ui};
use ezyx_core::{JobKind, Units, XyDatum, ZZero};
use ezyx_project::pipeline::{CutStrategy, FeatureKind, FeatureSource, PlanSettings, Severity, StockShape};
use ezyx_project::Command;
use ezyx_tooldb::ToolShape;

pub fn show(app: &mut EzApp, ui: &mut Ui, out: &mut Vec<Action>) {
    match app.step {
        Step::Stock => stock(app, ui),
        Step::Design => design(app, ui, out),
        Step::Toolpaths => toolpaths(app, ui, out),
        Step::Output => output(app, ui, out),
    }
}

fn heading(ui: &mut Ui, title: &str, feature: &str) {
    ui.heading(title);
    ui.label(RichText::new(tip(feature)).small().color(theme::STEEL));
    ui.separator();
}

// ------------------------------------------------------------------ ① stock

fn stock(app: &mut EzApp, ui: &mut Ui) {
    heading(ui, "Stock", "pipeline.stock");
    let mut changed = false;
    let units = app.project.job.display_units;
    ui.label(RichText::new("Material").strong());
    let cur_mat = app.project.job.material.clone();
    let mat = ezyx_materials::find(&cur_mat);
    ui.horizontal(|ui| {
        if let Some(m) = &mat {
            let c = m.appearance.base;
            let col = egui::Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8);
            let (r, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
            ui.painter().rect_filled(r, 4.0, col);
        }
        ComboBox::from_id_salt("material").selected_text(mat.as_ref().map(|m| m.name.clone()).unwrap_or(cur_mat.clone())).width(230.0).show_ui(ui, |ui| {
            for m in ezyx_materials::library() {
                if ui.selectable_label(m.id == cur_mat, format!("{} ({:?})", m.name, m.group)).on_hover_text(&m.notes).clicked() {
                    app.project.job.material = m.id.clone();
                    changed = true;
                }
            }
        });
    });
    if let Some(m) = &mat {
        if !m.notes.is_empty() {
            ui.label(RichText::new(&m.notes).small().color(theme::STEEL));
        }
    }
    ui.add_space(6.0);
    ui.label(RichText::new("Shape").strong());
    let rotary = app.project.job.is_rotary();
    let shape = app.project.stock_shape;
    let mut cmds = Vec::new();
    ui.horizontal(|ui| {
        if ui.selectable_label(!rotary && shape == StockShape::Rectangle, "▭ Board").clicked() {
            if rotary {
                cmds.push(Command::SetFlat);
            }
            cmds.push(Command::SetStockShape { shape: StockShape::Rectangle });
        }
        if ui.selectable_label(!rotary && shape == StockShape::Round, "◯ Round blank").clicked() {
            if rotary {
                cmds.push(Command::SetFlat);
            }
            cmds.push(Command::SetStockShape { shape: StockShape::Round });
        }
        if ui.selectable_label(rotary, "🔄 Rotary").on_hover_text("4th-axis cylinder: the design is its unwrapped surface").clicked() && !rotary {
            cmds.push(Command::SetStockShape { shape: StockShape::Rectangle });
            cmds.push(Command::SetRotary { diameter: 50.0, length: 200.0 });
        }
    });
    for c in cmds {
        app.exec(c);
        app.fit_view();
    }
    egui::Grid::new("job").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        let round = app.project.stock_shape == StockShape::Round;
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
        } else if round {
            ui.label("Diameter");
            if len_field(ui, &mut j.width, units, 1.0..=3000.0).changed() {
                j.height = j.width;
                changed = true;
            }
            ui.end_row();
            ui.label("Thickness (Z)");
            changed |= len_field(ui, &mut j.thickness, units, 0.1..=500.0).changed();
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
        ui.label("Units");
        ui.horizontal(|ui| {
            changed |= ui.selectable_value(&mut j.display_units, Units::Millimeters, "mm").changed();
            changed |= ui.selectable_value(&mut j.display_units, Units::Inches, "inch").changed();
        });
        ui.end_row();
        ui.label("Safe Z");
        changed |= len_field(ui, &mut j.safe_z, units, 0.5..=100.0).changed();
        ui.end_row();
        ui.label("Z zero");
        ui.horizontal(|ui| {
            changed |= ui.selectable_value(&mut j.z_zero, ZZero::MaterialSurface, "Stock top").changed();
            changed |= ui.selectable_value(&mut j.z_zero, ZZero::MachineBed, "Bed").changed();
        });
        ui.end_row();
        ui.label("XY zero");
        egui::Grid::new("datum").spacing([2.0, 2.0]).show(ui, |ui| {
            for (i, d) in XyDatum::ALL.iter().enumerate() {
                let sel = j.xy_datum == *d;
                if ui.add(egui::Button::new(if sel { "●" } else { "○" }).small()).on_hover_text(format!("{d:?}")).clicked() {
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
    ui.add_space(6.0);
    ui.label(RichText::new("Machine").strong());
    let cur_machine = app.project.machine.id.clone();
    let mut set_machine = None;
    ComboBox::from_id_salt("machine").selected_text(app.project.machine.name.clone()).width(260.0).show_ui(ui, |ui| {
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
    post_picker(app, ui);
    let travel = app.project.machine.travel;
    let j = &app.project.job;
    if travel[0] > 0.0 && (j.width > travel[0] || j.height > travel[1]) {
        ui.colored_label(
            theme::WARN,
            format!("Stock is larger than the machine's travel ({:.0} × {:.0} mm) — tile the job or use smaller stock.", travel[0], travel[1]),
        );
    }
    if changed {
        inspector::checkpoint_once(app, "edit stock".into());
        app.touch();
    }
}

fn post_picker(app: &mut EzApp, ui: &mut Ui) {
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
}

// ------------------------------------------------------------------ ② design

fn design(app: &mut EzApp, ui: &mut Ui, out: &mut Vec<Action>) {
    if !app.selection.is_empty() {
        selection(app, ui, out);
        ui.separator();
    }
    if let Some(id) = app.selected_feature {
        if app.project.features.iter().any(|f| f.id == id) {
            feature_props(app, ui, id, out);
            ui.separator();
        } else {
            app.selected_feature = None;
        }
    }
    if app.selection.is_empty() && app.selected_feature.is_none() {
        getting_started(app, ui, out);
        ui.separator();
    }
    layers(app, ui);
    ui.separator();
    ui.label(RichText::new("Snapping").strong()).on_hover_text(tip("pipeline.snap"));
    let mut ch = false;
    ui.horizontal(|ui| {
        ch |= ui.checkbox(&mut app.prefs.snap_grid, "Grid").changed();
        ch |= ui.add(egui::DragValue::new(&mut app.prefs.grid).range(0.01..=100.0).speed(0.1).suffix(" mm")).changed();
        ch |= ui.checkbox(&mut app.prefs.snap_objects, "Objects").changed();
    });
    ui.label(RichText::new("Hold Alt while drawing to snap off.").small().color(theme::STEEL));
    if ch {
        app.prefs.save();
    }
}

fn getting_started(app: &mut EzApp, ui: &mut Ui, out: &mut Vec<Action>) {
    ui.heading("Design");
    ui.label(RichText::new(tip("pipeline.overview")).small().color(theme::STEEL));
    ui.add_space(4.0);
    let p = &app.project;
    let steps = [
        ("1", "Stock chosen (① Stock)", true),
        ("2", "Draw shapes, import SVG/DXF/STL, or use the Image Studio / Solid Modeler", !p.vectors.is_empty() || !p.features.is_empty()),
        ("3", "Select shapes and press a Carve button to make a feature", !p.features.is_empty()),
        ("4", "Check the automatic toolpaths (③ Toolpaths)", !app.results.is_empty()),
        ("5", "Download G-code or send it to Machine Control (④ Output)", false),
    ];
    for (n, s, done) in steps {
        ui.horizontal(|ui| {
            ui.label(RichText::new(if done { "✔" } else { n }).color(if done { theme::OK } else { theme::ACCENT }).strong());
            ui.label(s);
        });
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button("🖼 Image Studio").clicked() {
            out.push(Action::OpenModule(Module::Image));
        }
        if ui.button("🧊 Solid Modeler").clicked() {
            out.push(Action::OpenModule(Module::Solid));
        }
    });
}

fn selection(app: &mut EzApp, ui: &mut Ui, out: &mut Vec<Action>) {
    ui.heading(format!("{} shape(s) selected", app.selection.len()));
    let ids = app.selection.clone();
    let b = ids.iter().filter_map(|id| app.project.vector(*id)).fold(ezyx_core::Bounds2::EMPTY, |a, v| a.union(&v.path.bounds()));
    if b.is_empty() {
        return;
    }
    let u = app.project.job.display_units;
    let mut cmd = None;
    // Exact position and size (VCarve-style transform panel).
    let (mut x, mut y, mut w, mut h) = (b.min.x, b.min.y, b.size().x, b.size().y);
    let mut lock = ui.ctx().data(|d| d.get_temp::<bool>(egui::Id::unique("lock_aspect")).unwrap_or(true));
    egui::Grid::new("xf").num_columns(4).spacing([6.0, 4.0]).show(ui, |ui| {
        ui.label("X");
        let rx = len_field(ui, &mut x, u, -10000.0..=10000.0);
        ui.label("Y");
        let ry = len_field(ui, &mut y, u, -10000.0..=10000.0);
        ui.end_row();
        ui.label("W");
        let rw = len_field(ui, &mut w, u, 0.01..=10000.0);
        ui.label("H");
        let rh = len_field(ui, &mut h, u, 0.01..=10000.0);
        ui.end_row();
        if (rx.lost_focus() || rx.drag_stopped()) && (x - b.min.x).abs() > 1e-9 || (ry.lost_focus() || ry.drag_stopped()) && (y - b.min.y).abs() > 1e-9 {
            cmd = Some(Command::MoveVectors { ids: ids.clone(), dx: x - b.min.x, dy: y - b.min.y });
        }
        let done_w = (rw.lost_focus() || rw.drag_stopped()) && (w - b.size().x).abs() > 1e-9;
        let done_h = (rh.lost_focus() || rh.drag_stopped()) && (h - b.size().y).abs() > 1e-9;
        if done_w || done_h {
            let (mut sx, mut sy) = (w / b.size().x.max(1e-9), h / b.size().y.max(1e-9));
            if lock {
                let s = if done_w { sx } else { sy };
                (sx, sy) = (s, s);
            }
            cmd = Some(Command::TransformVectors { ids: ids.clone(), matrix: [sx, 0.0, 0.0, sy, 0.0, 0.0], about_center: true });
        }
    });
    if ui.checkbox(&mut lock, "Keep proportions").changed() {
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("lock_aspect"), lock));
    }
    ui.horizontal(|ui| {
        for (lbl, dx, dy) in [("←", -1.0, 0.0), ("→", 1.0, 0.0), ("↑", 0.0, 1.0), ("↓", 0.0, -1.0)] {
            if ui.button(lbl).on_hover_text("Nudge 1 mm (Shift: 10 mm)").clicked() {
                let k = if ui.input(|i| i.modifiers.shift) { 10.0 } else { 1.0 };
                cmd = Some(Command::MoveVectors { ids: ids.clone(), dx: dx * k, dy: dy * k });
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
    if app.project.layers.len() > 1 {
        let cur = app.project.vector(ids[0]).map(|v| v.layer).unwrap_or(0);
        ComboBox::from_label("Layer").selected_text(app.project.layers.get(cur).map(|l| l.name.clone()).unwrap_or_default()).show_ui(ui, |ui| {
            for (i, l) in app.project.layers.iter().enumerate() {
                if ui.selectable_label(i == cur, &l.name).clicked() {
                    cmd = Some(Command::SetVectorLayer { ids: ids.clone(), layer: i });
                }
            }
        });
    }
    if let Some(c) = cmd {
        app.exec(c);
    }
    ui.add_space(4.0);
    ui.label(RichText::new("Carve it").strong()).on_hover_text(tip("pipeline.carve"));
    let paths = app.project.paths_for(&ids);
    let suggested = ezyx_project::pipeline::suggest_strategy(&paths, &app.project.job);
    ui.horizontal_wrapped(|ui| {
        for s in CutStrategy::ALL {
            let label = if s == suggested { format!("★ {}", s.label()) } else { s.label().to_string() };
            if ui.button(label).on_hover_text(if s == suggested { "Suggested for these shapes" } else { "" }).clicked() {
                out.push(Action::Carve(s));
            }
        }
    });
}

fn feature_props(app: &mut EzApp, ui: &mut Ui, id: u64, out: &mut Vec<Action>) {
    let Some(f) = app.project.features.iter().find(|f| f.id == id).cloned() else { return };
    ui.horizontal(|ui| {
        ui.label(RichText::new(f.kind.icon()).size(18.0));
        ui.heading(&f.name);
    });
    ui.label(RichText::new(format!("{} · revision {} · {}", f.kind.label(), f.revision, f.source.label())).small().color(theme::STEEL));
    let u = app.project.job.display_units;
    let mut name = f.name.clone();
    let mut cmd = None;
    ui.horizontal(|ui| {
        ui.label("Name");
        if ui.text_edit_singleline(&mut name).lost_focus() && name != f.name {
            cmd = Some(Command::UpdateFeature { id, name: Some(name.clone()), strategy: None, depth: None, plan: None, suppressed: None });
        }
    });
    if let FeatureKind::Cut2D { strategy, depth, vector_ids } = &f.kind {
        ComboBox::from_label("Strategy").selected_text(strategy.label()).show_ui(ui, |ui| {
            for s in CutStrategy::ALL {
                if ui.selectable_label(s == *strategy, s.label()).clicked() && s != *strategy {
                    let d = s.default_depth(app.project.job.thickness);
                    cmd = Some(Command::UpdateFeature { id, name: None, strategy: Some(s), depth: Some(d), plan: None, suppressed: None });
                }
            }
        });
        let mut d = *depth;
        ui.horizontal(|ui| {
            ui.label("Depth");
            let r = len_field(ui, &mut d, u, 0.01..=500.0);
            if (r.lost_focus() || r.drag_stopped()) && (d - depth).abs() > 1e-9 {
                cmd = Some(Command::UpdateFeature { id, name: None, strategy: None, depth: Some(d), plan: None, suppressed: None });
            }
            if ui.small_button("= stock").on_hover_text("Cut through the whole stock").clicked() {
                cmd = Some(Command::UpdateFeature { id, name: None, strategy: None, depth: Some(app.project.job.thickness), plan: None, suppressed: None });
            }
        });
        ui.label(RichText::new(format!("{} vectors", vector_ids.len())).small().color(theme::STEEL));
    }
    if let Some((b, depth)) = ezyx_project::pipeline::feature_extent(&app.project, &f) {
        ui.label(
            RichText::new(format!(
                "Footprint {} × {} at ({}, {}) · depth {}",
                u.fmt(b.size().x),
                u.fmt(b.size().y),
                u.fmt(b.min.x),
                u.fmt(b.min.y),
                u.fmt(depth)
            ))
            .small(),
        );
    }
    for c in app.conflicts.iter().filter(|c| c.features.contains(&id)) {
        let col = match c.severity {
            Severity::Error => theme::DANGER,
            Severity::Warning => theme::WARN,
            Severity::Info => theme::STEEL,
        };
        ui.colored_label(col, &c.message);
    }
    ui.horizontal(|ui| {
        if f.source.editable_elsewhere() {
            let lbl = if matches!(f.source, FeatureSource::ImageStudio { .. }) { "✏ Edit in Image Studio" } else { "✏ Edit in Solid Modeler" };
            if ui.button(lbl).on_hover_text(tip("pipeline.merge")).clicked() {
                out.push(Action::EditFeature(id));
            }
        }
        if ui.button("🛠 Toolpaths →").clicked() {
            out.push(Action::GoTo(Step::Toolpaths));
        }
    });
    if let Some(c) = cmd {
        app.exec(c);
    }
}

fn layers(app: &mut EzApp, ui: &mut Ui) {
    ui.label(RichText::new("Layers").strong()).on_hover_text(tip("pipeline.edit"));
    let mut touched = false;
    let counts: Vec<usize> = (0..app.project.layers.len()).map(|i| app.project.vectors.iter().filter(|v| v.layer == i).count()).collect();
    for (i, l) in app.project.layers.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            touched |= ui.checkbox(&mut l.visible, "").on_hover_text("Visible").changed();
            let icon = if l.locked { "🔒" } else { "🔓" };
            touched |= ui.toggle_value(&mut l.locked, icon).on_hover_text("Locked layers can't be selected").changed();
            ui.label(format!("{} ({})", l.name, counts[i]));
        });
    }
    if touched {
        app.history.checkpoint(&app.project, "layers");
        app.touch();
        let hidden: Vec<usize> = app.project.layers.iter().enumerate().filter(|(_, l)| !l.visible || l.locked).map(|(i, _)| i).collect();
        let vecs = &app.project.vectors;
        app.selection.retain(|id| vecs.iter().find(|v| v.id == *id).map(|v| !hidden.contains(&v.layer)).unwrap_or(false));
    }
    ui.horizontal(|ui| {
        let id = egui::Id::unique("new_layer");
        let mut name = ui.ctx().data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| format!("Layer {}", app.project.layers.len() + 1));
        ui.add(egui::TextEdit::singleline(&mut name).desired_width(120.0));
        if ui.small_button("➕ Add layer").clicked() {
            app.exec(Command::AddLayer { name: name.clone() });
            name = format!("Layer {}", app.project.layers.len() + 1);
        }
        ui.ctx().data_mut(|d| d.insert_temp(id, name));
    });
}

// ------------------------------------------------------------------ ③ toolpaths

fn tool_combo(
    ui: &mut Ui,
    app_lib: &ezyx_tooldb::ToolLibrary,
    id: &str,
    current: &Option<String>,
    auto_name: &str,
    filter_shape: fn(&ToolShape) -> bool,
    filter: &mut String,
) -> Option<Option<String>> {
    let mut pick = None;
    let txt = match current {
        Some(t) => app_lib.get(t).map(|t| t.name.clone()).unwrap_or(t.clone()),
        None => format!("Auto: {auto_name}"),
    };
    ComboBox::from_id_salt(id).width(250.0).selected_text(txt).show_ui(ui, |ui| {
        ui.text_edit_singleline(filter).on_hover_text("Filter, e.g. '1/8', 'ball', 'v 60', '3.175'");
        if ui.selectable_label(current.is_none(), "🪄 Auto (recommended)").clicked() {
            pick = Some(None);
        }
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            for t in app_lib.search(filter).into_iter().filter(|t| filter_shape(&t.shape)).take(150) {
                if ui.selectable_label(current.as_deref() == Some(t.id.as_str()), format!("{} — {} Ø{:.2}", t.name, t.shape.label(), t.diameter)).clicked() {
                    pick = Some(Some(t.id.clone()));
                }
            }
        });
    });
    pick
}

fn toolpaths(app: &mut EzApp, ui: &mut Ui, out: &mut Vec<Action>) {
    if let Some(id) = app.selected_op {
        if app.project.operations.iter().any(|o| o.id == id) {
            let auto = app.project.operations.iter().find(|o| o.id == id).map(|o| o.auto).unwrap_or(false);
            if auto {
                ui.label(
                    RichText::new("Planned automatically. Changing it here makes it a hand-tuned operation that re-planning won't overwrite.")
                        .small()
                        .color(theme::STEEL),
                );
            }
            inspector::op_editor(app, ui, id);
            if ui.button("← Back to the feature plan").clicked() {
                app.selected_op = None;
            }
            ui.separator();
            preview_info(app, ui);
            return;
        }
        app.selected_op = None;
    }
    if let Some(fid) = app.selected_feature.filter(|id| app.project.features.iter().any(|f| f.id == *id)) {
        plan_editor(app, ui, fid);
        ui.separator();
    } else {
        heading(ui, "Toolpaths", "pipeline.toolpaths");
        ui.label("Select a feature on the left to choose its bits, or an operation to fine-tune it.");
        if app.project.features.is_empty() {
            ui.colored_label(theme::WARN, "There are no features yet — make some in ② Design.");
            if ui.button("← ② Design").clicked() {
                out.push(Action::GoTo(Step::Design));
            }
        }
        ui.separator();
    }
    ui.label(RichText::new("Show").strong());
    ui.horizontal(|ui| {
        for (l, f) in [("🪓 Roughing", PassFilter::Rough), ("✨ Finishing", PassFilter::Finish), ("◫ Both", PassFilter::Both)] {
            if ui.selectable_label(app.passes == f, l).clicked() {
                out.push(Action::ShowPasses(f));
            }
        }
    });
    ui.checkbox(&mut app.show_rapids, "Show rapid moves");
    preview_info(app, ui);
}

fn plan_editor(app: &mut EzApp, ui: &mut Ui, fid: u64) {
    let Some(f) = app.project.features.iter().find(|f| f.id == fid).cloned() else { return };
    ui.horizontal(|ui| {
        ui.label(RichText::new(f.kind.icon()).size(18.0));
        ui.heading(&f.name);
    });
    ui.label(RichText::new(tip("pipeline.bits")).small().color(theme::STEEL));
    // What the planner would pick on its own (shown as "Auto: …").
    let auto_plan = {
        let mut g = f.clone();
        g.plan = PlanSettings { hand_rough: false, hand_finish: false, ..PlanSettings::default() };
        ezyx_project::pipeline::plan_feature(&app.project, &app.library, &g).unwrap_or_default()
    };
    let auto_name = |pass: ezyx_tooldb::Operation| {
        auto_plan
            .iter()
            .find(|o| o.pass == pass)
            .and_then(|o| app.library.get(&o.tool_id))
            .map(|t| t.name.clone())
            .unwrap_or_else(|| "not needed for this feature".into())
    };
    let (ar, af) = (auto_name(ezyx_tooldb::Operation::Roughing), auto_name(ezyx_tooldb::Operation::Finishing));
    let mut plan = f.plan.clone();
    let mut changed = false;
    let lib = app.library.clone();
    let three_d = matches!(f.kind, FeatureKind::Relief { .. } | FeatureKind::Model3D { .. });
    let cutting: fn(&ToolShape) -> bool = |s| !matches!(s, ToolShape::Laser { .. });
    ui.add_space(4.0);
    changed |= ui.checkbox(&mut plan.roughing, RichText::new("🪓 Roughing").strong()).changed();
    if plan.roughing {
        if plan.hand_rough {
            ui.label(RichText::new("Hand-tuned operation in use (see the timeline).").small().color(theme::STEEL));
        }
        if let Some(p) = tool_combo(ui, &lib, "rough_tool", &plan.rough_tool, &ar, cutting, &mut app.tool_filter) {
            plan.rough_tool = p;
            changed = true;
        }
    }
    ui.add_space(4.0);
    changed |= ui.checkbox(&mut plan.finishing, RichText::new("✨ Finishing").strong()).changed();
    if plan.finishing {
        if plan.hand_finish {
            ui.label(RichText::new("Hand-tuned operation in use (see the timeline).").small().color(theme::STEEL));
        }
        if let Some(p) = tool_combo(ui, &lib, "finish_tool", &plan.finish_tool, &af, cutting, &mut app.tool_filter) {
            plan.finish_tool = p;
            changed = true;
        }
    }
    let u = app.project.job.display_units;
    if plan.roughing && plan.finishing {
        ui.horizontal(|ui| {
            ui.label("Allowance for finishing");
            let r = len_field(ui, &mut plan.allowance, u, 0.0..=5.0);
            changed |= r.lost_focus() || r.drag_stopped();
        });
    }
    if matches!(f.kind, FeatureKind::Cut2D { strategy: CutStrategy::CutOut, .. }) {
        changed |= ui.checkbox(&mut plan.tabs, "Holding tabs").changed();
    }
    if three_d {
        ui.label(RichText::new("3D: flat end mill roughing in Z levels, ball-nose raster finishing.").small().color(theme::STEEL));
    }
    if (plan.hand_rough || plan.hand_finish) && ui.button("↺ Use planned passes again").clicked() {
        app.history.checkpoint(&app.project, "drop hand-tuned operations");
        app.project.operations.retain(|o| !(o.feature_id == Some(fid) && !o.auto));
        plan.hand_rough = false;
        plan.hand_finish = false;
        changed = true;
    }
    ui.horizontal(|ui| {
        if ui.button(RichText::new("⚡ Recalculate").strong()).clicked() {
            changed = true;
        }
        if ui.button("🪄 Recommended bits").clicked() {
            plan = PlanSettings::default();
            changed = true;
        }
    });
    if changed && app.exec(Command::UpdateFeature { id: fid, name: None, strategy: None, depth: None, plan: Some(plan), suppressed: None }) {
        let ctx = ui.ctx().clone();
        app.calculate(Some(&ctx));
    }
    ui.add_space(4.0);
    ui.label(RichText::new("Operations").strong());
    let max_feed = app.project.machine.limits.max_feed;
    for o in app.project.operations.clone().iter().filter(|o| o.feature_id == Some(fid)) {
        let res = app.results.iter().find(|r| r.op_id == o.id);
        let est = res.map(|r| r.toolpaths.iter().map(|t| t.stats(max_feed).est_seconds).sum::<f64>() / 60.0).unwrap_or(0.0);
        let tool = app.library.get(&o.tool_id).map(|t| t.name.clone()).unwrap_or_default();
        ui.horizontal(|ui| {
            let pass = match o.pass {
                ezyx_tooldb::Operation::Roughing => "🪓",
                ezyx_tooldb::Operation::Finishing => "✨",
            };
            if ui.selectable_label(false, format!("{pass} {} — {tool} · {est:.1} min", o.kind.label())).on_hover_text("Fine-tune this operation").clicked() {
                app.selected_op = Some(o.id);
            }
        });
        if let Some(e) = res.and_then(|r| r.error.as_ref()) {
            ui.colored_label(theme::DANGER, e);
        }
        for a in res.map(|r| r.advice.clone()).unwrap_or_default() {
            let c = match a.severity {
                ezyx_sim::Severity::Info => theme::STEEL,
                ezyx_sim::Severity::Warning => theme::WARN,
                ezyx_sim::Severity::Danger => theme::DANGER,
            };
            ui.colored_label(c, RichText::new(&a.message).small());
        }
    }
}

fn preview_info(app: &mut EzApp, ui: &mut Ui) {
    ui.label(RichText::new("Predicted result").strong()).on_hover_text(tip("pipeline.render"));
    let mat = ezyx_materials::find(&app.project.job.material);
    ui.label(format!("Material: {}", mat.map(|m| m.name).unwrap_or_default()));
    ui.add(egui::Slider::new(&mut app.perf.sim_cell_mm, 0.05..=1.0).logarithmic(true).text("sim cell mm"));
    if !app.preview_info.is_empty() {
        ui.label(&app.preview_info);
    }
    let secs: f64 = app.results.iter().flat_map(|r| r.toolpaths.iter()).map(|t| t.stats(app.project.machine.limits.max_feed).est_seconds).sum();
    if secs > 0.0 {
        ui.label(format!("Estimated machining time ≈ {:.0} min", secs / 60.0));
    }
}

// ------------------------------------------------------------------ ④ output

fn output(app: &mut EzApp, ui: &mut Ui, out: &mut Vec<Action>) {
    heading(ui, "Output", "pipeline.output");
    ui.label(RichText::new("Machine").strong());
    let cur_machine = app.project.machine.id.clone();
    let mut set_machine = None;
    ComboBox::from_id_salt("machine_out").selected_text(app.project.machine.name.clone()).width(260.0).show_ui(ui, |ui| {
        for m in ezyx_project::machines::builtin() {
            if ui.selectable_label(m.id == cur_machine, &m.name).clicked() {
                set_machine = Some(m.id.clone());
            }
        }
    });
    if let Some(id) = set_machine {
        app.exec(Command::SetMachine { id });
    }
    post_picker(app, ui);
    ui.separator();
    let s = ezyx_project::jobsheet::summarize(&app.project, &app.library, &app.results);
    ui.label(RichText::new("Summary").strong());
    ui.label(format!("{} toolpaths · ≈ {:.0} min · {} tool change(s)", s.steps.len(), s.total_seconds / 60.0, s.tool_changes));
    ui.label(RichText::new(format!("Bits in order: {}", s.tools_in_order.join(" → "))).small());
    egui::Grid::new("steps").striped(true).num_columns(3).show(ui, |ui| {
        for (i, st) in s.steps.iter().enumerate() {
            ui.label(format!("{}", i + 1));
            ui.label(RichText::new(&st.op_name).small());
            ui.label(RichText::new(format!("{:.1} min", st.seconds / 60.0)).small());
            ui.end_row();
        }
    });
    let errors = app.conflicts.iter().filter(|c| c.severity == Severity::Error).count() + s.errors.len();
    ui.separator();
    ui.label(RichText::new("Checks").strong()).on_hover_text(tip("pipeline.conflicts"));
    if errors == 0 && app.conflicts.is_empty() {
        ui.colored_label(theme::OK, "✔ No problems found.");
    }
    for e in &s.errors {
        ui.colored_label(theme::DANGER, format!("✖ {e}"));
    }
    for c in &app.conflicts {
        let (i, col) = match c.severity {
            Severity::Error => ("✖", theme::DANGER),
            Severity::Warning => ("⚠", theme::WARN),
            Severity::Info => ("ℹ", theme::STEEL),
        };
        ui.colored_label(col, format!("{i} {}", c.message));
    }
    ui.add_space(8.0);
    if errors > 0 {
        ui.colored_label(theme::DANGER, "Fix the errors above before cutting — or export anyway if you know why they're there.");
    }
    ui.horizontal(|ui| {
        if ui.add(egui::Button::new(RichText::new("💾 Download G-code").strong()).fill(theme::ACCENT_SOFT)).clicked() {
            out.push(Action::ExportGcode);
        }
        if ui.add(egui::Button::new(RichText::new("🎮 Send to Machine Control").strong())).on_hover_text(tip("pipeline.send_machine")).clicked() {
            out.push(Action::SendToMachine);
        }
    });
    if ui.button("📄 Save job sheet").clicked() {
        out.push(Action::JobSheet);
    }
}
