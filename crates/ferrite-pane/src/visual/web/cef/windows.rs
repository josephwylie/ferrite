//! Chromium on Windows runs inside CEF's sandbox bootstrap, shipped as
//! ferrite.exe. It loads ferrite.dll and supplies the sandbox broker context
//! before Ferrite dispatches its app or Chromium helper entry.

use std::cell::Cell;
use std::path::{Path, PathBuf};

use cef::args::Args;
use cef::MainArgs;
use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

pub(super) const SANDBOXED: bool = true;

thread_local! {
    // CEF boot and initialization run on the entry/main thread. A plain
    // Cargo-built executable has no broker and cannot render web content.
    static SANDBOX_INFO: Cell<*mut u8> = const { Cell::new(std::ptr::null_mut()) };
}

/// # Safety
/// The bootstrap owns this context and must keep it alive until the app exits.
pub(super) unsafe fn set_sandbox_info(info: *mut u8) {
    SANDBOX_INFO.with(|slot| slot.set(info));
}

pub(super) fn sandbox_info() -> *mut u8 {
    SANDBOX_INFO.with(Cell::get)
}

fn require_sandbox() -> Result<(), String> {
    if sandbox_info().is_null() {
        Err("Visuals require Ferrite's packaged Windows sandbox bootstrap".into())
    } else {
        Ok(())
    }
}

/// Nothing to hold: the DLL stays loaded for the life of the process.
pub(super) struct Library;

impl Library {
    pub(super) fn load(exe: &Path) -> Result<Self, String> {
        require_sandbox()?;
        let dll = exe
            .parent()
            .ok_or("executable has no parent directory")?
            .join("libcef.dll");
        if !dll.exists() {
            return Err(format!(
                "{} not found (visuals need Chromium beside ferrite.exe, as the installer lays it out)",
                dll.display()
            ));
        }
        Ok(Self)
    }
}

/// A helper process, readied to run. Chromium started it from this same
/// executable, so the DLL is beside it.
pub(super) struct Helper {
    args: Args,
}

impl Helper {
    pub(super) fn enter(_exe: &Path) -> Result<Self, String> {
        require_sandbox()?;
        Ok(Self { args: Args::new() })
    }

    pub(super) fn args(&self) -> &MainArgs {
        self.args.as_main_args()
    }
}

pub(super) fn process_alive(pid: u32) -> bool {
    // SAFETY: a query-only handle, closed below. None at all: the process is
    // gone, or its pid now belongs to a process that isn't ours (a Ferrite
    // of this user's could be opened), so its profile is a leftover.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut code = 0u32;
        let alive = GetExitCodeProcess(handle, &mut code) != 0 && code == STILL_ACTIVE as u32;
        CloseHandle(handle);
        alive
    }
}

/// As given: canonicalizing would hand CEF a `\\?\` verbatim path.
pub(super) fn real_path(dir: PathBuf) -> PathBuf {
    dir
}
