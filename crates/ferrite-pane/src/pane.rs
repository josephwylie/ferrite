//! One Pane: the visible cell for one Thread. Header, transcript, Composer,
//! and the three semantic-zoom renderings. Rendering only — everything it
//! shows is folded in core, and every key it answers to belongs to the
//! cockpit above it.
//!
//! A Pane is a card on the frame (theme WP-C, SPEC §5.3): `paint::CARD` at
//! `R_CARD`; its content starts at the card's own edges, and its 1px ring
//! is an overlay just outside them (`card_ring`) that only changes colour —
//! `CARD_RING` at rest, `FOCUS_RING` when focused on a board, `WAIT_RING`
//! while it waits (a focused waiting Pane shows only the yellow; its focus
//! reads in the head ground and title). On a board it wears one head
//! row — the dot (a working Thread's working mark), the title, the
//! provider mark — and no state word; top to bottom then the subagent
//! strip, the transcript body (its Decision is the transcript's tail), the
//! Composer. Two altitudes (R12): the transcript, or the wall tile — the
//! state word in colour, the last lines dim, the quick answers.

mod agents;
mod compact_composer;
mod composer;
mod menus;
mod output;
mod text;
mod usage;
mod working;
mod wrap_up;
pub(crate) use agents::{is_agent, render_agent};
pub(crate) use wrap_up::{render_wrap_up, Face as WrapUpFace};
pub use agents::{AgentFace, AgentFaces, AgentRun};
#[cfg(any(test, feature = "test-support"))]
pub use composer::model_label;
pub use composer::{
    attach_button, composer_footer, composer_row_limit, env_strip, env_word, flat_facts_text,
    mode_chip, model_effort_chip, offers_import, permission_mode_label, queued_close, queued_count,
    queued_edit, queued_send_now, queued_sends_now, queued_takes_back, send_button, send_is_stop,
    toolbar_chip, ComposerControls,
};
use composer::{
    composer_region, composer_stack, env_face, ghost_row, placeholder, queue_card, ComposerStack,
    Ghost,
};
pub use menus::{menu_item, menu_popover, menu_row, picker_row, picker_section, MenuRow};
use output::{call_body, diff_rows, hunk_rows, render_diff, running_tests, DiffRow};
pub use output::{diff_number_cells, test_elbow, DiffKind, DiffPaint, TestElbow};
pub(crate) use text::{collect_block_text, collect_group_text, group_live, turn_stamp};
pub use usage::{
    compact_count, context_meter_segments, context_segments, context_usage, ctx_meter_ink,
    plan_usage_fraction, plan_usage_ring, usage_ink,
};
pub(crate) use working::running_call;
pub use working::{background_clock, background_link, working_tail, WorkingFacts};
#[cfg(test)]
use {
    composer::{composer_fixed_height, composer_queue_height, queued_preview},
    output::{changed_words, result_highlights},
    usage::{category_ink, reset_label},
    working::{starting_line, working_line_for, working_row},
};

use ferrite_core::activity::{AgentStatus, Subject};
use ferrite_core::cockpit::ThreadView;
pub use ferrite_core::digest::{Turn, WallCard, WallLine};
use ferrite_core::docview::{is_test_run, passed_count, Level};
use ferrite_core::followup::{self, Followup};
use ferrite_core::progress::Phase;
use ferrite_core::roster::{DraftId, PaneIdentity};
use ferrite_core::store::Provider;
use ferrite_core::transcript::{
    Block, BlockId, Body, Diff, Span, Status, Style, Token, ToolBlock, ToolState, Transcript,
};
use ferrite_core::workspace::{Check, CheckState, PullRequest, WorkspaceBinding};
use ferrite_core::{Decision, ThreadId};
use gpui::prelude::*;
use gpui::{
    deferred, div, px, relative, rgb, rgba, AnyElement, Context, Div, Entity, FocusHandle,
    HighlightStyle, SharedString, Stateful, Styled, StyledText,
};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::composer::Composer;
use crate::decision;
use crate::select::TextRuns;
use ferrite_ui::components;
use ferrite_ui::components::Tip as _;
use ferrite_ui::icons::{self, icon};
use ferrite_ui::pointer::{Pointer, PointerPressed};
// Every color and metric here is a Soft token (ferrite_ui::theme) — no literal
// survives in render code, which is #22's grep-able law.
use ferrite_ui::theme;
use ferrite_ui::theme::*;

// The Cockpit's ⌘D, which a Group head's needs-you door dispatches. It is
// declared here, below the Cockpit, but keeps the `cockpit` namespace:
// operators' keymaps bind it as `cockpit::NextDecision` (ADR 0024).
gpui::actions!(cockpit, [NextDecision]);

/// One Pane's view state: what the window owns per Pane. Everything it
/// shows lives in core; this is the keyboard, the scrollback position, and
/// the wall cell's cached strings.
pub struct PaneView {
    /// Which Pane of the roster this is: a live Thread, or a draft still
    /// choosing its provider and CWD (#29) — no Thread, no Session, nothing
    /// durable. The roster's own identity, so the two can never disagree.
    pub identity: PaneIdentity,
    /// The draft's choices, while it is one.
    draft: Option<DraftBinding>,
    /// The Thread's slug name — `thread-NN` until display names exist
    /// (sidebar-and-impl §4.2 #8) — or the draft's placeholder title.
    /// Built once; the wall must not format names per frame.
    pub name: SharedString,
    pub composer: Entity<Composer>,
    pub preview: crate::preview::Preview,
    pub controls_focus: FocusHandle,
    pub selected: Subject,
    pub generation: u64,
    pub rich: crate::rich::TextCache,
    pub history_error: Option<String>,
    pub request_error: Option<(ferrite_core::activity::DecisionHandle, String)>,
    /// Retained, virtualized transcript entities keyed by Subject. The pane
    /// still owns chrome and cross-pane coordination; each Subject owns its
    /// expensive row tree, scroll position and native text cache.
    pub transcripts: HashMap<Subject, Entity<crate::transcript::TranscriptView>>,
    subject_views: HashMap<Subject, TranscriptViewport>,
    pub selection_scope: gpui::base::TextSelectionScopeId,
    pub transcript_focus: FocusHandle,
    /// A pending Decision takes the keyboard: y and n are answers, not text.
    pub decision_focus: FocusHandle,
    disclosure: ToolDisclosure,
    disclosure_revision: u64,
    /// Where this Pane's card and Composer were last laid out, so a card
    /// the Pane summons can hang off the Composer's edge or the head and
    /// stay inside the card (`card_corner`), whatever the pointer did.
    pub geometry: std::rc::Rc<std::cell::Cell<PaneGeometry>>,
}

/// A Pane's last laid-out card and Composer bounds, recorded in prepaint.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PaneGeometry {
    pub card: Option<gpui::Bounds<gpui::Pixels>>,
    pub composer: Option<gpui::Bounds<gpui::Pixels>>,
}

/// View ownership stays with a Subject even while its transcript is hidden.
struct TranscriptViewport {
    generation: u64,
    selection_scope: gpui::base::TextSelectionScopeId,
    disclosure: ToolDisclosure,
}

/// Disclosure identities preserve choices while content streams or Subjects
/// switch: a call's details (its output fold, its diffs), a long thought,
/// and a group of calls, named by its first call.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DisclosureId {
    Tool(String),
    Reasoning(BlockId),
    Group(String),
}
impl From<&str> for DisclosureId {
    fn from(call: &str) -> Self {
        Self::Tool(call.to_owned())
    }
}
impl From<&DisclosureId> for DisclosureId {
    fn from(id: &DisclosureId) -> Self {
        id.clone()
    }
}
impl std::fmt::Display for DisclosureId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

struct ToolDisclosure {
    expanded: HashSet<DisclosureId>,
    target: Option<DisclosureId>,
    focus: FocusHandle,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisclosureState {
    Collapsed,
    Expanded,
}

/// A draft Pane's presentation state around its headless choices.
pub struct DraftBinding {
    pub binding: ferrite_core::draft::DraftBinding,
    /// The band chip tab has parked on; None with the keyboard in the
    /// prompt line — the zero-keystroke default path.
    pub band_focus: Option<BandChip>,
    /// A failed bootstrap's words, shown where the band is. The Pane stays
    /// draft and the prompt stays in the Composer.
    pub error: Option<SharedString>,
}

/// The band's three chips, in tab order: the model (its effort a
/// submenu there), the Project, the Workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandChip {
    Provider,
    Project,
    Workspace,
}

impl BandChip {
    /// Where tab goes next: across the band, then back to the prompt.
    pub fn next(current: Option<BandChip>) -> Option<BandChip> {
        match current {
            None => Some(BandChip::Provider),
            Some(BandChip::Provider) => Some(BandChip::Project),
            Some(BandChip::Project) => Some(BandChip::Workspace),
            Some(BandChip::Workspace) => None,
        }
    }
}

impl PaneView {
    pub fn new<T: 'static>(thread: ThreadId, cx: &mut Context<T>) -> Self {
        let preview = crate::preview::Preview::new(cx);
        let rich = crate::rich::TextCache::default();
        let transcript_preview = preview.clone();
        let transcript_rich = rich.clone();
        let transcript = cx.new(move |cx| {
            crate::transcript::TranscriptView::empty(
                thread,
                format!("{thread}-main-0").into(),
                transcript_preview.clone(),
                transcript_rich.clone(),
                cx,
            )
        });
        let transcript_focus = transcript.read(cx).transcript_focus();
        Self {
            identity: PaneIdentity::Thread(thread),
            draft: None,
            name: SharedString::from(format!("thread-{thread:02}")),
            composer: cx.new(Composer::new),
            preview,
            controls_focus: cx.focus_handle(),
            selected: Subject::Main,
            generation: 0,
            rich,
            history_error: None,
            request_error: None,
            transcripts: HashMap::from([(Subject::Main, transcript)]),
            subject_views: HashMap::new(),
            selection_scope: gpui::base::TextSelectionScopeId::new(),
            transcript_focus,
            decision_focus: cx.focus_handle(),
            disclosure: ToolDisclosure {
                expanded: HashSet::new(),
                target: None,
                focus: cx.focus_handle(),
            },
            disclosure_revision: 0,
            geometry: std::rc::Rc::default(),
        }
    }

    /// A draft Pane (#29): cmd-t's answer — a Composer and the pre-prompt
    /// band, and nothing else until the first send bootstraps a Thread.
    pub fn new_draft<T: 'static>(
        draft: DraftId,
        binding: DraftBinding,
        cx: &mut Context<T>,
    ) -> Self {
        let preview = crate::preview::Preview::new(cx);
        let rich = crate::rich::TextCache::default();
        let transcript_preview = preview.clone();
        let transcript_rich = rich.clone();
        let transcript = cx.new(move |cx| {
            crate::transcript::TranscriptView::empty(
                ThreadId::new(0),
                "0-main-0".into(),
                transcript_preview.clone(),
                transcript_rich.clone(),
                cx,
            )
        });
        let transcript_focus = transcript.read(cx).transcript_focus();
        Self {
            identity: PaneIdentity::Draft(draft),
            draft: Some(binding),
            name: SharedString::from(DRAFT_TITLE),
            composer: cx.new(Composer::new),
            preview,
            controls_focus: cx.focus_handle(),
            selected: Subject::Main,
            generation: 0,
            rich,
            history_error: None,
            request_error: None,
            transcripts: HashMap::from([(Subject::Main, transcript)]),
            subject_views: HashMap::new(),
            selection_scope: gpui::base::TextSelectionScopeId::new(),
            transcript_focus,
            decision_focus: cx.focus_handle(),
            disclosure: ToolDisclosure {
                expanded: HashSet::new(),
                target: None,
                focus: cx.focus_handle(),
            },
            disclosure_revision: 0,
            geometry: std::rc::Rc::default(),
        }
    }

    /// The Thread this Pane shows, or None while it is still a draft.
    pub fn thread(&self) -> Option<ThreadId> {
        self.identity.thread()
    }

    pub fn draft(&self) -> Option<&DraftBinding> {
        self.draft.as_ref()
    }

    pub fn draft_mut(&mut self) -> Option<&mut DraftBinding> {
        self.draft.as_mut()
    }

    /// The lock's visible half (#29): the first send made a Thread of this
    /// draft, and the band disappears with the Pane's next frame.
    pub fn adopt_thread(&mut self, thread: ThreadId) {
        self.identity = PaneIdentity::Thread(thread);
        self.draft = None;
        self.name = SharedString::from(format!("thread-{thread:02}"));
    }

    pub fn is_main(&self) -> bool {
        self.selected == Subject::Main
    }

    pub fn text_namespace(&self) -> SharedString {
        let thread = self.thread().map(|id| id.get()).unwrap_or(0);
        match &self.selected {
            Subject::Main => format!("{thread}-main-{}", self.generation).into(),
            Subject::Subagent(key) => {
                format!("{thread}-{}-{}", key.as_str(), self.generation).into()
            }
        }
    }

    pub fn transcript(&self) -> Option<Entity<crate::transcript::TranscriptView>> {
        self.transcripts.get(&self.selected).cloned()
    }

    /// Core evicted this Subject's rendered projection. Drop its heavy view;
    /// ordinary Subject switches retain their entity and scroll position.
    pub fn release_transcript(
        &mut self,
        subject: &Subject,
    ) -> Option<Entity<crate::transcript::TranscriptView>> {
        self.transcripts.remove(subject)
    }

    /// Each Subject keeps one retained transcript entity while hidden. The
    /// entity itself owns scroll/layout state; this map only preserves the
    /// identity across Subject switches and roster redraws.
    pub fn ensure_transcript<T: 'static>(
        &mut self,
        cx: &mut Context<T>,
    ) -> Option<Entity<crate::transcript::TranscriptView>> {
        let thread = self.thread()?;
        let subject = self.selected.clone();
        let namespace = self.text_namespace();
        if !self.transcripts.contains_key(&subject) {
            let preview = self.preview.clone();
            let rich = self.rich.clone();
            let transcript = cx.new(|cx| {
                crate::transcript::TranscriptView::empty(thread, namespace, preview, rich, cx)
            });
            self.transcripts.insert(subject.clone(), transcript);
        }
        let transcript = self.transcripts.get(&subject).cloned();
        if let Some(transcript) = &transcript {
            self.transcript_focus = transcript.read(cx).transcript_focus();
        }
        transcript
    }

    pub fn select_subject<T: 'static>(
        &mut self,
        subject: Subject,
        generation: u64,
        cx: &mut Context<T>,
    ) {
        if self.selected == subject {
            if self.generation != generation {
                self.generation = generation;
            }
            return;
        }
        self.rich.clear_output_selection(&self.text_namespace(), cx);
        let mut next = self
            .subject_views
            .remove(&subject)
            .unwrap_or_else(|| TranscriptViewport {
                generation,
                selection_scope: gpui::base::TextSelectionScopeId::new(),
                disclosure: ToolDisclosure {
                    expanded: HashSet::new(),
                    target: None,
                    focus: cx.focus_handle(),
                },
            });
        if next.generation != generation {
            next.generation = generation;
        }
        std::mem::swap(&mut next.generation, &mut self.generation);
        std::mem::swap(&mut next.selection_scope, &mut self.selection_scope);
        std::mem::swap(&mut next.disclosure, &mut self.disclosure);
        self.subject_views
            .insert(std::mem::replace(&mut self.selected, subject), next);
        self.ensure_transcript(cx);
        self.history_error = None;
    }

    pub fn redirect_subject(
        &mut self,
        from: &ferrite_core::activity::AgentKey,
        to: &ferrite_core::activity::AgentKey,
    ) {
        let thread = self.thread().map(|id| id.get()).unwrap_or(0);
        self.rich.redirect_namespace(
            &format!("{thread}-{}-", from.as_str()),
            &format!("{thread}-{}-", to.as_str()),
        );
        let from = Subject::Subagent(from.clone());
        let to = Subject::Subagent(to.clone());
        if self.selected == from {
            self.selected = to.clone();
        }
        if let Some(old) = self.subject_views.remove(&from) {
            self.subject_views.entry(to.clone()).or_insert(old);
        }
        if let Some(old) = self.transcripts.remove(&from) {
            self.transcripts.entry(to).or_insert(old);
        }
    }

    pub fn toggle_tool(&mut self, call: &DisclosureId) {
        if !self.disclosure.expanded.remove(call) {
            self.disclosure.expanded.insert(call.clone());
        }
        // Toggling never moves the keyboard target: Tab/Shift-Tab set it,
        // and a pointer click only opens or closes the row.
        self.disclosure_revision = self.disclosure_revision.wrapping_add(1);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn tool_state(&self, call: impl Into<DisclosureId>) -> DisclosureState {
        if self.disclosure.expanded.contains(&call.into()) {
            DisclosureState::Expanded
        } else {
            DisclosureState::Collapsed
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn tool_targeted(&self, call: impl Into<DisclosureId>) -> bool {
        self.disclosure.target.as_ref() == Some(&call.into())
    }

    pub fn has_tool_target(&self) -> bool {
        self.disclosure.target.is_some()
    }

    pub fn targeted_tool(&self) -> Option<&DisclosureId> {
        self.disclosure.target.as_ref()
    }

    pub fn tool_focus(&self) -> FocusHandle {
        self.disclosure.focus.clone()
    }

    pub fn cycle_tools(&mut self, calls: &[DisclosureId], reverse: bool) -> Option<&DisclosureId> {
        let next = if calls.is_empty() {
            None
        } else if reverse {
            match self
                .disclosure
                .target
                .as_ref()
                .and_then(|target| calls.iter().position(|call| call == target))
            {
                None => calls.last().cloned(),
                Some(0) => None,
                Some(at) => calls.get(at - 1).cloned(),
            }
        } else {
            match self
                .disclosure
                .target
                .as_ref()
                .and_then(|target| calls.iter().position(|call| call == target))
            {
                None => calls.first().cloned(),
                Some(at) if at + 1 == calls.len() => None,
                Some(at) => calls.get(at + 1).cloned(),
            }
        };
        if self.disclosure.target != next {
            self.disclosure.target = next;
            self.disclosure_revision = self.disclosure_revision.wrapping_add(1);
        }
        self.disclosure.target.as_ref()
    }

    pub fn prune_tools(&mut self, calls: &HashSet<DisclosureId>) {
        let expanded = self.disclosure.expanded.clone();
        let target = self.disclosure.target.clone();
        self.disclosure.expanded.retain(|call| calls.contains(call));
        if self
            .disclosure
            .target
            .as_ref()
            .is_some_and(|call| !calls.contains(call))
        {
            self.disclosure.target = None;
        }
        if self.disclosure.expanded != expanded || self.disclosure.target != target {
            self.disclosure_revision = self.disclosure_revision.wrapping_add(1);
        }
    }

    pub fn clear_tool_target(&mut self) {
        if self.disclosure.target.take().is_some() {
            self.disclosure_revision = self.disclosure_revision.wrapping_add(1);
        }
    }

    pub fn transcript_disclosure_snapshot(
        &self,
    ) -> (HashSet<DisclosureId>, Option<DisclosureId>, FocusHandle) {
        // The retained renderer receives a snapshot; its display revision
        // changes for every state transition above.
        (
            self.disclosure.expanded.clone(),
            self.disclosure.target.clone(),
            self.disclosure.focus.clone(),
        )
    }

    pub fn disclosure_revision(&self) -> u64 {
        self.disclosure_revision
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn tool_expanded(&self, call: impl Into<DisclosureId>) -> bool {
        self.tool_state(call) == DisclosureState::Expanded
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn tool_bounds(
        &self,
        call: impl Into<DisclosureId>,
        cx: &gpui::App,
    ) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.transcript()?.read(cx).disclosure_bounds(&call.into())
    }
}

/// Everything one Pane draws, as the cockpit reads it for this frame.
/// What a frame knows about a Pane: the Thread's own facts through one
/// core handle — `None` for a Thread the cockpit could not open — beside
/// the four the window alone can answer.
pub struct PaneFacts<'a> {
    pub thread: Option<ThreadView<'a>>,
    /// Whether the Composer line is empty — what decides the idle
    /// placeholder, read where the cockpit has a `cx` to read it with.
    pub composer_empty: bool,
    /// How many files wait in the Composer's draft: an unfocused board
    /// cell counts them on its flat line instead of drawing their shelf.
    pub composer_files: usize,
    pub focused: bool,
    /// This Thread finished and the operator has not seen it since (an
    /// unread Notice): its head's mark is the done check until they do.
    pub unread: bool,
    /// The wall cell's folded reading, cached by the cockpit's facts —
    /// everything the L3 recipe needs that is not an O(1) transcript read.
    /// None for a Thread the facts have not met, which draws as empty.
    pub wall: Option<&'a WallCard>,
    /// The operator asked the system for reduced motion (`cx.reduce_motion()`).
    pub reduce_motion: bool,
    /// The Composer's own focus handle holds focus in the active window.
    pub editing: bool,
    /// Native files are dragged over this Pane: the drop sheet covers it
    /// and the Composer, drawn above the sheet, wears the strong edge.
    pub drop_target: bool,
    /// More than one Pane is on the board, so which one holds the keyboard
    /// needs showing: only then does focus draw the `FOCUS_RING` edge.
    pub show_focus: bool,
    /// This Thread's provider: the Group head's mark at its right.
    pub provider_mark: Option<Provider>,
    /// This Pane's docked Decision merges into its live Composer (one
    /// block, rule 2.8.1): the Composer drops its top edge and corners.
    pub decision_joined: bool,
    /// The Pane's width on the board this frame: what a wall tile's prose
    /// wraps against, measured in the tile's face (`text_system`).
    pub cell_width: f32,
    /// The window's text system, which measures where a wall tile's prose
    /// wraps.
    pub text_system: std::sync::Arc<gpui::TextSystem>,
    /// A parked Thread's two facts, as its tile reads them: `Parked 2h ago
    /// · 11 turns`. `None` for an open Thread.
    pub parked_line: Option<SharedString>,
}

/// The click-wired elements only the cockpit can build — gpui listeners
/// are made with its own `Context` — and the Pane only places. Each is
/// `None` (or empty) below the level that draws it.
#[derive(Default)]
pub struct PaneWiring {
    /// The retained L1 transcript. Its cached entity owns native text and
    /// row layout; the Pane only places the allocated viewport.
    pub transcript: Option<AnyElement>,
    pub attachments: Option<AnyElement>,
    /// The open `/` or `@` popover for this Pane's Composer, rows wired to
    /// their picks in the cockpit and hung above the input line here (#23).
    pub menu: Option<AnyElement>,
    /// The head's title cell, wired: the name with a double-click that
    /// opens the rename editor, or the editor itself while renaming. None
    /// draws the plain name (L2, L3, drafts).
    pub title: Option<AnyElement>,
    /// A subagent's head (`subagent_head`) while the Pane shows one; None
    /// on Main, which has none.
    pub subagent_head: Option<AnyElement>,
    pub activity_decisions: Option<AnyElement>,
    /// A question too big for this Pane's body: it answers in fullscreen
    /// (the expand key), and the body keeps its transcript meanwhile.
    pub expand_question: bool,
    pub question_measurement: Option<AnyElement>,
    /// Wires the whole L1 head as the Pane's drag handle (the cockpit
    /// supplies it while the board has somewhere to move the Pane to).
    pub head_drag: Option<HeadDrag>,
    /// The wall tile's quick answers while its Thread waits (L3 only): the
    /// approval's verbs or the question's options, each a boxed word wired
    /// to its answer in the cockpit.
    pub quick_answers: Option<AnyElement>,
    /// The Solo Composer's controls, wired in the cockpit (SPEC §5.5); on
    /// a board, the focused Pane's send circle alone.
    pub toolbar: Option<ComposerControls>,
    /// The Group head's fill button (`fill_button`), wired to fill the
    /// board with this Pane; None in Solo.
    pub fill: Option<AnyElement>,
    /// An empty Thread's hero project word (`hero_project`): the hero
    /// (`start_hero`) stands centred over its body until the first prompt.
    pub start: Option<AnyElement>,
}

/// Turns the Pane head's band into its drag handle: the cockpit gives it an
/// id and the drag payload, since only the cockpit knows the board.
pub type HeadDrag = Box<dyn FnOnce(Div) -> AnyElement>;

/// The wall's state matrix (glance.md §4), selected from O(1) reads plus the
/// folded tests flag. Pure so the matrix is assertable without a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallState {
    Working,
    /// Working with a red test suite: the green dot, the failure in red words.
    Failing,
    /// A Decision waits: amber dot, amber edge, `needs you`.
    Decision,
    /// The Session closed under the Thread: red dot, red edge, its reason.
    Blocked,
    /// Turn complete: quiet history, active Composer and a muted `done` —
    /// green never means finished.
    Done,
    Idle,
    /// No transcript in memory at all — the cockpit could not open it.
    Parked,
}

pub fn transcript_status(status: AgentStatus, fresh: bool) -> Status {
    if !fresh {
        return Status::Idle;
    }
    match status {
        AgentStatus::Working => Status::Streaming,
        AgentStatus::Waiting => Status::Blocked,
        AgentStatus::Failed | AgentStatus::Shutdown | AgentStatus::NotFound => Status::Closed,
        _ => Status::Idle,
    }
}

/// glance.md's matrix, one row per state. Transcript owns the turn's meaning;
/// the Pane chooses its presentation alongside Decisions and test results.
pub fn wall_state(turn: Option<&Turn>, pending: bool, tests_failing: bool) -> WallState {
    let Some(turn) = turn else {
        return WallState::Parked;
    };
    let status = turn.status;
    if pending || status == Status::Blocked {
        return WallState::Decision;
    }
    match status {
        Status::Closed => WallState::Blocked,
        Status::Streaming if tests_failing => WallState::Failing,
        Status::Streaming => WallState::Working,
        _ if turn.completed() => WallState::Done,
        _ => WallState::Idle,
    }
}

/// One row of a wall tile: its text, and — a wrapped line's continuation —
/// the glyph run it hangs under (`● `), laid out unseen so the text lines
/// up under the first word in the tile's proportional face.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WallRow {
    pub text: SharedString,
    pub hang: Option<SharedString>,
}

impl WallRow {
    fn plain(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            hang: None,
        }
    }
}

/// A tile's rows: each line one row, prose wrapping once — at its own line
/// break, else where `breaks` says a line that is too wide first breaks
/// (the byte the next row starts at) — its continuation hanging under the
/// first word. The budget is `WALL_LINES` rows, spent on whole lines from
/// the newest back, so a tile never opens on a continuation without its
/// line: with one row left, a wrapped line takes it unwrapped, truncating
/// with `…` at the tile's edge.
pub(crate) fn wall_rows(
    lines: &[WallLine],
    mut breaks: impl FnMut(&str) -> Option<usize>,
) -> Vec<WallRow> {
    let mut kept: Vec<Vec<WallRow>> = Vec::new();
    let mut budget = theme::WALL_LINES;
    for line in lines.iter().rev() {
        if budget == 0 {
            break;
        }
        let mut rows = if line.wraps {
            let (head, tail) = wrap_once(&line.text, &mut breaks);
            let hang = glyph_run(&head).map(SharedString::from);
            let mut rows = vec![WallRow::plain(head.clone())];
            rows.extend(tail.map(|tail| WallRow {
                text: tail.into(),
                hang,
            }));
            rows
        } else {
            vec![WallRow::plain(line.text.clone())]
        };
        if rows.len() > budget {
            let first = line.text.lines().next().unwrap_or_default().to_string();
            rows = vec![WallRow::plain(first)];
        }
        budget -= rows.len();
        kept.push(rows);
    }
    kept.into_iter().rev().flatten().collect()
}

/// A line's leading glyph and its space (`● `), when it opens on one.
fn glyph_run(text: &str) -> Option<&str> {
    let first = text.chars().next()?;
    if first.is_alphanumeric() {
        return None;
    }
    text.find(' ').map(|space| &text[..=space])
}

/// One line of prose into its row and, when it is too wide, the
/// continuation: a line break in the text wins, else `breaks` (never at
/// the glyph's own space).
fn wrap_once(
    text: &str,
    breaks: &mut impl FnMut(&str) -> Option<usize>,
) -> (String, Option<String>) {
    if let Some((head, rest)) = text.split_once('\n') {
        let rest = rest.lines().next().unwrap_or_default().trim();
        return (
            head.trim_end().to_string(),
            (!rest.is_empty()).then(|| rest.to_string()),
        );
    }
    let floor = glyph_run(text).map_or(0, str::len);
    match breaks(text).filter(|at| *at > floor && *at < text.len()) {
        Some(at) => {
            let rest = text[at..].trim();
            (
                text[..at].trim_end().to_string(),
                (!rest.is_empty()).then(|| rest.to_string()),
            )
        }
        None => (text.to_string(), None),
    }
}

