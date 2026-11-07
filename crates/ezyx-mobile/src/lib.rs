//! Mobile shells for EZ-YX-craft. The app itself is `ezyx-app`; this crate only provides the
//! entry points the platforms look for:
//!
//! * **Android** — `android_main`, called by NativeActivity in `libezyx_mobile.so`
//!   (`mobile/android`, built with `cargo ndk`).
//! * **iPad** — `ezyx_ios_main`, called from `main.m` in the Xcode project (`mobile/ios`), which
//!   links `libezyx_mobile.a`.

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: ezyx_app::platform::AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("ezyx"));
    if let Err(e) = ezyx_app::run_android(app) {
        log::error!("EZ-YX-craft stopped: {e}");
    }
}

/// Runs the app on the main thread; returns only if the app could not start.
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn ezyx_ios_main() -> i32 {
    match ezyx_app::run_ios() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("EZ-YX-craft could not start: {e}");
            1
        }
    }
}
