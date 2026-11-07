//! What differs between the desktop and the iPad / Android builds: where files live and what the
//! OS lets an app do. Everything else (UI, CAM, simulation, machine control over Wi-Fi) is shared.

use std::path::PathBuf;

/// iPad / Android build: one window (modules as tabs), touch input, in-app file browser.
pub const MOBILE: bool = cfg!(any(target_os = "android", target_os = "ios"));
/// Native open/save dialogs (desktop); mobile uses the in-app browser in [`crate::files`].
pub const NATIVE_FILE_DIALOGS: bool = !MOBILE;
/// Starting other programs (restart in another render mode, open SolidCraft). Mobile OSes don't allow it.
pub const CAN_SPAWN: bool = !MOBILE;
/// Wired USB serial to the machine. Mobile apps can't open serial ports: Wi-Fi only.
pub const USB_SERIAL: bool = !MOBILE;

pub fn name() -> &'static str {
    if cfg!(target_os = "android") {
        "Android"
    } else if cfg!(target_os = "ios") {
        "iPadOS"
    } else {
        "desktop"
    }
}

#[cfg(target_os = "android")]
pub use winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "android")]
static ANDROID_DIRS: std::sync::OnceLock<(Option<PathBuf>, Option<PathBuf>)> = std::sync::OnceLock::new();

/// Remember the app's private and shared storage folders (Android has no `$HOME` conventions).
#[cfg(target_os = "android")]
pub fn init_android(app: &AndroidApp) {
    let internal = app.internal_data_path();
    let external = app.external_data_path().or_else(|| internal.clone());
    let _ = ANDROID_DIRS.set((internal, external));
}

/// Base folder for settings, the assistant config and the recovery copy (like `dirs::config_dir`).
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        ANDROID_DIRS.get().and_then(|d| d.0.clone())
    }
    #[cfg(target_os = "ios")]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library").join("Application Support"))
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        dirs::config_dir()
    }
}

/// The user-visible folder the mobile file browser opens: the app's folder in the Files app on
/// iPad ("On My iPad › EZ-YX-craft"), `Android/data/org.ezyxcraft.app/files` on Android.
pub fn documents_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        ANDROID_DIRS.get().and_then(|d| d.1.clone())
    }
    #[cfg(target_os = "ios")]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Documents"))
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        dirs::document_dir().or_else(dirs::home_dir)
    }
}

/// Where users put files for the app, in words (shown in the file browser).
pub fn documents_hint() -> &'static str {
    if cfg!(target_os = "ios") {
        "Put files here with the Files app: On My iPad › EZ-YX-craft (or AirDrop / Share › Save to Files)."
    } else if cfg!(target_os = "android") {
        "Put files here over USB or with a file manager: Android/data/org.ezyxcraft.app/files."
    } else {
        ""
    }
}
