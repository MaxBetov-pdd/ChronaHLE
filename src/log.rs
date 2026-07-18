/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Logging and terminal output macros.

use std::any::Any;
use std::backtrace::Backtrace;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, Once};

static LAST_PANIC_SUMMARY: LazyLock<Mutex<Option<String>>> = LazyLock::new(Default::default);
static CRASH_FILE_STARTED: AtomicBool = AtomicBool::new(false);

fn panic_payload(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "(non-string payload)".to_string()
    }
}

fn persist_crash_report(summary: &str) {
    let base_path = crate::paths::user_data_base_path();
    let current_path = base_path.join("ChronaHLE_crash.log");
    let previous_path = base_path.join("ChronaHLE_crash.previous.log");
    let first_panic_this_run = !CRASH_FILE_STARTED.swap(true, Ordering::SeqCst);

    if first_panic_this_run {
        let _ = std::fs::remove_file(&previous_path);
        if current_path.is_file() {
            let _ = std::fs::rename(&current_path, &previous_path);
        }
    }

    let file = if first_panic_this_run {
        File::create(&current_path)
    } else {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&current_path)
    };
    let Ok(mut file) = file else { return };
    let _ = writeln!(file, "{summary}");
    let _ = writeln!(file, "Host backtrace:\n{}", Backtrace::force_capture());
    let _ = writeln!(file, "\n---\n");
}

/// Install a panic hook that keeps the source location available to popup
/// handlers and preserves two crash reports across launches.
pub fn install_panic_hook() {
    static INSTALL_HOOK: Once = Once::new();
    INSTALL_HOOK.call_once(|| {
        #[cfg(not(target_os = "android"))]
        let previous_hook = std::panic::take_hook();

        std::panic::set_hook(Box::new(move |info| {
            let payload = panic_payload(info.payload());
            let summary = if let Some(location) = info.location() {
                format!("Panic at {location}: {payload}")
            } else {
                format!("Panic: {payload}")
            };
            if let Ok(mut last_panic) = LAST_PANIC_SUMMARY.lock() {
                *last_panic = Some(summary.clone());
            }
            persist_crash_report(&summary);

            #[cfg(target_os = "android")]
            sdl2::log::log(&summary);
            #[cfg(not(target_os = "android"))]
            previous_hook(info);
        }));
    });
}

/// Return the most recent panic with its source location for a crash popup.
pub fn take_panic_summary(payload: &(dyn Any + Send)) -> String {
    LAST_PANIC_SUMMARY
        .lock()
        .ok()
        .and_then(|mut report| report.take())
        .unwrap_or_else(|| panic_payload(payload))
}

/// Get a handle to the log file. This is only for use by logging macros!
///
/// All the logging macros print to stderr or (on Android) logcat, but this
/// is not convenient for users who aren't accustomed to command-line tools or
/// who don't have access to ADB, so we also write to a log file.
pub fn get_log_file() -> &'static File {
    static LOG_FILE: LazyLock<File> = LazyLock::new(|| {
        File::create(crate::paths::user_data_base_path().join("ChronaHLE_log.txt")).unwrap()
    });

    &LOG_FILE
}

/// Prints a log message unconditionally. Use this for errors or warnings.
///
/// The message is prefixed with the module path, so it is clear where it comes
/// from.
macro_rules! log {
    ($($arg:tt)+) => {
        echo!("{}: {}", module_path!(), format_args!($($arg)+));
    }
}

/// Same as [log], but silently fails on panic instead of
/// panicking.
macro_rules! log_no_panic {
    ($($arg:tt)+) => {
        echo_no_panic!("{}: {}", module_path!(), format_args!($($arg)+));
    }
}

/// Like [log], but prints the message only if debugging is enabled for the
/// module where it is used. This can be used for verbose things only needed
/// when debugging.
macro_rules! log_dbg {
    ($($arg:tt)+) => {
        if $crate::log::ENABLED_MODULES.contains(&module_path!()) {
            log!($($arg)*);
        }
    }
}

/// Like [log], but messages only log once and cannot have formatting.
/// To be used for log messages that are known to spam the log file (like those
/// logged every frame).
macro_rules! log_once {
    ($msg:literal) => {{
        static LOG_ONCE: std::sync::Once = std::sync::Once::new();
        LOG_ONCE.call_once(|| {
            log!("{} [this log will only be shown once]", $msg);
        });
    }};
}

/// Print a message (with implicit newline). This should be used for all
/// ChronaHLE output that isn't coming from the app itself.
///
/// Prefer use [log] or [log_dbg] for errors and warnings during emulation.
macro_rules! echo {
    ($($arg:tt)+) => {
        {
            let formatted_str = format!($($arg)+);

            #[cfg(target_os = "android")]
            {
                sdl2::log::log(&formatted_str);
            }
            #[cfg(not(target_os = "android"))]
            eprintln!("{}", formatted_str);

            use std::io::Write;
            let mut log_file = $crate::log::get_log_file();
            let _ = log_file.write_all(formatted_str.as_bytes());
            let _ = log_file.write_all(b"\n");
        }
    };
    () => {
        {
            #[cfg(target_os = "android")]
            {
                sdl2::log::log("");
            }
            #[cfg(not(target_os = "android"))]
            eprintln!("");

            use std::io::Write;
            let _ = $crate::log::get_log_file().write_all(b"\n");
        }
    }
}

/// Same as [echo], but silently fails on panic instead of
/// panicking.
macro_rules! echo_no_panic {
    ($($arg:tt)*) => {
        {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                echo!($($arg)*);
            }));
        }
    }
}

/// Put modules to enable [log_dbg] for here, e.g. "chronahle::mem" to see when
/// memory is allocated and freed.
pub const ENABLED_MODULES: &[&str] = &[];
