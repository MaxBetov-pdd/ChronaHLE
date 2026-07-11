/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! The `NSValue` class cluster, including `NSNumber`.

use super::ns_string::{from_rust_ordering, from_rust_string, to_rust_string};
use super::{
    _nib_archive_decoder, ns_keyed_unarchiver, NSComparisonResult, NSOrderedSame, NSUInteger,
};
use crate::abi::GuestArg;
use crate::frameworks::core_foundation::cf_number::{
    kCFNumberCharType, kCFNumberFloat32Type, kCFNumberFloatType, kCFNumberIntType,
    kCFNumberSInt16Type, kCFNumberSInt32Type, kCFNumberSInt8Type, kCFNumberShortType, CFNumberType,
};
use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_keyed_archiver::get_value_to_encode_for_current_key;
use crate::frameworks::foundation::NSInteger;
use crate::mem::{ConstVoidPtr, MutVoidPtr, SafeRead};
use crate::objc::{
    autorelease, id, impl_HostObject_with_superclass, msg, msg_class, nil, objc_classes, release,
    retain, Class, ClassExports, HostObject, NSZonePtr,
};
use crate::{impl_GuestRet_for_large_struct, Environment};
use std::cmp::Ordering;

#[derive(Debug)]
pub(super) enum NSValueHostObject {
    CGPoint(CGPoint),
    CGSize(CGSize),
    CGRect(CGRect),
}
impl HostObject for NSValueHostObject {}

macro_rules! impl_AsValue {
    ($method_name:tt, $typ:tt) => {
        pub fn $method_name(&self) -> $typ {
            match self {
                // Cast to u8 is needed for float conversions
                NSNumberHostObject::Bool(x) => *x as u8 as _,
                NSNumberHostObject::UnsignedLongLong(x) => *x as _,
                NSNumberHostObject::UnsignedInt(x) => *x as _,
                NSNumberHostObject::Int(x) => *x as _,
                NSNumberHostObject::LongLong(x) => *x as _,
                NSNumberHostObject::Float(x) => *x as _,
                NSNumberHostObject::Double(x) => *x as _,
                NSNumberHostObject::Short(x) => *x as _,
                NSNumberHostObject::UnsignedShort(x) => *x as _,
                NSNumberHostObject::Char(x) => *x as _,
            }
        }
    };
}

#[derive(Debug)]
pub(super) enum NSNumberHostObject {
    Bool(bool),
    UnsignedLongLong(u64),
    UnsignedInt(u32),
    Int(i32), // Also covers Integer and Long since this is a 32-bit platform.
    LongLong(i64),
    Float(f32),
    Double(f64),
    Short(i16),
    UnsignedShort(u16),
    Char(i8),
}
impl HostObject for NSNumberHostObject {}

impl NSNumberHostObject {
    fn as_bool(&self) -> bool {
        match self {
            NSNumberHostObject::Bool(x) => *x,
            NSNumberHostObject::UnsignedLongLong(x) => *x != 0,
            NSNumberHostObject::UnsignedInt(x) => *x != 0,
            NSNumberHostObject::Int(x) => *x != 0,
            NSNumberHostObject::LongLong(x) => *x != 0,
            NSNumberHostObject::Float(x) => *x != 0.0,
            NSNumberHostObject::Double(x) => *x != 0.0,
            NSNumberHostObject::Short(x) => *x != 0,
            NSNumberHostObject::UnsignedShort(x) => *x != 0,
            NSNumberHostObject::Char(x) => *x != 0,
        }
    }
    fn is_float(&self) -> bool {
        matches!(
            self,
            NSNumberHostObject::Float(_) | NSNumberHostObject::Double(_)
        )
    }
    impl_AsValue!(as_int, i32);
    impl_AsValue!(as_long_long, i64);
    impl_AsValue!(as_unsigned_long_long, u64);
    impl_AsValue!(as_unsigned_int, u32);
    impl_AsValue!(as_float, f32);
    impl_AsValue!(as_double, f64);
    impl_AsValue!(as_short, i16);
    impl_AsValue!(as_unsigned_short, u16);
    impl_AsValue!(as_char, i8);
    impl_AsValue!(as_i128, i128);
}

