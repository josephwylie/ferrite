//! CEF creates the Windows sandbox broker in its bootstrap executable, then
//! enters Ferrite through this export. The bootstrap is shipped as ferrite.exe
//! and loads ferrite.dll for both the browser and every Chromium subprocess.

use std::ffi::c_void;

/// CEF 154's RunWinMain ABI (include/cef_sandbox_win.h). The bootstrap owns
/// sandbox_info for this entire call; Ferrite only lends it to CEF.
#[no_mangle]
unsafe extern "C" fn RunWinMain(
    _instance: *mut c_void,
    _command_line: *mut u16,
    _show: i32,
    sandbox_info: *mut c_void,
    _version_info: *mut c_void,
) -> i32 {
    if sandbox_info.is_null() {
        eprintln!("Ferrite requires CEF's Windows sandbox bootstrap");
        return 1;
    }
    // SAFETY: only the CEF bootstrap invokes this exported entry, and its
    // broker context remains valid until this function returns.
    unsafe { ferrite_pane::visual::web::cef::windows_bootstrap(sandbox_info.cast()) };
    // Panics must not unwind across the C entry point into the bootstrap.
    match std::panic::catch_unwind(crate::run) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}
