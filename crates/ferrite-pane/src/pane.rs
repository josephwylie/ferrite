//! One Pane: the visible cell for one Thread. Header, transcript, Composer,
//! and the three semantic-zoom renderings. Rendering only — everything it
//! shows is folded in core, and every key it answers to belongs to the
//! cockpit above it.
//!
//! A Pane is a card on the frame (theme WP-C, SPEC §5.3): `paint::CARD` at
//! `R_PANE`; its content starts at the card's own edges, and its 1px ring
//! is an overlay just outside them (`card_ring`) that only changes colour —
//! `CARD_RING` at rest, `FOCUS_RING` when focused on a board, `WAIT_RING`
//! while it waits (a focused waiting Pane shows only the yellow; its focus
//! reads in the head ground and title). On a board it wears one head
//! row — the dot (a working Thread's braille spinner), the title, the
//! provider mark — and no state word; top to bottom then the subagent
//! strip, the transcript body (its Decision is the transcript's tail), the
//! Composer. Two altitudes (R12): the transcript, or the wall tile — the
//! state word in colour, the last lines dim, the quick answers.

mod compact_composer;
mod text;
pub(crate) use text::{collect_block_text, collect_group_text, group_live};

use ferrite_core::activity::Subject;
use ferrite_core::cockpit::{ThreadView, ToolTiming};
pub(crate) use ferrite_core::digest::BULLET;
use ferrite_core::digest::ELBOW;
pub use ferrite_core::digest::{Turn, WallCard, WallLine};
use ferrite_core::docview::{is_test_run, passed_count, Level};
use ferrite_core::followup::{self, Followup};
use ferrite_core::progress::Phase;
use ferrite_core::roster::{DraftId, PaneIdentity};
use ferrite_core::store::Provider;
use ferrite_core::transcript::{
    Block, BlockId, Body, Diff, Span, Status, Style, Todos, Token, ToolBlock, ToolState, Transcript,
};
use ferrite_core::workspace::{Check, CheckState, PullRequest, WorkspaceBinding};
use ferrite_core::{Decision, ThreadId};
use gpui::prelude::*;
use gpui::{
    deferred, div, px, relative, rgb, rgba, AnyElement, Context, Div, Entity, FocusHandle,
    HighlightStyle, SharedString, Stateful, Styled, StyledText,
};
#[cfg(test)]
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
#[cfg(test)]
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::components;
use crate::components::Tip as _;
use crate::composer::Composer;
use crate::decision;
use crate::icons::{self, icon};
use crate::pointer::{Pointer, PointerPressed};
use crate::select::TextRuns;
use gpui::component::scroll::ScrollableElement;
// Every color and metric here is a Soft token (crate::theme) — no literal
// survives in render code, which is #22's grep-able law.
use crate::theme;
use crate::theme::*;

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
    pub agent_menu_open: bool,
    pub subject_strip_width: f32,
    pub tab_interaction: crate::cockpit::subagents::TabInteraction,
    pub history_error: Option<String>,
    pub request_forms: crate::cockpit::subagents::RequestForms,
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
    /// What the working line reads besides its transcript, as of this
    /// Pane's last render (`working_line`).
    pub(crate) working: std::cell::Cell<WorkingFacts>,
    /// What the Panel keeps of this Pane's preview (`cockpit/panel/
    /// preview.rs`): its scroll, its find bar, whether its tab is the one
    /// shown. Kept on the Pane, so a draft's first send — which makes the
    /// Pane a Thread's — keeps it as it was.
    pub preview_tab: crate::cockpit::panel::preview::PreviewTab,
    /// The Review beside this Pane on the board (`cockpit/beside.rs`).
    pub review: crate::cockpit::beside::ReviewSlot,
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
/// switch: a call's details (or its output fold), a long thought, a later
/// edit's folded diff (`+ show diff`), and a group of calls, named by its
/// first call.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DisclosureId {
    Tool(String),
    Reasoning(BlockId),
    Diff(String),
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
    #[cfg(test)]
    bounds: Rc<RefCell<HashMap<DisclosureId, gpui::Bounds<gpui::Pixels>>>>,
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

/// The band's four chips, in tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandChip {
    Provider,
    Effort,
    Project,
    Workspace,
}

impl BandChip {
    /// Where tab goes next: across the band, then back to the prompt.
    pub fn next(current: Option<BandChip>) -> Option<BandChip> {
        match current {
            None => Some(BandChip::Provider),
            Some(BandChip::Provider) => Some(BandChip::Effort),
            Some(BandChip::Effort) => Some(BandChip::Project),
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
            agent_menu_open: false,
            subject_strip_width: 0.,
            tab_interaction: Default::default(),
            history_error: None,
            request_forms: Default::default(),
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
                #[cfg(test)]
                bounds: Rc::new(RefCell::new(HashMap::new())),
            },
            disclosure_revision: 0,
            geometry: std::rc::Rc::default(),
            working: std::cell::Cell::default(),
            preview_tab: Default::default(),
            review: crate::cockpit::beside::ReviewSlot::new(cx),
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
            agent_menu_open: false,
            subject_strip_width: 0.,
            tab_interaction: Default::default(),
            history_error: None,
            request_forms: Default::default(),
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
                #[cfg(test)]
                bounds: Rc::new(RefCell::new(HashMap::new())),
            },
            disclosure_revision: 0,
            geometry: std::rc::Rc::default(),
            working: std::cell::Cell::default(),
            preview_tab: Default::default(),
            review: crate::cockpit::beside::ReviewSlot::new(cx),
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
                    #[cfg(test)]
                    bounds: Rc::new(RefCell::new(HashMap::new())),
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
        self.agent_menu_open = false;
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

    pub(crate) fn toggle_tool(&mut self, call: &DisclosureId) {
        if !self.disclosure.expanded.remove(call) {
            self.disclosure.expanded.insert(call.clone());
        }
        // Toggling never moves the keyboard target: Tab/Shift-Tab set it,
        // and a pointer click only opens or closes the row.
        self.disclosure_revision = self.disclosure_revision.wrapping_add(1);
    }

    #[cfg(test)]
    pub(crate) fn tool_state(&self, call: impl Into<DisclosureId>) -> DisclosureState {
        if self.disclosure.expanded.contains(&call.into()) {
            DisclosureState::Expanded
        } else {
            DisclosureState::Collapsed
        }
    }

    #[cfg(test)]
    pub(crate) fn tool_targeted(&self, call: impl Into<DisclosureId>) -> bool {
        self.disclosure.target.as_ref() == Some(&call.into())
    }

    pub(crate) fn has_tool_target(&self) -> bool {
        self.disclosure.target.is_some()
    }

    pub(crate) fn targeted_tool(&self) -> Option<&DisclosureId> {
        self.disclosure.target.as_ref()
    }

    pub(crate) fn tool_focus(&self) -> FocusHandle {
        self.disclosure.focus.clone()
    }

    pub(crate) fn cycle_tools(
        &mut self,
        calls: &[DisclosureId],
        reverse: bool,
    ) -> Option<&DisclosureId> {
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

    pub(crate) fn prune_tools(&mut self, calls: &HashSet<DisclosureId>) {
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

    pub(crate) fn clear_tool_target(&mut self) {
        if self.disclosure.target.take().is_some() {
            self.disclosure_revision = self.disclosure_revision.wrapping_add(1);
        }
    }

    pub(crate) fn transcript_disclosure_snapshot(
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

    pub(crate) fn disclosure_revision(&self) -> u64 {
        self.disclosure_revision
    }

    #[cfg(test)]
    pub(crate) fn tool_expanded(&self, call: impl Into<DisclosureId>) -> bool {
        self.tool_state(call) == DisclosureState::Expanded
    }

    #[cfg(test)]
    pub(crate) fn tool_bounds(
        &self,
        call: impl Into<DisclosureId>,
    ) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.disclosure.bounds.borrow().get(&call.into()).copied()
    }

    #[cfg(test)]
    pub(crate) fn tool_bounds_sink(
        &self,
    ) -> Rc<RefCell<HashMap<DisclosureId, gpui::Bounds<gpui::Pixels>>>> {
        self.disclosure.bounds.clone()
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
    /// wraps against (whole cells of the one face).
    pub cell_width: f32,
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
    /// The Session's running background tasks as chips at the Composer's
    /// right edge, each wired to its stop control — the other half of the
    /// shelf the attachment island sits on. None while nothing runs in the
    /// background, for a Subagent Subject, and at the wall.
    pub background: Option<AnyElement>,
    /// The retained transcript reports whether its received-reasoning row is
    /// mounted; this keeps the pinned live progress caption singular.
    pub received_reasoning_visible: bool,
    /// The open `/` or `@` popover for this Pane's Composer, rows wired to
    /// their picks in the cockpit and hung above the input line here (#23).
    pub menu: Option<AnyElement>,
    /// The head's title cell, wired: the name with a double-click that
    /// opens the rename editor, or the editor itself while renaming. None
    /// draws the plain name (L2, L3, drafts).
    pub title: Option<AnyElement>,
    /// The tab strip's tabs and its `N done` menu (`subject_strip`); None
    /// draws no strip.
    pub agents: Option<AnyElement>,
    /// The strip's split-right button (Solo).
    pub split: Option<AnyElement>,
    pub activity_decisions: Option<AnyElement>,
    /// A question too big for this Pane's body: it answers in fullscreen
    /// (the expand key), and the body keeps its transcript meanwhile.
    pub expand_question: bool,
    pub question_measurement: Option<AnyElement>,
    pub child_footer: Option<AnyElement>,
    /// Wires the whole L1 head as the Pane's drag handle (the cockpit
    /// supplies it while the board has somewhere to move the Pane to).
    pub head_drag: Option<HeadDrag>,
    /// The wall tile's quick answers while its Thread waits (L3 only): the
    /// approval's verbs or the question's options, each a boxed word wired
    /// to its answer in the cockpit.
    pub quick_answers: Option<AnyElement>,
    /// The Solo Composer's toolbar and its queued row's close button,
    /// wired in the cockpit (SPEC §5.5); on a board, the focused Pane's
    /// send circle alone.
    pub toolbar: Option<AnyElement>,
    pub unqueue: Option<AnyElement>,
    /// The Group head's fill button (`fill_button`), wired to fill the
    /// board with this Pane; None in Solo.
    pub fill: Option<AnyElement>,
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

/// A tile's rows, given how many whole cells its text column holds: each
/// line one row, prose wrapping once to a `  ` continuation at col 2 — at
/// the prose's own line break, else at the last space that fits — and the
/// last `WALL_LINES` rows kept. Rows truncate at the tile's edge.
pub(crate) fn wall_rows(lines: &[WallLine], columns: usize) -> Vec<SharedString> {
    let mut rows: Vec<SharedString> = Vec::new();
    for line in lines {
        if !line.wraps {
            rows.push(line.text.clone().into());
            continue;
        }
        let (head, tail) = wrap_once(&line.text, columns);
        rows.push(head.into());
        if let Some(tail) = tail {
            rows.push(format!("  {tail}").into());
        }
    }
    let skip = rows.len().saturating_sub(theme::WALL_LINES);
    rows.split_off(skip)
}

/// One line of prose into its row and, when it is wider than `columns`,
/// the continuation: a line break in the text wins, else the last space at
/// or before `columns` (never the glyph's own space).
fn wrap_once(text: &str, columns: usize) -> (String, Option<String>) {
    if let Some((head, rest)) = text.split_once('\n') {
        let rest = rest.lines().next().unwrap_or_default().trim();
        return (
            head.trim_end().to_string(),
            (!rest.is_empty()).then(|| rest.to_string()),
        );
    }
    if text.chars().count() <= columns {
        return (text.to_string(), None);
    }
    let mut split = None;
    for (seen, (byte, ch)) in text.char_indices().enumerate() {
        if seen > columns {
            break;
        }
        if ch == ' ' && seen >= 3 {
            split = Some(byte);
        }
    }
    match split {
        Some(byte) => {
            let rest = text[byte + 1..].trim();
            (
                text[..byte].trim_end().to_string(),
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
    pub queued: Vec<&'a str>,
    pub composer_empty: bool,
    pub composer_files: usize,
    pub suggestion: Option<&'a str>,
    pub received_reasoning_visible: bool,
    // ---- pre-plumbed for the work packages: wired, not read yet
    /// The operator asked the system for reduced motion.
    pub reduce_motion: bool,
    /// The Composer itself holds the keyboard in the active window.
    #[allow(dead_code)]
    pub editing: bool,
    /// Native files hover this Pane (`PaneFacts::drop_target`).
    pub drop_target: bool,
    /// The Session is starting or being replaced; nothing committed yet.
    pub starting: bool,
    /// Finished while the operator looked elsewhere (an unread Notice).
    #[allow(dead_code)]
    pub unread: bool,
    /// More than one Pane is on the board: the Composer is the grid's one
    /// fixed line (C4), not Solo's full block.
    pub grid: bool,
    // ---- owned wiring
    /// `l1_composer` / `l2_composer`.
    pub attachments: Option<AnyElement>,
    pub background: Option<AnyElement>,
    pub menu: Option<AnyElement>,
    /// A Subagent's footer, drawn where the Composer would be.
    pub child_footer: Option<AnyElement>,
    /// `l1_dock`: activity requests that were not docked in the body.
    pub activity_decisions: Option<AnyElement>,
    /// The docked Decision merges into this Pane's Composer (one block).
    pub decision_joined: bool,
    /// `l1_composer`: the Solo box's toolbar and queued-row close button.
    pub toolbar: Option<AnyElement>,
    pub unqueue: Option<AnyElement>,
}

/// The group name every Pane root carries (`.group(PANE_GROUP)`): what a
/// child styles itself by while the pointer is anywhere over its Pane (the
/// minimap's `group_hover`, the prototype's `.pane:hover .mm`).
pub(crate) const PANE_GROUP: &str = "pane";

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
        parked_line,
    } = facts;
    let empty = WallCard::default();
    let wall = wall.unwrap_or(&empty);
    let PaneWiring {
        transcript: retained_transcript,
        attachments,
        background,
        received_reasoning_visible,
        menu,
        title,
        agents,
        split,
        activity_decisions,
        expand_question,
        question_measurement,
        child_footer,
        head_drag,
        quick_answers,
        toolbar,
        unqueue,
        fill,
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
    let status = subject.as_ref().map(|subject| {
        crate::cockpit::subagents::transcript_status(subject.status(), subject.fresh())
    });
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
    let shell = record_card(pane_shell(), view);
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
            let word =
                crate::nav::NavTail::of(mark, slot.as_ref(), SharedString::default()).member_word();
            let head = group_head(GroupHead {
                key,
                name: view.name.clone(),
                mark: Some(mark),
                word,
                focused: framed,
                title,
                provider: provider_mark,
                action: fill,
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
        queued: thread.map(|thread| thread.queued_all()).unwrap_or_default(),
        composer_empty,
        composer_files,
        suggestion: thread.and_then(|thread| thread.suggestion()),
        received_reasoning_visible,
        reduce_motion,
        editing,
        drop_target,
        starting: thread.is_some_and(|thread| thread.starting()),
        unread,
        grid: show_focus,
        attachments,
        background,
        menu,
        child_footer,
        activity_decisions: None,
        decision_joined,
        toolbar,
        unqueue,
    };

    // Solo (fullscreen included) has no head: the nav's row names the
    // Thread (C2, ADR 0014), and the body starts at the Pane's own top edge. A
    // Group's Pane wears the one head. Subagent tabs keep a strip of their
    // own either way.
    let mut pane = shell.children(head(title));
    if let Some(agents) = agents {
        pane = pane.child(tab_strip(key, agents, l1_tasks(&mut cx), split));
    }
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
                    .children(question_measurement),
            );
            // The order is head · body · progress · dock · composer.
            // The working line is the Composer's top row now
            // (`working_line`); note what it reads before the Composer draws.
            cx.view.working.set(WorkingFacts {
                provider: cx.thread.map(|thread| thread.provider()),
                starting: cx.starting,
                reduce_motion: cx.reduce_motion,
                reasoning_visible: cx.received_reasoning_visible,
            });
            pane = pane.children(l1_dock(&mut cx));
            // A question too big for this body answers in fullscreen: the
            // Pane's foot says so, where its state reads (theme rule 7).
            pane = pane.children(expand_question.then(|| question_door(key)));
            // The drop sheet covers the Pane beneath the Composer, which
            // paints after it and so stays in view, edged in `TEXT_STRONG`.
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

/// What the working line reads besides its transcript, noted by
/// `render_pane` every frame before the Composer (which draws the line)
/// renders: the provider, whether a Session is starting, reduced motion, and
/// whether the live reasoning caption's own row is in view.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct WorkingFacts {
    pub provider: Option<Provider>,
    pub starting: bool,
    pub reduce_motion: bool,
    pub reasoning_visible: bool,
}

/// WP-A · the L1 working line (the prototype's `.comp > .r`), for the
/// Composer region to draw as its top row: while the transcript streams,
/// the line; while a Session starts with nothing streaming yet, its
/// starting shape; otherwise nothing. Only the Pane holding the keyboard
/// (`focused`) animates it; every line names `esc to interrupt`, as the
/// prototype's do, though the key acts only where the keyboard is.
pub(crate) fn working_line(
    view: &PaneView,
    transcript: &Transcript,
    solo: bool,
    focused: bool,
) -> Option<AnyElement> {
    let facts = view.working.get();
    let line = if transcript.status() == Status::Streaming {
        working_line_for(
            transcript,
            false,
            solo,
            focused,
            facts.reasoning_visible,
            facts.reduce_motion,
            facts.provider,
            view.thread().map_or(0, |thread| thread.get()),
        )
    } else if facts.starting {
        starting_line(facts.reduce_motion || !focused, solo)
    } else {
        return None;
    };
    let namespace = view.text_namespace();
    Some(
        div()
            .debug_selector(move || format!("transcript-progress-{namespace}"))
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .child(
                div()
                    .debug_selector(|| "transcript-progress".into())
                    .w_full()
                    .min_w_0()
                    .child(line),
            )
            .into_any_element(),
    )
}

/// The working line's shape while a Session starts: nothing the provider
/// says yet, so Ferrite's own mark holds the gutter — its shards snapping on
/// the focused Pane, assembled and still elsewhere and under reduced motion
/// — and `Starting` in `TEXT_DIM`, on the working line's own grid (Solo's
/// or a Group Pane's), so the line never shifts when the turn's words
/// replace it. Its element id is a constant: the timeline has to survive
/// every re-render of the line.
fn starting_line(still: bool, solo: bool) -> Div {
    let mark = if still {
        div()
            .debug_selector(|| "starting-mark-still".into())
            .child(icons::ferrite_icon(theme::GLYPH_BOX))
            .into_any_element()
    } else {
        div()
            .debug_selector(|| "starting-mark-live".into())
            .child(icons::animated_ferrite_icon(
                theme::GLYPH_BOX,
                "starting-progress-indicator",
            ))
            .into_any_element()
    };
    let grid = Grid::pane(Default::default(), solo);
    div()
        .debug_selector(|| "transcript-starting".into())
        .flex()
        .items_center()
        .w_full()
        .min_w_0()
        .h(px(grid.line))
        .font_family(theme::FONT_CODE)
        .text_size(px(grid.size))
        .line_height(px(grid.line))
        .text_color(rgb(TEXT_DIM))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .w(px(grid.gutter()))
                .child(mark),
        )
        .child(div().min_w_0().truncate().child("Starting"))
}

/// WP-C · the plan's meter beside the subagent tabs.
fn l1_tasks(cx: &mut PaneCtx) -> Option<AnyElement> {
    tasks_meter(cx.view.thread()?, cx.transcript?)
}

/// The plan's meter (`tasks_strip`) for a Thread working to one: beside the
/// subagent tabs.
pub(crate) fn tasks_meter(thread: ThreadId, transcript: &Transcript) -> Option<AnyElement> {
    let todos = transcript.todos()?;
    Some(
        tasks_strip(
            thread.get(),
            todos,
            transcript.current_task(),
            transcript.status() == Status::Streaming,
        )
        .into_any_element(),
    )
}

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

/// WP-D · the L1 Composer, or a Subagent's footer in its place.
fn l1_composer(cx: &mut PaneCtx) -> Option<AnyElement> {
    if let Some(footer) = cx.child_footer.take() {
        // A subagent's tab keeps the working line over its footer, spaced
        // as it stands over the Composer.
        let working = cx
            .transcript
            .and_then(|transcript| working_line(cx.view, transcript, !cx.grid, cx.focused));
        return Some(
            composer_stack(None, None, working, !cx.grid)
                .child(footer)
                .into_any_element(),
        );
    }
    let transcript = cx.transcript?;
    // The working line (`working_line`), noted for this frame above.
    let working = working_line(cx.view, transcript, !cx.grid, cx.focused);
    Some(
        composer_region(
            cx.view,
            Some(transcript),
            ComposerStack {
                grid: cx.grid,
                decision: cx.decision,
                queued: std::mem::take(&mut cx.queued),
                empty: cx.composer_empty,
                files: cx.composer_files,
                attachments: cx.attachments.take(),
                background: cx.background.take(),
                menu: cx.menu.take(),
                draft: false,
                draft_error: None,
                suggestion: cx.suggestion,
                working,
                focused: cx.focused,
                drop_target: cx.drop_target,
                joined: cx.decision_joined,
                toolbar: cx.toolbar.take(),
                unqueue: cx.unqueue.take(),
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

/// The Pane card (theme WP-C, SPEC §5.3): `paint::CARD` at `R_PANE`, its
/// content flush to its own edges — the ring is an overlay (`pane_frame`),
/// never a border in layout. `overflow: hidden` clips the children to the
/// card's box; the frame laid over the board (`components::frame_around`)
/// covers its rounded corners.
fn pane_shell() -> Div {
    div()
        .relative()
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .min_w_0()
        .rounded(px(theme::R_PANE))
        .bg(theme::paint::CARD)
        .font_family(theme::FONT_UI)
        .overflow_hidden()
}

/// What a Pane card's 1px ring says (F-2, SPEC §5.9): a waiting Pane
/// (`WAIT_RING`, focused or not) beats focus (`FOCUS_RING`), and a Pane
/// with neither rests on `CARD_RING`. Solo never recolours the ring (rule
/// 2.2.5): the Decision carries the signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaneEdge {
    Attention,
    Focused,
    Rest,
}

impl PaneEdge {
    pub(crate) fn of(focused: bool, attention: bool, solo: bool) -> Self {
        if attention && !solo {
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
            PaneEdge::Rest => rgba(CARD_RING).into(),
        }
    }
}

/// The Pane root (F-1): the card, then its 1px ring (`card_ring`) — it
/// takes no layout and no pointer, so the content starts at the card's own
/// edges and a state change moves nothing. Every Pane root is `PANE_GROUP`.
fn pane_frame(pane: Div, edge: PaneEdge, key: u64) -> Stateful<Div> {
    let overlay = card_ring(edge.ink());
    let overlay = match edge {
        PaneEdge::Focused => overlay.debug_selector(move || format!("pane-focus-edge-{key}")),
        PaneEdge::Attention => overlay.debug_selector(move || format!("pane-waiting-edge-{key}")),
        PaneEdge::Rest => overlay,
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
        .child(overlay)
}

/// A card's 1px ring in `ink`, just outside its box
/// (`components::outer_ring`), in the band the board's frame leaves around
/// the card (`components::frame_around`). The band is frame, so the ring lays
/// `paint::FRAME` under its ink there: the hairline reads over the frame,
/// as drawn. The cockpit rings a reader's card with it.
pub(crate) fn card_ring(ink: impl Into<gpui::Hsla>) -> Div {
    div()
        .absolute()
        .inset_0()
        .child(components::outer_ring(theme::R_PANE, theme::paint::FRAME))
        .child(components::outer_ring(theme::R_PANE, ink))
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
    /// The Solo box's toolbar (`PaneWiring::toolbar`).
    pub toolbar: Option<AnyElement>,
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
    } = state;
    // A draft wears the live Pane's edge: transparent at rest or, beside
    // other Panes, the focus ink. It has no state to announce.
    let framed = focused && show_focus;
    let edge = PaneEdge::of(framed, false, !show_focus);
    let key = view.identity.draft().map_or(0, DraftId::get);
    let mut shell = record_card(pane_shell(), view);
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
        }));
    }
    let _ = reduce_motion;
    let composer = composer_region(
        view,
        None,
        ComposerStack {
            grid: show_focus,
            decision: None,
            queued: Vec::new(),
            empty: composer_empty,
            files: composer_files,
            attachments,
            background: None,
            menu,
            draft: true,
            draft_error: error.cloned(),
            // A draft has no conversation yet, so nothing to predict.
            suggestion: None,
            working: None,
            focused,
            drop_target,
            joined: false,
            toolbar,
            unqueue: None,
        },
    );
    // The body says nothing — the Composer's placeholder says what to do,
    // once (rule 2.11.4) — but it is not a void. A lone draft at L1 is a
    // launch: the mark, embossed, over the Composer, the pair centred in
    // the Pane, where the eye already is. The first send makes it a
    // Thread, whose Composer sits at the foot. On a board the line keeps
    // the grid's foot, and the mark centres in the body above it.
    let launch = !show_focus && level == Level::Transcript;
    let mark = (level == Level::Transcript)
        .then(|| components::embossed_mark(theme::DRAFT_MARK, theme::EMBOSS_ON_PANE));
    let body = div()
        .debug_selector(|| "draft-empty".into())
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .flex()
        .flex_col()
        .items_center()
        .map(|body| {
            if launch {
                body.justify_end().pb(px(theme::DRAFT_MARK_GAP))
            } else {
                body.justify_center()
            }
        })
        .children(mark);
    pane_frame(
        shell
            .child(body)
            .children(drop_target.then(crate::prompt_drop::sheet))
            .when(level != Level::Wall, |pane| pane.child(composer))
            // The launch's lower half: a touch taller than the upper, so
            // the pair sits just above the Pane's middle.
            .when(launch, |pane| {
                pane.child(div().flex_1().min_h_0().pb(px(theme::DRAFT_LAUNCH_LIFT)))
            }),
        edge,
        u64::MAX - key,
    )
}

/// A Draft is disposable state, so its Pane advertises the same close action
/// as cmd-w: an `ICON_BUTTON` × in `TEXT_DIM` — the Group head's slot on a
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
        .child(icon(icons::CLOSE, theme::ICON_BUTTON_GLYPH, TEXT_DIM))
}

/// The × with its tooltip: `Discard draft`, and the key after it only where
/// one is bound (`menu::tooltip_with_key`).
pub fn draft_discard(draft: DraftId, button: AnyElement, keys: Option<String>) -> Stateful<Div> {
    div()
        .id(("discard-draft-tip", draft.get() as usize))
        .flex_shrink_0()
        .tooltip(crate::menu::tooltip_with_key("Discard draft", keys))
        .child(button)
}

/// A draft's Project or Workspace chip (SPEC §5.5), in the toolbar's chip
/// style: the choice and a chevron, ringed by the one focus recipe
/// (`components::focused`) while tab rests on it, because the popover
/// opens on ↵ and the chip must say where ↵ will land. In a narrow Pane its
/// label cuts first.
pub fn band_chip(slot: usize, label: SharedString, focused: bool) -> Stateful<Div> {
    // The label truncates in a narrow Pane; the tooltip keeps the whole
    // choice reachable and names the keys that change it.
    let tooltip = SharedString::from(format!("{label} \u{b7} Tab, then \u{21b5} to change"));
    div()
        .id(("band-chip", slot))
        .tooltip(move |window, cx| {
            gpui::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        })
        .debug_selector(move || format!("band-chip-{slot}"))
        .flex()
        .flex_shrink(1.)
        .min_w_0()
        .items_center()
        .h(px(theme::TOOL_CHIP_H))
        .px(px(theme::TOOL_CHIP_PAD_X))
        .rounded(px(theme::R_CHIP))
        .map(|chip| components::focused(chip, focused))
        .hover_raised(format!("band-chip-{slot}"))
        .press_raised()
        .child(chip_face(None, label, TEXT_MUTED, theme::W_BODY))
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
    /// The tile's width, for the prose's one wrap.
    cell_width: f32,
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
                .tooltip(crate::menu::tooltip(detail))
                .child(face)
                .into_any_element(),
            None => face.into_any_element(),
        }
    });
    let rows: Vec<SharedString> = match state {
        WallState::Parked => vec![
            parked_line.unwrap_or_else(|| SharedString::from(theme::words::PARKED)),
            SharedString::from(WAKE_HINT),
        ],
        _ => {
            let columns = ((cell_width - 2.0 * theme::WALL_PAD_X) / theme::CH)
                .floor()
                .max(0.0) as usize;
            wall_rows(&card.lines, columns)
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
            .font_weight(theme::W_STRONG)
            .child(div().flex_shrink_0().text_color(rgb(ink)).child(lead))
            .children(rest.map(|rest| {
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(TEXT_DIM))
                    .child(rest)
            })),
    )
}