#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(C, packed)]
struct NSDecimal {
    fields: u32,
    mantissa: [u16; 8],
}
unsafe impl SafeRead for NSDecimal {}
impl_GuestRet_for_large_struct!(NSDecimal);
impl GuestArg for NSDecimal {
    const REG_COUNT: usize = 5;

    fn from_regs(regs: &[u32]) -> Self {
        let mut mantissa = [0; 8];
        for (word, pair) in regs[1..].iter().zip(mantissa.chunks_exact_mut(2)) {
            let bytes = word.to_le_bytes();
            pair[0] = u16::from_le_bytes(bytes[0..2].try_into().unwrap());
            pair[1] = u16::from_le_bytes(bytes[2..4].try_into().unwrap());
        }
        Self {
            fields: regs[0],
            mantissa,
        }
    }

    fn to_regs(self, regs: &mut [u32]) {
        let NSDecimal { fields, mantissa } = self;
        regs[0] = fields;
        for (word, pair) in regs[1..].iter_mut().zip(mantissa.chunks_exact(2)) {
            let mut bytes = [0; 4];
            bytes[0..2].copy_from_slice(&pair[0].to_le_bytes());
            bytes[2..4].copy_from_slice(&pair[1].to_le_bytes());
            *word = u32::from_le_bytes(bytes);
        }
    }
}

#[derive(Clone, Debug)]
enum DecimalValue {
    NaN,
    Finite {
        negative: bool,
        coefficient: u128,
        exponent: i32,
    },
}

impl DecimalValue {
    fn finite(negative: bool, mut coefficient: u128, mut exponent: i32) -> Self {
        if coefficient == 0 {
            return Self::Finite {
                negative: false,
                coefficient: 0,
                exponent: 0,
            };
        }
        while coefficient.is_multiple_of(10) && exponent < i8::MAX.into() {
            coefficient /= 10;
            exponent += 1;
        }
        if !(i8::MIN as i32..=i8::MAX as i32).contains(&exponent) {
            return Self::NaN;
        }
        Self::Finite {
            negative,
            coefficient,
            exponent,
        }
    }

    fn zero() -> Self {
        Self::finite(false, 0, 0)
    }

    fn one() -> Self {
        Self::finite(false, 1, 0)
    }

    fn parse(input: &str) -> Self {
        let input = input.trim();
        let (negative, unsigned) = if let Some(rest) = input.strip_prefix('-') {
            (true, rest)
        } else if let Some(rest) = input.strip_prefix('+') {
            (false, rest)
        } else {
            (false, input)
        };
        let (base, scientific_exponent) = match unsigned.split_once(['e', 'E']) {
            Some((base, exponent)) => match exponent.parse::<i32>() {
                Ok(exponent) => (base, exponent),
                Err(_) => return Self::NaN,
            },
            None => (unsigned, 0),
        };
        let (integer, fraction) = match base.split_once('.') {
            Some((integer, fraction)) => (integer, fraction),
            None => (base, ""),
        };
        if integer.is_empty() && fraction.is_empty() {
            return Self::NaN;
        }
        if !integer
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
        {
            return Self::NaN;
        }

        let coefficient = integer
            .bytes()
            .chain(fraction.bytes())
            .try_fold(0u128, |value, digit| {
                value.checked_mul(10)?.checked_add(u128::from(digit - b'0'))
            });
        let Some(coefficient) = coefficient else {
            return Self::NaN;
        };
        let Ok(fraction_len) = i32::try_from(fraction.len()) else {
            return Self::NaN;
        };
        let Some(exponent) = scientific_exponent.checked_sub(fraction_len) else {
            return Self::NaN;
        };
        Self::finite(negative, coefficient, exponent)
    }

    fn from_ns_decimal(decimal: NSDecimal) -> Self {
        let NSDecimal { fields, mantissa } = decimal;
        let length = ((fields >> 8) & 0xf) as usize;
        let negative = fields & (1 << 12) != 0;
        if length == 0 {
            return if negative { Self::NaN } else { Self::zero() };
        }
        if length > mantissa.len() {
            return Self::NaN;
        }

        let coefficient = mantissa[..length]
            .iter()
            .rev()
            .fold(0u128, |value, limb| (value << 16) | u128::from(*limb));
        let exponent = i32::from(fields as u8 as i8);
        Self::finite(negative, coefficient, exponent)
    }

