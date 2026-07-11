/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! CoreTelephony framework.

use crate::dyld::{ConstantExports, HostConstant};
use crate::objc::{id, nil, objc_classes, ClassExports};

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation CTTelephonyNetworkInfo: NSObject

- (id)subscriberCellularProvider {
    nil
}

- (id)currentRadioAccessTechnology {
    nil
}

@end

@implementation CTCarrier: NSObject

- (id)carrierName {
    nil
}

- (id)isoCountryCode {
    nil
}

- (id)mobileCountryCode {
    nil
}

- (id)mobileNetworkCode {
    nil
}

- (bool)allowsVOIP {
    false
}

@end

};

pub const CONSTANTS: ConstantExports = &[(
    "_CTRadioAccessTechnologyDidChangeNotification",
    HostConstant::NSString("CTRadioAccessTechnologyDidChangeNotification"),
)];

pub const DYLIB: crate::dyld::HostDylib = crate::dyld::HostDylib {
    path: "/System/Library/Frameworks/CoreTelephony.framework/CoreTelephony",
    aliases: &[],
    class_exports: &[CLASSES],
    constant_exports: &[CONSTANTS],
    function_exports: &[],
};
