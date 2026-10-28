//! FeatureManager tree, Parameters / configurations and the PropertyManager of the Solid Modeler.
//! All edits change the modeler's draft part; `solid::commit` turns each gesture into one
//! `Command::SetPart`.

use super::SketchEdit;
use crate::app::EzApp;
use crate::ribbon::tip;
use crate::theme;
use egui::{DragValue, RichText, Ui};
use ezyx_cad::{Axis, Binding, BlendKind, BooleanOp, Configuration, EdgeTreatment, EndCondition, Feature, HoleKind, MirrorPlane, Modifier, Parameter, Shape};

fn icon(s: &Shape) -> &'static str {
    match s {
        Shape::Box { .. } => "⬛",
        Shape::Sphere { .. } => "⚪",
        Shape::Cylinder { .. } => "🛢",
        Shape::Cone { .. } => "🔻",
        Shape::Torus { .. } => "🍩",
        Shape::Extrude { .. } | Shape::SketchExtrude { .. } => "⬆",
        Shape::Revolve { .. } | Shape::SketchRevolve { .. } => "🔄",
        Shape::Relief { .. } => "⛰",
        Shape::Sweep { .. } => "➰",
        Shape::Loft { .. } => "🏺",
        Shape::Hole { .. } => "⚲",
        Shape::Thread { .. } => "🔩",
        Shape::Body { .. } => "⊕",
    }
}

pub fn tree(app: &mut EzApp, ui: &mut Ui) {
    let Some(part) = app.solid.draft.as_mut() else {
        ui.label(RichText::new("No part yet — use 🗋 New part.").color(theme::STEEL));
        return;
    };
    ui.horizontal(|ui| {
        ui.label("🧊");
        ui.add(egui::TextEdit::singleline(&mut part.name).desired_width(170.0));
    });
    ui.label(RichText::new("Origin · Top (XY) · Front (XZ) · Right (YZ)").small().color(theme::STEEL));
    ui.separator();
    let n = part.features.len();
    let mut action: Option<(usize, i32)> = None;
    for (i, f) in part.features.iter_mut().enumerate() {
        let sel = app.solid.sel == Some(i);
        ui.horizontal(|ui| {
            let op = match f.op {
                BooleanOp::Add => "",
                BooleanOp::Cut => " ✂",
                BooleanOp::Intersect => " ∩",
            };
            let mut txt = RichText::new(format!("{} {}{}", icon(&f.shape), f.name, op));
            if f.suppressed {
                txt = txt.strikethrough().color(theme::STEEL);
            }
            if ui.selectable_label(sel, txt).clicked() {
                app.solid.sel = Some(i);
                app.solid.json_edit = None;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("🗑").clicked() {
                    action = Some((i, 0));
                }
                if i + 1 < n && ui.small_button("▼").clicked() {
                    action = Some((i, 1));
                }
                if i > 0 && ui.small_button("▲").clicked() {
                    action = Some((i, -1));
                }
                ui.checkbox(&mut f.suppressed, "").on_hover_text("Suppress");
            });
        });
    }
    match action {
        Some((i, 0)) => {
            part.features.remove(i);
            app.solid.sel = None;
        }
        Some((i, d)) => {
            let j = (i as i32 + d) as usize;
            part.features.swap(i, j);
            app.solid.sel = Some(j);
        }
        None => {}
    }
    if !part.finish.is_empty() {
        ui.separator();
        ui.label(RichText::new("Whole part").strong());
        let mut del = None;
        for (i, m) in part.finish.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                modifier_editor(ui, m);
                if ui.small_button("🗑").clicked() {
                    del = Some(i);
                }
            });
        }
        if let Some(i) = del {
            part.finish.remove(i);
        }
    }
    if ui.button("Part properties").clicked() {
        app.solid.sel = None;
    }
}

