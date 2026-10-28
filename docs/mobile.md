# iPad and Android builds

EZ-YX-craft is the same Rust app on every platform. On tablets it is compiled for the device and
wrapped in a thin platform shell:

| | Android | iPad |
|---|---|---|
| Rust crate | `crates/ezyx-mobile` → `libezyx_mobile.so` (cdylib) | `crates/ezyx-mobile` → `libezyx_mobile.a` (staticlib) |
| Entry point | `android_main` (NativeActivity, no Java code) | `ezyx_ios_main` called from `mobile/ios/main.m` |
| Shell project | `mobile/android` (Gradle, AGP 8.11) | `mobile/ios/project.yml` (XcodeGen → Xcode project) |
| Graphics | wgpu → Vulkan, OpenGL ES 3 fallback | wgpu → Metal |
| Output | `.apk` (sideload/testing), `.aab` (Google Play) | `.ipa` (TestFlight/App Store/ad hoc), unsigned `.ipa`, simulator `.app` |
| Minimum OS | Android 8.0 (API 26) | iPadOS 16 |

What differs from the desktop lives in `crates/ezyx-app/src/platform.rs` (paths, what the OS
allows) and `files.rs` (the in-app file browser replacing native dialogs). USB serial is compiled
out on mobile (`crates/ezyx-machine/build.rs`); tablets talk to the machine over Wi-Fi.

## Builds on GitHub Actions

`.github/workflows/mobile.yml` builds both apps on GitHub's runners (Ubuntu for Android, macOS 15
with Xcode 26 for iPad).

* **Test build from any branch:** *Actions › Mobile › Run workflow*, pick the branch (for example
  `claude/nifty-rubin-i1grwr`) and *android*, *ios* or *both*. When it finishes, download
  **android** / **ipad** from the run's *Artifacts* section.
* **Release:** push a tag `vX.Y.Z` on `main`. `release.yml` builds the desktop installers and
  calls `mobile.yml`; the APK/AAB and IPAs are attached to the same GitHub release.
* **Pull requests** that touch `mobile/` or `crates/ezyx-mobile` build both apps (unsigned).
* `ci.yml` also runs `cargo check` for `aarch64-linux-android` and `aarch64-apple-ios` on every
  push, so code that breaks the mobile builds is caught early.

Version numbers: the tag (`v0.2.0` → `0.2.0`), or the workspace version for test builds. The
build number (Android `versionCode`, iOS `CFBundleVersion`) is the number of minutes since
2024-01-01, so it always grows.

Without any secrets everything still builds:

| File | Signed with | Use |
|---|---|---|
| `EZ-YX-craft-X.Y.Z-android-test.apk` | Debug key | Install by hand on any Android device |
| `EZ-YX-craft-X.Y.Z-ipad-unsigned.ipa` | Nothing | Sideload with AltStore / Sideloadly (they sign it with your Apple ID) |
| `EZ-YX-craft-X.Y.Z-ipad-simulator.app.zip` | Nothing | iPad simulator on a Mac |

With the secrets below you also get the store-ready `…-android.apk`, `…-android.aab` and
`…-ipad.ipa`, plus an automatic TestFlight upload for tagged releases.

### Secrets (Settings › Secrets and variables › Actions)

| Secret | What |
|---|---|
| `ANDROID_KEYSTORE_BASE64` | Your upload keystore, `base64 -w0 upload.jks` |
| `ANDROID_KEYSTORE_PASSWORD` | Keystore password |
| `ANDROID_KEY_ALIAS` | Key alias in the keystore |
| `ANDROID_KEY_PASSWORD` | Key password (defaults to the keystore password) |
| `IOS_CERTIFICATE_P12_BASE64` | *Apple Distribution* certificate + private key exported as .p12, base64 |
| `IOS_CERTIFICATE_PASSWORD` | Password of the .p12 |
| `IOS_PROVISIONING_PROFILE_BASE64` | App Store (or Ad Hoc) provisioning profile for the bundle ID, base64 |
| `APP_STORE_CONNECT_KEY_ID` | App Store Connect API key ID (TestFlight upload, optional) |
| `APP_STORE_CONNECT_ISSUER_ID` | Its issuer ID |
| `APP_STORE_CONNECT_KEY_P8` | Contents of the `AuthKey_XXXX.p8` file |

Repository **variable** `IOS_BUNDLE_ID` sets the iPad bundle ID (default `org.ezyxcraft.app`;
bundle IDs are global on Apple's side, so you will likely need your own, e.g.
`com.yourname.ezyxcraft`). It must match the provisioning profile.

