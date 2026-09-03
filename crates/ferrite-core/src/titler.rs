//! A model-written Title for a Thread.
//!
//! The first line of the first prompt is a poor name for a Thread once the
//! wall holds a dozen of them ("hey can you look at", "ok so"). So after
//! the first exchange, a small model is asked for a 3–6 word title in the
//! background. Ferrite never calls model APIs itself (CONTEXT.md), so this
//! runs the Claude CLI in print mode — the same official harness a Session
//! uses — on a throwaway std thread, and hands back Some(title) or, on any
//! failure, None. The prompt-derived title stays in place until then.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

/// The model alias the title is asked of — the cheapest one, so a Thread's
/// name costs nothing an operator would notice.
pub const MODEL: &str = "haiku";
/// The effort level passed with the model, for the same reason.
pub const EFFORT: &str = "low";

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

/// The instruction text sent as the CLI's positional prompt.
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

/// Ask `program` (the Claude CLI) for a title on a background thread. The
/// receiver yields exactly one value: Some(title), or None when the CLI is
/// missing, exits non-zero, prints nothing usable, or outlives `TIMEOUT`
/// (it is killed). The receiver is dropped without a value only if the
/// thread panics.
pub fn spawn(program: &str, req: TitleRequest) -> Receiver<Option<String>> {
    spawn_with_timeout(program, req, TIMEOUT)
}

/// [`spawn`] with the kill deadline chosen by the caller, so a test can
/// prove the kill without waiting thirty seconds.
pub fn spawn_with_timeout(
    program: &str,
    req: TitleRequest,
    timeout: Duration,
) -> Receiver<Option<String>> {
    let (tx, rx) = mpsc::channel();
    let program = program.to_string();
    thread::Builder::new()
        .name("ferrite-titler".into())
        .spawn(move || {
            let title = run(&program, &req, timeout);
            let _ = tx.send(title);
        })
        .expect("spawn titler thread");
    rx
}

/// The CLI arguments after the program: print mode, the cheap model, text
/// output, no tools (the title must not be a Bash call), no saved session
/// (a title turn is not a conversation to resume), no settings sources (so
/// no project or user hooks run in the throwaway directory), and nobody to
/// answer prompts (`--tools ""` should leave none, but a prompt that did
/// appear must be denied rather than hang). Each flag verified against
/// `claude --help` of 2.1.259; `--max-turns` does not exist there, and the
/// tool-less turn is single anyway.
fn args(prompt: &str) -> Vec<String> {
    [
        "-p",
        "--model",
        MODEL,
        "--effort",
        EFFORT,
        "--output-format",
        "text",
        "--tools",
        "",
        "--no-session-persistence",
        "--setting-sources",
        "",
        "--permission-prompts",
        "none",
        prompt,
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// An empty directory for the CLI's cwd, so no project CLAUDE.md or
/// `.claude/` settings are discovered. One per process; the CLI writes
/// nothing there with persistence off.
fn scratch_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ferrite-titler-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn run(program: &str, req: &TitleRequest, timeout: Duration) -> Option<String> {
    let mut child = Command::new(program)
        .args(args(&title_prompt(req)))
        .current_dir(scratch_dir())
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

    #[cfg(unix)]
    mod with_stub_programs {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        /// A shell script standing in for the CLI. It ignores its flags and
        /// runs `body`, so a test controls output, exit code and duration.
        fn stub(name: &str, body: &str) -> PathBuf {
            let dir = std::env::temp_dir()
                .join(format!("ferrite-titler-stub-{}-{name}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("claude");
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }

        #[test]
        fn a_printing_stub_yields_the_cleaned_title() {
            let path = stub("ok", "echo '\"Retry with backoff.\"'");
            let rx = spawn(path.to_str().unwrap(), req("anything", None));
            assert_eq!(
                rx.recv_timeout(Duration::from_secs(10)).unwrap().as_deref(),
                Some("Retry with backoff")
            );
        }

        #[test]
        fn the_stub_receives_the_prompt_as_the_last_argument() {
            // Print the final argument so the test can see the CLI got the
            // instruction text positionally, after the flags.
            let path = stub(
                "args",
                "for a in \"$@\"; do last=\"$a\"; done; echo \"$last\"",
            );
            let rx = spawn(path.to_str().unwrap(), req("Ship the widget", None));
            let echoed = rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
            assert!(echoed.starts_with("Write a title"), "{echoed}");
        }

        #[test]
        fn a_failing_stub_yields_none() {
            let path = stub("fail", "echo 'Looks fine'; exit 1");
            let rx = spawn(path.to_str().unwrap(), req("x", None));
            assert_eq!(rx.recv_timeout(Duration::from_secs(10)).unwrap(), None);
        }

        #[test]
        fn a_silent_stub_yields_none() {
            let path = stub("silent", "exit 0");
            let rx = spawn(path.to_str().unwrap(), req("x", None));
            assert_eq!(rx.recv_timeout(Duration::from_secs(10)).unwrap(), None);
        }

        #[test]
        fn a_missing_program_yields_none() {
            let rx = spawn("/nonexistent/ferrite-no-such-cli", req("x", None));
            assert_eq!(rx.recv_timeout(Duration::from_secs(10)).unwrap(), None);
        }

        #[test]
        fn a_hanging_stub_is_killed_at_the_deadline() {
            let path = stub("hang", "sleep 20; echo 'Too late'");
            let started = Instant::now();
            let rx = spawn_with_timeout(
                path.to_str().unwrap(),
                req("x", None),
                Duration::from_millis(300),
            );
            assert_eq!(rx.recv_timeout(Duration::from_secs(10)).unwrap(), None);
            assert!(started.elapsed() < Duration::from_secs(5));
        }
    }

    /// The real CLI, once, by hand: `cargo test -p ferrite-core titler -- --ignored`.
    /// Asserts only that some title came back, since the wording is the
    /// model's; the observed title and elapsed time go in the test output.
    #[test]
    #[ignore = "runs the installed claude CLI and costs a model call"]
    fn the_real_cli_titles_a_thread() {
        let home = std::env::var("HOME").unwrap();
        let program = format!("{home}/.local/bin/claude");
        let started = Instant::now();
        let rx = spawn(
            &program,
            req(
                "Add a retry with exponential backoff to the HTTP client in net.rs and \
                 cover it with tests. Keep the public API unchanged.",
                Some(
                    "I'll add a `retry` wrapper around `send` with jittered exponential backoff, \
                      then unit-test the delay schedule and the give-up path.",
                ),
            ),
        );
        let title = rx.recv_timeout(Duration::from_secs(60)).unwrap();
        eprintln!("observed title: {title:?} in {:?}", started.elapsed());
        let title = title.expect("the real CLI produced a title");
        assert!(title.split_whitespace().count() >= 2);
    }
}
