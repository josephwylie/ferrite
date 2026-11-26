//! Each agent's plugin installer, as setup sync runs it on the side that
//! turned a plugin on (ADR 0050): `claude plugin …` or `codex plugin …`, by
//! the copy a Session would run. Nothing it starts may ask anyone anything:
//! git is told never to prompt, and a run that does not end in its time is
//! stopped, with all it started.

use std::io;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use ferrite_core::provider::Provider;
use ferrite_core::setup_sync::{Installer, Installs};
use serde_json::Value;

use crate::providers::discover;
use crate::providers::process_tree::within;

/// How long one `<tool> plugin …` may run: a marketplace's clone takes
/// seconds, and a prompt for a password never ends.
const DEADLINE: Duration = Duration::from_secs(120);

/// This process's: one for every Home of the process, so what failed is
/// said by any later scan, and kept in this computer's Ferrite settings for
/// its next start.
pub fn installs() -> Arc<Installs> {
    static ONE: OnceLock<Arc<Installs>> = OnceLock::new();
    ONE.get_or_init(|| Installs::new(Tools, &ferrite_core::server::settings_dir())).clone()
}

struct Tools;

impl Installer for Tools {
    fn run(&self, tool: Provider, words: &[&str]) -> io::Result<()> {
        let program = ferrite_core::spawn::spawnable_program(&discover::program(tool));
        let mut command = ferrite_core::spawn::agent(program);
        command
            .arg("plugin")
            .args(words)
            // Git, as the installers clone with it: a credential it lacks
            // fails at once, never by asking at a terminal or in a window.
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never");
        let output = within(command, DEADLINE).map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => io::Error::other(format!("{} is not installed", tool.cli_name())),
            io::ErrorKind::TimedOut => io::Error::other(format!(
                "{}'s installer did not finish in {} minutes, so it was stopped",
                tool.cli_name(),
                DEADLINE.as_secs() / 60
            )),
            _ => error,
        })?;
        if output.status.success() {
            return Ok(());
        }
        Err(io::Error::other(said(tool, &output.stdout, &output.stderr)))
    }
}

/// What `<tool> plugin …` said went wrong: the message of its last line of
/// JSON, else its last line.
fn said(tool: Provider, stdout: &[u8], stderr: &[u8]) -> String {
    let lines = |bytes: &[u8]| {
        String::from_utf8_lossy(bytes)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let (out, err) = (lines(stdout), lines(stderr));
    let message = out
        .iter()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line).ok()?["message"].as_str().map(str::to_string));
    message
        .or_else(|| err.last().or(out.last()).map(|line| line.trim_start_matches(['✘', ' ']).to_string()))
        .unwrap_or_else(|| format!("{}'s installer failed", tool.cli_name()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_an_installer_said_is_its_message() {
        let failed = br#"{"command":"install","outcome":"failed","plugin":"pstack@cursor-plugins","message":"Plugin \"pstack\" not found in marketplace \"cursor-plugins\"."}"#;
        assert_eq!(said(Provider::Claude, failed, b""), r#"Plugin "pstack" not found in marketplace "cursor-plugins"."#);
        assert_eq!(
            said(Provider::Claude, b"", "\u{2718} Failed to add marketplace: no such repo\n".as_bytes()),
            "Failed to add marketplace: no such repo"
        );
        assert_eq!(said(Provider::Codex, b"", b""), "Codex's installer failed");
    }
}
