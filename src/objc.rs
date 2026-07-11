/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Objective-C runtime.
//!
//! Apple's [Programming with Objective-C](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/ProgrammingWithObjectiveC/Introduction/Introduction.html)
//! is a useful introduction to the language from a user's perspective.
//! There are further resources in the child modules of this module, but they
//! are more implementation-specific.
//!
//! The strategy for this emulator will be to provide our own implementations of
//! an Objective-C runtime and libraries for it (Foundation etc). These
//! implementations will be "host code": Rust code forming part of the emulator,
//! not emulated code. The runtime will need to be able to handle classes that
//! originate from the guest app, classes defined by the host, and sometimes
//! classes that are both (considering Objective-C's support for inheritance,
//! categories and dynamic class editing).

use crate::dyld::{export_c_func, ConstantExports, FunctionExports, HostConstant, HostDylib};
use crate::objc::messages::ThreadInitializer;
use crate::MutexId;
use std::collections::{HashMap, HashSet};

mod blocks;
mod classes;
mod messages;
mod methods;
mod objects;
mod properties;
mod protocols;
mod selectors;
mod synchronization;

pub(crate) use blocks::{copy_block, release_block};
pub use classes::{objc_classes, Class, ClassExports, ClassTemplate};
pub use messages::{
    autorelease, msg, msg_class, msg_send, msg_send_no_initialize, msg_send_no_type_checking,
    msg_send_super2, msg_super, objc_super, release, retain,
};
pub use methods::{HostIMP, IMP};
pub use objects::{
    id, impl_HostObject_with_superclass, nil, AnyHostObject, HostObject, TrivialHostObject,
};
pub use properties::todo_objc_setter;
pub use selectors::{selector, SEL};

use crate::mem::{guest_size_of, ConstPtr, MutPtr, Ptr};
use crate::Environment;
use classes::{
    class_getInstanceSize, class_getName, class_getProperty, class_getSuperclass, objc_getClass,
    ClassHostObject, FakeClass, UnimplementedClass,
};
pub(crate) use messages::objc_msgSend;
use messages::{objc_msgSendSuper2, objc_msgSend_stret, MsgSendSignature, MsgSendSuperSignature};
use methods::{class_addMethod, method_list_t};
use objects::{objc_object, object_getClass, HostObjectEntry};
use properties::{
    ivar_list_t, objc_copyStruct, objc_getProperty, objc_setProperty, objc_setProperty_atomic,
    objc_setProperty_atomic_copy, objc_setProperty_nonatomic, objc_setProperty_nonatomic_copy,
};
use selectors::sel_registerName;
use synchronization::{objc_sync_enter, objc_sync_exit};

/// Typedef for `NSZone *`. This is a [fossil type] found in the signature of
/// `allocWithZone:` and similar methods. Its value is always ignored.
///
/// [fossil type]: https://en.wiktionary.org/wiki/fossil_word
pub type NSZonePtr = crate::mem::MutVoidPtr;

/// Main type holding Objective-C runtime state.
pub struct ObjC {
    /// Known selectors (interned method name strings).
    selectors: HashMap<String, SEL>,

    /// Mapping of known (guest) object pointers to their host objects.
    ///
    /// If an object isn't in this map, we will consider it not to exist.
    objects: HashMap<id, HostObjectEntry>,

    /// Guest memory locations holding zeroing weak references, grouped by the
    /// object they currently reference.
    weak_references: HashMap<id, HashSet<MutPtr<id>>>,

    /// State owned by the Apple Block ABI runtime.
    block_state: blocks::State,

    /// Known classes.
    ///
    /// Look at the `isa` to get the metaclass for a class.
    classes: HashMap<String, Class>,

    /// Runtime-owned metadata used by class introspection. These maps include
    /// placeholder and substituted classes as well as full class objects.
    class_names: HashMap<Class, ConstPtr<u8>>,
    class_superclasses: HashMap<Class, Class>,

    /// Canonical protocol objects by name. All protocol object pointers are
    /// still registered in `objects`, including duplicate image definitions.
    protocols: HashMap<String, protocols::Protocol>,

    /// Protocols declared directly by classes and categories.
    class_protocols: HashMap<Class, Vec<protocols::Protocol>>,

    /// Mutexes used in @synchronized blocks (objc_sync_enter/exit).
    sync_mutexes: HashMap<id, MutexId>,

    /// Mutexes for running the +initialize function.
    initializer_threads: HashMap<id, ThreadInitializer>,

