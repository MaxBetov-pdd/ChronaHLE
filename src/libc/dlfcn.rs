/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `dlfcn.h` (`dlopen()` and friends)

use crate::dyld::{export_c_func, FunctionExports};
use crate::mem::{ConstPtr, MutVoidPtr, Ptr};
use crate::Environment;

const RTLD_DEFAULT: MutVoidPtr = Ptr::from_bits(-2 as _);

fn is_known_library(path: &str) -> bool {
    crate::dyld::DYLIB_LIST
        .iter()
        .any(|dylib| dylib.path == path || dylib.aliases.contains(&path))
}

fn dlopen(env: &mut Environment, path: ConstPtr<u8>, _mode: i32) -> MutVoidPtr {
    if path.is_null() {
        return RTLD_DEFAULT;
    }
    // TODO: dlopen() support for real dynamic libraries.
    assert!(is_known_library(env.mem.cstr_at_utf8(path).unwrap()));
    // For convenience, use the path as the handle.
    // TODO: Find out whether the handle is truly opaque on iPhone OS, and if
    // not, where it points.
    path.cast_mut().cast()
}

fn dlsym(env: &mut Environment, handle: MutVoidPtr, symbol: ConstPtr<u8>) -> MutVoidPtr {
    assert!(
        handle == RTLD_DEFAULT || is_known_library(env.mem.cstr_at_utf8(handle.cast()).unwrap())
    );
    // For some reason, the symbols passed to dlsym() don't have the leading _.
    let symbol = format!("_{}", env.mem.cstr_at_utf8(symbol).unwrap());
    // TODO: error handling. dlsym() should just return NULL in this case, but
    // currently it's probably more useful to have the emulator crash if there's
    // no symbol found, since it most likely indicates a missing host function.
    // TODO: Symbol lookup should be scoped to the specific library requested,
    // where appropriate!
    let addr = env
        .dyld
        .create_proc_address(&mut env.mem, &mut env.cpu, &symbol)
        .unwrap_or_else(|_| panic!("dlsym() for unimplemented function {symbol}"));
    Ptr::from_bits(addr.addr_with_thumb_bit())
}

fn dlclose(env: &mut Environment, handle: MutVoidPtr) -> i32 {
    assert!(
        handle == RTLD_DEFAULT || is_known_library(env.mem.cstr_at_utf8(handle.cast()).unwrap())
    );
    0 // success
}

fn _chartBoostInit(_env: &mut Environment, _app_id: ConstPtr<u8>, _app_signature: ConstPtr<u8>) {
    log!("TODO: __chartBoostInit(...) ignored");
}

fn _chartBoostShowInterstitial(_env: &mut Environment, _location: ConstPtr<u8>) {
    log_dbg!("TODO: __chartBoostShowInterstitial(...) ignored");
}

fn _chartBoostCacheInterstitial(_env: &mut Environment, _location: ConstPtr<u8>) {
    log_dbg!("TODO: __chartBoostCacheInterstitial(...) ignored");
}

fn _chartBoostHasCachedInterstitial(_env: &mut Environment, _location: ConstPtr<u8>) -> bool {
    log_dbg!("TODO: __chartBoostHasCachedInterstitial(...) -> false");
    false
}

fn _chartBoostShowMoreApps(_env: &mut Environment) {
    log_dbg!("TODO: __chartBoostShowMoreApps() ignored");
}

fn _chartBoostCacheMoreApps(_env: &mut Environment) {
    log_dbg!("TODO: __chartBoostCacheMoreApps() ignored");
}

fn _chartBoostHasCachedMoreApps(_env: &mut Environment) -> bool {
    log_dbg!("TODO: __chartBoostHasCachedMoreApps() -> false");
    false
}

fn _chartBoostSetShouldRequestInterstitialsInFirstSession(_env: &mut Environment, _value: bool) {
    log_dbg!("TODO: __chartBoostSetShouldRequestInterstitialsInFirstSession(...) ignored");
}

fn _chartBoostSetShouldDisplayLoadingViewForMoreApps(_env: &mut Environment, _value: bool) {
    log_dbg!("TODO: __chartBoostSetShouldDisplayLoadingViewForMoreApps(...) ignored");
}

fn _chartBoostSetShouldPauseClickForConfirmation(_env: &mut Environment, _value: bool) {
    log_dbg!("TODO: __chartBoostSetShouldPauseClickForConfirmation(...) ignored");
}

fn _chartBoostSetShouldDisplayMoreAppsOnLoad(_env: &mut Environment, _value: bool) {
    log_dbg!("TODO: __chartBoostSetShouldDisplayMoreAppsOnLoad(...) ignored");
}

