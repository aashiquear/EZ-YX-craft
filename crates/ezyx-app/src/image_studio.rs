//! **Image Studio** — a separate module window: load a picture, pick a conversion (2D outline,
//! single-line sketch, 2.5D relief, photo V-carve, 3D model), tune it against a live preview and
//! send the result into the pipeline as a feature — or, when opened from an existing feature,
//! update that feature (the pipeline merges the new revision downstream).

use crate::app::EzApp;
use crate::panels::{image::ImageState, len_field};
use crate::ribbon::{big_button, group, pill, strip, tip};
use crate::tasks::TaskResult;
use crate::theme;
use egui::{Color32, RichText, Stroke, TextureHandle, Ui};
use ezyx_core::{DVec2, DVec3, Heightmap, Polyline};
use ezyx_image::{photo_vcarve::GroovePattern, trace::ThresholdMode, GrayImage};
use ezyx_project::pipeline::{CutStrategy, DesignFeature, FeatureSource, Payload};
use ezyx_project::Command;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ImageMode {
    #[default]
    Outline,
    Sketch,
    Relief,
    PhotoVCarve,
    Model3D,
}

impl ImageMode {
    pub const ALL: [ImageMode; 5] = [ImageMode::Outline, ImageMode::Sketch, ImageMode::Relief, ImageMode::PhotoVCarve, ImageMode::Model3D];
    pub fn label(self) -> &'static str {
        match self {
            ImageMode::Outline => "✂ Outline → 2D cut",
            ImageMode::Sketch => "✒ Sketch → line",
            ImageMode::Relief => "⛰ 2.5D relief",
            ImageMode::PhotoVCarve => "📷 Photo V-carve",
            ImageMode::Model3D => "🧊 3D model",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            ImageMode::Outline => "outline",
            ImageMode::Sketch => "sketch",
            ImageMode::Relief => "relief",
            ImageMode::PhotoVCarve => "photo_vcarve",
            ImageMode::Model3D => "model3d",
        }
    }
    pub fn feature_id(self) -> &'static str {
        match self {
            ImageMode::Outline => "image.outline",
            ImageMode::Sketch => "image.sketch",
            ImageMode::Relief => "image.relief",
            ImageMode::PhotoVCarve => "image.photo_vcarve",
            ImageMode::Model3D => "image.to3d",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == s)
    }
}

/// Everything needed to reproduce a conversion (stored on the pipeline feature).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Settings {
    trace: ezyx_image::trace::TraceOptions,
    relief: ezyx_image::relief::ReliefOptions,
    photo: ezyx_image::photo_vcarve::PhotoVCarveOptions,
    thickness_3d: f64,
    strategy: CutStrategy,
    depth: f64,
}

pub enum StudioResult {
    Paths { key: String, paths: Vec<Polyline> },
    Relief { key: String, hm: Heightmap },
    Grooves { key: String, grooves: Vec<Vec<DVec3>> },
}

#[derive(Default)]
struct Preview {
    key: String,
    paths: Vec<Polyline>,
    grooves: Vec<Vec<DVec3>>,
    relief: Option<TextureHandle>,
    relief_pending: Option<Heightmap>,
    info: String,
}

pub struct Studio {
    pub img: ImageState,
    pub mode: ImageMode,
    pub strategy: CutStrategy,
    pub depth: f64,
    /// Pipeline feature being edited (Send becomes Update).
    pub target: Option<u64>,
    pub name: String,
    pub show_original: bool,
    preview: Preview,
    computing: Option<String>,
}

impl Default for Studio {
    fn default() -> Self {
        Self {
            img: ImageState::default(),
            mode: ImageMode::Outline,
            strategy: CutStrategy::VCarve,
            depth: 3.0,
            target: None,
            name: String::new(),
            show_original: false,
            preview: Preview::default(),
            computing: None,
        }
    }
}

