//! The kit the scripted worlds are built with (`parity`, the prototype's;
//! `mockup`, the UI refresh's): a fixture clock the world is scripted in,
//! scripted provider Sessions, a timeline of beats played in order, the
//! events a provider says, and real git checkouts under a disposable HOME.
//! Nothing here touches anything global: the caller owns HOME and the
//! clock.

#![cfg_attr(not(feature = "visual-reference"), allow(dead_code))]

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::process::Command;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ferrite_core::cockpit::{Cockpit, SpawnRequest, Spawner};
use ferrite_core::groups::{GroupChange, GroupId};
use ferrite_core::progress::{Phase, ProgressEvent};
use ferrite_core::providers::{applied, Session, SettingAck};
use ferrite_core::store::Provider;
use ferrite_core::workspace::WorkspaceChoice;
use ferrite_core::{
    Decision, DecisionAnswer, DecisionChoice, DecisionKind, Hunk, ModelInfo, PermissionModeChoice,
    QueueEvent, QueuedPrompt, RateLimitWindow, SessionEvent, ThreadId, ToolResult, TurnOutcome,
    UsageDetails, UsageScope,
};

// ------------------------------------------------------------------ clock

/// The process clock's fixture (`ferrite_core::clock::Fixture`), moved in
/// the `SystemTime`s the world is scripted in.
pub(crate) struct Fixture(ferrite_core::clock::Fixture);

impl Fixture {
    /// Freeze every visible time at `at` until dropped.
    pub(crate) fn install(at: SystemTime) -> Self {
        Fixture(ferrite_core::clock::Fixture::install(at.into()))
    }

    /// Move the clock to `at`.
    pub(crate) fn set(&self, at: SystemTime) {
        self.0.set(at.into());
    }
}

/// A wall-clock time of the capture's day, in seconds since local midnight:
/// a world is scripted in the times its picture prints.
pub(crate) type Secs = i64;

pub(crate) const fn hms(h: i64, m: i64, s: i64) -> Secs {
    h * 3600 + m * 60 + s
}

pub(crate) const MINUTE: Secs = 60;
pub(crate) const HOUR: Secs = 3600;
pub(crate) const DAY: Secs = 24 * HOUR;

/// Local midnight of today, from the core's own wall-clock label, so the
/// app crate needs no timezone library. Read it before a fixture clock is
/// installed: once one is, the label is the fixture's.
pub(crate) fn midnight() -> SystemTime {
    loop {
        let before = SystemTime::now();
        let label = ferrite_core::progress::clock_label();
        let after = SystemTime::now();
        let seconds = |at: SystemTime| {
            at.duration_since(UNIX_EPOCH)
                .map(|since| since.as_secs())
                .unwrap_or_default()
        };
        // A label read across a minute's edge says either minute; ask again.
        if seconds(before) / 60 != seconds(after) / 60 {
            continue;
        }
        let Some(local) = label_seconds(&label) else {
            // An unreadable label: the epoch's midnight still gives every
            // stamp its printed minute in a UTC zone, and no scene panics.
            return before - Duration::from_secs(seconds(before) % 86_400);
        };
        // Zones are whole minutes off UTC, so the second is the epoch's.
        let local = local + (seconds(before) % 60) as i64;
        return before - Duration::from_secs(local as u64);
    }
}

/// `7:41 pm` → the minute's seconds since midnight.
fn label_seconds(label: &str) -> Option<i64> {
    let (clock, meridiem) = label.trim().split_once(' ')?;
    let (hour, minute) = clock.split_once(':')?;
    let hour: i64 = hour.parse().ok()?;
    let minute: i64 = minute.parse().ok()?;
    let hour = hour % 12
        + if meridiem.eq_ignore_ascii_case("pm") {
            12
        } else {
            0
        };
    Some(hms(hour, minute, 0))
}

/// The world's clock: where midnight was, and the fixture to move.
#[derive(Clone, Copy)]
pub(crate) struct Clock<'a> {
    pub midnight: SystemTime,
    pub fixture: Option<&'a Fixture>,
}

impl Clock<'_> {
    /// The instant `secs` names (negative: before today).
    pub(crate) fn instant(&self, secs: Secs) -> SystemTime {
        if secs >= 0 {
            self.midnight + Duration::from_secs(secs as u64)
        } else {
            self.midnight - Duration::from_secs(secs.unsigned_abs())
        }
    }

    pub(crate) fn set(&self, secs: Secs) {
        if let Some(fixture) = self.fixture {
            fixture.set(self.instant(secs));
        }
    }
}

