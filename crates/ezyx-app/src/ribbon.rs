//! Ribbon: module tabs + grouped big buttons. Returns [`Action`]s for the app to execute, which
//! keeps all state changes in one place (and lets the AI trigger the same actions later).

use crate::theme;
use crate::viewport::ViewPreset;
use egui::{Align, Button, Color32, Layout, RichText, Ui, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Home,
    Design2D,
    Carve25D,
    Model3D,
    Mesh,
    Image,
    Toolpaths,
    Simulate,
    Machine,
}

impl Tab {
    pub const ALL: [Tab; 9] = [Tab::Home, Tab::Design2D, Tab::Carve25D, Tab::Model3D, Tab::Mesh, Tab::Image, Tab::Toolpaths, Tab::Simulate, Tab::Machine];
    pub fn label(self) -> &'static str {
        match self {
            Tab::Home => "🏠 Home",
            Tab::Design2D => "✏ 2D Design",
            Tab::Carve25D => "⛰ 2.5D",
            Tab::Model3D => "🧊 3D Model",
            Tab::Mesh => "🔺 Mesh",
            Tab::Image => "🖼 Image",
            Tab::Toolpaths => "🛠 Toolpaths",
            Tab::Simulate => "👁 Preview",
            Tab::Machine => "🎮 Machine",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Tab::Home => "Files, job & stock setup, machine and material",
            Tab::Design2D => "Draw and edit vectors: shapes, text, offsets, booleans",
            Tab::Carve25D => "Reliefs: image → relief, domes from vectors",
            Tab::Model3D => "Design 3D parts with a feature tree (extrude, revolve, fillet, patterns)",
            Tab::Mesh => "Import / repair / tet-mesh 3D models",
            Tab::Image => "Image → vectors, sketch → cut, photo V-carve, image → 3D",
            Tab::Toolpaths => "Turn designs into toolpaths and G-code",
            Tab::Simulate => "See the predicted finished piece in your material",
            Tab::Machine => "Connect over USB or Wi-Fi, jog, zero, run jobs",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawTool {
    Select,
    Rect,
    Circle,
    Polyline,
    Text,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prim {
    Box,
    Cylinder,
    Sphere,
    Cone,
    Torus,
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
    Tool(DrawTool),
    AddStar,
    AddPolygon,
    Offset,
    Union,
    Subtract,
    Intersect,
    Center,
    Delete,
    ReliefFromImage,
    DomeFromVectors,
    DemoPart,
    NewPart,
    Primitive(Prim),
    Extrude,
    Revolve,
    PartToRelief,
    ExportPartStl,
    PartTetMesh,
    MeshFit,
    MeshSmooth,
    ExportMeshStl,
    MeshToRelief,
    MeshTet,
    LoadImage,
    TraceOutline,
    TraceCenterline,
    ImageRelief,
    PhotoVCarve,
    ImageTo3d,
    AddOp(OpTemplate),
    Calculate,
    ExportGcode,
    Preview,
    ClearPreview,
    View(ViewPreset),
    Toggle2D,
    Fit,
    ToggleAi,
    ToggleDark,
    About,
}

fn big(ui: &mut Ui, icon: &str, label: &str, tip: &str, out: &mut Vec<Action>, a: Action) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(70.0, 56.0), egui::Sense::click());
    let resp = resp.on_hover_text(tip);
    if ui.is_rect_visible(rect) {
        let vis = ui.style().interact(&resp);
        let p = ui.painter();
        if resp.hovered() || resp.is_pointer_button_down_on() {
            p.rect_filled(rect, 6.0, vis.weak_bg_fill);
            p.rect_stroke(rect, 6.0, egui::Stroke::new(1.0, theme::ACCENT), egui::StrokeKind::Inside);
        }
        let col = vis.text_color();
        p.text(rect.center_top() + egui::vec2(0.0, 18.0), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(21.0), col);
        p.text(rect.center_bottom() - egui::vec2(0.0, 11.0), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(11.5), col);
    }
    if resp.clicked() {
        out.push(a);
    }
}

fn group(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            add(ui)
        });
        ui.label(RichText::new(title).small().color(theme::STEEL));
    });
    ui.separator();
}

pub fn tabs(ui: &mut Ui, tab: &mut Tab, ai_open: bool, out: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("EZ-YX").strong().color(theme::ACCENT).size(16.0));
        ui.label(RichText::new("craft").color(theme::STEEL).size(16.0));
        ui.add_space(12.0);
        for t in Tab::ALL {
            let sel = *tab == t;
            let txt = if sel { RichText::new(t.label()).strong().color(Color32::WHITE) } else { RichText::new(t.label()) };
            let btn = Button::new(txt).fill(if sel { theme::ACCENT } else { Color32::TRANSPARENT });
            if ui.add(btn).on_hover_text(t.hint()).clicked() {
                *tab = t;
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let ai = Button::new(RichText::new("🤖 Assistant").strong()).fill(if ai_open { theme::ACCENT_SOFT } else { Color32::TRANSPARENT });
            if ui.add(ai).on_hover_text("AI assistant (Claude / OpenAI / Ollama)").clicked() {
                out.push(Action::ToggleAi);
            }
            if ui.button("🌓").on_hover_text("Light / dark").clicked() {
                out.push(Action::ToggleDark);
            }
            if ui.button("↪").on_hover_text("Redo (Ctrl+Y)").clicked() {
                out.push(Action::Redo);
            }
            if ui.button("↩").on_hover_text("Undo (Ctrl+Z)").clicked() {
                out.push(Action::Undo);
            }
        });
    });
}