    fn to_ns_decimal(&self) -> NSDecimal {
        let mut mantissa = [0; 8];
        let (negative, mut coefficient, exponent) = match *self {
            Self::NaN => {
                return NSDecimal {
                    fields: 1 << 12,
                    mantissa,
                };
            }
            Self::Finite {
                negative,
                coefficient,
                exponent,
            } => (negative, coefficient, exponent),
        };

        let mut length = 0usize;
        while coefficient != 0 {
            assert!(length < mantissa.len());
            mantissa[length] = coefficient as u16;
            coefficient >>= 16;
            length += 1;
        }
        let exponent = i8::try_from(exponent).unwrap() as u8;
        let fields =
            u32::from(exponent) | ((length as u32) << 8) | (u32::from(negative) << 12) | (1 << 13);
        NSDecimal { fields, mantissa }
    }

    fn as_f64(&self) -> f64 {
        match *self {
            Self::NaN => f64::NAN,
            Self::Finite {
                negative,
                coefficient,
                exponent,
            } => {
                let value = coefficient as f64 * 10f64.powi(exponent);
                if negative {
                    -value
                } else {
                    value
                }
            }
        }
    }

    fn cmp_magnitude(
        a_coefficient: u128,
        a_exponent: i32,
        b_coefficient: u128,
        b_exponent: i32,
    ) -> Ordering {
        if a_coefficient == 0 || b_coefficient == 0 {
            return a_coefficient.cmp(&b_coefficient);
        }
        let a = a_coefficient.to_string();
        let b = b_coefficient.to_string();
        let a_decimal_position = a.len() as i32 + a_exponent;
        let b_decimal_position = b.len() as i32 + b_exponent;
        match a_decimal_position.cmp(&b_decimal_position) {
            Ordering::Equal => {
                let len = a.len().max(b.len());
                a.bytes()
                    .chain(std::iter::repeat(b'0'))
                    .zip(b.bytes().chain(std::iter::repeat(b'0')))
                    .take(len)
                    .find_map(|(a, b)| (a != b).then(|| a.cmp(&b)))
                    .unwrap_or(Ordering::Equal)
            }
            ordering => ordering,
        }
    }

    fn compare(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::NaN, Self::NaN) => Ordering::Equal,
            (Self::NaN, _) => Ordering::Greater,
            (_, Self::NaN) => Ordering::Less,
            (
                Self::Finite {
                    negative: a_negative,
                    coefficient: a_coefficient,
                    exponent: a_exponent,
                },
                Self::Finite {
                    negative: b_negative,
                    coefficient: b_coefficient,
                    exponent: b_exponent,
                },
            ) => match a_negative.cmp(b_negative) {
                Ordering::Equal => {
                    let magnitude = Self::cmp_magnitude(
                        *a_coefficient,
                        *a_exponent,
                        *b_coefficient,
                        *b_exponent,
                    );
                    if *a_negative {
                        magnitude.reverse()
                    } else {
                        magnitude
                    }
                }
                Ordering::Less => Ordering::Greater,
                Ordering::Greater => Ordering::Less,
            },
        }
    }

    fn checked_pow10(power: u32) -> Option<u128> {
        (0..power).try_fold(1u128, |value, _| value.checked_mul(10))
    }

    fn add(&self, other: &Self) -> Self {
        let (
            Self::Finite {
                negative: a_negative,
                coefficient: a_coefficient,
                exponent: a_exponent,
            },
            Self::Finite {
                negative: b_negative,
                coefficient: b_coefficient,
                exponent: b_exponent,
            },
        ) = (self, other)
        else {
            return Self::NaN;
        };
        let exponent = (*a_exponent).min(*b_exponent);
        let Some(a_scale) = Self::checked_pow10((*a_exponent - exponent) as u32) else {
            return Self::NaN;
        };
        let Some(b_scale) = Self::checked_pow10((*b_exponent - exponent) as u32) else {
            return Self::NaN;
        };
        let Some(a) = a_coefficient.checked_mul(a_scale) else {
            return Self::NaN;
        };
        let Some(b) = b_coefficient.checked_mul(b_scale) else {
            return Self::NaN;
        };

        if a_negative == b_negative {
            let Some(coefficient) = a.checked_add(b) else {
                return Self::NaN;
            };
            Self::finite(*a_negative, coefficient, exponent)
        } else if a >= b {
            Self::finite(*a_negative, a - b, exponent)
        } else {
            Self::finite(*b_negative, b - a, exponent)
        }
    }

    fn to_decimal_string(&self) -> String {
        let Self::Finite {
            negative,
            coefficient,
            exponent,
        } = *self
        else {
            return "NaN".to_string();
        };
        if coefficient == 0 {
            return "0".to_string();
        }

        let mut digits = coefficient.to_string();
        if exponent >= 0 {
            digits.extend(std::iter::repeat_n('0', exponent as usize));
        } else {
            let decimal_position = digits.len() as i32 + exponent;
            if decimal_position > 0 {
                digits.insert(decimal_position as usize, '.');
            } else {
                let mut prefixed =
                    String::with_capacity((2 - decimal_position) as usize + digits.len());
                prefixed.push_str("0.");
                prefixed.extend(std::iter::repeat_n('0', -decimal_position as usize));
                prefixed.push_str(&digits);
                digits = prefixed;
            }
        }
        if negative {
            format!("-{digits}")
        } else {
            digits
        }
    }
}

