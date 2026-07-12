/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Parts of this file are derived from SDL 2's Android project template, which
 * has a different license. Please see vendor/SDL/LICENSE.txt for details.
 */
import org.gradle.nativeplatform.platform.internal.DefaultNativePlatform
import org.gradle.api.tasks.Sync

plugins {
    id("com.android.application") version("8.10.1")
    id("com.github.willir.rust.cargo-ndk-android") version("0.3.4")
    id("org.jetbrains.kotlin.android") version("2.0.21")
}

fun runTouchHLEVersionTool(wantBranding: Boolean): String {
    val output = providers.exec {
        commandLine("cargo", "run", "--package", "touchHLE_version")
        if (wantBranding) {
            args("--", "--branding")
        }
    }.standardOutput.asText.get().trim()

    return output
}

fun getTouchHLEBranding(): String {
    return runTouchHLEVersionTool(/* wantBranding: */ true)
}

fun getTouchHLEVersionName(): String {
    return runTouchHLEVersionTool(/* wantBranding: */ false)
}

fun join(prefix: String, separator: String, branding: String): String {
    return if (branding.isEmpty()) prefix else prefix + separator + branding
}

val generatedRuntimeAssets = layout.buildDirectory.dir("generated/chronahle/runtime-assets")
val syncRuntimeAssets by tasks.registering(Sync::class) {
    into(generatedRuntimeAssets)
    from(rootDir.parentFile.resolve("touchHLE_default_options.txt"))
    from(rootDir.parentFile.resolve("touchHLE_dylibs")) {
        into("touchHLE_dylibs")
    }
    from(rootDir.parentFile.resolve("touchHLE_fonts")) {
        into("touchHLE_fonts")
    }
}

android {
    ndkVersion = "28.0.13004108"
    compileSdk = 35
    buildFeatures {
        buildConfig = true
    }
    defaultConfig {
        val branding = getTouchHLEBranding()
        applicationId = "org.chronahle.android"
        if (!branding.isEmpty()) {
            applicationIdSuffix = branding.lowercase()
        }
        resValue("string", "app_name", join("ChronaHLE", " ", branding))
        buildConfigField("String", "APP_NAME", "\"${join("ChronaHLE", " ", branding)}\"")
        manifestPlaceholders["icon"] = "@drawable/icon"
        buildConfigField("int", "APP_ICON", "R.drawable.icon")
        versionCode = 1
        versionName = join(getTouchHLEVersionName(), " ", branding)

        minSdk = 21 // first version with AArch64
        targetSdk = 35
        externalNativeBuild {
            ndkBuild {
                arguments("APP_PLATFORM=android-21")
                // abiFilters 'armeabi-v7a', 'arm64-v8a', 'x86', 'x86_64'
                // Only 'arm64-v8a' and 'x86_64' are supported by dynarmic
                // and hence touchHLE. The 'x86_64' build works, but the main
                // use for that would be the emulator in Android Studio, and
                // its OpenGL ES implementations don't seem to work properly
                // with touchHLE, so we disable it to reduce build time and
                // avoid shipping stuff we haven't meaningfully tested.
                // Make sure this matches the cargoNdk targets below.
                abiFilters("arm64-v8a")
            }
        }
    }
    // The target JVM version must be the same for Java and Kotlin.
    compileOptions {
        sourceCompatibility(JavaVersion.VERSION_11)
        targetCompatibility(JavaVersion.VERSION_11)
    }
    kotlinOptions {
        jvmTarget = "11"
    }
    buildTypes {
        release {
            val keystorePath = System.getenv("CHRONAHLE_RELEASE_KEYSTORE")?.takeIf { it.isNotBlank() }
            val keystorePassword = System.getenv("CHRONAHLE_RELEASE_STORE_PASSWORD")?.takeIf { it.isNotBlank() }
            val keyAliasValue = System.getenv("CHRONAHLE_RELEASE_KEY_ALIAS")?.takeIf { it.isNotBlank() }
            val keyPasswordValue = System.getenv("CHRONAHLE_RELEASE_KEY_PASSWORD")?.takeIf { it.isNotBlank() }
            if (keystorePath != null && file(keystorePath).isFile && keystorePassword != null && keyAliasValue != null && keyPasswordValue != null) {
                signingConfig = signingConfigs.create("chronaRelease") {
                    storeFile = file(keystorePath)
                    storePassword = keystorePassword
                    keyAlias = keyAliasValue
                    keyPassword = keyPasswordValue
                }
            } else {
                // Installable local builds use the debug key. Tagged releases
                // are signed with the four CHRONAHLE_RELEASE_* secrets in CI.
                signingConfig = signingConfigs.getByName("debug")
            }
            isMinifyEnabled = false
            isDebuggable = false
        }
        debug {
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("**/*.so")
            }
            isDebuggable = true
            isJniDebuggable = true
        }
    }

    applicationVariants.all {
        val variantName = name.replaceFirstChar { char ->
            if (char.isLowerCase()) char.titlecase() else char.toString()
        }
        tasks.named("merge${variantName}Assets").configure {
            dependsOn("externalNativeBuild${variantName}")
        }
    }

    sourceSets {
        getByName("main") {
            java.srcDir("${rootDir.parentFile}/vendor/SDL/android-project/app/src/main/java")
            assets.srcDir(generatedRuntimeAssets)
        }
    }

    tasks.named("preBuild").configure {
        dependsOn(syncRuntimeAssets)
    }

    if (!project.hasProperty("EXCLUDE_NATIVE_LIBS")) {
        sourceSets {
            getByName("main") {
                jniLibs.srcDir("${projectDir}/jniLibs")
            }
        }
        externalNativeBuild {
            ndkBuild {
                path("jni/Android.mk")
            }
        }
    }

    lint {
        abortOnError = false
    }
    packaging {
        jniLibs.useLegacyPackaging = false
    }
    namespace = "org.chronahle.android"
}