pub fn ribbon(ui: &mut Ui, tab: Tab, out: &mut Vec<Action>) {
    egui::ScrollArea::horizontal().id_salt("ribbon").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.set_min_height(74.0);
            match tab {
                Tab::Home => {
                    group(ui, "File", |ui| {
                        big(ui, "🗋", "New", "New project (job setup)", out, Action::New);
                        big(ui, "📂", "Open", "Open .ezyx project", out, Action::Open);
                        big(ui, "💾", "Save", "Save (Ctrl+S)", out, Action::Save);
                        big(ui, "💾", "Save As", "Save as…", out, Action::SaveAs);
                    });
                    group(ui, "Import", |ui| {
                        big(ui, "📐", "SVG/DXF", "Import vectors", out, Action::ImportVectors);
                        big(ui, "🔺", "STL/OBJ", "Import 3D model", out, Action::ImportMesh);
                        big(ui, "🖼", "Image", "Load an image for tracing / relief / photo V-carve", out, Action::LoadImage);
                    });
                    group(ui, "View", |ui| {
                        big(ui, "⬜", "2D / 3D", "Toggle 2D drafting and 3D orbit view (Tab)", out, Action::Toggle2D);
                        big(ui, "⛶", "Fit", "Zoom to fit (F)", out, Action::Fit);
                        big(ui, "ℹ", "About", "About EZ-YX-craft", out, Action::About);
                    });
                }
                Tab::Design2D => {
                    group(ui, "Draw", |ui| {
                        big(ui, "⬉", "Select", "Select vectors (click, Shift+click)", out, Action::Tool(DrawTool::Select));
                        big(ui, "▭", "Rectangle", "Drag to draw a rectangle", out, Action::Tool(DrawTool::Rect));
                        big(ui, "◯", "Circle", "Drag from the centre", out, Action::Tool(DrawTool::Circle));
                        big(ui, "〰", "Polyline", "Click points; double-click to finish (closes near start)", out, Action::Tool(DrawTool::Polyline));
                        big(ui, "🅰", "Text", "Click to place text", out, Action::Tool(DrawTool::Text));
                        big(ui, "⭐", "Star", "Add a star at the stock centre", out, Action::AddStar);
                        big(ui, "⬡", "Polygon", "Add a hexagon at the stock centre", out, Action::AddPolygon);
                    });
                    group(ui, "Edit", |ui| {
                        big(ui, "⧉", "Offset", "Offset selected vectors (inspector)", out, Action::Offset);
                        big(ui, "∪", "Weld", "Union selected", out, Action::Union);
                        big(ui, "∖", "Subtract", "First selected minus the rest", out, Action::Subtract);
                        big(ui, "∩", "Intersect", "Keep overlap", out, Action::Intersect);
                        big(ui, "⊕", "Center", "Centre selection on the stock", out, Action::Center);
                        big(ui, "🗑", "Delete", "Delete selection (Del)", out, Action::Delete);
                    });
                }
                Tab::Carve25D => {
                    group(ui, "Reliefs", |ui| {
                        big(ui, "🖼", "From Image", "Grey-scale image → relief", out, Action::ReliefFromImage);
                        big(ui, "◒", "Dome", "Inflate selected closed vectors into a pillow/dome relief", out, Action::DomeFromVectors);
                        big(ui, "🔺", "From Mesh", "Project the selected mesh into a relief", out, Action::MeshToRelief);
                    });
                    group(ui, "Machine it", |ui| {
                        big(ui, "🪓", "Rough 3D", "3D roughing of all reliefs", out, Action::AddOp(OpTemplate::Rough3d));
                        big(ui, "✨", "Finish 3D", "3D finishing of all reliefs", out, Action::AddOp(OpTemplate::Finish3d));
                    });
                }
                Tab::Model3D => {
                    group(ui, "Part", |ui| {
                        big(ui, "🧊", "New Part", "Start an empty part", out, Action::NewPart);
                        big(ui, "⚙", "Demo", "Add a demo part (plate + knob + holes)", out, Action::DemoPart);
                    });
                    group(ui, "Features", |ui| {
                        big(ui, "⬛", "Box", "Add box feature", out, Action::Primitive(Prim::Box));
                        big(ui, "🛢", "Cylinder", "Add cylinder feature", out, Action::Primitive(Prim::Cylinder));
                        big(ui, "⚪", "Sphere", "Add sphere", out, Action::Primitive(Prim::Sphere));
                        big(ui, "🔻", "Cone", "Add cone", out, Action::Primitive(Prim::Cone));
                        big(ui, "🍩", "Torus", "Add torus", out, Action::Primitive(Prim::Torus));
                        big(ui, "⬆", "Extrude", "Extrude selected vectors", out, Action::Extrude);
                        big(ui, "🔄", "Revolve", "Revolve selected vectors around Z (draw profile at x ≥ 0)", out, Action::Revolve);
                    });
                    group(ui, "Output", |ui| {
                        big(ui, "⛰", "To Relief", "Carve this part (3D toolpaths)", out, Action::PartToRelief);
                        big(ui, "💾", "STL", "Export part mesh", out, Action::ExportPartStl);
                        big(ui, "🔷", "Tet Mesh", "Volumetric tet mesh (VTK/Gmsh) for analysis", out, Action::PartTetMesh);
                    });
                }
                Tab::Mesh => {
                    group(ui, "Mesh", |ui| {
                        big(ui, "📥", "Import", "Import STL/OBJ", out, Action::ImportMesh);
                        big(ui, "⛶", "Fit Stock", "Scale selected mesh to fit the stock", out, Action::MeshFit);
                        big(ui, "〰", "Smooth", "Taubin smoothing", out, Action::MeshSmooth);
                        big(ui, "💾", "Export", "Export STL", out, Action::ExportMeshStl);
                    });
                    group(ui, "Analysis / CAM", |ui| {
                        big(ui, "⛰", "To Relief", "Top-down relief for 3D carving", out, Action::MeshToRelief);
                        big(ui, "🔷", "Tet Mesh", "Tetrahedral mesh of the relief solid + quality report", out, Action::MeshTet);
                    });
                }
                Tab::Image => {
                    group(ui, "Source", |ui| {
                        big(ui, "🖼", "Load", "Load photo / sketch / logo", out, Action::LoadImage);
                    });
                    group(ui, "Image → Carve", |ui| {
                        big(ui, "✂", "Outline", "Trace shapes to closed vectors (cut / pocket / V-carve)", out, Action::TraceOutline);
                        big(ui, "✒", "Sketch", "Pen sketch → single-line centreline vectors (engrave)", out, Action::TraceCenterline);
                        big(ui, "⛰", "Relief", "Image → 2.5D relief", out, Action::ImageRelief);
                        big(ui, "📷", "Photo V", "Shaded photo → V-carve grooves", out, Action::PhotoVCarve);
                        big(ui, "🧊", "Image→3D", "Silhouette → 3D model (offline); plug in AI backends in Settings", out, Action::ImageTo3d);
                    });
                }
                Tab::Toolpaths => {
                    group(ui, "2D", |ui| {
                        big(ui, "⬚", "Outside", "Profile outside (cut out)", out, Action::AddOp(OpTemplate::ProfileOutside));
                        big(ui, "⬚", "Inside", "Profile inside", out, Action::AddOp(OpTemplate::ProfileInside));
                        big(ui, "─", "On Line", "Follow the line", out, Action::AddOp(OpTemplate::ProfileOn));
                        big(ui, "▤", "Pocket", "Clear an area", out, Action::AddOp(OpTemplate::Pocket));
                        big(ui, "⚲", "Drill", "Drill circle centres", out, Action::AddOp(OpTemplate::Drill));
                    });
                    group(ui, "Carve", |ui| {
                        big(ui, "∨", "V-Carve", "V-carve text and shapes", out, Action::AddOp(OpTemplate::VCarve));
                        big(ui, "✎", "Engrave", "Engrave on vectors", out, Action::AddOp(OpTemplate::Engrave));
                        big(ui, "🔥", "Laser", "Laser vector engraving", out, Action::AddOp(OpTemplate::Laser));
                    });
                    group(ui, "3D", |ui| {
                        big(ui, "🪓", "Rough", "3D roughing", out, Action::AddOp(OpTemplate::Rough3d));
                        big(ui, "✨", "Finish", "3D finishing", out, Action::AddOp(OpTemplate::Finish3d));
                    });
                    group(ui, "Output", |ui| {
                        big(ui, "⚡", "Calculate", "Compute all toolpaths", out, Action::Calculate);
                        big(ui, "💾", "G-code", "Save G-code for the selected machine", out, Action::ExportGcode);
                    });
                }
                Tab::Simulate => {
                    group(ui, "Predicted result", |ui| {
                        big(ui, "👁", "Preview", "Simulate material removal and render the finished piece in your material", out, Action::Preview);
                        big(ui, "✖", "Clear", "Hide the preview", out, Action::ClearPreview);
                    });
                    group(ui, "Camera", |ui| {
                        big(ui, "⬇", "Top", "Top view", out, Action::View(ViewPreset::Top));
                        big(ui, "⬆", "Front", "Front view", out, Action::View(ViewPreset::Front));
                        big(ui, "➡", "Right", "Right view", out, Action::View(ViewPreset::Right));
                        big(ui, "◩", "Iso", "Isometric", out, Action::View(ViewPreset::Iso));
                        big(ui, "⛶", "Fit", "Fit", out, Action::Fit);
                    });
                }
                Tab::Machine => {
                    ui.label(RichText::new("Connect, jog and run jobs in the panel on the right →").color(theme::STEEL));
                }
            }
        });
    });
}
