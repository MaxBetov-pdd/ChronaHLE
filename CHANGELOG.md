# ChronaHLE changelog

All notable ChronaHLE-specific changes are recorded here. Historical touchHLE
release notes are preserved in [UPSTREAM_CHANGELOG.md](UPSTREAM_CHANGELOG.md).

## 0.3.0 - 2026-07-18

- Established the ChronaHLE product identity and independent Android package.
- Renamed the public executable, native Android library, Rust packages,
  resources and user-data paths to ChronaHLE, with legacy data migration.
- Added reproducible Windows, Linux and Android release artifacts with SHA-256
  checksums and automatic GitHub Release publication for version tags.
- Expanded iOS 4-iOS 6 framework, Objective-C runtime and libc compatibility.
- Added ARM fastmem support with protected null pages.
- Added broader ARMv7/NEON instruction support through the Dynarmic submodule.
- Added OpenGL ES 2 entry points and reusable EAGL presentation resources.
- Fixed scaled EAGL drawable presentation and input alignment.
- Added AudioUnit buffering and render-loop servicing for engine-owned loops.
- Added Android 16 KB page alignment and NDK 28 build support.
- Added a native Android OpenGL ES 2 context, native guest shader execution and
  a cross-context RGBA presentation fallback.
- Added host-call and frame-time diagnostics for performance work.
- Added page-aligned `mmap`, partial `munmap`, correct Mono file permissions,
  legacy Objective-C protocol metadata and guest-first dynamic symbol lookup.
- Reduced allocation overhead and host memory clearing during Unity asset
  reloads while retaining zeroed HLE allocations on every host.
- Implemented stateful `UIAlertView` titles, messages, buttons, visibility,
  dismissal and delegate callbacks with host-native presentation.
- Made GLES buffer mapping tolerate temporarily missing EAGL contexts during
  Unity level transitions instead of panicking on an empty context.
- Added persistent current/previous crash reports with source locations and
  host backtraces so intermittent failures survive the next launch.