fn _flurryStartSession(_env: &mut Environment, _api_key: ConstPtr<u8>) {
    log!("TODO: __flurryStartSession(...) ignored");
}

fn _flurryLogEvent(_env: &mut Environment, _event_name: ConstPtr<u8>) {
    log_dbg!("TODO: __flurryLogEvent(...) ignored");
}

fn _flurryLogEventWithParameters(
    _env: &mut Environment,
    _event_name: ConstPtr<u8>,
    _parameters: MutVoidPtr,
) {
    log_dbg!("TODO: __flurryLogEventWithParameters(...) ignored");
}

fn _flurryLogTimedEvent(_env: &mut Environment, _event_name: ConstPtr<u8>) {
    log_dbg!("TODO: __flurryLogTimedEvent(...) ignored");
}

fn _flurryLogTimedEventWithParameters(
    _env: &mut Environment,
    _event_name: ConstPtr<u8>,
    _parameters: MutVoidPtr,
) {
    log_dbg!("TODO: __flurryLogTimedEventWithParameters(...) ignored");
}

fn _flurryEndTimedEvent(_env: &mut Environment, _event_name: ConstPtr<u8>) {
    log_dbg!("TODO: __flurryEndTimedEvent(...) ignored");
}

fn _flurryEndTimedEventWithParameters(
    _env: &mut Environment,
    _event_name: ConstPtr<u8>,
    _parameters: MutVoidPtr,
) {
    log_dbg!("TODO: __flurryEndTimedEventWithParameters(...) ignored");
}

fn _flurrySetUserID(_env: &mut Environment, _user_id: ConstPtr<u8>) {
    log_dbg!("TODO: __flurrySetUserID(...) ignored");
}

fn _flurrySetAge(_env: &mut Environment, _age: i32) {
    log_dbg!("TODO: __flurrySetAge(...) ignored");
}

fn _flurrySetGender(_env: &mut Environment, _gender: ConstPtr<u8>) {
    log_dbg!("TODO: __flurrySetGender(...) ignored");
}

fn _flurrySetSessionReportsOnCloseEnabled(_env: &mut Environment, _enabled: bool) {
    log_dbg!("TODO: __flurrySetSessionReportsOnCloseEnabled(...) ignored");
}

fn _flurrySetSessionReportsOnPauseEnabled(_env: &mut Environment, _enabled: bool) {
    log_dbg!("TODO: __flurrySetSessionReportsOnPauseEnabled(...) ignored");
}

fn _flurrySetSecureTransportEnabled(_env: &mut Environment, _enabled: bool) {
    log_dbg!("TODO: __flurrySetSecureTransportEnabled(...) ignored");
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(dlopen(_, _)),
    export_c_func!(dlsym(_, _)),
    export_c_func!(dlclose(_)),
    export_c_func!(_chartBoostInit(_, _)),
    export_c_func!(_chartBoostShowInterstitial(_)),
    export_c_func!(_chartBoostCacheInterstitial(_)),
    export_c_func!(_chartBoostHasCachedInterstitial(_)),
    export_c_func!(_chartBoostShowMoreApps()),
    export_c_func!(_chartBoostCacheMoreApps()),
    export_c_func!(_chartBoostHasCachedMoreApps()),
    export_c_func!(_chartBoostSetShouldRequestInterstitialsInFirstSession(_)),
    export_c_func!(_chartBoostSetShouldDisplayLoadingViewForMoreApps(_)),
    export_c_func!(_chartBoostSetShouldPauseClickForConfirmation(_)),
    export_c_func!(_chartBoostSetShouldDisplayMoreAppsOnLoad(_)),
    export_c_func!(_flurryStartSession(_)),
    export_c_func!(_flurryLogEvent(_)),
    export_c_func!(_flurryLogEventWithParameters(_, _)),
    export_c_func!(_flurryLogTimedEvent(_)),
    export_c_func!(_flurryLogTimedEventWithParameters(_, _)),
    export_c_func!(_flurryEndTimedEvent(_)),
    export_c_func!(_flurryEndTimedEventWithParameters(_, _)),
    export_c_func!(_flurrySetUserID(_)),
    export_c_func!(_flurrySetAge(_)),
    export_c_func!(_flurrySetGender(_)),
    export_c_func!(_flurrySetSessionReportsOnCloseEnabled(_)),
    export_c_func!(_flurrySetSessionReportsOnPauseEnabled(_)),
    export_c_func!(_flurrySetSecureTransportEnabled(_)),
];
