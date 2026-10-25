//! EZ-YX-craft desktop app.
#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod app;
mod docshots;
mod gpu_guard;
mod image_studio;
mod machine_control;
mod panels;
mod pipeline;
mod ribbon;
mod solid;
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
    // Rendering mode: EZYX_RENDER overrides Settings › Graphics.
    let mode = ezyx_gpu::RenderMode::from_env().unwrap_or(app::Prefs::load().render_mode);
    gpu_guard::install_panic_fallback(mode);
    let options = eframe::NativeOptions { viewport, wgpu_options: gpu_guard::wgpu_options(mode), ..Default::default() };
    let result = eframe::run_native("EZ-YX-craft", options, Box::new(|cc| Ok(Box::new(app::EzApp::new(cc)))));
    // The GPU could not be initialised at all (no compatible adapter, device creation failed):
    // start again once with CPU (software) rendering and tell the user why.
    if let Err(e) = &result {
        if mode != ezyx_gpu::RenderMode::Cpu && std::env::var_os("EZYX_GPU_FAILED").is_none() {
            log::error!("graphics initialisation failed ({e}); retrying with CPU rendering");
            if let Ok(exe) = std::env::current_exe() {
                let status =
                    std::process::Command::new(exe).args(std::env::args_os().skip(1)).env("EZYX_RENDER", "cpu").env("EZYX_GPU_FAILED", e.to_string()).status();
                if let Ok(s) = status {
                    std::process::exit(s.code().unwrap_or(0));
                }
            }
        }
    }
    result
}
