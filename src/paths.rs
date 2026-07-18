/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Paths for host files used by ChronaHLE: settings, fonts, etc.
//!
//! There are three categories of files:
//!
//! * Resources bundled with ChronaHLE that neither ChronaHLE nor the user should
//!   modify: [DYLIBS_DIR], [FONTS_DIR], [DEFAULT_OPTIONS_FILE]. Depending on
//!   the platform these may or may not be ordinary files, and must be accessed
//!   through [ResourceFile].
//! * Files the user is expected to modify, but not ChronaHLE: [APPS_DIR],
//!   [USER_OPTIONS_FILE], [WALLPAPER_FILES]. These are ordinary files and are
//!   found in [user_data_base_path].
//! * Files that ChronaHLE will create and modify, and the user may modify if
//!   they want to: [SANDBOX_DIR]. These are ordinary files and are found in
//!   [user_data_base_path].
//!
//! See also [crate::fs], which provides a virtual filesystem for the guest app
//! and defines path types.

use std::borrow::Cow;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

/// Name of the directory containing ARMv6 dynamic libraries bundled with
/// ChronaHLE.
pub const DYLIBS_DIR: &str = "ChronaHLE_dylibs";
const LEGACY_DYLIBS_DIR: &str = "touchHLE_dylibs";

/// Name of the directory containing fonts bundled with ChronaHLE.
pub const FONTS_DIR: &str = "ChronaHLE_fonts";
const LEGACY_FONTS_DIR: &str = "touchHLE_fonts";

/// Name of the file containing ChronaHLE's default options for various apps.
pub const DEFAULT_OPTIONS_FILE: &str = "ChronaHLE_default_options.txt";
const LEGACY_DEFAULT_OPTIONS_FILE: &str = "touchHLE_default_options.txt";

/// macOS-only: If ChronaHLE is located in a .app bundle, return the path of the
/// Resources directory. If ChronaHLE is not located in a .app bundle, return
/// [None].
#[allow(dead_code)]
fn get_macos_bundled_resources_path() -> Option<PathBuf> {
    if std::env::consts::OS != "macos" {
        return None;
    }
    let base_path = PathBuf::from(sdl2::filesystem::base_path().ok()?);
    if base_path.file_name().is_some_and(|p| p == "Resources") {
        Some(base_path)
    } else {
        None
    }
}

#[cfg(target_os = "android")]
fn open_resource(path: &str) -> Result<sdl2::rwops::RWops<'static>, String> {
    sdl2::rwops::RWops::from_file(path, "r")
}

#[cfg(not(target_os = "android"))]
fn open_resource(path: &str) -> Result<std::fs::File, String> {
    let base_path = get_macos_bundled_resources_path();
    let path = base_path.as_deref().unwrap_or(Path::new(".")).join(path);
    std::fs::File::open(path).map_err(|e| e.to_string())
}

fn legacy_resource_path(path: &str) -> Option<String> {
    if path == DEFAULT_OPTIONS_FILE {
        Some(LEGACY_DEFAULT_OPTIONS_FILE.to_string())
    } else if let Some(suffix) = path.strip_prefix(DYLIBS_DIR) {
        Some(format!("{LEGACY_DYLIBS_DIR}{suffix}"))
    } else {
        path.strip_prefix(FONTS_DIR)
            .map(|suffix| format!("{LEGACY_FONTS_DIR}{suffix}"))
    }
}

/// Abstraction over a platform-specific type for accessing a bundled resource.
pub struct ResourceFile {
    #[cfg(target_os = "android")]
    file: sdl2::rwops::RWops<'static>,
    #[cfg(not(target_os = "android"))]
    file: std::fs::File,
}
impl ResourceFile {
    pub fn open(path: &str) -> Result<Self, String> {
        let file = match open_resource(path) {
            Ok(file) => file,
            Err(primary_error) => {
                let Some(legacy_path) = legacy_resource_path(path) else {
                    return Err(primary_error);
                };
                open_resource(&legacy_path).map_err(|legacy_error| {
                    format!(
                        "{primary_error}; legacy resource {legacy_path:?} also failed: {legacy_error}"
                    )
                })?
            }
        };
        Ok(Self { file })
    }
    pub fn get(&mut self) -> &mut (impl Read + Seek) {
        &mut self.file
    }
}
impl std::fmt::Debug for ResourceFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "ResourceFile")
    }
}

/// Whether various resources are in user-accessible files. If they aren't,
/// ChronaHLE has to be able to display their license terms.
pub const RESOURCES_ARE_EXTERNAL_FILES: bool = cfg!(not(target_os = "android"));

/// Name of the directory where the user can put apps if they want them to
/// appear in the app picker.
pub const APPS_DIR: &str = "ChronaHLE_apps";
const LEGACY_APPS_DIR: &str = "touchHLE_apps";

/// Name of the file intended for the user's own options.
pub const USER_OPTIONS_FILE: &str = "ChronaHLE_options.txt";
const LEGACY_USER_OPTIONS_FILE: &str = "touchHLE_options.txt";

/// Names of files the user can put a wallpaper image (for the app picker) in.
#[allow(unused)]
pub const WALLPAPER_FILES: &[&str] = &[
    "ChronaHLE_wallpaper.png",
    "ChronaHLE_wallpaper.jpg",
    "ChronaHLE_wallpaper.jpeg",
];
const LEGACY_WALLPAPER_FILES: &[&str] = &[
    "touchHLE_wallpaper.png",
    "touchHLE_wallpaper.jpg",
    "touchHLE_wallpaper.jpeg",
];