impl Studio {
    fn settings(&self) -> Settings {
        Settings {
            trace: self.img.trace.clone(),
            relief: self.img.relief.clone(),
            photo: self.img.photo.clone(),
            thickness_3d: self.img.thickness_3d,
            strategy: self.strategy,
            depth: self.depth,
        }
    }

    fn key(&self) -> String {
        format!("{}|{}|{}", self.img.path.clone().unwrap_or_default(), self.mode.key(), serde_json::to_string(&self.settings()).unwrap_or_default())
    }

    /// Prime the studio from a pipeline feature that came from it.
    pub fn edit_feature(&mut self, f: &DesignFeature, ctx: &egui::Context) {
        if let FeatureSource::ImageStudio { image, mode, settings } = &f.source {
            if self.img.path.as_deref() != Some(image.as_str()) {
                self.img.load(ctx, image);
            }
            self.mode = ImageMode::parse(mode).unwrap_or_default();
            if let Ok(s) = serde_json::from_value::<Settings>(settings.clone()) {
                self.img.trace = s.trace;
                self.img.relief = s.relief;
                self.img.photo = s.photo;
                self.img.thickness_3d = s.thickness_3d;
                self.strategy = s.strategy;
                self.depth = s.depth;
            }
            self.target = Some(f.id);
            self.name = f.name.clone();
        }
    }

    pub fn on_result(&mut self, r: StudioResult) {
        let key = match &r {
            StudioResult::Paths { key, .. } | StudioResult::Relief { key, .. } | StudioResult::Grooves { key, .. } => key.clone(),
        };
        if self.computing.as_deref() == Some(key.as_str()) {
            self.computing = None;
        }
        self.preview.key = key;
        self.preview.paths.clear();
        self.preview.grooves.clear();
        match r {
            StudioResult::Paths { paths, .. } => {
                let closed = paths.iter().filter(|p| p.closed).count();
                self.preview.info = format!("{} vectors ({closed} closed, {} open)", paths.len(), paths.len() - closed);
                self.preview.paths = paths;
            }
            StudioResult::Relief { hm, .. } => {
                let (lo, hi) = hm.min_max();
                self.preview.info = format!("Relief {} × {} cells, {:.2} mm deep", hm.width, hm.height, hi - lo);
                self.preview.relief_pending = Some(hm);
            }
            StudioResult::Grooves { grooves, .. } => {
                self.preview.info = format!("{} grooves, darkest {:.2} mm deep", grooves.len(), self.img.photo.max_depth());
                self.preview.grooves = grooves;
            }
        }
    }

    /// Recompute the preview in the background when settings change.
    fn refresh(&mut self, app_tasks: &mut crate::tasks::Tasks, ctx: &egui::Context) {
        let Some(gray) = self.img.gray.clone() else { return };
        let key = self.key();
        if key == self.preview.key || self.computing.as_deref() == Some(key.as_str()) || self.computing.is_some() {
            return;
        }
        self.computing = Some(key.clone());
        let mode = self.mode;
        let s = self.settings();
        app_tasks.spawn(ctx, move || {
            // Previews run on a reduced image so tuning stays interactive.
            let small = downscale(&gray, 700);
            TaskResult::Studio(match mode {
                ImageMode::Outline => StudioResult::Paths { key, paths: ezyx_image::trace::trace_outlines(&small, &s.trace) },
                ImageMode::Sketch | ImageMode::Model3D => {
                    let paths = if mode == ImageMode::Sketch {
                        ezyx_image::trace::trace_centerlines(&small, &s.trace)
                    } else {
                        ezyx_image::trace::trace_outlines(&small, &s.trace)
                    };
                    StudioResult::Paths { key, paths }
                }
                ImageMode::Relief => {
                    let mut o = s.relief.clone();
                    o.resolution = o.resolution.max(o.width_mm / 300.0);
                    StudioResult::Relief { key, hm: ezyx_image::relief::image_to_relief(&small, &o) }
                }
                ImageMode::PhotoVCarve => StudioResult::Grooves { key, grooves: ezyx_image::photo_vcarve::grooves(&small, &s.photo) },
            })
        });
    }

