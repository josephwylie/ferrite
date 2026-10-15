//! Spawning children: without flashing a console window, and, for the
//! provider CLIs, without the identity of whatever launched Ferrite.

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
    // Thread a Ferrite started from inside an agent is not this one's.
    "FERRITE_THREAD_ID",
    "FERRITE_SOCKET",
    "FERRITE_BIN",
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
}