    /// Temporary storage for optional type information when sending a message.
    /// Type information isn't part of the `objc_msgSend` ABI, so an alternative
    /// channel is needed.
    message_type_info: Option<(std::any::TypeId, &'static str)>,
}

impl ObjC {
    pub fn new() -> ObjC {
        ObjC {
            selectors: HashMap::new(),
            objects: HashMap::new(),
            weak_references: HashMap::new(),
            block_state: blocks::State::default(),
            classes: HashMap::new(),
            class_names: HashMap::new(),
            class_superclasses: HashMap::new(),
            protocols: HashMap::new(),
            class_protocols: HashMap::new(),
            sync_mutexes: HashMap::new(),
            initializer_threads: HashMap::new(),
            message_type_info: None,
        }
    }
}

pub const DYLIB: HostDylib = HostDylib {
    path: "/usr/lib/libobjc.A.dylib",
    aliases: &["/usr/lib/libobjc.dylib"],
    class_exports: &[],
    constant_exports: &[CONSTANTS, blocks::CONSTANTS],
    function_exports: &[FUNCTIONS, blocks::FUNCTIONS, protocols::FUNCTIONS],
};

const CONSTANTS: ConstantExports = &[
    // We don't use these in our Objective-C runtime, but exporting useless
    // symbols for these silences the warning about the unhandled relocation,
    // and avoids a linker error for the integration tests.
    ("__objc_empty_vtable", HostConstant::NullPtr),
    ("__objc_empty_cache", HostConstant::NullPtr),
];

#[allow(non_snake_case)]
fn objc_autoreleasePoolPush(env: &mut Environment) -> id {
    let pool: id = msg_class![env; NSAutoreleasePool new];
    pool
}

#[allow(non_snake_case)]
fn objc_autoreleasePoolPop(env: &mut Environment, pool: id) {
    if pool != nil {
        release(env, pool);
    }
}

fn objc_retain(env: &mut Environment, object: id) -> id {
    if object == nil {
        return object;
    }
    if let Some(block) = blocks::retain_if_block(env, object.cast_void().cast_const()) {
        return Ptr::from_bits(block.to_bits());
    }
    if env.objc.has_static_lifetime(object) {
        return object;
    }
    retain(env, object)
}

pub(crate) fn objc_release(env: &mut Environment, object: id) {
    if object == nil {
        return;
    }
    if blocks::release_if_block(env, object.cast_void().cast_const()) {
        return;
    }
    if env.objc.has_static_lifetime(object) {
        return;
    }
    release(env, object)
}

fn objc_autorelease(env: &mut Environment, object: id) -> id {
    if object == nil {
        return object;
    }
    if blocks::is_block(env, object.cast_void().cast_const()) {
        () = msg_class![env; NSAutoreleasePool addObject:object];
        return object;
    }
    if env.objc.has_static_lifetime(object) {
        return object;
    }
    autorelease(env, object)
}

#[allow(non_snake_case)]
fn objc_retainAutoreleasedReturnValue(env: &mut Environment, object: id) -> id {
    objc_retain(env, object)
}

#[allow(non_snake_case)]
fn objc_autoreleaseReturnValue(env: &mut Environment, object: id) -> id {
    objc_autorelease(env, object)
}

#[allow(non_snake_case)]
fn objc_retainAutoreleaseReturnValue(env: &mut Environment, object: id) -> id {
    let object = objc_retain(env, object);
    objc_autorelease(env, object)
}

#[allow(non_snake_case)]
fn objc_retainAutorelease(env: &mut Environment, object: id) -> id {
    let object = objc_retain(env, object);
    objc_autorelease(env, object)
}

fn registered_classes(env: &Environment) -> Vec<Class> {
    let mut classes: Vec<_> = env.objc.classes.iter().collect();
    classes.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
    classes.into_iter().map(|(_, class)| *class).collect()
}

#[allow(non_snake_case)]
fn objc_getClassList(env: &mut Environment, buffer: MutPtr<Class>, buffer_count: i32) -> i32 {
    assert!(buffer_count >= 0);
    let classes = registered_classes(env);
    if !buffer.is_null() {
        for (index, class) in classes.iter().take(buffer_count as usize).enumerate() {
            env.mem.write(buffer + index.try_into().unwrap(), *class);
        }
    }
    classes.len().try_into().unwrap()
}

#[allow(non_snake_case)]
fn objc_copyClassList(env: &mut Environment, out_count: MutPtr<u32>) -> MutPtr<Class> {
    let classes = registered_classes(env);
    let count: u32 = classes.len().try_into().unwrap();
    if !out_count.is_null() {
        env.mem.write(out_count, count);
    }

    let allocation_count = count.checked_add(1).unwrap();
    let byte_count = allocation_count
        .checked_mul(guest_size_of::<Class>())
        .unwrap();
    let result: MutPtr<Class> = env.mem.alloc(byte_count).cast();
    for (index, class) in classes.into_iter().enumerate() {
        env.mem.write(result + index.try_into().unwrap(), class);
    }
    env.mem.write(result + count, nil);
    result
}

fn unregister_weak_reference(env: &mut Environment, location: MutPtr<id>, object: id) {
    if object == nil {
        return;
    }

    let remove_object_entry = if let Some(locations) = env.objc.weak_references.get_mut(&object) {
        locations.remove(&location);
        locations.is_empty()
    } else {
        false
    };
    if remove_object_entry {
        env.objc.weak_references.remove(&object);
    }
}

fn initialize_weak_reference(env: &mut Environment, location: MutPtr<id>, object: id) -> id {
    let is_live_object = object == nil
        || env.objc.objects.contains_key(&object)
        || blocks::is_block(env, object.cast_void().cast_const());
    let object = if is_live_object {
        object
    } else {
        // A zeroing weak reference cannot legally point to a deallocated object.
        nil
    };

    env.mem.write(location, object);
    if object != nil {
        env.objc
            .weak_references
            .entry(object)
            .or_default()
            .insert(location);
    }
    object
}

#[allow(non_snake_case)]
pub(crate) fn objc_initWeak(env: &mut Environment, location: MutPtr<id>, object: id) -> id {
    initialize_weak_reference(env, location, object)
}

#[allow(non_snake_case)]
pub(crate) fn objc_storeWeak(env: &mut Environment, location: MutPtr<id>, object: id) -> id {
    let old_object = env.mem.read(location);
    unregister_weak_reference(env, location, old_object);
    initialize_weak_reference(env, location, object)
}

#[allow(non_snake_case)]
fn objc_loadWeakRetained(env: &mut Environment, location: MutPtr<id>) -> id {
    let object = env.mem.read(location);
    if object == nil {
        return nil;
    }
    if !env.objc.objects.contains_key(&object)
        && !blocks::is_block(env, object.cast_void().cast_const())
    {
        unregister_weak_reference(env, location, object);
        env.mem.write(location, nil);
        return nil;
    }
    objc_retain(env, object)
}

#[allow(non_snake_case)]
pub(crate) fn objc_loadWeak(env: &mut Environment, location: MutPtr<id>) -> id {
    let object = objc_loadWeakRetained(env, location);
    objc_autorelease(env, object)
}

#[allow(non_snake_case)]
pub(crate) fn objc_destroyWeak(env: &mut Environment, location: MutPtr<id>) {
    let object = env.mem.read(location);
    unregister_weak_reference(env, location, object);
    env.mem.write(location, nil);
}

#[allow(non_snake_case)]
fn objc_copyWeak(env: &mut Environment, destination: MutPtr<id>, source: MutPtr<id>) {
    let object = env.mem.read(source);
    initialize_weak_reference(env, destination, object);
}

#[allow(non_snake_case)]
fn objc_moveWeak(env: &mut Environment, destination: MutPtr<id>, source: MutPtr<id>) {
    let object = env.mem.read(source);
    unregister_weak_reference(env, source, object);
    env.mem.write(source, nil);
    initialize_weak_reference(env, destination, object);
}

#[allow(non_snake_case)]
fn objc_storeStrong(env: &mut Environment, location: MutPtr<id>, object: id) {
    let object = objc_retain(env, object);
    let old_object = env.mem.read(location);
    env.mem.write(location, object);
    objc_release(env, old_object);
}

const FUNCTIONS: FunctionExports = &[
    export_c_func!(class_getInstanceSize(_)),
    export_c_func!(class_getName(_)),
    export_c_func!(class_getSuperclass(_)),
    export_c_func!(class_getProperty(_, _)),
    export_c_func!(class_addMethod(_, _, _, _)),
    export_c_func!(objc_msgSend(_, _)),
    export_c_func!(objc_msgSend_stret(_, _, _)),
    export_c_func!(objc_msgSendSuper2(_, _)),
    export_c_func!(objc_getClass(_)),
    export_c_func!(objc_getProperty(_, _, _, _)),
    export_c_func!(objc_setProperty(_, _, _, _, _, _)),
    export_c_func!(objc_setProperty_atomic(_, _, _, _)),
    export_c_func!(objc_setProperty_atomic_copy(_, _, _, _)),
    export_c_func!(objc_setProperty_nonatomic(_, _, _, _)),
    export_c_func!(objc_setProperty_nonatomic_copy(_, _, _, _)),
    export_c_func!(objc_copyStruct(_, _, _, _, _)),
    export_c_func!(objc_sync_enter(_)),
    export_c_func!(objc_sync_exit(_)),
    export_c_func!(object_getClass(_)),
    export_c_func!(sel_registerName(_)),
    export_c_func!(objc_autoreleasePoolPush()),
    export_c_func!(objc_autoreleasePoolPop(_)),
    export_c_func!(objc_retain(_)),
    export_c_func!(objc_release(_)),
    export_c_func!(objc_autorelease(_)),
    export_c_func!(objc_retainAutoreleasedReturnValue(_)),
    export_c_func!(objc_autoreleaseReturnValue(_)),
    export_c_func!(objc_retainAutoreleaseReturnValue(_)),
    export_c_func!(objc_retainAutorelease(_)),
    export_c_func!(objc_getClassList(_, _)),
    export_c_func!(objc_copyClassList(_)),
    export_c_func!(objc_initWeak(_, _)),
    export_c_func!(objc_storeWeak(_, _)),
    export_c_func!(objc_loadWeakRetained(_)),
    export_c_func!(objc_loadWeak(_)),
    export_c_func!(objc_destroyWeak(_)),
    export_c_func!(objc_copyWeak(_, _)),
    export_c_func!(objc_moveWeak(_, _)),
    export_c_func!(objc_storeStrong(_, _)),
];
