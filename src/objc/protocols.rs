/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Handling of Objective-C protocols.

use super::{id, nil, Class, HostObject, ObjC, SEL};
use crate::dyld::{export_c_func, FunctionExports};
use crate::mach_o::MachO;
use crate::mem::{guest_size_of, ConstPtr, ConstVoidPtr, GuestUSize, Mem, MutPtr, Ptr, SafeRead};
use crate::Environment;
use std::collections::HashSet;

pub(super) type Protocol = id;

/// The 32-bit Objective-C 2 protocol layout used by iOS applications.
#[repr(C, packed)]
struct protocol_t {
    _isa: id,
    name: ConstPtr<u8>,
    protocols: ConstPtr<protocol_list_t>,
    required_instance_methods: ConstPtr<method_list_t>,
    required_class_methods: ConstPtr<method_list_t>,
    optional_instance_methods: ConstPtr<method_list_t>,
    optional_class_methods: ConstPtr<method_list_t>,
    _instance_properties: ConstVoidPtr,
    _size: GuestUSize,
    _flags: u32,
    _extended_method_types: ConstPtr<ConstPtr<u8>>,
}
unsafe impl SafeRead for protocol_t {}

#[repr(C, packed)]
struct protocol_list_t {
    count: GuestUSize,
    // Protocol pointers follow the struct.
}
unsafe impl SafeRead for protocol_list_t {}

#[repr(C, packed)]
struct method_list_t {
    entsize: GuestUSize,
    count: GuestUSize,
    // method_t entries follow the struct.
}
unsafe impl SafeRead for method_list_t {}

#[repr(C, packed)]
struct method_t {
    name: ConstPtr<u8>,
    types: ConstPtr<u8>,
    _imp: ConstVoidPtr,
}
unsafe impl SafeRead for method_t {}

#[repr(C, packed)]
struct objc_method_description {
    name: SEL,
    types: ConstPtr<u8>,
}
unsafe impl SafeRead for objc_method_description {}

struct ProtocolHostObject {
    name: String,
    name_ptr: ConstPtr<u8>,
    adopted_protocols: Vec<Protocol>,
    required_instance_methods: ConstPtr<method_list_t>,
    required_class_methods: ConstPtr<method_list_t>,
    optional_instance_methods: ConstPtr<method_list_t>,
    optional_class_methods: ConstPtr<method_list_t>,
}
impl HostObject for ProtocolHostObject {}

pub(super) fn read_protocol_list(mem: &Mem, list: ConstVoidPtr) -> Vec<Protocol> {
    if list.is_null() {
        return Vec::new();
    }

    let list: ConstPtr<protocol_list_t> = list.cast();
    let protocol_list_t { count } = mem.read(list);
    let protocols: ConstPtr<Protocol> = (list + 1).cast();
    (0..count).map(|i| mem.read(protocols + i)).collect()
}

impl ObjC {
    /// Register protocol metadata embedded in an Objective-C app binary.
    /// Protocol objects are owned by the image and therefore have static
    /// lifetime, just like class objects.
    pub fn register_bin_protocols(&mut self, bin: &MachO, mem: &Mem) {
        let Some(list) = bin.get_section("__objc_protolist") else {
            return;
        };

        assert!(list.size % guest_size_of::<Protocol>() == 0);
        let base: ConstPtr<Protocol> = Ptr::from_bits(list.addr);
        let count = list.size / guest_size_of::<Protocol>();

        for i in 0..count {
            let protocol = mem.read(base + i);
            let raw: protocol_t = mem.read(protocol.cast());
            assert!(raw._size >= guest_size_of::<protocol_t>());
            let name = mem.cstr_at_utf8(raw.name).unwrap().to_string();

            assert!(
                self.get_host_object(protocol).is_none(),
                "Protocol object {protocol:?} was registered more than once"
            );
            self.register_static_object(
                protocol,
                Box::new(ProtocolHostObject {
                    name: name.clone(),
                    name_ptr: raw.name,
                    adopted_protocols: Vec::new(),
                    required_instance_methods: raw.required_instance_methods,
                    required_class_methods: raw.required_class_methods,
                    optional_instance_methods: raw.optional_instance_methods,
                    optional_class_methods: raw.optional_class_methods,
                }),
            );

            // Static libraries can contribute duplicate protocol definitions.
            // Name-based runtime lookups return the first canonical instance,
            // while all instances remain valid protocol objects.
            self.protocols.entry(name).or_insert(protocol);
        }

        // Protocols may adopt protocols declared later in the image, so this
        // is intentionally a separate pass after every object is registered.
        for i in 0..count {
            let protocol = mem.read(base + i);
            let raw: protocol_t = mem.read(protocol.cast());
            self.borrow_mut::<ProtocolHostObject>(protocol)
                .adopted_protocols = read_protocol_list(mem, raw.protocols.cast());
        }
    }

