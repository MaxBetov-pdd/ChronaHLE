/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Small, cooperative subset of libdispatch.

use crate::abi::{CallFromHost, GuestFunction};
use crate::dyld::{export_c_func, ConstantExports, FunctionExports, HostConstant};
use crate::mem::{ConstPtr, ConstVoidPtr, MutPtr, MutVoidPtr, SafeRead};
use crate::Environment;
use std::collections::HashMap;

const DISPATCH_ONCE_DONE: i32 = -1;
const DISPATCH_TIME_NOW: u64 = 0;
const DISPATCH_TIME_FOREVER: u64 = u64::MAX;

#[repr(C, packed)]
struct BlockLiteral {
    _isa: ConstVoidPtr,
    _flags: i32,
    _reserved: i32,
    invoke: GuestFunction,
    _descriptor: ConstVoidPtr,
}
unsafe impl SafeRead for BlockLiteral {}

struct QueueHostObject {
    label: MutPtr<u8>,
    global: bool,
    specifics: HashMap<ConstVoidPtr, ConstVoidPtr>,
}

struct SourceHostObject {
    _queue: MutVoidPtr,
    event_handler: ConstVoidPtr,
    cancelled: bool,
}

#[derive(Default)]
pub struct State {
    main_queue: Option<MutVoidPtr>,
    global_queues: HashMap<i32, MutVoidPtr>,
    queues: HashMap<MutVoidPtr, QueueHostObject>,
    current_queue: Option<MutVoidPtr>,
    semaphores: HashMap<MutVoidPtr, i64>,
    sources: HashMap<MutVoidPtr, SourceHostObject>,
    source_type_timer: Option<MutVoidPtr>,
}

fn new_queue(env: &mut Environment, label: &str, global: bool) -> MutVoidPtr {
    let queue = env.mem.alloc(4);
    let label = env.mem.alloc_and_write_cstr(label.as_bytes());
    env.libc_state.dispatch.queues.insert(
        queue,
        QueueHostObject {
            label,
            global,
            specifics: HashMap::new(),
        },
    );
    queue
}

fn main_queue(env: &mut Environment) -> MutVoidPtr {
    if let Some(queue) = env.libc_state.dispatch.main_queue {
        queue
    } else {
        let queue = new_queue(env, "com.apple.main-thread", true);
        env.libc_state.dispatch.main_queue = Some(queue);
        queue
    }
}

fn run_block(env: &mut Environment, queue: MutVoidPtr, block: ConstVoidPtr) {
    if block.is_null() {
        return;
    }
    let old_queue = env.libc_state.dispatch.current_queue.replace(queue);
    let block_literal: BlockLiteral = env.mem.read(block.cast());
    let invoke = block_literal.invoke;
    let _: () = invoke.call_from_host(env, (block,));
    env.libc_state.dispatch.current_queue = old_queue;
}

fn dispatch_once(env: &mut Environment, predicate: MutPtr<i32>, block: ConstVoidPtr) {
    let state = env.mem.read(predicate);
    match state {
        0 => {
            env.mem.write(predicate, DISPATCH_ONCE_DONE);
            let queue = main_queue(env);
            run_block(env, queue, block);
        }
        DISPATCH_ONCE_DONE => {}
        other => log_dbg!(
            "dispatch_once predicate {:?} had unexpected state {:#x}, treating as done",
            predicate,
            other
        ),
    }
}

fn dispatch_once_f(
    env: &mut Environment,
    predicate: MutPtr<i32>,
    context: MutVoidPtr,
    function: GuestFunction,
) {
    if env.mem.read(predicate) == 0 {
        env.mem.write(predicate, DISPATCH_ONCE_DONE);
        let _: () = function.call_from_host(env, (context,));
    }
}

fn dispatch_get_main_queue(env: &mut Environment) -> MutVoidPtr {
    main_queue(env)
}