/// One of a tile's rows: one `TEXT_DIM` run, its glyph included, on one
/// line that truncates with `…` at the tile's edge.
fn wall_line(row: SharedString) -> Div {
    div()
        .debug_selector(|| "wall-line".into())
        .flex_shrink_0()
        .w_full()
        .min_w_0()
        .h(px(theme::LH_UI))
        .text_color(rgb(TEXT_DIM))
        .truncate()
        .child(row)
}

/// The tile's quick answers: boxed words a cell apart, half a row under
/// the lines, the first `WALL_PAD_X` in from the tile's edge and the row
/// `WALL_PAD_Y` above its foot — each the cockpit's wired button
/// (`quick_answer`).
pub(crate) fn quick_answers(buttons: Vec<AnyElement>) -> Div {
    div()
        .debug_selector(|| "wall-quick-answers".into())
        .flex()
        .flex_shrink_0()
        .flex_wrap()
        .gap(px(theme::QUICK_ANSWER_GAP))
        .mt(px(theme::HALF_ROW))
        .children(buttons)
}

/// One quick answer (F-7): `1 allow` as a secondary button, no edge
/// (theme rule: no grey borders) — a `paint::RAISED` face, radius
/// `R_TAB`, `QUICK_ANSWER_H` (22px) tall, 1ch in from each side. The digit
/// is `TEXT_DIM` on every waiting tile, the word `TEXT`; under the pointer
/// the face blends to `paint::SELECTED` and the word to `TEXT_STRONG` over
/// the one 150ms hover blend, the digit staying dim.
pub(crate) fn quick_answer(
    id: SharedString,
    key: SharedString,
    label: SharedString,
) -> Stateful<Div> {
    let hover = id.clone();
    let ground = crate::motion::hover_blend(
        &hover,
        theme::paint::RAISED.hsla(),
        theme::paint::SELECTED.hsla(),
    );
    let word = crate::motion::hover_blend(&hover, rgb(TEXT).into(), rgb(TEXT_STRONG).into());
    let selector = id.clone();
    // Its two runs and a cell of padding each side.
    let key = format!("{key} ");
    let width = components::run_width(&key)
        + components::run_width(&label)
        + 2.0 * theme::QUICK_ANSWER_PAD_X;
    div()
        .id(gpui::ElementId::Name(id))
        .debug_selector(move || selector.to_string())
        .flex()
        .flex_shrink_0()
        .items_center()
        .w(px(width))
        .h(px(theme::QUICK_ANSWER_H))
        .px(px(theme::QUICK_ANSWER_PAD_X))
        .rounded(px(theme::R_TAB))
        .bg(ground)
        .cursor_pointer()
        .whitespace_nowrap()
        .on_hover(crate::motion::hover_listener(hover))
        .child(components::cells(key).text_color(rgb(TEXT_DIM)))
        .child(components::cells(label).text_color(word))
}

/// The Pane head's mark: the main Thread's is its nav row's
/// (`nav::RowStatus`) — the done check while its finish is `unread`, the
/// green ring while `background` work runs behind its idle Main. A
/// subagent tab's is that agent's own transcript state.
fn head_mark(
    main: bool,
    state: WallState,
    unread: bool,
    background: bool,
    subject: Option<Status>,
) -> crate::nav::RowStatus {
    if main {
        return crate::nav::RowStatus::of(state, unread, background);
    }
    let state = match subject {
        Some(Status::Streaming) => WallState::Working,
        Some(Status::Blocked) => WallState::Decision,
        Some(Status::Closed) => WallState::Blocked,
        _ => WallState::Idle,
    };
    crate::nav::RowStatus::of(state, false, false)
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
pub(crate) enum HeadSlot {
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
            _ => TEXT_DIM,
        }
    }
}

/// The state vocabulary, shared by the Group head and the wall:
/// `needs you` > `failing` > `failed` > `working` > `done` >
/// `interrupted` > `parked`; idle is nothing. A finish is `done` only while
/// `unread` (not yet seen), as the nav row's mark is
/// (`nav::RowStatus::of`); seen, it is idle.
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
            Some(TurnOutcome::Error(_)) => HeadSlot::Failed,
            _ if unread => HeadSlot::Done,
            Some(TurnOutcome::Interrupted) => HeadSlot::Interrupted,
            _ => return None,
        },
    })
}

/// The state a Thread's dot shows: its wall state, squared with the state
/// word beside it so the two can never disagree. A turn that ended in an
/// error reads `failed`, so its dot is `BLOCKED` like a closed Session's;
/// a `failing` word is always a `BLOCKED` dot. Every surface that draws a
/// Thread's dot — the nav, the rail, the Group head — reads
/// it through here.
pub(crate) fn dot_state(state: WallState, word: Option<&HeadSlot>) -> WallState {
    match word {
        Some(HeadSlot::Failed) if state != WallState::Blocked => WallState::Blocked,
        Some(HeadSlot::Failing(_)) => WallState::Failing,
        _ => state,
    }
}

/// A Thread's state word away from its Pane, read exactly as the Pane
/// reads it.
pub(crate) fn thread_word(
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
        .text_color(rgb(TEXT_DIM))
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
                .child(components::key_combo(&keys, TEXT_DIM))
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
/// alike: one `PANE_HEAD_H` row (38px) closed by a `HAIRLINE`, padded
/// `PANE_HEAD_PAD_L` / `PANE_HEAD_PAD_R`, its items `PANE_HEAD_GAP` apart.
/// In order: the state mark (`nav::head_mark`), the title in 12.5 at
/// `W_LABEL` — `TEXT_STRONG` on the focused Pane, `TEXT` elsewhere — kept
/// to its `title_floor`, the state word in 12 and its ink, a spacer, the
/// provider's 13px logo and the trailing control: the fill button, or a
/// draft's ×. No band on the focused head: the ring says focus.
pub(crate) struct GroupHead {
    pub key: u64,
    pub name: SharedString,
    /// The Thread's mark; a draft has none.
    pub mark: Option<crate::nav::RowStatus>,
    /// The state word and its ink (`nav::NavTail::member_word`).
    pub word: Option<(SharedString, u32)>,
    /// The Pane holds the keyboard on a board: a strong title.
    pub focused: bool,
    /// The wired title (drag handle, double-click rename); `None` draws the
    /// name.
    pub title: Option<AnyElement>,
    pub provider: Option<Provider>,
    /// The trailing control: the fill button, or a draft's ×.
    pub action: Option<AnyElement>,
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
    } = head;
    let floor = title_floor(&name);
    let title_ink = if focused { TEXT_STRONG } else { TEXT };
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
        .border_color(theme::paint::HAIRLINE)
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_LABEL))
        .line_height(px(theme::LH_UI))
        .children(mark.map(|mark| {
            div()
                .debug_selector(move || format!("pane-head-dot-{key}"))
                .flex()
                .flex_shrink_0()
                .items_center()
                .h(px(theme::LH_UI))
                .child(crate::nav::head_mark(mark))
        }))
        .child(
            div()
                .debug_selector(move || format!("pane-head-title-{key}"))
                .flex()
                .flex_shrink(1.)
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
                .flex_shrink_0()
                .text_size(px(theme::FS_META))
                .text_color(rgb(ink))
                .child(text)
        }))
        .child(div().flex_1())
        .children(provider.map(|provider| {
            let (glyph, ink) = match provider {
                Provider::Codex => (icons::CODEX, theme::PROVIDER_CODEX),
                Provider::Claude => (icons::CLAUDE, theme::PROVIDER_CLAUDE),
            };
            div()
                .debug_selector(move || format!("pane-head-provider-{key}"))
                .flex()
                .flex_shrink_0()
                .child(icon(glyph, theme::PANE_HEAD_ICON, ink))
        }))
        .children(action)
}

/// The head's fill button (SPEC §5.9): a 28px `maximize` door, `RAISED`
/// under the pointer, whose press the cockpit wires to fill the board with
/// this Pane (`ToggleFullscreen`), its tooltip `Fill the board ⌘⇧⏎` read
/// from the keymap.
pub(crate) fn fill_button(key: u64) -> Stateful<Div> {
    let blend = SharedString::from(format!("pane-fill-{key}"));
    let t = crate::motion::hover_t(&blend);
    div()
        .id(gpui::ElementId::Name(blend.clone()))
        .debug_selector(move || format!("pane-fill-{key}"))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(theme::ICON_BUTTON))
        .rounded(px(theme::R_TAB))
        .cursor_pointer()
        .when(t > 0.0, |door| {
            door.bg(crate::motion::mix(
                rgba(TRANSPARENT).into(),
                theme::paint::RAISED.into(),
                t,
            ))
        })
        .on_hover(crate::motion::hover_listener(blend))
        .tooltip(crate::menu::action_tooltip(
            "Fill the board",
            "cockpit::ToggleFullscreen",
        ))
        .child(icon(icons::MAXIMIZE, theme::PANE_HEAD_FILL_GLYPH, TEXT_DIM))
}

/// The failure strip (SPEC §5.9 item 4), the transcript's tail row under
/// a failed turn's failure line, Solo's and a Group Pane's alike:
/// `FAILURE_STRIP_H` at `R_BLOCK` on `FAILURE_WASH`, an `x-circle` in
/// `BLOCKED`, `Stopped 3m ago` in `TEXT`, and at the right the wired
/// `Retry` (`retry_button`), which a closed Session goes without.
pub(crate) fn failure_strip(key: u64, words: SharedString, retry: Option<AnyElement>) -> Div {
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
        .rounded(px(theme::R_BLOCK))
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
                .text_color(rgb(TEXT))
                .child(words),
        )
        .child(div().flex_1())
        .children(retry)
}

/// The strip's `Retry`: `RETRY_H` at `R_TAB` on `SELECTED`, padded
/// `RETRY_PAD_X`, its label at `W_LABEL` in `TEXT_STRONG`. The caller wires
/// the press.
pub(crate) fn retry_button(key: u64, cx: &gpui::App) -> gpui::component::button::Button {
    components::faded_button(
        ("failure-retry", key as usize),
        theme::paint::SELECTED.into(),
        theme::paint::SELECTION_HOVER.into(),
        theme::paint::PRESS.into(),
        rgb(TEXT_STRONG).into(),
        cx,
    )
    .debug_selector(move || format!("failure-retry-{key}"))
    .flex_shrink_0()
    .h(px(theme::RETRY_H))
    .px(px(theme::RETRY_PAD_X))
    .rounded(px(theme::R_TAB))
    .child(
        components::text_ui()
            .text_size(px(theme::FS_LABEL))
            .font_weight(theme::W_LABEL)
            .text_color(rgb(TEXT_STRONG))
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
        .on_click(|_, window, cx| {
            window.dispatch_action(Box::new(crate::cockpit::NextDecision), cx)
        });
    door.tooltip(crate::menu::action_tooltip(
        "Next needs you",
        "cockpit::NextDecision",
    ))
    .into_any_element()
}

/// The card's tab strip (SPEC §5.3), under the head on a board and at the
/// card's top edge in Solo: one `SUBJECT_STRIP_H` row padded
/// `SUBJECT_STRIP_PAD_X`, closed by a `HAIRLINE` — the tabs and their menu
/// taking the width, then the plan's meter in 12 `TEXT_DIM` and the split
/// button at the right.
fn tab_strip(
    key: u64,
    agents: AnyElement,
    tasks: Option<AnyElement>,
    split: Option<AnyElement>,
) -> Div {
    div()
        .debug_selector(move || format!("pane-tabs-{key}"))
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::SUBJECT_TAB_GAP))
        // The rule lies under the row's 38, as the mockup's border does.
        .h(px(theme::SUBJECT_STRIP_H + 1.))
        .px(px(theme::SUBJECT_STRIP_PAD_X))
        .border_b_1()
        .border_color(theme::paint::HAIRLINE)
        .text_size(px(theme::FS_LABEL))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(TEXT_DIM))
        .child(agents)
        .children(tasks.map(|tasks| {
            div()
                .flex_shrink_0()
                .px(px(theme::SPACE_2))
                .text_size(px(theme::FS_META))
                .child(tasks)
        }))
        .children(split)
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

/// A check's ink (rule 2.2.2 — colour means something needs you): a run
/// still going is `RUNNING`, a failure `BLOCKED`, a pass settles to
/// `TEXT_DIM` and a skip is the faintest ink (drawn as a ring,
/// `check_dot`). Never the ochre that means an agent waits on you.
pub fn check_ink(state: CheckState) -> u32 {
    match state {
        CheckState::Pending => RUNNING,
        CheckState::Passing => TEXT_DIM,
        CheckState::Failing => BLOCKED,
        CheckState::Skipped => TEXT_FAINT,
    }
}

/// A check's dot: `check_ink` filled, or a hollow ring for a skipped run —
/// it claims nothing.
pub fn check_dot(state: CheckState) -> Div {
    match state {
        CheckState::Skipped => components::status_ring(check_ink(state)),
        state => components::status_dot(check_ink(state)),
    }
}

/// A run's state word ink: only a failure is coloured — the dot beside it
/// already says pending, passed or skipped.
pub fn check_detail_ink(state: CheckState) -> u32 {
    match state {
        CheckState::Failing => BLOCKED,
        _ => TEXT_DIM,
    }
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
        CheckState::Pending
            if ["queued", "waiting", "pending", "requested"]
                .iter()
                .any(|word| detail.contains(word)) =>
        {
            "queued"
        }
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
        (tally.pending, "running", TEXT_DIM),
        (tally.passing, "passed", TEXT_DIM),
        (tally.skipped, "skipped", TEXT_DIM),
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
        if ink != TEXT_DIM {
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
        .text_color(rgb(TEXT_DIM))
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
                        .text_color(rgb(TEXT_STRONG))
                        .child("Checks"),
                )
                .child(
                    div()
                        .text_color(rgb(TEXT_DIM))
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
        .rounded(px(theme::R_CHIP))
        // A matrix job's name can outrun the card; the whole of it is one
        // hover away.
        .tooltip(crate::menu::tooltip(SharedString::from(format!(
            "{name} \u{b7} {}",
            run.detail
        ))))
        .child(check_dot(run.state))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_color(rgb(if openable { TEXT } else { TEXT_DIM }))
                .child(name),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_color(rgb(check_detail_ink(run.state)))
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

/// How the tasks meter draws a plan: one segment per step up to
/// `METER_SEG_CAP`, past it one continuous track filled to the fraction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum MeterLayout {
    Segments { done: usize, total: usize },
    Track { fraction: f32 },
}

pub(crate) fn meter_layout(done: usize, total: usize) -> MeterLayout {
    let done = done.min(total);
    if total <= theme::METER_SEG_CAP {
        MeterLayout::Segments { done, total }
    } else {
        MeterLayout::Track {
            fraction: done as f32 / total as f32,
        }
    }
}

/// The painted plan meter beside the subagent tabs (neither face has
/// `▰▱`): done steps in `TEXT_DIM`, the rest unlit. `live` lights the
/// step being run in `RUNNING` — the one colour on a meter, because a step
/// in progress is live state. Finishing a plan is not a status: a full
/// meter is all `TEXT_DIM`, no check and no green.
fn meter_bar(done: usize, total: usize, live: bool) -> Div {
    let segment = |ink: gpui::Hsla| {
        div()
            .flex_shrink_0()
            .w(px(theme::METER_SEG_W))
            .h(px(theme::METER_SEG_H))
            .rounded(px(theme::METER_SEG_R))
            .bg(ink)
    };
    match meter_layout(done, total) {
        MeterLayout::Segments { done, total } => div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(theme::METER_SEG_GAP))
            .children((0..total).map(|index| {
                segment(if index < done {
                    rgb(TEXT_DIM).into()
                } else if index == done && live {
                    rgb(RUNNING).into()
                } else {
                    rgba(METER_OFF).into()
                })
            })),
        MeterLayout::Track { fraction } => div()
            .flex_shrink_0()
            .w(px(theme::METER_TRACK_W))
            .h(px(theme::METER_SEG_H))
            .rounded(px(theme::METER_SEG_R))
            .bg(rgba(METER_OFF))
            .child(
                div()
                    .h_full()
                    .w(px(theme::METER_TRACK_W * fraction))
                    .rounded(px(theme::METER_SEG_R))
                    .bg(rgb(TEXT_DIM)),
            ),
    }
}

/// The painted meter and its `3/4` count, in one row.
fn meter(done: usize, total: usize, live: bool) -> Div {
    let done = done.min(total);
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::METER_GAP))
        .child(meter_bar(done, total, live))
        .child(components::tabular(
            div()
                .flex_shrink_0()
                .text_color(rgb(TEXT_DIM))
                .child(SharedString::from(format!("{done}/{total}"))),
        ))
}

/// The tasks meter in the head's right cluster (it replaced the tasks
/// row): segments and count, the step being run lit while the transcript
/// streams, and that step's words in a tooltip — the working line and the
/// plan's own tool rows carry them in the body.
fn tasks_strip(key: u64, todos: Todos, current: Option<&str>, streaming: bool) -> Stateful<Div> {
    let done = todos.done.min(todos.total);
    let tip = SharedString::from(match current {
        Some(current) => format!("{done}/{} · {current}", todos.total),
        None => format!("{done}/{}", todos.total),
    });
    meter(done, todos.total, streaming && current.is_some())
        .id(("tasks-meter", key as usize))
        .debug_selector(move || format!("tasks-meter-{key}"))
        .tooltip(crate::menu::tooltip(tip))
}

/// The name a model shows under, from any spelling the wire uses:
/// `claude-sonnet-4-5` → `Sonnet 4.5`, `gpt-5.4-mini` → `GPT-5.4 Mini`.
/// Public because every chip and row must spell a model exactly one way —
/// one grooming, never two; the catalog's own display names win where a
/// Session announced them (see `providers::models::label`).
#[cfg(test)]
pub fn model_label(model: &str) -> SharedString {
    SharedString::from(ferrite_core::providers::models::display_name(model))
}

/// The model chip's face (SPEC §5.5): the provider's logomark in its brand
/// colour, the model's name as the picker names it, and a chevron.
/// Render-only; the cockpit gives it its button and its menu.
pub fn model_chip(provider: Option<Provider>, label: SharedString) -> Div {
    let mark = provider.map(|provider| {
        let (glyph, ink) = match provider {
            Provider::Codex => (icons::CODEX, theme::PROVIDER_CODEX),
            Provider::Claude => (icons::CLAUDE, theme::PROVIDER_CLAUDE),
        };
        icon(glyph, theme::TOOL_CHIP_LOGO, ink).into_any_element()
    });
    chip_face(mark, label, TEXT_MUTED, theme::W_BODY)
}

/// The effort chip's face: the level in force, sentence case, and a
/// chevron.
pub fn effort_chip(label: SharedString) -> Div {
    chip_face(None, label, TEXT_MUTED, theme::W_BODY)
}

/// The permission mode chip's face: the `shield`, the mode's label and a
/// chevron, in `theme::mode_ink`: `ATTENTION` for a mode that lets
/// everything run, else `TEXT_MUTED`.
pub fn mode_chip(label: SharedString, mode: &str) -> Div {
    let ink = theme::mode_ink(mode);
    chip_face(
        Some(icon(icons::SHIELD, theme::TOOL_CHIP_ICON, ink).into_any_element()),
        label,
        ink,
        theme::W_LABEL,
    )
}

/// A toolbar chip's face: an optional lead mark, the label cut at its end
/// in a narrow Pane, and the chevron that says it opens a menu.
fn chip_face(
    lead: Option<AnyElement>,
    label: SharedString,
    ink: u32,
    weight: gpui::FontWeight,
) -> Div {
    div()
        .flex()
        .flex_shrink(1.)
        .min_w_0()
        .items_center()
        .gap(px(theme::TOOL_CHIP_GAP))
        .whitespace_nowrap()
        .font_family(theme::FONT_UI)
        .font_weight(weight)
        .text_size(px(theme::FS_LABEL))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(ink))
        .children(lead)
        .child(div().min_w_0().truncate().child(label))
        .child(icon(icons::CHEVRON_DOWN, theme::TOOL_CHIP_CHEVRON, ink))
}

/// A toolbar chip's button (SPEC §5.5): `TOOL_CHIP_H` high at `R_CHIP`,
/// `TOOL_CHIP_PAD_X` at each side, no ground at rest, `paint::RAISED`
/// under the pointer. The cockpit gives it its face and its menu. It may
/// narrow past its label, which then cuts (`composer_toolbar`).
pub fn toolbar_chip(
    id: impl Into<gpui::ElementId>,
    cx: &gpui::App,
) -> gpui::component::button::Button {
    chip_button(id, cx)
        .p_0()
        .h(px(theme::TOOL_CHIP_H))
        .px(px(theme::TOOL_CHIP_PAD_X))
        .min_w_0()
        .flex_shrink(1.)
        .rounded(px(theme::R_CHIP))
}

/// The toolbar's `+`: a `COMPOSER_ATTACH` square that attaches files.
pub fn attach_button(cx: &gpui::App) -> gpui::component::button::Button {
    chip_button("composer-attach", cx)
        .debug_selector(|| "composer-attach".into())
        .p_0()
        .size(px(theme::COMPOSER_ATTACH))
        .flex_shrink_0()
        .rounded(px(theme::R_CHIP))
        .tip("Attach files")
        .child(icon(icons::PLUS, theme::COMPOSER_ATTACH_ICON, TEXT_DIM))
}

/// The send circle: `COMPOSER_SEND` round on `paint::SEND`, `arrow-up` in
/// `TEXT_STRONG` — or, while a turn runs with nothing typed, `stop`. The
/// `compact` Composer's (SPEC §5.9) is `SEND_CIRCLE_SM`, its glyph
/// `SEND_GLYPH_SM`, spaced by its box's gap.
pub fn send_button(stop: bool, compact: bool, cx: &gpui::App) -> gpui::component::button::Button {
    let (size, glyph, gap) = if compact {
        (theme::SEND_CIRCLE_SM, theme::SEND_GLYPH_SM, 0.)
    } else {
        (
            theme::COMPOSER_SEND,
            theme::COMPOSER_SEND_ICON,
            theme::COMPOSER_SEND_GAP,
        )
    };
    components::faded_button(
        "composer-send",
        theme::paint::SEND.into(),
        theme::paint::SEND.into(),
        theme::paint::PRESS.into(),
        rgb(TEXT_STRONG).into(),
        cx,
    )
    .debug_selector(move || {
        if stop {
            "composer-stop"
        } else {
            "composer-send"
        }
        .into()
    })
    .p_0()
    .size(px(size))
    .ml(px(gap))
    .flex_shrink_0()
    .rounded_full()
    .tip(if stop { "Stop" } else { "Send" })
    .child(icon(
        if stop { icons::STOP } else { icons::ARROW_UP },
        glyph,
        TEXT_STRONG,
    ))
}

/// The Solo box's toolbar (`COMPOSER_TOOLBAR_H`): the `+`, `lead` — a
/// draft's chips, the mode — a spacer, `trail` — the model, the effort —
/// and the send circle. In a narrow box the lead chips give way first,
/// their labels cut at the end (the mode keeps its shield and its ink);
/// the model and effort stay whole; narrower still, the chips clip at the
/// row's end. The `+` and send always stand whole inside the box.
pub fn composer_toolbar(
    attach: AnyElement,
    lead: Vec<AnyElement>,
    trail: Vec<AnyElement>,
    send: AnyElement,
) -> Div {
    // A lead chip's slot may grow narrower than the chip: the chip, a
    // block in it, takes the slot's width and cuts its label.
    let give = |chip: AnyElement| {
        div()
            .min_w_0()
            .flex_shrink(1.)
            .overflow_hidden()
            .child(chip)
    };
    let keep = |chip: AnyElement| div().flex_shrink_0().child(chip);
    let chips = div()
        .flex()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .items_center()
        .gap(px(theme::COMPOSER_TOOLBAR_GAP))
        .children(lead.into_iter().map(give))
        .child(div().flex_1().min_w_0())
        .children(trail.into_iter().map(keep));
    div()
        .debug_selector(|| "composer-toolbar".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::COMPOSER_TOOLBAR_GAP))
        .h(px(theme::COMPOSER_TOOLBAR_H))
        .min_w_0()
        .px(px(theme::COMPOSER_TOOLBAR_PAD_X))
        .child(attach)
        .child(chips)
        .child(send)
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

/// What every working line says at its right end.
const INTERRUPT_HINT: &str = "esc to interrupt";

/// The working line's mark (the prototype's `.spin`): the star spinner
/// cycling `· ✢ ✳ ✶ ✻ ✽` in the provider's colour (`live`), the still `✻`
/// under reduced motion. Its selector says which.
fn working_mark(ink: u32, live: bool) -> AnyElement {
    // At the gutter's start, as the prototype's `.spin` sets its glyph.
    if live {
        div()
            .debug_selector(|| "progress-mark-live".into())
            .child(components::working_spinner(ink))
            .into_any_element()
    } else {
        components::glyph(components::WORKED.to_string(), ink)
            .debug_selector(|| "progress-mark-still".into())
            .into_any_element()
    }
}

/// The working line with no provider known: the spinner and caption in
/// `TEXT_DIM` (the L2 cell's, retired with it; kept for the tests).
#[cfg(test)]
fn working_row(
    transcript: &Transcript,
    compact: bool,
    focused: bool,
    received_reasoning_is_visible: bool,
    reduce_motion: bool,
) -> Div {
    working_line_for(
        transcript,
        compact,
        true,
        focused,
        received_reasoning_is_visible,
        reduce_motion,
        None,
        0,
    )
}

/// The working line (SPEC §5.4): one row on a Solo (`solo`) or a Group
/// Pane's transcript grid, the star spinner in the gutter, the provider's
/// live caption in its colour — the turn's spinner verb (`Reticulating…`,
/// `progress::spinner_verb`, held for the whole turn) unless the provider
/// has something better to say (R16) — then `1m04s · ↑ 4.1k tokens` in
/// `TEXT_DIM`, and `esc to interrupt` (`TEXT_DIM`) at the line's right. Every working line's spinner cycles,
/// on the one shared clock, so all of them show the same frame (the
/// prototype's `.spin`); only the focused Pane's caption shimmers. The seconds
/// are whole (`progress::live_seconds`), tabular, so the text changes once a
/// second. Every line names `esc to interrupt`, focused or not, as the
/// prototype's do (the key acts only where the keyboard is). The caption is
/// what truncates; the facts keep
/// their room. L2 (`compact`) draws the same row without the token count.
fn working_line_for(
    transcript: &Transcript,
    compact: bool,
    solo: bool,
    focused: bool,
    received_reasoning_is_visible: bool,
    reduce_motion: bool,
    provider: Option<Provider>,
    thread: u64,
) -> Div {
    let ink = provider_ink(provider);
    let live = focused && !reduce_motion;
    let spinning = !reduce_motion;
    let mut facts: Vec<String> = Vec::new();
    if let Some(elapsed) = transcript.turn_elapsed() {
        facts.push(ferrite_core::progress::live_seconds(elapsed));
    }
    let tokens = transcript.turn_output_tokens();
    if tokens > 0 && !compact {
        facts.push(format!("\u{2191} {} tokens", tokens_label(tokens)));
    }
    let progress = transcript.progress();
    let generic = Phase::Working.label();
    let caption = progress
        .caption()
        .map(|caption| {
            if !compact && received_reasoning_is_visible {
                progress.phase.map(Phase::label).unwrap_or(generic).into()
            } else {
                caption
            }
        })
        .map(|caption| {
            // The selector names what the line means (`Working`); the words
            // are the turn's verb.
            if caption == generic {
                (
                    format!("progress-caption-{generic}"),
                    format!(
                        "{}\u{2026}",
                        ferrite_core::progress::spinner_verb(thread, transcript.turn_count())
                    ),
                )
            } else {
                // R16: the caption always trails off (`Reticulating…`).
                let words = if caption.ends_with('\u{2026}') {
                    caption.clone()
                } else {
                    format!("{caption}\u{2026}")
                };
                (format!("progress-caption-{caption}"), words)
            }
        });
    // The line stands at the Standard size: the reading keys scale the
    // transcript only.
    let grid = Grid::pane(Default::default(), solo);
    let mut row = div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .w_full()
        .min_w_0()
        .h(px(grid.line))
        .font_family(theme::FONT_CODE)
        .text_size(px(grid.size))
        .line_height(px(grid.line));
    if let Some((selector, caption)) = caption {
        let caption = SharedString::from(caption);
        // The caption and the facts hold their runs' widths, the facts'
        // leading space inside their run.
        let caption_w = components::run_width(&caption);
        let text = if live {
            components::shimmer(caption, ink)
        } else {
            div()
                .min_w_0()
                .truncate()
                .text_color(rgb(ink))
                .child(caption)
                .into_any_element()
        };
        let metadata = SharedString::from(format!(" {}", facts.join(" \u{b7} ")));
        let highlights = separators(&metadata);
        let metadata_w = components::run_width(&metadata);
        row = row
            .debug_selector(move || selector.clone())
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .w(px(grid.gutter()))
                    .h(px(grid.line))
                    .child(working_mark(ink, spinning)),
            )
            .child(
                div()
                    .debug_selector(|| "progress-reasoning".into())
                    .flex()
                    .min_w_0()
                    .w(px(caption_w))
                    .flex_shrink(1.)
                    .child(text),
            )
            .when(!facts.is_empty(), |row| {
                row.child(components::tabular(
                    div()
                        .debug_selector(|| "progress-metadata".into())
                        .flex_shrink_0()
                        .w(px(metadata_w))
                        .whitespace_nowrap()
                        .text_color(rgb(TEXT_DIM))
                        .child(StyledText::new(metadata).with_highlights(highlights)),
                ))
            })
            // Every working line names the key, focused or not (it acts
            // where the keyboard is), at the right end of the line.
            .child(div().flex_1().min_w(px(2.0 * grid.cell())))
            .child(
                div()
                    .debug_selector(|| "progress-interrupt".into())
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .text_color(rgb(TEXT_DIM))
                    .child(INTERRUPT_HINT),
            );
    }
    div().w_full().min_w_0().flex_shrink_0().child(row)
}

