//! Open / save requests.
//!
//! Code asks with [`Files::open`] / [`Files::save`] and gets the chosen path in a callback. On the
//! desktop the request is answered by the OS file dialog (rfd) right after the frame; on iPad and
//! Android — which have no blocking file dialogs — an in-app browser over the app's documents
//! folder (see [`crate::platform::documents_dir`]) answers it.

use crate::app::EzApp;
use egui::RichText;
use std::path::{Path, PathBuf};

type OnPick = Box<dyn FnOnce(&mut EzApp, PathBuf)>;

struct Request {
    /// What is asked for ("project", "G-code"): filter label and dialog title.
    kind: String,
    exts: Vec<String>,
    /// `Some(default file name)` for a save request.
    save_name: Option<String>,
    on_pick: OnPick,
}

#[derive(Default)]
pub struct Files {
    queue: Vec<Request>,
    browser: Option<Browser>,
    /// Use the in-app browser on the desktop too (`EZYX_INAPP_FILES=1`, docs screenshots).
    pub force_in_app: bool,
    /// Folder the in-app browser opens instead of `platform::documents_dir`.
    pub root: Option<PathBuf>,
}

struct Browser {
    dir: PathBuf,
    root: PathBuf,
    name: String,
    confirm_overwrite: bool,
}

impl Files {
    /// Ask for an existing `kind` file with one of `exts` (lower-case, no dot).
    pub fn open(&mut self, kind: &str, exts: &[&str], on_pick: impl FnOnce(&mut EzApp, PathBuf) + 'static) {
        self.queue.push(Request { kind: kind.into(), exts: exts.iter().map(|e| e.to_string()).collect(), save_name: None, on_pick: Box::new(on_pick) });
    }

    /// Ask where to save `name` (the first of `exts` is the default extension).
    pub fn save(&mut self, kind: &str, exts: &[&str], name: String, on_pick: impl FnOnce(&mut EzApp, PathBuf) + 'static) {
        self.queue.push(Request { kind: kind.into(), exts: exts.iter().map(|e| e.to_string()).collect(), save_name: Some(name), on_pick: Box::new(on_pick) });
    }
}

