//! Subagent acceptance through Cockpit's public Interface and scripted
//! provider Sessions. No CLI, network, or private folds.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use ferrite_core::activity::{
    ActivityEvent, AgentInfo, AgentKey, AgentStatus, ExecutionEvent, Subject, TranscriptCoverage,
};
use ferrite_core::cockpit::{Cockpit, ProviderChoice, RssSampler, SpawnRequest, Spawner};
use ferrite_core::providers::Session;
use ferrite_core::store::{Provider, Store};
use ferrite_core::transcript::{Body, ToolState, Transcript};
use ferrite_core::workspace::WorkspaceChoice;
use ferrite_core::{Decision, DecisionAnswer, SessionEvent, ThreadId, ToolResult, TurnOutcome};
use serde_json::json;

#[derive(Debug, Clone, PartialEq)]
struct Reply {
    id: String,
    answer: DecisionAnswer,
    succeeded: bool,
}

struct SessionControl {
    events: mpsc::Sender<SessionEvent>,
    sent: Vec<String>,
    replies: Vec<Reply>,
    fail_next_reply: bool,
    dropped: bool,
    resume: Option<String>,
}

#[derive(Clone, Default)]
struct Control(Arc<Mutex<Vec<SessionControl>>>);

impl Control {
    fn emit(&self, session: usize, event: SessionEvent) {
        self.0.lock().unwrap()[session].events.send(event).unwrap();
    }

    fn activity(&self, session: usize, event: ActivityEvent) {
        self.emit(session, SessionEvent::Activity(event));
    }

    fn sent(&self, session: usize) -> Vec<String> {
        self.0.lock().unwrap()[session].sent.clone()
    }

    fn replies(&self, session: usize) -> Vec<Reply> {
        self.0.lock().unwrap()[session].replies.clone()
    }

    fn fail_next_reply(&self, session: usize) {
        self.0.lock().unwrap()[session].fail_next_reply = true;
    }
}

struct ScriptedSession {
    events: mpsc::Receiver<SessionEvent>,
    control: Control,
    index: usize,
}

impl Session for ScriptedSession {
    fn events(&self) -> &mpsc::Receiver<SessionEvent> {
        &self.events
    }

    fn send(&mut self, text: &str) -> io::Result<()> {
        self.control.0.lock().unwrap()[self.index]
            .sent
            .push(text.to_owned());
        Ok(())
    }

    fn interrupt(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn respond_to_decision(&mut self, id: &str, answer: DecisionAnswer) -> io::Result<()> {
        let mut sessions = self.control.0.lock().unwrap();
        let session = &mut sessions[self.index];
        let succeeded = !std::mem::take(&mut session.fail_next_reply);
        session.replies.push(Reply {
            id: id.to_owned(),
            answer,
            succeeded,
        });
        if succeeded {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "scripted reply failure",
            ))
        }
    }
}

impl Drop for ScriptedSession {
    fn drop(&mut self) {
        self.control.0.lock().unwrap()[self.index].dropped = true;
    }
}

impl Spawner for Control {
    fn spawn(&mut self, request: SpawnRequest) -> io::Result<Box<dyn Session>> {
        let (sender, events) = mpsc::channel();
        let mut sessions = self.0.lock().unwrap();
        let index = sessions.len();
        sender
            .send(SessionEvent::Init {
                session_id: request
                    .resume
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("main-{index}")),
                model: request.model.unwrap_or("main-model").to_owned(),
            })
            .unwrap();
        sessions.push(SessionControl {
            events: sender,
            sent: Vec::new(),
            replies: Vec::new(),
            fail_next_reply: false,
            dropped: false,
            resume: request.resume.map(str::to_owned),
        });
        Ok(Box::new(ScriptedSession {
            events,
            control: self.clone(),
            index,
        }))
    }
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ferrite-subagent-cockpit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Harness {
    // Drop Sessions and writers before deleting their exclusively owned store.
    cockpit: Cockpit,
    control: Control,
    thread: ThreadId,
    scratch: Scratch,
}

impl Harness {
    fn new() -> Self {
        let scratch = Scratch::new();
        let checkout = scratch.0.join("checkout");
        fs::create_dir(&checkout).unwrap();
        let control = Control::default();
        let mut cockpit = Cockpit::new(
            Store::open(scratch.0.join("store")).unwrap(),
            Box::new(control.clone()),
        );
        let thread = cockpit
            .open(Provider::Codex, WorkspaceChoice::Main { checkout })
            .unwrap();
        cockpit.pump();
        Self {
            cockpit,
            control,
            thread,
            scratch,
        }
    }

    fn child(&self, native: &str) -> AgentKey {
        let key = AgentKey::new(Provider::Codex, "main-0", native);
        let mut info = AgentInfo::new(key.clone());
        info.parent = Some(Subject::Main);
        info.name = Some(native.to_owned());
        info.coverage = TranscriptCoverage::Live;
        self.control.activity(0, ActivityEvent::Discovered(info));
        key
    }