/// `8.0k`, `12k`, `340` — the token count the way Claude Code prints it
/// (`progress::token_label`, which the turn's stamp reads too).
fn tokens_label(tokens: u64) -> String {
    ferrite_core::progress::token_label(tokens)
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
        parked_line,
    })
}

// --------------------------------------------------------------- Composer

/// The Composer stack's slice of `PaneState`, bundled so `composer_region`
/// stays readable as the states grow.
struct ComposerStack<'a> {
    /// A board cell (C4): only the focused cell's Composer is live — its
    /// shelf and queue; the others keep their input band.
    grid: bool,
    decision: Option<&'a Decision>,
    /// Prompts held back while the turn runs, newest first: they pile up
    /// above the line, the latest on top.
    queued: Vec<&'a str>,
    empty: bool,
    /// How many files the draft holds: what the flat line counts while
    /// the shelf (`attachments`) waits for the live Composer.
    files: usize,
    attachments: Option<AnyElement>,
    /// Running background tasks as chips, hung at the right edge of the
    /// same shelf the pending files sit on.
    background: Option<AnyElement>,
    menu: Option<AnyElement>,
    /// A draft's Composer: nothing exists to steer yet.
    draft: bool,
    draft_error: Option<SharedString>,
    /// The follow-up predicted for this Thread's last response, if one has
    /// landed. The idle line shows it verbatim and Tab accepts it.
    suggestion: Option<&'a str>,
    /// The working line (`working_line`), the stack's first row while a turn
    /// runs.
    working: Option<AnyElement>,
    /// Whether this Pane holds the keyboard: the line's debug name, and on
    /// a board whether its Composer is live.
    focused: bool,
    /// Native files hover the Pane: the band's edge is `DROP_EDGE`,
    /// saying where they will land.
    drop_target: bool,
    /// A docked Decision sits flush on top. The band is a band either way
    /// (no box to join); kept so the levels hand the Composer one shape.
    #[allow(dead_code)]
    joined: bool,
    /// The Solo box's toolbar, wired in the cockpit (`+`, a draft's chips,
    /// the mode, model and effort chips, send); on a board, the focused
    /// Pane's send circle.
    toolbar: Option<AnyElement>,
    /// The queued row's close button, wired to ⌫'s act in the cockpit.
    unqueue: Option<AnyElement>,
}

/// The Composer stack (theme WP-D): a board cell's compact box
/// (`compact_composer`), Solo's box (`box_composer`, SPEC §5.5). The shelf —
/// pending files at left, background tasks at right — and the working line
/// stand above either. The Pane lays the stack out `flex_shrink_0` below
/// the body, so the transcript gives way. The Decision row is **not**
/// here: it is the transcript's tail. While a Decision pends the input
/// carries the `Decision` key context, so its keys answer with the
/// keyboard in the Composer (#23).
fn composer_region(view: &PaneView, transcript: Option<&Transcript>, stack: ComposerStack) -> Div {
    if stack.grid {
        compact_composer::compact_composer(view, transcript, stack)
    } else {
        box_composer(view, transcript, stack)
    }
}

/// The Solo Composer (SPEC §5.5): a box `COMPOSER_INSET` inside the card's
/// sides and foot — `paint::FIELD` at `R_PANE`, its `FIELD_RING` laid
/// outside it — holding the queued row, the input (Geist, no `❯`) and the
/// toolbar the cockpit wires.
fn box_composer(view: &PaneView, transcript: Option<&Transcript>, stack: ComposerStack) -> Div {
    let ComposerStack {
        grid: _,
        decision,
        queued,
        empty,
        files: _,
        attachments,
        background,
        menu,
        draft,
        draft_error,
        suggestion,
        working,
        focused,
        drop_target,
        joined: _,
        toolbar,
        unqueue,
    } = stack;
    let geometry = view.geometry.clone();
    let namespace = view.text_namespace();
    let ring = |ink: u32| components::outer_ring(theme::R_PANE, rgba(ink));
    let field = div()
        .debug_selector(|| "composer-block".into())
        .when(drop_target, |field| {
            field.debug_selector(|| "composer-drop-target".into())
        })
        .relative()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .min_w_0()
        .mx(px(theme::COMPOSER_INSET))
        .rounded(px(theme::R_PANE))
        .bg(theme::paint::FIELD)
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(TEXT))
        .when(decision.is_some(), |field| field.key_context("Decision"))
        .child(ring(theme::FIELD_RING));
    let mut field = components::on_bounds(field, move |bounds, _, _| {
        geometry.set(PaneGeometry {
            composer: Some(bounds),
            ..geometry.get()
        })
    });
    // Files hovering the Pane: the box's edge in `DROP_EDGE`.
    if drop_target {
        field = field.child(ring(theme::DROP_EDGE));
    }
    if let Some(row) = queued_row(&queued, &namespace, unqueue) {
        field = field.child(row);
    }
    if let Some(error) = draft_error {
        field = field.child(
            div()
                .flex_shrink_0()
                .min_w_0()
                .px(px(theme::COMPOSER_INPUT_PAD_X))
                .pt(px(theme::COMPOSER_INPUT_PAD_B))
                .whitespace_normal()
                .text_color(rgb(BLOCKED))
                .child(error),
        );
    }
    // The one line that grows: `COMPOSER_ROW_H` per visual row. The
    // placeholder overlays its first row while the line is empty.
    let mut line = div()
        .debug_selector(move || {
            if focused {
                "focused-prompt-editor"
            } else {
                "prompt-editor"
            }
            .into()
        })
        .relative()
        .flex_1()
        .min_w_0()
        .font_family(theme::FONT_UI)
        .font_weight(theme::W_BODY)
        .text_size(px(theme::FS_COMPOSER))
        .line_height(px(theme::COMPOSER_ROW_H))
        .text_color(rgb(TEXT_STRONG))
        .child(view.composer.clone());
    if empty {
        let ghost = placeholder(decision.is_some(), draft, transcript, suggestion);
        line = line.child(ghost_row(ghost, 0.));
    }
    field = field.child(
        div()
            .debug_selector(move || format!("composer-live-{namespace}"))
            .flex()
            .items_start()
            .min_h(px(theme::COMPOSER_ROW_H))
            .min_w_0()
            .pt(px(theme::COMPOSER_INPUT_PAD_T))
            .px(px(theme::COMPOSER_INPUT_PAD_X))
            .pb(px(theme::COMPOSER_INPUT_PAD_B))
            .child(line),
    );
    field = field.children(toolbar);
    // The `/` or `@` popover paints above the stack — deferred, so it
    // escapes the Pane's clip and draws over the transcript (#24) — from
    // the box's left edge, at most `SLASH_MENU_W` wide.
    if let Some(menu) = menu {
        field = field.child(deferred(
            div()
                .absolute()
                .bottom(relative(1.))
                .left_0()
                .right_0()
                .mb(px(theme::FLOAT_OFFSET))
                .child(div().max_w(px(theme::SLASH_MENU_W)).child(menu)),
        ));
    }
    composer_stack(attachments, background, working, true)
        .pb(px(theme::COMPOSER_INSET))
        .child(field)
}

/// What stands over either Composer: on top the working line (`.comp >
/// .r`, SPEC §5.4: the stack's top row), on the transcript rows' axis,
/// `COMPACT_WORKING_GAP` (Solo: `COMPOSER_WORKING_GAP_SOLO`) over what
/// follows; then, `SHELF_GAP` over the box it belongs to, the shelf —
/// pending files at left, background chips at right; the chips give way
/// first, cutting their labels. The shelf keeps to its box's inset
/// (`boxed`: Solo's).
fn composer_stack(
    attachments: Option<AnyElement>,
    background: Option<AnyElement>,
    working: Option<AnyElement>,
    boxed: bool,
) -> Div {
    let (shelf_l, shelf_r) = if boxed {
        (theme::COMPOSER_INSET, theme::COMPOSER_INSET)
    } else {
        (theme::COMPACT_COMPOSER_INSET, theme::COMPACT_COMPOSER_INSET)
    };
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .min_w_0()
        .when_some(working, |stack, working| {
            stack.child(
                div()
                    .debug_selector(|| "transcript-progress".into())
                    .flex_shrink_0()
                    .min_w_0()
                    .px(px(theme::tx_inset(boxed)))
                    .mb(px(if boxed {
                        theme::COMPOSER_WORKING_GAP_SOLO
                    } else {
                        theme::COMPACT_WORKING_GAP
                    }))
                    .child(working),
            )
        })
        .when(attachments.is_some() || background.is_some(), |stack| {
            stack.child(
                div()
                    .debug_selector(|| "composer-shelf".into())
                    .flex()
                    .items_end()
                    .gap(px(theme::SPACE_2))
                    .min_w_0()
                    .pl(px(shelf_l))
                    .pr(px(shelf_r))
                    .pb(px(theme::SHELF_GAP))
                    .when_some(attachments, |shelf, attachments| {
                        shelf.child(div().flex_1().min_w_0().child(attachments))
                    })
                    .when_some(background, |shelf, chips| {
                        shelf.child(div().ml_auto().min_w_0().max_w_full().child(chips))
                    }),
            )
        })
}

/// The Solo box's queued row (`QUEUED_ROW_H`, a `HAIRLINE` under it):
/// `Queued`, the top of the pile on one cut line — the prompt ↑ takes back
/// and ⌫ drops — `+N` for the rest, and the close button that drops it.
/// The keys act as ever; they are not printed. None with nothing queued.
fn queued_row(queued: &[&str], namespace: &str, close: Option<AnyElement>) -> Option<Div> {
    let latest = queued.first()?;
    let more = queued.len() - 1;
    let selector = format!("composer-queue-{namespace}");
    Some(
        div()
            .debug_selector(move || selector.clone())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(theme::QUEUED_GAP))
            .h(px(theme::QUEUED_ROW_H + 1.))
            .min_w_0()
            .pl(px(theme::QUEUED_PAD_L))
            .pr(px(theme::QUEUED_PAD_R))
            .border_b_1()
            .border_color(theme::paint::HAIRLINE)
            .whitespace_nowrap()
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(theme::FS_META))
                    .font_weight(theme::W_LABEL)
                    .text_color(rgb(TEXT_DIM))
                    .child("Queued"),
            )
            .child(
                div()
                    .debug_selector(|| "queued-0".into())
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(TEXT_MUTED))
                    .child(SharedString::from(queued_preview(latest))),
            )
            .when(more > 0, |row| {
                row.child(components::tabular(
                    div()
                        .debug_selector(|| "queued-more".into())
                        .flex_shrink_0()
                        .text_color(rgb(TEXT_DIM))
                        .child(format!("+{more}")),
                ))
            })
            .children(close),
    )
}

/// The queued row's close button: a `QUEUED_CLOSE` square at `R_TAB`, the
/// `close` icon in `TEXT_DIM`, `paint::RAISED` under the pointer. The
/// cockpit wires its click.
pub fn queued_close(cx: &gpui::App) -> gpui::component::button::Button {
    chip_button("queued-close", cx)
        .debug_selector(|| "queued-close".into())
        .p_0()
        .size(px(theme::QUEUED_CLOSE))
        .flex_shrink_0()
        .rounded(px(theme::R_TAB))
        .tip("Remove queued message")
        .child(icon(icons::CLOSE, theme::QUEUED_CLOSE_ICON, TEXT_DIM))
}

/// A quiet control's button: no ground at rest, `paint::RAISED` under the
/// pointer over the one 150ms blend, `paint::PRESS` pressed at once.
fn chip_button(id: impl Into<gpui::ElementId>, cx: &gpui::App) -> gpui::component::button::Button {
    components::faded_button(
        id,
        gpui::rgba(theme::TRANSPARENT).into(),
        theme::paint::RAISED.into(),
        theme::paint::PRESS.into(),
        rgb(TEXT_DIM).into(),
        cx,
    )
}

/// The idle line's placeholder: a couple of words in `TEXT_DIM` after
/// the caret cell and a space, cut at their end only where even they do not
/// fit. The keys live behind `?` (ADR 0014), not on the line.
///
/// A predicted follow-up is already in the operator's voice and already
/// filtered, so it is shown verbatim — a draft of their next prompt, not a
/// description of one — with `⇥ accept`, which is always kept whole: an
/// accept key nobody knows about is the same as no accept key, so the
/// prediction's own words give way to it.
#[derive(Clone, Debug, PartialEq)]
struct Ghost {
    head: SharedString,
    /// A verbatim prediction, followed by its accept key.
    accept: bool,
}

impl Ghost {
    fn says(head: &'static str) -> Self {
        Self {
            head: head.into(),
            accept: false,
        }
    }

    /// The whole line, as the tests read it.
    #[cfg(test)]
    fn text(&self) -> String {
        if self.accept {
            format!("{} \u{b7} {ACCEPT_HINT}", self.head)
        } else {
            self.head.to_string()
        }
    }
}

/// What follows a verbatim prediction.
const ACCEPT_HINT: &str = "\u{21e5} accept";

/// The placeholder for this line: a draft's first prompt, a Decision's
/// reply, a dead Session's revival, a landed prediction, a follow-up queued
/// behind a running turn, or steering a live Thread.
fn placeholder(
    pending: bool,
    draft: bool,
    transcript: Option<&Transcript>,
    suggestion: Option<&str>,
) -> Ghost {
    if draft {
        return Ghost::says("Start a thread");
    }
    let busy = transcript.is_some_and(|transcript| transcript.status() == Status::Streaming);
    match followup::suggest(pending, transcript, suggestion) {
        // A docked Decision owns the block above this line; the line itself
        // still steers, so it says so (rule 2.8.1: one input line).
        Followup::Decision => Ghost::says("Answer above"),
        Followup::Revive => Ghost::says("Revive and continue"),
        Followup::Suggested(text) => Ghost {
            head: SharedString::from(text),
            accept: true,
        },
        Followup::Steer if busy => Ghost::says("Queue a follow-up"),
        Followup::Steer => Ghost::says("Steer this thread"),
    }
}

/// `flat_facts`' words: empty when there is nothing to say.
pub(crate) fn flat_facts_text(files: usize, queued: usize) -> String {
    let mut facts = Vec::new();
    if files > 0 {
        facts.push(format!(
            "{files} attachment{}",
            if files == 1 { "" } else { "s" }
        ));
    }
    if queued > 0 {
        facts.push(format!("{queued} queued"));
    }
    facts.join(" \u{b7} ")
}

/// The placeholder drawn on a clipped row one line high, `left` in (Solo's
/// box: at the text's start; a board's band: two cells, after the caret's
/// cell and a space). A prediction lets its own words
/// truncate before the accept hint, which never goes.
fn ghost_row(ghost: Ghost, left: f32) -> Div {
    div()
        .debug_selector(|| "prompt-placeholder".into())
        .absolute()
        .left(px(left))
        .right_0()
        .top_0()
        .h(px(theme::COMPOSER_ROW_H))
        .flex()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_color(rgb(TEXT_DIM))
        .child(
            div()
                .flex_shrink(1.)
                .min_w_0()
                .h(px(theme::COMPOSER_ROW_H))
                .truncate()
                .child(ghost.head),
        )
        .when(ghost.accept, |row| {
            // The placeholder is one muted run, its `·` too (the
            // prototype's `.ph-text`).
            row.child(
                div()
                    .debug_selector(|| "prompt-placeholder-hint".into())
                    .flex()
                    .flex_shrink_0()
                    .h(px(theme::COMPOSER_ROW_H))
                    .child(
                        components::cells("\u{b7}")
                            .px(px(theme::CH))
                            .w(px(3.0 * theme::CH)),
                    )
                    .child(components::cells(ACCEPT_HINT)),
            )
        })
}

/// #11: whether this Thread still offers adopting a CLI session — no
/// conversation yet (nothing in the transcript beyond Ferrite's own notices
/// and bookkeeping) and at rest. One predicate for every surface that opens
/// the door — the placeholder hint, the import picker's choice of the
/// Thread's place, and the pick that closes the blank Thread — so no two
/// can disagree.
pub fn offers_import(transcript: Option<&Transcript>) -> bool {
    transcript.is_some_and(Transcript::offers_import)
}

/// The mode chip's label (SPEC §5.5): the Session's own label for `mode`
/// when its list has one (`permission_modes`, already sentence case); else
/// the mode's word (`theme::mode_word`), sentence case; `Default` at the
/// default. Never a raw id.
pub fn permission_mode_label(
    mode: &str,
    choices: &[ferrite_core::PermissionModeChoice],
) -> SharedString {
    if let Some(choice) = choices.iter().find(|choice| choice.value == mode) {
        return choice.label.clone().into();
    }
    theme::mode_word(mode).map_or_else(|| "Default".into(), |word| sentence_case(&word).into())
}

/// One row of the `/` or `@` popover, ready to draw: what a pick inserts,
/// what the row shows, and where the fuzzy filter matched. Prepared by the
/// cockpit when the menu changes — never per frame.
pub struct MenuRow {
    /// What lands in the line on ↵ — a command name, or a file's relative
    /// path.
    pub insert: SharedString,
    /// The row's leading text: `/name`, or the file's name.
    pub name: SharedString,
    /// Matched byte ranges inside `name`, painted `TEXT_STRONG` at `W_STRONG`
    /// (`components::match_highlights`; one face, so nothing reflows).
    pub matched: Vec<std::ops::Range<usize>>,
    /// The dimmer text after it: a command's description, or the file's
    /// directory. Empty draws nothing.
    pub detail: SharedString,
    /// Whether `detail` is Ferrite's description (cut at its end) or
    /// machine text such as a path (cut at its head so the useful tail
    /// survives).
    pub prose_detail: bool,
    /// A row kept visible but dead (#25's locked provider door): muted ink,
    /// no match highlights, and its pick does nothing but dismiss.
    pub inert: bool,
}

/// The Composer menus' popover: the float, as wide as its slot (at most
/// `SLASH_MENU_W`), capped at `MENU_MAX_H` (its row list scrolls past
/// that).
pub fn menu_popover() -> Div {
    crate::menu::float().w_full().max_h(px(theme::MENU_MAX_H))
}

/// A `/` or `@` row in the float grammar: the cursor row's fill, the name with
/// its matches in `TEXT_STRONG`, the detail muted in its own column
/// (`label_w` aligns the slash commands' descriptions), `↵` on the cursor
/// row.
pub fn menu_row(
    id: impl Into<gpui::ElementId>,
    row: &MenuRow,
    cursor: bool,
    label_w: Option<f32>,
) -> Stateful<Div> {
    // A description or a path cut at the popover's width keeps its whole
    // text one hover away.
    let detail = (!row.detail.is_empty()).then(|| row.detail.clone());
    crate::menu::item_row(id, &menu_item(row, cursor, label_w), cursor, false)
        .when_some(detail, |row, detail| {
            row.tooltip(crate::menu::tooltip(detail))
        })
}

/// A `MenuRow` as the shared menu row's content.
pub fn menu_item(row: &MenuRow, cursor: bool, label_w: Option<f32>) -> components::MenuItem {
    let mut item = components::MenuItem::new(row.name.clone())
        .matched(row.matched.clone())
        .disabled(row.inert);
    if let Some(width) = label_w {
        item = item.label_w(width);
    }
    if !row.detail.is_empty() {
        let detail = if row.prose_detail {
            row.detail.clone()
        } else {
            head_truncated(&row.detail, theme::MENU_PATH_TAIL)
        };
        item = item.detail(detail).mono(!row.prose_detail);
    }
    // No ↵ on an inert row: enter only dismisses there, and the key would
    // advertise an offer the row does not make.
    if cursor && !row.inert {
        item = item.shortcut("↵");
    }
    item
}

/// A path cut at its head to about its last `tail` characters, behind
/// `…/`: gpui truncates only at the end, where a path keeps what matters.
fn head_truncated(path: &SharedString, tail: usize) -> SharedString {
    let count = path.chars().count();
    if count <= tail {
        return path.clone();
    }
    let rest: String = path.chars().skip(count - tail).collect();
    let rest = rest.split_once('/').map_or(rest.as_str(), |(_, rest)| rest);
    format!("…/{rest}").into()
}

/// The queue's share of the editor's pane-height budget: Solo's one
/// queued row and its hairline whenever anything is queued; nothing in a
/// board's compact box, which is input only.
pub(crate) fn composer_queue_height(_height: f32, grid: bool, count: usize) -> f32 {
    if grid || count == 0 {
        0.
    } else {
        theme::QUEUED_ROW_H + 1.
    }
}

/// The Composer's height less its editor rows and queue, the same at every
/// level, focused or not. Solo's box: the input's padding, the toolbar and
/// the inset under it (its ring lies outside it). A board's compact box:
/// the air around its row and the inset under it. The shelf and the
/// working line stand above and are not part of it.
fn composer_fixed_height(grid: bool) -> f32 {
    if grid {
        theme::COMPACT_COMPOSER_H - theme::COMPOSER_ROW_H + theme::COMPACT_COMPOSER_INSET
    } else {
        theme::COMPOSER_INPUT_PAD_T
            + theme::COMPOSER_INPUT_PAD_B
            + theme::COMPOSER_TOOLBAR_H
            + theme::COMPOSER_INSET
    }
}

/// Leave the majority of a Pane available for its Thread context. Only the
/// viewport changes: the Composer keeps every character and scrolls to its
/// caret, then reveals more rows again when the Pane grows.
pub(crate) fn composer_row_limit(height: f32, grid: bool, queued: usize) -> usize {
    let fixed = composer_fixed_height(grid);
    let queue = composer_queue_height(height, grid, queued);
    ((height * theme::COMPOSER_MAX_PANE_FRACTION - fixed - queue) / theme::COMPOSER_ROW_H)
        .floor()
        .max(1.)
        .min(crate::composer::MAX_ROWS as f32) as usize
}

