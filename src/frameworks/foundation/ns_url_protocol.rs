/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSURLProtocol`.

use super::ns_url_request;
use crate::objc::{id, nil, objc_classes, release, retain, ClassExports, HostObject, NSZonePtr};

#[derive(Default)]
pub struct State {
    registered_classes: Vec<id>,
}

struct NSURLProtocolHostObject {
    request: id,
    cached_response: id,
    client: id,
}
impl HostObject for NSURLProtocolHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSURLProtocol: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    env.objc.alloc_object(
        this,
        Box::new(NSURLProtocolHostObject {
            request: nil,
            cached_response: nil,
            client: nil,
        }),
        &mut env.mem,
    )
}

+ (bool)registerClass:(id)protocol_class {
    let classes = &mut env.framework_state.foundation.ns_url_protocol.registered_classes;
    if !classes.contains(&protocol_class) {
        classes.push(protocol_class);
    }
    true
}

+ (())unregisterClass:(id)protocol_class {
    env.framework_state
        .foundation
        .ns_url_protocol
        .registered_classes
        .retain(|&class| class != protocol_class);
}

+ (bool)canInitWithRequest:(id)_request {
    false
}

+ (id)canonicalRequestForRequest:(id)request {
    request
}

+ (bool)requestIsCacheEquivalent:(id)first
                         toRequest:(id)second {
    first == second
}

+ (())setProperty:(id)value
            forKey:(id)key
         inRequest:(id)request {
    ns_url_request::set_protocol_property(env, request, key, value);
}

+ (id)propertyForKey:(id)key
             inRequest:(id)request {
    ns_url_request::protocol_property(env, request, key)
}

+ (())removePropertyForKey:(id)key
                    inRequest:(id)request {
    ns_url_request::remove_protocol_property(env, request, key);
}

- (id)initWithRequest:(id)request
        cachedResponse:(id)cached_response
                client:(id)client {
    let request = retain(env, request);
    let cached_response = retain(env, cached_response);
    let client = retain(env, client);
    let host = env.objc.borrow_mut::<NSURLProtocolHostObject>(this);
    host.request = request;
    host.cached_response = cached_response;
    host.client = client;
    this
}

- (id)request {
    env.objc.borrow::<NSURLProtocolHostObject>(this).request
}

- (id)cachedResponse {
    env.objc.borrow::<NSURLProtocolHostObject>(this).cached_response
}

- (id)client {
    env.objc.borrow::<NSURLProtocolHostObject>(this).client
}

- (())startLoading {
    log!("TODO: [(NSURLProtocol *){:?} startLoading]", this);
}

- (())stopLoading {
}

- (())dealloc {
    let host = env.objc.borrow::<NSURLProtocolHostObject>(this);
    let (request, cached_response, client) = (host.request, host.cached_response, host.client);
    release(env, request);
    release(env, cached_response);
    release(env, client);
    env.objc.dealloc_object(this, &mut env.mem);
}

@end

};