    fn content(&self, session: usize, key: &AgentKey, id: &str, event: ExecutionEvent) {
        self.control.activity(
            session,
            ActivityEvent::Content {
                key: key.clone(),
                id: Some(id.to_owned()),
                event,
            },
        );
    }

    fn child_decision(&self, session: usize, key: &AgentKey, id: &str) {
        self.control.activity(
            session,
            ActivityEvent::Decision {
                subject: Some(Subject::Subagent(key.clone())),
                decision: decision(id),
            },
        );
    }
}

fn decision(id: &str) -> Decision {
    Decision {
        delivery: Default::default(),
        id: id.to_owned(),
        tool_use_id: "shared-tool".to_owned(),
        tool_name: "Write".to_owned(),
        description: "Write the fixture".to_owned(),
        input: json!({"file_path": "fixture.txt"}),
        suggestions: Vec::new(),
    }
}

fn allow() -> DecisionAnswer {
    DecisionAnswer::Allow {
        input: json!({"file_path": "fixture.txt"}),
    }
}

fn deny() -> DecisionAnswer {
    DecisionAnswer::Deny {
        message: "test denial".to_owned(),
    }
}

fn ended() -> SessionEvent {
    SessionEvent::TurnEnded {
        outcome: TurnOutcome::Completed,
        cost_usd: None,
    }
}

fn child_ended() -> ExecutionEvent {
    ExecutionEvent::TurnEnded {
        outcome: TurnOutcome::Completed,
        cost_usd: Some(99.0),
    }
}

