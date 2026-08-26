//! What the Pane holds: either a live Claude Session or the scripted demo
//! stream. Both hand the pump the same `Receiver<SessionEvent>`, so `--demo`
//! exercises the real render path without spawning a CLI.

use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ferrite_core::cockpit::{RssSampler, Spawner};
use ferrite_core::providers::{ClaudeConfig, ClaudeSession, CodexConfig, CodexSession, Session};
use ferrite_core::store::Provider;
use ferrite_core::{Decision, DecisionAnswer, SessionEvent, ThreadId, TurnOutcome};

/// A scripted event stream: no process, same channel, same pump.
pub struct DemoSession {
    rx: Receiver<SessionEvent>,
    tx: Sender<SessionEvent>,
    cancel: Arc<AtomicBool>,
}

impl Session for DemoSession {
    fn events(&self) -> &Receiver<SessionEvent> {
        &self.rx
    }

    fn send(&mut self, _text: &str) -> io::Result<()> {
        self.play_reply();
        Ok(())
    }

    fn interrupt(&mut self) -> io::Result<()> {
        self.cancel.store(true, Ordering::Relaxed);
        Ok(())
    }

    fn respond_to_decision(&mut self, _id: &str, answer: DecisionAnswer) -> io::Result<()> {
        self.respond(answer);
        Ok(())
    }
}

/// The load generator behind the 24-Pane perf run: one Session streaming
/// words forever, at the tick rate the panes24 baseline was measured at.
pub fn streaming() -> DemoSession {
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let sender = tx.clone();
    thread::spawn(move || {
        let words = [
            "wiring", "the", "joiner", "into", "canvas", "path", "atlas", "stays", "per-cell",
            "checks", "green", "vitest", "run", "passed", "resume", "session", "delta", "coalesce",
            "channel", "spawn", "parse", "commit", "ferrite", "pane", "stream", "tokens", "metal",
            "frame", "budget",
        ];
        let mut at = 0usize;
        loop {
            // Real prose ends paragraphs, which is what lets a transcript
            // evict: an agent that streamed one endless line would grow one
            // Block forever.
            let word = words[at % words.len()];
            let text = if at % 40 == 39 {
                format!("{word}.\n\n")
            } else {
                format!("{word} ")
            };
            if sender.send(SessionEvent::TextDelta { text }).is_err() {
                return;
            }
            at += 1;
            thread::sleep(Duration::from_millis(8));
        }
    });
    DemoSession { rx, tx, cancel }
}

/// How the app starts Sessions. `--demo` swaps every provider for the
/// scripted one, which is also what drives the 24-Pane load test.
pub struct Spawn {
    pub demo: bool,
    /// Every Session streams forever — the perf load, not a demo to read.
    pub load: bool,
    /// How many demo Sessions this spawner has dealt — each one takes the
    /// next seed of the wall mix, so a grid shows every state at once.
    seeds: usize,
}

impl Spawn {
    pub fn new(demo: bool, load: bool) -> Self {
        Self {
            demo,
            load,
            seeds: 0,
        }
    }
}

impl Spawner for Spawn {
    fn spawn(
        &mut self,
        provider: Provider,
        resume: Option<&str>,
        cwd: Option<&Path>,
    ) -> io::Result<Box<dyn Session>> {
        if self.load {
            return Ok(Box::new(streaming()));
        }
        if self.demo {
            // A revived Thread already replayed its history from the log; a
            // seed that played again would draw the same turn twice.
            if resume.is_some() {
                return Ok(Box::new(DemoSession::quiet()));
            }
            let variant = self.seeds;
            self.seeds += 1;
            return Ok(Box::new(DemoSession::seeded(variant)));
        }
        // The Thread's workspace binding decides where the Session works; a
        // Thread from before bindings falls back to where Ferrite started.
        let cwd = cwd
            .map(|dir| dir.to_path_buf())
            .or_else(|| std::env::current_dir().ok());
        match provider {
            Provider::Claude => ClaudeSession::spawn(ClaudeConfig {
                cwd,
                resume: resume.map(|target| target.to_string()),
                ..Default::default()
            })
            .map(|session| Box::new(session) as Box<dyn Session>)
            // The typed spawn error keeps its words, which is what the Pane
            // shows; only its type is lost crossing this seam.
            .map_err(|e| io::Error::other(e.to_string())),
            Provider::Codex => CodexSession::spawn(CodexConfig {
                cwd,
                approval_policy: Some("on-request".into()),
                resume: resume.map(|target| target.to_string()),
                ..Default::default()
            })
            .map(|session| Box::new(session) as Box<dyn Session>)
            .map_err(|e| io::Error::other(e.to_string())),
        }
    }
}

