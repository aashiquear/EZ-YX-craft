//! Image Studio state: the loaded picture, its textures and the conversion options.

use crate::theme;
use egui::TextureHandle;
use ezyx_image::{photo_vcarve::PhotoVCarveOptions, relief::ReliefOptions, trace::TraceOptions, GrayImage};

pub struct ImageState {
    pub path: Option<String>,
    pub gray: Option<GrayImage>,
    pub tex: Option<TextureHandle>,
    pub mask_tex: Option<TextureHandle>,
    pub trace: TraceOptions,
    pub relief: ReliefOptions,
    pub photo: PhotoVCarveOptions,
    pub thickness_3d: f64,
    pub mask_key: String,
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

    pub fn refresh_mask(&mut self, ctx: &egui::Context) {
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
