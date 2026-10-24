//! Visual identity: warm workshop palette (walnut, maple, steel, signal orange).

use egui::{Color32, CornerRadius, Stroke, Visuals};

pub const ACCENT: Color32 = Color32::from_rgb(232, 120, 40); // signal orange
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(250, 214, 180);
pub const STEEL: Color32 = Color32::from_rgb(92, 108, 124);
pub const WALNUT: Color32 = Color32::from_rgb(74, 52, 38);
pub const MAPLE: Color32 = Color32::from_rgb(242, 230, 208);
pub const OK: Color32 = Color32::from_rgb(70, 160, 90);
pub const WARN: Color32 = Color32::from_rgb(220, 160, 40);
pub const DANGER: Color32 = Color32::from_rgb(210, 60, 50);
pub const VECTOR: Color32 = Color32::from_rgb(30, 30, 36);
pub const VECTOR_SEL: Color32 = Color32::from_rgb(232, 120, 40);
pub const RAPID: Color32 = Color32::from_rgb(220, 70, 60);

pub fn apply(ctx: &egui::Context, dark: bool) {
    let mut v = if dark { Visuals::dark() } else { Visuals::light() };
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = ACCENT;
    let r = CornerRadius::same(6);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.corner_radius = r;
    v.window_corner_radius = CornerRadius::same(10);
    if !dark {
        v.panel_fill = Color32::from_rgb(247, 244, 239);
        v.window_fill = Color32::from_rgb(252, 250, 247);
        v.extreme_bg_color = Color32::WHITE;
        v.faint_bg_color = Color32::from_rgb(238, 233, 225);
    } else {
        v.panel_fill = Color32::from_rgb(30, 30, 33);
    }
    ctx.set_visuals(v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
    });
}

pub fn viewport_bg(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(38, 40, 44)
    } else {
        Color32::from_rgb(226, 230, 234)
    }
}

pub fn depth_color(t: f32) -> Color32 {
    // Shallow = teal, deep = deep blue/violet.
    let t = t.clamp(0.0, 1.0);
    let a = [40.0, 170.0, 180.0];
    let b = [70.0, 40.0, 170.0];
    Color32::from_rgb((a[0] + (b[0] - a[0]) * t) as u8, (a[1] + (b[1] - a[1]) * t) as u8, (a[2] + (b[2] - a[2]) * t) as u8)
}
