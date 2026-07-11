/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIWebView`.

use crate::frameworks::foundation::ns_string::{get_static_str, to_rust_string};
use crate::msg;
use crate::objc::{id, nil, objc_classes, ClassExports};
use std::borrow::Cow;

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIWebView: UIView

// NSCoding implementation
- (id)initWithCoder:(id)_coder {
    this
}

- (bool)scalesPageToFit {
    false
}
- (())setScalesPageToFit:(bool)_scales {
}
- (())setDelegate:(id)_delegate {
}
- (())loadRequest:(id)request { // NSURLRequest*
    let url_string = if request != nil {
        let url = msg![env; request URL];
        let url_desc = msg![env; url description];
        to_rust_string(env, url_desc)
    } else {
        Cow::default()
    };
    log!("TODO: [(UIWebView*) {:?} loadRequest:{:?} ({})]", this, request, url_string);
}

- (())loadHTMLString:(id)html // NSString *
               baseURL:(id)base_url { // NSURL *
    log!(
        "TODO: [(UIWebView*) {:?} loadHTMLString:<{} chars> baseURL:{:?}]",
        this,
        to_rust_string(env, html).chars().count(),
        base_url,
    );
}

- (id)stringByEvaluatingJavaScriptFromString:(id)_script {
    log_once!("TODO: UIWebView JavaScript evaluation returns an empty string");
    get_static_str(env, "")
}

- (id)request {
    nil
}

- (bool)isLoading {
    false
}

- (())stopLoading {
}

- (())reload {
}

- (bool)canGoBack {
    false
}

- (bool)canGoForward {
    false
}

- (())goBack {
}

- (())goForward {
}

- (u32)dataDetectorTypes {
    0
}

- (())setDataDetectorTypes:(u32)_types {
}

- (bool)allowsInlineMediaPlayback {
    false
}

- (())setAllowsInlineMediaPlayback:(bool)_allows {
}

- (bool)mediaPlaybackRequiresUserAction {
    true
}

- (())setMediaPlaybackRequiresUserAction:(bool)_requires {
}

@end

};
