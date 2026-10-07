//! Replay bases (ADR 0008): where a revive may start reading a log, and the
//! earlier records it must still replay first. A base is a turn start far
//! enough back that Activity's retained window lies wholly after it; the
//! carry is what a full replay would know from before it — the newest of
//! each latest-wins kind, every identity a child was given, and the prompts
//! recall needs. Nothing here is truth: a mark names a base, and a reader
//! that ignores it replays the whole log and sees the same Thread.

use super::activity::{Execution, PersistedActivity};
use super::{PersistedProgress, Record};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// A replay base, as a mark names it: the offset of a turn start, and the
/// offsets of the records before it a replay from there needs first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Base {
    pub(super) at: u64,
    pub(super) carry: Vec<u64>,
}

/// Why a record before a base still matters to a replay from it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Slot {
    /// Only the newest of a kind: the runtime it sets is latest-wins.
    Latest(&'static str),
    /// Only the newest of a kind for one child.
    Child(&'static str, String),
}

/// What a record means to a replay that starts after it.
enum Carry {
    Slot(Slot),
    /// A child's identity: every introduction and alias is kept.
    Identity,
    /// A prompt: recall keeps every one since the last reset.
    Prompt,
    /// A conversation reset: recall starts again.
    Reset,
    /// A full task (or background) list: the deltas before it are spent.
    Snapshot(&'static str),
    /// A task (or background) delta, folded onto the newest snapshot.
    Delta(&'static str),
    /// Content: what a replay needs only inside the retained window.
    None,
}

fn progress(event: &PersistedProgress) -> Carry {
    match event {
        PersistedProgress::Plan { .. } => Carry::Slot(Slot::Latest("plan")),
        PersistedProgress::TasksSnapshot { .. } => Carry::Snapshot("tasks"),
        PersistedProgress::Task { .. } => Carry::Delta("tasks"),
        PersistedProgress::BackgroundSnapshot { .. } => Carry::Snapshot("background"),
        PersistedProgress::Background { .. } => Carry::Delta("background"),
        // Both end with the turn they belong to.
        PersistedProgress::Phase { .. } | PersistedProgress::Tool { .. } => Carry::None,
    }
}

/// The latest-wins runtime an execution event sets, by its kind.
fn execution(event: &Execution) -> Option<&'static str> {
    Some(match event {
        Execution::TokenUsage { .. } => "token_usage",
        Execution::ContextUsage { .. } => "context_usage",
        Execution::UsageDetails { .. } => "usage_details",
        Execution::TurnEnded { .. } => "turn_ended",
        Execution::TurnDiff { .. } => "turn_diff",
        _ => return None,
    })
}

fn classify(record: &Record) -> Carry {
    let latest = |kind| Carry::Slot(Slot::Latest(kind));
    match record {
        Record::Init { .. } => latest("init"),
        Record::ConversationReset { .. } => Carry::Reset,
        Record::Handover { .. } => latest("handover"),
        Record::ModelChanged { .. } => latest("model_changed"),
        Record::TokenUsage { .. } => latest("token_usage"),
        Record::ContextUsage { .. } => latest("context_usage"),
        Record::UsageDetails { .. } => latest("usage_details"),
        Record::ContextDetails { .. } => latest("context_details"),
        Record::RunState { .. } => latest("run_state"),
        Record::TurnEnded { .. } => latest("turn_ended"),
        Record::TurnDiff { .. } => latest("turn_diff"),
        Record::Closed { .. } => latest("closed"),
        Record::Progress { event } => progress(event),
        Record::Prompt { .. } => Carry::Prompt,
        Record::Activity { observation } => match observation {
            PersistedActivity::MainContent { event, .. } => match event {
                Execution::Progress { event } => progress(event),
                event => execution(event).map_or(Carry::None, latest),
            },
            PersistedActivity::BackgroundTurnEnded { .. } => latest("background_turn_ended"),
            PersistedActivity::Discovered { .. } | PersistedActivity::Alias { .. } => {
                Carry::Identity
            }
            PersistedActivity::Status { key, .. } => Carry::Slot(Slot::Child("status", key.clone())),
            PersistedActivity::Coverage { key, .. } => {
                Carry::Slot(Slot::Child("coverage", key.clone()))
            }
            PersistedActivity::Detached { key } => Carry::Slot(Slot::Child("detached", key.clone())),
            PersistedActivity::Content { key, event, .. }
            | PersistedActivity::HistoryContent { key, event, .. } => execution(event)
                .map_or(Carry::None, |kind| Carry::Slot(Slot::Child(kind, key.clone()))),
            PersistedActivity::CompletionObservation { subject, .. } => match subject.key() {
                Some(key) => Carry::Slot(Slot::Child("completion", key.to_string())),
                None => Carry::None,
            },
        },
        _ => Carry::None,
    }
}

/// What a record costs Activity's Main window, in Activity's own measure:
/// a base must have at least twice the Main budget of this after it.
pub(super) fn main_cost(record: &Record) -> u64 {
    let input = match record {
        Record::Facts { .. } | Record::Mark { .. } => return 0,
        Record::Activity { observation } => match observation {
            // A retraction removes content; it adds none.
            PersistedActivity::MainContent {
                event: Execution::Retract { .. },
                ..
            } => return 0,
            PersistedActivity::MainContent { event, .. } => event.live().into_input(),
            _ => return 0,
        },
        record => record.input(),
    };
    crate::activity::input_bytes(&input) as u64
}

/// A turn start not yet far enough back to be a base.
struct Candidate {
    at: u64,
    cost: u64,
    carry: Vec<u64>,
}

/// Follows a log record by record, in order, and answers the newest base
/// a mark may name (`base`).
pub(super) struct Tracker {
    slots: BTreeMap<Slot, u64>,
    identity: Vec<u64>,
    prompts: Vec<u64>,
    snapshots: BTreeMap<&'static str, u64>,
    deltas: BTreeMap<&'static str, Vec<u64>>,
    /// Main cost of everything observed.
    cost: u64,
    /// Whether Main is mid-turn: a prompt then is queued, not a turn start.
    busy: bool,
    candidates: VecDeque<Candidate>,
    /// The Main cost a base must have after it.
    needed: u64,
}

impl Tracker {
    /// `needed`: twice Activity's Main content budget.
    pub(super) fn new(needed: u64) -> Self {
        Self {
            slots: BTreeMap::new(),
            identity: Vec::new(),
            prompts: Vec::new(),
            snapshots: BTreeMap::new(),
            deltas: BTreeMap::new(),
            cost: 0,
            busy: false,
            candidates: VecDeque::new(),
            needed,
        }
    }

    /// The offsets a replay starting now would need first, in log order.
    fn carry(&self) -> Vec<u64> {
        let mut carry: Vec<u64> = self
            .slots
            .values()
            .chain(&self.identity)
            .chain(&self.prompts)
            .chain(self.snapshots.values())
            .chain(self.deltas.values().flatten())
            .copied()
            .collect();
        carry.sort_unstable();
        carry.dedup();
        carry
    }

    /// Take in a record a base names as carried: what it means for later
    /// carries, but not a turn of its own, and no cost.
    pub(super) fn seed(&mut self, at: u64, record: &Record) {
        self.note(at, record);
    }

    /// The record that starts at offset `at`, just written or read.
    pub(super) fn observe(&mut self, at: u64, record: &Record) {
        if matches!(record, Record::Prompt { .. }) && !self.busy {
            self.candidates.push_back(Candidate {
                at,
                cost: self.cost,
                carry: self.carry(),
            });
        }
        match record {
            Record::Prompt { .. } => self.busy = true,
            Record::TurnEnded { .. } | Record::Closed { .. } | Record::ConversationReset { .. } => {
                self.busy = false
            }
            Record::Activity {
                observation:
                    PersistedActivity::MainContent {
                        event: Execution::TurnEnded { .. },
                        ..
                    },
            } => self.busy = false,
            _ => {}
        }
        self.cost += main_cost(record);
        self.note(at, record);
    }

    fn note(&mut self, at: u64, record: &Record) {
        match classify(record) {
            Carry::Slot(slot) => {
                self.slots.insert(slot, at);
            }
            Carry::Identity => self.identity.push(at),
            Carry::Prompt => self.prompts.push(at),
            Carry::Reset => {
                self.prompts.clear();
                self.slots.insert(Slot::Latest("reset"), at);
            }
            Carry::Snapshot(kind) => {
                self.snapshots.insert(kind, at);
                self.deltas.remove(kind);
            }
            Carry::Delta(kind) => self.deltas.entry(kind).or_default().push(at),
            Carry::None => {}
        }
    }

    /// The newest turn start with enough Main cost after it to lie wholly
    /// before Activity's window, if one has been written.
    pub(super) fn base(&mut self) -> Option<Base> {
        let eligible = self
            .candidates
            .iter()
            .rposition(|candidate| self.cost - candidate.cost >= self.needed)?;
        // Older turn starts are further back still: never needed again.
        self.candidates.drain(..eligible);
        let chosen = &self.candidates[0];
        Some(Base {
            at: chosen.at,
            carry: chosen.carry.clone(),
        })
    }

    /// Main is between turns: what a replay base is, by construction.
    pub(super) fn idle(&mut self) {
        self.busy = false;
    }
}
