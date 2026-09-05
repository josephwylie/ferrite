//! Notifications through the Cockpit's public Interface: scripted Sessions
//! for the rules, and the committed Claude and Codex captures — replayed
//! through the real adapters — for the providers' own signals.
#![cfg(unix)]

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use ferrite_core::activity::{
    ActivityEvent, AgentInfo, AgentKey, AgentStatus, ExecutionEvent, Subject, TranscriptCoverage,
};
use ferrite_core::cockpit::{Cockpit, SpawnRequest, Spawner};
use ferrite_core::progress::{Phase, ProgressEvent};
use ferrite_core::providers::{ClaudeConfig, ClaudeSession, CodexConfig, CodexSession, Session};
use ferrite_core::roster::PaneIdentity;
use ferrite_core::store::{Provider, Store};
use ferrite_core::transcript::Body;
use ferrite_core::workspace::WorkspaceChoice;
use ferrite_core::{DecisionAnswer, SessionEvent, ThreadId, TurnOutcome};

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ferrite-notifications-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// ------------------------------------------------------------ scripted

#[derive(Clone, Default)]
struct Control(Arc<Mutex<Vec<mpsc::Sender<SessionEvent>>>>);

impl Control {
    fn emit(&self, session: usize, event: SessionEvent) {
        self.0.lock().unwrap()[session].send(event).unwrap();
    }
    fn activity(&self, session: usize, event: ActivityEvent) {
        self.emit(session, SessionEvent::Activity(event));
    }
}

struct Scripted(mpsc::Receiver<SessionEvent>);

impl Session for Scripted {
    fn events(&self) -> &mpsc::Receiver<SessionEvent> {
        &self.0
    }
    fn send(&mut self, _: &str) -> io::Result<()> {
        Ok(())
    }
    fn interrupt(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn respond_to_decision(&mut self, _: &str, _: DecisionAnswer) -> io::Result<()> {
        Ok(())
    }
}

impl Spawner for Control {
    fn spawn(&mut self, request: SpawnRequest) -> io::Result<Box<dyn Session>> {
        let (sender, events) = mpsc::channel();
        let mut sessions = self.0.lock().unwrap();
        sender
            .send(SessionEvent::Init {
                session_id: format!("main-{}", sessions.len()),
                model: request.model.unwrap_or("model").to_owned(),
            })
            .unwrap();
        sessions.push(sender);
        Ok(Box::new(Scripted(events)))
    }
}

struct Harness {
    cockpit: Cockpit,
    control: Control,
    threads: Vec<ThreadId>,
    _scratch: Scratch,
}

impl Harness {
    fn new(label: &str, threads: usize) -> Self {
        let scratch = Scratch::new(label);
        let checkout = scratch.0.join("checkout");
        fs::create_dir(&checkout).unwrap();
        let control = Control::default();
        let mut cockpit = Cockpit::new(
            Store::open(scratch.0.join("store")).unwrap(),
            Box::new(control.clone()),
        );
        cockpit.set_notification_grace(Duration::ZERO);
        let threads = (0..threads)
            .map(|_| {
                cockpit
                    .open(
                        Provider::Claude,
                        WorkspaceChoice::Main {
                            checkout: checkout.clone(),
                        },
                    )
                    .unwrap()
            })
            .collect();
        cockpit.pump();
        Self {
            cockpit,
            control,
            threads,
            _scratch: scratch,
        }
    }

    fn child(&self, session: usize, name: &str) -> AgentKey {
        let key = AgentKey::new(Provider::Claude, &format!("main-{session}"), name);
        let mut info = AgentInfo::new(key.clone());
        info.parent = Some(Subject::Main);
        info.coverage = TranscriptCoverage::Live;
        self.control
            .activity(session, ActivityEvent::Discovered(info));
        self.control.activity(
            session,
            ActivityEvent::Status {
                key: key.clone(),
                state: AgentStatus::Working,
            },
        );
        key
    }