/// Answer pending requests (call once per frame, after the UI).
pub fn process(app: &mut EzApp, ctx: &egui::Context) {
    if crate::platform::NATIVE_FILE_DIALOGS && !app.files.force_in_app {
        for r in std::mem::take(&mut app.files.queue) {
            if let Some(p) = native_dialog(&r) {
                (r.on_pick)(app, p);
            }
        }
    } else {
        browser(app, ctx);
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn native_dialog(r: &Request) -> Option<PathBuf> {
    let exts: Vec<&str> = r.exts.iter().map(|s| s.as_str()).collect();
    let d = rfd::FileDialog::new().add_filter(&r.kind, &exts);
    match &r.save_name {
        Some(name) => d.set_file_name(name).save_file(),
        None => d.pick_file(),
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn native_dialog(_: &Request) -> Option<PathBuf> {
    None
}

/// Folders and matching files in `dir`, folders first, both sorted by name.
fn list(dir: &Path, exts: &[String]) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let (mut dirs, mut files) = (Vec::new(), Vec::new());
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
            continue;
        }
        if p.is_dir() {
            dirs.push(p);
        } else if matches_ext(&p, exts) {
            files.push(p);
        }
    }
    dirs.sort();
    files.sort();
    (dirs, files)
}

fn matches_ext(p: &Path, exts: &[String]) -> bool {
    exts.is_empty() || p.extension().is_some_and(|e| exts.iter().any(|x| x.eq_ignore_ascii_case(&e.to_string_lossy())))
}

/// `name` with the default extension added when it has none of `exts`.
fn with_ext(name: &str, exts: &[String]) -> String {
    let name = name.trim();
    match exts.first() {
        Some(e) if !matches_ext(Path::new(name), exts) => format!("{name}.{e}"),
        _ => name.to_string(),
    }
}

fn browser(app: &mut EzApp, ctx: &egui::Context) {
    let Some(req) = app.files.queue.first() else {
        app.files.browser = None;
        return;
    };
    let root = app.files.root.clone().or_else(crate::platform::documents_dir).unwrap_or_else(|| PathBuf::from("."));
    let b = app.files.browser.get_or_insert_with(|| {
        let _ = std::fs::create_dir_all(&root);
        Browser { dir: root.clone(), root, name: req.save_name.clone().unwrap_or_default(), confirm_overwrite: false }
    });
    let saving = req.save_name.is_some();
    let (folders, files) = list(&b.dir, &req.exts);
    let mut chosen: Option<PathBuf> = None;
    let mut cancel = false;
    let title = if saving { format!("💾 Save {}", req.kind) } else { format!("📂 Open {}", req.kind) };
    egui::Window::new(title).collapsible(false).resizable(true).default_width(520.0).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        ui.horizontal(|ui| {
            let at_root = b.dir == b.root;
            if ui.add_enabled(!at_root, egui::Button::new("⬆ Up")).clicked() {
                if let Some(p) = b.dir.parent() {
                    b.dir = p.to_path_buf();
                }
            }
            let rel = b.dir.strip_prefix(&b.root).map(|r| r.display().to_string()).unwrap_or_default();
            ui.label(RichText::new(if rel.is_empty() { "EZ-YX-craft".to_string() } else { format!("EZ-YX-craft / {rel}") }).strong());
        });
        ui.separator();
        egui::ScrollArea::vertical().max_height(ui.available_height().min(420.0)).auto_shrink([false, true]).show(ui, |ui| {
            // Finger-sized rows.
            let w = ui.available_width();
            let big = |t: String| egui::Button::new(RichText::new(t).size(16.0)).frame(false).min_size(egui::vec2(w, 36.0));
            for d in &folders {
                if ui.add(big(format!("📁 {}", d.file_name().unwrap_or_default().to_string_lossy()))).clicked() {
                    b.dir = d.clone();
                }
            }
            for f in &files {
                let n = f.file_name().unwrap_or_default().to_string_lossy().into_owned();
                if ui.add(big(format!("📄 {n}"))).clicked() {
                    if saving {
                        b.name = n;
                    } else {
                        chosen = Some(f.clone());
                    }
                }
            }
            if folders.is_empty() && files.is_empty() {
                ui.label(RichText::new(format!("No .{} files here.", req.exts.join(" / ."))).weak());
            }
        });
        ui.separator();
        let hint = crate::platform::documents_hint();
        if !hint.is_empty() {
            ui.label(RichText::new(hint).small().weak());
        }
        if saving {
            ui.horizontal(|ui| {
                ui.label("Name:");
                if ui.add(egui::TextEdit::singleline(&mut b.name).desired_width(320.0)).changed() {
                    b.confirm_overwrite = false;
                }
            });
        }
        ui.horizontal(|ui| {
            if saving {
                let name = with_ext(&b.name, &req.exts);
                let target = b.dir.join(&name);
                let label = if b.confirm_overwrite { "Replace" } else { "Save" };
                if ui.add_enabled(!b.name.trim().is_empty(), egui::Button::new(RichText::new(label).strong()).min_size(egui::vec2(96.0, 36.0))).clicked() {
                    if target.exists() && !b.confirm_overwrite {
                        b.confirm_overwrite = true;
                    } else {
                        chosen = Some(target);
                    }
                }
                if b.confirm_overwrite {
                    ui.label(RichText::new(format!("{name} exists — tap Replace to overwrite it.")).color(crate::theme::WARN));
                }
            }
            if ui.add(egui::Button::new("Cancel").min_size(egui::vec2(96.0, 36.0))).clicked() {
                cancel = true;
            }
        });
    });
    if chosen.is_some() || cancel {
        let req = app.files.queue.remove(0);
        app.files.browser = None;
        if let Some(p) = chosen {
            (req.on_pick)(app, p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_names_get_the_default_extension() {
        let exts = vec!["ezyx".to_string()];
        assert_eq!(with_ext("sign", &exts), "sign.ezyx");
        assert_eq!(with_ext("sign.EZYX ", &exts), "sign.EZYX");
        assert_eq!(with_ext("a.b", &exts), "a.b.ezyx");
    }

    #[test]
    fn browser_lists_matching_files_folders_first() {
        let dir = std::env::temp_dir().join(format!("ezyx_files_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for n in ["b.svg", "a.SVG", "c.png", ".hidden.svg"] {
            std::fs::write(dir.join(n), "").unwrap();
        }
        let (d, f) = list(&dir, &["svg".into(), "dxf".into()]);
        assert_eq!(d.len(), 1);
        assert_eq!(f.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect::<Vec<_>>(), vec!["a.SVG", "b.svg"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
