//! Notifications: when a Thread's Main has stopped working for good.
//!
//! One deep module behind one question — *does this Thread need the
//! operator back?* — fed only by what the provider adapters already
//! normalised into `Activity`: Main's turn ends, Main's busy state, and
//! every child's status. No provider JSON, no clock of its own (the
//! Cockpit hands it `now`), no window.
//!
//! The rule: a Notice is born when a live Main turn ends and no descendant
//! is still working or awaiting permission. A Main that ended its turn
//! while children run is **deferred** — a Claude Main with busy background agents
//! will be resumed by their completions, and a Codex parent waiting on
//! `wait` never ended its turn at all. The deferral resolves when Main is
//! resumed (the Notice waits for that turn's end) or, if the provider
//! never resumes it, a short grace after the last child settles.
//!
//! A Notice is *unread* until the operator lands on its Thread or opens
//! it from the bell; an unread Notice is what makes a Pane ask for
//! attention. Interrupts never notify: the operator did that themselves.
//! A held prompt going out at turn end never notifies either: the
//! operator queued more work and is not waiting for this one.

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant, SystemTime};

use crate::activity::{ActivityView, AgentStatus};
use crate::{ThreadId, TurnOutcome};

/// How long Main may sit idle after its last child settles before the
/// deferral concludes that no provider is going to resume it.
pub const DEFAULT_GRACE: Duration = Duration::from_secs(3);

/// How many Notices are remembered, newest kept.
const CAPACITY: usize = 100;

/// A Notice's identity: monotonic per launch, so "newer than" is an
/// ordering the window can keep a watermark against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoticeId(u64);

impl NoticeId {
    pub fn get(self) -> u64 {
        self.0
    }

    /// A NoticeId from its number — for a window mapping an element id
    /// back to the Notice it stood for; `Notifications` mints the real ones.
    pub fn from_u64(id: u64) -> Self {
        Self(id)
    }
}

/// One "Main finished" fact, for the bell, a toast, and the Pane's ring.
#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub id: NoticeId,
    pub thread: ThreadId,
    /// How the turn ended: completed, or the provider's own error. Never
    /// `Interrupted` — see the module doc.
    pub outcome: TurnOutcome,
    pub at: SystemTime,
    /// False until the operator saw it: landed on the Thread, or opened
    /// it from the bell.
    pub read: bool,
}

/// One frame's facts about one Thread, as the Cockpit folded them.
#[derive(Clone, Copy)]
pub struct Frame<'a> {
    pub activity: ActivityView<'a>,
    /// A live Main turn ended this frame — the provider's own turn end,
    /// operator-prompted or autonomous.
    pub settled: bool,
    /// A held prompt went out on that turn end: the operator queued more
    /// work, so this Thread is not one they are waiting on.
    pub resumed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Nothing owed.
    Quiet,
    /// Main is at work (or the operator's prompt is out).
    Working,
    /// Main's turn ended while descendants were still working.
    Deferred,
    /// Every descendant has settled and Main is idle: waiting out the
    /// grace for a provider that resumes Main on its own.
    Grace(Instant),
}

pub struct Notifications {
    notices: VecDeque<Notice>,
    phases: BTreeMap<ThreadId, Phase>,
    next: u64,
    grace: Duration,
}

impl Default for Notifications {
    fn default() -> Self {
        Self::with_grace(DEFAULT_GRACE)
    }
}

impl Notifications {
    pub fn with_grace(grace: Duration) -> Self {
        Self {
            notices: VecDeque::new(),
            phases: BTreeMap::new(),
            next: 0,
            grace,
        }
    }