    /// Full-resolution conversion → pipeline payload.
    fn payload(&self) -> Result<Payload, String> {
        let gray = self.img.gray.as_ref().ok_or("load an image first")?;
        Ok(match self.mode {
            ImageMode::Outline => {
                Payload::Vectors { paths: ezyx_image::trace::trace_outlines(gray, &self.img.trace), strategy: Some(self.strategy), depth: Some(self.depth) }
            }
            ImageMode::Sketch => Payload::Vectors {
                paths: ezyx_image::trace::trace_centerlines(gray, &self.img.trace),
                strategy: Some(CutStrategy::Engrave),
                depth: Some(self.depth),
            },
            ImageMode::Relief => Payload::Relief { heightmap: ezyx_image::relief::image_to_relief(gray, &self.img.relief) },
            ImageMode::PhotoVCarve => {
                let o = &self.img.photo;
                let tool = if o.bit_angle <= 45.0 {
                    "generic.v30"
                } else if o.bit_angle <= 75.0 {
                    "generic.v60"
                } else {
                    "generic.v90"
                };
                Payload::Grooves { paths: ezyx_image::photo_vcarve::grooves(gray, o), tool_id: tool.into(), max_depth: o.max_depth() }
            }
            ImageMode::Model3D => {
                let path = self.img.path.clone().unwrap_or_default();
                let dynimg = image::open(&path).map_err(|e| e.to_string())?;
                let g = GrayImage::from_dynamic_limited(&dynimg, 512);
                let sil = if dynimg.color().has_alpha() {
                    let small = dynimg.resize(g.width as u32, g.height as u32, image::imageops::FilterType::Triangle);
                    GrayImage::alpha_mask(&small, 16)
                } else {
                    ezyx_image::trace::mask(&g, &self.img.trace)
                };
                let opt = ezyx_image::inflate::InflateOptions { width_mm: self.img.relief.width_mm, thickness: self.img.thickness_3d, ..Default::default() };
                Payload::Mesh { mesh: ezyx_mesh::img3d::inflated_mesh(&g, &sil, &opt) }
            }
        })
    }

    fn source(&self) -> FeatureSource {
        FeatureSource::ImageStudio {
            image: self.img.path.clone().unwrap_or_default(),
            mode: self.mode.key().into(),
            settings: serde_json::to_value(self.settings()).unwrap_or_default(),
        }
    }
}

fn downscale(g: &GrayImage, max: usize) -> GrayImage {
    if g.width.max(g.height) <= max {
        return g.clone();
    }
    let s = max as f64 / g.width.max(g.height) as f64;
    let (w, h) = (((g.width as f64 * s) as usize).max(1), ((g.height as f64 * s) as usize).max(1));
    let mut out = GrayImage::new(w, h, 0.0);
    for y in 0..h {
        for x in 0..w {
            let sx = ((x as f64 + 0.5) / s) as usize;
            let sy = ((y as f64 + 0.5) / s) as usize;
            out.data[y * w + x] = g.data[sy.min(g.height - 1) * g.width + sx.min(g.width - 1)];
        }
    }
    out
}

