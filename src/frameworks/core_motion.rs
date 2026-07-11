/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! The Core Motion framework.

use crate::abi::impl_GuestRet_for_large_struct;
use crate::dyld::HostDylib;
use crate::frameworks::foundation::{NSTimeInterval, NSUInteger};
use crate::mem::SafeRead;
use crate::objc::{autorelease, id, msg_class, objc_classes, ClassExports, HostObject, NSZonePtr};

pub const DYLIB: HostDylib = HostDylib {
    path: "/System/Library/Frameworks/CoreMotion.framework/CoreMotion",
    aliases: &[],
    class_exports: &[CLASSES],
    constant_exports: &[],
    function_exports: &[],
};

const DEFAULT_MOTION_UPDATE_INTERVAL: NSTimeInterval = 1.0 / 60.0;

#[derive(Debug, Copy, Clone, Default)]
#[repr(C, packed)]
pub struct CMRotationRate {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
unsafe impl SafeRead for CMRotationRate {}
impl_GuestRet_for_large_struct!(CMRotationRate);

#[derive(Debug, Copy, Clone, Default)]
#[repr(C, packed)]
pub struct CMAcceleration {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
unsafe impl SafeRead for CMAcceleration {}
impl_GuestRet_for_large_struct!(CMAcceleration);

#[derive(Debug, Copy, Clone)]
#[repr(C, packed)]
pub struct CMQuaternion {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}
impl Default for CMQuaternion {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        }
    }
}
unsafe impl SafeRead for CMQuaternion {}
impl_GuestRet_for_large_struct!(CMQuaternion);

#[derive(Debug, Copy, Clone)]
#[repr(C, packed)]
pub struct CMRotationMatrix {
    pub m11: f64,
    pub m12: f64,
    pub m13: f64,
    pub m21: f64,
    pub m22: f64,
    pub m23: f64,
    pub m31: f64,
    pub m32: f64,
    pub m33: f64,
}
impl Default for CMRotationMatrix {
    fn default() -> Self {
        Self {
            m11: 1.0,
            m12: 0.0,
            m13: 0.0,
            m21: 0.0,
            m22: 1.0,
            m23: 0.0,
            m31: 0.0,
            m32: 0.0,
            m33: 1.0,
        }
    }
}
unsafe impl SafeRead for CMRotationMatrix {}
impl_GuestRet_for_large_struct!(CMRotationMatrix);

struct CMMotionManagerHostObject {
    accelerometer_active: bool,
    accelerometer_update_interval: NSTimeInterval,
    gyro_active: bool,
    gyro_update_interval: NSTimeInterval,
    device_motion_active: bool,
    device_motion_update_interval: NSTimeInterval,
}
impl Default for CMMotionManagerHostObject {
    fn default() -> Self {
        Self {
            accelerometer_active: false,
            accelerometer_update_interval: DEFAULT_MOTION_UPDATE_INTERVAL,
            gyro_active: false,
            gyro_update_interval: DEFAULT_MOTION_UPDATE_INTERVAL,
            device_motion_active: false,
            device_motion_update_interval: DEFAULT_MOTION_UPDATE_INTERVAL,
        }
    }
}
impl HostObject for CMMotionManagerHostObject {}

#[derive(Default)]
struct CMAccelerometerDataHostObject {
    acceleration: CMAcceleration,
}
impl HostObject for CMAccelerometerDataHostObject {}

#[derive(Default)]
struct CMGyroDataHostObject {
    rotation_rate: CMRotationRate,
}
impl HostObject for CMGyroDataHostObject {}

#[derive(Default)]
struct CMDeviceMotionHostObject {
    attitude: CMAttitudeHostObject,
    rotation_rate: CMRotationRate,
    gravity: CMAcceleration,
    user_acceleration: CMAcceleration,
}
impl HostObject for CMDeviceMotionHostObject {}