    fn protocol_host_object(&self, protocol: Protocol) -> Option<&ProtocolHostObject> {
        self.get_host_object(protocol)?
            .as_any()
            .downcast_ref::<ProtocolHostObject>()
    }

    fn protocol_conforms_to_protocol(&self, protocol: Protocol, other: Protocol) -> bool {
        let other_name = match self.protocol_host_object(other) {
            Some(other) => other.name.clone(),
            None => return false,
        };

        let mut pending = vec![protocol];
        let mut visited = HashSet::new();
        while let Some(candidate) = pending.pop() {
            if !visited.insert(candidate) {
                continue;
            }
            let Some(candidate) = self.protocol_host_object(candidate) else {
                continue;
            };
            if candidate.name == other_name {
                return true;
            }
            pending.extend(candidate.adopted_protocols.iter().copied());
        }
        false
    }

    fn class_conforms_to_protocol(&self, class: Class, protocol: Protocol) -> bool {
        if self.protocol_host_object(protocol).is_none() {
            return false;
        }

        let mut class = class;
        while class != nil {
            if self.class_protocols.get(&class).is_some_and(|protocols| {
                protocols
                    .iter()
                    .any(|&candidate| self.protocol_conforms_to_protocol(candidate, protocol))
            }) {
                return true;
            }

            class = self.get_superclass(class);
        }
        false
    }
}

fn copy_protocol_array(env: &mut Environment, protocols: &[Protocol]) -> MutPtr<Protocol> {
    if protocols.is_empty() {
        return Ptr::null();
    }

    let count: GuestUSize = protocols.len().try_into().unwrap();
    let allocation_count = count.checked_add(1).unwrap();
    let byte_count = allocation_count
        .checked_mul(guest_size_of::<Protocol>())
        .unwrap();
    let result: MutPtr<Protocol> = env.mem.alloc(byte_count).cast();
    for (i, &protocol) in protocols.iter().enumerate() {
        env.mem.write(result + u32::try_from(i).unwrap(), protocol);
    }
    env.mem.write(result + count, nil);
    result
}

#[allow(non_snake_case)]
fn objc_getProtocol(env: &mut Environment, name: ConstPtr<u8>) -> Protocol {
    if name.is_null() {
        return nil;
    }
    let name = env.mem.cstr_at_utf8(name).unwrap();
    env.objc.protocols.get(name).copied().unwrap_or(nil)
}

#[allow(non_snake_case)]
fn objc_copyProtocolList(env: &mut Environment, out_count: MutPtr<u32>) -> MutPtr<Protocol> {
    let mut protocols: Vec<_> = env.objc.protocols.iter().collect();
    protocols.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
    let protocols: Vec<_> = protocols
        .into_iter()
        .map(|(_, &protocol)| protocol)
        .collect();

    if !out_count.is_null() {
        env.mem
            .write(out_count, u32::try_from(protocols.len()).unwrap());
    }
    copy_protocol_array(env, &protocols)
}