/// Resident memory per Session, read the way the panes24 spike reads it.
#[derive(Default)]
pub struct ProcessRss;

impl RssSampler for ProcessRss {
    fn sample(&mut self, _thread: ThreadId, pid: Option<u32>) -> Option<u64> {
        rss_bytes(pid?)
    }
}

/// One process's resident bytes, the way the panes24 spike reads it.
#[cfg(not(windows))]
pub(crate) fn rss_bytes(pid: u32) -> Option<u64> {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let kilobytes: u64 = String::from_utf8(out.stdout).ok()?.trim().parse().ok()?;
    Some(kilobytes * 1024)
}

/// One process's resident bytes. Windows has no `ps`; `tasklist` is the
/// stock instrument, and its working-set column is also kilobytes.
#[cfg(windows)]
pub(crate) fn rss_bytes(pid: u32) -> Option<u64> {
    let out = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .ok()?;
    tasklist_rss(&String::from_utf8_lossy(&out.stdout))
}

/// The memory column of one `tasklist` CSV row — the last quoted field,
/// `"12,345 K"`. Digit grouping is locale-typed (`.` in German), so only the
/// digits are read.
#[cfg(any(windows, test))]
fn tasklist_rss(row: &str) -> Option<u64> {
    let field = row.trim().rsplit('"').nth(1)?;
    let digits: String = field.chars().filter(char::is_ascii_digit).collect();
    let kilobytes: u64 = digits.parse().ok()?;
    Some(kilobytes * 1024)
}

impl DemoSession {
    /// A Session playing one seed of the wall mix — seed 0 is the
    /// interactive script, the rest each land in one of the states the
    /// Wall board draws and stay there.
    pub fn seeded(variant: usize) -> Self {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        play(tx.clone(), cancel.clone(), seed(variant));
        Self { rx, tx, cancel }
    }

    /// A Session that says nothing until spoken to — what a revived demo
    /// Thread gets, because its history already replayed from the log.
    pub fn quiet() -> Self {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        Self { rx, tx, cancel }
    }

    fn play_reply(&mut self) {
        self.cancel.store(false, Ordering::Relaxed);
        play(self.tx.clone(), self.cancel.clone(), reply());
    }

    /// The demo's agent does what it was told: allowed, it finishes the write
    /// and carries on; denied, it says so and ends the turn.
    fn respond(&mut self, answer: DecisionAnswer) {
        self.cancel.store(false, Ordering::Relaxed);
        let mut steps = match answer {
            // The demo's agent cannot tell "allow once" from "allow always":
            // the standing answer changes what the provider asks next time,
            // which a script has no next time to show.
            DecisionAnswer::Allow { .. } | DecisionAnswer::AllowAlways { .. } => {
                let mut steps = vec![Step::new(
                    60,
                    SessionEvent::ToolCompleted {
                        id: "toolu_demo".into(),
                        output: "File created".into(),
                        is_error: false,
                        result: ferrite_core::ToolResult::FileEdit {
                            path: "ferrite-perm.txt".into(),
                            hunks: vec![ferrite_core::Hunk {
                                old_start: 1,
                                old_lines: 0,
                                new_start: 1,
                                new_lines: 2,
                                lines: vec![
                                    "+permission granted by the operator".into(),
                                    "+the demo writes one honest line".into(),
                                ],
                            }],
                        },
                    },
                )];
                steps.extend(turn(&[], ALLOWED, 0.0124));
                steps
            }
            DecisionAnswer::Deny { .. } => turn(&[], DENIED, 0.0018),
        };
        if let Some(first) = steps.first_mut() {
            first.after = Duration::from_millis(120);
        }
        play(self.tx.clone(), self.cancel.clone(), steps);
    }
}

/// One scripted event and how long to wait before sending it.
pub struct Step {
    pub after: Duration,
    pub event: SessionEvent,
}

impl Step {
    fn new(ms: u64, event: SessionEvent) -> Self {
        Self {
            after: Duration::from_millis(ms),
            event,
        }
    }
}

