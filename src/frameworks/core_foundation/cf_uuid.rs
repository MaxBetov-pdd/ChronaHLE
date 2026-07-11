/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `CFUUID`.

use super::cf_allocator::{kCFAllocatorDefault, CFAllocatorRef};
use super::cf_string::CFStringRef;
use super::CFTypeRef;
use crate::abi::GuestArg;
use crate::dyld::{export_c_func, FunctionExports};
use crate::frameworks::foundation::ns_string::from_rust_string;
use crate::mem::SafeRead;
use crate::objc::{objc_classes, ClassExports, HostObject};
use crate::{impl_GuestRet_for_large_struct, Environment};
use uuid::Uuid;

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

// CFUUID doesn't have a corresponding NS type (at least, not up until iOS 6+
// and even that one is _not_ toll-free bridged, see NSUUID docs),
// but the callers of CFUUIDCreate() are expected to call CFRelease() on them.
@implementation _touchHLE_CFUUID: NSObject
@end

};

/// Note: Apple is using a pointer to an opaque struct instead
type CFUUIDRef = CFTypeRef;

#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(C, packed)]
struct CFUUIDBytes {
    bytes: [u8; 16],
}
unsafe impl SafeRead for CFUUIDBytes {}
impl_GuestRet_for_large_struct!(CFUUIDBytes);
impl GuestArg for CFUUIDBytes {
    const REG_COUNT: usize = 4;

    fn from_regs(regs: &[u32]) -> Self {
        let mut bytes = [0; 16];
        for (chunk, reg) in bytes.chunks_exact_mut(4).zip(regs) {
            chunk.copy_from_slice(&reg.to_le_bytes());
        }
        Self { bytes }
    }

    fn to_regs(self, regs: &mut [u32]) {
        for (reg, chunk) in regs.iter_mut().zip(self.bytes.chunks_exact(4)) {
            *reg = u32::from_le_bytes(chunk.try_into().unwrap());
        }
    }
}

struct CFUUIDHostObject {
    uuid: Uuid,
}
impl HostObject for CFUUIDHostObject {}

fn create_with_uuid(env: &mut Environment, allocator: CFAllocatorRef, uuid: Uuid) -> CFUUIDRef {
    assert!(allocator == kCFAllocatorDefault || env.mem.read(allocator).is_system_default()); // unimplemented

    let host_obj = Box::new(CFUUIDHostObject { uuid });
    let class = env.objc.get_known_class("_touchHLE_CFUUID", &mut env.mem);
    env.objc.alloc_object(class, host_obj, &mut env.mem)
}

fn CFUUIDCreate(env: &mut Environment, allocator: CFAllocatorRef) -> CFUUIDRef {
    create_with_uuid(env, allocator, Uuid::new_v4())
}

fn CFUUIDCreateFromUUIDBytes(
    env: &mut Environment,
    allocator: CFAllocatorRef,
    bytes: CFUUIDBytes,
) -> CFUUIDRef {
    create_with_uuid(env, allocator, Uuid::from_bytes(bytes.bytes))
}

fn CFUUIDGetUUIDBytes(env: &mut Environment, uuid: CFUUIDRef) -> CFUUIDBytes {
    let bytes = *env.objc.borrow::<CFUUIDHostObject>(uuid).uuid.as_bytes();
    CFUUIDBytes { bytes }
}

fn CFUUIDCreateString(
    env: &mut Environment,
    allocator: CFAllocatorRef,
    uuid: CFUUIDRef,
) -> CFStringRef {
    assert!(allocator == kCFAllocatorDefault || env.mem.read(allocator).is_system_default()); // unimplemented

    let host_object = env.objc.borrow::<CFUUIDHostObject>(uuid);
    let uuid_str = host_object.uuid.hyphenated().to_string().to_uppercase();
    from_rust_string(env, uuid_str)
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(CFUUIDCreate(_)),
    export_c_func!(CFUUIDCreateFromUUIDBytes(_, _)),
    export_c_func!(CFUUIDCreateString(_, _)),
    export_c_func!(CFUUIDGetUUIDBytes(_)),
];
