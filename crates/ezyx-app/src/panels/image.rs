//! Image module: load a picture, tune processing with live preview, send it to a pipeline.

use crate::app::EzApp;
use crate::ribbon::Action;
use crate::theme;
use egui::{RichText, TextureHandle, Ui};
use ezyx_image::{
    photo_vcarve::{GroovePattern, PhotoVCarveOptions},
    relief::ReliefOptions,
    trace::{ThresholdMode, TraceOptions},
    GrayImage,
};

pub struct ImageState {
    pub path: Option<String>,
    pub gray: Option<GrayImage>,
    pub tex: Option<TextureHandle>,
    pub mask_tex: Option<TextureHandle>,
    pub trace: TraceOptions,
    pub relief: ReliefOptions,
    pub photo: PhotoVCarveOptions,
    pub thickness_3d: f64,
    pub show_mask: bool,
    mask_key: String,
}

impl Default for ImageState {
    fn default() -> Self {
        Self {
            path: None,
            gray: None,
            tex: None,
            mask_tex: None,
            trace: TraceOptions::default(),
            relief: ReliefOptions::default(),
            photo: PhotoVCarveOptions::default(),
            thickness_3d: 12.0,
            show_mask: true,
            mask_key: String::new(),
        }
    }
}

impl ImageState {
    pub fn aspect(&self) -> f64 {
        self.gray.as_ref().map(|g| g.height as f64 / g.width.max(1) as f64).unwrap_or(1.0)
    }

    pub fn pick(&mut self, ctx: &egui::Context) {
        if let Some(p) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "bmp", "gif", "tif", "tiff", "webp"]).pick_file() {
            self.load(ctx, &p.display().to_string());
        }
    }

    pub fn load(&mut self, ctx: &egui::Context, path: &str) {
        match image::open(path) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                let ci = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
                self.tex = Some(ctx.load_texture("ezyx_image", ci, egui::TextureOptions::LINEAR));
                self.gray = Some(GrayImage::from_dynamic_limited(&img, 1600));
                self.path = Some(path.to_string());
                self.mask_key.clear();
            }
            Err(e) => log::warn!("image load failed: {e}"),
        }
    }

    pub fn ensure_loaded(&mut self, ctx: &egui::Context) -> bool {
        if self.path.is_none() {
            self.pick(ctx);
        }
        self.path.is_some()
    }

    fn refresh_mask(&mut self, ctx: &egui::Context) {
        let Some(g) = &self.gray else { return };
        let key = format!("{:?}", (&self.trace.threshold, self.trace.invert, self.trace.blur, self.trace.close_gaps, self.trace.min_area_px));
        if key == self.mask_key {
            return;
        }
        self.mask_key = key;
        let m = ezyx_image::trace::mask(g, &self.trace);
        let px: Vec<egui::Color32> = m.data.iter().map(|b| if *b { theme::WALNUT } else { egui::Color32::from_rgb(250, 246, 238) }).collect();
        self.mask_tex = Some(ctx.load_texture("ezyx_mask", egui::ColorImage::new([m.width, m.height], px), egui::TextureOptions::LINEAR));
    }
}