/// A held prompt as one line: the queued row is a single line and
/// gpui's truncation still breaks at `\n`, so a multi-line prompt would
/// stack its lines into the row and clip them both.
fn queued_preview(held: &str) -> String {
    held.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text of an approval's command well: a command's source, else the
/// input as a string, else its pretty JSON. `None` for a question or an
/// input-less request.
pub(crate) fn approval_source(decision: &Decision) -> Option<String> {
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
pub(crate) fn approval_diff(
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
                .child(render_diff(None, diff, Grid::of(reading), false))
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
            .child(render_diff(None, &diff, Grid::of(reading), false))
            .into_any_element(),
    )
}

/// Whether an approval's well holds a shell command, which reads after a
/// `$ ` prompt: Claude's `Bash` and Codex's `commandExecution` alike.
pub(crate) fn shell_command(decision: &Decision) -> bool {
    decision::shell_source(decision).is_some()
}

/// The exact tool input an approval would send. A shell command reads as
/// the operator would type it — Codex's login-shell wrapper (`/bin/zsh -lc
/// "…"`) taken off, so both providers read `gh issue close 212`; other
/// provider input remains inspectable as its JSON value.
pub(crate) fn approval_input(
    decision: &Decision,
    cache: &crate::rich::TextCache,
    id: SharedString,
) -> Option<AnyElement> {
    use gpui::component::scroll::ScrollableElement as _;

    let source = match decision::shell_source(decision) {
        Some(command) => command.into_owned(),
        None => approval_source(decision)?,
    };
    Some(
        div()
            .debug_selector(|| "approval-input".into())
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

/// A subscription window's plausible Unix reset instant in compact, useful
/// units. Providers disagree on the field's units, so only a future value
/// inside the window's own maximum span is safe to present as a countdown.
fn reset_label(resets_at: Option<u64>, span: Duration, now: SystemTime) -> Option<SharedString> {
    let now = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    // A reset the provider did not report, one already past, or one
    // outside the window's span reads nothing: a guess is worse than none.
    let remaining = resets_at?
        .checked_sub(now)
        .filter(|remaining| (1..=span.as_secs()).contains(remaining))?;
    let label = match remaining {
        0..=59 => "resets in <1m".into(),
        60..=3_599 => format!("resets in {}m", remaining / 60),
        3_600..=86_399 => {
            let hours = remaining / 3_600;
            let minutes = remaining % 3_600 / 60;
            if minutes == 0 {
                format!("resets in {hours}h")
            } else {
                format!("resets in {hours}h {minutes}m")
            }
        }
        86_400.. => {
            let days = remaining / 86_400;
            let hours = remaining % 86_400 / 3_600;
            if hours == 0 {
                format!("resets in {days}d")
            } else {
                format!("resets in {days}d {hours}h")
            }
        }
    };
    Some(SharedString::from(label))
}

/// A card's refusal, the one line grammar (rule 2.11.3): `failed` in
/// `BLOCKED`, the `·` in structure ink, then the message in `TEXT`, all
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
        .text_color(rgb(TEXT))
        .whitespace_normal()
        .child(StyledText::new(text).with_highlights(runs))
}

/// A token count as the card reads it (rule 2.11.6): only as precise as it
/// needs to be — `640`, `1.5k`, `64k`, `1.2M`.
pub(crate) fn compact_count(count: u64) -> String {
    fn trimmed(value: f64) -> String {
        let text = format!("{value:.1}");
        text.strip_suffix(".0").unwrap_or(&text).to_owned()
    }
    match count {
        0..=999 => count.to_string(),
        1_000..=9_999 => format!("{}k", trimmed(count as f64 / 1_000.)),
        10_000..=999_999 => format!("{}k", (count as f64 / 1_000.).round() as u64),
        _ => format!("{}M", trimmed(count as f64 / 1_000_000.)),
    }
}

/// A turn's cost as the card reads it: cents, or `<$0.01` below one.
pub(crate) fn cost_label(cost: f64) -> String {
    if cost < 0.005 {
        "<$0.01".into()
    } else {
        format!("${cost:.2}")
    }
}

/// A context category's ink in the card's stacked bar and in its legend
/// (`CTX_*`: the palette's own quiet hues). The categories a provider
/// usually reports keep one ink each; any other name takes the next from a
/// fixed cycle, so a new category still reads as distinct.
fn category_ink(name: &str, index: usize) -> u32 {
    let name = name.to_ascii_lowercase();
    if name.contains("deferred") {
        theme::CTX_DEFERRED
    } else if name.contains("free") {
        theme::CTX_FREE
    } else if name.contains("buffer") || name.contains("compact") {
        theme::CTX_BUFFER
    } else if name.contains("message") {
        theme::CTX_MESSAGES
    } else if name.contains("mcp") {
        theme::CTX_MCP
    } else if name.contains("tool") {
        theme::CTX_TOOLS
    } else if name.contains("skill") {
        theme::CTX_SKILLS
    } else if name.contains("prompt") {
        theme::CTX_PROMPT
    } else if name.contains("memory") {
        theme::CTX_MEMORY
    } else {
        theme::CTX_CYCLE[index % theme::CTX_CYCLE.len()]
    }
}

/// Free space and deferred tools are reported beside the window's
/// contents but take none of it: they are listed, never drawn in the bar.
fn category_in_window(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    !name.contains("free") && !name.contains("deferred")
}

/// What a context bar draws: each category in the window that holds
/// tokens, its ink and its share of `denominator`, in the provider's order.
pub(crate) fn context_segments(
    categories: &[ferrite_core::ContextCategory],
    denominator: u64,
) -> Vec<(u32, f32)> {
    if denominator == 0 {
        return Vec::new();
    }
    categories
        .iter()
        .enumerate()
        .filter(|(_, category)| category_in_window(&category.name) && category.tokens > 0)
        .map(|(index, category)| {
            (
                category_ink(&category.name, index),
                category.tokens as f32 / denominator as f32,
            )
        })
        .collect()
}

/// The panel's Context tab (SPEC §5.6): the context window and the
/// account windows, in that order, each a labelled bar over the reading
/// behind it. Counts are reported values, never estimates. An account
/// window the provider has not reported is left out; when it reports
/// neither, one line says so (`Limits not reported by this provider`) in
/// place of two empty bars. Each block holds the rows' inset
/// (the panel's `PANEL_ROW_ICON_X`).
///
/// The context bar is stacked by category when the provider says what
/// fills the window, and its heading is a row that opens the legend: every
/// category with its ink, its count and its share of the window. The tab
/// grows to `max_h` (the panel's room) and scrolls past it.
#[allow(clippy::too_many_arguments)]
pub fn context_usage(
    usage: ferrite_core::transcript::Usage,
    limits: ferrite_core::transcript::RateLimits,
    details: Option<&ferrite_core::ContextDetails>,
    usage_details: Option<&ferrite_core::UsageDetails>,
    last_cost: Option<f64>,
    expanded: bool,
    max_h: f32,
    on_toggle: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let maximum = usage.context_window.filter(|limit| *limit > 0);
    let now = ferrite_core::clock::system_time();
    // One 4px bar, full width, on the meter's track in its status ink.
    let bar = |fraction: f32| {
        let used = fraction.clamp(0., 1.);
        div()
            .w_full()
            .h(px(theme::USAGE_CARD_BAR_H))
            .bg(theme::paint::LINE2)
            .child(div().h_full().w(relative(used)).bg(rgb(usage_ink(used))))
    };
    // A window's heading: its name at the left, what it reads at the
    // right — the one line that answers the question at a glance. The
    // name is a row's label, not a title: `W_BODY`.
    let heading = |label: &'static str, value: AnyElement| {
        div()
            .flex()
            .items_baseline()
            .justify_between()
            .gap(px(theme::SPACE_3))
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(theme::W_BODY)
                    .text_color(rgb(TEXT))
                    .child(label),
            )
            .child(value)
    };
    let percent_value = |key: &'static str, fraction: f32| {
        let percent = (fraction.clamp(0., 1.) * 100.).round() as u32;
        div()
            .id(key)
            .debug_selector(move || format!("context-usage-{key}-{percent}"))
            .flex_shrink_0()
            .child(SharedString::from(format!("{percent}%")))
    };
    let reset_value = |key: &'static str, resets_at: Option<u64>, span: Duration| {
        reset_label(resets_at, span, now).map(|label| {
            div()
                .id(SharedString::from(format!("reset-{key}")))
                .text_size(px(theme::FS_UI))
                .debug_selector(move || format!("context-usage-{key}-reset-reported"))
                .text_color(rgb(TEXT_DIM))
                .child(label)
                .into_any_element()
        })
    };
    let block = || {
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(theme::USAGE_CARD_ROW_GAP))
            .px(px(theme::PANEL_ROW_ICON_X))
    };
    let window =
        |label: &'static str, key: &'static str, fraction: f32, detail: Option<AnyElement>| {
            block()
                .child(heading(
                    label,
                    percent_value(key, fraction).into_any_element(),
                ))
                .child(bar(fraction))
                .children(detail)
        };
    let categories = details.map_or(&[][..], |details| details.categories.as_slice());
    let expandable = details.is_some_and(|details| {
        !details.categories.is_empty()
            || details.usable_window.is_some()
            || details.auto_compact_threshold.is_some()
    });
    // Shares are of the whole window when the provider reports one; of
    // what the categories add up to when it does not.
    let denominator = maximum.unwrap_or_else(|| {
        categories
            .iter()
            .filter(|category| category_in_window(&category.name))
            .map(|category| category.tokens)
            .sum()
    });
    let share_of = |tokens: u64| -> f32 {
        if denominator == 0 {
            0.
        } else {
            tokens as f32 / denominator as f32
        }
    };
    // The counts behind the context bar, in the card's quietest ink: the
    // bar says how full, this says of what — `64k / 200k tokens`. When
    // there is a legend behind it, a chevron after the counts says so.
    let current = usage.total_tokens;
    let counts = components::tabular(
        div()
            .id("context-usage-counts")
            .flex()
            .items_center()
            .gap(px(theme::SPACE_1))
            .text_size(px(theme::FS_UI))
            .text_color(rgb(TEXT_DIM))
            .debug_selector(move || match maximum {
                Some(maximum) => format!("context-usage-maximum-{maximum}"),
                None => "context-usage-maximum-unknown".into(),
            })
            .child(
                div()
                    .debug_selector(move || format!("context-usage-current-{current}"))
                    .child(SharedString::from(match maximum {
                        Some(maximum) => format!(
                            "{} / {} tokens",
                            compact_count(current),
                            compact_count(maximum)
                        ),
                        None => format!("{} tokens", compact_count(current)),
                    })),
            ),
    );
    let context_fraction = maximum.map(|maximum| current as f32 / maximum as f32);
    // The bar: stacked by category when the provider says what fills the
    // window, one status-ink fill when it only reports the total.
    let in_window = context_segments(categories, denominator);
    let context_bar = if !in_window.is_empty() {
        Some(
            div()
                .flex()
                .w_full()
                .h(px(theme::USAGE_CARD_BAR_H))
                .overflow_hidden()
                .bg(theme::paint::LINE2)
                .children(in_window.into_iter().map(|(ink, share)| {
                    div()
                        .flex_shrink_0()
                        .h_full()
                        .w(relative(share.clamp(0., 1.)))
                        .bg(rgb(ink))
                }))
                .into_any_element(),
        )
    } else {
        // No window to divide by: the count alone, no empty bar.
        context_fraction.map(|fraction| bar(fraction).into_any_element())
    };
    // The context heading is the legend's door when there is one: the
    // whole line is the hit row, lifting to the hover face like a menu row,
    // with the chevron after the reading.
    let context_heading = heading(
        "Context",
        div()
            .flex()
            .items_center()
            .gap(px(theme::SPACE_1))
            .children(context_fraction.map(|fraction| percent_value("context", fraction)))
            // One chevron that turns a quarter as the legend opens, over
            // `motion::CHEVRON` — the disclosures' turn.
            .when(expandable, |reading| {
                reading.child(crate::motion::settled(
                    "context-legend-turn",
                    expanded,
                    crate::motion::CHEVRON,
                    |turn| {
                        icon(icons::CHEVRON_RIGHT, theme::ICON_CHEVRON, TEXT_DIM)
                            .with_transformation(gpui::Transformation::rotate(gpui::radians(
                                std::f32::consts::FRAC_PI_2 * turn,
                            )))
                    },
                ))
            })
            .into_any_element(),
    );
    let context_heading = if expandable {
        let id = gpui::ElementId::from("context-window-toggle");
        let key = crate::pointer::hover_key(&id);
        div()
            .id(id)
            .debug_selector(|| "context-window-toggle".into())
            .mx(px(-theme::PANEL_ROW_ICON_X))
            .px(px(theme::PANEL_ROW_ICON_X))
            .cursor_pointer()
            .hover_raised(key)
            .press_raised()
            .on_click(on_toggle)
            .child(context_heading)
            .into_any_element()
    } else {
        context_heading.into_any_element()
    };
    let mut context = block()
        .child(context_heading)
        .children(context_bar)
        .child(counts);
    // The legend: an ink square, the name, the count muted and the share
    // at the right edge, in tabular figures so the column lines up.
    let legend = |selector: String,
                  swatch: Option<u32>,
                  label: SharedString,
                  count: String,
                  share: SharedString| {
        components::tabular(
            div()
                .id(SharedString::from(selector.clone()))
                .debug_selector(move || selector.clone())
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(theme::SPACE_2))
                .h(px(theme::USAGE_LEGEND_ROW_H))
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(theme::USAGE_SWATCH))
                        .rounded(px(theme::R_TIGHT))
                        .when_some(swatch, |square, ink| square.bg(rgb(ink))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(TEXT))
                        .child(label),
                )
                .child(div().flex_shrink_0().text_color(rgb(TEXT_DIM)).child(count))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .flex_shrink_0()
                        .w(px(theme::USAGE_SHARE_W))
                        .text_color(rgb(TEXT))
                        .child(share),
                ),
        )
    };
    if let Some(details) = details.filter(|_| expanded && expandable) {
        let mut rows = div().flex().flex_col().pt(px(theme::SPACE_1));
        for (index, category) in details.categories.iter().enumerate() {
            let tokens = category.tokens;
            // Deferred tools are loaded on demand: listed, but no share.
            let share = if category.name.to_ascii_lowercase().contains("deferred") {
                "\u{2014}".into()
            } else {
                format!("{:.1}%", share_of(tokens) * 100.)
            };
            rows = rows.child(legend(
                format!("context-category-{index}-{tokens}"),
                Some(category_ink(&category.name, index)),
                sentence_case(&category.name).into(),
                compact_count(tokens),
                share.into(),
            ));
        }
        if let Some(usable) = details.usable_window {
            rows = rows.child(legend(
                format!("context-usable-{usable}"),
                None,
                "Usable window".into(),
                compact_count(usable),
                "".into(),
            ));
        }
        if let Some(threshold) = details.auto_compact_threshold {
            rows = rows.child(legend(
                format!("context-compaction-{threshold}"),
                None,
                "Auto-compact at".into(),
                compact_count(threshold),
                match details.is_auto_compact_enabled {
                    Some(false) => "off".into(),
                    _ => "".into(),
                },
            ));
        }
        // The legend's rows fade in as it opens (`ROW_IN`, opacity only);
        // closing is instant, like every exit.
        context = context.child(crate::motion::fade_in(
            "context-legend-in",
            rows,
            crate::motion::ROW_IN,
            1.0,
        ));
    }
    let mut card = div()
        .id("context-usage-body")
        .flex()
        .flex_col()
        .w_full()
        .gap(px(theme::USAGE_CARD_GAP))
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(TEXT))
        .child(context);
    if limits.five_hour.is_none() && limits.weekly.is_none() {
        card = card.child(
            crate::menu::note("limits not reported by this provider")
                .id("context-usage-limits-unknown")
                .debug_selector(|| "context-usage-limits-unknown".into()),
        );
    }
    if let Some(limit) = limits.five_hour {
        card = card.child(window(
            "5-hour limit",
            "five-hour",
            limit.used_fraction,
            reset_value("five-hour", limit.resets_at, Duration::from_secs(5 * 3_600)),
        ));
    }
    if let Some(limit) = limits.weekly {
        card = card.child(window(
            "Weekly limit",
            "weekly",
            limit.used_fraction,
            reset_value("weekly", limit.resets_at, Duration::from_secs(7 * 86_400)),
        ));
    }
    // Everything below the windows is a terminal readout: a quiet key at
    // the left, the reported value right-aligned in tabular digits, and a
    // section head where the scope changes.
    let row = |key: String, value: String| {
        div()
            .flex()
            .flex_shrink_0()
            .justify_between()
            .gap(px(theme::SPACE_3))
            .px(px(theme::PANEL_ROW_ICON_X))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(TEXT_DIM))
                    .child(key),
            )
            .child(components::tabular(div().flex_shrink_0().child(value)))
    };
    if let Some(details) = usage_details {
        let scope = match details.scope {
            ferrite_core::UsageScope::Message => ("message", "This message"),
            ferrite_core::UsageScope::Turn => ("turn", "This turn"),
            ferrite_core::UsageScope::Session => ("session", "This session"),
        };
        let mut section = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(theme::SPACE_0_5))
            .child(
                crate::menu::section(scope.1, None, None)
                    .debug_selector(move || format!("usage-scope-{}", scope.0)),
            );
        for (key, label, count) in [
            ("input", "Input", details.input_tokens),
            ("cached-input", "Cached input", details.cached_input_tokens),
            ("output", "Output", details.output_tokens),
            (
                "reasoning-output",
                "Reasoning output",
                details.reasoning_output_tokens,
            ),
        ] {
            section = section.child(
                row(label.into(), compact_count(count))
                    .debug_selector(move || format!("usage-{key}-{count}")),
            );
        }
        if let Some(cost) = last_cost {
            section = section.child(
                row("Last turn".into(), cost_label(cost))
                    .debug_selector(move || format!("usage-cost-{cost}")),
            );
        }
        card = card.child(section);
    } else if let Some(cost) = last_cost {
        card = card.child(
            row("Last turn".into(), cost_label(cost))
                .debug_selector(move || format!("usage-cost-{cost}")),
        );
    }
    // Counts, percentages and the cost tick while the card is open. Every
    // block holds its height (`flex_shrink_0`): once the open legend
    // outgrows the room the card scrolls rather than squeezing its rows.
    components::tabular(card)
        .max_h(px(max_h))
        .overflow_y_scrollbar()
}

