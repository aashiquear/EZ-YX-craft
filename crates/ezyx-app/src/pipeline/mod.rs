//! The main pipeline window: step bar, per-step ribbon, feature timeline (left), step panel
//! (right) and the pipeline viewer (centre).

mod panels;
mod timeline;
pub mod view;

use crate::app::{EzApp, STOCK_PRESETS};
use crate::ribbon::{big, big_sel, group, pill, strip, tip, Action, Align, DrawTool, Module, OpTemplate, PassFilter, Step};
use crate::theme;
use crate::viewport::ViewPreset;
use egui::{RichText, Ui};
use ezyx_project::pipeline::CutStrategy;

pub fn show(app: &mut EzApp, ui: &mut Ui, out: &mut Vec<Action>) {
    egui::Panel::top("steps").show(ui, |ui| step_bar(app, ui, out));
    egui::Panel::top("ribbon").show(ui, |ui| ribbon(app, ui, out));
    egui::Panel::left("timeline").resizable(true).default_size(270.0).show(ui, |ui| timeline::show(app, ui, out));
    egui::Panel::right("inspector").resizable(true).default_size(340.0).show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| panels::show(app, ui, out));
    });
    egui::CentralPanel::no_frame().show(ui, |ui| view::show(app, ui));
}

fn step_bar(app: &mut EzApp, ui: &mut Ui, out: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        for (i, s) in Step::ALL.iter().enumerate() {
            if i > 0 {
                ui.label(RichText::new("→").color(theme::STEEL));
            }
            let done = match s {
                Step::Stock => true,
                Step::Design => !app.project.features.is_empty(),
                Step::Toolpaths => !app.results.is_empty(),
                Step::Output => false,
            };
            let label = if done && app.step != *s { format!("{} ✔", s.label()) } else { s.label().to_string() };
            if pill(ui, &label, app.step == *s, s.hint()) {
                out.push(Action::GoTo(*s));
            }
        }
        ui.separator();
        // Live pipeline health.
        let errors = app.conflicts.iter().filter(|c| c.severity == ezyx_project::pipeline::Severity::Error).count();
        let warns = app.conflicts.iter().filter(|c| c.severity == ezyx_project::pipeline::Severity::Warning).count();
        if errors + warns == 0 {
            ui.label(RichText::new("✔ no conflicts").color(theme::OK)).on_hover_text(tip("pipeline.conflicts"));
        } else {
            ui.label(RichText::new(format!("✖ {errors}  ⚠ {warns}")).color(if errors > 0 { theme::DANGER } else { theme::WARN }))
                .on_hover_text(tip("pipeline.conflicts"));
        }
        let j = &app.project.job;
        ui.label(
            RichText::new(format!(
                "{} · {} × {} × {} · {}",
                app.project.stock_shape.label(),
                j.display_units.fmt(j.width),
                j.display_units.fmt(j.height),
                j.display_units.fmt(j.thickness),
                ezyx_materials::find(&j.material).map(|m| m.name).unwrap_or_default()
            ))
            .color(theme::STEEL),
        );
    });
}