pub fn parameters(app: &mut EzApp, ui: &mut Ui) {
    let Some(part) = app.solid.draft.as_mut() else { return };
    ui.label(RichText::new(tip("solid.parameters")).small().color(theme::STEEL));
    let values = part.parameter_values();
    if let Err(e) = &values {
        ui.colored_label(theme::DANGER, e.to_string());
    }
    let values = values.unwrap_or_default();
    let mut del = None;
    egui::Grid::new("params").num_columns(4).striped(true).show(ui, |ui| {
        ui.label(RichText::new("Name").strong());
        ui.label(RichText::new("Expression").strong());
        ui.label(RichText::new("Value").strong());
        ui.label("");
        ui.end_row();
        for (i, p) in part.parameters.iter_mut().enumerate() {
            ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(70.0));
            ui.add(egui::TextEdit::singleline(&mut p.expr).desired_width(90.0));
            ui.label(values.get(&p.name).map(|v| format!("{v:.3}")).unwrap_or_else(|| "—".into()));
            if ui.small_button("✖").clicked() {
                del = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = del {
        part.parameters.remove(i);
    }
    if ui.button("➕ Parameter").clicked() {
        let n = part.parameters.len() + 1;
        part.parameters.push(Parameter::new(format!("p{n}"), "10mm"));
    }
    ui.label(
        RichText::new("Units: mm cm m in ft deg rad · functions: sin cos tan sqrt min max round … · e.g. (1/4)in, width/2 + 3mm").small().color(theme::STEEL),
    );
    ui.separator();
    ui.label(RichText::new("Configurations").strong());
    let mut active = part.active_configuration.clone();
    ui.radio_value(&mut active, None, "Default (parameters as above)");
    for c in &part.configurations {
        ui.radio_value(&mut active, Some(c.name.clone()), format!("{} ({} override(s))", c.name, c.overrides.len()));
    }
    part.active_configuration = active;
    if let Some(name) = part.active_configuration.clone() {
        if let Some(c) = part.configurations.iter_mut().find(|c| c.name == name) {
            for o in c.overrides.iter_mut() {
                ui.horizontal(|ui| {
                    ui.label(&o.name);
                    ui.add(egui::TextEdit::singleline(&mut o.expr).desired_width(100.0));
                });
            }
        }
    }
    ui.horizontal(|ui| {
        if ui.button("➕ Configuration from current values").clicked() {
            let n = part.configurations.len() + 1;
            part.configurations.push(Configuration { name: format!("Config {n}"), overrides: part.parameters.clone() });
        }
        if part.active_configuration.is_some() && ui.button("🗑").clicked() {
            let name = part.active_configuration.take();
            part.configurations.retain(|c| Some(&c.name) != name.as_ref());
        }
    });
}

fn num(ui: &mut Ui, label: &str, v: &mut f64, range: std::ops::RangeInclusive<f64>) {
    ui.label(label);
    ui.add(DragValue::new(v).range(range).speed(0.1).max_decimals(3));
}

fn edge(ui: &mut Ui, label: &str, e: &mut EdgeTreatment) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut k = match e {
            EdgeTreatment::None => 0,
            EdgeTreatment::Fillet { .. } => 1,
            EdgeTreatment::Chamfer { .. } => 2,
        };
        let before = k;
        ui.selectable_value(&mut k, 0, "sharp");
        ui.selectable_value(&mut k, 1, "fillet");
        ui.selectable_value(&mut k, 2, "chamfer");
        if k != before {
            *e = match k {
                1 => EdgeTreatment::Fillet { radius: 1.0 },
                2 => EdgeTreatment::Chamfer { distance: 1.0 },
                _ => EdgeTreatment::None,
            };
        }
        match e {
            EdgeTreatment::Fillet { radius: v } | EdgeTreatment::Chamfer { distance: v } => {
                ui.add(DragValue::new(v).range(0.0..=500.0).speed(0.05).suffix(" mm"));
            }
            EdgeTreatment::None => {}
        }
    });
}

fn plane_pick(ui: &mut Ui, p: &mut MirrorPlane) {
    ui.selectable_value(p, MirrorPlane::YZ, "YZ");
    ui.selectable_value(p, MirrorPlane::XZ, "XZ");
    ui.selectable_value(p, MirrorPlane::XY, "XY");
}

