//! Chromium on Windows: `libcef.dll` and its resources beside `ferrite.exe`.
//!
//! cef-dll-sys links the DLL through its import library, and copies the whole
//! runtime next to the binaries it builds (`target/<profile>/`); the
//! installer ships the same files (`scripts/package-windows.ps1`). The DLL is
//! delay-loaded (`build.rs`), so a `ferrite.exe` without it still starts:
//! [`Library::load`] looks for it before the first CEF call loads it.
//!
//! **No sandbox.** Since CEF 138 its Windows sandbox only runs inside CEF's
//! `bootstrap.exe`, which would have to *be* `ferrite.exe` and load the app
//! as a DLL. Ferrite isn't built that way, so Chromium's processes run
//! unsandboxed here, with the user's rights (ADR 0013 weighs it).

use std::path::{Path, PathBuf};

use cef::args::Args;
use cef::MainArgs;
use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

pub(super) const SANDBOXED: bool = false;

/// Nothing to hold: the DLL stays loaded for the life of the process.
pub(super) struct Library;

impl Library {
    pub(super) fn load(exe: &Path) -> Result<Self, String> {
        let dll = exe.parent().ok_or("executable has no parent directory")?.join("libcef.dll");
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
