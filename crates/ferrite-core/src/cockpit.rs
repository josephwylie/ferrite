//! The Cockpit's headless state: what the operator is on the hook for, per
//! Thread.
//!
//! The pump's beginnings: Threads, their pending Decisions, and prompts held
//! back while a turn runs. No process and no window — a Thread's events are
//! fed in, and what the operator must answer comes out.

use std::collections::BTreeMap;
use std::io;
use std::sync::mpsc::Receiver;

use crate::providers::Session;
use crate::store::{LoadError, Provider, Store, ThreadWriter};
use crate::transcript::{BlockId, Input, Lexer, Transcript};
use crate::{Decision, DecisionAnswer, SessionEvent, ThreadId};

/// How a Session is started. Injected so the cockpit can be driven with
/// scripted Sessions in tests — nothing below this line spawns a process.
pub trait Spawner {
    /// `resume` is the provider-native id of a Thread being revived, which the
    /// provider reloads its own history from.
    fn spawn(&mut self, provider: Provider, resume: Option<&str>) -> io::Result<Box<dyn Session>>;
}

/// Resident memory per Session, injected. The cockpit never shells out for
/// this itself: a test drives the watchdog by saying what the number is.
pub trait RssSampler {
    /// Bytes resident for the Session serving `thread`, or None when it
    /// cannot be measured. `pid` is the Session's process where it has one.
    fn sample(&mut self, thread: ThreadId, pid: Option<u32>) -> Option<u64>;
}

/// A Session the watchdog replaced, for the operator and the app's log.
#[derive(Debug, Clone, PartialEq)]
pub struct Restart {
    pub thread: ThreadId,
    /// What it had grown to when the watchdog acted.
    pub rss: u64,
}

/// What one frame changed for one Pane. Threads that produced nothing are
/// absent: a quiet Pane costs the renderer nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneUpdate {
    pub thread: ThreadId,
    /// Exactly the Blocks whose content moved.
    pub dirty: Vec<BlockId>,
    /// Blocks that fell off the far end and no longer exist.
    pub evicted: Vec<BlockId>,
}

/// What one fold left for the caller to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Wake {
    Nothing,
    /// The turn ended and a prompt was waiting behind it — send this now.
    Send(String),
}

struct Thread {
    transcript: Transcript,
    /// Highlighting answers for this Thread's own Blocks. Per Thread because
    /// BlockIds are per Transcript: one shared channel could not say which
    /// Pane an answer belonged to.
    highlights: Receiver<Input>,
    /// The live Session, or None for a parked Thread.
    session: Option<Box<dyn Session>>,
    writer: ThreadWriter,
    provider: Provider,
    pending: Option<Decision>,
    /// A prompt the operator wrote while the turn was still running.
    queued: Option<String>,
    busy: bool,
    /// The provider-native id a replacement Session resumes from — the latest
    /// the provider announced. Held here rather than read back from the log,
    /// which has not necessarily flushed when the watchdog acts.
    resume: Option<String>,
}

impl Thread {
    /// A Thread ready to serve: its own Lexer, a fresh Transcript, a live
    /// Session. Revival replays history into the Transcript afterwards.
    fn fresh(
        session: Box<dyn Session>,
        writer: ThreadWriter,
        provider: Provider,
        resume: Option<String>,
    ) -> Self {
        let (lexer, highlights) = Lexer::new();
        Self {
            transcript: Transcript::new(std::sync::Arc::new(lexer)),
            highlights,
            session: Some(session),
            writer,
            provider,
            pending: None,
            queued: None,
            busy: false,
            resume,
        }
    }
}

pub struct Cockpit {
    threads: BTreeMap<ThreadId, Thread>,
    store: Store,
    spawner: Box<dyn Spawner>,
    sampler: Option<Box<dyn RssSampler>>,
    /// Bytes one Session may hold before the watchdog replaces it.
    limit: u64,
}