/// One Pane. A Thread with no open state in core is one the cockpit could
/// not open; it still gets a cell, because a Pane that vanishes hides the
/// problem.
/// Everything the `render_pane` slots read for one frame, gathered once by the
/// skeleton. **Foundation-owned:** only the integrator adds a field, so no
/// package edits this struct. The owned wiring pieces are `Option`s that the
/// slot named in their doc `take()`s.
pub(crate) struct PaneCtx<'a> {
    pub view: &'a PaneView,
    #[allow(dead_code)]
    pub thread: Option<ThreadView<'a>>,
    /// The selected Subject's transcript; `None` for a parked Pane.
    pub transcript: Option<&'a Transcript>,
    #[allow(dead_code)]
    pub level: Level,
    pub focused: bool,
    /// The Main Subject's pending Decision.
    pub decision: Option<&'a Decision>,
    /// Activity requests (or a question expander) exist for this Pane, so
    /// the plain Decision card steps aside.
    pub has_activity_decisions: bool,
    pub queued: &'a [ferrite_core::engine::QueueItem],
    pub composer_empty: bool,
    pub composer_files: usize,
    pub suggestion: Option<&'a str>,
    // ---- pre-plumbed for the work packages: wired, not read yet
    /// The operator asked the system for reduced motion.
    #[allow(dead_code)]
    pub reduce_motion: bool,
    /// The Composer itself holds the keyboard in the active window.
    #[allow(dead_code)]
    pub editing: bool,
    /// Native files hover this Pane (`PaneFacts::drop_target`).
    pub drop_target: bool,
    /// Finished while the operator looked elsewhere (an unread Notice).
    #[allow(dead_code)]
    pub unread: bool,
    /// More than one Pane is on the board: the Composer is the grid's one
    /// fixed line (C4), not Solo's full block.
    pub grid: bool,
    // ---- owned wiring
    /// `l1_composer` / `l2_composer`.
    pub attachments: Option<AnyElement>,
    pub menu: Option<AnyElement>,
    /// `l1_dock`: activity requests that were not docked in the body.
    pub activity_decisions: Option<AnyElement>,
    /// The docked Decision merges into this Pane's Composer (one block).
    pub decision_joined: bool,
    /// `l1_composer`: the Solo box's controls.
    pub toolbar: Option<ComposerControls>,
}

/// The group name every Pane root carries (`.group(PANE_GROUP)`): what a
/// child styles itself by while the pointer is anywhere over its Pane (the
/// minimap's `group_hover`, the prototype's `.pane:hover .mm`).
pub const PANE_GROUP: &str = "pane";

pub fn render_pane(
    view: &PaneView,
    facts: PaneFacts<'_>,
    wiring: PaneWiring,
    level: Level,
) -> impl IntoElement {
    let PaneFacts {
        thread,
        composer_empty,
        composer_files,
        focused,
        unread,
        wall,
        reduce_motion,
        editing,
        drop_target,
        show_focus,
        provider_mark,
        decision_joined,
        cell_width,
        text_system,
        parked_line,
    } = facts;
    let empty = WallCard::default();
    let wall = wall.unwrap_or(&empty);
    let PaneWiring {
        transcript: retained_transcript,
        attachments,
        menu,
        title,
        subagent_head,
        activity_decisions,
        expand_question,
        question_measurement,
        head_drag,
        quick_answers,
        toolbar,
        fill,
        start,
    } = wiring;
    let has_activity_decisions = activity_decisions.is_some() || expand_question;
    // The Decision is the transcript's tail now (`decision_tail`): no card
    // is laid over the body or docked above the Composer at this level.
    let _ = activity_decisions;
    let subject = thread.and_then(|thread| thread.activity().subject(&view.selected));
    let transcript = subject.as_ref().map(|subject| subject.transcript());
    let decision = if view.is_main() {
        thread.and_then(|thread| thread.pending())
    } else {
        None
    };
    let workspace = thread.and_then(|thread| thread.workspace());
    let status = subject
        .as_ref()
        .map(|subject| transcript_status(subject.status(), subject.fresh()));
    let turn = thread.and_then(|thread| thread.subject_turn(&view.selected));
    let state = wall_state(
        turn.as_ref(),
        decision.is_some_and(Decision::blocks_execution),
        wall.tests_failing,
    );
    // Attention and focus are two channels on one edge (F-2): the edge is
    // an overlay over the Pane's own content (`pane_frame`), recoloured
    // only, so nothing reflows when a Decision arrives. A waiting Pane
    // draws `WAIT_RING` on every side — focused or not; a focused one
    // says so in its head ground and title instead of a second ring.
    let pending = thread.map(|thread| thread.pending_decisions());
    let attention_pending =
        pending.is_some_and(|pending| !pending.is_empty()) || state == WallState::Decision;
    // Focus is drawn only where it tells the operator something: a lone
    // Pane is plainly the one holding the keyboard.
    let framed = focused && show_focus;
    let edge = PaneEdge::of(framed, attention_pending, !show_focus);
    let key = view.thread().map_or(0, ThreadId::get);
    let shell = record_card(pane_shell(edge), view);
    let frame = |pane: Div| pane_frame(pane, edge, key);
    // What a pending request asks, as the lexicon names it: the tile's
    // state word and the question door at the Pane's foot read it.
    let kind = attention_pending.then(|| {
        pending
            .and_then(|pending| pending.first())
            .map(|request| request_kind(&request.decision))
            .or_else(|| decision.map(request_kind))
            .unwrap_or(theme::words::APPROVAL)
    });
    // Solo (fullscreen included) has no head at any tier: the nav's row
    // names the Thread (C2, ADR 0014). A Group's head is the Pane's drag handle
    // whenever the cockpit wires one. It names the Thread and nothing
    // else (theme rule 7): the dot, the title, the provider's mark.
    let head = |title: Option<AnyElement>| {
        show_focus.then(|| {
            let slot = state_word(state, kind, wall, turn.as_ref(), unread);
            let mark = head_mark(
                view.is_main(),
                dot_state(state, slot.as_ref()),
                unread,
                thread.is_some_and(|thread| thread.background_work()),
                status,
            );
            let word = crate::mark::NavTail::of(mark, slot.as_ref(), SharedString::default())
                .member_word();
            let head = group_head(GroupHead {
                key,
                name: view.name.clone(),
                mark: Some(mark),
                word,
                focused: framed,
                title,
                provider: provider_mark,
                action: fill,
                width: Some(cell_width),
            });
            match head_drag {
                Some(drag) => drag(head),
                None => head.into_any_element(),
            }
        })
    };

    // Far enough away, a Pane is its tile: the head, the state word, the
    // last lines, and the quick answers while it waits.
    if level == Level::Wall {
        return frame(
            shell
                .children(head(title))
                .child(wall_cell(WallTile {
                    key,
                    card: wall,
                    state,
                    kind,
                    turn: turn.as_ref(),
                    unread,
                    quick_answers,
                    focused,
                    cell_width,
                    text_system: Some(&text_system),
                    parked_line,
                }))
                .children(drop_target.then(crate::prompt_drop::sheet)),
        );
    }

    let mut cx = PaneCtx {
        view,
        thread,
        transcript,
        level,
        focused,
        decision,
        has_activity_decisions,
        queued: thread.map_or(&[], |thread| thread.queue()),
        composer_empty,
        composer_files,
        suggestion: thread.and_then(|thread| thread.suggestion()),
        reduce_motion,
        editing,
        drop_target,
        unread,
        grid: show_focus,
        attachments,
        menu,
        activity_decisions: None,
        decision_joined,
        toolbar,
    };

    // Solo (fullscreen included) has no head: the nav's row names the
    // Thread (C2, ADR 0014), and the body starts at the Pane's own top edge. A
    // Group's Pane wears the one head. A subagent's transcript has its own
    // head over it either way.
    let mut pane = shell
        .children(head(title))
        .children(subagent_head.map(|head| subagent_strip(key, head)));
    match transcript {
        Some(_) => {
            view.rich
                .file_context(workspace.map(WorkspaceBinding::cwd), &view.preview);
            // A child's tab has no Composer to carry the `Decision` key
            // context: while that child's request pends, its body does, so
            // y/n/a and the digits reach the card from the transcript.
            let child_request = !view.is_main()
                && thread.is_some_and(|thread| {
                    thread
                        .pending_decisions()
                        .iter()
                        .any(|request| request.subject.as_ref() == Some(&view.selected))
                });
            pane = pane.child(
                div()
                    .debug_selector(move || format!("pane-body-{key}"))
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    // Nothing paints under the head: the body clips at its
                    // own top edge — the Pane's edge in Solo, the head rule
                    // on a board.
                    .overflow_hidden()
                    .when(child_request, |body| body.key_context("Decision"))
                    .child(
                        retained_transcript.expect("L1 transcript entity is wired by CockpitView"),
                    )
                    .children(start.map(|project| {
                        // An empty Thread, as a draft: the hero at the
                        // body's centre.
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .px(px(theme::SPACE_4))
                            .child(start_hero(project))
                    }))
                    .children(question_measurement),
            );
            // The order is head · body · progress · dock · composer.
            pane = pane.children(l1_dock(&mut cx));
            // A question too big for this body answers in fullscreen: the
            // Pane's foot says so, where its state reads (theme rule 7).
            pane = pane.children(expand_question.then(|| question_door(key)));
            // The drop sheet covers the Pane beneath the Composer, which
            // paints after it and so stays in view, edged in `INK_STRONG`.
            pane = pane.children(drop_target.then(crate::prompt_drop::sheet));
            pane = pane.children(l1_composer(&mut cx));
        }
        None => {
            // A parked Thread holds its slot as its tile, at any size.
            pane = pane
                .child(parked_body(parked_line))
                .children(drop_target.then(crate::prompt_drop::sheet));
        }
    }
    frame(pane)
}

// ---------------------------------------------------------- render_pane slots
// Each slot's body belongs to one package; its signature and `PaneCtx` are
// the integrator's.

/// WP-F · between the body and the Composer. Every L1 request — Main's
/// plain approval included — is the one Decision card in the body's
/// requests overlay (`activity_decisions`), and a failed send is that
/// card's own error line, so nothing docks here at L1; the slot keeps any
/// activity requests a level hands it undocked.
fn l1_dock(cx: &mut PaneCtx) -> Vec<AnyElement> {
    if !cx.has_activity_decisions {
        return Vec::new();
    }
    cx.activity_decisions.take().into_iter().collect()
}

/// WP-D · the L1 Composer: Main's alone. A subagent takes no prompts, so
/// its transcript runs to the Pane's foot.
fn l1_composer(cx: &mut PaneCtx) -> Option<AnyElement> {
    if !cx.view.is_main() {
        return None;
    }
    let transcript = cx.transcript?;
    Some(
        composer_region(
            cx.view,
            Some(transcript),
            ComposerStack {
                grid: cx.grid,
                decision: cx.decision,
                queued: cx.queued,
                empty: cx.composer_empty,
                files: cx.composer_files,
                attachments: cx.attachments.take(),
                menu: cx.menu.take(),
                draft: false,
                draft_error: None,
                suggestion: cx.suggestion,
                focused: cx.focused,
                drop_target: cx.drop_target,
                joined: cx.decision_joined,
                toolbar: cx.toolbar.take(),
            },
        )
        .into_any_element(),
    )
}

/// Records the card's bounds into the Pane's geometry each prepaint. The
/// edge is an overlay inside the Pane's box, so the box is the card.
fn record_card(shell: Div, view: &PaneView) -> Div {
    let geometry = view.geometry.clone();
    components::on_bounds(shell, move |bounds, _, _| {
        geometry.set(PaneGeometry {
            card: Some(bounds),
            ..geometry.get()
        })
    })
}

/// The Pane on the plane (theme rule 2, SPEC §5.3): `paint::CARD`, its
/// content flush to its own edges — the ring is an overlay (`pane_frame`),
/// never a border in layout. A Group's Pane is a cell at `R_CARD`; a Solo
/// Pane (`PaneEdge::Flush`) is the plane itself, square. `overflow:
/// hidden` clips the children to the box.
fn pane_shell(edge: PaneEdge) -> Div {
    let radius = if edge == PaneEdge::Flush {
        0.0
    } else {
        theme::R_CARD
    };
    div()
        .relative()
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .min_w_0()
        .rounded(px(radius))
        .bg(theme::paint::CARD)
        .font_family(theme::FONT_UI)
        .overflow_hidden()
}

/// What a Pane card's 1px ring says (F-2, SPEC §5.9): a waiting Pane
/// (`WAIT_RING`, focused or not) beats focus (`FOCUS_RING`), and a Pane
/// with neither rests on `CARD_RING`. A Solo Pane has no ring: it is the
/// plane itself, flush with the chrome's hairlines (theme rule 2), and its
/// Decision carries the signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaneEdge {
    Attention,
    Focused,
    Rest,
    Flush,
}

impl PaneEdge {
    pub(crate) fn of(focused: bool, attention: bool, solo: bool) -> Self {
        if solo {
            PaneEdge::Flush
        } else if attention {
            PaneEdge::Attention
        } else if focused {
            PaneEdge::Focused
        } else {
            PaneEdge::Rest
        }
    }

    pub(crate) fn ink(self) -> gpui::Hsla {
        match self {
            PaneEdge::Attention => rgba(WAIT_RING).into(),
            PaneEdge::Focused => rgba(FOCUS_RING).into(),
            PaneEdge::Rest => theme::paint::CARD_RING.into(),
            PaneEdge::Flush => rgba(theme::TRANSPARENT).into(),
        }
    }
}

/// The Pane root (F-1): the card, then its 1px ring (`card_ring`) — it
/// takes no layout and no pointer, so the content starts at the card's own
/// edges and a state change moves nothing. Every Pane root is `PANE_GROUP`.
fn pane_frame(pane: Div, edge: PaneEdge, key: u64) -> Stateful<Div> {
    let overlay = match edge {
        PaneEdge::Focused => {
            Some(card_ring(edge.ink()).debug_selector(move || format!("pane-focus-edge-{key}")))
        }
        PaneEdge::Attention => {
            Some(card_ring(edge.ink()).debug_selector(move || format!("pane-waiting-edge-{key}")))
        }
        PaneEdge::Rest => Some(card_ring(edge.ink())),
        PaneEdge::Flush => None,
    };
    div()
        .id(gpui::ElementId::Name(SharedString::from(format!(
            "pane-edge-{key}"
        ))))
        .debug_selector(move || format!("pane-root-{key}"))
        .group(PANE_GROUP)
        .relative()
        .flex()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .child(pane)
        .children(overlay)
}

/// A card's 1px ring in `ink`, just outside its box
/// (`components::outer_ring`), in the gutter between a Group's cells. The
/// cockpit rings a reader's card with it.
pub fn card_ring(ink: impl Into<gpui::Hsla>) -> Div {
    div()
        .absolute()
        .inset_0()
        .child(components::outer_ring(theme::R_CARD, ink))
}

// ------------------------------------------------------------------ drafts

/// Everything a draft Pane draws (#29), assembled in the cockpit where the
/// clicks are wired — the Pane only lays it out.
pub struct DraftState<'a> {
    pub attachments: Option<AnyElement>,
    /// The draft-only close control in the Pane header.
    pub discard: AnyElement,
    /// The open `/` or `@` menu, hung above the Composer like every menu.
    pub menu: Option<AnyElement>,
    pub composer_empty: bool,
    /// How many files wait in the draft (`PaneFacts::composer_files`).
    pub composer_files: usize,
    pub focused: bool,
    /// A failed bootstrap's words, shown where the band is.
    pub error: Option<&'a SharedString>,
    /// The operator asked for reduced motion.
    pub reduce_motion: bool,
    /// Native files hover the draft (`PaneFacts::drop_target`).
    pub drop_target: bool,
    /// More than one Pane is on the board (`PaneFacts::show_focus`).
    pub show_focus: bool,
    /// The Solo box's controls (`PaneWiring::toolbar`).
    pub toolbar: Option<ComposerControls>,
    /// The hero's project word (`hero_project`), wired in the cockpit to
    /// the Project picker. None on a board, where the cell has no room.
    pub start: Option<AnyElement>,
}

/// What a draft is called until its first send names the Thread — in its
/// Group head.
pub const DRAFT_TITLE: &str = "New thread";

/// A draft Pane (#29): an empty transcript area and the Composer wearing
/// the pre-prompt band. In Solo it has no head. On a board it wears the
/// Group head (the glyph column reserved, no dot: nothing runs yet) with the
/// × in its slot, then the grid's one Composer line; below L1 the body is
/// the head and that line alone.
pub fn render_draft(view: &PaneView, state: DraftState<'_>, level: Level) -> impl IntoElement {
    let DraftState {
        attachments,
        discard,
        menu,
        composer_empty,
        composer_files,
        focused,
        error,
        reduce_motion,
        drop_target,
        show_focus,
        toolbar,
        start,
    } = state;
    // A draft wears the live Pane's edge: transparent at rest or, beside
    // other Panes, the focus ink. It has no state to announce.
    let framed = focused && show_focus;
    let edge = PaneEdge::of(framed, false, !show_focus);
    let key = view.identity.draft().map_or(0, DraftId::get);
    let mut shell = record_card(pane_shell(edge), view);
    if show_focus {
        shell = shell.child(group_head(GroupHead {
            key,
            name: view.name.clone(),
            mark: None,
            word: None,
            focused: framed,
            title: None,
            provider: None,
            action: Some(discard),
            width: None,
        }));
    }
    let _ = reduce_motion;
    let composer = composer_region(
        view,
        None,
        ComposerStack {
            grid: show_focus,
            decision: None,
            queued: &[],
            empty: composer_empty,
            files: composer_files,
            attachments,
            menu,
            draft: true,
            draft_error: error.cloned(),
            // A draft has no conversation yet, so nothing to predict.
            suggestion: None,
            focused,
            drop_target,
            joined: false,
            toolbar,
        },
    );
    // Codex's home: at the body's centre the hero, and the Composer at the
    // foot, where the Thread it becomes keeps it: two rows tall, Codex's,
    // until the first send makes it Claude's one-row box.
    let card = start.filter(|_| level == Level::Transcript).map(start_hero);
    let body = div()
        .debug_selector(|| "draft-empty".into())
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .px(px(theme::SPACE_4))
        .children(card);
    pane_frame(
        shell
            .child(body)
            .children(drop_target.then(crate::prompt_drop::sheet))
            // The Composer holds itself to the reading column
            // (`TX_COLUMN_MAX_W`, `in_column`).
            .when(level != Level::Wall, |pane| pane.child(composer)),
        edge,
        u64::MAX - key,
    )
}

/// An empty Thread's or a draft's hero, Codex's home: Ferrite's mark in
/// `HERO_MARK_INK`, then `What should we build in <project>?` at the home's
/// display size, `project` the word the cockpit wires (`hero_project`).
pub fn start_hero(project: AnyElement) -> Div {
    div()
        .debug_selector(|| "start-hero".into())
        .flex()
        .flex_col()
        .items_center()
        .gap(px(theme::HERO_MARK_GAP))
        .max_w_full()
        .child(components::embossed_mark(
            theme::HERO_MARK,
            theme::HERO_MARK_INK,
        ))
        .child(
            div()
                .flex()
                .flex_wrap()
                .justify_center()
                .items_baseline()
                .max_w_full()
                .font_family(theme::FONT_UI)
                .font_weight(theme::W_BODY)
                .text_size(px(theme::FS_HOME_TITLE))
                .line_height(px(theme::LH_HOME_TITLE))
                .text_color(rgb(INK_STRONG))
                .child("What should we build in\u{a0}")
                .child(project)
                .child("?"),
        )
}

/// The hero's project word: the Project's name, and while it can still
/// change (a draft's) a switcher under a dashed `INK_SUBTLE` rule, Codex's.
/// The cockpit wires its press.
pub fn hero_project(label: SharedString, switcher: bool) -> Stateful<Div> {
    div()
        .id("start-hero-project")
        .debug_selector(|| "start-hero-project".into())
        .min_w_0()
        .truncate()
        .when(switcher, |word| {
            word.cursor_pointer()
                .border_b_1()
                .border_dashed()
                .border_color(rgb(INK_SUBTLE))
        })
        .child(label)
}

/// A Draft is disposable state, so its Pane advertises the same close action
/// as cmd-w: an `ICON_BUTTON` × in `INK_SUBTLE` — the Group head's slot on a
/// board (Solo has no head: cmd-w alone). Its tooltip names the key
/// only where one is bound. Live Threads deliberately keep keyboard and
/// context-menu closure instead of adding this control to every Pane.
pub fn draft_close_button(draft: DraftId) -> gpui::component::button::Button {
    components::button(("discard-draft", draft.get() as usize))
        .debug_selector(|| "discard-draft".into())
        .flex_shrink_0()
        .w(px(theme::ICON_BUTTON))
        .h(px(theme::ICON_BUTTON))
        .p_0()
        .accessibility_label("Discard draft")
        .child(icon(icons::CLOSE, theme::ICON_BUTTON_GLYPH, INK_STRONG))
}

/// The × with its tooltip: `Discard draft`, and the key after it only where
/// one is bound (`menu::tooltip_with_key`).
pub fn draft_discard(draft: DraftId, button: AnyElement, keys: Option<String>) -> Stateful<Div> {
    div()
        .id(("discard-draft-tip", draft.get() as usize))
        .flex_shrink_0()
        .tooltip(ferrite_ui::menu::tooltip_with_key("Discard draft", keys))
        .child(button)
}

/// A draft's Project or Workspace chip on the strip over the box
/// (`env_strip`, Codex's): its mark and the choice, ringed by the one focus
/// recipe (`components::focused`) while tab rests on it, because the
/// popover opens on ↵ and the chip must say where ↵ will land. In a narrow
/// Pane its label cuts first.
pub fn band_chip(
    slot: usize,
    mark: &'static str,
    label: SharedString,
    focused: bool,
) -> Stateful<Div> {
    // The label truncates in a narrow Pane; the tooltip keeps the whole
    // choice reachable and names the keys that change it.
    let tooltip = SharedString::from(format!("{label} \u{b7} Tab, then \u{21b5} to change"));
    div()
        .id(("band-chip", slot))
        .tooltip(ferrite_ui::menu::tooltip(tooltip))
        .debug_selector(move || format!("band-chip-{slot}"))
        .flex()
        .flex_shrink(1.)
        .min_w_0()
        .items_center()
        .h(px(theme::FOOTER_CHIP_H))
        .px(px(theme::FOOTER_CHIP_PAD_X))
        .rounded(px(theme::R_CONTROL))
        .map(|chip| components::focused(chip, focused))
        .hover_raised(format!("band-chip-{slot}"))
        .press_raised()
        .child(env_face(mark, label))
}

/// A band chip's text: the choice itself (the chip draws its own chevron).
pub fn band_chip_label(choice: &str) -> SharedString {
    SharedString::from(choice.to_owned())
}

/// A draft's model or effort chip: the live Composer's own toolbar chip,
/// ringed like the band chip while tab rests on it, so tab still says
/// where ↵ will land.
pub fn draft_picker(
    id: &'static str,
    focused: bool,
    face: Div,
    cx: &gpui::App,
) -> gpui::component::button::Button {
    toolbar_chip(id, cx)
        .debug_selector(move || id.to_string())
        .map(|chip| components::focused(chip, focused))
        .child(face)
}

/// Everything one wall tile reads, gathered by `render_pane`.
struct WallTile<'a> {
    key: u64,
    card: &'a WallCard,
    state: WallState,
    kind: Option<&'static str>,
    turn: Option<&'a Turn>,
    /// The tile's finish has not been seen: it reads `done`.
    unread: bool,
    quick_answers: Option<AnyElement>,
    /// The tile holds the keyboard: its working clock keeps its seconds
    /// past a minute (`1m04s`); the others round to `1m`.
    focused: bool,
    /// The tile's width, for the prose's one wrap, measured by
    /// `text_system` (none for a parked body, which never wraps).
    cell_width: f32,
    text_system: Option<&'a std::sync::Arc<gpui::TextSystem>>,
    /// A parked Thread's facts line (`Parked 2h ago · 11 turns`).
    parked_line: Option<SharedString>,
}

/// The wall's tile body (F-6), under the one head: the state word at the
/// strong weight — `working 1m04s` (the word `RUNNING`, its clock dim),
/// `needs you` (`ATTENTION`), `done · 3m 12s` (dim), `failing 2`
/// (`BLOCKED`), `parked` (dim) — half a row, then at most `WALL_LINES`
/// rows, each one dim run with its glyph (`wall_rows`), and, while the
/// Thread waits, its quick answers at the foot. No meter, no sparkline. A
/// parked tile reads its facts and how to wake it, with no glyph gutter.
fn wall_cell(tile: WallTile<'_>) -> Div {
    let WallTile {
        key,
        card,
        state,
        kind,
        turn,
        unread,
        quick_answers,
        focused,
        cell_width,
        text_system,
        parked_line,
    } = tile;
    let word = match state {
        WallState::Parked => Some(HeadSlot::Parked),
        _ => match state_word(state, kind, card, turn, unread) {
            Some(HeadSlot::Working(_)) => Some(HeadSlot::Working(working_clock(turn, focused))),
            word => word,
        },
    };
    let signal = word.map(|word| {
        // What the word stands for, one hover away: a working tile's
        // caption, a failed one's reason.
        let detail = match word {
            HeadSlot::Working(_) => Some(card.working.clone()),
            HeadSlot::Failed => Some(card.context.clone()),
            _ => None,
        }
        .filter(|detail| !detail.is_empty());
        let face = wall_word(&word, card);
        match detail {
            Some(detail) => div()
                .id(("wall-word", key as usize))
                .tooltip(ferrite_ui::menu::tooltip(detail))
                .child(face)
                .into_any_element(),
            None => face.into_any_element(),
        }
    });
    let rows: Vec<WallRow> = match (state, text_system) {
        (WallState::Parked, _) | (_, None) => vec![
            WallRow::plain(parked_line.unwrap_or_else(|| SharedString::from(theme::words::PARKED))),
            WallRow::plain(WAKE_HINT),
        ],
        (_, Some(text_system)) => {
            // Where the tile's own face breaks the line: its text column,
            // in Geist at the tile's size.
            let width = px((cell_width - 2.0 * theme::WALL_PAD_X).max(0.0));
            let mut wrapper =
                text_system.line_wrapper(gpui::font(theme::FONT_UI), px(theme::FS_UI));
            wall_rows(&card.lines, |line| {
                wrapper
                    .wrap_line(&[gpui::LineFragment::text(line)], width)
                    .next()
                    .map(|boundary| boundary.ix)
            })
        }
    };
    div()
        .debug_selector(|| "wall-tile".into())
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .overflow_hidden()
        .py(px(theme::WALL_PAD_Y))
        .px(px(theme::WALL_PAD_X))
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .child(
            div()
                .debug_selector(|| "wall-signal".into())
                .flex_shrink_0()
                .w_full()
                .min_w_0()
                .h(px(theme::LH_UI))
                .children(signal),
        )
        .child(div().flex_shrink_0().h(px(theme::HALF_ROW)))
        .children(rows.into_iter().map(wall_line))
        .child(div().flex_1().min_h_0())
        .children(quick_answers)
}

/// What a parked tile's second line says: the one key that wakes it.
const WAKE_HINT: &str = "\u{23ce} reopens it";

/// A working tile's clock: whole seconds (`12s`, `1m04s`) on the tile that
/// holds the keyboard; elsewhere a minute and past reads in minutes alone
/// (`1m`), as the prototype's quiet Panes do.
fn working_clock(turn: Option<&Turn>, focused: bool) -> String {
    let Some(elapsed) = turn.and_then(Turn::elapsed) else {
        return String::new();
    };
    if !focused && elapsed.as_secs() >= 60 {
        format!("{}m", elapsed.as_secs() / 60)
    } else {
        ferrite_core::progress::live_seconds(elapsed)
    }
}

/// The tile's state word at the strong weight: the word in its state ink
/// and what follows it (`1m04s`) dim; a finished turn's whole run
/// (`done · 3m 12s`) and a parked tile's are dim.
fn wall_word(word: &HeadSlot, card: &WallCard) -> Div {
    let (lead, rest): (String, Option<String>) = match word {
        HeadSlot::NeedsYou(_) => (theme::words::NEEDS_YOU.into(), None),
        HeadSlot::Working(elapsed) if !elapsed.is_empty() => {
            (theme::words::WORKING.into(), Some(elapsed.clone()))
        }
        HeadSlot::Done => (
            match card.done_after.as_ref() {
                Some(after) => format!("{} \u{b7} {after}", theme::words::DONE),
                None => theme::words::DONE.into(),
            },
            None,
        ),
        word => (word.text(), None),
    };
    let ink = match word {
        HeadSlot::NeedsYou(_) => ATTENTION,
        HeadSlot::Working(_) => RUNNING,
        word => word.ink(),
    };
    components::tabular(
        div()
            .flex()
            .min_w_0()
            .gap(px(theme::CH))
            .whitespace_nowrap()
            .overflow_hidden()
            .font_weight(theme::W_LABEL)
            .child(div().flex_shrink_0().text_color(rgb(ink)).child(lead))
            .children(rest.map(|rest| {
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(INK_SUBTLE))
                    .child(rest)
            })),
    )
}

/// One of a tile's rows: one `INK_SUBTLE` run, its glyph included, on one
/// line that truncates with `…` at the tile's edge. A continuation lays
/// its line's glyph run out unseen first, so it hangs under the first word.
fn wall_line(row: WallRow) -> Div {
    let line = div()
        .debug_selector(|| "wall-line".into())
        .flex_shrink_0()
        .w_full()
        .min_w_0()
        .h(px(theme::LH_UI))
        .text_color(rgb(INK_SUBTLE));
    match row.hang {
        None => line.truncate().child(row.text),
        Some(hang) => line
            .flex()
            .whitespace_nowrap()
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(rgba(theme::TRANSPARENT))
                    .child(hang),
            )
            .child(div().min_w_0().truncate().child(row.text)),
    }
}

