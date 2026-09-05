//! One Thread's attributed execution history and pending Decisions.
//!
//! Providers own wire attribution. This module owns presentation-independent
//! state; Main and each child cross the same transcript seam.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::{mpsc::Receiver, Arc};
use std::time::{Duration, Instant};

use crate::store::Provider;
use crate::transcript::{self, Input, Lexer, Transcript};
use crate::{Decision, SessionEvent, ToolResult, TurnOutcome};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AgentKey(String);

impl AgentKey {
    pub fn new(provider: Provider, root: &str, native: &str) -> Self {
        Self(serde_json::to_string(&(provider, root, native)).expect("string tuple serializes"))
    }
    pub fn from_stored(value: String) -> Self {
        Self(value)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Subject {
    Main,
    Subagent(AgentKey),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TranscriptCoverage {
    #[default]
    Unavailable,
    ToolActivity,
    /// Observed live messages; earlier/hidden history may be missing.
    Live,
    Partial,
    Complete,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AgentStatus {
    #[default]
    Unknown,
    Pending,
    Working,
    Waiting,
    Idle,
    Paused,
    Interrupted,
    Failed,
    Shutdown,
    NotFound,
    NotLoaded,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentInfo {
    pub key: AgentKey,
    pub parent: Option<Subject>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub kind: Option<String>,
    pub coverage: TranscriptCoverage,
}

impl AgentInfo {
    pub fn new(key: AgentKey) -> Self {
        Self {
            key,
            parent: None,
            name: None,
            description: None,
            kind: None,
            coverage: TranscriptCoverage::Unavailable,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActivityEvent {
    Discovered(AgentInfo),
    Status {
        key: AgentKey,
        state: AgentStatus,
    },
    Content {
        key: AgentKey,
        id: Option<String>,
        event: ExecutionEvent,
    },
    /// Settled history read while connected; never changes current runtime state.
    HistoryContent {
        key: AgentKey,
        id: Option<String>,
        event: ExecutionEvent,
    },
    Coverage {
        key: AgentKey,
        coverage: TranscriptCoverage,
    },
    /// Provider metadata disproved membership of this child in the owning tree.
    Detached {
        key: AgentKey,
    },
    /// Main content with identity; do not also emit its unscoped equivalent.
    MainContent {
        id: Option<String>,
        event: ExecutionEvent,
    },
    /// Autonomous provider turn; settles Main but cannot release operator input.
    BackgroundTurnEnded {
        outcome: TurnOutcome,
        cost_usd: Option<f64>,
    },
    /// None retains a visible connection-owned request with unresolved owner.
    Decision {
        subject: Option<Subject>,
        decision: Decision,
    },
    DecisionCancelled {
        id: String,
    },
    /// Explicit provider evidence that two provisional keys identify one child.
    Alias {
        from: AgentKey,
        to: AgentKey,
    },
}

/// Deliberately nonrecursive: execution cannot initialize or close a Session.
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionEvent {
    Progress {
        event: crate::progress::ProgressEvent,
    },
    ContentBoundary,
    ReasoningSummaryPart {
        item_id: String,
        summary_index: u64,
        text: String,
        snapshot: bool,
    },
    ToolOutputDelta {
        id: String,
        text: String,
    },
    TextDelta {
        text: String,
    },
    ThinkingDelta {
        text: String,
    },
    ReasoningSummaryDelta {
        text: String,
        summary_index: u64,
    },
    /// Completed block. Content.id identifies the delivery frame, not message.id.
    Text {
        text: String,
    },
    Thinking {
        text: String,
    },
    /// Authoritative completed item. Content.id matches the delta stream/item.
    TextSnapshot {
        text: String,
    },
    ThinkingSnapshot {
        text: String,
    },
    Prompt {
        text: String,
    },
    Notice {
        text: String,
    },
    ToolStarted {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolCompleted {
        id: String,
        output: String,
        is_error: bool,
        result: ToolResult,
    },
    TurnEnded {
        outcome: TurnOutcome,
        cost_usd: Option<f64>,
    },
    TokenUsage {
        total_tokens: u64,
        input_tokens: u64,
        cached_input_tokens: u64,
        output_tokens: u64,
        reasoning_output_tokens: u64,
        context_window: Option<u64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DecisionHandle {
    pub generation: u64,
    pub serial: u64,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingDecision {
    pub handle: DecisionHandle,
    pub subject: Option<Subject>,
    pub decision: Decision,
}

#[derive(Debug, Clone, Copy)]
pub enum ToolTiming {
    Running(Instant),
    Done(Duration),
}
impl ToolTiming {
    pub fn elapsed(&self) -> Duration {
        match self {
            Self::Running(at) => at.elapsed(),
            Self::Done(total) => *total,
        }
    }
}

pub enum ActivityInput {
    Main {
        input: Input,
        at: Instant,
    },
    Observe {
        generation: u64,
        event: ActivityEvent,
        at: Instant,
    },
    Replay(Input),
    ReplayEvent(ActivityEvent),
    Connect {
        generation: u64,
    },
    Disconnect,
    Answered {
        handle: DecisionHandle,
        allowed: bool,
        at: Instant,
    },
    RestoreTimings {
        subject: Subject,
        timings: HashMap<String, Duration>,
    },
    /// Keep this projection resident before replaying its stored content.
    Retain(Subject),
    /// Forget only the rendered projection; identity/runtime/Decisions survive.
    Evict(Subject),
    DrainHighlights,
}

#[derive(Debug, Clone, Copy)]
pub struct ActivityLimits {
    pub max_children: usize,
    pub blocks_per_subject: usize,
    pub content_bytes_per_subject: usize,
    pub dedup_ids_per_subject: usize,
}
impl Default for ActivityLimits {
    fn default() -> Self {
        Self {
            max_children: 128,
            blocks_per_subject: 2000,
            content_bytes_per_subject: 4 * 1024 * 1024,
            dedup_ids_per_subject: 8192,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ActivityUpdate {
    pub changed: Vec<Subject>,
    pub blocks: Vec<(Subject, transcript::Update)>,
    pub accepted: Vec<ActivityEvent>,
    pub main_turn_ended: bool,
    pub attention_changed: bool,
    pub order_changed: bool,
    pub rejected: bool,
    pub redirects: Vec<(AgentKey, AgentKey)>,
}

struct Record {
    sequence: u64,
    stream: Option<(String, bool)>, // item identity, thinking versus answer
    input: Input,
    bytes: usize,
}

struct SubjectState {
    transcript: Transcript,
    highlights: Receiver<Input>,
    records: VecDeque<Record>,
    bytes: usize,
    seen: BTreeSet<String>,
    seen_order: VecDeque<String>,
    timings: HashMap<String, ToolTiming>,
    status: AgentStatus,
    fresh: bool,
    busy: bool,
    coverage: TranscriptCoverage,
    last_outcome: Option<TurnOutcome>,
    revision: u64,
    retained: bool,
    truncated: bool,
}

impl SubjectState {
    fn new(limits: ActivityLimits) -> Self {
        let (lexer, highlights) = Lexer::new();
        Self {
            transcript: Transcript::with_capacity(Arc::new(lexer), limits.blocks_per_subject),
            highlights,
            records: VecDeque::new(),
            bytes: 0,
            seen: BTreeSet::new(),
            seen_order: VecDeque::new(),
            timings: HashMap::new(),
            status: AgentStatus::Unknown,
            fresh: false,
            busy: false,
            coverage: TranscriptCoverage::Unavailable,
            last_outcome: None,
            revision: 0,
            retained: true,
            truncated: false,
        }
    }

    fn remember(&mut self, id: String, limit: usize) -> bool {
        if !self.seen.insert(id.clone()) {
            return false;
        }
        self.seen_order.push_back(id);
        while self.seen_order.len() > limit.max(1) {
            if let Some(old) = self.seen_order.pop_front() {
                self.seen.remove(&old);
            }
        }
        true
    }

    fn bookkeeping(&mut self, input: &Input, at: Instant, live: bool) {
        match input {
            Input::Prompt(_)
            | Input::Event(SessionEvent::TextDelta { .. })
            | Input::Event(SessionEvent::ThinkingDelta { .. })
            | Input::Event(SessionEvent::ReasoningSummaryDelta { .. })
            | Input::Event(SessionEvent::ReasoningSummaryPart {
                snapshot: false, ..
            })
            | Input::Answered { .. } => {
                if live {
                    self.status = AgentStatus::Working;
                    self.busy = true;
                }
            }
            Input::Event(SessionEvent::ToolOutputDelta { id, .. }) => {
                if live && self.transcript.blocks().iter().any(|block| matches!(&block.body, transcript::Body::Tool(tool) if &tool.call == id && tool.state == transcript::ToolState::Running)) {
                    self.status = AgentStatus::Working;
                    self.busy = true;
                }
            }
            Input::Event(SessionEvent::Progress { event: crate::progress::ProgressEvent::Phase { phase, .. } }) => {
                if live {
                    self.status = if *phase == crate::progress::Phase::Waiting { AgentStatus::Waiting } else { AgentStatus::Working };
                    self.busy = true;
                }
            }
            Input::Event(SessionEvent::ToolStarted { id, .. }) => {
                if live {
                    self.status = AgentStatus::Working;
                    self.busy = true;
                    if self.retained {
                        self.timings
                            .entry(id.clone())
                            .or_insert(ToolTiming::Running(at));
                    }
                }
            }
            Input::Event(SessionEvent::ToolCompleted { id, .. }) => {
                if let Some(ToolTiming::Running(since)) = self.timings.get(id) {
                    self.timings.insert(
                        id.clone(),
                        ToolTiming::Done(at.saturating_duration_since(*since)),
                    );
                }
            }
            Input::Event(SessionEvent::TurnEnded { outcome, .. }) => {
                self.status = match outcome {
                    TurnOutcome::Completed => AgentStatus::Idle,
                    TurnOutcome::Interrupted => AgentStatus::Interrupted,
                    TurnOutcome::Error(_) => AgentStatus::Failed,
                };
                self.last_outcome = Some(outcome.clone());
                self.busy = false;
                self.stop_timings(at);
            }
            Input::Event(SessionEvent::DecisionRequested { .. }) => {
                if live {
                    self.status = AgentStatus::Waiting;
                }
            }
            Input::Event(SessionEvent::Closed { .. }) => {
                self.status = AgentStatus::Unknown;
                self.fresh = false;
                self.busy = false;
                self.stop_timings(at);
            }
            _ => {}
        }
        if live
            && !matches!(
                input,
                Input::Highlighted { .. }
                    | Input::Revived
                    | Input::Event(SessionEvent::Closed { .. })
            )
        {
            self.fresh = true;
        }
        if !live {
            self.fresh = false;
            self.busy = false;
        }
    }

    fn stop_timings(&mut self, at: Instant) {
        for timing in self.timings.values_mut() {
            if let ToolTiming::Running(since) = timing {
                *timing = ToolTiming::Done(at.saturating_duration_since(*since));
            }
        }
    }

    fn append(
        &mut self,
        input: Input,
        stream: Option<(String, bool)>,
        sequence: u64,
        at: Instant,
        live: bool,
        limits: ActivityLimits,
    ) -> transcript::Update {
        self.bookkeeping(&input, at, live);
        if !self.retained {
            return transcript::Update::default();
        }
        let mut update = self.transcript.apply(input.clone());
        if !update.evicted.is_empty() {
            self.truncated = true;
        }
        if matches!(input, Input::Highlighted { .. } | Input::Revived) {
            return update;
        }
        let bytes = input_bytes(&input);
        // A streamed item is retained as one growing record, not one allocation per token.
        let merged = self
            .records
            .back_mut()
            .filter(|last| last.stream == stream)
            .is_some_and(|last| {
                if append_delta(&mut last.input, &input) {
                    last.bytes += bytes;
                    true
                } else {
                    false
                }
            });
        if !merged {
            self.records.push_back(Record {
                sequence,
                stream,
                input,
                bytes,
            });
        }
        self.bytes += bytes;
        if self.trim(limits) {
            update = merge_updates(update, self.rebuild(limits));
        }
        self.prune_timings(limits);
        update
    }

    fn trim(&mut self, limits: ActivityLimits) -> bool {
        let max_records = limits.blocks_per_subject.saturating_mul(8).max(1);
        let mut trimmed = false;
        while self.records.len() > 1
            && (self.bytes > limits.content_bytes_per_subject || self.records.len() > max_records)
        {
            if let Some(old) = self.records.pop_front() {
                self.bytes = self.bytes.saturating_sub(old.bytes);
            }
            trimmed = true;
        }
        // Bound a single enormous text/tool event too. The original accepted event
        // still reaches the owning Store; only this rendering projection is shortened.
        if self.bytes > limits.content_bytes_per_subject {
            if let Some(last) = self.records.back_mut() {
                let preview = bounded_input(&last.input, limits.content_bytes_per_subject);
                last.bytes = input_bytes(&preview);
                last.input = preview;
                last.stream = None;
                self.bytes = last.bytes;
                trimmed = true;
            }
        }
        if trimmed {
            self.truncated = true;
            self.coverage = TranscriptCoverage::Partial;
        }
        trimmed
    }

    fn rebuild(&mut self, limits: ActivityLimits) -> transcript::Update {
        let runtime = self.transcript.runtime();
        let evicted = self
            .transcript
            .blocks()
            .iter()
            .map(|block| block.id)
            .collect();
        let (lexer, highlights) = Lexer::new();
        self.transcript = Transcript::with_capacity(Arc::new(lexer), limits.blocks_per_subject);
        self.highlights = highlights; // drops answers for the old content revision
        for record in &self.records {
            if !self
                .transcript
                .apply(record.input.clone())
                .evicted
                .is_empty()
            {
                self.truncated = true;
            }
        }
        self.transcript.restore_runtime(runtime);
        self.revision = self.revision.wrapping_add(1);
        transcript::Update {
            dirty: self
                .transcript
                .blocks()
                .iter()
                .map(|block| block.id)
                .collect(),
            evicted,
            boundary: None,
        }
    }

    fn snapshot(
        &mut self,
        id: Option<String>,
        text: String,
        thinking: bool,
        sequence: u64,
        at: Instant,
        live: bool,
        limits: ActivityLimits,
    ) -> transcript::Update {
        let input = if thinking {
            Input::Event(SessionEvent::ThinkingDelta { text })
        } else {
            Input::Event(SessionEvent::TextDelta { text })
        };
        if !self.retained {
            self.bookkeeping(&input, at, live);
            return transcript::Update::default();
        }
        let stream = id.map(|id| (id, thinking));
        if let Some(identity) = stream.as_ref() {
            if let Some(first) = self
                .records
                .iter()
                .position(|record| record.stream.as_ref() == Some(identity))
            {
                let old_sequence = self.records[first].sequence;
                let old_input = &self.records[first].input;
                if old_input == &input
                    && self
                        .records
                        .iter()
                        .filter(|record| record.stream.as_ref() == Some(identity))
                        .count()
                        == 1
                {
                    return transcript::Update::default();
                }
                let bytes = input_bytes(&input);
                let mut kept = VecDeque::new();
                for (index, record) in self.records.drain(..).enumerate() {
                    if index == first {
                        kept.push_back(Record {
                            sequence: old_sequence,
                            stream: stream.clone(),
                            input: input.clone(),
                            bytes,
                        });
                    }
                    if record.stream.as_ref() != Some(identity) {
                        kept.push_back(record);
                    }
                }
                self.records = kept;
                self.bytes = self.records.iter().map(|record| record.bytes).sum();
                self.bookkeeping(&input, at, live);
                self.trim(limits);
                return self.rebuild(limits);
            }
        } else {
            self.coverage = TranscriptCoverage::Partial;
        }
        self.append(input, stream, sequence, at, live, limits)
    }

    fn coverage(&self) -> TranscriptCoverage {
        if self.truncated && self.coverage != TranscriptCoverage::Unavailable {
            TranscriptCoverage::Partial
        } else {
            self.coverage
        }
    }

    fn prune_timings(&mut self, limits: ActivityLimits) {
        if self.timings.len() <= limits.blocks_per_subject.max(1) {
            return;
        }
        let visible: BTreeSet<&str> = self
            .transcript
            .blocks()
            .iter()
            .filter_map(|block| {
                if let transcript::Body::Tool(tool) = &block.body {
                    Some(tool.call.as_str())
                } else {
                    None
                }
            })
            .collect();
        self.timings.retain(|id, timing| {
            visible.contains(id.as_str()) || matches!(timing, ToolTiming::Running(_))
        });
    }
}

struct RuntimeState {
    transcript: transcript::Runtime,
    status: AgentStatus,
    fresh: bool,
    busy: bool,
    outcome: Option<TurnOutcome>,
    timings: HashMap<String, ToolTiming>,
}
impl RuntimeState {
    fn capture(state: &SubjectState) -> Self {
        Self {
            transcript: state.transcript.runtime(),
            status: state.status,
            fresh: state.fresh,
            busy: state.busy,
            outcome: state.last_outcome.clone(),
            timings: state.timings.clone(),
        }
    }
    fn restore(self, state: &mut SubjectState) {
        state.transcript.restore_runtime(self.transcript);
        state.status = self.status;
        state.fresh = self.fresh;
        state.busy = self.busy;
        state.last_outcome = self.outcome;
        state.timings = self.timings;
    }
}

struct AgentState {
    info: AgentInfo,
    state: SubjectState,
    discovered: u64,
    last_live: u64,
}

pub struct Activity {
    main: SubjectState,
    agents: BTreeMap<AgentKey, AgentState>,
    aliases: BTreeMap<AgentKey, AgentKey>,
    order: Vec<AgentKey>,
    pending: Vec<PendingDecision>,
    limits: ActivityLimits,
    generation: u64,
    next_decision: u64,
    sequence: u64,
    connected: bool,
    limited: bool,
    main_operator_turn: bool,
}

impl Default for Activity {
    fn default() -> Self {
        Self::new(ActivityLimits::default())
    }
}

impl Activity {
    pub fn new(limits: ActivityLimits) -> Self {
        Self {
            main: SubjectState::new(limits),
            agents: BTreeMap::new(),
            aliases: BTreeMap::new(),
            order: Vec::new(),
            pending: Vec::new(),
            limits,
            generation: 0,
            next_decision: 0,
            sequence: 0,
            connected: false,
            limited: false,
            main_operator_turn: false,
        }
    }

    pub fn view(&self) -> ActivityView<'_> {
        ActivityView { activity: self }
    }

    pub fn apply(&mut self, input: ActivityInput) -> ActivityUpdate {
        self.sequence = self.sequence.wrapping_add(1);
        let old_order = self.order.clone();
        let mut update = match input {
            ActivityInput::Main { input, at } => self.main_input(input, at, true),
            ActivityInput::Observe {
                generation,
                event,
                at,
            } => {
                if !self.connected || generation != self.generation {
                    return ActivityUpdate {
                        rejected: true,
                        ..ActivityUpdate::default()
                    };
                }
                self.event(event, at, true)
            }
            ActivityInput::Replay(input) => self.main_input(input, Instant::now(), false),
            ActivityInput::ReplayEvent(event) => self.replay_event(event),
            ActivityInput::Connect { generation } => {
                self.generation = generation;
                self.connected = true;
                self.invalidate()
            }
            ActivityInput::Disconnect => {
                self.connected = false;
                self.invalidate()
            }
            ActivityInput::Answered {
                handle,
                allowed,
                at,
            } => self.answered(handle, allowed, at),
            ActivityInput::RestoreTimings { subject, timings } => {
                let connected = self.connected;
                if let Some(state) = self.state_mut(&subject) {
                    for (id, elapsed) in timings {
                        if (!connected || !state.timings.contains_key(&id))
                            && !matches!(state.timings.get(&id), Some(ToolTiming::Running(_)))
                        {
                            state.timings.insert(id, ToolTiming::Done(elapsed));
                        }
                    }
                    ActivityUpdate {
                        changed: vec![subject],
                        ..ActivityUpdate::default()
                    }
                } else {
                    ActivityUpdate {
                        rejected: true,
                        ..ActivityUpdate::default()
                    }
                }
            }
            ActivityInput::Retain(subject) => self.retain(subject),
            ActivityInput::Evict(subject) => self.evict(subject),
            ActivityInput::DrainHighlights => self.drain_highlights(),
        };
        self.reorder();
        update.order_changed |= old_order != self.order;
        update
    }

    fn replay_event(&mut self, event: ActivityEvent) -> ActivityUpdate {
        let subject = match &event {
            ActivityEvent::Content { key, .. }
            | ActivityEvent::HistoryContent { key, .. }
            | ActivityEvent::Status { key, .. } => Some(Subject::Subagent(self.resolve(key))),
            ActivityEvent::MainContent { .. } | ActivityEvent::BackgroundTurnEnded { .. } => {
                Some(Subject::Main)
            }
            _ => None,
        };
        let previous = if self.connected {
            subject
                .as_ref()
                .and_then(|subject| self.state_mut(subject))
                .map(|state| RuntimeState::capture(state))
        } else {
            None
        };
        let update = self.event(event, Instant::now(), false);
        if let (Some(previous), Some(subject)) = (previous, subject) {
            if let Some(state) = self.state_mut(&subject) {
                previous.restore(state);
            }
        }
        update
    }

    fn retain(&mut self, subject: Subject) -> ActivityUpdate {
        let Subject::Subagent(key) = self.resolve_subject(&subject) else {
            return ActivityUpdate::default();
        };
        self.ensure_agent(key.clone());
        if self.agents[&key].state.retained {
            return ActivityUpdate::default();
        }
        let mut update = ActivityUpdate::default();
        // The selected view can always be retained, even with a zero configured
        // background-cache budget. Decisions are stored outside these caches.
        if let Some(evict) = self
            .agents
            .iter()
            .filter(|(candidate, agent)| **candidate != key && agent.state.retained)
            .min_by_key(|(_, agent)| {
                (
                    agent.state.fresh && agent.state.status == AgentStatus::Working,
                    agent.discovered,
                )
            })
            .map(|(candidate, _)| candidate.clone())
        {
            update = self.evict(Subject::Subagent(evict));
        }
        let agent = self.agents.get_mut(&key).expect("retained child exists");
        agent.state.retained = true;
        agent.state.truncated = false;
        agent.state.seen.clear();
        agent.state.seen_order.clear();
        agent.state.coverage = TranscriptCoverage::Partial;
        update.changed.push(Subject::Subagent(key));
        update
    }

    fn evict(&mut self, subject: Subject) -> ActivityUpdate {
        let Subject::Subagent(key) = self.resolve_subject(&subject) else {
            return ActivityUpdate {
                rejected: true,
                ..ActivityUpdate::default()
            };
        };
        let Some(agent) = self.agents.get_mut(&key) else {
            return ActivityUpdate {
                rejected: true,
                ..ActivityUpdate::default()
            };
        };
        agent.state.records.clear();
        agent.state.bytes = 0;
        agent.state.seen.clear();
        agent.state.seen_order.clear();
        agent.state.retained = false;
        agent.state.truncated = true;
        agent.state.coverage = TranscriptCoverage::Partial;
        let blocks = agent.state.rebuild(self.limits);
        let subject = Subject::Subagent(key);
        ActivityUpdate {
            changed: vec![subject.clone()],
            blocks: vec![(subject, blocks)],
            ..ActivityUpdate::default()
        }
    }

    fn invalidate(&mut self) -> ActivityUpdate {
        let mut changed = vec![Subject::Main];
        let at = Instant::now();
        self.main_operator_turn = false;
        self.main.fresh = false;
        self.main.busy = false;
        self.main.stop_timings(at);
        let mut blocks = vec![(Subject::Main, self.main.transcript.clear_activity())];
        for (key, agent) in &mut self.agents {
            agent.state.fresh = false;
            agent.state.busy = false;
            agent.state.stop_timings(at);
            blocks.push((
                Subject::Subagent(key.clone()),
                agent.state.transcript.clear_activity(),
            ));
            changed.push(Subject::Subagent(key.clone()));
        }
        let attention_changed = !self.pending.is_empty();
        self.pending.clear();
        ActivityUpdate {
            changed,
            blocks,
            attention_changed,
            ..ActivityUpdate::default()
        }
    }

    fn main_input(&mut self, input: Input, at: Instant, live: bool) -> ActivityUpdate {
        if let Input::Event(SessionEvent::DecisionRequested { decision }) = input {
            return self.event(
                ActivityEvent::Decision {
                    subject: Some(Subject::Main),
                    decision,
                },
                at,
                live,
            );
        }
        if let Input::Event(SessionEvent::Activity(event)) = input {
            return self.event(event, at, live);
        }
        let prompt = matches!(input, Input::Prompt(_));
        let previous_busy = self.main.busy;
        if prompt && live {
            self.main_operator_turn = true;
        }
        let ended = matches!(input, Input::Event(SessionEvent::TurnEnded { .. }));
        let closed = matches!(input, Input::Event(SessionEvent::Closed { .. }));
        let mut update = ActivityUpdate {
            changed: vec![Subject::Main],
            main_turn_ended: live && ended,
            ..ActivityUpdate::default()
        };
        if live && ended {
            self.main_operator_turn = false;
            self.remove_subject_decisions(&Subject::Main, &mut update);
        }
        if live && closed {
            let invalid = self.invalidate();
            update.changed.extend(invalid.changed);
            update.blocks.extend(invalid.blocks);
            update.attention_changed |= invalid.attention_changed;
            self.connected = false;
        }
        let blocks = self
            .main
            .append(input, None, self.sequence, at, live, self.limits);
        if prompt {
            self.main.busy = previous_busy;
        }
        update.blocks.push((Subject::Main, blocks));
        update
    }

    fn event(&mut self, event: ActivityEvent, at: Instant, live: bool) -> ActivityUpdate {
        let mut update = ActivityUpdate::default();
        match &event {
            ActivityEvent::Discovered(info) => {
                let key = self.resolve(&info.key);
                if self.would_cycle(&key, info.parent.as_ref()) {
                    update.rejected = true;
                    return update;
                }
                if !self.ensure_agent(key.clone()) {
                    update.rejected = true;
                    return update;
                }
                let agent = self.agents.get_mut(&key).expect("ensured child");
                if let Some(parent) = &info.parent {
                    agent.info.parent = Some(parent.clone());
                }
                if info.name.is_some() {
                    agent.info.name.clone_from(&info.name);
                }
                if info.description.is_some() {
                    agent.info.description.clone_from(&info.description);
                }
                if info.kind.is_some() {
                    agent.info.kind.clone_from(&info.kind);
                }
                if info.coverage != TranscriptCoverage::Unavailable {
                    agent.info.coverage = info.coverage;
                    agent.state.coverage = info.coverage;
                }
                update.changed.push(Subject::Subagent(key));
            }
            ActivityEvent::Status { key, state } => {
                let key = self.resolve(key);
                if !self.ensure_agent(key.clone()) {
                    update.rejected = true;
                    return update;
                }
                let subject = Subject::Subagent(key.clone());
                let agent = self.agents.get_mut(&key).expect("ensured child");
                let previous = agent.state.status;
                agent.state.status = *state;
                agent.state.fresh = live && *state != AgentStatus::NotLoaded;
                agent.state.busy =
                    live && matches!(state, AgentStatus::Working | AgentStatus::Pending);
                if live
                    && matches!(state, AgentStatus::Working | AgentStatus::Pending)
                    && agent.state.transcript.progress().phase.is_none()
                {
                    agent
                        .state
                        .transcript
                        .apply(Input::Event(SessionEvent::Progress {
                            event: crate::progress::ProgressEvent::Phase {
                                phase: crate::progress::Phase::Working,
                                detail: String::new(),
                            },
                        }));
                }
                if previous != *state {
                    let outcome = match state {
                        AgentStatus::Idle => Some(TurnOutcome::Completed),
                        AgentStatus::Interrupted | AgentStatus::Shutdown => {
                            Some(TurnOutcome::Interrupted)
                        }
                        AgentStatus::Failed => Some(TurnOutcome::Error("Subagent failed".into())),
                        _ => None,
                    };
                    if let Some(outcome) = outcome {
                        let blocks = agent.state.append(
                            Input::Event(SessionEvent::TurnEnded {
                                outcome,
                                cost_usd: None,
                            }),
                            None,
                            self.sequence,
                            at,
                            live,
                            self.limits,
                        );
                        // Keep provider distinctions after the Transcript consumes the outcome.
                        agent.state.status = *state;
                        update.blocks.push((subject.clone(), blocks));
                    }
                }
                if !matches!(
                    state,
                    AgentStatus::Working | AgentStatus::Pending | AgentStatus::Waiting
                ) {
                    agent.state.stop_timings(at);
                    let blocks = agent.state.transcript.clear_activity();
                    if !blocks.dirty.is_empty() {
                        update.blocks.push((subject.clone(), blocks));
                    }
                }
                if live
                    && matches!(
                        state,
                        AgentStatus::Idle
                            | AgentStatus::Interrupted
                            | AgentStatus::Failed
                            | AgentStatus::Shutdown
                            | AgentStatus::NotFound
                    )
                {
                    self.remove_subject_decisions(&subject, &mut update);
                }
                update.changed.push(subject);
            }
            ActivityEvent::HistoryContent { key, id, event } => {
                let key = self.resolve(key);
                self.ensure_agent(key.clone());
                let old = RuntimeState::capture(&self.agents[&key].state);
                update = self.execution(
                    Subject::Subagent(key.clone()),
                    id.clone(),
                    event.clone(),
                    at,
                    false,
                    false,
                );
                old.restore(&mut self.agents.get_mut(&key).expect("ensured child").state);
            }
            ActivityEvent::Coverage { key, coverage } => {
                let key = self.resolve(key);
                self.ensure_agent(key.clone());
                let agent = self.agents.get_mut(&key).expect("ensured child");
                agent.info.coverage = *coverage;
                agent.state.coverage = *coverage;
                update.changed.push(Subject::Subagent(key));
            }
            ActivityEvent::Detached { key } => {
                let key = self.resolve(key);
                if let Some(agent) = self.agents.get_mut(&key) {
                    agent.info.parent = None;
                    agent.state.fresh = false;
                    agent.state.busy = false;
                    agent.state.coverage = TranscriptCoverage::Unavailable;
                    let blocks = agent.state.transcript.clear_activity();
                    update.blocks.push((Subject::Subagent(key.clone()), blocks));
                    update.changed.push(Subject::Subagent(key));
                }
            }
            ActivityEvent::Content { key, id, event } => {
                let key = self.resolve(key);
                if !self.ensure_agent(key.clone()) {
                    update.rejected = true;
                    return update;
                }
                update = self.execution(
                    Subject::Subagent(key),
                    id.clone(),
                    event.clone(),
                    at,
                    live,
                    true,
                );
            }
            ActivityEvent::MainContent { id, event } => {
                update = self.execution(Subject::Main, id.clone(), event.clone(), at, live, true);
            }
            ActivityEvent::BackgroundTurnEnded { outcome, cost_usd } => {
                let preserve_foreground = self.main_operator_turn
                    || self
                        .pending
                        .iter()
                        .any(|pending| pending.subject.as_ref() == Some(&Subject::Main));
                let runtime = preserve_foreground.then(|| RuntimeState::capture(&self.main));
                let pending = self.pending.clone();
                update = self.execution(
                    Subject::Main,
                    None,
                    ExecutionEvent::TurnEnded {
                        outcome: outcome.clone(),
                        cost_usd: *cost_usd,
                    },
                    at,
                    live,
                    false,
                );
                self.pending = pending;
                if let Some(runtime) = runtime {
                    runtime.restore(&mut self.main);
                }
                update.attention_changed = false;
            }
            ActivityEvent::Decision { subject, decision } => {
                if !live {
                    return update;
                }
                let subject = subject
                    .as_ref()
                    .map(|subject| self.resolve_subject(subject));
                if let Some(index) = self
                    .pending
                    .iter()
                    .position(|pending| pending.decision.id == decision.id)
                {
                    let existing = &mut self.pending[index];
                    if existing.subject.is_none() && subject.is_some() {
                        existing.subject = subject.clone();
                    }
                    existing.decision = decision.clone();
                    let owner = existing.subject.clone();
                    update.attention_changed = true;
                    if let Some(owner) = owner {
                        if let Subject::Subagent(key) = &owner {
                            self.ensure_agent(key.clone());
                        }
                        self.refresh_waiting(&owner);
                        if let Subject::Subagent(key) = &owner {
                            self.agents
                                .get_mut(key)
                                .expect("decision owner exists")
                                .last_live = self.sequence;
                        }
                        if let Some(state) = self.state_mut(&owner) {
                            state.fresh = true;
                        }
                        update.changed.push(owner);
                    }
                    return update;
                }
                self.next_decision = self.next_decision.wrapping_add(1);
                self.pending.push(PendingDecision {
                    handle: DecisionHandle {
                        generation: self.generation,
                        serial: self.next_decision,
                        request_id: decision.id.clone(),
                    },
                    subject: subject.clone(),
                    decision: decision.clone(),
                });
                if let Some(subject) = subject {
                    if let Subject::Subagent(key) = &subject {
                        self.ensure_agent(key.clone());
                    }
                    let sequence = self.sequence;
                    let limits = self.limits;
                    if let Some(state) = self.state_mut(&subject) {
                        let blocks = state.append(
                            Input::Event(SessionEvent::DecisionRequested {
                                decision: decision.clone(),
                            }),
                            None,
                            sequence,
                            at,
                            true,
                            limits,
                        );
                        update.blocks.push((subject.clone(), blocks));
                    }
                    update.changed.push(subject);
                }
                update.attention_changed = true;
            }
            ActivityEvent::DecisionCancelled { id } => {
                if live {
                    let owners: Vec<_> = self
                        .pending
                        .iter()
                        .filter(|pending| &pending.decision.id == id)
                        .filter_map(|pending| pending.subject.clone())
                        .collect();
                    let before = self.pending.len();
                    self.pending.retain(|pending| &pending.decision.id != id);
                    update.attention_changed = before != self.pending.len();
                    for subject in owners {
                        if !self
                            .pending
                            .iter()
                            .any(|pending| pending.subject.as_ref() == Some(&subject))
                        {
                            if let Some(state) = self.state_mut(&subject) {
                                if state.status == AgentStatus::Waiting {
                                    state.status = AgentStatus::Unknown;
                                }
                                state.transcript.set_attention(false, state.busy);
                            }
                        }
                        update.changed.push(subject);
                    }
                }
            }
            ActivityEvent::Alias { from, to } => update = self.alias(from, to),
        }
        if live && !update.rejected {
            let key = match &event {
                ActivityEvent::Content { key, .. } | ActivityEvent::Status { key, .. } => Some(key),
                ActivityEvent::Decision {
                    subject: Some(Subject::Subagent(key)),
                    ..
                } => Some(key),
                _ => None,
            };
            if let Some(key) = key {
                let key = self.resolve(key);
                if let Some(agent) = self.agents.get_mut(&key) {
                    agent.last_live = self.sequence;
                }
            }
            update.accepted.push(event);
        }
        update
    }

    fn execution(
        &mut self,
        subject: Subject,
        id: Option<String>,
        event: ExecutionEvent,
        at: Instant,
        live: bool,
        root_signal: bool,
    ) -> ActivityUpdate {
        let sequence = self.sequence;
        let limits = self.limits;
        let ended = matches!(event, ExecutionEvent::TurnEnded { .. });
        let Some(state) = self.state_mut(&subject) else {
            return ActivityUpdate {
                rejected: true,
                ..ActivityUpdate::default()
            };
        };
        if let Some(delivery) = delivery_id(&event, id.as_deref()) {
            if !state.remember(delivery, limits.dedup_ids_per_subject) {
                return ActivityUpdate {
                    rejected: true,
                    ..ActivityUpdate::default()
                };
            }
        }
        if state.coverage == TranscriptCoverage::Unavailable {
            state.coverage = TranscriptCoverage::Live;
        }
        let blocks = match event {
            ExecutionEvent::TextSnapshot { text } => {
                state.snapshot(id, text, false, sequence, at, live, limits)
            }
            ExecutionEvent::ThinkingSnapshot { text } => {
                state.snapshot(id, text, true, sequence, at, live, limits)
            }
            event => {
                let stream = match &event {
                    ExecutionEvent::TextDelta { .. } => id.map(|id| (id, false)),
                    ExecutionEvent::ThinkingDelta { .. }
                    | ExecutionEvent::ReasoningSummaryDelta { .. } => id.map(|id| (id, true)),
                    _ => None,
                };
                state.append(event.into_input(), stream, sequence, at, live, limits)
            }
        };
        let mut update = ActivityUpdate {
            changed: vec![subject.clone()],
            blocks: vec![(subject.clone(), blocks)],
            main_turn_ended: live && root_signal && ended && subject == Subject::Main,
            ..ActivityUpdate::default()
        };
        if live && ended {
            if root_signal && subject == Subject::Main {
                self.main_operator_turn = false;
            }
            self.remove_subject_decisions(&subject, &mut update);
        }
        self.refresh_waiting(&subject);
        update
    }

    fn ensure_agent(&mut self, key: AgentKey) -> bool {
        if self.agents.contains_key(&key) {
            return true;
        }
        let retained = self
            .agents
            .values()
            .filter(|agent| agent.state.retained)
            .count()
            < self.limits.max_children;
        let mut state = SubjectState::new(self.limits);
        state.retained = retained;
        if !retained {
            state.truncated = true;
            state.coverage = TranscriptCoverage::Partial;
            self.limited = true;
        }
        self.agents.insert(
            key.clone(),
            AgentState {
                info: AgentInfo::new(key),
                state,
                discovered: self.sequence,
                last_live: 0,
            },
        );
        true
    }

    fn remove_subject_decisions(&mut self, subject: &Subject, update: &mut ActivityUpdate) {
        let before = self.pending.len();
        self.pending
            .retain(|pending| pending.subject.as_ref() != Some(subject));
        update.attention_changed |= before != self.pending.len();
    }

    fn answered(&mut self, handle: DecisionHandle, allowed: bool, at: Instant) -> ActivityUpdate {
        if !self.connected || handle.generation != self.generation {
            return ActivityUpdate {
                rejected: true,
                ..ActivityUpdate::default()
            };
        }
        let Some(index) = self
            .pending
            .iter()
            .position(|pending| pending.handle == handle)
        else {
            return ActivityUpdate {
                rejected: true,
                ..ActivityUpdate::default()
            };
        };
        let pending = self.pending.remove(index);
        let mut update = ActivityUpdate {
            attention_changed: true,
            ..ActivityUpdate::default()
        };
        if let Some(subject) = pending.subject {
            let sequence = self.sequence;
            let limits = self.limits;
            if let Some(state) = self.state_mut(&subject) {
                let blocks = state.append(
                    Input::Answered {
                        allowed,
                        tool_name: pending.decision.tool_name,
                    },
                    None,
                    sequence,
                    at,
                    true,
                    limits,
                );
                update.blocks.push((subject.clone(), blocks));
            }
            self.refresh_waiting(&subject);
            update.changed.push(subject);
        }
        update
    }

    fn refresh_waiting(&mut self, subject: &Subject) {
        if self
            .pending
            .iter()
            .any(|pending| pending.subject.as_ref() == Some(subject))
        {
            if let Some(state) = self.state_mut(subject) {
                state.status = AgentStatus::Waiting;
                state.transcript.set_attention(true, state.busy);
            }
        }
    }

    fn state_mut(&mut self, subject: &Subject) -> Option<&mut SubjectState> {
        match subject {
            Subject::Main => Some(&mut self.main),
            Subject::Subagent(key) => {
                let key = self.resolve(key);
                self.agents.get_mut(&key).map(|agent| &mut agent.state)
            }
        }
    }

    fn resolve(&self, key: &AgentKey) -> AgentKey {
        let mut key = key.clone();
        for _ in 0..=self.aliases.len() {
            let Some(next) = self.aliases.get(&key) else {
                break;
            };
            key = next.clone();
        }
        key
    }
    fn resolve_subject(&self, subject: &Subject) -> Subject {
        match subject {
            Subject::Main => Subject::Main,
            Subject::Subagent(key) => Subject::Subagent(self.resolve(key)),
        }
    }

    fn would_cycle(&self, key: &AgentKey, parent: Option<&Subject>) -> bool {
        let mut parent = parent.cloned();
        let mut visited = BTreeSet::new();
        while let Some(Subject::Subagent(candidate)) = parent {
            let candidate = self.resolve(&candidate);
            if &candidate == key || !visited.insert(candidate.clone()) {
                return true;
            }
            parent = self
                .agents
                .get(&candidate)
                .and_then(|agent| agent.info.parent.clone());
        }
        false
    }

    fn belongs(&self, key: &AgentKey) -> bool {
        let mut parent = self
            .agents
            .get(key)
            .and_then(|agent| agent.info.parent.clone());
        for _ in 0..=self.agents.len() {
            match parent {
                Some(Subject::Main) => return true,
                Some(Subject::Subagent(ref candidate)) => {
                    parent = self
                        .agents
                        .get(&self.resolve(candidate))
                        .and_then(|agent| agent.info.parent.clone());
                }
                None => return false,
            }
        }
        false
    }

    fn reorder(&mut self) {
        let mut keys: Vec<_> = self
            .agents
            .keys()
            .filter(|key| self.belongs(key))
            .cloned()
            .collect();
        keys.sort_by_key(|key| {
            let agent = &self.agents[key];
            (
                !(agent.state.fresh && agent.state.status == AgentStatus::Working),
                agent.discovered,
            )
        });
        self.order = keys;
    }

    fn alias(&mut self, from: &AgentKey, to: &AgentKey) -> ActivityUpdate {
        let from = self.resolve(from);
        let to = self.resolve(to);
        if from == to {
            return ActivityUpdate::default();
        }
        // An identity join cannot equate an ancestor with its own descendant.
        // Retain the last coherent tree when provider evidence is contradictory.
        if self.would_cycle(&from, Some(&Subject::Subagent(to.clone())))
            || self.would_cycle(&to, Some(&Subject::Subagent(from.clone())))
        {
            return ActivityUpdate {
                rejected: true,
                ..ActivityUpdate::default()
            };
        }
        let mut update = ActivityUpdate::default();
        if let Some(mut source) = self.agents.remove(&from) {
            if let Some(target) = self.agents.get_mut(&to) {
                target.discovered = target.discovered.min(source.discovered);
                let source_is_newer = source.last_live > target.last_live;
                let runtime = source_is_newer.then(|| RuntimeState::capture(&source.state));
                target.last_live = target.last_live.max(source.last_live);
                target.state.retained |= source.state.retained;
                target.state.truncated |= source.state.truncated;
                if target.state.coverage == TranscriptCoverage::Unavailable {
                    target.state.coverage = source.state.coverage;
                }
                for delivery in source.state.seen_order.drain(..) {
                    target
                        .state
                        .remember(delivery, self.limits.dedup_ids_per_subject);
                }
                if target.info.name.is_none() {
                    target.info.name = source.info.name;
                }
                if target.info.description.is_none() {
                    target.info.description = source.info.description;
                }
                if target.info.kind.is_none() {
                    target.info.kind = source.info.kind;
                }
                if target.info.parent.is_none() {
                    target.info.parent = source.info.parent;
                }
                target.state.records.append(&mut source.state.records);
                let mut records: Vec<_> = target.state.records.drain(..).collect();
                records.sort_by_key(|record| record.sequence);
                target.state.records = records.into();
                target.state.bytes = target.state.records.iter().map(|record| record.bytes).sum();
                target.state.trim(self.limits);
                let blocks = target.state.rebuild(self.limits);
                for (id, timing) in source.state.timings {
                    if source_is_newer || !target.state.timings.contains_key(&id) {
                        target.state.timings.insert(id, timing);
                    }
                }
                if let Some(runtime) = runtime {
                    // Keep timings from both provisional observations while choosing
                    // the latest provider runtime state, independent of key ordering.
                    let timings = std::mem::take(&mut target.state.timings);
                    runtime.restore(&mut target.state);
                    target.state.timings = timings;
                }
                update.blocks.push((Subject::Subagent(to.clone()), blocks));
            } else {
                source.info.key = to.clone();
                self.agents.insert(to.clone(), source);
            }
        }
        self.aliases.insert(from.clone(), to.clone());
        for agent in self.agents.values_mut() {
            if agent.info.parent.as_ref() == Some(&Subject::Subagent(from.clone())) {
                agent.info.parent = Some(Subject::Subagent(to.clone()));
            }
        }
        for pending in &mut self.pending {
            if pending.subject.as_ref() == Some(&Subject::Subagent(from.clone())) {
                pending.subject = Some(Subject::Subagent(to.clone()));
            }
        }
        self.refresh_waiting(&Subject::Subagent(to.clone()));
        update.changed.push(Subject::Subagent(to.clone()));
        update.redirects.push((from, to));
        update.attention_changed = true;
        update
    }

    fn drain_highlights(&mut self) -> ActivityUpdate {
        fn drain(state: &mut SubjectState) -> transcript::Update {
            let mut update = transcript::Update::default();
            while let Ok(answer) = state.highlights.try_recv() {
                update = merge_updates(update, state.transcript.apply(answer));
            }
            update
        }
        let mut update = ActivityUpdate::default();
        let main = drain(&mut self.main);
        if !main.dirty.is_empty() {
            update.changed.push(Subject::Main);
            update.blocks.push((Subject::Main, main));
        }
        for (key, agent) in &mut self.agents {
            let blocks = drain(&mut agent.state);
            if !blocks.dirty.is_empty() {
                let subject = Subject::Subagent(key.clone());
                update.changed.push(subject.clone());
                update.blocks.push((subject, blocks));
            }
        }
        update
    }
}

#[derive(Clone, Copy)]
pub struct ActivityView<'a> {
    activity: &'a Activity,
}
impl<'a> ActivityView<'a> {
    /// Resolve durable provider aliases without exposing their representation.
    pub fn canonical_subject(self, subject: &Subject) -> Subject {
        self.activity.resolve_subject(subject)
    }
    pub fn main(self) -> SubjectView<'a> {
        SubjectView {
            state: &self.activity.main,
        }
    }
    pub fn subject(self, subject: &Subject) -> Option<SubjectView<'a>> {
        match subject {
            Subject::Main => Some(self.main()),
            Subject::Subagent(key) => {
                self.activity
                    .agents
                    .get(&self.activity.resolve(key))
                    .map(|agent| SubjectView {
                        state: &agent.state,
                    })
            }
        }
    }
    pub fn children(self) -> Vec<AgentView<'a>> {
        self.activity
            .order
            .iter()
            .map(|key| AgentView {
                agent: &self.activity.agents[key],
            })
            .collect()
    }
    pub fn pending_decisions(self) -> &'a [PendingDecision] {
        &self.activity.pending
    }
    pub fn decisions(self) -> &'a [PendingDecision] {
        self.pending_decisions()
    }
    pub fn generation(self) -> u64 {
        self.activity.generation
    }
    pub fn limited(self) -> bool {
        self.activity.limited
    }
    pub fn working_descendants(self) -> usize {
        self.activity
            .order
            .iter()
            .filter(|key| {
                let state = &self.activity.agents[*key].state;
                state.fresh && state.status == AgentStatus::Working
            })
            .count()
    }
}

#[derive(Clone, Copy)]
pub struct SubjectView<'a> {
    state: &'a SubjectState,
}
impl<'a> SubjectView<'a> {
    pub fn transcript(self) -> &'a Transcript {
        &self.state.transcript
    }
    pub fn timings(self) -> &'a HashMap<String, ToolTiming> {
        &self.state.timings
    }
    pub fn tool_timings(self) -> &'a HashMap<String, ToolTiming> {
        self.timings()
    }
    pub fn status(self) -> AgentStatus {
        self.state.status
    }
    pub fn fresh(self) -> bool {
        self.state.fresh
    }
    pub fn busy(self) -> bool {
        self.state.busy
    }
    pub fn coverage(self) -> TranscriptCoverage {
        self.state.coverage()
    }
    pub fn last_outcome(self) -> Option<&'a TurnOutcome> {
        self.state.last_outcome.as_ref()
    }
    pub fn revision(self) -> u64 {
        self.state.revision
    }
    pub fn retained(self) -> bool {
        self.state.retained
    }
}

#[derive(Clone, Copy)]
pub struct AgentView<'a> {
    agent: &'a AgentState,
}
impl<'a> AgentView<'a> {
    pub fn info(self) -> &'a AgentInfo {
        &self.agent.info
    }
    pub fn key(self) -> &'a AgentKey {
        &self.agent.info.key
    }
    pub fn subject(self) -> Subject {
        Subject::Subagent(self.agent.info.key.clone())
    }
    pub fn transcript(self) -> &'a Transcript {
        &self.agent.state.transcript
    }
    pub fn timings(self) -> &'a HashMap<String, ToolTiming> {
        &self.agent.state.timings
    }
    pub fn tool_timings(self) -> &'a HashMap<String, ToolTiming> {
        self.timings()
    }
    pub fn status(self) -> AgentStatus {
        self.agent.state.status
    }
    pub fn fresh(self) -> bool {
        self.agent.state.fresh
    }
    pub fn coverage(self) -> TranscriptCoverage {
        self.agent.state.coverage()
    }
    pub fn retained(self) -> bool {
        self.agent.state.retained
    }
    pub fn last_outcome(self) -> Option<&'a TurnOutcome> {
        self.agent.state.last_outcome.as_ref()
    }
}

