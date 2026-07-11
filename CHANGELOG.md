# ChronaHLE changelog

All notable ChronaHLE-specific changes are recorded here. Historical touchHLE
release notes are preserved in [UPSTREAM_CHANGELOG.md](UPSTREAM_CHANGELOG.md).

## Unreleased

- Established the ChronaHLE product identity and independent Android package.
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