struct NSDecimalNumberHostObject {
    superclass: NSNumberHostObject,
    decimal: DecimalValue,
}
impl_HostObject_with_superclass!(NSDecimalNumberHostObject);

fn new_decimal_number(env: &mut Environment, class: Class, decimal: DecimalValue) -> id {
    let host_object = Box::new(NSDecimalNumberHostObject {
        superclass: NSNumberHostObject::Double(decimal.as_f64()),
        decimal,
    });
    env.objc.alloc_object(class, host_object, &mut env.mem)
}

fn set_decimal_number(env: &mut Environment, object: id, decimal: DecimalValue) {
    let host = env.objc.borrow_mut::<NSDecimalNumberHostObject>(object);
    host.superclass = NSNumberHostObject::Double(decimal.as_f64());
    host.decimal = decimal;
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

// NSValue is an abstract class. None of the things it should provide are
// implemented here yet (TODO).
@implementation NSValue: NSObject

+ (id)valueWithPointer:(ConstVoidPtr)ptr {
    // TODO: implement with `value:withObjCType:` instead
    msg_class![env; NSNumber numberWithUnsignedInt:(ptr.to_bits())]
}

+ (id)valueWithCGPoint:(CGPoint)value {
    let host_object = Box::new(NSValueHostObject::CGPoint(value));
    let new = env.objc.alloc_object(this, host_object, &mut env.mem);
    autorelease(env, new)
}

+ (id)valueWithCGSize:(CGSize)value {
    let host_object = Box::new(NSValueHostObject::CGSize(value));
    let new = env.objc.alloc_object(this, host_object, &mut env.mem);
    autorelease(env, new)
}

+ (id)valueWithCGRect:(CGRect)value {
    let host_object = Box::new(NSValueHostObject::CGRect(value));
    let new = env.objc.alloc_object(this, host_object, &mut env.mem);
    autorelease(env, new)
}

- (CGPoint)CGPointValue {
    let host_object = env.objc.borrow::<NSValueHostObject>(this);
    match host_object {
        NSValueHostObject::CGPoint(cg_point) => *cg_point,
        _ => unimplemented!()
    }
}

- (CGSize)CGSizeValue {
    let host_object = env.objc.borrow::<NSValueHostObject>(this);
    match host_object {
        NSValueHostObject::CGSize(cg_size) => *cg_size,
        _ => unimplemented!()
    }
}

- (CGRect)CGRectValue {
    let host_object = env.objc.borrow::<NSValueHostObject>(this);
    match host_object {
        NSValueHostObject::CGRect(cg_rect) => *cg_rect,
        _ => unimplemented!()
    }
}

// NSCopying implementation
- (id)copyWithZone:(NSZonePtr)_zone {
    retain(env, this)
}

- (MutVoidPtr)pointerValue {
    let class: Class = msg![env; this class];
    assert!(class == env.objc.get_known_class("NSNumber", &mut env.mem));
    // According to the docs, `If the value object was not created to hold
    // a pointer-sized data item, the result is undefined.`
    let val = msg![env; this unsignedIntValue];
    MutVoidPtr::from_bits(val)
}

@end

// NSNumber is not an abstract class.
@implementation NSNumber: NSValue

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSNumberHostObject::Bool(false));
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)numberWithBool:(bool)value {
    // TODO: for greater efficiency we could return a static-lifetime value

    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithBool:value];
    autorelease(env, new)
}