/// A name in sentence case — a provider's category, a chip label (SPEC
/// §5.5, amending rule 11 for chip labels only) — keeping known acronyms'
/// capitals ("mcp tools" → "MCP tools", not "Mcp tools").
pub(crate) fn sentence_case(name: &str) -> String {
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

/// A usage reading's ink: neutral `TEXT` until the window runs tight
/// (`USAGE_TIGHT`), then `ATTENTION`. There is no `BLOCKED` step: a full
/// window stops nothing until the provider says so. Colour is state; a
/// context half full is not one.
pub fn usage_ink(fraction: f32) -> u32 {
    if fraction >= theme::USAGE_TIGHT {
        ATTENTION
    } else {
        TEXT
    }
}

/// The ctx meter's fill: `RUNNING` while the window has room, `ATTENTION`
/// from `CTX_METER_WARN`, `BLOCKED` from `CTX_METER_FULL` — the prototype's
/// green, yellow and red by fill.
pub fn ctx_meter_ink(fraction: f32) -> u32 {
    if fraction >= theme::CTX_METER_FULL {
        BLOCKED
    } else if fraction >= theme::CTX_METER_WARN {
        ATTENTION
    } else {
        RUNNING
    }
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

/// The ✓-row the pickers share — the band popovers (#29) — so "what this
/// Pane is on right now" can never be spelled two ways: the strong check on
/// the standing choice, the muted tag ("checked out", "worktree · dir")
/// after the label.
pub fn picker_row(
    id: impl Into<gpui::ElementId>,
    label: SharedString,
    detail: SharedString,
    cursor: bool,
    active: bool,
    inert: bool,
) -> Stateful<Div> {
    let mut item = components::MenuItem::new(label)
        .checked(active)
        .disabled(inert);
    if !detail.is_empty() {
        item = item.detail(detail);
    }
    crate::menu::item_row(id, &item, cursor, false)
}

/// A picker's section row: the Provider's logomark in its brand colour (the
/// one place brand colour is allowed) and its name, lowercase, with an
/// optional note after a `·` (`codex · handover`). Non-interactive — the
/// arrows skip it.
pub fn picker_section(provider: Provider, note: SharedString) -> Div {
    let (mark, ink, title) = match provider {
        Provider::Codex => (icons::CODEX, theme::PROVIDER_CODEX, "codex"),
        Provider::Claude => (icons::CLAUDE, theme::PROVIDER_CLAUDE, "claude"),
    };
    crate::menu::section(title, Some((mark, ink)), (!note.is_empty()).then_some(note))
}

/// The popover's key-hint footer, each menu supplying its own verbs.
pub fn popover_footer(hints: &[(&str, &str)]) -> Div {
    crate::menu::footer(hints)
}

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
pub(crate) fn reasoning_text(thought: &str) -> (String, Option<String>) {
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
pub(crate) struct Grid {
    pub size: f32,
    pub line: f32,
}

impl Grid {
    /// A Solo transcript's grid (and a reader's, a review's): the reading
    /// size on its line.
    pub(crate) fn of(reading: ferrite_core::settings::ReadingSize) -> Self {
        Self::sized(theme::answer_text_size(reading))
    }

    /// The grid of a Solo (`solo`) or a Group Pane's transcript
    /// (`theme::tx_size`).
    pub(crate) fn pane(reading: ferrite_core::settings::ReadingSize, solo: bool) -> Self {
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
    pub(crate) fn block(self) -> Self {
        Self::sized(self.size - theme::TX_BLOCK_STEP)
    }

    /// One cell of the grid.
    pub(crate) fn cell(self) -> f32 {
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
    pub(crate) fn half(self) -> f32 {
        theme::tx_half(self.line)
    }
}

/// A call whose result never came: the hollow `○`.
const RING: &str = "\u{25cb}";

/// A provider's brand colour, for its `✻` and its working spinner;
/// `TEXT_DIM` when the provider is not known.
pub(crate) fn provider_ink(provider: Option<Provider>) -> u32 {
    match provider {
        Some(Provider::Claude) => theme::PROVIDER_CLAUDE,
        Some(Provider::Codex) => theme::PROVIDER_CODEX,
        None => TEXT_DIM,
    }
}

/// A typed gutter glyph (`●`, `○`) in `ink`: the face's own, in the
/// gutter's first cell on the row's first line. Plain text, so it is never
/// part of a selection.
pub(crate) fn glyph_gutter(grid: Grid, glyph: &'static str, ink: u32) -> Div {
    div()
        .flex_shrink_0()
        .w(px(grid.gutter()))
        .h(px(grid.line))
        .overflow_hidden()
        .whitespace_nowrap()
        .font_weight(theme::W_BODY)
        .text_color(rgb(ink))
        .child(glyph)
}

/// A gutter mark (`❯ ✻ ◆`, typed; `∴`, drawn): centred on the row's first
/// line, at the gutter's left edge.
pub(crate) fn mark_gutter(grid: Grid, mark: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .w(px(grid.gutter()))
        .h(px(grid.line))
        .child(mark)
}

/// A transcript row: its gutter, then the content column (the caller's next
/// child, `flex_1 min_w_0`); wrapped lines hang under the content column.
fn grid_row(gutter: Div) -> Div {
    div().flex().items_start().w_full().min_w_0().child(gutter)
}

/// A row whose gutter is empty: its content starts on the content column.
fn content_row(grid: Grid) -> Div {
    div()
        .flex()
        .items_start()
        .w_full()
        .min_w_0()
        .pl(px(grid.gutter()))
}

/// A row that hangs under the one above it: the typed `└` (`TEXT_FAINT`)
/// in the content column's first two cells, the caller's text after it in
/// `ink`.
pub(crate) fn elbow_line(grid: Grid, ink: u32) -> Div {
    content_row(grid).text_color(rgb(ink)).child(
        div()
            .debug_selector(|| "transcript-elbow".into())
            .flex_shrink_0()
            .w(px(2.0 * grid.cell()))
            .whitespace_nowrap()
            .text_color(rgb(TEXT_FAINT))
            .child(ELBOW),
    )
}

/// A line under an elbow line, on the text after its elbow: more output, a
/// fold's `+ N lines`, a folded diff's `+ show diff`.
fn after_elbow(grid: Grid) -> Div {
    div()
        .flex()
        .items_start()
        .w_full()
        .min_w_0()
        .pl(px(grid.gutter() + 2.0 * grid.cell()))
}

/// Everything one transcript row reads besides its block: the selection its
/// runs register with, the text cache a pinned copy draws from, the calls'
/// clocks, the provider, the preview host, the grid, the layout width,
/// whether the Pane holds the keyboard (a live counter coarsens when it
/// does not), and the path targets' scope and checkout.
pub(crate) struct RowCx<'a> {
    pub selection: &'a TextRuns,
    pub rich: &'a crate::rich::TextCache,
    pub timings: Option<&'a HashMap<String, ToolTiming>>,
    pub provider: Option<Provider>,
    pub preview: &'a crate::preview::Preview,
    pub grid: Grid,
    /// The Pane stands alone (Solo), not on a Group's board.
    pub solo: bool,
    pub wide: bool,
    pub focused: bool,
    pub scope: &'a SharedString,
    pub workspace: Option<&'a std::path::Path>,
}

impl RowCx<'_> {
    fn grid(&self) -> Grid {
        self.grid
    }

    /// A path a call names, as a target hosted by this transcript.
    fn path_target(&self, shown: &str) -> crate::file_links::PathTarget {
        let path = std::path::Path::new(shown);
        let resolved = match self.workspace {
            Some(root) if !path.is_absolute() => root.join(path),
            _ => path.to_path_buf(),
        };
        crate::file_links::PathTarget::new(resolved)
            .shown(SharedString::from(shown.to_string()))
            .in_scope(self.scope.clone())
    }
}

/// A tool row's disclosure state: whether its details (or its output fold)
/// are open, the keyboard's target and toggle, and its diff's fold.
pub(crate) struct ToolCx {
    pub expanded: bool,
    pub disclosure: Option<Disclosure>,
    pub diff: DiffFold,
}

/// A tool row's diff: whether it folds behind `+ show diff` (a later edit
/// to a file already diffed in its turn), whether it is drawn, and the
/// toggle its fold line calls.
#[derive(Clone, Default)]
pub(crate) struct DiffFold {
    pub folds: bool,
    pub shown: bool,
    pub toggle: Option<DisclosureToggle>,
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
        Body::Prompt(line) => prompt_row(block, line, row_cx, None),
        // Fallback prose (a block with no Markdown source) reads as the
        // answer does, on the content column.
        Body::Paragraph { spans } => row
            .child(
                content_row(grid).text_color(rgb(TEXT)).child(
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
                    .text_color(rgb(TEXT_STRONG))
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
                    .text_color(rgb(TEXT))
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(2.0 * grid.cell()))
                            .text_color(rgb(TEXT_DIM))
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
        // `TEXT_DIM`, never italic. A short thought shows whole; a long one
        // is its first line with the rest disclosed.
        Body::Thinking(thought) => {
            let (summary, details) = reasoning_text(thought);
            let mark = || mark_gutter(grid, icon(icons::REASONING, grid.mark(), TEXT_FAINT));
            let Some(details) = details else {
                // Nothing more was supplied. Keep the whole short block
                // visible, wrapped and selectable without a false disclosure.
                return row
                    .child(
                        grid_row(mark()).text_color(rgb(TEXT_DIM)).child(
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
            row.text_color(rgb(TEXT_DIM)).child(body).into_any_element()
        }
        // A notice: a faint dot and one line in `TEXT_DIM`, cut by width at
        // the column's edge with the whole of it one hover away. A Decision
        // never leaves one (it is its own row, at the tail).
        Body::Notice(text) => {
            let highlights = separators(text);
            let text = SharedString::from(text.clone());
            row.child(
                grid_row(glyph_gutter(grid, BULLET, TEXT_FAINT))
                    .text_color(rgb(TEXT_DIM))
                    .child(
                        div()
                            .id(SharedString::from(format!("notice-{:?}", block.id)))
                            .debug_selector({
                                let id = block.id;
                                move || format!("notice-{id:?}")
                            })
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .tooltip(crate::menu::tooltip(text.clone()))
                            .child(selection.line(block.id, text, highlights)),
                    ),
            )
            .into_any_element()
        }
        // A decision record (`allowed Write`) or a revival note hangs under
        // the row it answers.
        Body::Meta(text) => row
            .child(elbow_line(grid, TEXT_DIM).child(
                div().flex_1().min_w_0().child(selection.line(
                    block.id,
                    text.clone(),
                    separators(text),
                )),
            ))
            .into_any_element(),
        Body::TurnEnd(end) => row
            .child(turn_end(block.id, end, selection, row_cx.provider, grid))
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
                        .border_color(theme::paint::LINE2)
                        .font_family(theme::FONT_CODE)
                        .text_color(rgb(TEXT))
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
                disclosure,
                diff: DiffFold {
                    folds: false,
                    shown: true,
                    toggle: None,
                },
            },
        ),
    }
}

/// What a prompt's band text is: its words as one literal run, or — with
/// attachments — Markdown whose links are the inline chips, so a chip flows
/// right after the words and wraps only when it does not fit.
pub(crate) enum PromptText {
    Literal(String),
    Markdown(String),
}

/// A prompt line as its band draws (and copies) it, and the files it
/// carries.
pub(crate) fn prompt_text(line: &str) -> (PromptText, Vec<std::path::PathBuf>) {
    let (text, files) = ferrite_core::prompt_files::split(String::from(line));
    if files.is_empty() {
        return (PromptText::Literal(text), files);
    }
    (PromptText::Markdown(prompt_markdown(&text, &files)), files)
}

/// The prompt's words with every Markdown character escaped (they are the
/// operator's own, never markup), hard breaks where they broke the line,
/// then one `ferrite-chip:` link per file, which the band renders as its
/// chip (`prompt_chip`).
fn prompt_markdown(text: &str, files: &[std::path::PathBuf]) -> String {
    let mut out = String::with_capacity(text.len() + 64);
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push_str("\\\n");
        }
        for ch in line.chars() {
            if ch.is_ascii_punctuation() {
                out.push('\\');
            }
            out.push(ch);
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

/// The prompt echo (SPEC §5.4): a `paint::FIELD` box at `R_CONTROL`,
/// `PROMPT_BAND_PAD_X` either side (the row's inset pulls it out by as
/// much, so its text keeps the transcript's column) and
/// `prompt_band_pad_y` above and below; the `❯` in the gutter in
/// `W_STRONG` `TEXT_STRONG`, the operator's words in `TEXT_STRONG` at body
/// weight with its files' chips flowing inline after them, and its send
/// time (`7:31 pm`, `TEXT_DIM`) in its own right-aligned column on the
/// first line — so a wrapping prompt keeps the time beside its first line.
/// An image hangs under the band in its 1px frame with its caption.
/// Nothing on the band answers the pointer. `pinned` draws exactly the band
/// again for the top of the body (not part of the selection, no image).
pub(crate) fn prompt_row(
    block: &Block,
    line: &str,
    row_cx: &RowCx,
    preview_lines: Option<usize>,
) -> AnyElement {
    let grid = row_cx.grid();
    let pinned = preview_lines.is_some();
    let (text, files) = prompt_text(line);
    // The images a chip or a picture opens among: ← → step through them.
    let gallery: std::sync::Arc<[std::path::PathBuf]> = files
        .iter()
        .filter(|path| crate::image_viewer::is_image(path))
        .cloned()
        .collect();
    // The chips close the prompt; their line stands taller, as the
    // prototype's inline chip makes it.
    let chip_line = if files.is_empty() {
        0.0
    } else {
        theme::PROMPT_CHIP_LINE_EXTRA
    };
    // A prompt of attachments alone has no words to register or draw.
    let words = match text {
        PromptText::Literal(words) if words.is_empty() => None,
        // A clamped band ends its last line in `…`, so it says there is
        // more of the prompt above.
        PromptText::Literal(words) if pinned => Some(
            div()
                .w_full()
                .min_w_0()
                .when_some(preview_lines, |words, lines| {
                    words.line_clamp(lines).text_ellipsis()
                })
                .child(SharedString::from(words))
                .into_any_element(),
        ),
        PromptText::Literal(words) => Some(
            div()
                .w_full()
                .min_w_0()
                .child(row_cx.selection.line(block.id, words, Vec::new()))
                .into_any_element(),
        ),
        PromptText::Markdown(source) if pinned => Some(
            crate::rich::Markdown::new(
                format!("pinned-prompt-{}-{:?}", row_cx.scope, block.id),
                source,
                row_cx.rich.clone(),
            )
            .ink(TEXT_STRONG)
            .chips(row_cx.preview.clone(), gallery.clone())
            .into_any_element(),
        ),
        PromptText::Markdown(source) => Some(
            row_cx
                .selection
                .markdown(block.id, source)
                .ink(TEXT_STRONG)
                .chips(row_cx.preview.clone(), gallery.clone())
                .into_any_element(),
        ),
    };
    let time = block.sent_at.clone();
    let band = div()
        .debug_selector(move || {
            if pinned {
                "transcript-pinned-prompt".into()
            } else {
                "transcript-prompt".into()
            }
        })
        .id(SharedString::from(format!(
            "{}prompt-{:?}",
            if pinned { "pinned-" } else { "" },
            block.id
        )))
        .flex()
        .items_start()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .bg(theme::paint::FIELD)
        .rounded(px(theme::R_CONTROL))
        .pt(px(theme::prompt_band_pad_y(row_cx.solo)))
        .pb(px(theme::prompt_band_pad_y(row_cx.solo) + chip_line))
        .px(px(theme::PROMPT_BAND_PAD_X))
        .font_weight(theme::W_BODY)
        .text_color(rgb(TEXT_STRONG))
        .child(mark_gutter(
            grid,
            div()
                .debug_selector(|| "prompt-mark".into())
                .child(components::prompt_mark(TEXT_STRONG)),
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .when_some(preview_lines, |column, lines| {
                    column.max_h(px(lines as f32 * grid.line)).overflow_hidden()
                })
                .children(words),
        )
        .children(time.map(|time| {
            div()
                .debug_selector(|| "prompt-time".into())
                .flex_shrink_0()
                .pl(px(theme::PROMPT_TIME_PAD_CELLS * grid.cell()))
                .whitespace_nowrap()
                .text_color(rgb(TEXT_DIM))
                .child(SharedString::from(time))
        }));
    if pinned {
        return band.into_any_element();
    }
    let images: Vec<_> = gallery
        .iter()
        .enumerate()
        .map(|(index, path)| inline_image(block.id, index, path, &gallery, row_cx.preview, grid))
        .collect();
    if images.is_empty() {
        return band.into_any_element();
    }
    div()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .child(band)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(grid.half()))
                .w_full()
                .min_w_0()
                .pt(px(grid.half()))
                .pl(px(theme::PROMPT_BAND_PAD_X + grid.gutter()))
                .pr(px(theme::PROMPT_BAND_PAD_X))
                .children(images),
        )
        .into_any_element()
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
/// mark (or the file mark), a cell, the name, all `TEXT_STRONG`, on
/// `paint::RAISED` at `R_INLINE` with a cell of padding each side. A click opens it (an
/// image in the Pane's preview, in the Panel, among its prompt's `gallery`).
/// Sized for the Markdown flow that places it: monospace cells at the size
/// it is drawn in.
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
        .rounded(px(theme::R_INLINE))
        .cursor_pointer()
        .whitespace_nowrap()
        .font_weight(theme::W_BODY)
        .text_color(rgb(TEXT_STRONG))
        .child(icon(
            if image { icons::IMAGE } else { icons::FILE },
            theme::GLYPH_BOX,
            TEXT_STRONG,
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

/// A sent image under its prompt (the prototype's `.img` and `.cap`): a
/// frame `IMAGE_CELLS` wide at the reading size, its 1px `paint::LINE2`
/// border included (374.4px at Standard, drawn 374), the picture filling
/// it at its own proportions; under it `name · W×H · 41 KB`, all of it
/// `TEXT_DIM`. A click opens it in the Pane's preview, among its
/// prompt's `gallery`.
fn inline_image(
    block: BlockId,
    index: usize,
    path: &std::path::Path,
    gallery: &std::sync::Arc<[std::path::PathBuf]>,
    preview: &crate::preview::Preview,
    grid: Grid,
) -> AnyElement {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let facts = image_facts(path);
    // Whole pixels, as the prototype's frame paints: a browser snaps a box's
    // edges to the CSS pixel (374.4 draws 374, a 205.46 picture 205), where
    // gpui would round each length to the half pixel and push every row
    // under the frame down by one.
    let frame_w = (theme::IMAGE_CELLS * grid.cell()).floor();
    let width = frame_w - 2.0;
    let height = match facts {
        Some(ImageFacts {
            width: w,
            height: h,
            ..
        }) if w > 0 && h > 0 => width * h as f32 / w as f32,
        _ => width * 0.5,
    }
    .floor();
    let caption = image_caption(&name, facts);
    let host = preview.clone();
    let gallery = gallery.clone();
    let open = path.to_path_buf();
    div()
        .id(SharedString::from(format!("sent-image-{block:?}-{index}")))
        .debug_selector(|| "sent-image".into())
        .flex()
        .flex_col()
        .min_w_0()
        .cursor_pointer()
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            open_file(&open, &gallery, &host, window, cx);
        })
        .child(
            div()
                .debug_selector(|| "sent-image-frame".into())
                .flex_shrink_0()
                .w(px(frame_w))
                .border_1()
                .border_color(theme::paint::LINE2)
                .child(
                    gpui::img(path.to_path_buf())
                        .w(px(width))
                        .h(px(height))
                        .object_fit(gpui::ObjectFit::Fill),
                ),
        )
        .child(
            div()
                .debug_selector(|| "sent-image-caption".into())
                .min_w_0()
                .truncate()
                .text_color(rgb(TEXT_DIM))
                .child(SharedString::from(caption)),
        )
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

/// What a row of output omitted, under the output it belongs to.
pub(crate) fn omitted_line(bytes: usize, grid: Grid) -> Div {
    after_elbow(grid)
        .text_color(rgb(TEXT_DIM))
        .child(div().min_w_0().truncate().child(SharedString::from(format!(
            "\u{2026} {} not kept",
            text::byte_size(bytes)
        ))))
}

/// How a turn ended. A completed turn is `✻ Worked for 41s · 7:32 pm · ↑
/// 3.2k ↓ 1.1k`: the typed `✻` in the provider's colour in the gutter, the
/// words and their `·` seams `TEXT_DIM`. An interrupted or failed one
/// hangs under the turn's last row in the failure-line grammar
/// (`failure_line`): `└ failed · 0.1s · API Error: 529 overloaded`, its lead
/// word the only coloured one — `interrupted` in `TEXT` (the operator did
/// it; nothing failed), `failed` in `BLOCKED`.
fn turn_end(
    block: BlockId,
    end: &ferrite_core::transcript::TurnEnd,
    selection: &TextRuns,
    provider: Option<Provider>,
    grid: Grid,
) -> Div {
    use ferrite_core::TurnOutcome;
    let text = end.text();
    let (lead, ink, message) = match &end.outcome {
        TurnOutcome::Completed => {
            return components::tabular(
                grid_row(mark_gutter(
                    grid,
                    components::glyph(components::WORKED.to_string(), provider_ink(provider))
                        .debug_selector(|| "turn-stamp-mark".into()),
                ))
                .debug_selector(|| "turn-stamp".into())
                .text_color(rgb(TEXT_DIM))
                .child(div().flex_1().min_w_0().child(selection.line(
                    block,
                    text.clone(),
                    separators(&text),
                ))),
            );
        }
        TurnOutcome::Interrupted => (theme::words::INTERRUPTED, TEXT, None),
        TurnOutcome::Error(_) => (theme::words::FAILED, BLOCKED, turn_end_message(end)),
    };
    let (head, excerpt) = turn_end_runs(&text, message);
    failure_line(block, head, lead.len(), ink, excerpt, selection, grid)
        .debug_selector(|| "turn-stamp".into())
}

/// A failed or interrupted turn's two selectable runs: the head (everything
/// before the provider's message, its seam included) and the message.
pub(super) fn turn_end_runs(text: &str, message: Option<&str>) -> (String, Option<String>) {
    match message.and_then(|message| Some((text.strip_suffix(message)?, message))) {
        Some((head, message)) => (head.to_owned(), Some(message.to_owned())),
        None => (text.to_owned(), None),
    }
}

/// The message a turn end prints after its head, if any.
pub(super) fn turn_end_message(end: &ferrite_core::transcript::TurnEnd) -> Option<&str> {
    match &end.outcome {
        ferrite_core::TurnOutcome::Error(message) if !message.is_empty() => Some(message),
        _ => None,
    }
}

/// The `·` seams in a transcript line: `TEXT_DIM`, at body weight
/// (CT-23: a seam inside the transcript reads as the words around it — the
/// status line's faint seams are the Composer's). Highlighted in place so
/// the line stays one run and copies back exactly as written.
fn separators(text: &str) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    text.match_indices('\u{b7}')
        .map(|(at, dot)| {
            (
                at..at + dot.len(),
                HighlightStyle {
                    color: Some(rgb(TEXT_DIM).into()),
                    font_weight: Some(theme::W_BODY),
                    ..Default::default()
                },
            )
        })
        .collect()
}

/// A tool call's bullet: state is the bullet, never the name. Settled work
/// is `TEXT_DIM`, live work a still green `●` (no pulse: the working line
/// is the one live thing), a failure `BLOCKED`, a call whose result never
/// came a hollow `○`. Green never means finished.
fn tool_dot_ink(state: &ToolState) -> (u32, DotShape) {
    match state {
        ToolState::Running => (RUNNING, DotShape::Solid),
        ToolState::Ok => (TEXT_DIM, DotShape::Solid),
        ToolState::Failed(_) => (BLOCKED, DotShape::Solid),
        ToolState::Unavailable => (TEXT_FAINT, DotShape::Ring),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DotShape {
    Solid,
    Ring,
}

impl DotShape {
    fn glyph(self) -> &'static str {
        match self {
            Self::Solid => BULLET,
            Self::Ring => RING,
        }
    }
}

/// Whether a call's argument is a file path (`Read(crates/x.rs)`), which
/// reads `TEXT_DIM` with the rest of the arguments and is a path target.
pub(crate) fn argument_is_path(tool: &ToolBlock) -> bool {
    const PATH_TOOLS: &[&str] = &[
        "Read",
        "Edit",
        "Write",
        "Update",
        "MultiEdit",
        "NotebookEdit",
        "NotebookRead",
    ];
    !tool.summary.is_empty()
        && tool.title.is_none()
        && !tool.summary.contains(char::is_whitespace)
        && !ferrite_core::docview::is_command_run(&tool.name)
        && (PATH_TOOLS.contains(&tool.name.as_str()) || !tool.diffs.is_empty())
}

/// A call line `Name(args)`, one CLI token on the grid (the prototype's
/// `.name` and `.args`): the name — as Claude Code prints it (`Update`,
/// `Bash`) — in `TEXT_STRONG` at `W_STRONG`, the parens and arguments in the
/// line's `TEXT_DIM`, a path argument too (a hover target, never cyan:
/// ADR 0033); one selectable line
/// that copies back exactly as it reads.
fn call_highlights(tool: &ToolBlock) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    let label = tool.label();
    let name = ferrite_core::transcript::display_tool_name(&tool.name)
        .len()
        .min(label.len());
    let mut highlights = vec![(
        0..name,
        HighlightStyle {
            color: Some(rgb(TEXT_STRONG).into()),
            font_weight: Some(theme::W_STRONG),
            ..Default::default()
        },
    )];
    if argument_is_path(tool) && label.len() > name + 2 {
        highlights.push((
            name + 1..label.len() - 1,
            HighlightStyle {
                color: Some(rgb(theme::TEXT_DIM).into()),
                ..Default::default()
            },
        ));
    }
    highlights
}

/// A call's time in its trail: a live call ticks whole seconds from `1s`
/// (`progress::live_seconds`, repainted at 1Hz by the transcript's second
/// clock) — coarsened to whole minutes from a minute up on a Pane that does
/// not hold the keyboard (`1m`); a settled one freezes at its whole seconds
/// (`18s`, `1m01s`), shown only from one second up (`DURATION_MIN_MS`).
fn trail_duration(
    tool: &ToolBlock,
    timings: Option<&HashMap<String, ToolTiming>>,
    focused: bool,
) -> Option<String> {
    match timings.and_then(|map| map.get(&tool.call))? {
        ToolTiming::Running(started) => {
            let elapsed = ferrite_core::clock::instant().saturating_duration_since(*started);
            (elapsed.as_secs() >= 1).then(|| {
                if focused {
                    ferrite_core::progress::live_seconds(elapsed)
                } else {
                    ferrite_core::progress::coarse_seconds(elapsed)
                }
            })
        }
        ToolTiming::Done(total) => (total.as_millis() >= theme::DURATION_MIN_MS)
            .then(|| ferrite_core::progress::live_seconds(*total)),
    }
}

/// A change's size in a call's trail: `+7 −3`, the whole `+7` in `RUNNING`
/// and the whole `−3` (U+2212) in `BLOCKED`.
pub(crate) fn trail_diff_stat(added: usize, removed: usize) -> StyledText {
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

/// One failure-line grammar, for a failed call and a failed or interrupted
/// turn alike: `└ failed · 0.1s · API Error: 529 overloaded`. The head —
/// the lead word in its state ink (`BLOCKED` failed, `TEXT` interrupted),
/// the rest and its seams `TEXT_DIM` — is one selectable run with its
/// trailing seam; the excerpt the machine printed is its own run,
/// `TEXT_DIM`, soft-wrapping and never cut.
fn failure_line(
    block: BlockId,
    head: String,
    lead: usize,
    lead_ink: u32,
    excerpt: Option<String>,
    selection: &TextRuns,
    grid: Grid,
) -> Div {
    let mut highlights = separators(&head);
    highlights.insert(
        0,
        (
            0..lead,
            HighlightStyle {
                color: Some(rgb(lead_ink).into()),
                ..Default::default()
            },
        ),
    );
    elbow_line(grid, TEXT_DIM).child(
        div()
            .flex()
            .items_start()
            .flex_1()
            .min_w_0()
            .child(components::tabular(
                div()
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .child(selection.line(block, head, highlights)),
            ))
            .when_some(excerpt, |line, excerpt| {
                line.child(div().flex_1().min_w_0().child(selection.line(
                    block,
                    excerpt,
                    Vec::new(),
                )))
            }),
    )
}

/// A failed call's head run: `failed · `, its seam carried so the detail
/// after it copies back as one line.
fn failed_head() -> String {
    format!("{} \u{b7} ", theme::words::FAILED)
}

/// A failed call's excerpt: the first non-blank line of its error, unless
/// the result line already says exactly that.
fn failed_excerpt(tool: &ToolBlock) -> Option<&str> {
    let ToolState::Failed(message) = &tool.state else {
        return None;
    };
    let first = message
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    (!first.is_empty() && tool.result_line.as_deref() != Some(first)).then_some(first)
}

/// The output a call retained, line by line.
fn output_text(tool: &ToolBlock) -> &str {
    text::disclosed_output(tool).map_or("", |output| output.text.as_str())
}

/// A collapsed failed call's fold of its output (the prototype's `.out` and
/// `.more`): up to `OUTPUT_PREVIEW_LINES` non-blank lines of what it printed
/// after the line its elbow already shows, and how many non-blank lines
/// that leaves out — the `+ N lines` sits directly under the last preview
/// line. `None` when the call printed nothing worth a fold: only a
/// command's output, or a failure's, folds.
pub(super) fn output_fold(tool: &ToolBlock) -> Option<(String, usize)> {
    let runs = ferrite_core::docview::is_command_run(&tool.name);
    if !runs && !matches!(tool.state, ToolState::Failed(_)) {
        return None;
    }
    text::disclosed_output(tool)?;
    let mut lines = output_text(tool)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .peekable();
    // The elbow already prints the first line (the result line, or a
    // failure's first line): the fold continues after it.
    let elbow = tool.result_line.as_deref().or_else(|| failed_excerpt(tool));
    if let (Some(first), Some(shown)) = (lines.peek(), elbow) {
        if first.trim() == shown.trim() {
            lines.next();
        }
    }
    let rest: Vec<&str> = lines.collect();
    let shown: Vec<&str> = rest
        .iter()
        .copied()
        .take(theme::OUTPUT_PREVIEW_LINES)
        .collect();
    if shown.is_empty() {
        return None;
    }
    let hidden = rest.len() - shown.len();
    Some((shown.join("\n"), hidden))
}

/// What a test run's elbow says, and the lines it previews (CT-14).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TestElbow {
    /// `ok` or `failed`: the lead word, in its state ink.
    pub lead: &'static str,
    /// The words after the lead's seam: `38 passed`, `running 38 tests`,
    /// `357 passed; 2 failed`.
    pub rest: String,
    /// The lines shown under the elbow, verbatim.
    pub preview: Vec<String>,
    /// The non-blank retained lines a `+ N lines` fold holds.
    pub hidden: usize,
}

impl TestElbow {
    /// The elbow's one line: `ok · 38 passed`.
    pub(super) fn line(&self) -> String {
        if self.rest.is_empty() {
            self.lead.to_string()
        } else {
            format!("{} \u{b7} {}", self.lead, self.rest)
        }
    }
}

/// A settled test run's elbow (one rule for both pictures):
///
/// - passing: `ok · <N> passed`, no preview, no fold;
/// - failing with every `FAILED` among its first three test lines: `failed ·
///   <its first output line>` and those three test lines verbatim;
/// - failing otherwise: `failed · <P> passed; <F> failed` (its `test
///   result:` line, else counted) and its `FAILED` lines (at most three)
///   without their leading `test `;
///
/// then `+ <N> lines` for the rest of its non-blank output. `None` for a
/// call that is not a settled test run.
pub(super) fn test_elbow(tool: &ToolBlock) -> Option<TestElbow> {
    if !is_test_run(tool) {
        return None;
    }
    let exited = match &tool.state {
        ToolState::Ok => false,
        ToolState::Failed(_) => true,
        _ => return None,
    };
    let output = match tool.output.as_ref() {
        Some(output) => output.text.as_str(),
        None => match &tool.state {
            ToolState::Failed(message) => message.as_str(),
            _ => tool.result_line.as_deref().unwrap_or(""),
        },
    };
    let lines: Vec<&str> = output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let tests: Vec<(usize, &str, &str)> = lines
        .iter()
        .enumerate()
        .filter_map(|(at, line)| {
            ferrite_core::transcript::test_result_line(line)
                .map(|(name, verdict)| (at, name, verdict))
        })
        .collect();
    let results: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| line.trim_start().starts_with("test result:"))
        .collect();
    let counted = |verdict: &str| tests.iter().filter(|(_, _, v)| *v == verdict).count();
    let summary_of = |line: &str| -> Option<(usize, usize)> {
        let passed = passed_count(line)?;
        let failed = line
            .split(';')
            .find_map(|part| part.trim().strip_suffix(" failed")?.trim().parse().ok())
            .unwrap_or(0);
        Some((passed, failed))
    };
    // cargo prints one `test result:` per test binary (the crate's, each
    // integration suite's, the doc tests'): the run's tally is their sum —
    // unless the last one already accounts for every test the run announced
    // (`running 359 tests`, a workspace tally closing the output).
    let ignored_of = |line: &str| -> usize {
        line.split(';')
            .find_map(|part| part.trim().strip_suffix(" ignored")?.trim().parse().ok())
            .unwrap_or(0)
    };
    let announced: usize = lines
        .iter()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("running ")?
                .strip_suffix(" tests")
                .or_else(|| line.trim().strip_prefix("running ")?.strip_suffix(" test"))?
                .trim()
                .parse::<usize>()
                .ok()
        })
        .sum();
    let last = results
        .last()
        .and_then(|line| summary_of(line).map(|counts| (counts, ignored_of(line))));
    let totals = match last {
        Some(((passed, failed), ignored))
            if announced > 0 && passed + failed + ignored == announced =>
        {
            Some((passed, failed))
        }
        _ => results
            .iter()
            .filter_map(|line| summary_of(line))
            .reduce(|(p, f), (passed, failed)| (p + passed, f + failed)),
    };
    // A run is failed by its own report too: piped into `tail` or `grep`
    // the command exits 0 while its suite failed.
    let failed = exited
        || results
            .iter()
            .any(|line| line.trim_start().starts_with("test result: FAILED"))
        || tests.iter().any(|(_, _, verdict)| *verdict == "FAILED");
    if !failed {
        let passed = totals
            .map(|(passed, _)| passed)
            .or_else(|| (!tests.is_empty()).then(|| counted("ok")))
            .or_else(|| tool.result_line.as_deref().and_then(passed_count));
        return Some(TestElbow {
            lead: "ok",
            rest: passed.map_or_else(String::new, |passed| format!("{passed} passed")),
            preview: Vec::new(),
            hidden: 0,
        });
    }
    let fails: Vec<&(usize, &str, &str)> = tests
        .iter()
        .filter(|(_, _, verdict)| *verdict == "FAILED")
        .collect();
    let early = !tests.is_empty()
        && fails
            .iter()
            .all(|(at, _, _)| tests.iter().take(3).any(|(first, _, _)| first == at));
    // The early form heads with the run's first line (`running 38 tests`);
    // output a pipe cut to its test lines has no such line, and reads its
    // tally instead.
    let early = early && tests.first().is_some_and(|(at, _, _)| *at > 0);
    if early {
        let first = lines.first().map(|line| line.trim()).unwrap_or("");
        let preview: Vec<String> = tests
            .iter()
            .take(3)
            .map(|(at, _, _)| lines[*at].trim().to_string())
            .collect();
        let shown = 1 + preview.len();
        return Some(TestElbow {
            lead: theme::words::FAILED,
            rest: first.to_string(),
            hidden: lines.len().saturating_sub(shown),
            preview,
        });
    }
    let (passed, failed_count) = totals.unwrap_or_else(|| (counted("ok"), fails.len()));
    let preview: Vec<String> = fails
        .iter()
        .take(3)
        .map(|(at, _, _)| {
            let line = lines[*at].trim();
            line.strip_prefix("test ").unwrap_or(line).to_string()
        })
        .collect();
    let shown = preview.len() + usize::from(!results.is_empty());
    Some(TestElbow {
        lead: theme::words::FAILED,
        rest: format!("{passed} passed; {failed_count} failed"),
        hidden: lines.len().saturating_sub(shown),
        preview,
    })
}

/// A successful command's output folded whole behind its elbow (CT-26):
/// `└ 7 issues · + 7 lines` — its first non-blank line, and the non-blank
/// lines after it. `None` when it printed one line or none, or is a test
/// run (which reads its own elbow) or not a command.
pub(super) fn command_fold(tool: &ToolBlock) -> Option<(String, Vec<String>)> {
    if tool.state != ToolState::Ok
        || !ferrite_core::docview::is_command_run(&tool.name)
        || is_test_run(tool)
    {
        return None;
    }
    let mut lines = output_text(tool)
        .lines()
        .filter(|line| !line.trim().is_empty());
    let first = lines.next()?.trim_end().to_string();
    let rest: Vec<String> = lines.map(|line| line.trim_end().to_string()).collect();
    (!rest.is_empty()).then_some((first, rest))
}

/// A fold's toggle line, `+ N lines` (collapsed) or `− fold` (open): dim,
/// lifting to `TEXT` under the pointer, a click flips the call's
/// disclosure.
fn fold_toggle(
    id: SharedString,
    label: String,
    toggle: Option<DisclosureToggle>,
    grid: Grid,
) -> Div {
    after_elbow(grid).child(fold_label(id, label, toggle))
}

/// The clickable words of a fold (`+ 7 lines`, `− fold`, `+ show diff`).
fn fold_label(id: SharedString, label: String, toggle: Option<DisclosureToggle>) -> Stateful<Div> {
    let group = id.clone();
    div()
        .id(id)
        .group(group.clone())
        .debug_selector(|| "tool-fold".into())
        .flex_shrink_0()
        .whitespace_nowrap()
        .cursor_pointer()
        .text_color(rgb(TEXT_DIM))
        .group_hover(group, |style| style.text_color(rgb(TEXT)))
        .child(SharedString::from(label))
        .when_some(toggle, |line, toggle| {
            line.on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                toggle(window, cx);
            })
        })
}

/// `+ 1 line`, `+ 35 lines`.
fn more_lines(hidden: usize) -> String {
    format!("+ {hidden} line{}", if hidden == 1 { "" } else { "s" })
}

/// A fold's content easing open over `FOLD_EASE_MS` (CT-26): the lines it
/// holds grow in height from nothing, the rows above never move; a close
/// lands at once, and reduced motion opens at once. Mounted shut too, so the
/// next opening eases.
fn eased_fold(
    id: SharedString,
    open: bool,
    lines: usize,
    line: f32,
    content: Option<AnyElement>,
) -> AnyElement {
    let spec = crate::motion::MotionSpec::new(theme::FOLD_EASE_MS, crate::motion::EASE);
    match (open, content) {
        (true, Some(content)) => crate::motion::settled(id, true, spec, move |t| {
            let full = lines as f32 * line;
            div()
                .w_full()
                .min_w_0()
                .overflow_hidden()
                .when(t < 1., |fold| fold.h(px(full * t.max(0.))))
                .child(content)
        })
        .reveal_only()
        .into_any_element(),
        _ => crate::motion::settled(id, false, spec, |_| div())
            .reveal_only()
            .into_any_element(),
    }
}

/// Lines of output under the elbow (one non-wrapping run, cut at the
/// column's edge like a terminal's), in `TEXT_DIM` with only their verdict
/// words coloured.
fn output_lines_run(block: BlockId, text: String, selection: &TextRuns, grid: Grid) -> Div {
    after_elbow(grid)
        .debug_selector(|| "tool-fold-preview".into())
        .text_color(rgb(TEXT_DIM))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(selection.line(block, text.clone(), result_highlights(&text))),
        )
}

/// What an open fold shows and how many lines it eases open over: the
/// lines it held (`rest`) as one run under the elbow; or, when the call's
/// whole output is past `OUTPUT_INLINE_BYTES`, the whole output in one
/// bounded, selectable native viewer (`OUTPUT_MAX_LINES` high) — a copy
/// from it takes everything the call printed. `pane/text.rs` mirrors it
/// (`collect_fold`).
fn fold_open(
    block: BlockId,
    tool: &ToolBlock,
    rest: String,
    selection: &TextRuns,
    grid: Grid,
) -> (AnyElement, usize) {
    let full = output_text(tool);
    if text::output_scrolls(full) {
        let lines = text::output_lines(full).min(theme::OUTPUT_MAX_LINES);
        let viewer = after_elbow(grid)
            .text_color(rgb(TEXT_DIM))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(selection.output(block, "result", full)),
            )
            .into_any_element();
        return (viewer, lines);
    }
    let lines = rest.lines().count();
    (
        output_lines_run(block, rest, selection, grid).into_any_element(),
        lines,
    )
}

/// A running test's elbow (CT-25): `└ running 357 tests`, its bar, and
/// `212/357` in tabular `TEXT_DIM`.
fn running_tests(call: &str, progress: ferrite_core::transcript::TestProgress, grid: Grid) -> Div {
    // The runner's total unheard (`TestProgress::known`): the count alone,
    // where `212/357` would stand, and no bar to fill against nothing.
    if !progress.known() {
        return elbow_line(grid, TEXT_DIM)
            .debug_selector(|| "tool-test-progress".into())
            .items_center()
            .child(
                div()
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .child("running tests"),
            )
            .child(components::tabular(
                div()
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .ml(px(grid.cell()))
                    .child(SharedString::from(progress.done.to_string())),
            ));
    }
    let total = progress.total.max(1);
    let fraction = (progress.done as f32 / total as f32).clamp(0., 1.);
    elbow_line(grid, TEXT_DIM)
        .debug_selector(|| "tool-test-progress".into())
        .items_center()
        .child(
            div()
                .flex_shrink_0()
                .whitespace_nowrap()
                .child(SharedString::from(format!(
                    "running {} tests",
                    progress.total
                ))),
        )
        .child(TestBar {
            id: SharedString::from(format!("test-bar-{call}")),
            fraction,
            cell: grid.cell(),
        })
        .child(components::tabular(
            div()
                .flex_shrink_0()
                .whitespace_nowrap()
                .child(SharedString::from(format!(
                    "{}/{}",
                    progress.done, progress.total
                ))),
        ))
}

/// A running test's bar (the prototype's `.bar`): `TEST_BAR_CELLS` wide and
/// `TEST_BAR_H` tall on a `paint::LINE2` track, a cell's margin each side,
/// its `RUNNING` fill at sub-pixel precision easing to each new count over
/// `TEST_BAR_EASE_MS`.
#[derive(IntoElement)]
struct TestBar {
    id: SharedString,
    fraction: f32,
    cell: f32,
}

