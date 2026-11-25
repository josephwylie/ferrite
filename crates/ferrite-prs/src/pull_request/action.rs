//! What a PR action became: a hand-off to the Thread's agent, followed
//! from the press to the PR as the forge shows it afterwards.
//!
//! Every action belongs to one target (a PR number in a Thread's
//! repository), however the user moves meanwhile: another PR, another
//! Thread, another tab. A target has at most one action in flight:
//!
//! ```text
//! Idle ──press──▶ Pending(Queued) ─▶ Pending(Running) ─▶ Pending(Checking)
//!                    │ refused               │ turn ends      │ the PR read again
//!                    ▼                       ▼                ▼
//!                 Failed ◀──── the PR did not change ───── Done
//! ```
//!
//! A queued prompt the user takes back before it is sent never ran: its
//! action goes back to `Idle`.
//!
//! `Idle` is the absence of a state. A `Failed` keeps the PR's content and
//! offers the action again; a `Pending` can be given up on.

use std::collections::HashMap;

use ferrite_core::protocol::{WireMerge, WirePr, WirePrState};
use gpui::Context;

use super::read::PrRead;
use super::{PrAction, PrEvent, PullRequestView};
use ferrite_client::cockpit::ThreadView;
use ferrite_core::pull_request::{PrSource, PrTarget};
use ferrite_core::transcript::Body;

/// The queue entry an action waits as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Queued {
    /// `QueueItem::id`; empty from a Server too old to send ids.
    item: String,
    /// The Thread's prompts of this text before it: one more means a turn
    /// took it.
    prompts_before: usize,
}

/// Where a pending action stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Sent to the Thread, whose Server has not yet said where it went.
    Handing,
    /// Behind a running turn, or held for a Session still starting.
    Queued(Queued),
    /// The agent has the prompt; its turn has not ended.
    Running,
    /// The turn ended: the PR is read again to see what it did.
    Checking,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionState {
    Pending {
        action: PrAction,
        stage: Stage,
        /// What the agent was asked: how the queue names it.
        prompt: String,
    },
    Failed {
        action: PrAction,
        error: String,
        /// What Retry redoes: the hand-off, or only the read of the PR
        /// that failed after it.
        retry: Retry,
    },
    /// The PR shows what the action meant, or the action's result is the
    /// agent's reply.
    Done { action: PrAction },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retry {
    Action,
    Read,
}

#[derive(Default)]
pub(crate) struct PrActions(HashMap<PrTarget, ActionState>);

impl PrActions {
    pub(crate) fn get(&self, target: PrTarget) -> Option<&ActionState> {
        self.0.get(&target)
    }

    pub(crate) fn pending(&self, target: PrTarget) -> bool {
        matches!(self.get(target), Some(ActionState::Pending { .. }))
    }

    /// `action` handed over, unless the target has one in flight.
    fn begin(&mut self, target: PrTarget, action: PrAction, prompt: String, stage: Stage) -> bool {
        if self.pending(target) {
            return false;
        }
        self.0.insert(
            target,
            ActionState::Pending {
                action,
                stage,
                prompt,
            },
        );
        true
    }

    fn fail(&mut self, target: PrTarget, action: PrAction, error: String) {
        self.fail_with(target, action, error, Retry::Action);
    }

    fn fail_with(&mut self, target: PrTarget, action: PrAction, error: String, retry: Retry) {
        self.0.insert(
            target,
            ActionState::Failed {
                action,
                error,
                retry,
            },
        );
    }

    /// Keep only the targets `keep` holds to.
    pub(crate) fn retain(&mut self, keep: impl Fn(&PrTarget) -> bool) {
        self.0.retain(|target, _| keep(target));
    }

    /// A finished action's line goes when its PR's view changes.
    pub(crate) fn clear_done(&mut self, target: PrTarget) {
        if matches!(self.get(target), Some(ActionState::Done { .. })) {
            self.clear(target);
        }
    }

    pub(crate) fn clear(&mut self, target: PrTarget) {
        self.0.remove(&target);
    }

    fn set_stage(&mut self, target: PrTarget, to: Stage) {
        if let Some(ActionState::Pending { stage, .. }) = self.0.get_mut(&target) {
            *stage = to;
        }
    }

    /// The targets of `thread`'s repository with an action in flight.
    fn pending_of(
        &self,
        thread: ferrite_core::ThreadKey,
    ) -> Vec<(PrTarget, Stage, String, PrAction)> {
        self.0
            .iter()
            .filter_map(|(target, state)| match state {
                ActionState::Pending {
                    stage,
                    prompt,
                    action,
                } if target.source == PrSource::Thread(thread) => {
                    Some((*target, stage.clone(), prompt.clone(), *action))
                }
                _ => None,
            })
            .collect()
    }
}

impl PrAction {
    /// What the action is while it runs.
    pub(crate) fn working(self) -> &'static str {
        match self {
            Self::Ask => "Asking",
            Self::Explain => "Explaining",
            Self::FixFindings => "Fixing findings",
            Self::WriteGuide => "Writing guide",
            Self::ReadyForReview => "Marking ready",
            Self::ConvertToDraft => "Converting to draft",
            Self::MergeNow => "Merging",
            Self::AutoMerge => "Enabling auto-merge",
            Self::AllowEdits => "Allowing edits",
            Self::Close => "Closing",
            Self::Revert => "Reverting",
            Self::DeleteBranch => "Deleting branch",
            Self::CopyLink | Self::CopyBranch => "Copying",
        }
    }

    /// What the action did, once done.
    pub(crate) fn done(self) -> &'static str {
        match self {
            Self::ReadyForReview => "Marked ready for review",
            Self::ConvertToDraft => "Converted to draft",
            Self::MergeNow => "Merged",
            Self::Close => "Closed",
            Self::AutoMerge => "Auto-merge asked for",
            Self::Revert => "Revert asked for",
            Self::DeleteBranch => "Branch deletion asked for",
            Self::AllowEdits => "Edits allowed",
            Self::Explain => "Explained",
            Self::FixFindings => "Findings addressed",
            Self::WriteGuide => "Guide written",
            Self::Ask | Self::CopyLink | Self::CopyBranch => "Done",
        }
    }

    /// Whether the PR, read after the agent finished, shows the action
    /// done; `None` for an action the PR's own state does not witness.
    fn witnessed(self, pr: &WirePr) -> Option<Result<(), String>> {
        let n = pr.number;
        let (done, still) = match self {
            Self::ReadyForReview => (!pr.draft, "is still a draft"),
            Self::ConvertToDraft => (pr.draft, "is not a draft"),
            Self::MergeNow => (pr.state == WirePrState::Merged, "is not merged"),
            Self::Close => (pr.state == WirePrState::Closed, "is not closed"),
            _ => return None,
        };
        Some(if done {
            Ok(())
        } else {
            Err(format!(
                "#{n} {still} after the agent finished. Its reply in the Thread says why."
            ))
        })
    }

    /// Why the forge's own reading rules the action out, if it does.
    pub(crate) fn blocked_by(self, pr: &WirePr) -> Option<String> {
        match (self, pr.merge) {
            (Self::MergeNow | Self::AutoMerge, WireMerge::Conflicting) => Some(format!(
                "Conflicts with {}. Resolve them before merging.",
                pr.base
            )),
            (Self::MergeNow, WireMerge::Blocked) => {
                Some("Blocked by required checks or reviews.".into())
            }
            _ => None,
        }
    }
}

