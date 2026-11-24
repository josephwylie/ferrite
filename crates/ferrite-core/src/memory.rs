//! Resident memory, as a window and a Server read it: one process's
//! resident bytes, and a Session the memory watchdog replaced. The
//! watchdog is the Engine's.

use crate::ThreadId;

/// A Session the watchdog replaced, for the operator and the app's log.
/// The Engine names its Thread by id; a Cockpit by its key
/// (`cockpit::Restart`).
#[derive(Debug, Clone, PartialEq)]
pub struct Restart<T = ThreadId> {
    pub thread: T,
    /// What it had grown to when the watchdog acted.
    pub rss: u64,
}

/// One process's resident bytes, the way the panes24 spike reads it.
#[cfg(not(windows))]
pub fn rss_bytes(pid: u32) -> Option<u64> {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let kilobytes: u64 = String::from_utf8(out.stdout).ok()?.trim().parse().ok()?;
    Some(kilobytes * 1024)
}

/// One process's resident bytes through the native Windows process API.
#[cfg(windows)]
pub fn rss_bytes(pid: u32) -> Option<u64> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };

    unsafe {
        let process = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let ok = K32GetProcessMemoryInfo(process, &mut counters, counters.cb);
        CloseHandle(process);
        (ok != 0).then_some(counters.WorkingSetSize as u64)
    }
}
