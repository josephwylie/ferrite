//! The operator's own palette commands (Settings `commands`): a name and
//! what to run — one shell line, or an argv run as it stands — in the
//! focused Thread's working directory, its output said in a toast or shown
//! in a reader. `open in editor`, `run tests`, `gh pr create --fill`.
//!
//! A command runs once to its end with no terminal: stdin is empty, so a
//! program that would stop to ask fails instead of waiting for an answer
//! nobody can type. One that never ends on its own (a watcher, a hung
//! test) runs until the operator stops it; only the end of each stream is
//! kept, so a chatty one cannot fill memory.

use std::collections::VecDeque;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::providers::spawnable_program;
use crate::spawn::NoConsoleWindow as _;
use crate::ThreadId;

/// One command, as `settings.json` spells it:
/// `{ "name": "run tests", "run": "cargo test", "output": "reader" }`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserCommand {
    /// The palette row's name.
    pub name: String,
    pub run: Run,
    #[serde(default)]
    pub output: Output,
}

/// What a command runs: a line for the platform shell (`sh -c`, `cmd /d
/// /c`), or an argv whose first word is the program (on Windows, a bare
/// name found as a `.cmd` or `.bat` shim on PATH runs as that shim).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Run {
    Shell(String),
    Argv(Vec<String>),
}

/// Where a finished command's output goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Output {
    /// A toast: whether it worked, and the output's last lines.
    #[default]
    Toast,
    /// The whole output in a reader beside the Pane.
    Reader,
}

/// How a run ended: whether it succeeded, and what it printed (stdout,
/// then stderr), or why it never started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ran {
    pub ok: bool,
    pub output: String,
}

impl Run {
    /// The line the palette shows after the name.
    pub fn label(&self) -> String {
        match self {
            Run::Shell(line) => line.clone(),
            Run::Argv(argv) => argv.join(" "),
        }
    }

    fn command(&self) -> Option<Command> {
        match self {
            Run::Shell(line) if cfg!(windows) => {
                let mut command = Command::new("cmd");
                command.args(["/d", "/c", line]);
                Some(command)
            }
            Run::Shell(line) => {
                let mut command = Command::new("sh");
                command.args(["-c", line]);
                Some(command)
            }
            Run::Argv(argv) => {
                let (program, args) = argv.split_first()?;
                // `code` is `code.cmd` on Windows, as the npm CLIs are.
                let mut command = Command::new(spawnable_program(program));
                command.args(args);
                Some(command)
            }
        }
    }
}

/// How often a running command is checked for its end or a stop.
const POLL: Duration = Duration::from_millis(50);
/// How much of each stream a run keeps: the end, where a failure says why.
const KEPT: usize = 1 << 20;
/// How long a command's pipes may stay open after it ends — held by
/// something it left running in the background — before the output read
/// so far is taken as all of it.
const DRAIN: Duration = Duration::from_millis(500);

#[derive(Clone, Copy)]
enum Stream {
    Out,
    Err,
}

/// The end of one stream: the last `KEPT` bytes, and whether any were let
/// go. A ring, so letting the front go moves nothing that is kept.
#[derive(Default)]
struct Tail {
    bytes: VecDeque<u8>,
    cut: bool,
}

impl Tail {
    fn push(&mut self, chunk: &[u8]) {
        self.bytes.extend(chunk);
        let over = self.bytes.len().saturating_sub(KEPT);
        if over > 0 {
            self.bytes.drain(..over);
            self.cut = true;
        }
    }

    /// The kept text, trimmed; a cut starts at the first whole line.
    fn text(mut self) -> String {
        let text = String::from_utf8_lossy(self.bytes.make_contiguous());
        if !self.cut {
            return text.trim().to_string();
        }
        let whole = text.split_once('\n').map_or("", |(_, rest)| rest);
        format!("[earlier output dropped]\n{}", whole.trim())
    }
}