fn text(transcript: &Transcript) -> String {
    transcript
        .blocks()
        .iter()
        .filter_map(|block| match &block.body {
            Body::Paragraph { spans } | Body::Heading { spans, .. } | Body::Bullet { spans } => {
                Some(
                    spans
                        .iter()
                        .map(|span| span.text.as_str())
                        .collect::<String>(),
                )
            }
            Body::Code { source, .. } | Body::Prompt(source) => Some(source.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

struct OverBudget;

impl RssSampler for OverBudget {
    fn sample(&mut self, _: ThreadId, _: Option<u32>) -> Option<u64> {
        Some(2)
    }
}

#[test]
fn child_completion_cannot_release_mains_prompt_or_pending_decision() {
    let mut h = Harness::new();
    h.cockpit.send(h.thread, "main prompt".to_owned());
    h.control.emit(
        0,
        SessionEvent::TextDelta {
            text: "MAIN is still working".to_owned(),
        },
    );
    h.control.emit(
        0,
        SessionEvent::DecisionRequested {
            decision: decision("main-request"),
        },
    );
    let child = h.child("alpha");
    h.cockpit.pump();
    h.cockpit.queue(h.thread, "queued main prompt".to_owned());

    h.content(
        0,
        &child,
        "child-message",
        ExecutionEvent::Text {
            text: "CHILD finished".to_owned(),
        },
    );
    h.content(0, &child, "child-turn-end", child_ended());
    let updates = h.cockpit.pump();
    let view = h.cockpit.thread(h.thread).unwrap();
    assert!(view.busy(), "Main still owns its unfinished turn");
    assert_eq!(view.queued(), Some("queued main prompt"));
    assert_eq!(
        h.control.sent(0).len(),
        1,
        "child completion must not send Main's queue"
    );
    assert!(view
        .activity()
        .decisions()
        .iter()
        .any(|pending| pending.subject == Some(Subject::Main)
            && pending.decision.id == "main-request"));
    assert!(text(view.transcript()).contains("MAIN is still working"));
    assert!(!text(view.transcript()).contains("CHILD finished"));
    assert_eq!(view.transcript().last_cost(), None);
    assert!(!view.transcript().turn_completed());
    assert!(
        updates.iter().all(|update| update.dirty.is_empty()),
        "child changes must not mark overlapping Main BlockIds dirty"
    );

    h.control.emit(0, ended());
    h.cockpit.pump();
    assert_eq!(h.control.sent(0).len(), 2);
    assert!(h.control.sent(0)[1].contains("queued main prompt"));
}

#[test]
fn autonomous_background_completion_does_not_retire_mains_live_work() {
    let mut h = Harness::new();
    h.cockpit.send(h.thread, "main prompt".to_owned());
    h.control.emit(
        0,
        SessionEvent::TextDelta {
            text: "MAIN is still working".to_owned(),
        },
    );
    h.control.emit(
        0,
        SessionEvent::DecisionRequested {
            decision: decision("main-request"),
        },
    );
    h.cockpit.pump();
    h.cockpit.queue(h.thread, "queued main prompt".to_owned());
    h.control.activity(
        0,
        ActivityEvent::BackgroundTurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: Some(0.01),
        },
    );
    h.cockpit.pump();
    let view = h.cockpit.thread(h.thread).unwrap();
    assert!(
        view.busy(),
        "an autonomous notification result cannot retire Main's turn"
    );
    assert_eq!(view.queued(), Some("queued main prompt"));
    assert_eq!(h.control.sent(0).len(), 1);
    assert!(view
        .activity()
        .decisions()
        .iter()
        .any(|pending| pending.decision.id == "main-request"));
}

#[test]
fn an_autonomous_reply_after_main_finished_does_not_leave_main_permanently_busy() {
    let mut h = Harness::new();
    h.cockpit.send(h.thread, "main prompt".to_owned());
    h.control.emit(0, ended());
    h.cockpit.pump();
    assert!(!h.cockpit.thread(h.thread).unwrap().busy());
    h.control.emit(
        0,
        SessionEvent::TextDelta {
            text: "Autonomous task summary".to_owned(),
        },
    );
    h.control.activity(
        0,
        ActivityEvent::BackgroundTurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: Some(0.01),
        },
    );
    h.cockpit.pump();
    assert!(
        !h.cockpit.thread(h.thread).unwrap().busy(),
        "finished autonomous text cannot leave a permanent working state"
    );
    assert_eq!(h.control.sent(0).len(), 1);
}

#[test]
fn overlapping_tool_and_message_ids_keep_transcripts_usage_and_resume_isolated() {
    let mut h = Harness::new();
    let alpha = h.child("alpha");
    let beta = h.child("beta");
    h.control.emit(
        0,
        SessionEvent::TextDelta {
            text: "MAIN text".to_owned(),
        },
    );
    h.control.emit(
        0,
        SessionEvent::ToolStarted {
            id: "shared-tool".to_owned(),
            name: "Write".to_owned(),
            input: json!({"file_path": "main.txt"}),
        },
    );
    h.control.emit(
        0,
        SessionEvent::TokenUsage {
            total_tokens: 100,
            input_tokens: 80,
            cached_input_tokens: 0,
            output_tokens: 20,
            reasoning_output_tokens: 0,
            context_window: Some(1000),
        },
    );
    for (child, label) in [(&alpha, "ALPHA"), (&beta, "BETA")] {
        h.content(
            0,
            child,
            "shared-message",
            ExecutionEvent::Text {
                text: label.to_owned(),
            },
        );
        h.content(
            0,
            child,
            "shared-tool-start",
            ExecutionEvent::ToolStarted {
                id: "shared-tool".to_owned(),
                name: "Write".to_owned(),
                input: json!({"file_path": "child.txt"}),
            },
        );
    }
    h.content(
        0,
        &alpha,
        "alpha-output",
        ExecutionEvent::ToolCompleted {
            id: "shared-tool".to_owned(),
            output: "alpha output".to_owned(),
            is_error: false,
            result: ToolResult::Opaque,
        },
    );
    h.content(
        0,
        &alpha,
        "alpha-usage",
        ExecutionEvent::TokenUsage {
            total_tokens: 999,
            input_tokens: 900,
            cached_input_tokens: 0,
            output_tokens: 99,
            reasoning_output_tokens: 0,
            context_window: Some(9999),
        },
    );
    h.cockpit.pump();
    let view = h.cockpit.thread(h.thread).unwrap();
    let activity = view.activity();
    let alpha_view = activity.subject(&Subject::Subagent(alpha.clone())).unwrap();
    let beta_view = activity.subject(&Subject::Subagent(beta)).unwrap();
    assert!(text(view.transcript()).contains("MAIN text"));
    assert!(!text(view.transcript()).contains("ALPHA"));
    assert_eq!(text(alpha_view.transcript()), "ALPHA");
    assert_eq!(text(beta_view.transcript()), "BETA");
    assert_eq!(view.transcript().usage().unwrap().total_tokens, 100);
    assert_eq!(alpha_view.transcript().usage().unwrap().total_tokens, 999);
    assert_eq!(view.transcript().session_id(), Some("main-0"));
    assert_eq!(view.transcript().model(), Some("main-model"));
    let tool_state = |transcript: &Transcript| {
        transcript
            .blocks()
            .iter()
            .find_map(|block| match &block.body {
                Body::Tool(tool) if tool.call == "shared-tool" => Some(tool.state.clone()),
                _ => None,
            })
    };
    assert_eq!(tool_state(view.transcript()), Some(ToolState::Running));
    assert_eq!(tool_state(alpha_view.transcript()), Some(ToolState::Ok));
    assert_eq!(tool_state(beta_view.transcript()), Some(ToolState::Running));

    h.cockpit.park(h.thread).unwrap();
    let snapshot = Store::open(h.scratch.0.join("store"))
        .unwrap()
        .load(h.thread)
        .unwrap();
    assert_eq!(snapshot.resume_target(), Some("main-0"));
    h.cockpit.revive(h.thread).unwrap();
    assert_eq!(
        h.control.0.lock().unwrap()[1].resume.as_deref(),
        Some("main-0")
    );
}

#[test]
fn hidden_child_decisions_reply_in_reverse_order_and_failed_writes_stay_pending() {
    let mut h = Harness::new();
    let alpha = h.child("alpha");
    let beta = h.child("beta");
    h.child_decision(0, &alpha, "0");
    h.child_decision(0, &beta, "\"0\"");
    h.cockpit.pump();
    let pending = h
        .cockpit
        .thread(h.thread)
        .unwrap()
        .activity()
        .decisions()
        .to_vec();
    assert_eq!(pending.len(), 2);
    let alpha_handle = pending
        .iter()
        .find(|item| item.subject == Some(Subject::Subagent(alpha.clone())))
        .unwrap()
        .handle
        .clone();
    let beta_handle = pending
        .iter()
        .find(|item| item.subject == Some(Subject::Subagent(beta.clone())))
        .unwrap()
        .handle
        .clone();
    // Reading Main instead of either child cannot retarget the held request.
    assert!(h
        .cockpit
        .thread(h.thread)
        .unwrap()
        .activity()
        .subject(&Subject::Main)
        .is_some());
    h.control.fail_next_reply(0);
    assert_eq!(
        h.cockpit
            .respond_decision(h.thread, &beta_handle, allow())
            .unwrap_err()
            .kind(),
        io::ErrorKind::BrokenPipe
    );
    assert_eq!(
        h.cockpit
            .thread(h.thread)
            .unwrap()
            .activity()
            .decisions()
            .len(),
        2
    );
    assert!(h
        .cockpit
        .respond_decision(h.thread, &beta_handle, allow())
        .unwrap());
    assert_eq!(
        h.cockpit
            .thread(h.thread)
            .unwrap()
            .activity()
            .decisions()
            .len(),
        1
    );
    assert!(h
        .cockpit
        .respond_decision(h.thread, &alpha_handle, deny())
        .unwrap());
    assert!(!h
        .cockpit
        .respond_decision(h.thread, &beta_handle, allow())
        .unwrap());
    assert!(h
        .cockpit
        .thread(h.thread)
        .unwrap()
        .activity()
        .decisions()
        .is_empty());
    assert_eq!(
        h.control.replies(0),
        vec![
            Reply {
                id: "\"0\"".to_owned(),
                answer: allow(),
                succeeded: false
            },
            Reply {
                id: "\"0\"".to_owned(),
                answer: allow(),
                succeeded: true
            },
            Reply {
                id: "0".to_owned(),
                answer: deny(),
                succeeded: true
            },
        ]
    );
}

#[test]
fn an_unresolved_decision_remains_visible_without_guessing_its_owner() {
    let mut h = Harness::new();
    h.control.activity(
        0,
        ActivityEvent::Decision {
            subject: None,
            decision: decision("unresolved"),
        },
    );
    h.cockpit.pump();
    let pending = h
        .cockpit
        .thread(h.thread)
        .unwrap()
        .activity()
        .decisions()
        .to_vec();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].subject, None);
    assert!(h
        .cockpit
        .respond_decision(h.thread, &pending[0].handle, deny())
        .unwrap());
    assert_eq!(h.control.replies(0)[0].id, "unresolved");
}

