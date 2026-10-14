//! When the Server must stop on its own: SIGTERM and SIGINT (on Windows,
//! the console's close and shutdown events), and on macOS loginwindow's
//! point of no return at logout or shutdown — which comes 15–20 s before
//! launchd's SIGTERM, after loginwindow has begun killing the provider CLIs,
//! so the flush must not wait for the signal (after Herdr's
//! `platform/macos/shutdown.rs`; the keys are undocumented, and a missing
//! one leaves SIGTERM to do the work).

use std::sync::atomic::{AtomicBool, Ordering};

static SIGNALLED: AtomicBool = AtomicBool::new(false);

/// Start watching. Called once, before the pump runs.
pub(super) fn watch() {
    #[cfg(unix)]
    unix::watch();
    #[cfg(windows)]
    windows::watch();
    #[cfg(target_os = "macos")]
    loginwindow::watch();
}

/// Why the Server must stop now, if it must.
pub(super) fn stop_requested() -> Option<&'static str> {
    if SIGNALLED.load(Ordering::SeqCst) {
        return Some("signalled");
    }
    #[cfg(target_os = "macos")]
    if loginwindow::no_return() {
        return Some("the session is ending");
    }
    None
}

#[cfg(unix)]
mod unix {
    use super::{Ordering, SIGNALLED};

    extern "C" fn signalled(_: libc::c_int) {
        SIGNALLED.store(true, Ordering::SeqCst);
    }

    pub(super) fn watch() {
        let handler = signalled as extern "C" fn(libc::c_int) as libc::sighandler_t;
        // SAFETY: the handler only stores to an atomic, which is
        // async-signal-safe.
        unsafe {
            libc::signal(libc::SIGTERM, handler);
            libc::signal(libc::SIGINT, handler);
        }
    }
}

#[cfg(windows)]
mod windows {
    use super::{Ordering, SIGNALLED};
    use windows_sys::Win32::System::Console::{
        SetConsoleCtrlHandler, CTRL_CLOSE_EVENT, CTRL_C_EVENT, CTRL_SHUTDOWN_EVENT,
    };

    unsafe extern "system" fn signalled(event: u32) -> windows_sys::core::BOOL {
        if matches!(event, CTRL_C_EVENT | CTRL_CLOSE_EVENT | CTRL_SHUTDOWN_EVENT) {
            SIGNALLED.store(true, Ordering::SeqCst);
            // Hold the process up while the pump flushes: Windows ends it
            // when this returns for a close or shutdown.
            std::thread::sleep(std::time::Duration::from_secs(4));
            return 1;
        }
        0
    }

    pub(super) fn watch() {
        // SAFETY: registering a handler that only stores to an atomic.
        unsafe { SetConsoleCtrlHandler(Some(signalled), 1) };
    }
}

#[cfg(target_os = "macos")]
mod loginwindow {
    use std::ffi::{c_char, c_int, CString};
    use std::sync::OnceLock;

    const KEYS: [&str; 2] = [
        "com.apple.system.loginwindow.logoutNoReturn",
        "com.apple.system.loginwindow.shutdownNoReturn",
    ];

    extern "C" {
        fn notify_register_check(name: *const c_char, token: *mut c_int) -> u32;
        fn notify_check(token: c_int, check: *mut c_int) -> u32;
    }

    static TOKENS: OnceLock<Vec<c_int>> = OnceLock::new();

    pub(super) fn watch() {
        TOKENS.get_or_init(|| {
            KEYS.iter()
                .filter_map(|key| {
                    let name = CString::new(*key).ok()?;
                    let mut token = 0;
                    // SAFETY: a NUL-terminated name and a valid out-pointer.
                    if unsafe { notify_register_check(name.as_ptr(), &mut token) } != 0 {
                        crate::log!("server: cannot watch {key}");
                        return None;
                    }
                    // The first check after registering always says posted.
                    let mut posted = 0;
                    // SAFETY: a token registered above.
                    unsafe { notify_check(token, &mut posted) };
                    Some(token)
                })
                .collect()
        });
    }

    /// Whether loginwindow passed its point of no return since last asked.
    pub(super) fn no_return() -> bool {
        TOKENS.get().is_some_and(|tokens| {
            tokens.iter().any(|token| {
                let mut posted = 0;
                // SAFETY: a token registered in `watch`.
                unsafe { notify_check(*token, &mut posted) == 0 && posted != 0 }
            })
        })
    }
}
