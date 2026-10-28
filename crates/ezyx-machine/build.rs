//! `cfg(serial)`: USB serial support — the `serial` feature, except on Android/iOS, where apps
//! can't open serial ports (the machine is reached over Wi-Fi there).
fn main() {
    println!("cargo::rustc-check-cfg=cfg(serial)");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if std::env::var_os("CARGO_FEATURE_SERIAL").is_some() && os != "android" && os != "ios" {
        println!("cargo::rustc-cfg=serial");
    }
}