// ---------------------------------------------------------------- sessions

pub(crate) type Feeds = Rc<RefCell<Vec<mpsc::Sender<SessionEvent>>>>;

/// The provider side of each Thread's latest Session, by Thread.
type Sides = Rc<RefCell<HashMap<ThreadId, mpsc::Sender<SessionEvent>>>>;

/// Every Session a world's spawner started: what a scene keeps alive, and
/// each Thread's latest, which the timeline speaks for. A Session the
/// Engine ended at rest is started again by the next prompt, so a Thread's
/// side is whichever spawned last.
#[derive(Default)]
pub(crate) struct Sessions {
    feeds: Feeds,
    sides: Sides,
}

impl Sessions {
    /// Every Session's provider side, in spawn order.
    pub(crate) fn feeds(&self) -> Feeds {
        self.feeds.clone()
    }

    /// The spawner that starts this world's Sessions.
    pub(crate) fn spawner(&self) -> Spawn {
        Spawn {
            feeds: self.feeds.clone(),
            sides: self.sides.clone(),
        }
    }

    /// Open a Thread titled `title` on `workspace`.
    pub(crate) fn open(
        &self,
        core: &mut Cockpit,
        provider: Provider,
        workspace: WorkspaceChoice,
        title: &str,
    ) -> ThreadId {
        let thread = core
            .open(provider, workspace)
            .expect("open a fixture Thread");
        core.rename_thread(thread, title)
            .expect("name a fixture Thread");
        thread
    }

    /// The provider side of `thread`'s latest Session.
    pub(crate) fn feed(&self, thread: ThreadId) -> mpsc::Sender<SessionEvent> {
        self.sides
            .borrow()
            .get(&thread)
            .cloned()
            .expect("every fixture Thread has a Session")
    }
}

/// Hands every spawn a fresh channel; the world keeps the sending half.
pub(crate) struct Spawn {
    feeds: Feeds,
    sides: Sides,
}

impl Spawn {
    /// A spawner filling `feeds`.
    pub(crate) fn new(feeds: Feeds) -> Self {
        Spawn {
            feeds,
            sides: Sides::default(),
        }
    }
}

/// A scripted provider: it says what the timeline feeds it and nothing else.
struct Scripted {
    rx: mpsc::Receiver<SessionEvent>,
    tx: mpsc::Sender<SessionEvent>,
    provider: Provider,
}

impl Spawner for Spawn {
    fn spawn(&mut self, request: SpawnRequest) -> io::Result<Box<dyn Session>> {
        let (tx, rx) = mpsc::channel();
        // The day's clock jumps hours between beats, and the engine ends a
        // Session that rests that long (ADR 0032): a scripted one holds
        // scheduled work, so it lives until the day is played.
        tx.send(SessionEvent::ScheduledWork { pending: true })
            .expect("the receiver is held below");
        self.feeds.borrow_mut().push(tx.clone());
        self.sides.borrow_mut().insert(request.thread, tx.clone());
        Ok(Box::new(Scripted {
            rx,
            tx,
            provider: request.provider,
        }))
    }
}

impl Session for Scripted {
    fn events(&self) -> &mpsc::Receiver<SessionEvent> {
        &self.rx
    }
    fn send(&mut self, _: &str) -> io::Result<()> {
        Ok(())
    }
    /// The provider accepts every held prompt, as Claude's native queue does.
    fn enqueue(&mut self, id: &str, text: &str) -> io::Result<()> {
        let _ = self
            .tx
            .send(SessionEvent::Queue(QueueEvent::Accepted(QueuedPrompt {
                id: id.into(),
                client_id: id.into(),
                text: text.into(),
            })));
        Ok(())
    }
    /// A chosen effort lands on the live Session, as both CLIs allow. The
    /// model stays unsupported: chosen before the first prompt it respawns
    /// the Session fresh, which is how Nav rows gets `opus[1m]` and
    /// `medium` without a notice in its transcript.
    fn set_effort(&mut self, _: Option<&str>) -> io::Result<SettingAck> {
        Ok(applied())
    }
    fn permission_modes(&self) -> Vec<PermissionModeChoice> {
        let modes: &[(&str, &str)] = match self.provider {
            Provider::Claude => &[
                ("default", "Default"),
                ("acceptEdits", "Accept edits"),
                ("plan", "Plan"),
                ("bypassPermissions", "Bypass permissions"),
            ],
            Provider::Codex => &[
                ("untrusted", "Untrusted"),
                ("on-request", "On request"),
                ("never", "Never"),
            ],
        };
        modes
            .iter()
            .map(|(value, label)| PermissionModeChoice {
                value: (*value).into(),
                label: (*label).into(),
            })
            .collect()
    }
    fn interrupt(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn respond_to_decision(&mut self, _: &str, _: DecisionAnswer) -> io::Result<()> {
        Ok(())
    }
}

/// What one Thread does at one moment of the timeline. A fixture's script,
/// built once per capture: an event's size is no concern here.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Act {
    /// The operator sends a line.
    Prompt(String),
    /// The provider says something.
    Event(SessionEvent),
    /// The operator parks the Thread.
    Park,
}