impl gpui::RenderOnce for TestBar {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let spec = crate::motion::MotionSpec::new(theme::TEST_BAR_EASE_MS, crate::motion::EASE);
        let now = cx.background_executor().now();
        let reduced = crate::motion::reduced_motion(cx);
        let target = self.fraction;
        let (shown, running) = window.with_global_id(self.id.clone().into(), |id, window| {
            window.with_element_state(id, |state: Option<(f32, crate::motion::Tween)>, _| {
                let (at, tween) = match state {
                    Some((at, tween)) if at == target => (at, tween),
                    Some((_, tween)) => (
                        target,
                        crate::motion::Tween::retarget(
                            Some(tween),
                            target,
                            target,
                            spec,
                            now,
                            reduced,
                        ),
                    ),
                    None => (target, crate::motion::Tween::new(target, target, spec, now)),
                };
                let value = tween.value(now, reduced);
                ((value, tween.running(now, reduced)), (at, tween))
            })
        });
        if running {
            window.request_animation_frame();
        }
        let width = theme::TEST_BAR_CELLS * self.cell;
        div()
            .debug_selector(|| "test-bar".into())
            .relative()
            .flex_shrink_0()
            .mx(px(self.cell))
            .w(px(width))
            .h(px(theme::TEST_BAR_H))
            .rounded(px(theme::TEST_BAR_R))
            .overflow_hidden()
            .bg(theme::paint::LINE2)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(width * shown.clamp(0., 1.)))
                    .bg(rgb(RUNNING)),
            )
    }
}

/// A tool call (the prototype's `.r` with `.line`): `● Name(args)` with its
/// trail hard right — `+N −M` when a diff follows, then the time (ticking
/// while live, frozen when done) — and what it produced hanging under it on
/// the `└` elbow: a test run's `ok · 38 passed` or its failure and preview,
/// a running suite's bar, a command's first line folding the rest, a read's
/// `412 lines`, a failure, and its diffs (a later edit's folded behind `+
/// show diff`). Under the pointer the row changes nothing but a path
/// argument's underline: no ground, no chevron; the keyboard's disclosure
/// target alone wears `paint::RAISED`.
///
/// Expanded from the keyboard, a call that is not a command shows the input
/// where the call line could not show it whole, its output, and any
/// structured result, none of them labelled; a command's expansion is its
/// output fold opening.
pub(crate) fn render_tool(
    block: BlockId,
    tool: &ToolBlock,
    row_cx: &RowCx,
    state: ToolCx,
) -> AnyElement {
    let grid = row_cx.grid();
    let selection = row_cx.selection;
    let ToolCx {
        expanded,
        disclosure,
        diff,
    } = state;
    let (targeted, toggle, focus, bullet) = match disclosure {
        Some(Disclosure {
            targeted,
            toggle,
            focus,
            overlay,
            ..
        }) => (targeted, toggle, focus, overlay),
        None => (false, None, None, None),
    };
    let label = tool.label();
    let call = div()
        .relative()
        .min_w_0()
        .truncate()
        .font_weight(theme::W_BODY)
        .child(selection.line(block, label, call_highlights(tool)))
        .children(text::path_argument(tool).map(|(start, len)| {
            crate::file_links::path_overlay(
                row_cx.path_target(&tool.summary),
                start,
                len,
                grid.size,
                grid.line,
            )
        }));
    let mut trail = components::tabular(
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(grid.cell()))
            .whitespace_nowrap()
            .text_color(rgb(TEXT_DIM)),
    );
    let mut trailing = false;
    if let Some(ToolVerdict::Diff(added, removed)) = tool_verdicts(tool).into_iter().next() {
        // Right-aligned words hold exactly their cells: a measured run
        // rounds up a pixel and would stand its ink that much left.
        let cells = format!("+{added} \u{2212}{removed}").chars().count() as f32;
        trail = trail.child(
            div()
                .debug_selector(|| "tool-trail-diff".into())
                .flex_shrink_0()
                .w(px(cells * grid.cell()))
                .child(trail_diff_stat(added, removed)),
        );
        trailing = true;
    }
    // A change says its size; anything else says its time (`18s`).
    if let Some(duration) =
        trail_duration(tool, row_cx.timings, row_cx.focused).filter(|_| !trailing)
    {
        let cells = duration.chars().count() as f32;
        trail = trail.child(
            div()
                .debug_selector(|| "tool-trail-duration".into())
                .flex_shrink_0()
                .w(px(cells * grid.cell()))
                .child(SharedString::from(duration)),
        );
        trailing = true;
    }
    let (ink, shape) = match tool_dot_ink(&tool.state) {
        // Its own report failed it, whatever the pipe exited with.
        (TEXT_DIM, shape)
            if test_elbow(tool).is_some_and(|elbow| elbow.lead == theme::words::FAILED) =>
        {
            (BLOCKED, shape)
        }
        ink => ink,
    };
    let row_selector = format!("tool-row-{}", tool.call);
    let mut line =
        grid_row(glyph_gutter(grid, shape.glyph(), ink).debug_selector(|| "tool-dot".into()))
            .id(SharedString::from(row_selector.clone()))
            .debug_selector(move || row_selector.clone())
            .relative()
            .text_color(rgb(TEXT_DIM))
            .child(call)
            .child(div().flex_1())
            .when(trailing, |line| line.child(trail.pl(px(2.0 * grid.cell()))))
            // The bullet's cell toggles the call's details; nothing shows it.
            .children(bullet);
    if targeted {
        // The ground is the row's; the target's selector rides a layer over
        // it, so the row keeps its own.
        line = line.bg(theme::paint::RAISED).child(
            div()
                .debug_selector(|| "tool-disclosure-keyboard-target".into())
                .absolute()
                .inset_0(),
        );
        if let Some(focus) = &focus {
            line = line.track_focus(focus).key_context("ToolDisclosure");
        }
    }
    let mut card = div().flex().flex_col().w_full().min_w_0().child(line);
    let fold_id = SharedString::from(format!("tool-fold-{}", tool.call));
    let label_id = |open: bool| {
        SharedString::from(format!(
            "tool-fold-{}-{}",
            tool.call,
            if open { "open" } else { "shut" }
        ))
    };
    let command = ferrite_core::docview::is_command_run(&tool.name);
    match &tool.state {
        ToolState::Running => {
            if let Some(progress) = tool.progress {
                card = card.child(running_tests(&tool.call, progress, grid));
            } else if let Some(result) = &tool.result_line {
                card = card.child(elbow_line(grid, TEXT_DIM).child(
                    div().flex_1().min_w_0().truncate().child(selection.line(
                        block,
                        result.clone(),
                        result_highlights(result),
                    )),
                ));
            }
            // Opened while it runs: the command and what it has printed so
            // far, live.
            if expanded {
                card = card.child(tool_details(block, tool, selection, grid));
            }
        }
        ToolState::Unavailable => {
            card = card.child(elbow_line(grid, TEXT_DIM).child(text::NO_RESULT));
        }
        ToolState::Failed(_) => {
            if let Some(elbow) = test_elbow(tool) {
                card = card.child(test_elbow_row(block, &elbow, selection, grid));
                if !elbow.preview.is_empty() {
                    card = card.child(output_lines_run(
                        block,
                        elbow.preview.join("\n"),
                        selection,
                        grid,
                    ));
                }
                card = card.children(rest_fold(
                    block,
                    tool,
                    &elbow,
                    expanded,
                    &fold_id,
                    toggle.clone(),
                    selection,
                    grid,
                ));
            } else {
                // `└ failed · <what it said>`: the result line, else the
                // first line of the error.
                let detail = tool
                    .result_line
                    .clone()
                    .or_else(|| failed_excerpt(tool).map(str::to_owned))
                    .filter(|detail| !detail.trim().is_empty());
                // Nothing said: the word alone, no separator left hanging.
                let head = if detail.is_some() {
                    failed_head()
                } else {
                    theme::words::FAILED.to_string()
                };
                card = card.child(failure_line(
                    block,
                    head,
                    theme::words::FAILED.len(),
                    BLOCKED,
                    detail,
                    selection,
                    grid,
                ));
                if let Some((shown, hidden)) = output_fold(tool) {
                    card = card.child(output_lines_run(block, shown, selection, grid));
                    if hidden > 0 {
                        let open = expanded.then(|| {
                            fold_open(block, tool, failed_rest(tool, hidden), selection, grid)
                        });
                        let lines = open.as_ref().map_or(hidden, |(_, lines)| *lines);
                        card = card.child(eased_fold(
                            fold_id.clone(),
                            expanded,
                            lines,
                            grid.line,
                            open.map(|(content, _)| content),
                        ));
                        card = card.child(fold_toggle(
                            label_id(expanded),
                            if expanded {
                                "\u{2212} fold".into()
                            } else {
                                more_lines(hidden)
                            },
                            toggle.clone(),
                            grid,
                        ));
                    }
                }
            }
        }
        ToolState::Ok => {
            if let Some(elbow) = test_elbow(tool) {
                card = card.child(test_elbow_row(block, &elbow, selection, grid));
                // A suite that failed behind a pipe (`| tail`) exited 0: its
                // failures hang under it as a failed call's would.
                if elbow.lead == theme::words::FAILED {
                    if !elbow.preview.is_empty() {
                        card = card.child(output_lines_run(
                            block,
                            elbow.preview.join("\n"),
                            selection,
                            grid,
                        ));
                    }
                    card = card.children(rest_fold(
                        block,
                        tool,
                        &elbow,
                        expanded,
                        &fold_id,
                        toggle.clone(),
                        selection,
                        grid,
                    ));
                }
            } else if let Some((first, rest)) = command_fold(tool) {
                // `└ 7 issues · + 7 lines`: the whole output folds behind its
                // elbow; open, the rest hangs under it.
                // Each run holds exactly its cells: a measured run rounds
                // up a pixel and would push the fold's words off the grid.
                // Where the line is narrower, the first line gives way, cut
                // at its end: the fold's words always stay in the row.
                let first_w = first.chars().count() as f32 * grid.cell();
                card = card.child(
                    elbow_line(grid, TEXT_DIM)
                        .child(
                            div()
                                .debug_selector(|| "tool-fold-head".into())
                                .flex_shrink(1.)
                                .min_w_0()
                                .w(px(first_w))
                                .truncate()
                                .child(selection.line(
                                    block,
                                    first.clone(),
                                    result_highlights(&first),
                                )),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .w(px(3.0 * grid.cell()))
                                .whitespace_nowrap()
                                .text_color(rgb(TEXT_DIM))
                                .child(" \u{b7} "),
                        )
                        .child(fold_label(
                            label_id(expanded),
                            if expanded {
                                "\u{2212} fold".into()
                            } else {
                                more_lines(rest.len())
                            },
                            toggle.clone(),
                        )),
                );
                let open =
                    expanded.then(|| fold_open(block, tool, rest.join("\n"), selection, grid));
                let lines = open.as_ref().map_or(rest.len(), |(_, lines)| *lines);
                card = card.child(eased_fold(
                    fold_id.clone(),
                    expanded,
                    lines,
                    grid.line,
                    open.map(|(content, _)| content),
                ));
            } else if tool.diffs.is_empty() {
                if let Some(result) = &tool.result_line {
                    card = card.child(elbow_line(grid, TEXT_DIM).child(
                        div().flex_1().min_w_0().truncate().child(selection.line(
                            block,
                            result.clone(),
                            result_highlights(result),
                        )),
                    ));
                }
            }
            if expanded && !command {
                card = card.child(tool_details(block, tool, selection, grid));
            }
        }
    }
    if !tool.diffs.is_empty() {
        let diff_id = |open: bool| {
            SharedString::from(format!(
                "tool-diff-fold-{}-{}",
                tool.call,
                if open { "open" } else { "shut" }
            ))
        };
        if diff.folds {
            card = card.child(fold_toggle(
                diff_id(diff.shown),
                if diff.shown {
                    "\u{2212} fold".into()
                } else {
                    "+ show diff".into()
                },
                diff.toggle.clone(),
                grid,
            ));
        }
        if diff.shown {
            for each in &tool.diffs {
                card = card.child(render_diff(
                    Some((selection, block)),
                    each,
                    grid,
                    row_cx.wide,
                ));
            }
        }
    }
    div()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .child(card)
        .into_any_element()
}

/// A group's disclosure state: whether it is open, the keyboard's target
/// and the pointer's toggle over its summary, whether the pointer just
/// flipped it (its members ease in), and how its members draw — every call
/// while open; while shut, the call still working (`text::group_live`).
/// They draw after the summary, so their text registers after its own.
pub(crate) struct GroupCx<'a> {
    pub expanded: bool,
    pub disclosure: Option<Disclosure>,
    pub members: Box<dyn FnOnce() -> Vec<AnyElement> + 'a>,
}

/// A group's bullet, read as a call's is: green while any call works, the
/// failure ink once any failed, hollow when none answered, else settled.
fn group_dot_ink(members: &[Block]) -> (u32, DotShape) {
    let tools: Vec<&ToolBlock> = text::group_tools(members).collect();
    if tools.iter().any(|tool| tool.state == ToolState::Running) {
        (RUNNING, DotShape::Solid)
    } else if tools.iter().any(|tool| text::call_failed(tool)) {
        (BLOCKED, DotShape::Solid)
    } else if tools
        .iter()
        .all(|tool| tool.state == ToolState::Unavailable)
    {
        (TEXT_FAINT, DotShape::Ring)
    } else {
        (TEXT_DIM, DotShape::Solid)
    }
}

/// A group's time in its trail: its calls' times summed — ticking while
/// one works, coarsened as a live call's is on a Pane without the keyboard
/// — shown from one second up.
fn group_duration(
    members: &[Block],
    timings: Option<&HashMap<String, ToolTiming>>,
    focused: bool,
) -> Option<String> {
    let timings = timings?;
    let now = ferrite_core::clock::instant();
    let mut total = Duration::ZERO;
    let mut live = false;
    for tool in text::group_tools(members) {
        match timings.get(&tool.call) {
            Some(ToolTiming::Running(started)) => {
                total += now.saturating_duration_since(*started);
                live = true;
            }
            Some(ToolTiming::Done(spent)) => total += *spent,
            None => {}
        }
    }
    if total.as_millis() < theme::DURATION_MIN_MS {
        return None;
    }
    Some(if live && !focused {
        ferrite_core::progress::coarse_seconds(total)
    } else {
        ferrite_core::progress::live_seconds(total)
    })
}

/// A run of routine calls under one summary (`text::group_summary`):
/// `● Ran 12 commands, read 1 file · 2 failed ›` with the calls' summed
/// time hard right (shut over a working call, that call's clock alone). The bullet is the group's state (`group_dot_ink`); the
/// summary reads in `TEXT`, its failure count in `BLOCKED`. The whole line
/// is the pointer's toggle — it lifts to `TEXT_STRONG` under the pointer,
/// its chevron always shown, a quarter turned when open — and the
/// keyboard's (`ToolDisclosure`), wearing `paint::RAISED` when targeted.
///
/// Open, the calls hang under the summary one gutter in, each the row it
/// would be on its own (its folds and details its own), on a 1px
/// `paint::LINE2` rail under the bullet, easing in; shut, a working call
/// hangs there alone so a live suite's bar and a command's live line stay
/// in view.
pub(crate) fn render_tool_group(members: &[Block], row_cx: &RowCx, state: GroupCx) -> AnyElement {
    let grid = row_cx.grid();
    let GroupCx {
        expanded,
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
    if let Some(failed) = summary.failed.clone() {
        highlights.push((
            failed,
            HighlightStyle {
                color: Some(rgb(BLOCKED).into()),
                ..Default::default()
            },
        ));
    }
    let (ink, shape) = group_dot_ink(members);
    // Shut with its working call under it, the call's own clock is the
    // group's: one ticking time, never two.
    let live = !expanded && text::group_live(members).is_some();
    let trail = (!live)
        .then(|| group_duration(members, row_cx.timings, row_cx.focused))
        .flatten()
        .map(|duration| {
            // Right-aligned words hold exactly their cells.
            let cells = duration.chars().count() as f32;
            components::tabular(
                div()
                    .debug_selector(|| "tool-group-duration".into())
                    .flex_shrink_0()
                    .w(px(cells * grid.cell()))
                    .whitespace_nowrap()
                    .text_color(rgb(TEXT_DIM))
                    .child(SharedString::from(duration)),
            )
        });
    let row_selector = format!("tool-group-{leader_call}");
    let mut header =
        grid_row(glyph_gutter(grid, shape.glyph(), ink).debug_selector(|| "tool-group-dot".into()))
            .id(SharedString::from(row_selector.clone()))
            .debug_selector(move || row_selector.clone())
            .group(DISCLOSURE_ROW)
            .relative()
            .items_center()
            .child(
                div()
                    .debug_selector(|| "tool-group-summary".into())
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(TEXT))
                    .group_hover(DISCLOSURE_ROW, |style| style.text_color(rgb(TEXT_STRONG)))
                    .child(
                        row_cx
                            .selection
                            .line(leader.id, summary.text.clone(), highlights),
                    ),
            )
            .children(chevron)
            .child(div().flex_1())
            .when_some(trail, |header, trail| {
                header.child(div().flex_shrink_0().pl(px(2.0 * grid.cell())).child(trail))
            })
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
    let rail = |members: Vec<AnyElement>| {
        div()
            .debug_selector(|| "tool-group-members".into())
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .pl(px(grid.gutter()))
            .child(
                div()
                    .debug_selector(|| "tool-group-rail".into())
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px((grid.cell() / 2.).floor()))
                    .w(px(1.))
                    .bg(theme::paint::LINE2),
            )
            .children(members)
    };
    let drawn = draw_members();
    let open_id = SharedString::from(format!("tool-group-open-{leader_call}"));
    let spec = crate::motion::MotionSpec::new(theme::FOLD_EASE_MS, crate::motion::EASE);
    let body = if expanded {
        let members = rail(drawn);
        crate::motion::settled(open_id, true, spec, move |t| {
            div()
                .w_full()
                .min_w_0()
                .opacity(t.clamp(0., 1.))
                .child(members)
        })
        .reveal_only()
        .into_any_element()
    } else {
        // Mounted shut too, so the next opening eases.
        let shut = crate::motion::settled(open_id, false, spec, |_| div())
            .reveal_only()
            .into_any_element();
        if drawn.is_empty() {
            shut
        } else {
            div()
                .w_full()
                .min_w_0()
                .child(shut)
                .child(rail(drawn))
                .into_any_element()
        }
    };
    div()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .text_color(rgb(TEXT_DIM))
        .child(header)
        .child(body)
        .into_any_element()
}

/// A test run's elbow row: `└ ok · 38 passed`, `└ failed · running 38
/// tests` — the lead word alone in its state ink.
fn test_elbow_row(block: BlockId, elbow: &TestElbow, selection: &TextRuns, grid: Grid) -> Div {
    let text = elbow.line();
    elbow_line(grid, TEXT_DIM).child(
        div()
            .debug_selector(|| "tool-test-elbow".into())
            .flex_1()
            .min_w_0()
            .truncate()
            .child(selection.line(block, text.clone(), result_highlights(&text))),
    )
}

/// A failed test run's fold under its preview: the rest of its non-blank
/// output, easing open, and the `+ N lines` / `− fold` line.
#[allow(clippy::too_many_arguments)]
fn rest_fold(
    block: BlockId,
    tool: &ToolBlock,
    elbow: &TestElbow,
    expanded: bool,
    fold_id: &SharedString,
    toggle: Option<DisclosureToggle>,
    selection: &TextRuns,
    grid: Grid,
) -> Vec<AnyElement> {
    if elbow.hidden == 0 {
        return Vec::new();
    }
    let label = SharedString::from(format!(
        "{}-{}",
        fold_id,
        if expanded { "open" } else { "shut" }
    ));
    let open = expanded.then(|| fold_open(block, tool, test_rest(tool, elbow), selection, grid));
    let lines = open.as_ref().map_or(elbow.hidden, |(_, lines)| *lines);
    vec![
        eased_fold(
            fold_id.clone(),
            expanded,
            lines,
            grid.line,
            open.map(|(content, _)| content),
        ),
        fold_toggle(
            label,
            if expanded {
                "\u{2212} fold".into()
            } else {
                more_lines(elbow.hidden)
            },
            toggle,
            grid,
        )
        .into_any_element(),
    ]
}

/// The non-blank output lines a failed test run's fold holds: everything
/// its elbow and preview did not show, in order.
pub(super) fn test_rest(tool: &ToolBlock, elbow: &TestElbow) -> String {
    let lines: Vec<&str> = output_text(tool)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let shown: Vec<String> = std::iter::once(elbow.rest.clone())
        .chain(elbow.preview.iter().cloned())
        .collect();
    let mut skipped = vec![false; lines.len()];
    for shown in &shown {
        if let Some(at) = lines.iter().enumerate().position(|(at, line)| {
            !skipped[at] && {
                let line = line.trim();
                line == shown || line.strip_prefix("test ") == Some(shown.as_str())
            }
        }) {
            skipped[at] = true;
        }
    }
    // The summary line the elbow was read from counts as shown too.
    if elbow.rest.contains("passed;") {
        if let Some(at) = lines
            .iter()
            .rposition(|line| line.trim_start().starts_with("test result:"))
        {
            skipped[at] = true;
        }
    }
    lines
        .iter()
        .zip(skipped)
        .filter(|(_, skipped)| !skipped)
        .map(|(line, _)| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The non-blank output lines a failed command's fold holds after its
/// preview (`hidden` of them).
fn failed_rest(tool: &ToolBlock, hidden: usize) -> String {
    let lines: Vec<&str> = output_text(tool)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    lines[lines.len().saturating_sub(hidden)..]
        .iter()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

/// A call's disclosed details (keyboard-opened, not a command): its input
/// where the call line could not show it whole, its output, its structured
/// result.
fn tool_details(block: BlockId, tool: &ToolBlock, selection: &TextRuns, grid: Grid) -> Div {
    let mut details = div().flex().flex_col().min_w_0();
    let mut first = true;
    let mut part = |details: Div, name: &str, text: &str| {
        let block = output_block(block, name, text, TEXT_DIM, false, first, selection, grid);
        first = false;
        details.child(block)
    };
    if text::shows_input(tool) {
        details = part(details, "command", &tool.summary);
    }
    if let Some(output) = text::disclosed_output(tool) {
        details = part(details, "result", &output.text);
        if output.omitted_bytes > 0 {
            details = details.child(omitted_line(output.omitted_bytes, grid));
        }
    }
    if let Some(structured) = tool.structured_preview.as_ref() {
        details = part(details, "details", &structured.text);
        if structured.omitted_bytes > 0 {
            details = details.child(omitted_line(structured.omitted_bytes, grid));
        }
    }
    details
}

/// A result's verdict words in their state ink, the rest in the line's
/// `TEXT_DIM` (CT-15): a line's lead `ok` (`RUNNING`) or `failed`
/// (`BLOCKED`); a test line's trailing `... ok` / `... FAILED`; and the
/// verdict of a `test result:` line. `24 passed`, `357 passed; 2 failed`
/// and every other `passed` or `failed` stay muted.
fn result_highlights(text: &str) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    let ink = |range: std::ops::Range<usize>, ink: u32| {
        (
            range,
            HighlightStyle {
                color: Some(rgb(ink).into()),
                ..Default::default()
            },
        )
    };
    let verdict_ink = |word: &str| match word {
        "ok" => Some(RUNNING),
        "FAILED" | "failed" => Some(BLOCKED),
        _ => None,
    };
    let mut highlights = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let indent = body.len() - body.trim_start().len();
        let trimmed = &body[indent..];
        // The lead state word: `ok · 38 passed`, `failed · …`.
        for word in ["ok", "failed"] {
            if trimmed == word
                || trimmed.starts_with(&format!("{word} "))
                || trimmed.starts_with(&format!("{word}\u{b7}"))
            {
                let start = offset + indent;
                highlights.push(ink(
                    start..start + word.len(),
                    verdict_ink(word).unwrap_or(TEXT_DIM),
                ));
            }
        }
        // A test line's trailing verdict.
        if let Some((_, verdict)) = trimmed.rsplit_once(" ... ") {
            let verdict = verdict.trim_end();
            if let Some(color) = verdict_ink(verdict).filter(|_| verdict != "failed") {
                let start = offset + indent + trimmed.trim_end().len() - verdict.len();
                highlights.push(ink(start..start + verdict.len(), color));
            }
        }
        // `test result: FAILED. 37 passed; …`.
        if let Some(rest) = trimmed.strip_prefix("test result: ") {
            for word in ["ok", "FAILED"] {
                if rest.starts_with(word) {
                    let start = offset + indent + "test result: ".len();
                    highlights.push(ink(
                        start..start + word.len(),
                        verdict_ink(word).unwrap_or(TEXT_DIM),
                    ));
                }
            }
        }
        offset += line.len();
    }
    highlights.sort_by_key(|(range, _)| range.start);
    highlights.dedup_by(|a, b| a.0 == b.0);
    highlights
}

/// A disclosed block of text — a command (`$` first when it is one),
/// output, a structured result. Only the first part of a disclosure hangs
/// on the `└` elbow; later parts sit after it. Text draws inline, keeping
/// its whitespace; text past `OUTPUT_INLINE_BYTES` scrolls in one bounded,
/// selectable native control `OUTPUT_MAX_LINES` high, and `… +N lines`
/// under it says how much is out of view.
#[allow(clippy::too_many_arguments)]
pub(crate) fn output_block(
    block: BlockId,
    part: &str,
    text: &str,
    ink: u32,
    command: bool,
    first: bool,
    selection: &TextRuns,
    grid: Grid,
) -> Div {
    let rows = if first {
        elbow_line(grid, ink)
    } else {
        after_elbow(grid).text_color(rgb(ink))
    }
    .when(command, |rows| {
        rows.child(
            div()
                .flex_shrink_0()
                .w(px(2.0 * grid.cell()))
                .text_color(rgb(TEXT_FAINT))
                .child("$"),
        )
    });
    if text::output_scrolls(text) {
        let hidden = text::output_lines(text).saturating_sub(theme::OUTPUT_MAX_LINES);
        let output = rows.child(
            div()
                .flex_1()
                .min_w_0()
                .child(selection.output(block, part, text)),
        );
        if hidden == 0 {
            return output;
        }
        return div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .child(output)
            .child(
                after_elbow(grid)
                    .text_color(rgb(TEXT_DIM))
                    .child(SharedString::from(format!("\u{2026} +{hidden} lines"))),
            );
    }
    rows.child(div().flex_1().min_w_0().child(selection.line(
        block,
        text.to_string(),
        result_highlights(text),
    )))
}

#[derive(Debug, PartialEq, Eq)]
enum ToolVerdict {
    Diff(usize, usize),
    Failed,
}

fn tool_verdicts(tool: &ToolBlock) -> Vec<ToolVerdict> {
    let mut verdicts = Vec::with_capacity(2);
    if !tool.diffs.is_empty() {
        verdicts.push(ToolVerdict::Diff(
            tool.diffs.iter().map(|diff| diff.added).sum(),
            tool.diffs.iter().map(|diff| diff.removed).sum(),
        ));
    }
    if matches!(tool.state, ToolState::Failed(_)) {
        verdicts.push(ToolVerdict::Failed);
    }
    verdicts
}

/// Flip one disclosure from inside its row (a fold's `+ N lines`).
pub type DisclosureToggle = std::rc::Rc<dyn Fn(&mut gpui::Window, &mut gpui::App)>;

/// A disclosable row's parts, built by the transcript for the row renderer:
/// for a reasoning row the click target over its header and its chevron;
/// for every row whether the keyboard targets it (and the focus a targeted
/// tool row tracks), and the toggle a fold line inside the row calls.
pub struct Disclosure {
    /// The pointer's toggle: over a reasoning row's whole header, so its
    /// label toggles it too; over a tool row's bullet cell alone, so its
    /// call line stays plain selectable text.
    pub overlay: Option<AnyElement>,
    /// The trailing chevron (`disclosure_chevron`, reasoning rows).
    pub chevron: Option<AnyElement>,
    /// Keyboard cycling has landed on this row: it wears the `paint::RAISED`
    /// ground so the operator sees which row Enter will toggle. The pointer
    /// never sets it, and no other row ever wears a ground.
    pub targeted: bool,
    /// What a fold's toggle line calls.
    pub toggle: Option<DisclosureToggle>,
    /// The focus a targeted tool row tracks (its keys' context).
    pub focus: Option<FocusHandle>,
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
        (DisclosureId::Diff(_), false) => "Show diff",
        (DisclosureId::Diff(_), true) => "Hide diff",
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
                .tooltip(move |window, cx| {
                    gpui::component::tooltip::Tooltip::new(tooltip).build(window, cx)
                }),
        )
}