fn modifier_editor(ui: &mut Ui, m: &mut Modifier) {
    match m {
        Modifier::Fillet { radius } => {
            ui.label("◜ Fillet");
            ui.add(DragValue::new(radius).range(0.0..=100.0).speed(0.05).suffix(" mm"));
        }
        Modifier::Shell { thickness } => {
            ui.label("▢ Shell");
            ui.add(DragValue::new(thickness).range(0.1..=100.0).speed(0.05).suffix(" mm"));
        }
        Modifier::ShellOpen { thickness, open_top } => {
            ui.label("▢ Shell");
            ui.add(DragValue::new(thickness).range(0.1..=100.0).speed(0.05).suffix(" mm"));
            ui.checkbox(open_top, "open top");
        }
        Modifier::LinearPattern { count, step } => {
            ui.label("⋯ Linear");
            ui.add(DragValue::new(count).range(1..=100));
            ui.label("×");
            for s in step.iter_mut() {
                ui.add(DragValue::new(s).speed(0.2).max_decimals(2));
            }
        }
        Modifier::GridPattern { counts, step } => {
            ui.label("▦ Grid");
            ui.add(DragValue::new(&mut counts[0]).range(1..=100));
            ui.label("×");
            ui.add(DragValue::new(&mut counts[1]).range(1..=100));
            ui.label("step");
            ui.add(DragValue::new(&mut step[0]).speed(0.2));
            ui.add(DragValue::new(&mut step[1]).speed(0.2));
        }
        Modifier::CircularPattern { count, axis_z } => {
            ui.label("◌ Circular");
            ui.add(DragValue::new(count).range(1..=360));
            ui.checkbox(axis_z, "about Z");
        }
        Modifier::CircularPatternAngle { count, total_deg, axis } => {
            ui.label("◌ Circular");
            ui.add(DragValue::new(count).range(1..=360));
            ui.add(DragValue::new(total_deg).range(1.0..=360.0).suffix("°"));
            ui.selectable_value(axis, Axis::X, "X");
            ui.selectable_value(axis, Axis::Y, "Y");
            ui.selectable_value(axis, Axis::Z, "Z");
        }
        Modifier::Mirror { plane } => {
            ui.label("⇔ Mirror");
            plane_pick(ui, plane);
        }
        Modifier::Split { plane, offset, keep_positive } => {
            ui.label("✂ Split");
            plane_pick(ui, plane);
            ui.add(DragValue::new(offset).speed(0.2).suffix(" mm"));
            ui.checkbox(keep_positive, "keep +");
        }
        Modifier::Scale { factor } => {
            ui.label("⤢ Scale");
            ui.add(DragValue::new(factor).range(0.001..=1000.0).speed(0.01));
        }
    }
}

