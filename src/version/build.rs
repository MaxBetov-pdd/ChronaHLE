/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::path::{Path, PathBuf};
use std::process::Command;

fn rerun_if_changed(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.to_str().unwrap());
}

pub fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let package_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = package_root.join("../..");

    // Upstream tags remain in the fork history, so a normal `git describe`
    // would incorrectly report a touchHLE version. Only accept an exact tag
    // matching ChronaHLE's Cargo version; otherwise append the commit id.
    let toml_version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let expected_tag = format!("v{toml_version}");
    let exact_tag = Command::new("git")
        .args(["describe", "--tags", "--exact-match"])
        .output();
    let exact_tag = exact_tag
        .ok()
        .filter(|result| result.status.success())
        .and_then(|result| String::from_utf8(result.stdout).ok())
        .map(|tag| tag.trim().to_string());

    let version = if exact_tag.as_deref() == Some(expected_tag.as_str()) {
        expected_tag
    } else {
        let revision = Command::new("git")
            .args(["rev-parse", "--short=10", "HEAD"])
            .output();
        match revision {
            Ok(revision) if revision.status.success() => {
                let revision = String::from_utf8(revision.stdout).unwrap();
                let dirty = Command::new("git")
                    .args(["status", "--porcelain", "--untracked-files=no"])
                    .output()
                    .ok()
                    .is_some_and(|status| !status.stdout.is_empty());
                rerun_if_changed(&workspace_root.join(".git/HEAD"));
                rerun_if_changed(&workspace_root.join(".git/refs"));
                format!(
                    "v{toml_version}-dev+{}{}",
                    revision.trim(),
                    if dirty { "-dirty" } else { "" }
                )
            }
            _ => format!("v{toml_version}-dev+unknown"),
        }
    };
    rerun_if_changed(&workspace_root.join("Cargo.toml"));
    std::fs::write(out_dir.join("version.txt"), version).unwrap();
}