/// The tile's quick answers: boxed words a cell apart, half a row under
/// the lines, the first `WALL_PAD_X` in from the tile's edge and the row
/// `WALL_PAD_Y` above its foot — each the cockpit's wired button
/// (`quick_answer`).
pub fn quick_answers(buttons: Vec<AnyElement>) -> Div {
    div()
        .debug_selector(|| "wall-quick-answers".into())
        .flex()
        .flex_shrink_0()
        .w_full()
        .min_w_0()
        .flex_wrap()
        .gap(px(theme::QUICK_ANSWER_GAP))
        .mt(px(theme::HALF_ROW))
        .children(buttons)
}

/// One quick answer (F-7): `1 allow` as a secondary button, no edge
/// (theme rule: no grey borders) — a `paint::RAISED` face, radius
/// `R_CONTROL`, `QUICK_ANSWER_H` (22px) tall, 1ch in from each side. The digit
/// is `INK_SUBTLE` on every waiting tile, the word `INK_STRONG`; under the
/// pointer the face blends to `paint::SELECTED` over the one 150ms hover
/// blend, the digit staying dim. A label wider than
/// the tile ends in `…` inside the button, the digit kept, and reads whole
/// in its tooltip.
pub fn quick_answer(id: SharedString, key: SharedString, label: SharedString) -> Stateful<Div> {
    let hover = id.clone();
    let ground = ferrite_ui::motion::hover_blend(
        &hover,
        theme::paint::RAISED.hsla(),
        theme::paint::SELECTED.hsla(),
    );
    let selector = id.clone();
    // Its two runs and a cell of padding each side, capped at the tile.
    let key = format!("{key} ");
    let width = components::run_width(&key)
        + components::run_width(&label)
        + 2.0 * theme::QUICK_ANSWER_PAD_X;
    let whole = label.clone();
    div()
        .id(gpui::ElementId::Name(id))
        .debug_selector(move || selector.to_string())
        .flex()
        .flex_shrink_0()
        .items_center()
        .w(px(width))
        .max_w_full()
        .min_w_0()
        .overflow_hidden()
        .tooltip(ferrite_ui::menu::tooltip(whole))
        .h(px(theme::QUICK_ANSWER_H))
        .px(px(theme::QUICK_ANSWER_PAD_X))
        .rounded(px(theme::R_CONTROL))
        .bg(ground)
        .cursor_pointer()
        .whitespace_nowrap()
        .on_hover(ferrite_ui::motion::hover_listener(hover))
        .child(components::cells(key).text_color(rgb(INK_SUBTLE)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(rgb(INK_STRONG))
                .child(label),
        )
}

/// The Pane head's mark: the main Thread's is its nav row's
/// (`mark::RowStatus`) — the done check while its finish is `unread`, the
/// green ring while `background` work runs behind its idle Main. A
/// subagent tab's is that agent's own transcript state.
fn head_mark(
    main: bool,
    state: WallState,
    unread: bool,
    background: bool,
    subject: Option<Status>,
) -> crate::mark::RowStatus {
    if main {
        return crate::mark::RowStatus::of(state, unread, background);
    }
    let state = match subject {
        Some(Status::Streaming) => WallState::Working,
        Some(Status::Blocked) => WallState::Decision,
        Some(Status::Closed) => WallState::Blocked,
        _ => WallState::Idle,
    };
    crate::mark::RowStatus::of(state, false, false)
}

/// What a pending request asks of the operator, as the lexicon names it:
/// a Question, a plan to approve (Claude's `ExitPlanMode`), or an approval.
fn request_kind(decision: &Decision) -> &'static str {
    if questions_of(decision).is_some() {
        theme::words::QUESTION
    } else if decision.tool_name == "ExitPlanMode" {
        theme::words::PLAN
    } else {
        theme::words::APPROVAL
    }
}

/// One lexicon word for where a Thread stands (rule 2.11.2), shared by the
/// Group head's state slot and the wall tile's state word. Each variant
/// is one state; `text` is what it reads, `ink` the one ink it wears
/// where colour is allowed.
#[derive(Clone, Debug, PartialEq)]
pub enum HeadSlot {
    /// `needs you · approval` / `needs you · question` — `ATTENTION` on
    /// `needs you` only.
    NeedsYou(&'static str),
    /// `failing 2` (a test run), or bare `failing`.
    Failing(Option<usize>),
    /// A closed Session, or a turn that ended in an error.
    Failed,
    /// `working 12s`: whole seconds (`progress::live_seconds`).
    Working(String),
    Done,
    Interrupted,
    Parked,
}

impl HeadSlot {
    pub(crate) fn text(&self) -> String {
        use theme::words;
        match self {
            HeadSlot::NeedsYou(kind) => format!("{} \u{b7} {kind}", words::NEEDS_YOU),
            HeadSlot::Failing(Some(count)) => format!("{} {count}", words::FAILING),
            HeadSlot::Failing(None) => words::FAILING.into(),
            HeadSlot::Failed => words::FAILED.into(),
            HeadSlot::Working(elapsed) if elapsed.is_empty() => words::WORKING.into(),
            HeadSlot::Working(elapsed) => format!("{} {elapsed}", words::WORKING),
            HeadSlot::Done => words::DONE.into(),
            HeadSlot::Interrupted => words::INTERRUPTED.into(),
            HeadSlot::Parked => words::PARKED.into(),
        }
    }

    /// The word's ink: colour only where it means something needs you.
    pub(crate) fn ink(&self) -> u32 {
        match self {
            HeadSlot::Failing(_) | HeadSlot::Failed => BLOCKED,
            _ => INK_SUBTLE,
        }
    }
}

/// The state vocabulary, shared by the Group head and the wall:
/// `needs you` > `failing` > `failed` > `working` > `done` >
/// `interrupted` > `parked`; idle is nothing. A finish is `done` only while
/// `unread` (not yet seen), as the nav row's mark is
/// (`mark::RowStatus::of`); seen, it is idle. A turn that ended in an error
/// is a finish too: `failed` until seen, then idle (its failure line stays
/// in the transcript). A Session that closed under its Thread stays
/// `failed`: that is a standing state, not a finish.
fn state_word(
    state: WallState,
    kind: Option<&'static str>,
    card: &WallCard,
    turn: Option<&Turn>,
    unread: bool,
) -> Option<HeadSlot> {
    use ferrite_core::TurnOutcome;
    // A request pending anywhere in the Thread (a subagent's too) is what
    // needs you, whatever Main is doing.
    if let Some(kind) = kind {
        return Some(HeadSlot::NeedsYou(kind));
    }
    Some(match state {
        WallState::Decision => HeadSlot::NeedsYou(kind.unwrap_or(theme::words::APPROVAL)),
        WallState::Failing => HeadSlot::Failing(card.failing_count),
        WallState::Blocked => HeadSlot::Failed,
        WallState::Working => HeadSlot::Working(
            turn.and_then(Turn::elapsed)
                .map(ferrite_core::progress::live_seconds)
                .unwrap_or_default(),
        ),
        WallState::Parked => HeadSlot::Parked,
        WallState::Done | WallState::Idle => match turn.and_then(|turn| turn.outcome.as_ref()) {
            Some(TurnOutcome::Error(_)) if unread => HeadSlot::Failed,
            _ if unread => HeadSlot::Done,
            Some(TurnOutcome::Interrupted) => HeadSlot::Interrupted,
            _ => return None,
        },
    })
}

/// The state a Thread's dot shows: its wall state, squared with the state
/// word beside it so the two can never disagree. A turn that ended in an
/// error reads `failed` until seen, so its dot is `BLOCKED` like a closed
/// Session's until then;
/// a `failing` word is always a `BLOCKED` dot. Every surface that draws a
/// Thread's dot — the nav, the rail, the Group head — reads
/// it through here.
pub fn dot_state(state: WallState, word: Option<&HeadSlot>) -> WallState {
    match word {
        Some(HeadSlot::Failed) if state != WallState::Blocked => WallState::Blocked,
        Some(HeadSlot::Failing(_)) => WallState::Failing,
        _ => state,
    }
}

/// A Thread's state word away from its Pane, read exactly as the Pane
/// reads it.
pub fn thread_word(
    open: ThreadView<'_>,
    card: Option<&WallCard>,
    unread: bool,
) -> Option<HeadSlot> {
    let empty = WallCard::default();
    let card = card.unwrap_or(&empty);
    let turn = open.turn();
    let decision = open.pending();
    let state = wall_state(
        Some(&turn),
        decision.is_some_and(Decision::blocks_execution),
        card.tests_failing,
    );
    let pending = open.pending_decisions();
    let kind = (!pending.is_empty() || state == WallState::Decision).then(|| {
        pending
            .first()
            .map(|request| request_kind(&request.decision))
            .or_else(|| decision.map(request_kind))
            .unwrap_or(theme::words::APPROVAL)
    });
    state_word(state, kind, card, Some(&turn), unread)
}

/// The Pane's foot while its question is too big for the body: `◆ needs
/// you · question` and the key that answers it in full, as a door — a press
/// runs the ⌘D jump. It sits where the Pane's state reads (theme rule 7).
fn question_door(key: u64) -> AnyElement {
    let expand = components::bound_chord("cockpit::ToggleFullscreen");
    let face = div()
        .debug_selector(|| "question-expand".into())
        .flex()
        .flex_shrink(1.)
        .min_w_0()
        .items_center()
        .gap(px(theme::CH))
        .whitespace_nowrap()
        .overflow_hidden()
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(INK_SUBTLE))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .w(px(theme::GLYPH_GUTTER - theme::CH))
                .child(components::glyph("\u{25c6}", ATTENTION)),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_color(rgb(ATTENTION))
                .child(theme::words::NEEDS_YOU),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_color(rgb(TEXT_FAINT))
                .child("\u{b7}"),
        )
        .child(div().flex_shrink_0().child(theme::words::QUESTION))
        .children(expand.map(|keys| {
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(theme::CH))
                .child(div().text_color(rgb(TEXT_FAINT)).child("\u{b7}"))
                .child(components::key_combo(&keys, INK_SUBTLE))
                .child("answers in full")
        }));
    div()
        .flex()
        .flex_shrink_0()
        .w_full()
        .min_w_0()
        .h(px(theme::PANE_HEAD_H))
        .items_center()
        .px(px(theme::GLYPH_GUTTER))
        .child(needs_you_door(
            SharedString::from(format!("pane-needs-you-{key}")),
            SharedString::from(format!("pane-question-{key}")),
            face,
        ))
        .into_any_element()
}

// ------------------------------------------------------------- tile text

// ------------------------------------------------------------- the head

/// The one Group head (SPEC §5.9), for a transcript Pane and a wall tile
/// alike: one `PANE_HEAD_H` row (38px) closed by a `LINE`, padded
/// `PANE_HEAD_PAD_L` / `PANE_HEAD_PAD_R`, its items `PANE_HEAD_GAP` apart.
/// In order: the state mark (`mark::head_mark`), the title in `FS_LABEL`
/// at `W_LABEL` — `INK_STRONG` on the focused Pane, `INK_SUBTLE` elsewhere —
/// kept to its `title_floor`, the state word in `FS_META` and its ink, a
/// spacer, the provider's 13px logo and the trailing control: the fill
/// button, or a draft's ×. No band on the focused head: the ring says focus.
///
/// Narrow, it gives way in order, never slicing a glyph: the title
/// ellipsizes to its floor, then the word ellipsizes, and where even the
/// floor does not fit beside the mark, the logo and the control (`width`),
/// the floor gives way too. The mark, the logo and the control keep their
/// size, and the control stands outside the part that gives way, so the
/// board can always be filled from the head.
pub(crate) struct GroupHead {
    pub key: u64,
    pub name: SharedString,
    /// The Thread's mark; a draft has none.
    pub mark: Option<crate::mark::RowStatus>,
    /// The state word and its ink (`mark::NavTail::member_word`).
    pub word: Option<(SharedString, u32)>,
    /// The Pane holds the keyboard on a board: a strong title.
    pub focused: bool,
    /// The wired title (drag handle, double-click rename); `None` draws the
    /// name.
    pub title: Option<AnyElement>,
    pub provider: Option<Provider>,
    /// The trailing control: the fill button, or a draft's ×.
    pub action: Option<AnyElement>,
    /// The head's width this frame, when the board knows it.
    pub width: Option<f32>,
}

pub(crate) fn group_head(head: GroupHead) -> Div {
    let GroupHead {
        key,
        name,
        mark,
        word,
        focused,
        title,
        provider,
        action,
        width,
    } = head;
    let title_ink = if focused { INK_STRONG } else { INK_SUBTLE };
    let fit = HeadFit::of(
        width,
        mark.is_some(),
        word.as_ref().map(|(text, _)| text.as_ref()),
        provider.is_some(),
        action.is_some(),
        title_floor(&name),
    );
    let word = word.filter(|_| fit.word);
    let provider = provider.filter(|_| fit.provider);
    let floor = fit.floor;
    div()
        .debug_selector(move || format!("pane-head-{key}"))
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::PANE_HEAD_GAP))
        // The hairline lies under the row's 38, as the mockup's border does.
        .h(px(theme::PANE_HEAD_H + 1.))
        .pl(px(theme::PANE_HEAD_PAD_L))
        .pr(px(theme::PANE_HEAD_PAD_R))
        .border_b_1()
        .border_color(theme::paint::LINE)
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_LABEL))
        .line_height(px(theme::LH_UI))
        .child(
            // What gives way: everything but the control.
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(px(theme::PANE_HEAD_GAP))
                .overflow_hidden()
                .children(mark.map(|mark| {
                    div()
                        .debug_selector(move || format!("pane-head-dot-{key}"))
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .h(px(theme::LH_UI))
                        .child(crate::mark::head_mark(mark))
                }))
                .child(
                    div()
                        .debug_selector(move || format!("pane-head-title-{key}"))
                        .flex()
                        // Far the readier to shrink: the title gives way
                        // to its floor before the word gives any.
                        .flex_shrink(theme::HEAD_TITLE_SHRINK)
                        .min_w(px(floor))
                        .overflow_hidden()
                        .font_weight(theme::W_LABEL)
                        .text_color(rgb(title_ink))
                        .child(match title {
                            Some(title) => title,
                            None => div().min_w_0().truncate().child(name).into_any_element(),
                        }),
                )
                .children(word.map(|(text, ink)| {
                    div()
                        .debug_selector(move || format!("pane-head-word-{key}"))
                        .flex_shrink(1.)
                        .min_w_0()
                        .truncate()
                        .text_size(px(theme::FS_META))
                        .text_color(rgb(ink))
                        .child(text)
                }))
                .child(div().flex_1())
                .children(provider.map(|provider| {
                    let (glyph, ink) = ferrite_ui::provider::mark(provider);
                    div()
                        .debug_selector(move || format!("pane-head-provider-{key}"))
                        .flex()
                        .flex_shrink_0()
                        .child(icon(glyph, theme::PANE_HEAD_ICON, ink))
                })),
        )
        .children(action)
}

/// What a Group head shows at its width: past what the title's floor and
/// the word need beside the fixed parts, the logo goes first, then the
/// word, then the floor itself gives way to what is left.
#[derive(Clone, Copy, Debug, PartialEq)]
struct HeadFit {
    word: bool,
    provider: bool,
    floor: f32,
}

impl HeadFit {
    fn of(
        width: Option<f32>,
        mark: bool,
        word: Option<&str>,
        provider: bool,
        action: bool,
        floor: f32,
    ) -> Self {
        let Some(width) = width else {
            return Self {
                word: word.is_some(),
                provider,
                floor,
            };
        };
        // The word at its shortest likely run, and the gap before it.
        let word_w = word.map_or(0.0, |word| {
            word.chars().count() as f32 * theme::FS_META * theme::UI_ADVANCE_FLOOR
                + theme::PANE_HEAD_GAP
        });
        // The title's room beside the pads, the mark, the logo, the
        // control and the gaps (the spacer stands between two of them).
        let part = |shown: bool, w: f32| if shown { w + theme::PANE_HEAD_GAP } else { 0.0 };
        let room = |provider: bool| {
            width
                - theme::PANE_HEAD_PAD_L
                - theme::PANE_HEAD_PAD_R
                - part(mark, theme::PANE_HEAD_DOT_W)
                - part(provider, theme::PANE_HEAD_ICON)
                - part(action, theme::ICON_BUTTON)
        };
        if room(provider) >= floor + word_w {
            return Self {
                word: word.is_some(),
                provider,
                floor,
            };
        }
        if room(false) >= floor + word_w {
            return Self {
                word: word.is_some(),
                provider: false,
                floor,
            };
        }
        Self {
            word: false,
            provider: false,
            floor: floor.min(room(false).max(0.0)),
        }
    }
}

/// The head's fill button (SPEC §5.9): a 28px `maximize` door, `RAISED`
/// under the pointer, whose press the cockpit wires to fill the board with
/// this Pane (`ToggleFullscreen`), its tooltip `Fill the board ⌘⇧⏎` read
/// from the keymap.
pub fn fill_button(key: u64) -> Stateful<Div> {
    let blend = SharedString::from(format!("pane-fill-{key}"));
    let t = ferrite_ui::motion::hover_t(&blend);
    div()
        .id(gpui::ElementId::Name(blend.clone()))
        .debug_selector(move || format!("pane-fill-{key}"))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(theme::ICON_BUTTON))
        .rounded(px(theme::R_CONTROL))
        .cursor_pointer()
        .when(t > 0.0, |door| {
            door.bg(ferrite_ui::motion::mix(
                rgba(TRANSPARENT).into(),
                theme::paint::RAISED.into(),
                t,
            ))
        })
        .on_hover(ferrite_ui::motion::hover_listener(blend))
        .tooltip(ferrite_ui::menu::action_tooltip(
            "Fill the board",
            "cockpit::ToggleFullscreen",
        ))
        .child(icon(
            icons::MAXIMIZE,
            theme::PANE_HEAD_FILL_GLYPH,
            INK_STRONG,
        ))
}

/// The failure strip (SPEC §5.9 item 4), the transcript's tail row under
/// a failed turn's failure line, Solo's and a Group Pane's alike:
/// `FAILURE_STRIP_H` at `R_CARD` on `FAILURE_WASH`, an `x-circle` in
/// `BLOCKED`, why it failed (`API Error: 529 overloaded`) in `INK_STRONG`, cut to
/// one line, and at the right the wired `Retry` (`retry_button`), which a
/// closed Session goes without.
pub fn failure_strip(key: u64, words: SharedString, retry: Option<AnyElement>) -> Div {
    div()
        .debug_selector(move || format!("failure-strip-{key}"))
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::SPACE_2))
        .w_full()
        .min_w_0()
        .h(px(theme::FAILURE_STRIP_H))
        .pl(px(theme::FAILURE_STRIP_PAD_L))
        .pr(px(theme::FAILURE_STRIP_PAD_R))
        .rounded(px(theme::R_CARD))
        .bg(rgba(theme::FAILURE_WASH))
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_LABEL))
        .line_height(px(theme::LH_UI))
        .whitespace_nowrap()
        .child(icon(icons::X_CIRCLE, theme::FAILURE_STRIP_ICON, BLOCKED))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_color(rgb(INK_STRONG))
                .child(words),
        )
        .child(div().flex_1())
        .children(retry)
}

/// The strip's `Retry`: `RETRY_H` at `R_CONTROL` on `SELECTED`, padded
/// `RETRY_PAD_X`, its label at `W_LABEL` in `INK_STRONG`. The caller wires
/// the press.
pub fn retry_button(key: u64, cx: &gpui::App) -> gpui::component::button::Button {
    components::faded_button(
        ("failure-retry", key as usize),
        theme::paint::SELECTED.into(),
        theme::paint::SELECTION_HOVER.into(),
        theme::paint::PRESS.into(),
        rgb(INK_STRONG).into(),
        cx,
    )
    .debug_selector(move || format!("failure-retry-{key}"))
    .flex_shrink_0()
    .h(px(theme::RETRY_H))
    .px(px(theme::RETRY_PAD_X))
    .rounded(px(theme::R_CONTROL))
    .child(
        components::text_ui()
            .text_size(px(theme::FS_LABEL))
            .font_weight(theme::W_LABEL)
            .text_color(rgb(INK_STRONG))
            .child("Retry"),
    )
}

/// `needs you` as a door: a press runs the ⌘D jump (`NextDecision`) from
/// wherever the keyboard is — the press does not land on a Pane first — and
/// the tooltip names that key: `Next needs you ⌘D`. The Group head's
/// slot draws it.
pub(crate) fn needs_you_door(id: SharedString, selector: SharedString, face: Div) -> AnyElement {
    let door = div()
        .id(gpui::ElementId::Name(id))
        .debug_selector(move || selector.to_string())
        .flex()
        .flex_shrink(1.)
        .min_w_0()
        .overflow_hidden()
        .cursor_pointer()
        .child(face)
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(|_, window, cx| window.dispatch_action(Box::new(NextDecision), cx));
    door.tooltip(ferrite_ui::menu::action_tooltip(
        "Next needs you",
        "cockpit::NextDecision",
    ))
    .into_any_element()
}

/// The row a subagent's head (`subagent_head`) stands in, over its
/// transcript: under the head on a board, at the card's top edge in Solo —
/// one `SUBAGENT_HEAD_H` row padded `SUBAGENT_HEAD_PAD_X`, closed by a
/// `LINE`.
fn subagent_strip(key: u64, head: AnyElement) -> Div {
    div()
        .debug_selector(move || format!("pane-subagent-{key}"))
        .flex()
        .flex_shrink_0()
        .items_center()
        // The rule lies under the row's 38, as the mockup's border does.
        .h(px(theme::SUBAGENT_HEAD_H + 1.))
        .px(px(theme::SUBAGENT_HEAD_PAD_X))
        .border_b_1()
        .border_color(theme::paint::LINE)
        .text_color(rgb(INK_SUBTLE))
        .child(head)
}

/// The floor a head title keeps however narrow its head: `HEAD_TITLE_MIN_W`,
/// or about its whole text when that is shorter. The title is proportional
/// UI text, so its width is estimated from below (`UI_ADVANCE_FLOOR` a
/// character): a short title is never padded past its text, and only a head
/// squeezed to its floor can clip one's last letter.
fn title_floor(name: &str) -> f32 {
    (name.chars().count() as f32 * theme::FS_UI * theme::UI_ADVANCE_FLOOR)
        .floor()
        .min(theme::HEAD_TITLE_MIN_W)
}

/// A run's mark in the shared check vocabulary (`components::CheckMark`),
/// from its rollup state and the forge's own detail:
/// a pending run is running unless the forge says it waits.
pub fn check_mark(state: CheckState, detail: &str) -> components::CheckMark {
    use components::CheckMark;
    match state {
        CheckState::Failing => CheckMark::Failed,
        CheckState::Pending if check_waits(detail) => CheckMark::Queued,
        CheckState::Pending => CheckMark::Running,
        CheckState::Passing => CheckMark::Passed,
        CheckState::Skipped => CheckMark::Skipped,
    }
}

/// Whether the forge's detail says a pending run has not started.
fn check_waits(detail: &str) -> bool {
    let detail = detail.to_ascii_lowercase();
    ["queued", "waiting", "pending", "requested"]
        .iter()
        .any(|word| detail.contains(word))
}

/// A run's state in the lexicon's lowercase words — `failed`, `running`,
/// `queued`, `passed`, `skipped`, `cancelled` — from its rollup state and
/// the forge's own detail (`in_progress`, `timed_out`, …), which the row
/// keeps in its tooltip.
pub fn check_word(state: CheckState, detail: &str) -> &'static str {
    let detail = detail.to_ascii_lowercase();
    if detail.contains("cancel") {
        return "cancelled";
    }
    match state {
        CheckState::Failing => theme::words::FAILED,
        CheckState::Pending if check_waits(&detail) => "queued",
        CheckState::Pending => "running",
        CheckState::Passing => "passed",
        CheckState::Skipped => "skipped",
    }
}

/// The checks card: the one floating surface, for the cockpit to fill
/// with `checks_head` and the `check_row`s it has wired. It grows to its
/// content between `CHECKS_CARD_W` and `CHECKS_CARD_MAX_W`, because the
/// runs it lists are named by the forge and a job name — or the tally — is
/// longer than a menu row.
pub fn checks_card() -> Div {
    components::floating_surface()
        .min_w(px(theme::CHECKS_CARD_W))
        .max_w(px(theme::CHECKS_CARD_MAX_W))
}

/// The card's heading: the label `Checks` and the PR's number at the
/// left, and how its runs divide at the right — the counts the head's chip
/// had no room for. Only
/// states with runs in them are named, so the line never reads `0 failed`,
/// and only the failure is coloured; its figures are tabular, so a run
/// finishing never shifts the tally. Space, not a rule, separates summary
/// from runs (`CHECKS_CARD_GAP`); the heading, the workflow titles and the
/// runs' dots share one leading edge, a menu row's.
pub fn checks_head(pr: &PullRequest) -> Div {
    let tally = pr.tally();
    let parts: Vec<(String, u32)> = [
        (tally.failing, "failed", BLOCKED),
        (tally.pending, "running", INK_SUBTLE),
        (tally.passing, "passed", INK_SUBTLE),
        (tally.skipped, "skipped", INK_SUBTLE),
    ]
    .into_iter()
    .filter(|(count, _, _)| *count > 0)
    .map(|(count, word, ink)| (format!("{count} {word}"), ink))
    .collect();
    // One run, so a narrow card truncates it with an ellipsis; the `·`
    // seams are structure ink and only the failure is coloured.
    let mut text = String::new();
    let mut runs = Vec::new();
    for (index, (part, ink)) in parts.into_iter().enumerate() {
        if index > 0 {
            let at = text.len();
            text.push_str(" · ");
            runs.push((at..text.len(), TEXT_FAINT));
        }
        let at = text.len();
        text.push_str(&part);
        if ink != INK_SUBTLE {
            runs.push((at..text.len(), ink));
        }
    }
    let runs = runs
        .into_iter()
        .map(|(range, ink)| {
            (
                range,
                HighlightStyle {
                    color: Some(rgb(ink).into()),
                    ..Default::default()
                },
            )
        })
        .collect::<Vec<_>>();
    let tally_line = div()
        .debug_selector(|| "checks-tally".into())
        .min_w_0()
        .truncate()
        .text_color(rgb(INK_SUBTLE))
        .child(StyledText::new(text).with_highlights(runs));
    let tally_line = components::tabular(tally_line);
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .gap(px(theme::EVENT_GAP))
        .h(px(theme::CHECKS_HEAD_H))
        .px(px(theme::MENU_ROW_PAD_X))
        .mb(px(theme::CHECKS_CARD_GAP))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .gap(px(theme::SPACE_1_5))
                .child(
                    div()
                        .font_weight(theme::W_LABEL)
                        .text_color(rgb(INK_STRONG))
                        .child("Checks"),
                )
                .child(
                    div()
                        .text_color(rgb(INK_SUBTLE))
                        .child(SharedString::from(format!("#{}", pr.number))),
                ),
        )
        .child(tally_line)
}

/// A workflow's heading in the card, above the runs it owns. Actions
/// groups its jobs under a workflow and the card says so; a run that
/// belongs to no workflow — a posted commit status — is grouped under
/// `status` rather than being given a heading it does not have. It is the
/// one menu section title, so the card's groups read as a menu's do.
pub fn checks_group(workflow: Option<&str>, first: bool) -> Div {
    components::menu_section(workflow.unwrap_or("status").to_string(), None, None)
        .flex_shrink_0()
        .when(!first, |group| group.mt(px(theme::CHECKS_GROUP_GAP)))
}

/// One run in the card: its state's dot, its name, and the forge's own
/// word for where it stands at the trailing edge. The name is what gives
/// way when it is longer than the card — the state word is the shorter
/// string and the one the row exists to pair with the name.
///
/// The row is a control only where the run has a log to open; the cockpit
/// wires the press. A run with no URL is drawn without the hover face and
/// its name one step dimmer, so nothing offers a press that would do
/// nothing.
pub fn check_row(index: usize, run: &Check) -> Stateful<Div> {
    let openable = run.url.is_some();
    let name = SharedString::from(run.name.clone());
    div()
        .id(("check-row", index))
        .debug_selector(move || format!("check-row-{index}"))
        .flex()
        .flex_shrink_0()
        .items_center()
        // A status dot sits 8px from its text on every floating surface
        // (notification rows, MCP servers, runs).
        .gap(px(theme::SPACE_2))
        .h(px(theme::CHECKS_ROW_H))
        .px(px(theme::MENU_ROW_PAD_X))
        .rounded(px(theme::R_CONTROL))
        // A matrix job's name can outrun the card; the whole of it is one
        // hover away.
        .tooltip(ferrite_ui::menu::tooltip(SharedString::from(format!(
            "{name} \u{b7} {}",
            run.detail
        ))))
        .child(check_mark(run.state, &run.detail).element())
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_color(rgb(if openable { INK_STRONG } else { INK_SUBTLE }))
                .child(name),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_color(rgb(check_mark(run.state, &run.detail).word_ink()))
                .child(check_word(run.state, &run.detail)),
        )
        // The card is a raised surface: its rows take the raised faces.
        .when(openable, |row| {
            row.hover_raised(format!("check-row-{index}"))
                .press_raised()
        })
}

/// The head's title: the name, truncating, with no hover face and the
/// default cursor — it is a name, not a button. Render-only; the cockpit
/// gives it its id, its double-click rename and its `Rename · double-click`
/// tooltip.
pub fn head_title(name: SharedString) -> Div {
    div().min_w_0().truncate().child(name)
}

