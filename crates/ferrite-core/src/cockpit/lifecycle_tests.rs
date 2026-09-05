//! Delayed provider adapters cross the same startup seam as production.
//! Gates decide readiness; no CLI, timing assumptions, or worker-side sends.
use super::*;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};

struct Scripted {
    events: Receiver<SessionEvent>,
    sent: Arc<Mutex<Vec<String>>>,
    fail_send: Arc<AtomicBool>,
    dropped: Arc<AtomicUsize>,
}
impl Drop for Scripted {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}
impl Session for Scripted {
    fn events(&self) -> &Receiver<SessionEvent> {
        &self.events
    }
    fn send(&mut self, text: &str) -> io::Result<()> {
        if self.fail_send.load(Ordering::SeqCst) {
            return Err(io::Error::other("first send refused"));
        }
        self.sent.lock().unwrap().push(text.into());
        Ok(())
    }
    fn interrupt(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn respond_to_decision(&mut self, _: &str, _: DecisionAnswer) -> io::Result<()> {
        Ok(())
    }
}
struct Control {
    gate: Option<mpsc::Sender<io::Result<()>>>,
    events: mpsc::Sender<SessionEvent>,
    sent: Arc<Mutex<Vec<String>>>,
    fail_send: Arc<AtomicBool>,
    dropped: Arc<AtomicUsize>,
}
impl Control {
    fn ready(&mut self) {
        self.gate.take().unwrap().send(Ok(())).unwrap();
    }
    fn fail(&mut self) {
        self.gate
            .take()
            .unwrap()
            .send(Err(io::Error::other("CLI could not start")))
            .unwrap();
    }
    fn sent(&self) -> Vec<String> {
        self.sent.lock().unwrap().clone()
    }
    fn ended(&self) {
        self.events
            .send(SessionEvent::TurnEnded {
                outcome: crate::TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
    }
}
enum Plan {
    Ready(Box<dyn Session + Send>),
    Delayed(Box<dyn Session + Send>, Receiver<io::Result<()>>),
}
fn planned(delayed: bool) -> (Plan, Control) {
    let (tx, rx) = mpsc::channel();
    let sent = Arc::new(Mutex::new(Vec::new()));
    let dropped = Arc::new(AtomicUsize::new(0));
    let fail_send = Arc::new(AtomicBool::new(false));
    let session = Box::new(Scripted {
        events: rx,
        sent: sent.clone(),
        dropped: dropped.clone(),
        fail_send: fail_send.clone(),
    });
    let (gate, plan) = if delayed {
        let (tx, rx) = mpsc::channel();
        (
            Some(tx),
            Plan::Delayed(session as Box<dyn Session + Send>, rx),
        )
    } else {
        (None, Plan::Ready(session as Box<dyn Session + Send>))
    };
    (
        plan,
        Control {
            gate,
            events: tx,
            sent,
            dropped,
            fail_send,
        },
    )
}
struct ControlledSpawner(VecDeque<Plan>);
impl Spawner for ControlledSpawner {
    fn spawn(&mut self, _: SpawnRequest) -> io::Result<Box<dyn Session>> {
        panic!("Cockpit must always use the startup seam")
    }
    fn start(&mut self, _: SpawnRequest) -> io::Result<SessionLifecycle> {
        match self.0.pop_front().expect("unexpected extra startup") {
            Plan::Ready(session) => Ok(SessionLifecycle::ready(session)),
            Plan::Delayed(session, gate) => SessionLifecycle::background(move || {
                gate.recv().unwrap()?;
                Ok(session)
            }),
        }
    }
}
fn cockpit(name: &str, plans: Vec<Plan>) -> Cockpit {
    let dir = std::env::temp_dir().join(format!("ferrite-lifecycle-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    Cockpit::new(
        Store::open(dir).unwrap(),
        Box::new(ControlledSpawner(plans.into())),
    )
}
fn choice(provider: Provider) -> ProviderChoice {
    ProviderChoice {
        provider,
        model: None,
    }
}
fn workspace() -> WorkspaceChoice {
    WorkspaceChoice::Main {
        checkout: std::env::current_dir().unwrap(),
    }
}
fn until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(Instant::now() < deadline, "startup did not settle");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn settle(cockpit: &mut Cockpit) -> BootstrapResult {
    let mut result = None;
    until(|| {
        cockpit.pump();
        result = cockpit.take_bootstrap_results().pop();
        result.is_some()
    });
    result.unwrap()
}

#[test]
fn delayed_draft_success_delivers_once_and_preserves_unrelated_roster_actions() {
    let (plan, mut provider) = planned(true);
    let mut cockpit = cockpit("draft-success", vec![plan]);
    let draft = cockpit.open_draft();
    assert!(cockpit
        .bootstrap_draft(draft, choice(Provider::Claude), workspace(), "first", None)
        .unwrap()
        .is_none());
    let provisional = cockpit.threads()[0];
    assert!(!cockpit.thread(provisional).unwrap().first_prompt_sent());
    assert!(!cockpit.thread(provisional).unwrap().has_prompt_history());
    assert!(cockpit
        .store
        .load(provisional)
        .unwrap()
        .prompt_texts()
        .is_empty());
    assert!(provider.sent().is_empty());
    assert_eq!(cockpit.roster().panes(), &[PaneIdentity::Draft(draft)]);
    // Duplicate Enter does not allocate a second Thread or Session.
    assert!(cockpit
        .bootstrap_draft(
            draft,
            choice(Provider::Claude),
            workspace(),
            "duplicate",
            None
        )
        .unwrap()
        .is_none());
    let other = cockpit.open_draft();
    cockpit.focus(PaneIdentity::Draft(other));
    provider.ready();
    let completed = settle(&mut cockpit);
    assert_eq!(completed.draft, Some(draft));
    assert_eq!(completed.prompt, "first");
    assert_eq!(completed.result.unwrap().thread, provisional);
    assert_eq!(provider.sent(), ["first"]);
    assert!(cockpit.thread(provisional).unwrap().first_prompt_sent());
    assert!(cockpit.thread(provisional).unwrap().has_prompt_history());
    assert_eq!(cockpit.roster().focused(), Some(PaneIdentity::Draft(other)));
    assert!(cockpit
        .roster()
        .panes()
        .contains(&PaneIdentity::Thread(provisional)));
}

#[test]
fn delayed_draft_startup_and_first_send_failures_restore_the_original_draft() {
    for fail_start in [true, false] {
        let (plan, mut provider) = planned(true);
        let mut cockpit = cockpit(
            if fail_start {
                "spawn-failure"
            } else {
                "send-failure"
            },
            vec![plan],
        );
        let draft = cockpit.open_draft();
        cockpit
            .bootstrap_draft(
                draft,
                choice(Provider::Claude),
                workspace(),
                "keep my prompt",
                None,
            )
            .unwrap();
        if fail_start {
            provider.fail();
        } else {
            provider.fail_send.store(true, Ordering::SeqCst);
            provider.ready();
        }
        let completed = settle(&mut cockpit);
        assert_eq!(completed.draft, Some(draft));
        assert_eq!(completed.prompt, "keep my prompt");
        assert!(completed.result.is_err());
        assert!(cockpit.threads().is_empty());
        assert!(cockpit.store.thread_ids().unwrap().is_empty());
        assert_eq!(cockpit.roster().panes(), &[PaneIdentity::Draft(draft)]);
        assert!(provider.sent().is_empty());
    }
}

#[test]
fn discard_and_park_before_ready_cannot_deliver_even_without_another_pump() {
    for discard in [true, false] {
        let (plan, mut provider) = planned(true);
        let mut cockpit = cockpit(if discard { "discard" } else { "park" }, vec![plan]);
        let draft = cockpit.open_draft();
        cockpit
            .bootstrap_draft(
                draft,
                choice(Provider::Claude),
                workspace(),
                "must never run",
                None,
            )
            .unwrap();
        if discard {
            cockpit.discard_draft(draft).unwrap();
        } else {
            cockpit.park(cockpit.threads()[0]).unwrap();
        }
        provider.ready();
        until(|| provider.dropped.load(Ordering::SeqCst) == 1);
        assert!(provider.sent().is_empty());
        assert!(cockpit.store.thread_ids().unwrap().is_empty());
    }
}

#[test]
fn delayed_replacement_failure_preserves_header_and_the_original_session() {
    let (old, original) = planned(false);
    let (new, mut replacement) = planned(true);
    let mut cockpit = cockpit("replace-fail", vec![old, new]);
    let thread = cockpit.open(Provider::Claude, workspace()).unwrap();
    cockpit
        .set_provider(thread, choice(Provider::Codex))
        .unwrap();
    assert!(cockpit.thread(thread).unwrap().starting());
    assert_eq!(cockpit.peek(thread).unwrap().provider, Provider::Claude);
    assert_eq!(original.dropped.load(Ordering::SeqCst), 0);
    replacement.fail();
    until(|| {
        cockpit.pump();
        !cockpit.thread(thread).unwrap().starting()
    });
    assert_eq!(cockpit.peek(thread).unwrap().provider, Provider::Claude);
    assert_eq!(original.dropped.load(Ordering::SeqCst), 0);
    cockpit.send(thread, "old still serves".into());
    assert_eq!(original.sent(), ["old still serves"]);
}

#[test]
fn handover_commit_and_delivery_wait_for_readiness_and_failed_send_keeps_carry() {
    let (old, original) = planned(false);
    let (new, mut replacement) = planned(true);
    let mut cockpit = cockpit("handover", vec![old, new]);
    let thread = cockpit.open(Provider::Claude, workspace()).unwrap();
    cockpit.send(thread, "before".into());
    original.ended();
    cockpit.pump();
    cockpit
        .set_provider(thread, choice(Provider::Codex))
        .unwrap();
    cockpit.send(thread, "after".into());
    assert!(cockpit
        .store
        .load(thread)
        .unwrap()
        .last_handover()
        .is_none());
    assert_eq!(cockpit.peek(thread).unwrap().provider, Provider::Claude);
    assert_eq!(original.sent(), ["before"]);
    replacement.fail_send.store(true, Ordering::SeqCst);
    replacement.ready();
    until(|| {
        cockpit.pump();
        !cockpit.thread(thread).unwrap().starting()
    });
    assert_eq!(cockpit.peek(thread).unwrap().provider, Provider::Codex);
    assert!(
        !cockpit
            .store
            .load(thread)
            .unwrap()
            .last_handover()
            .unwrap()
            .delivered
    );
    assert_eq!(cockpit.thread(thread).unwrap().queued(), Some("after"));
    assert!(replacement.sent().is_empty());
    replacement.fail_send.store(false, Ordering::SeqCst);
    let prompt = cockpit.unqueue(thread).unwrap();
    cockpit.send(thread, prompt);
    let sent = replacement.sent();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].contains("before") && sent[0].ends_with("after"));
    cockpit.park(thread).unwrap();
    assert!(
        cockpit
            .store
            .load(thread)
            .unwrap()
            .last_handover()
            .unwrap()
            .delivered
    );
}

#[test]
fn failed_handover_rewrite_leaves_no_durable_handover_and_keeps_serving() {
    let (old, original) = planned(false);
    let (new, mut replacement) = planned(true);
    let mut cockpit = cockpit("handover-store-fail", vec![old, new]);
    let thread = cockpit.open(Provider::Claude, workspace()).unwrap();
    cockpit.send(thread, "before".into());
    original.ended();
    cockpit.pump();
    cockpit
        .set_provider(thread, choice(Provider::Codex))
        .unwrap();
    let tmp = cockpit
        .store
        .dir()
        .join(thread.to_string())
        .join("log.jsonl.tmp");
    std::fs::create_dir(&tmp).unwrap();
    replacement.ready();
    until(|| {
        cockpit.pump();
        !cockpit.thread(thread).unwrap().starting()
    });
    let snapshot = cockpit.store.load(thread).unwrap();
    assert_eq!(snapshot.provider(), Provider::Claude);
    assert!(snapshot.last_handover().is_none());
    assert_eq!(original.dropped.load(Ordering::SeqCst), 0);
    cockpit.send(thread, "still old".into());
    assert_eq!(original.sent(), ["before", "still old"]);
}

#[test]
fn grouped_draft_revalidates_membership_before_delivering() {
    let (first, _) = planned(false);
    let (second, _) = planned(false);
    let (new, mut replacement) = planned(true);
    let mut cockpit = cockpit("group-changed", vec![first, second, new]);
    let first = cockpit.open(Provider::Claude, workspace()).unwrap();
    let second = cockpit.open(Provider::Claude, workspace()).unwrap();
    let group = cockpit
        .apply_group(GroupChange::Create { first, second })
        .unwrap()
        .group
        .unwrap();
    cockpit.enter_group(group).unwrap();
    let draft = cockpit.open_draft();
    cockpit
        .bootstrap_draft(draft, choice(Provider::Claude), workspace(), "join", None)
        .unwrap();
    // Group can change through another operator action during startup.
    cockpit
        .apply_group(GroupChange::Leave { thread: first })
        .unwrap();
    replacement.ready();
    let completed = settle(&mut cockpit);
    assert!(completed.result.is_err());
    assert!(replacement.sent().is_empty());
    assert!(cockpit.roster().draft_scope(draft).is_some());
    assert_eq!(cockpit.threads().len(), 2);
}
