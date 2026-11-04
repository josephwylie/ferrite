//! A Bash call's output while it runs, in the foreground or backgrounded.
//!
//! stream-json reports a Bash call's output only when the call ends, so a
//! running test suite would show no count until it finished. Claude Code
//! runs every Bash call as a shell task (`system/task_started`, `task_type`
//! `local_bash`) and writes the task's output to a file while it runs:
//! `<tmp>/claude-<uid>/<cwd slug>/<session>/tasks/<task>.output`, where
//! `<tmp>` is `CLAUDE_CODE_TMPDIR` else `/tmp`, and the slug is the cwd with
//! every character outside `[A-Za-z0-9]` made a `-` (a slug past 200
//! characters is cut there and given a hash suffix). Observed on 2.1.289;
//! the CLI removes the file when the call ends.
//!
//! Following that file turns it into the same `ToolOutputDelta`s Codex
//! streams natively, so the row's `└ running 357 tests` bar and `212/357`
//! count work on both providers. Where the file never appears (another
//! layout, a CLI that writes none) nothing is sent and the row reads as it
//! did: the call's result when it ends. The result itself always comes from
//! stream-json; these deltas only fill the wait.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::SessionEvent;

/// The longest a slug runs before the CLI cuts it and adds a hash.
const SLUG_MAX: usize = 200;
/// How often a followed file is read.
const POLL: Duration = Duration::from_millis(150);
/// How long a task's file may take to appear before following gives up.
const APPEAR_WITHIN: Duration = Duration::from_secs(20);
/// How much of one call's output is streamed; its result carries the rest.
const STREAM_LIMIT: u64 = 1 << 20;

/// The foreground shell tasks being followed, by the tool call each serves.
#[derive(Default)]
pub(super) struct ShellOutputs {
    cwd: Option<String>,
    session: Option<String>,
    /// Tool call id → the flag that stops its follower.
    following: HashMap<String, Arc<AtomicBool>>,
    /// Task id → the tool call it runs.
    tasks: HashMap<String, String>,
}

