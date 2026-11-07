// Versions and signing come from the environment so CI (and you) can build without editing:
//   EZYX_VERSION_NAME (e.g. 0.2.0), EZYX_VERSION_CODE (integer, must grow for Play uploads)
//   EZYX_ANDROID_KEYSTORE (path to a .jks), EZYX_ANDROID_KEYSTORE_PASSWORD,
//   EZYX_ANDROID_KEY_ALIAS, EZYX_ANDROID_KEY_PASSWORD
// Without a keystore the release build is signed with the debug key: installable for testing,
// not accepted by Google Play.
plugins {
    id("com.android.application")
}

val releaseKeystore = System.getenv("EZYX_ANDROID_KEYSTORE")?.let { file(it) }?.takeIf { it.exists() }

android {
    namespace = "org.ezyxcraft.app"
    compileSdk = 36

    defaultConfig {
        applicationId = "org.ezyxcraft.app"
        // Android 8.0: Vulkan 1.0 / GLES 3.2 drivers good enough for wgpu.
        minSdk = 26
        targetSdk = 36
        versionCode = System.getenv("EZYX_VERSION_CODE")?.toIntOrNull() ?: 1
        versionName = System.getenv("EZYX_VERSION_NAME") ?: "0.0.0-dev"
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    signingConfigs {
        if (releaseKeystore != null) {
            create("release") {
                storeFile = releaseKeystore
                storePassword = System.getenv("EZYX_ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("EZYX_ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("EZYX_ANDROID_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        getByName("release") {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName(if (releaseKeystore != null) "release" else "debug")
        }
    }

    packaging {
        jniLibs {
            // Store the Rust library uncompressed and page-aligned (16 KB pages on Android 15+).
            useLegacyPackaging = false
        }
    }
}
