# Android OpenGL ES status

## Implementation status

Desktop hosts emulate applications that request `EAGLRenderingAPIOpenGLES2`
with an OpenGL 2.1 compatibility-profile context. Android now creates a native
EGL OpenGL ES 2.0 context instead. Guest shader source is passed to the native
GLSL ES compiler without the desktop GLSL 1.20 translation.

Framebuffer, vertex-array and map-buffer entry points resolve from the desktop
EXT/ARB names used by the shared abstraction to their GLES2 core/OES names.
The NDK API 21 exports required by this mapping are checked during development.

## Presentation path

The first implementation favors correctness over speed. On
`presentRenderbuffer:` it reads the guest renderbuffer to RGBA memory, switches
to ChronaHLE's internal GLES1 context, and uses the shared presentation code for
rotation, scaling, letterboxing and the virtual cursor.

This avoids desktop fixed-pipeline calls in the guest GLES2 context and makes
the implementation functional on EGL. It is slower than the future optimized
path because every frame crosses GPU -> CPU -> GPU. A direct shader-based GPU
presentation path can replace it without changing the guest API.

## Remaining verification

1. Physical ARM64 device smoke tests for GLES1 and GLES2 guest applications.
2. Pause/resume and EGL context-loss tests.
3. Driver coverage across Adreno, Mali and PowerVR.
4. Replacement of the RGBA round trip with direct GPU presentation after the
   correctness path is verified.

Do not mark Android GLES2 as supported based only on a successful APK build.
The release checklist requires a real ARM64 Android device and a captured log
showing the selected EGL context and renderer.
