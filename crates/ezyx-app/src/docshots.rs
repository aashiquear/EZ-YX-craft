//! Documentation screenshot mode. With `EZYX_DOC_SHOTS=<registry.json>` and
//! `EZYX_DOC_SHOTS_OUT=<dir>` the app walks through every entry of the registry's `shots` list
//! (workspace, step, project, scripted setup), waits until background work has settled, saves
//! `<id>.png` and quits — non-zero exit if any shot failed. Used by `tools/docs/render_shots.sh`
//! and the docs workflow so the user manual's images always match the current UI.

use crate::app::EzApp;
use crate::ribbon::{Module, PassFilter, Step};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Deserialize)]
pub struct ShotSpec {
    pub id: String,
    pub workspace: String,
    #[serde(default)]
    pub step: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub setup: Vec<String>,
    #[serde(default)]
    pub size: Option<[u32; 2]>,
}

#[derive(Deserialize)]
struct RegistryShots {
    shots: Vec<ShotSpec>,
}

enum Phase {
    Setup,
    Settle { frames: u32, waited: u32 },
    Capture { waited: u32 },
}

pub struct DocShots {
    shots: Vec<ShotSpec>,
    out: PathBuf,
    i: usize,
    phase: Phase,
    image: Arc<Mutex<Option<Arc<egui::ColorImage>>>>,
    failures: Vec<String>,
}

impl DocShots {
    pub fn from_env() -> Option<Self> {
        let reg = std::env::var("EZYX_DOC_SHOTS").ok()?;
        let out = PathBuf::from(std::env::var("EZYX_DOC_SHOTS_OUT").unwrap_or_else(|_| "docs/manual/src/img".into()));
        let txt = std::fs::read_to_string(&reg).map_err(|e| eprintln!("docs: cannot read {reg}: {e}")).ok()?;
        let r: RegistryShots = serde_json::from_str(&txt).map_err(|e| eprintln!("docs: bad registry {reg}: {e}")).ok()?;
        let _ = std::fs::create_dir_all(&out);
        // Only some shots? EZYX_DOC_SHOTS_ONLY=id1,id2
        let only: Vec<String> = std::env::var("EZYX_DOC_SHOTS_ONLY").map(|s| s.split(',').map(|x| x.trim().to_string()).collect()).unwrap_or_default();
        let shots = r.shots.into_iter().filter(|s| only.is_empty() || only.contains(&s.id)).collect();
        Some(Self { shots, out, i: 0, phase: Phase::Setup, image: Arc::new(Mutex::new(None)), failures: vec![] })
    }

    /// Advance the script by one frame. Returns false when finished.
    pub fn tick(&mut self, app: &mut EzApp, ctx: &egui::Context) -> bool {
        let Some(shot) = self.shots.get(self.i).cloned() else {
            eprintln!("docs: {} shot(s) saved to {}, {} failed", self.shots.len() - self.failures.len(), self.out.display(), self.failures.len());
            for f in &self.failures {
                eprintln!("docs: ✖ {f}");
            }
            std::process::exit(if self.failures.is_empty() { 0 } else { 1 });
        };
        ctx.request_repaint();
        match &mut self.phase {
            Phase::Setup => {
                if let Some([w, h]) = shot.size {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w as f32, h as f32)));
                }
                if let Err(e) = setup(app, ctx, &shot) {
                    self.failures.push(format!("{}: {e}", shot.id));
                    self.i += 1;
                    return true;
                }
                self.phase = Phase::Settle { frames: 0, waited: 0 };
            }
            Phase::Settle { frames, waited } => {
                *waited += 1;
                let busy = app.tasks.running > 0 || (shot.setup.iter().any(|s| s == "simulate") && app.preview.is_none());
                if busy {
                    *frames = 0;
                } else {
                    *frames += 1;
                }
                // A few quiet frames so layout and textures settle; give up after ~2 minutes.
                if *frames > 12 || *waited > 1200 {
                    if *waited > 1200 {
                        eprintln!("docs: {} timed out waiting for background work; capturing anyway", shot.id);
                    }
                    let slot = self.image.clone();
                    ctx.request_screenshot(move |img| {
                        if let Ok(mut s) = slot.lock() {
                            *s = Some(img);
                        }
                    });
                    self.phase = Phase::Capture { waited: 0 };
                }
            }
            Phase::Capture { waited } => {
                *waited += 1;
                let img = self.image.lock().ok().and_then(|mut s| s.take());
                if let Some(img) = img {
                    if let Err(e) = save_png(&img, &self.out.join(format!("{}.png", shot.id))) {
                        self.failures.push(format!("{}: {e}", shot.id));
                    }
                    self.i += 1;
                    self.phase = Phase::Setup;
                } else if *waited > 300 {
                    self.failures.push(format!("{}: no screenshot received", shot.id));
                    self.i += 1;
                    self.phase = Phase::Setup;
                }
            }
        }
        true
    }
}

