/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Wrapper functions exposing OpenGL ES to the guest.
//!
//! This code is intentionally somewhat lax with calculating array sizes when
//! obtainining a pointer with [Mem::ptr_at]. For large chunks of data, e.g. the
//! `pixels` parameter of `glTexImage2D`, it's worth being precise, but for
//! `glFoofv(pname, param)` where `param` is a pointer to one to four `GLfloat`s
//! depending on the value of `pname`, using the upper bound (4 in this case)
//! every time is never going to cause a problem in practice.

use touchHLE_gl_bindings::gles11::{
    ARRAY_BUFFER, ARRAY_BUFFER_BINDING, ELEMENT_ARRAY_BUFFER, ELEMENT_ARRAY_BUFFER_BINDING,
    WRITE_ONLY_OES,
};

use crate::dyld::{export_c_func, FunctionExports};
use crate::frameworks::opengles::eagl::EAGLContextHostObject;
use crate::gles::{gles11_raw as gles11, GLES}; // constants only
use crate::mem::{ConstPtr, ConstVoidPtr, GuestISize, GuestUSize, Mem, MutPtr, MutVoidPtr, Ptr};
use crate::objc::nil;
use crate::Environment;

use std::slice::from_raw_parts;
use std::sync::atomic::{AtomicU32, Ordering};

// These types are the same size in guest code (32-bit) and host code (64-bit).
use crate::gles::gles11_raw::types::{
    GLbitfield, GLboolean, GLchar, GLclampf, GLclampx, GLenum, GLfixed, GLfloat, GLint, GLsizei,
    GLubyte, GLuint, GLvoid,
};
// These types have different sizes, so some care is needed.
use crate::gles::gles11_raw::types::{GLintptr as HostGLintptr, GLsizeiptr as HostGLsizeiptr};
type GuestGLsizeiptr = GuestISize;
type GuestGLintptr = GuestISize;

/// List of compressed formats supported by our emulation.
/// Currently, it's all the PVRTC and all paletted ones.
const SUPPORTED_COMPRESSED_TEXTURE_FORMATS: &[GLenum] = &[
    // PVRTC
    gles11::COMPRESSED_RGBA_PVRTC_2BPPV1_IMG,
    gles11::COMPRESSED_RGBA_PVRTC_4BPPV1_IMG,
    gles11::COMPRESSED_RGB_PVRTC_2BPPV1_IMG,
    gles11::COMPRESSED_RGB_PVRTC_4BPPV1_IMG,
    // Paletted texture
    gles11::PALETTE4_R5_G6_B5_OES,
    gles11::PALETTE4_RGB5_A1_OES,
    gles11::PALETTE4_RGB8_OES,
    gles11::PALETTE4_RGBA4_OES,
    gles11::PALETTE4_RGBA8_OES,
    gles11::PALETTE8_R5_G6_B5_OES,
    gles11::PALETTE8_RGB5_A1_OES,
    gles11::PALETTE8_RGB8_OES,
    gles11::PALETTE8_RGBA4_OES,
    gles11::PALETTE8_RGBA8_OES,
];

static NEXT_QUERY_EXT_ID: AtomicU32 = AtomicU32::new(1);
static DRAW_TRACE_COUNT: AtomicU32 = AtomicU32::new(0);
const GL_CURRENT_PROGRAM: GLenum = 0x8B8D;

/// Sync the current context and performs a function `f` within it.
///
/// In case of missing EAGL context for a current thread,
/// returns a default value.
fn with_ctx_and_mem<T, U: Default>(env: &mut Environment, f: T) -> U
where
    T: FnOnce(&mut dyn GLES, &mut Mem) -> U,
{
    if env
        .framework_state
        .opengles
        .current_ctx_for_thread(env.current_thread)
        .is_none()
    {
        log!(
            "Warning: No EAGLContext for thread {}! Ignoring OpenGL ES call, returning default value.",
            env.current_thread
        );
        return U::default();
    }

    let mut gles = super::sync_context(
        &mut env.framework_state.opengles,
        &mut env.objc,
        env.window
            .as_mut()
            .expect("OpenGL ES is not supported in headless mode"),
        env.current_thread,
    );

    //panic_on_gl_errors(&mut *gles);
    let res = f(gles.as_mut(), &mut env.mem);
    //panic_on_gl_errors(&mut *gles);
    #[allow(clippy::let_and_return)]
    res
}

/// Version of with_ctx_and_mem which panics on a missing context.
///
/// Needed because for return types such as `*mut GLvoid` we cannnot
/// return a default value in case EAGL context is missing for
/// a current thread.
fn with_ctx_and_mem_no_skip<T, U>(env: &mut Environment, f: T) -> U
where
    T: FnOnce(&mut dyn GLES, &mut Mem) -> U,
{
    let mut gles = super::sync_context(
        &mut env.framework_state.opengles,
        &mut env.objc,
        env.window
            .as_mut()
            .expect("OpenGL ES is not supported in headless mode"),
        env.current_thread,
    );

    //panic_on_gl_errors(&mut **gles);
    let res = f(gles.as_mut(), &mut env.mem);
    //panic_on_gl_errors(&mut **gles);
    #[allow(clippy::let_and_return)]
    res
}

fn trace_draw_call(gles: &mut dyn GLES, kind: &str, mode: GLenum, count: GLsizei) {
    let Some(limit) = std::env::var("TOUCHHLE_TRACE_DRAWS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
    else {
        return;
    };
    let draw_index = DRAW_TRACE_COUNT.fetch_add(1, Ordering::Relaxed);
    if draw_index >= limit {
        return;
    }

    unsafe {
        let mut program = 0;
        let mut texture = 0;
        let mut array_buffer = 0;
        let mut element_array_buffer = 0;
        let mut active_texture = 0;
        gles.GetIntegerv(GL_CURRENT_PROGRAM, &mut program);
        gles.GetIntegerv(gles11::TEXTURE_BINDING_2D, &mut texture);
        gles.GetIntegerv(gles11::ARRAY_BUFFER_BINDING, &mut array_buffer);
        gles.GetIntegerv(
            gles11::ELEMENT_ARRAY_BUFFER_BINDING,
            &mut element_array_buffer,
        );
        gles.GetIntegerv(gles11::ACTIVE_TEXTURE, &mut active_texture);
        log!(
            "GL draw #{draw_index}: {kind}(mode={mode:#x}, count={count}) program={program} texture2d={texture} active_texture={active_texture:#x} array_buffer={array_buffer} element_array_buffer={element_array_buffer} blend={} depth_test={} scissor_test={} cull_face={}",
            gles.IsEnabled(gles11::BLEND) != 0,
            gles.IsEnabled(gles11::DEPTH_TEST) != 0,
            gles.IsEnabled(gles11::SCISSOR_TEST) != 0,
            gles.IsEnabled(gles11::CULL_FACE) != 0,
        );
    }
}

/// Useful for debugging
#[allow(dead_code)]
fn panic_on_gl_errors(gles: &mut dyn GLES) {
    let mut did_error = false;
    loop {
        let err = unsafe { gles.GetError() };
        if err == 0 {
            break;
        }
        did_error = true;
        echo!("glGetError() => {:#x}", err);
    }
    if did_error {
        panic!();
    }
}

// Generic state manipulation
fn glGetError(env: &mut Environment) -> GLenum {
    let ignore_gl_errors = env.options.ignore_gl_errors;
    with_ctx_and_mem(env, |gles, _mem| {
        let err = unsafe { gles.GetError() };
        if err != 0 {
            if ignore_gl_errors {
                log_once!(
                    "Warning: Guest error reporting is ignored for glGetError(), returning 0."
                );
                return 0;
            }
            log!("Warning: glGetError() returned {:#x}", err);
        }
        err
    })
}
fn glEnable(env: &mut Environment, cap: GLenum) {
    with_ctx_and_mem(env, |gles, _mem| {
        unsafe { gles.Enable(cap) };
    });
}
fn glIsEnabled(env: &mut Environment, cap: GLenum) -> GLboolean {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.IsEnabled(cap) })
}
fn glDisable(env: &mut Environment, cap: GLenum) {
    with_ctx_and_mem(env, |gles, _mem| {
        unsafe { gles.Disable(cap) };
    });
}
fn glClientActiveTexture(env: &mut Environment, texture: GLenum) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.ClientActiveTexture(texture)
    })
}
fn glEnableClientState(env: &mut Environment, array: GLenum) {
    with_ctx_and_mem(env, |gles, _mem| {
        unsafe { gles.EnableClientState(array) };
    });
}
fn glDisableClientState(env: &mut Environment, array: GLenum) {
    with_ctx_and_mem(env, |gles, _mem| {
        unsafe { gles.DisableClientState(array) };
    });
}
fn glGetBooleanv(env: &mut Environment, pname: GLenum, params: MutPtr<GLboolean>) {
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at_mut(params, 16 /* upper bound */);
        unsafe { gles.GetBooleanv(pname, params) };
    });
}
fn glGetFloatv(env: &mut Environment, pname: GLenum, params: MutPtr<GLfloat>) {
    assert_ne!(gles11::NUM_COMPRESSED_TEXTURE_FORMATS, pname);
    assert_ne!(gles11::COMPRESSED_TEXTURE_FORMATS, pname);
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at_mut(params, 16 /* upper bound */);
        unsafe { gles.GetFloatv(pname, params) };
    });
}
fn glGetIntegerv(env: &mut Environment, pname: GLenum, params: MutPtr<GLint>) {
    let scale_hack = if matches!(pname, gles11::VIEWPORT | gles11::SCISSOR_BOX)
        && framebuffer_should_scale(env)
    {
        env.options.scale_hack.get() as GLint
    } else {
        1
    };
    with_ctx_and_mem(env, |gles, mem| {
        match pname {
            gles11::NUM_COMPRESSED_TEXTURE_FORMATS => {
                mem.write(params, SUPPORTED_COMPRESSED_TEXTURE_FORMATS.len() as _);
            }
            gles11::COMPRESSED_TEXTURE_FORMATS => {
                for (idx, &format) in SUPPORTED_COMPRESSED_TEXTURE_FORMATS.iter().enumerate() {
                    mem.write(params + idx as GuestUSize, format as _);
                }
            }
            // MAX_COLOR_ATTACHMENTS_EXT or MAX_COLOR_ATTACHMENTS_OES
            0x8cdf => {
                // According to [OES_framebuffer_object](https://registry.khronos.org/OpenGL/extensions/OES/OES_framebuffer_object.txt),
                // MAX_COLOR_ATTACHMENTS_OES is not supported in the extension,
                // but we return 1 to match the real device.
                mem.write(params, 1 as _);
            }
            // MAX_SAMPLES or MAX_SAMPLES_ANGLE
            0x8d57 => {
                // TODO: handle GetBooleanv and GetFloatv as well
                // 1 is an initial value
                // TODO: This is an OpenGL ES 2.0 extension, not supported yet
                mem.write(params, 1 as _);
            }
            gles11::VIEWPORT | gles11::SCISSOR_BOX => {
                let params = mem.ptr_at_mut(params, 4);
                unsafe { gles.GetIntegerv(pname, params) };
                if scale_hack != 1 {
                    for idx in 0..4 {
                        unsafe {
                            let value = params.add(idx);
                            value.write_unaligned(value.read_unaligned() / scale_hack);
                        }
                    }
                }
            }
            _ => {
                let params = mem.ptr_at_mut(params, 16 /* upper bound */);
                unsafe { gles.GetIntegerv(pname, params) };
            }
        }
    });
}
fn glGetPointerv(env: &mut Environment, pname: GLenum, params: MutPtr<ConstVoidPtr>) {
    use crate::gles::gles1_on_gl2::{ArrayInfo, ARRAYS};
    let &ArrayInfo { buffer_binding, .. } =
        ARRAYS.iter().find(|info| info.pointer == pname).unwrap();
    with_ctx_and_mem(env, |gles, mem| {
        // params always points to just one pointer for this function
        let mut host_pointer_or_offset = std::ptr::null();
        let guest_pointer_or_offset = unsafe {
            gles.GetPointerv(pname, &mut host_pointer_or_offset);
            translate_pointer_or_offset_to_guest(gles, mem, host_pointer_or_offset, buffer_binding)
        };
        mem.write(params, guest_pointer_or_offset);
    });
}
fn glGetTexEnviv(env: &mut Environment, target: GLenum, pname: GLenum, params: MutPtr<GLint>) {
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at_mut(params, 16 /* upper bound */);
        unsafe { gles.GetTexEnviv(target, pname, params) };
    });
}
fn glGetTexEnvfv(env: &mut Environment, target: GLenum, pname: GLenum, params: MutPtr<GLfloat>) {
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at_mut(params, 16 /* upper bound */);
        unsafe { gles.GetTexEnvfv(target, pname, params) };
    });
}

