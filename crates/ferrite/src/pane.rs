//! One Pane: the visible cell for one Thread. Header, transcript, Composer,
//! and the three semantic-zoom renderings. Rendering only — everything it
//! shows is folded in core, and every key it answers to belongs to the
//! cockpit above it.
//!
//! The frame is the terminal-native board's (theme WP-C): a Pane is square
//! and flat on the reading plane (`paint::PLANE`); its content starts at
//! the Pane's own edges, and its 1px edge is an overlay laid over them
//! (the prototype's `.pane::after`) that only changes colour — transparent
//! at rest, the accent when focused on a board, `ATTENTION_EDGE` on every
//! side while it waits (a focused waiting Pane shows only the yellow; its
//! focus reads in the head ground and title). On a board it wears one head
//! row — the dot (a working Thread's braille spinner), the title, the
//! provider mark — and no state word; top to bottom then the subagent
//! strip, the transcript body (its Decision is the transcript's tail), the
//! Composer. Two altitudes (R12): the transcript, or the wall tile — the
//! state word in colour, the last lines dim, the quick answers.

mod text;
pub(crate) use text::collect_block_text;

use ferrite_core::activity::Subject;
use ferrite_core::cockpit::{ThreadView, ToolTiming};
use ferrite_core::docview::{is_test_run, passed_count, Instruments, Level, Tests};
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
use crate::composer::Composer;
#[allow(unused_imports)]
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
    pub preview: crate::attachment_preview::Preview,
    pub controls_focus: FocusHandle,
    pub selected: Subject,
    pub generation: u64,
    pub rich: crate::rich::TextCache,
    pub document_rich: crate::rich::TextCache,
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
    /// Where this Pane's card and Composer were last laid out, so a surface
    /// summoned from a chip can hang off the Composer's edge and stay inside
    /// the card (`components::float_place`), whatever the pointer did.
    pub geometry: std::rc::Rc<std::cell::Cell<PaneGeometry>>,
    /// What the working line reads besides its transcript, as of this
    /// Pane's last render (`working_line`).
    pub(crate) working: std::cell::Cell<WorkingFacts>,
    /// The reader beside this Pane scrolls on its own (`cockpit/beside.rs`):
    /// kept here so its position survives every frame, and so opening it at
    /// a line can scroll that line into view.
    pub reader_scroll: gpui::UniformListScrollHandle,
}

/// A Pane's last laid-out card and Composer bounds, recorded in prepaint.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PaneGeometry {
    pub card: Option<gpui::Bounds<gpui::Pixels>>,
    pub composer: Option<gpui::Bounds<gpui::Pixels>>,
}

impl PaneGeometry {
    /// The float limits for this Pane: the Composer's top edge (a surface
    /// opening upward rests `FLOAT_OFFSET` above it) and the card's inner
    /// right edge (`PANE_PAD_X` in from its right).
    pub fn float_limits(&self) -> Option<(f32, f32)> {
        let card = self.card?;
        let composer = self.composer?;
        Some((
            f32::from(composer.top()),
            f32::from(card.right()) - PANE_PAD_X,
        ))
    }
}

/// View ownership stays with a Subject even while its transcript is hidden.
struct TranscriptViewport {
    generation: u64,
    selection_scope: gpui::base::TextSelectionScopeId,
    disclosure: ToolDisclosure,
}

/// Disclosure identities preserve choices while content streams or Subjects
/// switch: a call's details (or its output fold), a long thought, and a
/// later edit's folded diff (`+ show diff`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DisclosureId {
    Tool(String),
    Reasoning(BlockId),
    Diff(String),
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
        let preview = crate::attachment_preview::Preview::new(cx);
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
            document_rich: crate::rich::TextCache::default(),
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
            reader_scroll: gpui::UniformListScrollHandle::new(),
        }
    }

    /// A draft Pane (#29): cmd-t's answer — a Composer and the pre-prompt
    /// band, and nothing else until the first send bootstraps a Thread.
    pub fn new_draft<T: 'static>(
        draft: DraftId,
        binding: DraftBinding,
        cx: &mut Context<T>,
    ) -> Self {
        let preview = crate::attachment_preview::Preview::new(cx);
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
            document_rich: crate::rich::TextCache::default(),
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
            reader_scroll: gpui::UniformListScrollHandle::new(),
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
    /// Queue viewport derived from this Pane's actual available height.
    pub composer_queue_height: f32,
    pub focused: bool,
    /// This Thread finished while the operator looked elsewhere and they
    /// have not landed on it since (an unread Notice): the focus ring
    /// pulses in its place until they do. Never true with `focused`.
    pub attention: bool,
    /// The wall cell's folded reading, cached by the cockpit's facts —
    /// everything the L3 recipe needs that is not an O(1) transcript read.
    /// None for a Thread the facts have not met, which draws as empty.
    pub wall: Option<&'a WallCard>,
    /// The operator asked the system for reduced motion (`cx.reduce_motion()`).
    pub reduce_motion: bool,
    /// The Composer's own focus handle holds focus in the active window.
    pub editing: bool,
    /// Native files are dragged over this Pane: the drop sheet covers it
    /// and the Composer, drawn above the sheet, wears the accent edge.
    pub drop_target: bool,
    /// More than one Pane is on the board, so which one holds the keyboard
    /// needs showing: only then does focus draw the `FOCUS_RING` edge.
    pub show_focus: bool,
    /// The Pane is wider than the reading column, so its Solo tab strip
    /// lays out on the column's grid.
    pub head_column: bool,
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
    /// Files edited by this Thread, as one-click preview entries.
    pub changed_files: Option<AnyElement>,
    pub attachments: Option<AnyElement>,
    /// Pointer equivalents of the owning Composer's send and interrupt keys.
    pub composer_actions: Option<AnyElement>,
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
    /// The Composer's model picker — logomark, model label, chevron —
    /// supplied for **every** L1 Pane, before and after the first-prompt
    /// lock: the prototype draws it in all four Panes (#25).
    pub model_picker: Option<AnyElement>,
    /// Context and account usage lines beside the model control.
    pub usage_meter: Option<AnyElement>,
    pub session_controls: Option<AnyElement>,
    /// The mode chip as a menu trigger: a click lists the Session's native
    /// permission modes and a pick switches the running Session. `None`
    /// draws the plain chip.
    pub mode_picker: Option<AnyElement>,
    /// The head's title cell, wired: the name with a double-click that
    /// opens the rename editor, or the editor itself while renaming. None
    /// draws the plain name (L2, L3, drafts).
    pub title: Option<AnyElement>,
    pub agents: Option<AnyElement>,
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
pub fn wall_state(
    transcript: Option<&Transcript>,
    pending: bool,
    tests_failing: bool,
) -> WallState {
    let Some(transcript) = transcript else {
        return WallState::Parked;
    };
    let status = transcript.status();
    if pending || status == Status::Blocked {
        return WallState::Decision;
    }
    match status {
        Status::Closed => WallState::Blocked,
        Status::Streaming if tests_failing => WallState::Failing,
        Status::Streaming => WallState::Working,
        _ if transcript.turn_completed() => WallState::Done,
        _ => WallState::Idle,
    }
}

/// The wall tile's folded reading — rebuilt only when the Thread changed,
/// never per frame (the wall budget: 24 tiles × 60fps must not walk Blocks
/// or format strings). Words only: the tile draws them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WallCard {
    /// The latest test run failed (from `Instruments`, the one O(blocks)
    /// read the wall needs).
    pub tests_failing: bool,
    /// The failing run's own count, where it reported one: the tile reads
    /// `failing 2`, else `failing`.
    pub failing_count: Option<usize>,
    /// The working signal's detail: the progress caption (`Thinking`,
    /// `Retrying · Server busy`), the working state word's tooltip.
    pub working: SharedString,
    /// An alert tile's context: the Decision's subject, or the reason the
    /// Session closed — the `failed` state word's tooltip. Empty when
    /// neither applies.
    pub context: SharedString,
    /// How long the last completed turn worked (`3m 12s`): `done · 3m 12s`.
    pub done_after: Option<SharedString>,
    /// The transcript's last lines as the tile prints them, oldest first
    /// (the tile shows the last `WALL_LINES` rows), a pending Decision's
    /// lines last.
    pub lines: Vec<WallLine>,
}

/// One line of a wall tile: one `TEXT_MUTED` run, its glyph included —
/// `● Update(canvas.rs) +31 −9`, `  └ 212/357 tests`, `◆ Bash wants to
/// run`, `  $ gh issue close 212`, `? Which fix do you want?`.
#[derive(Clone, Debug, PartialEq)]
pub struct WallLine {
    pub text: SharedString,
    /// Agent prose: wider than the tile it wraps once, to a continuation
    /// at col 2 (`  from the spike notes`); every other line truncates.
    pub wraps: bool,
}

impl WallLine {
    fn line(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            wraps: false,
        }
    }
}

/// How many Blocks a wall card reaches back for — more than any tile can
/// show.
const WALL_TAIL_BLOCKS: usize = 16;

/// `◆`, typed: the tile's lines are one run each, glyph included (F-6).
const DIAMOND_GLYPH: &str = "\u{25c6}";

/// Fold one Thread's wall reading. The activity phrase stays a status word
/// — naming the running tool would put `Instruments::of` on every tile
/// every rebuild during streaming.
pub fn wall_card(transcript: Option<&Transcript>, decision: Option<&Decision>) -> WallCard {
    wall_card_timed(transcript, decision, None)
}

/// `wall_card` with the Thread's call clocks (`ThreadView::tool_timings`):
/// a settled call's line carries its time (`● Bash(cargo test) 1m01s`).
pub fn wall_card_timed(
    transcript: Option<&Transcript>,
    decision: Option<&Decision>,
    timings: Option<&HashMap<String, ToolTiming>>,
) -> WallCard {
    let Some(transcript) = transcript else {
        return WallCard::default();
    };
    let working = SharedString::from(
        transcript
            .progress()
            .caption()
            .unwrap_or_else(|| "Working".into()),
    );
    let context = match decision {
        Some(decision) => decision_subject(decision),
        // A closed Thread's context is the reason it closed — the last
        // Notice the fold pushed (#22 C14).
        None if transcript.status() == Status::Closed => transcript
            .blocks()
            .iter()
            .rev()
            .find_map(|block| match &block.body {
                Body::Notice(reason) => Some(SharedString::from(reason.clone())),
                _ => None,
            })
            .unwrap_or_else(|| SharedString::from("closed")),
        None => SharedString::default(),
    };
    let tests = Instruments::of(transcript).tests;
    let failing_count = match tests {
        Some(Tests::Failed { count }) => count,
        _ => None,
    };
    let blocks = transcript.blocks();
    let tail = &blocks[blocks.len().saturating_sub(WALL_TAIL_BLOCKS)..];
    let done_after = tail.iter().rev().find_map(|block| match &block.body {
        Body::TurnEnd(end) if end.outcome == ferrite_core::TurnOutcome::Completed => end
            .elapsed_ms
            .map(|ms| SharedString::from(worked_label(Duration::from_millis(ms)))),
        _ => None,
    });
    WallCard {
        tests_failing: matches!(tests, Some(Tests::Failed { .. })),
        failing_count,
        working,
        context,
        done_after,
        lines: wall_lines(tail, decision, timings),
    }
}

/// A finished turn's length as the tile says it: `41s`, `3m 12s`.
fn worked_label(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    if secs < 60 {
        ferrite_core::progress::settled_duration_label(elapsed)
    } else {
        format!("{}m {}s", secs / 60, secs % 60)
    }
}

