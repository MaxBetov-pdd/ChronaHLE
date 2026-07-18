# Stable release checklist

A successful compiler or APK build is not enough for a stable ChronaHLE
release. A release tag may be approved only after the following matrix passes
on the exact candidate commit.

Commercial test applications are supplied locally by the tester and are never
committed, uploaded as CI artifacts or redistributed with ChronaHLE.

## Required hosts

- Windows x86_64 release bundle.
- Android ARM64 signed release APK on a physical 16 KB-page-capable device.
- Linux x86_64 release build and unit tests in CI. Runtime game testing is
  recommended when suitable hardware is available.

## LEGO Ninjago: The Final Battle

Application identifier: `com.lego.ninjago.thefinalbattle`.

- Reach the main menu without missing UI.
- Start a level and play for at least ten minutes.
- Verify touch coordinates, orientation and full-frame presentation.
- Verify music and effects without continuous crackle or repeated underruns.
- Confirm no fatal missing symbol/API messages appeared in the log.

## Angry Birds 5.2.5

- Reach the main menu and level selector.
- Start a level and play for at least ten minutes.
- Complete/restart a level and return to menus.
- Verify GLES2 rendering, input coordinates, audio and save data.
- Confirm no fatal missing symbol/API messages appeared in the log.

## General checks

- `cargo fmt --all -- --check`
- `cargo test --lib`
- Windows and Linux release builds pass.
- Android APK signature and ZIP alignment verify.
- Every Android native library has 16 KB-compatible LOAD alignment.
- No IPA, extracted application, log, keystore or local path is present in Git.
- `--copyright` shows MPL-2.0 and touchHLE attribution.
- Release notes list known limitations, including host-specific graphics and
  audio differences still observed in tested applications.

The GitHub `release` environment should require manual approval. Approval means
the tester has completed this checklist for the candidate commit.