fn glHint(env: &mut Environment, target: GLenum, mode: GLenum) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.Hint(target, mode) })
}
fn glFinish(env: &mut Environment) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.Finish() })
}
fn glFlush(env: &mut Environment) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.Flush() })
}
fn glGetString(env: &mut Environment, name: GLenum) -> ConstPtr<GLubyte> {
    let res = if let Some(&str) = env.framework_state.opengles.strings_cache.get(&name) {
        str
    } else {
        let new_str = with_ctx_and_mem(env, |_gles, mem| {
            // Those values are extracted from the iPod touch 2nd gen, iOS 4.2.1
            let s: &[u8] = match name {
                gles11::VENDOR => {
                    b"Imagination Technologies"
                }
                gles11::RENDERER => {
                    b"PowerVR MBXLite with VGPLite"
                }
                gles11::VERSION => {
                    b"OpenGL ES-CM 1.1 (76)"
                }
                gles11::EXTENSIONS => {
                    b"GL_APPLE_framebuffer_multisample GL_APPLE_texture_max_level GL_EXT_discard_framebuffer GL_EXT_texture_filter_anisotropic GL_EXT_texture_lod_bias GL_IMG_read_format GL_IMG_texture_compression_pvrtc GL_IMG_texture_format_BGRA8888 GL_OES_blend_subtract GL_OES_compressed_paletted_texture GL_OES_depth24 GL_OES_draw_texture GL_OES_framebuffer_object GL_OES_mapbuffer GL_OES_matrix_palette GL_OES_point_size_array GL_OES_point_sprite GL_OES_read_format GL_OES_rgb8_rgba8 GL_OES_texture_mirrored_repeat GL_OES_vertex_array_object "
                }
                _ => unreachable!(),
            };
            mem.alloc_and_write_cstr(s).cast_const()
        });
        env.framework_state
            .opengles
            .strings_cache
            .insert(name, new_str);
        new_str
    };
    log_dbg!("glGetString({}) => {:?}", name, res);
    res
}

// Other state manipulation
fn glAlphaFunc(env: &mut Environment, func: GLenum, ref_: GLclampf) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.AlphaFunc(func, ref_) })
}
fn glAlphaFuncx(env: &mut Environment, func: GLenum, ref_: GLclampx) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.AlphaFuncx(func, ref_) })
}
fn glBlendFunc(env: &mut Environment, sfactor: GLenum, dfactor: GLenum) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.BlendFunc(sfactor, dfactor)
    })
}
fn glBlendFuncSeparate(
    env: &mut Environment,
    sfactor_rgb: GLenum,
    dfactor_rgb: GLenum,
    sfactor_alpha: GLenum,
    dfactor_alpha: GLenum,
) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.BlendFuncSeparate(sfactor_rgb, dfactor_rgb, sfactor_alpha, dfactor_alpha)
    })
}
fn glBlendEquationOES(env: &mut Environment, mode: GLenum) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.BlendEquationOES(mode) })
}
fn glBlendEquation(env: &mut Environment, mode: GLenum) {
    glBlendEquationOES(env, mode)
}
fn glBlendEquationSeparate(env: &mut Environment, mode_rgb: GLenum, mode_alpha: GLenum) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.BlendEquationSeparate(mode_rgb, mode_alpha)
    })
}
fn glColorMask(
    env: &mut Environment,
    red: GLboolean,
    green: GLboolean,
    blue: GLboolean,
    alpha: GLboolean,
) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.ColorMask(red, green, blue, alpha)
    })
}
fn glClipPlanef(env: &mut Environment, plane: GLenum, equation: ConstPtr<GLfloat>) {
    with_ctx_and_mem(env, |gles, mem| {
        let equation = mem.ptr_at(equation, 4 /* upper bound */);
        unsafe { gles.ClipPlanef(plane, equation) }
    })
}
fn glClipPlanex(env: &mut Environment, plane: GLenum, equation: ConstPtr<GLfixed>) {
    with_ctx_and_mem(env, |gles, mem| {
        let equation = mem.ptr_at(equation, 4 /* upper bound */);
        unsafe { gles.ClipPlanex(plane, equation) }
    })
}
fn glCullFace(env: &mut Environment, mode: GLenum) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.CullFace(mode) })
}
fn glDepthFunc(env: &mut Environment, func: GLenum) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.DepthFunc(func) })
}
fn glDepthMask(env: &mut Environment, flag: GLboolean) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.DepthMask(flag) })
}
fn glDepthRangef(env: &mut Environment, near: GLclampf, far: GLclampf) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.DepthRangef(near, far) })
}
fn glDepthRangex(env: &mut Environment, near: GLclampx, far: GLclampx) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.DepthRangex(near, far) })
}
fn glFrontFace(env: &mut Environment, mode: GLenum) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.FrontFace(mode) })
}
fn glPolygonOffset(env: &mut Environment, factor: GLfloat, units: GLfloat) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.PolygonOffset(factor, units)
    })
}
fn glPolygonOffsetx(env: &mut Environment, factor: GLfixed, units: GLfixed) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.PolygonOffsetx(factor, units)
    })
}
fn glSampleCoverage(env: &mut Environment, value: GLclampf, invert: GLboolean) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.SampleCoverage(value, invert)
    })
}
fn glSampleCoveragex(env: &mut Environment, value: GLclampx, invert: GLboolean) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.SampleCoveragex(value, invert)
    })
}
fn glShadeModel(env: &mut Environment, mode: GLenum) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.ShadeModel(mode) })
}
fn glScissor(env: &mut Environment, x: GLint, y: GLint, width: GLsizei, height: GLsizei) {
    // apply scale hack: assume framebuffer's size is larger than the app thinks
    // and scale scissor appropriately
    let factor = if framebuffer_should_scale(env) {
        env.options.scale_hack.get() as GLsizei
    } else {
        1
    };
    let (x, y) = (x * factor, y * factor);
    let (width, height) = (width * factor, height * factor);
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.Scissor(x, y, width, height)
    })
}
fn glViewport(env: &mut Environment, x: GLint, y: GLint, width: GLsizei, height: GLsizei) {
    // apply scale hack: assume framebuffer's size is larger than the app thinks
    // and scale viewport appropriately
    let factor = if framebuffer_should_scale(env) {
        env.options.scale_hack.get() as GLsizei
    } else {
        1
    };
    let (x, y) = (x * factor, y * factor);
    let (width, height) = (width * factor, height * factor);
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.Viewport(x, y, width, height)
    })
}
fn glLineWidth(env: &mut Environment, val: GLfloat) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.LineWidth(val) })
}
fn glLineWidthx(env: &mut Environment, val: GLfixed) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.LineWidthx(val) })
}
fn glStencilFunc(env: &mut Environment, func: GLenum, ref_: GLint, mask: GLuint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.StencilFunc(func, ref_, mask)
    });
}
fn glStencilOp(env: &mut Environment, sfail: GLenum, dpfail: GLenum, dppass: GLenum) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.StencilOp(sfail, dpfail, dppass)
    });
}
fn glStencilMask(env: &mut Environment, mask: GLuint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.StencilMask(mask) });
}
fn glLogicOp(env: &mut Environment, opcode: GLenum) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.LogicOp(opcode) });
}
// Points
fn glPointSize(env: &mut Environment, size: GLfloat) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.PointSize(size) })
}
fn glPointSizex(env: &mut Environment, size: GLfixed) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.PointSizex(size) })
}
fn glPointParameterf(env: &mut Environment, pname: GLenum, param: GLfloat) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.PointParameterf(pname, param)
    })
}
fn glPointParameterx(env: &mut Environment, pname: GLenum, param: GLfixed) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.PointParameterx(pname, param)
    })
}
fn glPointParameterfv(env: &mut Environment, pname: GLenum, params: ConstPtr<GLfloat>) {
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at(params, 4 /* upper bound */);
        unsafe { gles.PointParameterfv(pname, params) }
    })
}
fn glPointParameterxv(env: &mut Environment, pname: GLenum, params: ConstPtr<GLfixed>) {
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at(params, 4 /* upper bound */);
        unsafe { gles.PointParameterxv(pname, params) }
    })
}

// Lighting and materials
fn glFogf(env: &mut Environment, pname: GLenum, param: GLfloat) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.Fogf(pname, param) })
}
fn glFogx(env: &mut Environment, pname: GLenum, param: GLfixed) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.Fogx(pname, param) })
}
fn glFogfv(env: &mut Environment, pname: GLenum, params: ConstPtr<GLfloat>) {
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at(params, 4 /* upper bound */);
        unsafe { gles.Fogfv(pname, params) }
    })
}
fn glFogxv(env: &mut Environment, pname: GLenum, params: ConstPtr<GLfixed>) {
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at(params, 4 /* upper bound */);
        unsafe { gles.Fogxv(pname, params) }
    })
}
fn glLightf(env: &mut Environment, light: GLenum, pname: GLenum, param: GLfloat) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.Lightf(light, pname, param)
    })
}
fn glLightx(env: &mut Environment, light: GLenum, pname: GLenum, param: GLfixed) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.Lightx(light, pname, param)
    })
}
fn glLightfv(env: &mut Environment, light: GLenum, pname: GLenum, params: ConstPtr<GLfloat>) {
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at(params, 4 /* upper bound */);
        unsafe { gles.Lightfv(light, pname, params) }
    })
}
fn glLightxv(env: &mut Environment, light: GLenum, pname: GLenum, params: ConstPtr<GLfixed>) {
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at(params, 4 /* upper bound */);
        unsafe { gles.Lightxv(light, pname, params) }
    })
}
fn glLightModelf(env: &mut Environment, pname: GLenum, param: GLfloat) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.LightModelf(pname, param) })
}
fn glLightModelx(env: &mut Environment, pname: GLenum, param: GLfixed) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.LightModelx(pname, param) })
}
fn glLightModelfv(env: &mut Environment, pname: GLenum, params: ConstPtr<GLfloat>) {
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at(params, 4 /* upper bound */);
        unsafe { gles.LightModelfv(pname, params) }
    })
}
fn glLightModelxv(env: &mut Environment, pname: GLenum, params: ConstPtr<GLfixed>) {
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at(params, 4 /* upper bound */);
        unsafe { gles.LightModelxv(pname, params) }
    })
}
fn glMaterialf(env: &mut Environment, face: GLenum, pname: GLenum, param: GLfloat) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.Materialf(face, pname, param)
    })
}
fn glMaterialx(env: &mut Environment, face: GLenum, pname: GLenum, param: GLfixed) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.Materialx(face, pname, param)
    })
}
fn glMaterialfv(env: &mut Environment, face: GLenum, pname: GLenum, params: ConstPtr<GLfloat>) {
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at(params, 4 /* upper bound */);
        unsafe { gles.Materialfv(face, pname, params) }
    })
}
fn glMaterialxv(env: &mut Environment, face: GLenum, pname: GLenum, params: ConstPtr<GLfixed>) {
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at(params, 4 /* upper bound */);
        unsafe { gles.Materialxv(face, pname, params) }
    })
}

