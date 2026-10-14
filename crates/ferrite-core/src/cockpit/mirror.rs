//! One open Thread as a window holds it (ADR 0025): an Activity built from
//! exactly the inputs the Engine applied, as its messages carry them, and
//! the Thread's state from its latest `thread` message. What `ThreadView`
//! reads, the window draws.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::activity::{Activity, ActivityInput, ActivityView, ToolTiming};
use crate::engine::{event_changes_content, PaneUpdate};
use crate::protocol::{self, ServerMessage, ThreadState, WireInput};
use crate::store::Provider;
use crate::transcript::{Input, Transcript};
use crate::workspace::WorkspaceBinding;
use crate::{ControlKind, Decision, ModelInfo, PermissionModeChoice, SessionCommand, ThreadId};

pub(crate) struct Mirror {
    activity: Activity,
    /// The Session generation the last `Connect` named: what attributed
    /// inputs are applied under, as the Engine applied them.
    generation: u64,
    /// The last input applied.
    pub(crate) seq: u64,
    facts: Option<Facts>,
}

/// A `ThreadState` in core types, so a view can lend borrows of it.
struct Facts {
    generation: u64,
    provider: Provider,
    model: Option<String>,
    effort: Option<String>,
    title: Option<String>,
    workspace: Option<WorkspaceBinding>,
    session_project_root: Option<PathBuf>,
    first_prompt_sent: bool,
    has_prompt_history: bool,
    starting: bool,
    tuning: bool,
    native_queue_pending: bool,
    queued: Vec<String>,
    suggestion: Option<String>,
    commands: Vec<SessionCommand>,
    models: Vec<ModelInfo>,
    permission_mode: Option<String>,
    permission_modes: Vec<PermissionModeChoice>,
    controls: Vec<ControlKind>,
    store_error: Option<String>,
}

impl From<ThreadState> for Facts {
    fn from(state: ThreadState) -> Self {
        Self {
            generation: state.generation,
            provider: state.provider,
            model: state.model,
            effort: state.effort,
            title: state.title,
            workspace: state
                .workspace
                .as_ref()
                .and_then(|binding| binding.try_into().ok()),
            session_project_root: state.session_project_root,
            first_prompt_sent: state.first_prompt_sent,
            has_prompt_history: state.has_prompt_history,
            starting: state.starting,
            tuning: state.tuning,
            native_queue_pending: state.native_queue_pending,
            queued: state.queued,
            suggestion: state.suggestion,
            commands: state.commands.iter().map(Into::into).collect(),
            models: state.models,
            permission_mode: state.permission_mode,
            permission_modes: state
                .permission_modes
                .into_iter()
                .map(|mode| PermissionModeChoice {
                    value: mode.value,
                    label: mode.label,
                })
                .collect(),
            controls: state
                .controls
                .iter()
                .filter_map(|kind| kind.try_into().ok())
                .collect(),
            store_error: state.store_error,
        }
    }
}

impl Mirror {
    fn new() -> Self {
        Self {
            activity: Activity::default(),
            generation: 0,
            seq: 0,
            facts: None,
        }
    }

    fn apply(&mut self, input: ActivityInput, update: &mut PaneUpdate) {
        let content_changed = match &input {
            ActivityInput::Main {
                input: Input::Event(event),
                ..
            } => event_changes_content(event),
            ActivityInput::Observe { event, .. } => {
                event_changes_content(&crate::SessionEvent::Activity(event.clone()))
            }
            ActivityInput::Main { .. }
            | ActivityInput::Replay(_)
            | ActivityInput::ReplayEvent(_) => true,
            _ => false,
        };
        if let ActivityInput::Connect { generation } = input {
            self.generation = generation;
        }
        update.absorb(self.activity.apply(input), content_changed);
    }

    fn apply_wire(&mut self, input: &WireInput, at: Instant, update: &mut PaneUpdate) {
        if let Some(input) = protocol::incoming(input, self.generation, at) {
            self.apply(input, update);
        }
    }
}

/// The open Threads' mirrors, fed from the Engine's messages.
#[derive(Default)]
pub(crate) struct Mirrors {
    threads: std::collections::BTreeMap<ThreadId, Mirror>,
    /// What changed for each Pane since the window last asked.
    updates: Vec<PaneUpdate>,
    /// Threads whose Activity started over: every Block is new to the Pane.
    restarted: Vec<ThreadId>,
    /// A Session announced other models since the window last asked.
    models_changed: bool,
}