impl Cockpit {
    pub fn new(store: Store, spawner: Box<dyn Spawner>) -> Self {
        Self {
            threads: BTreeMap::new(),
            store,
            spawner,
            sampler: None,
            limit: u64::MAX,
        }
    }

    /// Watch Session memory. Off until asked for: the operator's budget is
    /// theirs to set, and a cockpit with no sampler never restarts anything.
    pub fn watch_memory(&mut self, sampler: Box<dyn RssSampler>, limit: u64) {
        self.sampler = Some(sampler);
        self.limit = limit;
    }

    /// Replace any Session that has grown past the limit. The Thread keeps its
    /// transcript and its log; only the process is new, and the Pane says so.
    pub fn sweep(&mut self) -> Vec<Restart> {
        let Some(sampler) = &mut self.sampler else {
            return Vec::new();
        };
        let over: Vec<(ThreadId, u64)> = self
            .threads
            .iter()
            .filter_map(|(id, thread)| {
                let session = thread.session.as_ref()?;
                Some((*id, sampler.sample(*id, session.pid())?))
            })
            .filter(|(_, rss)| *rss > self.limit)
            .collect();

        let mut restarts = Vec::new();
        for (id, rss) in over {
            let Some(thread) = self.threads.get_mut(&id) else {
                continue;
            };
            let resume = thread.resume.clone();
            // Drop the old Session before asking for a new one: the leaking
            // process must not outlive its replacement.
            thread.session = None;
            let spawned = self.spawner.spawn(thread.provider, resume.as_deref());
            let note = match spawned {
                Ok(session) => {
                    thread.session = Some(session);
                    format!("restarted — the Session had grown to {}", megabytes(rss))
                }
                Err(e) => format!("restart failed after {}: {e}", megabytes(rss)),
            };
            thread.transcript.apply(Input::Notice(note));
            restarts.push(Restart { thread: id, rss });
        }
        restarts
    }

    /// Start a Thread: a durable log, and a Session serving it.
    pub fn open(&mut self, provider: Provider) -> io::Result<ThreadId> {
        let (id, writer) = self.store.create(provider)?;
        let session = self.spawner.spawn(provider, None)?;
        self.threads
            .insert(id, Thread::fresh(session, writer, provider, None));
        Ok(id)
    }

    /// Close a Pane: the Session ends, the log is flushed, and the Thread
    /// keeps nothing in memory until it is opened again.
    pub fn park(&mut self, thread: ThreadId) -> io::Result<()> {
        let Some(mut state) = self.threads.remove(&thread) else {
            return Ok(());
        };
        state.session = None;
        state.writer.flush()
    }

    /// Reopen a parked Thread: its history is replayed from the log into a
    /// fresh Transcript, and the new Session is told where to resume.
    pub fn revive(&mut self, thread: ThreadId) -> Result<(), LoadError> {
        let snapshot = self.store.load(thread)?;
        let provider = snapshot.provider();
        let session = self
            .spawner
            .spawn(provider, snapshot.resume_target())
            .map_err(LoadError::Io)?;
        let writer = self.store.writer(thread)?;

        let resume = snapshot.resume_target().map(|target| target.to_string());
        let mut state = Thread::fresh(session, writer, provider, resume);
        for input in snapshot.inputs() {
            state.transcript.apply(input);
        }
        state.transcript.apply(Input::Revived);

        self.threads.insert(thread, state);
        Ok(())
    }

    /// Send a prompt now: on the wire, in the transcript, in the log.
    pub fn send(&mut self, thread: ThreadId, text: String) {
        let Some(state) = self.threads.get_mut(&thread) else {
            return;
        };
        match &mut state.session {
            Some(session) => {
                if let Err(e) = session.send(&text) {
                    state
                        .transcript
                        .apply(Input::Notice(format!("send failed: {e}")));
                    return;
                }
                let _ = state.writer.record_prompt(&text);
                state.transcript.apply(Input::Prompt(text));
            }
            None => {
                state.transcript.apply(Input::Prompt(text));
                state
                    .transcript
                    .apply(Input::Notice("no session — this Thread is parked".into()));
            }
        }
    }

