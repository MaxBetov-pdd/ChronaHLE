# ChronaHLE

[![CI](https://github.com/MaxBetov-pdd/ChronaHLE/actions/workflows/build.yml/badge.svg)](https://github.com/MaxBetov-pdd/ChronaHLE/actions/workflows/build.yml)
[![Latest release](https://img.shields.io/github/v/release/MaxBetov-pdd/ChronaHLE?display_name=tag)](https://github.com/MaxBetov-pdd/ChronaHLE/releases/latest)
[![License: MPL-2.0](https://img.shields.io/badge/license-MPL--2.0-blue.svg)](LICENSE)
[![Website](https://img.shields.io/badge/website-chronahle.xyz-13b8a6.svg)](https://chronahle.xyz/)
[![Compatibility database](https://img.shields.io/badge/game%20compatibility-appdb.chronahle.xyz-4c8bf5.svg)](https://appdb.chronahle.xyz/)

Game compatibility, tested versions, platform-specific results and current
support status are tracked at
[appdb.chronahle.xyz](https://appdb.chronahle.xyz/).

ChronaHLE is a cross-platform high-level emulator targeting the complete
32-bit era from iPhone OS 1 through iOS 6. The goal is one emulator for games
and applications across these releases, with compatibility expanded globally
through reusable framework and system API implementations rather than
per-game patches.

ChronaHLE reimplements Foundation, UIKit, OpenGL ES, AudioToolbox, Objective-C
runtime services and other parts of iOS while Dynarmic executes guest ARM code.
Support is still developed and verified application by application, so the
iPhone OS 1-iOS 6 target range does not yet mean every application works.

Long term, ChronaHLE is designed to grow beyond the 32-bit generation. The
roadmap includes a new ARM64 guest execution and ABI layer, 64-bit Mach-O
loading, and the newer APIs needed to make iOS 7 and later applications
possible. This is an architectural direction, not implemented compatibility
today.

ChronaHLE is derived from [touchHLE](https://github.com/touchHLE/touchHLE) and
retains the required license notices and third-party acknowledgements. See
[FORK_NOTICE.md](FORK_NOTICE.md) for the exact relationship.

Project website: [chronahle.xyz](https://chronahle.xyz/)

> ChronaHLE is early development software. Compatibility is per-application,
> and no Apple software, applications, games or encryption keys are included.

## Current scope

- Guest CPU: ARMv6 and ARMv7 (32-bit) through Dynarmic.
- Guest OS target: iPhone OS 1 through iOS 6, with compatibility still varying
  by application and release.
- Graphics: OpenGL ES 1.1 and an expanding OpenGL ES 2.0 compatibility path.
- Hosts: Windows, Linux and Android share the same Rust core.
- Future scope: ARM64 guest execution and iOS 7+ API expansion. Neither is
  implemented yet.

## Supported platforms

| Platform | Host architecture | Release package |
| --- | --- | --- |
| Windows | x86_64 | ZIP archive |
| Linux | x86_64 | TAR.GZ archive |
| Android | ARM64 | APK |

All three platforms use the same Rust emulation core and receive the same iOS
framework and compatibility implementations. Windowing, graphics drivers,
audio devices, input and packaging use platform-specific integrations, so an
individual application can still behave differently between hosts.

More detail is in [docs/PLATFORMS.md](docs/PLATFORMS.md). Public application
compatibility status is maintained at
[appdb.chronahle.xyz](https://appdb.chronahle.xyz/).

## Quick start

1. Download the build for your host from
   [GitHub Releases](https://github.com/MaxBetov-pdd/ChronaHLE/releases) or build
   ChronaHLE using
   [docs/BUILDING.md](docs/BUILDING.md).
2. Use only decrypted `.ipa` or `.app` files that you are legally entitled to
   use. ChronaHLE does not bypass DRM.
3. Desktop: run `ChronaHLE.exe "Path to App.ipa"` (Windows) or the equivalent
   binary on Linux.
4. Android: launch ChronaHLE once, then place applications in the
   `ChronaHLE_apps` data directory through the Android document provider.

ChronaHLE automatically migrates legacy local app, sandbox, options and
wallpaper paths from builds that used the old prefix.

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
cargo build --release --bin ChronaHLE
```

On Windows, use `dev-scripts\apply-vendor-patches.ps1` for the first command.

See [docs/BUILDING.md](docs/BUILDING.md) for platform prerequisites.

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