/// A transcript tail as the tile prints it, oldest first (F-6): agent
/// prose behind `●` (it may wrap once); every call behind `●` with its
/// trail inline after one space (`+31 −9`, a settled time, `ok` for a
/// passed test run); under a test run its progress (`  └ 212/357 tests`)
/// or, settled red, its result (`  └ 357 passed; 2 failed`); a failed
/// call's reason; a notice behind `◆`. Prompts, reasoning, code, meta and
/// a completed turn's end leave no line (the state word says `done`). A
/// pending approval drops the call it gates and closes the list with
/// `◆ Bash wants to run` and `  $ <command>`; a question with `? <text>`.
fn wall_lines(
    tail: &[Block],
    decision: Option<&Decision>,
    timings: Option<&HashMap<String, ToolTiming>>,
) -> Vec<WallLine> {
    let first_line = |text: &str| -> String {
        text.lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let gated = decision
        .map(|decision| decision.tool_use_id.as_str())
        .filter(|id| !id.is_empty());
    let mut lines = Vec::new();
    for block in tail {
        match &block.body {
            Body::Paragraph { .. } | Body::Bullet { .. } | Body::Heading { .. } => {
                let Some(text) = tail_text(&block.body, false) else {
                    continue;
                };
                // Two source lines at most: a line break in the prose is
                // where its continuation starts.
                let mut source = text.lines().filter(|line| !line.trim().is_empty());
                let head = source.next().unwrap_or_default().trim().to_string();
                let text = match source.next() {
                    Some(next) => format!("{BULLET} {head}\n{}", next.trim()),
                    None => format!("{BULLET} {head}"),
                };
                lines.push(WallLine {
                    text: text.into(),
                    wraps: true,
                });
            }
            Body::Notice(text) if decision.is_none() => {
                let text = first_line(notice_text(text.trim(), false));
                if !text.is_empty() {
                    lines.push(WallLine::line(format!("{DIAMOND_GLYPH} {text}")));
                }
            }
            Body::TurnEnd(end) if !end.completed() => {
                lines.push(WallLine::line(format!("  {ELBOW} {}", end.text())));
            }
            Body::Tool(tool) => {
                if gated == Some(tool.call.as_str()) {
                    continue;
                }
                lines.push(WallLine::line(wall_call_line(tool, timings)));
                if let Some(result) = wall_result_line(tool) {
                    lines.push(WallLine::line(format!("  {ELBOW} {result}")));
                }
            }
            _ => {}
        }
    }
    if let Some(decision) = decision {
        lines.extend(wall_decision_lines(decision));
    }
    lines
}

/// `● Name(args) trail`: the call as the transcript names it, its trail
/// one space after it — the change (`+31 −9`), `ok` for a passed test run,
/// else a settled time from one second up. A running call has no trail:
/// the state line's clock is ticking.
fn wall_call_line(tool: &ToolBlock, timings: Option<&HashMap<String, ToolTiming>>) -> String {
    let label = text::tool_label(tool);
    let trail = match &tool.state {
        ToolState::Running => None,
        ToolState::Ok if !tool.diffs.is_empty() => {
            let added: usize = tool.diffs.iter().map(|diff| diff.added).sum();
            let removed: usize = tool.diffs.iter().map(|diff| diff.removed).sum();
            Some(format!("+{added} \u{2212}{removed}"))
        }
        ToolState::Ok if is_test_run(tool) => Some("ok".to_string()),
        _ => match timings.and_then(|map| map.get(&tool.call)) {
            Some(ToolTiming::Done(total)) if total.as_millis() >= theme::DURATION_MIN_MS => {
                Some(ferrite_core::progress::live_seconds(*total))
            }
            _ => None,
        },
    };
    match trail {
        Some(trail) => format!("{BULLET} {label} {trail}"),
        None => format!("{BULLET} {label}"),
    }
}

/// What hangs under a call on its tile: a running test run's progress
/// (`212/357 tests`, `ToolBlock::progress`), a red run's result (`357
/// passed; 2 failed`), a failed call's reason. A passed run says `ok` on
/// its own line, and nothing else hangs.
fn wall_result_line(tool: &ToolBlock) -> Option<String> {
    match &tool.state {
        ToolState::Running => tool
            .progress
            .as_ref()
            .filter(|progress| progress.total > 0)
            .map(|progress| format!("{}/{} tests", progress.done, progress.total)),
        ToolState::Failed(message) => {
            if is_test_run(tool) {
                if let Some(summary) = test_summary(tool) {
                    return Some(summary);
                }
            }
            let reason = message
                .lines()
                .find(|line| !line.trim().is_empty())
                .map(str::trim)
                .unwrap_or_default();
            Some(if reason.is_empty() {
                theme::words::FAILED.to_string()
            } else {
                format!("{} \u{b7} {reason}", theme::words::FAILED)
            })
        }
        _ => None,
    }
}

/// A red test run's own count line, `357 passed; 2 failed`, read from the
/// last line of its output that reports both.
fn test_summary(tool: &ToolBlock) -> Option<String> {
    let failure = match &tool.state {
        ToolState::Failed(message) => Some(message.as_str()),
        _ => None,
    };
    let texts = [
        failure,
        tool.result_line.as_deref(),
        tool.output.as_ref().map(|output| output.text.as_str()),
    ];
    texts.into_iter().flatten().find_map(|text| {
        text.lines().rev().find_map(|line| {
            let passed = passed_count(line)?;
            let failed = count_before(line, "failed")?;
            Some(format!("{passed} passed; {failed} failed"))
        })
    })
}

/// The number standing directly before `word` (`2 failed` → 2).
fn count_before(line: &str, word: &str) -> Option<usize> {
    let lower = line.to_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();
    tokens
        .windows(2)
        .find_map(|pair| (pair[1] == word).then(|| pair[0].parse().ok()).flatten())
}

/// A pending Decision's lines at the foot of its tile: an approval names
/// what it would run (`◆ Bash wants to run`, then `  $ <command>`), else
/// what it asks; a question is `? <its text>`.
fn wall_decision_lines(decision: &Decision) -> Vec<WallLine> {
    if let Some(questions) = questions_of(decision) {
        let text = questions
            .first()
            .map(|question| question.question.trim().to_string())
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| ferrite_core::questions::summary(questions));
        let text = text.lines().next().unwrap_or_default().trim().to_string();
        return vec![WallLine::line(format!("? {text}"))];
    }
    let tool = ferrite_core::transcript::display_tool_name(&decision.tool_name).to_string();
    let command = decision
        .input
        .get("command")
        .and_then(serde_json::Value::as_str)
        .filter(|command| !command.trim().is_empty());
    match (tool.is_empty(), command) {
        (false, Some(command)) => {
            let command = ferrite_core::providers::shell::unwrap_shell(command).to_string();
            let command = command
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or_default()
                .trim()
                .to_string();
            vec![
                WallLine::line(format!("{DIAMOND_GLYPH} {tool} wants to run")),
                WallLine::line(format!("  $ {command}")),
            ]
        }
        (false, None) => {
            let mut lines = vec![WallLine::line(format!(
                "{DIAMOND_GLYPH} {tool} wants approval"
            ))];
            let description = decision.description.trim();
            if !description.is_empty() {
                let description = description.lines().next().unwrap_or_default().trim();
                lines.push(WallLine::line(format!("  {description}")));
            }
            lines
        }
        (true, _) => vec![WallLine::line(format!(
            "{DIAMOND_GLYPH} {}",
            decision_subject(decision)
        ))],
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
            rows.push(line.text.clone());
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
    pub queued: Vec<&'a str>,
    pub queue_height: f32,
    pub composer_empty: bool,
    pub composer_files: usize,
    pub permission_mode: Option<SharedString>,
    pub suggestion: Option<&'a str>,
    pub received_reasoning_visible: bool,
    // ---- pre-plumbed for the work packages: wired, not read yet
    /// The operator asked the system for reduced motion.
    #[allow(dead_code)]
    pub reduce_motion: bool,
    /// The Composer itself holds the keyboard in the active window.
    #[allow(dead_code)]
    pub editing: bool,
    /// Native files hover this Pane (`PaneFacts::drop_target`).
    pub drop_target: bool,
    /// The Session is starting or being replaced; nothing committed yet.
    #[allow(dead_code)]
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
    pub composer_actions: Option<AnyElement>,
    pub background: Option<AnyElement>,
    /// Files this Thread edited: the Composer's shelf above the prompt.
    pub changed_files: Option<AnyElement>,
    pub menu: Option<AnyElement>,
    pub model_picker: Option<AnyElement>,
    pub usage_meter: Option<AnyElement>,
    pub session_controls: Option<AnyElement>,
    pub mode_picker: Option<AnyElement>,
    /// A Subagent's footer, drawn where the Composer would be.
    pub child_footer: Option<AnyElement>,
    /// `l1_dock`: activity requests that were not docked in the body.
    pub activity_decisions: Option<AnyElement>,
    /// The docked Decision merges into this Pane's Composer (one block).
    pub decision_joined: bool,
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
        composer_queue_height,
        focused,
        attention,
        wall,
        reduce_motion,
        editing,
        drop_target,
        show_focus,
        head_column,
        provider_mark,
        decision_joined,
        cell_width,
        parked_line,
    } = facts;
    let empty = WallCard::default();
    let wall = wall.unwrap_or(&empty);
    let PaneWiring {
        transcript: retained_transcript,
        changed_files,
        attachments,
        composer_actions,
        background,
        received_reasoning_visible,
        menu,
        model_picker,
        usage_meter,
        session_controls,
        mode_picker,
        title,
        agents,
        activity_decisions,
        expand_question,
        question_measurement,
        child_footer,
        head_drag,
        quick_answers,
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
    let state = wall_state(
        transcript,
        decision.is_some_and(Decision::blocks_execution),
        wall.tests_failing,
    );
    // Attention and focus are two channels on one edge (F-2): the edge is
    // an overlay over the Pane's own content (`pane_frame`), recoloured
    // only, so nothing reflows when a Decision arrives. A waiting Pane
    // draws `ATTENTION_EDGE` on every side — focused or not; a focused one
    // says so in its head ground and title instead of a second ring.
    let pending = thread.map(|thread| thread.activity().pending_decisions());
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
    // Solo (fullscreen included) has no head at any tier: the titlebar
    // carries the Thread (C2). A Group's head is the Pane's drag handle
    // whenever the cockpit wires one. It names the Thread and nothing
    // else (theme rule 7): the dot, the title, the provider's mark.
    let head = |title: Option<AnyElement>| {
        show_focus.then(|| {
            let (dot, working) = head_dot(
                view.is_main(),
                dot_state(state, state_word(state, kind, wall, transcript).as_ref()),
                status,
            );
            let head = group_head(GroupHead {
                key,
                name: view.name.clone(),
                dot: Some(dot),
                working,
                focused: framed,
                title,
                provider: provider_mark,
                action: None,
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
                    transcript,
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
        queue_height: composer_queue_height,
        composer_empty,
        composer_files,
        permission_mode: thread.and_then(|thread| {
            thread
                .permission_mode()
                .and_then(|mode| permission_mode_label(mode, &thread.permission_modes()))
        }),
        suggestion: thread.and_then(|thread| thread.suggestion()),
        received_reasoning_visible,
        reduce_motion,
        editing,
        drop_target,
        starting: thread.is_some_and(|thread| thread.starting()),
        unread: attention,
        grid: show_focus,
        attachments,
        composer_actions,
        background,
        changed_files,
        menu,
        model_picker,
        usage_meter,
        session_controls,
        mode_picker,
        child_footer,
        activity_decisions: None,
        decision_joined,
    };

    // Solo (fullscreen included) has no head: the titlebar carries the
    // Thread (C2), and the body starts at the Pane's own top edge. A
    // Group's Pane wears the one head. Subagent tabs keep a strip of their
    // own either way.
    let mut pane = shell.children(head(title));
    if let Some(agents) = agents {
        pane = pane.child(tab_strip(key, agents, l1_tasks(&mut cx), head_column));
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
                        .activity()
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
            // paints after it and so stays in view, edged in the accent.
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
    focused: bool,
) -> Option<AnyElement> {
    let facts = view.working.get();
    let line = if transcript.status() == Status::Streaming {
        working_line_for(
            transcript,
            false,
            focused,
            facts.reasoning_visible,
            facts.reduce_motion,
            facts.provider,
            view.thread().map_or(0, |thread| thread.get()),
        )
    } else if facts.starting {
        starting_line(
            facts.reduce_motion || !focused,
            provider_ink(facts.provider),
        )
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
/// — and `Starting` in `TEXT_MUTED`. Its element id is a constant: the
/// timeline has to survive every re-render of the line.
fn starting_line(still: bool, _ink: u32) -> Div {
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
    div()
        .debug_selector(|| "transcript-starting".into())
        .flex()
        .items_center()
        .w_full()
        .min_w_0()
        .h(px(theme::LH_UI))
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(TEXT_MUTED))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .w(px(theme::GLYPH_GUTTER))
                .child(mark),
        )
        .child(div().min_w_0().truncate().child("Starting"))
}

/// WP-C · the plan's meter beside the subagent tabs.
fn l1_tasks(cx: &mut PaneCtx) -> Option<AnyElement> {
    tasks_meter(cx.view.thread()?, cx.transcript?)
}

/// The plan's meter (`tasks_strip`) for a Thread working to one: beside the
/// subagent tabs, and in the Solo titlebar.
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
        // A subagent's tab keeps the working line over its footer.
        let working = cx
            .transcript
            .and_then(|transcript| working_line(cx.view, transcript, cx.focused));
        return Some(
            div()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .min_w_0()
                .when_some(working, |stack, line| {
                    stack.child(
                        div()
                            .pl(px(theme::COMPOSER_PAD_L))
                            .pr(px(theme::COMPOSER_PAD_R))
                            .child(line),
                    )
                })
                .child(footer)
                .into_any_element(),
        );
    }
    let transcript = cx.transcript?;
    // The working line (`working_line`), noted for this frame above.
    let working = working_line(cx.view, transcript, cx.focused);
    // The session controls are the palette's (`background tasks`, `refresh
    // MCP`): the status line has no `•••`.
    let _ = cx.session_controls.take();
    Some(
        composer_region(
            cx.view,
            Some(transcript),
            ComposerStack {
                compact: false,
                grid: cx.grid,
                decision: cx.decision,
                queued: std::mem::take(&mut cx.queued),
                queue_height: cx.queue_height,
                empty: cx.composer_empty,
                files: cx.composer_files,
                attachments: cx.attachments.take(),
                actions: cx.composer_actions.take(),
                background: cx.background.take(),
                changed_files: cx.changed_files.take(),
                menu: cx.menu.take(),
                mode: cx.permission_mode.as_deref(),
                mode_picker: cx.mode_picker.take(),
                model_picker: cx.model_picker.take(),
                usage_meter: cx.usage_meter.take(),
                setup_controls: None,
                draft_error: None,
                suggestion: cx.suggestion,
                working,
                focused: cx.focused,
                editing: cx.editing,
                drop_target: cx.drop_target,
                joined: cx.decision_joined,
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

/// The Pane box (theme WP-C): the reading plane (`paint::PLANE`), square
/// and flat, its content flush to its own edges — the edge is an overlay
/// (`pane_frame`), never a border in layout. `overflow: hidden` keeps the
/// body under the head rule.
fn pane_shell() -> Div {
    div()
        .relative()
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .min_w_0()
        .bg(theme::paint::PLANE)
        .font_family(theme::FONT_UI)
        .overflow_hidden()
}

/// What a Pane's 1px edge says (F-2): a waiting Pane (`ATTENTION_EDGE`, on
/// every side, focused or not) beats focus (`FOCUS_RING`), and a Pane with
/// neither rests on a transparent edge — the seams separate Panes. Solo
/// never recolours the frame (rule 2.2.5): the Decision carries the signal.
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
            PaneEdge::Attention => rgba(ATTENTION_EDGE).into(),
            PaneEdge::Focused => rgb(FOCUS_RING).into(),
            PaneEdge::Rest => rgba(TRANSPARENT).into(),
        }
    }
}

/// The Pane root (F-1): the Pane box, then its 1px edge laid over it as an
/// absolute `inset-0` overlay (the prototype's `.pane::after`) — it takes
/// no layout and no pointer, so the content starts at the Pane's own edges
/// and a state change moves nothing. Every Pane root is `PANE_GROUP`.
fn pane_frame(pane: Div, edge: PaneEdge, key: u64) -> Stateful<Div> {
    let overlay = div()
        .absolute()
        .inset_0()
        .border_1()
        .border_color(edge.ink());
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

// ------------------------------------------------------------------ drafts

/// Everything a draft Pane draws (#29), assembled in the cockpit where the
/// clicks are wired — the Pane only lays it out.
pub struct DraftState<'a> {
    pub attachments: Option<AnyElement>,
    pub composer_actions: Option<AnyElement>,
    /// The draft-only close control in the Pane header.
    pub discard: AnyElement,
    /// The draft's setup chips — project and workspace — riding the left
    /// of the controls row, where a live Composer's mode chip rides.
    pub band: AnyElement,
    /// The draft's model and effort controls, in the trailing slot a live
    /// Composer's model picker occupies.
    pub picker: AnyElement,
    /// The open band popover, hung above the Composer like every menu.
    pub menu: Option<AnyElement>,
    pub composer_empty: bool,
    /// How many files wait in the draft (`PaneFacts::composer_files`).
    pub composer_files: usize,
    pub focused: bool,
    /// A failed bootstrap's words, shown where the band is.
    pub error: Option<&'a SharedString>,
    /// The usage meter, in the slot a live Composer hangs it in: a draft
    /// has spent no context, and its account windows answer before the
    /// prompt is written.
    pub usage_meter: Option<AnyElement>,
    /// The Composer holds the keyboard in the active window, and the
    /// operator asked for reduced motion.
    pub editing: bool,
    #[allow(dead_code)]
    pub reduce_motion: bool,
    /// Native files hover the draft (`PaneFacts::drop_target`).
    pub drop_target: bool,
    /// More than one Pane is on the board (`PaneFacts::show_focus`).
    pub show_focus: bool,
}

/// What a draft is called until its first send names the Thread — in the
/// titlebar (Solo) and in its Group head.
pub const DRAFT_TITLE: &str = "New thread";

/// A draft Pane (#29): an empty transcript area and the Composer wearing
/// the pre-prompt band. In Solo it has no head — the titlebar reads
/// `ferrite / New thread` and holds the discard ×. On a board it wears the
/// Group head (the glyph column reserved, no dot: nothing runs yet) with the
/// × in its slot, then the grid's one Composer line; below L1 the body is
/// the head and that line alone.
pub fn render_draft(view: &PaneView, state: DraftState<'_>, level: Level) -> impl IntoElement {
    let DraftState {
        attachments,
        composer_actions,
        discard,
        band,
        picker,
        menu,
        composer_empty,
        composer_files,
        focused,
        error,
        usage_meter,
        editing,
        reduce_motion,
        drop_target,
        show_focus,
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
            dot: None,
            working: false,
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
            compact: level != Level::Transcript,
            grid: show_focus,
            decision: None,
            queued: Vec::new(),
            queue_height: 0.,
            empty: composer_empty,
            files: composer_files,
            attachments,
            actions: composer_actions,
            background: None,
            changed_files: None,
            menu,
            mode: None,
            mode_picker: None,
            model_picker: Some(picker),
            usage_meter,
            setup_controls: Some(band),
            draft_error: error.cloned(),
            // A draft has no conversation yet, so nothing to predict.
            suggestion: None,
            working: None,
            focused,
            editing,
            drop_target,
            joined: false,
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
/// as cmd-w: an `ICON_BUTTON` × in `TEXT_MUTED` — the Group head's slot on a
/// board, the titlebar's trailing slot in Solo. Its tooltip names the key
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
        .child(icon(icons::CLOSE, theme::ICON_BUTTON_GLYPH, TEXT_MUTED))
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

/// Draft setup controls ride the status line's first segments. In a narrow
/// Pane they give way first: their labels truncate before the model and
/// effort pair or the usage meter loses a pixel. Their focus ring is inset
/// and takes no layout.
pub fn draft_band() -> Div {
    div()
        .debug_selector(|| "draft-band".into())
        .flex()
        .flex_shrink(1.)
        .min_w_0()
        .items_center()
        .h(px(theme::COMPOSER_ROW_H))
}

/// One band chip (project, workspace): a status segment — the choice — ringed
/// by the one focus recipe (`components::focused`) while tab rests on it,
/// because the popover opens on ↵ and the segment must say where ↵ will
/// land.
pub fn band_chip(slot: usize, label: SharedString, accent: bool, focused: bool) -> Stateful<Div> {
    // The label truncates in a narrow Pane; the tooltip keeps the whole
    // choice reachable and names the keys that change it.
    let tooltip = SharedString::from(format!("{label} \u{b7} Tab, then \u{21b5} to change"));
    div()
        .id(("band-chip", slot))
        .tooltip(move |window, cx| {
            gpui::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        })
        .debug_selector(move || format!("band-chip-{slot}"))
        .flex_shrink(1.)
        .min_w_0()
        .map(|chip| components::focused(chip, focused))
        .hover_raised(format!("band-chip-{slot}"))
        .press_raised()
        .child(
            status_seg(if accent { TEXT } else { TEXT_MUTED })
                .flex_shrink(1.)
                .min_w_0()
                .child(div().min_w_0().truncate().child(label)),
        )
}

/// A band chip's text: the choice itself (the chip draws its own chevron).
pub fn band_chip_label(choice: &str) -> SharedString {
    SharedString::from(choice.to_owned())
}

/// A draft's model or effort control: the live Composer's own picker
/// chip, ringed like the band chip while tab rests on it, so tab still says
/// where ↵ will land.
pub fn draft_picker(
    id: &'static str,
    focused: bool,
    control: Div,
    cx: &gpui::App,
) -> gpui::component::button::Button {
    chip_button(id, cx)
        .debug_selector(move || id.to_string())
        .p_0()
        .h_auto()
        .flex()
        .flex_shrink_0()
        .rounded(px(theme::R_CHIP))
        .map(|chip| components::focused(chip, focused))
        .child(control)
}

/// Everything one wall tile reads, gathered by `render_pane`.
struct WallTile<'a> {
    key: u64,
    card: &'a WallCard,
    state: WallState,
    kind: Option<&'static str>,
    transcript: Option<&'a Transcript>,
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
        transcript,
        quick_answers,
        focused,
        cell_width,
        parked_line,
    } = tile;
    let word = match state {
        WallState::Parked => Some(HeadSlot::Parked),
        _ => match state_word(state, kind, card, transcript) {
            Some(HeadSlot::Working(_)) => {
                Some(HeadSlot::Working(working_clock(transcript, focused)))
            }
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
const WAKE_HINT: &str = "\u{23ce} wakes it on a fresh Session";

/// A working tile's clock: whole seconds (`12s`, `1m04s`) on the tile that
/// holds the keyboard; elsewhere a minute and past reads in minutes alone
/// (`1m`), as the prototype's quiet Panes do.
fn working_clock(transcript: Option<&Transcript>, focused: bool) -> String {
    let Some(elapsed) = transcript.and_then(Transcript::turn_elapsed) else {
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
                    .text_color(rgb(TEXT_MUTED))
                    .child(rest)
            })),
    )
}

/// One of a tile's rows: one `TEXT_MUTED` run, its glyph included, on one
/// line that truncates with `…` at the tile's edge.
fn wall_line(row: SharedString) -> Div {
    div()
        .debug_selector(|| "wall-line".into())
        .flex_shrink_0()
        .w_full()
        .min_w_0()
        .h(px(theme::LH_UI))
        .text_color(rgb(TEXT_MUTED))
        .truncate()
        .child(row)
}

/// The tile's quick answers: boxed words a cell apart, half a row under
/// the lines, the first `WALL_PAD_X` in from the tile's edge and the row
/// `WALL_PAD_Y` above its foot — each the cockpit's wired button
/// (`quick_answer`).
/// `lead` moves the row off the tile's inset, to where the browser's
/// whole-pixel rounding of the tile's track puts the first box.
pub(crate) fn quick_answers(buttons: Vec<AnyElement>, lead: f32) -> Div {
    div()
        .debug_selector(|| "wall-quick-answers".into())
        .flex()
        .flex_shrink_0()
        .flex_wrap()
        .gap(px(theme::QUICK_ANSWER_GAP))
        .mt(px(theme::HALF_ROW))
        .ml(px(lead))
        .children(buttons)
}

/// One quick answer (F-7): `1 allow` in a 1px `paint::LINE2` box,
/// `QUICK_ANSWER_H` (22px) outer — one line and its two borders — 1ch in
/// from each side. The digit is `TEXT_MUTED` on every waiting tile, the
/// word `TEXT`; under the pointer the face blends to `paint::BAND2` and the
/// word to `TEXT_STRONG` over the one 150ms hover blend, the digit staying
/// dim.
pub(crate) fn quick_answer(
    id: SharedString,
    key: SharedString,
    label: SharedString,
) -> Stateful<Div> {
    let hover = id.clone();
    let ground =
        crate::motion::hover_blend(&hover, rgba(TRANSPARENT).into(), theme::paint::BAND2.hsla());
    let word = crate::motion::hover_blend(&hover, rgb(TEXT).into(), rgb(TEXT_STRONG).into());
    let selector = id.clone();
    // Its cells, a cell of padding each side and its edge, on the whole
    // pixel as the browser rounds the box (`1 allow` is 72.2px: 72).
    let key = format!("{key} ");
    let width = (components::cells_width(&key)
        + components::cells_width(&label)
        + 2.0 * theme::QUICK_ANSWER_PAD_X
        + 2.0)
        .round();
    div()
        .id(gpui::ElementId::Name(id))
        .debug_selector(move || selector.to_string())
        .flex()
        .flex_shrink_0()
        .items_center()
        .w(px(width))
        .h(px(theme::QUICK_ANSWER_H))
        .px(px(theme::QUICK_ANSWER_PAD_X))
        .border_1()
        .border_color(theme::paint::LINE2)
        .bg(ground)
        .cursor_pointer()
        .whitespace_nowrap()
        .on_hover(crate::motion::hover_listener(hover))
        .child(components::cells(key).text_color(rgb(TEXT_MUTED)))
        .child(components::cells(label).text_color(word))
}

/// A cell's status dot, one recipe for the wall, the Pane head and the
/// nav (`cockpit::thread_status`): running green, a Decision ochre, a
/// failing suite or a closed Session red, done and idle the idle ink —
/// green never means finished — and a parked Thread a faint ring.
#[cfg(test)]
pub(crate) fn cell_dot(state: WallState, unread: bool) -> Div {
    crate::cockpit::thread_status(state, unread).dot()
}

/// The Pane head's dot, and whether it is a working Thread's (drawn as the
/// braille spinner). The main Thread's is the status truth the nav and the
/// cells share (`thread_status`); a subagent tab's is that agent's own
/// transcript state. Unread is the title's ink, never the dot's.
fn head_dot(
    main: bool,
    state: WallState,
    subject: Option<Status>,
) -> (crate::cockpit::ThreadStatus, bool) {
    let state = if main {
        state
    } else {
        match subject {
            Some(Status::Streaming) => WallState::Working,
            Some(Status::Blocked) => WallState::Decision,
            Some(Status::Closed) => WallState::Blocked,
            _ => WallState::Idle,
        }
    };
    (
        crate::cockpit::thread_status(state, false),
        state == WallState::Working,
    )
}

/// What a pending request asks of the operator, as the lexicon names it.
fn request_kind(decision: &Decision) -> &'static str {
    if questions_of(decision).is_some() {
        theme::words::QUESTION
    } else {
        theme::words::APPROVAL
    }
}

/// One lexicon word for where a Thread stands (rule 2.11.2), shared by the
/// Solo titlebar's state segment and the wall tile's state word. Each
/// variant is one state; `text` is what it reads, `ink` the one ink it
/// wears where colour is allowed (the titlebar sets it dim).
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
            _ => TEXT_MUTED,
        }
    }
}

/// The state vocabulary, shared by the titlebar and the wall:
/// `needs you` > `failing` > `failed` > `working` > `done` > `interrupted`
/// > `parked`; idle is nothing.
fn state_word(
    state: WallState,
    kind: Option<&'static str>,
    card: &WallCard,
    transcript: Option<&Transcript>,
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
            transcript
                .and_then(Transcript::turn_elapsed)
                .map(ferrite_core::progress::live_seconds)
                .unwrap_or_default(),
        ),
        WallState::Done => HeadSlot::Done,
        WallState::Parked => HeadSlot::Parked,
        WallState::Idle => match transcript.and_then(Transcript::turn_outcome) {
            Some(TurnOutcome::Interrupted) => HeadSlot::Interrupted,
            Some(TurnOutcome::Error(_)) => HeadSlot::Failed,
            _ => return None,
        },
    })
}

