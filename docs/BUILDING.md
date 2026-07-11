# Building ChronaHLE

## Common prerequisites

- Git with submodule support
- Rust stable
- C and C++ build tools
- CMake
- Boost headers/libraries

Clone with submodules:

```text
git clone --recurse-submodules <repository-url> ChronaHLE
cd ChronaHLE
```

If the repository was cloned without them:

```text
git submodule update --init --recursive
```

ChronaHLE carries small reviewed patches for Dynarmic and SDL. Apply them once
after cloning or updating submodules. The scripts are idempotent and report
when the patches are already present:

```powershell
.\dev-scripts\apply-vendor-patches.ps1
```

On Linux:

```text
bash ./dev-scripts/apply-vendor-patches.sh
```

GitHub Actions performs this step automatically. Do not reset a patched
submodule and then build without applying the patches again: the added NEON
instructions are required by tested applications.

The internal Cargo binary is named `touchHLE`. Release bundles rename it to
ChronaHLE while keeping the internal library ABI stable.

## Windows

Install Visual Studio Build Tools with the Desktop C++ workload, Rust, CMake,
Python and Boost. Extract Boost into `vendor/boost`.

```powershell
cargo test --lib
cargo build --release --bin touchHLE
```

The executable is `target\release\touchHLE.exe`. Use
`dev-scripts/make-windows-bundle.sh` from Git Bash to create a distributable
bundle with runtime files.

## Linux

Ubuntu/Debian example prerequisites:

```text
sudo apt-get update
sudo apt-get install -y build-essential cmake ninja-build libboost-dev \
  libasound2-dev libpulse-dev libudev-dev libx11-dev libxcursor-dev \
  libxi-dev libxinerama-dev libxrandr-dev libxss-dev libwayland-dev \
  libxkbcommon-dev
```

Then run:

```text
cargo test --lib
cargo build --release --bin touchHLE
```

## Android ARM64

Requirements:

- Android SDK Platform 35
- Android NDK `28.0.13004108`
- JDK 17 or newer
- Rust target `aarch64-linux-android`
- `cargo-ndk` 3.5.4 or newer

```powershell
rustup target add aarch64-linux-android
cargo install cargo-ndk --version 3.5.4
cd android
.\gradlew.bat assembleRelease
```

On Linux use `./gradlew assembleRelease`. The APK is written to
`android/app/build/outputs/apk/release/app-release.apk`.

Local release builds fall back to the Android debug signing key so they remain
installable. Official releases use these environment variables:

- `CHRONAHLE_RELEASE_KEYSTORE`
- `CHRONAHLE_RELEASE_STORE_PASSWORD`
- `CHRONAHLE_RELEASE_KEY_ALIAS`
- `CHRONAHLE_RELEASE_KEY_PASSWORD`

Never commit a release keystore or its passwords.

## Integration tests

`cargo test --lib` is self-contained. The full `cargo test` command also builds
an ARM iPhone OS test application and therefore needs the LLVM/toolchain bundle
described in [the upstream test documentation](../tests/README.md). CI can
download that toolchain independently; it must not be committed.