impl ExecutionEvent {
    pub fn from_session(event: &SessionEvent) -> Option<Self> {
        Some(match event {
            SessionEvent::Progress { event } => Self::Progress {
                event: event.clone(),
            },
            SessionEvent::ContentBoundary => Self::ContentBoundary,
            SessionEvent::ReasoningSummaryPart {
                item_id,
                summary_index,
                text,
                snapshot,
            } => Self::ReasoningSummaryPart {
                item_id: item_id.clone(),
                summary_index: *summary_index,
                text: text.clone(),
                snapshot: *snapshot,
            },
            SessionEvent::ToolOutputDelta { id, text } => Self::ToolOutputDelta {
                id: id.clone(),
                text: text.clone(),
            },
            SessionEvent::TextDelta { text } => Self::TextDelta { text: text.clone() },
            SessionEvent::ThinkingDelta { text } => Self::ThinkingDelta { text: text.clone() },
            SessionEvent::ReasoningSummaryDelta {
                text,
                summary_index,
            } => Self::ReasoningSummaryDelta {
                text: text.clone(),
                summary_index: *summary_index,
            },
            SessionEvent::ToolStarted { id, name, input } => Self::ToolStarted {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            },
            SessionEvent::ToolCompleted {
                id,
                output,
                is_error,
                result,
            } => Self::ToolCompleted {
                id: id.clone(),
                output: output.clone(),
                is_error: *is_error,
                result: result.clone(),
            },
            SessionEvent::TurnEnded { outcome, cost_usd } => Self::TurnEnded {
                outcome: outcome.clone(),
                cost_usd: *cost_usd,
            },
            SessionEvent::TokenUsage {
                total_tokens,
                input_tokens,
                cached_input_tokens,
                output_tokens,
                reasoning_output_tokens,
                context_window,
            } => Self::TokenUsage {
                total_tokens: *total_tokens,
                input_tokens: *input_tokens,
                cached_input_tokens: *cached_input_tokens,
                output_tokens: *output_tokens,
                reasoning_output_tokens: *reasoning_output_tokens,
                context_window: *context_window,
            },
            _ => return None,
        })
    }

