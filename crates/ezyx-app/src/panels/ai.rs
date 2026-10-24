//! AI assistant panel. Runs the agent on a background thread with a copy of the project; tool
//! calls that move the machine or write files pop up a confirmation dialog on the UI thread.

use crate::app::EzApp;
use crate::theme;
use crossbeam_channel::{unbounded, Receiver, Sender};
use egui::{RichText, Ui};
use ezyx_ai::{agent::AgentEvent, ChatMessage, ProviderConfig, ProviderKind, ToolHost, ToolSpec};
use ezyx_machine::{controller::Command as MCmd, grbl::cmd, StatusReport};
use ezyx_project::{agent_host::ProjectHost, History, Project};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

pub enum AiMsg {
    Event(AgentEvent),
    Confirm(String, Value, Sender<bool>),
    Done { project: Box<Project>, convo: Vec<ChatMessage>, reply: Result<String, String>, changed: bool },
}

pub struct AiState {
    pub config: ProviderConfig,
    pub remember_key: bool,
    pub transcript: Vec<(String, String)>,
    pub input: String,
    pub busy: bool,
    pub rx: Option<Receiver<AiMsg>>,
    pub confirm: Option<(String, Value, Sender<bool>)>,
    pub convo: Vec<ChatMessage>,
    pub show_settings: bool,
}

fn settings_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("ez-yx-craft").join("assistant.json"))
}

impl AiState {
    pub fn load() -> Self {
        let config =
            settings_path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str::<ProviderConfig>(&s).ok()).unwrap_or_default();
        let remember_key = config.api_key.is_some();
        Self {
            config,
            remember_key,
            transcript: vec![(
                "assistant".into(),
                "Hi! I can design, set up toolpaths, explain feeds & speeds, and (with your OK) run the machine. Try: \"Make a 250×120 mm maple sign that says WORKSHOP with a V-carved border and cut it out.\"".into(),
            )],
            input: String::new(),
            busy: false,
            rx: None,
            confirm: None,
            convo: vec![],
            show_settings: false,
        }
    }

    fn save(&self) {
        if let Some(p) = settings_path() {
            let _ = std::fs::create_dir_all(p.parent().unwrap());
            let mut c = self.config.clone();
            if !self.remember_key {
                c.api_key = None;
            }
            let _ = std::fs::write(p, serde_json::to_string_pretty(&c).unwrap_or_default());
        }
    }

    pub fn poll(&mut self, project: &mut Project, history: &mut History, rev: &mut u64) {
        let Some(rx) = &self.rx else { return };
        let msgs: Vec<AiMsg> = rx.try_iter().collect();
        for m in msgs {
            match m {
                AiMsg::Event(AgentEvent::ToolCall { name, input }) => self.transcript.push(("tool".into(), format!("🔧 {name} {input}"))),
                AiMsg::Event(AgentEvent::ToolResult { name, ok, output }) => {
                    let short: String = output.chars().take(300).collect();
                    self.transcript.push(("tool".into(), format!("{} {name}: {short}", if ok { "✔" } else { "✖" })));
                }
                AiMsg::Event(AgentEvent::AssistantText(_)) => {}
                AiMsg::Confirm(n, i, tx) => self.confirm = Some((n, i, tx)),
                AiMsg::Done { project: p, convo, reply, changed } => {
                    self.busy = false;
                    self.convo = convo;
                    match reply {
                        Ok(t) => self.transcript.push(("assistant".into(), t)),
                        Err(e) => self.transcript.push(("error".into(), e)),
                    }
                    if changed {
                        history.checkpoint(project, "assistant changes");
                        *project = *p;
                        *rev += 1;
                    }
                }
            }
        }
        if !self.busy {
            self.rx = None;
        }
    }
}

/// Wraps the project host to stream events to the UI and add machine tools.
struct LiveHost<'a> {
    inner: ProjectHost<'a>,
    tx: Sender<AiMsg>,
    machine: Option<Sender<MCmd>>,
    status: Arc<Mutex<Option<StatusReport>>>,
}

