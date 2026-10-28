// EZ-YX-craft Android shell. The app is Rust (crates/ezyx-mobile → libezyx_mobile.so, built with
// cargo-ndk into app/src/main/jniLibs); Gradle only packages, signs and versions it.
// See docs/mobile.md.
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}
rootProject.name = "ez-yx-craft"
include(":app")
