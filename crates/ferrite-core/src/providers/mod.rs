//! Provider Sessions: one implementation per agent backend, end-to-end
//! (process + protocol + quirks). No shared transport middle layer unless
//! real duplication demands one.

use std::ffi::OsStr;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use crate::{DecisionAnswer, SessionEvent};

mod claude;
mod codex;
pub mod models;
// The Win32 calls are cfg(windows); the pid-selection logic inside is pure
// and part of the host suite, like `cmd_shim` below.
#[cfg(any(windows, test))]
mod job;

/// The program `Command::new` should exec for a configured CLI name.
///
/// npm installs the provider CLIs as `claude.cmd` / `codex.cmd` shims on
/// Windows, and CreateProcess resolves a bare name against `.exe` only — so
/// every npm install would report its CLI missing. A bare name with no
/// `.exe` on PATH but a `.cmd`/`.bat` shim becomes the shim's full path,
/// which std (1.77+) runs through cmd.exe with escaped arguments. Anything
/// with a separator or an extension — the test harness's stub paths
/// included — passes through untouched, as does every name off Windows.
pub(crate) fn spawnable_program(program: &str) -> String {
    if cfg!(windows) {
        if let Some(shim) = cmd_shim(program, std::env::var_os("PATH").as_deref()) {
            return shim.to_string_lossy().into_owned();
        }
    }
    program.to_string()
}

/// The shim a bare `program` resolves to on this PATH: directories in
/// order, and within one, `.exe` (which needs no help) before `.cmd` and
/// `.bat`. Compiled everywhere so the walk is testable off Windows.
fn cmd_shim(program: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    if program.contains(['/', '\\']) || program.contains('.') {
        return None;
    }
    for dir in std::env::split_paths(path?) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        if dir.join(format!("{program}.exe")).is_file() {
            return None;
        }
        for extension in ["cmd", "bat"] {
            let shim = dir.join(format!("{program}.{extension}"));
            if shim.is_file() {
                return Some(shim);
            }
        }
    }
    None
}

/// A live provider Session, whatever backend serves it. Each provider
/// implements this end to end — process, protocol and quirks — and nothing
/// above this line knows which one it is holding.
///
/// No park: a Thread is parked by dropping its Session, which is the whole
/// lifecycle the caller needs and the only one a provider can honour.
pub trait Session {
    /// The bounded event stream. The pump drains this per frame.
    fn events(&self) -> &Receiver<SessionEvent>;
    fn send(&mut self, text: &str) -> io::Result<()>;
    fn interrupt(&mut self) -> io::Result<()>;
    fn respond_to_decision(&mut self, id: &str, answer: DecisionAnswer) -> io::Result<()>;

    /// The process whoever is watching memory should meter. `None` when
    /// there is no process — a scripted Session in a test, for instance.
    /// Providers answer the process worth metering: under a Windows `.cmd`
    /// shim that is the CLI beneath the cmd.exe wrapper, not the ~5MB
    /// wrapper itself.
    fn pid(&self) -> Option<u32> {
        None
    }
}

impl Session for ClaudeSession {
    fn events(&self) -> &Receiver<SessionEvent> {
        ClaudeSession::events(self)
    }

    fn send(&mut self, text: &str) -> io::Result<()> {
        ClaudeSession::send(self, text)
    }

    fn interrupt(&mut self) -> io::Result<()> {
        ClaudeSession::interrupt(self)
    }

    fn respond_to_decision(&mut self, id: &str, answer: DecisionAnswer) -> io::Result<()> {
        ClaudeSession::respond_to_decision(self, id, answer)
    }

    fn pid(&self) -> Option<u32> {
        ClaudeSession::pid(self)
    }
}

impl Session for CodexSession {
    fn events(&self) -> &Receiver<SessionEvent> {
        CodexSession::events(self)
    }

    fn send(&mut self, text: &str) -> io::Result<()> {
        CodexSession::send(self, text)
    }

    fn interrupt(&mut self) -> io::Result<()> {
        CodexSession::interrupt(self)
    }

    fn respond_to_decision(&mut self, id: &str, answer: DecisionAnswer) -> io::Result<()> {
        CodexSession::respond_to_decision(self, id, answer)
    }

    fn pid(&self) -> Option<u32> {
        CodexSession::pid(self)
    }
}
pub use claude::{
    ClaudeCapabilities, ClaudeConfig, ClaudeSession, ClaudeSpawnError,
    CLAUDE_CLI_MAX_VERSION_EXCLUSIVE, CLAUDE_CLI_MIN_VERSION,
};
pub use codex::{
    CodexCapabilities, CodexConfig, CodexSession, CodexSpawnError, CODEX_CLI_MAX_VERSION_EXCLUSIVE,
    CODEX_CLI_MIN_VERSION,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ferrite-shim-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn path_of(dirs: &[&Path]) -> std::ffi::OsString {
        std::env::join_paths(dirs.iter().copied()).unwrap()
    }

    #[test]
    fn a_bare_name_with_only_a_cmd_shim_resolves_to_the_shim() {
        let dir = scratch("cmd-only");
        let shim = dir.join("claude.cmd");
        fs::write(&shim, "").unwrap();
        let path = path_of(&[&dir]);
        assert_eq!(cmd_shim("claude", Some(&path)), Some(shim));
    }

    #[test]
    fn an_exe_on_path_needs_no_help() {
        let dir = scratch("exe-first");
        fs::write(dir.join("claude.exe"), "").unwrap();
        fs::write(dir.join("claude.cmd"), "").unwrap();
        let path = path_of(&[&dir]);
        assert_eq!(cmd_shim("claude", Some(&path)), None);
    }

    #[test]
    fn earlier_path_directories_win() {
        let first = scratch("order-first");
        let second = scratch("order-second");
        fs::write(first.join("claude.cmd"), "").unwrap();
        fs::write(second.join("claude.exe"), "").unwrap();
        let path = path_of(&[&first, &second]);
        assert_eq!(
            cmd_shim("claude", Some(&path)),
            Some(first.join("claude.cmd"))
        );
    }

    /// A separator or an extension is the operator's own choice of program;
    /// it must reach `Command::new` untouched — the stub-CLI harness spawns
    /// by absolute path through exactly this door.
    #[test]
    fn paths_and_extensions_pass_through() {
        let dir = scratch("explicit");
        fs::write(dir.join("claude.cmd"), "").unwrap();
        let path = path_of(&[&dir]);
        for explicit in ["./claude", "tools\\claude", "claude.cmd"] {
            assert_eq!(cmd_shim(explicit, Some(&path)), None, "{explicit}");
        }
    }

    #[test]
    fn nothing_found_stays_a_bare_name() {
        let dir = scratch("empty");
        let path = path_of(&[&dir]);
        assert_eq!(cmd_shim("claude", Some(&path)), None);
        assert_eq!(cmd_shim("claude", None), None);
    }
}