// Buffers
fn glIsBuffer(env: &mut Environment, buffer: GLuint) -> GLboolean {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.IsBuffer(buffer) })
}
fn glGenBuffers(env: &mut Environment, n: GLsizei, buffers: MutPtr<GLuint>) {
    with_ctx_and_mem(env, |gles, mem| {
        let n_usize: GuestUSize = n.try_into().unwrap();
        let buffers = mem.ptr_at_mut(buffers, n_usize);
        unsafe { gles.GenBuffers(n, buffers) }
    })
}
fn glDeleteBuffers(env: &mut Environment, n: GLsizei, buffers: ConstPtr<GLuint>) {
    with_ctx_and_mem(env, |gles, mem| {
        let n_usize: GuestUSize = n.try_into().unwrap();
        let buffers = mem.ptr_at(buffers, n_usize);
        unsafe { gles.DeleteBuffers(n, buffers) }
    })
}
fn glBindBuffer(env: &mut Environment, target: GLenum, buffer: GLuint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.BindBuffer(target, buffer) })
}
fn glBufferData(
    env: &mut Environment,
    target: GLenum,
    size: GuestGLsizeiptr,
    data: ConstPtr<GLvoid>,
    usage: GLenum,
) {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let data = if data.is_null() {
            std::ptr::null()
        } else {
            mem.ptr_at(data.cast::<u8>(), size.try_into().unwrap())
                .cast()
        };
        gles.BufferData(target, size as HostGLsizeiptr, data, usage)
    })
}

fn glBufferSubData(
    env: &mut Environment,
    target: GLenum,
    offset: GuestGLintptr,
    size: GuestGLsizeiptr,
    data: ConstPtr<GLvoid>,
) {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let data = if data.is_null() {
            std::ptr::null()
        } else {
            mem.ptr_at(data.cast::<u8>(), size.try_into().unwrap())
                .cast()
        };
        gles.BufferSubData(target, offset as HostGLintptr, size as HostGLsizeiptr, data)
    })
}

// OpenGL ES 2.0 shader/program entry points
fn glCreateShader(env: &mut Environment, type_: GLenum) -> GLuint {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.CreateShader(type_) })
}

fn remove_glsl_precision_qualifiers(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut idx = 0;

    while idx < bytes.len() {
        let rest = &source[idx..];
        let precision = ["highp", "mediump", "lowp"].iter().find(|qualifier| {
            rest.starts_with(**qualifier)
                && (idx == 0 || !bytes[idx - 1].is_ascii_alphanumeric() && bytes[idx - 1] != b'_')
                && bytes
                    .get(idx + qualifier.len())
                    .is_none_or(|next| !next.is_ascii_alphanumeric() && *next != b'_')
        });

        if let Some(precision) = precision {
            idx += precision.len();
        } else {
            let ch = rest.chars().next().unwrap();
            out.push(ch);
            idx += ch.len_utf8();
        }
    }

    out
}

fn translate_gles2_shader_source_to_desktop(source: &str) -> String {
    let mut translated = String::with_capacity(source.len());

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("precision ") && trimmed.ends_with(';') {
            continue;
        }
        if trimmed.starts_with("#version 100") {
            translated.push_str("#version 120\n");
            continue;
        }
        translated.push_str(line);
        translated.push('\n');
    }

    remove_glsl_precision_qualifiers(&translated)
}

fn glShaderSource(
    env: &mut Environment,
    shader: GLuint,
    count: GLsizei,
    string: ConstPtr<ConstPtr<GLchar>>,
    length: ConstPtr<GLint>,
) {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let count_usize: GuestUSize = count.try_into().unwrap();
        let mut source = Vec::new();

        for idx in 0..count_usize {
            let guest_string: ConstPtr<GLchar> = mem.read(string + idx);
            let maybe_len = if length.is_null() {
                None
            } else {
                Some(mem.read(length + idx))
            };

            if !guest_string.is_null() {
                let byte_count = match maybe_len {
                    Some(len) if len >= 0 => len.try_into().unwrap(),
                    _ => mem.cstr_at(guest_string.cast::<u8>()).len() as GuestUSize,
                };
                source.extend_from_slice(mem.bytes_at(guest_string.cast::<u8>(), byte_count));
            }
        }

        let source = String::from_utf8_lossy(&source);
        let host_source = if gles.is_native_gles2() {
            source.into_owned()
        } else {
            translate_gles2_shader_source_to_desktop(&source)
        };
        let translated_bytes: Vec<GLchar> = host_source
            .as_bytes()
            .iter()
            .map(|byte| *byte as GLchar)
            .collect();
        let host_strings = [translated_bytes.as_ptr()];
        let host_lengths = [translated_bytes.len().try_into().unwrap()];
        let host_lengths_ptr = host_lengths.as_ptr();
        gles.ShaderSource(shader, 1, host_strings.as_ptr(), host_lengths_ptr)
    })
}
fn glCompileShader(env: &mut Environment, shader: GLuint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.CompileShader(shader) })
}
fn glGetShaderiv(env: &mut Environment, shader: GLuint, pname: GLenum, params: MutPtr<GLint>) {
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at_mut(params, 1);
        unsafe { gles.GetShaderiv(shader, pname, params) }
    })
}
fn glGetShaderInfoLog(
    env: &mut Environment,
    shader: GLuint,
    buf_size: GLsizei,
    length: MutPtr<GLsizei>,
    info_log: MutPtr<GLchar>,
) {
    with_ctx_and_mem(env, |gles, mem| {
        let length = if length.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(length, 1)
        };
        let info_log = if info_log.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(info_log, buf_size.try_into().unwrap())
        };
        unsafe { gles.GetShaderInfoLog(shader, buf_size, length, info_log) }
    })
}
fn glGetShaderSource(
    env: &mut Environment,
    shader: GLuint,
    buf_size: GLsizei,
    length: MutPtr<GLsizei>,
    source: MutPtr<GLchar>,
) {
    with_ctx_and_mem(env, |gles, mem| {
        let length = if length.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(length, 1)
        };
        let source = if source.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(source, buf_size.try_into().unwrap())
        };
        unsafe { gles.GetShaderSource(shader, buf_size, length, source) }
    })
}
fn glDeleteShader(env: &mut Environment, shader: GLuint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.DeleteShader(shader) })
}
fn glCreateProgram(env: &mut Environment) -> GLuint {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.CreateProgram() })
}
fn glAttachShader(env: &mut Environment, program: GLuint, shader: GLuint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.AttachShader(program, shader)
    })
}
fn glDetachShader(env: &mut Environment, program: GLuint, shader: GLuint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.DetachShader(program, shader)
    })
}
fn glBindAttribLocation(
    env: &mut Environment,
    program: GLuint,
    index: GLuint,
    name: ConstPtr<GLchar>,
) {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let len = mem.cstr_at(name.cast::<u8>()).len() as GuestUSize + 1;
        let name = mem.ptr_at(name, len);
        gles.BindAttribLocation(program, index, name)
    })
}
fn glGetAttribLocation(env: &mut Environment, program: GLuint, name: ConstPtr<GLchar>) -> GLint {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let len = mem.cstr_at(name.cast::<u8>()).len() as GuestUSize + 1;
        let name = mem.ptr_at(name, len);
        gles.GetAttribLocation(program, name)
    })
}
fn glLinkProgram(env: &mut Environment, program: GLuint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.LinkProgram(program) })
}
fn glValidateProgram(env: &mut Environment, program: GLuint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.ValidateProgram(program) })
}
fn glGetProgramiv(env: &mut Environment, program: GLuint, pname: GLenum, params: MutPtr<GLint>) {
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at_mut(params, 1);
        unsafe { gles.GetProgramiv(program, pname, params) }
    })
}
fn glGetProgramInfoLog(
    env: &mut Environment,
    program: GLuint,
    buf_size: GLsizei,
    length: MutPtr<GLsizei>,
    info_log: MutPtr<GLchar>,
) {
    with_ctx_and_mem(env, |gles, mem| {
        let length = if length.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(length, 1)
        };
        let info_log = if info_log.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(info_log, buf_size.try_into().unwrap())
        };
        unsafe { gles.GetProgramInfoLog(program, buf_size, length, info_log) }
    })
}
fn glGetActiveUniform(
    env: &mut Environment,
    program: GLuint,
    index: GLuint,
    buf_size: GLsizei,
    length: MutPtr<GLsizei>,
    size: MutPtr<GLint>,
    type_: MutPtr<GLenum>,
    name: MutPtr<GLchar>,
) {
    with_ctx_and_mem(env, |gles, mem| {
        let length = if length.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(length, 1)
        };
        let size = if size.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(size, 1)
        };
        let type_ = if type_.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(type_, 1)
        };
        let name = if name.is_null() {
            std::ptr::null_mut()
        } else {
            mem.ptr_at_mut(name, buf_size.try_into().unwrap())
        };
        unsafe { gles.GetActiveUniform(program, index, buf_size, length, size, type_, name) }
    })
}
fn glUseProgram(env: &mut Environment, program: GLuint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.UseProgram(program) })
}
fn glDeleteProgram(env: &mut Environment, program: GLuint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.DeleteProgram(program) })
}
fn glGetUniformLocation(env: &mut Environment, program: GLuint, name: ConstPtr<GLchar>) -> GLint {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let len = mem.cstr_at(name.cast::<u8>()).len() as GuestUSize + 1;
        let name = mem.ptr_at(name, len);
        gles.GetUniformLocation(program, name)
    })
}
fn glUniform1f(env: &mut Environment, location: GLint, v0: GLfloat) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.Uniform1f(location, v0) })
}
fn glUniform1i(env: &mut Environment, location: GLint, v0: GLint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.Uniform1i(location, v0) })
}
fn glUniform1fv(env: &mut Environment, location: GLint, count: GLsizei, value: ConstPtr<GLfloat>) {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let value = mem.ptr_at(value, count.try_into().unwrap());
        gles.Uniform1fv(location, count, value)
    })
}
fn glUniform2fv(env: &mut Environment, location: GLint, count: GLsizei, value: ConstPtr<GLfloat>) {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let value = mem.ptr_at(value, (count * 2).try_into().unwrap());
        gles.Uniform2fv(location, count, value)
    })
}
fn glUniform3fv(env: &mut Environment, location: GLint, count: GLsizei, value: ConstPtr<GLfloat>) {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let value = mem.ptr_at(value, (count * 3).try_into().unwrap());
        gles.Uniform3fv(location, count, value)
    })
}
fn glUniform4fv(env: &mut Environment, location: GLint, count: GLsizei, value: ConstPtr<GLfloat>) {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let value = mem.ptr_at(value, (count * 4).try_into().unwrap());
        gles.Uniform4fv(location, count, value)
    })
}
fn glUniform4f(
    env: &mut Environment,
    location: GLint,
    v0: GLfloat,
    v1: GLfloat,
    v2: GLfloat,
    v3: GLfloat,
) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.Uniform4f(location, v0, v1, v2, v3)
    })
}
fn glUniformMatrix3fv(
    env: &mut Environment,
    location: GLint,
    count: GLsizei,
    transpose: GLboolean,
    value: ConstPtr<GLfloat>,
) {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let value = mem.ptr_at(value, (count * 9).try_into().unwrap());
        gles.UniformMatrix3fv(location, count, transpose, value)
    })
}
fn glUniformMatrix4fv(
    env: &mut Environment,
    location: GLint,
    count: GLsizei,
    transpose: GLboolean,
    value: ConstPtr<GLfloat>,
) {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let value = mem.ptr_at(value, (count * 16).try_into().unwrap());
        gles.UniformMatrix4fv(location, count, transpose, value)
    })
}
fn glVertexAttribPointer(
    env: &mut Environment,
    index: GLuint,
    size: GLint,
    type_: GLenum,
    normalized: GLboolean,
    stride: GLsizei,
    pointer: ConstVoidPtr,
) {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let pointer =
            translate_pointer_or_offset_to_host(gles, mem, pointer, gles11::ARRAY_BUFFER_BINDING);
        gles.VertexAttribPointer(index, size, type_, normalized, stride, pointer)
    })
}
fn glEnableVertexAttribArray(env: &mut Environment, index: GLuint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.EnableVertexAttribArray(index)
    })
}
fn glDisableVertexAttribArray(env: &mut Environment, index: GLuint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.DisableVertexAttribArray(index)
    })
}
fn glInsertEventMarkerEXT(_env: &mut Environment, _length: GLsizei, _marker: ConstPtr<GLchar>) {}
fn glPushGroupMarkerEXT(_env: &mut Environment, _length: GLsizei, _marker: ConstPtr<GLchar>) {}
fn glPopGroupMarkerEXT(_env: &mut Environment) {}
fn glDiscardFramebufferEXT(
    _env: &mut Environment,
    _target: GLenum,
    _num_attachments: GLsizei,
    _attachments: ConstPtr<GLenum>,
) {
}
fn glGenQueriesEXT(env: &mut Environment, n: GLsizei, ids: MutPtr<GLuint>) {
    let n_usize: GuestUSize = n.try_into().unwrap();
    for idx in 0..n_usize {
        let id = NEXT_QUERY_EXT_ID.fetch_add(1, Ordering::Relaxed);
        env.mem.write(ids + idx, id);
    }
}
fn glDeleteQueriesEXT(_env: &mut Environment, _n: GLsizei, _ids: ConstPtr<GLuint>) {}
fn glIsQueryEXT(_env: &mut Environment, id: GLuint) -> GLboolean {
    (id != 0) as GLboolean
}
fn glBeginQueryEXT(_env: &mut Environment, _target: GLenum, _id: GLuint) {}
fn glEndQueryEXT(_env: &mut Environment, _target: GLenum) {}
fn glGetQueryivEXT(env: &mut Environment, _target: GLenum, pname: GLenum, params: MutPtr<GLint>) {
    // GL_CURRENT_QUERY_EXT = 0x8865, GL_QUERY_COUNTER_BITS_EXT = 0x8864.
    let value = match pname {
        0x8865 => 0,
        0x8864 => 32,
        _ => 0,
    };
    env.mem.write(params, value);
}
fn glGetQueryObjectivEXT(env: &mut Environment, _id: GLuint, pname: GLenum, params: MutPtr<GLint>) {
    // GL_QUERY_RESULT_EXT = 0x8866, GL_QUERY_RESULT_AVAILABLE_EXT = 0x8867.
    let value = match pname {
        0x8866 | 0x8867 => 1,
        _ => 0,
    };
    env.mem.write(params, value);
}
fn glGetQueryObjectuivEXT(
    env: &mut Environment,
    _id: GLuint,
    pname: GLenum,
    params: MutPtr<GLuint>,
) {
    // GL_QUERY_RESULT_EXT = 0x8866, GL_QUERY_RESULT_AVAILABLE_EXT = 0x8867.
    let value = match pname {
        0x8866 | 0x8867 => 1,
        _ => 0,
    };
    env.mem.write(params, value);
}
fn glGetProgramBinaryOES(
    env: &mut Environment,
    _program: GLuint,
    _buf_size: GLsizei,
    length: MutPtr<GLsizei>,
    binary_format: MutPtr<GLenum>,
    _binary: MutPtr<GLvoid>,
) {
    if !length.is_null() {
        env.mem.write(length, 0);
    }
    if !binary_format.is_null() {
        env.mem.write(binary_format, 0);
    }
}
fn glProgramBinaryOES(
    _env: &mut Environment,
    _program: GLuint,
    _binary_format: GLenum,
    _binary: ConstPtr<GLvoid>,
    _length: GLsizei,
) {
}
fn glRenderbufferStorageMultisampleAPPLE(
    env: &mut Environment,
    target: GLenum,
    _samples: GLsizei,
    internalformat: GLenum,
    width: GLsizei,
    height: GLsizei,
) {
    let factor = env.options.scale_hack.get() as GLsizei;
    let renderbuffer = current_renderbuffer_name(env);
    env.framework_state
        .opengles
        .scaled_renderbuffers
        .insert(renderbuffer, factor != 1);
    let (width, height) = (width * factor, height * factor);
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.RenderbufferStorageOES(target, internalformat, width, height)
    })
}
fn glResolveMultisampleFramebufferAPPLE(_env: &mut Environment) {}
fn glRenderbufferStorageMultisampleIMG(
    env: &mut Environment,
    target: GLenum,
    samples: GLsizei,
    internalformat: GLenum,
    width: GLsizei,
    height: GLsizei,
) {
    glRenderbufferStorageMultisampleAPPLE(env, target, samples, internalformat, width, height)
}
fn glFramebufferTexture2DMultisampleIMG(
    env: &mut Environment,
    target: GLenum,
    attachment: GLenum,
    textarget: GLenum,
    texture: GLuint,
    level: GLint,
    _samples: GLsizei,
) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.FramebufferTexture2DOES(target, attachment, textarget, texture, level)
    })
}
fn glDrawBuffersNV(_env: &mut Environment, _n: GLsizei, _bufs: ConstPtr<GLenum>) {}
fn glQueryCounterNV(_env: &mut Environment, _id: GLuint, _target: GLenum) {}
fn glGetQueryObjecti64vNV(env: &mut Environment, _id: GLuint, pname: GLenum, params: MutPtr<i64>) {
    // GL_QUERY_RESULT_EXT = 0x8866, GL_QUERY_RESULT_AVAILABLE_EXT = 0x8867.
    let value = match pname {
        0x8866 | 0x8867 => 1,
        _ => 0,
    };
    env.mem.write(params, value);
}
fn glGetQueryObjectui64vNV(env: &mut Environment, _id: GLuint, pname: GLenum, params: MutPtr<u64>) {
    // GL_QUERY_RESULT_EXT = 0x8866, GL_QUERY_RESULT_AVAILABLE_EXT = 0x8867.
    let value = match pname {
        0x8866 | 0x8867 => 1,
        _ => 0,
    };
    env.mem.write(params, value);
}
fn glAlphaFuncQCOM(env: &mut Environment, func: GLenum, ref_: GLclampf) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.AlphaFunc(func, ref_) })
}

