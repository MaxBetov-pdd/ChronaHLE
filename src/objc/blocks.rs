/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Apple Block ABI runtime support.

use super::id;
use crate::abi::{CallFromHost, GuestFunction};
use crate::dyld::{export_c_func, ConstantExports, FunctionExports, HostConstant};
use crate::mem::{guest_size_of, ConstVoidPtr, MutVoidPtr, Ptr, SafeRead};
use crate::Environment;
use std::collections::HashMap;

const BLOCK_REFCOUNT_MASK: i32 = 0x0000_fffe;
const BLOCK_NEEDS_FREE: i32 = 1 << 24;
const BLOCK_HAS_COPY_DISPOSE: i32 = 1 << 25;
const BLOCK_FIELD_IS_OBJECT: i32 = 3;
const BLOCK_FIELD_IS_BLOCK: i32 = 7;
const BLOCK_FIELD_IS_BYREF: i32 = 8;
const BLOCK_FIELD_IS_WEAK: i32 = 16;
const BLOCK_BYREF_CALLER: i32 = 128;
const BLOCK_FIELD_FLAGS_MASK: i32 = 0xff;
const BLOCK_FIELD_IS_WEAK_OBJECT: i32 = BLOCK_FIELD_IS_OBJECT | BLOCK_FIELD_IS_WEAK;
const BLOCK_FIELD_IS_WEAK_BYREF: i32 = BLOCK_FIELD_IS_BYREF | BLOCK_FIELD_IS_WEAK;
const BLOCK_BYREF_CALLER_OBJECT: i32 = BLOCK_BYREF_CALLER | BLOCK_FIELD_IS_OBJECT;
const BLOCK_BYREF_CALLER_BLOCK: i32 = BLOCK_BYREF_CALLER | BLOCK_FIELD_IS_BLOCK;
const BLOCK_BYREF_CALLER_WEAK_OBJECT: i32 = BLOCK_BYREF_CALLER | BLOCK_FIELD_IS_WEAK_OBJECT;

#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
struct BlockLiteral {
    isa: ConstVoidPtr,
    flags: i32,
    reserved: i32,
    invoke: GuestFunction,
    descriptor: ConstVoidPtr,
}
unsafe impl SafeRead for BlockLiteral {}

#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
struct BlockDescriptor {
    _reserved: u32,
    size: u32,
}
unsafe impl SafeRead for BlockDescriptor {}

#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
struct BlockDescriptorCopyDispose {
    _reserved: u32,
    _size: u32,
    copy: GuestFunction,
    dispose: GuestFunction,
}
unsafe impl SafeRead for BlockDescriptorCopyDispose {}

#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
struct BlockByrefHeader {
    isa: ConstVoidPtr,
    forwarding: MutVoidPtr,
    flags: i32,
    size: i32,
}
unsafe impl SafeRead for BlockByrefHeader {}

#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
struct BlockByrefCopyDispose {
    _header: BlockByrefHeader,
    keep: GuestFunction,
    dispose: GuestFunction,
}
unsafe impl SafeRead for BlockByrefCopyDispose {}

#[derive(Default)]
pub(super) struct State {
    concrete_stack_block: Option<ConstVoidPtr>,
    concrete_global_block: Option<ConstVoidPtr>,
    concrete_malloc_block: Option<ConstVoidPtr>,
    malloc_block_refcounts: HashMap<u32, u32>,
    malloc_byref_refcounts: HashMap<u32, u32>,
}

#[derive(Copy, Clone, Debug)]
enum BlockKind {
    Stack,
    Global,
    Malloc,
}

#[derive(Copy, Clone)]
enum ConcreteBlockClass {
    Stack,
    Global,
    Malloc,
}

fn concrete_block_class(env: &mut Environment, class: ConcreteBlockClass) -> ConstVoidPtr {
    let existing = match class {
        ConcreteBlockClass::Stack => env.objc.block_state.concrete_stack_block,
        ConcreteBlockClass::Global => env.objc.block_state.concrete_global_block,
        ConcreteBlockClass::Malloc => env.objc.block_state.concrete_malloc_block,
    };
    if let Some(existing) = existing {
        return existing;
    }

    // The real symbols are arrays used as class identities. Their contents are
    // private runtime data; guest blocks only need a stable, distinct address.
    let token = env.mem.calloc(32 * 4).cast_const();
    match class {
        ConcreteBlockClass::Stack => env.objc.block_state.concrete_stack_block = Some(token),
        ConcreteBlockClass::Global => env.objc.block_state.concrete_global_block = Some(token),
        ConcreteBlockClass::Malloc => env.objc.block_state.concrete_malloc_block = Some(token),
    }
    token
}