#[test]
fn a_child_turn_end_retires_only_that_childs_pending_decisions() {
    let mut h = Harness::new();
    let alpha = h.child("alpha");
    let beta = h.child("beta");
    h.control.emit(
        0,
        SessionEvent::DecisionRequested {
            decision: decision("main-request"),
        },
    );
    h.child_decision(0, &alpha, "alpha-request");
    h.child_decision(0, &beta, "beta-request");
    h.cockpit.pump();
    let alpha_handle = h
        .cockpit
        .thread(h.thread)
        .unwrap()
        .activity()
        .decisions()
        .iter()
        .find(|pending| pending.decision.id == "alpha-request")
        .unwrap()
        .handle
        .clone();
    h.content(0, &alpha, "alpha-done", child_ended());
    h.cockpit.pump();
    let mut remaining = h
        .cockpit
        .thread(h.thread)
        .unwrap()
        .activity()
        .decisions()
        .iter()
        .map(|pending| pending.decision.id.as_str())
        .collect::<Vec<_>>();
    remaining.sort_unstable();
    assert_eq!(remaining, ["beta-request", "main-request"]);
    assert!(!h
        .cockpit
        .respond_decision(h.thread, &alpha_handle, allow())
        .unwrap());
    assert!(h.control.replies(0).is_empty());
}

#[test]
fn cancellation_retires_only_its_request_and_clears_waiting_after_the_last_one() {
    let mut h = Harness::new();
    h.cockpit.send(h.thread, "main prompt".to_owned());
    h.control.emit(
        0,
        SessionEvent::TextDelta {
            text: "MAIN is still working".to_owned(),
        },
    );
    let alpha = h.child("alpha");
    for id in ["main-one", "main-two"] {
        h.control.emit(
            0,
            SessionEvent::DecisionRequested {
                decision: decision(id),
            },
        );
    }
    h.child_decision(0, &alpha, "child-request");
    h.cockpit.pump();
    let old = h
        .cockpit
        .thread(h.thread)
        .unwrap()
        .activity()
        .decisions()
        .to_vec();
    h.control.activity(
        0,
        ActivityEvent::DecisionCancelled {
            id: "main-one".to_owned(),
        },
    );
    h.cockpit.pump();
    let view = h.cockpit.thread(h.thread).unwrap();
    assert_eq!(view.activity().decisions().len(), 2);
    assert_eq!(view.activity().main().status(), AgentStatus::Waiting);

    h.control.activity(
        0,
        ActivityEvent::DecisionCancelled {
            id: "main-two".to_owned(),
        },
    );
    h.cockpit.pump();
    let view = h.cockpit.thread(h.thread).unwrap();
    assert!(view.busy(), "request cancellation is not turn completion");
    assert_ne!(view.activity().main().status(), AgentStatus::Waiting);
    assert_eq!(view.activity().decisions().len(), 1);

    h.control.activity(
        0,
        ActivityEvent::DecisionCancelled {
            id: "child-request".to_owned(),
        },
    );
    h.cockpit.pump();
    let view = h.cockpit.thread(h.thread).unwrap();
    assert!(view.activity().decisions().is_empty());
    assert_ne!(
        view.activity()
            .subject(&Subject::Subagent(alpha))
            .unwrap()
            .status(),
        AgentStatus::Waiting
    );
    for pending in old {
        assert!(!h
            .cockpit
            .respond_decision(h.thread, &pending.handle, allow())
            .unwrap());
    }
    assert!(h.control.replies(0).is_empty());
}

