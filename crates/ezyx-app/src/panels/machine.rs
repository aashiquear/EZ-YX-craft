//! Machine control: connect (USB / Wi-Fi TCP / WebSocket / simulator), DRO, jog, zero, probe,
//! run jobs with overrides, console.

use crate::app::EzApp;
use crate::theme;
use crossbeam_channel::{unbounded, Receiver};
use egui::{Color32, RichText, Ui};
use ezyx_machine::{
    controller::{Command as MCmd, Controller},
    grbl::{self, cmd, rt},
    Endpoint, Event, JobState, MachineState as MState, StatusReport,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    Usb,
    WifiTcp,
    WifiWs,
    Simulator,
}

pub struct MachineState {
    pub link: Link,
    pub ports: Vec<String>,
    pub port: String,
    pub baud: u32,
    pub host: String,
    pub tcp_port: u16,
    pub ws_url: String,
    pub subnet: String,
    pub scan_rx: Option<Receiver<Vec<Endpoint>>>,
    pub found: Vec<Endpoint>,
    pub controller: Option<Controller>,
    pub status: Arc<Mutex<Option<StatusReport>>>,
    pub console: VecDeque<String>,
    pub input: String,
    pub step: f64,
    pub jog_feed: f64,
    pub plate: f64,
    pub progress: (usize, usize, JobState),
    pub settings: Vec<(u32, String)>,
    pub error: Option<String>,
    pub pending_job: Option<String>,
    /// Job queue: G-code from files or sent by the pipeline (Machine Control never edits designs).
    pub jobs: Vec<MachineJob>,
    pub selected_job: Option<usize>,
    /// Index of the job currently streaming.
    pub running_job: Option<usize>,
}