pub fn show(app: &mut EzApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    // Upload a freshly computed relief preview as a texture.
    if let Some(hm) = app.image.preview.relief_pending.take() {
        let (lo, hi) = hm.min_max();
        let span = (hi - lo).max(1e-6);
        let px: Vec<Color32> = (0..hm.height)
            .rev()
            .flat_map(|y| {
                let hm = &hm;
                (0..hm.width).map(move |x| {
                    let v = ((hm.get(x, y) - lo) / span * 255.0) as u8;
                    Color32::from_rgb(v, v, v)
                })
            })
            .collect();
        app.image.preview.relief = Some(ctx.load_texture("studio_relief", egui::ColorImage::new([hm.width, hm.height], px), egui::TextureOptions::LINEAR));
    }
    let mut send = false;
    egui::Panel::top("studio_ribbon").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("🖼 Image Studio").strong().size(16.0).color(theme::ACCENT));
            if let Some(t) = app.image.target.and_then(|id| app.project.features.iter().find(|f| f.id == id)) {
                ui.label(RichText::new(format!("editing feature '{}' (rev {})", t.name, t.revision)).color(theme::STEEL));
            }
        });
        strip(ui, "studio_strip", |ui| {
            group(ui, "Image", |ui| {
                if big_button(ui, "📂", "Load", "Load a photo, pen sketch or logo", false) {
                    let ctx = ctx.clone();
                    app.files.open("image", &crate::panels::image::ImageState::EXTS, move |app, p| app.image.img.load(&ctx, &p.display().to_string()));
                    app.image.target = None;
                }
                if big_button(
                    ui,
                    "◐",
                    if app.image.show_original { "Original" } else { "Processed" },
                    "Toggle original / processed view",
                    app.image.show_original,
                ) {
                    app.image.show_original = !app.image.show_original;
                }
            });
            group(ui, "Convert to", |ui| {
                for m in ImageMode::ALL {
                    let (icon, label) = m.label().split_once(' ').unwrap_or(("", m.label()));
                    if big_button(ui, icon, label.split(" →").next().unwrap_or(label), &tip(m.feature_id()), app.image.mode == m) {
                        app.image.mode = m;
                    }
                }
            });
            group(ui, "Pipeline", |ui| {
                let (icon, label) = if app.image.target.is_some() { ("⟳", "Update") } else { ("➡", "Send") };
                if big_button(ui, icon, label, &tip("image.send"), false) {
                    send = true;
                }
                if app.image.target.is_some() && big_button(ui, "➕", "As new", "Send as a new feature instead of updating", false) {
                    app.image.target = None;
                    send = true;
                }
            });
        });
    });
    egui::Panel::right("studio_settings").resizable(true).default_size(330.0).show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| settings_panel(app, ui));
    });
    egui::Panel::bottom("studio_status").show(ui, |ui| {
        ui.horizontal(|ui| {
            if app.image.computing.is_some() {
                ui.spinner();
            }
            ui.label(&app.image.preview.info);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(app.image.img.path.clone().unwrap_or_else(|| "no image".into())).small().color(theme::STEEL));
            });
        });
    });
    egui::CentralPanel::default().show(ui, |ui| viewer(app, ui));
    let s = &mut app.image;
    s.img.refresh_mask(&ctx);
    s.refresh(&mut app.tasks, &ctx);
    if send {
        send_to_pipeline(app);
    }
}

fn send_to_pipeline(app: &mut EzApp) {
    let s = &app.image;
    let payload = match s.payload() {
        Ok(p) => p,
        Err(e) => {
            app.status = format!("⚠ {e}");
            return;
        }
    };
    let source = s.source();
    let ok = match s.target.filter(|id| app.project.features.iter().any(|f| f.id == *id)) {
        Some(id) => app.exec(Command::ReimportFeature { id, payload, source: Some(source) }),
        None => {
            let name = if s.name.is_empty() {
                let stem = s
                    .img
                    .path
                    .as_deref()
                    .and_then(|p| std::path::Path::new(p).file_stem())
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Image".into());
                format!("{stem} ({})", s.mode.key().replace('_', " "))
            } else {
                s.name.clone()
            };
            let ok = app.exec(Command::ImportFeature { name, payload, source });
            if ok {
                let id = app.project.features.last().map(|f| f.id);
                app.image.target = id;
                app.image.name = app.project.features.last().map(|f| f.name.clone()).unwrap_or_default();
                app.selected_feature = id;
            }
            ok
        }
    };
    if ok {
        let ctx = egui::Context::default();
        let _ = ctx;
        app.results_rev = 0; // force a toolpath refresh in the pipeline
        if app.step == crate::ribbon::Step::Stock {
            app.step = crate::ribbon::Step::Design;
        }
    }
}