fn concrete_stack_block(env: &mut Environment) -> ConstVoidPtr {
    concrete_block_class(env, ConcreteBlockClass::Stack)
}

fn concrete_global_block(env: &mut Environment) -> ConstVoidPtr {
    concrete_block_class(env, ConcreteBlockClass::Global)
}

fn classify(env: &Environment, block: ConstVoidPtr) -> Option<BlockKind> {
    if block.is_null() {
        return None;
    }
    if env
        .objc
        .block_state
        .malloc_block_refcounts
        .contains_key(&block.to_bits())
    {
        return Some(BlockKind::Malloc);
    }
    if env
        .mem
        .get_bytes_fallible(block, guest_size_of::<BlockLiteral>())
        .is_none()
    {
        return None;
    }

    let literal: BlockLiteral = env.mem.read(block.cast());
    if Some(literal.isa) == env.objc.block_state.concrete_global_block {
        Some(BlockKind::Global)
    } else if Some(literal.isa) == env.objc.block_state.concrete_stack_block {
        Some(BlockKind::Stack)
    } else if Some(literal.isa) == env.objc.block_state.concrete_malloc_block
        && literal.flags & BLOCK_NEEDS_FREE != 0
    {
        Some(BlockKind::Malloc)
    } else {
        None
    }
}

pub(super) fn is_block(env: &Environment, block: ConstVoidPtr) -> bool {
    classify(env, block).is_some()
}

fn set_block_refcount(env: &mut Environment, block: MutVoidPtr, refcount: u32) {
    let mut literal: BlockLiteral = env.mem.read(block.cast());
    let encoded = refcount.saturating_mul(2).min(BLOCK_REFCOUNT_MASK as u32) as i32;
    literal.flags = (literal.flags & !BLOCK_REFCOUNT_MASK) | encoded | BLOCK_NEEDS_FREE;
    env.mem.write(block.cast(), literal);
}

fn copy_helper(literal: BlockLiteral, env: &Environment) -> Option<GuestFunction> {
    if literal.flags & BLOCK_HAS_COPY_DISPOSE == 0 {
        return None;
    }
    let descriptor: BlockDescriptorCopyDispose = env.mem.read(literal.descriptor.cast());
    Some(descriptor.copy)
}

fn dispose_helper(literal: BlockLiteral, env: &Environment) -> Option<GuestFunction> {
    if literal.flags & BLOCK_HAS_COPY_DISPOSE == 0 {
        return None;
    }
    let descriptor: BlockDescriptorCopyDispose = env.mem.read(literal.descriptor.cast());
    Some(descriptor.dispose)
}

fn copy_known_block(env: &mut Environment, block: ConstVoidPtr, kind: BlockKind) -> ConstVoidPtr {
    match kind {
        BlockKind::Global => block,
        BlockKind::Malloc => {
            let refcount = env
                .objc
                .block_state
                .malloc_block_refcounts
                .entry(block.to_bits())
                .or_insert(1);
            *refcount = refcount.checked_add(1).unwrap();
            let refcount = *refcount;
            set_block_refcount(env, block.cast_mut(), refcount);
            block
        }
        BlockKind::Stack => {
            let literal: BlockLiteral = env.mem.read(block.cast());
            assert!(!literal.descriptor.is_null());
            let descriptor: BlockDescriptor = env.mem.read(literal.descriptor.cast());
            assert!(descriptor.size >= guest_size_of::<BlockLiteral>());

            let copy = env.mem.alloc(descriptor.size);
            env.mem.memmove(copy, block, descriptor.size);

            let mut copied_literal: BlockLiteral = env.mem.read(copy.cast());
            copied_literal.isa = concrete_block_class(env, ConcreteBlockClass::Malloc);
            env.mem.write(copy.cast(), copied_literal);
            env.objc
                .block_state
                .malloc_block_refcounts
                .insert(copy.to_bits(), 1);
            set_block_refcount(env, copy, 1);

            if let Some(helper) = copy_helper(literal, env) {
                let _: () = helper.call_from_host(env, (copy, block));
            }
            copy.cast_const()
        }
    }
}

