/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIBarButtonItem`.

use crate::frameworks::core_graphics::CGFloat;
use crate::frameworks::foundation::NSInteger;
use crate::objc::{id, objc_classes, release, retain, ClassExports, HostObject, NSZonePtr, SEL};

struct UIBarButtonItemHostObject {
    title: id,
    custom_view: id,
    target: id,
    action: SEL,
    style: NSInteger,
    system_item: NSInteger,
    width: CGFloat,
    enabled: bool,
}
impl HostObject for UIBarButtonItemHostObject {}
impl Default for UIBarButtonItemHostObject {
    fn default() -> Self {
        Self {
            title: Default::default(),
            custom_view: Default::default(),
            target: Default::default(),
            action: SEL::null(),
            style: 0,
            system_item: 0,
            width: 0.0,
            enabled: true,
        }
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIBarButtonItem: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UIBarButtonItemHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithTitle:(id)title
             style:(NSInteger)style
             target:(id)target
             action:(SEL)action {
    retain(env, title);
    let host_obj = env.objc.borrow_mut::<UIBarButtonItemHostObject>(this);
    host_obj.title = title;
    host_obj.style = style;
    host_obj.target = target;
    host_obj.action = action;
    this
}

- (id)initWithBarButtonSystemItem:(NSInteger)system_item
                           target:(id)target
                           action:(SEL)action {
    let host_obj = env.objc.borrow_mut::<UIBarButtonItemHostObject>(this);
    host_obj.system_item = system_item;
    host_obj.target = target;
    host_obj.action = action;
    this
}

- (id)initWithCustomView:(id)custom_view {
    retain(env, custom_view);
    let host_obj = env.objc.borrow_mut::<UIBarButtonItemHostObject>(this);
    host_obj.custom_view = custom_view;
    this
}

- (())dealloc {
    let &UIBarButtonItemHostObject {
        title,
        custom_view,
        target: _,
        action: _,
        style: _,
        system_item: _,
        width: _,
        enabled: _,
    } = env.objc.borrow(this);
    release(env, title);
    release(env, custom_view);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)title {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).title
}

- (())setTitle:(id)title {
    retain(env, title);
    let old_title = {
        let host_obj = env.objc.borrow_mut::<UIBarButtonItemHostObject>(this);
        std::mem::replace(&mut host_obj.title, title)
    };
    release(env, old_title);
}

- (id)customView {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).custom_view
}

- (())setCustomView:(id)custom_view {
    retain(env, custom_view);
    let old_custom_view = {
        let host_obj = env.objc.borrow_mut::<UIBarButtonItemHostObject>(this);
        std::mem::replace(&mut host_obj.custom_view, custom_view)
    };
    release(env, old_custom_view);
}

- (id)target {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).target
}

- (())setTarget:(id)target {
    env.objc.borrow_mut::<UIBarButtonItemHostObject>(this).target = target;
}

- (SEL)action {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).action
}

- (())setAction:(SEL)action {
    env.objc.borrow_mut::<UIBarButtonItemHostObject>(this).action = action;
}

- (NSInteger)style {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).style
}

- (())setStyle:(NSInteger)style {
    env.objc.borrow_mut::<UIBarButtonItemHostObject>(this).style = style;
}

- (CGFloat)width {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).width
}

- (())setWidth:(CGFloat)width {
    env.objc.borrow_mut::<UIBarButtonItemHostObject>(this).width = width;
}

- (bool)isEnabled {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).enabled
}

- (())setEnabled:(bool)enabled {
    env.objc.borrow_mut::<UIBarButtonItemHostObject>(this).enabled = enabled;
}

@end

};