// Non-pointers
fn glColor4f(env: &mut Environment, red: GLfloat, green: GLfloat, blue: GLfloat, alpha: GLfloat) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.Color4f(red, green, blue, alpha)
    })
}
fn glColor4x(env: &mut Environment, red: GLfixed, green: GLfixed, blue: GLfixed, alpha: GLfixed) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.Color4x(red, green, blue, alpha)
    })
}
fn glColor4ub(env: &mut Environment, red: GLubyte, green: GLubyte, blue: GLubyte, alpha: GLubyte) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.Color4ub(red, green, blue, alpha)
    })
}
fn glNormal3f(env: &mut Environment, nx: GLfloat, ny: GLfloat, nz: GLfloat) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.Normal3f(nx, ny, nz) })
}
fn glNormal3x(env: &mut Environment, nx: GLfixed, ny: GLfixed, nz: GLfixed) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.Normal3x(nx, ny, nz) })
}

// Pointers

/// Helper for implementing OpenGL pointer setting functions.
///
/// One of the ugliest things in OpenGL is that, depending on dynamic state
/// (`ARRAY_BUFFER_BINDING` or `ELEMENT_ARRAY_BUFFER_BINDING`), the pointer
/// parameter of certain functions is either a pointer or an offset!
///
/// See also: [translate_pointer_or_offset_to_guest]
unsafe fn translate_pointer_or_offset_to_host(
    gles: &mut dyn GLES,
    mem: &Mem,
    pointer_or_offset: ConstVoidPtr,
    which_binding: GLenum,
) -> *const GLvoid {
    let mut buffer_binding = 0;
    gles.GetIntegerv(which_binding, &mut buffer_binding);
    if buffer_binding != 0 {
        let offset = pointer_or_offset.to_bits();
        offset as usize as *const _
    } else if pointer_or_offset.is_null() {
        std::ptr::null()
    } else {
        let pointer = pointer_or_offset;
        // We need to use an unchecked version of ptr_at to avoid crashing here
        // if dynamic state was disabled.
        // Also, bounds checking is hopeless here
        mem.unchecked_ptr_at(pointer.cast::<u8>(), 0)
            .cast::<GLvoid>()
    }
}

/// Helper for implementing OpenGL pointer retrieval.
///
/// Reverse of [translate_pointer_or_offset_to_host]. Depending on the value
/// of `VERTEX_ARRAY_BUFFER_BINDING`/`NORMAL_ARRAY_BUFFER_BINDING`/etc
/// (not to be confused with `ARRAY_BUFFER_BINDING`, only used when *setting*),
/// the pointer retrieved with `glGetPointerv` may actually be an offset.
///
/// See also: [translate_pointer_or_offset_to_host]
unsafe fn translate_pointer_or_offset_to_guest(
    gles: &mut dyn GLES,
    mem: &Mem,
    pointer_or_offset: *const GLvoid,
    which_binding: GLenum,
) -> ConstVoidPtr {
    let mut buffer_binding = 0;
    gles.GetIntegerv(which_binding, &mut buffer_binding);
    if buffer_binding != 0 {
        let offset = pointer_or_offset as usize;
        Ptr::from_bits(u32::try_from(offset).unwrap())
    } else if pointer_or_offset.is_null() {
        Ptr::null()
    } else {
        let pointer = pointer_or_offset;
        mem.host_ptr_to_guest_ptr(pointer)
    }
}

fn glColorPointer(
    env: &mut Environment,
    size: GLint,
    type_: GLenum,
    stride: GLsizei,
    pointer: ConstVoidPtr,
) {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let pointer =
            translate_pointer_or_offset_to_host(gles, mem, pointer, gles11::ARRAY_BUFFER_BINDING);
        gles.ColorPointer(size, type_, stride, pointer)
    })
}
fn glNormalPointer(env: &mut Environment, type_: GLenum, stride: GLsizei, pointer: ConstVoidPtr) {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let pointer =
            translate_pointer_or_offset_to_host(gles, mem, pointer, gles11::ARRAY_BUFFER_BINDING);
        gles.NormalPointer(type_, stride, pointer)
    })
}
fn glTexCoordPointer(
    env: &mut Environment,
    size: GLint,
    type_: GLenum,
    stride: GLsizei,
    pointer: ConstVoidPtr,
) {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let pointer =
            translate_pointer_or_offset_to_host(gles, mem, pointer, gles11::ARRAY_BUFFER_BINDING);
        gles.TexCoordPointer(size, type_, stride, pointer)
    })
}
fn glVertexPointer(
    env: &mut Environment,
    size: GLint,
    type_: GLenum,
    stride: GLsizei,
    pointer: ConstVoidPtr,
) {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let pointer =
            translate_pointer_or_offset_to_host(gles, mem, pointer, gles11::ARRAY_BUFFER_BINDING);
        gles.VertexPointer(size, type_, stride, pointer)
    })
}

// Drawing
fn glDrawArrays(env: &mut Environment, mode: GLenum, first: GLint, count: GLsizei) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        trace_draw_call(gles, "glDrawArrays", mode, count);
        let fog_state_backup = clamp_fog_state_values(gles);
        gles.DrawArrays(mode, first, count);
        restore_fog_state_values(gles, fog_state_backup);
    })
}
fn glDrawElements(
    env: &mut Environment,
    mode: GLenum,
    count: GLsizei,
    type_: GLenum,
    indices: ConstVoidPtr,
) {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        trace_draw_call(gles, "glDrawElements", mode, count);
        let fog_state_backup = clamp_fog_state_values(gles);
        let indices = translate_pointer_or_offset_to_host(
            gles,
            mem,
            indices,
            gles11::ELEMENT_ARRAY_BUFFER_BINDING,
        );
        gles.DrawElements(mode, count, type_, indices);
        restore_fog_state_values(gles, fog_state_backup);
    })
}

// Clearing
fn glClear(env: &mut Environment, mask: GLbitfield) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.Clear(mask) });
}
fn glClearColor(
    env: &mut Environment,
    red: GLclampf,
    green: GLclampf,
    blue: GLclampf,
    alpha: GLclampf,
) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.ClearColor(red, green, blue, alpha)
    });
}
fn glClearColorx(
    env: &mut Environment,
    red: GLclampx,
    green: GLclampx,
    blue: GLclampx,
    alpha: GLclampx,
) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.ClearColorx(red, green, blue, alpha)
    });
}
fn glClearDepthf(env: &mut Environment, depth: GLclampf) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.ClearDepthf(depth) });
}
fn glClearDepthx(env: &mut Environment, depth: GLclampx) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.ClearDepthx(depth) });
}
fn glClearStencil(env: &mut Environment, s: GLint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.ClearStencil(s) });
}