impl UserCommand {
    /// Run to the end in `cwd`, `FERRITE_THREAD_ID` naming `thread`, or
    /// until `stop` is set, which ends it and what it started. Blocks: call
    /// it on a thread of its own.
    pub fn run(&self, cwd: &Path, thread: ThreadId, stop: &AtomicBool) -> Ran {
        let Some(mut command) = self.run.command() else {
            return Ran {
                ok: false,
                output: "nothing to run: `run` is empty".into(),
            };
        };
        command
            .current_dir(cwd)
            .env("FERRITE_THREAD_ID", thread.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .no_console_window();
        // A group of its own, so a stop reaches whatever the shell started.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                return Ran {
                    ok: false,
                    output: format!("could not start `{}`: {error}", self.run.label()),
                }
            }
        };
        let (tx, rx) = mpsc::channel();
        if let Some(pipe) = child.stdout.take() {
            read(pipe, Stream::Out, tx.clone());
        }
        if let Some(pipe) = child.stderr.take() {
            read(pipe, Stream::Err, tx);
        }
        let (mut out, mut err) = (Tail::default(), Tail::default());
        let mut keep = |(stream, chunk): (Stream, Vec<u8>)| match stream {
            Stream::Out => out.push(&chunk),
            Stream::Err => err.push(&chunk),
        };
        let mut stopped = false;
        let status = loop {
            rx.try_iter().for_each(&mut keep);
            if !stopped && stop.load(Ordering::Relaxed) {
                stopped = true;
                end(&mut child);
            }
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => thread::sleep(POLL),
                Err(_) => break None,
            }
        };
        let drained = Instant::now() + DRAIN;
        while let Ok(chunk) = rx.recv_timeout(drained.saturating_duration_since(Instant::now())) {
            keep(chunk);
        }
        let ok = !stopped && status.is_some_and(|status| status.success());
        let ending = if stopped {
            "stopped".to_string()
        } else {
            status
                .and_then(|status| status.code())
                .filter(|_| !ok)
                .map(|code| format!("exit {code}"))
                .unwrap_or_default()
        };
        Ran {
            ok,
            output: [out.text(), err.text(), ending]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

/// Read `pipe` to its end on a thread of its own, sending what comes.
fn read(mut pipe: impl Read + Send + 'static, stream: Stream, tx: mpsc::Sender<(Stream, Vec<u8>)>) {
    thread::spawn(move || {
        let mut buffer = [0; 8192];
        while let Ok(read @ 1..) = pipe.read(&mut buffer) {
            if tx.send((stream, buffer[..read].to_vec())).is_err() {
                break;
            }
        }
    });
}

/// End a running command: its whole process group on Unix. On Windows
/// only the child (`cmd` for a shell line) is reached; its pipes held by a
/// survivor are what `DRAIN` bounds.
fn end(child: &mut Child) {
    #[cfg(unix)]
    {
        // SAFETY: a plain syscall on the group `process_group(0)` made,
        // whose id is the child's pid; the child is not yet reaped.
        unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL) };
    }
    #[cfg(not(unix))]
    let _ = child.kill();
}

impl Ran {
    /// The last `lines` non-blank lines, what a toast has room for.
    pub fn tail(&self, lines: usize) -> String {
        let kept: Vec<&str> = self
            .output
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        kept[kept.len().saturating_sub(lines)..].join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ferrite-user-command-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::canonicalize(dir).unwrap()
    }

    fn command(run: Run) -> UserCommand {
        UserCommand {
            name: "test".into(),
            run,
            output: Output::Toast,
        }
    }