#[test]
fn detached_children_wait_for_verified_parentage_without_leaking_into_main() {
    let mut h = Harness::new();
    let parent = AgentKey::new(Provider::Codex, "main-0", "late-parent");
    let child = AgentKey::new(Provider::Codex, "main-0", "early-child");
    let mut child_info = AgentInfo::new(child.clone());
    child_info.parent = Some(Subject::Subagent(parent.clone()));
    child_info.coverage = TranscriptCoverage::Live;
    h.control.activity(0, ActivityEvent::Discovered(child_info));
    h.content(
        0,
        &child,
        "early-frame",
        ExecutionEvent::Text {
            text: "quarantined child history".to_owned(),
        },
    );
    h.child_decision(0, &child, "detached-request");
    h.cockpit.pump();
    let view = h.cockpit.thread(h.thread).unwrap();
    assert!(
        view.activity().children().is_empty(),
        "an incomplete ancestry chain must not produce a tab"
    );
    assert!(!text(view.transcript()).contains("quarantined child history"));
    assert_eq!(
        view.activity().decisions().len(),
        1,
        "detached attention must remain answerable"
    );

    let mut parent_info = AgentInfo::new(parent.clone());
    parent_info.parent = Some(Subject::Main);
    h.control
        .activity(0, ActivityEvent::Discovered(parent_info.clone()));
    h.cockpit.pump();
    let view = h.cockpit.thread(h.thread).unwrap();
    assert_eq!(view.activity().children().len(), 2);
    assert!(text(
        view.activity()
            .subject(&Subject::Subagent(child.clone()))
            .unwrap()
            .transcript()
    )
    .contains("quarantined child history"));

    parent_info.parent = Some(Subject::Subagent(child));
    h.control
        .activity(0, ActivityEvent::Discovered(parent_info));
    h.cockpit.pump();
    let view = h.cockpit.thread(h.thread).unwrap();
    let parent_view = view
        .activity()
        .children()
        .into_iter()
        .find(|agent| agent.key() == &parent)
        .unwrap();
    assert_eq!(
        parent_view.info().parent,
        Some(Subject::Main),
        "a cycle cannot overwrite verified parentage"
    );
}

#[derive(Clone, Copy)]
enum Replacement {
    Watchdog,
    ParkAndRevive,
    Handover,
}

fn stale_request_after_replacement(replacement: Replacement) {
    let mut h = Harness::new();
    h.cockpit.send(h.thread, "main prompt".to_owned());
    h.control.emit(
        0,
        SessionEvent::TextDelta {
            text: "MAIN history".to_owned(),
        },
    );
    h.control.emit(0, ended());
    let alpha = h.child("alpha");
    h.content(
        0,
        &alpha,
        "history",
        ExecutionEvent::Text {
            text: "CHILD history".to_owned(),
        },
    );
    h.child_decision(0, &alpha, "reused-request-id");
    h.cockpit.pump();
    let old_handle = h.cockpit.thread(h.thread).unwrap().activity().decisions()[0]
        .handle
        .clone();
    match replacement {
        Replacement::Watchdog => {
            h.cockpit.watch_memory(Box::new(OverBudget), 1);
            assert_eq!(h.cockpit.sweep().len(), 1);
        }
        Replacement::ParkAndRevive => {
            h.cockpit.queue(h.thread, "unpersisted queue".to_owned());
            h.cockpit.park(h.thread).unwrap();
            h.cockpit.revive(h.thread).unwrap();
        }
        Replacement::Handover => {
            h.cockpit
                .set_provider(
                    h.thread,
                    ProviderChoice {
                        provider: Provider::Claude,
                        model: Some("replacement-model".to_owned()),
                    },
                )
                .unwrap();
        }
    }
    h.cockpit.pump();
    assert!(h.control.0.lock().unwrap()[0].dropped);
    assert!(
        h.control.0.lock().unwrap()[0].events.send(ended()).is_err(),
        "old Session events cannot enter the replacement"
    );
    assert!(
        h.control.sent(1).is_empty(),
        "restoration/replacement must not release a queued prompt"
    );
    let view = h.cockpit.thread(h.thread).unwrap();
    assert!(
        view.activity().decisions().is_empty(),
        "historical approvals must not become actionable"
    );
    assert!(text(
        view.activity()
            .subject(&Subject::Subagent(alpha.clone()))
            .unwrap()
            .transcript()
    )
    .contains("CHILD history"));
    assert!(text(view.transcript()).contains("MAIN history"));
    assert!(!h
        .cockpit
        .respond_decision(h.thread, &old_handle, allow())
        .unwrap());

    let current = if matches!(replacement, Replacement::Handover) {
        let key = AgentKey::new(Provider::Claude, "main-1", "alpha");
        let mut info = AgentInfo::new(key.clone());
        info.parent = Some(Subject::Main);
        h.control.activity(1, ActivityEvent::Discovered(info));
        key
    } else {
        alpha
    };
    h.child_decision(1, &current, "reused-request-id");
    h.cockpit.pump();
    let current_handle = h.cockpit.thread(h.thread).unwrap().activity().decisions()[0]
        .handle
        .clone();
    assert_ne!(current_handle, old_handle);
    assert!(!h
        .cockpit
        .respond_decision(h.thread, &old_handle, deny())
        .unwrap());
    assert!(h.control.replies(1).is_empty());
    assert!(h
        .cockpit
        .respond_decision(h.thread, &current_handle, allow())
        .unwrap());
    assert_eq!(h.control.replies(1).len(), 1);
    assert!(h.control.replies(0).is_empty());
}