+ (id)numberWithFloat:(f32)value {
    // TODO: for greater efficiency we could return a static-lifetime value

    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithFloat:value];
    autorelease(env, new)
}

+ (id)numberWithDouble:(f64)value {
    // TODO: for greater efficiency we could return a static-lifetime value

    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithDouble:value];
    autorelease(env, new)
}

+ (id)numberWithUnsignedInt:(u32)value {
    // TODO: for greater efficiency we could return a static-lifetime value

    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithUnsignedInt:value];
    autorelease(env, new)
}

+ (id)numberWithInt:(i32)value {
    // TODO: for greater efficiency we could return a static-lifetime value

    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithInt:value];
    autorelease(env, new)
}

+ (id)numberWithLong:(i32)value {
    // TODO: for greater efficiency we could return a static-lifetime value

    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithLong:value];
    autorelease(env, new)
}

+ (id)numberWithInteger:(NSInteger)value {
    // TODO: for greater efficiency we could return a static-lifetime value

    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithInteger:value];
    autorelease(env, new)
}

+ (id)numberWithUnsignedInteger:(NSUInteger)value {
    // TODO: for greater efficiency we could return a static-lifetime value

    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithUnsignedInteger:value];
    autorelease(env, new)
}

+ (id)numberWithLongLong:(i64)value {
    // TODO: for greater efficiency we could return a static-lifetime value

    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithLongLong:value];
    autorelease(env, new)
}

+ (id)numberWithUnsignedLongLong:(u64)value {
    // TODO: for greater efficiency we could return a static-lifetime value

    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithUnsignedLongLong:value];
    autorelease(env, new)
}

+ (id)numberWithShort:(i16)value {
    // TODO: for greater efficiency we could return a static-lifetime value

    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithShort:value];
    autorelease(env, new)
}

+ (id)numberWithUnsignedShort:(u16)value {
    // TODO: for greater efficiency we could return a static-lifetime value

    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithUnsignedShort:value];
    autorelease(env, new)
}

+ (id)numberWithChar:(i8)value {
    // TODO: for greater efficiency we could return a static-lifetime value

    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithChar:value];
    autorelease(env, new)
}

// TODO: types other than booleans and long longs

// NSCoding implementation
- (id)initWithCoder:(id)coder {
    let class: Class = msg![env; coder class];
    let keyed_unarch_class: Class = msg_class![env; NSKeyedUnarchiver class];
    let nib_archive_class: Class = msg_class![env; _touchHLE_NIBArchiveDecoder class];
    let new_num = if env.objc.class_is_subclass_of(class, keyed_unarch_class) {
        ns_keyed_unarchiver::decode_current_number(env, coder)
    } else if env.objc.class_is_subclass_of(class, nib_archive_class) {
        _nib_archive_decoder::decode_current_number(env, coder)
    } else {
        unimplemented!();
    };
    release(env, this);
    new_num
}
- (())encodeWithCoder:(id)coder {
    let host_object = env.objc.borrow::<NSNumberHostObject>(this);
    let (key, val) = match host_object {
        NSNumberHostObject::Int(i) => ("NS.intval", plist::Value::Integer((*i).into())),
        NSNumberHostObject::Double(d) => ("NS.dblval", plist::Value::Real(*d)),
        NSNumberHostObject::Bool(b) => ("NS.boolval", plist::Value::Boolean(*b)),
        _ => unimplemented!("{:?}", host_object)
    };

    let scope = get_value_to_encode_for_current_key(env, coder);
    scope.insert(key.to_string(), val);
}

- (id)initWithBool:(bool)value {
    *env.objc.borrow_mut(this) = NSNumberHostObject::Bool(value);
    this
}

- (id)initWithFloat:(f32)value {
    *env.objc.borrow_mut(this) = NSNumberHostObject::Float(value);
    this
}

- (id)initWithDouble:(f64)value {
    *env.objc.borrow_mut(this) = NSNumberHostObject::Double(value);
    this
}

- (id)initWithLongLong:(i64)value {
    *env.objc.borrow_mut(this) = NSNumberHostObject::LongLong(value);
    this
}