/// Feed a script down the channel at its own pace, stopping short — and
/// ending the turn Interrupted — as soon as `cancel` is raised.
pub fn play(
    tx: Sender<SessionEvent>,
    cancel: Arc<AtomicBool>,
    steps: Vec<Step>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for step in steps {
            thread::sleep(step.after);
            if cancel.load(Ordering::Relaxed) {
                let _ = tx.send(SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Interrupted,
                    cost_usd: None,
                });
                return;
            }
            if tx.send(step.event).is_err() {
                return;
            }
        }
    })
}

const TURN_ONE: &str = "Ferrite renders whatever the provider streams: no harness, \
    no model calls of its own. This paragraph is long on purpose so the transcript \
    has to wrap inside the Pane and the tail keeps following the newest line as \
    deltas land. Each word arrives as its own TextDelta at roughly thirty \
    milliseconds, which is close enough to a real turn to see whether the layout \
    holds still while text grows underneath it.\n\n\
    ## What the fold produces\n\
    - headings and bullets, each its own Block\n\
    - inline `code` kept in the run of the sentence\n\
    - **bold runs** and [inert links](https://example.com) in their own styles\n\
    - fenced blocks handed to the injected highlighter\n\n\
    ```rust\n\
    fn apply(&mut self, input: Input) -> Update {\n\
        // events in, Blocks out\n\
    }\n\
    ```\n\n";

const ALLOWED: &str = "Written. The Decision came back allowed, so the tool ran \
    and the turn carried on from where it stopped.";

const DENIED: &str = "Understood — I will leave that file alone and stop here.";

const REPLY: &str = "Reading the composer path now. The event pump drains the same \
    bounded channel a live Session would write to, so what you are looking at is \
    the shipping render path with a scripted producer behind it.";

const THINKING: &[&str] = &[
    "checking the cockpit binding",
    "the Pane owns view state only",
    "everything durable belongs to core",
];

// ------------------------------------------------------------------ seeds

/// How many distinct seeds `--demo` deals before cycling — the Wall board's
/// census: working with and without plans, failing tests, done with a cost,
/// blocked, idle, and Decisions with real subjects.
const SEEDS: usize = 12;

/// One spawned demo Session's script, by deal order. Seed 0 is the
/// interactive script a single Pane opens on; the rest each land in one of
/// the states the Wall board draws and stay there.
fn seed(variant: usize) -> Vec<Step> {
    match variant % SEEDS {
        0 => script(),
        1 => seed_working_planned(),
        2 => seed_failing(),
        3 => seed_done(0.22),
        4 => seed_reading(),
        5 => seed_blocked(),
        // Two idle seeds on purpose: the Wall census keeps a pair of
        // quiet cells.
        6 | 11 => seed_idle(),
        7 => seed_editing(),
        8 => seed_done(0.31),
        9 => seed_checking(),
        10 => seed_decision(),
        // `variant % SEEDS` is 0..SEEDS; the compiler just cannot see it.
        _ => unreachable!("seed variants cycle modulo SEEDS"),
    }
}

fn boot(session_id: &str) -> Vec<Step> {
    vec![Step::new(
        120,
        SessionEvent::Init {
            session_id: session_id.into(),
            model: "claude-sonnet-4-5".into(),
        },
    )]
}

fn usage(steps: &mut Vec<Step>, total_tokens: u64) {
    steps.push(Step::new(
        20,
        SessionEvent::TokenUsage {
            total_tokens,
            input_tokens: total_tokens,
            cached_input_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            context_window: Some(200_000),
        },
    ));
}

/// Thinking lines, word-paced like a real turn's.
fn think(steps: &mut Vec<Step>, lines: &[&str]) {
    for line in lines {
        for word in line.split_whitespace() {
            steps.push(Step::new(
                18,
                SessionEvent::ThinkingDelta {
                    text: format!("{word} "),
                },
            ));
        }
        steps.push(Step::new(
            18,
            SessionEvent::ThinkingDelta { text: "\n".into() },
        ));
    }
}

/// Word-paced prose. split_inclusive, not split_whitespace: the newlines
/// are what the markdown fold reads, so a pacer that ate them would test
/// nothing.
fn prose(steps: &mut Vec<Step>, text: &str) {
    for chunk in text.split_inclusive(char::is_whitespace) {
        steps.push(Step::new(
            14,
            SessionEvent::TextDelta {
                text: chunk.to_string(),
            },
        ));
    }
}