pub fn property_manager(app: &mut EzApp, ui: &mut Ui) {
    let Some(i) = app.solid.sel else {
        part_props(app, ui);
        return;
    };
    let Some(part) = app.solid.draft.as_mut() else { return };
    let Some(f) = part.features.get_mut(i) else {
        app.solid.sel = None;
        return;
    };
    ui.horizontal(|ui| {
        ui.heading(icon(&f.shape));
        ui.add(egui::TextEdit::singleline(&mut f.name).desired_width(200.0));
    });
    ui.horizontal(|ui| {
        ui.selectable_value(&mut f.op, BooleanOp::Add, "Add");
        ui.selectable_value(&mut f.op, BooleanOp::Cut, "Cut");
        ui.selectable_value(&mut f.op, BooleanOp::Intersect, "Intersect");
        ui.checkbox(&mut f.suppressed, "Suppressed");
    });
    ui.separator();
    let mut edit_sketch = None;
    egui::Grid::new("shape").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| match &mut f.shape {
        Shape::Box { size, corner_radius } => {
            for (k, l) in ["Width X", "Depth Y", "Height Z"].iter().enumerate() {
                num(ui, l, &mut size[k], 0.01..=10000.0);
                ui.end_row();
            }
            num(ui, "Corner radius", corner_radius, 0.0..=1000.0);
            ui.end_row();
        }
        Shape::Sphere { radius } => {
            num(ui, "Radius", radius, 0.01..=10000.0);
            ui.end_row();
        }
        Shape::Cylinder { radius, height } => {
            num(ui, "Radius", radius, 0.01..=10000.0);
            ui.end_row();
            num(ui, "Height", height, 0.01..=10000.0);
            ui.end_row();
        }
        Shape::Cone { r_bottom, r_top, height } => {
            num(ui, "Bottom radius", r_bottom, 0.0..=10000.0);
            ui.end_row();
            num(ui, "Top radius", r_top, 0.0..=10000.0);
            ui.end_row();
            num(ui, "Height", height, 0.01..=10000.0);
            ui.end_row();
        }
        Shape::Torus { major, minor } => {
            num(ui, "Major radius", major, 0.01..=10000.0);
            ui.end_row();
            num(ui, "Minor radius", minor, 0.01..=10000.0);
            ui.end_row();
        }
        Shape::Extrude { height, draft_deg, .. } => {
            num(ui, "Height", height, 0.01..=10000.0);
            ui.end_row();
            num(ui, "Draft °", draft_deg, -45.0..=45.0);
            ui.end_row();
        }
        Shape::Revolve { angle_deg, .. } | Shape::SketchRevolve { angle_deg, .. } => {
            num(ui, "Angle °", angle_deg, 1.0..=360.0);
            ui.end_row();
            if matches!(f.shape, Shape::SketchRevolve { .. }) {
                ui.label("Sketch");
                if ui.button("✎ Edit sketch").clicked() {
                    edit_sketch = f.shape.sketch_mut().cloned();
                }
                ui.end_row();
            }
        }
        Shape::SketchExtrude { depth, end, draft_deg, top_edge, bottom_edge, reverse, .. } => {
            num(ui, "Depth", depth, 0.01..=10000.0);
            ui.end_row();
            ui.label("End");
            ui.horizontal(|ui| {
                ui.selectable_value(end, EndCondition::Blind, "Blind");
                ui.selectable_value(end, EndCondition::MidPlane, "Mid-plane");
                ui.selectable_value(end, EndCondition::ThroughAll, "Through all");
            });
            ui.end_row();
            num(ui, "Draft °", draft_deg, -45.0..=45.0);
            ui.end_row();
            ui.label("Direction");
            ui.checkbox(reverse, "Reverse");
            ui.end_row();
            ui.label("Cap edges");
            ui.vertical(|ui| {
                edge(ui, "top", top_edge);
                edge(ui, "bottom", bottom_edge);
            });
            ui.end_row();
            ui.label("Sketch");
            if ui.button("✎ Edit sketch").clicked() {
                edit_sketch = f.shape.sketch_mut().cloned();
            }
            ui.end_row();
        }
        Shape::Relief { base, .. } => {
            num(ui, "Base", base, 0.0..=1000.0);
            ui.end_row();
        }
        Shape::Sweep { twist_deg, path, .. } => {
            num(ui, "Twist °", twist_deg, -3600.0..=3600.0);
            ui.end_row();
            ui.label("Path points");
            ui.vertical(|ui| {
                let mut del = None;
                for (k, p) in path.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        for c in p.iter_mut() {
                            ui.add(DragValue::new(c).speed(0.2).max_decimals(2));
                        }
                        if ui.small_button("✖").clicked() {
                            del = Some(k);
                        }
                    });
                }
                if let Some(k) = del.filter(|_| path.len() > 2) {
                    path.remove(k);
                }
                if ui.small_button("➕ point").clicked() {
                    let last = *path.last().unwrap_or(&[0.0; 3]);
                    path.push([last[0] + 10.0, last[1], last[2]]);
                }
            });
            ui.end_row();
        }
        Shape::Loft { sections } => {
            for (k, s) in sections.iter_mut().enumerate() {
                num(ui, &format!("Section {} Z", k + 1), &mut s.z, -10000.0..=10000.0);
                ui.end_row();
            }
        }
        Shape::Hole { kind, diameter, depth } => {
            ui.label("Type");
            ui.horizontal_wrapped(|ui| {
                let cur = std::mem::discriminant(kind);
                for (l, k) in [
                    ("Simple", HoleKind::Simple),
                    ("Drilled", HoleKind::Drilled),
                    ("Counterbore", HoleKind::Counterbore { diameter: *diameter * 1.8, depth: 3.0 }),
                    ("Countersink", HoleKind::Countersink { diameter: *diameter * 2.0, angle_deg: 90.0 }),
                    ("Tapped", HoleKind::Tapped { pitch: 1.0, cosmetic: false }),
                ] {
                    if ui.selectable_label(cur == std::mem::discriminant(&k), l).clicked() && cur != std::mem::discriminant(&k) {
                        *kind = k;
                    }
                }
            });
            ui.end_row();
            num(ui, "Diameter", diameter, 0.1..=1000.0);
            ui.end_row();
            num(ui, "Depth", depth, 0.1..=10000.0);
            ui.end_row();
            match kind {
                HoleKind::Counterbore { diameter: d, depth: dd } => {
                    num(ui, "C'bore ⌀", d, 0.1..=1000.0);
                    ui.end_row();
                    num(ui, "C'bore depth", dd, 0.0..=1000.0);
                    ui.end_row();
                }
                HoleKind::Countersink { diameter: d, angle_deg } => {
                    num(ui, "C'sink ⌀", d, 0.1..=1000.0);
                    ui.end_row();
                    num(ui, "Angle °", angle_deg, 30.0..=150.0);
                    ui.end_row();
                }
                HoleKind::Tapped { pitch, cosmetic } => {
                    num(ui, "Pitch", pitch, 0.1..=10.0);
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(cosmetic, "Cosmetic (tap-drill only)");
                    ui.end_row();
                }
                _ => {}
            }
        }
        Shape::Thread { major_diameter, pitch, length, internal, depth } => {
            num(ui, "Major ⌀", major_diameter, 0.5..=1000.0);
            ui.end_row();
            num(ui, "Pitch", pitch, 0.1..=20.0);
            ui.end_row();
            num(ui, "Length", length, 0.1..=10000.0);
            ui.end_row();
            num(ui, "Depth (0 = ISO)", depth, 0.0..=20.0);
            ui.end_row();
            ui.label("");
            ui.checkbox(internal, "Internal (void, use with Cut)");
            ui.end_row();
        }
        Shape::Body { part } => {
            ui.label("Body");
            ui.label(format!("'{}' ({} features)", part.name, part.features.len()));
            ui.end_row();
        }
    });
    if matches!(f.shape, Shape::Hole { kind: HoleKind::Tapped { cosmetic: false, .. }, .. } | Shape::Thread { .. }) {
        ui.label(RichText::new("Modelled threads need a fine preview (≤ pitch/8) to look right.").small().color(theme::STEEL));
    }
    ui.separator();
    ui.label(RichText::new("Placement").strong());
    egui::Grid::new("place").num_columns(4).show(ui, |ui| {
        ui.label("Move");
        for v in f.placement.translate.iter_mut() {
            ui.add(DragValue::new(v).speed(0.2).max_decimals(3));
        }
        ui.end_row();
        ui.label("Rotate °");
        for v in f.placement.rotate_deg.iter_mut() {
            ui.add(DragValue::new(v).speed(0.5).max_decimals(2));
        }
        ui.end_row();
    });
    ui.horizontal(|ui| {
        ui.label("Blend with body");
        ui.add(DragValue::new(&mut f.blend).range(0.0..=100.0).speed(0.05).suffix(" mm"));
        ui.selectable_value(&mut f.blend_kind, BlendKind::Round, "round");
        ui.selectable_value(&mut f.blend_kind, BlendKind::Chamfer, "chamfer");
    });
    ui.separator();
    ui.label(RichText::new("Patterns & modifiers").strong()).on_hover_text(tip("solid.patterns"));
    let mut del = None;
    for (k, m) in f.modifiers.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            modifier_editor(ui, m);
            if ui.small_button("🗑").clicked() {
                del = Some(k);
            }
        });
    }
    if let Some(k) = del {
        f.modifiers.remove(k);
    }
    ui.horizontal_wrapped(|ui| {
        for (l, m) in [
            ("⋯ Linear", Modifier::LinearPattern { count: 3, step: [20.0, 0.0, 0.0] }),
            ("▦ Grid", Modifier::GridPattern { counts: [3, 2], step: [20.0, 20.0] }),
            ("◌ Circular", Modifier::CircularPatternAngle { count: 6, total_deg: 360.0, axis: Axis::Z }),
            ("⇔ Mirror", Modifier::Mirror { plane: MirrorPlane::YZ }),
            ("◜ Fillet", Modifier::Fillet { radius: 1.0 }),
            ("▢ Shell", Modifier::Shell { thickness: 2.0 }),
            ("✂ Split", Modifier::Split { plane: MirrorPlane::XY, offset: 0.0, keep_positive: true }),
            ("⤢ Scale", Modifier::Scale { factor: 1.0 }),
        ] {
            if ui.small_button(l).clicked() {
                f.modifiers.push(m);
            }
        }
    });
    ui.separator();
    ui.label(RichText::new("ƒ Bindings (drive values with parameters)").strong())
        .on_hover_text("JSON pointer into this feature, e.g. /shape/size/0 or /placement/translate/2");
    let mut del = None;
    for (k, b) in f.bindings.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut b.path).desired_width(120.0));
            ui.label("=");
            ui.add(egui::TextEdit::singleline(&mut b.expr).desired_width(100.0));
            if ui.small_button("✖").clicked() {
                del = Some(k);
            }
        });
    }
    if let Some(k) = del {
        f.bindings.remove(k);
    }
    // Offer the feature's numeric fields as binding targets.
    let targets = numeric_paths(f);
    ui.menu_button("ƒ Bind a value…", |ui| {
        for (path, v) in targets.iter().take(40) {
            if ui.button(format!("{path} ({v:.3})")).clicked() {
                f.bindings.push(Binding { path: path.clone(), expr: format!("{v}mm") });
                ui.close();
            }
        }
    });
    let values = part.parameter_values().unwrap_or_default();
    if !values.is_empty() {
        ui.label(
            RichText::new(format!("Parameters: {}", values.iter().map(|(k, v)| format!("{k}={v:.2}")).collect::<Vec<_>>().join(", ")))
                .small()
                .color(theme::STEEL),
        );
    }
    ui.separator();
    // Raw JSON for power users (and for shapes without a dedicated editor, e.g. loft profiles).
    let f = &mut part.features[i];
    let mut close = false;
    let mut json = app.solid.json_edit.take();
    match &mut json {
        None => {
            if ui.small_button("{ } Edit as JSON").clicked() {
                json = Some(serde_json::to_string_pretty(f).unwrap_or_default());
            }
        }
        Some(txt) => {
            ui.add(egui::TextEdit::multiline(txt).code_editor().desired_rows(12).desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
                    match serde_json::from_str::<Feature>(txt) {
                        Ok(nf) => {
                            *f = nf;
                            close = true;
                        }
                        Err(e) => app.solid.status = format!("⚠ JSON: {e}"),
                    }
                }
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        }
    }
    app.solid.json_edit = if close { None } else { json };
    if let Some(sk) = edit_sketch {
        app.solid.sketch = Some(SketchEdit::new(Some(i), sk));
    }
}

