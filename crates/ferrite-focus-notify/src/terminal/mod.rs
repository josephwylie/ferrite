//! Raises the terminal container that shows Herdr before a notification click
//! activates the bound terminal.
//!
//! Every Herdr client attached to a session mirrors the same focused pane, so
//! any client's terminal container shows the target once Herdr focuses it.
//! `open -b` alone raises whichever window macOS picks; selecting a client's
//! container first makes the terminal bring that one forward.
//!
//! The shared core finds the session's clients and runs a focus command.
//! Each supported terminal is a [`TerminalAdapter`] that only declares how a
//! client maps to that command, so adding a terminal is one adapter plus an
//! entry in [`ADAPTERS`]. Terminals without an adapter, and any failure, leave
//! the caller's app-level activation unchanged.

mod client;
mod iterm2;
mod kitty;

use std::cmp::Reverse;
use std::ffi::OsString;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub(crate) use client::HerdrClient;

/// A terminal that can focus the container running a given Herdr client.
pub(crate) trait TerminalAdapter: Sync {
    /// Bundle identifier of the terminal app, as stored in workspace bindings.
    fn bundle_id(&self) -> &'static str;

    /// The command that focuses the deepest container running `client` (for
    /// example a kitty window or an iTerm2 session), including its tab and OS
    /// window. `None` when the client does not run in this terminal or the
    /// terminal cannot be controlled.
    fn focus_command(&self, client: &HerdrClient) -> Option<FocusCommand>;
}

/// A command an adapter asks the core to run.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FocusCommand {
    pub(crate) program: OsString,
    pub(crate) args: Vec<OsString>,
}

impl FocusCommand {
    pub(crate) fn new<P, I, A>(program: P, args: I) -> Self
    where
        P: Into<OsString>,
        I: IntoIterator<Item = A>,
        A: Into<OsString>,
    {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

const ADAPTERS: &[&dyn TerminalAdapter] = &[&iterm2::Iterm2, &kitty::Kitty];

/// How long a focus command may take. A terminal that asks the user to confirm
/// a control request would otherwise block the click indefinitely.
const FOCUS_COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

fn adapter_for(bundle_id: &str) -> Option<&'static dyn TerminalAdapter> {
    ADAPTERS
        .iter()
        .copied()
        .find(|adapter| adapter.bundle_id() == bundle_id)
}

/// Focuses the `bound_terminal` container of a Herdr client attached to the
/// session behind `herdr_socket_path`. Clients are tried from the most
/// recently used one until a focus command succeeds.
pub(crate) fn raise_client_container(
    bound_terminal: &str,
    herdr_socket_path: &str,
) -> Result<(), String> {
    let adapter = adapter_for(bound_terminal)
        .ok_or_else(|| format!("no terminal adapter for {bound_terminal}"))?;
    let mut commands: Vec<(u32, FocusCommand)> = client::herdr_clients(herdr_socket_path)?
        .iter()
        .filter_map(|client| Some((client.pid, adapter.focus_command(client)?)))
        .collect();
    // Only clients the adapter can control need their tty looked up.
    commands.sort_by_cached_key(|(pid, _)| Reverse(client::last_input(*pid)));

    let mut last_error = format!("no Herdr client with a controllable {bound_terminal} container");
    for (_, command) in &commands {
        match run_focus_command(command) {
            Ok(()) => return Ok(()),
            Err(err) => last_error = err,
        }
    }
    Err(last_error)
}

fn run_focus_command(command: &FocusCommand) -> Result<(), String> {
    let program = command.program.to_string_lossy();
    let mut child = Command::new(&command.program)
        .args(&command.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| format!("failed to run {program}: {err}"))?;

    let deadline = Instant::now() + FOCUS_COMMAND_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("{program} failed: {status}")),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{program} timed out"));
            }
            Err(err) => return Err(format!("failed to wait for {program}: {err}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_adapters_by_bundle_id() {
        assert_eq!(
            adapter_for("com.googlecode.iterm2").map(|adapter| adapter.bundle_id()),
            Some("com.googlecode.iterm2")
        );
        assert_eq!(
            adapter_for("net.kovidgoyal.kitty").map(|adapter| adapter.bundle_id()),
            Some("net.kovidgoyal.kitty")
        );
        assert!(adapter_for("dev.zed.Zed").is_none());
    }

    #[test]
    fn adapter_bundle_ids_are_unique() {
        let mut ids: Vec<_> = ADAPTERS.iter().map(|adapter| adapter.bundle_id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), ADAPTERS.len());
    }

    #[test]
    fn runs_focus_commands_and_reports_failures() {
        assert_eq!(
            run_focus_command(&FocusCommand::new("true", [""; 0])),
            Ok(())
        );
        assert!(run_focus_command(&FocusCommand::new("false", [""; 0])).is_err());
        assert!(run_focus_command(&FocusCommand::new("/nonexistent/focus", [""; 0])).is_err());
    }

    #[test]
    fn times_out_focus_commands() {
        let started = Instant::now();
        let result = run_focus_command(&FocusCommand::new("sleep", ["10"]));
        assert_eq!(result, Err("sleep timed out".to_string()));
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
