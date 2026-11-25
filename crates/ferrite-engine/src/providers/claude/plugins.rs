//! Claude's plugin installer, as setup sync runs it on the side that turned
//! a plugin on (ADR 0050): `claude plugin …`, by the copy a Session would
//! run.

use std::io;
use std::process::Stdio;
use std::sync::{Arc, OnceLock};

use ferrite_core::provider::Provider;
use ferrite_core::setup_sync::{Installer, Installs};
use serde_json::Value;

use crate::providers::discover;

/// This process's: one for every Home of the process, so what failed is
/// said by any later scan.
pub fn installs() -> Arc<Installs> {
    static ONE: OnceLock<Arc<Installs>> = OnceLock::new();
    ONE.get_or_init(|| Installs::new(Claude)).clone()
}

struct Claude;

impl Installer for Claude {
    fn run(&self, words: &[&str]) -> io::Result<()> {
        let program = ferrite_core::spawn::spawnable_program(&discover::program(Provider::Claude));
        let output = ferrite_core::spawn::agent(program)
            .arg("plugin")
            .args(words)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| match error.kind() {
                io::ErrorKind::NotFound => io::Error::other("Claude Code is not installed there"),
                _ => error,
            })?;
        if output.status.success() {
            return Ok(());
        }
        Err(io::Error::other(said(&output.stdout, &output.stderr)))
    }
}

/// What `claude plugin … --json` said went wrong: the message of its last
/// line of JSON, else its last line.
fn said(stdout: &[u8], stderr: &[u8]) -> String {
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
        .unwrap_or_else(|| "Claude's installer failed".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_claudes_installer_said_is_its_message() {
        let failed = br#"{"command":"install","outcome":"failed","plugin":"pstack@cursor-plugins","message":"Plugin \"pstack\" not found in marketplace \"cursor-plugins\"."}"#;
        assert_eq!(said(failed, b""), r#"Plugin "pstack" not found in marketplace "cursor-plugins"."#);
        assert_eq!(
            said(b"", "\u{2718} Failed to add marketplace: no such repo\n".as_bytes()),
            "Failed to add marketplace: no such repo"
        );
    }
}