impl ShellOutputs {
    /// Read one stream-json line for what starts or ends a follow.
    pub(super) fn observe(&mut self, value: &Value, sender: &SyncSender<SessionEvent>) {
        match (value["type"].as_str(), value["subtype"].as_str()) {
            (Some("system"), Some("init")) => {
                self.cwd = value["cwd"].as_str().map(str::to_owned);
                self.session = value["session_id"].as_str().map(str::to_owned);
            }
            (Some("system"), Some("task_started")) => self.started(value, sender),
            (Some("system"), Some("task_notification" | "task_updated")) => {
                let terminal = matches!(
                    value["status"]
                        .as_str()
                        .or(value["patch"]["status"].as_str()),
                    Some("completed" | "failed" | "stopped" | "killed")
                );
                if terminal {
                    if let Some(tool) = value["task_id"]
                        .as_str()
                        .and_then(|task| self.tasks.remove(task))
                    {
                        self.stop(&tool);
                    }
                }
            }
            // A background launch's result only acknowledges the launch: the
            // shell runs on, and its notification ends the follow.
            (Some("user"), _) if value["tool_use_result"]["backgroundTaskId"].is_string() => {}
            (Some("user"), _) => {
                for block in value["message"]["content"].as_array().into_iter().flatten() {
                    if block["type"] == "tool_result" {
                        if let Some(tool) = block["tool_use_id"].as_str() {
                            self.stop(tool);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn started(&mut self, value: &Value, sender: &SyncSender<SessionEvent>) {
        let (Some(task), Some(tool)) = (value["task_id"].as_str(), value["tool_use_id"].as_str())
        else {
            return;
        };
        // Main's own calls only: a subagent's rows are its own. A
        // backgrounded call's row runs on past its turn (HD64), so its
        // output streams there too, until its notification settles it.
        let main = value["owned_by_subagent"] != true
            && matches!(value.get("parent_tool_use_id"), None | Some(Value::Null));
        if value["task_type"] != "local_bash" || !main {
            return;
        }
        let (Some(cwd), Some(session)) = (self.cwd.clone(), self.session.clone()) else {
            return;
        };
        let stop = Arc::new(AtomicBool::new(false));
        if let Some(previous) = self.following.insert(tool.to_owned(), stop.clone()) {
            previous.store(true, Ordering::Relaxed);
        }
        self.tasks.insert(task.to_owned(), tool.to_owned());
        let root = std::env::var_os("CLAUDE_CODE_TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        follow(
            Locator {
                root,
                slug: slug(&cwd),
                session,
                task: task.to_owned(),
            },
            tool.to_owned(),
            stop,
            sender.clone(),
        );
    }

    fn stop(&mut self, tool: &str) {
        if let Some(stop) = self.following.remove(tool) {
            stop.store(true, Ordering::Relaxed);
        }
    }

    /// The Session ended: every follower stops.
    pub(super) fn stop_all(&mut self) {
        for (_, stop) in self.following.drain() {
            stop.store(true, Ordering::Relaxed);
        }
        self.tasks.clear();
    }
}

/// The most of a finished background command's output a handback keeps:
/// its row holds a bounded output anyway, and the tail is where a command
/// says how it ended.
const HANDBACK_LIMIT: u64 = 64 * 1024;

/// A background task's output file, as its notification names it: the
/// tail, at most `HANDBACK_LIMIT` bytes of it, lossily decoded. `None`
/// when the file cannot be read (another machine's path, a CLI that
/// removed it).
pub(super) fn read_output(path: &str) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let skip = length.saturating_sub(HANDBACK_LIMIT);
    file.seek(SeekFrom::Start(skip)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    // A cut tail starts at the next whole line.
    Some(match (skip > 0, text.find('\n')) {
        (true, Some(at)) => text[at + 1..].to_owned(),
        _ => text.into_owned(),
    })
}

/// The cwd as the CLI names its directory: every character outside
/// `[A-Za-z0-9]` a `-`.
fn slug(cwd: &str) -> String {
    cwd.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

/// Where one task's output file is: found under whichever `claude-<uid>`
/// directory holds this Session (the session id is unique, so no uid is
/// needed to name it).
struct Locator {
    root: PathBuf,
    slug: String,
    session: String,
    task: String,
}

impl Locator {
    fn find(&self) -> Option<PathBuf> {
        let file = format!("{}.output", self.task);
        for user in std::fs::read_dir(&self.root).ok()?.flatten() {
            if !user.file_name().to_string_lossy().starts_with("claude-") {
                continue;
            }
            for project in self.project_dirs(&user.path()) {
                let path = project.join(&self.session).join("tasks").join(&file);
                if path.is_file() {
                    return Some(path);
                }
            }
        }
        None
    }

    /// The project directory the slug names: exact, or for a long cwd the
    /// cut slug with the CLI's hash after it.
    fn project_dirs(&self, user: &Path) -> Vec<PathBuf> {
        if self.slug.len() <= SLUG_MAX {
            return vec![user.join(&self.slug)];
        }
        let cut = format!("{}-", &self.slug[..SLUG_MAX]);
        std::fs::read_dir(user)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(&cut))
            .map(|entry| entry.path())
            .collect()
    }
}

/// Follow one task's file on its own thread, sending what it gains as the
/// tool call's output, until stopped, the file goes, or the Session closes.
fn follow(locator: Locator, tool: String, stop: Arc<AtomicBool>, sender: SyncSender<SessionEvent>) {
    let _ = thread::Builder::new()
        .name("ferrite-claude-shell-output".into())
        .spawn(move || {
            let begun = Instant::now();
            let mut file: Option<(PathBuf, File)> = None;
            let mut offset: u64 = 0;
            let mut partial: Vec<u8> = Vec::new();
            while !stop.load(Ordering::Relaxed) {
                if file.is_none() {
                    match locator
                        .find()
                        .and_then(|path| Some((path.clone(), File::open(path).ok()?)))
                    {
                        Some(found) => file = Some(found),
                        None if begun.elapsed() > APPEAR_WITHIN => return,
                        None => {
                            thread::sleep(POLL);
                            continue;
                        }
                    }
                }
                let Some((path, handle)) = file.as_mut() else {
                    return;
                };
                if !path.exists() {
                    return;
                }
                let mut gained = Vec::new();
                if handle.seek(SeekFrom::Start(offset)).is_err()
                    || handle.read_to_end(&mut gained).is_err()
                {
                    return;
                }
                if !gained.is_empty() {
                    offset += gained.len() as u64;
                    partial.extend_from_slice(&gained);
                    let text = take_text(&mut partial);
                    if !text.is_empty()
                        && sender
                            .send(SessionEvent::ToolOutputDelta {
                                id: tool.clone(),
                                text,
                            })
                            .is_err()
                    {
                        return;
                    }
                    if offset >= STREAM_LIMIT {
                        return;
                    }
                }
                thread::sleep(POLL);
            }
        });
}

/// The valid UTF-8 at the front of `bytes`, taken; a character cut by the
/// read stays for the next.
fn take_text(bytes: &mut Vec<u8>) -> String {
    let valid = match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(error) if error.error_len().is_none() => error.valid_up_to(),
        Err(_) => {
            let text = String::from_utf8_lossy(bytes).into_owned();
            bytes.clear();
            return text;
        }
    };
    let rest = bytes.split_off(valid);
    String::from_utf8(std::mem::replace(bytes, rest)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slug_is_the_cwd_with_every_other_character_a_dash() {
        assert_eq!(
            slug("/Users/me/Desktop/Projects/ferrite/.worktrees/ui-overhaul"),
            "-Users-me-Desktop-Projects-ferrite--worktrees-ui-overhaul"
        );
        assert_eq!(
            slug("/private/tmp/ferrite-live/repo"),
            "-private-tmp-ferrite-live-repo"
        );
    }

    /// A handback reads the task's whole output file, or the tail of a
    /// long one from its next whole line; a missing file reads as nothing.
    #[test]
    fn a_handback_reads_the_tail_of_the_output_file() {
        let dir = std::env::temp_dir().join(format!("ferrite-handback-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let short = dir.join("short.output");
        std::fs::write(&short, "bg-done\n").unwrap();
        assert_eq!(read_output(short.to_str().unwrap()).as_deref(), Some("bg-done\n"));
        let long = dir.join("long.output");
        let line = "x".repeat(99) + "\n";
        std::fs::write(&long, line.repeat(1000) + "last\n").unwrap();
        let tail = read_output(long.to_str().unwrap()).unwrap();
        assert!(tail.len() as u64 <= HANDBACK_LIMIT);
        assert!(tail.starts_with('x') && tail.ends_with("last\n"));
        assert_eq!(read_output(dir.join("gone").to_str().unwrap()), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cut_character_waits_for_the_next_read() {
        let mut bytes = "ok \u{2713}".as_bytes().to_vec();
        let last = bytes.pop().unwrap();
        assert_eq!(take_text(&mut bytes), "ok ");
        bytes.push(last);
        assert_eq!(take_text(&mut bytes), "\u{2713}");
        assert!(bytes.is_empty());
    }

    /// A shell task's file streams as its call's output, a backgrounded
    /// one's too: its launch result leaves it running, and only its
    /// notification ends the follow.
    #[test]
    fn a_foreground_shell_streams_its_file_as_the_calls_output() {
        let root =
            std::env::temp_dir().join(format!("ferrite-shell-output-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let tasks = root
            .join("claude-501")
            .join(slug("/work/repo"))
            .join("session-1")
            .join("tasks");
        std::fs::create_dir_all(&tasks).unwrap();
        std::fs::write(tasks.join("b1.output"), "running 2 tests\ntest a ... ok\n").unwrap();
        let (sender, events) = std::sync::mpsc::sync_channel(16);
        let stop = Arc::new(AtomicBool::new(false));
        follow(
            Locator {
                root: root.clone(),
                slug: slug("/work/repo"),
                session: "session-1".into(),
                task: "b1".into(),
            },
            "toolu_1".into(),
            stop.clone(),
            sender,
        );
        let first = events.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            first,
            SessionEvent::ToolOutputDelta {
                id: "toolu_1".into(),
                text: "running 2 tests\ntest a ... ok\n".into(),
            }
        );
        stop.store(true, Ordering::Relaxed);
        let _ = std::fs::remove_dir_all(&root);

        let mut outputs = ShellOutputs::default();
        let (sender, events) = std::sync::mpsc::sync_channel(16);
        outputs.observe(
            &serde_json::json!({"type":"system","subtype":"init","cwd":"/work/repo","session_id":"s"}),
            &sender,
        );
        outputs.observe(
            &serde_json::json!({"type":"system","subtype":"task_started","task_id":"b2","tool_use_id":"toolu_2","task_type":"local_bash","is_backgrounded":true}),
            &sender,
        );
        assert!(outputs.following.contains_key("toolu_2"));
        outputs.observe(
            &serde_json::json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_2","content":"Command running in background with ID: b2."}]},"tool_use_result":{"stdout":"","backgroundTaskId":"b2"}}),
            &sender,
        );
        assert!(
            outputs.following.contains_key("toolu_2"),
            "the launch is not the end"
        );
        outputs.observe(
            &serde_json::json!({"type":"system","subtype":"task_notification","task_id":"b2","tool_use_id":"toolu_2","status":"completed"}),
            &sender,
        );
        assert!(!outputs.following.contains_key("toolu_2"));
        outputs.observe(
            &serde_json::json!({"type":"system","subtype":"task_started","task_id":"b3","tool_use_id":"toolu_3","task_type":"local_bash","is_backgrounded":false}),
            &sender,
        );
        assert!(outputs.following.contains_key("toolu_3"));
        outputs.observe(
            &serde_json::json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_3","content":"done"}]}}),
            &sender,
        );
        assert!(outputs.following.is_empty(), "the result ends the follow");
        drop(events);
    }
}