/// The state a Thread's dot shows: its wall state, squared with the state
/// word beside it so the two can never disagree. A turn that ended in an
/// error reads `failed`, so its dot is `BLOCKED` like a closed Session's;
/// a `failing` word is always a `BLOCKED` dot. Every surface that draws a
/// Thread's dot — the nav, the rail, the Group head, the titlebar — reads
/// it through here.
pub(crate) fn dot_state(state: WallState, word: Option<&HeadSlot>) -> WallState {
    match word {
        Some(HeadSlot::Failed) if state != WallState::Blocked => WallState::Blocked,
        Some(HeadSlot::Failing(_)) => WallState::Failing,
        _ => state,
    }
}

/// A Thread's face away from its Pane — the Solo titlebar, which carries
/// the Thread the headless Solo Pane does not (C2): its status dot and its
/// state word, read exactly as the Pane reads them.
pub(crate) fn thread_face(
    open: ThreadView<'_>,
    card: Option<&WallCard>,
    unread: bool,
) -> (crate::cockpit::ThreadStatus, Option<HeadSlot>) {
    let empty = WallCard::default();
    let card = card.unwrap_or(&empty);
    let transcript = open.transcript();
    let decision = open.pending();
    let state = wall_state(
        Some(transcript),
        decision.is_some_and(Decision::blocks_execution),
        card.tests_failing,
    );
    let pending = open.activity().pending_decisions();
    let kind = (!pending.is_empty() || state == WallState::Decision).then(|| {
        pending
            .first()
            .map(|request| request_kind(&request.decision))
            .or_else(|| decision.map(request_kind))
            .unwrap_or(theme::words::APPROVAL)
    });
    let word = state_word(state, kind, card, Some(transcript));
    (
        crate::cockpit::thread_status(dot_state(state, word.as_ref()), unread),
        word,
    )
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
        .text_color(rgb(TEXT_MUTED))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .w(px(theme::GLYPH_GUTTER - theme::CH))
                .child(icon(icons::DIAMOND, theme::GLYPH_BOX, ATTENTION)),
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
                .child(components::key_combo(&keys, TEXT_MUTED))
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

/// A Block's visible text as one line of words — what a wall tile's prose
/// line reads (`wall_lines`), spelled once.
pub(crate) fn tail_text(body: &Body, docked: bool) -> Option<String> {
    use ferrite_core::TurnOutcome;
    let prose = |spans: &[Span]| -> String {
        spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>()
            .trim()
            .to_string()
    };
    let text = match body {
        Body::Prompt(text) => text.trim().to_string(),
        Body::Paragraph { spans } | Body::Bullet { spans } | Body::Heading { spans, .. } => {
            prose(spans)
        }
        Body::Code { language, .. } => format!("```{}", language.as_deref().unwrap_or("")),
        Body::Tool(tool) => text::tool_label(tool).to_string(),
        Body::Notice(text) => notice_text(text.trim(), docked).to_string(),
        Body::Meta(text) => text.clone(),
        Body::TurnEnd(end) if end.outcome == TurnOutcome::Completed => return None,
        Body::TurnEnd(end) => end.text(),
        Body::Thinking(text) => ferrite_core::progress::headline(text).to_string(),
    };
    (!text.is_empty()).then_some(text)
}

// ------------------------------------------------------------- the head

/// The one Group head (theme WP-C, the prototype's `.ph`), for a
/// transcript Pane and a wall tile alike: one `PANE_HEAD_H` row (24px, its
/// `paint::LINE` rule inside it) 1ch in from each side. In order: the state
/// mark in a 2ch column — the dot the size of the face's `●`, centred in
/// the column's first cell; a working Thread's braille spinner where a
/// typed `⠋` would sit (`HEAD_SPINNER_LEFT`) — the title at `W_LABEL`,
/// `TEXT` on every Pane and `TEXT_STRONG` only on the focused one (unread
/// does not lift it), and the provider's 11px mark 1ch after it, its right
/// edge 1ch in from the Pane's. The focused Pane's head lays the
/// `paint::HEAD` band. No state word and no number (theme rule 7). A draft
/// has no dot and carries its × at the right.
pub(crate) struct GroupHead {
    pub key: u64,
    pub name: SharedString,
    pub dot: Option<crate::cockpit::ThreadStatus>,
    /// The Thread is working: its dot is the braille spinner.
    pub working: bool,
    /// The Pane holds the keyboard on a board: the head band, a strong title.
    pub focused: bool,
    /// The wired title (drag handle, double-click rename); `None` draws the
    /// name.
    pub title: Option<AnyElement>,
    pub provider: Option<Provider>,
    /// A trailing control in the mark's place (a draft's ×).
    pub action: Option<AnyElement>,
}

pub(crate) fn group_head(head: GroupHead) -> Div {
    let GroupHead {
        key,
        name,
        dot,
        working,
        focused,
        title,
        provider,
        action,
    } = head;
    let floor = title_floor(&name);
    let mark = match dot {
        // The spinner's frames are drawn in a `GLYPH_BOX` square; laid at
        // `HEAD_SPINNER_LEFT` its dots start where the prototype's typed
        // braille does.
        Some(dot) if working => div()
            .absolute()
            .left(px(theme::HEAD_SPINNER_LEFT))
            .top(px((theme::LH_UI - theme::GLYPH_BOX) / 2.0))
            .child(components::braille_spinner(dot.ink, theme::GLYPH_BOX))
            .into_any_element(),
        // The dot is the face's `●`: centred in the column's first cell.
        Some(dot) => div()
            .flex()
            .items_center()
            .justify_center()
            .w(px(theme::CH))
            .h(px(theme::LH_UI))
            .child(dot.dot())
            .into_any_element(),
        None => div().into_any_element(),
    };
    let title_ink = if focused { TEXT_STRONG } else { TEXT };
    div()
        .debug_selector(move || format!("pane-head-{key}"))
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::PANE_HEAD_H))
        .px(px(theme::HEAD_PAD_X))
        .border_b_1()
        .border_color(theme::paint::LINE)
        .when(focused, |head| head.bg(theme::paint::HEAD))
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        // The head's line sits centred in the 23px over its rule, half a
        // pixel off the grid: set where the browser sets it.
        .child(components::css_line(
            div()
                .flex()
                .flex_1()
                .min_w(px(theme::HEAD_DOT_W + floor))
                .overflow_hidden()
                .items_center()
                .child(
                    div()
                        .debug_selector(move || format!("pane-head-dot-{key}"))
                        .relative()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .w(px(theme::HEAD_DOT_W))
                        .h(px(theme::LH_UI))
                        .child(mark),
                )
                .child(
                    div()
                        .debug_selector(move || format!("pane-head-title-{key}"))
                        .flex()
                        .flex_1()
                        .flex_shrink(1.)
                        .min_w(px(floor))
                        .overflow_hidden()
                        .font_weight(theme::W_LABEL)
                        .text_color(rgb(title_ink))
                        .child(match title {
                            Some(title) => title,
                            None => div().min_w_0().truncate().child(name).into_any_element(),
                        }),
                ),
        ))
        .children(provider.map(|provider| {
            let (glyph, ink) = match provider {
                Provider::Codex => (icons::CODEX, theme::PROVIDER_CODEX),
                Provider::Claude => (icons::CLAUDE, theme::PROVIDER_CLAUDE),
            };
            div()
                .debug_selector(move || format!("pane-head-provider-{key}"))
                .flex_shrink_0()
                .ml(px(theme::CH))
                .child(icon(glyph, theme::PROVIDER_MARK_SM, ink))
        }))
        .children(action)
}

/// `needs you` as a door: a press runs the ⌘D jump (`NextDecision`) from
/// wherever the keyboard is — the press does not land on a Pane first — and
/// the tooltip names that key: `Next needs you ⌘D`. The Group head's
/// slot and the Solo titlebar share it.
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

/// The subagent tabs' own strip, under the head (or at the card's top edge
/// in Solo, which has no head): one `PANE_HEAD_H` row closed by the head's
/// rule, the tabs taking the width and the plan's meter at the right. It
/// exists only while tabs do.
fn tab_strip(key: u64, agents: AnyElement, tasks: Option<AnyElement>, column: bool) -> Div {
    let row = div()
        .flex()
        .items_center()
        .w_full()
        .h_full()
        .min_w_0()
        .gap(px(theme::HEAD_CLUSTER_GAP))
        // Main's pill hangs its inline padding left of the text column, so
        // its label starts where the transcript's text does.
        .pl(px(theme::GUTTER_W - theme::SUBJECT_TAB_PAD_X))
        .child(agents)
        .children(tasks.map(|tasks| div().flex_shrink_0().child(tasks)));
    let strip = div()
        .debug_selector(move || format!("pane-tabs-{key}"))
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::PANE_HEAD_H))
        .px(px(theme::PANE_PAD_X))
        .border_b_1()
        .border_color(theme::paint::LINE)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(TEXT_MUTED));
    let inset = div()
        .h_full()
        .w_full()
        .px(px(theme::BOX_INSET_X))
        .child(row);
    if column {
        strip.child(components::reading_column(inset))
    } else {
        strip.child(inset)
    }
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
/// `TEXT_MUTED` and a skip is the faintest ink (drawn as a ring,
/// `check_dot`). Never the ochre that means an agent waits on you.
pub fn check_ink(state: CheckState) -> u32 {
    match state {
        CheckState::Pending => RUNNING,
        CheckState::Passing => TEXT_MUTED,
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
        _ => TEXT_MUTED,
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

/// The card's heading: the PR by number at the left, and how its runs
/// divide at the right — the counts the head's chip had no room for. Only
/// states with runs in them are named, so the line never reads `0 failed`,
/// and only the failure is coloured; its figures are tabular, so a run
/// finishing never shifts the tally. Space, not a rule, separates summary
/// from runs (`CHECKS_CARD_GAP`); the heading, the workflow titles and the
/// runs' dots share one leading edge, a menu row's.
pub fn checks_head(pr: &PullRequest) -> Div {
    let tally = pr.tally();
    let parts: Vec<(String, u32)> = [
        (tally.failing, "failed", BLOCKED),
        (tally.pending, "running", TEXT_MUTED),
        (tally.passing, "passed", TEXT_MUTED),
        (tally.skipped, "skipped", TEXT_MUTED),
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
        if ink != TEXT_MUTED {
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
        .text_color(rgb(TEXT_MUTED))
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
                        .child(SharedString::from(format!("#{}", pr.number))),
                )
                .child(div().text_color(rgb(TEXT_MUTED)).child("checks")),
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
                .text_color(rgb(if openable { TEXT } else { TEXT_MUTED }))
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
/// `▰▱`): done steps in `TEXT_MUTED`, the rest unlit. `live` lights the
/// step being run in `RUNNING` — the one colour on a meter, because a step
/// in progress is live state. Finishing a plan is not a status: a full
/// meter is all `TEXT_MUTED`, no check and no green.
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
                    rgb(TEXT_MUTED).into()
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
                    .bg(rgb(TEXT_MUTED)),
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
                .text_color(rgb(TEXT_MUTED))
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
        .tooltip(move |window, cx| {
            gpui::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
        })
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

/// The status line's model segment (FL-10): the provider's logomark in its
/// brand colour, a cell, and the model's name in the status line's own
/// voice — lowercase, a parenthesised context size kept as written (`opus
/// 5.5 (1M)`, `gpt-6 astra`) — then, when an effort resolves, ` · medium`
/// with its `·` muted like the rest: one segment, one control. Render-only;
/// the cockpit gives it its id and its click.
pub fn model_picker(
    provider: Option<Provider>,
    label: SharedString,
    effort: Option<SharedString>,
    _busy: bool,
) -> Div {
    let mark = provider.map(|provider| {
        let (glyph, ink) = match provider {
            Provider::Codex => (icons::CODEX, theme::PROVIDER_CODEX),
            Provider::Claude => (icons::CLAUDE, theme::PROVIDER_CLAUDE),
        };
        // Inline in the segment's line, as the prototype's `svg.logo`
        // (`vertical-align: -1px`): its foot a pixel under the baseline.
        icon(glyph, theme::STATUS_LOGO, ink).mt(px(-theme::STATUS_LOGO_FOOT))
    });
    let word = status_model_word(&label);
    let text: SharedString = match effort {
        Some(effort) => format!("{word} \u{b7} {effort}").into(),
        None => word,
    };
    status_seg(TEXT_MUTED)
        .children(mark)
        .child(components::cells(text))
}

/// A model's name as the status line says it: lowercase outside any
/// parentheses, so `Opus 5.5 (1M)` reads `opus 5.5 (1M)`.
pub(crate) fn status_model_word(label: &str) -> SharedString {
    let mut depth = 0usize;
    let mut word = String::with_capacity(label.len());
    for ch in label.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth == 0 {
            word.extend(ch.to_lowercase());
        } else {
            word.push(ch);
        }
    }
    word.into()
}

/// The effort segment after the model: the level in force, lowercase (the
/// menu rows keep their titles).
pub fn effort_picker(label: SharedString, _busy: bool) -> Div {
    status_seg(TEXT_MUTED).child(div().flex_shrink_0().child(label))
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
        || tool.structured_result.is_some()
        || !tool.summary.is_empty()
        || !tool.diffs.is_empty()
}

/// One visibility rule for rendering controls, keyboard cycling and focus:
/// every call is its own row, so each call with details and each long
/// thought is one stop, oldest first.
pub fn rendered_disclosures(_view: &PaneView, blocks: &[Block], level: Level) -> Vec<DisclosureId> {
    rendered_window(blocks, level)
        .iter()
        .filter_map(|block| match &block.body {
            Body::Tool(tool) if tool_has_details(tool) => {
                Some(DisclosureId::Tool(tool.call.clone()))
            }
            Body::Thinking(text) if reasoning_text(text).1.is_some() => {
                Some(DisclosureId::Reasoning(block.id))
            }
            _ => None,
        })
        .collect()
}

/// The working line's mark (the prototype's `.spin`): the star spinner
/// cycling `· ✢ ✳ ✶ ✻ ✽` in the provider's colour on the focused Pane
/// (`live`), the still `✻` everywhere else and under reduced motion. Its
/// selector says which.
fn working_mark(ink: u32, live: bool) -> AnyElement {
    // Centred on the gutter's first cell, as the prototype's glyph sits.
    let lead = px((theme::CH - theme::STAR_MARK) / 2.0);
    if live {
        div()
            .debug_selector(|| "progress-mark-live".into())
            .ml(lead)
            .child(components::working_spinner(ink, theme::STAR_MARK))
            .into_any_element()
    } else {
        div()
            .debug_selector(|| "progress-mark-still".into())
            .ml(lead)
            .flex()
            .items_center()
            .justify_center()
            .size(px(theme::STAR_MARK))
            .child(icon(icons::WORKED, theme::STAR_MARK, ink))
            .into_any_element()
    }
}

/// The working line with no provider known: the spinner and caption in
/// `TEXT_MUTED` (the L2 cell's, retired with it; kept for the tests).
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
        focused,
        received_reasoning_is_visible,
        reduce_motion,
        None,
        0,
    )
}

/// The working line (the prototype's `.comp > .r`): one row on the chrome
/// grid, the star spinner in the gutter, the provider's live caption in its
/// colour — the turn's spinner verb (`Reticulating…`,
/// `progress::spinner_verb`, held for the whole turn) unless the provider
/// has something better to say (R16) — and then `(1m04s · ↑ 4.1k tokens ·
/// esc to interrupt)` in `TEXT_MUTED`. Only the
/// focused Pane animates it: the spinner cycles and the caption's shimmer
/// sweeps there; elsewhere the `✻` and the caption hold still. The seconds
/// are whole (`progress::live_seconds`), tabular, so the text changes once a
/// second. Every line names `esc to interrupt`, focused or not, as the
/// prototype's do (the key acts only where the keyboard is). The caption is
/// what truncates; the facts keep
/// their room. L2 (`compact`) draws the same row without the token count.
fn working_line_for(
    transcript: &Transcript,
    compact: bool,
    focused: bool,
    received_reasoning_is_visible: bool,
    reduce_motion: bool,
    provider: Option<Provider>,
    thread: u64,
) -> Div {
    let ink = provider_ink(provider);
    let live = focused && !reduce_motion;
    let mut facts: Vec<String> = Vec::new();
    if let Some(elapsed) = transcript.turn_elapsed() {
        facts.push(ferrite_core::progress::live_seconds(elapsed));
    }
    let tokens = transcript.turn_output_tokens();
    if tokens > 0 && !compact {
        facts.push(format!("\u{2191} {} tokens", tokens_label(tokens)));
    }
    // Every working line names the key, as the prototype's do — focused
    // or not (the key acts where the keyboard is).
    facts.push("esc to interrupt".into());
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
    let mut row = div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .w_full()
        .min_w_0()
        .h(px(theme::LH_UI))
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI));
    if let Some((selector, caption)) = caption {
        let caption = SharedString::from(caption);
        // The caption and the facts hold exactly their cells, the facts'
        // leading space inside their run: measured runs round up a pixel
        // and a 7.8 pad snaps to 8, drifting the facts off the grid.
        let caption_w = components::cells_width(&caption) + 0.1;
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
        let metadata = SharedString::from(format!(" ({})", facts.join(" \u{b7} ")));
        let highlights = separators(&metadata);
        let metadata_w = components::cells_width(&metadata);
        row = row
            .debug_selector(move || selector.clone())
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .w(px(theme::GLYPH_GUTTER))
                    .h(px(theme::LH_UI))
                    .child(working_mark(ink, live)),
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
                        .text_color(rgb(TEXT_MUTED))
                        .child(StyledText::new(metadata).with_highlights(highlights)),
                ))
            });
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
        transcript: None,
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
    compact: bool,
    /// A board cell (C4): only the focused cell's Composer is live — its
    /// shelf, queue and status line; the others keep their input band.
    grid: bool,
    decision: Option<&'a Decision>,
    /// Prompts held back while the turn runs, newest first: they pile up
    /// above the line, the latest on top.
    queued: Vec<&'a str>,
    queue_height: f32,
    empty: bool,
    /// How many files the draft holds: what the flat line counts while
    /// the shelf (`attachments`) waits for the live Composer.
    files: usize,
    attachments: Option<AnyElement>,
    /// Solo's status line right: `⇧⇥ mode · ? shortcuts`.
    actions: Option<AnyElement>,
    /// Running background tasks as chips, hung at the right edge of the
    /// same shelf the pending files sit on.
    background: Option<AnyElement>,
    /// The status line's changed-files segment (`1 file +9 −4`), Solo's.
    changed_files: Option<AnyElement>,
    menu: Option<AnyElement>,
    mode: Option<&'a str>,
    /// The mode segment wired to its menu; `None` draws the plain segment.
    mode_picker: Option<AnyElement>,
    /// The model segment: the model and its effort, one control (#25).
    model_picker: Option<AnyElement>,
    /// The `ctx` segment, after the model.
    usage_meter: Option<AnyElement>,
    setup_controls: Option<AnyElement>,
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
    /// The Composer itself holds the keyboard in the active window: the
    /// queue's keys act only then.
    editing: bool,
    /// Native files hover the Pane: the band's edge is `ACCENT_EDGE`,
    /// saying where they will land.
    drop_target: bool,
    /// A docked Decision sits flush on top. The band is a band either way
    /// (no box to join); kept so the levels hand the Composer one shape.
    #[allow(dead_code)]
    joined: bool,
}

/// One of the band's two rules (the prototype's `.comp .rule`): 1px of
/// room, transparent on these themes.
fn composer_rule() -> Div {
    div().flex_shrink_0().h(px(theme::COMPOSER_RULE))
}