#[test]
fn watchdog_rejects_old_decisions_and_preserves_child_history() {
    stale_request_after_replacement(Replacement::Watchdog);
}

#[test]
fn parking_restores_child_history_without_decisions_or_a_queued_prompt() {
    stale_request_after_replacement(Replacement::ParkAndRevive);
}

#[test]
fn handover_preserves_child_history_and_rejects_old_provider_decisions() {
    stale_request_after_replacement(Replacement::Handover);
}

fn pump_until(h: &mut Harness, mut ready: impl FnMut(&Cockpit) -> bool) {
    // These are persistence/replay acceptance checks, not throughput budgets.
    // Parallel Windows runners can spend over five seconds flushing the tree.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        h.cockpit.pump();
        if ready(&h.cockpit) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "background history did not settle ({} children observed)",
            h.cockpit
                .thread(h.thread)
                .unwrap()
                .activity()
                .children()
                .len()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Fill the default 128-child transcript cache, then view child 129 to evict
/// the oldest idle child. Every child's metadata and log remain available.
fn evicted_oldest(h: &mut Harness) -> AgentKey {
    let mut children = Vec::new();
    for index in 0..129 {
        let key = h.child(&format!("cache-child-{index}"));
        h.content(
            0,
            &key,
            &format!("prefix-{index}"),
            ExecutionEvent::Text {
                text: format!("FROZEN PREFIX {index}\n\n"),
            },
        );
        h.content(0, &key, &format!("finished-{index}"), child_ended());
        children.push(key);
    }
    let thread = h.thread;
    let last = Subject::Subagent(children.last().unwrap().clone());
    pump_until(h, |cockpit| {
        let view = cockpit.thread(thread).unwrap().activity();
        view.children().len() == 129 && view.subject(&last).unwrap().status() == AgentStatus::Idle
    });
    assert!(!h
        .cockpit
        .thread(thread)
        .unwrap()
        .activity()
        .subject(&last)
        .unwrap()
        .retained());
    assert!(h.cockpit.ensure_subject_history(thread, &last).unwrap());
    assert!(
        !h.cockpit.ensure_subject_history(thread, &last).unwrap(),
        "a pending read must not be queued twice"
    );
    pump_until(h, |cockpit| {
        let view = cockpit.thread(thread).unwrap().activity();
        let last = view.subject(&last).unwrap();
        last.retained() && text(last.transcript()).contains("FROZEN PREFIX 128")
    });
    let first = children.remove(0);
    let view = h.cockpit.thread(thread).unwrap().activity();
    let first_view = view.subject(&Subject::Subagent(first.clone())).unwrap();
    assert!(!first_view.retained());
    assert!(text(first_view.transcript()).is_empty());
    assert_eq!(view.children().len(), 129);
    first
}

#[test]
fn evicted_child_history_merges_frozen_prefix_and_live_tail_once_without_changing_runtime() {
    let mut h = Harness::new();
    let key = evicted_oldest(&mut h);
    let subject = Subject::Subagent(key.clone());
    h.content(
        0,
        &key,
        "working-before-read",
        ExecutionEvent::Text {
            text: "PRE CHECKPOINT\n\n".to_owned(),
        },
    );
    h.child_decision(0, &key, "history-still-waiting");
    h.cockpit.pump();
    let thread = h.thread;
    let before = h.cockpit.thread(thread).unwrap().activity();
    let before_subject = before.subject(&subject).unwrap();
    assert!(before_subject.busy());
    assert_eq!(before_subject.status(), AgentStatus::Waiting);
    let handle = before.decisions()[0].handle.clone();

    assert!(h.cockpit.ensure_subject_history(thread, &subject).unwrap());
    h.content(
        0,
        &key,
        "after-checkpoint",
        ExecutionEvent::Text {
            text: "LIVE AFTER CHECKPOINT\n\n".to_owned(),
        },
    );
    pump_until(&mut h, |cockpit| {
        let view = cockpit.thread(thread).unwrap().activity();
        let child = view.subject(&subject).unwrap();
        child.retained()
            && text(child.transcript()).contains("FROZEN PREFIX 0")
            && text(child.transcript()).contains("LIVE AFTER CHECKPOINT")
    });
    let view = h.cockpit.thread(thread).unwrap().activity();
    let child = view.subject(&subject).unwrap();
    assert_eq!(
        text(child.transcript()),
        "FROZEN PREFIX 0\nPRE CHECKPOINT\nLIVE AFTER CHECKPOINT"
    );
    assert!(child.busy());
    assert!(child.fresh());
    assert_eq!(child.status(), AgentStatus::Waiting);
    assert_eq!(view.decisions().len(), 1);
    assert_eq!(view.decisions()[0].handle, handle);
    assert!(h.control.sent(0).is_empty(), "viewing cannot send a prompt");
    assert!(!h.cockpit.ensure_subject_history(thread, &subject).unwrap());
    assert!(!h
        .cockpit
        .ensure_subject_history(thread, &Subject::Main)
        .unwrap());
}

#[test]
fn header_rewrite_invalidates_old_history_read_and_a_fresh_selection_recovers() {
    let mut h = Harness::new();
    let key = evicted_oldest(&mut h);
    let subject = Subject::Subagent(key.clone());
    let thread = h.thread;
    assert!(h.cockpit.ensure_subject_history(thread, &subject).unwrap());
    h.cockpit
        .rename_thread(thread, "renamed during read")
        .unwrap();
    h.content(
        0,
        &key,
        "after-rewrite",
        ExecutionEvent::Text {
            text: "AFTER HEADER REWRITE\n\n".to_owned(),
        },
    );
    h.cockpit.pump();
    assert!(
        !h.cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&subject)
            .unwrap()
            .retained(),
        "the pre-rewrite result cannot repopulate the cache"
    );
    assert!(h.cockpit.ensure_subject_history(thread, &subject).unwrap());
    pump_until(&mut h, |cockpit| {
        let child = cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&subject)
            .unwrap();
        child.retained() && text(child.transcript()).contains("AFTER HEADER REWRITE")
    });
    let child = h
        .cockpit
        .thread(thread)
        .unwrap()
        .activity()
        .subject(&subject)
        .unwrap();
    assert_eq!(
        text(child.transcript()),
        "FROZEN PREFIX 0\nAFTER HEADER REWRITE"
    );
}