An App Store profile gives an IPA for TestFlight / App Store. An Ad Hoc profile (one listing your
iPad's UDID) gives an IPA you can install directly; the workflow detects which one it has.

**Android keystore** (once; keep the file and passwords safe, Play needs the same key forever):

```bash
keytool -genkeypair -v -keystore upload.jks -alias upload -keyalg RSA -keysize 4096 -validity 10000
base64 -w0 upload.jks      # → ANDROID_KEYSTORE_BASE64 (macOS: base64 -i upload.jks)
```

**Apple** (needs the paid Apple Developer Program):

1. *Certificates, IDs & Profiles › Identifiers*: register an App ID with your bundle ID.
2. *Certificates*: create an **Apple Distribution** certificate. Then open it in Keychain Access
   and export the certificate with its private key as `.p12`. `base64 -i dist.p12 | pbcopy`
   gives you `IOS_CERTIFICATE_P12_BASE64`.
3. *Profiles*: create an **App Store Connect** (or **Ad Hoc**) profile for the App ID and that
   certificate. Download it, then `base64 -i EZYX.mobileprovision | pbcopy`.
4. For TestFlight: in App Store Connect, create the app record. Then go to *Users and Access ›
   Integrations › App Store Connect API*, make a key with the *App Manager* role, and download the
   `.p8`.

## Installing test builds

* **Android:** copy the `.apk` to the tablet, open it and allow *Install unknown apps* for your
  file manager. Or `adb install -r EZ-YX-craft-…-android-test.apk`. Logs: `adb logcat -s ezyx`.
* **iPad, ad hoc IPA:** drag the `.ipa` onto the iPad in Finder (or Apple Configurator).
* **iPad, unsigned IPA:** use AltStore or Sideloadly, which sign it with your Apple ID (free
  accounts: 7-day validity).
* **iPad simulator (Mac):** unzip, then `xcrun simctl install booted EZ-YX-craft.app` and
  `xcrun simctl launch booted org.ezyxcraft.app`.

## Building locally

**Android** (Linux, macOS or Windows): Android SDK + NDK (Android Studio's SDK Manager), JDK 17,
Gradle 8.14.

```bash
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/<version>
cargo ndk -t arm64-v8a -t x86_64 -P 26 -o mobile/android/app/src/main/jniLibs build --release -p ezyx-mobile
gradle -p mobile/android assembleRelease     # → mobile/android/app/build/outputs/apk/release/
adb install -r mobile/android/app/build/outputs/apk/release/app-release.apk
```

**iPad** (Mac with Xcode 26):

```bash
brew install xcodegen
mobile/ios/build-rust.sh                     # libezyx_mobile.a for device + simulator, Rust.xcconfig
xcodegen generate --spec mobile/ios/project.yml
open mobile/ios/EZYXCraft.xcodeproj          # pick your team under Signing, run on an iPad or simulator
```

Re-run `build-rust.sh` after Rust changes. Debug builds: `PROFILE=debug mobile/ios/build-rust.sh`.
App icons are generated from `assets/icon.png` with `python3 mobile/make_icons.py`.

## Platform notes and limits

* **Graphics.** `ezyx-gpu` classifies Apple A/M-series, Adreno, Mali/Immortalis, Xclipse and
  PowerVR GPUs. Tablets get capped budgets (`RenderBudget::mobile`: ≤ 900 k triangles per frame,
  ≤ 12 M simulation cells) because egui tessellates on the CPU and memory is shared with the OS.
  Android prefers Vulkan and falls back to OpenGL ES.
* **Graphics crashes.** Mobile apps can't relaunch themselves, so after a graphics crash the
  panic hook leaves a marker. The next start then uses halved budgets (and OpenGL ES on Android)
  and reopens the recovery copy (`gpu_guard::take_crash_marker`).
* **Files.** Files open and save only inside the app's folder: *On My iPad › EZ-YX-craft* or
  `Android/data/org.ezyxcraft.app/files`. The system document picker (UIDocumentPicker / Android
  SAF) is not wired up yet.
* **Machine.** Machine Control connects over Wi-Fi only (GRBL over TCP or WebSocket). iPad asks
  for *Local Network* permission the first time.
* **Android text input.** NativeActivity's soft keyboard covers ASCII text and numbers.
  Accessibility (accesskit) needs GameActivity, which would mean a Java/Kotlin activity; that is
  a possible later switch.
* **iOS SDK.** winit 0.30 uses the classic app lifecycle. Apple requires the UIScene lifecycle
  for apps built with the SDK after iOS 26, so CI pins Xcode 26. Moving to a later SDK needs a
  winit release with UIScene support.
* **Page size.** `.cargo/config.toml` links the Android library with 16 KB page alignment, which
  Google Play requires for Android 15+ devices.
