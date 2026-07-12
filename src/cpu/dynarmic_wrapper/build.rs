/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::env;
use std::fs;
use std::path::Path;

fn rerun_if_changed(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.to_str().unwrap());
}

fn rerun_if_dynarmic_source_changed(path: &Path) {
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rerun_if_dynarmic_source_changed(&path);
            continue;
        }

        let extension = path.extension().and_then(|extension| extension.to_str());
        let is_source = matches!(
            extension,
            Some("c" | "cc" | "cpp" | "h" | "hpp" | "inc" | "cmake")
        );
        let is_cmake_list =
            path.file_name().and_then(|name| name.to_str()) == Some("CMakeLists.txt");
        if is_source || is_cmake_list {
            rerun_if_changed(&path);
        }
    }
}
fn link_search(path: &Path) {
    println!("cargo:rustc-link-search=native={}", path.to_str().unwrap());
}
fn link_lib(lib: &str) {
    println!("cargo:rustc-link-lib=static={lib}");
}

fn build_type_windows() -> &'static str {
    let os = env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS was not set");
    if os.eq_ignore_ascii_case("windows") {
        if cfg!(debug_assertions) {
            "Debug"
        } else {
            "Release"
        }
    } else {
        ""
    }
}

fn main() {
    let package_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = package_root.join("../../..");
    let dynarmic_root = workspace_root.join("vendor/dynarmic");

    let mut build = cmake::Config::new(&dynarmic_root);
    build.define("DYNARMIC_FRONTENDS", "A32"); // We don't need 64-bit
    build.define("DYNARMIC_WARNINGS_AS_ERRORS", "OFF");
    build.define("DYNARMIC_TESTS", "OFF");
    build.define("DYNARMIC_USE_BUNDLED_EXTERNALS", "ON");
    build.define("CMAKE_POLICY_VERSION_MINIMUM", "3.5");

    // This is Windows- and Android-specific because on macOS or Linux, you can
    // easily get Boost with a package manager.
    let os = env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS was not set");
    let host = env::var("HOST").expect("HOST was not set");
    let boost_path = workspace_root.join("vendor/boost");
    let needs_vendored_boost = os.eq_ignore_ascii_case("windows")
        || (os.eq_ignore_ascii_case("android") && host.contains("windows"));
    if needs_vendored_boost && !boost_path.is_dir() {
        panic!("Could not find Boost. Download it from https://www.boost.org/users/download/ and put it at vendor/boost");
    }
    // Allow providing Boost manually regardless of what platform we're on
    // (or whether the target platform was detected correctly…)
    if boost_path.is_dir() {
        build.define("Boost_INCLUDE_DIR", boost_path);
    } else if Path::new("/usr/include/boost").is_dir() {
        // Android cross-CMake restricts its default search roots to the NDK.
        // CI installs host Boost headers, so point CMake at them explicitly.
        build.define("Boost_INCLUDE_DIR", "/usr/include");
    }
    // Prevent CMake from using macOS-only linker commands when cross-compiling
    // for Android.
    // https://stackoverflow.com/questions/69697715/cross-compiling-c-program-for-android-on-mac-failed-using-ndks-clang
    if os.eq_ignore_ascii_case("android") {
        build.define("CMAKE_SYSTEM_NAME", "Android");
        build.define("CMAKE_SYSTEM_VERSION", "21");
        build.define("ANDROID", "ON");
    }
    // dynarmic can't be dynamically linked
    let dynarmic_out = build.build();

    if os.eq_ignore_ascii_case("android") {
        // Work around weird issue with the NDK where there are missing
        // references to compiler-rt/libgcc symbols.
        // Translated from: https://github.com/termux/termux-packages/issues/8029#issuecomment-1369150244
        let mut cc_command = cc::Build::new().get_compiler().to_command();
        let libclang_rt_path = cc_command
            .arg("-print-libgcc-file-name")
            .output()
            .unwrap()
            .stdout;
        let libclang_rt_path: &Path = std::str::from_utf8(&libclang_rt_path).unwrap().as_ref();
        link_search(libclang_rt_path.parent().unwrap());
        link_lib(
            libclang_rt_path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .trim()
                .strip_prefix("lib")
                .unwrap()
                .strip_suffix(".a")
                .unwrap(),
        );
    }

    link_search(&dynarmic_out.join("lib"));
    link_search(&dynarmic_out.join("lib64")); // some Linux systems
    link_lib("dynarmic");
    link_search(
        &dynarmic_out
            .join("build/externals/fmt")
            .join(build_type_windows()),
    );
    link_lib(
        if os.eq_ignore_ascii_case("windows") && cfg!(debug_assertions) {
            "fmtd"
        } else {
            "fmt"
        },
    );
    link_search(
        &dynarmic_out
            .join("build/externals/mcl/src")
            .join(build_type_windows()),
    );
    link_lib("mcl");
    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH was not set");
    if arch.eq_ignore_ascii_case("x86_64") {
        link_search(
            &dynarmic_out
                .join("build/externals/zydis")
                .join(build_type_windows()),
        );
        link_lib("Zydis");
    }

    // Cargo does not reliably notice changes below a watched directory, so
    // register Dynarmic's build inputs individually.
    rerun_if_dynarmic_source_changed(&dynarmic_root);
    rerun_if_changed(&workspace_root.join(".git/modules/dynarmic/HEAD"));

    cc::Build::new()
        .file(package_root.join("lib.cpp"))
        .cpp(true)
        .std("c++17")
        .include(dynarmic_out.join("include"))
        .compile("dynarmic_wrapper");
    rerun_if_changed(&package_root.join("lib.cpp"));
}