/// One planned step, made with the provider's own tool so `todos()` and
/// the tasks strip count it. Every call settles — a planning call left
/// running would wear the ◐ activity line forever.
fn task(steps: &mut Vec<Step>, at: usize, subject: &str) {
    steps.push(Step::new(
        20,
        SessionEvent::ToolStarted {
            id: format!("task_{at}"),
            name: "TaskCreate".into(),
            input: serde_json::json!({ "subject": subject }),
        },
    ));
    steps.push(Step::new(
        10,
        SessionEvent::ToolCompleted {
            id: format!("task_{at}"),
            output: String::new(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        },
    ));
}

/// One planned step marked finished — the update names its step's subject
/// so the row reads like every other tool call.
fn tick(steps: &mut Vec<Step>, at: usize, subject: &str) {
    steps.push(Step::new(
        20,
        SessionEvent::ToolStarted {
            id: format!("tick_{at}"),
            name: "TaskUpdate".into(),
            input: serde_json::json!({
                "taskId": format!("{at}"),
                "status": "completed",
                "subject": subject,
            }),
        },
    ));
    steps.push(Step::new(
        10,
        SessionEvent::ToolCompleted {
            id: format!("tick_{at}"),
            output: String::new(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        },
    ));
}

/// A whole plan at once — the far seeds' shorthand; the interactive script
/// spreads its steps through the turn instead, the way a real agent plans.
fn plan(steps: &mut Vec<Step>, subjects: &[&str], done: usize) {
    for (at, subject) in subjects.iter().enumerate() {
        task(steps, at, subject);
    }
    for (at, subject) in subjects.iter().enumerate().take(done) {
        tick(steps, at, subject);
    }
}

fn tool(steps: &mut Vec<Step>, id: &str, name: &str, input: serde_json::Value) {
    steps.push(Step::new(
        60,
        SessionEvent::ToolStarted {
            id: id.into(),
            name: name.into(),
            input,
        },
    ));
}

fn settled(steps: &mut Vec<Step>, id: &str, output: &str) {
    steps.push(Step::new(
        140,
        SessionEvent::ToolCompleted {
            id: id.into(),
            output: output.into(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        },
    ));
}

/// A real edit with real hunks — what diff cards, `+N −N` stats and the
/// CHANGED strip render from.
fn edit(steps: &mut Vec<Step>, id: &str, path: &str) {
    tool(steps, id, "Edit", serde_json::json!({ "file_path": path }));
    steps.push(Step::new(
        140,
        SessionEvent::ToolCompleted {
            id: id.into(),
            output: "applied".into(),
            is_error: false,
            result: ferrite_core::ToolResult::FileEdit {
                path: path.into(),
                hunks: vec![ferrite_core::Hunk {
                    old_start: 341,
                    old_lines: 3,
                    new_start: 341,
                    new_lines: 4,
                    lines: vec![
                        "     let fraction = plan_fraction(todos);".into(),
                        "-    cell.child(bar(fraction));".into(),
                        "+    cell.child(meter_run(todos.done, todos.total));".into(),
                        "+    cell.child(fraction_label(todos));".into(),
                        "     cell".into(),
                    ],
                }],
            },
        },
    ));
}

fn seed_working_planned() -> Vec<Step> {
    let mut steps = boot("demo-seed-work");
    usage(&mut steps, 118_000);
    plan(
        &mut steps,
        &[
            "wire the joiner",
            "fold the counts",
            "retune the meter",
            "rerun the suite",
        ],
        3,
    );
    prose(
        &mut steps,
        "Wiring the joiner into the canvas path so the atlas stays per-cell; \
         the fold keeps the tail following the newest line while the suite \
         stays green.\n\n",
    );
    tool(
        &mut steps,
        "t_pass",
        "Bash",
        serde_json::json!({ "command": "vitest run tests/unit" }),
    );
    settled(&mut steps, "t_pass", "41 passed (41)");
    tool(
        &mut steps,
        "t_watch",
        "Bash",
        serde_json::json!({ "command": "vitest run tests/unit --watch" }),
    );
    prose(
        &mut steps,
        "Holding the watcher open for the next hunk.\n\n",
    );
    steps
}

fn seed_failing() -> Vec<Step> {
    let mut steps = boot("demo-seed-fail");
    usage(&mut steps, 74_000);
    plan(
        &mut steps,
        &[
            "reproduce the flake",
            "pin the frame",
            "fix the fold",
            "rerun",
            "land",
        ],
        2,
    );
    prose(
        &mut steps,
        "Two cases regressed after the retune; pinning the exact frame the \
         layout goes wrong before touching the fold.\n\n",
    );
    tool(
        &mut steps,
        "t_fail",
        "Bash",
        serde_json::json!({ "command": "cargo test --workspace" }),
    );
    steps.push(Step::new(
        140,
        SessionEvent::ToolCompleted {
            id: "t_fail".into(),
            output: "test result: FAILED. 357 passed; 2 failed".into(),
            is_error: true,
            result: ferrite_core::ToolResult::Opaque,
        },
    ));
    prose(
        &mut steps,
        "Rerunning the pair with the fold instrumented.\n\n",
    );
    steps
}

fn seed_done(cost: f64) -> Vec<Step> {
    let mut steps = boot("demo-seed-done");
    usage(&mut steps, 96_000);
    prose(
        &mut steps,
        "Landed the retune behind the theme tokens; the suite is green and \
         the diff is small enough to review at a glance.\n\n",
    );
    edit(&mut steps, "t_edit", "crates/ferrite/src/pane.rs");
    tool(
        &mut steps,
        "t_pass",
        "Bash",
        serde_json::json!({ "command": "cargo test --workspace" }),
    );
    settled(&mut steps, "t_pass", "359 passed");
    steps.push(Step::new(
        60,
        SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: Some(cost),
        },
    ));
    steps
}

fn seed_reading() -> Vec<Step> {
    let mut steps = boot("demo-seed-read");
    plan(
        &mut steps,
        &[
            "map the recipes",
            "compare the boards",
            "note the gaps",
            "draft the fix",
            "land it",
        ],
        1,
    );
    prose(
        &mut steps,
        "Reading the board recipes side by side before touching anything; \
         the gaps are instruments, not palette.\n\n",
    );
    tool(
        &mut steps,
        "t_read",
        "Read",
        serde_json::json!({ "file_path": "crates/ferrite/src/pane.rs" }),
    );
    steps
}

fn seed_blocked() -> Vec<Step> {
    let mut steps = boot("demo-seed-block");
    prose(&mut steps, "Pushing the preview build to the edge.\n\n");
    steps.push(Step::new(
        400,
        SessionEvent::Closed {
            reason: "wrangler 403 — deploy refused".into(),
        },
    ));
    steps
}

fn seed_idle() -> Vec<Step> {
    boot("demo-seed-idle")
}

fn seed_editing() -> Vec<Step> {
    let mut steps = boot("demo-seed-edit");
    usage(&mut steps, 142_000);
    prose(
        &mut steps,
        "Folding the diff stats into the badge row; the hunks carry their \
         own counts so the render never re-walks the patch.\n\n",
    );
    edit(&mut steps, "t_edit", "crates/ferrite/src/pane.rs");
    tool(
        &mut steps,
        "t_check",
        "Bash",
        serde_json::json!({ "command": "cargo check --workspace" }),
    );
    steps
}

fn seed_checking() -> Vec<Step> {
    let mut steps = boot("demo-seed-check");
    usage(&mut steps, 88_000);
    plan(
        &mut steps,
        &["retune", "fold", "render", "verify", "land"],
        4,
    );
    prose(
        &mut steps,
        "Verification pass: the wall census, the badges, and the meters \
         against the boards, one cell at a time.\n\n",
    );
    tool(
        &mut steps,
        "t_check",
        "Bash",
        serde_json::json!({ "command": "cargo check --workspace" }),
    );
    steps
}

fn seed_decision() -> Vec<Step> {
    let mut steps = boot("demo-seed-close");
    prose(
        &mut steps,
        "The stale issue is superseded by the tracking one; closing it \
         needs a ruling only the operator can give.\n\n",
    );
    tool(
        &mut steps,
        "t_close",
        "Bash",
        serde_json::json!({ "command": "gh issue close 212" }),
    );
    steps.push(Step::new(
        200,
        SessionEvent::DecisionRequested {
            decision: Decision {
                id: "perm_close".into(),
                tool_use_id: "t_close".into(),
                tool_name: "Bash".into(),
                description: "gh issue close 212".into(),
                input: serde_json::json!({
                    "command": "gh issue close 212",
                    "cwd": "/work/ferrite",
                }),
                suggestions: vec![],
            },
        },
    ));
    steps
}

/// Startup: init, thinking, a long streamed turn with real tools, a paid
/// stop, then a permission wait — the interactive seed a single Pane opens
/// on.
pub fn script() -> Vec<Step> {
    let mut steps = boot("4f2a1c9e-7b30-4d18-9c62-1ea55d0b7742");
    steps.push(Step::new(
        40,
        SessionEvent::PermissionMode {
            mode: "acceptEdits".into(),
        },
    ));
    usage(&mut steps, 124_000);
    think(&mut steps, THINKING);
    prose(&mut steps, TURN_ONE);
    // The plan grows as the work does — one step made, worked, ticked —
    // never a front-loaded block of planning rows no comp draws.
    task(&mut steps, 0, "read the pane recipe");
    tool(
        &mut steps,
        "toolu_read",
        "Read",
        serde_json::json!({ "file_path": "crates/ferrite/src/pane.rs" }),
    );
    settled(
        &mut steps,
        "toolu_read",
        "2,320 lines — the Pane recipes, all three levels",
    );
    tick(&mut steps, 0, "read the pane recipe");
    task(&mut steps, 1, "run the suite");
    tool(
        &mut steps,
        "toolu_test",
        "Bash",
        serde_json::json!({ "command": "vitest run tests/unit" }),
    );
    // A real run takes real seconds — what the row's duration reads.
    steps.push(Step::new(
        2400,
        SessionEvent::ToolCompleted {
            id: "toolu_test".into(),
            output: "41 passed (41)".into(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        },
    ));
    tick(&mut steps, 1, "run the suite");
    task(&mut steps, 2, "retune the meter");
    edit(&mut steps, "toolu_edit", "crates/ferrite/src/pane.rs");
    tick(&mut steps, 2, "retune the meter");
    // The step still being worked — what the tasks strip names.
    task(&mut steps, 3, "land the diff");
    steps.push(Step::new(
        30,
        SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: Some(0.0380),
        },
    ));

    // The second turn opens by asking permission, and stops there: nothing
    // else plays until the operator answers the card.
    steps.push(Step::new(
        400,
        SessionEvent::ToolStarted {
            id: "toolu_demo".into(),
            name: "Write".into(),
            input: serde_json::json!({ "file_path": "ferrite-perm.txt" }),
        },
    ));
    steps.push(Step::new(
        120,
        SessionEvent::DecisionRequested {
            decision: Decision {
                id: "perm_demo".into(),
                tool_use_id: "toolu_demo".into(),
                tool_name: "Write".into(),
                description: "ferrite-perm.txt".into(),
                input: serde_json::json!({ "file_path": "ferrite-perm.txt", "content": "ok" }),
                suggestions: vec![serde_json::json!({
                    "type": "setMode",
                    "mode": "acceptEdits",
                    "destination": "session",
                })],
            },
        },
    ));
    steps
}

/// The canned answer to a prompt sent from the Composer.
pub fn reply() -> Vec<Step> {
    turn(&[], REPLY, 0.0091)
}

/// One turn: thinking lines, then word-by-word text, then TurnEnded.
fn turn(thinking: &[&str], text: &str, cost: f64) -> Vec<Step> {
    let mut steps = Vec::new();
    think(&mut steps, thinking);
    prose(&mut steps, text);
    steps.push(Step::new(
        30,
        SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: Some(cost),
        },
    ));
    steps
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::transcript::{Body, Input, Status, Transcript};

    /// Runs against whichever process table this platform has — `ps` here,
    /// `tasklist` when the suite runs on Windows.
    #[test]
    fn the_rss_sampler_reads_this_very_process() {
        let rss = rss_bytes(std::process::id()).expect("own pid must sample");
        assert!(rss > 1024 * 1024, "resident bytes were {rss}");
    }

    #[test]
    fn a_tasklist_row_yields_bytes_whatever_the_locale_groups_with() {
        for row in [
            "\"node.exe\",\"1234\",\"Console\",\"1\",\"54,321 K\"\r\n",
            "\"node.exe\",\"1234\",\"Console\",\"1\",\"54.321 K\"",
        ] {
            assert_eq!(tasklist_rss(row), Some(54_321 * 1024), "{row:?}");
        }
        assert_eq!(tasklist_rss("INFO: No tasks are running.\r\n"), None);
        assert_eq!(tasklist_rss(""), None);
    }

    #[test]
    fn replaying_the_demo_script_leaves_a_paid_turn_and_a_thread_on_the_operator() {
        let mut transcript = Transcript::default();
        for step in script() {
            transcript.apply(Input::Event(step.event));
        }

        assert_eq!(transcript.model(), Some("claude-sonnet-4-5"));
        // The demo ends where a real Thread ends: waiting on a person.
        assert_eq!(transcript.status(), Status::Blocked);

        // No dollar value reaches the transcript (#22 operator amendment)
        // — the cost is recorded, never rendered.
        assert_eq!(transcript.last_cost(), Some(0.0380));
        assert!(transcript.blocks().iter().all(|block| match &block.body {
            Body::Meta(text) => !text.contains('$'),
            _ => true,
        }));

        let longest = transcript
            .blocks()
            .iter()
            .filter_map(|block| match &block.body {
                Body::Paragraph { spans } => {
                    Some(spans.iter().map(|s| s.text.chars().count()).sum::<usize>())
                }
                _ => None,
            })
            .max()
            .unwrap();
        assert!(longest > 200, "demo text must wrap; longest was {longest}");
    }

    #[test]
    fn answering_the_demo_decision_plays_the_rest_of_the_turn() {
        let mut demo = DemoSession::seeded(0);

        demo.respond(DecisionAnswer::Allow {
            input: serde_json::Value::Null,
        });

        // The scripted answer runs the tool it was blocked on, then finishes.
        let mut seen = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            match demo.rx.recv_timeout(Duration::from_millis(200)) {
                Ok(event) => {
                    let done = matches!(event, SessionEvent::TurnEnded { .. });
                    seen.push(event);
                    if done {
                        break;
                    }
                }
                Err(_) => continue,
            }
        }
        assert!(
            seen.iter()
                .any(|event| matches!(event, SessionEvent::ToolCompleted { .. })),
            "the allowed tool must run: {seen:?}"
        );
        assert!(matches!(
            seen.last(),
            Some(SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                ..
            })
        ));
    }

    #[test]
    fn a_cancelled_playback_ends_the_turn_interrupted() {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(true));

        play(
            tx,
            cancel,
            vec![Step::new(
                0,
                SessionEvent::TextDelta {
                    text: "never sent".into(),
                },
            )],
        )
        .join()
        .unwrap();

        let events: Vec<SessionEvent> = rx.try_iter().collect();
        assert_eq!(
            events,
            vec![SessionEvent::TurnEnded {
                outcome: TurnOutcome::Interrupted,
                cost_usd: None,
            }]
        );
    }

    /// #22 B: the seeds deal the Wall board's census — every state the wall
    /// can draw appears somewhere in one cycle, instead of N identical
    /// permission-waits.
    #[test]
    fn the_demo_seeds_deal_the_wall_census() {
        use crate::pane::{wall_card, wall_state, WallState};
        let mut census = Vec::new();
        for variant in 0..SEEDS {
            let mut transcript = Transcript::default();
            let mut pending = false;
            for step in seed(variant) {
                if matches!(step.event, SessionEvent::DecisionRequested { .. }) {
                    pending = true;
                }
                transcript.apply(Input::Event(step.event));
            }
            let card = wall_card(Some(&transcript), None);
            census.push(wall_state(
                Some(transcript.status()),
                pending,
                card.tests_failing,
                transcript.last_cost().is_some(),
            ));
        }
        use WallState::*;
        for wanted in [Working, Failing, Decision, Blocked, Done, Idle] {
            assert!(census.contains(&wanted), "no {wanted:?} in {census:?}");
        }
    }

    /// #22 B: a revived demo Thread already replayed its history from the
    /// log — the fresh Session must not play the same turn over it again.
    #[test]
    fn a_revived_demo_thread_replays_nothing() {
        use ferrite_core::cockpit::Spawner;
        use ferrite_core::store::Provider;
        let mut spawn = Spawn::new(true, false);
        let revived = spawn
            .spawn(Provider::Claude, Some("4f2a"), None)
            .expect("demo spawns never fail");
        assert!(
            revived
                .events()
                .recv_timeout(Duration::from_millis(200))
                .is_err(),
            "a resumed demo Session must stay quiet"
        );
    }

    #[test]
    fn the_demo_script_opens_a_session_and_stops_on_a_decision() {
        let steps = script();

        assert!(matches!(
            steps.first().unwrap().event,
            SessionEvent::Init { .. }
        ));
        assert!(matches!(
            steps.last().unwrap().event,
            SessionEvent::DecisionRequested { .. }
        ));
    }
}
