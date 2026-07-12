/*
 * SDL 2.26 uses ALooper_pollAll, which NDK 28 marks unavailable. Include the
 * platform declarations before replacing the obsolete call so the attribute
 * attached to pollAll is not accidentally transferred to pollOnce.
 */
#pragma once

#include <android/looper.h>

#define ALooper_pollAll ALooper_pollOnce
