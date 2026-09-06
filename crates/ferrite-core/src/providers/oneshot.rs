//! Bounded, isolated CLI completions shared by Titles and follow-ups.
//! Provider adapters own flags, models and output formats. Callers supply
//! text; the runner owns stdin, pipe draining, the scratch cwd and deadline.

use crate::spawn::NoConsoleWindow;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::{codex, discover, spawnable_program};
use crate::store::Provider;

pub(crate) const TIMEOUT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(25);

/// The form a provider fills for an isolated CLI completion: everything
/// the agnostic runner needs and nothing it has to understand. The
/// instruction text is already inside `args`, wherever that CLI wants it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Form {
    /// The CLI to run — the provider's configured program, as the Session
    /// would spawn it.
    pub program: String,
    /// Everything after the program. The reply is read from stdout, so the
    /// flags select the output format interpreted by the provider adapter.
    pub args: Vec<String>,
    /// The provider-owned model name, available to callers describing cost.
    pub model: &'static str,
    /// The effort level sent with the model, in the provider's own words.
    pub effort: &'static str,
}

/// Codex predicts here; Claude uses its live native stream. Unavailable
/// providers never fall back to another account. Discovery happens on the worker, away from the UI.
pub(crate) fn predict(provider: Provider, system: &str, context: &str) -> Option<String> {
    if provider == Provider::Claude {
        return None;
    }
    discover::located(provider)?;
    let program = discover::program(provider);
    predict_using(provider, &program, system, context)
}

fn predict_using(provider: Provider, program: &str, system: &str, context: &str) -> Option<String> {
    let form = match provider {
        Provider::Claude => return None,
        Provider::Codex => codex::followup::fill(program, system),
    };
    let output = run(&form, Some(context), TIMEOUT)?;
    Some(output.trim().to_string())
}

/// Raw stdout on success. No provider interpretation or feature-specific
/// cleaning belongs here. A timeout covers the child AND its pipe readers.
pub(crate) fn run(form: &Form, input: Option<&str>, timeout: Duration) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("ferrite-oneshot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let mut child = Command::new(spawnable_program(&form.program))
        .args(&form.args)
        .no_console_window()
        .current_dir(dir)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut output = String::new();
        let result = stdout.read_to_string(&mut output).map(|_| output);
        let _ = tx.send(result);
    });
    // Write concurrently too: a CLI that never reads stdin cannot hold up
    // the deadline. Dropping the writer closes stdin when the context ends.
    let writer = input.map(|input| {
        let input = input.to_owned();
        let mut stdin = child.stdin.take().expect("piped stdin");
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(stdin.write_all(input.as_bytes()));
        });
        rx
    });
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => thread::sleep(POLL),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    if let Some(writer) = writer {
        writer
            .recv_timeout(timeout.saturating_sub(started.elapsed()))
            .ok()?
            .ok()?;
    }
    rx.recv_timeout(timeout.saturating_sub(started.elapsed()))
        .ok()?
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_policies_keep_predictions_local_and_lightweight() {
        let codex = codex::followup::fill("codex-only", "Predict a prompt");
        assert_eq!(codex.program, "codex-only");
        assert_eq!(codex.model, "gpt-5.5");
        assert_eq!(codex.effort, "low");
        for flag in [
            "--ephemeral",
            "--ignore-user-config",
            "--ignore-rules",
            "read-only",
            "features.shell_tool=false",
            "features.apps=false",
            "features.multi_agent=false",
            "model_reasoning_effort=\"low\"",
        ] {
            assert!(codex.args.iter().any(|arg| arg == flag), "{flag}");
        }
    }

    #[cfg(unix)]
    mod processes {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        fn stub(name: &str, body: &str) -> Form {
            let dir = std::env::temp_dir().join(format!(
                "ferrite-oneshot-test-{}-{name}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let program = dir.join("cli");
            std::fs::write(&program, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
            Form {
                program: program.to_string_lossy().into(),
                args: vec![],
                model: "stub",
                effort: "none",
            }
        }

        #[test]
        fn predictions_use_each_adapters_output_format_and_pipe_the_context() {
            // Each stub checks argv and stdin, then speaks its provider's
            // real output format. Neither test starts an authenticated CLI.
            let context = "Operator asked:\nécho $HOME `untouched`\nAgent replied:\nDone.";
            let codex = stub("codex", "test \"$1\" = exec || exit 1; cat");
            assert_eq!(
                predict_using(Provider::Codex, &codex.program, "predict", context),
                Some(context.into())
            );
        }

        #[test]
        fn absent_provider_never_falls_back_to_another_cli() {
            for provider in [Provider::Claude, Provider::Codex] {
                assert_eq!(
                    predict_using(
                        provider,
                        "/nonexistent/ferrite-no-cli",
                        "predict",
                        "context"
                    ),
                    None
                );
            }
        }

        #[test]
        fn unread_stdin_and_inherited_stdout_cannot_defeat_the_deadline() {
            let blocked = stub("blocked-stdin", "exec sleep 2");
            let started = Instant::now();
            assert_eq!(
                run(
                    &blocked,
                    Some(&"x".repeat(1024 * 1024)),
                    Duration::from_millis(100)
                ),
                None
            );
            assert!(started.elapsed() < Duration::from_secs(1));
            let held = stub("held-stdout", "sleep 2 & exit 0");
            let started = Instant::now();
            assert_eq!(run(&held, None, Duration::from_millis(100)), None);
            assert!(started.elapsed() < Duration::from_secs(1));
        }
    }
}
