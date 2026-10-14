//! The operator's own palette commands (Settings `commands`): a name and
//! what to run — one shell line, or an argv run as it stands — in the
//! focused Thread's working directory, its output said in a toast or shown
//! in a reader. `open in editor`, `run tests`, `gh pr create --fill`.
//!
//! A command runs once to its end with no terminal: stdin is empty, so a
//! program that would stop to ask fails instead of waiting for an answer
//! nobody can type.

use std::path::Path;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

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
/// /c`), or an argv whose first word is the program.
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
                let mut command = Command::new(program);
                command.args(args);
                Some(command)
            }
        }
    }
}

impl UserCommand {
    /// Run to the end in `cwd`, `FERRITE_THREAD_ID` naming `thread`.
    /// Blocks: call it off the UI thread.
    pub fn run(&self, cwd: &Path, thread: ThreadId) -> Ran {
        let Some(mut command) = self.run.command() else {
            return Ran {
                ok: false,
                output: "nothing to run: `run` is empty".into(),
            };
        };
        let result = command
            .current_dir(cwd)
            .env("FERRITE_THREAD_ID", thread.to_string())
            .stdin(Stdio::null())
            .no_console_window()
            .output();
        match result {
            Ok(out) => {
                let ok = out.status.success();
                let code = out.status.code().filter(|_| !ok);
                let parts = [
                    String::from_utf8_lossy(&out.stdout).trim().to_string(),
                    String::from_utf8_lossy(&out.stderr).trim().to_string(),
                    code.map(|code| format!("exit {code}")).unwrap_or_default(),
                ];
                Ran {
                    ok,
                    output: parts
                        .into_iter()
                        .filter(|part| !part.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n"),
                }
            }
            Err(error) => Ran {
                ok: false,
                output: format!("could not start `{}`: {error}", self.run.label()),
            },
        }
    }
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
        let ran = command(Run::Shell("pwd; echo \"$FERRITE_THREAD_ID\"".into()))
            .run(&dir, ThreadId::new(42));
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
        .run(&dir, ThreadId::new(1));
        assert!(!ran.ok);
        assert_eq!(ran.output, "out\nbroke\nexit 3");
        assert_eq!(ran.tail(2), "broke\nexit 3");

        let missing =
            command(Run::Argv(vec!["ferrite-no-such-program".into()])).run(&dir, ThreadId::new(1));
        assert!(!missing.ok);
        assert!(missing.output.starts_with("could not start"), "{missing:?}");

        let empty = command(Run::Argv(vec![])).run(&dir, ThreadId::new(1));
        assert!(!empty.ok);
    }

    /// No terminal: a program that reads stdin sees it end at once.
    #[cfg(unix)]
    #[test]
    fn stdin_is_empty() {
        let dir = scratch("stdin");
        let ran = command(Run::Shell("cat; echo done".into())).run(&dir, ThreadId::new(1));
        assert_eq!(ran.output, "done");
    }
}
