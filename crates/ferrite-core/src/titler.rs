//! A model-written Title for a Thread.
//!
//! The first line of the first prompt is a poor name for a Thread once the
//! wall holds a dozen of them ("hey can you look at", "ok so"). So after
//! the first exchange, a small model is asked for a 3–6 word title in the
//! background. Ferrite never calls model APIs itself (CONTEXT.md), so the
//! Thread's own Provider CLI is run in its non-interactive mode — the same
//! official harness a Session uses — on a throwaway std thread, and hands
//! back Some(title) or, on any failure, None. The prompt-derived title
//! stays in place until then.
//!
//! The provider-agnostic part lives here: what to ask, how to run a CLI
//! with a kill deadline, how to clean its reply. What differs per provider
//! — which program, which flags, which cheap model — is a [`TitleForm`]
//! that each provider fills in ([`claude::fill`], [`codex::fill`]); the
//! cockpit picks the filler by the Thread's Provider. Those fillers belong
//! beside their Sessions in `providers/`; they sit here only until that
//! module is free to take them.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use crate::providers::spawnable_program;
use crate::store::Provider;

/// Longest title kept; longer replies mean the model ignored the ask.
const TITLE_CHARS: usize = 60;
/// How much of the first prompt and first reply the model sees. A title
/// comes from the opening lines; the rest only costs tokens.
const PROMPT_CHARS: usize = 2000;
const REPLY_CHARS: usize = 1000;
/// Longer than any sane title turn, shorter than an operator's patience.
const TIMEOUT: Duration = Duration::from_secs(30);
/// How often the watcher thread checks whether the CLI has exited.
const POLL: Duration = Duration::from_millis(50);

/// What the model is shown: the Thread's first prompt and, if the turn has
/// finished, the first reply. Both are cut to their caps when the
/// instruction text is built, so a pasted log does not become the bill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitleRequest {
    pub prompt: String,
    pub reply: Option<String>,
}

/// The form a provider fills in so the titler can run its CLI: everything
/// the agnostic runner needs and nothing it has to understand. The
/// instruction text is already inside `args`, wherever that CLI wants it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitleForm {
    /// The CLI to run — the provider's configured program, as the Session
    /// would spawn it.
    pub program: String,
    /// Everything after the program. The reply is read from stdout, so the
    /// flags must put the model's final text there and nothing else.
    pub args: Vec<String>,
    /// The model the title is asked of, for a UI that says what a title
    /// costs. A provider's own alias ("haiku", "gpt-5.4-mini").
    pub model: &'static str,
    /// The effort level sent with the model, in the provider's own words.
    pub effort: &'static str,
}

/// The instruction text a provider puts in its form.
pub fn title_prompt(req: &TitleRequest) -> String {
    let mut text = String::from(
        "Write a title for the task below: 3 to 6 words, sentence case, \
         no quotes, no trailing period. Name the task itself, not the tool \
         or the person doing it. Reply with the title only.\n\n\
         Task:\n",
    );
    text.push_str(&cut(req.prompt.trim(), PROMPT_CHARS));
    if let Some(reply) = req.reply.as_deref().map(str::trim) {
        if !reply.is_empty() {
            text.push_str("\n\nFirst reply:\n");
            text.push_str(&cut(reply, REPLY_CHARS));
        }
    }
    text
}

/// The first `chars` characters of `text`, whole characters only, so a cap
/// never splits a multi-byte character.
fn cut(text: &str, chars: usize) -> String {
    text.chars().take(chars).collect()
}

/// A usable title from the CLI's raw output: the first non-empty line with
/// wrapping quotes, a trailing period and stray whitespace removed, cut to
/// `TITLE_CHARS`. None when nothing usable remains — the caller keeps the
/// prompt-derived title rather than showing an empty one.
pub fn clean(raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).find(|line| !line.is_empty())?;
    let line = line
        .trim_matches(|c| matches!(c, '"' | '\'' | '“' | '”' | '‘' | '’' | '`'))
        .trim_end_matches('.')
        .trim();
    let collapsed: String = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    Some(collapsed.chars().take(TITLE_CHARS).collect::<String>())
}

/// The filled form for a Thread on `provider`, whose Session runs
/// `program`. The one place that knows which filler goes with which
/// Provider, so the cockpit does not.
pub fn form(provider: Provider, program: &str, req: &TitleRequest) -> TitleForm {
    let prompt = title_prompt(req);
    match provider {
        Provider::Claude => claude::fill(program, &prompt),
        Provider::Codex => codex::fill(program, &prompt),
    }
}

/// Ask for a title on a background thread. The receiver yields exactly one
/// value: Some(title), or None when the CLI is missing, exits non-zero,
/// prints nothing usable, or outlives `TIMEOUT` (it is killed). The
/// receiver is dropped without a value only if the thread panics.
pub fn spawn(form: TitleForm) -> Receiver<Option<String>> {
    spawn_with_timeout(form, TIMEOUT)
}

