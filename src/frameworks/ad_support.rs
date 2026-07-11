/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! AdSupport framework.

use crate::frameworks::foundation::ns_uuid;
use crate::objc::{id, objc_classes, ClassExports, TrivialHostObject};

#[derive(Default)]
pub struct State {
    shared_manager: Option<id>,
    advertising_identifier: Option<id>,
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation ASIdentifierManager: NSObject

+ (id)sharedManager {
    if let Some(manager) = env.framework_state.ad_support.shared_manager {
        manager
    } else {
        let manager = env.objc.alloc_static_object(
            this,
            Box::new(TrivialHostObject),
            &mut env.mem,
        );
        env.framework_state.ad_support.shared_manager = Some(manager);
        manager
    }
}

- (id)advertisingIdentifier {
    if let Some(identifier) = env.framework_state.ad_support.advertising_identifier {
        identifier
    } else {
        let identifier = ns_uuid::zero_uuid(env);
        env.framework_state.ad_support.advertising_identifier = Some(identifier);
        identifier
    }
}

- (bool)isAdvertisingTrackingEnabled {
    false
}

- (id)retain { this }
- (())release {}
- (id)autorelease { this }

@end

};

pub const DYLIB: crate::dyld::HostDylib = crate::dyld::HostDylib {
    path: "/System/Library/Frameworks/AdSupport.framework/AdSupport",
    aliases: &[],
    class_exports: &[CLASSES],
    constant_exports: &[],
    function_exports: &[],
};