    fn unread(&self) -> usize {
        self.cockpit.notifications().unread()
    }
    fn count(&self) -> usize {
        self.cockpit.notifications().notices().count()
    }
}

fn ended() -> SessionEvent {
    SessionEvent::TurnEnded {
        outcome: TurnOutcome::Completed,
        cost_usd: None,
    }
}

fn working() -> SessionEvent {
    SessionEvent::Progress {
        event: ProgressEvent::Phase {
            phase: Phase::Working,
            detail: String::new(),
        },
    }
}

#[test]
fn a_finished_thread_the_operator_is_not_on_is_unread_until_they_land_on_it() {
    let mut h = Harness::new("unread", 2);
    let [first, second] = [h.threads[0], h.threads[1]];
    assert!(h.cockpit.focus(PaneIdentity::Thread(first)));
    h.cockpit.send(second, "go".into());
    h.control
        .emit(1, SessionEvent::TextDelta { text: "…".into() });
    h.cockpit.pump();
    assert_eq!(h.count(), 0, "a working Thread is not news");

    h.control.emit(1, ended());
    h.cockpit.pump();
    assert_eq!(h.unread(), 1);
    let notice = h.cockpit.notifications().notices().next().unwrap().clone();
    assert_eq!(notice.thread, second);
    assert_eq!(notice.outcome, TurnOutcome::Completed);
    assert!(h.cockpit.notifications().attention(second));
    assert!(!h.cockpit.notifications().attention(first));

    assert!(h.cockpit.focus_thread(second));
    assert_eq!(h.unread(), 0, "landing on the Pane reads it");
    assert!(!h.cockpit.notifications().attention(second));
}

#[test]
fn a_finish_on_the_focused_thread_is_born_read() {
    let mut h = Harness::new("focused", 1);
    let thread = h.threads[0];
    h.cockpit.send(thread, "go".into());
    h.control.emit(0, ended());
    h.cockpit.pump();
    assert_eq!(h.count(), 1);
    assert_eq!(h.unread(), 0);
    assert!(!h.cockpit.notifications().attention(thread));
}

#[test]
fn notifications_wait_for_a_child_permission_before_finishing() {
    let mut h = Harness::new("waiting-permission", 1);
    let thread = h.threads[0];
    h.cockpit.send(thread, "go".into());
    let child = h.child(0, "blocked");
    h.control.activity(
        0,
        ActivityEvent::Decision {
            subject: Some(Subject::Subagent(child.clone())),
            decision: ferrite_core::Decision {
                delivery: Default::default(),
                id: "approval".into(),
                tool_use_id: "tool".into(),
                tool_name: "Write".into(),
                description: "file.txt".into(),
                input: serde_json::Value::Null,
                suggestions: vec![],
            },
        },
    );
    h.control.emit(0, ended());
    h.cockpit.pump();
    let activity = h.cockpit.thread(thread).unwrap().activity();
    assert_eq!(activity.children()[0].status(), AgentStatus::Waiting);
    assert_eq!(activity.pending_decisions().len(), 1);
    assert_eq!(h.count(), 0, "a child awaiting permission has not finished");
    h.cockpit.pump();
    h.cockpit.pump();
    assert_eq!(h.count(), 0, "permission must also hold off the grace");

    h.control.activity(
        0,
        ActivityEvent::DecisionCancelled {
            id: "approval".into(),
        },
    );
    h.control.activity(
        0,
        ActivityEvent::Status {
            key: child,
            state: AgentStatus::Idle,
        },
    );
    h.cockpit.pump();
    h.cockpit.pump();
    assert_eq!(
        h.count(),
        1,
        "settling the child permits the deferred finish"
    );
}

#[test]
fn notifications_discard_a_parked_deferral_but_keep_existing_notices() {
    let mut h = Harness::new("park-deferred", 1);
    let thread = h.threads[0];
    h.cockpit.send(thread, "first".into());
    h.control.emit(0, ended());
    h.cockpit.pump();
    let notice = h.cockpit.notifications().newest().unwrap();

    h.cockpit.send(thread, "delegate".into());
    h.child(0, "worker");
    h.control.emit(0, ended());
    h.cockpit.pump();
    assert_eq!(h.count(), 1, "the new finish is deferred");
    h.cockpit.park(thread).unwrap();
    assert!(h.cockpit.notifications().get(notice).is_some());
    h.cockpit.reopen(thread).unwrap();
    for _ in 0..3 {
        h.cockpit.pump();
    }
    assert_eq!(h.count(), 1, "reopening history is not a new live finish");

    h.cockpit.send(thread, "new live turn".into());
    h.control.emit(1, ended());
    h.cockpit.pump();
    assert_eq!(h.count(), 2, "a new live finish still notifies");
}

#[test]
fn a_held_prompt_going_out_at_turn_end_is_not_a_finish() {
    let mut h = Harness::new("held", 2);
    let second = h.threads[1];
    h.cockpit.send(second, "first".into());
    h.control
        .emit(1, SessionEvent::TextDelta { text: "…".into() });
    h.cockpit.pump();
    h.cockpit.queue(second, "and then this".into());
    h.control.emit(1, ended());
    h.cockpit.pump();
    assert_eq!(
        h.count(),
        0,
        "the operator queued more work and waits for that"
    );
    assert_eq!(
        h.cockpit.thread(second).unwrap().queued(),
        None,
        "the held prompt went out"
    );

    h.control
        .emit(1, SessionEvent::TextDelta { text: "…".into() });
    h.control.emit(1, ended());
    h.cockpit.pump();
    assert_eq!(h.count(), 1, "the held prompt's own turn end is the finish");
}

#[test]
fn opening_a_notice_lands_on_its_thread_and_revives_a_parked_one() {
    let mut h = Harness::new("open", 2);
    let [first, second] = [h.threads[0], h.threads[1]];
    assert!(h.cockpit.focus(PaneIdentity::Thread(first)));
    h.cockpit.send(second, "go".into());
    h.control.emit(1, ended());
    h.cockpit.pump();
    let id = h.cockpit.notifications().newest().unwrap();

    // Parked after it finished: opening the Notice brings it back.
    h.cockpit.park(second).unwrap();
    assert!(h.cockpit.thread(second).is_none());
    assert_eq!(h.cockpit.open_notice(id), Some(second));
    assert_eq!(h.cockpit.roster().focused_thread(), Some(second));
    assert!(h.cockpit.thread(second).is_some(), "revived");
    assert!(h.cockpit.notifications().get(id).unwrap().read);

    assert!(h.cockpit.dismiss_notice(id));
    assert_eq!(h.count(), 0);
    assert_eq!(h.cockpit.open_notice(id), None);
}

#[test]
fn a_main_that_ends_while_children_work_waits_for_its_resume() {
    let mut h = Harness::new("deferred", 2);
    let second = h.threads[1];
    h.cockpit.send(second, "go".into());
    let alpha = h.child(1, "alpha");
    let beta = h.child(1, "beta");
    // Claude: Main's own result while both background agents run.
    h.control.emit(1, ended());
    h.cockpit.pump();
    assert_eq!(h.count(), 0, "Main is waiting on its agents");

    // Alpha finishes; the CLI's per-turn init resumes Main; that turn
    // ends autonomously with beta still working.
    h.control.activity(
        1,
        ActivityEvent::Status {
            key: alpha,
            state: AgentStatus::Idle,
        },
    );
    h.control.emit(1, working());
    h.control.emit(
        1,
        SessionEvent::TextDelta {
            text: "alpha done".into(),
        },
    );
    h.control.activity(
        1,
        ActivityEvent::BackgroundTurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        },
    );
    h.cockpit.pump();
    assert_eq!(h.count(), 0, "beta is still working");

