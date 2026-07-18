/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIAlertView`.

use crate::frameworks::foundation::{ns_string, NSInteger};
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_super, nil, objc_classes, release, ClassExports,
    NSZonePtr,
};
use crate::Environment;

struct UIAlertViewHostObject {
    superclass: super::UIViewHostObject,
    title: id,
    message: id,
    delegate: id,
    buttons: Vec<id>,
    cancel_button_index: NSInteger,
    visible: bool,
    style: NSInteger,
}
impl_HostObject_with_superclass!(UIAlertViewHostObject);

impl Default for UIAlertViewHostObject {
    fn default() -> Self {
        Self {
            superclass: Default::default(),
            title: nil,
            message: nil,
            delegate: nil,
            buttons: Vec::new(),
            cancel_button_index: -1,
            visible: false,
            style: 0,
        }
    }
}

fn delegate_responds(env: &mut Environment, delegate: id, selector_name: &str) -> bool {
    if delegate == nil {
        return false;
    }
    let Some(selector) = env.objc.lookup_selector(selector_name) else {
        return false;
    };
    msg![env; delegate respondsToSelector:selector]
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIAlertView: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UIAlertViewHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithTitle:(id)title
                      message:(id)message
                     delegate:(id)delegate
            cancelButtonTitle:(id)cancelButtonTitle
            otherButtonTitles:(id)otherButtonTitles {
    let this: id = msg_super![env; this init];
    () = msg![env; this setTitle:title];
    () = msg![env; this setMessage:message];
    () = msg![env; this setDelegate:delegate];

    if cancelButtonTitle != nil {
        let cancel_index: NSInteger = msg![env; this addButtonWithTitle:cancelButtonTitle];
        env.objc.borrow_mut::<UIAlertViewHostObject>(this).cancel_button_index = cancel_index;
    }
    // Objective-C varargs expose the first other title through this argument.
    // Additional titles normally arrive through repeated addButtonWithTitle: calls.
    if otherButtonTitles != nil {
        let _: NSInteger = msg![env; this addButtonWithTitle:otherButtonTitles];
    }
    this
}

- (())dealloc {
    let (title, message, buttons) = {
        let host = env.objc.borrow::<UIAlertViewHostObject>(this);
        (host.title, host.message, host.buttons.clone())
    };
    release(env, title);
    release(env, message);
    for button in buttons {
        release(env, button);
    }
    msg_super![env; this dealloc]
}

- (id)title {
    env.objc.borrow::<UIAlertViewHostObject>(this).title
}

- (())setTitle:(id)title {
    let title: id = if title == nil { nil } else { msg![env; title copy] };
    let old_title = std::mem::replace(
        &mut env.objc.borrow_mut::<UIAlertViewHostObject>(this).title,
        title,
    );
    release(env, old_title);
}

- (id)message {
    env.objc.borrow::<UIAlertViewHostObject>(this).message
}

- (())setMessage:(id)message {
    let message: id = if message == nil { nil } else { msg![env; message copy] };
    let old_message = std::mem::replace(
        &mut env.objc.borrow_mut::<UIAlertViewHostObject>(this).message,
        message,
    );
    release(env, old_message);
}

- (id)delegate {
    env.objc.borrow::<UIAlertViewHostObject>(this).delegate
}

- (())setDelegate:(id)delegate {
    // UIKit's UIAlertView delegate property is assign, not retained.
    env.objc.borrow_mut::<UIAlertViewHostObject>(this).delegate = delegate;
}

- (NSInteger)addButtonWithTitle:(id)title {
    let title: id = msg![env; title copy];
    let host = env.objc.borrow_mut::<UIAlertViewHostObject>(this);
    let index = host.buttons.len().try_into().unwrap();
    host.buttons.push(title);
    index
}

- (NSInteger)numberOfButtons {
    env.objc.borrow::<UIAlertViewHostObject>(this).buttons.len().try_into().unwrap()
}

- (id)buttonTitleAtIndex:(NSInteger)index {
    let Ok(index) = usize::try_from(index) else { return nil };
    env.objc
        .borrow::<UIAlertViewHostObject>(this)
        .buttons
        .get(index)
        .copied()
        .unwrap_or(nil)
}

- (NSInteger)cancelButtonIndex {
    env.objc.borrow::<UIAlertViewHostObject>(this).cancel_button_index
}

- (NSInteger)firstOtherButtonIndex {
    let host = env.objc.borrow::<UIAlertViewHostObject>(this);
    (0..host.buttons.len())
        .find(|&index| NSInteger::try_from(index).unwrap() != host.cancel_button_index)
        .and_then(|index| NSInteger::try_from(index).ok())
        .unwrap_or(-1)
}

- (bool)isVisible {
    env.objc.borrow::<UIAlertViewHostObject>(this).visible
}

- (NSInteger)alertViewStyle {
    env.objc.borrow::<UIAlertViewHostObject>(this).style
}

- (())setAlertViewStyle:(NSInteger)style {
    env.objc.borrow_mut::<UIAlertViewHostObject>(this).style = style;
}

- (())show {
    let (title, message, buttons, delegate, cancel_button_index) = {
        let host = env.objc.borrow::<UIAlertViewHostObject>(this);
        (host.title, host.message, host.buttons.clone(), host.delegate, host.cancel_button_index)
    };
    let title = if title == nil {
        crate::PRODUCT_NAME.to_string()
    } else {
        ns_string::to_rust_string(env, title).into_owned()
    };
    let message = if message == nil {
        String::new()
    } else {
        ns_string::to_rust_string(env, message).into_owned()
    };
    let button_titles = buttons
        .into_iter()
        .map(|button| ns_string::to_rust_string(env, button).into_owned())
        .collect::<Vec<_>>();

    env.objc.borrow_mut::<UIAlertViewHostObject>(this).visible = true;
    if delegate_responds(env, delegate, "willPresentAlertView:") {
        () = msg![env; delegate willPresentAlertView:this];
    }
    if delegate_responds(env, delegate, "didPresentAlertView:") {
        () = msg![env; delegate didPresentAlertView:this];
    }

    let clicked_index = crate::window::show_alert_messagebox(env, &title, &message, &button_titles)
        .unwrap_or_else(|| if cancel_button_index >= 0 { cancel_button_index } else { 0 });

    if delegate_responds(env, delegate, "alertView:clickedButtonAtIndex:") {
        () = msg![env; delegate alertView:this clickedButtonAtIndex:clicked_index];
    }
    if delegate_responds(env, delegate, "alertView:willDismissWithButtonIndex:") {
        () = msg![env; delegate alertView:this willDismissWithButtonIndex:clicked_index];
    }
    env.objc.borrow_mut::<UIAlertViewHostObject>(this).visible = false;
    if delegate_responds(env, delegate, "alertView:didDismissWithButtonIndex:") {
        () = msg![env; delegate alertView:this didDismissWithButtonIndex:clicked_index];
    }
}

- (())dismissWithClickedButtonIndex:(NSInteger)button_index animated:(bool)_animated {
    let delegate = env.objc.borrow::<UIAlertViewHostObject>(this).delegate;
    if delegate_responds(env, delegate, "alertView:willDismissWithButtonIndex:") {
        () = msg![env; delegate alertView:this willDismissWithButtonIndex:button_index];
    }
    env.objc.borrow_mut::<UIAlertViewHostObject>(this).visible = false;
    if delegate_responds(env, delegate, "alertView:didDismissWithButtonIndex:") {
        () = msg![env; delegate alertView:this didDismissWithButtonIndex:button_index];
    }
}

@end

};