/// The Composer (theme WP-D, the prototype's `.comp`): no box. While a turn
/// runs, the working line; then an input band across the Pane's width on
/// `paint::INBAND` between two transparent rules — queued prompts, then the
/// `❯` in its 2-cell gutter and the line growing upward to
/// `composer::MAX_ROWS` rows — and, in Solo and the focused board Pane, the
/// status line under the lower rule: the mode, the model with its effort,
/// `ctx`, and in Solo the changed files and `⇧⇥ mode · ? shortcuts` at the
/// right. An unfocused board Pane is flush: band, rule, and the stack's
/// foot.
///
/// The shelf — pending files at left, background tasks at right — stands
/// above the band. The Pane lays the stack out `flex_shrink_0` below the
/// body, so the transcript gives way. The Decision row is **not** here: it
/// is the transcript's tail. While a Decision pends the band carries the
/// `Decision` key context, so its keys answer with the keyboard in the
/// Composer (#23).
fn composer_region(view: &PaneView, transcript: Option<&Transcript>, stack: ComposerStack) -> Div {
    let ComposerStack {
        compact,
        grid,
        decision,
        queued,
        queue_height,
        empty,
        files,
        attachments,
        actions,
        background,
        changed_files,
        menu,
        mode,
        mode_picker,
        model_picker,
        usage_meter,
        setup_controls,
        draft_error,
        suggestion,
        working,
        focused,
        editing,
        drop_target,
        joined: _,
    } = stack;
    // On a board only the focused cell's Composer is live (C4): the others
    // keep their band — `❯`, the hollow caret, a Decision's placeholder —
    // and say what the shelf and the queue hold in words after the `❯`
    // (`flat_facts`); focus brings them back as they were. The status line
    // is the live Composer's: Solo always, the focused board Pane; an
    // unfocused board Pane draws none and holds no room for one (FL-11).
    let live = !grid || focused || drop_target;
    let status = !grid || focused;
    let flat = (!live).then(|| flat_facts(files, queued.len())).flatten();
    let (attachments, queued) = if live {
        (attachments, queued)
    } else {
        (None, Vec::new())
    };
    // Solo's status line alone names the changed files and the keys at its
    // right; a focused board Pane's stops at `ctx` (R4).
    let (changed_files, actions) = if grid {
        (None, None)
    } else {
        (changed_files, actions)
    };
    let geometry = view.geometry.clone();
    let band = div()
        .debug_selector(|| "composer-block".into())
        .when(drop_target, |band| {
            band.debug_selector(|| "composer-drop-target".into())
        })
        .when(!live, |band| band.debug_selector(|| "composer-flat".into()))
        .relative()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .min_w_0()
        .pl(px(theme::COMPOSER_PAD_L))
        .pr(px(theme::COMPOSER_PAD_R))
        .py(px(theme::COMPOSER_PAD_Y))
        .bg(theme::paint::INBAND)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(TEXT))
        .when(decision.is_some(), |band| band.key_context("Decision"));
    let mut band = components::on_bounds(band, move |bounds, _, _| {
        geometry.set(PaneGeometry {
            composer: Some(bounds),
            ..geometry.get()
        })
    });
    // Files hovering the Pane: the band's edge in `ACCENT_EDGE`, laid over
    // it so nothing reflows.
    if drop_target {
        band = band.child(
            div()
                .absolute()
                .inset_0()
                .border_1()
                .border_color(rgba(theme::ACCENT_EDGE)),
        );
    }
    if let Some(error) = draft_error {
        band = band.child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .pl(px(theme::COMPOSER_GUTTER))
                .text_color(rgb(BLOCKED))
                .child(div().min_w_0().whitespace_normal().child(error)),
        );
    }

    // The queue shares the Composer's height budget. Keep the latest on
    // top and every earlier prompt reachable by scrolling; a long queue
    // must never push the editor or the Thread's status out of its Pane.
    if !queued.is_empty() {
        let count = queued.len();
        let namespace = view.text_namespace();
        band =
            band.child(
                div()
                    .debug_selector({
                        let namespace = namespace.clone();
                        move || format!("composer-queue-{namespace}")
                    })
                    .flex_shrink_0()
                    .h(px(queue_height))
                    .child(
                        div()
                            .h_full()
                            .overflow_y_scrollbar()
                            // Set the wrapper ID: assigning the inner Div's ID
                            // would leave all Panes sharing call-site scroll state.
                            .id(SharedString::from(format!("composer-queue-{namespace}")))
                            .child(div().flex().flex_col().children(
                                queued.iter().enumerate().map(|(index, held)| {
                                    let namespace = namespace.clone();
                                    div()
                                        .flex_shrink_0()
                                        .font_family(theme::FONT_CODE)
                                        .debug_selector(move || {
                                            format!("queue-row-{namespace}-{index}")
                                        })
                                        .child(queued_line(held, index, count, editing && empty))
                                }),
                            )),
                    ),
            );
    }
    // The one line that grows: the Composer's element is `COMPOSER_ROW_H`
    // per visual row, so the line height here IS the row pitch. The
    // placeholder overlays its first row while the line is empty — on the
    // live line, and on a flat one only while a Decision waits (it says
    // where the answer goes); any other unfocused Pane's line says nothing.
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
        .font_family(theme::FONT_CODE)
        .font_weight(theme::W_BODY)
        .line_height(px(theme::COMPOSER_ROW_H))
        .text_color(rgb(TEXT))
        .child(view.composer.clone());
    if empty && (live || decision.is_some()) {
        let ghost = placeholder(
            decision.is_some(),
            setup_controls.is_some(),
            transcript,
            suggestion,
        );
        line = line.child(ghost_row(ghost, compact));
    }

    // The `❯` is always in layout and always the accent (the prompt's own
    // colour), hanging in its 2-cell gutter on the first row.
    let line_selector = format!(
        "composer-{}-{}",
        if live { "live" } else { "flat" },
        view.text_namespace()
    );
    let input = div()
        .debug_selector(move || line_selector.clone())
        .flex()
        .items_start()
        .min_h(px(theme::COMPOSER_ROW_H))
        .min_w_0()
        .child(
            div()
                .debug_selector(|| "composer-mark".into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .w(px(theme::COMPOSER_GUTTER))
                .h(px(theme::COMPOSER_ROW_H))
                .child(components::prompt_mark(ACCENT)),
        )
        // A flat line's facts come straight after its `❯`, before any
        // draft text it holds.
        .children(flat.map(|facts| facts.mr(px(theme::CH))))
        .child(line);
    band = band.child(input);
    // The popover paints above the stack — deferred, so it escapes the
    // Pane's clip and draws over the transcript (#24) — hung from the `❯`
    // column, at most `SLASH_MENU_W` wide.
    if let Some(menu) = menu {
        band = band.child(deferred(
            div()
                .absolute()
                .bottom(relative(1.))
                .left(px(theme::COMPOSER_PAD_L))
                .right(px(theme::COMPOSER_PAD_R))
                .mb(px(theme::FLOAT_OFFSET))
                .child(div().max_w(px(theme::SLASH_MENU_W)).child(menu)),
        ));
    }

    // The status line (the prototype's `.status`), the live Composer's
    // only: segments split by a faint `·`, Solo's keys at the right. Its
    // first segment hangs its padding out, so its text starts on the `❯`
    // column; the right segment's text ends three cells in from the Pane's
    // edge.
    let status_line = status.then(|| {
        let mut segments: Vec<AnyElement> = Vec::new();
        if let Some(setup) = setup_controls {
            segments.push(
                div()
                    .flex_shrink(1.)
                    .min_w_0()
                    .child(setup)
                    .into_any_element(),
            );
        }
        // The word is the live Session's permission mode, so it rides every
        // Pane whose Session has announced one other than the default — a
        // pending Decision too: the mode is what the answer will run under.
        if let Some(mode) = mode {
            let key = view.thread().map_or(0, ThreadId::get);
            segments.push(
                div()
                    .debug_selector(move || format!("composer-mode-{key}"))
                    .flex_shrink_0()
                    .child(match mode_picker {
                        Some(picker) => picker,
                        None => mode_chip(mode, false).into_any_element(),
                    })
                    .into_any_element(),
            );
        }
        for segment in [model_picker, usage_meter, changed_files]
            .into_iter()
            .flatten()
        {
            segments.push(div().flex_shrink_0().child(segment).into_any_element());
        }
        let mut line = div()
            .debug_selector(|| "composer-meta".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(theme::COMPOSER_STATUS_H))
            .mt(px(theme::COMPOSER_STATUS_GAP))
            // The first segment hangs its padding out (the prototype's
            // `.seg:first-child{margin-left:-1ch}`); the last keeps its own
            // inside the Composer's three cells.
            .pl(px(theme::STATUS_HANG_PAD))
            .pr(px(theme::COMPOSER_PAD_R))
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(px(theme::FS_UI))
            .line_height(px(theme::LH_UI))
            .text_color(rgb(TEXT_MUTED));
        for (index, segment) in segments.into_iter().enumerate() {
            if index > 0 {
                line = line.child(status_seam());
            }
            line = line.child(segment);
        }
        line.child(div().flex_1().min_w(px(theme::CH)))
            .children(actions.map(|actions| div().flex_shrink_0().child(actions)))
    });
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .min_w_0()
        .pb(px(theme::COMPOSER_STATUS_PAD_B))
        .when(attachments.is_some() || background.is_some(), |stack| {
            // The shelf stands over the band: pending files from the `❯`
            // column, the background chips at right. The chips give way
            // first — they cut their labels, the files do not.
            stack.child(
                div()
                    .debug_selector(|| "composer-shelf".into())
                    .flex()
                    .items_end()
                    .gap(px(theme::CH))
                    .min_w_0()
                    .pl(px(theme::COMPOSER_PAD_L))
                    .pr(px(theme::COMPOSER_PAD_R))
                    .pb(px(theme::SHELF_GAP))
                    .when_some(attachments, |shelf, attachments| {
                        shelf.child(div().flex_1().min_w_0().child(attachments))
                    })
                    .when_some(background, |shelf, chips| {
                        shelf.child(div().ml_auto().min_w_0().max_w_full().child(chips))
                    }),
            )
        })
        // The working line (`.comp > .r`), on the transcript rows' axis,
        // `COMPOSER_WORKING_GAP` over the band's upper rule.
        .when_some(working, |stack, working| {
            stack.child(
                div()
                    .debug_selector(|| "transcript-progress".into())
                    .flex_shrink_0()
                    .min_w_0()
                    .pl(px(theme::COMPOSER_PAD_L))
                    .pr(px(theme::COMPOSER_PAD_R))
                    .mb(px(theme::COMPOSER_WORKING_GAP))
                    .child(working),
            )
        })
        .child(composer_rule())
        .child(band)
        .child(composer_rule())
        .children(status_line)
}

/// The faint `·` between two status segments.
pub(crate) fn status_seam() -> Div {
    components::cells("\u{b7}")
        .w(px(theme::STATUS_SEAM_W))
        .text_color(rgb(TEXT_FAINT))
}

/// A status segment (the prototype's `.status .seg`): one row, a cell of
/// padding each side, its words in `ink`, a mark a cell before them. No
/// ground at rest and no chevron: the button it rides in
/// (`composer_control`, `draft_picker`) or its own id'd wrapper wears
/// `paint::HOVER` under the pointer. Render-only; the cockpit wires it.
fn status_seg(ink: u32) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::CH))
        .h(px(theme::COMPOSER_STATUS_H))
        .pl(px(theme::COMPOSER_SEG_PAD_X))
        .pr(px(theme::COMPOSER_SEG_PAD_TAIL))
        .whitespace_nowrap()
        .font_family(theme::FONT_UI)
        .font_weight(theme::W_BODY)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(ink))
}

/// The status line's mode segment: the drawn marker (`⏵⏵` for a mode that
/// lets edits run, `⏸` for plan, `⏵` for any other) as the fallback face
/// (STIX Two Math) sets it — its cell of advance one line tall, on the
/// line's baseline — one space, and the mode word, both in `MODE_INK`.
/// Hidden at the default: the mode stays reachable through ⇧⇥ and the
/// palette's `permission mode`.
pub fn mode_chip(mode: &str, _menu: bool) -> Div {
    let (mark, advance) = match mode {
        "accept edits" | "bypass permissions" | "auto" => {
            (icons::MODE_ACCEPT, theme::MODE_MARK_ADVANCE)
        }
        "plan" => (icons::MODE_PLAN, theme::MODE_PLAN_ADVANCE),
        _ => (icons::MODE_ON, theme::MODE_MARK_ONE_ADVANCE),
    };
    status_seg(theme::MODE_INK)
        .child(icon(mark, advance, theme::MODE_INK).h(px(theme::LH_UI)))
        .child(components::cells(mode.to_owned()))
}

/// The button a status segment rides in (model, effort, mode, files, the
/// session `•••`): no padding of its own and square, so the hover
/// (`paint::HOVER`) and press faces fill exactly the segment.
pub fn composer_control(
    id: impl Into<gpui::ElementId>,
    cx: &gpui::App,
) -> gpui::component::button::Button {
    chip_button(id, cx)
        .p_0()
        .h_auto()
        .rounded(px(theme::R_CHIP))
}

/// A segment's button: no ground at rest, `paint::HOVER` under the pointer
/// over the one 150ms blend, `paint::PRESS` pressed at once.
fn chip_button(id: impl Into<gpui::ElementId>, cx: &gpui::App) -> gpui::component::button::Button {
    components::faded_button(
        id,
        gpui::rgba(theme::TRANSPARENT).into(),
        theme::paint::HOVER.into(),
        theme::paint::PRESS.into(),
        rgb(TEXT_MUTED).into(),
        cx,
    )
}

/// The status line's changed-files segment: `3 files` (`1 file`) and the
/// Thread's whole `+N −N`, the signs in the diff hues — a reading that
/// opens the changed-files card.
pub fn files_chip(count: usize, added: usize, removed: usize) -> Div {
    let word = if count == 1 { "file" } else { "files" };
    status_seg(TEXT_MUTED)
        .debug_selector(move || format!("changed-files-{count}"))
        .child(components::tabular(components::cells(format!(
            "{count} {word}"
        ))))
        .child(diff_stat(added, removed))
}

/// One row of the changed-files card, in the float grammar: the file's
/// name, its directory muted two cells after it, and its `+N −N` hard
/// right. The whole row opens the file in the reader.
pub fn changed_file_row(
    index: usize,
    name: SharedString,
    dir: Option<SharedString>,
    added: usize,
    removed: usize,
) -> gpui::Stateful<Div> {
    let id = gpui::ElementId::from(SharedString::from(format!("changed-file-{index}")));
    let key = crate::pointer::hover_key(&id);
    components::text_ui()
        .id(id)
        .debug_selector(move || format!("changed-file-{index}"))
        .flex()
        .items_center()
        .min_w_0()
        .h(px(theme::FLOAT_ROW_H))
        .px(px(theme::FLOAT_PAD_X))
        .whitespace_nowrap()
        .cursor_pointer()
        .hover_float(key)
        .press_float()
        .child(crate::menu::gutter(false))
        .child(
            div()
                .flex_shrink(1.)
                .min_w_0()
                .truncate()
                .text_color(rgb(theme::PATH_INK))
                .child(name),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .ml(px(theme::FLOAT_DETAIL_GAP))
                .truncate()
                .text_color(rgb(TEXT_MUTED))
                .children(dir),
        )
        .child(
            div()
                .flex_shrink_0()
                .ml(px(theme::FLOAT_DETAIL_GAP))
                .child(diff_stat(added, removed)),
        )
}

/// Solo's status line right (FL-10): `⇧⇥ mode · ? shortcuts` in
/// `TEXT_MUTED`, its keys read from the key table and drawn
/// (`components::key_combo`), as one quiet segment.
pub fn shortcuts_hint(cycle: Option<String>, help: Option<String>) -> Div {
    let mut seg = status_seg(TEXT_MUTED)
        .gap(px(0.))
        .debug_selector(|| "status-keys".into());
    let led = cycle.is_some();
    if let Some(cycle) = cycle {
        seg = seg
            .child(components::key_combo(&cycle, TEXT_MUTED))
            .child(components::cells("\u{a0}mode"));
    }
    if let Some(help) = help {
        if led {
            seg = seg.child(components::cells("\u{a0}\u{b7}\u{a0}"));
        }
        seg = seg
            .child(components::key_combo(&help, TEXT_MUTED))
            .child(components::cells("\u{a0}shortcuts"));
    }
    seg
}

/// The idle line's placeholder: words in `TEXT_MUTED` after the caret cell
/// and a space — a head that always shows, then pieces after a faint `·`
/// that drop out whole from the right where the line is narrow, never a
/// word cut in half.
///
/// A predicted follow-up is already in the operator's voice and already
/// filtered, so it is shown verbatim — a draft of their next prompt, not a
/// description of one — with `⇥ accept`, which is always kept whole: an
/// accept key nobody knows about is the same as no accept key, so the
/// prediction's own words give way to it.
#[derive(Clone, Debug, PartialEq)]
struct Ghost {
    /// The words that always show (cut at their end only when even they do
    /// not fit).
    head: SharedString,
    /// The pieces after it, each after a faint `·`; they drop out whole.
    more: Vec<SharedString>,
    /// A verbatim prediction: its words give way to the accept hint (the
    /// one piece in `more`), which never goes.
    verbatim: bool,
}

impl Ghost {
    fn pieces(head: &'static str, more: &[&'static str]) -> Self {
        Self {
            head: head.into(),
            more: more
                .iter()
                .map(|piece| SharedString::from(*piece))
                .collect(),
            verbatim: false,
        }
    }

    /// Whether the hint stays whole whatever the width (`⇥ accept`).
    fn keeps_hint(&self) -> bool {
        self.verbatim
    }

    /// Every rung the line can show, longest first: what `ghost_row`'s
    /// row-fit picks from, spelled out for the tests.
    #[cfg(test)]
    fn rungs(&self) -> Vec<String> {
        if self.verbatim {
            return vec![std::iter::once(self.head.to_string())
                .chain(self.more.iter().map(|piece| piece.to_string()))
                .collect::<Vec<_>>()
                .join(" \u{b7} ")];
        }
        (0..=self.more.len())
            .rev()
            .map(|kept| {
                std::iter::once(self.head.to_string())
                    .chain(self.more[..kept].iter().map(|piece| piece.to_string()))
                    .collect::<Vec<_>>()
                    .join(" \u{b7} ")
            })
            .collect()
    }
}

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
        return Ghost::pieces(
            "Start a thread",
            &["/ for commands", "drop or paste images"],
        );
    }
    let busy = transcript.is_some_and(|transcript| transcript.status() == Status::Streaming);
    match followup::suggest(pending, transcript, suggestion) {
        // A docked Decision owns the block above this line; the line itself
        // still steers, so it says so (rule 2.8.1: one input line).
        Followup::Decision => Ghost::pieces("answer above, or steer", &[]),
        Followup::Revive => Ghost::pieces("Revive and continue", &["/ for commands"]),
        Followup::Suggested(text) => Ghost {
            head: SharedString::from(text),
            more: vec![SharedString::from("\u{21e5} accept")],
            verbatim: true,
        },
        Followup::Steer if busy => {
            Ghost::pieces("queue a follow-up", &["\u{23ce} sends when the turn ends"])
        }
        Followup::Steer => Ghost::pieces(
            "Steer this thread",
            &["/ for commands", "drop or paste images"],
        ),
    }
}

