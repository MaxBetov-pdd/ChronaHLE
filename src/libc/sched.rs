/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `sched.h`.

use crate::dyld::{export_c_func, FunctionExports};
use crate::libc::errno::{set_errno, EINVAL};
use crate::Environment;

const SCHED_OTHER_LINUX: i32 = 0;
const SCHED_OTHER_DARWIN: i32 = 1;
const SCHED_RR_DARWIN: i32 = 2;
const SCHED_FIFO_DARWIN: i32 = 4;

fn is_known_policy(policy: i32) -> bool {
    matches!(
        policy,
        SCHED_OTHER_LINUX | SCHED_OTHER_DARWIN | SCHED_RR_DARWIN | SCHED_FIFO_DARWIN
    )
}

fn sched_yield(env: &mut Environment) -> i32 {
    log_dbg!(
        "TODO: thread {} requested processor yield, ignoring",
        env.current_thread
    );
    0 // success
}

fn sched_get_priority_min(env: &mut Environment, policy: i32) -> i32 {
    if !is_known_policy(policy) {
        set_errno(env, EINVAL);
        return -1;
    }

    log_dbg!(
        "TODO: sched_get_priority_min({}) returning guest-compatible default",
        policy
    );
    0
}

fn sched_get_priority_max(env: &mut Environment, policy: i32) -> i32 {
    if !is_known_policy(policy) {
        set_errno(env, EINVAL);
        return -1;
    }

    log_dbg!(
        "TODO: sched_get_priority_max({}) returning guest-compatible default",
        policy
    );
    31
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(sched_yield()),
    export_c_func!(sched_get_priority_min(_)),
    export_c_func!(sched_get_priority_max(_)),
];