fn viewer(app: &mut EzApp, ui: &mut Ui) {
    let s = &app.image;
    let rect = ui.available_rect_before_wrap();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::viewport_bg(app.prefs.dark));
    let Some(tex) = (match (s.mode, s.show_original) {
        (_, true) => s.img.tex.as_ref(),
        (ImageMode::Relief, false) => s.preview.relief.as_ref().or(s.img.tex.as_ref()),
        (ImageMode::Outline | ImageMode::Sketch | ImageMode::Model3D, false) => s.img.mask_tex.as_ref().or(s.img.tex.as_ref()),
        _ => s.img.tex.as_ref(),
    }) else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "📂 Load an image to start (photo, pen sketch or logo)",
            egui::FontId::proportional(16.0),
            theme::STEEL,
        );
        return;
    };
    let size = tex.size_vec2();
    let scale = ((rect.width() - 40.0) / size.x).min((rect.height() - 40.0) / size.y).max(0.01);
    let img_rect = egui::Rect::from_center_size(rect.center(), size * scale);
    let alpha = if s.mode == ImageMode::PhotoVCarve && !s.show_original { 90 } else { 255 };
    painter.image(tex.id(), img_rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::from_white_alpha(alpha));
    painter.rect_stroke(img_rect, 0.0, Stroke::new(1.0, theme::STEEL), egui::StrokeKind::Outside);
    // Overlay results (mm → image rect; world Y is up, image rows go down).
    let (wmm, aspect) = match s.mode {
        ImageMode::PhotoVCarve => (s.img.photo.width_mm, s.img.aspect()),
        _ => (s.img.trace.width_mm, s.img.aspect()),
    };
    let hmm = wmm * aspect;
    let to_screen = |p: DVec2| egui::pos2(img_rect.left() + (p.x / wmm) as f32 * img_rect.width(), img_rect.bottom() - (p.y / hmm) as f32 * img_rect.height());
    if !s.show_original {
        // GPU vertex budget for the overlay: thin out huge traces / groove sets evenly.
        let want = s.preview.paths.iter().map(|p| p.pts.len()).sum::<usize>() + s.preview.grooves.iter().map(|g| g.len()).sum::<usize>();
        let granted = crate::gpu_guard::take_segments(ui.ctx(), want);
        let keep = want.div_ceil(granted.max(1)).max(1);
        for pl in &s.preview.paths {
            let mut pts: Vec<egui::Pos2> = pl.pts.iter().step_by(keep).map(|p| to_screen(*p)).collect();
            if pl.closed && !pts.is_empty() {
                pts.push(pts[0]);
            }
            painter.add(egui::Shape::line(pts, Stroke::new(1.4, theme::ACCENT)));
        }
        let maxd = s.img.photo.max_depth().max(1e-6);
        for g in s.preview.grooves.iter().step_by(keep.min(4)) {
            let g: Vec<DVec3> = g.iter().step_by((keep / 4).max(1)).copied().collect();
            for w in g.windows(2) {
                let t = (-(w[1].z) / maxd) as f32;
                painter.line_segment(
                    [to_screen(DVec2::new(w[0].x, w[0].y)), to_screen(DVec2::new(w[1].x, w[1].y))],
                    Stroke::new(0.4 + 2.0 * t * scale.min(1.0), theme::WALNUT),
                );
            }
        }
    }
    let u = app.project.job.display_units;
    painter.text(
        img_rect.left_bottom() + egui::vec2(0.0, 4.0),
        egui::Align2::LEFT_TOP,
        format!("{} × {}", u.fmt(wmm), u.fmt(hmm)),
        egui::FontId::monospace(12.0),
        theme::STEEL,
    );
}