fn dispatch_get_global_queue(env: &mut Environment, priority: i32, _flags: u32) -> MutVoidPtr {
    if let Some(&queue) = env.libc_state.dispatch.global_queues.get(&priority) {
        queue
    } else {
        let queue = new_queue(env, &format!("com.apple.root.default-qos.{priority}"), true);
        env.libc_state
            .dispatch
            .global_queues
            .insert(priority, queue);
        queue
    }
}

fn dispatch_get_current_queue(env: &mut Environment) -> MutVoidPtr {
    env.libc_state
        .dispatch
        .current_queue
        .unwrap_or_else(|| main_queue(env))
}

fn dispatch_queue_create(
    env: &mut Environment,
    label: ConstPtr<u8>,
    _attr: ConstVoidPtr,
) -> MutVoidPtr {
    let label = if label.is_null() {
        "".to_string()
    } else {
        env.mem.cstr_at_utf8(label).unwrap().to_string()
    };
    new_queue(env, &label, false)
}

fn dispatch_queue_get_label(env: &mut Environment, queue: MutVoidPtr) -> ConstPtr<u8> {
    let queue = if queue.is_null() {
        dispatch_get_current_queue(env)
    } else {
        queue
    };
    env.libc_state
        .dispatch
        .queues
        .get(&queue)
        .map(|queue| queue.label.cast_const())
        .unwrap_or(ConstPtr::null())
}

fn dispatch_sync(env: &mut Environment, queue: MutVoidPtr, block: ConstVoidPtr) {
    run_block(env, queue, block);
}

fn dispatch_async(env: &mut Environment, queue: MutVoidPtr, block: ConstVoidPtr) {
    run_block(env, queue, block);
}

fn dispatch_after(env: &mut Environment, _when: u64, queue: MutVoidPtr, block: ConstVoidPtr) {
    run_block(env, queue, block);
}

fn dispatch_time(_env: &mut Environment, when: u64, delta: i64) -> u64 {
    if when == DISPATCH_TIME_FOREVER {
        return DISPATCH_TIME_FOREVER;
    }
    let base = if when == DISPATCH_TIME_NOW { 0 } else { when };
    if delta >= 0 {
        base.saturating_add(delta as u64)
    } else {
        base.saturating_sub(delta.unsigned_abs())
    }
}

fn dispatch_queue_set_specific(
    env: &mut Environment,
    queue: MutVoidPtr,
    key: ConstVoidPtr,
    context: ConstVoidPtr,
    _destructor: GuestFunction,
) {
    if let Some(queue) = env.libc_state.dispatch.queues.get_mut(&queue) {
        if context.is_null() {
            queue.specifics.remove(&key);
        } else {
            queue.specifics.insert(key, context);
        }
    }
}

fn dispatch_get_specific(env: &mut Environment, key: ConstVoidPtr) -> ConstVoidPtr {
    let Some(queue) = env.libc_state.dispatch.current_queue else {
        return ConstVoidPtr::null();
    };
    env.libc_state
        .dispatch
        .queues
        .get(&queue)
        .and_then(|queue| queue.specifics.get(&key).copied())
        .unwrap_or(ConstVoidPtr::null())
}

fn dispatch_set_target_queue(_env: &mut Environment, _object: MutVoidPtr, _queue: MutVoidPtr) {}

fn dispatch_suspend(_env: &mut Environment, _object: MutVoidPtr) {}

fn dispatch_resume(_env: &mut Environment, _object: MutVoidPtr) {}

fn dispatch_release(env: &mut Environment, object: MutVoidPtr) {
    if let Some(queue) = env.libc_state.dispatch.queues.get(&object) {
        if queue.global {
            return;
        }
    }
    if let Some(queue) = env.libc_state.dispatch.queues.remove(&object) {
        env.mem.free(queue.label.cast());
        env.mem.free(object);
    } else if env.libc_state.dispatch.semaphores.remove(&object).is_some()
        || env.libc_state.dispatch.sources.remove(&object).is_some()
    {
        env.mem.free(object);
    }
}

fn dispatch_semaphore_create(env: &mut Environment, value: i64) -> MutVoidPtr {
    if value < 0 {
        return MutVoidPtr::null();
    }
    let semaphore = env.mem.alloc(4);
    env.libc_state.dispatch.semaphores.insert(semaphore, value);
    semaphore
}