impl Mirrors {
    /// Apply one message. `boot` is the Engine's clock zero on this side.
    pub(crate) fn receive(&mut self, message: ServerMessage, boot: Instant) {
        match message {
            ServerMessage::History {
                thread,
                after,
                inputs,
            } => {
                let mirror = self.threads.entry(thread).or_insert_with(Mirror::new);
                if after == 0 {
                    mirror.activity = Activity::default();
                    mirror.generation = 0;
                    self.restarted.push(thread);
                }
                let update = updated(&mut self.updates, thread);
                for input in &inputs {
                    mirror.apply_wire(input, boot, update);
                }
            }
            ServerMessage::HistoryEnd { thread, seq, live } => {
                let mirror = self.threads.entry(thread).or_insert_with(Mirror::new);
                mirror.seq = seq;
                let update = updated(&mut self.updates, thread);
                for input in &live {
                    mirror.apply_wire(input, crate::clock::instant(), update);
                }
            }
            ServerMessage::Input {
                thread,
                seq,
                at_ms,
                input,
            } => {
                let mirror = self.threads.entry(thread).or_insert_with(Mirror::new);
                mirror.seq = seq;
                let update = updated(&mut self.updates, thread);
                mirror.apply_wire(&input, protocol::instant_at(at_ms, boot), update);
            }
            ServerMessage::ToolDraft {
                thread,
                id,
                name,
                input,
            } => {
                let mirror = self.threads.entry(thread).or_insert_with(Mirror::new);
                let update = updated(&mut self.updates, thread);
                mirror.apply(
                    ActivityInput::Main {
                        input: Input::Event(crate::SessionEvent::ToolDraft { id, name, input }),
                        at: crate::clock::instant(),
                    },
                    update,
                );
            }
            ServerMessage::Thread { state, .. } => {
                let thread = state.thread;
                let mirror = self.threads.entry(thread).or_insert_with(Mirror::new);
                let facts = Facts::from(state);
                self.models_changed |= mirror
                    .facts
                    .as_ref()
                    .is_none_or(|before| before.models != facts.models);
                mirror.facts = Some(facts);
                updated(&mut self.updates, thread).activity_changed = true;
            }
            ServerMessage::Roster { roster, .. } => {
                self.threads
                    .retain(|thread, _| roster.open.contains(thread));
            }
            _ => {}
        }
    }

    /// Each mirror's highlighter answers, as the Engine drains its own.
    pub(crate) fn drain_highlights(&mut self) {
        for (&thread, mirror) in &mut self.threads {
            let update = updated(&mut self.updates, thread);
            mirror.apply(ActivityInput::DrainHighlights, update);
        }
    }

    pub(crate) fn take_models_changed(&mut self) -> bool {
        std::mem::take(&mut self.models_changed)
    }

    /// What changed for each Pane since the last call.
    pub(crate) fn take_updates(&mut self) -> Vec<PaneUpdate> {
        for thread in std::mem::take(&mut self.restarted) {
            if let Some(mirror) = self.threads.get(&thread) {
                let update = updated(&mut self.updates, thread);
                update.dirty = mirror
                    .activity
                    .view()
                    .main()
                    .transcript()
                    .blocks()
                    .iter()
                    .map(|block| block.id)
                    .collect();
                update.activity_changed = true;
            }
        }
        std::mem::take(&mut self.updates)
            .into_iter()
            .filter(|update| {
                update.activity_changed || !update.dirty.is_empty() || !update.subjects.is_empty()
            })
            .collect()
    }

    /// The last input this window applied to a Thread: where a resume
    /// picks up.
    #[cfg(test)]
    pub(crate) fn seq(&self, thread: ThreadId) -> Option<u64> {
        self.threads.get(&thread).map(|mirror| mirror.seq)
    }

    pub(crate) fn view(&self, thread: ThreadId) -> Option<ThreadView<'_>> {
        let mirror = self.threads.get(&thread)?;
        Some(ThreadView {
            activity: mirror.activity.view(),
            facts: mirror.facts.as_ref()?,
        })
    }
}

fn updated(updates: &mut Vec<PaneUpdate>, thread: ThreadId) -> &mut PaneUpdate {
    let index = match updates.iter().position(|update| update.thread == thread) {
        Some(index) => index,
        None => {
            updates.push(PaneUpdate::new(thread));
            updates.len() - 1
        }
    };
    &mut updates[index]
}

/// An open Thread's facts, read through `Cockpit::thread`. Every accessor
/// hands back a borrow of the Cockpit itself (`'a`), so a caller keeps the
/// facts it read after the handle is gone.
#[derive(Clone, Copy)]
pub struct ThreadView<'a> {
    activity: ActivityView<'a>,
    facts: &'a Facts,
}

impl<'a> ThreadView<'a> {
    /// Changes whenever this Thread receives a replacement Session.
    pub fn generation(&self) -> u64 {
        self.facts.generation
    }

    pub fn supports_control(&self, kind: ControlKind) -> bool {
        self.facts.controls.contains(&kind)
    }

