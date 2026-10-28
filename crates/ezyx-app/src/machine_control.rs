//! **Machine Control** — a separate module window for running the CNC. It never changes the
//! design: it works on its own job queue (G-code files from anywhere, or jobs the pipeline's
//! Output step sends), shows the connection (USB / Wi-Fi TCP / WebSocket / simulator) at the top,
//! a large DRO and jog pad on the left, the job queue and run controls on the right, a path
//! preview with live progress in the middle and the console at the bottom.

use crate::app::EzApp;
use crate::panels::machine::{self, Link};
use crate::ribbon::tip;
use crate::theme;
use egui::{Color32, RichText, Stroke, Ui};
use ezyx_machine::{grbl::cmd, JobState};

pub fn show(app: &mut EzApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    egui::Panel::top("mc_conn").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("🎮 Machine Control").strong().size(16.0).color(theme::ACCENT));
            ui.label(RichText::new(tip("machine.connect")).small().color(theme::STEEL));
        });
        machine::connection(app, ui);
        ui.add_space(2.0);
    });
    egui::Panel::bottom("mc_console").resizable(true).default_size(150.0).show(ui, |ui| {
        ui.label(RichText::new("Console").strong()).on_hover_text(tip("machine.console"));
        machine::console(app, ui);
    });
    egui::Panel::left("mc_jog").resizable(true).default_size(300.0).show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            if app.machine.connected() {
                ui.label(RichText::new("Position & jog").strong()).on_hover_text(tip("machine.jog"));
                machine::dro_jog(app, ui);
                ui.label(RichText::new("Arrow keys jog X/Y, Page Up/Down jog Z while this window has focus.").small().color(theme::STEEL));
            } else {
                ui.label(RichText::new("Not connected").strong().color(theme::STEEL));
                ui.label("Choose USB, Wi-Fi TCP, WebSocket or the Simulator above, then Connect.");
                ui.label(
                    RichText::new("Tip: the 🧪 Simulator behaves like a GRBL machine — try jogging and running jobs without risk.").small().color(theme::STEEL),
                );
                if ui.button("🧪 Connect to the simulator").clicked() {
                    connect_sim(app);
                }
            }
        });
    });
    egui::Panel::right("mc_jobs").resizable(true).default_size(320.0).show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| jobs(app, ui));
    });
    egui::CentralPanel::default().show(ui, |ui| preview(app, ui));
    keyboard_jog(app, &ctx);
}

/// Connect to the built-in GRBL simulator (also used by the docs screenshots).
pub fn connect_sim(app: &mut EzApp) {
    app.machine.link = Link::Simulator;
    match ezyx_machine::controller::Controller::connect(ezyx_machine::Endpoint::Simulator) {
        Ok(c) => {
            c.line(cmd::SETTINGS);
            app.machine.controller = Some(c);
            app.machine.error = None;
        }
        Err(e) => app.machine.error = Some(e.to_string()),
    }
}

fn jobs(app: &mut EzApp, ui: &mut Ui) {
    ui.label(RichText::new("Jobs").strong()).on_hover_text(tip("machine.jobs"));
    ui.horizontal(|ui| {
        if ui.button("📂 Load G-code…").clicked() {
            app.files.open("G-code", &["nc", "gcode", "ngc", "tap", "cnc", "gc", "txt"], |app, p| match std::fs::read_to_string(&p) {
                Ok(g) => {
                    let name = p.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                    app.machine.add_job(name, g, &p.display().to_string());
                }
                Err(e) => app.machine.error = Some(e.to_string()),
            });
        }
        if ui.button("⬇ From pipeline").on_hover_text(tip("pipeline.send_machine")).clicked() {
            app.send_to_machine();
        }
    });
    if app.machine.jobs.is_empty() {
        ui.label(
            RichText::new("No jobs yet. Load a G-code file, or press 🎮 Send to Machine Control in the pipeline's ④ Output step.").small().color(theme::STEEL),
        );
    }
    let mut remove = None;
    for (i, j) in app.machine.jobs.iter().enumerate() {
        let sel = app.machine.selected_job == Some(i);
        let running = app.machine.running_job == Some(i) && matches!(app.machine.progress.2, JobState::Running | JobState::Paused);
        ui.horizontal(|ui| {
            let icon = if running {
                "▶"
            } else if j.source == "pipeline" {
                "🛠"
            } else {
                "📄"
            };
            if ui.selectable_label(sel, format!("{icon} {}", j.name)).clicked() {
                app.machine.selected_job = Some(i);
            }
            if !running && ui.small_button("🗑").clicked() {
                remove = Some(i);
            }
        });
        if sel {
            ui.label(
                RichText::new(format!(
                    "{} lines · ≈ {:.0} min · {:.0} × {:.0} mm · Z ≥ {:.2}",
                    j.lines(),
                    j.est_seconds / 60.0,
                    j.max[0] - j.min[0],
                    j.max[1] - j.min[1],
                    j.min[2]
                ))
                .small()
                .color(theme::STEEL),
            );
        }
    }
    if let Some(i) = remove {
        app.machine.jobs.remove(i);
        app.machine.selected_job = None;
        if let Some(r) = app.machine.running_job {
            app.machine.running_job = if r == i {
                None
            } else if r > i {
                Some(r - 1)
            } else {
                Some(r)
            };
        }
    }
    ui.separator();
    ui.label(RichText::new("Run").strong());
    machine::job_controls(app, ui);
    if !app.machine.connected() {
        ui.label(RichText::new("Connect a machine to run jobs.").small().color(theme::STEEL));
    }
}