fn dispatch_semaphore_signal(env: &mut Environment, semaphore: MutVoidPtr) -> i32 {
    let value = env
        .libc_state
        .dispatch
        .semaphores
        .get_mut(&semaphore)
        .unwrap();
    *value = value.saturating_add(1);
    0
}

fn dispatch_semaphore_wait(env: &mut Environment, semaphore: MutVoidPtr, _timeout: u64) -> i32 {
    let value = env
        .libc_state
        .dispatch
        .semaphores
        .get_mut(&semaphore)
        .unwrap();
    if *value > 0 {
        *value -= 1;
        0
    } else {
        1
    }
}

fn dispatch_source_create(
    env: &mut Environment,
    _source_type: ConstVoidPtr,
    _handle: u32,
    _mask: u32,
    queue: MutVoidPtr,
) -> MutVoidPtr {
    let source = env.mem.alloc(4);
    env.libc_state.dispatch.sources.insert(
        source,
        SourceHostObject {
            _queue: queue,
            event_handler: ConstVoidPtr::null(),
            cancelled: false,
        },
    );
    source
}

fn dispatch_source_set_event_handler(
    env: &mut Environment,
    source: MutVoidPtr,
    handler: ConstVoidPtr,
) {
    env.libc_state
        .dispatch
        .sources
        .get_mut(&source)
        .unwrap()
        .event_handler = handler;
}

fn dispatch_source_set_timer(
    _env: &mut Environment,
    _source: MutVoidPtr,
    _start: u64,
    _interval: u64,
    _leeway: u64,
) {
}

fn dispatch_source_cancel(env: &mut Environment, source: MutVoidPtr) {
    if let Some(source) = env.libc_state.dispatch.sources.get_mut(&source) {
        source.cancelled = true;
    }
}

fn get_dispatch_main_q(env: &mut Environment) -> ConstVoidPtr {
    main_queue(env).cast_const()
}

fn get_dispatch_source_type_timer(env: &mut Environment) -> ConstVoidPtr {
    if let Some(source_type) = env.libc_state.dispatch.source_type_timer {
        source_type.cast_const()
    } else {
        let source_type = env.mem.alloc(4);
        env.libc_state.dispatch.source_type_timer = Some(source_type);
        source_type.cast_const()
    }
}

pub const CONSTANTS: ConstantExports = &[
    (
        "__dispatch_main_q",
        HostConstant::Custom(get_dispatch_main_q),
    ),
    (
        "__dispatch_source_type_timer",
        HostConstant::Custom(get_dispatch_source_type_timer),
    ),
];

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(dispatch_once(_, _)),
    export_c_func!(dispatch_once_f(_, _, _)),
    export_c_func!(dispatch_get_main_queue()),
    export_c_func!(dispatch_get_global_queue(_, _)),
    export_c_func!(dispatch_get_current_queue()),
    export_c_func!(dispatch_queue_create(_, _)),
    export_c_func!(dispatch_queue_get_label(_)),
    export_c_func!(dispatch_sync(_, _)),
    export_c_func!(dispatch_async(_, _)),
    export_c_func!(dispatch_after(_, _, _)),
    export_c_func!(dispatch_time(_, _)),
    export_c_func!(dispatch_queue_set_specific(_, _, _, _)),
    export_c_func!(dispatch_get_specific(_)),
    export_c_func!(dispatch_set_target_queue(_, _)),
    export_c_func!(dispatch_suspend(_)),
    export_c_func!(dispatch_resume(_)),
    export_c_func!(dispatch_release(_)),
    export_c_func!(dispatch_semaphore_create(_)),
    export_c_func!(dispatch_semaphore_signal(_)),
    export_c_func!(dispatch_semaphore_wait(_, _)),
    export_c_func!(dispatch_source_create(_, _, _, _)),
    export_c_func!(dispatch_source_set_event_handler(_, _)),
    export_c_func!(dispatch_source_set_timer(_, _, _, _)),
    export_c_func!(dispatch_source_cancel(_)),
];