/// The rendered tail of a transcript at one level — the window `body`
/// draws and the selection overlay resolves against (#27). One function,
/// two callers, so the wash can never resolve against a different window
/// than is drawn.
pub fn rendered_window(blocks: &[Block], level: Level) -> &[Block] {
    let tail = blocks.len().saturating_sub(level.visible_blocks());
    &blocks[tail..]
}

/// Tool rows with something to disclose — output, a structured result, an
/// input, or a diff — in exactly the window L1 draws. Disclosure cycling,
/// focus validation, and controls all consume this one eligibility rule so
/// an invisible row can never remain keyboard-addressable.
pub fn tool_has_details(tool: &ToolBlock) -> bool {
    tool.output.is_some()
        || tool.structured_preview.is_some()
        || !tool.summary.is_empty()
        || !tool.diffs.is_empty()
}

/// A parked Thread's Pane at transcript size: its tile body — `parked`,
/// its facts, how to wake it — on the tile's grid, so the same Thread
/// reads the same at every size.
fn parked_body(parked_line: Option<SharedString>) -> Div {
    let card = WallCard::default();
    wall_cell(WallTile {
        key: 0,
        card: &card,
        state: WallState::Parked,
        kind: None,
        turn: None,
        unread: false,
        quick_answers: None,
        focused: false,
        cell_width: 0.0,
        text_system: None,
        parked_line,
    })
}

/// The text of an approval's command well: a command's source, else the
/// input as a string, else its pretty JSON. `None` for a question or an
/// input-less request.
pub fn approval_source(decision: &Decision) -> Option<String> {
    if questions_of(decision).is_some() {
        return None;
    }
    decision
        .input
        .get("command")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .or_else(|| decision.input.as_str().map(str::to_owned))
        .or_else(|| {
            (!decision.input.is_null()).then(|| {
                serde_json::to_string_pretty(&decision.input)
                    .expect("decision input is serializable")
            })
        })
}

/// An edit approval's change as the diff it would leave (`proposed_diff`):
/// unified, under the head, in the rows' own red and green — the edit the
/// operator is asked about rather than its JSON. `None` for anything that
/// is not an edit with its text.
pub fn approval_diff(
    decision: &Decision,
    asked: Option<&Diff>,
    workspace: Option<&std::path::Path>,
    reading: ferrite_core::settings::ReadingSize,
) -> Option<AnyElement> {
    if let Some(diff) = asked {
        return Some(
            div()
                .debug_selector(|| "approval-diff".into())
                .w_full()
                .min_w_0()
                .flex_shrink_0()
                .child(render_diff(None, diff, Grid::of(reading)))
                .into_any_element(),
        );
    }
    thread_local! {
        /// Each asked edit's diff, read off the file once: the card draws
        /// every frame while it stands.
        static PROPOSED: std::cell::RefCell<HashMap<String, Option<ferrite_core::transcript::Diff>>> =
            std::cell::RefCell::new(HashMap::new());
    }
    let key = format!("{}\u{1f}{}", decision.id, decision.tool_use_id);
    let diff = PROPOSED.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() > 32 {
            cache.clear();
        }
        cache
            .entry(key)
            .or_insert_with(|| {
                ferrite_core::transcript::proposed_diff(
                    &decision.tool_name,
                    &decision.input,
                    workspace,
                )
            })
            .clone()
    })?;
    Some(
        div()
            .debug_selector(|| "approval-diff".into())
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .child(render_diff(None, &diff, Grid::of(reading)))
            .into_any_element(),
    )
}

/// Whether an approval's well holds a shell command, which reads after a
/// `$ ` prompt: Claude's `Bash` and Codex's `commandExecution` alike.
pub fn shell_command(decision: &Decision) -> bool {
    decision::shell_source(decision).is_some()
}

/// The exact tool input an approval would send. A shell command reads as
/// the operator would type it — Codex's login-shell wrapper (`/bin/zsh -lc
/// "…"`) taken off, so both providers read `gh issue close 212`; other
/// provider input remains inspectable as its JSON value.
pub fn approval_input(
    decision: &Decision,
    cache: &crate::rich::TextCache,
    id: SharedString,
) -> Option<AnyElement> {
    use gpui::component::scroll::ScrollableElement as _;

    // A command reads in Claude's command green; other input in the
    // well's own ink.
    let (source, ink) = match decision::shell_source(decision) {
        Some(command) => (command.into_owned(), Some(theme::SHELL_COMMAND_INK)),
        None => (approval_source(decision)?, None),
    };
    Some(
        div()
            .debug_selector(|| "approval-input".into())
            .when_some(ink, |input, ink| input.text_color(rgb(ink)))
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            // Let the bar measure this row's content height; an unspecified
            // height inherits the toolkit wrapper's full-height default.
            .h_auto()
            .max_h(px(theme::DECISION_INPUT_MAX_H))
            .overflow_y_scrollbar()
            .child(crate::rich::Literal {
                id,
                document: None,
                text: source.into(),
                highlights: Vec::new(),
                cache: cache.clone(),
                find: None,
            })
            .into_any_element(),
    )
}

// -------------------------------------------------------------- questions

/// The normalized questions a Decision carries. Providers classify the wire
/// request before it reaches the shared renderer.
pub fn question_of(decision: &Decision) -> Option<Vec<ferrite_core::questions::Question>> {
    questions_of(decision).map(<[_]>::to_vec)
}

/// `question_of`, borrowed: no clone per frame.
pub fn questions_of(decision: &Decision) -> Option<&[ferrite_core::questions::Question]> {
    match &decision.kind {
        ferrite_core::DecisionKind::Questions(questions) => Some(questions),
        _ => None,
    }
}

// ------------------------------------------------------------ shared bits

/// A card's refusal, the one line grammar (rule 2.11.3): `failed` in
/// `BLOCKED`, the `·` in structure ink, then the message in `INK_STRONG`, all
/// the grid's type as one run that wraps, on the rows' inset.
pub fn card_error(message: impl Into<SharedString>) -> Div {
    let message = message.into();
    let word = theme::words::FAILED;
    let seam = " \u{b7} ";
    let text = format!("{word}{seam}{message}");
    let ink = |ink: u32| HighlightStyle {
        color: Some(rgb(ink).into()),
        ..Default::default()
    };
    let runs = vec![
        (0..word.len(), ink(BLOCKED)),
        (word.len()..word.len() + seam.len(), ink(TEXT_FAINT)),
    ];
    components::text_meta()
        .flex_shrink_0()
        .px(px(theme::MENU_ROW_PAD_X))
        .py(px(theme::SPACE_1))
        .text_color(rgb(INK_STRONG))
        .whitespace_normal()
        .child(StyledText::new(text).with_highlights(runs))
}

/// A name in sentence case — a provider's category, a chip label (SPEC
/// §5.5, amending rule 11 for chip labels only) — keeping known acronyms'
/// capitals ("mcp tools" → "MCP tools", not "Mcp tools").
pub fn sentence_case(name: &str) -> String {
    let lower = name.to_lowercase();
    let words: Vec<String> = lower
        .split(' ')
        .enumerate()
        .map(|(index, word)| match word {
            "mcp" => "MCP".to_owned(),
            "ai" => "AI".to_owned(),
            _ if index == 0 => {
                let mut chars = word.chars();
                chars
                    .next()
                    .map(|first| first.to_uppercase().chain(chars).collect())
                    .unwrap_or_default()
            }
            _ => word.to_owned(),
        })
        .collect();
    words.join(" ")
}

/// Which checkout a Thread works in — a worktree's own name, or "main" for
/// the shared one. One line, because an operator running many Threads has to
/// know which of them can trample the others. Shared with the nav's rows
/// (#21), so both surfaces name a binding the same way.
#[cfg_attr(not(test), allow(dead_code))]
pub fn binding_label(workspace: Option<&WorkspaceBinding>) -> SharedString {
    match workspace {
        Some(WorkspaceBinding::Worktree { path, .. }) => SharedString::from(
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "worktree".into()),
        ),
        Some(WorkspaceBinding::Main { .. }) => SharedString::from("main"),
        None => SharedString::from(""),
    }
}

// The interactive session-project-root selector (#24) is deleted, not
// dormant (#29): an interactive path to change where a Thread works
// post-lock is exactly what must not exist. `binding_label` above survives
// for the nav's rows; the header's binding slot is the display-only branch
// text now.

// ----------------------------------------------------------- Block render

/// The text left after the first `n` whitespace-separated words.
fn skip_words(text: &str, n: usize) -> &str {
    let mut rest = text.trim_start();
    for _ in 0..n {
        match rest.find(char::is_whitespace) {
            Some(cut) => rest = rest[cut..].trim_start(),
            None => return "",
        }
    }
    rest
}

/// A preview of received text, never a second provider reasoning channel.
/// Keep a short first line in the header; disclose only what follows it.
/// The disclosure never repeats what the header already shows: where the
/// first line was cut to fit, the details open on the rest of that line and
/// run into the paragraphs under it, so expanding reads as a continuation
/// rather than the same sentence twice.
pub fn reasoning_text(thought: &str) -> (String, Option<String>) {
    let thought = thought.trim();
    let (first, rest) = thought.split_once('\n').unwrap_or((thought, ""));
    let first = first.trim();
    let heading = first
        .strip_prefix("**")
        .and_then(|text| text.strip_suffix("**"))
        .or_else(|| {
            let text = first.trim_start_matches('#');
            (text.len() < first.len() && text.starts_with(' ')).then(|| text.trim())
        })
        .unwrap_or(first);
    let summary = ferrite_core::progress::one_line(heading, 160);
    let rest = rest.trim();
    // `one_line` cuts the header at 160 characters and marks the cut with
    // `…`. The disclosure picks the line up from the last word the header
    // shows, so expanding continues the sentence instead of repeating it —
    // and that one word is shared, because the cut can land mid-word and
    // half a word is no place to resume reading. The tail runs through
    // `one_line` too: a header and its continuation are one sentence, so
    // they collapse whitespace the same way.
    let tail = summary
        .strip_suffix('…')
        .map(|shown| skip_words(heading, shown.split_whitespace().count().saturating_sub(1)))
        .map(|tail| ferrite_core::progress::one_line(tail, usize::MAX))
        .unwrap_or_default();
    let tail = tail.as_str();
    let details = match (tail.is_empty(), rest.is_empty()) {
        (true, true) => None,
        (true, false) => Some(rest.to_owned()),
        (false, true) => Some(tail.to_owned()),
        (false, false) => Some(format!("{tail}\n\n{rest}")),
    };
    (summary, details)
}

pub(crate) fn reasoning_has_details(thought: &str) -> bool {
    reasoning_text(thought).1.is_some()
}

// ---------------------------------------------------------- transcript rows

/// The grid a transcript row is set on (the WP-A grammar in `theme.rs`):
/// the reading size, its 1.5x line and its cell. The transcript list sets
/// the size and the line once; rows read the numbers for their gutters,
/// elbows and folds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    pub size: f32,
    pub line: f32,
}

impl Grid {
    /// A Solo transcript's grid (and a reader's, a review's): the reading
    /// size on its line.
    pub fn of(reading: ferrite_core::settings::ReadingSize) -> Self {
        Self::sized(theme::answer_text_size(reading))
    }

    /// The grid of a Solo (`solo`) or a Group Pane's transcript
    /// (`theme::tx_size`).
    pub fn pane(reading: ferrite_core::settings::ReadingSize, solo: bool) -> Self {
        Self::sized(theme::tx_size(reading, solo))
    }

    fn sized(size: f32) -> Self {
        Self {
            size,
            line: theme::prose_line_height(size),
        }
    }

    /// The grid a diff or a code fence inside this one is set on:
    /// `TX_BLOCK_STEP` smaller.
    pub fn block(self) -> Self {
        Self::sized(self.size - theme::TX_BLOCK_STEP)
    }

    /// The grid activity's words are set on (a call line, a group's
    /// summary, their trails): the block size on this grid's line, so the
    /// agent's work recedes beside its prose and keeps the rows' rhythm.
    pub(crate) fn activity(self) -> Self {
        Self {
            size: self.block().size,
            line: self.line,
        }
    }

    /// One cell of the grid.
    pub fn cell(self) -> f32 {
        theme::tx_cell(self.size)
    }

    /// The 2-cell glyph gutter.
    pub(crate) fn gutter(self) -> f32 {
        theme::tx_gutter(self.size)
    }

    /// A drawn mark's box (`∴`).
    pub(crate) fn mark(self) -> f32 {
        theme::tx_mark(self.size)
    }

    /// Half a line, whole pixels.
    pub fn half(self) -> f32 {
        theme::tx_half(self.line)
    }
}

/// A row's leading mark (`❯ ✻`, typed; `∴`, drawn): centred on the row's
/// first line, its words a small gap after it.
pub(crate) fn mark_gutter(grid: Grid, mark: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(grid.line))
        .mr(px(theme::SPACE_2))
        .child(mark)
}

/// Something literal laid out on the grid's cells (a call's failure, its
/// open body, a diff): Geist Mono whatever face the transcript reads in.
pub(crate) fn on_cells(row: Div) -> Div {
    row.font_family(theme::FONT_CODE)
}

/// A transcript row led by its mark, then its words (the caller's next
/// child, `flex_1 min_w_0`); wrapped lines hang under the words.
fn grid_row(mark: Div) -> Div {
    div().flex().items_start().w_full().min_w_0().child(mark)
}

/// A row on the column's edge, as every row's words are.
fn content_row(_grid: Grid) -> Div {
    div().flex().items_start().w_full().min_w_0()
}

/// Everything one transcript row reads besides its block: the selection its
/// runs register with, the text cache a pinned copy draws from, the
/// preview host, the grid, and the transcript's scope.
pub(crate) struct RowCx<'a> {
    pub selection: &'a TextRuns,
    pub rich: &'a crate::rich::TextCache,
    pub preview: &'a crate::preview::Preview,
    pub grid: Grid,
    pub scope: &'a SharedString,
    /// The Session's skills: a prompt's `/name` that names one is blue.
    pub skills: &'a [ferrite_core::SessionCommand],
}

impl RowCx<'_> {
    fn grid(&self) -> Grid {
        self.grid
    }
}

/// A tool row's disclosure state: whether its details (its output fold,
/// its diffs) are open, whether it stands in a group's card, and the
/// keyboard's target and toggle.
pub(crate) struct ToolCx {
    pub expanded: bool,
    pub grouped: bool,
    pub disclosure: Option<Disclosure>,
    /// Its work running in the background: `View live output ▸`, pressed
    /// to open the Background tasks pane (`working::background_link`).
    pub live: Option<AnyElement>,
}

/// One Block as a transcript row, in the terminal grammar of `theme.rs`'s
/// WP-A section: a mark in the 2-cell gutter, the content after it. Rows own
/// no spacing; the list gives each row its gap from the gap table.
///
/// Every text run routes through the selection overlay (#27) — that is what
/// makes it selectable and copyable; the marks, elbows, trails, the `$` and
/// the diff numbers around the runs are chrome, and stay plain. Anything a
/// row registers is mirrored by its `pane/text.rs` collector.
pub(crate) fn render_block(
    block: &Block,
    row_cx: &RowCx,
    expanded: bool,
    disclosure: Option<Disclosure>,
) -> AnyElement {
    let grid = row_cx.grid();
    let selection = row_cx.selection;
    let row = div().w_full().min_w_0().flex_shrink_0();
    match &block.body {
        // A visual's row owns a live page, so the transcript's `Shelf`
        // draws it (`TranscriptView::render_row`); it never reaches here.
        Body::Visual(_) => row.into_any_element(),
        // A wrap-up's steps are presses the transcript wires
        // (`TranscriptView::render_row`); drawn bare, it has none.
        Body::WrapUp(wrap) => {
            wrap_up::render_wrap_up(block.id, wrap, row_cx, WrapUpFace::Steps(Vec::new()))
        }
        Body::Prompt(line) => prompt_row(block, line, row_cx, None),
        // Fallback prose (a block with no Markdown source) reads as the
        // answer does, on the content column.
        Body::Paragraph { spans } => row
            .child(
                content_row(grid).text_color(rgb(INK_STRONG)).child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(prose(block.id, spans, selection)),
                ),
            )
            .into_any_element(),
        Body::Heading { spans, .. } => row
            .child(
                content_row(grid)
                    .font_weight(theme::W_STRONG)
                    .text_color(rgb(INK_STRONG))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(prose(block.id, spans, selection)),
                    ),
            )
            .into_any_element(),
        // A fallback list item: a dim `•` hanging in two cells, its text
        // after it.
        Body::Bullet { spans } => row
            .child(
                content_row(grid)
                    .text_color(rgb(INK_STRONG))
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(2.0 * grid.cell()))
                            .text_color(rgb(INK_SUBTLE))
                            .child("\u{2022}"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(prose(block.id, spans, selection)),
                    ),
            )
            .into_any_element(),
        // A blank thought from an older log (redacted thinking, before the
        // fold learned to drop it) draws nothing — not even its margin.
        Body::Thinking(thought) if thought.trim().is_empty() => div().into_any_element(),
        // Reasoning is agent prose one ink down, under the drawn `∴`,
        // `INK_SUBTLE`, never italic. A short thought shows whole; a long one
        // is its first line with the rest disclosed.
        Body::Thinking(thought) => {
            let (summary, details) = reasoning_text(thought);
            let mark = || mark_gutter(grid, icon(icons::REASONING, grid.mark(), TEXT_FAINT));
            let Some(details) = details else {
                // Nothing more was supplied. Keep the whole short block
                // visible, wrapped and selectable without a false disclosure.
                return row
                    .child(
                        grid_row(mark()).text_color(rgb(INK_SUBTLE)).child(
                            div().flex_1().min_w_0().child(
                                selection
                                    .markdown(block.id, thought.trim().to_owned())
                                    .muted(),
                            ),
                        ),
                    )
                    .into_any_element();
            };
            let (overlay, chevron, targeted) = disclosure_parts(disclosure);
            let header = grid_row(mark())
                .id(SharedString::from(format!("reasoning-row-{:?}", block.id)))
                .group(DISCLOSURE_ROW)
                .relative()
                .child(
                    div()
                        .debug_selector(|| "reasoning-summary".into())
                        .min_w_0()
                        .truncate()
                        .child(SharedString::from(summary)),
                )
                .children(chevron)
                .children(overlay);
            let header = Disclosure::ground(targeted, header);
            let mut body = gpui::component::collapsible::Collapsible::new()
                .w_full()
                .open(expanded)
                .child(header);
            if expanded {
                body = body.content(
                    content_row(grid).child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(selection.markdown(block.id, details.clone()).muted()),
                    ),
                );
            }
            row.text_color(rgb(INK_SUBTLE))
                .child(body)
                .into_any_element()
        }
        // A notice: an activity line, its words cut at the column's edge
        // with the whole of it one hover away. A Decision
        // never leaves one (it is its own row, at the tail).
        Body::Notice(text) => {
            let highlights = separators(text);
            let text = SharedString::from(text.clone());
            let words = div().child(
                div()
                    .id(SharedString::from(format!("notice-{:?}", block.id)))
                    .debug_selector({
                        let id = block.id;
                        move || format!("notice-{id:?}")
                    })
                    .flex_shrink(1.)
                    .min_w_0()
                    .tooltip(ferrite_ui::menu::tooltip(text.clone()))
                    .child(edge_cut(
                        grid.line,
                        INK_SUBTLE,
                        selection.line(block.id, text, highlights),
                    )),
            );
            row.child(activity_line(grid, None, words))
                .into_any_element()
        }
        // A decision record (`allowed Write`) or a revival note, under the
        // row it answers.
        Body::Meta(text) => row
            .child(
                activity_note(grid).child(div().flex_1().min_w_0().child(selection.line(
                    block.id,
                    text.clone(),
                    separators(text),
                ))),
            )
            .into_any_element(),
        Body::TurnEnd(end) => row
            .child(turn_end(block.id, end, row_cx))
            .into_any_element(),
        // Code keeps literal indentation and highlighting on a 1px rule,
        // with no ground.
        Body::Code {
            language: _,
            source,
            tokens,
        } => row
            .child(
                content_row(grid).child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .pl(px(2.0 * grid.cell()))
                        .border_l_1()
                        .border_color(theme::paint::LINE)
                        .font_family(theme::FONT_CODE)
                        .text_color(rgb(INK_STRONG))
                        .children(code_lines(
                            block.id,
                            source,
                            code(source, tokens.as_deref()),
                            selection,
                        )),
                ),
            )
            .into_any_element(),
        Body::Tool(tool) => render_tool(
            block.id,
            tool,
            row_cx,
            ToolCx {
                expanded,
                grouped: false,
                disclosure,
                live: None,
            },
        ),
    }
}

/// What a prompt's text is: its words as one literal run, or — with
/// attachments that chip, or `code` quoted in backticks — Markdown whose
/// links are the inline chips, so a chip flows right after the words and
/// wraps only when it does not fit, and whose code spans are code chips.
pub enum PromptText {
    Literal(String),
    Markdown(String),
}

/// A prompt line as its bubble draws (and copies) it, and the files it
/// carries: an image is its thumbnail, so only its other files chip. The
/// quotes leading it are not its words: they draw as Quote rows over it.
pub fn prompt_text(
    line: &str,
    skills: &[ferrite_core::SessionCommand],
) -> (PromptText, Vec<std::path::PathBuf>) {
    prompt_text_chipping(line, skills, |path| !crate::image_viewer::is_image(path))
}

/// A prompt line with a chip for each of its files that `chips`.
fn prompt_text_chipping(
    line: &str,
    skills: &[ferrite_core::SessionCommand],
    chips: fn(&std::path::Path) -> bool,
) -> (PromptText, Vec<std::path::PathBuf>) {
    let words = crate::quote::split(line).1;
    let (text, files) = ferrite_core::prompt_files::split(String::from(words));
    let chipped: Vec<_> = files.iter().filter(|path| chips(path)).cloned().collect();
    let quotes_code = text
        .split('\n')
        .any(|line| code_spans(line).iter().any(|(code, _)| *code));
    let names_skill = text
        .split_whitespace()
        .any(|word| skill_named(word, skills).is_some());
    if chipped.is_empty() && !quotes_code && !names_skill {
        return (PromptText::Literal(text), files);
    }
    (
        PromptText::Markdown(prompt_markdown(&text, &chipped, skills)),
        files,
    )
}

/// The prompt's words with every Markdown character escaped (they are the
/// operator's own, never markup) but for their code spans, which stay code
/// (Claude draws them as chips); hard breaks where they broke the line,
/// then one `ferrite-chip:` link per file, which the band renders as its
/// chip (`prompt_chip`).
fn prompt_markdown(
    text: &str,
    files: &[std::path::PathBuf],
    skills: &[ferrite_core::SessionCommand],
) -> String {
    let mut out = String::with_capacity(text.len() + 64);
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push_str("\\\n");
        }
        for (code, span) in code_spans(line) {
            if code {
                out.push_str(span);
                continue;
            }
            // Words and the space between them: a word that names a skill
            // is its link (`skill_token`), the rest is escaped.
            for (word, is_word) in words_of(span) {
                match skill_named(word, skills).filter(|_| is_word) {
                    Some(skill) => {
                        out.push_str("[\\/");
                        escape_into(&mut out, &skill.name);
                        out.push_str("](<ferrite-skill:");
                        out.push_str(&skill.name);
                        out.push_str(">)");
                    }
                    None => escape_into(&mut out, word),
                }
            }
        }
    }
    for path in files {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if !out.is_empty() {
            out.push(' ');
        }
        out.push('[');
        for ch in name.chars() {
            if ch.is_ascii_punctuation() {
                out.push('\\');
            }
            out.push(ch);
        }
        out.push_str("](<ferrite-chip:");
        out.push_str(&path.display().to_string());
        out.push_str(">)");
    }
    out
}

/// `text` with every Markdown character escaped.
fn escape_into(out: &mut String, text: &str) {
    for ch in text.chars() {
        if ch.is_ascii_punctuation() {
            out.push('\\');
        }
        out.push(ch);
    }
}

/// `text` cut into its words and the whitespace between them, in order,
/// each with whether it is a word.
fn words_of(text: &str) -> Vec<(&str, bool)> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_word = false;
    for (at, ch) in text.char_indices() {
        if ch.is_whitespace() == in_word {
            if at > start {
                parts.push((&text[start..at], in_word));
            }
            start = at;
            in_word = !in_word;
        }
    }
    if start < text.len() {
        parts.push((&text[start..], in_word));
    }
    parts
}

/// The skill a word names: `/name` for one of the Session's `skills`.
fn skill_named<'a>(
    word: &str,
    skills: &'a [ferrite_core::SessionCommand],
) -> Option<&'a ferrite_core::SessionCommand> {
    let name = word.strip_prefix('/')?;
    skills.iter().find(|skill| skill.name == name)
}

/// A skill in a sent prompt (Claude's, `claude/parity/skill-sent-bubble.png`,
/// `skill-hover.png`): `SKILL_INK` on no ground, in the prompt's face, a
/// faint blue wash behind it under the pointer, and a card above it — the
/// full name in `INK_SUBTLE`, then the catalogue's description, as the `/` picker reads it — wrapped at
/// `SKILL_TIP_W`.
pub(crate) fn skill_token(
    label: &str,
    skill: &ferrite_core::SessionCommand,
    window: &mut gpui::Window,
) -> (gpui::Size<gpui::Pixels>, AnyElement) {
    let style = window.text_style();
    let size = style.font_size.to_pixels(window.rem_size());
    let shaped = window.text_system().shape_line(
        SharedString::from(label.to_string()),
        size,
        &[style.to_run(label.len())],
        None,
    );
    let width = shaped.width();
    let line = window.line_height();
    let name = skill.name.clone();
    let description = skill.description.clone();
    let element = div()
        .id(SharedString::from(format!("prompt-skill-{name}")))
        .debug_selector({
            let name = name.clone();
            move || format!("prompt-skill-{name}")
        })
        .w(width)
        .h(line)
        .rounded(px(theme::R_CONTROL))
        .text_color(rgb(theme::SKILL_INK))
        .whitespace_nowrap()
        .hover(|style| style.bg(gpui::rgba(theme::SKILL_HOVER)))
        .tooltip(ferrite_ui::menu::tooltip_card(
            format!("/{name}"),
            description,
            theme::SKILL_TIP_W,
        ))
        .child(SharedString::from(label.to_string()))
        .into_any_element();
    (gpui::size(width, line), element)
}

/// A line cut into its code spans (`true`, backticks and all) and the
/// words between them, in order: as CommonMark reads a code span, a run of
/// backticks closed by the next run of the same length. A run never
/// closed is words.
fn code_spans(line: &str) -> Vec<(bool, &str)> {
    let ticks = |at: usize| line[at..].bytes().take_while(|&b| b == b'`').count();
    let mut spans = Vec::new();
    let (mut words, mut at) = (0, 0);
    while let Some(found) = line[at..].find('`') {
        let open = at + found;
        let run = ticks(open);
        let mut search = open + run;
        let close = loop {
            let Some(found) = line[search..].find('`') else {
                break None;
            };
            let start = search + found;
            let length = ticks(start);
            if length == run {
                break Some(start + length);
            }
            search = start + length;
        };
        let Some(end) = close else {
            at = open + run;
            continue;
        };
        if words < open {
            spans.push((false, &line[words..open]));
        }
        spans.push((true, &line[open..end]));
        (words, at) = (end, end);
    }
    if words < line.len() {
        spans.push((false, &line[words..]));
    }
    spans
}

/// A prompt's words: its literal run or its Markdown with its chips, in
/// `INK_STRONG`; in the selection, or (`clamp`, the pinned band) outside
/// it, ending its last kept line in `…` so it says there is more above.
/// `None` for a prompt of attachments alone.
fn prompt_words(
    block: &Block,
    text: PromptText,
    gallery: &std::sync::Arc<[std::path::PathBuf]>,
    row_cx: &RowCx,
    clamp: Option<usize>,
) -> Option<AnyElement> {
    Some(match text {
        PromptText::Literal(words) if words.is_empty() => return None,
        PromptText::Literal(words) => match clamp {
            Some(lines) => div()
                .w_full()
                .min_w_0()
                .line_clamp(lines)
                .text_ellipsis()
                .child(SharedString::from(words))
                .into_any_element(),
            None => div()
                .min_w_0()
                .child(row_cx.selection.line(block.id, words, Vec::new()))
                .into_any_element(),
        },
        PromptText::Markdown(source) => match clamp {
            Some(_) => crate::rich::Markdown::new(
                format!("pinned-prompt-{}-{:?}", row_cx.scope, block.id),
                source,
                row_cx.rich.clone(),
            ),
            None => row_cx.selection.markdown(block.id, source),
        }
        .ink(INK_STRONG)
        .skills(row_cx.skills.into())
        .chips(row_cx.preview.clone(), gallery.clone())
        .into_any_element(),
    })
}

/// A prompt's images, in its order: what a chip or a thumbnail opens
/// among (← → step through them).
fn prompt_gallery(files: &[std::path::PathBuf]) -> std::sync::Arc<[std::path::PathBuf]> {
    files
        .iter()
        .filter(|path| crate::image_viewer::is_image(path))
        .cloned()
        .collect()
}