    /// Stop the running turn.
    pub fn interrupt(&mut self, thread: ThreadId) {
        let Some(state) = self.threads.get_mut(&thread) else {
            return;
        };
        if let Some(session) = &mut state.session {
            if let Err(e) = session.interrupt() {
                state
                    .transcript
                    .apply(Input::Notice(format!("interrupt failed: {e}")));
            }
        }
    }

    /// Answer a Decision, if it is still the one this Thread waits on.
    pub fn respond(&mut self, thread: ThreadId, decision: &Decision, answer: DecisionAnswer) {
        if !self.answer(thread, &decision.id) {
            return;
        }
        let allowed = !matches!(answer, DecisionAnswer::Deny { .. });
        let Some(state) = self.threads.get_mut(&thread) else {
            return;
        };
        if let Some(session) = &mut state.session {
            if let Err(e) = session.respond_to_decision(&decision.id, answer) {
                state
                    .transcript
                    .apply(Input::Notice(format!("answer failed: {e}")));
            }
        }
        state.transcript.apply(Input::Answered {
            allowed,
            tool_name: decision.tool_name.clone(),
        });
    }

    /// Something Ferrite itself needs to say in a Pane.
    pub fn apply_input(&mut self, thread: ThreadId, input: Input) {
        if let Some(state) = self.threads.get_mut(&thread) {
            state.transcript.apply(input);
        }
    }

    pub fn transcript(&self, thread: ThreadId) -> Option<&Transcript> {
        Some(&self.threads.get(&thread)?.transcript)
    }

    /// Threads the store holds that no Pane is showing — what a restart finds,
    /// and what "reopen" reopens.
    pub fn parked(&self) -> io::Result<Vec<ThreadId>> {
        Ok(self
            .store
            .thread_ids()?
            .into_iter()
            .filter(|id| !self.threads.contains_key(id))
            .collect())
    }

    /// Which backend serves this Thread.
    pub fn provider(&self, thread: ThreadId) -> Option<Provider> {
        Some(self.threads.get(&thread)?.provider)
    }

    pub fn threads(&self) -> Vec<ThreadId> {
        self.threads.keys().copied().collect()
    }

    /// One frame: drain every live Session, fold what arrived, and write it
    /// down. What comes back is only the Panes that actually changed.
    pub fn pump(&mut self) -> Vec<PaneUpdate> {
        let mut frame = Vec::new();
        for (id, thread) in &mut self.threads {
            let Some(session) = &thread.session else {
                continue;
            };
            let events: Vec<SessionEvent> = session.events().try_iter().collect();
            let answers: Vec<Input> = thread.highlights.try_iter().collect();
            if events.is_empty() && answers.is_empty() {
                continue;
            }
            let mut update = PaneUpdate {
                thread: *id,
                dirty: Vec::new(),
                evicted: Vec::new(),
            };
            let mut release = None;
            for event in events {
                let _ = thread.writer.record_event(&event);
                if let Wake::Send(held) = fold(thread, &event) {
                    release = Some(held);
                }
                let applied = thread.transcript.apply(Input::Event(event));
                update.dirty.extend(applied.dirty);
                update.evicted.extend(applied.evicted);
            }
            for answer in answers {
                update.dirty.extend(thread.transcript.apply(answer).dirty);
            }
            if let Some(held) = release {
                if let Some(session) = &mut thread.session {
                    let _ = session.send(&held);
                    let _ = thread.writer.record_prompt(&held);
                    let applied = thread.transcript.apply(Input::Prompt(held));
                    update.dirty.extend(applied.dirty);
                }
            }
            frame.push(update);
        }
        frame
    }

    /// Is a turn running? A prompt written now has to wait for it.
    pub fn busy(&self, thread: ThreadId) -> bool {
        self.threads.get(&thread).is_some_and(|state| state.busy)
    }

