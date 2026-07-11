# Contributing to ChronaHLE

ChronaHLE welcomes focused fixes that improve reusable emulator behavior.
Implement real API semantics where practical; application-specific workarounds
must be isolated, documented and used only when a general implementation is
not currently possible.

## Setup

1. Clone the repository with submodules.
2. Follow [docs/BUILDING.md](docs/BUILDING.md).
3. Create a branch from `main`.
4. Keep changes scoped and preserve MPL-2.0 headers on derived files.

Before opening a pull request, run:

```text
cargo fmt --all -- --check
cargo test --lib
cargo build --release --bin touchHLE
```

Platform-specific changes should also run the relevant build documented in
`docs/BUILDING.md`. Do not commit IPA files, extracted applications, logs,
copyrighted SDKs, signing keys, build outputs or user data.

## Compatibility reports

Include the application identifier and version, guest architecture, minimum OS
version, host OS/GPU, exact ChronaHLE commit and the smallest useful log. Do not
upload commercial application binaries or copyrighted game assets.

## Upstream work

ChronaHLE retains touchHLE history and periodically reviews upstream changes.
When a fix is also appropriate for touchHLE, contributors are encouraged to
submit it upstream under that project's contribution process.
