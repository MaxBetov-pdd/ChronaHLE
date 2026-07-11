/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSUUID`.

use super::ns_string;
use crate::mem::{ConstPtr, MutPtr};
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, release, retain, ClassExports, HostObject,
    NSZonePtr,
};
use crate::Environment;
use uuid::Uuid;

struct NSUUIDHostObject {
    uuid: Uuid,
}
impl HostObject for NSUUIDHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSUUID: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    env.objc.alloc_object(
        this,
        Box::new(NSUUIDHostObject { uuid: Uuid::new_v4() }),
        &mut env.mem,
    )
}

+ (id)UUID {
    let uuid: id = msg_class![env; NSUUID alloc];
    let uuid: id = msg![env; uuid init];
    autorelease(env, uuid)
}

- (id)initWithUUIDString:(id)uuid_string {
    let uuid_string = ns_string::to_rust_string(env, uuid_string);
    let Ok(uuid) = Uuid::parse_str(&uuid_string) else {
        release(env, this);
        return nil;
    };
    env.objc.borrow_mut::<NSUUIDHostObject>(this).uuid = uuid;
    this
}

- (id)initWithUUIDBytes:(ConstPtr<u8>)bytes {
    let bytes: [u8; 16] = env.mem.bytes_at(bytes, 16).try_into().unwrap();
    env.objc.borrow_mut::<NSUUIDHostObject>(this).uuid = Uuid::from_bytes(bytes);
    this
}

- (())getUUIDBytes:(MutPtr<u8>)bytes {
    let uuid = *env.objc.borrow::<NSUUIDHostObject>(this).uuid.as_bytes();
    env.mem.bytes_at_mut(bytes, 16).copy_from_slice(&uuid);
}

- (id)UUIDString {
    let uuid = env.objc.borrow::<NSUUIDHostObject>(this).uuid;
    let string = ns_string::from_rust_string(env, uuid.hyphenated().to_string().to_uppercase());
    autorelease(env, string)
}

- (id)copyWithZone:(NSZonePtr)_zone {
    retain(env, this)
}

@end

};

pub fn zero_uuid(env: &mut Environment) -> id {
    let class = env.objc.get_known_class("NSUUID", &mut env.mem);
    env.objc.alloc_static_object(
        class,
        Box::new(NSUUIDHostObject { uuid: Uuid::nil() }),
        &mut env.mem,
    )
}