- (id)initWithUnsignedInt:(u32)value {
    *env.objc.borrow_mut(this) = NSNumberHostObject::UnsignedInt(value);
    this
}

- (id)initWithInt:(i32)value {
    *env.objc.borrow_mut(this) = NSNumberHostObject::Int(value);
    this
}

- (id)initWithLong:(i32)value {
    *env.objc.borrow_mut(this) = NSNumberHostObject::Int(value);
    this
}

- (id)initWithInteger:(NSInteger)value {
    *env.objc.borrow_mut(this) = NSNumberHostObject::Int(value);
    this
}

- (id)initWithUnsignedInteger:(NSUInteger)value {
    *env.objc.borrow_mut(this) = NSNumberHostObject::UnsignedInt(value);
    this
}

- (id)initWithUnsignedLongLong:(u64)value {
    *env.objc.borrow_mut(this) = NSNumberHostObject::UnsignedLongLong(value);
    this
}

- (id)initWithShort:(i16)value {
    *env.objc.borrow_mut(this) = NSNumberHostObject::Short(value);
    this
}

- (id)initWithUnsignedShort:(u16)value {
    *env.objc.borrow_mut(this) = NSNumberHostObject::UnsignedShort(value);
    this
}

- (id)initWithChar:(i8)value {
    *env.objc.borrow_mut(this) = NSNumberHostObject::Char(value);
    this
}

- (bool)boolValue {
    env.objc.borrow::<NSNumberHostObject>(this).as_bool()
}

- (NSInteger)integerValue {
    env.objc.borrow::<NSNumberHostObject>(this).as_int()
}

- (i32)intValue {
    env.objc.borrow::<NSNumberHostObject>(this).as_int()
}

- (i32)longValue {
    env.objc.borrow::<NSNumberHostObject>(this).as_int()
}

- (f32)floatValue {
    env.objc.borrow::<NSNumberHostObject>(this).as_float()
}

- (f64)doubleValue {
    env.objc.borrow::<NSNumberHostObject>(this).as_double()
}

- (i64)longLongValue {
    env.objc.borrow::<NSNumberHostObject>(this).as_long_long()
}

- (u64)unsignedLongLongValue {
    env.objc.borrow::<NSNumberHostObject>(this).as_unsigned_long_long()
}

- (u32)unsignedIntValue {
    env.objc.borrow::<NSNumberHostObject>(this).as_unsigned_int()
}

- (NSUInteger)unsignedIntegerValue {
    env.objc.borrow::<NSNumberHostObject>(this).as_unsigned_int()
}

- (i16)shortValue {
    env.objc.borrow::<NSNumberHostObject>(this).as_short()
}

- (u16)unsignedShortValue {
    env.objc.borrow::<NSNumberHostObject>(this).as_unsigned_short()
}

- (i8)charValue {
    env.objc.borrow::<NSNumberHostObject>(this).as_char()
}

- (id)description {
    msg![env; this stringValue]
}