/// What an unfocused board cell's flat line says in place of the shelf and
/// the queue it keeps for the live Composer: `1 attachment`, `3
/// attachments`, `2 queued`, joined by a faint `·`, in `TEXT_MUTED` with
/// tabular digits — words, no chip and no ×. Nothing when the draft holds
/// neither.
fn flat_facts(files: usize, queued: usize) -> Option<Div> {
    let text = flat_facts_text(files, queued);
    (!text.is_empty()).then(|| {
        let seams = separators(&text);
        components::tabular(
            div()
                .debug_selector(|| "composer-flat-facts".into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .h(px(theme::COMPOSER_ROW_H))
                .whitespace_nowrap()
                .font_family(theme::FONT_UI)
                .text_size(px(theme::FS_UI))
                .text_color(rgb(TEXT_MUTED))
                .child(StyledText::new(text).with_highlights(seams)),
        )
    })
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

/// The placeholder drawn on a clipped, wrapping row one line high, starting
/// two cells in (after the caret's cell and a space): the head first, then
/// each further piece after its faint `·` as one whole, so a piece that does
/// not fit falls to the hidden second line and the row shows the longest
/// rung that fits. A prediction instead lets its own words truncate before
/// the accept hint, which never goes. A compact (L2) line shows the head
/// only.
fn ghost_row(ghost: Ghost, compact: bool) -> Div {
    // The placeholder is one muted run, its `·` too (the prototype's
    // `.ph-text`).
    let seam = || {
        components::cells("\u{b7}")
            .px(px(theme::CH))
            .w(px(3.0 * theme::CH))
            .text_color(rgb(TEXT_MUTED))
    };
    let more: Vec<SharedString> = if compact && !ghost.verbatim {
        Vec::new()
    } else {
        ghost.more.clone()
    };
    let pieces = more.into_iter().enumerate().map(move |(index, piece)| {
        div()
            .flex()
            .flex_shrink_0()
            .h(px(theme::COMPOSER_ROW_H))
            .when(index == 0, |piece| {
                piece.debug_selector(|| "prompt-placeholder-hint".into())
            })
            .child(seam())
            .child(components::cells(piece))
    });
    let row = div()
        .debug_selector(|| "prompt-placeholder".into())
        .absolute()
        .left(px(2.0 * theme::FS_UI * theme::CODE_ADVANCE))
        .right_0()
        .top_0()
        .h(px(theme::COMPOSER_ROW_H))
        .flex()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_color(rgb(TEXT_MUTED));
    if ghost.keeps_hint() {
        return row
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h(px(theme::COMPOSER_ROW_H))
                    .truncate()
                    .child(ghost.head),
            )
            .children(pieces);
    }
    // A hair over its cells, so the shaped run never reads as cut.
    let head_w = components::cells_width(&ghost.head) + 0.1;
    row.flex_wrap()
        .child(
            div()
                .flex_shrink(1.)
                .min_w_0()
                .w(px(head_w))
                .h(px(theme::COMPOSER_ROW_H))
                .truncate()
                .child(ghost.head),
        )
        .children(pieces)
}

/// #11: whether this Thread still offers adopting a CLI session — no
/// conversation yet (nothing in the transcript beyond Ferrite's own notices
/// and bookkeeping) and at rest. One predicate for every surface that opens
/// the door — the placeholder hint, the `/` menu's local entry, and the
/// pick that closes the blank Thread — so no two can disagree.
pub fn offers_import(transcript: Option<&Transcript>) -> bool {
    transcript.is_some_and(Transcript::offers_import)
}

/// The status line's word for the Session's permission mode (rule 2.11.5):
/// `None` at the default, which is hidden; a known id's own word
/// (`theme::mode_word`); else the adapter's label, lowercased; else the id
/// split at its humps. Never a raw id, never a capital.
pub fn permission_mode_label(
    mode: &str,
    choices: &[ferrite_core::PermissionModeChoice],
) -> Option<SharedString> {
    let word = theme::mode_word(mode)?;
    if theme::known_mode(mode).is_some() {
        return Some(word);
    }
    Some(
        choices
            .iter()
            .find(|choice| choice.value == mode)
            .map_or(word, |choice| choice.label.to_lowercase().into()),
    )
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
    /// Matched byte ranges inside `name`, painted `ACCENT` at `W_STRONG`
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

/// A `/` or `@` row in the float grammar: the cursor's `❯`, the name with
/// its matches in the accent, the detail muted in its own column
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

/// The bounded queue viewport, shared with the editor's pane-height budget.
pub(crate) fn composer_queue_height(height: f32, compact: bool, grid: bool, count: usize) -> f32 {
    let budget = height * theme::COMPOSER_MAX_PANE_FRACTION
        - composer_fixed_height(compact, grid)
        - theme::COMPOSER_ROW_H;
    let fitting = (budget / theme::QUEUE_ROW_H).floor().max(1.) as usize;
    let rows = count.min(fitting).min(if compact {
        theme::COMPOSER_COMPACT_QUEUE_ROWS
    } else {
        theme::COMPOSER_QUEUE_ROWS
    });
    rows as f32 * theme::QUEUE_ROW_H
}

/// The live Composer's height less its editor rows and queue: the band's
/// padding, its two rules, the status line with its gap, and the air under
/// the stack — the same at every level. A quiet board cell is shorter by
/// the status line and its gap (`composer_quiet_height`); the budget reads
/// the live one, the taller. The shelf and the working line stand above
/// and are not part of it.
fn composer_fixed_height(_compact: bool, _grid: bool) -> f32 {
    composer_quiet_height() + theme::COMPOSER_STATUS_GAP + theme::COMPOSER_STATUS_H
}

/// An unfocused board cell's Composer less its editor rows (FL-11): the
/// band's padding, its two rules and the air under it — flush, no status.
pub(crate) fn composer_quiet_height() -> f32 {
    2. * theme::COMPOSER_PAD_Y + 2. * theme::COMPOSER_RULE + theme::COMPOSER_STATUS_PAD_B
}

/// Leave the majority of a Pane available for its Thread context. Only the
/// viewport changes: the Composer keeps every character and scrolls to its
/// caret, then reveals more rows again when the Pane grows.
pub(crate) fn composer_row_limit(height: f32, compact: bool, grid: bool, queued: usize) -> usize {
    let fixed = composer_fixed_height(compact, grid);
    let queue = composer_queue_height(height, compact, grid, queued);
    ((height * theme::COMPOSER_MAX_PANE_FRACTION - fixed - queue) / theme::COMPOSER_ROW_H)
        .floor()
        .max(1.)
        .min(crate::composer::MAX_ROWS as f32) as usize
}

/// A prompt written while the agent was still working: a dim `❯` line, in
/// the grammar it will enter the transcript with. `index` counts down the
/// pile from the top; only the top row (0, the latest) carries the count
/// and — while the Composer holds the keyboard and its line is empty, the
/// only time they act — the keys that take it back: ↑ restores it into the
/// line, ⌫ drops it.
fn queued_line(held: &str, index: usize, count: usize, keys: bool) -> impl IntoElement {
    let latest = index == 0;
    let hints = if count > 1 {
        format!("{count} queued")
    } else {
        "queued".to_owned()
    };
    let hints = if keys {
        format!("{hints} \u{b7} \u{2191} edit \u{b7} \u{232b} drop")
    } else {
        hints
    };
    let seams = separators(&hints);
    div()
        .debug_selector(move || format!("queued-{index}"))
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::QUEUE_ROW_H))
        .min_w_0()
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .w(px(theme::COMPOSER_GUTTER))
                .h(px(theme::QUEUE_ROW_H))
                .child(components::prompt_mark(TEXT_FAINT)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(rgb(TEXT_MUTED))
                .child(SharedString::from(held.to_owned())),
        )
        .when(latest, |row| {
            row.child(components::tabular(
                div()
                    .flex_shrink_0()
                    .ml(px(theme::FLOAT_DETAIL_GAP))
                    .whitespace_nowrap()
                    .text_color(rgb(TEXT_MUTED))
                    .child(StyledText::new(hints).with_highlights(seams)),
            ))
        })
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
            })
            .into_any_element(),
    )
}

/// The Decision's subject in its parts: the tool, then what it would do —
/// the command itself when the input carries one, else the provider's
/// description. A question is its summary.
struct DecisionSubject {
    tool: Option<SharedString>,
    text: Option<SharedString>,
}

fn subject_parts(decision: &Decision) -> DecisionSubject {
    if let Some(questions) = questions_of(decision) {
        return DecisionSubject {
            tool: None,
            text: Some(ferrite_core::questions::summary(questions).into()),
        };
    }
    let command = decision
        .input
        .get("command")
        .and_then(serde_json::Value::as_str)
        .filter(|command| !command.trim().is_empty());
    let tool = (!decision.tool_name.is_empty()).then(|| decision.tool_name.clone().into());
    let text = match command {
        Some(command) => Some(command.to_string().into()),
        None => (!decision.description.is_empty()).then(|| decision.description.clone().into()),
    };
    if tool.is_none() && text.is_none() {
        return DecisionSubject {
            tool: None,
            text: Some("unreadable permission request".into()),
        };
    }
    DecisionSubject { tool, text }
}

/// The Decision's subject as one line of words — `Bash · gh issue close
/// 212`, never a `Bash:` label; the tool's name alone without a
/// description, else the honest unreadable fallback. Every surface that
/// names a Decision in words (the wall alert's tooltip) goes through here.
fn decision_subject(decision: &Decision) -> SharedString {
    let DecisionSubject { tool, text } = subject_parts(decision);
    match (tool, text) {
        (Some(tool), Some(text)) => format!("{tool} \u{b7} {text}").into(),
        (Some(only), None) | (None, Some(only)) => only,
        (None, None) => "unreadable permission request".into(),
    }
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

/// `+N −N`: a change's size, the whole `+9` in the diff's green and the
/// whole `−4` in its red (FL-10), at the grid's own size, one text run so
/// gpui's per-run pixel rounding cannot widen it. Shared by a tool row's
/// trail, a changed-strip chip, aggregate instruments and the Composer's
/// file shelf.
pub(crate) fn diff_stat(added: usize, removed: usize) -> Div {
    let added_text = format!("+{added}");
    let text = format!("{added_text} \u{2212}{removed}");
    let removed_at = added_text.len() + 1;
    let run = |range: std::ops::Range<usize>, ink: u32| {
        (
            range,
            HighlightStyle {
                color: Some(rgb(ink).into()),
                ..Default::default()
            },
        )
    };
    let highlights = vec![
        run(0..added_text.len(), RUNNING),
        run(removed_at..text.len(), BLOCKED),
    ];
    components::tabular(
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .w(px(components::cells_width(&text)))
            .text_size(px(theme::FS_UI))
            .text_color(rgb(TEXT_MUTED))
            .child(StyledText::new(SharedString::from(text)).with_highlights(highlights)),
    )
}

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

/// The usage meter's detail card: the meter's own windows, in the meter's
/// own order, each a labelled bar over the reading behind it. Counts are
/// reported values, never estimates. An account window the provider has
/// not reported is left out; when it reports neither, one line says so
/// (`Limits not reported by this provider`) in place of two empty bars.
/// Each block holds the menu rows' inset (`MENU_ROW_PAD_X`) inside the
/// floating surface's `FLOAT_PAD`.
///
/// The context bar is stacked by category when the provider says what
/// fills the window, and its heading is a row that opens the legend: every
/// category with its ink, its count and its share of the window. The card
/// grows to `max_h` (the window's room) and scrolls past it.
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
    // One 4px bar, full width: the same track and the same status ink as
    // the meter that opened the card, at a size a card can afford.
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
                .text_color(rgb(TEXT_MUTED))
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
            .px(px(theme::FLOAT_PAD_X))
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
            .text_color(rgb(TEXT_MUTED))
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
    let in_window: Vec<(u32, f32)> = categories
        .iter()
        .enumerate()
        .filter(|(_, category)| category_in_window(&category.name) && category.tokens > 0)
        .map(|(index, category)| {
            (
                category_ink(&category.name, index),
                share_of(category.tokens),
            )
        })
        .collect();
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
                        icon(icons::CHEVRON_RIGHT, theme::ICON_CHEVRON, TEXT_MUTED)
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
            .mx(px(-theme::FLOAT_PAD_X))
            .px(px(theme::FLOAT_PAD_X))
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
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(rgb(TEXT_MUTED))
                        .child(count),
                )
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
        .w(px(theme::USAGE_CARD_W))
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
            .px(px(theme::FLOAT_PAD_X))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(TEXT_MUTED))
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

/// A provider's category name in sentence case ("mcp tools" → "Mcp
/// tools" is wrong, so known acronyms keep their capitals).
fn sentence_case(name: &str) -> String {
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

/// A usage token's ink: `TEXT_MUTED` like every value word, the whole
/// token turning `ATTENTION` once its window runs tight
/// (`USAGE_TIGHT`), the same step as the card behind it (`usage_ink`).
pub fn readout_ink(fraction: f32) -> u32 {
    if fraction >= theme::USAGE_TIGHT {
        ATTENTION
    } else {
        TEXT_MUTED
    }
}

/// The tokens the status line reads, in order: `ctx 32%` when the context
/// window is known, then the tightest account window only while it runs
/// tight (`5h 91%`). Each is one run and one ink.
fn usage_tokens(
    context: Option<f32>,
    limits: ferrite_core::transcript::RateLimits,
) -> Vec<(String, f32)> {
    let percent = |fraction: f32| (fraction.clamp(0., 1.) * 100.).round() as u32;
    let worst = [
        ("5h", limits.five_hour.map(|limit| limit.used_fraction)),
        ("wk", limits.weekly.map(|limit| limit.used_fraction)),
    ]
    .into_iter()
    .filter_map(|(name, used)| used.map(|used| (name, used)))
    .filter(|(_, used)| *used >= theme::USAGE_TIGHT)
    .max_by(|a, b| a.1.total_cmp(&b.1));
    context
        .map(|used| ("ctx", used))
        .into_iter()
        .chain(worst)
        .map(|(name, used)| (format!("{name} {}%", percent(used)), used))
        .collect()
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

/// The 8-cell, 4px ctx meter on its `paint::LINE2` track, filled to
/// `fraction` in `ctx_meter_ink`: the prototype's `.meter`, its 1px
/// radius rounding the track's ends and the fill's start (the track clips
/// it), set `CTX_METER_DROP` below the row's centre as `vertical-align:
/// middle` sets it on the text.
fn ctx_meter(fraction: f32) -> Div {
    let fraction = fraction.clamp(0., 1.);
    let radius = px(theme::CTX_METER_R);
    div()
        .debug_selector(|| "usage-meter-bar".into())
        .relative()
        .top(px(theme::CTX_METER_DROP))
        .flex_shrink_0()
        .w(px(theme::CTX_METER_W))
        .h(px(theme::CTX_METER_H))
        .rounded(radius)
        .bg(theme::paint::LINE2)
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(relative(fraction))
                .rounded_l(radius)
                .when(fraction >= 1., |fill| fill.rounded(radius))
                .bg(rgb(ctx_meter_ink(fraction))),
        )
}

/// The status line's usage segment (the prototype's `ctx ▬▬▭▭ 32%`): `ctx`,
/// the 8-cell meter and the percentage, tabular `TEXT_MUTED`, then a tight
/// account window (`5h 91%`, `ATTENTION`) — inside the segment that opens
/// the usage card. `None` when there is nothing to read: no reading is ever
/// invented (no `ctx —`).
pub fn usage_meter_body(
    context: Option<f32>,
    limits: ferrite_core::transcript::RateLimits,
) -> Option<Div> {
    let tokens = usage_tokens(context, limits);
    if tokens.is_empty() {
        return None;
    }
    let key = context.map_or_else(
        || "unknown".to_owned(),
        |used| ((used.clamp(0., 1.) * 100.).round() as u32).to_string(),
    );
    Some(
        status_seg(TEXT_MUTED).child(components::tabular(
            div()
                .debug_selector(move || format!("usage-readout-{key}"))
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(theme::CH))
                .whitespace_nowrap()
                .children(tokens.into_iter().map(|(token, used)| {
                    let selector = token.clone();
                    let reading = div()
                        .debug_selector(move || format!("usage-token-{selector}"))
                        .flex()
                        .items_center()
                        .gap(px(theme::CH));
                    // The context window reads as a meter between its name
                    // and its share; an account window as plain words.
                    match token.strip_prefix("ctx ") {
                        Some(share) => reading
                            .child(components::cells("ctx"))
                            .child(ctx_meter(used))
                            .child(components::cells(share.to_owned())),
                        None => reading
                            .text_color(rgb(readout_ink(used)))
                            .child(components::cells(token)),
                    }
                })),
        )),
    )
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
/// Pane is on right now" can never be spelled two ways: the accent check on
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
    pub(crate) fn of(reading: ferrite_core::settings::ReadingSize) -> Self {
        Self {
            size: theme::answer_text_size(reading),
            line: theme::answer_line_height(reading),
        }
    }

    /// One cell of the grid.
    pub(crate) fn cell(self) -> f32 {
        theme::tx_cell(self.size)
    }

    /// The 2-cell glyph gutter.
    pub(crate) fn gutter(self) -> f32 {
        theme::tx_gutter(self.size)
    }

    /// A drawn mark's box (`❯ ✻ ◆ ∴`).
    pub(crate) fn mark(self) -> f32 {
        theme::tx_mark(self.size)
    }

    /// A drawn star's box (`✻`), whole pixels.
    pub(crate) fn star(self) -> f32 {
        theme::tx_star(self.size)
    }

    /// Half a line, whole pixels.
    pub(crate) fn half(self) -> f32 {
        theme::tx_half(self.line)
    }
}

/// The agent's and a tool call's bullet: Geist Mono's own `●`.
pub(crate) const BULLET: &str = "\u{25cf}";
/// A call whose result never came: the hollow `○`.
const RING: &str = "\u{25cb}";
/// The result elbow: Geist Mono's own `└`.
const ELBOW: &str = "\u{2514}";