    // Beta finishes; the resume that follows is the finish.
    h.control.activity(
        1,
        ActivityEvent::Status {
            key: beta,
            state: AgentStatus::Idle,
        },
    );
    h.control.emit(1, working());
    h.cockpit.pump();
    assert_eq!(
        h.count(),
        0,
        "Main was resumed before the grace could run out"
    );
    h.control.emit(
        1,
        SessionEvent::TextDelta {
            text: "beta done".into(),
        },
    );
    h.control.activity(
        1,
        ActivityEvent::BackgroundTurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        },
    );
    h.cockpit.pump();
    assert_eq!(h.count(), 1);
    assert_eq!(h.unread(), 1);
}

#[test]
fn a_subagents_own_end_never_notifies_and_a_deleted_thread_is_forgotten() {
    let mut h = Harness::new("child", 2);
    let second = h.threads[1];
    h.cockpit.send(second, "go".into());
    h.control
        .emit(1, SessionEvent::TextDelta { text: "…".into() });
    let child = h.child(1, "helper");
    h.control.activity(
        1,
        ActivityEvent::Content {
            key: child.clone(),
            id: Some("turn".into()),
            event: ExecutionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: None,
            },
        },
    );
    h.control.activity(
        1,
        ActivityEvent::Status {
            key: child,
            state: AgentStatus::Idle,
        },
    );
    h.cockpit.pump();
    assert_eq!(
        h.count(),
        0,
        "a subagent finishing is its parent's business"
    );

    h.control.emit(1, ended());
    h.cockpit.pump();
    assert_eq!(h.count(), 1);
    h.cockpit.delete(second).unwrap();
    assert_eq!(h.count(), 0, "nothing survives its Thread");
}

// ------------------------------------------------------------- captures

/// A spawner that runs a stub CLI which answers the handshake, plays one
/// committed capture into the real adapter, then marks the end and
/// keeps stdin open so the Session stays alive.
struct Replay {
    provider: Provider,
    program: PathBuf,
    /// The capture plays for the first Session only; the other Thread —
    /// the one the operator sits on — gets a quiet scripted one.
    played: bool,
    frame: Arc<AtomicBool>,
}

