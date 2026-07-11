/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIToolbar`.

use crate::frameworks::foundation::NSInteger;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_super, objc_classes, release, retain,
    ClassExports, NSZonePtr,
};

#[derive(Default)]
struct UIToolbarHostObject {
    superclass: super::UIViewHostObject,
    items: id,
    tint_color: id,
    bar_style: NSInteger,
}
impl_HostObject_with_superclass!(UIToolbarHostObject);

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIToolbar: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UIToolbarHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())dealloc {
    let &UIToolbarHostObject {
        superclass: _,
        items,
        tint_color,
        bar_style: _,
    } = env.objc.borrow(this);
    release(env, items);
    release(env, tint_color);
    msg_super![env; this dealloc]
}

- (id)items {
    env.objc.borrow::<UIToolbarHostObject>(this).items
}

- (())setItems:(id)items {
    let host_obj = env.objc.borrow_mut::<UIToolbarHostObject>(this);
    let old_items = std::mem::replace(&mut host_obj.items, items);
    retain(env, items);
    release(env, old_items);
}

- (())setItems:(id)items animated:(bool)_animated {
    () = msg![env; this setItems:items];
}

- (NSInteger)barStyle {
    env.objc.borrow::<UIToolbarHostObject>(this).bar_style
}

- (())setBarStyle:(NSInteger)bar_style {
    env.objc.borrow_mut::<UIToolbarHostObject>(this).bar_style = bar_style;
}

- (id)tintColor {
    env.objc.borrow::<UIToolbarHostObject>(this).tint_color
}

- (())setTintColor:(id)tint_color {
    let host_obj = env.objc.borrow_mut::<UIToolbarHostObject>(this);
    let old_tint_color = std::mem::replace(&mut host_obj.tint_color, tint_color);
    retain(env, tint_color);
    release(env, old_tint_color);
}

@end

};