/// [`spawn`] with the kill deadline chosen by the caller, so a test can
/// prove the kill without waiting thirty seconds.
pub fn spawn_with_timeout(form: TitleForm, timeout: Duration) -> Receiver<Option<String>> {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("ferrite-titler".into())
        .spawn(move || {
            let title = run(&form, timeout);
            let _ = tx.send(title);
        })
        .expect("spawn titler thread");
    rx
}

/// An empty directory for the CLI's cwd, so no project instructions or
/// settings are discovered. One per process; both CLIs are told not to
/// persist, so nothing accumulates there.
fn scratch_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ferrite-titler-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn run(form: &TitleForm, timeout: Duration) -> Option<String> {
    let mut child = Command::new(spawnable_program(&form.program))
        .args(&form.args)
        .current_dir(scratch_dir())
        // Closed, not inherited: Codex reads a piped stdin as more prompt.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    // Drain stdout on its own thread: waiting on the child first would
    // deadlock if the CLI ever filled the pipe, and reading first would
    // defeat the timeout.
    let mut stdout = child.stdout.take()?;
    let reader = thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf);
        buf
    });

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < timeout => thread::sleep(POLL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Err(_) => {
                let _ = child.kill();
                break None;
            }
        }
    };
    let output = reader.join().ok()?;
    if status?.success() {
        clean(&output)
    } else {
        None
    }
}

/// Each Provider's own filler lives beside its Session; the titler only
/// runs the filled form.
pub use crate::providers::claude_title as claude;
pub use crate::providers::codex_title as codex;

#[cfg(test)]
mod tests {
    use super::*;

    fn req(prompt: &str, reply: Option<&str>) -> TitleRequest {
        TitleRequest {
            prompt: prompt.into(),
            reply: reply.map(str::to_string),
        }
    }

    #[test]
    fn prompt_asks_for_a_short_title_and_includes_both_texts() {
        let text = title_prompt(&req(
            "Add retries to the client",
            Some("I'll add exponential backoff."),
        ));
        assert!(text.contains("3 to 6 words"));
        assert!(text.contains("sentence case"));
        assert!(text.contains("no quotes"));
        assert!(text.contains("no trailing period"));
        assert!(text.contains("not the tool"));
        assert!(text.contains("Task:\nAdd retries to the client"));
        assert!(text.contains("First reply:\nI'll add exponential backoff."));
    }

    #[test]
    fn prompt_omits_an_absent_or_blank_reply() {
        assert!(!title_prompt(&req("x", None)).contains("First reply"));
        assert!(!title_prompt(&req("x", Some("  \n"))).contains("First reply"));
    }

    #[test]
    fn prompt_caps_long_inputs_on_char_boundaries() {
        let long_prompt = "é".repeat(PROMPT_CHARS + 500);
        let long_reply = "ü".repeat(REPLY_CHARS + 500);
        let text = title_prompt(&req(&long_prompt, Some(&long_reply)));
        assert_eq!(text.matches('é').count(), PROMPT_CHARS);
        assert_eq!(text.matches('ü').count(), REPLY_CHARS);
    }

    #[test]
    fn clean_strips_quotes_period_and_noise() {
        assert_eq!(
            clean("\"Retry with exponential backoff.\"\n").as_deref(),
            Some("Retry with exponential backoff")
        );
        assert_eq!(
            clean("\n\n  “Fix   flaky  login test”  \nExplanation follows.").as_deref(),
            Some("Fix flaky login test")
        );
        assert_eq!(
            clean("'Migrate to Postgres'").as_deref(),
            Some("Migrate to Postgres")
        );
        assert_eq!(clean("Plain title").as_deref(), Some("Plain title"));
    }

    #[test]
    fn clean_rejects_empty_and_caps_length() {
        assert_eq!(clean(""), None);
        assert_eq!(clean("\n   \n"), None);
        assert_eq!(clean("\"\"."), None);
        let long = "w".repeat(TITLE_CHARS + 20);
        assert_eq!(clean(&long).unwrap().chars().count(), TITLE_CHARS);
    }

    #[test]
    fn claude_fills_a_print_mode_form_with_the_prompt_last() {
        let form = form(Provider::Claude, "/opt/claude", &req("Ship it", None));
        assert_eq!(form.program, "/opt/claude");
        assert_eq!(form.model, claude::MODEL);
        assert_eq!(form.effort, claude::EFFORT);
        assert_eq!(form.args[0], "-p");
        for flag in ["--tools", "--no-session-persistence", "--setting-sources"] {
            assert!(form.args.contains(&flag.to_string()), "{flag}");
        }
        assert!(form.args.last().unwrap().starts_with("Write a title"));
        assert!(form.args.last().unwrap().contains("Ship it"));
    }

