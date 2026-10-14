//! When the Server must stop on its own: SIGTERM and SIGINT (on Windows,
//! the console's events and the session's end), and on macOS loginwindow's
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
    //! A console's Ctrl+C and close (a Server run by hand), and the end of
    //! the session. The Server started for the app has no console, so the
    //! end of the session reaches it only as `WM_ENDSESSION`, sent to every
    //! top-level window: it keeps a hidden one for that.

    use super::{Ordering, SIGNALLED};
    use std::ptr::null;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::Console::{
        SetConsoleCtrlHandler, CTRL_CLOSE_EVENT, CTRL_C_EVENT, CTRL_SHUTDOWN_EVENT,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW, MSG,
        WM_ENDSESSION, WNDCLASSW,
    };

    /// Ask the pump to stop, and hold the process up while it flushes:
    /// Windows ends it when the handler returns. The pump's exit ends it
    /// sooner.
    fn stop_and_wait() {
        SIGNALLED.store(true, Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_secs(4));
    }

    unsafe extern "system" fn console(event: u32) -> windows_sys::core::BOOL {
        if matches!(event, CTRL_C_EVENT | CTRL_CLOSE_EVENT | CTRL_SHUTDOWN_EVENT) {
            stop_and_wait();
            return 1;
        }
        0
    }

    unsafe extern "system" fn window(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_ENDSESSION && wparam != 0 {
            stop_and_wait();
            return 0;
        }
        // SAFETY: the arguments Windows passed this window procedure.
        unsafe { DefWindowProcW(window, message, wparam, lparam) }
    }

    pub(super) fn watch() {
        // SAFETY: registering a handler that only stores to an atomic.
        unsafe { SetConsoleCtrlHandler(Some(console), 1) };
        let watcher = std::thread::Builder::new()
            .name("ferrite-session-end".into())
            .spawn(|| {
                let class: Vec<u16> = "FerriteServer\0".encode_utf16().collect();
                // SAFETY: a class and a never-shown window owned by this
                // thread, whose messages it pumps until the process ends.
                unsafe {
                    let instance = GetModuleHandleW(null());
                    let registered = RegisterClassW(&WNDCLASSW {
                        lpfnWndProc: Some(window),
                        hInstance: instance,
                        lpszClassName: class.as_ptr(),
                        ..Default::default()
                    });
                    let hidden = CreateWindowExW(
                        0,
                        class.as_ptr(),
                        class.as_ptr(),
                        0,
                        0,
                        0,
                        0,
                        0,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        instance,
                        null(),
                    );
                    if registered == 0 || hidden.is_null() {
                        crate::log!(
                            "server: cannot watch for the session's end: {}",
                            std::io::Error::last_os_error()
                        );
                        return;
                    }
                    let mut message: MSG = std::mem::zeroed();
                    while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                        DispatchMessageW(&message);
                    }
                }
            });
        if let Err(error) = watcher {
            crate::log!("server: cannot watch for the session's end: {error}");
        }
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