    /// Fold one Thread's frame. The Notice born this frame, if one was.
    pub fn observe(
        &mut self,
        thread: ThreadId,
        frame: Frame<'_>,
        now: Instant,
    ) -> Option<NoticeId> {
        let view = frame.activity;
        if !view.connected() {
            self.disconnect(thread);
            return None;
        }
        let main = view.main();
        let busy = main.busy() || view.main_operator_turn();
        // A child awaiting a Decision is unfinished even though it is
        // Waiting rather than Working. Retained history is never live work.
        let unfinished = !view.pending_decisions().is_empty()
            || view.children().iter().any(|child| {
                child.fresh()
                    && matches!(
                        child.status(),
                        AgentStatus::Pending | AgentStatus::Working | AgentStatus::Waiting
                    )
            });
        let phase = self.phases.entry(thread).or_insert(Phase::Quiet);
        let settle = |outcome: Option<&TurnOutcome>| {
            // A provider that says the turn ended but reports Main as
            // still busy is mid-handover; an interrupt is the operator's
            // own act and never news to them.
            match outcome {
                Some(TurnOutcome::Interrupted) | None => None,
                Some(outcome) => Some(outcome.clone()),
            }
        };
        // One frame folds many provider events: a turn that ended and a
        // next turn that began inside it read as a resume, and the finish
        // is that next turn's to report.
        let born = match *phase {
            Phase::Quiet | Phase::Working => {
                if frame.settled && !frame.resumed && !busy {
                    if !unfinished {
                        *phase = Phase::Quiet;
                        settle(main.last_outcome())
                    } else {
                        *phase = Phase::Deferred;
                        None
                    }
                } else {
                    if busy || frame.resumed {
                        *phase = Phase::Working;
                    }
                    None
                }
            }
            Phase::Deferred => {
                if busy {
                    *phase = Phase::Working;
                } else if !unfinished {
                    *phase = Phase::Grace(now);
                }
                None
            }
            Phase::Grace(since) => {
                if busy {
                    *phase = Phase::Working;
                    None
                } else if unfinished {
                    *phase = Phase::Deferred;
                    None
                } else if frame.settled || now.saturating_duration_since(since) >= self.grace {
                    *phase = Phase::Quiet;
                    settle(main.last_outcome())
                } else {
                    None
                }
            }
        };
        Some(self.push(thread, born?))
    }

    fn push(&mut self, thread: ThreadId, outcome: TurnOutcome) -> NoticeId {
        self.next += 1;
        let id = NoticeId(self.next);
        self.notices.push_back(Notice {
            id,
            thread,
            outcome,
            at: SystemTime::now(),
            read: false,
        });
        while self.notices.len() > CAPACITY {
            self.notices.pop_front();
        }
        id
    }

    /// Every Notice, newest first.
    pub fn notices(&self) -> impl Iterator<Item = &Notice> {
        self.notices.iter().rev()
    }

    /// The Notices newer than `after` (all of them for `None`), oldest
    /// first — what a window presents once and then moves its watermark
    /// past.
    pub fn since(&self, after: Option<NoticeId>) -> impl Iterator<Item = &Notice> {
        self.notices
            .iter()
            .filter(move |notice| after.is_none_or(|after| notice.id > after))
    }

    pub fn newest(&self) -> Option<NoticeId> {
        self.notices.back().map(|notice| notice.id)
    }

    pub fn get(&self, id: NoticeId) -> Option<&Notice> {
        self.notices.iter().find(|notice| notice.id == id)
    }

    pub fn unread(&self) -> usize {
        self.notices.iter().filter(|notice| !notice.read).count()
    }

    /// Does this Thread hold an unread Notice — should its Pane ask for
    /// the operator's eye?
    pub fn attention(&self, thread: ThreadId) -> bool {
        self.notices
            .iter()
            .any(|notice| notice.thread == thread && !notice.read)
    }

    /// The operator opened this Notice: it is read, and its Thread is
    /// where they want to land.
    pub fn open(&mut self, id: NoticeId) -> Option<ThreadId> {
        let notice = self.notices.iter_mut().find(|notice| notice.id == id)?;
        notice.read = true;
        Some(notice.thread)
    }

    /// The operator landed on this Thread: everything it had to say is
    /// seen.
    pub fn acknowledge(&mut self, thread: ThreadId) -> bool {
        let mut changed = false;
        for notice in self.notices.iter_mut() {
            if notice.thread == thread && !notice.read {
                notice.read = true;
                changed = true;
            }
        }
        changed
    }

    pub fn dismiss(&mut self, id: NoticeId) -> bool {
        let before = self.notices.len();
        self.notices.retain(|notice| notice.id != id);
        before != self.notices.len()
    }

    pub fn clear(&mut self) {
        self.notices.clear();
    }

    /// The live Session ended: discard any deferred finish, but keep its
    /// existing Notices so the operator can still open the parked Thread.
    pub fn disconnect(&mut self, thread: ThreadId) {
        self.phases.remove(&thread);
    }