    #[test]
    fn codex_fills_an_exec_form_with_the_prompt_last() {
        let form = form(Provider::Codex, "codex", &req("Ship it", None));
        assert_eq!(form.program, "codex");
        assert_eq!(form.model, codex::MODEL);
        assert_eq!(form.effort, codex::EFFORT);
        assert_eq!(form.args[0], "exec");
        for flag in [
            "--ephemeral",
            "--ignore-user-config",
            "--skip-git-repo-check",
        ] {
            assert!(form.args.contains(&flag.to_string()), "{flag}");
        }
        assert!(form.args.last().unwrap().starts_with("Write a title"));
        assert!(form.args.last().unwrap().contains("Ship it"));
    }

    #[cfg(unix)]
    mod with_stub_programs {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        /// A shell script standing in for a CLI, run through a form that
        /// passes only the prompt. It ignores its arguments and runs
        /// `body`, so a test controls output, exit code and duration.
        fn stub(name: &str, body: &str) -> TitleForm {
            let dir = std::env::temp_dir()
                .join(format!("ferrite-titler-stub-{}-{name}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("cli");
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            TitleForm {
                program: path.to_string_lossy().into_owned(),
                args: vec![title_prompt(&req("anything", None))],
                model: "stub",
                effort: "none",
            }
        }

        #[test]
        fn a_printing_stub_yields_the_cleaned_title() {
            let rx = spawn(stub("ok", "echo '\"Retry with backoff.\"'"));
            assert_eq!(
                rx.recv_timeout(Duration::from_secs(10)).unwrap().as_deref(),
                Some("Retry with backoff")
            );
        }

        #[test]
        fn the_stub_receives_the_forms_arguments() {
            // Print the final argument so the test can see the form's args
            // reach the CLI as given.
            let rx = spawn(stub(
                "args",
                "for a in \"$@\"; do last=\"$a\"; done; echo \"$last\"",
            ));
            let echoed = rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
            assert!(echoed.starts_with("Write a title"), "{echoed}");
        }

        #[test]
        fn a_failing_stub_yields_none() {
            let rx = spawn(stub("fail", "echo 'Looks fine'; exit 1"));
            assert_eq!(rx.recv_timeout(Duration::from_secs(10)).unwrap(), None);
        }

        #[test]
        fn a_silent_stub_yields_none() {
            let rx = spawn(stub("silent", "exit 0"));
            assert_eq!(rx.recv_timeout(Duration::from_secs(10)).unwrap(), None);
        }

        #[test]
        fn a_missing_program_yields_none() {
            let rx = spawn(TitleForm {
                program: "/nonexistent/ferrite-no-such-cli".into(),
                args: vec![],
                model: "stub",
                effort: "none",
            });
            assert_eq!(rx.recv_timeout(Duration::from_secs(10)).unwrap(), None);
        }

        #[test]
        fn a_hanging_stub_is_killed_at_the_deadline() {
            let started = Instant::now();
            let rx = spawn_with_timeout(
                stub("hang", "sleep 20; echo 'Too late'"),
                Duration::from_millis(300),
            );
            assert_eq!(rx.recv_timeout(Duration::from_secs(10)).unwrap(), None);
            assert!(started.elapsed() < Duration::from_secs(5));
        }
    }

    /// The real CLIs, by hand: `cargo test -p ferrite-core titler -- --ignored --nocapture`.
    /// Asserts only that some title came back, since the wording is the
    /// model's; the observed title and elapsed time go in the test output.
    mod real_clis {
        use super::*;

        fn realistic() -> TitleRequest {
            req(
                "Add a retry with exponential backoff to the HTTP client in net.rs and \
                 cover it with tests. Keep the public API unchanged.",
                Some(
                    "I'll add a `retry` wrapper around `send` with jittered exponential \
                     backoff, then unit-test the delay schedule and the give-up path.",
                ),
            )
        }

        fn observe(provider: Provider, program: &str) {
            let started = Instant::now();
            let rx = spawn(form(provider, program, &realistic()));
            let title = rx.recv_timeout(Duration::from_secs(60)).unwrap();
            eprintln!(
                "{provider:?} observed title: {title:?} in {:?}",
                started.elapsed()
            );
            let title = title.expect("the real CLI produced a title");
            assert!(title.split_whitespace().count() >= 2);
        }

        #[test]
        #[ignore = "runs the installed claude CLI and costs a model call"]
        fn claude_titles_a_thread() {
            let home = std::env::var("HOME").unwrap();
            observe(Provider::Claude, &format!("{home}/.local/bin/claude"));
        }

        #[test]
        #[ignore = "runs the installed codex CLI and costs a model call"]
        fn codex_titles_a_thread() {
            observe(Provider::Codex, "codex");
        }
    }
}
