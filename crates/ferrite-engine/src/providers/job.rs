//! One Session's whole process tree, held in a Win32 Job Object.
//!
//! npm installs the provider CLIs as `.cmd` shims, so a Session's child on
//! Windows is cmd.exe with the real CLI beneath it. `Child::kill` reaches
//! only the wrapper and orphans the CLI; a job with
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` is the OS primitive that ends the
//! tree — every descendant inherits membership, `terminate` kills them all,
//! and closing the last handle is the backstop kill when terminate never ran.
//!
//! Metering deliberately does NOT use the job's own memory accounting: those
//! counters are job-wide commit, which swings with every tool process the CLI
//! runs (an agent's `cargo build` would read as a Session leak) and is a
//! different metric from the per-process RSS the Unix sampler reads. Instead
//! the job answers *which pid* to meter — the wrapper's own child — and the
//! existing sampler meters it, keeping one metric on both platforms.
//!
//! The selection logic is pure and compiled everywhere so the host suite can
//! test it; only the Win32 calls are `cfg(windows)`.

#[cfg(windows)]
use std::io;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
#[cfg(windows)]
use std::process::Child;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
#[cfg(windows)]
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
#[cfg(windows)]
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicProcessIdList,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
    TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

/// The job holding one Session's tree: the spawned child and everything it
/// starts. Killing the Session goes through here, never through the wrapper
/// alone.
#[cfg(windows)]
pub(crate) struct SessionJob {
    handle: HANDLE,
}

// SAFETY: a job handle is a kernel object handle with no thread affinity; the
// raw pointer inside HANDLE is what strips the auto trait, not any real
// constraint. Sessions were Send before this field and must stay so.
#[cfg(windows)]
unsafe impl Send for SessionJob {}

#[cfg(windows)]
impl SessionJob {
    /// Create the job, arm `KILL_ON_JOB_CLOSE`, and put `child` in it.
    /// Called with the handle fresh from spawn: CreateProcess returns before
    /// the child has run, so membership lands before a `.cmd` shim can start
    /// the real CLI, and every process the tree ever forks inherits it.
    pub(crate) fn assign(child: &Child) -> io::Result<Self> {
        // SAFETY: the raw process handle stays valid for the `&Child`
        // borrow; the job handle is null-checked before use.
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            // From here Drop owns the handle, so an early return leaks nothing.
            let job = Self { handle };
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job.handle,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            if AssignProcessToJobObject(job.handle, child.as_raw_handle()) == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(job)
        }
    }

    /// End every process in the job — the wrapper and whatever it started.
    /// Failure is ignored the way `Child::kill` failure is: the close in
    /// `Drop` is the backstop.
    pub(crate) fn terminate(&self) {
        // SAFETY: the handle is valid from `assign` until `Drop` closes it.
        let _ = unsafe { TerminateJobObject(self.handle, 1) };
    }

    /// `assign`, with the failure path every caller owes: a child that
    /// cannot be jailed is reaped on the spot. Killing it here reaches only
    /// the wrapper — which is exactly why such a Session is refused.
    pub(crate) fn assign_or_reap(child: &mut Child) -> io::Result<Self> {
        let assigned = Self::assign(child);
        if assigned.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        assigned
    }

    /// The pid whoever watches memory should meter: the wrapper's own child
    /// inside the job when the wrapper is cmd.exe (a `.cmd` shim), otherwise
    /// `wrapper` itself. Resolved fresh per call, never cached — a cached pid
    /// outlives its process, and a reused pid would meter a stranger.
    pub(crate) fn watchdog_pid(&self, wrapper: u32) -> u32 {
        choose_watchdog_pid(wrapper, &self.members(), &process_table())
    }

    /// Pids currently in the job. Empty on any query failure — the chooser
    /// then falls back to the wrapper, which is the pre-job behaviour.
    fn members(&self) -> Vec<u32> {
        /// `JOBOBJECT_BASIC_PROCESS_ID_LIST` with room for a working
        /// Session's tree. Overflow truncates this *view* (the query fails),
        /// never the kill.
        #[repr(C)]
        struct PidList {
            number_of_assigned_processes: u32,
            number_of_process_ids_in_list: u32,
            process_id_list: [usize; 64],
        }
        unsafe {
            let mut list = PidList {
                number_of_assigned_processes: 0,
                number_of_process_ids_in_list: 0,
                process_id_list: [0; 64],
            };
            let ok = QueryInformationJobObject(
                self.handle,
                JobObjectBasicProcessIdList,
                (&mut list as *mut PidList).cast(),
                std::mem::size_of::<PidList>() as u32,
                std::ptr::null_mut(),
            );
            if ok == 0 {
                return Vec::new();
            }
            let count = (list.number_of_process_ids_in_list as usize).min(64);
            list.process_id_list[..count]
                .iter()
                .map(|&pid| pid as u32)
                .collect()
        }
    }
}

#[cfg(windows)]
impl Drop for SessionJob {
    fn drop(&mut self) {
        // KILL_ON_JOB_CLOSE: with the last handle gone the kernel ends
        // whatever is still in the job — the kill that cannot be skipped.
        unsafe { CloseHandle(self.handle) };
    }
}

/// One row of the process table, as the chooser needs it.
struct ProcessRow {
    pid: u32,
    parent: u32,
    image: String,
}

/// Every process on the machine, from a Toolhelp snapshot. Far cheaper than
/// the `tasklist` the sampler will spawn for the answer this feeds.
#[cfg(windows)]
fn process_table() -> Vec<ProcessRow> {
    // SAFETY: the snapshot handle is checked against INVALID_HANDLE_VALUE
    // and closed before return; `dwSize` is set before the first walk call,
    // as Toolhelp requires.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Vec::new();
        }
        let mut rows = Vec::new();
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let name_len = entry
                    .szExeFile
                    .iter()
                    .position(|&unit| unit == 0)
                    .unwrap_or(entry.szExeFile.len());
                rows.push(ProcessRow {
                    pid: entry.th32ProcessID,
                    parent: entry.th32ParentProcessID,
                    image: String::from_utf16_lossy(&entry.szExeFile[..name_len]),
                });
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
        rows
    }
}

/// The metered pid, chosen from what the job and the process table said.
///
/// Only a cmd.exe wrapper hides a CLI: any other image *is* the CLI, and
/// redirecting to its child would meter one of its own tool processes. Under
/// cmd.exe the CLI is the wrapper's direct child — job membership screens out
/// a stale parent-pid match from an unrelated process, and conhost.exe is the
/// one wrapper child that is never the CLI (the console host a windowless
/// parent gets given).
fn choose_watchdog_pid(wrapper: u32, members: &[u32], table: &[ProcessRow]) -> u32 {
    let wrapper_is_shim = table
        .iter()
        .any(|row| row.pid == wrapper && row.image.eq_ignore_ascii_case("cmd.exe"));
    if !wrapper_is_shim {
        return wrapper;
    }
    table
        .iter()
        .find(|row| {
            row.parent == wrapper
                && row.pid != wrapper
                && members.contains(&row.pid)
                && !row.image.eq_ignore_ascii_case("conhost.exe")
        })
        .map_or(wrapper, |row| row.pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pid: u32, parent: u32, image: &str) -> ProcessRow {
        ProcessRow {
            pid,
            parent,
            image: image.into(),
        }
    }

    #[test]
    fn a_cmd_wrapper_yields_the_cli_beneath_it() {
        let table = [row(100, 1, "cmd.exe"), row(200, 100, "node.exe")];
        assert_eq!(choose_watchdog_pid(100, &[100, 200], &table), 200);
    }

    /// A direct `.exe` install has no wrapper: the child is the CLI, and its
    /// own tool children in the job must never be metered in its place.
    #[test]
    fn a_direct_cli_is_metered_itself_not_its_tools() {
        let table = [row(100, 1, "claude.exe"), row(300, 100, "cargo.exe")];
        assert_eq!(choose_watchdog_pid(100, &[100, 300], &table), 100);
    }

    /// The CLI's own subprocesses share the job but are not the wrapper's
    /// child; the CLI stays the metered process while tools run.
    #[test]
    fn the_clis_tool_children_are_not_chosen_over_it() {
        let table = [
            row(100, 1, "cmd.exe"),
            row(200, 100, "node.exe"),
            row(300, 200, "cargo.exe"),
        ];
        assert_eq!(choose_watchdog_pid(100, &[100, 200, 300], &table), 200);
    }

    /// A parent pid can be stale history: a process spawned long ago by a
    /// dead pid the wrapper now wears. Job membership is what rules it out.
    #[test]
    fn a_parent_match_outside_the_job_is_a_stranger() {
        let table = [row(100, 1, "cmd.exe"), row(200, 100, "node.exe")];
        assert_eq!(choose_watchdog_pid(100, &[100], &table), 100);
    }

    #[test]
    fn conhost_is_never_the_cli() {
        let table = [
            row(100, 1, "cmd.exe"),
            row(150, 100, "conhost.exe"),
            row(200, 100, "node.exe"),
        ];
        assert_eq!(choose_watchdog_pid(100, &[100, 150, 200], &table), 200);
    }

    /// Before the shim has started the CLI — or after the CLI died — the
    /// wrapper is all there is to meter.
    #[test]
    fn a_wrapper_with_no_cli_yet_is_metered_as_itself() {
        let table = [row(100, 1, "cmd.exe")];
        assert_eq!(choose_watchdog_pid(100, &[100], &table), 100);
    }

    /// Image names come off a case-preserving filesystem; the comparison
    /// cannot care.
    #[test]
    fn image_names_match_case_insensitively() {
        let table = [
            row(100, 1, "CMD.EXE"),
            row(150, 100, "CONHOST.EXE"),
            row(200, 100, "node.exe"),
        ];
        assert_eq!(choose_watchdog_pid(100, &[100, 150, 200], &table), 200);
    }
}

/// The job behaviour itself, provable only where jobs exist. These run on the
/// Windows CI leg: real cmd.exe trees whose grandchild is a loopback ping —
/// no network beyond 127.0.0.1, done in a couple of seconds.
#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::STILL_ACTIVE;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    /// The shim shape: cmd.exe running a long-lived grandchild, exactly what
    /// an npm `.cmd` provider install spawns. Ping's 30 counts outlive the
    /// test many times over; the kill under test is what ends it.
    fn shim_like_tree() -> std::process::Child {
        Command::new("cmd")
            .args(["/C", "ping", "-n", "30", "127.0.0.1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("cmd.exe must spawn")
    }

    /// The grandchild's pid, via the same selection the watchdog uses. cmd
    /// needs a moment to start ping; a poll is what makes this deterministic.
    fn resolved_cli(job: &SessionJob, wrapper: u32) -> u32 {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let pid = job.watchdog_pid(wrapper);
            if pid != wrapper {
                return pid;
            }
            assert!(
                Instant::now() < deadline,
                "the shim's grandchild never appeared in the job"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// A query-only handle: holding it keeps the exit code readable after
    /// death without keeping the process alive.
    fn open_limited(pid: u32) -> HANDLE {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        assert!(!handle.is_null(), "pid {pid} must be open-able while alive");
        handle
    }

    fn assert_dies(handle: HANDLE, who: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let mut code = 0u32;
            // SAFETY: `handle` came from `open_limited` and is closed only
            // below, after the last query.
            let ok = unsafe { GetExitCodeProcess(handle, &mut code) };
            if ok != 0 && code != STILL_ACTIVE as u32 {
                break;
            }
            assert!(Instant::now() < deadline, "{who} survived the kill");
            std::thread::sleep(Duration::from_millis(25));
        }
        // SAFETY: same handle, closed exactly once.
        unsafe { CloseHandle(handle) };
    }

    #[test]
    fn terminating_the_job_kills_the_grandchild_too() {
        let mut child = shim_like_tree();
        let job = SessionJob::assign(&child).expect("assign must succeed");
        let cli = resolved_cli(&job, child.id());
        let cli_handle = open_limited(cli);

        job.terminate();

        assert_dies(cli_handle, "the grandchild");
        let _ = child.wait();
    }

    /// The backstop: no explicit terminate, only the handle going away.
    #[test]
    fn closing_the_last_job_handle_is_itself_the_kill() {
        let mut child = shim_like_tree();
        let job = SessionJob::assign(&child).expect("assign must succeed");
        let cli = resolved_cli(&job, child.id());
        let cli_handle = open_limited(cli);

        drop(job);

        assert_dies(cli_handle, "the grandchild");
        let _ = child.wait();
    }

    /// A direct spawn — no cmd.exe anywhere — must meter the child itself,
    /// never redirect.
    #[test]
    fn a_direct_child_is_its_own_watchdog_pid() {
        let mut child = Command::new("ping")
            .args(["-n", "30", "127.0.0.1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("ping must spawn");
        let job = SessionJob::assign(&child).expect("assign must succeed");

        assert_eq!(job.watchdog_pid(child.id()), child.id());

        job.terminate();
        let _ = child.wait();
    }
}