// Matrix stack operations
fn glMatrixMode(env: &mut Environment, mode: GLenum) {
    with_ctx_and_mem(env, |gles, _mem| {
        unsafe { gles.MatrixMode(mode) };
    });
}
fn glLoadIdentity(env: &mut Environment) {
    with_ctx_and_mem(env, |gles, _mem| {
        unsafe { gles.LoadIdentity() };
    });
}
fn glLoadMatrixf(env: &mut Environment, m: ConstPtr<GLfloat>) {
    with_ctx_and_mem(env, |gles, mem| {
        let m = mem.ptr_at(m, 16);
        unsafe { gles.LoadMatrixf(m) };
    });
}
fn glLoadMatrixx(env: &mut Environment, m: ConstPtr<GLfixed>) {
    with_ctx_and_mem(env, |gles, mem| {
        let m = mem.ptr_at(m, 16);
        unsafe { gles.LoadMatrixx(m) };
    });
}
fn glMultMatrixf(env: &mut Environment, m: ConstPtr<GLfloat>) {
    with_ctx_and_mem(env, |gles, mem| {
        let m = mem.ptr_at(m, 16);
        unsafe { gles.MultMatrixf(m) };
    });
}
fn glMultMatrixx(env: &mut Environment, m: ConstPtr<GLfixed>) {
    with_ctx_and_mem(env, |gles, mem| {
        let m = mem.ptr_at(m, 16);
        unsafe { gles.MultMatrixx(m) };
    });
}
fn glPushMatrix(env: &mut Environment) {
    with_ctx_and_mem(env, |gles, _mem| {
        unsafe { gles.PushMatrix() };
    });
}
fn glPopMatrix(env: &mut Environment) {
    with_ctx_and_mem(env, |gles, _mem| {
        unsafe { gles.PopMatrix() };
    });
}
fn glOrthof(
    env: &mut Environment,
    left: GLfloat,
    right: GLfloat,
    bottom: GLfloat,
    top: GLfloat,
    near: GLfloat,
    far: GLfloat,
) {
    with_ctx_and_mem(env, |gles, _mem| {
        unsafe { gles.Orthof(left, right, bottom, top, near, far) };
    });
}
fn glOrthox(
    env: &mut Environment,
    left: GLfixed,
    right: GLfixed,
    bottom: GLfixed,
    top: GLfixed,
    near: GLfixed,
    far: GLfixed,
) {
    with_ctx_and_mem(env, |gles, _mem| {
        unsafe { gles.Orthox(left, right, bottom, top, near, far) };
    });
}
fn glFrustumf(
    env: &mut Environment,
    left: GLfloat,
    right: GLfloat,
    bottom: GLfloat,
    top: GLfloat,
    near: GLfloat,
    far: GLfloat,
) {
    with_ctx_and_mem(env, |gles, _mem| {
        unsafe { gles.Frustumf(left, right, bottom, top, near, far) };
    });
}
fn glFrustumx(
    env: &mut Environment,
    left: GLfixed,
    right: GLfixed,
    bottom: GLfixed,
    top: GLfixed,
    near: GLfixed,
    far: GLfixed,
) {
    with_ctx_and_mem(env, |gles, _mem| {
        unsafe { gles.Frustumx(left, right, bottom, top, near, far) };
    });
}
fn glRotatef(env: &mut Environment, angle: GLfloat, x: GLfloat, y: GLfloat, z: GLfloat) {
    with_ctx_and_mem(env, |gles, _mem| {
        unsafe { gles.Rotatef(angle, x, y, z) };
    });
}
fn glRotatex(env: &mut Environment, angle: GLfixed, x: GLfixed, y: GLfixed, z: GLfixed) {
    with_ctx_and_mem(env, |gles, _mem| {
        unsafe { gles.Rotatex(angle, x, y, z) };
    });
}
fn glScalef(env: &mut Environment, x: GLfloat, y: GLfloat, z: GLfloat) {
    with_ctx_and_mem(env, |gles, _mem| {
        unsafe { gles.Scalef(x, y, z) };
    });
}
fn glScalex(env: &mut Environment, x: GLfixed, y: GLfixed, z: GLfixed) {
    with_ctx_and_mem(env, |gles, _mem| {
        unsafe { gles.Scalex(x, y, z) };
    });
}
fn glTranslatef(env: &mut Environment, x: GLfloat, y: GLfloat, z: GLfloat) {
    with_ctx_and_mem(env, |gles, _mem| {
        unsafe { gles.Translatef(x, y, z) };
    });
}
fn glTranslatex(env: &mut Environment, x: GLfixed, y: GLfixed, z: GLfixed) {
    with_ctx_and_mem(env, |gles, _mem| {
        unsafe { gles.Translatex(x, y, z) };
    });
}

// Textures
fn glPixelStorei(env: &mut Environment, pname: GLenum, param: GLint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.PixelStorei(pname, param) })
}
fn glReadPixels(
    env: &mut Environment,
    x: GLint,
    y: GLint,
    width: GLsizei,
    height: GLsizei,
    format: GLenum,
    type_: GLenum,
    pixels: MutVoidPtr,
) {
    with_ctx_and_mem(env, |gles, mem| {
        let pixels = {
            let pixel_count: GuestUSize = width.checked_mul(height).unwrap().try_into().unwrap();
            let size = image_size_estimate(pixel_count, format, type_);
            mem.ptr_at_mut(pixels.cast::<u8>(), size).cast::<GLvoid>()
        };
        unsafe { gles.ReadPixels(x, y, width, height, format, type_, pixels) }
    })
}
fn glGenTextures(env: &mut Environment, n: GLsizei, textures: MutPtr<GLuint>) {
    with_ctx_and_mem(env, |gles, mem| {
        let n_usize: GuestUSize = n.try_into().unwrap();
        let textures = mem.ptr_at_mut(textures, n_usize);
        unsafe { gles.GenTextures(n, textures) }
    })
}
fn glDeleteTextures(env: &mut Environment, n: GLsizei, textures: ConstPtr<GLuint>) {
    with_ctx_and_mem(env, |gles, mem| {
        let n_usize: GuestUSize = n.try_into().unwrap();
        let textures = mem.ptr_at(textures, n_usize);
        unsafe { gles.DeleteTextures(n, textures) }
    })
}
fn glActiveTexture(env: &mut Environment, texture: GLenum) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.ActiveTexture(texture) })
}
fn glIsTexture(env: &mut Environment, texture: GLuint) -> GLboolean {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.IsTexture(texture) })
}
fn glBindTexture(env: &mut Environment, target: GLenum, texture: GLuint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.BindTexture(target, texture)
    })
}
fn glTexParameteri(env: &mut Environment, target: GLenum, pname: GLenum, param: GLint) {
    // So long as we haven't implemented glDrawTexOES yet, we can just ignore
    // this parameter, because it doesn't do anything for normal texture use.
    if pname == gles11::TEXTURE_CROP_RECT_OES {
        return;
    }
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.TexParameteri(target, pname, param)
    })
}
fn glTexParameterf(env: &mut Environment, target: GLenum, pname: GLenum, param: GLfloat) {
    // See above.
    if pname == gles11::TEXTURE_CROP_RECT_OES {
        return;
    }
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.TexParameterf(target, pname, param)
    })
}
fn glTexParameterx(env: &mut Environment, target: GLenum, pname: GLenum, param: GLfixed) {
    // See above.
    if pname == gles11::TEXTURE_CROP_RECT_OES {
        return;
    }
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.TexParameterx(target, pname, param)
    })
}
fn glTexParameteriv(env: &mut Environment, target: GLenum, pname: GLenum, params: ConstPtr<GLint>) {
    // See above.
    if pname == gles11::TEXTURE_CROP_RECT_OES {
        return;
    }
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let params = mem.ptr_at(params, 1 /* upper bound */);
        gles.TexParameteriv(target, pname, params)
    })
}
fn glTexParameterfv(
    env: &mut Environment,
    target: GLenum,
    pname: GLenum,
    params: ConstPtr<GLfloat>,
) {
    // See above.
    if pname == gles11::TEXTURE_CROP_RECT_OES {
        return;
    }
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let params = mem.ptr_at(params, 1 /* upper bound */);
        gles.TexParameterfv(target, pname, params)
    })
}
fn glTexParameterxv(
    env: &mut Environment,
    target: GLenum,
    pname: GLenum,
    params: ConstPtr<GLfixed>,
) {
    // See above.
    if pname == gles11::TEXTURE_CROP_RECT_OES {
        return;
    }
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let params = mem.ptr_at(params, 1 /* upper bound */);
        gles.TexParameterxv(target, pname, params)
    })
}
fn image_size_estimate(pixel_count: GuestUSize, format: GLenum, type_: GLenum) -> GuestUSize {
    let bytes_per_pixel: GuestUSize = match type_ {
        gles11::UNSIGNED_BYTE => match format {
            gles11::ALPHA | gles11::LUMINANCE => 1,
            gles11::LUMINANCE_ALPHA => 2,
            gles11::RGB => 3,
            gles11::RGBA => 4,
            gles11::BGRA_EXT => 4,
            _ => panic!("Unexpected format {format:#x}"),
        },
        gles11::UNSIGNED_SHORT_5_6_5
        | gles11::UNSIGNED_SHORT_4_4_4_4
        | gles11::UNSIGNED_SHORT_5_5_5_1 => 2,
        _ => panic!("Unexpected type {type_:#x}"),
    };
    // This is approximate, it doesn't account for alignment.
    pixel_count.checked_mul(bytes_per_pixel).unwrap()
}

fn current_framebuffer_name(env: &mut Environment) -> GLuint {
    with_ctx_and_mem_no_skip(env, |gles, _mem| unsafe {
        let mut framebuffer: GLint = 0;
        gles.GetIntegerv(gles11::FRAMEBUFFER_BINDING_OES, &mut framebuffer);
        framebuffer as GLuint
    })
}

fn current_renderbuffer_name(env: &mut Environment) -> GLuint {
    with_ctx_and_mem_no_skip(env, |gles, _mem| unsafe {
        let mut renderbuffer: GLint = 0;
        gles.GetIntegerv(gles11::RENDERBUFFER_BINDING_OES, &mut renderbuffer);
        renderbuffer as GLuint
    })
}

fn current_texture_2d_name(env: &mut Environment) -> GLuint {
    with_ctx_and_mem_no_skip(env, |gles, _mem| unsafe {
        let mut texture: GLint = 0;
        gles.GetIntegerv(gles11::TEXTURE_BINDING_2D, &mut texture);
        texture as GLuint
    })
}

fn framebuffer_should_scale(env: &mut Environment) -> bool {
    if env.options.scale_hack.get() == 1 {
        return false;
    }

    let framebuffer = current_framebuffer_name(env);
    framebuffer == 0
        || env
            .framework_state
            .opengles
            .scaled_framebuffers
            .get(&framebuffer)
            .copied()
            .unwrap_or(false)
}

fn is_screen_sized_texture(env: &Environment, width: GLsizei, height: GLsizei) -> bool {
    let Some(window) = env.window.as_ref() else {
        return false;
    };
    let (screen_width, screen_height) = window.size_unrotated_unscaled();
    let (width, height) = (width as u32, height as u32);
    (width == screen_width && height == screen_height)
        || (width == screen_height && height == screen_width)
}