    /// The Thread is gone: nothing about it is worth keeping.
    pub fn forget(&mut self, thread: ThreadId) {
        self.notices.retain(|notice| notice.thread != thread);
        self.disconnect(thread);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::{
        Activity, ActivityEvent, ActivityInput, ActivityUpdate, AgentInfo, AgentKey, AgentStatus,
        Subject,
    };
    use crate::store::Provider;
    use crate::transcript::Input;
    use crate::SessionEvent;

    /// A Thread's Activity driven the way the Cockpit's pump drives it:
    /// every apply is followed by an observe that carries the update's
    /// settle flag, so the frames are exactly the pump's frames.
    struct Bench {
        activity: Activity,
        notifications: Notifications,
        thread: ThreadId,
        now: Instant,
        held: bool,
    }

    impl Bench {
        fn new(grace: Duration) -> Self {
            let mut activity = Activity::default();
            activity.apply(ActivityInput::Connect { generation: 7 });
            Self {
                activity,
                notifications: Notifications::with_grace(grace),
                thread: ThreadId::new(1),
                now: Instant::now(),
                held: false,
            }
        }

        fn frame(&mut self, applied: ActivityUpdate) -> Option<NoticeId> {
            let resumed = std::mem::take(&mut self.held);
            self.notifications.observe(
                self.thread,
                Frame {
                    activity: self.activity.view(),
                    settled: applied.main_settled,
                    resumed,
                },
                self.now,
            )
        }

        fn main(&mut self, input: Input) -> Option<NoticeId> {
            let applied = self.activity.apply(ActivityInput::Main {
                input,
                at: self.now,
            });
            self.frame(applied)
        }

        fn event(&mut self, event: ActivityEvent) -> Option<NoticeId> {
            let applied = self.activity.apply(ActivityInput::Observe {
                generation: 7,
                event,
                at: self.now,
            });
            self.frame(applied)
        }

        /// A quiet frame: the pump ran, nothing arrived.
        fn idle(&mut self) -> Option<NoticeId> {
            self.frame(ActivityUpdate::default())
        }

        fn wait(&mut self, by: Duration) -> Option<NoticeId> {
            self.now += by;
            self.idle()
        }

        fn prompt(&mut self) {
            assert_eq!(self.main(Input::Prompt("go".into())), None);
        }

        fn stream(&mut self) {
            assert_eq!(
                self.main(Input::Event(SessionEvent::TextDelta { text: "…".into() })),
                None
            );
        }

        fn turn_ended(&mut self, outcome: TurnOutcome) -> Option<NoticeId> {
            self.main(Input::Event(SessionEvent::TurnEnded {
                outcome,
                cost_usd: None,
            }))
        }

        fn child(&mut self, name: &str) -> AgentKey {
            let key = AgentKey::new(Provider::Claude, "root", name);
            let mut info = AgentInfo::new(key.clone());
            info.parent = Some(Subject::Main);
            assert_eq!(self.event(ActivityEvent::Discovered(info)), None);
            assert_eq!(
                self.event(ActivityEvent::Status {
                    key: key.clone(),
                    state: AgentStatus::Working,
                }),
                None
            );
            key
        }

        fn child_done(&mut self, key: &AgentKey) -> Option<NoticeId> {
            self.event(ActivityEvent::Status {
                key: key.clone(),
                state: AgentStatus::Idle,
            })
        }
    }

    fn done() -> TurnOutcome {
        TurnOutcome::Completed
    }

    #[test]
    fn a_turn_that_ends_with_no_children_working_notifies_at_once() {
        let mut b = Bench::new(DEFAULT_GRACE);
        b.prompt();
        b.stream();
        let id = b.turn_ended(done()).expect("the turn end is the news");
        let notice = b.notifications.get(id).unwrap();
        assert_eq!(notice.thread, b.thread);
        assert_eq!(notice.outcome, TurnOutcome::Completed);
        assert!(!notice.read);
        assert!(b.notifications.attention(b.thread));
        assert_eq!(b.notifications.unread(), 1);
        // Quiet frames afterwards say nothing new.
        assert_eq!(b.idle(), None);
        assert_eq!(b.wait(Duration::from_secs(60)), None);
    }

    #[test]
    fn a_failed_turn_notifies_with_the_providers_error_and_an_interrupt_never_does() {
        let mut b = Bench::new(DEFAULT_GRACE);
        b.prompt();
        let id = b
            .turn_ended(TurnOutcome::Error("rate limited".into()))
            .expect("a failure is news");
        assert_eq!(
            b.notifications.get(id).unwrap().outcome,
            TurnOutcome::Error("rate limited".into())
        );

        b.prompt();
        b.stream();
        assert_eq!(
            b.turn_ended(TurnOutcome::Interrupted),
            None,
            "esc is the operator's own act"
        );
        assert_eq!(b.notifications.unread(), 1);
    }

    #[test]
    fn a_turn_end_that_sends_a_held_prompt_is_not_a_finish() {
        let mut b = Bench::new(DEFAULT_GRACE);
        b.prompt();
        b.stream();
        // The pump delivers the held prompt in the same frame the turn
        // ends; the operator queued more work and waits for *that*.
        b.held = true;
        assert_eq!(b.turn_ended(done()), None);
        b.stream();
        let id = b.turn_ended(done());
        assert!(id.is_some(), "the held prompt's own turn end is the finish");
    }

    #[test]
    fn a_turn_end_with_children_working_waits_for_them_and_for_mains_resume() {
        let mut b = Bench::new(DEFAULT_GRACE);
        b.prompt();
        let alpha = b.child("alpha");
        let beta = b.child("beta");
        // Claude: Main's own result arrives while both background agents
        // run. Not a finish.
        assert_eq!(b.turn_ended(done()), None);
        assert!(!b.notifications.attention(b.thread));
        // Alpha finishes; the provider resumes Main for it (the per-turn
        // init marks Main working before the API answers) …
        assert_eq!(b.child_done(&alpha), None);
        b.stream();
        // … and that autonomous turn ends with beta still at work.
        let applied = b.activity.apply(ActivityInput::Observe {
            generation: 7,
            event: ActivityEvent::BackgroundTurnEnded {
                outcome: done(),
                cost_usd: None,
            },
            at: b.now,
        });
        assert!(applied.main_settled);
        assert_eq!(b.frame(applied), None, "beta is still working");
        // Beta finishes and Main is resumed once more; *that* turn's end
        // is the finish, with no grace to wait out.
        assert_eq!(b.child_done(&beta), None);
        b.stream();
        let applied = b.activity.apply(ActivityInput::Observe {
            generation: 7,
            event: ActivityEvent::BackgroundTurnEnded {
                outcome: done(),
                cost_usd: None,
            },
            at: b.now,
        });
        assert!(b.frame(applied).is_some());
        assert_eq!(b.notifications.unread(), 1);
    }

    #[test]
    fn a_provider_that_never_resumes_main_notifies_after_the_grace() {
        let grace = Duration::from_secs(3);
        let mut b = Bench::new(grace);
        b.prompt();
        let child = b.child("worker");
        assert_eq!(b.turn_ended(done()), None);
        assert_eq!(b.child_done(&child), None, "the grace starts now");
        assert_eq!(
            b.wait(Duration::from_secs(2)),
            None,
            "still inside the grace"
        );
        assert!(
            b.wait(Duration::from_secs(2)).is_some(),
            "nobody resumed Main"
        );
        assert_eq!(b.wait(Duration::from_secs(60)), None, "and only once");
    }

    #[test]
    fn a_resume_inside_the_grace_cancels_it_until_that_turn_ends() {
        let mut b = Bench::new(Duration::from_secs(3));
        b.prompt();
        let child = b.child("worker");
        assert_eq!(b.turn_ended(done()), None);
        assert_eq!(b.child_done(&child), None);
        assert_eq!(b.wait(Duration::from_secs(1)), None);
        // Main streams again inside the grace: the finish is that turn's.
        b.stream();
        assert_eq!(b.wait(Duration::from_secs(10)), None, "Main is at work");
        assert!(b.turn_ended(done()).is_some());
    }

    #[test]
    fn an_unfinished_child_at_the_grace_holds_the_deferral() {
        for state in [
            AgentStatus::Working,
            AgentStatus::Pending,
            AgentStatus::Waiting,
        ] {
            let mut b = Bench::new(Duration::from_secs(3));
            b.prompt();
            let first = b.child("first");
            assert_eq!(b.turn_ended(done()), None);
            assert_eq!(b.child_done(&first), None);
            // The provider reports another child during the grace, even
            // if it is waiting and no Decision has been attributed yet.
            let key = AgentKey::new(Provider::Claude, "root", "second");
            let mut info = AgentInfo::new(key.clone());
            info.parent = Some(Subject::Main);
            assert_eq!(b.event(ActivityEvent::Discovered(info)), None);
            assert_eq!(
                b.event(ActivityEvent::Status {
                    key: key.clone(),
                    state
                }),
                None
            );
            assert_eq!(b.wait(Duration::from_secs(10)), None, "{state:?}");
            assert_eq!(b.child_done(&key), None);
            assert!(b.wait(Duration::from_secs(4)).is_some());
        }
    }

    #[test]
    fn a_subagent_finishing_is_never_news_on_its_own() {
        let mut b = Bench::new(Duration::from_millis(1));
        b.prompt();
        b.stream();
        let child = b.child("helper");
        assert_eq!(b.child_done(&child), None, "Main is still at work");
        assert_eq!(b.wait(Duration::from_secs(10)), None);
        assert!(b.turn_ended(done()).is_some());
    }

    #[test]
    fn reading_is_by_thread_or_by_notice_and_attention_follows_the_unread() {
        let mut b = Bench::new(DEFAULT_GRACE);
        b.prompt();
        let first = b.turn_ended(done()).unwrap();
        b.prompt();
        let second = b.turn_ended(done()).unwrap();
        assert_eq!(b.notifications.unread(), 2);
        assert!(b.notifications.attention(b.thread));
        // Newest first for the bell; oldest first past a watermark.
        let listed: Vec<_> = b.notifications.notices().map(|n| n.id).collect();
        assert_eq!(listed, vec![second, first]);
        let fresh: Vec<_> = b.notifications.since(Some(first)).map(|n| n.id).collect();
        assert_eq!(fresh, vec![second]);
        assert_eq!(b.notifications.newest(), Some(second));

        assert_eq!(b.notifications.open(first), Some(b.thread));
        assert_eq!(b.notifications.unread(), 1);
        assert!(
            b.notifications.attention(b.thread),
            "the second is still unread"
        );
        assert!(b.notifications.acknowledge(b.thread));
        assert!(
            !b.notifications.acknowledge(b.thread),
            "nothing left to read"
        );
        assert!(!b.notifications.attention(b.thread));
        assert_eq!(b.notifications.unread(), 0);

        assert!(b.notifications.dismiss(first));
        assert!(!b.notifications.dismiss(first));
        assert_eq!(b.notifications.notices().count(), 1);
        b.notifications.forget(b.thread);
        assert_eq!(b.notifications.notices().count(), 0);
        assert_eq!(b.notifications.open(second), None);
    }

    /// The pump folds up to 256 events a frame: a `result` and the next
    /// turn's `init` can land together. That frame is a resume, and the
    /// finish belongs to the turn that just began.
    #[test]
    fn a_settle_and_a_resume_in_one_frame_defer_to_the_new_turn() {
        let mut b = Bench::new(DEFAULT_GRACE);
        b.prompt();
        let applied = b.activity.apply(ActivityInput::Main {
            input: Input::Event(SessionEvent::TurnEnded {
                outcome: done(),
                cost_usd: None,
            }),
            at: b.now,
        });
        assert!(applied.main_settled);
        b.activity.apply(ActivityInput::Main {
            input: Input::Event(SessionEvent::Progress {
                event: crate::progress::ProgressEvent::Phase {
                    phase: crate::progress::Phase::Working,
                    detail: String::new(),
                },
            }),
            at: b.now,
        });
        assert_eq!(b.frame(applied), None, "Main is already at work again");
        assert!(b.turn_ended(done()).is_some());
    }

    #[test]
    fn a_disconnected_thread_owes_nothing() {
        let mut b = Bench::new(DEFAULT_GRACE);
        b.prompt();
        let child = b.child("worker");
        assert_eq!(b.turn_ended(done()), None);
        assert_eq!(b.child_done(&child), None);
        let applied = b.activity.apply(ActivityInput::Disconnect);
        assert_eq!(b.frame(applied), None);
        assert_eq!(b.wait(Duration::from_secs(60)), None, "the Session is gone");
    }

    #[test]
    fn the_store_keeps_the_newest_hundred() {
        let mut b = Bench::new(DEFAULT_GRACE);
        for _ in 0..(CAPACITY + 5) {
            b.prompt();
            b.turn_ended(done()).unwrap();
        }
        assert_eq!(b.notifications.notices().count(), CAPACITY);
        assert_eq!(
            b.notifications.newest().unwrap().get(),
            (CAPACITY + 5) as u64
        );
    }
}