pub(crate) struct Beat {
    pub(crate) at: Secs,
    pub(crate) who: ThreadId,
    pub(crate) act: Act,
}

/// The timeline under construction.
#[derive(Default)]
pub(crate) struct Script {
    pub(crate) beats: Vec<Beat>,
}

impl Script {
    pub(crate) fn prompt(&mut self, at: Secs, who: ThreadId, text: impl Into<String>) {
        self.beats.push(Beat {
            at,
            who,
            act: Act::Prompt(text.into()),
        });
    }

    pub(crate) fn ev(&mut self, at: Secs, who: ThreadId, event: SessionEvent) {
        self.beats.push(Beat {
            at,
            who,
            act: Act::Event(event),
        });
    }

    pub(crate) fn park(&mut self, at: Secs, who: ThreadId) {
        self.beats.push(Beat {
            at,
            who,
            act: Act::Park,
        });
    }

    /// A tool call that starts and settles.
    pub(crate) fn tool(
        &mut self,
        (start, end): (Secs, Secs),
        who: ThreadId,
        id: &str,
        name: &str,
        input: serde_json::Value,
        (output, is_error, result): Outcome,
    ) {
        self.ev(start, who, started(id, name, input));
        self.ev(end, who, completed(id, &output, is_error, result));
    }

    /// A shell command that ran from `start` for `ms`.
    pub(crate) fn bash(&mut self, start: Secs, who: ThreadId, id: &str, command: &str, run: Run) {
        let end = start + (run.ms / 1000) as Secs;
        self.tool(
            (start, end),
            who,
            id,
            "Bash",
            serde_json::json!({ "command": command }),
            (
                run.stdout.clone(),
                run.exit != 0,
                ToolResult::Command {
                    stdout: run.stdout.clone(),
                    stderr: String::new(),
                    exit_code: Some(run.exit),
                    duration_ms: Some(run.ms),
                },
            ),
        );
    }

    /// An edit the provider applied, with its structured patch.
    pub(crate) fn edit(&mut self, at: Secs, who: ThreadId, id: &str, path: &str, hunks: Vec<Hunk>) {
        self.tool(
            (at, at + 1),
            who,
            id,
            "Edit",
            serde_json::json!({ "file_path": path }),
            (
                String::new(),
                false,
                ToolResult::FileEdit {
                    path: path.into(),
                    hunks,
                },
            ),
        );
    }

    /// A completed turn: the provider's accounting, then its end.
    pub(crate) fn end(&mut self, at: Secs, who: ThreadId) {
        self.ev(
            at,
            who,
            SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: None,
            },
        );
    }
}

/// What a call handed back: its output, whether it failed, and the
/// provider's structured result.
pub(crate) type Outcome = (String, bool, ToolResult);

/// A call that settled with nothing to say.
pub(crate) fn quiet() -> Outcome {
    (String::new(), false, ToolResult::Opaque)
}

/// One command's run: what it printed, how it exited, how long it took.
pub(crate) struct Run {
    pub(crate) stdout: String,
    pub(crate) exit: i64,
    pub(crate) ms: u64,
}

/// Run git in `dir` as the fixture's own user, under the disposable HOME:
/// the operator's config, hooks and signing never apply.
pub(crate) fn git(dir: &Path, home: &Path, args: &[&str]) {
    let status = git_command(dir, home).args(args).status();
    if !matches!(status, Ok(status) if status.success()) {
        eprintln!(
            "ferrite: fixture world: git {args:?} failed in {}",
            dir.display()
        );
    }
}