impl Stage {
    /// Behind a running turn or held: as `open`'s queue lists `prompt`,
    /// beside how many prompts of its text the Thread held before it.
    pub fn queued_in(open: Option<&ThreadView>, prompt: &str) -> Self {
        Self::Queued(Queued {
            item: queued_as(open, prompt).unwrap_or_default(),
            prompts_before: prompts_of(open, prompt),
        })
    }
}

/// The id the newest queued prompt of this text goes by (the queue lists
/// newest first).
fn queued_as(open: Option<&ThreadView>, prompt: &str) -> Option<String> {
    open?
        .queue()
        .iter()
        .find(|item| item.text.trim() == prompt.trim())
        .map(|item| item.id.clone())
}

/// How many prompts of this text the Thread's transcript holds.
fn prompts_of(open: Option<&ThreadView>, prompt: &str) -> usize {
    open.map_or(0, |open| {
        open.transcript()
            .blocks()
            .iter()
            .filter(|block| matches!(&block.body, Body::Prompt(text) if text.trim() == prompt.trim()))
            .count()
    })
}

impl PullRequestView {
    /// A fixture world's action on `target`, caught as `state`.
    #[cfg(any(test, feature = "test-support"))]
    pub fn hold_action(&mut self, target: PrTarget, state: ActionState) {
        self.actions.0.insert(target, state);
    }

    /// Whether `action` may be handed to the Thread's agent now: not with
    /// one in flight on the PR, nor when the forge's reading rules it out,
    /// which fails it with why.
    pub fn may_hand_off(
        &mut self,
        target: PrTarget,
        action: PrAction,
        pr: &WirePr,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.actions.pending(target) {
            return false;
        }
        if let Some(why) = action.blocked_by(pr) {
            self.actions.fail(target, action, why);
            cx.notify();
            return false;
        }
        true
    }