#[allow(non_snake_case)]
fn protocol_getName(env: &mut Environment, protocol: Protocol) -> ConstPtr<u8> {
    env.objc
        .protocol_host_object(protocol)
        .map_or(Ptr::null(), |protocol| protocol.name_ptr)
}

#[allow(non_snake_case)]
fn protocol_isEqual(env: &mut Environment, protocol: Protocol, other: Protocol) -> bool {
    let Some(protocol) = env.objc.protocol_host_object(protocol) else {
        return false;
    };
    let Some(other) = env.objc.protocol_host_object(other) else {
        return false;
    };
    protocol.name == other.name
}

#[allow(non_snake_case)]
fn protocol_conformsToProtocol(env: &mut Environment, protocol: Protocol, other: Protocol) -> bool {
    env.objc.protocol_conforms_to_protocol(protocol, other)
}

#[allow(non_snake_case)]
fn class_conformsToProtocol(env: &mut Environment, class: Class, protocol: Protocol) -> bool {
    if class == nil {
        false
    } else {
        env.objc.class_conforms_to_protocol(class, protocol)
    }
}

#[allow(non_snake_case)]
fn protocol_copyProtocolList(
    env: &mut Environment,
    protocol: Protocol,
    out_count: MutPtr<u32>,
) -> MutPtr<Protocol> {
    let adopted_protocols = env
        .objc
        .protocol_host_object(protocol)
        .map(|protocol| protocol.adopted_protocols.clone())
        .unwrap_or_default();
    if !out_count.is_null() {
        env.mem
            .write(out_count, u32::try_from(adopted_protocols.len()).unwrap());
    }
    copy_protocol_array(env, &adopted_protocols)
}

#[allow(non_snake_case)]
fn protocol_copyMethodDescriptionList(
    env: &mut Environment,
    protocol: Protocol,
    is_required_method: bool,
    is_instance_method: bool,
    out_count: MutPtr<u32>,
) -> MutPtr<objc_method_description> {
    if !out_count.is_null() {
        env.mem.write(out_count, 0);
    }

    let Some(protocol) = env.objc.protocol_host_object(protocol) else {
        return Ptr::null();
    };
    let list = match (is_required_method, is_instance_method) {
        (true, true) => protocol.required_instance_methods,
        (true, false) => protocol.required_class_methods,
        (false, true) => protocol.optional_instance_methods,
        (false, false) => protocol.optional_class_methods,
    };
    if list.is_null() {
        return Ptr::null();
    }

    let method_list_t { entsize, count } = env.mem.read(list);
    assert!(entsize >= guest_size_of::<method_t>());
    if count == 0 {
        return Ptr::null();
    }

    let byte_count = count
        .checked_mul(guest_size_of::<objc_method_description>())
        .unwrap();
    let result: MutPtr<objc_method_description> = env.mem.alloc(byte_count).cast();
    let methods: ConstPtr<method_t> = (list + 1).cast();
    for i in 0..count {
        let method: ConstPtr<method_t> =
            Ptr::from_bits(methods.to_bits() + i.checked_mul(entsize).unwrap());
        let method: method_t = env.mem.read(method);
        let name = env.objc.register_bin_selector(method.name, &env.mem);
        env.mem.write(
            result + i,
            objc_method_description {
                name,
                types: method.types,
            },
        );
    }

    if !out_count.is_null() {
        env.mem.write(out_count, count);
    }
    result
}

pub(super) const FUNCTIONS: FunctionExports = &[
    export_c_func!(objc_getProtocol(_)),
    export_c_func!(objc_copyProtocolList(_)),
    export_c_func!(protocol_getName(_)),
    export_c_func!(protocol_isEqual(_, _)),
    export_c_func!(protocol_conformsToProtocol(_, _)),
    export_c_func!(class_conformsToProtocol(_, _)),
    export_c_func!(protocol_copyProtocolList(_, _)),
    export_c_func!(protocol_copyMethodDescriptionList(_, _, _, _)),
];