#[derive(Copy, Clone, Default)]
struct CMAttitudeHostObject {
    roll: f64,
    pitch: f64,
    yaw: f64,
    rotation_matrix: CMRotationMatrix,
    quaternion: CMQuaternion,
}
impl HostObject for CMAttitudeHostObject {}

const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation CMLogItem: NSObject

- (NSTimeInterval)timestamp {
    0.0
}

@end

@implementation CMMotionManager: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    env.objc.alloc_object(this, Box::new(CMMotionManagerHostObject::default()), &mut env.mem)
}

+ (NSUInteger)availableAttitudeReferenceFrames {
    1 // CMAttitudeReferenceFrameXArbitraryZVertical
}

- (bool)isGyroAvailable {
    log_dbg!("[(CMMotionManager *){:?} isGyroAvailable] -> true", this);
    true
}
- (bool)isDeviceMotionAvailable {
    // According to docs, this is functionally equivalent to `isGyroAvailable`
    // method. (All devices have accelerometer, but only some do have gyro).
    log_dbg!("[(CMMotionManager *){:?} isDeviceMotionAvailable] -> true", this);
    true
}
- (bool)isAccelerometerAvailable {
    // According to https://developer.apple.com/documentation/coremotion/getting-raw-accelerometer-events?language=objc,
    // every iOS device has an accelerometer, but on real hardware this method
    // can still return false if the device isn't ready to produce data yet.
    // Here we always return true since we don't model that readiness state.
    true
}
- (NSTimeInterval)accelerometerUpdateInterval {
    env.objc.borrow::<CMMotionManagerHostObject>(this).accelerometer_update_interval
}
- (())setAccelerometerUpdateInterval:(NSTimeInterval)interval {
    env.objc.borrow_mut::<CMMotionManagerHostObject>(this).accelerometer_update_interval =
        interval.max(DEFAULT_MOTION_UPDATE_INTERVAL);
}
- (())startAccelerometerUpdates {
    env.objc.borrow_mut::<CMMotionManagerHostObject>(this).accelerometer_active = true;
}
- (())stopAccelerometerUpdates {
    env.objc.borrow_mut::<CMMotionManagerHostObject>(this).accelerometer_active = false;
}
- (bool)isAccelerometerActive {
    env.objc.borrow::<CMMotionManagerHostObject>(this).accelerometer_active
}
- (id)accelerometerData {
    let acceleration_data: id = msg_class![env; CMAccelerometerData alloc];
    let (x, y, z) = env.window().get_acceleration(&env.options);
    env.objc.borrow_mut::<CMAccelerometerDataHostObject>(acceleration_data).acceleration =
        CMAcceleration {
            x: x.into(),
            y: y.into(),
            z: z.into(),
        };
    autorelease(env, acceleration_data)
}
- (NSTimeInterval)gyroUpdateInterval {
    env.objc.borrow::<CMMotionManagerHostObject>(this).gyro_update_interval
}
- (())setGyroUpdateInterval:(NSTimeInterval)interval {
    env.objc.borrow_mut::<CMMotionManagerHostObject>(this).gyro_update_interval =
        interval.max(DEFAULT_MOTION_UPDATE_INTERVAL);
}
- (())startGyroUpdates {
    env.objc.borrow_mut::<CMMotionManagerHostObject>(this).gyro_active = true;
}
- (())stopGyroUpdates {
    env.objc.borrow_mut::<CMMotionManagerHostObject>(this).gyro_active = false;
}
- (bool)isGyroActive {
    env.objc.borrow::<CMMotionManagerHostObject>(this).gyro_active
}
- (id)gyroData {
    let gyro_data: id = msg_class![env; CMGyroData alloc];
    autorelease(env, gyro_data)
}
- (NSTimeInterval)deviceMotionUpdateInterval {
    env.objc.borrow::<CMMotionManagerHostObject>(this).device_motion_update_interval
}
- (())setDeviceMotionUpdateInterval:(NSTimeInterval)interval {
    env.objc.borrow_mut::<CMMotionManagerHostObject>(this).device_motion_update_interval =
        interval.max(DEFAULT_MOTION_UPDATE_INTERVAL);
}
- (())startDeviceMotionUpdates {
    env.objc.borrow_mut::<CMMotionManagerHostObject>(this).device_motion_active = true;
}
- (())startDeviceMotionUpdatesUsingReferenceFrame:(NSUInteger)_reference_frame {
    env.objc.borrow_mut::<CMMotionManagerHostObject>(this).device_motion_active = true;
}
- (())stopDeviceMotionUpdates {
    env.objc.borrow_mut::<CMMotionManagerHostObject>(this).device_motion_active = false;
}
- (bool)isDeviceMotionActive {
    env.objc.borrow::<CMMotionManagerHostObject>(this).device_motion_active
}
- (id)deviceMotion {
    let device_motion: id = msg_class![env; CMDeviceMotion alloc];
    let (x, y, z) = env.window().get_acceleration(&env.options);
    let host_object = env.objc.borrow_mut::<CMDeviceMotionHostObject>(device_motion);
    host_object.gravity = CMAcceleration {
        x: x.into(),
        y: y.into(),
        z: z.into(),
    };
    host_object.user_acceleration = CMAcceleration::default();
    autorelease(env, device_motion)
}