fn setup(app: &mut EzApp, ctx: &egui::Context, s: &ShotSpec) -> Result<(), String> {
    // Fresh state for every shot.
    app.preview = None;
    app.selection.clear();
    app.selected_feature = None;
    app.selected_op = None;
    app.passes = PassFilter::Both;
    app.show_ai = false;
    app.dialogs.merge_report = None;
    if s.project.is_empty() {
        app.project = ezyx_project::demo::sign("OAK")?;
        app.history = Default::default();
        app.path = None;
        app.touch();
    } else {
        app.load_project(std::path::Path::new(&s.project));
    }
    app.set_mode2d(true);
    app.calculate(None);
    app.focus = match s.workspace.as_str() {
        "image" => Some(Module::Image),
        "solid" => Some(Module::Solid),
        "machine" => Some(Module::Machine),
        _ => None,
    };
    if let Some(step) = Step::parse(&s.step) {
        app.step = step;
    }
    for cmd in &s.setup {
        let (k, v) = cmd.split_once(':').unwrap_or((cmd.as_str(), ""));
        match k {
            "simulate" => app.simulate(ctx, true),
            "select_feature" => {
                let i: usize = v.parse().unwrap_or(0);
                app.selected_feature = app.project.features.get(i).map(|f| f.id);
            }
            "select_all" => app.selection = app.project.vectors.iter().map(|v| v.id).collect(),
            "image" => {
                app.image.img.load(ctx, v);
                if app.image.img.path.is_none() {
                    return Err(format!("could not load image {v}"));
                }
                app.image.mode = crate::image_studio::ImageMode::parse(&s.step).unwrap_or_default();
            }
            "template" => {
                let i: usize = v.parse().unwrap_or(0);
                let (_, p) = ezyx_cad::templates().into_iter().nth(i).ok_or("no such template")?;
                crate::solid::new_part(app, p);
                app.solid.tab = crate::solid::SolidTab::Features;
                app.solid.sel = Some(0);
            }
            "sketch_demo" => {
                let (_, p) = ezyx_cad::templates().into_iter().last().ok_or("no templates")?;
                let sk = p.features.iter().find_map(|f| match &f.shape {
                    ezyx_cad::Shape::SketchExtrude { sketch, .. } => Some(sketch.clone()),
                    _ => None,
                });
                crate::solid::new_part(app, p);
                app.solid.tab = crate::solid::SolidTab::Sketch;
                app.solid.sketch = Some(crate::solid::SketchEdit::new(Some(0), sk.unwrap_or_else(|| ezyx_cad::Sketch::rectangle(60.0, 40.0))));
            }
            "connect_sim" => {
                crate::machine_control::connect_sim(app);
                app.send_to_machine();
                app.focus = Some(Module::Machine);
            }
            other => return Err(format!("unknown setup step '{other}'")),
        }
    }
    Ok(())
}

fn save_png(img: &egui::ColorImage, path: &std::path::Path) -> Result<(), String> {
    let [w, h] = img.size;
    let mut buf = image::RgbaImage::new(w as u32, h as u32);
    for (i, px) in img.pixels.iter().enumerate() {
        buf.put_pixel((i % w) as u32, (i / w) as u32, image::Rgba(px.to_array()));
    }
    buf.save(path).map_err(|e| e.to_string())
}
