# ChronaHLE

ChronaHLE is an experimental high-level emulator for classic 32-bit iPhone OS
and iOS applications. It reimplements system frameworks such as Foundation,
UIKit, OpenGL ES, AudioToolbox and Objective-C runtime services while Dynarmic
executes the guest ARM code.

ChronaHLE is derived from [touchHLE](https://github.com/touchHLE/touchHLE) and
retains its commit history, license notices and third-party acknowledgements.
See [FORK_NOTICE.md](FORK_NOTICE.md) for the exact relationship.

> ChronaHLE is early development software. Compatibility is per-application,
> and no Apple software, applications, games or encryption keys are included.

## Current scope

- Guest CPU: ARMv6 and ARMv7 (32-bit) through Dynarmic.
- Guest OS APIs: original iPhone OS APIs plus ongoing iOS 4-iOS 6 expansion.
- Graphics: OpenGL ES 1.1 and an expanding OpenGL ES 2.0 compatibility path.
- Hosts: Windows, Linux and Android share the same Rust core.
- ARM64 iOS guests are a planned new execution/API layer, not implemented yet.

## Platform status

| Host | Status | Notes |
| --- | --- | --- |
| Windows x86_64 | Verified | Primary development platform; release build and unit tests pass. |
| Linux x86_64 | CI target | Built and tested by GitHub Actions. Runtime graphics still needs hardware coverage. |
| Android ARM64 | Experimental | APK, 16 KB page alignment and native GLES2 compilation are verified. Physical-device game testing is still required. |

More detail is in [docs/PLATFORMS.md](docs/PLATFORMS.md).
Candidate game-test results are tracked in
[docs/COMPATIBILITY.md](docs/COMPATIBILITY.md).

## Quick start

1. Obtain a release for your host or build ChronaHLE using
   [docs/BUILDING.md](docs/BUILDING.md).
2. Use only decrypted `.ipa` or `.app` files that you are legally entitled to
   use. ChronaHLE does not bypass DRM.
3. Desktop: run `ChronaHLE.exe "Path to App.ipa"` (Windows) or the equivalent
   binary on Linux.
4. Android: launch ChronaHLE once, then place applications in the
   `touchHLE_apps` data directory through the Android document provider.

Internal data directory names currently retain the `touchHLE_*` prefix so
existing runtime layouts and upstream tools remain compatible.

## Architecture

```text
Guest application (ARMv6/ARMv7 Mach-O)
        |
        +-- Dynarmic CPU execution and guest memory
        +-- Objective-C runtime and dynamic linker
        +-- HLE frameworks (UIKit, Foundation, OpenGL ES, AudioToolbox, ...)
        |
Shared Rust core
        |
SDL / OpenGL or EGL / OpenAL / host filesystem
        |
Windows | Linux | Android
```

Platform frontends are deliberately thin. Most API implementations and game
compatibility fixes benefit every host, while graphics context creation,
window lifecycle, JIT permissions and packaging remain host-specific.

## Development

Useful commands:

```text
bash ./dev-scripts/apply-vendor-patches.sh
cargo fmt --all -- --check
cargo test --lib
cargo build --release --bin touchHLE
```

On Windows, use `dev-scripts\apply-vendor-patches.ps1` for the first command.

The internal Cargo binary is still named `touchHLE`; release packaging renames
it to ChronaHLE. This avoids a risky ABI-wide rename while the fork stabilizes.

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution rules and
[docs/BUILDING.md](docs/BUILDING.md) for platform prerequisites.

Stable tags additionally require the manual game regression matrix in
[docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md).

## Legal

ChronaHLE is not affiliated with or endorsed by Apple Inc. Apple product names
and trademarks belong to their respective owners.

The covered source code is available under the Mozilla Public License 2.0.
See [LICENSE](LICENSE). Bundled fonts, guest dynamic libraries and third-party
submodules retain their own licenses. The application exposes dependency
license information through `--copyright` and its graphical copyright view.

Copyright 2023-2026 touchHLE project contributors.
ChronaHLE modifications copyright 2026 ChronaHLE contributors.