/// A provider's brand colour, for its `✻` and its working spinner;
/// `TEXT_MUTED` when the provider is not known.
pub(crate) fn provider_ink(provider: Option<Provider>) -> u32 {
    match provider {
        Some(Provider::Claude) => theme::PROVIDER_CLAUDE,
        Some(Provider::Codex) => theme::PROVIDER_CODEX,
        None => TEXT_MUTED,
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

/// A drawn gutter mark (`❯ ✻ ◆ ∴`): centred on the row's first line, at the
/// gutter's left edge.
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
    pub preview: &'a crate::attachment_preview::Preview,
    pub reading: ferrite_core::settings::ReadingSize,
    pub wide: bool,
    pub focused: bool,
    pub scope: &'a SharedString,
    pub workspace: Option<&'a std::path::Path>,
}

impl RowCx<'_> {
    fn grid(&self) -> Grid {
        Grid::of(self.reading)
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
        Body::Prompt(line) => prompt_row(block, line, row_cx, false),
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
                            .text_color(rgb(TEXT_MUTED))
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
        // `TEXT_MUTED`, never italic. A short thought shows whole; a long one
        // is its first line with the rest disclosed.
        Body::Thinking(thought) => {
            let (summary, details) = reasoning_text(thought);
            let mark = || mark_gutter(grid, icon(icons::REASONING, grid.mark(), TEXT_FAINT));
            let Some(details) = details else {
                // Nothing more was supplied. Keep the whole short block
                // visible, wrapped and selectable without a false disclosure.
                return row
                    .child(
                        grid_row(mark()).text_color(rgb(TEXT_MUTED)).child(
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
            row.text_color(rgb(TEXT_MUTED))
                .child(body)
                .into_any_element()
        }
        // A notice: a faint dot and one line in `TEXT_MUTED`, cut by width at
        // the column's edge with the whole of it one hover away. A Decision
        // never leaves one (it is its own row, at the tail).
        Body::Notice(text) => {
            let highlights = separators(text);
            let text = SharedString::from(text.clone());
            row.child(
                grid_row(glyph_gutter(grid, BULLET, TEXT_FAINT))
                    .text_color(rgb(TEXT_MUTED))
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
            .child(elbow_line(grid, TEXT_MUTED).child(
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

/// The prompt echo (the prototype's `.prompt`): a full-width
/// `paint::BAND` with half a line above and below, the accent `❯` in the
/// gutter, the operator's words in `TEXT_STRONG` at body weight with its
/// files' chips flowing inline after them, and its send time (`7:31 pm`,
/// `TEXT_MUTED`) in its own right-aligned column on the first line — so a
/// wrapping prompt keeps the time beside its first line. An image hangs
/// under the band in its 1px frame with its caption. Nothing on the band
/// answers the pointer. `pinned` draws exactly the band again for the top
/// of the body (not part of the selection, no image).
pub(crate) fn prompt_row(block: &Block, line: &str, row_cx: &RowCx, pinned: bool) -> AnyElement {
    let grid = row_cx.grid();
    let (text, files) = prompt_text(line);
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
        PromptText::Literal(words) if pinned => Some(
            div()
                .w_full()
                .min_w_0()
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
            .chips(row_cx.preview.clone())
            .into_any_element(),
        ),
        PromptText::Markdown(source) => Some(
            row_cx
                .selection
                .markdown(block.id, source)
                .ink(TEXT_STRONG)
                .chips(row_cx.preview.clone())
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
        .bg(theme::paint::BAND)
        .pt(px(grid.half()))
        .pb(px(grid.half() + chip_line))
        .pl(px(theme::TX_PAD_L))
        .pr(px(theme::TX_PAD_R))
        .font_weight(theme::W_BODY)
        .text_color(rgb(TEXT_STRONG))
        .child(mark_gutter(
            grid,
            icon(icons::PROMPT, grid.mark(), ACCENT).debug_selector(|| "prompt-mark".into()),
        ))
        .child(div().flex().flex_col().flex_1().min_w_0().children(words))
        .children(time.map(|time| {
            // Exactly its cells after its pad: right-aligned, a measured
            // run's rounding would stand it a pixel left.
            let cells = time.chars().count() as f32 + theme::PROMPT_TIME_PAD_CELLS;
            div()
                .debug_selector(|| "prompt-time".into())
                .flex_shrink_0()
                .w(px(cells * grid.cell()))
                .pl(px(theme::PROMPT_TIME_PAD_CELLS * grid.cell()))
                .whitespace_nowrap()
                .text_color(rgb(TEXT_MUTED))
                .child(SharedString::from(time))
        }));
    if pinned {
        return band.into_any_element();
    }
    let images: Vec<_> = files
        .iter()
        .enumerate()
        .filter(|(_, path)| is_image(path))
        .map(|(index, path)| inline_image(block.id, index, path, row_cx.preview, grid))
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
                .pl(px(theme::TX_PAD_L + grid.gutter()))
                .pr(px(theme::TX_PAD_R))
                .children(images),
        )
        .into_any_element()
}

/// Whether a file is an image gpui can draw.
fn is_image(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| gpui::Img::extensions().contains(&ext.to_ascii_lowercase().as_str()))
}

/// Open a sent file: an image in the Pane's preview, anything else with the
/// system.
fn open_file(
    path: &std::path::Path,
    preview: &crate::attachment_preview::Preview,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    if is_image(path) {
        preview.open(path.to_path_buf(), name, window, cx);
    } else {
        crate::file_links::FileLink {
            path: path.to_path_buf(),
            location: None,
        }
        .open(window, cx);
    }
}

/// A prompt's file as its band's inline chip (the prototype's `.att`): the
/// 12px image mark (or the file mark), a cell, the name in `PATH_INK`, on
/// `paint::BAND2` with a cell of padding each side. A click opens it (an
/// image in the Pane's preview). Sized for the Markdown flow that places
/// it: monospace cells at the size it is drawn in.
pub(crate) fn prompt_chip(
    path: std::path::PathBuf,
    label: &str,
    preview: Option<&crate::attachment_preview::Preview>,
    window: &mut gpui::Window,
    _cx: &mut gpui::App,
) -> (gpui::Size<gpui::Pixels>, AnyElement) {
    let size = f32::from(window.text_style().font_size.to_pixels(window.rem_size()));
    let cell = theme::tx_cell(size);
    let line = window.line_height();
    let width = 3.0 * cell + theme::GLYPH_BOX + label.chars().count() as f32 * cell;
    let image = is_image(&path);
    let host = preview.cloned();
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
        .bg(theme::paint::BAND2)
        .cursor_pointer()
        .whitespace_nowrap()
        .font_weight(theme::W_BODY)
        .text_color(rgb(theme::PATH_INK))
        .child(icon(
            if image { icons::IMAGE } else { icons::FILE },
            theme::GLYPH_BOX,
            theme::PATH_INK,
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
            match &host {
                Some(host) => open_file(&open, host, window, cx),
                None => crate::file_links::FileLink {
                    path: open.clone(),
                    location: None,
                }
                .open(window, cx),
            }
        });
    (gpui::size(px(width), line), element.into_any_element())
}

/// A sent image under its prompt (the prototype's `.img` and `.cap`): a
/// frame `IMAGE_CELLS` wide at the reading size, its 1px `paint::LINE2`
/// border included (374.4px at Standard, drawn 374), the picture filling
/// it at its own proportions; under it `name · W×H · 41 KB`, all of it
/// `TEXT_MUTED`. A click opens the preview.
fn inline_image(
    block: BlockId,
    index: usize,
    path: &std::path::Path,
    preview: &crate::attachment_preview::Preview,
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
            open_file(&open, &host, window, cx);
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
                .text_color(rgb(TEXT_MUTED))
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
        .text_color(rgb(TEXT_MUTED))
        .child(div().min_w_0().truncate().child(SharedString::from(format!(
            "\u{2026} {} not kept",
            text::byte_size(bytes)
        ))))
}

/// How a turn ended. A completed turn is `✻ Worked for 41s · 7:32 pm · ↑
/// 3.2k ↓ 1.1k`: the drawn `✻` in the provider's colour in the gutter, the
/// words and their `·` seams `TEXT_MUTED`. An interrupted or failed one
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
                    icon(icons::WORKED, grid.star(), provider_ink(provider))
                        .ml(px((grid.cell() - grid.star()) / 2.0))
                        .debug_selector(|| "turn-stamp-mark".into()),
                ))
                .debug_selector(|| "turn-stamp".into())
                .text_color(rgb(TEXT_MUTED))
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

/// The `·` seams in a transcript line: `TEXT_MUTED`, at body weight
/// (CT-23: a seam inside the transcript reads as the words around it — the
/// status line's faint seams are the Composer's). Highlighted in place so
/// the line stays one run and copies back exactly as written.
fn separators(text: &str) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    text.match_indices('\u{b7}')
        .map(|(at, dot)| {
            (
                at..at + dot.len(),
                HighlightStyle {
                    color: Some(rgb(TEXT_MUTED).into()),
                    font_weight: Some(theme::W_BODY),
                    ..Default::default()
                },
            )
        })
        .collect()
}

/// A notice as the L2 tail reads it: whole, or only its lead phrase (before
/// the first ` · `) while the Decision it announces is docked below it.
pub(crate) fn notice_text(text: &str, docked: bool) -> &str {
    match text.split_once(" \u{b7} ") {
        Some((lead, _)) if docked => lead,
        _ => text,
    }
}

/// A tool call's bullet: state is the bullet, never the name. Settled work
/// is `TEXT_MUTED`, live work a still green `●` (no pulse: the working line
/// is the one live thing), a failure `BLOCKED`, a call whose result never
/// came a hollow `○`. Green never means finished.
fn tool_dot_ink(state: &ToolState) -> (u32, DotShape) {
    match state {
        ToolState::Running => (RUNNING, DotShape::Solid),
        ToolState::Ok => (TEXT_MUTED, DotShape::Solid),
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
/// reads in `PATH_INK` like every path in the transcript and is a path
/// target.
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
/// `Bash`) — in `TEXT_STRONG` at `W_LABEL`, the parens and arguments in the
/// line's `TEXT_MUTED`, a path argument in `PATH_INK`; one selectable line
/// that copies back exactly as it reads.
fn call_highlights(tool: &ToolBlock) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    let label = text::tool_label(tool);
    let name = ferrite_core::transcript::display_tool_name(&tool.name)
        .len()
        .min(label.len());
    let mut highlights = vec![(
        0..name,
        HighlightStyle {
            color: Some(rgb(TEXT_STRONG).into()),
            font_weight: Some(theme::W_LABEL),
            ..Default::default()
        },
    )];
    if argument_is_path(tool) && label.len() > name + 2 {
        highlights.push((
            name + 1..label.len() - 1,
            HighlightStyle {
                color: Some(rgb(theme::PATH_INK).into()),
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
fn trail_diff_stat(added: usize, removed: usize) -> StyledText {
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
/// the rest and its seams `TEXT_MUTED` — is one selectable run with its
/// trailing seam; the excerpt the machine printed is its own run,
/// `TEXT_MUTED`, soft-wrapping and never cut.
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
    elbow_line(grid, TEXT_MUTED).child(
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
    let failed = match &tool.state {
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
    let result = lines
        .iter()
        .copied()
        .rfind(|line| line.trim_start().starts_with("test result:"));
    let counted = |verdict: &str| tests.iter().filter(|(_, _, v)| *v == verdict).count();
    let summary_of = |line: &str| -> Option<(usize, usize)> {
        let passed = passed_count(line)?;
        let failed = line
            .split(';')
            .find_map(|part| part.trim().strip_suffix(" failed")?.trim().parse().ok())
            .unwrap_or(0);
        Some((passed, failed))
    };
    if !failed {
        let passed = result
            .and_then(summary_of)
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
    let (passed, failed_count) = result
        .and_then(summary_of)
        .unwrap_or_else(|| (counted("ok"), fails.len()));
    let preview: Vec<String> = fails
        .iter()
        .take(3)
        .map(|(at, _, _)| {
            let line = lines[*at].trim();
            line.strip_prefix("test ").unwrap_or(line).to_string()
        })
        .collect();
    let shown = preview.len() + usize::from(result.is_some());
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
        .text_color(rgb(TEXT_MUTED))
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
/// column's edge like a terminal's), in `TEXT_MUTED` with only their verdict
/// words coloured.
fn output_lines_run(block: BlockId, text: String, selection: &TextRuns, grid: Grid) -> Div {
    after_elbow(grid)
        .debug_selector(|| "tool-fold-preview".into())
        .text_color(rgb(TEXT_MUTED))
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
            .text_color(rgb(TEXT_MUTED))
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
/// `212/357` in tabular `TEXT_MUTED`.
fn running_tests(call: &str, progress: ferrite_core::transcript::TestProgress, grid: Grid) -> Div {
    let total = progress.total.max(1);
    let fraction = (progress.done as f32 / total as f32).clamp(0., 1.);
    elbow_line(grid, TEXT_MUTED)
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
/// target alone wears `paint::HOVER`.
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
    let label = text::tool_label(tool);
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
            .text_color(rgb(TEXT_MUTED)),
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
    let (ink, shape) = tool_dot_ink(&tool.state);
    let row_selector = format!("tool-row-{}", tool.call);
    let mut line =
        grid_row(glyph_gutter(grid, shape.glyph(), ink).debug_selector(|| "tool-dot".into()))
            .id(SharedString::from(row_selector.clone()))
            .debug_selector(move || row_selector.clone())
            .relative()
            .text_color(rgb(TEXT_MUTED))
            .child(call)
            .child(div().flex_1())
            .when(trailing, |line| line.child(trail.pl(px(2.0 * grid.cell()))))
            // The bullet's cell toggles the call's details; nothing shows it.
            .children(bullet);
    if targeted {
        // The ground is the row's; the target's selector rides a layer over
        // it, so the row keeps its own.
        line = line.bg(theme::paint::HOVER).child(
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
                card = card.child(elbow_line(grid, TEXT_MUTED).child(
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
            card = card.child(elbow_line(grid, TEXT_MUTED).child(text::NO_RESULT));
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
                    .or_else(|| failed_excerpt(tool).map(str::to_owned));
                card = card.child(failure_line(
                    block,
                    failed_head(),
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
            } else if let Some((first, rest)) = command_fold(tool) {
                // `└ 7 issues · + 7 lines`: the whole output folds behind its
                // elbow; open, the rest hangs under it.
                // Each run holds exactly its cells: a measured run rounds
                // up a pixel and would push the fold's words off the grid.
                let first_w = first.chars().count() as f32 * grid.cell();
                card = card.child(
                    elbow_line(grid, TEXT_MUTED)
                        .child(
                            div()
                                .flex_shrink_0()
                                .w(px(first_w))
                                .whitespace_nowrap()
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
                                .text_color(rgb(TEXT_MUTED))
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
                    card = card.child(elbow_line(grid, TEXT_MUTED).child(
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
                    true,
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

/// A test run's elbow row: `└ ok · 38 passed`, `└ failed · running 38
/// tests` — the lead word alone in its state ink.
fn test_elbow_row(block: BlockId, elbow: &TestElbow, selection: &TextRuns, grid: Grid) -> Div {
    let text = elbow.line();
    elbow_line(grid, TEXT_MUTED).child(
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
        let block = output_block(block, name, text, TEXT_MUTED, false, first, selection, grid);
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
    if let Some(structured) = tool.structured_output() {
        details = part(details, "details", &structured.text);
        if structured.omitted_bytes > 0 {
            details = details.child(omitted_line(structured.omitted_bytes, grid));
        }
    }
    details
}

/// A result's verdict words in their state ink, the rest in the line's
/// `TEXT_MUTED` (CT-15): a line's lead `ok` (`RUNNING`) or `failed`
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
                    verdict_ink(word).unwrap_or(TEXT_MUTED),
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
                        verdict_ink(word).unwrap_or(TEXT_MUTED),
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
                    .text_color(rgb(TEXT_MUTED))
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
    /// Keyboard cycling has landed on this row: it wears the `paint::HOVER`
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
            row.bg(theme::paint::HOVER)
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
/// pointer is on the row (lifting to `TEXT_MUTED`) or while the keyboard
/// targets the row, and it turns a quarter when open. The turn eases over
/// `motion::CHEVRON` only when the pointer flipped it (`eased`); a keyboard
/// toggle, reduced motion and a row drawn already open are simply turned.
pub fn disclosure_chevron(expanded: bool, targeted: bool, eased: bool) -> AnyElement {
    let chevron = |turn: f32| {
        icon(icons::CHEVRON_RIGHT, theme::ICON_CHEVRON, TEXT_FAINT)
            .group_hover(DISCLOSURE_ROW, |style| style.text_color(rgb(TEXT_MUTED)))
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
/// `plain` draws the changed tokens in the row's own ink, their syntax
/// colours dropped (only a stated mark asks for it, `WordMark`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DiffSide {
    pub kind: DiffKind,
    pub number: u32,
    pub body: String,
    pub words: Option<std::ops::Range<usize>>,
    pub plain: bool,
}

/// A changed-token span stated for one diff line instead of the one
/// `changed_words` derives: the line by its file (a path suffix), its kind
/// and its number, the span by its text (the first occurrence in the line).
///
/// Only the prototype's world states marks (the parity capture and
/// `--demo parity`). The approved prototype marks its
/// diffs' changed tokens by hand, and no rule derives every one of them
/// (it marks one pure insertion but not the next, and wraps a call's
/// argument where the token diff sees the call), so its world states the
/// prototype's own spans for the lines it shows; every other diff, and
/// every other line, is marked by `changed_words`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WordMark {
    pub path: &'static str,
    pub added: bool,
    pub number: u32,
    pub text: &'static str,
    pub plain: bool,
}

thread_local! {
    /// The marks the prototype's world states (`set_word_marks`).
    static WORD_MARKS: std::cell::RefCell<Vec<WordMark>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// State `marks` for the diffs drawn from here on (the prototype's world,
/// captured or live; see `WordMark`).
pub(crate) fn set_word_marks(marks: Vec<WordMark>) {
    WORD_MARKS.with(|stated| *stated.borrow_mut() = marks);
}

/// `side` of the diff of `path` with the span stated for it, if any.
fn stated_mark(path: &str, side: &mut DiffSide) {
    WORD_MARKS.with(|stated| {
        let stated = stated.borrow();
        let found = stated.iter().find(|mark| {
            path.ends_with(mark.path)
                && mark.number == side.number
                && mark.added == (side.kind == DiffKind::Added)
                && side.kind != DiffKind::Context
        });
        if let Some(mark) = found {
            if let Some(at) = side.body.find(mark.text) {
                side.words = Some(at..at + mark.text.len());
                side.plain = mark.plain;
            }
        }
    });
}

/// A diff as rows (the prototype's `.diff`). `Unified` rows carry both line
/// numbers; `Split` pairs a removed line with the added line it became, a
/// side with no partner left empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DiffRow {
    /// `@@ -208,7 +208,11 @@ fn thread_row`: drawn only where the hunk
    /// names a section.
    Hunk(String),
    Unified {
        old: Option<u32>,
        new: Option<u32>,
        side: DiffSide,
    },
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
            Self::Unified { side, .. } => vec![side],
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
/// The rows a diff under a call draws: split, every row; unified (a
/// narrow Pane), its changes alone under their hunk heads — the prototype's
/// narrow `91 -` `91 +` `92 +` with no context between.
pub(super) fn preview_rows(diff: &Diff, cap: usize, split: bool) -> Vec<DiffRow> {
    let rows = diff_rows(diff, cap, split);
    if split {
        return rows;
    }
    rows.into_iter()
        .filter(
            |row| !matches!(row, DiffRow::Unified { side, .. } if side.kind == DiffKind::Context),
        )
        .collect()
}

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
                        plain: false,
                    };
                    rows.push(if split {
                        DiffRow::Split {
                            left: Some(side(old)),
                            right: Some(side(new)),
                        }
                    } else {
                        DiffRow::Unified {
                            old: Some(old),
                            new: Some(new),
                            side: side(new),
                        }
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
                            plain: false,
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
                            plain: false,
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
                    for side in left.iter_mut().chain(right.iter_mut()) {
                        stated_mark(&diff.path, side);
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
                        for side in left {
                            rows.push(DiffRow::Unified {
                                old: Some(side.number),
                                new: None,
                                side,
                            });
                        }
                        for side in right {
                            rows.push(DiffRow::Unified {
                                old: None,
                                new: Some(side.number),
                                side,
                            });
                        }
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
        DiffKind::Added => Some(theme::DIFF_ADDED_WORD),
        DiffKind::Removed => Some(theme::DIFF_REMOVED_WORD),
        DiffKind::Context => None,
    };
    let words = side
        .words
        .clone()
        .filter(|words| !words.is_empty())
        .zip(wash)
        .map(|(words, wash)| (words, rgba(wash).into()));
    let syntax = match (&words, side.plain) {
        // The changed tokens in the row's own ink: the syntax runs stop at
        // the span and pick up after it.
        (Some((words, _)), true) => syntax
            .into_iter()
            .flat_map(|(range, style)| {
                [
                    range.start..range.end.min(words.start),
                    range.start.max(words.end)..range.end,
                ]
                .into_iter()
                .filter(|part| !part.is_empty())
                .map(move |part| (part, style))
            })
            .collect(),
        _ => syntax,
    };
    (syntax, words)
}

/// A diff under its call (the prototype's `.diff`, clean): no box, half a
/// line under the call, on the content column (`inset`; the compare reader
/// draws it flush). A `TEXT_MUTED` hunk header opens a hunk that names a
/// section (none otherwise); each row is `[number][sign][code]` (unified
/// carries the old and the new number), the numbers `TEXT_MUTED`
/// right-aligned in `DIFF_NUMBER_CELLS`, the sign in its hue, the code in
/// full syntax colour with its indentation intact — exactly one line tall,
/// cut with `…` at the column's edge. Removed and added rows wear their
/// washes, the changed tokens a deeper one. `wide` lays it side by side, a
/// 1px `paint::LINE` between the halves and an empty side on
/// `paint::NODIFF`.
///
/// The code cells route through the overlay — their lines copy honestly and
/// whole, however they are cut; numbers, signs and headers are chrome and
/// never do (#27). Without a selection (`None`) nothing registers.
///
/// Under a call the diff draws at most `HUNK_MAX_ROWS` lines and then names
/// what it left out; the reader draws it all.
fn render_diff(
    selection: Option<(&TextRuns, BlockId)>,
    diff: &Diff,
    grid: Grid,
    wide: bool,
    inset: bool,
) -> impl IntoElement {
    let total: usize = diff.hunks.iter().map(|hunk| hunk.lines.len()).sum();
    let (cap, omitted) = if inset { hunk_rows(total) } else { (total, 0) };
    let cells = diff_number_cells(diff_max_number(diff, cap));
    let number_w = cells * grid.cell();
    let sign_w = theme::DIFF_SIGN_CELLS * grid.cell();
    let language = ferrite_core::transcript::language_for_path(std::path::Path::new(&diff.path));
    let number = |number: Option<u32>| {
        components::tabular(
            div()
                .flex_shrink_0()
                .w(px(number_w))
                .pr(px(grid.cell()))
                .text_right()
                .whitespace_nowrap()
                .text_color(rgb(TEXT_MUTED))
                .children(number.map(|number| SharedString::from(number.to_string()))),
        )
    };
    // The fixed columns before a row's code (its numbers and sign), as the
    // browser sizes them: the cut measures the code cell against these, not
    // the device-snapped widths gpui lays out (`CellCut::lead`).
    let lead = |numbers: usize| [number_w, if numbers > 1 { number_w } else { 0. }, sign_w];
    let cell = |side: &DiffSide, selectable: bool, numbers: usize| {
        let DiffPaint {
            sign,
            sign_color,
            code_color,
            wash,
        } = side.kind.paint();
        let (highlights, words) = diff_highlights(side, language);
        let cut = CellCut::new(
            side.body.clone(),
            words,
            rgb(code_color).into(),
            grid.cell(),
            lead(numbers),
        );
        let code = match selection {
            Some((selection, block)) if selectable => selection
                .line(block, side.body.clone(), highlights)
                .into_any_element(),
            _ => StyledText::new(SharedString::from(side.body.clone()))
                .with_highlights(highlights)
                .into_any_element(),
        };
        (
            div()
                .flex_shrink_0()
                .w(px(sign_w))
                .pl(px(grid.cell() / 2.))
                .whitespace_nowrap()
                .text_color(rgb(sign_color))
                .child(sign),
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_color(rgb(code_color))
                .child(cut.child(code)),
            wash,
        )
    };
    // `right`: the split's right half, whose text keeps the browser's
    // fraction of a pixel while its wash stays on the half's box
    // (`SplitRow`).
    let side_view = |side: Option<&DiffSide>, selectable: bool, right: bool| -> Div {
        match side {
            Some(side) => {
                let (sign, code, wash) = cell(side, selectable, 1);
                let content = div()
                    .flex()
                    .items_start()
                    .w_full()
                    .min_w_0()
                    .child(number(Some(side.number)))
                    .child(sign)
                    .child(code);
                div()
                    .flex()
                    .items_start()
                    .flex_1()
                    .min_w_0()
                    .h(px(grid.line))
                    .overflow_hidden()
                    .when_some(wash, |row, wash| row.bg(rgba(wash)))
                    .map(|row| {
                        if right {
                            row.child(SplitRow::right(content))
                        } else {
                            row.child(content)
                        }
                    })
            }
            None => div()
                .flex_1()
                .min_w_0()
                .h(px(grid.line))
                .bg(theme::paint::NODIFF),
        }
    };
    let mut lines = div()
        .debug_selector(|| "tool-diff".into())
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .text_color(rgb(TEXT));
    let rows = if inset {
        preview_rows(diff, cap, wide)
    } else {
        diff_rows(diff, cap, wide)
    };
    for row in rows {
        lines = lines.child(match &row {
            DiffRow::Hunk(header) => div()
                .debug_selector(|| "diff-hunk".into())
                .w_full()
                .min_w_0()
                .h(px(grid.line))
                .truncate()
                .text_color(rgb(TEXT_MUTED))
                .child(SharedString::from(header.clone())),
            DiffRow::Unified { old, new, side } => {
                let (sign, code, wash) = cell(side, true, 2);
                div()
                    .flex()
                    .items_start()
                    .w_full()
                    .min_w_0()
                    .h(px(grid.line))
                    .overflow_hidden()
                    .when_some(wash, |row, wash| row.bg(rgba(wash)))
                    .child(number(*old))
                    .child(number(*new))
                    .child(sign)
                    .child(code)
            }
            DiffRow::Split { left, right } => {
                let selectable = row.selectable();
                let pick = |side: &Option<DiffSide>| {
                    side.as_ref().is_some_and(|side| {
                        selectable.iter().any(|picked| std::ptr::eq(*picked, side))
                    })
                };
                div().w_full().min_w_0().child(SplitRow::new(
                    div()
                        .flex()
                        .items_stretch()
                        .w_full()
                        .min_w_0()
                        .child(side_view(left.as_ref(), pick(left), false))
                        .child(div().flex_shrink_0().w(px(1.)).bg(theme::paint::LINE))
                        .child(side_view(right.as_ref(), pick(right), true)),
                ))
            }
        });
    }
    // What the cap left out, on the code column — never a silent truncation.
    if omitted > 0 {
        lines = lines.child(
            div()
                .w_full()
                .min_w_0()
                .truncate()
                .text_color(rgb(TEXT_MUTED))
                .child(SharedString::from(format!("\u{2026} +{omitted} lines"))),
        );
    }
    if inset {
        // The prototype's `.diff` keeps its 1px border, clear: the rows sit
        // a pixel in from every side.
        content_row(grid).pt(px(grid.half())).child(
            div()
                .w_full()
                .min_w_0()
                .border_1()
                .border_color(gpui::transparent_black())
                .child(lines),
        )
    } else {
        div().w_full().min_w_0().child(lines)
    }
}

thread_local! {
    /// The split rows being prepainted, innermost last (`SplitRow`).
    static SPLIT_ROWS: std::cell::RefCell<Vec<gpui::Bounds<gpui::Pixels>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// A split diff row's halves where the browser sets their text. The
/// prototype's `minmax(0,1fr) 1px minmax(0,1fr)` puts the right half at a
/// fraction of a pixel (865.3px in Solo); its box rounds to the pixel but
/// its glyphs keep the fraction. gpui lays the half on the device pixel
/// (865), so the right half's text stood a device pixel left. The row
/// records its bounds (`SplitRow::new`); the right half's content
/// (`SplitRow::right`) paints from the device pixel nearest the browser's
/// track start, its wash staying on the half's own box.
enum SplitRow {
    Row(Option<AnyElement>),
    Right(Option<AnyElement>),
}

impl SplitRow {
    fn new(child: impl IntoElement) -> Self {
        Self::Row(Some(child.into_any_element()))
    }

    fn right(child: impl IntoElement) -> Self {
        Self::Right(Some(child.into_any_element()))
    }

    fn child(&mut self) -> &mut Option<AnyElement> {
        match self {
            Self::Row(child) | Self::Right(child) => child,
        }
    }
}

impl IntoElement for SplitRow {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl gpui::Element for SplitRow {
    type RequestLayoutState = ();
    type PrepaintState = ();

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
    ) -> (gpui::LayoutId, ()) {
        let child = self.child().get_or_insert_with(|| div().into_any_element());
        (child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<gpui::Pixels>,
        _: &mut (),
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        match self {
            Self::Row(child) => {
                SPLIT_ROWS.with(|rows| rows.borrow_mut().push(bounds));
                if let Some(child) = child.as_mut() {
                    child.prepaint(window, cx);
                }
                SPLIT_ROWS.with(|rows| rows.borrow_mut().pop());
            }
            Self::Right(child) => {
                let track = SPLIT_ROWS.with(|rows| rows.borrow().last().copied());
                // gpui moves an element only by whole device pixels: the
                // nearest one at or past the browser's track.
                let scale = window.scale_factor();
                let shift = track.map_or(px(0.), |row| {
                    let track = f32::from(row.left() + (row.size.width - px(1.)) / 2. + px(1.));
                    px(((track * scale) + 0.5).floor() / scale) - bounds.left()
                });
                if let Some(child) = child.as_mut() {
                    window.with_element_offset(gpui::point(shift, px(0.)), |window| {
                        child.prepaint(window, cx)
                    });
                }
            }
        }
    }

    fn paint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<gpui::Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        if let Some(child) = self.child().as_mut() {
            child.paint(window, cx);
        }
    }
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
    /// The widths of the fixed columns laid before this cell in its row, as
    /// authored. gpui rounds each to the device pixel before layout (a
    /// 2-cell sign column is 15.5px, the browser's 15.6px), handing their
    /// rounding to this cell; the cut gives it back, so a run that
    /// overflows the browser's cell by a tenth of a pixel is cut here too.
    lead: [f32; 3],
    child: Option<AnyElement>,
}

impl CellCut {
    fn new(
        text: String,
        words: Option<WordWash>,
        ink: gpui::Hsla,
        cell: f32,
        lead: [f32; 3],
    ) -> Self {
        Self {
            text,
            words,
            ink,
            cell,
            lead,
            child: None,
        }
    }

    /// What the device-pixel rounding of the `lead` columns added to this
    /// cell's width at `scale` (negative when it took some away).
    fn slack(&self, scale: f32) -> f32 {
        self.lead
            .iter()
            .map(|width| width - (width * scale).round() / scale)
            .sum()
    }

    /// A run of one face cut with `…` in `ink` (its line's own colour, as
    /// the browser draws an ellipsis whatever the run it hides): no wash, no
    /// columns before it.
    pub(crate) fn plain(text: String, ink: gpui::Hsla, cell: f32) -> Self {
        Self::new(text, None, ink, cell, [0.; 3])
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
    type RequestLayoutState = ();
    type PrepaintState = ();

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
    ) -> (gpui::LayoutId, ()) {
        let child = self.child.get_or_insert_with(|| div().into_any_element());
        (child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<gpui::Pixels>,
        _: &mut (),
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        if let Some(child) = self.child.as_mut() {
            child.prepaint(window, cx);
        }
    }

    fn paint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<gpui::Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        // On the device pixel: the clip, the wash and the `…` meet without
        // a column two of them would otherwise both paint.
        let scale = window.scale_factor();
        let kept = self.kept(f32::from(bounds.size.width) - self.slack(scale));
        let snap = |x: f32| px((x * scale).round() / scale);
        let left = f32::from(bounds.left());
        let cut = kept.map(|kept| snap(left + kept as f32 * self.cell));
        if let (Some((from, to, runs_on)), Some((_, wash))) =
            (self.wash_cells(kept), self.words.as_ref())
        {
            let (top, height) = inline_content_box(window);
            let x0 = snap(left + from as f32 * self.cell);
            let x1 = match (runs_on, cut) {
                (true, _) => bounds.right(),
                (false, Some(cut)) => snap(left + to as f32 * self.cell).min(cut),
                (false, None) => snap(left + to as f32 * self.cell),
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

/// A change set as the compare reader shows it (frame's `Beside::Compare`):
/// each file's head — its path in `TEXT_STRONG` at `W_LABEL`, `+N −M` at its
/// right — then its whole diff, split when `wide`, on the transcript's grid
/// at the reading size. Nothing in it registers with a selection.
pub(crate) fn diff_document(
    edits: &[ferrite_core::FileEdit],
    reading: ferrite_core::settings::ReadingSize,
    wide: bool,
) -> AnyElement {
    let grid = Grid::of(reading);
    let mut column = div()
        .debug_selector(|| "diff-document".into())
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .font_family(theme::FONT_CODE)
        .text_size(px(grid.size))
        .line_height(px(grid.line))
        .text_color(rgb(TEXT));
    for edit in edits {
        let lines = || edit.hunks.iter().flat_map(|hunk| hunk.lines.iter());
        let diff = Diff {
            path: edit.path.clone(),
            hunks: edit.hunks.clone(),
            added: lines().filter(|line| line.starts_with('+')).count(),
            removed: lines().filter(|line| line.starts_with('-')).count(),
        };
        column = column
            .child(
                div()
                    .flex()
                    .items_center()
                    .w_full()
                    .min_w_0()
                    .h(px(grid.line))
                    .mt(px(grid.half()))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(theme::W_LABEL)
                            .text_color(rgb(TEXT_STRONG))
                            .child(SharedString::from(diff.path.clone())),
                    )
                    .child(components::tabular(
                        div()
                            .flex_shrink_0()
                            .pl(px(2.0 * grid.cell()))
                            .child(trail_diff_stat(diff.added, diff.removed)),
                    )),
            )
            .child(render_diff(None, &diff, grid, wide, false));
    }
    column.into_any_element()
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

/// The number column in cells: `DIFF_NUMBER_CELLS` (four digits and the
/// cell after them), wider only for a number past four digits.
fn diff_number_cells(max: usize) -> f32 {
    let digits = max.max(1).ilog10() as f32 + 1.;
    theme::DIFF_NUMBER_CELLS.max(digits + 1.)
}

/// What a unified-diff line is, read from its first byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DiffKind {
    Added,
    Removed,
    Context,
}

/// A diff row's colours: the sign column, the code, and the row's wash.
#[derive(Debug, PartialEq, Eq)]
struct DiffPaint {
    sign: &'static str,
    sign_color: u32,
    code_color: u32,
    wash: Option<u32>,
}

impl DiffKind {
    fn of(line: &str) -> Self {
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
    fn paint(self) -> DiffPaint {
        match self {
            Self::Added => DiffPaint {
                sign: "+",
                sign_color: theme::DIFF_ADDED_SIGN,
                code_color: DIFF_ADDED_INK,
                wash: Some(DIFF_ADDED_WASH),
            },
            Self::Removed => DiffPaint {
                sign: "-",
                sign_color: theme::DIFF_REMOVED_SIGN,
                code_color: DIFF_REMOVED_INK,
                wash: Some(DIFF_REMOVED_WASH),
            },
            Self::Context => DiffPaint {
                sign: "",
                sign_color: TEXT_MUTED,
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
        // Inline code is cyan ink with no chip (rule 6).
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
        // A link or a path reads cyan, like every path in the transcript,
        // with no underline at rest. Inert — paths render, nothing opens.
        Style::Link => Some(HighlightStyle {
            color: Some(rgb(theme::PATH_INK).into()),
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
    highlights
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::transcript::Class;
    #[test]
    fn progress_token_counts_stay_compact() {
        assert_eq!(tokens_label(340), "340");
        assert_eq!(tokens_label(8_040), "8.0k");
        assert_eq!(tokens_label(12_400), "12k");
    }
    /// The placeholder's pieces after its head (terminal-native, WP-D): at
    /// rest the `/` menu and the drop hint; a prediction's accept key (the
    /// one thing about it the line cannot show). A Decision's line carries
    /// none.
    #[test]
    fn the_placeholder_carries_the_one_key_hint() {
        let live = Transcript::default();
        let hint = |ghost: Ghost| ghost.more.first().map(|piece| piece.to_string());
        assert_eq!(
            hint(placeholder(false, false, Some(&live), None)).as_deref(),
            Some("/ for commands")
        );
        assert_eq!(hint(placeholder(true, false, Some(&live), None)), None);
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
            hint(placeholder(
                false,
                false,
                Some(&answered),
                Some("Run the tests")
            ))
            .as_deref(),
            Some("\u{21e5} accept")
        );
        assert_eq!(
            hint(placeholder(false, true, None, Some("Run the tests"))).as_deref(),
            Some("/ for commands"),
            "a draft has no conversation to predict from"
        );
        // The accept key is never cut; the menu pointer may drop out whole.
        assert!(placeholder(false, false, Some(&answered), Some("Run the tests")).keeps_hint());
        assert!(!placeholder(false, false, Some(&live), None).keeps_hint());
        assert_eq!(
            placeholder(false, false, Some(&answered), Some("Run the tests")).rungs(),
            ["Run the tests \u{b7} \u{21e5} accept"]
        );
    }
    use ferrite_core::transcript::{Input, Lexer, Todos};
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
                px(theme::LH_UI),
                "one line, and no command detail below it"
            );
            assert!(metadata.right() <= px(width), "the caption truncates first");
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
        transcript.apply(Input::Revived);
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
                preview: crate::attachment_preview::Preview::new(cx),
                expanded,
                target: None,
                disclosure_focus: cx.focus_handle(),
                tail: None,
                banner: None,
                pending_call: None,
                workspace: None,
                solo: false,
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
                    preview: crate::attachment_preview::Preview::new(cx),
                    expanded: HashSet::new(),
                    target: None,
                    disclosure_focus: cx.focus_handle(),
                    tail: None,
                    banner: None,
                    pending_call: None,
                    workspace: None,
                    solo: false,
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
                            decision::prose(decision_subject(decision)).into_any_element(),
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
            "meta",
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
        assert_eq!(wall_state(Some(&transcript), false, false), WallState::Done);
    }

    /// glance.md §4's wall matrix, one assertion per row — the selection
    /// logic the wall cell renders from.
    #[test]
    fn the_wall_state_matrix_reads_exactly_as_the_glance_spec() {
        use WallState::*;
        let mut transcript = Transcript::default();
        assert_eq!(wall_state(Some(&transcript), false, false), Idle);
        transcript.apply(Input::Prompt("go".into()));
        // Working, focused or not, is the streaming Thread.
        assert_eq!(wall_state(Some(&transcript), false, false), Working);
        // Failing tests stay a working Thread — red text, not a ring.
        assert_eq!(wall_state(Some(&transcript), true, false), Decision);
        assert_eq!(wall_state(Some(&transcript), false, true), Failing);
        // A Decision waits: pending flag or Blocked status, either way.
        let decision = crate::demo::script()
            .into_iter()
            .map(|step| step.event)
            .find(|event| matches!(event, SessionEvent::DecisionRequested { .. }))
            .unwrap();
        transcript.apply(Input::Event(decision));
        assert_eq!(wall_state(Some(&transcript), false, false), Decision);
        // Only successful outcomes read Done, regardless of cost.
        for cost_usd in [None, Some(0.038)] {
            for (outcome, expected) in [
                (ferrite_core::TurnOutcome::Completed, Done),
                (ferrite_core::TurnOutcome::Interrupted, Idle),
                (ferrite_core::TurnOutcome::Error("failed".into()), Idle),
            ] {
                transcript.apply(Input::Event(SessionEvent::TurnEnded { outcome, cost_usd }));
                assert_eq!(wall_state(Some(&transcript), false, false), expected);
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
        assert_eq!(wall_state(Some(&transcript), false, false), Blocked);
        // No transcript at all — the cockpit could not open the Thread.
        assert_eq!(wall_state(None, false, false), Parked);
    }

    /// The wall card folds everything the L3 recipe needs that is not an
    /// O(1) read — built on change, never per frame.
    #[test]
    fn the_wall_card_folds_progress_result_and_context_lines() {
        // No transcript: an empty card.
        let empty = wall_card(None, None);
        assert!(!empty.tests_failing);
        assert!(empty.lines.is_empty());

        let mut transcript = Transcript::default();
        for (id, subject) in [("1", "a"), ("2", "b"), ("3", "c"), ("4", "d")] {
            transcript.apply(Input::Event(SessionEvent::Progress {
                event: ferrite_core::progress::ProgressEvent::Task {
                    id: id.into(),
                    subject: subject.into(),
                    status: Some(ferrite_core::progress::StepStatus::Pending),
                    deleted: false,
                },
            }));
        }
        for task in ["1", "2", "3"] {
            transcript.apply(Input::Event(SessionEvent::Progress {
                event: ferrite_core::progress::ProgressEvent::Task {
                    id: task.into(),
                    subject: String::new(),
                    status: Some(ferrite_core::progress::StepStatus::Completed),
                    deleted: false,
                },
            }));
        }
        assert_eq!(transcript.todos(), Some(Todos { done: 3, total: 4 }));
        let card = wall_card(Some(&transcript), None);
        assert_eq!(card.working.as_ref(), "Working");

        // A red suite flips the folded flag and folds the failing line —
        // with the run's own count when its output reported one.
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "test1".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "cargo test" }),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "test1".into(),
            output: "test result: FAILED. 357 passed; 2 failed".into(),
            is_error: true,
            result: ToolResult::Opaque,
        }));
        let red = wall_card(Some(&transcript), None);
        assert!(red.tests_failing);
        assert_eq!(red.failing_count, Some(2));
        assert_eq!(HeadSlot::Failing(red.failing_count).text(), "failing 2");

        // A Decision's subject becomes the alert's second line, wearing the
        // tool prefix every Decision surface shares (#22 C7).
        let decision = Decision {
            delivery: Default::default(),
            kind: Default::default(),
            policy: Default::default(),
            id: "perm".into(),
            tool_use_id: "toolu".into(),
            tool_name: "Bash".into(),
            description: "gh issue close 212".into(),
            input: serde_json::Value::Null,
            suggestions: vec![],
        };
        assert_eq!(
            wall_card(Some(&transcript), Some(&decision))
                .context
                .as_ref(),
            "Bash \u{b7} gh issue close 212"
        );

        // A closed Session's reason is promoted into the alert line itself
        // (#22 C14).
        let mut closed = Transcript::default();
        closed.apply(Input::Event(SessionEvent::Closed {
            reason: "claude CLI exited with code 1".into(),
        }));
        assert_eq!(
            wall_card(Some(&closed), None).context.as_ref(),
            "claude CLI exited with code 1"
        );
    }

    /// F-6: a tile's lines are one dim run each, glyph included — a call's
    /// trail inline after one space, a red run's result under it, a passed
    /// run's `ok`, and a pending approval's command in place of the call it
    /// gates.
    #[test]
    fn the_wall_card_prints_calls_trails_results_and_the_decision() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("fix it".into()));
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "t1".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "cargo test --workspace" }),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "t1".into(),
            output: "test result: FAILED. 357 passed; 2 failed; 0 ignored".into(),
            is_error: true,
            result: ToolResult::Opaque,
        }));
        let timings: HashMap<String, ToolTiming> =
            [("t1".to_string(), ToolTiming::Done(Duration::from_secs(61)))]
                .into_iter()
                .collect();
        let card = wall_card_timed(Some(&transcript), None, Some(&timings));
        let texts: Vec<&str> = card.lines.iter().map(|line| line.text.as_ref()).collect();
        assert_eq!(
            texts,
            [
                "\u{25cf} Bash(cargo test --workspace) 1m01s",
                "  \u{2514} 357 passed; 2 failed"
            ],
            "the prompt leaves no line"
        );
        assert!(card.lines.iter().all(|line| !line.wraps));

        // A passed run says `ok` in its trail and hangs nothing.
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "t2".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "cargo test -p ferrite theme::" }),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "t2".into(),
            output: "test result: ok. 24 passed; 0 failed".into(),
            is_error: false,
            result: ToolResult::Opaque,
        }));
        let card = wall_card(Some(&transcript), None);
        assert_eq!(
            card.lines.last().unwrap().text.as_ref(),
            "\u{25cf} Bash(cargo test -p ferrite theme::) ok"
        );

        // Prose is a line that may wrap; the call a Decision gates gives
        // way to what the Decision would run.
        transcript.apply(Input::Event(SessionEvent::TextDelta {
            text: "Six of seven stale issues closed\n\n".into(),
        }));
        let command = "gh issue close 212 --reason \"not planned\"";
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "t3".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": command }),
        }));
        let decision = Decision {
            delivery: Default::default(),
            kind: Default::default(),
            policy: Default::default(),
            id: "perm".into(),
            tool_use_id: "t3".into(),
            tool_name: "Bash".into(),
            description: String::new(),
            input: serde_json::json!({ "command": command }),
            suggestions: vec![],
        };
        let card = wall_card(Some(&transcript), Some(&decision));
        assert_eq!(
            wall_rows(&card.lines, 60),
            [
                "\u{25cf} Six of seven stale issues closed",
                "\u{25c6} Bash wants to run",
                "  $ gh issue close 212 --reason \"not planned\"",
            ]
        );
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
        assert_eq!(worked_label(Duration::from_secs(192)), "3m 12s");
        assert_eq!(worked_label(Duration::from_secs(41)), "41s");
    }

    /// One derivation for every surface that names a Decision (L1 card, L2
    /// cell, wall alert) — and the unreadable-request fallback holds even
    /// when the provider names no tool at all.
    #[test]
    fn every_decision_surface_shares_one_subject_derivation() {
        let decision = |tool: &str, description: &str| Decision {
            delivery: Default::default(),
            kind: Default::default(),
            policy: Default::default(),
            id: "perm".into(),
            tool_use_id: "toolu".into(),
            tool_name: tool.into(),
            description: description.into(),
            input: serde_json::Value::Null,
            suggestions: vec![],
        };
        let full = decision("Bash", "gh issue close 212");
        assert_eq!(
            decision_subject(&full).as_ref(),
            "Bash \u{b7} gh issue close 212"
        );
        assert!(!decision_subject(&full).contains(':'), "no colon label");
        // No description: the tool's name is the subject.
        let bare = decision("Write", "");
        assert_eq!(decision_subject(&bare).as_ref(), "Write");
        // No tool at all: the honest fallback, on both lines.
        let unreadable = decision("", "");
        assert_eq!(
            decision_subject(&unreadable).as_ref(),
            "unreadable permission request"
        );
        // The wall's alert context runs through the same derivation.
        let transcript = Transcript::default();
        assert_eq!(
            wall_card(Some(&transcript), Some(&unreadable))
                .context
                .as_ref(),
            "unreadable permission request"
        );
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
            permission_mode_label("opaque-mode", &choices).as_deref(),
            Some("ask for changes")
        );
        assert_eq!(
            permission_mode_label("unknown", &choices).as_deref(),
            Some("unknown")
        );
    }

    /// C17: a mode id never renders raw — the known ids read as words, the
    /// default is hidden, and nothing shown carries a capital or a hump.
    #[test]
    fn the_mode_word_is_never_a_raw_id() {
        let claude = vec![
            ferrite_core::PermissionModeChoice {
                value: "acceptEdits".into(),
                label: "Accept Edits".into(),
            },
            ferrite_core::PermissionModeChoice {
                value: "dontAsk".into(),
                label: "Don't Ask".into(),
            },
        ];
        assert_eq!(
            permission_mode_label("acceptEdits", &[]).as_deref(),
            Some("accept edits")
        );
        assert_eq!(
            permission_mode_label("acceptEdits", &claude).as_deref(),
            Some("accept edits")
        );
        assert_eq!(
            permission_mode_label("bypassPermissions", &[]).as_deref(),
            Some("bypass permissions")
        );
        assert_eq!(permission_mode_label("plan", &[]).as_deref(), Some("plan"));
        assert_eq!(permission_mode_label("default", &claude), None);
        assert_eq!(permission_mode_label("", &[]), None);
        assert_eq!(
            permission_mode_label("someNewMode", &[]).as_deref(),
            Some("some new mode")
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
                let word = permission_mode_label(id, choices).unwrap();
                assert_eq!(word.to_lowercase(), word.as_ref(), "{word}");
                if id.chars().any(char::is_uppercase) {
                    assert_ne!(word.as_ref(), id, "never the raw camelCase id");
                }
                assert!(!word.chars().any(char::is_uppercase), "{word}");
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
                code_color: DIFF_ADDED_INK,
                wash: Some(DIFF_ADDED_WASH),
            }
        );
        assert_eq!(
            DiffKind::Removed.paint(),
            DiffPaint {
                sign: "-",
                sign_color: theme::DIFF_REMOVED_SIGN,
                code_color: DIFF_REMOVED_INK,
                wash: Some(DIFF_REMOVED_WASH),
            }
        );
        assert_eq!(
            DiffKind::Context.paint(),
            DiffPaint {
                sign: "",
                sign_color: TEXT_MUTED,
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
        assert_eq!(link.color, Some(rgb(theme::PATH_INK).into()));
        assert_eq!(link.underline, None);
        assert!(span_style(Style::Plain).is_none());
    }

    #[test]
    fn a_tool_row_reads_its_outcome_from_its_bullet_and_keeps_its_name_neutral() {
        let failed = ToolState::Failed("boom".into());
        // Terminal-native: settled work is a dim `●`; live work a still
        // green one (only the working line moves); a failure the blocked
        // one; a lost result the hollow `○`. Green never means finished.
        assert_eq!(tool_dot_ink(&ToolState::Ok), (TEXT_MUTED, DotShape::Solid));
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
                structured_result: None,
                result_line: None,
                output: None,
                progress: None,
            };
            assert_eq!(text::tool_label(&tool), "Bash(cargo test)");
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
                    font_weight: Some(theme::W_LABEL),
                    ..Default::default()
                },
                "the name is the prototype's `.name`: strong ink at 500, whatever happened"
            );
        }
        // A path argument reads cyan, like every path in the transcript.
        let read = ToolBlock {
            call: "r".into(),
            name: "Read".into(),
            title: None,
            summary: "crates/ferrite/src/nav.rs".into(),
            state: ToolState::Ok,
            diffs: Vec::new(),
            structured_result: None,
            result_line: None,
            output: None,
            progress: None,
        };
        let highlights = call_highlights(&read);
        assert_eq!(highlights.len(), 2);
        assert_eq!(highlights[1].0, 5..30);
        assert_eq!(highlights[1].1.color, Some(rgb(theme::PATH_INK).into()));
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
            structured_result: None,
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
        assert_eq!(diff_number_cells(9_999), theme::DIFF_NUMBER_CELLS);
        assert_eq!(diff_number_cells(10_000), 6.);
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

    /// F-2: one attention edge. A waiting Pane draws `ATTENTION_EDGE` on a
    /// board whether it holds focus or not — no inner ring, no full-ink
    /// answer target, no red blocked edge; a calm focused Pane draws the
    /// accent; at rest the edge is transparent and the seams separate
    /// Panes. Solo never recolours the frame.
    #[test]
    fn pane_edge_ranks_state_over_focus() {
        assert_eq!(PaneEdge::of(true, true, false), PaneEdge::Attention);
        assert_eq!(PaneEdge::of(false, true, false), PaneEdge::Attention);
        assert_eq!(PaneEdge::of(true, false, false), PaneEdge::Focused);
        assert_eq!(PaneEdge::of(false, false, false), PaneEdge::Rest);
        assert_eq!(PaneEdge::Rest.ink(), rgba(TRANSPARENT).into());
        assert_eq!(PaneEdge::Focused.ink(), rgb(FOCUS_RING).into());
        assert_eq!(PaneEdge::Attention.ink(), rgba(ATTENTION_EDGE).into());
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
            assert_eq!(check_detail_ink(state), TEXT_MUTED);
        }
        // CI dots are never the ochre that means an agent waits on you.
        assert_eq!(check_ink(CheckState::Pending), RUNNING);
        assert_eq!(check_ink(CheckState::Passing), TEXT_MUTED);
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
                if let Some(word) = state_word(state, kind, &failing, None) {
                    said.push(word.text());
                }
            }
        }
        assert_eq!(
            state_word(WallState::Idle, None, &failing, None),
            None,
            "idle says nothing"
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

    /// What a tile's line says is one spelling per Body — a completed turn
    /// leaves no row, a docked Decision's notice keeps only its lead
    /// phrase, a call reads as the transcript spells it.
    #[test]
    fn the_tile_text_is_its_visible_text() {
        use ferrite_core::transcript::TurnEnd;
        assert_eq!(
            tail_text(&Body::Prompt("  Fix the board  ".into()), false).as_deref(),
            Some("Fix the board")
        );
        let notice = Body::Notice("asks 1 question \u{b7} Which layout?".into());
        assert_eq!(
            tail_text(&notice, false).as_deref(),
            Some("asks 1 question \u{b7} Which layout?")
        );
        assert_eq!(tail_text(&notice, true).as_deref(), Some("asks 1 question"));
        let done = Body::TurnEnd(TurnEnd {
            outcome: TurnOutcome::Completed,
            elapsed_ms: Some(18_000),
            completed_at: None,
            input_tokens: None,
            output_tokens: None,
        });
        assert_eq!(tail_text(&done, false), None, "no `Worked for` row");
        let stopped = TurnEnd {
            outcome: TurnOutcome::Interrupted,
            elapsed_ms: Some(4_100),
            completed_at: None,
            input_tokens: None,
            output_tokens: None,
        };
        assert_eq!(
            tail_text(&Body::TurnEnd(stopped.clone()), false),
            Some(stopped.text())
        );
        assert_eq!(tail_text(&Body::Prompt("   ".into()), false), None);
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
    /// Session — and, once a prediction lands, shows it verbatim as the
    /// operator's own next line. Its pieces drop out whole, from the right;
    /// none ends in an ellipsis, a terminal placeholder's own grammar.
    #[test]
    fn the_placeholder_says_what_the_pane_is_waiting_on() {
        let live = Transcript::default();
        assert_eq!(
            placeholder(false, false, Some(&live), None).rungs(),
            [
                "Steer this thread \u{b7} / for commands \u{b7} drop or paste images",
                "Steer this thread \u{b7} / for commands",
                "Steer this thread",
            ]
        );
        assert_eq!(
            placeholder(true, false, Some(&live), None).rungs(),
            ["answer above, or steer"]
        );
        let mut running = Transcript::default();
        running.apply(Input::Prompt("fix the decoder".into()));
        running.apply(Input::Event(SessionEvent::TextDelta {
            text: "Looking".into(),
        }));
        assert_eq!(
            placeholder(false, false, Some(&running), None).rungs(),
            [
                "queue a follow-up \u{b7} \u{23ce} sends when the turn ends",
                "queue a follow-up",
            ]
        );

        let mut closed = Transcript::default();
        closed.apply(Input::Event(SessionEvent::Closed {
            reason: "the CLI exited".into(),
        }));
        assert_eq!(
            placeholder(false, false, Some(&closed), None).rungs(),
            [
                "Revive and continue \u{b7} / for commands",
                "Revive and continue"
            ]
        );
        // A draft: what the first prompt does, then the menu pointer.
        assert_eq!(
            placeholder(false, true, None, None).rungs(),
            [
                "Start a thread \u{b7} / for commands \u{b7} drop or paste images",
                "Start a thread \u{b7} / for commands",
                "Start a thread",
            ]
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
            placeholder(true, false, Some(&answered), Some("Run the tests")).rungs(),
            ["answer above, or steer"]
        );
        assert_eq!(
            placeholder(false, false, Some(&closed), Some("Run the tests")).rungs(),
            [
                "Revive and continue \u{b7} / for commands",
                "Revive and continue"
            ]
        );

        // No rung names a message or ends in an ellipsis.
        for ghost in [
            placeholder(false, false, Some(&live), None),
            placeholder(true, false, Some(&live), None),
            placeholder(false, false, Some(&running), None),
            placeholder(false, false, Some(&closed), None),
            placeholder(false, true, None, None),
        ] {
            for rung in ghost.rungs() {
                assert!(!rung.contains("message"), "{rung}");
                assert!(!rung.ends_with('\u{2026}'), "{rung}");
            }
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
    /// editor rows (terminal-native, WP-D): the band's padding, the status
    /// line and its gap, and the air under the stack — at every level, since
    /// a quiet cell holds the status line's room.
    #[test]
    fn the_fixed_height_follows_the_composer_tokens() {
        assert_eq!(theme::COMPOSER_STATUS_H, theme::LH_UI);
        // A quiet board cell (FL-11): the band's padding, its two 1px rules
        // and 6px to the Pane's foot — 7px under the band.
        let quiet =
            2. * theme::COMPOSER_PAD_Y + 2. * theme::COMPOSER_RULE + theme::COMPOSER_STATUS_PAD_B;
        assert_eq!(composer_quiet_height(), quiet);
        assert_eq!(theme::COMPOSER_RULE + theme::COMPOSER_STATUS_PAD_B, 7.);
        // The live Composer adds the status line and its 2px gap: band,
        // rule, 2px, 20px, 6px.
        let status = theme::COMPOSER_STATUS_GAP + theme::COMPOSER_STATUS_H;
        assert_eq!(status, 22.);
        for grid in [false, true] {
            for compact in [false, true] {
                assert_eq!(composer_fixed_height(compact, grid), quiet + status);
            }
        }
        // One band row is the old grid line's 32px: 6 + 20 + 6.
        assert_eq!(theme::COMPOSER_GRID_H, 32.);
        assert_eq!(
            theme::COMPOSER_GRID_H,
            2. * theme::COMPOSER_PAD_Y + theme::COMPOSER_ROW_H
        );
        // The `❯` hangs in the transcript's 2-cell gutter, two cells in.
        assert_eq!(theme::COMPOSER_PAD_L, theme::GLYPH_GUTTER);
        assert_eq!(theme::COMPOSER_GUTTER, theme::GLYPH_GUTTER);
        // A tall Pane shows every editor row; a short one keeps its majority.
        for grid in [false, true] {
            assert_eq!(
                composer_row_limit(900., false, grid, 0),
                crate::composer::MAX_ROWS
            );
            let limit = composer_row_limit(300., false, grid, 0) as f32;
            assert!(
                composer_fixed_height(false, grid) + limit * theme::COMPOSER_ROW_H
                    <= 300. * theme::COMPOSER_MAX_PANE_FRACTION
            );
            // Queued rows stack at the input's own pitch with no gap between
            // them: three rows are exactly three lines.
            assert_eq!(theme::QUEUE_ROW_H, theme::COMPOSER_ROW_H);
            assert_eq!(
                composer_queue_height(900., false, grid, 3),
                3. * theme::COMPOSER_ROW_H
            );
            assert_eq!(
                composer_queue_height(900., true, grid, 3),
                theme::COMPOSER_COMPACT_QUEUE_ROWS as f32 * theme::COMPOSER_ROW_H
            );
        }
    }

    /// The ctx meter fills green, then yellow, then red (the prototype's
    /// status line); the model reads lowercase but for its context size.
    #[test]
    fn the_status_line_reads_like_a_terminal() {
        assert_eq!(ctx_meter_ink(0.32), RUNNING);
        assert_eq!(ctx_meter_ink(0.61), ATTENTION);
        assert_eq!(ctx_meter_ink(0.90), BLOCKED);
        assert_eq!(status_model_word("Opus 5.5 (1M)").as_ref(), "opus 5.5 (1M)");
        assert_eq!(status_model_word("GPT-6 Astra").as_ref(), "gpt-6 astra");
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
        // The status line's readout: muted, then attention at 80% as a
        // whole token, and never blocked.
        assert_eq!(theme::USAGE_TIGHT, 0.80);
        assert_eq!(readout_ink(0.79), TEXT_MUTED);
        assert_eq!(readout_ink(theme::USAGE_TIGHT), ATTENTION);
        assert_eq!(readout_ink(1.0), ATTENTION);
    }

    /// `ctx 32%` is text: one token per window, no reading invented where
    /// none was reported, and an account window only while it runs tight.
    #[test]
    fn the_usage_readout_is_one_run_per_window() {
        use ferrite_core::transcript::RateLimits;
        use ferrite_core::RateLimitWindow;
        let quiet = RateLimits::default();
        let tokens = |context, limits| {
            usage_tokens(context, limits)
                .into_iter()
                .map(|(token, _)| token)
                .collect::<Vec<_>>()
        };
        assert_eq!(tokens(Some(0.32), quiet), ["ctx 32%"]);
        assert!(tokens(None, quiet).is_empty(), "no `ctx —`");
        assert!(usage_meter_body(None, quiet).is_none());
        let tight = RateLimits {
            five_hour: Some(RateLimitWindow {
                used_fraction: 0.91,
                resets_at: None,
            }),
            weekly: Some(RateLimitWindow {
                used_fraction: 0.85,
                resets_at: None,
            }),
        };
        assert_eq!(tokens(Some(0.52), tight), ["ctx 52%", "5h 91%"]);
        assert_eq!(tokens(None, tight), ["5h 91%"]);
    }
    // (end WP-D)

    // ---- WP-E tests (append above the end line)
    // (end WP-E)

    // ---- WP-F tests (append above the end line)
    // (end WP-F)

    // ---- WP-G tests (append above the end line)
    // (end WP-G)
}