cargoNdk {
    // Make sure this matches the android abiFilters above.
    targets = arrayListOf("arm64")
    module = ".."
    librariesNames = arrayListOf("libtouchHLE.so", "libSDL2.so", "libc++_shared.so")
    extraCargoEnv = mapOf(
        "ANDROID_NDK" to android.ndkDirectory.toString(),
        "ANDROID_NDK_HOME" to android.ndkDirectory.toString(),
        "CMAKE_EXE_LINKER_FLAGS" to "-Wl,-z,max-page-size=16384 -Wl,-z,common-page-size=16384",
        "CMAKE_MODULE_LINKER_FLAGS" to "-Wl,-z,max-page-size=16384 -Wl,-z,common-page-size=16384",
        "CMAKE_SHARED_LINKER_FLAGS" to "-Wl,-z,max-page-size=16384 -Wl,-z,common-page-size=16384",
        "RUSTFLAGS" to "-C link-arg=-Wl,-z,max-page-size=16384 -C link-arg=-Wl,-z,common-page-size=16384",
        // NDK 28 rejects SDL 2.26's obsolete pollAll call. Both APIs have the
        // same signature; compile the bundled SDL against the supported API.
        "CFLAGS" to "-DALooper_pollAll=ALooper_pollOnce",
    )

    if (DefaultNativePlatform.host().operatingSystem.isWindows) {
        val binPath =
            android.ndkDirectory.toPath().resolve("toolchains/llvm/prebuilt/windows-x86_64/bin")
        val clangPath = binPath.resolve("clang.exe")
        val clangXXPath = binPath.resolve("clang++.exe")

        if (!clangPath.toFile().exists()) {
            throw GradleException("NDK clang compiler not found at expected location: $clangPath")
        }
        if (!clangXXPath.toFile().exists()) {
            throw GradleException("NDK clang++ compiler not found at expected location: $clangXXPath")
        }

        extraCargoEnv.putAll(
            mapOf(
                "CC" to clangPath.toString(),
                "CXX" to clangXXPath.toString(),
                // The default generator on Windows (Visual Studio) does not respect
                // the CC and CXX environment variables. Using Ninja ensures that
                // the specified compilers are used
                "CMAKE_GENERATOR" to "Ninja",
            )
        )
    }
    // The default feature, "static", makes us use static linking for SDL2 and OpenAL Soft.
    // For Android, we need dynamic linking for SDL2, but static linking for OpenAL Soft.
    extraCargoBuildArguments = arrayListOf(
        "--lib",
        "--no-default-features",
        "--features",
        "touchHLE_openal_soft_wrapper/static,sdl2/bundled"
    )
}

dependencies {
    implementation(fileTree("libs") {
        include("*.jar")
    })
}