/// The prompt echo (the prototype's `.me`): right-aligned, the operator's
/// words in a `paint::PROMPT_BUBBLE` bubble at `R_CARD` that shrinks to them
/// (never wider than `PROMPT_BUBBLE_MAX_W` of the column), Geist
/// `INK_STRONG` at body weight with its non-image files' chips flowing
/// inline after them; its images as thumbnails in a wrapping row under it.
/// `actions` (the transcript's: its send time, `Copy`) stand under it all,
/// on its right edge, as Claude's and Codex's do. The passages it quoted
/// stand over the bubble as their Quote rows.
pub(crate) fn prompt_row(
    block: &Block,
    line: &str,
    row_cx: &RowCx,
    actions: Option<AnyElement>,
) -> AnyElement {
    let (text, files) = prompt_text(line, row_cx.skills);
    let gallery = prompt_gallery(&files);
    // The passages it replied to, as their Quote rows (Claude's).
    let quotes: Vec<_> = crate::quote::split(line)
        .0
        .into_iter()
        .enumerate()
        .map(|(index, reply)| {
            div()
                .max_w(relative(theme::PROMPT_BUBBLE_MAX_W))
                .min_w_0()
                .child(crate::attachments::quote_row(index, reply))
        })
        .collect();
    let bubble = prompt_words(block, text, &gallery, row_cx, None).map(|words| {
        prompt_bubble()
            .debug_selector(|| "transcript-prompt".into())
            .child(words)
    });
    let images: Vec<_> = gallery
        .iter()
        .enumerate()
        .map(|(index, path)| inline_image(block.id, index, path, &gallery, row_cx.preview))
        .collect();
    // The images stand over the bubble, right edges level (Claude's).
    let message = div()
        .flex()
        .flex_col()
        .items_end()
        .gap(px(theme::PROMPT_IMAGE_GAP))
        .w_full()
        .min_w_0()
        .when(!images.is_empty(), |row| {
            row.child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_end()
                    .gap(px(theme::PROMPT_BUBBLE_GAP))
                    .max_w_full()
                    .children(images),
            )
        })
        .children(quotes)
        .children(bubble);
    div()
        .id(SharedString::from(format!("prompt-{:?}", block.id)))
        .relative()
        .flex()
        .flex_col()
        .items_end()
        .w_full()
        .min_w_0()
        .child(message)
        .children(actions)
        .into_any_element()
}

/// The pinned prompt: the turn's bubble (`prompt_row`'s) stuck to the top
/// of the body while its output scrolls under it — the same `paint::RAISED`
/// bubble at `R_CARD`, right-aligned, its words clamped to `lines` with
/// all its files' chips (its images' too) flowing inline after them. Not
/// part of the selection; the transcript makes it the way back to its turn.
pub(crate) fn pinned_prompt(block: &Block, line: &str, row_cx: &RowCx, lines: usize) -> Div {
    let grid = row_cx.grid();
    let (text, files) = prompt_text_chipping(line, row_cx.skills, |_| true);
    let gallery = prompt_gallery(&files);
    // The chips close the prompt; their line stands taller, as the
    // inline chip makes it.
    let chip_line = if files.is_empty() {
        0.0
    } else {
        theme::PROMPT_CHIP_LINE_EXTRA
    };
    let words = prompt_words(block, text, &gallery, row_cx, Some(lines));
    prompt_bubble()
        .debug_selector(|| "transcript-pinned-prompt".into())
        .pb(px(theme::PROMPT_BUBBLE_PAD_Y + chip_line))
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .max_h(px(lines as f32 * grid.line))
                .overflow_hidden()
                .children(words),
        )
}

/// The prompt's bubble (the prototype's `.me`): `paint::PROMPT_BUBBLE` at
/// `R_CARD`, shrinking to its words in Geist `INK_STRONG` at body
/// weight, never wider than `PROMPT_BUBBLE_MAX_W` of the column. One face
/// for the echo in its row and its pinned copy.
fn prompt_bubble() -> Div {
    div()
        .max_w(relative(theme::PROMPT_BUBBLE_MAX_W))
        .min_w_0()
        .py(px(theme::PROMPT_BUBBLE_PAD_Y))
        .px(px(theme::PROMPT_BUBBLE_PAD_X))
        .bg(theme::paint::PROMPT_BUBBLE)
        .rounded(px(theme::R_CARD))
        .font_family(theme::FONT_UI)
        .font_weight(theme::W_BODY)
        .text_color(rgb(INK_STRONG))
}

/// Open a sent file in the Pane's preview (the Panel's Preview tab), an
/// image stepping among its prompt's `gallery`.
fn open_file(
    path: &std::path::Path,
    gallery: &[std::path::PathBuf],
    preview: &crate::preview::Preview,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) {
    preview.open(path.to_path_buf(), None, gallery, window, cx);
}

/// A prompt's file as its band's inline chip (SPEC §5.4): the 12px image
/// mark (or the file mark), a cell, the name, all `INK_STRONG`, on
/// `paint::RAISED` at `R_CONTROL` with a cell of padding each side. A click opens it (an
/// image in the Pane's preview, in the Panel, among its prompt's `gallery`).
/// Sized for the Markdown flow that places it: monospace cells at the size
/// it is drawn in, its name set in `FONT_CODE` so it fills them.
pub(crate) fn prompt_chip(
    path: std::path::PathBuf,
    label: &str,
    preview: &crate::preview::Preview,
    gallery: &std::sync::Arc<[std::path::PathBuf]>,
    window: &mut gpui::Window,
    _cx: &mut gpui::App,
) -> (gpui::Size<gpui::Pixels>, AnyElement) {
    let size = f32::from(window.text_style().font_size.to_pixels(window.rem_size()));
    let cell = theme::tx_cell(size);
    let line = window.line_height();
    let width = 3.0 * cell + theme::GLYPH_BOX + label.chars().count() as f32 * cell;
    let image = crate::image_viewer::is_image(&path);
    let host = preview.clone();
    let gallery = gallery.clone();
    let open = path.clone();
    let element = div()
        .id(SharedString::from(format!(
            "prompt-chip-{}",
            path.display()
        )))
        .debug_selector({
            let name = label.to_string();
            move || format!("prompt-chip-{name}")
        })
        .flex()
        .items_center()
        .gap(px(cell))
        .w(px(width))
        .h(line)
        .px(px(cell))
        .bg(theme::paint::RAISED)
        .rounded(px(theme::R_CONTROL))
        .cursor_pointer()
        .whitespace_nowrap()
        .font_family(theme::FONT_CODE)
        .font_weight(theme::W_BODY)
        .text_color(rgb(INK_STRONG))
        .child(icon(
            if image { icons::IMAGE } else { icons::FILE },
            theme::GLYPH_BOX,
            INK_STRONG,
        ))
        // The chip is sized in whole cells; the label never cuts (a
        // measured run rounds up past its cells by a fraction of a pixel).
        .child(
            div()
                .flex_shrink_0()
                .whitespace_nowrap()
                .child(SharedString::from(label.to_string())),
        )
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            open_file(&open, &gallery, &host, window, cx);
        });
    (gpui::size(px(width), line), element.into_any_element())
}

/// A sent image over its prompt's bubble (Claude's): its card
/// (`attachments::thumbnail`) at the picture's own proportions, no larger
/// than `THUMB_MAX_W` × `THUMB_MAX_H` nor its own size (covering that box
/// when its header cannot be read). A click opens it in the lightbox,
/// among its prompt's `gallery`.
fn inline_image(
    block: BlockId,
    index: usize,
    path: &std::path::Path,
    gallery: &std::sync::Arc<[std::path::PathBuf]>,
    preview: &crate::preview::Preview,
) -> AnyElement {
    // Whole pixels, so the ring sits on the pixel grid.
    let size = match image_facts(path) {
        Some(ImageFacts {
            width: w,
            height: h,
            ..
        }) if w > 0 && h > 0 => {
            let scale = (theme::THUMB_MAX_W / w as f32)
                .min(theme::THUMB_MAX_H / h as f32)
                .min(1.0);
            (
                (w as f32 * scale).round().max(1.0),
                (h as f32 * scale).round().max(1.0),
            )
        }
        _ => (theme::THUMB_MAX_W, theme::THUMB_MAX_H),
    };
    let host = preview.clone();
    let gallery = gallery.clone();
    let open = path.to_path_buf();
    crate::attachments::thumbnail(
        SharedString::from(format!("sent-image-{block:?}-{index}")),
        path,
        size,
    )
    .debug_selector(|| "sent-image".into())
    .cursor_pointer()
    .on_click(move |_, window, cx| {
        cx.stop_propagation();
        open_file(&open, &gallery, &host, window, cx);
    })
    .into_any_element()
}

/// An image's caption: `nav-jitter.png · 580×320 · 41 KB` (its name alone
/// when the header could not be read).
pub(crate) fn image_caption(name: &str, facts: Option<ImageFacts>) -> String {
    let mut caption = vec![name.to_string()];
    if let Some(facts) = facts {
        caption.push(format!("{}\u{d7}{}", facts.width, facts.height));
        caption.push(text::byte_size(facts.bytes as usize));
    }
    caption.join(" \u{b7} ")
}

/// What an image caption says about the file: its pixel size, read from
/// its header, and its size on disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ImageFacts {
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
}

thread_local! {
    /// Image headers are read once per path: a caption is drawn every frame
    /// its row is on screen.
    static IMAGE_FACTS: std::cell::RefCell<HashMap<std::path::PathBuf, Option<ImageFacts>>> =
        std::cell::RefCell::new(HashMap::new());
}

/// An image's pixel size and byte size, or `None` when the file cannot be
/// read or its format is not one the header reader knows (PNG, GIF, JPEG,
/// WebP, BMP).
pub(crate) fn image_facts(path: &std::path::Path) -> Option<ImageFacts> {
    IMAGE_FACTS.with(|cache| {
        if let Some(facts) = cache.borrow().get(path) {
            return *facts;
        }
        let facts = read_image_facts(path);
        cache.borrow_mut().insert(path.to_path_buf(), facts);
        facts
    })
}

fn read_image_facts(path: &std::path::Path) -> Option<ImageFacts> {
    use std::io::Read as _;
    let bytes = std::fs::metadata(path).ok()?.len();
    let mut head = Vec::with_capacity(64 * 1024);
    std::fs::File::open(path)
        .ok()?
        .take(64 * 1024)
        .read_to_end(&mut head)
        .ok()?;
    let (width, height) = image_size(&head)?;
    Some(ImageFacts {
        width,
        height,
        bytes,
    })
}

/// An image's pixel size from the first bytes of its file.
fn image_size(head: &[u8]) -> Option<(u32, u32)> {
    let be16 = |at: usize| Some(u16::from_be_bytes([*head.get(at)?, *head.get(at + 1)?]) as u32);
    let le16 = |at: usize| Some(u16::from_le_bytes([*head.get(at)?, *head.get(at + 1)?]) as u32);
    let be32 = |at: usize| {
        Some(u32::from_be_bytes([
            *head.get(at)?,
            *head.get(at + 1)?,
            *head.get(at + 2)?,
            *head.get(at + 3)?,
        ]))
    };
    let le32 = |at: usize| {
        Some(u32::from_le_bytes([
            *head.get(at)?,
            *head.get(at + 1)?,
            *head.get(at + 2)?,
            *head.get(at + 3)?,
        ]))
    };
    if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some((be32(16)?, be32(20)?));
    }
    if head.starts_with(b"GIF8") {
        return Some((le16(6)?, le16(8)?));
    }
    if head.starts_with(b"BM") {
        return Some((le32(18)?, (le32(22)? as i32).unsigned_abs()));
    }
    if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP") {
        return match head.get(12..16)? {
            b"VP8X" => Some((1 + (le32(24)? & 0xff_ffff), 1 + (le32(27)? & 0xff_ffff))),
            b"VP8L" => {
                let bits = le32(21)?;
                Some((1 + (bits & 0x3fff), 1 + ((bits >> 14) & 0x3fff)))
            }
            b"VP8 " => Some((le16(26)? & 0x3fff, le16(28)? & 0x3fff)),
            _ => None,
        };
    }
    if head.starts_with(&[0xff, 0xd8]) {
        let mut at = 2;
        while at + 9 < head.len() {
            if head[at] != 0xff {
                at += 1;
                continue;
            }
            let marker = head[at + 1];
            let length = be16(at + 2)? as usize;
            // Start-of-frame markers carry the size; the rest are skipped.
            if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
                return Some((be16(at + 7)?, be16(at + 5)?));
            }
            at += 2 + length;
        }
    }
    None
}

/// The `group` every disclosable row names, so its trailing chevron shows
/// under the pointer anywhere on the row.
pub(crate) const DISCLOSURE_ROW: &str = "disclosure-row";

/// What a block of output omitted, under the output it belongs to.
pub(crate) fn omitted_line(bytes: usize) -> Div {
    div()
        .min_w_0()
        .truncate()
        .text_color(rgb(INK_SUBTLE))
        .child(SharedString::from(format!(
            "\u{2026} {} not kept",
            text::byte_size(bytes)
        )))
}

/// How a turn ended (`text::turn_stamp`), one selectable run that copies
/// back as it reads. A completed turn that took a while closes on `Worked
/// for 5m 22s` in `INK_SUBTLE`, a `LINE` running on from it to the
/// column's end; an interrupted one is a muted activity line, all
/// `INK_SUBTLE` (the operator did it; nothing failed); a failed one leads
/// with `failed` in `BLOCKED`. A quick completed turn has no row at all.
fn turn_end(block: BlockId, end: &ferrite_core::transcript::TurnEnd, row_cx: &RowCx) -> AnyElement {
    use ferrite_core::TurnOutcome;
    let Some(text) = text::turn_stamp(end) else {
        return div().into_any_element();
    };
    let mut highlights = separators(&text);
    if matches!(end.outcome, TurnOutcome::Error(_)) && text.starts_with(theme::words::FAILED) {
        highlights.insert(
            0,
            (
                0..theme::words::FAILED.len(),
                HighlightStyle {
                    color: Some(rgb(BLOCKED).into()),
                    ..Default::default()
                },
            ),
        );
    }
    let words = div().child(
        div()
            .min_w_0()
            .child(row_cx.selection.line(block, text, highlights)),
    );
    let line = components::tabular(activity_line(row_cx.grid(), None, words))
        .debug_selector(|| "turn-stamp".into());
    if end.completed() {
        return line
            .items_center()
            .gap(px(theme::TURN_STAMP_RULE_GAP))
            .child(div().flex_1().h(px(1.)).bg(theme::paint::LINE))
            .into_any_element();
    }
    line.into_any_element()
}

/// The `·` seams in a transcript line: `INK_SUBTLE`, at body weight
/// (CT-23: a seam inside the transcript reads as the words around it — the
/// status line's faint seams are the Composer's). Highlighted in place so
/// the line stays one run and copies back exactly as written.
fn separators(text: &str) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    text.match_indices('\u{b7}')
        .map(|(at, dot)| {
            (
                at..at + dot.len(),
                HighlightStyle {
                    color: Some(rgb(INK_SUBTLE).into()),
                    font_weight: Some(theme::W_BODY),
                    ..Default::default()
                },
            )
        })
        .collect()
}

/// Whether a call's argument is a file path (`Read crates/x.rs`): a path
/// target.
pub(crate) fn argument_is_path(tool: &ToolBlock) -> bool {
    use ferrite_core::transcript::tools::{self, ToolKind};
    let file_tool = matches!(
        tools::policy(&tool.name).kind,
        ToolKind::Read | ToolKind::Edit | ToolKind::Write
    );
    !tool.summary.is_empty()
        && tool.title.is_none()
        && !tool.summary.contains(char::is_whitespace)
        && !tools::is_command_run(&tool.name)
        && (file_tool || !tool.diffs.is_empty())
}

/// A change's size in a call's trail: `+7 −3`, the whole `+7` in `RUNNING`
/// and the whole `−3` (U+2212) in `BLOCKED`.
pub fn trail_diff_stat(added: usize, removed: usize) -> StyledText {
    let added = format!("+{added}");
    let text = format!("{added} \u{2212}{removed}");
    let removed_at = added.len() + 1;
    let ink = |range: std::ops::Range<usize>, ink: u32| {
        (
            range,
            HighlightStyle {
                color: Some(rgb(ink).into()),
                ..Default::default()
            },
        )
    };
    let highlights = vec![
        ink(0..added.len(), RUNNING),
        ink(removed_at..text.len(), BLOCKED),
    ];
    StyledText::new(SharedString::from(text)).with_highlights(highlights)
}

/// An activity row's line (a call, a group's summary, a visual, a notice,
/// the stamp; the reference's `Edited 3 files ›`): plain words a step under
/// the prose (`Grid::activity`) in `INK_SUBTLE` on the row's own line, from
/// the column's edge — no icon, no time — after a `mark` where the row has
/// one (the stamp's `✻`).
pub(crate) fn activity_line(grid: Grid, mark: Option<AnyElement>, words: Div) -> Div {
    let small = grid.activity();
    div()
        .flex()
        .items_center()
        .w_full()
        .min_w_0()
        .h(px(grid.line))
        .relative()
        .text_size(px(small.size))
        .text_color(rgb(INK_SUBTLE))
        .children(mark.map(|mark| mark_gutter(grid, mark)))
        .child(words.flex().items_center().min_w_0())
}

/// One line of words cut at its box's edge as CSS's `text-overflow:
/// ellipsis` cuts it: `content` lays out at its own width on a `line`-tall
/// box; wider than the box, it is clipped and `…` (in `ink`) stands at the
/// edge. What the content holds — what a selection copies — is never cut.
pub(crate) fn edge_cut(line: f32, ink: u32, content: impl IntoElement) -> EdgeCut {
    let natural: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>> =
        std::rc::Rc::default();
    let measured = natural.clone();
    let content = components::on_bounds(
        div().relative().flex_shrink_0().child(content),
        move |bounds, _, _| measured.set(Some(bounds)),
    );
    EdgeCut {
        child: Some(
            div()
                .flex()
                .items_center()
                .w_full()
                .min_w_0()
                .h(px(line))
                .overflow_hidden()
                .child(content)
                .into_any_element(),
        ),
        natural,
        ink: rgb(ink).into(),
    }
}

/// See [`edge_cut`].
pub(crate) struct EdgeCut {
    child: Option<AnyElement>,
    natural: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    ink: gpui::Hsla,
}

impl IntoElement for EdgeCut {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl gpui::Element for EdgeCut {
    type RequestLayoutState = gpui::LayoutId;
    type PrepaintState = gpui::Bounds<gpui::Pixels>;

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) -> (gpui::LayoutId, gpui::LayoutId) {
        let child = self.child.get_or_insert_with(|| div().into_any_element());
        let layout_id = child.request_layout(window, cx);
        (layout_id, layout_id)
    }

    /// The box's unrounded bounds: the content's are measured unrounded too.
    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<gpui::Pixels>,
        layout_id: &mut gpui::LayoutId,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) -> gpui::Bounds<gpui::Pixels> {
        let bounds = window.unsnapped_layout_bounds(*layout_id);
        if let Some(child) = self.child.as_mut() {
            child.prepaint(window, cx);
        }
        bounds
    }

    fn paint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<gpui::Pixels>,
        _: &mut gpui::LayoutId,
        bounds: &mut gpui::Bounds<gpui::Pixels>,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        let bounds = *bounds;
        let Some(mut child) = self.child.take() else {
            return;
        };
        let wider = self
            .natural
            .get()
            .is_some_and(|natural| natural.size.width > bounds.size.width + px(0.5));
        if !wider {
            child.paint(window, cx);
            return;
        }
        let style = window.text_style();
        let size = style.font_size.to_pixels(window.rem_size());
        let line_height = style.line_height_in_pixels(window.rem_size());
        let ellipsis = "\u{2026}";
        let run = gpui::TextRun {
            len: ellipsis.len(),
            font: style.font(),
            color: self.ink,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window
            .text_system()
            .shape_line(ellipsis.into(), size, &[run], None);
        let cut = bounds.right() - line.width;
        let shown = gpui::Bounds::from_corners(bounds.origin, gpui::point(cut, bounds.bottom()));
        window.with_content_mask(Some(gpui::ContentMask { bounds: shown }), |window| {
            child.paint(window, cx)
        });
        let top = bounds.top() + (bounds.size.height - line_height) / 2.;
        let _ = line.paint(
            gpui::point(cut, top),
            line_height,
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        );
    }
}

/// A line under an activity row, in its words (a visual's caption, a
/// decision record, why a page is missing): a step under the prose in
/// `INK_SUBTLE`.
pub(crate) fn activity_note(grid: Grid) -> Div {
    content_row(grid)
        .text_size(px(grid.activity().size))
        .text_color(rgb(INK_SUBTLE))
}

/// A tool call (the reference's `List a directory that doesn't exist
/// Failed ▸`): one muted line in the app's words (`text::call_line`) — the
/// provider's description where it wrote one, else the verb and the
/// argument (a literal one a step brighter on no chip, a path a link), and a
/// test run's verdict after a `·` — then a failed call's `Failed` tag in
/// `BLOCKED` (`text::call_verdict`), then its chevron. No icon, no time, no
/// preview of what it printed: a shut row says only what it did and whether
/// it failed. Under it while it runs: a suite's bar.
///
/// Open, a call shows its body (`call_body`, Claude's): its input in a quiet
/// well (a command's `$ cmd`), a failed command's `Exit code N`, what it
/// printed as plain mono — `SPACE_3` under its line and `SPACE_1` over the
/// card's next seam in a group's card; alone, in a card of its own, the
/// group card's (`CARD_RING` at `R_CARD`, `CARD_OUTSET` past the
/// column, its body `TOOL_CARD_INSET` in, `SPACE_3` top and bottom) — then
/// its diffs. Opening and shutting snap. The whole
/// line is the pointer's toggle; the keyboard's disclosure target wears
/// `paint::RAISED`.
pub(crate) fn render_tool(
    block: BlockId,
    tool: &ToolBlock,
    row_cx: &RowCx,
    state: ToolCx,
) -> AnyElement {
    let grid = row_cx.grid();
    let small = grid.activity();
    let selection = row_cx.selection;
    let ToolCx {
        expanded,
        grouped,
        disclosure,
        live,
    } = state;
    let (overlay, chevron, targeted) = disclosure_parts(disclosure);
    // One run on one line at its own width, cut with `…` at the row's edge:
    // it selects and copies as one line, and never spills onto the rows
    // under it. The run hangs a cell past its box (`rich::Markdown`); the
    // box makes room for it. A failed call's tag follows the cut, its lead
    // word in the failure ink (rule 6: colour on the word). Runs register
    // as they are built: the call line first, then its tag.
    let call = selection
        .markdown(block, text::call_markdown(tool))
        .ink(INK_SUBTLE)
        .activity();
    let verdict = text::call_verdict(tool).map(|verdict| {
        let mut highlights = separators(&verdict);
        highlights.insert(
            0,
            (
                0..text::FAILED_TAG.len(),
                HighlightStyle {
                    color: Some(rgb(BLOCKED).into()),
                    ..Default::default()
                },
            ),
        );
        div()
            .debug_selector(|| "tool-verdict".into())
            .flex_shrink_0()
            .whitespace_nowrap()
            .ml(px(theme::SPACE_2))
            .child(selection.line(block, verdict, highlights))
    });
    let words = div()
        .child(
            div()
                .debug_selector(|| "tool-call".into())
                .flex_shrink(1.)
                .min_w_0()
                .child(edge_cut(
                    grid.line,
                    INK_SUBTLE,
                    div().pr(px(small.cell())).child(call),
                )),
        )
        .children(verdict)
        .children(chevron);
    let row_selector = format!("tool-row-{}", tool.call);
    // The live link comes after the row's press overlay, so it is pressed
    // itself rather than unfolding the row.
    let line = activity_line(grid, None, words)
        .id(SharedString::from(row_selector.clone()))
        .debug_selector(move || row_selector.clone())
        .group(DISCLOSURE_ROW)
        .children(overlay)
        .children(live);
    let mut card = div()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .child(target_ground(targeted, line));
    if let Some(progress) = tool.progress.filter(|_| tool.state == ToolState::Running) {
        card = card.child(running_tests(&tool.call, progress, grid));
    }
    if let Some(path) = shown_image(tool) {
        // The picture the call read, under its row whether the row is open
        // or shut (Claude's); a click opens it in the lightbox.
        let gallery: std::sync::Arc<[std::path::PathBuf]> = [path.clone()].into();
        let picture = inline_image(block, 0, &path, &gallery, row_cx.preview);
        card = card.child(if grouped {
            div()
                .px(px(theme::TOOL_CARD_INSET))
                .py(px(theme::SPACE_3))
                .child(picture)
        } else {
            div().mt(px(theme::PROMPT_IMAGE_GAP)).child(picture)
        });
    }
    if expanded {
        if let Some(body) = call_body(block, tool, grid, selection) {
            card = card.child(if grouped {
                body.mt(px(theme::SPACE_3)).pb(px(theme::SPACE_1))
            } else {
                div()
                    .debug_selector(|| "tool-card".into())
                    .mt(px(theme::SPACE_1))
                    .ml(px(-theme::CARD_OUTSET))
                    .px(px(theme::TOOL_CARD_INSET))
                    .py(px(theme::SPACE_3))
                    .rounded(px(theme::R_CARD))
                    .border_1()
                    .border_color(theme::paint::CARD_RING)
                    .child(body)
            });
        }
        for each in &tool.diffs {
            card = card.child(render_diff(Some((selection, block)), each, grid));
        }
    }
    div()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .child(card)
        .into_any_element()
}

/// The file a call that read a picture shows: the one its summary names,
/// when it names one whole (the provider's own paths are absolute).
fn shown_image(tool: &ToolBlock) -> Option<std::path::PathBuf> {
    let path = std::path::Path::new(&tool.summary);
    (tool.image && path.is_absolute()).then(|| path.to_path_buf())
}

/// A group's disclosure state: the keyboard's target and the pointer's
/// toggle over its summary, and how its members draw —
/// every call while open; while shut, the call still working
/// (`text::group_live`). They draw after the summary, so their text
/// registers after its own.
pub(crate) struct GroupCx<'a> {
    pub disclosure: Option<Disclosure>,
    pub members: Box<dyn FnOnce() -> Vec<AnyElement> + 'a>,
}

/// A run of calls under one summary (`text::group_summary`), an activity
/// line as a call's is (`activity_line`): `Ran 2 commands (1 failed) ▸`,
/// all in `INK_SUBTLE` — only a named file (`INK_STRONG`) and its diff counts wear
/// their own ink; the failure is red on its call's row, not here. The whole
/// line is the pointer's toggle — it lifts to `INK_STRONG` under the pointer, its
/// chevron always shown, a quarter turned when open — and the keyboard's
/// (`ToolDisclosure`), wearing `paint::RAISED` when targeted.
///
/// Under it, the calls stand in one card (Claude's): a `CARD_RING`
/// hairline at `R_CARD`, no fill, hanging `CARD_OUTSET` past the
/// column; each call the row it would be alone, its words `TOOL_CARD_INSET`
/// in, `TOOL_CARD_ROW_H` for its line, a `LINE` hairline between rows. Open,
/// every call; shut, a working call alone, so a live suite's bar stays in
/// view. Opening and shutting snap.
pub(crate) fn render_tool_group(members: &[Block], row_cx: &RowCx, state: GroupCx) -> AnyElement {
    let grid = row_cx.grid();
    let GroupCx {
        disclosure,
        members: draw_members,
    } = state;
    let Some(leader) = members.first() else {
        return div().into_any_element();
    };
    let leader_call = match &leader.body {
        Body::Tool(tool) => tool.call.clone(),
        _ => format!("{:?}", leader.id),
    };
    let (overlay, chevron, targeted, focus) = match disclosure {
        Some(Disclosure {
            overlay,
            chevron,
            targeted,
            focus,
            ..
        }) => (overlay, chevron, targeted, focus),
        None => (None, None, false, None),
    };
    let summary = text::group_summary(members);
    let mut highlights = separators(&summary.text);
    for (range, mark) in &summary.marks {
        let ink = match mark {
            text::SummaryMark::File => INK_STRONG,
            text::SummaryMark::Added => RUNNING,
            text::SummaryMark::Removed => BLOCKED,
        };
        highlights.push((
            range.clone(),
            HighlightStyle {
                color: Some(rgb(ink).into()),
                ..Default::default()
            },
        ));
    }
    // Highlights apply in order: the seams and the marks interleave.
    highlights.sort_by_key(|(range, _)| range.start);
    let words = div()
        .child(
            div()
                .debug_selector(|| "tool-group-summary".into())
                .flex_shrink(1.)
                .min_w_0()
                .group_hover(DISCLOSURE_ROW, |style| style.text_color(rgb(INK_STRONG)))
                .child(edge_cut(
                    grid.line,
                    INK_SUBTLE,
                    row_cx
                        .selection
                        .line(leader.id, summary.text.clone(), highlights),
                )),
        )
        .children(chevron);
    let row_selector = format!("tool-group-{leader_call}");
    let mut header = activity_line(grid, None, words)
        .id(SharedString::from(row_selector.clone()))
        .debug_selector(move || row_selector.clone())
        .group(DISCLOSURE_ROW)
        .children(overlay);
    if targeted {
        header = header.bg(theme::paint::RAISED).child(
            div()
                .debug_selector(|| "tool-disclosure-keyboard-target".into())
                .absolute()
                .inset_0(),
        );
        if let Some(focus) = &focus {
            header = header.track_focus(focus).key_context("ToolDisclosure");
        }
    }
    let drawn = draw_members();
    let pad_y = ((theme::TOOL_CARD_ROW_H - grid.line) / 2.).max(0.);
    let card = (!drawn.is_empty()).then(|| {
        div()
            .debug_selector(|| "tool-group-members".into())
            .flex()
            .flex_col()
            .min_w_0()
            .mt(px(theme::SPACE_1))
            .ml(px(-theme::CARD_OUTSET))
            .rounded(px(theme::R_CARD))
            .border_1()
            .border_color(theme::paint::CARD_RING)
            .overflow_hidden()
            .children(drawn.into_iter().enumerate().map(|(at, member)| {
                div()
                    .w_full()
                    .min_w_0()
                    .px(px(theme::TOOL_CARD_INSET))
                    .py(px(pad_y))
                    .when(at > 0, |row| {
                        row.border_t_1().border_color(theme::paint::CARD_RING)
                    })
                    .child(member)
            }))
    });
    div()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .text_color(rgb(INK_SUBTLE))
        .child(header)
        .children(card)
        .into_any_element()
}