    /// Both spellings load: a string is a shell line, an array an argv;
    /// `output` defaults to a toast.
    #[test]
    fn a_command_is_a_shell_line_or_an_argv() {
        let commands: Vec<UserCommand> = serde_json::from_str(
            r#"[
                {"name": "run tests", "run": "cargo test", "output": "reader"},
                {"name": "open in editor", "run": ["code", "."]}
            ]"#,
        )
        .unwrap();
        assert_eq!(commands[0].run, Run::Shell("cargo test".into()));
        assert_eq!(commands[0].output, Output::Reader);
        assert_eq!(commands[1].run, Run::Argv(vec!["code".into(), ".".into()]));
        assert_eq!(commands[1].output, Output::Toast);
        assert_eq!(commands[1].run.label(), "code .");
    }

    /// The command stands in the Thread's directory and is told which
    /// Thread it runs for.
    #[cfg(unix)]
    #[test]
    fn a_command_runs_in_the_threads_directory_with_its_id() {
        let dir = scratch("cwd");
        let ran = command(Run::Shell("pwd; echo \"$FERRITE_THREAD_ID\"".into())).run(
            &dir,
            ThreadId::new(42),
            &AtomicBool::new(false),
        );
        assert!(ran.ok, "{ran:?}");
        assert_eq!(ran.output, format!("{}\n42", dir.display()));
    }

    /// A failure says so, keeps stderr and the exit code; a program that
    /// is not there says it could not start.
    #[cfg(unix)]
    #[test]
    fn a_failure_keeps_its_stderr_and_exit_code() {
        let dir = scratch("fail");
        let ran = command(Run::Argv(vec![
            "sh".into(),
            "-c".into(),
            "echo out; echo broke >&2; exit 3".into(),
        ]))
        .run(&dir, ThreadId::new(1), &AtomicBool::new(false));
        assert!(!ran.ok);
        assert_eq!(ran.output, "out\nbroke\nexit 3");
        assert_eq!(ran.tail(2), "broke\nexit 3");

        let missing = command(Run::Argv(vec!["ferrite-no-such-program".into()])).run(
            &dir,
            ThreadId::new(1),
            &AtomicBool::new(false),
        );
        assert!(!missing.ok);
        assert!(missing.output.starts_with("could not start"), "{missing:?}");

        let empty = command(Run::Argv(vec![])).run(&dir, ThreadId::new(1), &AtomicBool::new(false));
        assert!(!empty.ok);
    }

    /// No terminal: a program that reads stdin sees it end at once.
    #[cfg(unix)]
    #[test]
    fn stdin_is_empty() {
        let dir = scratch("stdin");
        let ran = command(Run::Shell("cat; echo done".into())).run(
            &dir,
            ThreadId::new(1),
            &AtomicBool::new(false),
        );
        assert_eq!(ran.output, "done");
    }

    /// A command that never ends runs until it is stopped, and the stop
    /// reaches what its shell started: nothing is left holding the pipes.
    #[cfg(unix)]
    #[test]
    fn a_stop_ends_the_command_and_what_it_started() {
        let dir = scratch("stop");
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let stopper = {
            let stop = stop.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(200));
                stop.store(true, Ordering::Relaxed);
            })
        };
        let started = Instant::now();
        let ran = command(Run::Shell(
            "echo up; (sleep 1; touch survived) & sleep 60".into(),
        ))
        .run(&dir, ThreadId::new(1), &stop);
        stopper.join().unwrap();
        assert!(!ran.ok);
        assert_eq!(ran.output, "up\nstopped");
        assert!(started.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(1500));
        assert!(
            !dir.join("survived").exists(),
            "the background job lived on"
        );
    }

    /// Only the end of a long output is kept, from its first whole line.
    #[cfg(unix)]
    #[test]
    fn only_the_end_of_a_long_output_is_kept() {
        let dir = scratch("long");
        let ran = command(Run::Shell("seq 1 400000".into())).run(
            &dir,
            ThreadId::new(1),
            &AtomicBool::new(false),
        );
        assert!(ran.ok);
        assert!(ran.output.len() <= KEPT + 32, "{}", ran.output.len());
        assert!(ran.output.starts_with("[earlier output dropped]\n"));
        assert!(ran.output.ends_with("\n400000"));
        let first = ran.output.lines().nth(1).unwrap();
        let next = ran.output.lines().nth(2).unwrap();
        assert_eq!(
            first.parse::<u32>().unwrap() + 1,
            next.parse::<u32>().unwrap()
        );
    }
}
