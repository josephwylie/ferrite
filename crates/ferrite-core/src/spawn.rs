//! Spawning children: without flashing a console window, and, for the
//! provider CLIs, without the identity of whatever launched Ferrite and a
//! class below the Server's (ADR 0034).

use std::ffi::OsStr;
use std::process::Command;

/// `CREATE_NO_WINDOW`: the child gets no console of its own.
///
/// Every provider and tool child here talks over piped stdio, never a
/// terminal — but a GUI process spawning a console program (or, for the
/// npm `.cmd` shims, the `cmd.exe` std runs them through) makes Windows
/// allocate a console window for it, which flashes over the cockpit.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Suppress the console window a spawned child would otherwise get.
///
/// A no-op off Windows, so call sites stay free of `cfg`.
pub trait NoConsoleWindow {
    fn no_console_window(&mut self) -> &mut Self;
}

impl NoConsoleWindow for Command {
    fn no_console_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            self.creation_flags(CREATE_NO_WINDOW);
        }
        self
    }
}

/// `BELOW_NORMAL_PRIORITY_CLASS`: the class a child of a below-normal
/// process inherits, so the agent's whole tree runs at it.
#[cfg(windows)]
const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;

/// How much nicer than the Server an agent runs on Linux: about a tenth of
/// the CPU against it under contention, all of it when nothing contends.
#[cfg(target_os = "linux")]
const AGENT_NICE: i32 = 10;

/// A child that does the agents' work: a Session's CLI, or one the Server
/// runs on their behalf (titles, command lists, the model catalog). It and
/// everything it starts — the agent's builds, tests and engines — run a
/// class below the Server, so the window's acts, which wait on the
/// Server's pump, win the CPU when the machine is full, and the agents
/// still have all of it when nothing else wants it (ADR 0034). The
/// environment is the agents' (`AgentEnvironment`) and, on Windows, there
/// is no console window.
///
/// On macOS the program runs through `taskpolicy -c utility`: a QoS clamp
/// can only be set by `posix_spawn`, which `Command` offers no hook into,
/// and `taskpolicy` execs the program in place, so the child's pid, pipes
/// and exit status are the program's own.
pub fn agent(program: impl AsRef<OsStr>) -> Command {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("/usr/sbin/taskpolicy");
        command.args(["-c", "utility"]).arg(program);
        command
    };
    #[cfg(not(target_os = "macos"))]
    let mut command = Command::new(program);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: nice is a single system call, and nothing else runs
        // between fork and exec. A nice that fails leaves the agent at the
        // Server's class, which is how it ran before.
        unsafe {
            command.pre_exec(|| {
                libc::nice(AGENT_NICE);
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
    }
    command.agent_environment();
    command
}

/// Run the calling thread as one the operator waits on: the Server's pump
/// and the threads that carry a window's commands to it and its replies
/// back. The counterpart of `agent`: what the agents start runs below, so
/// this wins the CPU when the machine is full (ADR 0034). Best effort; a
/// thread left where it was runs as it did before.
///
/// A new thread does not inherit its creator's class on macOS, so each
/// such thread asks for it itself. USER_INITIATED, not USER_INTERACTIVE:
/// the pump's work is a reply the operator waits for, sometimes seconds
/// of it, not a frame; and XNU gives a process with no UI role the same
/// priority for both.
pub fn serve_the_operator() {
    #[cfg(target_os = "macos")]
    // SAFETY: the call takes plain values and affects only this thread.
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INITIATED, 0);
    }
    #[cfg(windows)]
    // SAFETY: GetCurrentThread's pseudo-handle is always valid here.
    unsafe {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL,
        };
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL);
    }
}

