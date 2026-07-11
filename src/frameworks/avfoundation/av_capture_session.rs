/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! AV capture/session constants.

use crate::dyld::{ConstantExports, HostConstant};

pub const CONSTANTS: ConstantExports = &[
    (
        "_AVMediaTypeAudio",
        HostConstant::NSString("AVMediaTypeAudio"),
    ),
    (
        "_AVMediaTypeVideo",
        HostConstant::NSString("AVMediaTypeVideo"),
    ),
    (
        "_AVCaptureSessionPreset1280x720",
        HostConstant::NSString("AVCaptureSessionPreset1280x720"),
    ),
    (
        "_AVCaptureSessionPreset640x480",
        HostConstant::NSString("AVCaptureSessionPreset640x480"),
    ),
    (
        "_AVCaptureSessionPresetLow",
        HostConstant::NSString("AVCaptureSessionPresetLow"),
    ),
    (
        "_AVCaptureSessionPresetMedium",
        HostConstant::NSString("AVCaptureSessionPresetMedium"),
    ),
    (
        "_AVPlayerItemDidPlayToEndTimeNotification",
        HostConstant::NSString("AVPlayerItemDidPlayToEndTimeNotification"),
    ),
];
