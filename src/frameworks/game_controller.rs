/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! GameController framework.

use crate::abi::{CallFromHost, GuestFunction};
use crate::dyld::{ConstantExports, HostConstant};
use crate::mem::{ConstVoidPtr, SafeRead};
use crate::objc::{id, msg_class, nil, objc_classes, ClassExports};

#[repr(C, packed)]
struct BlockLiteral {
    _isa: id,
    _flags: u32,
    _reserved: u32,
    invoke: GuestFunction,
}
unsafe impl SafeRead for BlockLiteral {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation GCController: NSObject

+ (id)controllers {
    msg_class![env; NSArray array]
}

+ (())startWirelessControllerDiscoveryWithCompletionHandler:(ConstVoidPtr)completion {
    log_once!("TODO: GCController wireless discovery is stubbed with no controllers");
    if !completion.is_null() {
        let block_literal: BlockLiteral = env.mem.read(completion.cast());
        let invoke = block_literal.invoke;
        let _: () = invoke.call_from_host(env, (completion,));
    }
}

+ (())stopWirelessControllerDiscovery {
}

+ (id)controllerWithExtendedGamepad {
    nil
}

+ (id)controllerWithMicroGamepad {
    nil
}

- (id)extendedGamepad {
    nil
}

- (id)gamepad {
    nil
}

- (id)microGamepad {
    nil
}

- (id)motion {
    nil
}

@end

};

pub const GCControllerDidConnectNotification: &str = "GCControllerDidConnectNotification";
pub const GCControllerDidDisconnectNotification: &str = "GCControllerDidDisconnectNotification";

pub const CONSTANTS: ConstantExports = &[
    (
        "_GCControllerDidConnectNotification",
        HostConstant::NSString(GCControllerDidConnectNotification),
    ),
    (
        "_GCControllerDidDisconnectNotification",
        HostConstant::NSString(GCControllerDidDisconnectNotification),
    ),
];

pub const DYLIB: crate::dyld::HostDylib = crate::dyld::HostDylib {
    path: "/System/Library/Frameworks/GameController.framework/GameController",
    aliases: &[],
    class_exports: &[CLASSES],
    constant_exports: &[CONSTANTS],
    function_exports: &[],
};