    /// Hold a prompt written mid-turn. It stays visible and editable until the
    /// turn ends, which is when it is sent.
    pub fn queue(&mut self, thread: ThreadId, text: String) {
        if let Some(state) = self.threads.get_mut(&thread) {
            state.queued = Some(text);
        }
    }

    /// Take a held prompt back into the Composer, so a typo written mid-turn
    /// is fixable before it goes out.
    pub fn unqueue(&mut self, thread: ThreadId) -> Option<String> {
        self.threads.get_mut(&thread)?.queued.take()
    }

    pub fn queued(&self, thread: ThreadId) -> Option<&str> {
        self.threads.get(&thread)?.queued.as_deref()
    }

    /// Take the pending Decision, if `id` is really what this Thread is
    /// waiting on. False means the answer is stale — the request was already
    /// answered, or the turn ended under it — and must not reach the provider,
    /// which would either ignore it or, worse, apply it to the next request.
    pub fn answer(&mut self, thread: ThreadId, id: &str) -> bool {
        let Some(state) = self.threads.get_mut(&thread) else {
            return false;
        };
        if state
            .pending
            .as_ref()
            .is_none_or(|pending| pending.id != id)
        {
            return false;
        }
        state.pending = None;
        true
    }

    /// What this Thread is blocked on, if anything.
    pub fn pending(&self, thread: ThreadId) -> Option<&Decision> {
        self.threads.get(&thread)?.pending.as_ref()
    }

    /// The next Thread waiting on the operator, after `from`, wrapping. One
    /// key held down walks every Decision in the cockpit and stops nowhere
    /// else.
    pub fn next_blocked(&self, from: Option<ThreadId>) -> Option<ThreadId> {
        let blocked = self.blocked();
        match from {
            Some(after) => blocked
                .iter()
                .find(|id| **id > after)
                .copied()
                .or_else(|| blocked.first().copied()),
            None => blocked.first().copied(),
        }
    }

    /// Threads waiting on the operator — what the wall badges.
    pub fn blocked(&self) -> Vec<ThreadId> {
        self.threads
            .iter()
            .filter(|(_, state)| state.pending.is_some())
            .map(|(id, _)| *id)
            .collect()
    }
}

fn megabytes(bytes: u64) -> String {
    format!("{} MB", bytes / (1024 * 1024))
}

