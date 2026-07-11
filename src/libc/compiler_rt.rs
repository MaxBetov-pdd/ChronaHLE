/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Compiler runtime helpers used by some ARM builds.

use crate::dyld::{export_c_func, FunctionExports};
use crate::Environment;

fn __udivsi3(_env: &mut Environment, lhs: u32, rhs: u32) -> u32 {
    lhs / rhs
}

fn __umodsi3(_env: &mut Environment, lhs: u32, rhs: u32) -> u32 {
    lhs % rhs
}

fn __divsi3(_env: &mut Environment, lhs: i32, rhs: i32) -> i32 {
    lhs / rhs
}

fn __modsi3(_env: &mut Environment, lhs: i32, rhs: i32) -> i32 {
    lhs % rhs
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(__udivsi3(_, _)),
    export_c_func!(__umodsi3(_, _)),
    export_c_func!(__divsi3(_, _)),
    export_c_func!(__modsi3(_, _)),
];
