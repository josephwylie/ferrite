//! A Session behind an npm-style `.cmd` shim, end to end on Windows: the
//! child is cmd.exe with the real CLI beneath it. `pid()` must name the CLI
//! (what the RSS watchdog meters), and killing the Session must leave no
//! orphan. The stub's "CLI" is a loopback ping standing in for node — no
//! network beyond 127.0.0.1, and its 30 counts outlive the test many times
//! over unless the kill under test works.
#![cfg(windows)]

use std::fs;
use std::time::{Duration, Instant};

use ferrite_core::providers::{ClaudeConfig, ClaudeSession};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, STILL_ACTIVE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// An npm-style shim on disk: answers the version pin, answers spawn's
/// initialize control request, then holds a long-lived "CLI" underneath.
/// CRLF throughout — cmd.exe parses labels reliably only that way.
fn stub_shim() -> String {
    let dir = std::env::temp_dir().join(format!("ferrite-shim-tree-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("claude-shim.cmd");
    let script = concat!(
        "@echo off\r\n",
        "if \"%~1\"==\"--version\" goto version\r\n",
        "echo {\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",",
        "\"request_id\":\"req_1\",\"response\":{}}}\r\n",
        "ping -n 30 127.0.0.1 >NUL\r\n",
        "exit /b 0\r\n",
        ":version\r\n",
        "echo 2.1.243 (Claude Code)\r\n",
        "exit /b 0\r\n",
    );
    fs::write(&path, script).unwrap();
    path.display().to_string()
}

/// The image name behind a pid, from a Toolhelp snapshot. `None` when the
/// pid is not running.
fn image_of(pid: u32) -> Option<String> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut image = None;
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                if entry.th32ProcessID == pid {
                    let len = entry
                        .szExeFile
                        .iter()
                        .position(|&unit| unit == 0)
                        .unwrap_or(entry.szExeFile.len());
                    image = Some(String::from_utf16_lossy(&entry.szExeFile[..len]));
                    break;
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
        image
    }
}

/// Poll until `pid()` names the ping "CLI" rather than the cmd.exe wrapper.
/// cmd needs a moment to start it; the poll is what makes this deterministic.
fn metered_cli_pid(session: &ClaudeSession) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let pid = session.pid().expect("a spawned Session has a process");
        if let Some(image) = image_of(pid) {
            if image.eq_ignore_ascii_case("ping.exe") {
                return pid;
            }
        }
        assert!(
            Instant::now() < deadline,
            "pid() kept naming the wrapper, never the CLI beneath it"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// One Session, both defects: the watchdog must meter the CLI under the
/// wrapper, and dropping the Session must kill that CLI, not orphan it.
#[test]
fn a_shim_session_meters_and_kills_the_cli_beneath_the_wrapper() {
    let session = ClaudeSession::spawn(ClaudeConfig {
        program: stub_shim(),
        ..Default::default()
    })
    .expect("the shim answers the pin and the handshake");

    let cli = metered_cli_pid(&session);
    // A query-only handle keeps the exit code readable after death without
    // keeping the process alive.
    let handle: HANDLE = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, cli) };
    assert!(
        !handle.is_null(),
        "the CLI must be open-able while it lives"
    );

    drop(session);

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let mut code = 0u32;
        let ok = unsafe { GetExitCodeProcess(handle, &mut code) };
        if ok != 0 && code != STILL_ACTIVE as u32 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the CLI outlived its Session: orphaned"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    unsafe { CloseHandle(handle) };
}
