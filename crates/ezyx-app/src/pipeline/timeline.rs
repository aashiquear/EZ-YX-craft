//! Left panel: the stock card and the feature timeline, with conflict badges, source badges and
//! "edit in …" round trips; in the Toolpaths step each feature lists its planned operations.

use crate::app::EzApp;
use crate::ribbon::{tip, Action, Step};
use crate::theme;
use egui::{RichText, Ui};
use ezyx_project::pipeline::{FeatureSource, Severity};
use ezyx_project::Command;

pub fn show(app: &mut EzApp, ui: &mut Ui, out: &mut Vec<Action>) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(&app.project.name).strong().size(15.0));
    });
    // Stock card.
    let j = app.project.job.clone();
    let u = j.display_units;
    let card = egui::Frame::group(ui.style()).fill(if app.step == Step::Stock { theme::ACCENT_SOFT } else { ui.visuals().faint_bg_color });
    let r = card
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("① Stock").strong());
            ui.label(
                RichText::new(format!(
                    "{} {} × {} × {}\n{}",
                    app.project.stock_shape.label(),
                    u.fmt(j.width),
                    u.fmt(j.height),
                    u.fmt(j.thickness),
                    ezyx_materials::find(&j.material).map(|m| m.name).unwrap_or_default()
                ))
                .small(),
            );
        })
        .response
        .interact(egui::Sense::click());
    if r.on_hover_text(tip("pipeline.stock")).clicked() {
        out.push(Action::GoTo(Step::Stock));
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("Features ({})", app.project.features.len())).strong()).on_hover_text(tip("pipeline.timeline"));
    });
    egui::ScrollArea::vertical().id_salt("timeline").show(ui, |ui| {
        if app.project.features.is_empty() {
            ui.label(RichText::new("No features yet. In ② Design draw shapes and press a Carve button, import SVG/DXF/STL, or send something from the Image Studio or Solid Modeler.").small().color(theme::STEEL));
        }
        let n = app.project.features.len();
        let mut cmd = None;
        let feats = app.project.features.clone();
        for (i, f) in feats.iter().enumerate() {
            let sel = app.selected_feature == Some(f.id);
            let worst = app.conflicts.iter().filter(|c| c.features.contains(&f.id)).map(|c| c.severity).max();
            let frame = egui::Frame::group(ui.style()).fill(if sel { theme::ACCENT_SOFT } else { ui.visuals().panel_fill });
            let resp = frame
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(f.kind.icon()).size(16.0));
                        let mut name = RichText::new(&f.name).strong();
                        if f.suppressed {
                            name = name.strikethrough().color(theme::STEEL);
                        }
                        ui.label(name);
                        match worst {
                            Some(Severity::Error) => {
                                ui.label(RichText::new("✖").color(theme::DANGER));
                            }
                            Some(Severity::Warning) => {
                                ui.label(RichText::new("⚠").color(theme::WARN));
                            }
                            _ => {}
                        }
                    });
                    ui.label(RichText::new(format!("{} · rev {}", f.kind.label(), f.revision)).small().color(theme::STEEL));
                    ui.label(RichText::new(f.source.label()).small().color(theme::STEEL));
                    ui.horizontal(|ui| {
                        if i > 0 && ui.small_button("▲").on_hover_text("Earlier in the timeline").clicked() {
                            cmd = Some(Command::MoveFeature { id: f.id, to: i - 1 });
                        }
                        if i + 1 < n && ui.small_button("▼").on_hover_text("Later in the timeline").clicked() {
                            cmd = Some(Command::MoveFeature { id: f.id, to: i + 1 });
                        }
                        let eye = if f.suppressed { "🚫" } else { "👁" };
                        if ui.small_button(eye).on_hover_text("Suppress / restore (no toolpaths while suppressed)").clicked() {
                            cmd = Some(Command::UpdateFeature { id: f.id, name: None, strategy: None, depth: None, plan: None, suppressed: Some(!f.suppressed) });
                        }
                        if f.source.editable_elsewhere() {
                            let where_ = if matches!(f.source, FeatureSource::ImageStudio { .. }) { "Image Studio" } else { "Solid Modeler" };
                            if ui.small_button("✏").on_hover_text(format!("Edit in the {where_}. {}", tip("pipeline.merge"))).clicked() {
                                out.push(Action::EditFeature(f.id));
                            }
                        }
                        if ui.small_button("🗑").on_hover_text("Delete the feature, its toolpaths and its geometry").clicked() {
                            cmd = Some(Command::RemoveFeature { id: f.id, delete_geometry: true });
                        }
                    });
                    if app.step == Step::Toolpaths || app.step == Step::Output {
                        for o in app.project.operations.iter().filter(|o| o.feature_id == Some(f.id)) {
                            let est = app.results.iter().find(|r| r.op_id == o.id).map(|r| match &r.error {
                                Some(e) => format!("⚠ {e}"),
                                None => format!("{:.1} min", r.toolpaths.iter().map(|t| t.stats(app.project.machine.limits.max_feed).est_seconds).sum::<f64>() / 60.0),
                            });
                            let tool = app.library.get(&o.tool_id).map(|t| t.name.clone()).unwrap_or_else(|| o.tool_id.clone());
                            let txt = format!("  {} {} — {}{}", if o.auto { "•" } else { "✋" }, o.name.trim_start_matches(&format!("{} · ", f.name)), tool, est.map(|e| format!(" · {e}")).unwrap_or_default());
                            if ui.selectable_label(app.selected_op == Some(o.id), RichText::new(txt).small()).clicked() {
                                app.selected_op = Some(o.id);
                                app.selected_feature = Some(f.id);
                            }
                        }
                    }
                })
                .response
                .interact(egui::Sense::click());
            if resp.double_clicked() && f.source.editable_elsewhere() {
                out.push(Action::EditFeature(f.id));
            } else if resp.clicked() {
                app.selected_feature = Some(f.id);
                app.selected_op = None;
                if let ezyx_project::FeatureKind::Cut2D { vector_ids, .. } = &f.kind {
                    if app.step == Step::Design {
                        app.selection = vector_ids.clone();
                    }
                }
            }
        }
        if let Some(c) = cmd {
            if let Command::RemoveFeature { id, .. } = &c {
                if app.selected_feature == Some(*id) {
                    app.selected_feature = None;
                }
            }
            app.exec(c);
        }
        // Hand-made operations that don't belong to a feature (e.g. older projects).
        let loose: Vec<(u64, String)> = app.project.operations.iter().filter(|o| o.feature_id.is_none()).map(|o| (o.id, format!("✋ {} · {}", o.name, o.kind.label()))).collect();
        if !loose.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Other operations").strong()).on_hover_text(tip("pipeline.manual_ops"));
            for (id, l) in loose {
                if ui.selectable_label(app.selected_op == Some(id), l).clicked() {
                    app.selected_op = Some(id);
                    out.push(Action::GoTo(Step::Toolpaths));
                }
            }
        }
        // Conflicts list.
        if !app.conflicts.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(format!("Conflicts ({})", app.conflicts.len())).strong()).on_hover_text(tip("pipeline.conflicts"));
            for c in app.conflicts.clone() {
                let (icon, col) = match c.severity {
                    Severity::Error => ("✖", theme::DANGER),
                    Severity::Warning => ("⚠", theme::WARN),
                    Severity::Info => ("ℹ", theme::STEEL),
                };
                if ui.add(egui::Label::new(RichText::new(format!("{icon} {}", c.message)).small().color(col)).sense(egui::Sense::click())).clicked() {
                    app.selected_feature = c.features.first().copied();
                }
            }
        }
    });
}