#[test]
fn watchdog_replacement_invalidates_pending_history_read_before_a_new_read() {
    let mut h = Harness::new();
    let key = evicted_oldest(&mut h);
    let subject = Subject::Subagent(key.clone());
    let thread = h.thread;
    let generation = h.cockpit.thread(thread).unwrap().activity().generation();
    assert!(h.cockpit.ensure_subject_history(thread, &subject).unwrap());
    h.cockpit.watch_memory(Box::new(OverBudget), 1);
    assert_eq!(h.cockpit.sweep().len(), 1);
    h.cockpit.pump();
    let view = h.cockpit.thread(thread).unwrap().activity();
    assert_ne!(view.generation(), generation);
    assert!(
        !view.subject(&subject).unwrap().retained(),
        "the previous Session generation cannot repopulate the cache"
    );
    assert!(h.cockpit.ensure_subject_history(thread, &subject).unwrap());
    pump_until(&mut h, |cockpit| {
        let child = cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&subject)
            .unwrap();
        child.retained() && text(child.transcript()).contains("FROZEN PREFIX 0")
    });
    let child = h
        .cockpit
        .thread(thread)
        .unwrap()
        .activity()
        .subject(&subject)
        .unwrap();
    assert_eq!(text(child.transcript()), "FROZEN PREFIX 0");
    assert!(!child.busy());
    assert!(!child.fresh());
    assert!(h.control.sent(1).is_empty());
}