fn ribbon(app: &mut EzApp, ui: &mut Ui, out: &mut Vec<Action>) {
    let step = app.step;
    strip(ui, "pipeline_ribbon", |ui| match step {
        Step::Stock => {
            group(ui, "Project", |ui| {
                big(ui, "🗋", "New", "New project", out, Action::New);
                big(ui, "📂", "Open", "Open .ezyx project", out, Action::Open);
                big(ui, "💾", "Save", "Save (Ctrl+S)", out, Action::Save);
            });
            group(ui, "Quick stock", |ui| {
                for (i, (name, ..)) in STOCK_PRESETS.iter().enumerate() {
                    let (icon, short) = [("🪵", "Walnut"), ("🪵", "Pine 1×6"), ("▭", "MDF"), ("◯", "Round"), ("⬜", "Alu plate"), ("🧽", "HDU foam")][i];
                    big(ui, icon, short, name, out, Action::StockPreset(i));
                }
            });
            group(ui, "", |ui| big(ui, "➡", "Next", "② Design", out, Action::NextStep));
        }
        Step::Design => {
            group(ui, "Draw", |ui| {
                let t = app.tool;
                let d = tip("pipeline.draw");
                big_sel(ui, "⬉", "Select", "Select (Esc) — click, Shift+click or drag a box", t == DrawTool::Select, out, Action::Tool(DrawTool::Select));
                big_sel(ui, "▭", "Rectangle", &format!("Rectangle (R). {d}"), t == DrawTool::Rect, out, Action::Tool(DrawTool::Rect));
                big_sel(
                    ui,
                    "◯",
                    "Circle",
                    "Circle (C): drag from the centre or click and type the radius",
                    t == DrawTool::Circle,
                    out,
                    Action::Tool(DrawTool::Circle),
                );
                big_sel(ui, "⬭", "Ellipse", "Ellipse: drag the bounding box or type rx,ry", t == DrawTool::Ellipse, out, Action::Tool(DrawTool::Ellipse));
                big_sel(
                    ui,
                    "〰",
                    "Polyline",
                    "Polyline (P): click points; double-click/Enter to finish",
                    t == DrawTool::Polyline,
                    out,
                    Action::Tool(DrawTool::Polyline),
                );
                big_sel(ui, "🅰", "Text", "Text (T): click to place", t == DrawTool::Text, out, Action::Tool(DrawTool::Text));
                big(ui, "⭐", "Star", "Add a star at the stock centre", out, Action::AddStar);
                big(ui, "⬡", "Polygon", "Add a hexagon at the stock centre", out, Action::AddPolygon);
                big_sel(ui, "📏", "Measure", &tip("pipeline.snap"), t == DrawTool::Measure, out, Action::Tool(DrawTool::Measure));
            });
            group(ui, "Edit", |ui| {
                big(ui, "⧉", "Offset", "Offset the selection by the distance in the panel", out, Action::Offset);
                big(ui, "∪", "Weld", "Union selected shapes", out, Action::Union);
                big(ui, "∖", "Subtract", "First selected minus the rest", out, Action::Subtract);
                big(ui, "∩", "Intersect", "Keep the overlap", out, Action::Intersect);
                big(ui, "🗑", "Delete", "Delete selection (Del)", out, Action::Delete);
            });
            group(ui, "Arrange", |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        for (l, a, t) in [("⇤", Align::Left, "Align left"), ("⇹", Align::CenterX, "Centre horizontally"), ("⇥", Align::Right, "Align right")]
                        {
                            if ui.button(l).on_hover_text(format!("{t} (one shape: to the stock)")).clicked() {
                                out.push(Action::Align(a));
                            }
                        }
                        if ui.button("⊕").on_hover_text("Centre on the stock").clicked() {
                            out.push(Action::Center);
                        }
                    });
                    ui.horizontal(|ui| {
                        for (l, a, t) in [("⤒", Align::Top, "Align top"), ("⇳", Align::CenterY, "Centre vertically"), ("⤓", Align::Bottom, "Align bottom")]
                        {
                            if ui.button(l).on_hover_text(t).clicked() {
                                out.push(Action::Align(a));
                            }
                        }
                        if ui.button("⋯").on_hover_text("Distribute horizontally").clicked() {
                            out.push(Action::Distribute { horizontal: true });
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("⇋ Mirror").on_hover_text("Mirror left ↔ right").clicked() {
                            out.push(Action::Mirror { horizontal: true });
                        }
                        if ui.button("⇵").on_hover_text("Mirror top ↕ bottom").clicked() {
                            out.push(Action::Mirror { horizontal: false });
                        }
                        if ui.button("⋮").on_hover_text("Distribute vertically").clicked() {
                            out.push(Action::Distribute { horizontal: false });
                        }
                    });
                });
            });
            group(ui, "Carve the selection", |ui| {
                let c = tip("pipeline.carve");
                for (icon, s) in [
                    ("✂", CutStrategy::CutOut),
                    ("⬚", CutStrategy::InsideCut),
                    ("▤", CutStrategy::Pocket),
                    ("∨", CutStrategy::VCarve),
                    ("✎", CutStrategy::Engrave),
                    ("⚲", CutStrategy::Drill),
                    ("─", CutStrategy::OnLine),
                ] {
                    big(ui, icon, s.label(), &c, out, Action::Carve(s));
                }
            });
            group(ui, "Bring in", |ui| {
                let i = tip("pipeline.import");
                big(ui, "📐", "SVG/DXF", &i, out, Action::ImportVectors);
                big(ui, "🔺", "STL/OBJ", &i, out, Action::ImportMesh);
                big(ui, "🖼", "Image", &tip("image.studio"), out, Action::OpenModule(Module::Image));
                big(ui, "🧊", "Solid", &tip("solid.modeler"), out, Action::OpenModule(Module::Solid));
            });
            group(ui, "", |ui| big(ui, "➡", "Next", "③ Toolpaths", out, Action::NextStep));
        }
        Step::Toolpaths => {
            group(ui, "Plan", |ui| {
                big(ui, "🪄", "Auto-plan", &tip("pipeline.toolpaths"), out, Action::AutoPlanAll);
                big(ui, "⚡", "Recalculate", "Re-plan with your bit choices and recompute every toolpath", out, Action::Calculate);
            });
            group(ui, "Show", |ui| {
                let p = app.passes;
                let t = tip("pipeline.preview");
                big_sel(ui, "🪓", "Rough", &t, p == PassFilter::Rough, out, Action::ShowPasses(PassFilter::Rough));
                big_sel(ui, "✨", "Finish", &t, p == PassFilter::Finish, out, Action::ShowPasses(PassFilter::Finish));
                big_sel(ui, "◫", "Both", &t, p == PassFilter::Both, out, Action::ShowPasses(PassFilter::Both));
            });
            group(ui, "Preview", |ui| {
                big(ui, "👁", "Simulate", "Cut the virtual stock with the passes shown", out, Action::Simulate);
                big(ui, "🎨", "Final render", &tip("pipeline.render"), out, Action::FinalRender);
                big(ui, "✖", "Clear", "Back to the toolpath view", out, Action::ClearPreview);
            });
            group(ui, "Camera", |ui| {
                big(ui, "⬇", "Top", "Top view", out, Action::View(ViewPreset::Top));
                big(ui, "◩", "Iso", "Isometric", out, Action::View(ViewPreset::Iso));
                big(ui, "⛶", "Fit", "Fit (F)", out, Action::Fit);
            });
            group(ui, "Advanced", |ui| {
                ui.menu_button(RichText::new("➕ Add operation…").strong(), |ui| {
                    ui.label(RichText::new(tip("pipeline.manual_ops")).small().color(theme::STEEL));
                    for (l, t) in [
                        ("Profile outside", OpTemplate::ProfileOutside),
                        ("Profile inside", OpTemplate::ProfileInside),
                        ("Profile on line", OpTemplate::ProfileOn),
                        ("Pocket", OpTemplate::Pocket),
                        ("V-carve", OpTemplate::VCarve),
                        ("Drill", OpTemplate::Drill),
                        ("Engrave", OpTemplate::Engrave),
                        ("Laser", OpTemplate::Laser),
                        ("3D roughing", OpTemplate::Rough3d),
                        ("3D finishing", OpTemplate::Finish3d),
                    ] {
                        if ui.button(l).clicked() {
                            out.push(Action::AddOp(t));
                            ui.close();
                        }
                    }
                });
            });
            group(ui, "", |ui| big(ui, "➡", "Next", "④ Output", out, Action::NextStep));
        }
        Step::Output => {
            group(ui, "Output", |ui| {
                big(ui, "💾", "G-code", &tip("pipeline.output"), out, Action::ExportGcode);
                big(ui, "🎮", "To machine", &tip("pipeline.send_machine"), out, Action::SendToMachine);
                big(ui, "📄", "Job sheet", "Printable HTML job sheet: bits in order, depths, feeds, checks", out, Action::JobSheet);
            });
            group(ui, "Check", |ui| {
                big(ui, "🎨", "Final render", &tip("pipeline.render"), out, Action::FinalRender);
                big(ui, "⚡", "Recalculate", "Recompute every toolpath", out, Action::Calculate);
            });
        }
    });
}
