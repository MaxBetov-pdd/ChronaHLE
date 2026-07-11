/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSCalendar`.

use crate::dyld::{ConstantExports, HostConstant};

pub const CONSTANTS: ConstantExports = &[
    (
        "_NSBuddhistCalendar",
        HostConstant::NSString("NSBuddhistCalendar"),
    ),
    (
        "_NSChineseCalendar",
        HostConstant::NSString("NSChineseCalendar"),
    ),
    (
        "_NSGregorianCalendar",
        HostConstant::NSString("NSGregorianCalendar"),
    ),
    (
        "_NSHebrewCalendar",
        HostConstant::NSString("NSHebrewCalendar"),
    ),
    (
        "_NSISO8601Calendar",
        HostConstant::NSString("NSISO8601Calendar"),
    ),
    (
        "_NSIndianCalendar",
        HostConstant::NSString("NSIndianCalendar"),
    ),
    (
        "_NSIslamicCalendar",
        HostConstant::NSString("NSIslamicCalendar"),
    ),
    (
        "_NSIslamicCivilCalendar",
        HostConstant::NSString("NSIslamicCivilCalendar"),
    ),
    (
        "_NSJapaneseCalendar",
        HostConstant::NSString("NSJapaneseCalendar"),
    ),
    (
        "_NSPersianCalendar",
        HostConstant::NSString("NSPersianCalendar"),
    ),
    (
        "_NSRepublicOfChinaCalendar",
        HostConstant::NSString("NSRepublicOfChinaCalendar"),
    ),
];