    fn into_input(self) -> Input {
        Input::Event(match self {
            Self::Progress { event } => SessionEvent::Progress { event },
            Self::ContentBoundary => SessionEvent::ContentBoundary,
            Self::ReasoningSummaryPart {
                item_id,
                summary_index,
                text,
                snapshot,
            } => SessionEvent::ReasoningSummaryPart {
                item_id,
                summary_index,
                text,
                snapshot,
            },
            Self::ToolOutputDelta { id, text } => SessionEvent::ToolOutputDelta { id, text },
            Self::TextDelta { text } | Self::TextSnapshot { text } => {
                SessionEvent::TextDelta { text }
            }
            Self::Text { text } => SessionEvent::TextDelta {
                text: format!("{text}\n\n"),
            },
            Self::ThinkingDelta { text }
            | Self::Thinking { text }
            | Self::ThinkingSnapshot { text } => SessionEvent::ThinkingDelta { text },
            Self::ReasoningSummaryDelta {
                text,
                summary_index,
            } => SessionEvent::ReasoningSummaryDelta {
                text,
                summary_index,
            },
            Self::Prompt { text } => return Input::Prompt(text),
            Self::Notice { text } => return Input::Notice(text),
            Self::ToolStarted { id, name, input } => SessionEvent::ToolStarted { id, name, input },
            Self::ToolCompleted {
                id,
                output,
                is_error,
                result,
            } => SessionEvent::ToolCompleted {
                id,
                output,
                is_error,
                result,
            },
            Self::TurnEnded { outcome, cost_usd } => SessionEvent::TurnEnded { outcome, cost_usd },
            Self::TokenUsage {
                total_tokens,
                input_tokens,
                cached_input_tokens,
                output_tokens,
                reasoning_output_tokens,
                context_window,
            } => SessionEvent::TokenUsage {
                total_tokens,
                input_tokens,
                cached_input_tokens,
                output_tokens,
                reasoning_output_tokens,
                context_window,
            },
        })
    }
}