/// The bookkeeping half of a fold: what the operator is on the hook for.
fn fold(state: &mut Thread, event: &SessionEvent) -> Wake {
    let mut wake = Wake::Nothing;
    match event {
        SessionEvent::TextDelta { .. }
        | SessionEvent::ThinkingDelta { .. }
        | SessionEvent::ReasoningSummaryDelta { .. }
        | SessionEvent::ToolStarted { .. } => state.busy = true,
        // A turn that ends takes its Decision with it: the provider is no
        // longer waiting, so an answer would go nowhere. Anything the operator
        // wrote behind the turn goes out now.
        SessionEvent::TurnEnded { .. } | SessionEvent::Closed { .. } => {
            state.pending = None;
            state.busy = false;
            if let Some(held) = state.queued.take() {
                wake = Wake::Send(held);
            }
        }
        SessionEvent::DecisionRequested { decision } => {
            state.pending = Some(decision.clone());
        }
        // Kept for the watchdog: a replacement Session resumes from the newest
        // id the provider gave, even one it renamed mid-Thread.
        SessionEvent::Init { session_id, .. } => {
            state.resume = Some(session_id.clone());
        }
        _ => {}
    }
    wake
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::mpsc::{self, Sender};

    use crate::store::{Provider, Store};
    use crate::transcript::{Body, Class};

    /// A fresh per-test scratch directory.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ferrite-cockpit-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A Session with no process behind it: the test pushes the events a
    /// provider would have streamed, and reads back what Ferrite sent.
    struct Scripted {
        rx: Receiver<SessionEvent>,
        sent: Rc<RefCell<Vec<String>>>,
    }

    impl crate::providers::Session for Scripted {
        fn events(&self) -> &Receiver<SessionEvent> {
            &self.rx
        }

        fn send(&mut self, text: &str) -> std::io::Result<()> {
            self.sent.borrow_mut().push(text.to_string());
            Ok(())
        }

        fn interrupt(&mut self) -> std::io::Result<()> {
            Ok(())
        }

        fn respond_to_decision(
            &mut self,
            _id: &str,
            _answer: crate::DecisionAnswer,
        ) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Hands out Scripted Sessions and keeps the handles the test drives them
    /// with. Nothing here spawns a process.
    #[derive(Clone, Default)]
    struct Fake {
        streams: Rc<RefCell<Vec<Sender<SessionEvent>>>>,
        sent: Rc<RefCell<Vec<String>>>,
        resumed: Rc<RefCell<Vec<Option<String>>>>,
    }

    impl Spawner for Fake {
        fn spawn(
            &mut self,
            _provider: Provider,
            resume: Option<&str>,
        ) -> std::io::Result<Box<dyn crate::providers::Session>> {
            let (tx, rx) = mpsc::channel();
            self.streams.borrow_mut().push(tx);
            self.resumed
                .borrow_mut()
                .push(resume.map(|target| target.to_string()));
            Ok(Box::new(Scripted {
                rx,
                sent: self.sent.clone(),
            }))
        }
    }

    fn cockpit(name: &str) -> (Cockpit, Fake) {
        let fake = Fake::default();
        let spawner = Fake {
            streams: fake.streams.clone(),
            sent: fake.sent.clone(),
            resumed: fake.resumed.clone(),
        };
        let store = Store::open(scratch(name)).unwrap();
        (Cockpit::new(store, Box::new(spawner)), fake)
    }

    fn text(s: &str) -> SessionEvent {
        SessionEvent::TextDelta { text: s.into() }
    }

    #[test]
    fn each_thread_folds_only_its_own_events() {
        let (mut cockpit, fake) = cockpit("own-events");
        let one = cockpit.open(Provider::Claude).unwrap();
        let two = cockpit.open(Provider::Claude).unwrap();

        fake.streams.borrow()[0].send(text("only for one")).unwrap();

        let updates = cockpit.pump();

        // Coalesced: a Thread that produced nothing is not in the frame.
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].thread, one);
        assert_eq!(updates[0].dirty.len(), 1);
        assert_eq!(cockpit.transcript(one).unwrap().blocks().len(), 1);
        assert!(cockpit.transcript(two).unwrap().blocks().is_empty());
    }

    fn decision(id: &str, tool: &str) -> SessionEvent {
        SessionEvent::DecisionRequested {
            decision: Decision {
                id: id.into(),
                tool_use_id: format!("toolu_{id}"),
                tool_name: tool.into(),
                description: "ferrite-perm.txt".into(),
                input: serde_json::json!({ "file_path": "ferrite-perm.txt" }),
                suggestions: vec![],
            },
        }
    }

    fn ended() -> SessionEvent {
        SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        }
    }

    /// Reports whatever the test tells it to.
    struct Meter(Rc<RefCell<u64>>);

    impl RssSampler for Meter {
        fn sample(&mut self, _thread: ThreadId, _pid: Option<u32>) -> Option<u64> {
            Some(*self.0.borrow())
        }
    }

    #[test]
    fn a_sent_prompt_reaches_the_provider_the_transcript_and_the_log() {
        let (mut cockpit, fake) = cockpit("send");
        let thread = cockpit.open(Provider::Claude).unwrap();

        cockpit.send(thread, "run the tests".into());

        assert_eq!(fake.sent.borrow().as_slice(), ["run the tests"]);
        let echoed = cockpit.transcript(thread).unwrap().blocks().last().cloned();
        assert!(
            matches!(echoed.map(|b| b.body), Some(Body::Prompt(line)) if line == "run the tests")
        );
        // And durably: a restart must not lose what the operator asked for.
        cockpit.park(thread).unwrap();
        cockpit.revive(thread).unwrap();
        assert!(cockpit
            .transcript(thread)
            .unwrap()
            .blocks()
            .iter()
            .any(|block| matches!(&block.body, Body::Prompt(line) if line == "run the tests")));
    }

    #[test]
    fn answering_through_the_cockpit_clears_the_card_and_records_who_did_it() {
        let (mut cockpit, fake) = cockpit("respond");
        let thread = cockpit.open(Provider::Claude).unwrap();
        fake.streams.borrow()[0]
            .send(decision("perm_01", "Write"))
            .unwrap();
        cockpit.pump();
        let pending = cockpit.pending(thread).cloned().unwrap();

        cockpit.respond(
            thread,
            &pending,
            crate::DecisionAnswer::Allow {
                input: pending.input.clone(),
            },
        );

        assert_eq!(cockpit.pending(thread), None);
        let last = cockpit.transcript(thread).unwrap().blocks().last().cloned();
        assert!(matches!(last.map(|b| b.body), Some(Body::Meta(line)) if line == "allowed Write"));

        // A second press answers nothing twice.
        cockpit.respond(
            thread,
            &pending,
            crate::DecisionAnswer::Allow {
                input: pending.input.clone(),
            },
        );
        let blocks = cockpit.transcript(thread).unwrap().blocks();
        assert_eq!(
            blocks
                .iter()
                .filter(|b| matches!(&b.body, Body::Meta(line) if line == "allowed Write"))
                .count(),
            1
        );
    }

    #[test]
    fn a_code_fence_comes_back_highlighted_without_the_app_routing_anything() {
        let (mut cockpit, fake) = cockpit("highlight");
        let thread = cockpit.open(Provider::Claude).unwrap();

        fake.streams.borrow()[0]
            .send(text("```rust\nfn main() {}\n```\n\n"))
            .unwrap();
        cockpit.pump();
        // The lexer answers on its own channel; the next frame picks it up.
        let update = cockpit.pump();

        let code = cockpit
            .transcript(thread)
            .unwrap()
            .blocks()
            .iter()
            .find(|block| matches!(block.body, Body::Code { .. }))
            .cloned()
            .expect("a code block");
        assert!(update.iter().any(|pane| pane.dirty.contains(&code.id)));
        let Body::Code { tokens, .. } = &code.body else {
            unreachable!()
        };
        assert!(
            tokens
                .as_deref()
                .is_some_and(|tokens| tokens.iter().any(|t| t.class == Class::Keyword)),
            "the cockpit owns highlighting: {tokens:?}"
        );
    }

    #[test]
    fn jumping_to_the_next_decision_wraps_around_the_cockpit() {
        let (mut cockpit, fake) = cockpit("next-decision");
        let one = cockpit.open(Provider::Claude).unwrap();
        let two = cockpit.open(Provider::Claude).unwrap();
        let three = cockpit.open(Provider::Claude).unwrap();
        assert_eq!(cockpit.next_blocked(None), None);

        fake.streams.borrow()[0]
            .send(decision("perm_01", "Write"))
            .unwrap();
        fake.streams.borrow()[2]
            .send(decision("perm_03", "Bash"))
            .unwrap();
        cockpit.pump();

        // From nowhere, the first blocked Thread; from one, the next one on.
        assert_eq!(cockpit.next_blocked(None), Some(one));
        assert_eq!(cockpit.next_blocked(Some(one)), Some(three));
        // Past the last, back to the first — the operator keeps pressing one
        // key until the cockpit is quiet.
        assert_eq!(cockpit.next_blocked(Some(three)), Some(one));
        // A Thread with nothing pending is still a valid place to jump from.
        assert_eq!(cockpit.next_blocked(Some(two)), Some(three));
    }

    #[test]
    fn a_leaking_session_is_restarted_where_the_operator_can_see_it() {
        let (mut cockpit, fake) = cockpit("watchdog");
        let rss = Rc::new(RefCell::new(200 * 1024 * 1024));
        cockpit.watch_memory(Box::new(Meter(rss.clone())), 1024 * 1024 * 1024);
        let thread = cockpit.open(Provider::Claude).unwrap();
        fake.streams.borrow()[0]
            .send(SessionEvent::Init {
                session_id: "sess-1".into(),
                model: "claude-haiku-4-5".into(),
            })
            .unwrap();
        fake.streams.borrow()[0].send(text("work so far")).unwrap();
        cockpit.pump();

        // Comfortably under: nothing happens.
        assert!(cockpit.sweep().is_empty());

        *rss.borrow_mut() = 4 * 1024 * 1024 * 1024;
        let restarted = cockpit.sweep();

        assert_eq!(restarted.len(), 1);
        assert_eq!(restarted[0].thread, thread);
        assert_eq!(restarted[0].rss, 4 * 1024 * 1024 * 1024);
        // A second Session, told where to pick up.
        assert_eq!(fake.resumed.borrow().len(), 2);
        assert_eq!(
            fake.resumed.borrow().last().unwrap().as_deref(),
            Some("sess-1")
        );

        let blocks = cockpit.transcript(thread).unwrap().blocks();
        let last = blocks.last().unwrap();
        assert!(
            matches!(&last.body, Body::Notice(line) if line.contains("restarted")),
            "the Pane must say it happened: {:?}",
            last.body
        );
        // The scrollback is not collateral damage.
        assert!(blocks.iter().any(|block| matches!(
            &block.body,
            Body::Paragraph { spans } if spans.iter().any(|s| s.text == "work so far")
        )));
    }

    #[test]
    fn a_restart_finds_the_threads_it_left_behind() {
        let dir = scratch("restart");
        let fake = Fake::default();
        let mut cockpit = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake.clone()));
        let first = cockpit.open(Provider::Claude).unwrap();
        let second = cockpit.open(Provider::Claude).unwrap();
        cockpit.park(first).unwrap();
        cockpit.park(second).unwrap();
        drop(cockpit);

        // A new run of Ferrite over the same store.
        let mut restarted = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake));

        assert_eq!(restarted.parked().unwrap(), vec![first, second]);
        restarted.revive(second).unwrap();
        assert_eq!(restarted.threads(), vec![second]);
        // What is open is no longer parked.
        assert_eq!(restarted.parked().unwrap(), vec![first]);
    }

    #[test]
    fn a_parked_thread_revives_with_its_history_and_says_so() {
        let (mut cockpit, fake) = cockpit("revive");
        let thread = cockpit.open(Provider::Claude).unwrap();
        fake.streams.borrow()[0]
            .send(SessionEvent::Init {
                session_id: "sess-1".into(),
                model: "claude-haiku-4-5".into(),
            })
            .unwrap();
        fake.streams.borrow()[0]
            .send(text("an answer from before"))
            .unwrap();
        fake.streams.borrow()[0].send(ended()).unwrap();
        cockpit.pump();

        cockpit.park(thread).unwrap();
        // A parked Thread costs memory nothing: no Session, no transcript.
        assert!(cockpit.transcript(thread).is_none());
        assert!(cockpit.threads().is_empty());

        cockpit.revive(thread).unwrap();

        let blocks = cockpit.transcript(thread).unwrap().blocks();
        let text_of = |body: &Body| match body {
            Body::Paragraph { spans } => spans.iter().map(|s| s.text.clone()).collect::<String>(),
            Body::Meta(line) => line.clone(),
            _ => String::new(),
        };
        assert!(
            blocks
                .iter()
                .any(|b| text_of(&b.body) == "an answer from before"),
            "history should come back: {blocks:?}"
        );
        // Honest about what it is: a new Session wearing an old Thread.
        assert_eq!(
            text_of(&blocks.last().unwrap().body),
            "revived — new Session, history from the log"
        );
        // And the provider is told where to pick up.
        assert_eq!(
            fake.resumed.borrow().last().unwrap().as_deref(),
            Some("sess-1")
        );
    }

    #[test]
    fn a_decision_is_pending_against_the_thread_that_raised_it() {
        let (mut cockpit, fake) = cockpit("pending");
        let one = cockpit.open(Provider::Claude).unwrap();
        let two = cockpit.open(Provider::Claude).unwrap();

        fake.streams.borrow()[0]
            .send(decision("perm_01", "Write"))
            .unwrap();
        cockpit.pump();

        assert_eq!(cockpit.pending(one).unwrap().tool_name, "Write");
        assert_eq!(cockpit.pending(two), None);
        assert_eq!(cockpit.blocked(), vec![one]);
    }

    #[test]
    fn answering_a_decision_that_is_no_longer_pending_is_refused_not_forwarded() {
        let (mut cockpit, fake) = cockpit("stale");
        let thread = cockpit.open(Provider::Claude).unwrap();
        fake.streams.borrow()[0]
            .send(decision("perm_01", "Write"))
            .unwrap();
        cockpit.pump();

        assert!(cockpit.answer(thread, "perm_01"));
        assert_eq!(cockpit.pending(thread), None);

        // A second keystroke on a card already answered, and an answer to a
        // request this Thread never had: neither may reach the provider.
        assert!(!cockpit.answer(thread, "perm_01"));
        assert!(!cockpit.answer(thread, "perm_99"));
    }

    #[test]
    fn a_turn_that_ends_takes_its_unanswered_decision_with_it() {
        let (mut cockpit, fake) = cockpit("moot");
        let thread = cockpit.open(Provider::Claude).unwrap();
        fake.streams.borrow()[0]
            .send(decision("perm_01", "Write"))
            .unwrap();
        cockpit.pump();

        // The turn was interrupted, or the provider gave up waiting: the
        // request is gone, so the card must not linger over the Composer.
        fake.streams.borrow()[0].send(ended()).unwrap();
        cockpit.pump();

        assert_eq!(cockpit.pending(thread), None);
        assert!(cockpit.blocked().is_empty());
        assert!(!cockpit.answer(thread, "perm_01"));
    }

    #[test]
    fn a_prompt_typed_during_a_turn_is_sent_when_the_turn_ends() {
        let (mut cockpit, fake) = cockpit("queued");
        let thread = cockpit.open(Provider::Claude).unwrap();
        fake.streams.borrow()[0].send(text("working")).unwrap();
        cockpit.pump();
        assert!(cockpit.busy(thread));

        cockpit.queue(thread, "and then run the tests".into());
        assert_eq!(cockpit.queued(thread), Some("and then run the tests"));
        assert!(fake.sent.borrow().is_empty(), "nothing goes out mid-turn");

        fake.streams.borrow()[0].send(ended()).unwrap();
        cockpit.pump();

        assert_eq!(fake.sent.borrow().as_slice(), ["and then run the tests"]);
        assert_eq!(cockpit.queued(thread), None);
        assert!(!cockpit.busy(thread));
    }

    #[test]
    fn a_held_prompt_can_be_taken_back_for_editing() {
        let (mut cockpit, _fake) = cockpit("unqueue");
        let thread = cockpit.open(Provider::Claude).unwrap();
        cockpit.queue(thread, "run the tets".into());

        let back = cockpit.unqueue(thread);

        assert_eq!(back.as_deref(), Some("run the tets"));
        assert_eq!(cockpit.queued(thread), None);
        assert_eq!(cockpit.unqueue(thread), None);
    }

    #[test]
    fn a_turn_ending_with_nothing_held_sends_nothing() {
        let (mut cockpit, fake) = cockpit("nothing-held");
        cockpit.open(Provider::Claude).unwrap();

        fake.streams.borrow()[0].send(ended()).unwrap();
        cockpit.pump();

        assert!(fake.sent.borrow().is_empty());
    }
}