/// A reasoning row's trailing chevron: `ICON_CHEVRON` in `TEXT_FAINT`, its
/// box always reserved so nothing moves when it shows. It shows while the
/// pointer is on the row (lifting to `TEXT_DIM`) or while the keyboard
/// targets the row, and it turns a quarter when open. The turn eases over
/// `motion::CHEVRON` only when the pointer flipped it (`eased`); a keyboard
/// toggle, reduced motion and a row drawn already open are simply turned.
pub fn disclosure_chevron(expanded: bool, targeted: bool, eased: bool) -> AnyElement {
    let chevron = |turn: f32| {
        icon(icons::CHEVRON_RIGHT, theme::ICON_CHEVRON, TEXT_FAINT)
            .group_hover(DISCLOSURE_ROW, |style| style.text_color(rgb(TEXT_DIM)))
            .with_transformation(gpui::Transformation::rotate(gpui::radians(
                std::f32::consts::FRAC_PI_2 * turn,
            )))
    };
    let mark = if eased {
        crate::motion::settled("disclosure-turn", expanded, crate::motion::CHEVRON, chevron)
            .into_any_element()
    } else {
        chevron(if expanded { 1. } else { 0. }).into_any_element()
    };
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

// ------------------------------------------------------------------ diffs

/// One side of a diff row: its line number, its kind, its code and the
/// changed tokens' range in it (a removed line paired with an added one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DiffSide {
    pub kind: DiffKind,
    pub number: u32,
    pub body: String,
    pub words: Option<std::ops::Range<usize>>,
}

/// A diff as rows. A `Unified` row is one side, numbered by the file it is
/// in (the old file's for a removed line, the new file's else); `Split` pairs a removed line with the added line it became, a
/// side with no partner left empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DiffRow {
    /// `@@ -208,7 +208,11 @@ fn thread_row`: drawn only where the hunk
    /// names a section.
    Hunk(String),
    Unified(DiffSide),
    Split {
        left: Option<DiffSide>,
        right: Option<DiffSide>,
    },
}

impl DiffRow {
    /// The code a copy takes from this row, in order: every unified line;
    /// a split row's changed left side and its right side (context once).
    pub(super) fn selectable(&self) -> Vec<&DiffSide> {
        match self {
            Self::Hunk(_) => Vec::new(),
            Self::Unified(side) => vec![side],
            Self::Split { left, right } => {
                let left = left
                    .as_ref()
                    .filter(|side| side.kind != DiffKind::Context || right.is_none());
                left.into_iter().chain(right.as_ref()).collect()
            }
        }
    }
}

/// A line's tokens for word marks (CT-22): identifier runs
/// (`[A-Za-z0-9_]+`, so `0x7fbf95` is one), whitespace runs, and every
/// other character alone.
fn diff_tokens(line: &str) -> Vec<std::ops::Range<usize>> {
    let mut tokens = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        let word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
        let mut end = start + ch.len_utf8();
        if word(ch) {
            while let Some(&(at, next)) = chars.peek() {
                if !word(next) {
                    break;
                }
                end = at + next.len_utf8();
                chars.next();
            }
        } else if ch.is_whitespace() {
            while let Some(&(at, next)) = chars.peek() {
                if !next.is_whitespace() {
                    break;
                }
                end = at + next.len_utf8();
                chars.next();
            }
        }
        tokens.push(start..end);
    }
    tokens
}

/// The changed tokens between a removed line and the added line it became:
/// what is left of each once their common leading and trailing characters
/// go, snapped to whole tokens — the tokens wholly inside what changed, or,
/// where no whole token is (`0x7fbf95` → `0x93cf8c` past their common `0x`;
/// `live` past `if l`), the tokens it touches — and trimmed of whitespace
/// at its edges. `None` when nothing is common (the whole line changed) or
/// nothing differs.
fn changed_words(old: &str, new: &str) -> Option<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    // Common leading and trailing characters, in bytes, never splitting a
    // character and never overlapping.
    let prefix = old
        .char_indices()
        .zip(new.chars())
        .take_while(|((_, x), y)| x == y)
        .last()
        .map_or(0, |((at, ch), _)| at + ch.len_utf8());
    let suffix = old[prefix..]
        .chars()
        .rev()
        .zip(new[prefix..].chars().rev())
        .take_while(|(x, y)| x == y)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    if prefix == 0 && suffix == 0 {
        return None;
    }
    let span = |line: &str| -> std::ops::Range<usize> {
        let tokens = diff_tokens(line);
        let (start, end) = (prefix, line.len() - suffix);
        let solid = |token: &&std::ops::Range<usize>| !line[(*token).clone()].trim().is_empty();
        // The tokens wholly inside the change, else those it touches.
        let inside: Vec<&std::ops::Range<usize>> = tokens
            .iter()
            .filter(|token| token.start >= start && token.end <= end)
            .filter(solid)
            .collect();
        let picked = if inside.is_empty() {
            tokens
                .iter()
                .filter(|token| token.start < end && token.end > start)
                .filter(solid)
                .collect()
        } else {
            inside
        };
        match (picked.first(), picked.last()) {
            (Some(first), Some(last)) => first.start..last.end,
            _ => start.min(line.len())..start.min(line.len()),
        }
    };
    let (old_range, new_range) = (span(old), span(new));
    if old_range.is_empty() && new_range.is_empty() {
        return None;
    }
    Some((old_range, new_range))
}

/// How alike two changed lines are for pairing them side by side: the
/// tokens their ends share (common leading plus trailing tokens, whitespace
/// aside) over the longer line's tokens. Lines that begin or end alike —
/// `if let Some(…) {` and `if live {` — pair; lines that share only words in
/// the middle do not.
fn line_affinity(old: &str, new: &str) -> f32 {
    let words = |line: &str| -> Vec<String> {
        diff_tokens(line)
            .into_iter()
            .map(|range| line[range].to_string())
            .filter(|token| !token.trim().is_empty())
            .collect()
    };
    let (a, b) = (words(old), words(new));
    if a.is_empty() || b.is_empty() {
        return 0.;
    }
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    (prefix + suffix) as f32 / a.len().max(b.len()) as f32
}

/// Which removed and added lines of one change pair up, in the order the
/// rows draw them: the alignment that keeps both sides in order and
/// maximizes the pairs' affinity (`line_affinity`), unpaired lines on rows
/// of their own. `(Some(i), Some(j))` pairs; `(Some(i), None)` and `(None,
/// Some(j))` stand alone.
fn pair_lines(removed: &[&str], added: &[&str]) -> Vec<(Option<usize>, Option<usize>)> {
    let (n, m) = (removed.len(), added.len());
    let affinity: Vec<Vec<f32>> = removed
        .iter()
        .map(|old| added.iter().map(|new| line_affinity(old, new)).collect())
        .collect();
    // best[i][j]: the most affinity pairing removed[i..] with added[j..].
    let mut best = vec![vec![0f32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let pair = if affinity[i][j] > 0. {
                affinity[i][j] + best[i + 1][j + 1]
            } else {
                f32::MIN
            };
            best[i][j] = pair.max(best[i + 1][j]).max(best[i][j + 1]);
        }
    }
    let mut rows = Vec::with_capacity(n.max(m));
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        let pair =
            affinity[i][j] > 0. && (affinity[i][j] + best[i + 1][j + 1] - best[i][j]).abs() < 1e-6;
        if pair {
            rows.push((Some(i), Some(j)));
            i += 1;
            j += 1;
        } else if (best[i][j + 1] - best[i][j]).abs() < 1e-6 && best[i][j + 1] >= best[i + 1][j] {
            rows.push((None, Some(j)));
            j += 1;
        } else {
            rows.push((Some(i), None));
            i += 1;
        }
    }
    rows.extend((i..n).map(|i| (Some(i), None)));
    rows.extend((j..m).map(|j| (None, Some(j))));
    rows
}

thread_local! {
    /// Sections found by reading a post-edit file, once per file and hunk.
    static FILE_SECTIONS: std::cell::RefCell<HashMap<(String, u32), Option<String>>> =
        std::cell::RefCell::new(HashMap::new());
}

/// A hunk's section where its own context named none (a `structuredPatch`
/// hunk deep in an item): git's default funcname rule over the post-edit
/// file — the nearest line above the hunk that starts with a letter, `_` or
/// `$` — read once per file and hunk. `None` for a path that is not an
/// absolute file on disk.
fn file_section(path: &str, hunk: &ferrite_core::Hunk) -> Option<String> {
    if !std::path::Path::new(path).is_absolute() || hunk.new_start <= 1 {
        return None;
    }
    FILE_SECTIONS.with(|cache| {
        cache
            .borrow_mut()
            .entry((path.to_string(), hunk.new_start))
            .or_insert_with(|| {
                let text = std::fs::read_to_string(path).ok()?;
                text.lines()
                    .take(hunk.new_start.saturating_sub(1) as usize)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .find(|line| ferrite_core::transcript::funcname_line(line))
                    .and_then(ferrite_core::transcript::section_head)
            })
            .clone()
    })
}

/// A diff's rows, at most `cap` code lines (`hunk_rows`), unified or split:
/// a header only where the hunk names a section; each change's removed and
/// added lines paired by likeness (`pair_lines`), their changed tokens
/// marked.
pub(super) fn diff_rows(diff: &Diff, cap: usize, split: bool) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    let mut drawn = 0usize;
    for hunk in &diff.hunks {
        if drawn == cap {
            break;
        }
        if let Some(section) = hunk
            .section
            .clone()
            .or_else(|| file_section(&diff.path, hunk))
        {
            rows.push(DiffRow::Hunk(format!(
                "@@ -{},{} +{},{} @@ {section}",
                hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
            )));
        }
        let (mut old, mut new) = (hunk.old_start, hunk.new_start);
        let lines: Vec<&str> = hunk
            .lines
            .iter()
            .take(cap - drawn)
            .map(String::as_str)
            .collect();
        drawn += lines.len();
        let mut at = 0;
        while at < lines.len() {
            match DiffKind::of(lines[at]) {
                DiffKind::Context => {
                    let body = text::diff_body(lines[at]).to_owned();
                    let side = |number| DiffSide {
                        kind: DiffKind::Context,
                        number,
                        body: body.clone(),
                        words: None,
                    };
                    rows.push(if split {
                        DiffRow::Split {
                            left: Some(side(old)),
                            right: Some(side(new)),
                        }
                    } else {
                        DiffRow::Unified(side(new))
                    });
                    old += 1;
                    new += 1;
                    at += 1;
                }
                _ => {
                    // A change: its removed run, then its added run, paired
                    // by likeness for the rows and the word marks.
                    let removed: Vec<&str> = lines[at..]
                        .iter()
                        .take_while(|line| DiffKind::of(line) == DiffKind::Removed)
                        .map(|line| text::diff_body(line))
                        .collect();
                    at += removed.len();
                    let added: Vec<&str> = lines[at..]
                        .iter()
                        .take_while(|line| DiffKind::of(line) == DiffKind::Added)
                        .map(|line| text::diff_body(line))
                        .collect();
                    at += added.len();
                    let pairs = pair_lines(&removed, &added);
                    let mut left: Vec<DiffSide> = removed
                        .iter()
                        .enumerate()
                        .map(|(index, body)| DiffSide {
                            kind: DiffKind::Removed,
                            number: old + index as u32,
                            body: (*body).to_owned(),
                            words: None,
                        })
                        .collect();
                    let mut right: Vec<DiffSide> = added
                        .iter()
                        .enumerate()
                        .map(|(index, body)| DiffSide {
                            kind: DiffKind::Added,
                            number: new + index as u32,
                            body: (*body).to_owned(),
                            words: None,
                        })
                        .collect();
                    for (i, j) in &pairs {
                        if let (Some(i), Some(j)) = (i, j) {
                            if let Some((was, became)) = changed_words(removed[*i], added[*j]) {
                                left[*i].words = Some(was);
                                right[*j].words = Some(became);
                            }
                        }
                    }
                    old += removed.len() as u32;
                    new += added.len() as u32;
                    if split {
                        for (i, j) in pairs {
                            rows.push(DiffRow::Split {
                                left: i.map(|i| left[i].clone()),
                                right: j.map(|j| right[j].clone()),
                            });
                        }
                    } else {
                        rows.extend(left.into_iter().chain(right).map(DiffRow::Unified));
                    }
                }
            }
        }
    }
    rows
}

/// One styled run of a line.
type Highlight = (std::ops::Range<usize>, HighlightStyle);
/// A changed-token span and the deeper hue it is washed in.
type WordWash = (std::ops::Range<usize>, gpui::Hsla);

/// A diff line's syntax colours, and its changed tokens' wash: the range
/// and the deeper hue `CellCut` paints under them as the prototype's inline
/// `.wr` / `.wa` span boxes (the text's own content box, not the row).
fn diff_highlights(side: &DiffSide, language: Option<&str>) -> (Vec<Highlight>, Option<WordWash>) {
    let syntax = language
        .map(|language| {
            let tokens = ferrite_core::transcript::highlight_tokens(Some(language), &side.body);
            code(&side.body, Some(&tokens))
        })
        .unwrap_or_default();
    let wash = match side.kind {
        DiffKind::Added => Some((theme::DIFF_ADDED_WORD, theme::DIFF_ADDED_WORD_ALPHA)),
        DiffKind::Removed => Some((theme::DIFF_REMOVED_WORD, theme::DIFF_REMOVED_WORD_ALPHA)),
        DiffKind::Context => None,
    };
    let words = side
        .words
        .clone()
        .filter(|words| !words.is_empty())
        .zip(wash)
        .map(|(words, (wash, alpha))| {
            let wash: gpui::Hsla = rgb(wash >> 8).into();
            (words, wash.opacity(alpha))
        });
    let syntax = match &words {
        // The changed tokens are a span of their own: the runs break at
        // its edges, as the prototype's `.wa` / `.wr` breaks its line.
        Some((words, _)) => syntax
            .into_iter()
            .flat_map(|(range, style)| {
                let mut cuts = vec![range.start];
                cuts.extend(
                    [words.start, words.end]
                        .into_iter()
                        .filter(|cut| range.start < *cut && *cut < range.end),
                );
                cuts.push(range.end);
                cuts.windows(2)
                    .map(|cut| (cut[0]..cut[1], style))
                    .collect::<Vec<_>>()
            })
            .collect(),
        None => syntax,
    };
    (syntax, words)
}

/// A diff under its call (SPEC §5.4): a block box (`block_box`) on the
/// content column, set `TX_BLOCK_STEP` under the transcript. A `TEXT_DIM`
/// hunk header opens a hunk that names a section (none otherwise); each
/// row is `[number][sign][code]`: the number right-aligned in
/// `DIFF_NUMBER_CELLS` with `DIFF_NUMBER_PAD_CELLS` after it, in
/// `TEXT_FAINT` or the add/delete number tint; the sign (`DIFF_SIGN_CELLS`)
/// in its hue; the code in full syntax colour with its indentation intact,
/// exactly one line tall, cut with `…` at the column's edge. Removed and
/// added rows wear their washes, the changed tokens a deeper one. `wide`
/// lays it side by side, a 1px `paint::LINE` between the halves and an
/// empty side on `paint::NODIFF`.
///
/// The code cells route through the overlay — their lines copy honestly and
/// whole, however they are cut; numbers, signs and headers are chrome and
/// never do (#27). Without a selection (`None`) nothing registers.
///
/// The diff draws at most `HUNK_MAX_ROWS` lines and then names what it
/// left out.
fn render_diff(
    selection: Option<(&TextRuns, BlockId)>,
    diff: &Diff,
    grid: Grid,
    wide: bool,
) -> impl IntoElement {
    let content = grid;
    let grid = grid.block();
    let total: usize = diff.hunks.iter().map(|hunk| hunk.lines.len()).sum();
    let (cap, omitted) = hunk_rows(total);
    let cells = diff_number_cells(diff_max_number(diff, cap));
    let number_w = cells * grid.cell();
    let sign_w = theme::DIFF_SIGN_CELLS * grid.cell();
    let language = ferrite_core::transcript::language_for_path(std::path::Path::new(&diff.path));
    let number = |number: u32, ink: u32| {
        components::tabular(
            div()
                .flex_shrink_0()
                .w(px(number_w))
                .pr(px(theme::DIFF_NUMBER_PAD_CELLS * grid.cell()))
                .text_right()
                .whitespace_nowrap()
                .text_color(rgb(ink))
                .child(SharedString::from(number.to_string())),
        )
    };
    // One side of a row: its number, sign and code, and its wash.
    let side_cells = |side: &DiffSide, selectable: bool| {
        let DiffPaint {
            sign,
            sign_color,
            number_color,
            code_color,
            wash,
        } = side.kind.paint();
        let (highlights, words) = diff_highlights(side, language);
        let runs = highlights.iter().map(|(range, _)| range.clone()).collect();
        let cut = CellCut::new(
            side.body.clone(),
            words,
            rgb(code_color).into(),
            grid.cell(),
        )
        .runs(runs);
        let code = match selection {
            Some((selection, block)) if selectable => selection
                .line(block, side.body.clone(), highlights)
                .into_any_element(),
            _ => StyledText::new(SharedString::from(side.body.clone()))
                .with_highlights(highlights)
                .into_any_element(),
        };
        div()
            .flex()
            .items_start()
            .flex_1()
            .min_w_0()
            .h(px(grid.line))
            .overflow_hidden()
            .when_some(wash, |row, wash| row.bg(rgba(wash)))
            .child(number(side.number, number_color))
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(sign_w))
                    .whitespace_nowrap()
                    .text_color(rgb(sign_color))
                    .child(sign),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(rgb(code_color))
                    .child(cut.child(code)),
            )
    };
    let empty_side = || {
        div()
            .flex_1()
            .min_w_0()
            .h(px(grid.line))
            .bg(theme::paint::NODIFF)
    };
    let mut lines = div()
        .debug_selector(|| "tool-diff".into())
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .text_color(rgb(TEXT));
    for row in diff_rows(diff, cap, wide) {
        lines = lines.child(match &row {
            DiffRow::Hunk(header) => div()
                .debug_selector(|| "diff-hunk".into())
                .w_full()
                .min_w_0()
                .h(px(grid.line))
                .pl(px(number_w + sign_w))
                .truncate()
                .text_color(rgb(TEXT_DIM))
                .child(SharedString::from(header.clone())),
            DiffRow::Unified(side) => div()
                .flex()
                .w_full()
                .min_w_0()
                .child(side_cells(side, true)),
            DiffRow::Split { left, right } => {
                let selectable = row.selectable();
                let pick =
                    |side: &DiffSide| selectable.iter().any(|picked| std::ptr::eq(*picked, side));
                let half = |side: &Option<DiffSide>| match side {
                    Some(side) => side_cells(side, pick(side)),
                    None => empty_side(),
                };
                div()
                    .debug_selector(|| "diff-split".into())
                    .flex()
                    .items_stretch()
                    .w_full()
                    .min_w_0()
                    .child(half(left))
                    .child(div().flex_shrink_0().w(px(1.)).bg(theme::paint::LINE))
                    .child(half(right))
            }
        });
    }
    // What the cap left out, on the code column — never a silent truncation.
    if omitted > 0 {
        lines = lines.child(
            div()
                .debug_selector(|| "diff-omitted".into())
                .w_full()
                .min_w_0()
                .h(px(grid.line))
                .pl(px(number_w + sign_w))
                .truncate()
                .text_color(rgb(TEXT_DIM))
                .child(SharedString::from(format!("\u{2026} +{omitted} lines"))),
        );
    }
    content_row(content).pt(px(theme::BLOCK_MARGIN_T)).child(
        block_box(grid)
            .debug_selector(|| "diff-box".into())
            .child(lines),
    )
}

/// The box a diff or a code fence sits in (SPEC §5.4): `paint::BAR`,
/// `R_CONTROL` corners, `BLOCK_PAD_Y` above and below its lines, and an
/// inset 1px `HAIRLINE` laid over it (so it moves nothing), set at the
/// block's grid (`Grid::block`).
pub(crate) fn block_box(grid: Grid) -> Div {
    div()
        .relative()
        .w_full()
        .min_w_0()
        .py(px(theme::BLOCK_PAD_Y))
        .rounded(px(theme::R_CONTROL))
        .bg(theme::paint::BAR)
        .text_size(px(grid.size))
        .line_height(px(grid.line))
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(px(theme::R_CONTROL))
                .border_1()
                .border_color(theme::paint::HAIRLINE),
        )
}

/// A one-line code run cut at its column's edge as the prototype's
/// `text-overflow: ellipsis` cuts it: the whole cells that leave room for
/// `…`, then `…` in the cell's own ink (the browser draws the ellipsis in
/// the block's colour, whatever token it hides). gpui's own truncation
/// trims trailing punctuation before its `…` (`clone…` where the browser
/// draws `clone(…`), so a diff row draws its text whole and this clips it;
/// the text itself — what a selection copies — is never cut.
///
/// It also paints the changed tokens' wash (`words`) the way the browser
/// paints an inline span's background: over the face's content box —
/// ascent plus descent, each rounded, a whole-pixel half-leading above —
/// not the row (17px inside a 20px row at 13px), so washes on adjacent
/// rows stay apart. A wash that runs past the cut keeps washing, unbroken
/// under the `…`, to the column's edge.
pub(crate) struct CellCut {
    text: String,
    words: Option<WordWash>,
    ink: gpui::Hsla,
    cell: f32,
    /// Where the run's styled runs break, in cells: each run is laid out
    /// from the last one's width rounded up to the layout unit (gpui's
    /// text paint, vendor/gpui-pre), so a cell's x counts them.
    breaks: Vec<usize>,
    child: Option<AnyElement>,
}

impl CellCut {
    fn new(text: String, words: Option<WordWash>, ink: gpui::Hsla, cell: f32) -> Self {
        Self {
            text,
            words,
            ink,
            cell,
            breaks: Vec::new(),
            child: None,
        }
    }

    /// The styled runs the cell's text is painted in (byte ranges).
    fn runs(mut self, runs: Vec<std::ops::Range<usize>>) -> Self {
        let cell_of = |at: usize| self.text[..at.min(self.text.len())].chars().count();
        let total = self.text.chars().count();
        let mut breaks: Vec<usize> = runs
            .iter()
            .flat_map(|run| [cell_of(run.start), cell_of(run.end)])
            .filter(|at| *at > 0 && *at < total)
            .collect();
        breaks.sort_unstable();
        breaks.dedup();
        self.breaks = breaks;
        self
    }

    /// The x of cell `at` from the run's start: its cells, each run before
    /// it rounded up to the layout unit.
    fn x_at(&self, at: usize) -> f32 {
        let unit = |width: f32| (width * 64.0).ceil() / 64.0;
        let mut x = 0.0;
        let mut from = 0;
        for &cut in self.breaks.iter().filter(|cut| **cut <= at) {
            x += unit((cut - from) as f32 * self.cell);
            from = cut;
        }
        x + (at - from) as f32 * self.cell
    }

    pub(crate) fn child(mut self, child: AnyElement) -> Self {
        self.child = Some(child);
        self
    }

    /// How many whole cells stay when `width` cannot hold the run: the
    /// cells that leave one for `…`. `None` when the whole run fits.
    fn kept(&self, width: f32) -> Option<usize> {
        let whole = self.text.chars().count() as f32 * self.cell;
        (whole > width + 0.01)
            .then(|| ((width - self.cell) / self.cell + 0.001).floor().max(0.) as usize)
    }

    /// The cells the wash covers, `[from, to)`, and whether it reaches the
    /// first hidden cell (then it runs on under the `…` to the edge).
    fn wash_cells(&self, kept: Option<usize>) -> Option<(usize, usize, bool)> {
        let (range, _) = self.words.as_ref()?;
        let cell_of = |at: usize| self.text[..at.min(self.text.len())].chars().count();
        let (from, to) = (cell_of(range.start), cell_of(range.end));
        match kept {
            Some(kept) if from > kept => None,
            Some(kept) => Some((from, to.min(kept), to > kept)),
            None => Some((from, to, false)),
        }
    }
}

/// The browser's inline content box in a line of `line_height`: its top
/// below the line's and its height — the face's ascent plus descent,
/// rounded to the pixel (17px at 13px), under a floored half-leading.
fn inline_content_box(window: &gpui::Window) -> (f32, f32) {
    let style = window.text_style();
    let size = style.font_size.to_pixels(window.rem_size());
    let line_height = f32::from(style.line_height_in_pixels(window.rem_size()));
    let text_system = window.text_system();
    let font = text_system.resolve_font(&style.font());
    let ascent = f32::from(text_system.ascent(font, size)).abs();
    let descent = f32::from(text_system.descent(font, size)).abs();
    let height = (ascent + descent).round().min(line_height);
    (((line_height - height) / 2.).floor().max(0.), height)
}

impl IntoElement for CellCut {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl gpui::Element for CellCut {
    /// The child's layout node: the cut and the wash measure from its
    /// unrounded box, where the run's glyphs stand.
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
        // The run keeps its glyphs where they fall; the wash is a box, its
        // edges on the whole pixel the browser rounds a span's background
        // to (`components::css_px`).
        let kept = self.kept(f32::from(bounds.size.width));
        let left = f32::from(bounds.left());
        let cut = kept.map(|kept| px(left + self.x_at(kept)));
        if let (Some((from, to, runs_on)), Some((_, wash))) =
            (self.wash_cells(kept), self.words.as_ref())
        {
            let (top, height) = inline_content_box(window);
            let edge = |x: f32| px(crate::components::css_px(x));
            let x0 = edge(left + self.x_at(from));
            let x1 = match (runs_on, cut) {
                (true, _) => edge(f32::from(bounds.right())),
                (false, Some(cut)) => edge(left + self.x_at(to)).min(edge(f32::from(cut))),
                (false, None) => edge(left + self.x_at(to)),
            };
            if x1 > x0 {
                let y0 = bounds.top() + px(top);
                window.paint_quad(gpui::fill(
                    gpui::Bounds::from_corners(
                        gpui::point(x0, y0),
                        gpui::point(x1, y0 + px(height)),
                    ),
                    *wash,
                ));
            }
        }
        let Some(cut) = cut else {
            child.paint(window, cx);
            return;
        };
        let shown = gpui::Bounds::from_corners(bounds.origin, gpui::point(cut, bounds.bottom()));
        window.with_content_mask(Some(gpui::ContentMask { bounds: shown }), |window| {
            child.paint(window, cx)
        });
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
        let _ = line.paint(
            gpui::point(cut, bounds.top()),
            line_height,
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        );
    }
}

/// How many of a diff's rows the card draws, and how many are left for
/// the omission line to account for. Split out so the arithmetic the card
/// depends on is assertable without a window.
fn hunk_rows(total: usize) -> (usize, usize) {
    let drawn = total.min(theme::HUNK_MAX_ROWS);
    (drawn, total - drawn)
}

/// The largest line number the diff will draw.
fn diff_max_number(diff: &Diff, cap: usize) -> usize {
    let mut drawn = 0;
    let mut max = 0usize;
    for hunk in &diff.hunks {
        let (mut old, mut new) = (hunk.old_start, hunk.new_start);
        for line in &hunk.lines {
            if drawn == cap {
                return max;
            }
            drawn += 1;
            match DiffKind::of(line) {
                DiffKind::Added => new += 1,
                DiffKind::Removed => old += 1,
                DiffKind::Context => {
                    old += 1;
                    new += 1;
                }
            }
            max = max
                .max(old.saturating_sub(1) as usize)
                .max(new.saturating_sub(1) as usize);
        }
    }
    max
}

/// The number column in cells: `DIFF_NUMBER_CELLS` (three digits and the
/// `DIFF_NUMBER_PAD_CELLS` after them), wider only for a longer number.
pub(crate) fn diff_number_cells(max: usize) -> f32 {
    let digits = max.max(1).ilog10() as f32 + 1.;
    theme::DIFF_NUMBER_CELLS.max(digits + theme::DIFF_NUMBER_PAD_CELLS)
}

/// What a unified-diff line is, read from its first byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiffKind {
    Added,
    Removed,
    Context,
}

/// A diff row's colours: the sign column, the code, and the row's wash.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DiffPaint {
    pub sign: &'static str,
    pub sign_color: u32,
    /// The line number's ink: the sign's hue dimmed, structure on a
    /// context row.
    pub number_color: u32,
    pub code_color: u32,
    pub wash: Option<u32>,
}

impl DiffKind {
    pub(crate) fn of(line: &str) -> Self {
        match line.chars().next() {
            Some('+') => Self::Added,
            Some('-') => Self::Removed,
            _ => Self::Context,
        }
    }