/// A G-code program in the Machine Control queue.
pub struct MachineJob {
    pub name: String,
    /// `pipeline` or the file path.
    pub source: String,
    pub gcode: String,
    pub moves: Vec<ezyx_gcode::parse::ParsedMove>,
    pub est_seconds: f64,
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl MachineJob {
    pub fn new(name: String, gcode: String, source: &str, rapid: f64) -> Self {
        let moves = ezyx_gcode::parse(&gcode);
        let est_seconds = ezyx_gcode::parse::estimate_seconds(&moves, rapid);
        let (mut min, mut max) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for m in moves.iter().filter(|m| !m.rapid) {
            for i in 0..3 {
                min[i] = min[i].min(m.to[i]).min(m.from[i]);
                max[i] = max[i].max(m.to[i]).max(m.from[i]);
            }
        }
        if min[0] > max[0] {
            (min, max) = ([0.0; 3], [0.0; 3]);
        }
        Self { name, source: source.into(), gcode, moves, est_seconds, min, max }
    }
    pub fn lines(&self) -> usize {
        self.gcode.lines().count()
    }
}

impl Default for MachineState {
    fn default() -> Self {
        Self {
            link: Link::Usb,
            ports: vec![],
            port: String::new(),
            baud: 115_200,
            host: "192.168.1.50".into(),
            tcp_port: 23,
            ws_url: "ws://192.168.1.50:81/".into(),
            subnet: "192.168.1".into(),
            scan_rx: None,
            found: vec![],
            controller: None,
            status: Arc::new(Mutex::new(None)),
            console: VecDeque::new(),
            input: String::new(),
            step: 1.0,
            jog_feed: 1500.0,
            plate: 15.0,
            progress: (0, 0, JobState::Idle),
            settings: vec![],
            error: None,
            pending_job: None,
            jobs: vec![],
            selected_job: None,
            running_job: None,
        }
    }
}

impl MachineState {
    /// Queue a program (newest first in the list) and select it.
    pub fn add_job(&mut self, name: String, gcode: String, source: &str) {
        // Re-sending from the pipeline replaces the previous pipeline job.
        if source == "pipeline" {
            if let Some(i) = self.jobs.iter().position(|j| j.source == "pipeline" && j.name == name) {
                if self.running_job != Some(i) {
                    self.jobs.remove(i);
                }
            }
        }
        self.jobs.push(MachineJob::new(name, gcode, source, 3000.0));
        self.selected_job = Some(self.jobs.len() - 1);
    }
    pub fn connected(&self) -> bool {
        self.controller.is_some()
    }
    pub fn badge(&self) -> String {
        match (&self.controller, self.status.lock().ok().and_then(|s| s.clone())) {
            (None, _) => "⚪ Machine: not connected".into(),
            (Some(c), Some(s)) => format!("🟢 {} · {:?}", c.endpoint.label(), s.state),
            (Some(c), None) => format!("🟡 {}", c.endpoint.label()),
        }
    }
    fn log(&mut self, s: impl Into<String>) {
        self.console.push_back(s.into());
        while self.console.len() > 500 {
            self.console.pop_front();
        }
    }
    pub fn poll(&mut self, _ctx: &egui::Context) {
        if let Some(rx) = &self.scan_rx {
            if let Ok(found) = rx.try_recv() {
                self.found = found;
                self.scan_rx = None;
            }
        }
        let mut drop_ctrl = false;
        let events: Vec<Event> = self.controller.as_ref().map(|c| c.events.try_iter().collect()).unwrap_or_default();
        for e in events {
            match e {
                Event::Status(s) => {
                    if let Ok(mut g) = self.status.lock() {
                        *g = Some(s);
                    }
                }
                Event::Rx(l) => self.log(format!("← {l}")),
                Event::Tx(l) => self.log(format!("→ {l}")),
                Event::Response(grbl::Response::Setting(k, v)) => {
                    self.settings.retain(|(n, _)| *n != k);
                    self.settings.push((k, v));
                }
                Event::Progress { acked, total, state } => self.progress = (acked, total, state),
                Event::Error(e) => {
                    self.log(format!("⚠ {e}"));
                    self.error = Some(e);
                }
                Event::Connected(d) => self.log(format!("Connected: {d}")),
                Event::Disconnected(r) => {
                    self.log(format!("Disconnected: {r}"));
                    drop_ctrl = true;
                }
                _ => {}
            }
        }
        if drop_ctrl {
            self.controller = None;
            if let Ok(mut g) = self.status.lock() {
                *g = None;
            }
        }
    }
    pub fn sender(&self) -> Option<crossbeam_channel::Sender<MCmd>> {
        self.controller.as_ref().map(|c| c.sender())
    }
    pub fn line(&mut self, l: impl Into<String>) {
        if let Some(c) = &self.controller {
            c.line(l);
        }
    }
    pub fn rt(&self, b: u8) {
        if let Some(c) = &self.controller {
            c.realtime(b);
        }
    }
}

/// Connection bar: link type, endpoint, connect / disconnect. Returns true when connected.
pub fn connection(app: &mut EzApp, ui: &mut Ui) -> bool {
    let m = &mut app.machine;
    ui.label(RichText::new(&app.project.machine.connection_hint).small().color(theme::STEEL));
    if m.controller.is_none() {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut m.link, Link::Usb, "🔌 USB");
            ui.selectable_value(&mut m.link, Link::WifiTcp, "📶 Wi-Fi TCP");
            ui.selectable_value(&mut m.link, Link::WifiWs, "📶 WebSocket");
            ui.selectable_value(&mut m.link, Link::Simulator, "🧪 Simulator");
        });
        let ep = match m.link {
            Link::Usb => {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("port").selected_text(if m.port.is_empty() { "choose port" } else { m.port.as_str() }).show_ui(ui, |ui| {
                        for p in m.ports.clone() {
                            ui.selectable_value(&mut m.port, p.clone(), p);
                        }
                    });
                    if ui.button("⟳").on_hover_text("Refresh ports").clicked() {
                        m.ports = ezyx_machine::discovery::serial_ports()
                            .into_iter()
                            .filter_map(|e| if let Endpoint::Serial { port, .. } = e { Some(port) } else { None })
                            .collect();
                        if m.port.is_empty() {
                            m.port = m.ports.first().cloned().unwrap_or_default();
                        }
                    }
                    ui.add(egui::DragValue::new(&mut m.baud).range(9600..=2_000_000));
                });
                (!m.port.is_empty()).then(|| Endpoint::Serial { port: m.port.clone(), baud: m.baud })
            }
            Link::WifiTcp => {
                ui.horizontal(|ui| {
                    ui.label("Host");
                    ui.text_edit_singleline(&mut m.host);
                    ui.add(egui::DragValue::new(&mut m.tcp_port).range(1..=65535));
                });
                Some(Endpoint::Tcp { host: m.host.clone(), port: m.tcp_port })
            }
            Link::WifiWs => {
                ui.text_edit_singleline(&mut m.ws_url);
                Some(Endpoint::WebSocket { url: m.ws_url.clone() })
            }
            Link::Simulator => Some(Endpoint::Simulator),
        };
        if matches!(m.link, Link::WifiTcp | Link::WifiWs) {
            ui.horizontal(|ui| {
                ui.label("Scan LAN");
                ui.add(egui::TextEdit::singleline(&mut m.subnet).desired_width(110.0));
                if m.scan_rx.is_some() {
                    ui.spinner();
                } else if ui.button("🔍 Find machines").clicked() {
                    let (tx, rx) = unbounded();
                    let prefix = m.subnet.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(ezyx_machine::discovery::scan_subnet(&prefix, 300));
                    });
                    m.scan_rx = Some(rx);
                }
            });
            let mut pick = None;
            for e in &m.found {
                if ui.button(e.label()).clicked() {
                    pick = Some(e.clone());
                }
            }
            match pick {
                Some(Endpoint::Tcp { host, port }) => {
                    m.host = host;
                    m.tcp_port = port;
                    m.link = Link::WifiTcp;
                }
                Some(Endpoint::WebSocket { url }) => {
                    m.ws_url = url;
                    m.link = Link::WifiWs;
                }
                _ => {}
            }
        }
        if ui.add_enabled(ep.is_some(), egui::Button::new(RichText::new("Connect").strong()).fill(theme::ACCENT_SOFT)).clicked() {
            if let Some(ep) = ep {
                match Controller::connect(ep) {
                    Ok(c) => {
                        c.line(cmd::SETTINGS);
                        m.controller = Some(c);
                        m.error = None;
                    }
                    Err(e) => m.error = Some(e.to_string()),
                }
            }
        }
        if let Some(e) = &m.error {
            ui.colored_label(theme::DANGER, e);
        }
        return false;
    }

    // ---- connected
    let st = m.status.lock().ok().and_then(|s| s.clone());
    ui.horizontal(|ui| {
        ui.label(RichText::new(m.controller.as_ref().map(|c| c.endpoint.label()).unwrap_or_default()).strong());
        let (txt, col) = match st.as_ref().map(|s| s.state) {
            Some(MState::Idle) => ("IDLE", theme::OK),
            Some(MState::Run) => ("RUN", theme::ACCENT),
            Some(MState::Hold) => ("HOLD", theme::WARN),
            Some(MState::Alarm) => ("ALARM", theme::DANGER),
            Some(MState::Jog) => ("JOG", theme::STEEL),
            Some(MState::Home) => ("HOMING", theme::STEEL),
            Some(other) => (if other == MState::Door { "DOOR" } else { "…" }, theme::WARN),
            None => ("…", theme::STEEL),
        };
        ui.label(RichText::new(txt).strong().color(Color32::WHITE).background_color(col).size(16.0));
        if ui.button("Disconnect").clicked() {
            m.controller = None;
        }
    });
    m.controller.is_some()
}