/// JSON pointers of the numeric fields of a feature (shape and placement).
fn numeric_paths(f: &Feature) -> Vec<(String, f64)> {
    fn walk(v: &serde_json::Value, path: String, out: &mut Vec<(String, f64)>) {
        match v {
            serde_json::Value::Number(n) => {
                if let Some(x) = n.as_f64() {
                    out.push((path, x));
                }
            }
            serde_json::Value::Object(m) => {
                for (k, v) in m {
                    if k != "sketch" && k != "profile" && k != "sections" && k != "part" && k != "heightmap" {
                        walk(v, format!("{path}/{k}"), out);
                    }
                }
            }
            serde_json::Value::Array(a) if a.len() <= 4 => {
                for (i, v) in a.iter().enumerate() {
                    walk(v, format!("{path}/{i}"), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    if let Ok(v) = serde_json::to_value(f) {
        for key in ["shape", "placement", "blend", "modifiers"] {
            if let Some(x) = v.get(key) {
                walk(x, format!("/{key}"), &mut out);
            }
        }
    }
    out
}

fn part_props(app: &mut EzApp, ui: &mut Ui) {
    ui.heading("Part");
    ui.label(RichText::new(tip("solid.modeler")).small().color(theme::STEEL));
    let Some(part) = app.solid.draft.as_mut() else {
        ui.label("Use 🗋 New part to start from a template, an empty part or a sketch.");
        return;
    };
    ui.label(format!("{} features · {} parameters · {} configurations", part.features.len(), part.parameters.len(), part.configurations.len()));
    ui.label("Select a feature in the tree to edit it, or add one from the Features tab.");
    ui.separator();
    ui.label(RichText::new("Whole-part finish").strong()).on_hover_text(tip("solid.dress"));
    let mut fillet = part.finish.iter().find_map(|m| if let Modifier::Fillet { radius } = m { Some(*radius) } else { None }).unwrap_or(0.0);
    ui.horizontal(|ui| {
        ui.label("Fillet all edges");
        if ui.add(DragValue::new(&mut fillet).range(0.0..=20.0).speed(0.05).suffix(" mm")).changed() {
            part.finish.retain(|m| !matches!(m, Modifier::Fillet { .. }));
            if fillet > 0.0 {
                part.finish.push(Modifier::Fillet { radius: fillet });
            }
        }
    });
}