    /// Both CLIs sign a diff in ASCII, `+` and `-`. The sign carries the
    /// hue (`DIFF_*_SIGN`); the code keeps body ink under its syntax
    /// colours on the row's wash; a context line is the same code with no
    /// wash.
    pub(crate) fn paint(self) -> DiffPaint {
        match self {
            Self::Added => DiffPaint {
                sign: "+",
                sign_color: theme::DIFF_ADDED_SIGN,
                number_color: theme::DIFF_ADDED_NUMBER,
                code_color: DIFF_ADDED_INK,
                wash: Some(DIFF_ADDED_WASH),
            },
            Self::Removed => DiffPaint {
                sign: "-",
                sign_color: theme::DIFF_REMOVED_SIGN,
                number_color: theme::DIFF_REMOVED_NUMBER,
                code_color: DIFF_REMOVED_INK,
                wash: Some(DIFF_REMOVED_WASH),
            },
            Self::Context => DiffPaint {
                sign: "",
                sign_color: TEXT_DIM,
                number_color: TEXT_FAINT,
                code_color: theme::SYN_PLAIN,
                wash: None,
            },
        }
    }
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
        // `strong`: weight 600 in `TEXT_STRONG`.
        Style::Bold => Some(HighlightStyle {
            color: Some(rgb(TEXT_STRONG).into()),
            font_weight: Some(W_STRONG),
            ..Default::default()
        }),
        // A link or a path reads `TEXT_STRONG`, with no underline at rest.
        // Inert — paths render, nothing opens.
        Style::Link => Some(HighlightStyle {
            color: Some(rgb(theme::LINK_INK).into()),
            ..Default::default()
        }),
    }
}

/// A prose Block's text: one wrapping run, so a sentence breaks where the
/// terminal's does.
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
pub(crate) fn init_components(cx: &mut gpui::App) {
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
    crate::theme::init_components(cx);
    if first {
        crate::attachments::init(cx);
        crate::rich::init(cx);
    }
}

/// Syntax highlight runs for a code Block, or none while the highlighter is
/// still thinking.
pub(crate) fn code(
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
    #[test]
    fn progress_token_counts_stay_compact() {
        assert_eq!(tokens_label(340), "340");
        assert_eq!(tokens_label(8_040), "8.0k");
        assert_eq!(tokens_label(12_400), "12k");
    }
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
            "Start a thread",
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

    #[gpui::test]
    fn progress_metadata_shares_the_caption_line_without_a_duplicate_command(
        cx: &mut TestAppContext,
    ) {
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
            let footer = cx
                .debug_bounds("progress-caption-Checking build progress")
                .unwrap();
            // One row, as the CLIs draw it: the caption, then its facts.
            assert_eq!(metadata.top(), reasoning.top());
            assert!(metadata.left() >= reasoning.right());
            assert_eq!(
                footer.size.height,
                px(theme::LH_PROSE),
                "one line, and no command detail below it"
            );
            assert!(metadata.right() <= px(width), "the caption truncates first");
            let interrupt = cx.debug_bounds("progress-interrupt").unwrap();
            assert!(
                interrupt.left() >= metadata.right(),
                "`esc to interrupt` closes the line, after its facts"
            );
        }
    }

    struct StartingAndWorking(Transcript);

    impl Render for StartingAndWorking {
        fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
            let line = |selector: &'static str, line: Div| {
                div().debug_selector(move || selector.into()).child(line)
            };
            let working =
                |solo| working_line_for(&self.0, false, solo, false, false, true, None, 0);
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
                provider: None,
                timings: HashMap::new(),
                focused: true,
                reading_size: Default::default(),
                selection_scope: gpui::base::TextSelectionScopeId::new(),
                preview: crate::preview::Preview::new(cx),
                expanded,
                target: None,
                disclosure_focus: cx.focus_handle(),
                tail: None,
                banner: None,
                pending_call: None,
                workspace: None,
                solo: false,
                thinking: true,
                settled_at: Default::default(),
                #[cfg(test)]
                disclosure_bounds: Rc::new(RefCell::new(HashMap::new())),
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
                    provider: None,
                    timings: HashMap::new(),
                    focused: true,
                    reading_size: Default::default(),
                    selection_scope: gpui::base::TextSelectionScopeId::new(),
                    preview: crate::preview::Preview::new(cx),
                    expanded: HashSet::new(),
                    target: None,
                    disclosure_focus: cx.focus_handle(),
                    tail: None,
                    banner: None,
                    pending_call: None,
                    workspace: None,
                    solo: false,
                    thinking: true,
                    settled_at: Default::default(),
                    #[cfg(test)]
                    disclosure_bounds: Rc::new(RefCell::new(HashMap::new())),
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

    struct ShowsDecisions {
        cache: crate::rich::TextCache,
        decisions: Vec<Decision>,
    }

    impl Render for ShowsDecisions {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut Context<Self>,
        ) -> impl IntoElement {
            div()
                .flex()
                .flex_col()
                .w(px(900.))
                .font_family(crate::theme::FONT_UI)
                .text_size(px(crate::theme::FS_UI))
                .line_height(px(crate::theme::LH_UI))
                .children(self.decisions.iter().enumerate().map(|(at, decision)| {
                    let rows = decision::approval_rows(decision, None, None)
                        .into_iter()
                        .enumerate()
                        .map(|(row_at, row)| {
                            decision::option_row(
                                ("decision-row", at * 16 + row_at),
                                decision::Row {
                                    key: Some(row.key),
                                    label: row.label,
                                    code: row.code,
                                    cursor: row_at == 0,
                                    enabled: row.enabled,
                                    ..Default::default()
                                },
                            )
                            .into_any_element()
                        });
                    decision::card(
                        at as u64,
                        [
                            decision::head(
                                decision::kind_word(decision),
                                Some(decision.tool_name.clone().into()),
                                None,
                            )
                            .into_any_element(),
                            decision::prose(ferrite_core::digest::decision_subject(decision))
                                .into_any_element(),
                        ]
                        .into_iter()
                        .chain(
                            approval_input(decision, &self.cache, "decision-reference".into()).map(
                                |input| {
                                    decision::well(shell_command(decision), input)
                                        .into_any_element()
                                },
                            ),
                        )
                        .chain(rows),
                    )
                }))
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
            name: "/import".into(),
            matched: vec![],
            detail: "adopt a CLI session".into(),
            prose_detail: true,
            inert: false,
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
        };
        assert_eq!(cursor(menu_row(("r", 1usize), &inert, false, None)), None);
        assert_eq!(cursor(menu_row(("r", 1usize), &inert, true, None)), None);
        // ↵ rides the cursor row only, and never an inert one.
        assert_eq!(menu_item(&offer, true, None).shortcut.as_deref(), Some("↵"));
        assert_eq!(menu_item(&offer, false, None).shortcut, None);
        assert_eq!(menu_item(&inert, true, None).shortcut, None);
        // A long directory keeps its tail.
        let deep = MenuRow {
            detail: "crates/ferrite/src/some/very/deeply/nested/module/tree/of/files".into(),
            prose_detail: false,
            ..inert
        };
        let detail = menu_item(&deep, false, None).detail.unwrap();
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
            tool_verdicts(failed_edit),
            vec![ToolVerdict::Diff(1, 1), ToolVerdict::Failed]
        );
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
        assert_eq!(tool_verdicts(tool), [ToolVerdict::Diff(2, 2)]);
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
            shut.contains("Ran 2 commands, reading 1 file \u{b7} 1 failed"),
            "{shut}"
        );
        // Shut, it draws its working call alone.
        assert!(shut.contains("Read(notes.md)"), "{shut}");
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
            "12 matched",
            "grep -n adelaide",
            "Read(notes.md)",
        ] {
            assert!(open.contains(text), "{text}: {open}");
        }
        // Each call hangs one gutter in, on the rail under the bullet.
        let summary = cx.debug_bounds("tool-group-g1").unwrap();
        let member = cx.debug_bounds("tool-row-g1").unwrap();
        let rail = cx.debug_bounds("tool-group-rail").unwrap();
        let gutter = px(Grid::of(Default::default()).gutter());
        assert!(
            // Boxes round to the pixel; the gutter keeps its fraction.
            ((member.left() - summary.left()) - gutter).abs() <= px(1.0),
            "{member:?} {summary:?} {gutter:?}"
        );
        assert!(rail.left() > summary.left() && rail.left() < member.left());
        assert_eq!(
            member.top(),
            summary.bottom(),
            "the calls hang flush under it"
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
        assert_eq!(
            text::group_summary(blocks).text,
            "Searched for 1 pattern, read 2 files, called github 2 times, \
             fetching 1 page \u{b7} 1 failed"
        );
        let summary = text::group_summary(&blocks[..2]);
        assert_eq!(summary.text, "Searched for 1 pattern, read 1 file");
        assert_eq!(summary.failed, None);
        let failed = text::group_summary(&blocks[..3]);
        assert_eq!(&failed.text[failed.failed.clone().unwrap()], "1 failed");
        assert_eq!(
            text::group_live(blocks).map(|block| block.id),
            Some(blocks[5].id)
        );
        assert_eq!(text::group_live(&blocks[..5]), None);
    }

    #[gpui::test]
    fn every_block_kind_registers_its_selectable_text(cx: &mut TestAppContext) {
        let transcript = every_kind();
        let instruments = Instruments::of(&transcript);
        let blocks: Vec<Block> = transcript.blocks().to_vec();
        let ids: Vec<ferrite_core::transcript::BlockId> = blocks
            .iter()
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
        view.update(cx, |view, cx| {
            view.reasoning_expanded = true;
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
            all.contains("Bash(cargo test)"),
            "tool pieces compose the call: {all}"
        );
        // Every call is its own row: the edit's diff hangs under it at
        // once, each changed line registering exactly once.
        assert_eq!(
            collapsed_and_expanded_count(&runs, "delta"),
            1,
            "the edit's diff shows under its call: {all}"
        );
        // A failure's line registers where it renders (Edit's); the test
        // run reads `ok · 42 passed` on its elbow. The `└` elbow is chrome
        // and never joins the run.
        assert!(all.contains("applied"), "the result line: {all}");
        assert!(all.contains("ok \u{b7} 42 passed"), "the test elbow: {all}");
        assert!(!all.contains("└"), "the elbow is chrome: {all}");

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

    #[gpui::test]
    fn a_blocked_thread_paints_its_decision_card(cx: &mut TestAppContext) {
        cx.update(gpui::component::init);
        let event = crate::demo::script()
            .into_iter()
            .map(|step| step.event)
            .find(|event| matches!(event, SessionEvent::DecisionRequested { .. }))
            .expect("the demo stops on a Decision");
        let SessionEvent::DecisionRequested { decision } = event else {
            unreachable!()
        };
        assert_eq!(decision.tool_name, "Write");

        // A request Ferrite could not read is still a card, or the operator
        // has nothing to deny and the turn hangs.
        let unreadable = Decision {
            tool_name: String::new(),
            description: String::new(),
            ..decision.clone()
        };

        let (_, cx) = cx.add_window_view(|_, _| ShowsDecisions {
            cache: Default::default(),
            decisions: vec![decision, unreadable],
        });
        cx.simulate_resize(size(px(900.), px(300.)));
        cx.run_until_parked();
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

    /// glance.md §4's wall matrix, one assertion per row — the selection
    /// logic the wall cell renders from.
    #[test]
    fn the_wall_state_matrix_reads_exactly_as_the_glance_spec() {
        use WallState::*;
        let mut transcript = Transcript::default();
        assert_eq!(wall_state(Some(&Turn::of(&transcript)), false, false), Idle);
        transcript.apply(Input::Prompt("go".into()));
        // Working, focused or not, is the streaming Thread.
        assert_eq!(
            wall_state(Some(&Turn::of(&transcript)), false, false),
            Working
        );
        // Failing tests stay a working Thread — red text, not a ring.
        assert_eq!(
            wall_state(Some(&Turn::of(&transcript)), true, false),
            Decision
        );
        assert_eq!(
            wall_state(Some(&Turn::of(&transcript)), false, true),
            Failing
        );
        // A Decision waits: pending flag or Blocked status, either way.
        let decision = crate::demo::script()
            .into_iter()
            .map(|step| step.event)
            .find(|event| matches!(event, SessionEvent::DecisionRequested { .. }))
            .unwrap();
        transcript.apply(Input::Event(decision));
        assert_eq!(
            wall_state(Some(&Turn::of(&transcript)), false, false),
            Decision
        );
        // Only successful outcomes read Done, regardless of cost.
        for cost_usd in [None, Some(0.038)] {
            for (outcome, expected) in [
                (ferrite_core::TurnOutcome::Completed, Done),
                (ferrite_core::TurnOutcome::Interrupted, Idle),
                (ferrite_core::TurnOutcome::Error("failed".into()), Idle),
            ] {
                transcript.apply(Input::Event(SessionEvent::TurnEnded { outcome, cost_usd }));
                assert_eq!(
                    wall_state(Some(&Turn::of(&transcript)), false, false),
                    expected
                );
            }
        }
        // A closed Session is the red hard-blocker.
        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        }));
        transcript.apply(Input::Event(SessionEvent::Closed {
            reason: "Session exited".into(),
        }));
        assert_eq!(
            wall_state(Some(&Turn::of(&transcript)), false, false),
            Blocked
        );
        // No transcript at all — the cockpit could not open the Thread.
        assert_eq!(wall_state(None, false, false), Parked);
    }

    /// F-6: prose wraps once to a continuation at col 2 — at its own line
    /// break, else at the last space that fits — and the tile keeps its
    /// last `WALL_LINES` rows.
    #[test]
    fn wall_prose_wraps_once_inside_the_row_budget() {
        let prose = |text: &str| WallLine {
            text: text.to_string().into(),
            wraps: true,
        };
        assert_eq!(
            wall_rows(
                &[prose(
                    "\u{25cf} Drafting the decision section\nfrom the spike notes"
                )],
                80
            ),
            [
                "\u{25cf} Drafting the decision section",
                "  from the spike notes"
            ]
        );
        assert_eq!(
            wall_rows(
                &[prose(
                    "\u{25cf} Drafting the decision section from the spike notes"
                )],
                31
            ),
            [
                "\u{25cf} Drafting the decision section",
                "  from the spike notes"
            ]
        );
        // Short enough: one row. A call never wraps.
        assert_eq!(wall_rows(&[prose("\u{25cf} Done")], 31), ["\u{25cf} Done"]);
        let call = WallLine {
            text: "\u{25cf} Read(spikes/panes24/NOTES.md)".into(),
            wraps: false,
        };
        assert_eq!(wall_rows(std::slice::from_ref(&call), 10).len(), 1);
        // The budget is rows: a wrapped prose line spends two.
        let rows = wall_rows(
            &[
                call.clone(),
                call,
                prose("\u{25cf} Drafting the decision section from the spike notes"),
            ],
            31,
        );
        assert_eq!(rows.len(), theme::WALL_LINES);
        assert_eq!(rows[2].as_ref(), "  from the spike notes");
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
    /// sentence case, the default reads `Default`, and nothing shown
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
        assert_eq!(
            permission_mode_label("default", &claude).as_ref(),
            "Default"
        );
        assert_eq!(permission_mode_label("", &[]).as_ref(), "Default");
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
                code_color: DIFF_ADDED_INK,
                wash: Some(DIFF_ADDED_WASH),
            }
        );
        assert_eq!(
            DiffKind::Removed.paint(),
            DiffPaint {
                sign: "-",
                sign_color: theme::DIFF_REMOVED_SIGN,
                number_color: theme::DIFF_REMOVED_NUMBER,
                code_color: DIFF_REMOVED_INK,
                wash: Some(DIFF_REMOVED_WASH),
            }
        );
        assert_eq!(
            DiffKind::Context.paint(),
            DiffPaint {
                sign: "",
                sign_color: TEXT_DIM,
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
    fn a_tool_row_reads_its_outcome_from_its_bullet_and_keeps_its_name_neutral() {
        let failed = ToolState::Failed("boom".into());
        // Terminal-native: settled work is a dim `●`; live work a still
        // green one (only the working line moves); a failure the blocked
        // one; a lost result the hollow `○`. Green never means finished.
        assert_eq!(tool_dot_ink(&ToolState::Ok), (TEXT_DIM, DotShape::Solid));
        assert_eq!(
            tool_dot_ink(&ToolState::Running),
            (RUNNING, DotShape::Solid)
        );
        assert_eq!(tool_dot_ink(&failed), (BLOCKED, DotShape::Solid));
        assert_eq!(
            tool_dot_ink(&ToolState::Unavailable),
            (TEXT_FAINT, DotShape::Ring)
        );
        assert_eq!(DotShape::Solid.glyph(), "\u{25cf}");
        assert_eq!(DotShape::Ring.glyph(), "\u{25cb}");
        for state in [ToolState::Ok, ToolState::Running, failed.clone()] {
            let tool = ToolBlock {
                call: "c".into(),
                name: "Bash".into(),
                title: None,
                summary: "cargo test".into(),
                state,
                diffs: Vec::new(),
                structured_preview: None,
                result_line: None,
                output: None,
                progress: None,
            };
            assert_eq!(tool.label(), "Bash(cargo test)");
            let highlights = call_highlights(&tool);
            assert_eq!(
                highlights.len(),
                1,
                "one line: only the name is lifted, the arguments keep the line's ink"
            );
            assert_eq!(highlights[0].0, 0..4, "only the name is lifted");
            assert_eq!(
                highlights[0].1,
                HighlightStyle {
                    color: Some(rgb(TEXT_STRONG).into()),
                    font_weight: Some(theme::W_STRONG),
                    ..Default::default()
                },
                "the name is strong ink at 600 (SPEC §5.4), whatever happened"
            );
        }
        // A path argument reads `TEXT_DIM`, like the rest of the arguments.
        let read = ToolBlock {
            call: "r".into(),
            name: "Read".into(),
            title: None,
            summary: "crates/ferrite/src/nav.rs".into(),
            state: ToolState::Ok,
            diffs: Vec::new(),
            structured_preview: None,
            result_line: None,
            output: None,
            progress: None,
        };
        let highlights = call_highlights(&read);
        assert_eq!(highlights.len(), 2);
        assert_eq!(highlights[1].0, 5..30);
        assert_eq!(highlights[1].1.color, Some(rgb(theme::TEXT_DIM).into()));
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
    fn a_long_command_output_folds_after_the_line_its_elbow_shows() {
        let output = (1..=8)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let tool = |name: &str, state: ToolState| ToolBlock {
            call: "c".into(),
            name: name.into(),
            title: None,
            summary: "cargo test".into(),
            state,
            diffs: Vec::new(),
            structured_preview: None,
            result_line: Some("line 1".into()),
            output: Some(ferrite_core::transcript::ToolOutput {
                text: output.clone(),
                omitted_bytes: 0,
            }),
            progress: None,
        };
        let (shown, hidden) = output_fold(&tool("Bash", ToolState::Ok)).expect("a command folds");
        assert_eq!(shown, "line 2\nline 3\nline 4");
        assert_eq!(hidden, 4);
        // A read's output is the file: it does not fold under the call.
        assert!(output_fold(&tool("Read", ToolState::Ok)).is_none());
        // A failure folds whatever the tool.
        assert!(output_fold(&tool("Read", ToolState::Failed("boom".into()))).is_some());
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
        let unified = diff_rows(&diff, 24, false);
        assert_eq!(unified.len(), 5, "a hunk naming no section draws no header");
        let mut named = diff.clone();
        named.hunks[0].section = Some("fn a".into());
        assert_eq!(
            diff_rows(&named, 24, false)[0],
            DiffRow::Hunk("@@ -10,3 +10,3 @@ fn a".into())
        );
        let split = diff_rows(&diff, 24, true);
        assert_eq!(split.len(), 4, "the change pairs side by side");
        let DiffRow::Split { left, right } = &split[1] else {
            panic!("a split row")
        };
        assert_eq!(left.as_ref().unwrap().body, "    old();");
        assert_eq!(right.as_ref().unwrap().body, "    new();");
        assert_eq!(left.as_ref().unwrap().words, Some(4..7));
        let DiffRow::Split { left, right } = &split[2] else {
            panic!("a split row")
        };
        assert!(
            left.is_none(),
            "an added line with no partner leaves the left empty"
        );
        assert_eq!(right.as_ref().unwrap().number, 12);
        // A copy takes a context line once and both sides of a change.
        let copied: Vec<_> = split
            .iter()
            .flat_map(DiffRow::selectable)
            .map(|side| side.body.as_str())
            .collect();
        assert_eq!(
            copied,
            vec!["fn a() {", "    old();", "    new();", "    more();", "}"]
        );
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

    /// A turn's end leads with the lexicon's own lowercase words, and the
    /// answer's mark is structure ink.
    #[test]
    fn turn_ends_speak_the_lexicon_and_the_answer_bullet_is_bright() {
        use ferrite_core::transcript::TurnEnd;
        assert_eq!(TurnEnd::INTERRUPTED, theme::words::INTERRUPTED);
        assert_eq!(TurnEnd::FAILED, theme::words::FAILED);
        for (outcome, text) in [
            (
                ferrite_core::TurnOutcome::Interrupted,
                "interrupted \u{b7} 0.1s",
            ),
            (
                ferrite_core::TurnOutcome::Error("API Error: 529 overloaded".into()),
                "failed \u{b7} 0.1s \u{b7} API Error: 529 overloaded",
            ),
        ] {
            let end = TurnEnd {
                outcome,
                elapsed_ms: Some(100),
                completed_at: None,
                input_tokens: None,
                output_tokens: None,
            };
            assert_eq!(end.text(), text);
            let (head, message) = turn_end_runs(&end.text(), turn_end_message(&end));
            assert_eq!(
                format!("{head}{}", message.unwrap_or_default()),
                text,
                "the two runs copy back as the line"
            );
        }
        assert_eq!(crate::transcript::ANSWER_MARK_INK, TEXT_STRONG);
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
    fn rate_limit_resets_read_as_compact_countdowns() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        let after = |seconds: u64| Some(1_000_000 + seconds);
        let week = Duration::from_secs(7 * 86_400);

        let label = |at| reset_label(at, week, now).map(|label| label.to_string());
        assert_eq!(label(None), None, "an unreported reset reads nothing");
        assert_eq!(label(after(0)), None, "a reset due now is already past");
        assert_eq!(label(after(45)).as_deref(), Some("resets in <1m"));
        assert_eq!(label(after(42 * 60)).as_deref(), Some("resets in 42m"));
        assert_eq!(
            label(after(3 * 3_600 + 14 * 60)).as_deref(),
            Some("resets in 3h 14m")
        );
        assert_eq!(
            label(after(4 * 86_400 + 2 * 3_600)).as_deref(),
            Some("resets in 4d 2h")
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
    fn card_counts_and_costs_read_compactly() {
        assert_eq!(compact_count(640), "640");
        assert_eq!(compact_count(1_500), "1.5k");
        assert_eq!(compact_count(2_000), "2k");
        assert_eq!(compact_count(64_000), "64k");
        assert_eq!(compact_count(124_400), "124k");
        assert_eq!(compact_count(200_000), "200k");
        assert_eq!(compact_count(1_000_000), "1M");
        assert_eq!(compact_count(1_200_000), "1.2M");
        assert_eq!(cost_label(0.42), "$0.42");
        assert_eq!(cost_label(0.004), "<$0.01");
        assert_eq!(cost_label(1.5), "$1.50");
    }

    /// The painted tasks meter stays glanceable: a segment per step up to
    /// the cap, one continuous track past it, and done never overshoots.
    /// Replaces the ▰▱ glyph run, which Geist Mono cannot draw.
    #[test]
    fn tasks_meter_draws_segments_until_the_cap_then_a_track() {
        assert_eq!(
            meter_layout(3, 4),
            MeterLayout::Segments { done: 3, total: 4 }
        );
        assert_eq!(
            meter_layout(5, 4),
            MeterLayout::Segments { done: 4, total: 4 }
        );
        assert_eq!(
            meter_layout(0, theme::METER_SEG_CAP),
            MeterLayout::Segments {
                done: 0,
                total: theme::METER_SEG_CAP
            }
        );
        assert_eq!(meter_layout(5, 20), MeterLayout::Track { fraction: 0.25 });
        assert_eq!(meter_layout(30, 20), MeterLayout::Track { fraction: 1.0 });
    }

    /// F-2: one attention edge. A waiting Pane draws `WAIT_RING` on a
    /// board whether it holds focus or not — no inner ring, no full-ink
    /// answer target, no red blocked edge; a calm focused Pane draws the
    /// focus ring; at rest the card's own ring. Solo never recolours it.
    #[test]
    fn pane_edge_ranks_state_over_focus() {
        assert_eq!(PaneEdge::of(true, true, false), PaneEdge::Attention);
        assert_eq!(PaneEdge::of(false, true, false), PaneEdge::Attention);
        assert_eq!(PaneEdge::of(true, false, false), PaneEdge::Focused);
        assert_eq!(PaneEdge::of(false, false, false), PaneEdge::Rest);
        assert_eq!(PaneEdge::Rest.ink(), rgba(CARD_RING).into());
        assert_eq!(PaneEdge::Focused.ink(), rgba(FOCUS_RING).into());
        assert_eq!(PaneEdge::Attention.ink(), rgba(WAIT_RING).into());
        for focused in [false, true] {
            for attention in [false, true] {
                let edge = PaneEdge::of(focused, attention, true);
                assert!(
                    matches!(edge, PaneEdge::Rest | PaneEdge::Focused),
                    "solo {focused} {attention}: {edge:?}"
                );
            }
        }
    }

    /// A run's state word is coloured only when it failed; the dot beside
    /// it carries pending, passing and skipped.
    #[test]
    fn check_rows_colour_only_failure() {
        assert_eq!(check_detail_ink(CheckState::Failing), BLOCKED);
        for state in [
            CheckState::Passing,
            CheckState::Pending,
            CheckState::Skipped,
        ] {
            assert_eq!(check_detail_ink(state), TEXT_DIM);
        }
        // CI dots are never the ochre that means an agent waits on you.
        assert_eq!(check_ink(CheckState::Pending), RUNNING);
        assert_eq!(check_ink(CheckState::Passing), TEXT_DIM);
        assert_eq!(check_ink(CheckState::Failing), BLOCKED);
        assert_eq!(check_ink(CheckState::Skipped), TEXT_FAINT);
        for state in [
            CheckState::Passing,
            CheckState::Pending,
            CheckState::Failing,
            CheckState::Skipped,
        ] {
            assert_ne!(check_ink(state), ATTENTION, "{state:?}");
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

    /// §D.7 (terminal-native, WP-D): the idle line says what the Pane is
    /// waiting on — a Decision, a live Thread, a running turn or a closed
    /// Session — in a couple of words, and, once a prediction lands, shows
    /// it verbatim as the operator's own next line. None ends in an
    /// ellipsis, a terminal placeholder's own grammar.
    #[test]
    fn the_placeholder_says_what_the_pane_is_waiting_on() {
        let live = Transcript::default();
        assert_eq!(
            placeholder(false, false, Some(&live), None).text(),
            "Steer this thread"
        );
        assert_eq!(
            placeholder(true, false, Some(&live), None).text(),
            "Answer above"
        );
        let mut running = Transcript::default();
        running.apply(Input::Prompt("fix the decoder".into()));
        running.apply(Input::Event(SessionEvent::TextDelta {
            text: "Looking".into(),
        }));
        assert_eq!(
            placeholder(false, false, Some(&running), None).text(),
            "Queue a follow-up"
        );

        let mut closed = Transcript::default();
        closed.apply(Input::Event(SessionEvent::Closed {
            reason: "the CLI exited".into(),
        }));
        assert_eq!(
            placeholder(false, false, Some(&closed), None).text(),
            "Revive and continue"
        );
        assert_eq!(
            placeholder(false, true, None, None).text(),
            "Start a thread"
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
            "Answer above"
        );
        assert_eq!(
            placeholder(false, false, Some(&closed), Some("Run the tests")).text(),
            "Revive and continue"
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
        assert_eq!(composer_fixed_height(false), 14. + 8. + 42. + 14.);
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
        // Solo's queue is one 34px row and its hairline, however many wait;
        // the compact box shows none (its Pane says `2 queued` unfocused).
        assert_eq!(composer_queue_height(900., false, 0), 0.);
        assert_eq!(composer_queue_height(900., false, 3), 35.);
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
        assert_eq!(usage_ink(0.62), TEXT);
        assert_eq!(usage_ink(0.79), TEXT);
        assert_eq!(usage_ink(theme::USAGE_TIGHT), ATTENTION);
        assert_eq!(usage_ink(0.9), ATTENTION);
        assert_eq!(usage_ink(0.95), ATTENTION);
        assert_eq!(usage_ink(1.0), ATTENTION);
        assert_eq!(theme::USAGE_TIGHT, 0.80);
    }

    // (end WP-D)

    // ---- WP-E tests (append above the end line)
    // (end WP-E)

    // ---- WP-F tests (append above the end line)
    // (end WP-F)

    // ---- WP-G tests (append above the end line)
    // (end WP-G)
}
