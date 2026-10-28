//! EZ-YX-craft desktop app.
#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn main() -> ezyx_app::eframe::Result {
    ezyx_app::run_desktop()
}

/// On iPad/Android the app is started by the platform shell (`crates/ezyx-mobile`).
#[cfg(any(target_os = "android", target_os = "ios"))]
fn main() {
    eprintln!("EZ-YX-craft for iPad/Android is built from crates/ezyx-mobile (see docs/mobile.md).");
}
