//! EZ-YX-craft application (egui + wgpu). The desktop binary (`main.rs`) and the iPad/Android
//! shells (`crates/ezyx-mobile`) all start the same app through [`run`].

mod app;
mod docshots;
mod files;
mod gpu_guard;
mod image_studio;
mod machine_control;
mod panels;
mod pipeline;
pub mod platform;
mod ribbon;
mod solid;
mod tasks;
mod theme;
mod viewport;

pub use eframe;

/// Rendering mode for this run: `EZYX_RENDER` overrides Settings › Graphics.
fn render_mode(prefs: &app::Prefs) -> ezyx_gpu::RenderMode {
    ezyx_gpu::RenderMode::from_env().unwrap_or(prefs.render_mode)
}

/// Window + graphics options shared by every platform.
fn options(mode: ezyx_gpu::RenderMode, prefs: &app::Prefs) -> eframe::NativeOptions {
    let mut viewport = egui::ViewportBuilder::default().with_title("EZ-YX-craft").with_app_id("org.ezyxcraft.app");
    if !platform::MOBILE {
        viewport = viewport.with_inner_size([1480.0, 920.0]).with_min_inner_size([960.0, 600.0]);
        if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/icon-128.png")) {
            viewport = viewport.with_icon(icon);
        }
    }
    eframe::NativeOptions { viewport, wgpu_options: gpu_guard::wgpu_options(mode, prefs.gpu_backend), ..Default::default() }
}

/// Run the app until its window closes (never returns on iOS).
pub fn run(options: eframe::NativeOptions) -> eframe::Result {
    eframe::run_native("EZ-YX-craft", options, Box::new(|cc| Ok(Box::new(app::EzApp::new(cc)))))
}

/// Desktop entry point (Linux, macOS, Windows).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn run_desktop() -> eframe::Result {
    env_logger::init();
    if let Some(b) = ezyx_gpu::apply_backend_override() {
        log::info!("GPU backend forced to {b}");
    }
    let prefs = app::Prefs::load();
    let mode = render_mode(&prefs);
    gpu_guard::install_panic_fallback(mode);
    let result = run(options(mode, &prefs));
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

/// Settings for a tablet start. After a graphics crash on the last run (see
/// `gpu_guard::take_crash_marker`) Android tries OpenGL ES this time.
#[cfg(any(target_os = "android", target_os = "ios"))]
fn mobile_prefs() -> app::Prefs {
    let mut prefs = app::Prefs::load();
    if gpu_guard::take_crash_marker().is_some() && cfg!(target_os = "android") {
        prefs.gpu_backend = ezyx_gpu::GpuBackend::Gl;
    }
    prefs
}

/// Android entry point, called from `android_main` in `ezyx-mobile`.
#[cfg(target_os = "android")]
pub fn run_android(android_app: platform::AndroidApp) -> eframe::Result {
    platform::init_android(&android_app);
    let prefs = mobile_prefs();
    let mode = render_mode(&prefs);
    gpu_guard::install_panic_fallback(mode);
    let mut options = options(mode, &prefs);
    options.android_app = Some(android_app);
    run(options)
}

/// iOS / iPadOS entry point, called from `main()` in the Xcode shell (never returns).
#[cfg(target_os = "ios")]
pub fn run_ios() -> eframe::Result {
    let prefs = mobile_prefs();
    let mode = render_mode(&prefs);
    gpu_guard::install_panic_fallback(mode);
    run(options(mode, &prefs))
}