fn glTexImage2D(
    env: &mut Environment,
    target: GLenum,
    level: GLint,
    internalformat: GLint,
    width: GLsizei,
    height: GLsizei,
    border: GLint,
    format: GLenum,
    type_: GLenum,
    pixels: ConstVoidPtr,
) {
    let scale_hack = env.options.scale_hack.get() as GLsizei;
    let scale_empty_screen_texture = scale_hack != 1
        && pixels.is_null()
        && target == gles11::TEXTURE_2D
        && level == 0
        && border == 0
        && is_screen_sized_texture(env, width, height);
    let (host_width, host_height) = if scale_empty_screen_texture {
        (width * scale_hack, height * scale_hack)
    } else {
        (width, height)
    };
    let texture = if target == gles11::TEXTURE_2D && level == 0 {
        Some(current_texture_2d_name(env))
    } else {
        None
    };
    if let Some(texture) = texture {
        env.framework_state
            .opengles
            .scaled_textures
            .insert(texture, scale_empty_screen_texture);
    }

    with_ctx_and_mem(env, |gles, mem| unsafe {
        let pixels = if pixels.is_null() {
            std::ptr::null()
        } else {
            let pixel_count: GuestUSize = width.checked_mul(height).unwrap().try_into().unwrap();
            let size = image_size_estimate(pixel_count, format, type_);
            mem.ptr_at(pixels.cast::<u8>(), size).cast::<GLvoid>()
        };
        gles.TexImage2D(
            target,
            level,
            internalformat,
            host_width,
            host_height,
            border,
            format,
            type_,
            pixels,
        )
    })
}
fn glTexSubImage2D(
    env: &mut Environment,
    target: GLenum,
    level: GLint,
    xoffset: GLint,
    yoffset: GLint,
    width: GLsizei,
    height: GLsizei,
    format: GLenum,
    type_: GLenum,
    pixels: ConstVoidPtr,
) {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let pixel_count: GuestUSize = width.checked_mul(height).unwrap().try_into().unwrap();
        let size = image_size_estimate(pixel_count, format, type_);
        let pixels = mem.ptr_at(pixels.cast::<u8>(), size).cast::<GLvoid>();
        gles.TexSubImage2D(
            target, level, xoffset, yoffset, width, height, format, type_, pixels,
        )
    })
}
fn glCompressedTexImage2D(
    env: &mut Environment,
    target: GLenum,
    level: GLint,
    internalformat: GLenum,
    width: GLsizei,
    height: GLsizei,
    border: GLint,
    image_size: GLsizei,
    data: ConstVoidPtr,
) {
    with_ctx_and_mem(env, |gles, mem| unsafe {
        let data = mem
            .ptr_at(data.cast::<u8>(), image_size.try_into().unwrap())
            .cast();
        gles.CompressedTexImage2D(
            target,
            level,
            internalformat,
            width,
            height,
            border,
            image_size,
            data,
        )
    })
}
fn glCopyTexImage2D(
    env: &mut Environment,
    target: GLenum,
    level: GLint,
    internalformat: GLenum,
    x: GLint,
    y: GLint,
    width: GLsizei,
    height: GLsizei,
    border: GLint,
) {
    let should_scale = framebuffer_should_scale(env);
    let scale_hack = env.options.scale_hack.get() as GLint;
    let (x, y, width, height) = if should_scale {
        (
            x * scale_hack,
            y * scale_hack,
            width * scale_hack,
            height * scale_hack,
        )
    } else {
        (x, y, width, height)
    };
    if target == gles11::TEXTURE_2D && level == 0 {
        let texture = current_texture_2d_name(env);
        env.framework_state
            .opengles
            .scaled_textures
            .insert(texture, should_scale);
    }
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.CopyTexImage2D(target, level, internalformat, x, y, width, height, border)
    })
}
fn glCopyTexSubImage2D(
    env: &mut Environment,
    target: GLenum,
    level: GLint,
    xoffset: GLint,
    yoffset: GLint,
    x: GLint,
    y: GLint,
    width: GLsizei,
    height: GLsizei,
) {
    let source_should_scale = framebuffer_should_scale(env);
    let texture_should_scale = if target == gles11::TEXTURE_2D {
        let texture = current_texture_2d_name(env);
        env.framework_state
            .opengles
            .scaled_textures
            .get(&texture)
            .copied()
            .unwrap_or(false)
    } else {
        false
    };
    let scale_hack = env.options.scale_hack.get() as GLint;
    let (x, y, width, height) = if source_should_scale {
        (
            x * scale_hack,
            y * scale_hack,
            width * scale_hack,
            height * scale_hack,
        )
    } else {
        (x, y, width, height)
    };
    let (xoffset, yoffset) = if texture_should_scale {
        (xoffset * scale_hack, yoffset * scale_hack)
    } else {
        (xoffset, yoffset)
    };
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.CopyTexSubImage2D(target, level, xoffset, yoffset, x, y, width, height)
    })
}
fn glTexEnvf(env: &mut Environment, target: GLenum, pname: GLenum, param: GLfloat) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.TexEnvf(target, pname, param)
    })
}
fn glTexEnvx(env: &mut Environment, target: GLenum, pname: GLenum, param: GLfixed) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.TexEnvx(target, pname, param)
    })
}
fn glTexEnvi(env: &mut Environment, target: GLenum, pname: GLenum, param: GLint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.TexEnvi(target, pname, param)
    })
}
fn glTexEnvfv(env: &mut Environment, target: GLenum, pname: GLenum, params: ConstPtr<GLfloat>) {
    assert!(
        target == gles11::TEXTURE_ENV || target == gles11::TEXTURE_FILTER_CONTROL_EXT,
        "target {target:#x}, pname {pname:#x}"
    );
    // TODO: GL_POINT_SPRITE_OES
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at(params, 4 /* upper bound */);
        unsafe { gles.TexEnvfv(target, pname, params) }
    })
}
fn glTexEnvxv(env: &mut Environment, target: GLenum, pname: GLenum, params: ConstPtr<GLfixed>) {
    // TODO: GL_POINT_SPRITE_OES
    assert!(target == gles11::TEXTURE_ENV);
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at(params, 4 /* upper bound */);
        unsafe { gles.TexEnvxv(target, pname, params) }
    })
}
fn glTexEnviv(env: &mut Environment, target: GLenum, pname: GLenum, params: ConstPtr<GLint>) {
    // TODO: GL_POINT_SPRITE_OES
    assert!(target == gles11::TEXTURE_ENV);
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at(params, 4 /* upper bound */);
        unsafe { gles.TexEnviv(target, pname, params) }
    })
}

fn glMultiTexCoord4f(
    env: &mut Environment,
    target: GLenum,
    s: GLfloat,
    t: GLfloat,
    r: GLfloat,
    q: GLfloat,
) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.MultiTexCoord4f(target, s, t, r, q)
    })
}
fn glMultiTexCoord4x(
    env: &mut Environment,
    target: GLenum,
    s: GLfixed,
    t: GLfixed,
    r: GLfixed,
    q: GLfixed,
) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.MultiTexCoord4x(target, s, t, r, q)
    })
}

// OES_framebuffer_object
fn glGenFramebuffersOES(env: &mut Environment, n: GLsizei, framebuffers: MutPtr<GLuint>) {
    with_ctx_and_mem(env, |gles, mem| {
        let n_usize: GuestUSize = n.try_into().unwrap();
        let framebuffers = mem.ptr_at_mut(framebuffers, n_usize);
        unsafe { gles.GenFramebuffersOES(n, framebuffers) }
    })
}
fn glGenRenderbuffersOES(env: &mut Environment, n: GLsizei, renderbuffers: MutPtr<GLuint>) {
    with_ctx_and_mem(env, |gles, mem| {
        let n_usize: GuestUSize = n.try_into().unwrap();
        let renderbuffers = mem.ptr_at_mut(renderbuffers, n_usize);
        unsafe { gles.GenRenderbuffersOES(n, renderbuffers) }
    })
}
fn glIsFramebufferOES(env: &mut Environment, framebuffer: GLuint) -> GLboolean {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.IsFramebufferOES(framebuffer)
    })
}
fn glIsRenderbufferOES(env: &mut Environment, renderbuffer: GLuint) -> GLboolean {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.IsRenderbufferOES(renderbuffer)
    })
}
fn glBindFramebufferOES(env: &mut Environment, target: GLenum, framebuffer: GLuint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.BindFramebufferOES(target, framebuffer)
    })
}
fn glBindRenderbufferOES(env: &mut Environment, target: GLenum, renderbuffer: GLuint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.BindRenderbufferOES(target, renderbuffer)
    })
}
fn glRenderbufferStorageOES(
    env: &mut Environment,
    target: GLenum,
    internalformat: GLenum,
    width: GLsizei,
    height: GLsizei,
) {
    // apply scale hack: give the app a larger framebuffer than it asked for
    let factor = env.options.scale_hack.get() as GLsizei;
    let renderbuffer = current_renderbuffer_name(env);
    env.framework_state
        .opengles
        .scaled_renderbuffers
        .insert(renderbuffer, factor != 1);
    let (width, height) = (width * factor, height * factor);
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.RenderbufferStorageOES(target, internalformat, width, height)
    })
}
fn glFramebufferRenderbufferOES(
    env: &mut Environment,
    target: GLenum,
    attachment: GLenum,
    renderbuffertarget: GLenum,
    renderbuffer: GLuint,
) {
    let framebuffer = current_framebuffer_name(env);
    let scaled = env
        .framework_state
        .opengles
        .scaled_renderbuffers
        .get(&renderbuffer)
        .copied()
        .unwrap_or(false);
    env.framework_state
        .opengles
        .scaled_framebuffers
        .insert(framebuffer, scaled);
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.FramebufferRenderbufferOES(target, attachment, renderbuffertarget, renderbuffer)
    })
}
fn glFramebufferTexture2DOES(
    env: &mut Environment,
    target: GLenum,
    attachment: GLenum,
    textarget: GLenum,
    texture: GLuint,
    level: i32,
) {
    let framebuffer = current_framebuffer_name(env);
    let scaled = env
        .framework_state
        .opengles
        .scaled_textures
        .get(&texture)
        .copied()
        .unwrap_or(false);
    env.framework_state
        .opengles
        .scaled_framebuffers
        .insert(framebuffer, scaled);
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.FramebufferTexture2DOES(target, attachment, textarget, texture, level)
    })
}
fn glGetFramebufferAttachmentParameterivOES(
    env: &mut Environment,
    target: GLenum,
    attachment: GLenum,
    pname: GLenum,
    params: MutPtr<GLint>,
) {
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at_mut(params, 1);
        unsafe { gles.GetFramebufferAttachmentParameterivOES(target, attachment, pname, params) }
    })
}
fn glGetRenderbufferParameterivOES(
    env: &mut Environment,
    target: GLenum,
    pname: GLenum,
    params: MutPtr<GLint>,
) {
    let factor = env.options.scale_hack.get() as GLint;
    with_ctx_and_mem(env, |gles, mem| {
        let params = mem.ptr_at_mut(params, 1);
        unsafe { gles.GetRenderbufferParameterivOES(target, pname, params) };
        // apply scale hack: scale down the reported size of the framebuffer,
        // assuming the framebuffer's true size is larger than it should be
        if pname == gles11::RENDERBUFFER_WIDTH_OES || pname == gles11::RENDERBUFFER_HEIGHT_OES {
            unsafe { params.write_unaligned(params.read_unaligned() / factor) }
        }
    })
}
fn glCheckFramebufferStatusOES(env: &mut Environment, target: GLenum) -> GLenum {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.CheckFramebufferStatusOES(target)
    })
}
fn glDeleteFramebuffersOES(env: &mut Environment, n: GLsizei, framebuffers: ConstPtr<GLuint>) {
    with_ctx_and_mem(env, |gles, mem| {
        let n_usize: GuestUSize = n.try_into().unwrap();
        let framebuffers = mem.ptr_at(framebuffers, n_usize);
        unsafe { gles.DeleteFramebuffersOES(n, framebuffers) }
    })
}
fn glDeleteRenderbuffersOES(env: &mut Environment, n: GLsizei, renderbuffers: ConstPtr<GLuint>) {
    with_ctx_and_mem(env, |gles, mem| {
        let n_usize: GuestUSize = n.try_into().unwrap();
        let renderbuffers = mem.ptr_at(renderbuffers, n_usize);
        unsafe { gles.DeleteRenderbuffersOES(n, renderbuffers) }
    })
}
fn glGenerateMipmapOES(env: &mut Environment, target: GLenum) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.GenerateMipmapOES(target) })
}