/// Markers a Ferrite started from inside an agent or a terminal inherits
/// and must not hand on: they say "you are a child of that agent Session"
/// or "you are in that terminal pane", and a Session Ferrite starts is
/// neither. A Claude CLI that sees `CLAUDECODE` believes it is nested in
/// another Claude Code; a Codex that sees its sandbox markers believes it
/// is already sandboxed. The operator's own configuration (`CLAUDE_CODE_USE_BEDROCK`,
/// `CODEX_HOME`, …) is not a marker and passes through.
const INHERITED_MARKERS: &[&str] = &[
    // Claude Code's Session identity.
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_SSE_PORT",
    // Codex's.
    "CODEX_THREAD_ID",
    "CODEX_SANDBOX",
    "CODEX_SANDBOX_NETWORK_DISABLED",
    // The host terminal's pane, tab and multiplexer.
    "TERM_SESSION_ID",
    "ITERM_SESSION_ID",
    "LC_TERMINAL",
    "LC_TERMINAL_VERSION",
    "WEZTERM_PANE",
    "KITTY_WINDOW_ID",
    "WT_SESSION",
    "TMUX",
    "TMUX_PANE",
    "STY",
    "ZELLIJ",
    "ZELLIJ_SESSION_NAME",
    "ZELLIJ_PANE_ID",
    // A Ferrite's own caller context (`api::caller_environment`): the
    // Thread a Ferrite started from inside an agent is not this one's, nor
    // the Server its Projects link.
    "FERRITE_THREAD_ID",
    "FERRITE_SOCKET",
    "FERRITE_BIN",
    "HERDR_PANE_ID",
    "HERDR_SOCKET_PATH",
    "FERRITE_PROJECTS_BIN",
];

/// The environment of a provider CLI Ferrite starts: the inherited
/// markers removed, and the host named as Ferrite in their place.
pub trait AgentEnvironment {
    fn agent_environment(&mut self) -> &mut Self;
}

impl AgentEnvironment for Command {
    fn agent_environment(&mut self) -> &mut Self {
        for marker in INHERITED_MARKERS {
            self.env_remove(marker);
        }
        self.env("TERM_PROGRAM", "ferrite")
            .env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn an_agent_child_loses_inherited_markers_and_keeps_configuration() {
        let mut command = Command::new("claude");
        command
            .env("CLAUDECODE", "1")
            .env("CODEX_THREAD_ID", "outer")
            .env("ITERM_SESSION_ID", "w0t0p0")
            .env("TERM_PROGRAM", "iTerm.app")
            .env("CLAUDE_CODE_USE_BEDROCK", "1");

        command.agent_environment();

        let env: Vec<(&OsStr, Option<&OsStr>)> = command.get_envs().collect();
        for marker in ["CLAUDECODE", "CODEX_THREAD_ID", "ITERM_SESSION_ID"] {
            assert!(
                env.contains(&(OsStr::new(marker), None)),
                "{marker} is removed"
            );
        }
        assert!(env.contains(&(OsStr::new("TERM_PROGRAM"), Some(OsStr::new("ferrite")))));
        assert!(env.contains(&(OsStr::new("CLAUDE_CODE_USE_BEDROCK"), Some(OsStr::new("1")))));
    }

    /// What a process the agent's own child starts reports about its class:
    /// the agent's builds and tests are its grandchildren.
    fn grandchilds_class(report: &str) -> String {
        #[cfg(unix)]
        let mut command = agent("/bin/sh");
        #[cfg(unix)]
        command.args(["-c", &format!("/bin/sh -c '{report}'")]);
        #[cfg(windows)]
        let mut command = agent("cmd");
        #[cfg(windows)]
        command.args([
            "/c",
            &format!("powershell -NoProfile -Command \"{report}\""),
        ]);
        let output = command.output().expect("the agent child runs");
        assert!(output.status.success(), "{output:?}");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// UTILITY's ceiling: a process under the clamp cannot be scheduled above
    /// it, whatever it asks for (XNU's `thread_qos_policy_params`).
    #[cfg(target_os = "macos")]
    #[test]
    fn an_agents_descendants_run_clamped_to_utility() {
        let priority: u32 = grandchilds_class("ps -o pri= -p $$").parse().unwrap();
        assert!(priority <= 20, "priority {priority}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn an_agents_descendants_run_niced_below_the_server() {
        let ours = unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) };
        let theirs: i32 = grandchilds_class("nice").parse().unwrap();
        assert_eq!(theirs, (ours + AGENT_NICE).min(19));
    }

    #[cfg(windows)]
    #[test]
    fn an_agents_descendants_run_below_normal() {
        assert_eq!(
            grandchilds_class("(Get-Process -Id $PID).PriorityClass"),
            "BelowNormal"
        );
    }
}
