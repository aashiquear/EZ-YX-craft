//! EZ-YX-craft desktop app.
#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod app;
mod panels;
mod ribbon;
mod tasks;
mod theme;
mod viewport;

fn main() -> eframe::Result {
    env_logger::init();
    if let Some(b) = ezyx_gpu::apply_backend_override() {
        log::info!("GPU backend forced to {b}");
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("EZ-YX-craft")
        .with_inner_size([1480.0, 920.0])
        .with_min_inner_size([960.0, 600.0])
        .with_app_id("org.ezyxcraft.app");
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/icon-128.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("EZ-YX-craft", options, Box::new(|cc| Ok(Box::new(app::EzApp::new(cc)))))
}