/// Top-down plot of the selected job, cut moves coloured by depth, done part highlighted.
fn preview(app: &mut EzApp, ui: &mut Ui) {
    let rect = ui.available_rect_before_wrap();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::viewport_bg(app.prefs.dark));
    let m = &app.machine;
    let Some(j) = m.selected_job.and_then(|i| m.jobs.get(i)) else {
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, "Select or load a job to see its path", egui::FontId::proportional(16.0), theme::STEEL);
        return;
    };
    let (w, h) = ((j.max[0] - j.min[0]).max(1.0), (j.max[1] - j.min[1]).max(1.0));
    let scale = (((rect.width() - 60.0) as f64 / w).min((rect.height() - 60.0) as f64 / h)) as f32;
    let origin = rect.center() - egui::vec2(w as f32 * scale * 0.5, -(h as f32) * scale * 0.5);
    let to = |p: [f64; 4]| egui::pos2(origin.x + (p[0] - j.min[0]) as f32 * scale, origin.y - (p[1] - j.min[1]) as f32 * scale);
    // Work area outline + origin.
    painter.rect_stroke(
        egui::Rect::from_two_pos(to([j.min[0], j.min[1], 0.0, 0.0]), to([j.max[0], j.max[1], 0.0, 0.0])),
        0.0,
        Stroke::new(1.0, theme::STEEL),
        egui::StrokeKind::Outside,
    );
    let zero = to([0.0, 0.0, 0.0, 0.0]);
    if rect.contains(zero) {
        painter.circle_filled(zero, 4.0, theme::ACCENT);
        painter.text(zero + egui::vec2(6.0, 4.0), egui::Align2::LEFT_TOP, "0,0", egui::FontId::monospace(11.0), theme::ACCENT);
    }
    let running = m.running_job == m.selected_job && m.progress.1 > 0;
    let done_line = if running { (m.progress.0 as f64 / m.progress.1.max(1) as f64 * j.lines() as f64) as usize } else { 0 };
    let depth = (-j.min[2]).max(0.1);
    // Stay inside the window's GPU vertex budget (and keep huge programs responsive).
    let granted = crate::gpu_guard::take_segments(ui.ctx(), j.moves.len().min(120_000));
    let stride = j.moves.len().div_ceil(granted.max(1)).max(1);
    for (i, mv) in j.moves.iter().enumerate() {
        if mv.rapid {
            if i % stride == 0 {
                painter.line_segment([to(mv.from), to(mv.to)], Stroke::new(0.5, Color32::from_rgba_unmultiplied(220, 70, 60, 90)));
            }
            continue;
        }
        if i % stride != 0 {
            continue;
        }
        let c = if running && mv.line <= done_line { theme::OK } else { theme::depth_color((-mv.to[2] / depth) as f32) };
        painter.line_segment([to(mv.from), to(mv.to)], Stroke::new(1.0, c));
    }
    // Live tool position.
    if let Some(p) = m.status.lock().ok().and_then(|s| s.clone()).and_then(|s| s.work_pos()) {
        let s = to([p[0], p[1], 0.0, 0.0]);
        painter.circle_stroke(s, 7.0, Stroke::new(2.0, theme::ACCENT));
        painter.line_segment([s - egui::vec2(10.0, 0.0), s + egui::vec2(10.0, 0.0)], Stroke::new(1.0, theme::ACCENT));
        painter.line_segment([s - egui::vec2(0.0, 10.0), s + egui::vec2(0.0, 10.0)], Stroke::new(1.0, theme::ACCENT));
    }
    painter.text(
        rect.left_top() + egui::vec2(10.0, 8.0),
        egui::Align2::LEFT_TOP,
        format!("{} — {}", j.name, j.source),
        egui::FontId::proportional(13.0),
        theme::STEEL,
    );
}

fn keyboard_jog(app: &mut EzApp, ctx: &egui::Context) {
    if !app.machine.connected() || ctx.egui_wants_keyboard_input() || matches!(app.machine.progress.2, JobState::Running | JobState::Paused) {
        return;
    }
    // Only when this module has focus (its own window, or the embedded tab).
    let embedded_here = app.focus == Some(crate::ribbon::Module::Machine);
    let own_window = ctx.viewport_id() == crate::app::module_viewport(crate::ribbon::Module::Machine);
    if !(embedded_here || own_window) || !ctx.input(|i| i.viewport().focused.unwrap_or(true)) {
        return;
    }
    let (step, feed) = (app.machine.step, app.machine.jog_feed);
    let mut jog = None;
    ctx.input(|i| {
        if i.key_pressed(egui::Key::ArrowLeft) {
            jog = Some(cmd::jog(Some(-step), None, None, None, feed));
        } else if i.key_pressed(egui::Key::ArrowRight) {
            jog = Some(cmd::jog(Some(step), None, None, None, feed));
        } else if i.key_pressed(egui::Key::ArrowUp) {
            jog = Some(cmd::jog(None, Some(step), None, None, feed));
        } else if i.key_pressed(egui::Key::ArrowDown) {
            jog = Some(cmd::jog(None, Some(-step), None, None, feed));
        } else if i.key_pressed(egui::Key::PageUp) {
            jog = Some(cmd::jog(None, None, Some(step), None, feed * 0.5));
        } else if i.key_pressed(egui::Key::PageDown) {
            jog = Some(cmd::jog(None, None, Some(-step), None, feed * 0.5));
        }
    });
    if let Some(j) = jog {
        app.machine.line(j);
    }
}