/// A disclosable row's parts, built by the transcript for the row renderer:
/// for a reasoning row the click target over its header and its chevron;
/// for every row whether the keyboard targets it (and the focus a targeted
/// tool row tracks).
pub struct Disclosure {
    /// The pointer's toggle: over a reasoning row's whole header, so its
    /// label toggles it too; over a tool row's icon cell alone, so its
    /// call line stays plain selectable text.
    pub overlay: Option<AnyElement>,
    /// The trailing chevron (`disclosure_chevron`, reasoning rows).
    pub chevron: Option<AnyElement>,
    /// Keyboard cycling has landed on this row: it wears the `paint::RAISED`
    /// ground so the operator sees which row Enter will toggle. The pointer
    /// never sets it, and no other row ever wears a ground.
    pub targeted: bool,
    /// The focus a targeted tool row tracks (its keys' context).
    pub focus: Option<FocusHandle>,
}

/// An activity row under the keyboard's disclosure target: `paint::RAISED`,
/// the target's selector on a layer over it so the row keeps its own.
fn target_ground(targeted: bool, row: Stateful<Div>) -> Stateful<Div> {
    row.when(targeted, |row| {
        row.bg(theme::paint::RAISED).child(
            div()
                .debug_selector(|| "tool-disclosure-keyboard-target".into())
                .absolute()
                .inset_0(),
        )
    })
}

impl Disclosure {
    /// The row's keyboard-target ground, and its selector.
    fn ground(targeted: bool, row: Stateful<Div>) -> Stateful<Div> {
        row.when(targeted, |row| {
            row.bg(theme::paint::RAISED)
                .debug_selector(|| "tool-disclosure-keyboard-target".into())
        })
    }
}

/// Split an optional disclosure into its parts for a row renderer.
fn disclosure_parts(
    disclosure: Option<Disclosure>,
) -> (Option<AnyElement>, Option<AnyElement>, bool) {
    match disclosure {
        Some(Disclosure {
            overlay,
            chevron,
            targeted,
            ..
        }) => (overlay, chevron, targeted),
        None => (None, None, false),
    }
}

/// A reasoning row's click target: an overlay over the whole row, so the
/// label toggles it too. The row keeps its own mark in the gutter (`∴`);
/// the disclosure is the trailing chevron (`disclosure_chevron`). Nothing
/// grounds the row under the pointer.
pub fn tool_disclosure_control(
    call: &DisclosureId,
    expanded: bool,
    targeted: bool,
    focus: &FocusHandle,
    hit: f32,
) -> Div {
    let tooltip = match (call, expanded) {
        (DisclosureId::Reasoning(_), false) => "Show reasoning",
        (DisclosureId::Reasoning(_), true) => "Hide reasoning",
        (DisclosureId::Group(_), false) => "Show tool calls",
        (DisclosureId::Group(_), true) => "Hide tool calls",
        (_, false) => "Show tool details",
        (_, true) => "Hide tool details",
    };
    // The whole row is the target; its gutter keeps the named `hit` box
    // (the 2-cell gutter, with the tooltip) where the row's mark hangs, so
    // the pointer finds the same target it always has.
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_start()
        .cursor_pointer()
        .when(targeted, |control| {
            control.track_focus(focus).key_context("ToolDisclosure")
        })
        .child(
            div()
                .id(SharedString::from(format!("tool-button-{call}")))
                .flex_shrink_0()
                .w(px(hit))
                .h(px(hit.max(theme::TOOL_DISCLOSURE_HIT)))
                .tooltip(ferrite_ui::menu::tooltip(tooltip)),
        )
}

/// A disclosure's trailing chevron: `ICON_CHEVRON` in `TEXT_FAINT`, its
/// box always reserved so nothing moves when it shows. It shows while the
/// pointer is on the row (lifting to `INK_SUBTLE`) or while the keyboard
/// targets the row (or always, for a group's), and it is turned a quarter
/// when open. The turn eases over `motion::CHEVRON` (Claude's 60ms
/// `TurnStatus` caret) when the fold flips under it, keyed by `id`; the
/// group and its folds themselves snap, as Claude's tool steps do.
pub fn disclosure_chevron(
    id: impl Into<gpui::ElementId>,
    expanded: bool,
    targeted: bool,
) -> AnyElement {
    let mark = ferrite_ui::motion::settled(id, expanded, ferrite_ui::motion::CHEVRON, |turn| {
        components::chevron_at(turn, theme::ICON_CHEVRON, TEXT_FAINT)
            .group_hover(DISCLOSURE_ROW, |style| style.text_color(rgb(INK_SUBTLE)))
    });
    div()
        .debug_selector(|| "disclosure-chevron".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .ml(px(theme::SPACE_1))
        .size(px(theme::ICON_CHEVRON))
        .when(!targeted, |chevron| {
            chevron
                .invisible()
                .group_hover(DISCLOSURE_ROW, |style| style.visible())
        })
        .child(mark)
        .into_any_element()
}

/// The hairline well a diff sits in, as Claude's (the command's well in
/// `output.rs` is the same): no fill, `R_CONTROL` corners, `BLOCK_PAD_Y`
/// above and below its lines, and an inset 1px `CARD_RING` laid over it (so
/// it moves nothing), set at the block's grid (`Grid::block`). Inside an
/// open tool group it is a well in the card, never a filled box in a box.
pub(crate) fn diff_well(grid: Grid) -> Div {
    div()
        .relative()
        .w_full()
        .min_w_0()
        .py(px(theme::BLOCK_PAD_Y))
        .rounded(px(theme::R_CONTROL))
        .text_size(px(grid.size))
        .line_height(px(grid.line))
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(px(theme::R_CONTROL))
                .border_1()
                .border_color(theme::paint::CARD_RING),
        )
}

/// Markdown spans flattened to one wrapping run — its text and highlight
/// runs, for the selection overlay to wash and register (#27) — so inline
/// code keeps its place in the sentence instead of becoming its own box.
/// Bold and links carry their own styles (#22 C13); links stay inert —
/// paths render, nothing opens.
fn inline(spans: &[Span]) -> (String, Vec<(std::ops::Range<usize>, HighlightStyle)>) {
    let mut text = String::new();
    let mut highlights = Vec::new();
    for span in spans {
        let start = text.len();
        text.push_str(&span.text);
        if let Some(style) = span_style(span.style) {
            highlights.push((start..text.len(), style));
        }
    }
    (text, highlights)
}

/// One span's highlight, or none where the run wears the block's own ink.
fn span_style(style: Style) -> Option<HighlightStyle> {
    match style {
        Style::Plain => None,
        // Inline code's ink (its chip is the Markdown path's).
        Style::Code => Some(HighlightStyle {
            color: Some(rgb(INLINE_CODE_INK).into()),
            ..Default::default()
        }),
        // `strong`: weight 600 in `INK_STRONG`.
        Style::Bold => Some(HighlightStyle {
            color: Some(rgb(INK_STRONG).into()),
            font_weight: Some(W_STRONG),
            ..Default::default()
        }),
        // A link or a path reads `INK_STRONG`, with no underline at rest.
        // Inert — paths render, nothing opens.
        Style::Link => Some(HighlightStyle {
            color: Some(rgb(theme::LINK_INK).into()),
            ..Default::default()
        }),
    }
}

/// A prose Block's text: one wrapping run, so a sentence breaks where the
/// terminal's does. Narration (`quiet`) takes its row's ink: its bold and
/// links keep their weight, its code a step brighter.
fn prose(block: BlockId, spans: &[Span], selection: &TextRuns) -> AnyElement {
    let (text, highlights) = inline(spans);
    selection.line(block, text, highlights).into_any_element()
}

/// A fenced block's rows: the whole block as one literal run, so a copy
/// takes it back exactly and the indentation is the text's own.
fn code_lines(
    block: BlockId,
    source: &str,
    highlights: Vec<(std::ops::Range<usize>, HighlightStyle)>,
    selection: &TextRuns,
) -> Vec<Div> {
    // Native text views expose their own bounds to the test harness.
    vec![div().child(selection.line(block, source.to_string(), highlights))]
}

/// `theme::init_components`, with the transcript's code highlighting and
/// keys: what every window and test window of the app calls.
pub fn init_components(cx: &mut gpui::App) {
    let defaults = gpui::base::text::TextViewDefaults::global(cx);
    if !defaults.has_code_block_highlighter() {
        // GPUI caches highlighting by callback identity. Install it once so
        // unrelated pane renders do not highlight completed code again.
        defaults
            .with_code_block_highlighter(|block| {
                let source = block.code();
                let language = block.lang();
                let tokens =
                    ferrite_core::transcript::highlight_tokens(language.as_deref(), &source);
                code(&source, Some(&tokens))
            })
            .install(cx);
    }
    let first = !cx.has_global::<gpui::component::Theme>();
    ferrite_ui::theme::init_components(cx);
    if first {
        crate::attachments::init(cx);
        crate::rich::init(cx);
    }
}

/// Syntax highlight runs for a code Block, or none while the highlighter is
/// still thinking.
pub fn code(
    source: &str,
    tokens: Option<&[Token]>,
) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    let Some(tokens) = tokens else {
        return Vec::new();
    };
    let mut highlights = Vec::new();
    let mut at = 0;
    for token in tokens {
        let end = at + token.text.len();
        // A highlighter that disagrees with the source is ignored, not trusted
        // into a panic.
        if end > source.len() {
            return Vec::new();
        }
        highlights.push((at..end, crate::rich::syntax_style(token.class)));
        at = end;
    }
    joined_runs(source, highlights)
}