/// Name of the directory where ChronaHLE will store sandboxed app data, e.g.
/// the `Documents` directory.
pub const SANDBOX_DIR: &str = "ChronaHLE_sandbox";
const LEGACY_SANDBOX_DIR: &str = "touchHLE_sandbox";

/// Get a platform-specific base path needed for accessing ChronaHLE's
/// user-modifiable files. This is empty on platforms other than Android.
pub fn user_data_base_path() -> Cow<'static, Path> {
    #[cfg(target_os = "android")]
    unsafe {
        // This is an exception to the rule that SDL2 should only be used
        // directly from src/window.rs. This is just too distant from windowing
        // to belong there.

        // Android storage has evolved in a quite messy fashion. Both "internal
        // storage" and "external storage" (aka the "SD card") are likely to be
        // internal on a modern device, as absurd as that might sound. SDL2 has
        // APIs to get paths for both. We use the "external storage" because
        // it's more likely to be user-accessible.
        extern "C" {
            fn SDL_AndroidGetExternalStoragePath() -> *const std::ffi::c_char;
        }
        let path = SDL_AndroidGetExternalStoragePath();
        if path.is_null() {
            log!("Couldn't get Android external storage path!");
            panic!();
        }
        Cow::from(Path::new(std::ffi::CStr::from_ptr(path).to_str().unwrap()))
    }
    #[cfg(not(target_os = "android"))]
    {
        // When ChronaHLE is run from a .app bundle on macOS, the user might not
        // be able to control the current directory, so user data needs to go in
        // a standard location.
        if get_macos_bundled_resources_path().is_some() {
            return Cow::from(PathBuf::from(
                sdl2::filesystem::pref_path("chronahle.xyz", "ChronaHLE").unwrap(),
            ));
        }
        Cow::from(Path::new("."))
    }
}

/// Get a URI that can be used to open a file manager or similar for the path
/// that [user_data_base_path] represents.
pub fn url_for_opening_user_data_dir() -> Result<String, String> {
    if std::env::consts::OS == "android" {
        // See DocumentsProvider.kt, app/build.gradle and AndroidManifest.xml
        let brand = crate::branding();
        Ok(format!(
            "content://org.chronahle.android{}{}.provider/root/root",
            if brand.is_empty() { "" } else { "." },
            brand.to_lowercase()
        ))
    } else {
        let path = user_data_base_path()
            .join(".")
            .canonicalize()
            .map_err(|e| format!("Can't canonicalize path to user data directory: {e}"))?;
        let path = path
            .to_str()
            .ok_or_else(|| "User data directory path is not UTF-8".to_string())?;
        // std::fs::canonicalize() on Windows uses the extended-length path
        // syntax, but Windows Explorer doesn't understand it.
        let path = if std::env::consts::OS == "windows" {
            path.strip_prefix("\\\\?\\").unwrap_or(path)
        } else {
            path
        };
        Ok(format!("file://{path}"))
    }
}

fn migrate_legacy_path(base_path: &Path, legacy_name: &str, current_name: &str) {
    let legacy_path = base_path.join(legacy_name);
    let current_path = base_path.join(current_name);
    if current_path.exists() || !legacy_path.exists() {
        return;
    }
    match std::fs::rename(&legacy_path, &current_path) {
        Ok(()) => {
            log!(
                "Migrated legacy ChronaHLE data: {} -> {}",
                legacy_path.display(),
                current_path.display()
            );
        }
        Err(error) => {
            log!(
                "Warning: Couldn't migrate {} to {}: {}",
                legacy_path.display(),
                current_path.display(),
                error
            );
        }
    }
}

fn migrate_legacy_user_data(base_path: &Path) {
    migrate_legacy_path(base_path, LEGACY_APPS_DIR, APPS_DIR);
    migrate_legacy_path(base_path, LEGACY_SANDBOX_DIR, SANDBOX_DIR);
    migrate_legacy_path(base_path, LEGACY_USER_OPTIONS_FILE, USER_OPTIONS_FILE);
    for (&legacy_name, &current_name) in LEGACY_WALLPAPER_FILES.iter().zip(WALLPAPER_FILES) {
        migrate_legacy_path(base_path, legacy_name, current_name);
    }
}

/// Create the user data directory, migrate legacy names and populate templates.
pub fn prepopulate_user_data_dir() {
    let base_path = user_data_base_path();
    migrate_legacy_user_data(&base_path);

    if std::env::consts::OS != "android" && std::env::consts::OS != "macos" {
        return;
    }
    if base_path == Path::new(".") {
        return;
    }

    let apps_dir = base_path.join(APPS_DIR);
    if !apps_dir.is_dir() {
        match std::fs::create_dir(&apps_dir) {
            Ok(()) => {
                log!("Created: {}", apps_dir.display());
            }
            Err(e) => {
                log!("Warning: Couldn't create {}: {}", apps_dir.display(), e);
            }
        }
    }

    fn create_file(path: &Path, content: &str) {
        match std::fs::write(path, content) {
            Ok(()) => {
                log!("Created: {}", path.display());
            }
            Err(e) => {
                log!("Warning: Couldn't create {}: {}", path.display(), e);
            }
        }
    }

    let apps_dir_readme = apps_dir.join("README.txt");
    if !apps_dir_readme.is_file() {
        let content = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/ChronaHLE_apps/README.txt"
        ));
        create_file(&apps_dir_readme, content);
    }

    let user_options = base_path.join(USER_OPTIONS_FILE);
    if !user_options.is_file() {
        let content = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/ChronaHLE_options.txt"
        ));
        create_file(&user_options, content);
    }

    let options_help = base_path.join("OPTIONS_HELP.txt");
    if !options_help.is_file() {
        create_file(&options_help, crate::options::OPTIONS_HELP);
    }
}