    /// `action` is on its way to the Thread's agent as `prompt`.
    pub fn handing(&mut self, target: PrTarget, action: PrAction, prompt: String, cx: &mut Context<Self>) {
        self.actions.begin(target, action, prompt, Stage::Handing);
        cx.notify();
    }

    /// What became of handing `action` over: where its prompt stands, or
    /// why the agent was not given it. A hand-off the operator stopped
    /// waiting on stays stopped.
    pub fn handed(
        &mut self,
        target: PrTarget,
        action: PrAction,
        handed: Result<Stage, String>,
        cx: &mut Context<Self>,
    ) {
        match handed {
            Ok(stage) => self.actions.set_stage(target, stage),
            Err(error) => self.actions.fail(target, action, error),
        }
        cx.notify();
    }

    /// Give up waiting on, or hide the outcome of, `target`'s action.
    pub(crate) fn dismiss(&mut self, target: PrTarget, cx: &mut Context<Self>) {
        self.actions.clear(target);
        cx.notify();
    }

    /// `thread` streamed, its queue and transcript now `open`'s: its
    /// pending actions move on. A prompt that left the queue for a turn is
    /// running, one taken back never ran; a turn that `settled` is checked,
    /// and its PR is to be read back (the targets returned).
    pub fn advance(
        &mut self,
        thread: ferrite_core::ThreadKey,
        open: Option<&ThreadView>,
        settled: bool,
        cx: &mut Context<Self>,
    ) -> Vec<(PrTarget, PrAction)> {
        let mut checking = Vec::new();
        for (target, stage, prompt, action) in self.actions.pending_of(thread) {
            if let Stage::Queued(queued) = &stage {
                let listed =
                    open.is_some_and(|open| open.queue().iter().any(|item| item.id == queued.item));
                // Sent now takes a prompt back and queues it again under
                // an id of its own.
                let resent = queued_as(open, &prompt).filter(|_| !listed);
                if let Some(item) = resent {
                    let queued = Queued {
                        item,
                        ..queued.clone()
                    };
                    self.actions.set_stage(target, Stage::Queued(queued));
                    continue;
                }
                if listed {
                    continue;
                }
                if prompts_of(open, &prompt) <= queued.prompts_before {
                    self.actions.clear(target);
                    cx.notify();
                    continue;
                }
                // A turn that settled this frame ran before the pickup.
                self.actions.set_stage(target, Stage::Running);
                cx.notify();
                continue;
            }
            if stage == Stage::Running && settled {
                self.actions.set_stage(target, Stage::Checking);
                checking.push((target, action));
                cx.notify();
            }
        }
        checking
    }

    /// The reads `checking` asked for: an action whose PR could not be
    /// read again at all is done.
    pub fn unread_are_done(&mut self, checking: &[(PrTarget, PrAction)], cx: &mut Context<Self>) {
        for &(target, action) in checking {
            if !self.reads.get(target).is_some_and(PrRead::reading) {
                self.actions.0.insert(target, ActionState::Done { action });
            }
        }
        cx.notify();
    }

    /// Retry of `target`'s failed action: the hand-off again, or only the
    /// read of the PR.
    pub(crate) fn retry(&mut self, target: PrTarget, cx: &mut Context<Self>) {
        let Some(ActionState::Failed { action, retry, .. }) = self.actions.get(target).cloned()
        else {
            return;
        };
        match retry {
            Retry::Action => cx.emit(PrEvent::Run(target, action)),
            Retry::Read => {
                self.actions.clear(target);
                self.actions
                    .begin(target, action, String::new(), Stage::Checking);
                cx.emit(PrEvent::Read(target));
                cx.notify();
            }
        }
    }

    /// The forge answered the read `target`'s finished action waited on:
    /// what the PR shows settles it.
    pub(crate) fn action_checked(&mut self, target: PrTarget, cx: &mut Context<Self>) {
        let Some(ActionState::Pending {
            action,
            stage: Stage::Checking,
            ..
        }) = self.actions.get(target).cloned()
        else {
            return;
        };
        let Some(read) = self.reads.get(target) else {
            // The PR moved to another number: nothing here to judge.
            self.actions.clear(target);
            cx.notify();
            return;
        };
        let failure = read.failure().map(|failure| failure.note.clone());
        let snapshot = read.snapshot.as_ref();
        match (failure, snapshot.and_then(|pr| action.witnessed(pr))) {
            (Some(note), _) => {
                let error = format!(
                    "The PR could not be read to see whether it worked ({note}). Its reply in the Thread says what the agent did."
                );
                self.actions.fail_with(target, action, error, Retry::Read);
            }
            (None, Some(Err(error))) => self.actions.fail(target, action, error),
            (None, _) => {
                self.actions.0.insert(target, ActionState::Done { action });
            }
        }
        cx.notify();
    }
}