pub(super) fn retain_if_block(env: &mut Environment, block: ConstVoidPtr) -> Option<ConstVoidPtr> {
    let kind = classify(env, block)?;
    Some(copy_known_block(env, block, kind))
}

pub(crate) fn copy_block(env: &mut Environment, block: ConstVoidPtr) -> ConstVoidPtr {
    if block.is_null() {
        return block;
    }
    let kind = classify(env, block).unwrap_or_else(|| panic!("Not a valid block: {block:?}"));
    copy_known_block(env, block, kind)
}

fn release_malloc_block(env: &mut Environment, block: MutVoidPtr) {
    let refcount = *env
        .objc
        .block_state
        .malloc_block_refcounts
        .get(&block.to_bits())
        .expect("Untracked malloc block");
    if refcount > 1 {
        let new_refcount = refcount - 1;
        env.objc
            .block_state
            .malloc_block_refcounts
            .insert(block.to_bits(), new_refcount);
        set_block_refcount(env, block, new_refcount);
        return;
    }

    env.objc
        .block_state
        .malloc_block_refcounts
        .remove(&block.to_bits());
    let literal: BlockLiteral = env.mem.read(block.cast());
    if let Some(helper) = dispose_helper(literal, env) {
        let _: () = helper.call_from_host(env, (block.cast_const(),));
    }
    let object: id = Ptr::from_bits(block.to_bits());
    env.objc.clear_weak_references(object, &mut env.mem);
    env.mem.free(block);
}

pub(super) fn release_if_block(env: &mut Environment, block: ConstVoidPtr) -> bool {
    let Some(kind) = classify(env, block) else {
        return false;
    };
    if let BlockKind::Malloc = kind {
        release_malloc_block(env, block.cast_mut());
    }
    true
}

pub(crate) fn release_block(env: &mut Environment, block: ConstVoidPtr) {
    if block.is_null() {
        return;
    }
    assert!(release_if_block(env, block), "Not a valid block: {block:?}");
}

fn set_byref_refcount(env: &mut Environment, byref: MutVoidPtr, refcount: u32) {
    let mut header: BlockByrefHeader = env.mem.read(byref.cast());
    let encoded = refcount.min(u16::MAX.into()) as i32;
    header.flags = (header.flags & !0xffff) | encoded | BLOCK_NEEDS_FREE;
    env.mem.write(byref.cast(), header);
}

fn copy_byref(env: &mut Environment, source: ConstVoidPtr) -> MutVoidPtr {
    let source_header: BlockByrefHeader = env.mem.read(source.cast());
    let forwarded = if source_header.forwarding.is_null() {
        source.cast_mut()
    } else {
        source_header.forwarding
    };

    if let Some(refcount) = env
        .objc
        .block_state
        .malloc_byref_refcounts
        .get(&forwarded.to_bits())
        .copied()
    {
        let new_refcount = refcount.checked_add(1).unwrap();
        env.objc
            .block_state
            .malloc_byref_refcounts
            .insert(forwarded.to_bits(), new_refcount);
        set_byref_refcount(env, forwarded, new_refcount);
        return forwarded;
    }

    let forwarded_header: BlockByrefHeader = env.mem.read(forwarded.cast());
    assert!(forwarded_header.size >= guest_size_of::<BlockByrefHeader>() as i32);
    let size = forwarded_header.size as u32;
    let copy = env.mem.alloc(size);
    env.mem.memmove(copy, forwarded.cast_const(), size);

    let mut copied_header: BlockByrefHeader = env.mem.read(copy.cast());
    copied_header.forwarding = copy;
    env.mem.write(copy.cast(), copied_header);
    set_byref_refcount(env, copy, 1);

    let mut original_header: BlockByrefHeader = env.mem.read(forwarded.cast());
    original_header.forwarding = copy;
    env.mem.write(forwarded.cast(), original_header);
    env.objc
        .block_state
        .malloc_byref_refcounts
        .insert(copy.to_bits(), 1);

    if forwarded_header.flags & BLOCK_HAS_COPY_DISPOSE != 0 {
        let helpers: BlockByrefCopyDispose = env.mem.read(copy.cast());
        let keep = helpers.keep;
        let _: () = keep.call_from_host(env, (copy, forwarded.cast_const()));
    }
    copy
}