/// Runs joined as a page's markup joins them: neighbours of one style are
/// one run, and so are two of one style with only blanks between them
/// (`pub const`, `let mut`). A browser lays each run out as its own inline
/// box, so where the runs break decides where the glyphs after them fall.
fn joined_runs(
    source: &str,
    highlights: Vec<(std::ops::Range<usize>, HighlightStyle)>,
) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    let blank = |range: &std::ops::Range<usize>| source[range.clone()].trim().is_empty();
    let mut joined: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
    let mut at = 0;
    while at < highlights.len() {
        let (range, style) = highlights[at].clone();
        match joined.last_mut() {
            Some((last, last_style)) if *last_style == style && last.end == range.start => {
                last.end = range.end;
            }
            // `a␠b`: a blank run between two of one style.
            Some((last, last_style))
                if blank(&range)
                    && last.end == range.start
                    && highlights.get(at + 1).is_some_and(|(next, next_style)| {
                        next_style == last_style && next.start == range.end
                    }) =>
            {
                last.end = highlights[at + 1].0.end;
                at += 1;
            }
            _ => joined.push((range, style)),
        }
        at += 1;
    }
    joined
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::docview::Instruments;
    use ferrite_core::transcript::Class;
    /// A multi-line queued prompt reads as one line in its one-line row.
    #[test]
    fn a_queued_prompt_previews_on_one_line() {
        assert_eq!(
            queued_preview("try again...\n@\"/tmp/shot.png\"\r\n\n  thanks "),
            "try again... @\"/tmp/shot.png\" thanks"
        );
    }
    /// The placeholder carries one key hint at most (ADR 0014): a
    /// prediction's accept key, the one thing about it the line cannot
    /// show, never cut.
    #[test]
    fn the_placeholder_carries_the_one_key_hint() {
        let live = Transcript::default();
        assert!(!placeholder(false, false, Some(&live), None).accept);
        let mut answered = Transcript::default();
        answered.apply(Input::Prompt("fix the decoder".into()));
        answered.apply(Input::Event(SessionEvent::TextDelta {
            text: "Fixed it.".into(),
        }));
        answered.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        }));
        assert_eq!(
            placeholder(false, false, Some(&answered), Some("Run the tests")).text(),
            "Run the tests \u{b7} \u{21e5} accept"
        );
        assert_eq!(
            placeholder(false, true, None, Some("Run the tests")).text(),
            "Describe a task or ask a question",
            "a draft has no conversation to predict from"
        );
    }
    use ferrite_core::transcript::{Input, Lexer};
    use ferrite_core::{Hunk, SessionEvent, ToolResult, TurnOutcome};
    use gpui::{size, TestAppContext};
    use std::sync::Arc;

    struct ShowsProgress(Transcript);

    impl Render for ShowsProgress {
        fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w_full()
                .child(working_row(&self.0, false, false, false, false))
        }
    }

    /// The line says what runs: a running call's own words take the line,
    /// on one row with its facts, over the thinking heading that came
    /// before it, and with no key hint (the stop button says it).
    #[gpui::test]
    fn the_line_names_the_running_call_on_one_row(cx: &mut TestAppContext) {
        let (lexer, _) = Lexer::new();
        let mut transcript = Transcript::new(Arc::new(lexer));
        transcript.apply(Input::Prompt("Build".into()));
        transcript.apply(Input::Event(SessionEvent::ReasoningSummaryDelta {
            text: "**Checking build progress**".into(),
            summary_index: 0,
        }));
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "build".into(),
            name: "commandExecution".into(),
            input: serde_json::json!({"command": "cargo build --release"}),
        }));
        let (_, cx) = cx.add_window_view(|_, cx| {
            gpui::component::init(cx);
            ShowsProgress(transcript)
        });
        for width in [740., 260.] {
            cx.simulate_resize(size(px(width), px(400.)));
            cx.run_until_parked();
            let reasoning = cx.debug_bounds("progress-reasoning").unwrap();
            let metadata = cx.debug_bounds("progress-metadata").unwrap();
            let line = cx.debug_bounds("progress-call").expect("the call's words");
            assert!(cx
                .debug_bounds("progress-caption-Checking build progress")
                .is_none());
            // One row, as the Claude app draws it: the step, then its facts.
            assert_eq!(metadata.top(), reasoning.top());
            assert!(metadata.left() >= reasoning.right());
            assert_eq!(
                line.size.height,
                px(Grid::pane(Default::default(), true).line),
                "one line, and no command detail below it"
            );
            assert!(metadata.right() <= px(width), "the caption truncates first");
        }
    }

    struct StartingAndWorking(Transcript);

    impl Render for StartingAndWorking {
        fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
            let line = |selector: &'static str, line: Div| {
                div().debug_selector(move || selector.into()).child(line)
            };
            let working =
                |solo| working_line_for(&self.0, false, solo, false, false, true, 0, None);
            div()
                .w_full()
                .flex()
                .flex_col()
                .child(line("solo-starting", starting_line(true, true)))
                .child(line("solo-working", working(true)))
                .child(line("group-starting", starting_line(true, false)))
                .child(line("group-working", working(false)))
        }
    }

    /// `Starting` holds the working line's own row, Solo's and a Group
    /// Pane's, so the line never moves when the turn's words replace it.
    #[gpui::test]
    fn the_starting_line_stands_on_the_working_line_grid(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, cx| {
            gpui::component::init(cx);
            StartingAndWorking(Transcript::default())
        });
        cx.simulate_resize(size(px(640.), px(400.)));
        cx.run_until_parked();
        for (starting, working) in [
            ("solo-starting", "solo-working"),
            ("group-starting", "group-working"),
        ] {
            assert_eq!(
                cx.debug_bounds(starting).unwrap().size.height,
                cx.debug_bounds(working).unwrap().size.height,
                "{starting} is {working}'s height"
            );
        }
    }

    /// While a Thread's Session starts its model chip shimmers (Codex's
    /// skeleton chip); once it is up the name is plain again.
    #[gpui::test]
    fn the_model_chip_shimmers_while_the_session_starts(cx: &mut TestAppContext) {
        struct Chips;
        impl Render for Chips {
            fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
                let chip = |selector: &'static str, loading| {
                    div()
                        .debug_selector(move || selector.into())
                        .child(model_effort_chip("Sonnet 4.5".into(), None, loading))
                };
                div()
                    .flex()
                    .flex_col()
                    .child(chip("starting-chip", true))
                    .child(chip("ready-chip", false))
            }
        }
        let (_, cx) = cx.add_window_view(|_, cx| {
            gpui::component::init(cx);
            Chips
        });
        cx.simulate_resize(size(px(640.), px(400.)));
        cx.run_until_parked();
        let shimmering = |chip: &'static str, cx: &mut gpui::VisualTestContext| {
            let chip = cx.debug_bounds(chip).unwrap();
            cx.debug_bounds("shimmer")
                .is_some_and(|shimmer| chip.contains(&shimmer.origin))
        };
        assert!(shimmering("starting-chip", cx), "the starting chip shimmers");
        assert!(!shimmering("ready-chip", cx), "a ready one is plain");
    }

    /// A transcript holding one of every Block kind the Pane can draw.
    fn every_kind() -> Transcript {
        let (lexer, answers) = Lexer::new();
        let mut transcript = Transcript::new(Arc::new(lexer));
        transcript.apply(Input::Prompt("run the tests".into()));
        transcript.apply(Input::Event(SessionEvent::ThinkingDelta {
            text: "weighing it up".into(),
        }));
        transcript.apply(Input::Event(SessionEvent::TextDelta {
            text: "## Plan\nI will run `cargo test` first.\n- one\n- two\n\n\
                   ```rust\nfn main() {}\n```\ndone.\n\n"
                .into(),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "toolu_1".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "cargo test" }),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "toolu_1".into(),
            output: "42 passed".into(),
            is_error: false,
            result: ToolResult::Opaque,
        }));
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "toolu_2".into(),
            name: "Edit".into(),
            input: serde_json::json!({ "file_path": "/workspace/x.txt" }),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "toolu_2".into(),
            output: "applied".into(),
            is_error: true,
            result: ToolResult::FileEdit {
                path: "/workspace/x.txt".into(),
                hunks: vec![Hunk {
                    old_start: 1,
                    old_lines: 2,
                    new_start: 1,
                    new_lines: 2,
                    lines: vec![" alpha".into(), "-bravo".into(), "+delta".into()],
                    section: None,
                }],
            },
        }));
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "toolu_3".into(),
            name: "Read".into(),
            input: serde_json::json!({ "file_path": "/workspace/missing" }),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "toolu_3".into(),
            output: "No such file or directory".into(),
            is_error: true,
            result: ToolResult::Opaque,
        }));
        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: Some(0.038),
        }));
        transcript.apply(Input::CompletionObservation {
            elapsed_ms: 4_100,
            completed_at: "8:53 pm".into(),
            input_tokens: None,
            output_tokens: None,
        });
        transcript.apply(Input::Notice("send failed: broken pipe".into()));
        for answer in answers.try_iter() {
            transcript.apply(answer);
        }
        transcript
    }

    /// Renders Blocks through a real view: hover styles look up the view
    /// they are painting under, which a bare `cx.draw` does not have. Owns
    /// the selection whose overlay every run routes through (#27), so tests
    /// can read what registered and aim carets at it.
    struct ShowsBlocks {
        thread: ThreadId,
        selection: crate::select::TranscriptText,
        blocks: Vec<Block>,
        expanded: HashSet<String>,
        /// Groups open, by their first call.
        groups: HashSet<String>,
        reasoning_expanded: bool,
        transcript: Entity<crate::transcript::TranscriptView>,
        display_revision: u64,
    }

    impl Render for ShowsBlocks {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            self.display_revision = self.display_revision.wrapping_add(1);
            let mut expanded: HashSet<DisclosureId> = self
                .expanded
                .iter()
                .cloned()
                .map(DisclosureId::Tool)
                .chain(self.groups.iter().cloned().map(DisclosureId::Group))
                .collect();
            if self.reasoning_expanded {
                expanded.extend(self.blocks.iter().filter_map(|block| {
                    matches!(&block.body, Body::Thinking(_))
                        .then_some(DisclosureId::Reasoning(block.id))
                }));
            }
            let input = crate::transcript::TranscriptInput {
                thread: self.thread,
                namespace: "pane-block-test".into(),
                content_revision: (0, 0),
                display_revision: self.display_revision,
                blocks: self.blocks.clone(),
                signal_status: Some(Status::Idle),
                focused: true,
                reading_size: Default::default(),
                selection_scope: gpui::base::TextSelectionScopeId::new(),
                preview: crate::preview::Preview::new(cx),
                expanded,
                target: None,
                disclosure_focus: cx.focus_handle(),
                working: None,
                working_reasoning: None,
                tail: None,
                pending_call: None,
                workspace: None,
                solo: false,
                thinking: true,
                agents: Default::default(),
                rewind: None,
                quotes: true,
                reply_to: "Claude".into(),
                skills: Default::default(),
                note: None,
            };
            let selection = self.selection.clone();
            self.transcript.update(cx, |transcript, cx| {
                transcript.sync(input, selection, cx);
                transcript.assert_text_projection(cx);
            });
            self.transcript.clone()
        }
    }

    fn shows_blocks(blocks: Vec<Block>, cx: &mut Context<ShowsBlocks>) -> ShowsBlocks {
        let thread = ThreadId::new(1);
        let selection = crate::select::TranscriptText::default();
        let transcript = cx.new(|cx| {
            crate::transcript::TranscriptView::new(
                crate::transcript::TranscriptInput {
                    thread,
                    namespace: "pane-block-test".into(),
                    content_revision: (0, 0),
                    display_revision: 0,
                    blocks: blocks.clone(),
                    signal_status: Some(Status::Idle),
                    focused: true,
                    reading_size: Default::default(),
                    selection_scope: gpui::base::TextSelectionScopeId::new(),
                    preview: crate::preview::Preview::new(cx),
                    expanded: HashSet::new(),
                    target: None,
                    disclosure_focus: cx.focus_handle(),
                    working: None,
                    working_reasoning: None,
                    tail: None,
                    pending_call: None,
                    workspace: None,
                    solo: false,
                    thinking: true,
                    agents: Default::default(),
                    rewind: None,
                    quotes: true,
                    reply_to: "Claude".into(),
                    skills: Default::default(),
                    note: None,
                },
                crate::rich::TextCache::default(),
                selection.clone(),
                cx,
            )
        });
        ShowsBlocks {
            thread,
            selection,
            blocks,
            expanded: HashSet::new(),
            groups: HashSet::new(),
            reasoning_expanded: false,
            transcript,
            display_revision: 0,
        }
    }

    /// An operator running many Threads has to know which of them share the
    /// checkout and which cannot trample it.
    #[test]
    fn the_chrome_names_the_workspace_a_thread_works_in() {
        assert_eq!(
            binding_label(Some(&WorkspaceBinding::Worktree {
                repo: "/repo".into(),
                path: "/repo/../ferrite-thread-3".into(),
            })),
            "ferrite-thread-3"
        );
        assert_eq!(
            binding_label(Some(&WorkspaceBinding::Main {
                checkout: "/repo".into()
            })),
            "main"
        );
        // A Thread from before bindings existed claims nothing.
        assert_eq!(binding_label(None), "");
    }

    /// #25: the picker shows the bare model name — the provider is the
    /// logomark beside it, so the label never repeats it and never carries
    /// a `·` seam. An id with no known provider prefix stands verbatim
    /// rather than being guessed apart.
    #[test]
    fn the_model_label_strips_the_provider_the_logomark_already_names() {
        assert_eq!(model_label("claude-sonnet-4-5").as_ref(), "Sonnet 4.5");
        assert_eq!(model_label("codex-gpt-5.4-mini").as_ref(), "GPT-5.4 Mini");
        assert_eq!(model_label("gpt-5.6").as_ref(), "GPT-5.6");
        assert_eq!(model_label("claude-fable-5-1").as_ref(), "Fable 5.1");
    }

    /// #26, the Row rule from the pointer's side: rows that answer clicks
    /// advertise it with the cursor — the selected row keeps it while
    /// skipping the wash — and an inert row promises nothing, for the same
    /// reason it draws no ↵ hint. Keycaps are Controls and say so too.
    #[test]
    fn rows_advertise_their_click_with_the_cursor_and_inert_rows_do_not() {
        use gpui::CursorStyle;
        fn cursor(mut drawn: impl Styled) -> Option<CursorStyle> {
            drawn.style().mouse_cursor
        }
        let offer = MenuRow {
            insert: "/import".into(),
            name: "import".into(),
            matched: vec![],
            detail: "adopt a CLI session".into(),
            prose_detail: true,
            inert: false,
            tag: None,
        };
        assert_eq!(
            cursor(menu_row(("r", 0usize), &offer, false, None)),
            Some(CursorStyle::PointingHand)
        );
        assert_eq!(
            cursor(menu_row(("r", 0usize), &offer, true, None)),
            Some(CursorStyle::PointingHand),
            "the selected row skips the wash, never the cursor"
        );
        let inert = MenuRow {
            insert: offer.insert.clone(),
            name: offer.name.clone(),
            matched: vec![],
            detail: offer.detail.clone(),
            prose_detail: true,
            inert: true,
            tag: None,
        };
        assert_eq!(cursor(menu_row(("r", 1usize), &inert, false, None)), None);
        assert_eq!(cursor(menu_row(("r", 1usize), &inert, true, None)), None);
        // No ↵, not even on the cursor row (Claude's).
        assert_eq!(menu_item(&offer, None).shortcut, None);
        // A command's source rides as its tag.
        let tagged = MenuRow {
            insert: offer.insert.clone(),
            name: offer.name.clone(),
            matched: vec![],
            detail: offer.detail.clone(),
            prose_detail: true,
            inert: false,
            tag: Some("Skill".into()),
        };
        assert_eq!(menu_item(&tagged, None).tag.as_deref(), Some("Skill"));
        assert_eq!(menu_item(&offer, None).tag, None);
        // A long directory keeps its tail.
        let deep = MenuRow {
            detail: "crates/ferrite/src/some/very/deeply/nested/module/tree/of/files".into(),
            prose_detail: false,
            ..inert
        };
        let detail = menu_item(&deep, None).detail.unwrap();
        assert!(detail.starts_with("…/") && detail.ends_with("tree/of/files"));
        assert!(detail.chars().count() <= theme::MENU_PATH_TAIL + 2);

        // The ✓-row both selectors share follows the same rule.
        assert_eq!(
            cursor(picker_row(
                ("p", 0usize),
                "workspace root".into(),
                "".into(),
                false,
                false,
                false
            )),
            Some(CursorStyle::PointingHand)
        );
        assert_eq!(
            cursor(picker_row(
                ("p", 1usize),
                "workspace root".into(),
                "".into(),
                true,
                true,
                false
            )),
            Some(CursorStyle::PointingHand)
        );

        // The decide keycaps answer the mouse (#26) and say so.
        assert_eq!(
            cursor(decision::key_action("y allow", "y", "allow", true)),
            Some(CursorStyle::PointingHand)
        );
        assert_eq!(
            cursor(decision::key_action("a always", "a", "always", false)),
            Some(CursorStyle::PointingHand)
        );
    }

    /// The app is thin by design, so its render test is that every Block kind
    /// the core can produce actually lays out and paints in a window.
    #[gpui::test]
    fn every_block_kind_paints(cx: &mut TestAppContext) {
        let transcript = every_kind();
        let failed_edit = transcript
            .blocks()
            .iter()
            .find_map(|block| match &block.body {
                Body::Tool(tool) if tool.call == "toolu_2" => Some(tool),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            (failed_edit.diffs[0].added, failed_edit.diffs[0].removed),
            (1, 1)
        );
        assert!(text::call_failed(failed_edit));
        let blocks: Vec<Block> = transcript.blocks().to_vec();

        let kinds: Vec<&str> = blocks
            .iter()
            .map(|block| match &block.body {
                Body::Prompt(_) => "prompt",
                Body::Paragraph { .. } => "paragraph",
                Body::Heading { .. } => "heading",
                Body::Bullet { .. } => "bullet",
                Body::Code { .. } => "code",
                Body::Tool(tool) => match (&tool.state, tool.diffs.is_empty()) {
                    (_, false) => "diff",
                    (ToolState::Failed(_), _) => "tool-failed",
                    _ => "tool",
                },
                Body::Thinking(_) => "thinking",
                Body::Notice(_) => "notice",
                Body::Meta(_) => "meta",
                Body::TurnEnd(_) => "turn-end",
                Body::Visual(_) => "visual",
                Body::WrapUp(_) => "wrap-up",
            })
            .collect();
        for wanted in [
            "prompt",
            "paragraph",
            "heading",
            "bullet",
            "code",
            "tool",
            "diff",
            "tool-failed",
            "thinking",
            "notice",
            "turn-end",
        ] {
            assert!(kinds.contains(&wanted), "no {wanted} block in {kinds:?}");
        }

        let (_view, cx) = cx.add_window_view(|_, cx| {
            gpui::component::init(cx);
            shows_blocks(blocks, cx)
        });
        // A resize forces a real layout-and-paint pass through the view.
        cx.simulate_resize(size(px(900.), px(600.)));
        cx.run_until_parked();

        cx.update(|_, cx| crate::rich::testing::select_all(cx));
        cx.run_until_parked();
    }

    #[gpui::test]
    fn contract_structured_tool_result_is_visible_in_shared_disclosure(cx: &mut TestAppContext) {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "structured".into(),
            name: "Tool".into(),
            input: serde_json::json!({}),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "structured".into(),
            output: String::new(),
            is_error: false,
            result: ToolResult::Structured {
                value: serde_json::json!({"detail":"visible-provider-detail"}),
                duration_ms: None,
            },
        }));
        let (view, cx) = cx.add_window_view(|_, cx| {
            gpui::component::init(cx);
            let mut view = shows_blocks(transcript.blocks().to_vec(), cx);
            view.expanded.insert("structured".into());
            view
        });
        cx.simulate_resize(size(px(900.), px(600.)));
        cx.run_until_parked();
        let runs = view.read_with(cx, |view, _| view.selection.registered(ThreadId::new(1)));
        assert!(
            runs.iter()
                .any(|(_, _, _, text)| text.contains("visible-provider-detail")),
            "preserved provider data must be inspectable even without model-facing output"
        );
    }

    #[test]
    fn contract_multi_file_tool_verdict_counts_all_native_hunks() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "edit".into(),
            name: "Edit".into(),
            input: serde_json::json!({}),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "edit".into(),
            output: String::new(),
            is_error: false,
            result: ToolResult::FileEdits {
                edits: ["a.txt", "b.txt"]
                    .into_iter()
                    .map(|path| ferrite_core::FileEdit {
                        path: path.into(),
                        hunks: vec![Hunk {
                            old_start: 1,
                            old_lines: 1,
                            new_start: 1,
                            new_lines: 1,
                            lines: vec!["-old".into(), "+new".into()],
                            section: None,
                        }],
                    })
                    .collect(),
            },
        }));
        let Body::Tool(tool) = &transcript.blocks()[0].body else {
            panic!("expected tool")
        };
        assert_eq!(
            tool.diffs
                .iter()
                .map(|diff| (diff.added, diff.removed))
                .fold((0, 0), |(a, r), (added, removed)| (a + added, r + removed)),
            (2, 2)
        );
    }

    /// A call line is one line, however long or many-lined its command:
    /// the argument is the command's first line, cut with `…` at the row's
    /// edge, and the row under it starts where it ends (the overlap bug).
    #[gpui::test]
    fn a_long_or_multi_line_command_keeps_its_call_line_one_line_tall(cx: &mut TestAppContext) {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("check".into()));
        let heredoc = format!(
            "python3 - <<'EOF'\nimport sys\n{}\nEOF",
            "print('a long line of the script')\n".repeat(20)
        );
        let long = format!(
            "grep -rn {} crates/",
            "\"paint::FRAME\\|paint::BAR\" ".repeat(12)
        );
        for (id, command) in [("multi", heredoc.as_str()), ("long", long.as_str())] {
            transcript.apply(Input::Event(SessionEvent::ToolStarted {
                id: id.into(),
                name: "Bash".into(),
                input: serde_json::json!({ "command": command }),
            }));
            transcript.apply(Input::Event(SessionEvent::ToolCompleted {
                id: id.into(),
                output: "ok".into(),
                is_error: false,
                result: ferrite_core::ToolResult::Opaque,
            }));
            transcript.apply(Input::Event(SessionEvent::TextDelta {
                text: format!("after {id}\n\n"),
            }));
        }
        let blocks: Vec<Block> = transcript.blocks().to_vec();
        let id_of = |pick: &dyn Fn(&Body) -> bool| {
            blocks
                .iter()
                .find(|block| pick(&block.body))
                .map(|block| block.markdown_run.unwrap_or(block.id))
                .unwrap()
        };
        let after = |id: &'static str| {
            move |body: &Body| {
                matches!(body, Body::Paragraph { spans }
                    if spans.iter().any(|span| span.text.contains(id)))
            }
        };
        let calls = [
            id_of(&|body| matches!(body, Body::Tool(tool) if tool.call == "multi")),
            id_of(&|body| matches!(body, Body::Tool(tool) if tool.call == "long")),
        ];
        let answers = [id_of(&after("after multi")), id_of(&after("after long"))];
        let (_view, cx) = cx.add_window_view(|_, cx| {
            gpui::component::init(cx);
            shows_blocks(blocks.clone(), cx)
        });
        cx.simulate_resize(size(px(520.), px(900.)));
        cx.run_until_parked();
        let line = px(Grid::pane(Default::default(), false).line);
        let rows = [
            cx.debug_bounds("tool-row-multi")
                .expect("the heredoc's call line"),
            cx.debug_bounds("tool-row-long")
                .expect("the long command's call line"),
        ];
        cx.update(|_, cx| {
            for ((row, call), answer) in rows.iter().zip(calls).zip(answers) {
                assert!(
                    (row.size.height - line).abs() <= px(0.5),
                    "one line tall: {row:?} / {line:?}"
                );
                let words = crate::rich::testing::bounds(
                    &format!("thinking-pane-block-test-{call:?}"),
                    0,
                    cx,
                )
                .expect("the call line's run");
                assert!(
                    words.size.height <= line + px(0.5),
                    "the run is one line, never wrapped: {words:?}"
                );
                let next = crate::rich::testing::bounds(
                    &format!("markdown-pane-block-test-{answer:?}"),
                    0,
                    cx,
                )
                .expect("the prose under it");
                assert!(next.top() >= row.bottom(), "no overlap: {row:?} / {next:?}");
            }
        });
    }

    /// AC2's copy half, relocated from `block_text` (#27): every Block kind
    /// must register its text with the selection overlay when it renders —
    /// a kind that registers nothing would select and copy as a silent
    /// hole. Chrome — gutter glyphs, bullets, verdict chips, the diff
    /// number column — never registers, so it can never be copied.
    /// A group copies as it reads: shut, its summary alone; open, the
    /// summary and every call as its own row would copy. Each render also
    /// checks the copy projection against what was drawn.
    #[gpui::test]
    fn a_group_copies_what_it_draws_shut_and_open(cx: &mut TestAppContext) {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("reconcile".into()));
        for (id, command, output, error) in [
            ("g1", "python3 match.py", "12 matched", false),
            ("g2", "grep -n adelaide notes.md", "", true),
        ] {
            transcript.apply(Input::Event(SessionEvent::ToolStarted {
                id: id.into(),
                name: "Bash".into(),
                input: serde_json::json!({ "command": command }),
            }));
            transcript.apply(Input::Event(SessionEvent::ToolCompleted {
                id: id.into(),
                output: output.into(),
                is_error: error,
                result: ferrite_core::ToolResult::Opaque,
            }));
        }
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "g3".into(),
            name: "Read".into(),
            input: serde_json::json!({ "file_path": "notes.md" }),
        }));
        let blocks: Vec<Block> = transcript.blocks().to_vec();
        let thread = ThreadId::new(1);
        let (view, cx) = cx.add_window_view(|_, cx| {
            gpui::component::init(cx);
            shows_blocks(blocks, cx)
        });
        cx.simulate_resize(size(px(900.), px(600.)));
        cx.run_until_parked();
        let copied = |view: &Entity<ShowsBlocks>, cx: &mut gpui::VisualTestContext| {
            view.read_with(cx, |view, _| view.selection.registered(thread))
                .into_iter()
                .map(|(_, _, _, text)| text)
                .collect::<Vec<_>>()
                .join("\n")
        };
        let shut = copied(&view, cx);
        assert!(
            shut.contains("Ran 2 commands, reading 1 file (1 failed)"),
            "{shut}"
        );
        // Shut, it draws its working call alone.
        assert!(
            shut.contains("Reading [notes\\.md](<ferrite-path:notes.md>)"),
            "{shut}"
        );
        assert!(!shut.contains("python3 match.py"), "{shut}");
        assert!(cx.debug_bounds("tool-group-g1").is_some());
        assert!(cx.debug_bounds("tool-row-g1").is_none());
        assert!(cx.debug_bounds("tool-row-g3").is_some(), "the live call");

        view.update(cx, |view, cx| {
            view.groups.insert("g1".into());
            cx.notify();
        });
        cx.run_until_parked();
        let open = copied(&view, cx);
        for text in [
            "python3 match.py",
            "grep -n adelaide",
            "Failed",
            "Reading [notes\\.md](<ferrite-path:notes.md>)",
        ] {
            assert!(open.contains(text), "{text}: {open}");
        }
        // A shut call previews nothing it printed.
        assert!(!open.contains("12 matched"), "{open}");
        // The calls stand in a card hanging `CARD_OUTSET` past the
        // column, their words `TOOL_CARD_INSET` in from its edge.
        let summary = cx.debug_bounds("tool-group-g1").unwrap();
        let card = cx.debug_bounds("tool-group-members").unwrap();
        let member = cx.debug_bounds("tool-row-g1").unwrap();
        assert!(
            ((summary.left() - card.left()) - px(theme::CARD_OUTSET)).abs() <= px(1.0),
            "{card:?} {summary:?}"
        );
        assert!(
            ((member.left() - card.left()) - px(theme::TOOL_CARD_INSET + 1.)).abs() <= px(1.0),
            "{member:?} {card:?}"
        );
        assert!(card.top() >= summary.bottom(), "the card hangs under it");
        let second = cx.debug_bounds("tool-row-g2").unwrap();
        assert!(
            ((second.top() - member.top()) - px(theme::TOOL_CARD_ROW_H)).abs() <= px(1.5),
            "rows {} apart: {member:?} {second:?}",
            theme::TOOL_CARD_ROW_H
        );

        // A call opened in the card draws Claude's body, no box of its own:
        // its `$ cmd` in a well `SPACE_3` under its line, what it printed
        // under that. A success says nothing.
        view.update(cx, |view, cx| {
            view.expanded.insert("g1".into());
            cx.notify();
        });
        cx.run_until_parked();
        assert!(copied(&view, cx).contains("12 matched"));
        let line = cx.debug_bounds("tool-row-g1").unwrap();
        let well = cx.debug_bounds("tool-input").unwrap();
        let output = cx.debug_bounds("tool-output").unwrap();
        assert!(
            ((well.top() - line.bottom()) - px(theme::SPACE_3)).abs() <= px(1.0),
            "{line:?} {well:?}"
        );
        assert!(output.top() > well.bottom(), "{well:?} {output:?}");
        assert!(cx.debug_bounds("tool-card").is_none(), "no card in a card");
        assert!(cx.debug_bounds("tool-exit").is_none(), "a success is quiet");
    }

    /// A call opened alone stands in the group's card, its body the same;
    /// a failed command says its exit code over what it printed.
    #[gpui::test]
    fn a_call_opened_alone_stands_in_its_own_card(cx: &mut TestAppContext) {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("list it".into()));
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "s1".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "ls /missing" }),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "s1".into(),
            output: "Exit code 1\nls: /missing: No such file or directory".into(),
            is_error: true,
            result: ferrite_core::ToolResult::Opaque,
        }));
        let blocks: Vec<Block> = transcript.blocks().to_vec();
        let (view, cx) = cx.add_window_view(|_, cx| {
            gpui::component::init(cx);
            shows_blocks(blocks, cx)
        });
        cx.simulate_resize(size(px(900.), px(600.)));
        view.update(cx, |view, cx| {
            view.expanded.insert("s1".into());
            cx.notify();
        });
        cx.run_until_parked();
        let line = cx.debug_bounds("tool-row-s1").unwrap();
        let card = cx.debug_bounds("tool-card").unwrap();
        let well = cx.debug_bounds("tool-input").unwrap();
        let exit = cx.debug_bounds("tool-exit").unwrap();
        let output = cx.debug_bounds("tool-output").unwrap();
        assert!(
            ((line.left() - card.left()) - px(theme::CARD_OUTSET)).abs() <= px(1.0),
            "{line:?} {card:?}"
        );
        assert!(card.top() >= line.bottom(), "{line:?} {card:?}");
        assert!(
            well.bottom() <= exit.top() && exit.bottom() <= output.top(),
            "{well:?} {exit:?} {output:?}"
        );
    }

    #[test]
    fn a_group_summary_names_its_work_in_order_and_tense() {
        let mut transcript = Transcript::default();
        let mut call = |id: &str, name: &str, input: serde_json::Value, done: Option<bool>| {
            transcript.apply(Input::Event(SessionEvent::ToolStarted {
                id: id.into(),
                name: name.into(),
                input,
            }));
            if let Some(error) = done {
                transcript.apply(Input::Event(SessionEvent::ToolCompleted {
                    id: id.into(),
                    output: "out".into(),
                    is_error: error,
                    result: ferrite_core::ToolResult::Opaque,
                }));
            }
        };
        let path = serde_json::json!({ "file_path": "a.rs" });
        call(
            "a",
            "Grep",
            serde_json::json!({ "pattern": "x" }),
            Some(false),
        );
        call("b", "Read", path.clone(), Some(false));
        call("c", "Read", path.clone(), Some(true));
        call(
            "d",
            "mcp__github__get_issue",
            serde_json::json!({}),
            Some(false),
        );
        call(
            "e",
            "mcp__github__list_prs",
            serde_json::json!({}),
            Some(false),
        );
        call(
            "f",
            "WebFetch",
            serde_json::json!({ "url": "https://x" }),
            None,
        );
        let blocks = transcript.blocks();
        // Two kinds named, the other calls counted, the failure said in
        // the same words.
        assert_eq!(
            text::group_summary(blocks).text,
            "Searched for 1 pattern, read 2 files and 3 more (1 failed)"
        );
        let summary = text::group_summary(&blocks[..2]);
        assert_eq!(summary.text, "Searched for 1 pattern, read 1 file");
        assert!(summary.marks.is_empty());
        assert_eq!(
            text::group_summary(&blocks[..3]).text,
            "Searched for 1 pattern, read 2 files (1 failed)"
        );
        assert_eq!(
            text::group_live(blocks).first().map(|block| block.id),
            Some(blocks[5].id)
        );
        assert!(text::group_live(&blocks[..5]).is_empty());
    }

    #[gpui::test]
    fn every_block_kind_registers_its_selectable_text(cx: &mut TestAppContext) {
        let transcript = every_kind();
        let instruments = Instruments::of(&transcript);
        let blocks: Vec<Block> = transcript.blocks().to_vec();
        let ids: Vec<ferrite_core::transcript::BlockId> = blocks
            .iter()
            // A quick turn's end draws no row (`text::turn_stamp`).
            .filter(|block| !matches!(&block.body, Body::TurnEnd(end) if turn_stamp(end).is_none()))
            .map(|block| {
                if block.markdown.is_some() {
                    block.markdown_run.unwrap_or(block.id)
                } else {
                    block.id
                }
            })
            .collect();
        let reasoning: Vec<_> = blocks
            .iter()
            .filter(|block| {
                matches!(&block.body, Body::Thinking(text) if reasoning_text(text).1.is_some())
            })
            .map(|block| block.id)
            .collect();
        let thread = ThreadId::new(1);
        let (view, cx) = cx.add_window_view(|_, cx| {
            gpui::component::init(cx);
            shows_blocks(blocks, cx)
        });
        cx.simulate_resize(size(px(900.), px(600.)));
        cx.run_until_parked();

        let collapsed = view.read_with(cx, |view, _| view.selection.registered(thread));
        assert!(
            collapsed
                .iter()
                .all(|(block, _, _, _)| !reasoning.contains(block)),
            "hidden reasoning must not join copied text"
        );
        // The calls fold into one group (`Ran 1 command, edited 1 file, read
        // 1 file`): open it, so each call registers as its row does.
        view.update(cx, |view, cx| {
            view.reasoning_expanded = true;
            view.groups.insert("toolu_1".into());
            cx.notify();
        });
        cx.run_until_parked();
        let runs = view.read_with(cx, |view, _| view.selection.registered(thread));
        for id in &ids {
            assert!(
                runs.iter()
                    .any(|(block, _, _, text)| block == id && !text.trim().is_empty()),
                "no selectable text registered for Block {id:?}"
            );
        }
        // What a whole-transcript copy would assemble: pieces of one visual
        // row join with nothing, rows join with newlines.
        let mut all = String::new();
        for (_, _, starts_line, text) in &runs {
            if *starts_line && !all.is_empty() {
                all.push('\n');
            }
            all.push_str(text);
        }
        assert!(all.contains("run the tests"), "the prompt line: {all}");
        assert!(all.contains("weighing it up"), "expanded reasoning: {all}");
        assert!(
            all.contains("fn main() {}"),
            "code registers its source: {all}"
        );
        assert!(
            all.contains("Ran `cargo test`"),
            "tool pieces compose the call: {all}"
        );
        // An edit's diff shows only once its call is opened.
        assert_eq!(
            collapsed_and_expanded_count(&runs, "delta"),
            0,
            "the edit's diff waits for its call to open: {all}"
        );
        // A shut failure registers its `Failed` tag, not what it printed
        // (that waits for its call to open, below); the test run says
        // `· 42 passed` after its call line. No `└` elbow joins the run.
        assert!(all.contains("Failed"), "the failure's tag: {all}");
        assert!(
            !all.contains("applied"),
            "a shut call previews nothing: {all}"
        );
        assert!(
            all.contains("Ran `cargo test` \u{b7} 42 passed"),
            "the verdict: {all}"
        );
        assert!(!all.contains("└"), "no elbow: {all}");

        view.update(cx, |view, cx| {
            view.expanded.insert("toolu_2".into());
            cx.notify();
        });
        cx.run_until_parked();
        let expanded = view.read_with(cx, |view, _| view.selection.registered(thread));
        assert_eq!(
            expanded
                .iter()
                .filter(|(_, _, _, text)| text == "delta" || text == "bravo")
                .count(),
            2,
            "the disclosed edit diff registers each line exactly once"
        );
        // A hunk registers its code, never its sign or number columns.
        assert!(
            expanded
                .iter()
                .all(|(_, _, _, text)| text != "+delta" && text != "-bravo"),
            "the diff sign column is chrome and never copies: {expanded:?}"
        );
        assert!(expanded.iter().any(|(_, _, _, text)| text == "applied"));
        assert_eq!(instruments.changed.len(), 1);
        assert_eq!((instruments.added, instruments.removed), (1, 1));
        // The prototype's body draws two glyphs and one elbow, all chrome;
        // the old ❯/⏺/• gutter glyphs are gone entirely.
        for chrome in ['❯', '⏺', '•', '✓', '▸', '●', '└'] {
            assert!(!all.contains(chrome), "{chrome} is chrome: {all}");
        }
        assert!(
            expanded.iter().any(|(_, _, _, text)| text == "delta"),
            "a diff cell is its bare code — no number, no sign: {expanded:?}"
        );
    }

    /// How many registered runs read exactly `text`.
    fn collapsed_and_expanded_count(runs: &[(BlockId, u32, bool, String)], text: &str) -> usize {
        runs.iter().filter(|(_, _, _, run)| run == text).count()
    }

    #[test]
    fn the_wall_reads_turn_completion_without_a_cost() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("finish the task".into()));
        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        }));
        assert_eq!(
            wall_state(Some(&Turn::of(&transcript)), false, false),
            WallState::Done
        );
    }

    /// A break finder by cell count, for the pure tests: the last space
    /// at or before `columns` characters in a line wider than that.
    fn cells(columns: usize) -> impl FnMut(&str) -> Option<usize> {
        move |text: &str| {
            if text.chars().count() <= columns {
                return None;
            }
            text.char_indices()
                .take(columns + 1)
                .filter(|(_, ch)| *ch == ' ')
                .map(|(byte, _)| byte + 1)
                .last()
        }
    }

    fn texts(rows: &[WallRow]) -> Vec<&str> {
        rows.iter().map(|row| row.text.as_ref()).collect()
    }

    /// A narrow Group head gives way in order: the logo, then the word,
    /// then the title's floor — never the control.
    #[test]
    fn a_narrow_head_drops_the_logo_then_the_word() {
        let fit = |width| HeadFit::of(Some(width), true, Some("needs you"), true, true, 96.0);
        assert_eq!(
            fit(400.0),
            HeadFit {
                word: true,
                provider: true,
                floor: 96.0
            }
        );
        let narrower = fit(240.0);
        assert!(narrower.word && !narrower.provider, "{narrower:?}");
        let narrowest = fit(150.0);
        assert!(!narrowest.word && !narrowest.provider, "{narrowest:?}");
        assert!(narrowest.floor < 96.0 && narrowest.floor > 0.0);
        assert_eq!(fit(20.0).floor, 0.0);
    }

    /// F-6: prose wraps once — at its own line break, else where the face
    /// breaks it — its continuation hanging under the first word, and the
    /// tile keeps its last `WALL_LINES` rows.
    #[test]
    fn wall_prose_wraps_once_inside_the_row_budget() {
        let prose = |text: &str| WallLine {
            text: text.to_string().into(),
            wraps: true,
        };
        let rows = wall_rows(
            &[prose(
                "\u{25cf} Drafting the decision section\nfrom the spike notes",
            )],
            cells(80),
        );
        assert_eq!(
            texts(&rows),
            [
                "\u{25cf} Drafting the decision section",
                "from the spike notes"
            ]
        );
        assert_eq!(
            rows[1].hang.as_deref(),
            Some("\u{25cf} "),
            "under the first word"
        );
        assert_eq!(
            texts(&wall_rows(
                &[prose(
                    "\u{25cf} Drafting the decision section from the spike notes"
                )],
                cells(31),
            )),
            [
                "\u{25cf} Drafting the decision section",
                "from the spike notes"
            ]
        );
        // Short enough: one row. A call never wraps.
        assert_eq!(
            texts(&wall_rows(&[prose("\u{25cf} Done")], cells(31))),
            ["\u{25cf} Done"]
        );
        let call = WallLine {
            text: "\u{25cf} Read(spikes/panes24/NOTES.md)".into(),
            wraps: false,
        };
        assert_eq!(wall_rows(std::slice::from_ref(&call), cells(10)).len(), 1);
        // The budget is rows: a wrapped prose line spends two.
        let rows = wall_rows(
            &[
                call.clone(),
                call.clone(),
                prose("\u{25cf} Drafting the decision section from the spike notes"),
            ],
            cells(31),
        );
        assert_eq!(rows.len(), theme::WALL_LINES);
        assert_eq!(rows[2].text.as_ref(), "from the spike notes");
        // Over the budget by one row, the tile never opens on a
        // continuation: the oldest wrapped line keeps its first row alone.
        let mut lines = vec![prose(
            "\u{25cf} Drafting the decision section from the spike notes",
        )];
        lines.extend(std::iter::repeat_n(call, theme::WALL_LINES - 1));
        let rows = wall_rows(&lines, cells(31));
        assert_eq!(rows.len(), theme::WALL_LINES);
        assert_eq!(
            rows[0].text.as_ref(),
            "\u{25cf} Drafting the decision section from the spike notes",
            "whole, to truncate at the edge"
        );
        assert!(rows.iter().all(|row| row.hang.is_none()), "no orphan");
    }

    /// A written file's patch is the whole file, so the card draws a
    /// bounded number of rows and accounts for the rest. Every line is
    /// still counted — the cap is what is drawn, never what is claimed.
    #[test]
    fn a_hunk_card_draws_a_bounded_number_of_rows_and_says_what_it_left() {
        assert_eq!(hunk_rows(0), (0, 0));
        assert_eq!(hunk_rows(4), (4, 0));
        assert_eq!(
            hunk_rows(theme::HUNK_MAX_ROWS),
            (theme::HUNK_MAX_ROWS, 0),
            "a patch that exactly fills the card is not truncated"
        );
        assert_eq!(
            hunk_rows(theme::HUNK_MAX_ROWS + 900),
            (theme::HUNK_MAX_ROWS, 900)
        );
    }

    #[test]
    fn the_mode_chip_uses_provider_supplied_labels() {
        let choices = vec![ferrite_core::PermissionModeChoice {
            value: "opaque-mode".into(),
            label: "Ask for changes".into(),
        }];
        assert_eq!(
            permission_mode_label("opaque-mode", &choices).as_ref(),
            "Ask for changes"
        );
        assert_eq!(
            permission_mode_label("unknown", &choices).as_ref(),
            "Unknown"
        );
    }

    /// C17: a mode id never renders raw — the known ids read as words in
    /// sentence case, the default reads `Manual`, and nothing shown
    /// carries a hump.
    #[test]
    fn the_mode_word_is_never_a_raw_id() {
        let claude = vec![
            ferrite_core::PermissionModeChoice {
                value: "acceptEdits".into(),
                label: "Accept edits".into(),
            },
            ferrite_core::PermissionModeChoice {
                value: "dontAsk".into(),
                label: "Don't ask".into(),
            },
        ];
        assert_eq!(
            permission_mode_label("acceptEdits", &[]).as_ref(),
            "Accept edits"
        );
        assert_eq!(
            permission_mode_label("acceptEdits", &claude).as_ref(),
            "Accept edits"
        );
        assert_eq!(
            permission_mode_label("bypassPermissions", &[]).as_ref(),
            "Bypass permissions"
        );
        assert_eq!(permission_mode_label("plan", &[]).as_ref(), "Plan");
        assert_eq!(permission_mode_label("default", &claude).as_ref(), "Manual");
        assert_eq!(permission_mode_label("", &[]).as_ref(), "Manual");
        assert_eq!(
            permission_mode_label("someNewMode", &[]).as_ref(),
            "Some new mode"
        );
        for id in [
            "acceptEdits",
            "bypassPermissions",
            "plan",
            "dontAsk",
            "someNewMode",
            "on-request",
        ] {
            for choices in [&claude[..], &[]] {
                let word = permission_mode_label(id, choices);
                assert_ne!(word.as_ref(), id, "never the raw id");
                assert!(
                    !word.chars().skip(1).any(char::is_uppercase),
                    "sentence case: {word}"
                );
            }
        }
    }

    /// #22 amendment: durations read in the comps' grammar at every scale.
    /// The transcript's colour (2026-09): each pure helper hands the site
    /// the ink the operator approved, and the state inks stay the palette's.
    #[test]
    fn a_hunk_row_colours_its_code_by_which_way_it_went() {
        assert_eq!(DiffKind::of("+let x = 1;"), DiffKind::Added);
        assert_eq!(DiffKind::of("-let x = 1;"), DiffKind::Removed);
        assert_eq!(DiffKind::of(" let x = 1;"), DiffKind::Context);
        assert_eq!(DiffKind::of(""), DiffKind::Context);
        // Terminal-native: the sign carries the hue; the code keeps body
        // ink under its syntax colours on the row's wash.
        assert_eq!(
            DiffKind::Added.paint(),
            DiffPaint {
                sign: "+",
                sign_color: theme::DIFF_ADDED_SIGN,
                number_color: theme::DIFF_ADDED_NUMBER,
                code_color: SYN_PLAIN,
                wash: Some(DIFF_ADDED_WASH),
            }
        );
        assert_eq!(
            DiffKind::Removed.paint(),
            DiffPaint {
                sign: "-",
                sign_color: theme::DIFF_REMOVED_SIGN,
                number_color: theme::DIFF_REMOVED_NUMBER,
                code_color: SYN_PLAIN,
                wash: Some(DIFF_REMOVED_WASH),
            }
        );
        assert_eq!(
            DiffKind::Context.paint(),
            DiffPaint {
                sign: "",
                sign_color: INK_SUBTLE,
                number_color: TEXT_FAINT,
                code_color: theme::SYN_PLAIN,
                wash: None,
            }
        );
    }

    #[test]
    fn a_fenced_block_paints_every_syntax_class() {
        let tokens = |pairs: &[(&str, Class)]| -> Vec<Token> {
            pairs
                .iter()
                .map(|(text, class)| Token {
                    text: text.to_string(),
                    class: *class,
                })
                .collect()
        };
        let source = "let s = \"hi\"; // 42";
        let runs = code(
            source,
            Some(&tokens(&[
                ("let", Class::Keyword),
                (" s = ", Class::Plain),
                ("\"hi\"", Class::Str),
                ("; ", Class::Plain),
                ("// 42", Class::Comment),
            ])),
        );
        let inks: Vec<(std::ops::Range<usize>, gpui::Hsla, Option<gpui::FontStyle>)> = runs
            .iter()
            .map(|(range, style)| (range.clone(), style.color.unwrap(), style.font_style))
            .collect();
        assert_eq!(
            inks,
            vec![
                (0..3, rgb(SYN_KEYWORD).into(), None),
                (3..8, rgb(theme::SYN_PLAIN).into(), None),
                (8..12, rgb(SYN_STRING).into(), None),
                (12..14, rgb(theme::SYN_PLAIN).into(), None),
                (
                    14..19,
                    rgb(theme::SYN_COMMENT).into(),
                    Some(gpui::FontStyle::Italic)
                ),
            ]
        );
        for (class, ink) in [
            (Class::Number, SYN_NUMBER),
            (Class::Function, theme::SYN_FUNCTION),
            (Class::Type, theme::SYN_TYPE),
            (Class::Punct, theme::SYN_PUNCT),
        ] {
            let runs = code("x", Some(&tokens(&[("x", class)])));
            assert_eq!(runs[0].1.color, Some(rgb(ink).into()), "{class:?}");
        }
        assert!(
            code(
                source,
                Some(&tokens(&[("far too long a token", Class::Plain)]))
            )
            .is_empty(),
            "a highlighter that disagrees with the source is ignored"
        );
        assert!(code(source, None).is_empty());
    }

    #[test]
    fn inline_code_and_links_carry_their_own_ink() {
        let code = span_style(Style::Code).unwrap();
        assert_eq!(code.color, Some(rgb(INLINE_CODE_INK).into()));
        assert_eq!(code.background_color, None, "inline code has no chip");
        // Terminal-native: a link reads cyan like every path, with no
        // underline at rest.
        let link = span_style(Style::Link).unwrap();
        assert_eq!(link.color, Some(rgb(theme::LINK_INK).into()));
        assert_eq!(link.underline, None);
        assert!(span_style(Style::Plain).is_none());
    }

    #[test]
    fn a_call_line_reads_in_the_apps_words() {
        let tool = |name: &str, summary: &str, state: ToolState| ToolBlock {
            caption: None,
            background: None,
            image: false,
            call: "c".into(),
            name: name.into(),
            title: None,
            summary: summary.into(),
            state,
            diffs: Vec::new(),
            structured_preview: None,
            result_line: None,
            output: None,
            progress: None,
        };
        let failed = ToolState::Failed("boom".into());
        // The verb in the app's words, the argument as written, the result
        // after its seam.
        for (name, verb) in [
            ("Bash", "Ran "),
            ("Read", "Read "),
            ("Edit", "Edited "),
            ("Write", "Wrote "),
            ("Grep", "Searched for "),
            ("Task", "Ran agent "),
        ] {
            let call = tool(name, "cargo-test", ToolState::Ok);
            assert_eq!(text::call_line(&call).verb, verb, "{name}");
        }
        let running = tool("Bash", "cargo test", ToolState::Running);
        assert_eq!(
            text::call_line(&running),
            text::CallLine {
                verb: "Running ".into(),
                argument: "cargo test".into(),
                literal: true,
                result: None,
            }
        );
        // A failure is the row's tag, not a preview after a seam.
        let make = tool("Bash", "make", failed);
        assert_eq!(text::call_line(&make).result, None);
        assert_eq!(text::call_verdict(&make).as_deref(), Some("Failed"));
        assert_eq!(
            text::call_verdict(&tool("Bash", "make", ToolState::Ok)),
            None
        );
        // A shut row previews nothing a command printed.
        let printed = ToolBlock {
            caption: None,
            background: None,
            image: false,
            result_line: Some("16:HOLD={".into()),
            ..tool("Bash", "python3 prep.py --hold", ToolState::Ok)
        };
        assert_eq!(text::call_line(&printed).result, None);
        // Where the command ran is not the row's business.
        assert_eq!(
            text::call_line(&tool(
                "Bash",
                "cd outputs/x && python3 match.py",
                ToolState::Ok
            ))
            .argument,
            "python3 match.py"
        );
        // The provider's description leads, as words; the command waits
        // behind the disclosure.
        let described = ToolBlock {
            caption: None,
            background: None,
            image: false,
            title: Some("List a directory that doesn't exist".into()),
            ..tool("Bash", "ls /definitely-missing-dir", ToolState::Ok)
        };
        assert_eq!(
            text::call_line(&described),
            text::CallLine {
                verb: String::new(),
                argument: "List a directory that doesn't exist".into(),
                literal: false,
                result: None,
            }
        );
        // One Markdown run that reads exactly as written: words escaped, a
        // literal argument one code span, a path a file link.
        let line = text::CallLine {
            verb: "Ran ".into(),
            argument: "echo `a` *b*".into(),
            literal: true,
            result: Some(" \u{b7} [ok]".into()),
        };
        assert_eq!(line.markdown(false), "Ran ``echo `a` *b*`` \u{b7} \\[ok\\]");
        let read = text::CallLine {
            verb: "Read ".into(),
            argument: "src/a_b.rs".into(),
            literal: true,
            result: None,
        };
        assert_eq!(
            read.markdown(true),
            "Read [src\\/a\\_b\\.rs](<ferrite-path:src/a_b.rs>)"
        );
        // Verdict words in a result take their state ink, whole words only.
        let runs = result_highlights("test nav::a ... ok\ntest nav::b ... FAILED\nbook");
        let inks: Vec<_> = runs
            .iter()
            .map(|(range, style)| (range.clone(), style.color))
            .collect();
        assert_eq!(
            inks,
            vec![
                (16..18, Some(rgb(RUNNING).into())),
                (35..41, Some(rgb(BLOCKED).into())),
            ]
        );
    }

    #[test]
    fn an_open_command_says_how_it_failed_and_nothing_on_success() {
        let tool = |state: ToolState, output: Option<&str>| ToolBlock {
            caption: None,
            background: None,
            image: false,
            call: "c".into(),
            name: "Bash".into(),
            title: None,
            summary: "bash -lc 'ls /missing'".into(),
            state,
            diffs: Vec::new(),
            structured_preview: None,
            result_line: None,
            output: output.map(|text| ferrite_core::transcript::ToolOutput {
                text: text.into(),
                omitted_bytes: 0,
            }),
            progress: None,
        };
        // Claude's `Exit code N` line becomes the status; the rest is the
        // output in the output's ink, the command out of its wrapper.
        let failed = tool(
            ToolState::Failed("Exit code 1\nls: /missing: No such file or directory".into()),
            None,
        );
        let shell = output::shell(&failed);
        assert_eq!(shell.command, "ls /missing");
        assert_eq!(
            (shell.output, shell.exit, shell.failure),
            (
                Some("ls: /missing: No such file or directory"),
                Some(1),
                false
            )
        );
        // No code said: no status; the failure's own message reads in the
        // failure ink (claude/tools-expanded.png's `Blocked: …`).
        let bare = tool(ToolState::Failed("boom".into()), None);
        let bare = output::shell(&bare);
        assert_eq!(
            (bare.output, bare.exit, bare.failure),
            (Some("boom"), None, true)
        );
        // A success says nothing (Claude's), its output plain.
        let ok = tool(ToolState::Ok, Some("a\nb"));
        let ok = output::shell(&ok);
        assert_eq!(
            (ok.output, ok.exit, ok.failure),
            (Some("a\nb"), None, false)
        );
        let running = tool(ToolState::Running, None);
        let running = output::shell(&running);
        assert_eq!((running.output, running.exit), (None, None));
    }

    #[test]
    fn a_diff_pairs_its_changes_and_marks_the_words_that_changed() {
        assert_eq!(text::diff_body("+    let x = 1;"), "    let x = 1;");
        assert_eq!(text::diff_body("-\tfoo"), "\tfoo");
        assert_eq!(text::diff_body("  indented context"), " indented context");
        assert_eq!(
            text::diff_body("\\ No newline at end of file"),
            "\\ No newline at end of file"
        );
        assert_eq!(text::diff_body(""), "");
        assert_eq!(DiffKind::Removed.paint().sign, "-", "ASCII, like both CLIs");
        // The number column is `DIFF_NUMBER_CELLS` until a number outgrows it.
        assert_eq!(diff_number_cells(7), theme::DIFF_NUMBER_CELLS);
        assert_eq!(diff_number_cells(999), theme::DIFF_NUMBER_CELLS);
        assert_eq!(diff_number_cells(9_999), 5.5);
        // Word marks: the tokens left once the common leading and trailing
        // tokens go — whole identifiers, never a piece of one.
        assert_eq!(
            changed_words("let row = h(ROW_H);", "let row = h(ROW_LIVE_H);"),
            Some((12..17, 12..22))
        );
        assert_eq!(
            changed_words(
                "pub const RUNNING: u32 = 0x7fbf95;",
                "pub const RUNNING: u32 = 0x93cf8c;"
            ),
            Some((25..33, 25..33)),
            "a hex literal is one token, `0x` included"
        );
        // Past a common `if l`, the old side keeps its whole tokens (from
        // `Some`) and the new side, with none whole, its touched `live`.
        let old = "if let Some(summary) = facts.and_then(|f| f.summary.clone()) {";
        let (was, became) = changed_words(old, "if live {").expect("a pair");
        assert_eq!(
            &old[was],
            "Some(summary) = facts.and_then(|f| f.summary.clone())"
        );
        assert_eq!(&"if live {"[became], "live");
        assert_eq!(changed_words("abc", "xyz"), None, "the whole line changed");
        assert_eq!(changed_words("same", "same"), None);
        let diff = Diff {
            path: "src/nav.rs".into(),
            hunks: vec![ferrite_core::Hunk {
                old_start: 10,
                old_lines: 3,
                new_start: 10,
                new_lines: 3,
                lines: vec![
                    " fn a() {".into(),
                    "-    old();".into(),
                    "+    new();".into(),
                    "+    more();".into(),
                    " }".into(),
                ],
                section: None,
            }],
            added: 2,
            removed: 1,
        };
        let rows = diff_rows(&diff, 24);
        assert_eq!(rows.len(), 5, "a hunk naming no section draws no header");
        let mut named = diff.clone();
        named.hunks[0].section = Some("fn a".into());
        assert_eq!(
            diff_rows(&named, 24)[0],
            DiffRow::Hunk("@@ -10,3 +10,3 @@ fn a".into())
        );
        let side = |at: usize| match &rows[at] {
            DiffRow::Line(side) => side.clone(),
            row => panic!("a line: {row:?}"),
        };
        // The removed line, then the added lines it became, its changed
        // tokens marked against its likeliest partner.
        assert_eq!(side(1).body, "    old();");
        assert_eq!(side(1).words, Some(4..7));
        assert_eq!(side(2).body, "    new();");
        assert_eq!((side(3).body.as_str(), side(3).number), ("    more();", 12));
    }

    #[test]
    fn an_image_header_names_its_size() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&580u32.to_be_bytes());
        png.extend_from_slice(&320u32.to_be_bytes());
        assert_eq!(image_size(&png), Some((580, 320)));
        let gif = b"GIF89a\x40\x01\xf0\x00";
        assert_eq!(image_size(gif), Some((320, 240)));
        assert_eq!(image_size(b"not an image"), None);
    }

    #[test]
    fn output_past_the_byte_cap_scrolls_in_its_viewport() {
        let lines = |n: usize| {
            (0..n)
                .map(|i| format!("line {i}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        // Line count alone keeps output inline, where a transcript copy
        // sweep reaches it; only the byte cap moves it to the viewport.
        assert!(!text::output_scrolls(&lines(40)));
        assert_eq!(text::output_lines(&lines(40)), 40);
        assert!(text::output_scrolls(
            &"x".repeat(theme::OUTPUT_INLINE_BYTES + 1)
        ));
        assert_eq!(text::byte_size(512), "512 B");
        assert_eq!(text::byte_size(1_229), "1 KB");
        assert_eq!(text::byte_size(41 * 1024 + 300), "41 KB", "whole kilobytes");
        assert_eq!(text::byte_size(3 * 1024 * 1024), "3.0 MB");
    }

    /// A turn's end leads with the lexicon's own lowercase words; a
    /// finished one says how long it worked once that was a while, and
    /// nothing when it was quick.
    #[test]
    fn turn_ends_speak_the_lexicon() {
        use ferrite_core::transcript::TurnEnd;
        assert_eq!(TurnEnd::INTERRUPTED, theme::words::INTERRUPTED);
        assert_eq!(TurnEnd::FAILED, theme::words::FAILED);
        let end = |outcome, elapsed_ms| TurnEnd {
            outcome,
            elapsed_ms: Some(elapsed_ms),
            completed_at: Some("7:10 pm".into()),
            input_tokens: Some(1_260_000),
            output_tokens: Some(3_700),
        };
        use ferrite_core::TurnOutcome::*;
        assert_eq!(turn_stamp(&end(Completed, 100)), None);
        assert_eq!(
            turn_stamp(&end(Completed, 322_000)).as_deref(),
            Some("Worked for 5m 22s")
        );
        assert_eq!(
            turn_stamp(&end(Interrupted, 100)).as_deref(),
            Some("You stopped after 0.1s")
        );
        assert_eq!(
            turn_stamp(&end(Error("API Error: 529 overloaded".into()), 100)).as_deref(),
            Some("failed \u{b7} 0.1s \u{b7} API Error: 529 overloaded")
        );
    }

    #[test]
    fn durations_read_at_the_comps_grammar() {
        assert_eq!(
            ferrite_core::progress::duration_label(Duration::from_millis(340)),
            "0.3s"
        );
        assert_eq!(
            ferrite_core::progress::duration_label(Duration::from_millis(8_200)),
            "8.2s"
        );
        assert_eq!(
            ferrite_core::progress::duration_label(Duration::from_secs(42)),
            "42s"
        );
        assert_eq!(
            ferrite_core::progress::duration_label(Duration::from_secs(134)),
            "2m14s"
        );
    }

    #[test]
    fn rate_limit_resets_read_as_wall_times_inside_their_window() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        let after = |seconds: u64| Some(1_000_000 + seconds);
        let week = Duration::from_secs(7 * 86_400);

        let label = |at| reset_label(at, week, now).map(|label| label.to_string());
        assert_eq!(label(None), None, "an unreported reset reads nothing");
        assert_eq!(label(after(0)), None, "a reset due now is already past");
        let at = UNIX_EPOCH + Duration::from_secs(1_000_000 + 4 * 86_400);
        assert_eq!(
            label(after(4 * 86_400)),
            Some(format!(
                "Resets {}",
                ferrite_core::clock::day_label(at, now)
            ))
        );
        assert_eq!(
            label(Some(999_999)),
            None,
            "an elapsed or relative provider timestamp must not underflow"
        );
        assert_eq!(
            label(after(8 * 86_400)),
            None,
            "a value outside the window span is not guessed to be Unix seconds"
        );
    }

    /// The card's numbers are only as precise as they need to be.
    #[test]
    fn card_counts_read_compactly() {
        assert_eq!(compact_count(640), "640");
        assert_eq!(compact_count(1_500), "1.5k");
        assert_eq!(compact_count(2_000), "2k");
        assert_eq!(compact_count(64_000), "64k");
        assert_eq!(compact_count(124_400), "124k");
        assert_eq!(compact_count(200_000), "200k");
        assert_eq!(compact_count(1_000_000), "1M");
        assert_eq!(compact_count(1_200_000), "1.2M");
    }

    /// F-2: one attention edge. A waiting Pane draws `WAIT_RING` on a
    /// board whether it holds focus or not — no inner ring, no full-ink
    /// answer target, no red blocked edge; a calm focused Pane draws the
    /// focus ring; at rest the card's own ring. Solo draws none: it is the
    /// plane itself.
    #[test]
    fn pane_edge_ranks_state_over_focus() {
        assert_eq!(PaneEdge::of(true, true, false), PaneEdge::Attention);
        assert_eq!(PaneEdge::of(false, true, false), PaneEdge::Attention);
        assert_eq!(PaneEdge::of(true, false, false), PaneEdge::Focused);
        assert_eq!(PaneEdge::of(false, false, false), PaneEdge::Rest);
        assert_eq!(PaneEdge::Rest.ink(), theme::paint::CARD_RING.into());
        assert_eq!(PaneEdge::Focused.ink(), rgba(FOCUS_RING).into());
        assert_eq!(PaneEdge::Attention.ink(), rgba(WAIT_RING).into());
        for focused in [false, true] {
            for attention in [false, true] {
                let edge = PaneEdge::of(focused, attention, true);
                assert_eq!(edge, PaneEdge::Flush, "solo {focused} {attention}");
            }
        }
    }

    /// A run's state word is coloured only when it failed; the mark beside
    /// it carries the rest, in the one check vocabulary: green only while a
    /// run is going, a pass dim, never the ochre that means an agent waits.
    #[test]
    fn check_rows_colour_only_failure() {
        use components::CheckMark;
        for (state, detail, mark) in [
            (CheckState::Failing, "failure", CheckMark::Failed),
            (CheckState::Pending, "in_progress", CheckMark::Running),
            (CheckState::Pending, "queued", CheckMark::Queued),
            (CheckState::Passing, "success", CheckMark::Passed),
            (CheckState::Skipped, "skipped", CheckMark::Skipped),
        ] {
            assert_eq!(check_mark(state, detail), mark, "{state:?} {detail}");
        }
        assert_eq!(CheckMark::Failed.word_ink(), BLOCKED);
        assert_eq!(CheckMark::Running.ink(), RUNNING);
        assert_eq!(CheckMark::Passed.ink(), INK_SUBTLE);
        for mark in [
            CheckMark::Running,
            CheckMark::Queued,
            CheckMark::Passed,
            CheckMark::Skipped,
        ] {
            assert_eq!(mark.word_ink(), INK_SUBTLE, "{mark:?}");
            assert_ne!(mark.ink(), ATTENTION, "{mark:?}");
        }
        // A run's word is the lexicon's, lowercase, whatever the forge said.
        for (state, detail, word) in [
            (CheckState::Failing, "failure", "failed"),
            (CheckState::Failing, "timed_out", "failed"),
            (CheckState::Pending, "in_progress", "running"),
            (CheckState::Pending, "queued", "queued"),
            (CheckState::Pending, "waiting", "queued"),
            (CheckState::Passing, "success", "passed"),
            (CheckState::Skipped, "skipped", "skipped"),
            (CheckState::Skipped, "neutral", "skipped"),
            (CheckState::Failing, "cancelled", "cancelled"),
        ] {
            assert_eq!(check_word(state, detail), word, "{state:?} {detail}");
        }
    }

    /// Every word the wall and the Group head's slot can say is the
    /// lexicon's (`theme::words`), lowercase — `failing 2`, never `2
    /// failing`; `failed`, never a raw reason; idle says nothing.
    #[test]
    fn every_wall_and_slot_word_is_the_lexicons_and_lowercase() {
        use theme::words;
        let lexicon = [
            words::NEEDS_YOU,
            words::APPROVAL,
            words::QUESTION,
            words::DONE,
            words::FAILED,
            words::FAILING,
            words::INTERRUPTED,
            words::WORKING,
            words::PARKED,
        ];
        let failing = WallCard {
            failing_count: Some(2),
            ..Default::default()
        };
        let mut said = Vec::new();
        for state in [
            WallState::Working,
            WallState::Failing,
            WallState::Decision,
            WallState::Blocked,
            WallState::Done,
            WallState::Idle,
            WallState::Parked,
        ] {
            for kind in [None, Some(words::APPROVAL), Some(words::QUESTION)] {
                for unread in [false, true] {
                    if let Some(word) = state_word(state, kind, &failing, None, unread) {
                        said.push(word.text());
                    }
                }
            }
        }
        assert_eq!(
            state_word(WallState::Idle, None, &failing, None, false),
            None,
            "idle says nothing"
        );
        assert_eq!(
            state_word(WallState::Done, None, &failing, None, false),
            None,
            "a finish already seen says nothing"
        );
        assert_eq!(
            state_word(WallState::Done, None, &failing, None, true),
            Some(HeadSlot::Done),
            "a finish not yet seen is done"
        );
        assert!(said.contains(&"failing 2".to_string()));
        assert!(said.contains(&"needs you \u{b7} approval".to_string()));
        assert!(said.contains(&"needs you \u{b7} question".to_string()));
        assert!(said.contains(&"failed".to_string()));
        assert!(said.contains(&"parked".to_string()));
        for text in said {
            assert_eq!(text, text.to_lowercase(), "{text}");
            let words_only: String = text
                .split(|c: char| c.is_ascii_digit() || c == '\u{b7}')
                .collect::<Vec<_>>()
                .join(" ");
            for part in words_only
                .split("  ")
                .map(str::trim)
                .filter(|part| !part.is_empty())
            {
                assert!(
                    lexicon
                        .iter()
                        .any(|word| part == *word || part.starts_with(word)),
                    "`{part}` of `{text}` is not in theme::words"
                );
            }
        }
        assert_eq!(HeadSlot::Working("12s".into()).text(), "working 12s");
    }

    /// #11: import is offered exactly while a Thread has no conversation —
    /// at rest, with nothing in its transcript but Ferrite's own notices
    /// and bookkeeping. The first prompt retires the offer; a refused pick
    /// (a Notice) does not.
    #[test]
    fn import_is_offered_only_while_the_thread_has_no_conversation() {
        assert!(!offers_import(None), "a parked Pane offers nothing");

        let mut fresh = Transcript::default();
        assert!(offers_import(Some(&fresh)));
        fresh.apply(Input::Notice("cannot import x: not a session file".into()));
        fresh.apply(Input::Revived);
        assert!(
            offers_import(Some(&fresh)),
            "Ferrite's own out-of-band lines keep the door open"
        );
        fresh.apply(Input::Prompt("hello".into()));
        assert!(
            !offers_import(Some(&fresh)),
            "the first prompt is a conversation"
        );

        let mut streaming = Transcript::default();
        streaming.apply(Input::Event(SessionEvent::TextDelta { text: "x".into() }));
        assert!(!offers_import(Some(&streaming)), "not at rest");
    }

    /// The queue strip counts what waits, Claude's words.
    #[test]
    fn the_queue_strip_counts_its_messages() {
        assert_eq!(queued_count(1), "1 message queued");
        assert_eq!(queued_count(3), "3 messages queued");
    }

    /// The footer ring reads the fullest account window, and nothing when
    /// the provider reports none.
    #[test]
    fn the_usage_ring_reads_the_fullest_plan_window() {
        use ferrite_core::transcript::RateLimits;
        use ferrite_core::RateLimitWindow;
        let window = |used_fraction| RateLimitWindow {
            used_fraction,
            resets_at: None,
        };
        assert_eq!(plan_usage_fraction(RateLimits::default()), None);
        assert_eq!(
            plan_usage_fraction(RateLimits {
                five_hour: Some(window(0.2)),
                weekly: Some(window(0.6)),
            }),
            Some(0.6)
        );
        assert_eq!(
            plan_usage_fraction(RateLimits {
                five_hour: Some(window(1.4)),
                weekly: None,
            }),
            Some(1.0),
            "clamped"
        );
    }

    /// Claude's three placeholders: the first prompt (a draft's, an empty
    /// Thread's), a running turn's keys, and else the way to the commands —
    /// a Decision and a closed Session included, since the line still
    /// takes typing. Once a prediction lands, it shows verbatim as the
    /// operator's own next line. None ends in an ellipsis.
    #[test]
    fn the_placeholder_says_what_the_pane_is_waiting_on() {
        let empty = Transcript::default();
        assert_eq!(
            placeholder(false, false, Some(&empty), None).text(),
            "Describe a task or ask a question",
            "an empty Thread asks for its first prompt, as a draft does"
        );
        let mut live = Transcript::default();
        live.apply(Input::Prompt("fix the decoder".into()));
        live.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        }));
        assert_eq!(
            placeholder(false, false, Some(&live), None).text(),
            "Type / for commands"
        );
        assert_eq!(
            placeholder(true, false, Some(&live), None).text(),
            "Type / for commands"
        );
        let mut running = Transcript::default();
        running.apply(Input::Prompt("fix the decoder".into()));
        running.apply(Input::Event(SessionEvent::TextDelta {
            text: "Looking".into(),
        }));
        assert_eq!(
            placeholder(false, false, Some(&running), None).text(),
            "\u{21b5} to steer this turn"
        );

        let mut closed = Transcript::default();
        closed.apply(Input::Prompt("fix the decoder".into()));
        closed.apply(Input::Event(SessionEvent::Closed {
            reason: "the CLI exited".into(),
        }));
        assert_eq!(
            placeholder(false, false, Some(&closed), None).text(),
            "Type / for commands"
        );
        assert_eq!(
            placeholder(false, true, None, None).text(),
            "Describe a task or ask a question"
        );

        // A landed prediction is the line, verbatim and unadorned — it is a
        // draft of the next prompt, not a description of one.
        let mut answered = Transcript::default();
        answered.apply(Input::Prompt("fix the decoder".into()));
        answered.apply(Input::Event(SessionEvent::TextDelta {
            text: "Fixed it.".into(),
        }));
        answered.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        }));
        assert_eq!(
            placeholder(false, false, Some(&answered), Some("Run the tests")).head,
            "Run the tests"
        );
        // A Decision and a dead Session both outrank it.
        assert_eq!(
            placeholder(true, false, Some(&answered), Some("Run the tests")).text(),
            "Type / for commands"
        );
        assert_eq!(
            placeholder(false, false, Some(&closed), Some("Run the tests")).text(),
            "Type / for commands"
        );

        // No line names a message or ends in an ellipsis.
        for ghost in [
            placeholder(false, false, Some(&live), None),
            placeholder(true, false, Some(&live), None),
            placeholder(false, false, Some(&running), None),
            placeholder(false, false, Some(&closed), None),
            placeholder(false, true, None, None),
        ] {
            let text = ghost.text();
            assert!(!text.contains("message"), "{text}");
            assert!(!text.ends_with('\u{2026}'), "{text}");
        }
    }

    // ---- WP-A tests (append above the end line)
    // (end WP-A)

    // ---- WP-B tests (append above the end line)
    // (end WP-B)

    // ---- WP-C tests (append above the end line)
    // (end WP-C)

    // ---- WP-D tests (append above the end line)
    /// The height budget counts exactly what the Composer draws around its
    /// editor rows, at every level, focused or not: Solo's box — the
    /// input's 14 + 8, the 42px toolbar, the 14px inset — and a board's
    /// compact box — 9 above and below its one 20px row, 10 to the foot.
    #[test]
    fn the_fixed_height_follows_the_composer_tokens() {
        // Solo: the row's 7 + 5 of air above and below one line (a 44 box),
        // then the 41px footer down to the card's foot.
        assert_eq!(composer_fixed_height(false), 2. * (7. + 5.) + 41.);
        assert_eq!(composer_fixed_height(true), 9. + 9. + 10.);
        assert_eq!(
            theme::COMPACT_COMPOSER_H,
            composer_fixed_height(true) - theme::COMPACT_COMPOSER_INSET + theme::COMPOSER_ROW_H
        );
        for grid in [false, true] {
            // A tall Pane shows every editor row; a short one keeps its
            // majority.
            assert_eq!(composer_row_limit(900., grid, 0), crate::composer::MAX_ROWS);
            let limit = composer_row_limit(300., grid, 0) as f32;
            assert!(
                composer_fixed_height(grid) + limit * theme::COMPOSER_ROW_H
                    <= 300. * theme::COMPOSER_MAX_PANE_FRACTION
            );
        }
        // Solo's queue is a 44px row per message and its 8 gap; the
        // compact box counts none (its Pane says `2 queued` unfocused).
        assert_eq!(composer_queue_height(900., false, 0), 0.);
        // A row per message, up to the rows it shows; more scroll.
        assert_eq!(
            composer_queue_height(900., false, 1),
            theme::QUEUE_CARD_H + theme::QUEUE_CARD_GAP
        );
        assert_eq!(
            composer_queue_height(900., false, 2),
            2. * theme::QUEUE_CARD_H + theme::QUEUE_CARD_GAP
        );
        assert_eq!(
            composer_queue_height(900., false, 9),
            theme::QUEUE_ROWS_SHOWN as f32 * theme::QUEUE_CARD_H + theme::QUEUE_CARD_GAP
        );
        assert_eq!(composer_queue_height(900., true, 3), 0.);
    }

    /// The ctx meter fills green, then yellow, then red.
    #[test]
    fn the_ctx_meter_fills_by_state() {
        assert_eq!(ctx_meter_ink(0.32), RUNNING);
        assert_eq!(ctx_meter_ink(0.61), ATTENTION);
        assert_eq!(ctx_meter_ink(0.90), BLOCKED);
    }

    /// Usage is neutral until it runs tight: colour is state.
    #[test]
    fn usage_reads_neutral_until_it_runs_tight() {
        assert_eq!(usage_ink(0.62), INK_STRONG);
        assert_eq!(usage_ink(0.79), INK_STRONG);
        assert_eq!(usage_ink(theme::USAGE_TIGHT), ATTENTION);
        assert_eq!(usage_ink(0.9), ATTENTION);
        assert_eq!(usage_ink(0.95), ATTENTION);
        assert_eq!(usage_ink(1.0), ATTENTION);
        assert_eq!(theme::USAGE_TIGHT, 0.80);
    }

    /// Past the warning the foot's meter turns the state's ink, while the
    /// category segments the Context tab draws keep the inks its legend
    /// shows.
    #[test]
    fn past_the_warning_only_the_foot_recolours() {
        let details = ferrite_core::ContextDetails {
            categories: vec![
                ferrite_core::ContextCategory {
                    name: "System prompt".into(),
                    tokens: 30_000,
                },
                ferrite_core::ContextCategory {
                    name: "Messages".into(),
                    tokens: 100_000,
                },
            ],
            ..Default::default()
        };
        let foot = context_meter_segments(130_000, 200_000, Some(&details));
        assert!(foot.iter().all(|(ink, _)| *ink == ctx_meter_ink(0.65)));
        let tab = context_segments(&details.categories, 200_000);
        assert_eq!(tab[0].0, category_ink("System prompt"));
        assert_eq!(tab[1].0, category_ink("Messages"));
    }

    // (end WP-D)

    // ---- WP-E tests (append above the end line)
    // (end WP-E)

    // ---- WP-F tests (append above the end line)
    // (end WP-F)

    // ---- WP-G tests (append above the end line)
    // (end WP-G)
}