fn machine_specs() -> Vec<ToolSpec> {
    let s = |n: &str, d: &str, schema: Value, c: bool| ToolSpec { name: n.into(), description: d.into(), input_schema: schema, requires_confirmation: c };
    vec![
        s("machine_status", "Connection state, machine state and work position.", json!({"type": "object", "properties": {}}), false),
        s("machine_home", "Run the homing cycle ($H).", json!({"type": "object", "properties": {}}), true),
        s(
            "machine_jog",
            "Jog relative distances in mm.",
            json!({"type": "object", "properties": {"x": {"type": "number"}, "y": {"type": "number"}, "z": {"type": "number"}, "feed": {"type": "number"}}}),
            true,
        ),
        s(
            "machine_zero",
            "Set current position as work zero for the given axes.",
            json!({"type": "object", "properties": {"x": {"type": "boolean"}, "y": {"type": "boolean"}, "z": {"type": "boolean"}}}),
            true,
        ),
        s("machine_start_job", "Post-process the project and stream it to the connected machine.", json!({"type": "object", "properties": {}}), true),
    ]
}

impl ToolHost for LiveHost<'_> {
    fn tools(&self) -> Vec<ToolSpec> {
        let mut t = self.inner.tools();
        t.extend(machine_specs());
        t
    }
    fn call(&mut self, name: &str, i: &Value) -> Result<String, String> {
        if name.starts_with("machine_") {
            if name == "machine_status" {
                return Ok(match (&self.machine, self.status.lock().ok().and_then(|s| s.clone())) {
                    (None, _) => "Not connected. Ask the user to connect in the Machine tab.".into(),
                    (Some(_), Some(s)) => format!("{:?} at {:?}", s.state, s.work_pos()),
                    (Some(_), None) => "Connected, waiting for status.".into(),
                });
            }
            let tx = self.machine.as_ref().ok_or("machine not connected")?;
            let send = |c: MCmd| tx.send(c).map_err(|e| e.to_string());
            return match name {
                "machine_home" => send(MCmd::Line(cmd::HOME.into())).map(|_| "homing".into()),
                "machine_jog" => {
                    let g = |k: &str| i[k].as_f64();
                    send(MCmd::Line(cmd::jog(g("x"), g("y"), g("z"), None, g("feed").unwrap_or(1000.0)))).map(|_| "jogging".into())
                }
                "machine_zero" => {
                    let b = |k: &str| i[k].as_bool().unwrap_or(false);
                    send(MCmd::Line(cmd::zero(b("x"), b("y"), b("z"), false))).map(|_| "zero set".into())
                }
                "machine_start_job" => {
                    let (g, w) = ezyx_project::ops::export_gcode(self.inner.project, self.inner.library)?;
                    send(MCmd::StartJob(ezyx_machine::controller::prepare_job(&g)))?;
                    Ok(format!("Job started ({} lines). Warnings: {}", g.lines().count(), w.join("; ")))
                }
                _ => Err("unknown machine tool".into()),
            };
        }
        self.inner.call(name, i)
    }
    fn confirm(&mut self, name: &str, input: &Value) -> bool {
        let (tx, rx) = unbounded();
        if self.tx.send(AiMsg::Confirm(name.into(), input.clone(), tx)).is_err() {
            return false;
        }
        rx.recv().unwrap_or(false)
    }
    fn on_event(&mut self, e: &AgentEvent) {
        let _ = self.tx.send(AiMsg::Event(e.clone()));
    }
}

fn run(app: &mut EzApp, ctx: &egui::Context) {
    let text = std::mem::take(&mut app.ai.input);
    if text.trim().is_empty() || app.ai.busy {
        return;
    }
    app.ai.transcript.push(("user".into(), text.clone()));
    app.ai.busy = true;
    let (tx, rx) = unbounded();
    app.ai.rx = Some(rx);
    let mut project = app.project.clone();
    let library = app.library.clone();
    let cfg = app.ai.config.clone();
    let convo = app.ai.convo.clone();
    let machine = app.machine.sender();
    let status = app.machine.status.clone();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let mut agent = ezyx_ai::Agent::new(cfg);
        agent.history = convo;
        agent.system.push_str("\n\nCurrent project:\n");
        agent.system.push_str(&project.summary());
        let mut hist = History::default();
        let reply = {
            let inner = ProjectHost { project: &mut project, history: &mut hist, library: &library, confirm: None, events: vec![], extra: None };
            let mut host = LiveHost { inner, tx: tx.clone(), machine, status };
            agent.run_turn(ChatMessage::user(text), &mut host).map_err(|e| e.to_string())
        };
        let changed = !hist.log.is_empty() || hist.can_undo();
        let _ = tx.send(AiMsg::Done { project: Box::new(project), convo: agent.history, reply, changed });
        ctx.request_repaint();
    });
}