const MARK: &str = "REPLAY_FINISHED";

/// Release one real adapter event per pump. Sleeping between wire lines
/// cannot enforce frame boundaries on a loaded runner: a finish and its
/// next resume may otherwise land in the same frame and correctly coalesce.
struct ReplaySession {
    inner: Box<dyn Session>,
    sender: mpsc::Sender<SessionEvent>,
    events: mpsc::Receiver<SessionEvent>,
    frame: Arc<AtomicBool>,
}

impl Session for ReplaySession {
    fn events(&self) -> &mpsc::Receiver<SessionEvent> {
        if self.frame.swap(false, Ordering::Relaxed) {
            if let Ok(event) = self.inner.events().try_recv() {
                self.sender.send(event).unwrap();
            }
        }
        &self.events
    }
    fn send(&mut self, text: &str) -> io::Result<()> {
        self.inner.send(text)
    }
    fn interrupt(&mut self) -> io::Result<()> {
        self.inner.interrupt()
    }
    fn respond_to_decision(&mut self, id: &str, answer: DecisionAnswer) -> io::Result<()> {
        self.inner.respond_to_decision(id, answer)
    }
}

impl Replay {
    fn claude(dir: &Path, fixture: &str) -> Self {
        let fixture = fixture_path(fixture);
        let program = dir.join("claude");
        let mark = serde_json::json!({"type":"stream_event","parent_tool_use_id":null,"event":{
            "type":"content_block_delta","delta":{"type":"text_delta","text":MARK}}});
        fs::write(&program, format!(
            "#!/bin/sh\ncase \"$1\" in --version) echo '2.1.261 (Claude Code)'; exit 0;; esac\n\
             echo '{{\"type\":\"control_response\",\"response\":{{\"subtype\":\"success\",\"request_id\":\"req_1\",\"response\":{{}}}}}}'\n\
             cat {}\nprintf '%s\\n' '{}'\nexec cat > /dev/null\n",
            quoted(&fixture), mark,
        ))
        .unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            provider: Provider::Claude,
            program,
            played: false,
            frame: Arc::new(AtomicBool::new(false)),
        }
    }

    fn codex(dir: &Path, fixture: &str, root: &str) -> Self {
        let fixture = fixture_path(fixture);
        let program = dir.join("codex");
        let mark = serde_json::json!({"method":"item/agentMessage/delta","params":{
            "threadId":root,"turnId":"mark","itemId":"mark","delta":MARK}});
        fs::write(
            &program,
            format!(
                "#!/bin/sh\ncase \"$1\" in --version) echo 'codex-cli 0.153.4'; exit 0;; esac\n\
             echo '{{\"id\":1,\"result\":{{\"userAgent\":\"stub\"}}}}'\n\
             cat {}\nprintf '%s\\n' '{}'\nexec cat > /dev/null\n",
                quoted(&fixture),
                mark,
            ),
        )
        .unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            provider: Provider::Codex,
            program,
            played: false,
            frame: Arc::new(AtomicBool::new(false)),
        }
    }
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/subagents")
        .join(name)
}

