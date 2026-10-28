//! Shared GUI vocabulary: pipeline steps, module windows, [`Action`]s and the ribbon widgets.
//! Ribbons and panels only *emit* actions; `EzApp::handle` executes them, which keeps every state
//! change in one place (and lets the AI or the docs-screenshot script trigger the same actions).

use crate::theme;
use crate::viewport::ViewPreset;
use egui::{Button, Color32, RichText, Ui, Vec2};
use ezyx_project::pipeline::CutStrategy;

/// The four steps of the main pipeline window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Stock,
    Design,
    Toolpaths,
    Output,
}

impl Step {
    pub const ALL: [Step; 4] = [Step::Stock, Step::Design, Step::Toolpaths, Step::Output];
    pub fn label(self) -> &'static str {
        match self {
            Step::Stock => "① Stock",
            Step::Design => "② Design",
            Step::Toolpaths => "③ Toolpaths",
            Step::Output => "④ Output",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Step::Stock => "Material, shape and size of the blank, datum, machine",
            Step::Design => "Draw, import and carve features (or bring them in from the Image Studio / Solid Modeler)",
            Step::Toolpaths => "Automatic roughing + finishing per feature; change bits, preview and simulate",
            Step::Output => "Download G-code for your machine or send it to Machine Control",
        }
    }
    pub fn next(self) -> Option<Step> {
        match self {
            Step::Stock => Some(Step::Design),
            Step::Design => Some(Step::Toolpaths),
            Step::Toolpaths => Some(Step::Output),
            Step::Output => None,
        }
    }
    pub fn parse(s: &str) -> Option<Step> {
        Some(match s {
            "stock" => Step::Stock,
            "design" => Step::Design,
            "toolpaths" => Step::Toolpaths,
            "output" => Step::Output,
            _ => return None,
        })
    }
}

/// Separate modules that run in their own windows and feed the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Module {
    Image,
    Solid,
    Machine,
}

impl Module {
    pub const ALL: [Module; 3] = [Module::Image, Module::Solid, Module::Machine];
    pub fn label(self) -> &'static str {
        match self {
            Module::Image => "🖼 Image Studio",
            Module::Solid => "🧊 Solid Modeler",
            Module::Machine => "🎮 Machine Control",
        }
    }
    pub fn feature_id(self) -> &'static str {
        match self {
            Module::Image => "image.studio",
            Module::Solid => "solid.modeler",
            Module::Machine => "machine.control",
        }
    }
    pub fn title(self) -> String {
        format!("EZ-YX-craft — {}", &self.label()[self.label().find(' ').map(|i| i + 1).unwrap_or(0)..])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawTool {
    Select,
    Rect,
    Circle,
    Ellipse,
    Polyline,
    Text,
    Measure,
}

/// Hand-made operations (Toolpaths → Advanced).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpTemplate {
    ProfileOutside,
    ProfileInside,
    ProfileOn,
    Pocket,
    VCarve,
    Drill,
    Engrave,
    Finish3d,
    Rough3d,
    Laser,
}

/// Which passes the viewer and the simulation show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PassFilter {
    Rough,
    Finish,
    #[default]
    Both,
}

impl PassFilter {
    pub fn shows(self, pass: ezyx_tooldb::Operation) -> bool {
        match self {
            PassFilter::Both => true,
            PassFilter::Rough => pass == ezyx_tooldb::Operation::Roughing,
            PassFilter::Finish => pass == ezyx_tooldb::Operation::Finishing,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    CenterX,
    Right,
    Top,
    CenterY,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    New,
    Open,
    Save,
    SaveAs,
    ImportVectors,
    ImportMesh,
    Undo,
    Redo,
    GoTo(Step),
    NextStep,
    StockPreset(usize),
    Tool(DrawTool),
    AddStar,
    AddPolygon,
    Offset,
    Union,
    Subtract,
    Intersect,
    Center,
    Align(Align),
    Distribute { horizontal: bool },
    Mirror { horizontal: bool },
    Delete,
    SelectAll,
    Carve(CutStrategy),
    OpenModule(Module),
    EditFeature(u64),
    AutoPlanAll,
    Calculate,
    AddOp(OpTemplate),
    ShowPasses(PassFilter),
    Simulate,
    FinalRender,
    ClearPreview,
    ExportGcode,
    JobSheet,
    SendToMachine,
    View(ViewPreset),
    Toggle2D,
    Fit,
    ToggleAi,
    ToggleDark,
    TileWindows,
    Settings,
    About,
}

/// Big ribbon button with icon + label. `tip` is usually a registry summary.
pub fn big(ui: &mut Ui, icon: &str, label: &str, tip: &str, out: &mut Vec<Action>, a: Action) {
    big_sel(ui, icon, label, tip, false, out, a)
}

pub fn big_sel(ui: &mut Ui, icon: &str, label: &str, tip: &str, selected: bool, out: &mut Vec<Action>, a: Action) {
    if big_button(ui, icon, label, tip, selected) {
        out.push(a);
    }
}

pub fn big_button(ui: &mut Ui, icon: &str, label: &str, tip: &str, selected: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(70.0, 56.0), egui::Sense::click());
    let resp = resp.on_hover_text(tip);
    if ui.is_rect_visible(rect) {
        let vis = ui.style().interact(&resp);
        let p = ui.painter();
        if selected {
            p.rect_filled(rect, 6.0, theme::ACCENT_SOFT);
        }
        if resp.hovered() || resp.is_pointer_button_down_on() {
            p.rect_filled(rect, 6.0, vis.weak_bg_fill);
            p.rect_stroke(rect, 6.0, egui::Stroke::new(1.0, theme::ACCENT), egui::StrokeKind::Inside);
        }
        let col = if selected { theme::WALNUT } else { vis.text_color() };
        p.text(rect.center_top() + egui::vec2(0.0, 18.0), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(21.0), col);
        p.text(rect.center_bottom() - egui::vec2(0.0, 11.0), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(11.5), col);
    }
    resp.clicked()
}

pub fn group(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            add(ui)
        });
        ui.label(RichText::new(title).small().color(theme::STEEL));
    });
    ui.separator();
}

/// Ribbon strip (scrolls horizontally on narrow windows).
pub fn strip(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    egui::ScrollArea::horizontal().id_salt(id).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.set_min_height(74.0);
            add(ui)
        });
    });
}

/// Pill-style tab button used by the step bar and module tab bars.
pub fn pill(ui: &mut Ui, text: &str, selected: bool, tip: &str) -> bool {
    let txt = if selected { RichText::new(text).strong().color(Color32::WHITE) } else { RichText::new(text) };
    ui.add(Button::new(txt).fill(if selected { theme::ACCENT } else { Color32::TRANSPARENT }).corner_radius(12)).on_hover_text(tip).clicked()
}

/// Registry tooltip shorthand.
pub fn tip(id: &str) -> String {
    ezyx_project::registry::tip(id)
}