fn delivery_id(event: &ExecutionEvent, id: Option<&str>) -> Option<String> {
    let kind = match event {
        ExecutionEvent::Text { .. } => "text",
        ExecutionEvent::Thinking { .. } => "thinking",
        ExecutionEvent::Prompt { .. } => "prompt",
        ExecutionEvent::Notice { .. } => "notice",
        ExecutionEvent::ToolStarted { .. } => "tool-start",
        ExecutionEvent::ToolCompleted { .. } => "tool-end",
        ExecutionEvent::TurnEnded { .. } => "turn-end",
        _ => return None,
    };
    id.map(|id| format!("{kind}:{id}"))
}

fn append_delta(previous: &mut Input, next: &Input) -> bool {
    match (previous, next) {
        (
            Input::Event(SessionEvent::ToolOutputDelta { id, text }),
            Input::Event(SessionEvent::ToolOutputDelta {
                id: next,
                text: more,
            }),
        ) if id == next => {
            text.push_str(more);
            true
        }
        (
            Input::Event(SessionEvent::ReasoningSummaryPart {
                item_id,
                summary_index,
                text,
                snapshot: false,
            }),
            Input::Event(SessionEvent::ReasoningSummaryPart {
                item_id: next,
                summary_index: part,
                text: more,
                snapshot: false,
            }),
        ) if item_id == next && summary_index == part => {
            text.push_str(more);
            true
        }
        (
            Input::Event(SessionEvent::TextDelta { text: a }),
            Input::Event(SessionEvent::TextDelta { text: b }),
        )
        | (
            Input::Event(SessionEvent::ThinkingDelta { text: a }),
            Input::Event(SessionEvent::ThinkingDelta { text: b }),
        ) => {
            a.push_str(b);
            true
        }
        _ => false,
    }
}