    pub fn permission_modes(&self) -> Vec<PermissionModeChoice> {
        self.facts.permission_modes.clone()
    }

    pub fn activity(&self) -> ActivityView<'a> {
        self.activity
    }

    pub fn store_error(&self) -> Option<&'a str> {
        self.facts.store_error.as_deref()
    }

    pub fn transcript(&self) -> &'a Transcript {
        self.activity.main().transcript()
    }

    /// Which backend serves this Thread.
    pub fn provider(&self) -> Provider {
        self.facts.provider
    }

    /// The model this Thread chose before its first prompt (#25) — the ✓ in
    /// the provider picker. `None` is the provider's default.
    pub fn model(&self) -> Option<&'a str> {
        self.facts.model.as_deref()
    }

    /// The reasoning effort this Thread chose, in the provider's own word.
    /// `None` is the operator's default for the provider.
    pub fn effort(&self) -> Option<&'a str> {
        self.facts.effort.as_deref()
    }

    /// The durable operator title, as this open Thread holds it.
    pub fn title(&self) -> Option<&'a str> {
        self.facts.title.as_deref()
    }

    /// The follow-up predicted for this Thread's last response, if one has
    /// landed and survived the filter. What the Composer's idle line offers
    /// and what Tab accepts.
    pub fn suggestion(&self) -> Option<&'a str> {
        self.facts.suggestion.as_deref()
    }

    /// What this Thread is blocked on, if anything.
    pub fn pending(&self) -> Option<&'a Decision> {
        self.activity
            .decisions()
            .first()
            .map(|pending| &pending.decision)
    }

    /// The prompt held back while the turn runs — the latest, which is
    /// the one `⌫ unqueue` and Up act on.
    pub fn queued(&self) -> Option<&'a str> {
        self.facts.queued.first().map(String::as_str)
    }

    /// Every prompt held back, newest first: the latest sits on top of the
    /// pile and what was said before it reads underneath.
    pub fn queued_all(&self) -> Vec<&'a str> {
        self.facts.queued.iter().map(String::as_str).collect()
    }

    /// Startup/replacement is pending; its choices and first prompt have
    /// not committed yet. Ready adapters never enter this state.
    pub fn starting(&self) -> bool {
        self.facts.starting
    }

    /// Is a turn running? A prompt written now has to wait for it.
    pub fn busy(&self) -> bool {
        self.working() || self.starting()
    }

    /// Is the agent mid-turn? Unlike `busy`, a Session still starting
    /// (a launch's revival, a model or provider change) is not working.
    pub fn working(&self) -> bool {
        self.activity.main().busy()
    }

    /// Whether work runs behind Main: its Subagents, or its transcript's
    /// background tasks.
    pub fn background_work(&self) -> bool {
        self.activity.working_descendants() > 0
            || self.transcript().progress().working_background() > 0
    }

    /// Whether a model or effort change waits on the provider's answer:
    /// the pickers hold until it lands.
    pub fn tuning(&self) -> bool {
        self.facts.tuning
    }

    pub fn needs_queue(&self) -> bool {
        self.busy() || self.activity.main_operator_turn() || self.facts.native_queue_pending
    }

    /// The checkout this Thread works in — what the Pane's chrome shows.
    pub fn workspace(&self) -> Option<&'a WorkspaceBinding> {
        self.facts.workspace.as_ref()
    }

    /// Where inside the binding its work happens (#24). `None` means the
    /// binding itself — today's behavior, and every Thread's starting point.
    pub fn session_project_root(&self) -> Option<&'a Path> {
        self.facts.session_project_root.as_deref()
    }

    /// Whether the first operator prompt has gone out — the one lock every
    /// pre-prompt control reads (#25, #29).
    pub fn first_prompt_sent(&self) -> bool {
        self.facts.first_prompt_sent
    }

    pub fn has_prompt_history(&self) -> bool {
        self.facts.has_prompt_history
    }

    /// The wall clocks of this Thread's tool calls, keyed by call id. Never
    /// a guess: a call with no measured clock is absent.
    pub fn tool_timings(&self) -> &'a HashMap<String, ToolTiming> {
        self.activity.main().tool_timings()
    }

    /// The live Session's command menu (#23): what `/` offers in this
    /// Thread's Composer. Empty until the Session announces one.
    pub fn commands(&self) -> &'a [SessionCommand] {
        &self.facts.commands
    }

    /// The models the live Session's install offers (#25). Empty until the
    /// Session announces a list.
    pub fn models(&self) -> &'a [ModelInfo] {
        &self.facts.models
    }

    /// The live Session's permission mode (#23): the meta row's mode chip.
    pub fn permission_mode(&self) -> Option<&'a str> {
        self.facts.permission_mode.as_deref()
    }
}
