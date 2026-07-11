/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSOperationQueue`.

use super::NSInteger;
use crate::abi::{CallFromHost, GuestFunction};
use crate::mem::{ConstVoidPtr, SafeRead};
use crate::objc::{
    autorelease, id, msg, msg_send, nil, objc_classes, release, retain, ClassExports, HostObject,
    NSZonePtr,
};

struct NSOperationQueueHostObject {
    name: id,
    max_concurrent_operation_count: NSInteger,
    suspended: bool,
}
impl HostObject for NSOperationQueueHostObject {}

struct NSOperationHostObject {
    cancelled: bool,
    executing: bool,
    finished: bool,
    dependencies: id,
    completion_block: ConstVoidPtr,
    execution_blocks: Vec<ConstVoidPtr>,
    queue_priority: NSInteger,
}
impl HostObject for NSOperationHostObject {}

#[repr(C, packed)]
struct BlockLiteral {
    _isa: ConstVoidPtr,
    _flags: i32,
    _reserved: i32,
    invoke: GuestFunction,
    _descriptor: ConstVoidPtr,
}
unsafe impl SafeRead for BlockLiteral {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSOperation: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let array_class = env.objc.get_known_class("NSMutableArray", &mut env.mem);
    let dependencies: id = msg![env; array_class new];
    let host_object = Box::new(NSOperationHostObject {
        cancelled: false,
        executing: false,
        finished: false,
        dependencies,
        completion_block: ConstVoidPtr::null(),
        execution_blocks: Vec::new(),
        queue_priority: 0,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())start {
    if env.objc.borrow::<NSOperationHostObject>(this).finished {
        return;
    }
    if env.objc.borrow::<NSOperationHostObject>(this).cancelled {
        env.objc.borrow_mut::<NSOperationHostObject>(this).finished = true;
        return;
    }

    env.objc.borrow_mut::<NSOperationHostObject>(this).executing = true;
    () = msg![env; this main];
    let host = env.objc.borrow_mut::<NSOperationHostObject>(this);
    host.executing = false;
    host.finished = true;

    let completion_block = host.completion_block;
    if !completion_block.is_null() {
        let block_literal: BlockLiteral = env.mem.read(completion_block.cast());
        let invoke = block_literal.invoke;
        let _: () = invoke.call_from_host(env, (completion_block,));
    }
}

- (())main {
}

- (())cancel {
    env.objc.borrow_mut::<NSOperationHostObject>(this).cancelled = true;
}

- (bool)isCancelled {
    env.objc.borrow::<NSOperationHostObject>(this).cancelled
}

- (bool)isExecuting {
    env.objc.borrow::<NSOperationHostObject>(this).executing
}

- (bool)isFinished {
    env.objc.borrow::<NSOperationHostObject>(this).finished
}

- (bool)isConcurrent {
    false
}

- (bool)isAsynchronous {
    false
}

- (bool)isReady {
    let dependencies = env.objc.borrow::<NSOperationHostObject>(this).dependencies;
    let count: u32 = msg![env; dependencies count];
    for i in 0..count {
        let dependency: id = msg![env; dependencies objectAtIndex:i];
        let finished: bool = msg![env; dependency isFinished];
        if !finished {
            return false;
        }
    }
    true
}

- (())addDependency:(id)operation {
    let dependencies = env.objc.borrow::<NSOperationHostObject>(this).dependencies;
    () = msg![env; dependencies addObject:operation];
}

- (())removeDependency:(id)operation {
    let dependencies = env.objc.borrow::<NSOperationHostObject>(this).dependencies;
    () = msg![env; dependencies removeObject:operation];
}

- (id)dependencies {
    env.objc.borrow::<NSOperationHostObject>(this).dependencies
}

- (ConstVoidPtr)completionBlock {
    env.objc.borrow::<NSOperationHostObject>(this).completion_block
}

- (())setCompletionBlock:(ConstVoidPtr)block {
    env.objc.borrow_mut::<NSOperationHostObject>(this).completion_block = block;
}

- (NSInteger)queuePriority {
    env.objc.borrow::<NSOperationHostObject>(this).queue_priority
}

- (())setQueuePriority:(NSInteger)priority {
    env.objc.borrow_mut::<NSOperationHostObject>(this).queue_priority = priority;
}

- (())waitUntilFinished {
}

- (())dealloc {
    let dependencies = env.objc.borrow::<NSOperationHostObject>(this).dependencies;
    release(env, dependencies);
    env.objc.dealloc_object(this, &mut env.mem)
}

@end

@implementation NSBlockOperation: NSOperation

+ (id)blockOperationWithBlock:(ConstVoidPtr)block {
    let operation: id = msg![env; this alloc];
    let operation: id = msg![env; operation init];
    () = msg![env; operation addExecutionBlock:block];
    autorelease(env, operation)
}

- (())addExecutionBlock:(ConstVoidPtr)block {
    if !block.is_null() {
        env.objc
            .borrow_mut::<NSOperationHostObject>(this)
            .execution_blocks
            .push(block);
    }
}

- (())main {
    let blocks = env
        .objc
        .borrow::<NSOperationHostObject>(this)
        .execution_blocks
        .clone();
    for block in blocks {
        if env.objc.borrow::<NSOperationHostObject>(this).cancelled {
            break;
        }
        let block_literal: BlockLiteral = env.mem.read(block.cast());
        let invoke = block_literal.invoke;
        let _: () = invoke.call_from_host(env, (block,));
    }
}

@end

@implementation NSOperationQueue: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSOperationQueueHostObject {
        name: nil,
        max_concurrent_operation_count: -1,
        suspended: false,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)mainQueue {
    let queue: id = msg![env; this alloc];
    let queue: id = msg![env; queue init];
    autorelease(env, queue)
}

+ (id)currentQueue {
    nil
}

- (id)init {
    this
}

- (())addOperationWithBlock:(ConstVoidPtr)block {
    if block.is_null() {
        return;
    }
    let block_literal: BlockLiteral = env.mem.read(block.cast());
    let invoke = block_literal.invoke;
    let _: () = invoke.call_from_host(env, (block,));
}

- (())addOperation:(id)operation {
    if operation == nil {
        return;
    }
    let start = env.objc.lookup_selector("start").unwrap();
    if env.objc.object_has_method(&env.mem, operation, start) {
        () = msg_send(env, (operation, start));
    } else {
        log!(
            "TODO: [(NSOperationQueue *){:?} addOperation:{:?}] could not start operation",
            this,
            operation
        );
    }
}

- (())addOperations:(id)operations // NSArray *
     waitUntilFinished:(bool)wait {
    let count: u32 = msg![env; operations count];
    for i in 0..count {
        let operation: id = msg![env; operations objectAtIndex:i];
        () = msg![env; this addOperation:operation];
    }
    if wait {
        () = msg![env; this waitUntilAllOperationsAreFinished];
    }
}

- (())waitUntilAllOperationsAreFinished {
}

- (())cancelAllOperations {
    log_dbg!("TODO: [(NSOperationQueue *){:?} cancelAllOperations]", this);
}

- (u32)operationCount {
    0
}

- (NSInteger)maxConcurrentOperationCount {
    env.objc
        .borrow::<NSOperationQueueHostObject>(this)
        .max_concurrent_operation_count
}

- (())setMaxConcurrentOperationCount:(NSInteger)count {
    env.objc
        .borrow_mut::<NSOperationQueueHostObject>(this)
        .max_concurrent_operation_count = count;
}

- (bool)isSuspended {
    env.objc.borrow::<NSOperationQueueHostObject>(this).suspended
}

- (())setSuspended:(bool)suspended {
    env.objc.borrow_mut::<NSOperationQueueHostObject>(this).suspended = suspended;
}

- (id)name {
    env.objc.borrow::<NSOperationQueueHostObject>(this).name
}

- (())setName:(id)name {
    let old_name = env.objc.borrow::<NSOperationQueueHostObject>(this).name;
    retain(env, name);
    env.objc.borrow_mut::<NSOperationQueueHostObject>(this).name = name;
    release(env, old_name);
}

- (())dealloc {
    let host_object = env.objc.borrow::<NSOperationQueueHostObject>(this);
    release(env, host_object.name);
    env.objc.dealloc_object(this, &mut env.mem)
}

@end

};
