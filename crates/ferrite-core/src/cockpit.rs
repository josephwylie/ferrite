//! The Cockpit's headless state: what the operator is on the hook for, per
//! Thread.
//!
//! The pump's beginnings: Threads, their pending Decisions, and prompts held
//! back while a turn runs. No process and no window — a Thread's events are
//! fed in, and what the operator must answer comes out.

use std::collections::BTreeMap;

use crate::{Decision, SessionEvent};

/// A Thread's identity inside one run of the cockpit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThreadId(pub u64);

/// What one fold left for the caller to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Wake {
    Nothing,
    /// The turn ended and a prompt was waiting behind it — send this now.
    Send(String),
}

#[derive(Debug, Default)]
struct Thread {
    pending: Option<Decision>,
    /// A prompt the operator wrote while the turn was still running.
    queued: Option<String>,
    busy: bool,
}

#[derive(Debug, Default)]
pub struct Cockpit {
    threads: BTreeMap<ThreadId, Thread>,
}

impl Cockpit {
    /// Fold one Thread's event into what the operator sees, and say what the
    /// caller must do about it.
    pub fn apply(&mut self, thread: ThreadId, event: &SessionEvent) -> Wake {
        let state = self.threads.entry(thread).or_default();
        let mut wake = Wake::Nothing;
        match event {
            SessionEvent::TextDelta { .. }
            | SessionEvent::ThinkingDelta { .. }
            | SessionEvent::ReasoningSummaryDelta { .. }
            | SessionEvent::ToolStarted { .. } => state.busy = true,
            // A turn that ends takes its Decision with it: the provider is no
            // longer waiting, so an answer would go nowhere. Anything the
            // operator wrote behind the turn goes out now.
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
            _ => {}
        }
        wake
    }

    /// Is a turn running? A prompt written now has to wait for it.
    pub fn busy(&self, thread: ThreadId) -> bool {
        self.threads.get(&thread).is_some_and(|state| state.busy)
    }

    /// Hold a prompt written mid-turn. It stays visible and editable until the
    /// turn ends, which is when it is sent.
    pub fn queue(&mut self, thread: ThreadId, text: String) {
        self.threads.entry(thread).or_default().queued = Some(text);
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

    /// Threads waiting on the operator — what the wall badges.
    pub fn blocked(&self) -> Vec<ThreadId> {
        self.threads
            .iter()
            .filter(|(_, state)| state.pending.is_some())
            .map(|(id, _)| *id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn a_decision_is_pending_against_the_thread_that_raised_it() {
        let mut cockpit = Cockpit::default();
        let one = ThreadId(1);
        let two = ThreadId(2);

        cockpit.apply(one, &decision("perm_01", "Write"));

        assert_eq!(cockpit.pending(one).unwrap().tool_name, "Write");
        assert_eq!(cockpit.pending(two), None);
        assert_eq!(cockpit.blocked(), vec![one]);
    }

    #[test]
    fn answering_a_decision_that_is_no_longer_pending_is_refused_not_forwarded() {
        let mut cockpit = Cockpit::default();
        let thread = ThreadId(1);
        cockpit.apply(thread, &decision("perm_01", "Write"));

        assert!(cockpit.answer(thread, "perm_01"));
        assert_eq!(cockpit.pending(thread), None);

        // A second keystroke on a card already answered, and an answer to a
        // request this Thread never had: neither may reach the provider.
        assert!(!cockpit.answer(thread, "perm_01"));
        assert!(!cockpit.answer(thread, "perm_99"));
        assert!(!cockpit.answer(ThreadId(2), "perm_01"));
    }

    #[test]
    fn a_prompt_typed_during_a_turn_is_held_until_the_turn_ends() {
        let mut cockpit = Cockpit::default();
        let thread = ThreadId(1);
        assert!(!cockpit.busy(thread));

        cockpit.apply(
            thread,
            &SessionEvent::TextDelta {
                text: "working".into(),
            },
        );
        assert!(cockpit.busy(thread));

        cockpit.queue(thread, "and then run the tests".into());
        assert_eq!(cockpit.queued(thread), Some("and then run the tests"));

        let woken = cockpit.apply(
            thread,
            &SessionEvent::TurnEnded {
                outcome: crate::TurnOutcome::Completed,
                cost_usd: None,
            },
        );

        assert_eq!(woken, Wake::Send("and then run the tests".into()));
        assert_eq!(cockpit.queued(thread), None);
        assert!(!cockpit.busy(thread));
    }

    #[test]
    fn a_held_prompt_can_be_taken_back_for_editing() {
        let mut cockpit = Cockpit::default();
        let thread = ThreadId(1);
        cockpit.queue(thread, "run the tets".into());

        let back = cockpit.unqueue(thread);

        assert_eq!(back.as_deref(), Some("run the tets"));
        assert_eq!(cockpit.queued(thread), None);
        assert_eq!(cockpit.unqueue(thread), None);
    }

    #[test]
    fn a_turn_ending_with_nothing_held_wakes_nobody() {
        let mut cockpit = Cockpit::default();
        let thread = ThreadId(1);

        let woken = cockpit.apply(
            thread,
            &SessionEvent::TurnEnded {
                outcome: crate::TurnOutcome::Completed,
                cost_usd: None,
            },
        );

        assert_eq!(woken, Wake::Nothing);
    }

    #[test]
    fn a_turn_that_ends_takes_its_unanswered_decision_with_it() {
        let mut cockpit = Cockpit::default();
        let thread = ThreadId(1);
        cockpit.apply(thread, &decision("perm_01", "Write"));

        // The turn was interrupted, or the provider gave up waiting: the
        // request is gone, so the card must not linger over the Composer.
        cockpit.apply(
            thread,
            &SessionEvent::TurnEnded {
                outcome: crate::TurnOutcome::Interrupted,
                cost_usd: None,
            },
        );

        assert_eq!(cockpit.pending(thread), None);
        assert!(cockpit.blocked().is_empty());
        assert!(!cockpit.answer(thread, "perm_01"));
    }
}