fn settings_panel(app: &mut EzApp, ui: &mut Ui) {
    let units = app.project.job.display_units;
    let mode = app.image.mode;
    ui.horizontal_wrapped(|ui| {
        for m in ImageMode::ALL {
            if pill(ui, m.label(), mode == m, &tip(m.feature_id())) {
                app.image.mode = m;
            }
        }
    });
    ui.label(RichText::new(tip(mode.feature_id())).small().color(theme::STEEL));
    ui.separator();
    let s = &mut app.image;
    ui.horizontal(|ui| {
        ui.label("Feature name");
        ui.add(egui::TextEdit::singleline(&mut s.name).hint_text("from the file name").desired_width(170.0));
    });
    match mode {
        ImageMode::Outline | ImageMode::Sketch | ImageMode::Model3D => {
            ui.label(RichText::new("Clean-up").strong());
            let t = &mut s.img.trace;
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
                len_field(ui, &mut t.width_mm, units, 5.0..=3000.0);
                s.img.relief.width_mm = t.width_mm;
            });
            if mode == ImageMode::Outline {
                ui.separator();
                ui.label(RichText::new("Carve as").strong());
                egui::ComboBox::from_id_salt("studio_strategy").selected_text(s.strategy.label()).show_ui(ui, |ui| {
                    for st in [CutStrategy::VCarve, CutStrategy::Pocket, CutStrategy::CutOut, CutStrategy::InsideCut, CutStrategy::Engrave] {
                        if ui.selectable_label(s.strategy == st, st.label()).clicked() {
                            s.strategy = st;
                            s.depth = st.default_depth(app.project.job.thickness);
                        }
                    }
                });
            }
            if mode != ImageMode::Model3D {
                ui.horizontal(|ui| {
                    ui.label("Depth");
                    len_field(ui, &mut s.depth, units, 0.05..=500.0);
                });
            } else {
                ui.horizontal(|ui| {
                    ui.label("3D thickness");
                    len_field(ui, &mut s.img.thickness_3d, units, 1.0..=200.0);
                });
                ui.label(
                    RichText::new("The silhouette (preview) is inflated into a rounded 3D model. Transparent PNGs work best.").small().color(theme::STEEL),
                );
            }
        }
        ImageMode::Relief => {
            let r = &mut s.img.relief;
            ui.horizontal(|ui| {
                ui.label("Width");
                len_field(ui, &mut r.width_mm, units, 5.0..=3000.0);
            });
            ui.horizontal(|ui| {
                ui.label("Depth");
                len_field(ui, &mut r.depth, units, 0.1..=100.0);
            });
            ui.checkbox(&mut r.bright_is_high, "Bright = high");
            ui.add(egui::Slider::new(&mut r.gamma, 0.3..=3.0).text("gamma"));
            ui.add(egui::Slider::new(&mut r.blur, 0.0..=6.0).text("smooth"));
            ui.add(egui::Slider::new(&mut r.background_cutoff, 0.0..=0.5).text("flatten background"));
            ui.horizontal(|ui| {
                ui.label("Resolution");
                len_field(ui, &mut r.resolution, units, 0.05..=2.0);
            });
            s.img.trace.width_mm = r.width_mm;
            if r.depth > app.project.job.thickness {
                ui.colored_label(theme::WARN, "Deeper than the stock — the pipeline will flag this.");
            }
        }
        ImageMode::PhotoVCarve => {
            let p = &mut s.img.photo;
            ui.horizontal(|ui| {
                ui.label("Width");
                len_field(ui, &mut p.width_mm, units, 10.0..=3000.0);
            });
            ui.horizontal(|ui| {
                ui.label("Line spacing");
                len_field(ui, &mut p.spacing, units, 0.2..=10.0);
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
            s.img.trace.width_mm = p.width_mm;
        }
    }
    ui.separator();
    let label = if app.image.target.is_some() { "⟳ Update feature in the pipeline" } else { "➡ Send to pipeline" };
    ui.label(RichText::new(tip("image.send")).small().color(theme::STEEL));
    if ui.add(egui::Button::new(RichText::new(label).strong()).fill(theme::ACCENT_SOFT)).clicked() {
        send_to_pipeline(app);
    }
}