#[test]
fn aliasing_evicted_history_into_a_retained_child_recovers_both_prefixes_and_live_decision() {
    // Cover selection both before and after the identity join. The selected
    // case is valid whether disk completion or the live Alias reaches a pump
    // first; the unselected case always requires recovery of an evicted prefix.
    for select_before_alias in [false, true] {
        let mut h = Harness::new();
        let source = evicted_oldest(&mut h);
        let canonical = AgentKey::new(Provider::Codex, "main-0", "cache-child-1");
        let source_subject = Subject::Subagent(source.clone());
        let canonical_subject = Subject::Subagent(canonical.clone());
        let thread = h.thread;
        assert!(h
            .cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&canonical_subject)
            .unwrap()
            .retained());
        h.content(
            0,
            &canonical,
            "canonical-live",
            ExecutionEvent::Text {
                text: "CANONICAL LIVE\n\n".to_owned(),
            },
        );
        h.content(
            0,
            &source,
            "source-live",
            ExecutionEvent::Text {
                text: "SOURCE LIVE\n\n".to_owned(),
            },
        );
        h.child_decision(0, &source, "alias-still-waiting");
        h.cockpit.pump();
        let handle = h.cockpit.thread(thread).unwrap().activity().decisions()[0]
            .handle
            .clone();
        if select_before_alias {
            assert!(h
                .cockpit
                .ensure_subject_history(thread, &source_subject)
                .unwrap());
        }
        h.control.activity(
            0,
            ActivityEvent::Alias {
                from: source.clone(),
                to: canonical.clone(),
            },
        );
        h.content(
            0,
            &canonical,
            "after-alias",
            ExecutionEvent::Text {
                text: "TAIL AFTER ALIAS\n\n".to_owned(),
            },
        );
        pump_until(&mut h, |cockpit| {
            let view = cockpit.thread(thread).unwrap().activity();
            let child = view.subject(&canonical_subject).unwrap();
            child.retained()
                && text(child.transcript()).contains("FROZEN PREFIX 0")
                && text(child.transcript()).contains("FROZEN PREFIX 1\n")
                && text(child.transcript()).contains("TAIL AFTER ALIAS")
        });
        let view = h.cockpit.thread(thread).unwrap().activity();
        let child = view.subject(&canonical_subject).unwrap();
        assert_eq!(
            text(child.transcript()),
            "FROZEN PREFIX 0\nFROZEN PREFIX 1\nCANONICAL LIVE\nSOURCE LIVE\nTAIL AFTER ALIAS",
            "selection before alias: {select_before_alias}"
        );
        assert_eq!(view.canonical_subject(&source_subject), canonical_subject);
        assert_eq!(view.children().len(), 128);
        assert!(child.busy());
        assert!(child.fresh());
        assert_eq!(child.status(), AgentStatus::Waiting);
        assert_eq!(view.decisions().len(), 1);
        assert_eq!(view.decisions()[0].handle, handle);
        assert_eq!(view.decisions()[0].subject, Some(canonical_subject));
        assert!(h.control.sent(0).is_empty());
        assert!(h.control.replies(0).is_empty());
        assert!(h
            .cockpit
            .respond_decision(thread, &handle, allow())
            .unwrap());
        assert_eq!(
            h.control.replies(0),
            [Reply {
                id: "alias-still-waiting".to_owned(),
                answer: allow(),
                succeeded: true,
            }]
        );
    }
}

#[test]
fn child_history_reload_cannot_restore_old_labels_or_reattach_a_detached_child() {
    let mut h = Harness::new();
    let key = evicted_oldest(&mut h);
    let subject = Subject::Subagent(key.clone());
    let thread = h.thread;
    assert!(h.cockpit.ensure_subject_history(thread, &subject).unwrap());
    let mut latest = AgentInfo::new(key.clone());
    latest.parent = Some(Subject::Main);
    latest.name = Some("Updated provider name".to_owned());
    latest.description = Some("Current task description".to_owned());
    latest.coverage = TranscriptCoverage::Live;
    h.control.activity(0, ActivityEvent::Discovered(latest));
    h.cockpit.pump();
    pump_until(&mut h, |cockpit| {
        let child = cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&subject)
            .unwrap();
        child.retained() && text(child.transcript()).contains("FROZEN PREFIX 0")
    });
    let children = h.cockpit.thread(thread).unwrap().activity().children();
    let child = children.iter().find(|child| child.key() == &key).unwrap();
    assert_eq!(child.info().name.as_deref(), Some("Updated provider name"));
    assert_eq!(
        child.info().description.as_deref(),
        Some("Current task description")
    );

    // Viewing the next evicted child makes this idle child evicted again.
    let next = Subject::Subagent(AgentKey::new(Provider::Codex, "main-0", "cache-child-1"));
    assert!(h.cockpit.ensure_subject_history(thread, &next).unwrap());
    pump_until(&mut h, |cockpit| {
        cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&next)
            .unwrap()
            .retained()
    });
    assert!(!h
        .cockpit
        .thread(thread)
        .unwrap()
        .activity()
        .subject(&subject)
        .unwrap()
        .retained());
    assert!(h.cockpit.ensure_subject_history(thread, &subject).unwrap());
    h.control
        .activity(0, ActivityEvent::Detached { key: key.clone() });
    h.cockpit.pump();
    pump_until(&mut h, |cockpit| {
        let child = cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&subject)
            .unwrap();
        child.retained() && text(child.transcript()).contains("FROZEN PREFIX 0")
    });
    let view = h.cockpit.thread(thread).unwrap().activity();
    assert!(view.children().iter().all(|child| child.key() != &key));
    let child = view.subject(&subject).unwrap();
    assert!(!child.busy());
    assert!(!child.fresh());
    assert_eq!(child.coverage(), TranscriptCoverage::Unavailable);
    assert_eq!(text(child.transcript()), "FROZEN PREFIX 0");
}
