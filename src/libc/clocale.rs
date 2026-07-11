/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `clocale.h`

use std::collections::{hash_map::Entry, HashMap};

use crate::dyld::{ConstantExports, FunctionExports, HostConstant};
use crate::environment::Environment;
use crate::export_c_func;
use crate::mem::{ConstPtr, ConstVoidPtr, MutPtr};

pub type LocaleCategory = i32;
pub const LC_ALL: LocaleCategory = 0;
pub const LC_COLLATE: LocaleCategory = 1;
pub const LC_CTYPE: LocaleCategory = 2;
pub const LC_MONETARY: LocaleCategory = 3;
pub const LC_NUMERIC: LocaleCategory = 4;
pub const LC_TIME: LocaleCategory = 5;
pub const LC_MESSAGES: LocaleCategory = 6;

pub type LocaleInfoItem = i32;
pub const CODESET: LocaleInfoItem = 0;
pub const D_T_FMT: LocaleInfoItem = 1;
pub const D_FMT: LocaleInfoItem = 2;
pub const T_FMT: LocaleInfoItem = 3;
pub const T_FMT_AMPM: LocaleInfoItem = 4;
pub const AM_STR: LocaleInfoItem = 5;
pub const PM_STR: LocaleInfoItem = 6;
pub const DAY_1: LocaleInfoItem = 7;
pub const ABDAY_1: LocaleInfoItem = 14;
pub const MON_1: LocaleInfoItem = 21;
pub const ABMON_1: LocaleInfoItem = 33;
pub const ERA: LocaleInfoItem = 45;
pub const ERA_D_FMT: LocaleInfoItem = 46;
pub const ERA_D_T_FMT: LocaleInfoItem = 47;
pub const ERA_T_FMT: LocaleInfoItem = 48;
pub const ALT_DIGITS: LocaleInfoItem = 49;
pub const RADIXCHAR: LocaleInfoItem = 50;
pub const THOUSEP: LocaleInfoItem = 51;
pub const YESEXPR: LocaleInfoItem = 52;
pub const NOEXPR: LocaleInfoItem = 53;
pub const YESSTR: LocaleInfoItem = 54;
pub const NOSTR: LocaleInfoItem = 55;
pub const CRNCYSTR: LocaleInfoItem = 56;
pub const D_MD_ORDER: LocaleInfoItem = 57;

#[derive(Default)]
pub struct State {
    locale: HashMap<LocaleCategory, MutPtr<u8>>,
    langinfo: HashMap<LocaleInfoItem, MutPtr<u8>>,
}

pub fn setlocale(
    env: &mut Environment,
    category: LocaleCategory,
    locale: ConstPtr<u8>,
) -> MutPtr<u8> {
    assert!(matches!(
        category,
        LC_ALL | LC_COLLATE | LC_CTYPE | LC_MONETARY | LC_NUMERIC | LC_TIME | LC_MESSAGES
    ));
    if !locale.is_null() {
        // TODO: Handle empty locale string and ensure the combination of
        // category and locale is valid.
        let locale_cstr = env.mem.cstr_at(locale).to_owned();
        assert_ne!(locale_cstr.len(), 0);
        let new_locale = env.mem.alloc_and_write_cstr(locale_cstr.as_slice());
        if let Some(old_locale) = env.libc_state.clocale.locale.insert(category, new_locale) {
            env.mem.free(old_locale.cast())
        };
    } else if let Entry::Vacant(entry) = env.libc_state.clocale.locale.entry(category) {
        let default_locale = env.mem.alloc_and_write_cstr(b"C");
        entry.insert(default_locale);
    }
    env.libc_state.clocale.locale.get(&category).unwrap().cast()
}

fn langinfo_value(item: LocaleInfoItem) -> &'static [u8] {
    const DAYS: [&[u8]; 7] = [
        b"Sunday",
        b"Monday",
        b"Tuesday",
        b"Wednesday",
        b"Thursday",
        b"Friday",
        b"Saturday",
    ];
    const ABBR_DAYS: [&[u8]; 7] = [b"Sun", b"Mon", b"Tue", b"Wed", b"Thu", b"Fri", b"Sat"];
    const MONTHS: [&[u8]; 12] = [
        b"January",
        b"February",
        b"March",
        b"April",
        b"May",
        b"June",
        b"July",
        b"August",
        b"September",
        b"October",
        b"November",
        b"December",
    ];
    const ABBR_MONTHS: [&[u8]; 12] = [
        b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun", b"Jul", b"Aug", b"Sep", b"Oct", b"Nov",
        b"Dec",
    ];

    match item {
        CODESET => b"UTF-8",
        D_T_FMT => b"%a %b %e %H:%M:%S %Y",
        D_FMT => b"%m/%d/%y",
        T_FMT => b"%H:%M:%S",
        T_FMT_AMPM => b"%I:%M:%S %p",
        AM_STR => b"AM",
        PM_STR => b"PM",
        DAY_1..=13 => DAYS[(item - DAY_1) as usize],
        ABDAY_1..=20 => ABBR_DAYS[(item - ABDAY_1) as usize],
        MON_1..=32 => MONTHS[(item - MON_1) as usize],
        ABMON_1..=44 => ABBR_MONTHS[(item - ABMON_1) as usize],
        RADIXCHAR => b".",
        THOUSEP => b"",
        YESEXPR => b"^[yY]",
        NOEXPR => b"^[nN]",
        YESSTR => b"yes",
        NOSTR => b"no",
        CRNCYSTR => b"",
        D_MD_ORDER => b"md",
        ERA | ERA_D_FMT | ERA_D_T_FMT | ERA_T_FMT | ALT_DIGITS => b"",
        _ => b"",
    }
}

fn nl_langinfo(env: &mut Environment, item: LocaleInfoItem) -> MutPtr<u8> {
    if let Some(info) = env.libc_state.clocale.langinfo.get(&item) {
        return *info;
    }

    let info = env.mem.alloc_and_write_cstr(langinfo_value(item));
    env.libc_state.clocale.langinfo.insert(item, info);
    info
}

fn get_mb_cur_max(env: &mut Environment) -> ConstVoidPtr {
    env.mem.alloc_and_write(1_i32).cast().cast_const()
}

pub const CONSTANTS: ConstantExports = &[("___mb_cur_max", HostConstant::Custom(get_mb_cur_max))];

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(setlocale(_, _)),
    export_c_func!(nl_langinfo(_)),
];
