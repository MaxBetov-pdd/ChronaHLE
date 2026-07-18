# Supported platforms

ChronaHLE uses one Rust emulation core on every host. Host integrations are not
identical: graphics context creation, audio devices, filesystem access, input,
JIT memory permissions and application packaging are platform-specific.

## Windows

Windows x86_64 is the primary development host. ARMv7 guest execution,
fastmem, desktop OpenGL compatibility rendering and AudioToolbox changes have
been exercised here with real applications.

## Linux

Linux x86_64 uses the same desktop code path and is a required CI build. A CI
success proves compilation and unit behavior, but GPU/driver runtime coverage
still depends on community testing across Mesa, proprietary drivers, X11 and
Wayland.

## Android

Android supports ARM64 hosts only. APK builds use NDK 28 and all native LOAD
segments are aligned for 16 KB memory-page devices.

The Android app picker and native OpenGL ES 1.1 host path are supported.
Applications that create an OpenGL ES 2 EAGL context receive a native EGL
GLES2 context. The compatibility presentation path has been exercised in real
gameplay on a Pixel 7a; its RGBA readback/upload path can still be slower than
direct GPU presentation. See [ANDROID_GLES.md](ANDROID_GLES.md).

## Guest architecture

Current guest execution is 32-bit ARMv6/ARMv7. A future ARM64 guest layer would
reuse many framework implementations, but requires a 64-bit Mach-O loader,
ABI/calling convention, Objective-C metadata, memory model and CPU frontend.