pub fn show(app: &mut EzApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.heading("🤖 Assistant");
        if ui.small_button("⚙").on_hover_text("Provider settings").clicked() {
            app.ai.show_settings = !app.ai.show_settings;
        }
        if ui.small_button("🗑").on_hover_text("New conversation").clicked() {
            app.ai.convo.clear();
            app.ai.transcript.truncate(1);
        }
    });
    let s = &mut app.ai;
    if s.show_settings {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                let mut kind = s.config.kind.clone();
                if ui.selectable_value(&mut kind, ProviderKind::Anthropic, "Claude").clicked() {
                    s.config = ProviderConfig { api_key: s.config.api_key.clone(), ..ProviderConfig::anthropic("claude-sonnet-5-5") };
                }
                if ui.selectable_value(&mut kind, ProviderKind::OpenAi, "OpenAI").clicked() {
                    s.config = ProviderConfig { api_key: s.config.api_key.clone(), ..ProviderConfig::openai("gpt-5") };
                }
                if ui.selectable_value(&mut kind, ProviderKind::OpenAiCompatible, "Ollama / local").clicked() {
                    s.config = ProviderConfig::ollama("llama3.1");
                }
            });
            egui::Grid::new("ai_cfg").num_columns(2).show(ui, |ui| {
                ui.label("Model");
                ui.text_edit_singleline(&mut s.config.model);
                ui.end_row();
                ui.label("Base URL");
                ui.text_edit_singleline(&mut s.config.base_url);
                ui.end_row();
                ui.label("API key");
                let mut k = s.config.api_key.clone().unwrap_or_default();
                if ui.add(egui::TextEdit::singleline(&mut k).password(true).hint_text(format!("or set {}", s.config.api_key_env))).changed() {
                    s.config.api_key = (!k.is_empty()).then_some(k);
                }
                ui.end_row();
            });
            ui.checkbox(&mut s.remember_key, "Remember key on this computer (stored in plain text in your config folder)");
            ui.label(
                RichText::new("Uses your own API key or a local model. Chat-app subscriptions generally don't include API access.").small().color(theme::STEEL),
            );
            if ui.button("Save settings").clicked() {
                s.save();
                s.show_settings = false;
            }
        });
    }
    let avail = ui.available_height() - 90.0;
    egui::ScrollArea::vertical().max_height(avail.max(120.0)).stick_to_bottom(true).show(ui, |ui| {
        for (role, text) in &s.transcript {
            let (bg, label) = match role.as_str() {
                "user" => (theme::ACCENT_SOFT, "You"),
                "assistant" => (ui.visuals().faint_bg_color, "Assistant"),
                "tool" => (ui.visuals().extreme_bg_color, ""),
                _ => (egui::Color32::from_rgb(250, 220, 215), "Error"),
            };
            egui::Frame::NONE.fill(bg).corner_radius(8).inner_margin(8).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if !label.is_empty() {
                    ui.label(RichText::new(label).small().strong());
                }
                if role == "tool" {
                    ui.label(RichText::new(text).monospace().small());
                } else {
                    ui.label(text);
                }
            });
            ui.add_space(4.0);
        }
        if s.busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Thinking…");
            });
        }
    });
    let mut go = false;
    ui.horizontal(|ui| {
        let r = ui.add(
            egui::TextEdit::multiline(&mut app.ai.input).desired_rows(2).desired_width(ui.available_width() - 60.0).hint_text("Ask or instruct… (Ctrl+Enter)"),
        );
        if r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command) {
            go = true;
        }
        if ui.add_enabled(!app.ai.busy, egui::Button::new("Send")).clicked() {
            go = true;
        }
    });
    if go {
        let ctx = ui.ctx().clone();
        run(app, &ctx);
    }
}

pub fn confirm_dialog(app: &mut EzApp, ctx: &egui::Context) {
    let Some((name, input, _)) = app.ai.confirm.as_ref().map(|(n, i, t)| (n.clone(), i.clone(), t.clone())) else { return };
    let mut decision = None;
    egui::Window::new("Assistant needs your OK").collapsible(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        ui.label(RichText::new(&name).strong());
        ui.label(RichText::new(input.to_string()).monospace().small());
        if name.starts_with("machine_") {
            ui.colored_label(theme::WARN, "This will move the machine. Make sure the work area is clear.");
        }
        ui.horizontal(|ui| {
            if ui.button("Allow").clicked() {
                decision = Some(true);
            }
            if ui.button("Deny").clicked() {
                decision = Some(false);
            }
        });
    });
    if let Some(d) = decision {
        if let Some((_, _, tx)) = app.ai.confirm.take() {
            let _ = tx.send(d);
        }
    }
}