/// Position read-out, home / unlock / zero, jog pad and Z probe.
pub fn dro_jog(app: &mut EzApp, ui: &mut Ui) {
    let m = &mut app.machine;
    let st = m.status.lock().ok().and_then(|s| s.clone());
    if let Some(s) = &st {
        if let Some(p) = s.work_pos() {
            egui::Grid::new("dro").num_columns(3).show(ui, |ui| {
                for (i, a) in ["X", "Y", "Z", "A"].iter().enumerate() {
                    if i == 3 && p[3] == 0.0 {
                        continue;
                    }
                    ui.label(RichText::new(*a).strong().size(24.0));
                    ui.label(RichText::new(format!("{:>9.3}", p[i])).monospace().size(26.0));
                    if ui.small_button("Zero").clicked() {
                        let l = cmd::zero(i == 0, i == 1, i == 2, i == 3);
                        m.line(l);
                    }
                    ui.end_row();
                }
            });
            ui.label(
                RichText::new(format!(
                    "Feed {:.0} · Spindle {:.0}{}",
                    s.feed,
                    s.spindle,
                    s.overrides.map(|o| format!(" · Ov {}%/{}%/{}%", o[0], o[1], o[2])).unwrap_or_default()
                ))
                .small(),
            );
        }
        if s.state == MState::Alarm {
            ui.colored_label(theme::DANGER, "Alarm: home the machine ($H) or unlock ($X) once safe.");
        }
    }
    ui.horizontal(|ui| {
        if ui.button("🏠 Home").clicked() {
            m.line(cmd::HOME);
        }
        if ui.button("🔓 Unlock").clicked() {
            m.line(cmd::UNLOCK);
        }
        if ui.button("Zero XYZ").clicked() {
            m.line(cmd::zero(true, true, true, false));
        }
        if ui.button("⟲ Reset").clicked() {
            m.rt(rt::RESET);
        }
    });
    ui.separator();
    ui.label(RichText::new("Jog").strong());
    ui.horizontal(|ui| {
        for s in [0.1, 1.0, 10.0, 50.0] {
            ui.selectable_value(&mut m.step, s, format!("{s}"));
        }
        ui.add(egui::DragValue::new(&mut m.jog_feed).range(10.0..=10000.0).suffix(" mm/min"));
    });
    let step = m.step;
    let feed = m.jog_feed;
    let mut jog: Option<String> = None;
    egui::Grid::new("jog").spacing([4.0, 4.0]).show(ui, |ui| {
        let b = |ui: &mut Ui, t: &str| ui.add_sized([58.0, 46.0], egui::Button::new(RichText::new(t).size(18.0))).clicked();
        ui.label("");
        if b(ui, "Y+") {
            jog = Some(cmd::jog(None, Some(step), None, None, feed));
        }
        ui.label("");
        ui.label("");
        if b(ui, "Z+") {
            jog = Some(cmd::jog(None, None, Some(step), None, feed * 0.5));
        }
        ui.end_row();
        if b(ui, "X−") {
            jog = Some(cmd::jog(Some(-step), None, None, None, feed));
        }
        if b(ui, "⊙") {
            jog = Some(cmd::jog_to(Some(0.0), Some(0.0), None, feed));
        }
        if b(ui, "X+") {
            jog = Some(cmd::jog(Some(step), None, None, None, feed));
        }
        ui.label("");
        ui.label("");
        ui.end_row();
        ui.label("");
        if b(ui, "Y−") {
            jog = Some(cmd::jog(None, Some(-step), None, None, feed));
        }
        ui.label("");
        ui.label("");
        if b(ui, "Z−") {
            jog = Some(cmd::jog(None, None, Some(-step), None, feed * 0.5));
        }
        ui.end_row();
        if app.project.machine.has_rotary {
            if b(ui, "A−") {
                jog = Some(cmd::jog(None, None, None, Some(-step * 10.0), feed));
            }
            if b(ui, "A+") {
                jog = Some(cmd::jog(None, None, None, Some(step * 10.0), feed));
            }
            ui.end_row();
        }
    });
    if let Some(j) = jog {
        m.line(j);
    }
    ui.horizontal(|ui| {
        ui.label("Touch plate");
        ui.add(egui::DragValue::new(&mut m.plate).range(0.0..=50.0).suffix(" mm"));
        if ui.button("⬇ Probe Z").on_hover_text("Attach the clip; the bit will move down until it touches the plate").clicked() {
            for l in cmd::probe_z(m.plate, 40.0, 100.0, 5.0) {
                m.line(l);
            }
        }
    });
    if let Some(t) = grbl::travel_from_settings(&m.settings) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Machine travel: {:.0} × {:.0} × {:.0} mm", t[0], t[1], t[2])).small());
            if app.project.machine.travel != t && ui.small_button("Use in profile").clicked() {
                app.project.machine.travel = t;
                app.project.machine.verify = false;
            }
        });
    }
}