// OpenGL ES 2.0 core aliases for OES_framebuffer_object.
fn glGenFramebuffers(env: &mut Environment, n: GLsizei, framebuffers: MutPtr<GLuint>) {
    glGenFramebuffersOES(env, n, framebuffers)
}
fn glGenRenderbuffers(env: &mut Environment, n: GLsizei, renderbuffers: MutPtr<GLuint>) {
    glGenRenderbuffersOES(env, n, renderbuffers)
}
fn glIsFramebuffer(env: &mut Environment, framebuffer: GLuint) -> GLboolean {
    glIsFramebufferOES(env, framebuffer)
}
fn glIsRenderbuffer(env: &mut Environment, renderbuffer: GLuint) -> GLboolean {
    glIsRenderbufferOES(env, renderbuffer)
}
fn glBindFramebuffer(env: &mut Environment, target: GLenum, framebuffer: GLuint) {
    glBindFramebufferOES(env, target, framebuffer)
}
fn glBindRenderbuffer(env: &mut Environment, target: GLenum, renderbuffer: GLuint) {
    glBindRenderbufferOES(env, target, renderbuffer)
}
fn glRenderbufferStorage(
    env: &mut Environment,
    target: GLenum,
    internalformat: GLenum,
    width: GLsizei,
    height: GLsizei,
) {
    glRenderbufferStorageOES(env, target, internalformat, width, height)
}
fn glFramebufferRenderbuffer(
    env: &mut Environment,
    target: GLenum,
    attachment: GLenum,
    renderbuffertarget: GLenum,
    renderbuffer: GLuint,
) {
    glFramebufferRenderbufferOES(env, target, attachment, renderbuffertarget, renderbuffer)
}
fn glFramebufferTexture2D(
    env: &mut Environment,
    target: GLenum,
    attachment: GLenum,
    textarget: GLenum,
    texture: GLuint,
    level: i32,
) {
    glFramebufferTexture2DOES(env, target, attachment, textarget, texture, level)
}
fn glGetFramebufferAttachmentParameteriv(
    env: &mut Environment,
    target: GLenum,
    attachment: GLenum,
    pname: GLenum,
    params: MutPtr<GLint>,
) {
    glGetFramebufferAttachmentParameterivOES(env, target, attachment, pname, params)
}
fn glGetRenderbufferParameteriv(
    env: &mut Environment,
    target: GLenum,
    pname: GLenum,
    params: MutPtr<GLint>,
) {
    glGetRenderbufferParameterivOES(env, target, pname, params)
}
fn glCheckFramebufferStatus(env: &mut Environment, target: GLenum) -> GLenum {
    glCheckFramebufferStatusOES(env, target)
}
fn glDeleteFramebuffers(env: &mut Environment, n: GLsizei, framebuffers: ConstPtr<GLuint>) {
    glDeleteFramebuffersOES(env, n, framebuffers)
}
fn glDeleteRenderbuffers(env: &mut Environment, n: GLsizei, renderbuffers: ConstPtr<GLuint>) {
    glDeleteRenderbuffersOES(env, n, renderbuffers)
}
fn glGenerateMipmap(env: &mut Environment, target: GLenum) {
    glGenerateMipmapOES(env, target)
}

fn glBindVertexArrayOES(env: &mut Environment, array: GLuint) {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.BindVertexArrayOES(array) })
}
fn glDeleteVertexArraysOES(env: &mut Environment, n: GLsizei, arrays: ConstPtr<GLuint>) {
    with_ctx_and_mem(env, |gles, mem| {
        let n_usize: GuestUSize = n.try_into().unwrap();
        let arrays = mem.ptr_at(arrays, n_usize);
        unsafe { gles.DeleteVertexArraysOES(n, arrays) }
    })
}
fn glGenVertexArraysOES(env: &mut Environment, n: GLsizei, arrays: MutPtr<GLuint>) {
    with_ctx_and_mem(env, |gles, mem| {
        let n_usize: GuestUSize = n.try_into().unwrap();
        let arrays = mem.ptr_at_mut(arrays, n_usize);
        unsafe { gles.GenVertexArraysOES(n, arrays) }
    })
}
fn glIsVertexArrayOES(env: &mut Environment, array: GLuint) -> GLboolean {
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.IsVertexArrayOES(array) })
}

fn glGetBufferParameteriv(
    env: &mut Environment,
    target: GLenum,
    pname: GLenum,
    params: MutPtr<GLint>,
) {
    let params = env.mem.ptr_at_mut(params, 1);
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        gles.GetBufferParameteriv(target, pname, params)
    })
}
fn glMapBufferOES(env: &mut Environment, target: GLenum, access: GLenum) -> MutPtr<GLvoid> {
    //  "glMapBuffer maps to the client's address space the entire data store
    //  of the buffer object currently bound to target. The data can then be
    //  directly read and/or written relative to the returned pointer,
    //  depending on the specified access policy."
    // https://docs.gl/gl2/glMapBuffer
    //
    // We have to make an address space in the guest and "forward" those
    // reads/writes to the address space in the host, which is mapped to the
    // target buffer.
    // Since the mapped buffer can't be used until it's unmapped, we defer the
    // "forwarding" of read/writes until the moment the buffer is unmapped.
    assert!(matches!(target, ARRAY_BUFFER | ELEMENT_ARRAY_BUFFER));
    assert!(access == WRITE_ONLY_OES);
    let buffer_object_name = _get_currently_bound_buffer_object_name(env, target);

    let existing_mapping = {
        let current_ctx = env
            .framework_state
            .opengles
            .current_ctx_for_thread(env.current_thread);
        let current_ctx_host_object = env
            .objc
            .borrow_mut::<EAGLContextHostObject>(current_ctx.unwrap());
        current_ctx_host_object
            .mapped_buffers
            .remove(&buffer_object_name)
    };
    if let Some((guest_buffer, host_buffer)) = existing_mapping {
        log!(
            "Warning: glMapBufferOES called for already mapped buffer {}, implicitly unmapping old mapping",
            buffer_object_name
        );
        let buffer_size = _get_buffer_size(env, target) as u32;
        unsafe {
            host_buffer.copy_from(
                env.mem.bytes_at(guest_buffer.cast(), buffer_size).as_ptr() as *mut GLvoid,
                buffer_size as usize,
            );
        }
        env.mem.free(guest_buffer);
        with_ctx_and_mem_no_skip(env, |gles, _mem| unsafe { gles.UnmapBufferOES(target) });
    }

    let host_buffer = with_ctx_and_mem_no_skip(env, |gles, _mem| unsafe {
        gles.MapBufferOES(target, access)
    });
    if host_buffer.is_null() {
        nil.cast()
    } else {
        let buffer_size = _get_buffer_size(env, target) as u32;
        let guest_buffer: MutVoidPtr = env.mem.alloc(buffer_size).cast();
        // Copy host buffer to guest buffer
        unsafe {
            env.mem
                .bytes_at_mut(guest_buffer.cast(), buffer_size)
                .copy_from_slice(from_raw_parts(host_buffer as *mut u8, buffer_size as usize));
        }

        let current_ctx = env
            .framework_state
            .opengles
            .current_ctx_for_thread(env.current_thread);
        let current_ctx_host_object = env
            .objc
            .borrow_mut::<EAGLContextHostObject>(current_ctx.unwrap());
        if let Some((old_guest_buffer, _old_host_buffer)) = current_ctx_host_object
            .mapped_buffers
            .insert(buffer_object_name, (guest_buffer, host_buffer))
        {
            log!(
                "Warning: replacing stale glMapBufferOES mapping for buffer {}",
                buffer_object_name
            );
            env.mem.free(old_guest_buffer);
        }

        guest_buffer
    }
}
fn glUnmapBufferOES(env: &mut Environment, target: GLenum) -> GLboolean {
    //  "A mapped data store must be unmapped with glUnmapBuffer before its
    //  buffer object is used. Otherwise an error will be generated by any GL
    //  command that attempts to dereference the buffer object's data store.
    //  When a data store is unmapped, the pointer to its data store becomes
    //  invalid."
    // https://docs.gl/gl2/glMapBuffer
    //
    // Since the mapped buffer can't be used until it's unmapped, we defer the
    // "forwarding" of read/writes until the moment the buffer is unmapped.
    // The guest buffer is deallocated here
    let buffer_object_name = _get_currently_bound_buffer_object_name(env, target);

    let current_ctx = env
        .framework_state
        .opengles
        .current_ctx_for_thread(env.current_thread);
    let current_ctx_host_object = env
        .objc
        .borrow_mut::<EAGLContextHostObject>(current_ctx.unwrap());

    if let Some((guest_buffer, host_buffer)) = current_ctx_host_object
        .mapped_buffers
        .remove(&buffer_object_name)
    {
        let buffer_size = _get_buffer_size(env, target) as u32;
        // Copy guest buffer to host buffer
        unsafe {
            host_buffer.copy_from(
                env.mem.bytes_at(guest_buffer.cast(), buffer_size).as_ptr() as *mut GLvoid,
                buffer_size as usize,
            );
        }
        env.mem.free(guest_buffer);
    }
    with_ctx_and_mem(env, |gles, _mem| unsafe { gles.UnmapBufferOES(target) })
}

/// If fog is enabled, check if the values for start and end distances
/// are equal. Apple platforms (even modern Mac OS) seem to handle that
/// gracefully, however, both Windows and Android have issues in those cases.
/// This workaround is required so Doom 2 RPG renders correctly.
/// It prevents divisions by zero in levels where fog is used and both
/// values are set to 10000.
unsafe fn clamp_fog_state_values(gles: &mut dyn GLES) -> Option<(f32, f32)> {
    let mut fogEnabled: GLboolean = 0;
    gles.GetBooleanv(gles11::FOG, &mut fogEnabled);
    if fogEnabled != 0 {
        let mut fogStart: GLfloat = 0.0;
        let mut fogEnd: GLfloat = 0.0;
        gles.GetFloatv(gles11::FOG_START, &mut fogStart);
        gles.GetFloatv(gles11::FOG_END, &mut fogEnd);
        if fogStart == fogEnd {
            let newFogStart = fogEnd - 0.001;
            gles.Fogf(gles11::FOG_START, newFogStart);
            return Some((fogStart, fogEnd));
        }
    }
    None
}
unsafe fn restore_fog_state_values(gles: &mut dyn GLES, from_backup: Option<(f32, f32)>) {
    if let Some((fogStart, fogEnd)) = from_backup {
        gles.Fogf(gles11::FOG_START, fogStart);
        gles.Fogf(gles11::FOG_END, fogEnd);
    }
}