/// `git` in `dir` as the fixture's own user, under the disposable HOME, its
/// arguments still to add.
pub(crate) fn git_command(dir: &Path, home: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .args([
            "-c",
            "user.name=Ferrite",
            "-c",
            "user.email=fixture@ferrite.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .current_dir(dir)
        .env("HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_DATE", "2026-10-04T09:00:00")
        .env("GIT_COMMITTER_DATE", "2026-10-04T09:00:00");
    command
}

pub(crate) fn write_lines(path: &Path, lines: &[String]) {
    std::fs::create_dir_all(path.parent().expect("a file has a directory"))
        .expect("create a fixture directory");
    let mut text = lines.join("\n");
    text.push('\n');
    std::fs::write(path, text).expect("write a fixture file");
}

/// A Project with nothing but a first commit on `main`.
pub(crate) fn lay_out_plain(dir: &Path, home: &Path, name: &str) {
    std::fs::create_dir_all(dir).expect("create a fixture Project");
    std::fs::write(dir.join("README.md"), format!("# {name}\n")).expect("write README");
    git(dir, home, &["init", "-q", "-b", "main"]);
    git(dir, home, &["add", "-A"]);
    git(dir, home, &["commit", "-q", "-m", "first"]);
}

// ---------------------------------------------------------------- events

pub(crate) fn started(id: &str, name: &str, input: serde_json::Value) -> SessionEvent {
    SessionEvent::ToolStarted {
        id: id.into(),
        name: name.into(),
        input,
    }
}

pub(crate) fn completed(
    id: &str,
    output: &str,
    is_error: bool,
    result: ToolResult,
) -> SessionEvent {
    SessionEvent::ToolCompleted {
        id: id.into(),
        output: output.into(),
        is_error,
        result,
    }
}

pub(crate) fn text(text: &str) -> SessionEvent {
    SessionEvent::TextDelta { text: text.into() }
}

pub(crate) fn init(session: &str, model: &str) -> SessionEvent {
    SessionEvent::Init {
        session_id: session.into(),
        model: model.into(),
    }
}

pub(crate) fn mode(mode: &str) -> SessionEvent {
    SessionEvent::PermissionMode { mode: mode.into() }
}

/// Occupancy alone: the context meter, no turn accounting.
pub(crate) fn context(total: u64, window: u64) -> SessionEvent {
    SessionEvent::ContextUsage {
        total_tokens: total,
        context_window: Some(window),
    }
}

/// A turn's accounting as Claude reports it: what it read and wrote, and
/// the context it now occupies.
pub(crate) fn tokens(total: u64, input: u64, output: u64, window: u64) -> [SessionEvent; 2] {
    [
        SessionEvent::TokenUsage {
            total_tokens: total,
            input_tokens: input,
            cached_input_tokens: total.saturating_sub(input + output),
            output_tokens: output,
            reasoning_output_tokens: 0,
            context_window: Some(window),
        },
        SessionEvent::UsageDetails {
            details: UsageDetails {
                scope: UsageScope::Turn,
                input_tokens: input,
                cached_input_tokens: total.saturating_sub(input + output),
                output_tokens: output,
                reasoning_output_tokens: 0,
            },
        },
    ]
}

pub(crate) fn limits(five_hour: f32, weekly: f32) -> SessionEvent {
    let window = |used_fraction| {
        Some(RateLimitWindow {
            used_fraction,
            resets_at: None,
        })
    };
    SessionEvent::RateLimits {
        five_hour: window(five_hour),
        weekly: window(weekly),
    }
}

pub(crate) fn phase(detail: &str) -> SessionEvent {
    SessionEvent::Progress {
        event: ProgressEvent::Phase {
            phase: Phase::Working,
            detail: detail.into(),
        },
    }
}

/// A Claude `Read`'s structured result (`toolUseResult.file`).
pub(crate) fn read_result(path: &str, lines: &[String], total: usize) -> Outcome {
    let content = lines.join("\n");
    (
        numbered(lines),
        false,
        ToolResult::Structured {
            value: serde_json::json!({
                "type": "text",
                "file": {
                    "filePath": path,
                    "content": content,
                    "numLines": total,
                    "startLine": 1,
                    "totalLines": total,
                }
            }),
            duration_ms: None,
        },
    )
}

/// What `Read` hands the model: the lines, numbered.
pub(crate) fn numbered(lines: &[String]) -> String {
    lines
        .iter()
        .enumerate()
        .map(|(at, line)| format!("{:>6}\u{2192}{line}", at + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Codex asking to run a command, with its own standing answer for the
/// command's family (`acceptWithExecpolicyAmendment`).
pub(crate) fn approval(id: &str, command: &str, family: &[&str]) -> SessionEvent {
    let amendment = serde_json::json!({
        "acceptWithExecpolicyAmendment": { "execpolicy_amendment": family }
    });
    SessionEvent::DecisionRequested {
        decision: Decision {
            delivery: Default::default(),
            kind: DecisionKind::Approval,
            policy: Default::default(),
            id: format!("{id}-approval"),
            tool_use_id: id.into(),
            tool_name: "Bash".into(),
            description: command.into(),
            input: serde_json::json!({
                "itemId": id,
                "command": command,
                "reason": null,
                "proposedExecpolicyAmendment": family,
                "availableDecisions": ["accept", amendment.clone(), "decline", "cancel"],
            }),
            suggestions: vec![DecisionChoice {
                label: format!("Always allow {}", family.join(" ")),
                value: amendment,
                standing: true,
            }],
        },
    }
}

/// Claude asking to run a command: the call, then its permission request.
pub(crate) fn bash_approval(id: &str, command: &str) -> [SessionEvent; 2] {
    [
        started(id, "Bash", serde_json::json!({ "command": command })),
        SessionEvent::DecisionRequested {
            decision: Decision {
                delivery: Default::default(),
                kind: DecisionKind::Approval,
                policy: Default::default(),
                id: format!("{id}-approval"),
                tool_use_id: id.into(),
                tool_name: "Bash".into(),
                description: command.into(),
                input: serde_json::json!({ "command": command }),
                suggestions: vec![],
            },
        },
    ]
}

/// A Claude `AskUserQuestion` with one single-select question.
pub(crate) fn question(id: &str, question: &str, options: &[(&str, &str)]) -> SessionEvent {
    let input = serde_json::json!({
        "questions": [{
            "question": question,
            "header": "Fix",
            "multiSelect": false,
            "options": options
                .iter()
                .map(|(label, description)| serde_json::json!({
                    "label": label,
                    "description": description,
                }))
                .collect::<Vec<_>>(),
        }]
    });
    SessionEvent::DecisionRequested {
        decision: Decision {
            delivery: Default::default(),
            kind: DecisionKind::Questions(
                ferrite_core::questions::parse(&input).expect("a parity question"),
            ),
            policy: Default::default(),
            id: format!("{id}-question"),
            tool_use_id: id.into(),
            tool_name: "AskUserQuestion".into(),
            description: String::new(),
            input,
            suggestions: vec![],
        },
    }
}

/// A passing `cargo test` run of `tests` under `module`.
pub(crate) fn passing(module: &str, tests: &[&str], count: usize, took: &str) -> String {
    let mut lines = vec![format!("running {count} tests")];
    lines.extend(
        tests
            .iter()
            .take(count)
            .map(|test| format!("test {module}::{test} ... ok")),
    );
    lines.push(format!(
        "test result: ok. {count} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; \
         finished in {took}"
    ));
    lines.join("\n")
}

/// Small methods to stand between the lines a scene shows, up to line
/// `upto` (1-based, exclusive of what follows): whole methods while they
/// fit, then section rules.
pub(crate) fn filler(lines: &mut Vec<String>, upto: usize, names: &[&str], returns: &str) {
    let mut at = 0;
    while lines.len() + 6 <= upto {
        let name = names[at % names.len()];
        let suffix = if at < names.len() {
            String::new()
        } else {
            format!("_{}", at / names.len())
        };
        lines.extend([
            format!("fn {name}{suffix}(&self, cx: &App) -> {returns} {{"),
            "    let theme = cx.theme();".to_string(),
            format!("    let facts = self.facts.{name}();"),
            format!("    self.paint_{name}(facts, theme)"),
            "}".to_string(),
            String::new(),
        ]);
        at += 1;
    }
    while lines.len() < upto {
        lines.push("// ----".into());
    }
}

/// `before` with `hunks` applied, in order. Each hunk's old range must
/// match `before` (checked in the tests).
pub(crate) fn apply(before: &[String], hunks: &[Hunk]) -> Vec<String> {
    let mut after = Vec::new();
    let mut next = 0;
    for hunk in hunks {
        let start = hunk.old_start as usize - 1;
        after.extend(before[next..start].iter().cloned());
        let mut old = start;
        for line in &hunk.lines {
            let (marker, body) = line.split_at(1);
            match marker {
                " " => {
                    after.push(body.to_string());
                    old += 1;
                }
                "-" => old += 1,
                _ => after.push(body.to_string()),
            }
        }
        next = old;
    }
    after.extend(before[next..].iter().cloned());
    after
}

/// A Claude Session's model menu: the catalog's rows, on the four-step
/// effort ladder the picker draws.
pub(crate) fn claude_menu() -> Vec<ModelInfo> {
    ferrite_core::providers::models::fallback(Provider::Claude)
        .into_iter()
        .map(|mut row| {
            if !row.efforts.is_empty() {
                row.efforts = ["low", "medium", "high", "max"]
                    .into_iter()
                    .map(String::from)
                    .collect();
            }
            row
        })
        .collect()
}

/// Play `script` into `core` in time order: the clock first, then the act,
/// then a pump, so each act lands on its own second. `feed` is the provider
/// side of a Thread's Session. Answers when each Thread last acted.
pub(crate) fn play(
    core: &mut Cockpit,
    mut script: Script,
    clock: Clock,
    feed: impl Fn(ThreadId) -> mpsc::Sender<SessionEvent>,
) -> HashMap<ThreadId, Secs> {
    script.beats.sort_by_key(|beat| beat.at);
    let mut last = HashMap::new();
    for beat in script.beats {
        clock.set(beat.at);
        last.insert(beat.who, beat.at);
        match beat.act {
            Act::Prompt(text) => core.send(beat.who, text),
            Act::Event(event) => {
                if feed(beat.who).send(event).is_err() {
                    panic!(
                        "thread {}'s scripted Session stopped listening by {}s",
                        beat.who, beat.at
                    );
                }
            }
            Act::Park => core.park(beat.who).expect("park a scripted Thread"),
        }
        core.pump();
        core.pump();
    }
    last
}

/// A Group of `first`, `second` and then `rest`, titled `title`.
pub(crate) fn group(
    core: &mut Cockpit,
    first: ThreadId,
    second: ThreadId,
    rest: &[ThreadId],
    title: &str,
) -> GroupId {
    let group = core
        .apply_group(GroupChange::Create { first, second })
        .expect("create a fixture Group")
        .group
        .expect("a new Group");
    for thread in rest {
        core.apply_group(GroupChange::Join {
            thread: *thread,
            group,
            index: None,
        })
        .expect("join a fixture Group");
    }
    core.apply_group(GroupChange::Rename {
        group,
        title: title.into(),
    })
    .expect("name a fixture Group");
    group
}

/// Stamp `thread`'s log as last written at `at`: recency is the log's
/// mtime (`Store::last_used`).
pub(crate) fn last_used(store: &Path, thread: ThreadId, at: SystemTime) {
    let log = store.join(thread.to_string()).join("log.jsonl");
    if let Ok(file) = std::fs::File::options().append(true).open(&log) {
        let _ = file.set_modified(at);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Every hunk's old side is what the file said there.
    pub(crate) fn assert_applies(before: &[String], hunks: &[Hunk]) {
        for hunk in hunks {
            let mut at = hunk.old_start as usize - 1;
            for line in &hunk.lines {
                let (marker, body) = line.split_at(1);
                if marker != "+" {
                    assert_eq!(
                        before[at],
                        body,
                        "hunk at {} line {}",
                        hunk.old_start,
                        at + 1
                    );
                    at += 1;
                }
            }
        }
    }

    #[test]
    fn a_clock_label_reads_back_as_its_minute() {
        assert_eq!(label_seconds("7:41 pm"), Some(hms(19, 41, 0)));
        assert_eq!(label_seconds("12:05 am"), Some(hms(0, 5, 0)));
        assert_eq!(label_seconds("12:05 pm"), Some(hms(12, 5, 0)));
        assert_eq!(label_seconds("noon"), None);
    }
}