fn quoted(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

impl Spawner for Replay {
    fn spawn(&mut self, _: SpawnRequest) -> io::Result<Box<dyn Session>> {
        if std::mem::replace(&mut self.played, true) {
            let (sender, events) = mpsc::channel();
            sender
                .send(SessionEvent::Init {
                    session_id: "quiet".into(),
                    model: "model".into(),
                })
                .unwrap();
            std::mem::forget(sender);
            return Ok(Box::new(Scripted(events)));
        }
        let program = self.program.display().to_string();
        let inner: Box<dyn Session> = match self.provider {
            Provider::Claude => Box::new(
                ClaudeSession::spawn(ClaudeConfig {
                    program,
                    ..Default::default()
                })
                .map_err(io::Error::other)?,
            ),
            Provider::Codex => Box::new(
                CodexSession::spawn(CodexConfig {
                    program,
                    ..Default::default()
                })
                .map_err(io::Error::other)?,
            ),
        };
        let (sender, events) = mpsc::channel();
        Ok(Box::new(ReplaySession {
            inner,
            sender,
            events,
            frame: self.frame.clone(),
        }))
    }
}

fn main_text(cockpit: &Cockpit, thread: ThreadId) -> String {
    cockpit
        .thread(thread)
        .map(|open| {
            open.transcript()
                .blocks()
                .iter()
                .filter_map(|block| match &block.body {
                    Body::Paragraph { spans } => Some(
                        spans
                            .iter()
                            .map(|span| span.text.as_str())
                            .collect::<String>(),
                    ),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// Two Threads so the finished one is never the focused one; the whole
/// capture pumped through the real adapter; the Notices that resulted.
fn replay(label: &str, spawner: Replay) -> Vec<TurnOutcome> {
    let scratch = Scratch::new(label);
    let checkout = scratch.0.join("checkout");
    fs::create_dir(&checkout).unwrap();
    let provider = spawner.provider;
    let frame = spawner.frame.clone();
    let mut cockpit = Cockpit::new(
        Store::open(scratch.0.join("store")).unwrap(),
        Box::new(spawner),
    );
    // These captures resume Main explicitly. Grace expiry is covered by
    // the injected-clock unit tests, not the replay runner's wall clock.
    cockpit.set_notification_grace(Duration::MAX);
    let watched = cockpit
        .open(
            provider,
            WorkspaceChoice::Main {
                checkout: checkout.clone(),
            },
        )
        .unwrap();
    let other = cockpit
        .open(provider, WorkspaceChoice::Main { checkout })
        .unwrap();
    assert!(cockpit.focus(PaneIdentity::Thread(other)));
    cockpit.send(watched, "go".into());
    let deadline = Instant::now() + Duration::from_secs(15);
    while !main_text(&cockpit, watched).contains(MARK) {
        assert!(
            Instant::now() < deadline,
            "the capture never finished replaying"
        );
        let before = cockpit.notifications().newest();
        frame.store(true, Ordering::Relaxed);
        cockpit.pump();
        if cockpit.notifications().newest() != before {
            let activity = cockpit.thread(watched).unwrap().activity();
            assert!(!activity.main().busy(), "Main has not finished");
            assert!(activity.pending_decisions().is_empty());
            assert!(
                activity.children().iter().all(|child| {
                    !child.fresh()
                        || !matches!(
                            child.status(),
                            AgentStatus::Working | AgentStatus::Pending | AgentStatus::Waiting
                        )
                }),
                "a Notice must not precede an unfinished child"
            );
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let notices: Vec<_> = cockpit
        .notifications()
        .notices()
        .map(|notice| {
            assert_eq!(notice.thread, watched);
            notice.outcome.clone()
        })
        .collect();
    drop(cockpit);
    notices
}

/// Claude 2.1.261, two background agents outliving Main's own result:
/// four `result` envelopes, two of them autonomous. Exactly two finishes —
/// the operator's second turn, and the autonomous turn that reported the
/// last agent — and none while agents were still at work.
#[test]
fn the_claude_overlap_capture_finishes_twice_never_while_agents_run() {
    let scratch = Scratch::new("claude-overlap");
    let outcomes = replay(
        "claude-overlap-cockpit",
        Replay::claude(&scratch.0, "claude-overlap-2.1.261.jsonl"),
    );
    assert_eq!(
        outcomes,
        vec![TurnOutcome::Completed, TurnOutcome::Completed]
    );
}

/// Claude 2.1.261, a child and a grandchild in the foreground: Main's one
/// result comes after both, and is the one finish.
#[test]
fn the_claude_nested_capture_finishes_once() {
    let scratch = Scratch::new("claude-nested");
    let outcomes = replay(
        "claude-nested-cockpit",
        Replay::claude(&scratch.0, "claude-nested-2.1.261.jsonl"),
    );
    assert_eq!(outcomes, vec![TurnOutcome::Completed]);
}

/// Codex 0.153.4, two concurrent children and a reused one: the parent
/// waits on them inside its own turn, so its one `turn/completed` — after
/// three child turns — is the one finish.
#[test]
fn the_codex_overlap_capture_finishes_once_after_every_child() {
    let scratch = Scratch::new("codex-overlap");
    let outcomes = replay(
        "codex-overlap-cockpit",
        Replay::codex(
            &scratch.0,
            "codex-overlap-reuse-0.153.4.jsonl",
            "01a07039-dc5d-76f3-95be-b9343f62216a",
        ),
    );
    assert_eq!(outcomes, vec![TurnOutcome::Completed]);
}

/// Codex 0.153.4, child and grandchild: one finish, the root's.
#[test]
fn the_codex_nested_capture_finishes_once() {
    let scratch = Scratch::new("codex-nested");
    let outcomes = replay(
        "codex-nested-cockpit",
        Replay::codex(
            &scratch.0,
            "codex-nested-0.153.4.jsonl",
            "01a0703a-f38e-7b71-a7c8-a820c9d87bfa",
        ),
    );
    assert_eq!(outcomes, vec![TurnOutcome::Completed]);
}