fn release_byref(env: &mut Environment, source: ConstVoidPtr) {
    let source_header: BlockByrefHeader = env.mem.read(source.cast());
    let forwarded = if source_header.forwarding.is_null() {
        source.cast_mut()
    } else {
        source_header.forwarding
    };
    let Some(refcount) = env
        .objc
        .block_state
        .malloc_byref_refcounts
        .get(&forwarded.to_bits())
        .copied()
    else {
        return;
    };

    if refcount > 1 {
        let new_refcount = refcount - 1;
        env.objc
            .block_state
            .malloc_byref_refcounts
            .insert(forwarded.to_bits(), new_refcount);
        set_byref_refcount(env, forwarded, new_refcount);
        return;
    }

    env.objc
        .block_state
        .malloc_byref_refcounts
        .remove(&forwarded.to_bits());
    let header: BlockByrefHeader = env.mem.read(forwarded.cast());
    if header.flags & BLOCK_HAS_COPY_DISPOSE != 0 {
        let helpers: BlockByrefCopyDispose = env.mem.read(forwarded.cast());
        let dispose = helpers.dispose;
        let _: () = dispose.call_from_host(env, (forwarded.cast_const(),));
    }
    env.mem.free(forwarded);
}

#[allow(non_snake_case)]
fn _Block_copy(env: &mut Environment, block: ConstVoidPtr) -> ConstVoidPtr {
    copy_block(env, block)
}

#[allow(non_snake_case)]
fn _Block_release(env: &mut Environment, block: ConstVoidPtr) {
    release_block(env, block)
}

#[allow(non_snake_case)]
fn _Block_object_assign(
    env: &mut Environment,
    destination: MutVoidPtr,
    object: ConstVoidPtr,
    flags: i32,
) {
    match flags & BLOCK_FIELD_FLAGS_MASK {
        BLOCK_FIELD_IS_OBJECT => {
            let object: id = Ptr::from_bits(object.to_bits());
            let retained = super::objc_retain(env, object);
            env.mem.write(destination.cast(), retained);
        }
        BLOCK_FIELD_IS_WEAK_OBJECT => {
            let object: id = Ptr::from_bits(object.to_bits());
            super::initialize_weak_reference(env, destination.cast(), object);
        }
        BLOCK_FIELD_IS_BLOCK => {
            let copied = copy_block(env, object);
            env.mem.write(destination.cast(), copied);
        }
        BLOCK_FIELD_IS_BYREF | BLOCK_FIELD_IS_WEAK_BYREF => {
            let copied = copy_byref(env, object);
            env.mem.write(destination.cast(), copied);
        }
        BLOCK_BYREF_CALLER_OBJECT | BLOCK_BYREF_CALLER_BLOCK | BLOCK_BYREF_CALLER_WEAK_OBJECT => {
            env.mem.write(destination.cast(), object);
        }
        flags => panic!("Unsupported _Block_object_assign flags: {flags:#x}"),
    }
}

#[allow(non_snake_case)]
fn _Block_object_dispose(env: &mut Environment, object: ConstVoidPtr, flags: i32) {
    match flags & BLOCK_FIELD_FLAGS_MASK {
        BLOCK_FIELD_IS_OBJECT => {
            let object: id = Ptr::from_bits(object.to_bits());
            super::objc_release(env, object);
        }
        BLOCK_FIELD_IS_BLOCK => {
            assert!(release_if_block(env, object));
        }
        BLOCK_FIELD_IS_BYREF | BLOCK_FIELD_IS_WEAK_BYREF => {
            release_byref(env, object);
        }
        BLOCK_BYREF_CALLER_OBJECT | BLOCK_BYREF_CALLER_BLOCK | BLOCK_BYREF_CALLER_WEAK_OBJECT => {}
        flags => panic!("Unsupported _Block_object_dispose flags: {flags:#x}"),
    }
}

#[allow(non_snake_case)]
fn objc_retainBlock(env: &mut Environment, block: ConstVoidPtr) -> ConstVoidPtr {
    copy_block(env, block)
}

pub const CONSTANTS: ConstantExports = &[
    (
        "__NSConcreteStackBlock",
        HostConstant::Custom(concrete_stack_block),
    ),
    (
        "__NSConcreteGlobalBlock",
        HostConstant::Custom(concrete_global_block),
    ),
];

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(_Block_copy(_)),
    export_c_func!(_Block_release(_)),
    export_c_func!(_Block_object_assign(_, _, _)),
    export_c_func!(_Block_object_dispose(_, _)),
    export_c_func!(objc_retainBlock(_)),
];