@end

@implementation CMAccelerometerData: CMLogItem

+ (id)allocWithZone:(NSZonePtr)_zone {
    env.objc.alloc_object(this, Box::new(CMAccelerometerDataHostObject::default()), &mut env.mem)
}

- (CMAcceleration)acceleration {
    env.objc.borrow::<CMAccelerometerDataHostObject>(this).acceleration
}

@end

@implementation CMGyroData: CMLogItem

+ (id)allocWithZone:(NSZonePtr)_zone {
    env.objc.alloc_object(this, Box::new(CMGyroDataHostObject::default()), &mut env.mem)
}

- (CMRotationRate)rotationRate {
    env.objc.borrow::<CMGyroDataHostObject>(this).rotation_rate
}

@end

@implementation CMDeviceMotion: CMLogItem

+ (id)allocWithZone:(NSZonePtr)_zone {
    env.objc.alloc_object(this, Box::new(CMDeviceMotionHostObject::default()), &mut env.mem)
}

- (id)attitude {
    let attitude: id = msg_class![env; CMAttitude alloc];
    let host_object = env.objc.borrow::<CMDeviceMotionHostObject>(this).attitude;
    *env.objc.borrow_mut::<CMAttitudeHostObject>(attitude) = host_object;
    autorelease(env, attitude)
}
- (CMRotationRate)rotationRate {
    env.objc.borrow::<CMDeviceMotionHostObject>(this).rotation_rate
}
- (CMAcceleration)gravity {
    env.objc.borrow::<CMDeviceMotionHostObject>(this).gravity
}
- (CMAcceleration)userAcceleration {
    env.objc.borrow::<CMDeviceMotionHostObject>(this).user_acceleration
}

@end

@implementation CMAttitude: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    env.objc.alloc_object(this, Box::new(CMAttitudeHostObject::default()), &mut env.mem)
}

- (f64)roll {
    env.objc.borrow::<CMAttitudeHostObject>(this).roll
}
- (f64)pitch {
    env.objc.borrow::<CMAttitudeHostObject>(this).pitch
}
- (f64)yaw {
    env.objc.borrow::<CMAttitudeHostObject>(this).yaw
}
- (CMRotationMatrix)rotationMatrix {
    env.objc.borrow::<CMAttitudeHostObject>(this).rotation_matrix
}
- (CMQuaternion)quaternion {
    env.objc.borrow::<CMAttitudeHostObject>(this).quaternion
}
- (())multiplyByInverseOfAttitude:(id)_attitude {
    // Neutral attitude stays neutral.
}

@end

};