pub fn show(app: &mut EzApp, ui: &mut Ui, actions: &mut Vec<Action>) {
    let ctx = ui.ctx().clone();
    ui.heading("Image → Carve");
    let s = &mut app.image;
    if s.path.is_none() {
        ui.label("Load a photo, a pencil/pen sketch, or a logo.");
        if ui.button(RichText::new("🖼 Load image…").strong()).clicked() {
            s.pick(&ctx);
        }
        return;
    }
    ui.label(RichText::new(s.path.clone().unwrap_or_default()).small().color(theme::STEEL));
    ui.horizontal(|ui| {
        if ui.button("Change…").clicked() {
            s.pick(&ctx);
        }
        ui.checkbox(&mut s.show_mask, "Show processed");
    });
    s.refresh_mask(&ctx);
    let tex = if s.show_mask { s.mask_tex.as_ref().or(s.tex.as_ref()) } else { s.tex.as_ref() };
    if let Some(t) = tex {
        let w = ui.available_width().min(300.0);
        let size = t.size_vec2();
        ui.image(egui::load::SizedTexture::new(t.id(), egui::vec2(w, w * size.y / size.x.max(1.0))));
    }
    let units = app.project.job.display_units;
    ui.separator();
    ui.label(RichText::new("Clean-up (sketch & logo tracing)").strong());
    let t = &mut s.trace;
    egui::ComboBox::from_label("Threshold")
        .selected_text(match t.threshold {
            ThresholdMode::Auto => "Auto (Otsu)",
            ThresholdMode::Fixed(_) => "Fixed",
            ThresholdMode::Adaptive { .. } => "Adaptive (pencil / uneven light)",
            ThresholdMode::Edges(_) => "Edges (photos)",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut t.threshold, ThresholdMode::Auto, "Auto (Otsu)");
            ui.selectable_value(&mut t.threshold, ThresholdMode::Fixed(128), "Fixed");
            ui.selectable_value(&mut t.threshold, ThresholdMode::Adaptive { radius: 25, offset: 12 }, "Adaptive (pencil / uneven light)");
            ui.selectable_value(&mut t.threshold, ThresholdMode::Edges(60), "Edges (photos)");
        });
    match &mut t.threshold {
        ThresholdMode::Fixed(v) | ThresholdMode::Edges(v) => {
            ui.add(egui::Slider::new(v, 1..=254).text("level"));
        }
        ThresholdMode::Adaptive { radius, offset } => {
            ui.add(egui::Slider::new(radius, 3..=100).text("window px"));
            ui.add(egui::Slider::new(offset, 0..=60).text("offset"));
        }
        _ => {}
    }
    ui.checkbox(&mut t.invert, "Invert (light lines on dark)");
    ui.add(egui::Slider::new(&mut t.blur, 0.0..=4.0).text("smooth paper"));
    ui.add(egui::Slider::new(&mut t.close_gaps, 0..=5).text("close gaps"));
    ui.add(egui::Slider::new(&mut t.min_area_px, 0..=500).text("remove specks"));
    ui.add(egui::Slider::new(&mut t.smooth, 0..=4).text("curve smoothing"));
    ui.horizontal(|ui| {
        ui.label("Width");
        super::len_field(ui, &mut t.width_mm, units, 5.0..=3000.0);
    });
    ui.horizontal(|ui| {
        if ui.button("✂ Trace outline").on_hover_text("Closed shapes for cut-out / pocket / V-carve").clicked() {
            actions.push(Action::TraceOutline);
        }
        if ui.button("✒ Sketch → line").on_hover_text("Single-line centreline for engraving").clicked() {
            actions.push(Action::TraceCenterline);
        }
    });
    ui.separator();
    ui.label(RichText::new("2.5D relief / 3D").strong());
    let r = &mut s.relief;
    ui.horizontal(|ui| {
        ui.label("Width");
        super::len_field(ui, &mut r.width_mm, units, 5.0..=3000.0);
        ui.label("Depth");
        super::len_field(ui, &mut r.depth, units, 0.1..=100.0);
    });
    ui.checkbox(&mut r.bright_is_high, "Bright = high");
    ui.add(egui::Slider::new(&mut r.gamma, 0.3..=3.0).text("gamma"));
    ui.add(egui::Slider::new(&mut r.blur, 0.0..=6.0).text("smooth"));
    ui.horizontal(|ui| {
        ui.label("Resolution");
        super::len_field(ui, &mut r.resolution, units, 0.05..=2.0);
    });
    ui.horizontal(|ui| {
        if ui.button("⛰ Make relief").clicked() {
            actions.push(Action::ImageRelief);
        }
        ui.label("3D thickness");
        super::len_field(ui, &mut s.thickness_3d, units, 1.0..=200.0);
        if ui.button("🧊 Image→3D").on_hover_text("Offline silhouette inflation; transparent PNGs work best").clicked() {
            actions.push(Action::ImageTo3d);
        }
    });
    ui.separator();
    ui.label(RichText::new("Photo V-carve (shaded photos)").strong());
    let p = &mut s.photo;
    ui.horizontal(|ui| {
        ui.label("Width");
        super::len_field(ui, &mut p.width_mm, units, 10.0..=3000.0);
        ui.label("Line spacing");
        super::len_field(ui, &mut p.spacing, units, 0.2..=10.0);
    });
    ui.horizontal(|ui| {
        ui.label("V-bit");
        for a in [30.0, 60.0, 90.0] {
            ui.selectable_value(&mut p.bit_angle, a, format!("{a}°"));
        }
    });
    ui.horizontal(|ui| {
        ui.selectable_value(&mut p.pattern, GroovePattern::Lines, "Lines");
        ui.selectable_value(&mut p.pattern, GroovePattern::Waves, "Waves");
        ui.selectable_value(&mut p.pattern, GroovePattern::Circles, "Circles");
    });
    ui.add(egui::Slider::new(&mut p.angle_deg, 0.0..=180.0).text("angle °"));
    ui.add(egui::Slider::new(&mut p.contrast, 0.3..=3.0).text("contrast"));
    ui.add(egui::Slider::new(&mut p.gamma, 0.3..=3.0).text("gamma"));
    ui.label(RichText::new(format!("Darkest groove depth: {}", units.fmt(p.max_depth()))).small().color(theme::STEEL));
    if ui.button(RichText::new("📷 Create photo V-carve").strong()).clicked() {
        actions.push(Action::PhotoVCarve);
    }
}