/// Run controls for the selected queued job, progress and overrides.
pub fn job_controls(app: &mut EzApp, ui: &mut Ui) {
    let m = &mut app.machine;
    let (acked, total, state) = m.progress;
    if total > 0 {
        ui.add(egui::ProgressBar::new(acked as f32 / total as f32).text(format!("{acked}/{total} · {state:?}")));
    }
    ui.horizontal(|ui| {
        let running = matches!(state, JobState::Running | JobState::Paused);
        let can_run = !running && m.controller.is_some() && m.selected_job.is_some();
        if ui
            .add_enabled(can_run, egui::Button::new(RichText::new("▶ Run").strong().size(16.0)).fill(theme::ACCENT_SOFT))
            .on_hover_text("Stream the selected job")
            .clicked()
        {
            if let Some(j) = m.selected_job.and_then(|i| m.jobs.get(i)) {
                m.pending_job = Some(j.gcode.clone());
                m.running_job = m.selected_job;
            }
        }
        if ui.add_enabled(running, egui::Button::new("⏸ Pause")).clicked() {
            if let Some(c) = &m.controller {
                c.send(MCmd::Pause);
            }
        }
        if ui.add_enabled(running, egui::Button::new("⏵ Resume")).clicked() {
            if let Some(c) = &m.controller {
                c.send(MCmd::Resume);
            }
        }
        if ui.add(egui::Button::new(RichText::new("⏹ STOP").color(Color32::WHITE)).fill(theme::DANGER)).clicked() {
            if let Some(c) = &m.controller {
                c.send(MCmd::Stop);
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Feed");
        if ui.small_button("−10%").clicked() {
            m.rt(rt::FEED_MINUS_10);
        }
        if ui.small_button("100%").clicked() {
            m.rt(rt::FEED_100);
        }
        if ui.small_button("+10%").clicked() {
            m.rt(rt::FEED_PLUS_10);
        }
        ui.label("Spindle");
        if ui.small_button("−10%").clicked() {
            m.rt(rt::SPINDLE_MINUS_10);
        }
        if ui.small_button("+10%").clicked() {
            m.rt(rt::SPINDLE_PLUS_10);
        }
    });
    if let Some(e) = &m.error {
        ui.colored_label(theme::WARN, e);
    }
}

/// Console log and manual command entry.
pub fn console(app: &mut EzApp, ui: &mut Ui) {
    let m = &mut app.machine;
    egui::ScrollArea::vertical().max_height(ui.available_height() - 34.0).stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
        for l in &m.console {
            ui.label(RichText::new(l).monospace().small());
        }
    });
    ui.horizontal(|ui| {
        let running = matches!(m.progress.2, JobState::Running | JobState::Paused);
        let r = ui.add_enabled(!running, egui::TextEdit::singleline(&mut m.input).hint_text("G-code or $ command").desired_width(320.0));
        if (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || ui.button("Send").clicked() {
            let l = std::mem::take(&mut m.input);
            if !l.trim().is_empty() {
                m.line(l);
            }
        }
    });
}

/// "Are you sure?" before streaming a job — the machine is about to move.
pub fn confirm_dialog(app: &mut EzApp, ctx: &egui::Context) {
    let Some(g) = app.machine.pending_job.clone() else { return };
    let mut decision = None;
    egui::Window::new("Start job?").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        let secs = ezyx_gcode::parse::estimate_seconds(&ezyx_gcode::parse(&g), app.project.machine.limits.max_feed);
        if let Some(j) = app.machine.running_job.and_then(|i| app.machine.jobs.get(i)) {
            ui.label(RichText::new(&j.name).strong());
            ui.label(format!("X {:.1}…{:.1}  Y {:.1}…{:.1}  Z down to {:.2} mm", j.min[0], j.max[0], j.min[1], j.max[1], j.min[2]));
        }
        ui.label(format!("{} lines · about {:.0} min", g.lines().count(), secs / 60.0));
        ui.label("✔ Stock clamped, ✔ correct bit installed, ✔ XYZ zero set, ✔ dust collection & eye protection on.");
        ui.horizontal(|ui| {
            if ui.button(RichText::new("▶ Start").strong()).clicked() {
                decision = Some(true);
            }
            if ui.button("Cancel").clicked() {
                decision = Some(false);
            }
        });
    });
    match decision {
        Some(true) => {
            if let Some(c) = &app.machine.controller {
                c.start_job(&g);
            }
            app.machine.pending_job = None;
        }
        Some(false) => {
            app.machine.pending_job = None;
            app.machine.running_job = None;
        }
        None => {}
    }
}
