//! Each agent's plugin installer, as setup sync runs it on the side that
//! turned a plugin on (ADR 0050): `claude plugin …` or `codex plugin …`, by
//! the copy a Session would run. Nothing it starts may ask anyone anything:
//! git is told never to prompt, and a run that does not end in its time is
//! stopped, with all it started.

use std::io::{self, Read};
use std::process::{Command, Output, Stdio};
use std::sync::{mpsc, Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use ferrite_core::provider::Provider;
use ferrite_core::setup_sync::{Installer, Installs};
use serde_json::Value;

use crate::providers::discover;

/// How long one `<tool> plugin …` may run: a marketplace's clone takes
/// seconds, and a prompt for a password never ends.
const DEADLINE: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_millis(25);
/// How long a stopped run's output is waited for: whatever it started
/// that outlived the stop may still hold its pipes.
const DRAIN: Duration = Duration::from_secs(1);

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
            io::ErrorKind::NotFound => io::Error::other(format!("{} is not installed", name(tool))),
            io::ErrorKind::TimedOut => io::Error::other(format!(
                "{}'s installer did not finish in {} minutes, so it was stopped",
                name(tool),
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

/// Run `command` to its end, with nothing on its input: what it wrote, or
/// `TimedOut` once `deadline` passed, when it is stopped with everything it
/// started.
fn within(mut command: Command, deadline: Duration) -> io::Result<Output> {
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    // A group of its own, so a stop reaches whatever it started.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn()?;
    // A `.cmd` shim's child is cmd.exe with the tool beneath it: only a job
    // reaches the tree.
    #[cfg(windows)]
    let job = crate::providers::job::SessionJob::assign(&child).ok();
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        let (tx, rx) = mpsc::channel();
        if let Some(mut pipe) = pipe {
            thread::spawn(move || {
                let mut bytes = Vec::new();
                let _ = pipe.read_to_end(&mut bytes);
                let _ = tx.send(bytes);
            });
        }
        rx
    };
    let stdout = read(child.stdout.take().map(|pipe| Box::new(pipe) as Box<dyn Read + Send>));
    let stderr = read(child.stderr.take().map(|pipe| Box::new(pipe) as Box<dyn Read + Send>));
    let ends = Instant::now() + deadline;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if Instant::now() >= ends {
            #[cfg(unix)]
            stop(&mut child);
            #[cfg(windows)]
            stop(&mut child, job.as_ref());
            let _ = child.wait();
            break None;
        }
        thread::sleep(POLL);
    };
    let drained = Instant::now() + DRAIN;
    let output = |rx: mpsc::Receiver<Vec<u8>>| rx.recv_timeout(drained.saturating_duration_since(Instant::now())).unwrap_or_default();
    let (stdout, stderr) = (output(stdout), output(stderr));
    match status {
        Some(status) => Ok(Output { status, stdout, stderr }),
        None => Err(io::Error::new(io::ErrorKind::TimedOut, "it did not finish in its time")),
    }
}

/// Stop `child` and everything it started: its process group on Unix, its
/// job on Windows, or the child alone when it could not be put in one.
#[cfg(unix)]
fn stop(child: &mut std::process::Child) {
    // SAFETY: a plain syscall on the group `process_group(0)` made, whose id
    // is the child's pid; the child is not yet reaped.
    unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL) };
}

#[cfg(windows)]
fn stop(child: &mut std::process::Child, job: Option<&crate::providers::job::SessionJob>) {
    match job {
        Some(job) => job.terminate(),
        None => drop(child.kill()),
    }
}

fn name(tool: Provider) -> &'static str {
    match tool {
        Provider::Claude => "Claude Code",
        Provider::Codex => "Codex",
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
        .unwrap_or_else(|| format!("{}'s installer failed", name(tool)))
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

    /// A shell that starts a command that never ends, as a tool waiting on
    /// a prompt does, and holds its pipes open.
    fn never_ending() -> Command {
        if cfg!(windows) {
            let mut command = Command::new("cmd");
            command.args(["/C", "ping -n 60 127.0.0.1 & echo done"]);
            command
        } else {
            let mut command = Command::new("sh");
            command.args(["-c", "sleep 60; echo done"]);
            command
        }
    }

    #[test]
    fn an_installer_that_does_not_end_is_stopped_with_all_it_started() {
        let started = Instant::now();
        let error = within(never_ending(), Duration::from_millis(500)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(
            started.elapsed() < Duration::from_millis(500) + DRAIN,
            "the command it started held the pipes: it was not stopped ({:?})",
            started.elapsed()
        );

        let mut echo = Command::new(if cfg!(windows) { "cmd" } else { "sh" });
        echo.args(if cfg!(windows) { ["/C", "echo said"] } else { ["-c", "echo said"] });
        let output = within(echo, Duration::from_secs(30)).unwrap();
        assert!(output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "said", "{output:?}");
    }
}