pub const FUNCTIONS: FunctionExports = &[
    // Generic state manipulation
    export_c_func!(glGetError()),
    export_c_func!(glEnable(_)),
    export_c_func!(glIsEnabled(_)),
    export_c_func!(glDisable(_)),
    export_c_func!(glClientActiveTexture(_)),
    export_c_func!(glEnableClientState(_)),
    export_c_func!(glDisableClientState(_)),
    export_c_func!(glGetBooleanv(_, _)),
    export_c_func!(glGetFloatv(_, _)),
    export_c_func!(glGetIntegerv(_, _)),
    export_c_func!(glGetPointerv(_, _)),
    export_c_func!(glGetTexEnviv(_, _, _)),
    export_c_func!(glGetTexEnvfv(_, _, _)),
    export_c_func!(glHint(_, _)),
    export_c_func!(glFinish()),
    export_c_func!(glFlush()),
    export_c_func!(glGetString(_)),
    // Other state manipulation
    export_c_func!(glAlphaFunc(_, _)),
    export_c_func!(glAlphaFuncx(_, _)),
    export_c_func!(glBlendFunc(_, _)),
    export_c_func!(glBlendFuncSeparate(_, _, _, _)),
    export_c_func!(glBlendEquation(_)),
    export_c_func!(glBlendEquationOES(_)),
    export_c_func!(glBlendEquationSeparate(_, _)),
    export_c_func!(glColorMask(_, _, _, _)),
    export_c_func!(glClipPlanef(_, _)),
    export_c_func!(glClipPlanex(_, _)),
    export_c_func!(glCullFace(_)),
    export_c_func!(glDepthFunc(_)),
    export_c_func!(glDepthMask(_)),
    export_c_func!(glDepthRangef(_, _)),
    export_c_func!(glDepthRangex(_, _)),
    export_c_func!(glFrontFace(_)),
    export_c_func!(glPolygonOffset(_, _)),
    export_c_func!(glPolygonOffsetx(_, _)),
    export_c_func!(glSampleCoverage(_, _)),
    export_c_func!(glSampleCoveragex(_, _)),
    export_c_func!(glShadeModel(_)),
    export_c_func!(glScissor(_, _, _, _)),
    export_c_func!(glViewport(_, _, _, _)),
    export_c_func!(glLineWidth(_)),
    export_c_func!(glLineWidthx(_)),
    export_c_func!(glStencilFunc(_, _, _)),
    export_c_func!(glStencilOp(_, _, _)),
    export_c_func!(glStencilMask(_)),
    export_c_func!(glLogicOp(_)),
    // Points
    export_c_func!(glPointSize(_)),
    export_c_func!(glPointSizex(_)),
    export_c_func!(glPointParameterf(_, _)),
    export_c_func!(glPointParameterx(_, _)),
    export_c_func!(glPointParameterfv(_, _)),
    export_c_func!(glPointParameterxv(_, _)),
    // Lighting and materials
    export_c_func!(glFogf(_, _)),
    export_c_func!(glFogx(_, _)),
    export_c_func!(glFogfv(_, _)),
    export_c_func!(glFogxv(_, _)),
    export_c_func!(glLightf(_, _, _)),
    export_c_func!(glLightx(_, _, _)),
    export_c_func!(glLightfv(_, _, _)),
    export_c_func!(glLightxv(_, _, _)),
    export_c_func!(glLightModelf(_, _)),
    export_c_func!(glLightModelfv(_, _)),
    export_c_func!(glLightModelx(_, _)),
    export_c_func!(glLightModelxv(_, _)),
    export_c_func!(glMaterialf(_, _, _)),
    export_c_func!(glMaterialx(_, _, _)),
    export_c_func!(glMaterialfv(_, _, _)),
    export_c_func!(glMaterialxv(_, _, _)),
    // Buffers
    export_c_func!(glIsBuffer(_)),
    export_c_func!(glGenBuffers(_, _)),
    export_c_func!(glDeleteBuffers(_, _)),
    export_c_func!(glBindBuffer(_, _)),
    export_c_func!(glBufferData(_, _, _, _)),
    export_c_func!(glBufferSubData(_, _, _, _)),
    // OpenGL ES 2.0 shader/program entry points
    export_c_func!(glCreateShader(_)),
    export_c_func!(glShaderSource(_, _, _, _)),
    export_c_func!(glCompileShader(_)),
    export_c_func!(glGetShaderiv(_, _, _)),
    export_c_func!(glGetShaderInfoLog(_, _, _, _)),
    export_c_func!(glGetShaderSource(_, _, _, _)),
    export_c_func!(glDeleteShader(_)),
    export_c_func!(glCreateProgram()),
    export_c_func!(glAttachShader(_, _)),
    export_c_func!(glDetachShader(_, _)),
    export_c_func!(glBindAttribLocation(_, _, _)),
    export_c_func!(glGetAttribLocation(_, _)),
    export_c_func!(glLinkProgram(_)),
    export_c_func!(glValidateProgram(_)),
    export_c_func!(glGetProgramiv(_, _, _)),
    export_c_func!(glGetProgramInfoLog(_, _, _, _)),
    export_c_func!(glGetActiveUniform(_, _, _, _, _, _, _)),
    export_c_func!(glUseProgram(_)),
    export_c_func!(glDeleteProgram(_)),
    export_c_func!(glGetUniformLocation(_, _)),
    export_c_func!(glUniform1f(_, _)),
    export_c_func!(glUniform1i(_, _)),
    export_c_func!(glUniform1fv(_, _, _)),
    export_c_func!(glUniform2fv(_, _, _)),
    export_c_func!(glUniform3fv(_, _, _)),
    export_c_func!(glUniform4fv(_, _, _)),
    export_c_func!(glUniform4f(_, _, _, _, _)),
    export_c_func!(glUniformMatrix3fv(_, _, _, _)),
    export_c_func!(glUniformMatrix4fv(_, _, _, _)),
    export_c_func!(glVertexAttribPointer(_, _, _, _, _, _)),
    export_c_func!(glEnableVertexAttribArray(_)),
    export_c_func!(glDisableVertexAttribArray(_)),
    export_c_func!(glInsertEventMarkerEXT(_, _)),
    export_c_func!(glPushGroupMarkerEXT(_, _)),
    export_c_func!(glPopGroupMarkerEXT()),
    export_c_func!(glDiscardFramebufferEXT(_, _, _)),
    export_c_func!(glGenQueriesEXT(_, _)),
    export_c_func!(glDeleteQueriesEXT(_, _)),
    export_c_func!(glIsQueryEXT(_)),
    export_c_func!(glBeginQueryEXT(_, _)),
    export_c_func!(glEndQueryEXT(_)),
    export_c_func!(glGetQueryivEXT(_, _, _)),
    export_c_func!(glGetQueryObjectivEXT(_, _, _)),
    export_c_func!(glGetQueryObjectuivEXT(_, _, _)),
    export_c_func!(glGetProgramBinaryOES(_, _, _, _, _)),
    export_c_func!(glProgramBinaryOES(_, _, _, _)),
    export_c_func!(glRenderbufferStorageMultisampleAPPLE(_, _, _, _, _)),
    export_c_func!(glResolveMultisampleFramebufferAPPLE()),
    export_c_func!(glRenderbufferStorageMultisampleIMG(_, _, _, _, _)),
    export_c_func!(glFramebufferTexture2DMultisampleIMG(_, _, _, _, _, _)),
    export_c_func!(glDrawBuffersNV(_, _)),
    export_c_func!(glQueryCounterNV(_, _)),
    export_c_func!(glGetQueryObjecti64vNV(_, _, _)),
    export_c_func!(glGetQueryObjectui64vNV(_, _, _)),
    export_c_func!(glAlphaFuncQCOM(_, _)),
    // Non-pointers
    export_c_func!(glColor4f(_, _, _, _)),
    export_c_func!(glColor4x(_, _, _, _)),
    export_c_func!(glColor4ub(_, _, _, _)),
    export_c_func!(glNormal3f(_, _, _)),
    export_c_func!(glNormal3x(_, _, _)),
    // Pointers
    export_c_func!(glColorPointer(_, _, _, _)),
    export_c_func!(glNormalPointer(_, _, _)),
    export_c_func!(glTexCoordPointer(_, _, _, _)),
    export_c_func!(glVertexPointer(_, _, _, _)),
    // Drawing
    export_c_func!(glDrawArrays(_, _, _)),
    export_c_func!(glDrawElements(_, _, _, _)),
    // Clearing
    export_c_func!(glClear(_)),
    export_c_func!(glClearColor(_, _, _, _)),
    export_c_func!(glClearColorx(_, _, _, _)),
    export_c_func!(glClearDepthf(_)),
    export_c_func!(glClearDepthx(_)),
    export_c_func!(glClearStencil(_)),
    // Matrix stack operations
    export_c_func!(glMatrixMode(_)),
    export_c_func!(glLoadIdentity()),
    export_c_func!(glLoadMatrixf(_)),
    export_c_func!(glLoadMatrixx(_)),
    export_c_func!(glMultMatrixf(_)),
    export_c_func!(glMultMatrixx(_)),
    export_c_func!(glPushMatrix()),
    export_c_func!(glPopMatrix()),
    export_c_func!(glOrthof(_, _, _, _, _, _)),
    export_c_func!(glOrthox(_, _, _, _, _, _)),
    export_c_func!(glFrustumf(_, _, _, _, _, _)),
    export_c_func!(glFrustumx(_, _, _, _, _, _)),
    export_c_func!(glRotatef(_, _, _, _)),
    export_c_func!(glRotatex(_, _, _, _)),
    export_c_func!(glScalef(_, _, _)),
    export_c_func!(glScalex(_, _, _)),
    export_c_func!(glTranslatef(_, _, _)),
    export_c_func!(glTranslatex(_, _, _)),
    // Textures
    export_c_func!(glPixelStorei(_, _)),
    export_c_func!(glReadPixels(_, _, _, _, _, _, _)),
    export_c_func!(glGenTextures(_, _)),
    export_c_func!(glDeleteTextures(_, _)),
    export_c_func!(glActiveTexture(_)),
    export_c_func!(glIsTexture(_)),
    export_c_func!(glBindTexture(_, _)),
    export_c_func!(glTexParameteri(_, _, _)),
    export_c_func!(glTexParameterf(_, _, _)),
    export_c_func!(glTexParameterx(_, _, _)),
    export_c_func!(glTexParameteriv(_, _, _)),
    export_c_func!(glTexParameterfv(_, _, _)),
    export_c_func!(glTexParameterxv(_, _, _)),
    export_c_func!(glTexImage2D(_, _, _, _, _, _, _, _, _)),
    export_c_func!(glTexSubImage2D(_, _, _, _, _, _, _, _, _)),
    export_c_func!(glCompressedTexImage2D(_, _, _, _, _, _, _, _)),
    export_c_func!(glCopyTexImage2D(_, _, _, _, _, _, _, _)),
    export_c_func!(glCopyTexSubImage2D(_, _, _, _, _, _, _, _)),
    export_c_func!(glTexEnvf(_, _, _)),
    export_c_func!(glTexEnvx(_, _, _)),
    export_c_func!(glTexEnvi(_, _, _)),
    export_c_func!(glTexEnvfv(_, _, _)),
    export_c_func!(glTexEnvxv(_, _, _)),
    export_c_func!(glTexEnviv(_, _, _)),
    export_c_func!(glMultiTexCoord4f(_, _, _, _, _)),
    export_c_func!(glMultiTexCoord4x(_, _, _, _, _)),
    // OpenGL ES 2.0 framebuffer object core entry points
    export_c_func!(glGenFramebuffers(_, _)),
    export_c_func!(glGenRenderbuffers(_, _)),
    export_c_func!(glIsFramebuffer(_)),
    export_c_func!(glIsRenderbuffer(_)),
    export_c_func!(glBindFramebuffer(_, _)),
    export_c_func!(glBindRenderbuffer(_, _)),
    export_c_func!(glRenderbufferStorage(_, _, _, _)),
    export_c_func!(glFramebufferRenderbuffer(_, _, _, _)),
    export_c_func!(glFramebufferTexture2D(_, _, _, _, _)),
    export_c_func!(glGetFramebufferAttachmentParameteriv(_, _, _, _)),
    export_c_func!(glGetRenderbufferParameteriv(_, _, _)),
    export_c_func!(glCheckFramebufferStatus(_)),
    export_c_func!(glDeleteFramebuffers(_, _)),
    export_c_func!(glDeleteRenderbuffers(_, _)),
    export_c_func!(glGenerateMipmap(_)),
    // OES_framebuffer_object
    export_c_func!(glGenFramebuffersOES(_, _)),
    export_c_func!(glGenRenderbuffersOES(_, _)),
    export_c_func!(glIsFramebufferOES(_)),
    export_c_func!(glIsRenderbufferOES(_)),
    export_c_func!(glBindFramebufferOES(_, _)),
    export_c_func!(glBindRenderbufferOES(_, _)),
    export_c_func!(glRenderbufferStorageOES(_, _, _, _)),
    export_c_func!(glFramebufferRenderbufferOES(_, _, _, _)),
    export_c_func!(glFramebufferTexture2DOES(_, _, _, _, _)),
    export_c_func!(glGetFramebufferAttachmentParameterivOES(_, _, _, _)),
    export_c_func!(glGetRenderbufferParameterivOES(_, _, _)),
    export_c_func!(glCheckFramebufferStatusOES(_)),
    export_c_func!(glDeleteFramebuffersOES(_, _)),
    export_c_func!(glDeleteRenderbuffersOES(_, _)),
    export_c_func!(glGenerateMipmapOES(_)),
    // OES_vertex_array_object
    export_c_func!(glBindVertexArrayOES(_)),
    export_c_func!(glDeleteVertexArraysOES(_, _)),
    export_c_func!(glGenVertexArraysOES(_, _)),
    export_c_func!(glIsVertexArrayOES(_)),
    export_c_func!(glGetBufferParameteriv(_, _, _)),
    export_c_func!(glMapBufferOES(_, _)),
    export_c_func!(glUnmapBufferOES(_)),
];

fn _get_currently_bound_buffer_object_name(env: &mut Environment, target: GLenum) -> GLuint {
    with_ctx_and_mem(env, |gles, _mem| unsafe {
        let pname = match target {
            ARRAY_BUFFER => ARRAY_BUFFER_BINDING,
            ELEMENT_ARRAY_BUFFER => ELEMENT_ARRAY_BUFFER_BINDING,
            _ => panic!(),
        };
        let mut currently_bound_buffer_name: GLint = 0;
        gles.GetIntegerv(pname, &mut currently_bound_buffer_name);
        currently_bound_buffer_name as GLuint
    })
}

fn _get_buffer_size(env: &mut Environment, target: GLenum) -> GLint {
    with_ctx_and_mem(env, |gles, _mem| {
        let mut buffer_size: GLint = 0;
        unsafe { gles.GetBufferParameteriv(target, gles11::BUFFER_SIZE, &mut buffer_size) }
        buffer_size
    })
}
