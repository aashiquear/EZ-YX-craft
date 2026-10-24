//! Left panel: project tree.

use crate::app::EzApp;
use crate::theme;
use egui::{CollapsingHeader, RichText};

pub fn show(app: &mut EzApp, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.label(RichText::new(&app.project.name).strong().size(15.0));
    let j = &app.project.job;
    ui.label(
        RichText::new(format!("{} × {} × {}", j.display_units.fmt(j.width), j.display_units.fmt(j.height), j.display_units.fmt(j.thickness)))
            .small()
            .color(theme::STEEL),
    );
    if ui.small_button("⚙ Job setup").clicked() {
        app.selection.clear();
        app.selected_op = None;
        app.selected_part = None;
        app.tab = crate::ribbon::Tab::Home;
    }
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        CollapsingHeader::new(format!("✏ Vectors ({})", app.project.vectors.len())).default_open(true).show(ui, |ui| {
            let mut clicked = None;
            for v in app.project.vectors.iter().take(400) {
                let sel = app.selection.contains(&v.id);
                let label = format!("#{} {} {}", v.id, v.name, if v.path.closed { "◻" } else { "∿" });
                if ui.selectable_label(sel, label).clicked() {
                    clicked = Some(v.id);
                }
            }
            if app.project.vectors.len() > 400 {
                ui.label(format!("… {} more", app.project.vectors.len() - 400));
            }
            if let Some(id) = clicked {
                if ui.input(|i| i.modifiers.shift || i.modifiers.command) {
                    if let Some(i) = app.selection.iter().position(|x| *x == id) {
                        app.selection.remove(i);
                    } else {
                        app.selection.push(id);
                    }
                } else {
                    app.selection = vec![id];
                }
                app.selected_op = None;
            }
            ui.horizontal(|ui| {
                if ui.small_button("Select all").clicked() {
                    app.selection = app.project.vectors.iter().map(|v| v.id).collect();
                }
                if ui.small_button("None").clicked() {
                    app.selection.clear();
                }
            });
        });
        let mut touched = false;
        if !app.project.reliefs.is_empty() {
            CollapsingHeader::new(format!("⛰ Reliefs ({})", app.project.reliefs.len())).default_open(true).show(ui, |ui| {
                let mut del = None;
                for r in &mut app.project.reliefs {
                    ui.horizontal(|ui| {
                        touched |= ui.checkbox(&mut r.visible, "").changed();
                        ui.label(format!("#{} {}", r.id, r.name));
                        if ui.small_button("🗑").clicked() {
                            del = Some(r.id);
                        }
                    });
                }
                if let Some(id) = del {
                    app.history.checkpoint(&app.project, "delete relief");
                    app.project.reliefs.retain(|r| r.id != id);
                    touched = true;
                }
            });
        }
        if !app.project.meshes.is_empty() {
            CollapsingHeader::new(format!("🔺 Meshes ({})", app.project.meshes.len())).default_open(true).show(ui, |ui| {
                let mut del = None;
                for m in &mut app.project.meshes {
                    ui.horizontal(|ui| {
                        touched |= ui.checkbox(&mut m.visible, "").changed();
                        if ui
                            .selectable_label(app.selected_mesh == Some(m.id), format!("#{} {} ({} tris)", m.id, short(&m.name), m.mesh.indices.len()))
                            .clicked()
                        {
                            app.selected_mesh = if app.selected_mesh == Some(m.id) { None } else { Some(m.id) };
                        }
                        if ui.small_button("🗑").clicked() {
                            del = Some(m.id);
                        }
                    });
                }
                if let Some(id) = del {
                    app.history.checkpoint(&app.project, "delete mesh");
                    app.project.meshes.retain(|m| m.id != id);
                    touched = true;
                }
            });
        }
        if !app.project.parts.is_empty() {
            CollapsingHeader::new(format!("🧊 Parts ({})", app.project.parts.len())).default_open(true).show(ui, |ui| {
                let mut del = None;
                for p in &mut app.project.parts {
                    ui.horizontal(|ui| {
                        touched |= ui.checkbox(&mut p.visible, "").changed();
                        if ui
                            .selectable_label(app.selected_part == Some(p.id), format!("#{} {} ({} features)", p.id, p.part.name, p.part.features.len()))
                            .clicked()
                        {
                            app.selected_part = Some(p.id);
                            app.tab = crate::ribbon::Tab::Model3D;
                        }
                        if ui.small_button("🗑").clicked() {
                            del = Some(p.id);
                        }
                    });
                }
                if let Some(id) = del {
                    app.history.checkpoint(&app.project, "delete part");
                    app.project.parts.retain(|p| p.id != id);
                    if app.selected_part == Some(id) {
                        app.selected_part = None;
                    }
                    touched = true;
                }
            });
        }
        CollapsingHeader::new(format!("🛠 Toolpaths ({})", app.project.operations.len())).default_open(true).show(ui, |ui| {
            let mut sel = None;
            let mut del = None;
            let mut up = None;
            let n = app.project.operations.len();
            for (i, o) in app.project.operations.iter_mut().enumerate() {
                let est = app.results.iter().find(|r| r.op_id == o.id).map(|r| {
                    if let Some(e) = &r.error {
                        format!("⚠ {e}")
                    } else {
                        let s: f64 = r.toolpaths.iter().map(|t| t.stats(3000.0).est_seconds).sum();
                        format!("{:.1} min", s / 60.0)
                    }
                });
                ui.horizontal(|ui| {
                    touched |= ui.checkbox(&mut o.enabled, "").changed();
                    if ui.selectable_label(app.selected_op == Some(o.id), format!("{} · {}", o.name, o.kind.label())).clicked() {
                        sel = Some(o.id);
                    }
                    if i > 0 && ui.small_button("⬆").on_hover_text("Run earlier").clicked() {
                        up = Some(i);
                    }
                    if ui.small_button("🗑").clicked() {
                        del = Some(o.id);
                    }
                });
                if let Some(e) = est {
                    ui.label(RichText::new(format!("    {e}")).small().color(theme::STEEL));
                }
            }
            let _ = n;
            if let Some(id) = sel {
                app.selected_op = Some(id);
                app.tab = crate::ribbon::Tab::Toolpaths;
            }
            if let Some(i) = up {
                app.history.checkpoint(&app.project, "reorder");
                app.project.operations.swap(i, i - 1);
                touched = true;
            }
            if let Some(id) = del {
                app.exec(ezyx_project::Command::RemoveOperation { id });
                app.results.retain(|r| r.op_id != id);
            }
        });
        if touched {
            app.touch();
        }
    });
}

fn short(s: &str) -> String {
    let name = std::path::Path::new(s).file_name().and_then(|f| f.to_str()).unwrap_or(s);
    name.chars().take(22).collect()
}