- (id)stringValue {
    msg![env; this descriptionWithLocale:nil]
}
- (id)descriptionWithLocale:(id)locale {
    assert_eq!(locale, nil); // TODO
    // TODO: do not alloc format strings each time
    let format = match env.objc.borrow(this) {
        NSNumberHostObject::Bool(_) | NSNumberHostObject::Char(_) | NSNumberHostObject::Int(_) => from_rust_string(env, "%i".to_string()),
        NSNumberHostObject::Double(_) => from_rust_string(env, "%0.16g".to_string()),
        NSNumberHostObject::Float(_) => from_rust_string(env, "%0.7g".to_string()),
        NSNumberHostObject::LongLong(_) => from_rust_string(env, "%lli".to_string()),
        NSNumberHostObject::Short(_) => from_rust_string(env, "%hi".to_string()),
        NSNumberHostObject::UnsignedInt(_) => from_rust_string(env, "%u".to_string()),
        NSNumberHostObject::UnsignedLongLong(_) => from_rust_string(env, "%llu".to_string()),
        NSNumberHostObject::UnsignedShort(_) => from_rust_string(env, "%hu".to_string()),
    };
    let ns_string_class = env.objc.get_known_class("NSString", &mut env.mem);
    let sel = env.objc.lookup_selector("stringWithFormat:").unwrap();
    // TODO: type info for host-to-host message calls with var-args
    let res = match env.objc.borrow(this) {
        NSNumberHostObject::Bool(value) => crate::objc::msg_send_no_type_checking(env, (ns_string_class, sel, format, *value as i32)),
        NSNumberHostObject::Char(value) => crate::objc::msg_send_no_type_checking(env, (ns_string_class, sel, format, *value)),
        NSNumberHostObject::Double(value) => crate::objc::msg_send_no_type_checking(env, (ns_string_class, sel, format, *value)),
        NSNumberHostObject::Float(value) => {
            // Need to promote float to double for the expected argument of %g
            crate::objc::msg_send_no_type_checking(env, (ns_string_class, sel, format, *value as f64))
        },
        NSNumberHostObject::Int(value) => crate::objc::msg_send_no_type_checking(env, (ns_string_class, sel, format, *value)),
        NSNumberHostObject::LongLong(value) => crate::objc::msg_send_no_type_checking(env, (ns_string_class, sel, format, *value)),
        NSNumberHostObject::Short(value) => crate::objc::msg_send_no_type_checking(env, (ns_string_class, sel, format, *value)),
        NSNumberHostObject::UnsignedInt(value) => crate::objc::msg_send_no_type_checking(env, (ns_string_class, sel, format, *value)),
        NSNumberHostObject::UnsignedLongLong(value) => crate::objc::msg_send_no_type_checking(env, (ns_string_class, sel, format, *value)),
        NSNumberHostObject::UnsignedShort(value) => crate::objc::msg_send_no_type_checking(env, (ns_string_class, sel, format, *value)),
    };
    release(env, format);
    res
}

- (NSUInteger)hash {
    // The only requirement for [obj hash] is that values that compare equal
    // (via [obj isEqual] have the same hash. Hashing the underlying
    // bits works here.
    let value =
    match env.objc.borrow(this) {
        NSNumberHostObject::Bool(value) => *value as u64,
        NSNumberHostObject::UnsignedLongLong(value) => *value,
        NSNumberHostObject::UnsignedInt(value) => *value as u64,
        NSNumberHostObject::Int(value) => *value as u64,
        NSNumberHostObject::LongLong(value) => *value as u64,
        NSNumberHostObject::Float(value) => value.to_bits() as u64,
        NSNumberHostObject::Double(value) => value.to_bits(),
        NSNumberHostObject::Short(value) => *value as u64,
        NSNumberHostObject::UnsignedShort(value) => *value as u64,
        NSNumberHostObject::Char(value) => *value as u64,
    };
    super::hash_helper(&value)
}

- (bool)isEqual:(id)other {
    if this == other {
        return true;
    }
    let class: Class = msg_class![env; NSNumber class];
    if !msg![env; other isKindOfClass:class] {
        return false;
    }
    msg![env; this isEqualToNumber:other]
}

- (bool)isEqualToNumber:(id)other {
    let res: NSComparisonResult = msg![env; this compare:other];
    res == NSOrderedSame
}

- (NSComparisonResult)compare:(id)other { // NSNumber *
    let num = env.objc.borrow::<NSNumberHostObject>(this);
    let other_num = env.objc.borrow::<NSNumberHostObject>(other);
    let ordering = match (num.is_float(), other_num.is_float()) {
        (false, false) => num.as_i128().cmp(&other_num.as_i128()),
        // In case of having a float, we promote to double for comparison
        _ => {
            // TODO: handle partial cmp fails
            let res = num.as_double().partial_cmp(&other_num.as_double()).unwrap();
            if res == Ordering::Equal {
                // On ties, we compare as i128 as well
                num.as_i128().cmp(&other_num.as_i128())
            } else {
                res
            }
        },
    };
    from_rust_ordering(ordering)
}

// TODO: accessors etc

@end

@implementation NSDecimalNumber: NSNumber

+ (id)allocWithZone:(NSZonePtr)_zone {
    new_decimal_number(env, this, DecimalValue::zero())
}

+ (id)decimalNumberWithMantissa:(u64)mantissa
                       exponent:(i16)exponent
                     isNegative:(bool)is_negative {
    let number = new_decimal_number(
        env,
        this,
        DecimalValue::finite(is_negative, u128::from(mantissa), i32::from(exponent)),
    );
    autorelease(env, number)
}

+ (id)decimalNumberWithString:(id)number_value {
    let decimal = DecimalValue::parse(&to_rust_string(env, number_value));
    let number = new_decimal_number(env, this, decimal);
    autorelease(env, number)
}