fn input_bytes(input: &Input) -> usize {
    match input {
        Input::Prompt(text) | Input::Notice(text) => text.len(),
        Input::Event(event) => match event {
            SessionEvent::ReasoningSummaryPart { item_id, text, .. } => item_id.len() + text.len(),
            SessionEvent::ToolOutputDelta { id, text } => id.len() + text.len(),
            SessionEvent::Progress { event } => event.retained_bytes(),
            SessionEvent::TextDelta { text }
            | SessionEvent::ThinkingDelta { text }
            | SessionEvent::ReasoningSummaryDelta { text, .. } => text.len(),
            SessionEvent::ToolStarted { id, name, input } => {
                id.len() + name.len() + input.to_string().len()
            }
            SessionEvent::ToolCompleted {
                id, output, result, ..
            } => {
                id.len()
                    + output.len()
                    + match result {
                        ToolResult::Command { stdout, stderr } => stdout.len() + stderr.len(),
                        ToolResult::FileEdit { path, hunks } => {
                            path.len()
                                + hunks
                                    .iter()
                                    .flat_map(|hunk| &hunk.lines)
                                    .map(String::len)
                                    .sum::<usize>()
                        }
                        ToolResult::Opaque => 0,
                    }
            }
            _ => 64,
        },
        _ => 64,
    }
}

fn bounded_input(input: &Input, capacity: usize) -> Input {
    fn tail(text: &str, capacity: usize) -> String {
        let mut start = text.len().saturating_sub(capacity);
        while start < text.len() && !text.is_char_boundary(start) {
            start += 1;
        }
        text[start..].to_string()
    }
    match input {
        Input::Event(SessionEvent::TextDelta { text }) => Input::Event(SessionEvent::TextDelta {
            text: tail(text, capacity),
        }),
        Input::Event(SessionEvent::ThinkingDelta { text }) => {
            Input::Event(SessionEvent::ThinkingDelta {
                text: tail(text, capacity),
            })
        }
        Input::Prompt(text) => Input::Prompt(tail(text, capacity)),
        Input::Notice(text) => Input::Notice(tail(text, capacity)),
        _ => Input::Notice(
            "Earlier output omitted from the live view"
                .chars()
                .take(capacity)
                .collect(),
        ),
    }
}

fn merge_updates(mut first: transcript::Update, second: transcript::Update) -> transcript::Update {
    first.dirty.extend(second.dirty);
    first.evicted.extend(second.evicted);
    first.boundary = second.boundary.or(first.boundary);
    first
}