+ (id)decimalNumberWithDecimal:(NSDecimal)decimal {
    let number = new_decimal_number(env, this, DecimalValue::from_ns_decimal(decimal));
    autorelease(env, number)
}

+ (id)zero {
    let number = new_decimal_number(env, this, DecimalValue::zero());
    autorelease(env, number)
}

+ (id)one {
    let number = new_decimal_number(env, this, DecimalValue::one());
    autorelease(env, number)
}

+ (id)notANumber {
    let number = new_decimal_number(env, this, DecimalValue::NaN);
    autorelease(env, number)
}

- (id)initWithMantissa:(u64)mantissa
               exponent:(i16)exponent
             isNegative:(bool)is_negative {
    set_decimal_number(
        env,
        this,
        DecimalValue::finite(is_negative, u128::from(mantissa), i32::from(exponent)),
    );
    this
}

- (id)initWithString:(id)number_value {
    let decimal = DecimalValue::parse(&to_rust_string(env, number_value));
    set_decimal_number(env, this, decimal);
    this
}

- (id)initWithDecimal:(NSDecimal)decimal {
    set_decimal_number(env, this, DecimalValue::from_ns_decimal(decimal));
    this
}

- (NSDecimal)decimalValue {
    env.objc
        .borrow::<NSDecimalNumberHostObject>(this)
        .decimal
        .to_ns_decimal()
}

- (id)decimalNumberByAdding:(id)other {
    let this_decimal = env
        .objc
        .borrow::<NSDecimalNumberHostObject>(this)
        .decimal
        .clone();
    let other_decimal = env
        .objc
        .borrow::<NSDecimalNumberHostObject>(other)
        .decimal
        .clone();
    let class: Class = msg![env; this class];
    let result = new_decimal_number(env, class, this_decimal.add(&other_decimal));
    autorelease(env, result)
}

- (NSComparisonResult)compare:(id)other {
    let this_decimal = env
        .objc
        .borrow::<NSDecimalNumberHostObject>(this)
        .decimal
        .clone();
    let decimal_class = env.objc.get_known_class("NSDecimalNumber", &mut env.mem);
    let other_class: Class = msg![env; other class];
    let other_decimal = if env.objc.class_is_subclass_of(other_class, decimal_class) {
        env.objc
            .borrow::<NSDecimalNumberHostObject>(other)
            .decimal
            .clone()
    } else {
        let value: f64 = msg![env; other doubleValue];
        DecimalValue::parse(&value.to_string())
    };
    from_rust_ordering(this_decimal.compare(&other_decimal))
}

- (id)descriptionWithLocale:(id)_locale {
    let description = env
        .objc
        .borrow::<NSDecimalNumberHostObject>(this)
        .decimal
        .to_decimal_string();
    let string = from_rust_string(env, description);
    autorelease(env, string)
}

- (id)description {
    msg![env; this descriptionWithLocale:nil]
}

- (id)stringValue {
    msg![env; this descriptionWithLocale:nil]
}

- (NSUInteger)hash {
    let value = env
        .objc
        .borrow::<NSDecimalNumberHostObject>(this)
        .decimal
        .to_decimal_string();
    super::hash_helper(&value)
}

@end

};

pub fn is_conversion_lossless(env: &mut Environment, this: id, type_: CFNumberType) -> bool {
    let num = env.objc.borrow::<NSNumberHostObject>(this);
    let num2: id = match type_ {
        kCFNumberSInt32Type | kCFNumberIntType => {
            let val: i32 = num.as_int();
            msg_class![env; NSNumber numberWithInt:val]
        }
        kCFNumberFloat32Type | kCFNumberFloatType => {
            let val: f32 = num.as_float();
            msg_class![env; NSNumber numberWithFloat:val]
        }
        kCFNumberSInt16Type | kCFNumberShortType => {
            let val: i16 = num.as_short();
            msg_class![env; NSNumber numberWithShort:val]
        }
        kCFNumberSInt8Type | kCFNumberCharType => {
            let val: i8 = num.as_char();
            msg_class![env; NSNumber numberWithChar:val]
        }
        _ => unimplemented!("is_conversion_lossless for {}", type_),
    };
    msg![env; this isEqualToNumber:num2]
}
