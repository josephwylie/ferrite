//! One Pane: the visible cell for one Thread. Header, transcript, Composer,
//! and the three semantic-zoom renderings. Rendering only — everything it
//! shows is folded in core, and every key it answers to belongs to the
//! cockpit above it.
//!
//! L1 is the Soft prototype's Pane, drawn top to bottom: a 32px head, a
//! 24px tasks strip, the transcript body, the Decision card and the 58px
//! Composer — all on `--pane`, inside an
//! always-in-layout 1px border that only changes colour, with the focus
//! neutral focus ring inset on alert Panes so both signals remain visible.
//! Tools inherit JetBrains Mono; assistant prose uses the native UI face.
//! L2 (Instruments) and L3 (Wall) keep the metrics they have — the
//! prototype specifies only L1 — and take the new palette and scale.

mod text;
pub(crate) use text::{collect_activity_text, collect_block_text, collect_output_text};

use ferrite_core::activity::Subject;
use ferrite_core::cockpit::{ThreadView, ToolTiming};
use ferrite_core::docview::{is_test_run, passed_count, Instruments, Level, Tests};
use ferrite_core::followup::{self, Followup};
use ferrite_core::progress::Phase;
use ferrite_core::roster::{DraftId, PaneIdentity};
use ferrite_core::store::Provider;
use ferrite_core::transcript::{
    Block, BlockId, Body, Diff, Span, Status, Style, Todos, Token, ToolActivity, ToolBlock,
    ToolState, Transcript,
};
use ferrite_core::workspace::{
    BranchStatus, Check, CheckState, PrState, PullRequest, WorkspaceBinding,
};
use ferrite_core::{Decision, ThreadId};
use gpui::prelude::*;
use gpui::{
    canvas, deferred, div, point, px, relative, rgb, rgba, AnyElement, Context, Div, Entity,
    FocusHandle, HighlightStyle, SharedString, Stateful, Styled, StyledText,
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

/// Independent disclosure identities keep a group's first call separate from
/// its parent, and preserve choices while content streams or Subjects switch.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DisclosureId {
    Tool(String),
    Group(String),
    Reasoning(BlockId),
    TurnDiff(String),
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
    /// A completed singleton inherits its call's open state until the
    /// operator explicitly closes the group.
    collapsed_groups: HashSet<String>,
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
                collapsed_groups: HashSet::new(),
                target: None,
                focus: cx.focus_handle(),
                #[cfg(test)]
                bounds: Rc::new(RefCell::new(HashMap::new())),
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
                collapsed_groups: HashSet::new(),
                target: None,
                focus: cx.focus_handle(),
                #[cfg(test)]
                bounds: Rc::new(RefCell::new(HashMap::new())),
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
                    collapsed_groups: HashSet::new(),
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
        if let DisclosureId::Group(group) = call {
            if self.tool_state(call) == DisclosureState::Expanded {
                self.disclosure.expanded.remove(call);
                self.disclosure.collapsed_groups.insert(group.clone());
            } else {
                self.disclosure.collapsed_groups.remove(group);
                self.disclosure.expanded.insert(call.clone());
            }
        } else if !self.disclosure.expanded.remove(call) {
            self.disclosure.expanded.insert(call.clone());
        }
        // Toggling never moves the keyboard target: Tab/Shift-Tab set it,
        // and a pointer click only opens or closes the row.
        self.disclosure_revision = self.disclosure_revision.wrapping_add(1);
    }

    pub(crate) fn tool_state(&self, call: impl Into<DisclosureId>) -> DisclosureState {
        let call = call.into();
        let inherited_open = match &call {
            DisclosureId::Group(group) => {
                !self.disclosure.collapsed_groups.contains(group)
                    && self
                        .disclosure
                        .expanded
                        .contains(&DisclosureId::Tool(group.clone()))
            }
            _ => false,
        };
        if self.disclosure.expanded.contains(&call) || inherited_open {
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
        let collapsed_groups = self.disclosure.collapsed_groups.clone();
        let target = self.disclosure.target.clone();
        self.disclosure.expanded.retain(|call| calls.contains(call));
        self.disclosure
            .collapsed_groups
            .retain(|group| calls.contains(&DisclosureId::Group(group.clone())));
        if self
            .disclosure
            .target
            .as_ref()
            .is_some_and(|call| !calls.contains(call))
        {
            self.disclosure.target = None;
        }
        if self.disclosure.expanded != expanded
            || self.disclosure.collapsed_groups != collapsed_groups
            || self.disclosure.target != target
        {
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
        let mut expanded = self.disclosure.expanded.clone();
        // The retained renderer receives a snapshot, so materialize inherited
        // singleton-group openness here. Its display revision changes for
        // every state transition above, including an explicit group close.
        for call in &self.disclosure.expanded {
            let DisclosureId::Tool(group) = call else {
                continue;
            };
            if !self.disclosure.collapsed_groups.contains(group) {
                expanded.insert(DisclosureId::Group(group.clone()));
            }
        }
        (
            expanded,
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
    /// The actual git checkout of the Thread's cwd (#29), cached by the
    /// cockpit and refreshed on turn end and the watchdog cadence — the L1
    /// header's binding slot. Display-only, never a control: the CWD moves
    /// only when the binding follows the agent (`workspace::follow`), and
    /// nothing here may look like a way to move it.
    pub branch: Option<SharedString>,
    /// What the header's second line says about that checkout (#29): its
    /// drift from the upstream, its dirt, and its PR and CI when `gh` can
    /// answer. Cached on the same cadence as `branch`; `None` draws the
    /// line away entirely rather than claiming a clean tree it has not
    /// read.
    pub checkout: Option<&'a BranchStatus>,
    /// Whether the Composer line is empty — what decides the idle
    /// placeholder, read where the cockpit has a `cx` to read it with.
    pub composer_empty: bool,
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
    /// This Thread's provider, only when it differs from the board's
    /// majority: the Group head names the odd one out and no other.
    pub provider_mark: Option<Provider>,
    /// The one waiting Thread the answer keys act on (C6): its cell alone
    /// wears the full `ATTENTION` edge and the inline `y n a` pairs.
    pub answer_target: bool,
    /// This Pane's docked Decision merges into its live Composer (one
    /// block, rule 2.8.1): the Composer drops its top edge and corners.
    pub decision_joined: bool,
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
    /// The pending Decision's keycaps, wired to the exact decide verbs the
    /// keys run (#26) — laid into the L1 card or the L2 body. None while
    /// nothing pends, and at the wall, which draws no keycaps.
    pub decide: Option<AnyElement>,
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
}

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

/// The wall cell's folded reading — rebuilt only when the Thread changed,
/// never per frame (the L3 budget: 24 cells × 60fps must not walk Blocks or
/// format strings). Words only: the cells draw their own marks.
#[derive(Default)]
pub struct WallCard {
    /// The latest test run failed (from `Instruments`, the one O(blocks)
    /// read the wall needs).
    pub tests_failing: bool,
    /// The failing run's own count, where it reported one: the head slot
    /// and the wall read `failing 2`, else `failing`.
    pub failing_count: Option<usize>,
    /// The plan as (done, total), for the painted meter; `None` without one.
    pub todos: Option<(usize, usize)>,
    /// The working signal's detail: the progress caption (`Thinking`,
    /// `Retrying · Server busy`), the `working 12s` slot's tooltip.
    pub working: SharedString,
    /// An alert cell's context: the Decision's subject, or the reason the
    /// Session closed — the `failed` slot's tooltip. Empty when neither
    /// applies.
    pub context: SharedString,
}

/// Fold one Thread's wall reading. The activity phrase stays a status word —
/// naming the running tool at L3 would put `Instruments::of` on every wall
/// cell every rebuild during streaming for a line nobody can read at
/// distance (sidebar-and-impl §4.2 #6 keeps names at L2).
pub fn wall_card(transcript: Option<&Transcript>, decision: Option<&Decision>) -> WallCard {
    let Some(transcript) = transcript else {
        return WallCard::default();
    };
    let todos = transcript
        .todos()
        .filter(|todos| todos.total > 0)
        .map(|todos| (todos.done.min(todos.total), todos.total));
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
    WallCard {
        tests_failing: matches!(tests, Some(Tests::Failed { .. })),
        failing_count,
        todos,
        working,
        context,
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
    pub menu: Option<AnyElement>,
    pub model_picker: Option<AnyElement>,
    pub usage_meter: Option<AnyElement>,
    pub session_controls: Option<AnyElement>,
    pub mode_picker: Option<AnyElement>,
    /// A Subagent's footer, drawn where the Composer would be.
    pub child_footer: Option<AnyElement>,
    /// `l1_dock` (and the L2 cell).
    pub decide: Option<AnyElement>,
    /// `l1_dock`: activity requests that were not docked in the body.
    pub activity_decisions: Option<AnyElement>,
    /// The docked Decision merges into this Pane's Composer (one block).
    pub decision_joined: bool,
}

pub fn render_pane(
    view: &PaneView,
    facts: PaneFacts<'_>,
    wiring: PaneWiring,
    level: Level,
) -> impl IntoElement {
    let PaneFacts {
        thread,
        branch,
        checkout,
        composer_empty,
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
        answer_target,
        decision_joined,
    } = facts;
    let empty = WallCard::default();
    let wall = wall.unwrap_or(&empty);
    let PaneWiring {
        transcript: retained_transcript,
        attachments,
        composer_actions,
        background,
        received_reasoning_visible,
        menu,
        model_picker,
        usage_meter,
        session_controls,
        mode_picker,
        decide,
        title,
        agents,
        activity_decisions,
        expand_question,
        question_measurement,
        child_footer,
    } = wiring;
    let has_activity_decisions = activity_decisions.is_some() || expand_question;
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
    // Attention and focus are two independent channels, and they no longer
    // nest (§D.1): the edge is the Pane's own 1px border — always in layout,
    // only ever recoloured, so nothing reflows when a Decision arrives — and
    // one colour by precedence (`PaneEdge`). A focused alert Pane also draws
    // the focus ring inset inside that edge (`pane_frame`).
    let pending = thread.map(|thread| thread.activity().pending_decisions());
    let attention_pending = pending.is_some_and(|pending| !pending.is_empty());
    let blocked = state == WallState::Blocked;
    let alert = attention_pending || blocked;
    // Focus is drawn only where it tells the operator something: a lone
    // Pane is plainly the one holding the keyboard, and rests on its
    // hairline like any other.
    let framed = focused && show_focus;
    let edge =
        PaneEdge::of(framed, attention_pending, blocked, !show_focus).answer_target(answer_target);
    let key = view.thread().map_or(0, ThreadId::get);
    let hover = HoverEdge::of(
        edge,
        show_focus,
        SharedString::from(format!("pane-edge-{key}")),
    );
    let shell = record_card(
        pane_shell(hover.ink(edge)).when(edge == PaneEdge::Focused, |shell| {
            shell.debug_selector(move || format!("pane-focus-edge-{key}"))
        }),
        view,
    );
    let frame = |shell: Div| pane_frame(shell, framed, alert, hover.clone());
    // The one head recipe for a Group at every tier (rule 2.4.6): what the
    // cell is, and one word for where it stands.
    let kind = (attention_pending || state == WallState::Decision).then(|| {
        pending
            .and_then(|pending| pending.first())
            .map(|request| request_kind(&request.decision))
            .or_else(|| decision.map(request_kind))
            .unwrap_or(theme::words::APPROVAL)
    });
    let slot = head_slot(SlotFacts {
        state,
        kind,
        card: wall,
        ci_failing: checkout
            .and_then(|status| status.pr.as_ref())
            .map_or(0, |pr| pr.tally().failing as usize),
        transcript,
        focused,
        mode: thread.and_then(|thread| {
            thread
                .permission_mode()
                .and_then(|mode| permission_mode_label(mode, &thread.permission_modes()))
        }),
    });
    // Solo (fullscreen included) has no head at any tier: the titlebar
    // carries the Thread (C2).
    let head = |title: Option<AnyElement>, slot: Option<HeadSlot>| {
        show_focus.then(|| {
            group_head(GroupHead {
                key,
                name: view.name.clone(),
                dot: Some(head_dot(view.is_main(), state, attention, status)),
                unread: attention && view.is_main(),
                reduce_motion,
                title,
                branch: head_branch(checkout, branch.as_ref(), workspace),
                provider: provider_mark,
                slot_detail: match slot {
                    Some(HeadSlot::Working(_)) => Some(wall.working.clone()),
                    Some(HeadSlot::Failed) => Some(wall.context.clone()),
                    _ => None,
                },
                slot,
                action: None,
                expand_question,
            })
        })
    };

    // Far enough away, a Pane is one signal: the head and one line, nothing
    // that stops reading at a glance.
    if level == Level::Wall {
        return frame(
            shell
                .children(head(title, None))
                .child(wall_cell(wall, state, kind, transcript))
                .children(drop_target.then(crate::prompt_drop::sheet)),
        );
    }

    // Requests occupy the space below this Thread's header and above its
    // actual Composer. Keeping the overlay in that flex slot makes it follow
    // multiline drafts and split resizing without escaping into other Panes.
    // (At L2 the cell hangs them itself.)
    let mut activity_decisions = activity_decisions;
    let docked_requests = if level == Level::Instruments {
        None
    } else {
        activity_decisions.take()
    };
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
        menu,
        model_picker,
        usage_meter,
        session_controls,
        mode_picker,
        child_footer,
        decide,
        activity_decisions,
        decision_joined,
    };

    if level == Level::Instruments {
        let composer = l2_composer(&mut cx);
        let decide = cx.decide.take();
        let activity_decisions = cx.activity_decisions.take();
        return frame(shell.children(head(title, slot)).child(l2_cell(
            view,
            transcript,
            decision,
            decide,
            composer,
            activity_decisions.filter(|_| !expand_question),
            expand_question,
            focused,
            reduce_motion,
            drop_target,
        )));
    }

    // Solo (fullscreen included) has no head: the titlebar carries the
    // Thread (C2), and the body starts at the card edge. A Group's L1 Pane
    // wears the one head. Subagent tabs keep a strip of their own either
    // way.
    let mut pane = shell.children(head(title, slot));
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
                    // own top edge — the card edge in Solo, the head rule on
                    // a board.
                    .overflow_hidden()
                    .when(child_request, |body| body.key_context("Decision"))
                    .child(
                        retained_transcript.expect("L1 transcript entity is wired by CockpitView"),
                    )
                    .children(question_measurement)
                    .when_some(docked_requests, |body, requests| {
                        body.child(deferred(requests_overlay(requests)))
                    }),
            );
            // The order is head · body · progress · dock · composer.
            pane = pane.children(l1_progress(&mut cx));
            pane = pane.children(l1_dock(&mut cx));
            // The drop sheet covers the Pane beneath the Composer, which
            // paints after it and so stays in view, edged in the accent.
            pane = pane.children(drop_target.then(crate::prompt_drop::sheet));
            pane = pane.children(l1_composer(&mut cx));
        }
        None => {
            pane = pane
                .child(parked_body())
                .children(drop_target.then(crate::prompt_drop::sheet));
        }
    }
    frame(pane)
}

// ---------------------------------------------------------- render_pane slots
// Each slot's body belongs to one package; its signature and `PaneCtx` are
// the integrator's.

/// WP-A · the L1 working line, while the transcript streams (or its
/// starting shape while a Session starts). It overlays
/// the bottom of the transcript body — the list's own bottom padding, which
/// is taller than the line — so a turn starting or stopping never resizes
/// the list viewport or moves what the operator is reading. It sits in the
/// reading column on the transcript rows' axis, over the Pane's ground.
fn l1_progress(cx: &mut PaneCtx) -> Option<AnyElement> {
    let transcript = cx.transcript?;
    let line = if transcript.status() == Status::Streaming {
        working_line(
            transcript,
            false,
            // Only the Pane holding the keyboard offers `esc to interrupt`:
            // the key acts nowhere else.
            cx.focused,
            cx.received_reasoning_visible,
            cx.reduce_motion,
        )
    } else if cx.starting {
        // A Session starting or being replaced, with nothing streaming yet:
        // the same line, saying so.
        starting_line(cx.reduce_motion)
    } else {
        return None;
    };
    Some(
        div()
            .relative()
            .w_full()
            .h(px(0.))
            .flex_shrink_0()
            .child(
                div()
                    .debug_selector(|| "transcript-progress".into())
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .px(px(theme::PANE_PAD_X))
                    .pb(px(theme::GAP_ROW))
                    .bg(rgb(PANE))
                    .child(components::reading_column(
                        div().px(px(theme::BOX_INSET_X)).child(line),
                    )),
            )
            .into_any_element(),
    )
}

/// The working line's shape while a Session starts: the Ferrite mark (still
/// under reduced motion) and `Starting`, in the working line's own row.
fn starting_line(reduce_motion: bool) -> Div {
    let mark = working_mark(reduce_motion);
    div()
        .debug_selector(|| "transcript-starting".into())
        .flex()
        .items_center()
        .w_full()
        .min_w_0()
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(TEXT_2))
        .child(components::gutter(mark, theme::LH_UI))
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
        return Some(footer);
    }
    let transcript = cx.transcript?;
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
                attachments: cx.attachments.take(),
                actions: cx.composer_actions.take(),
                background: cx.background.take(),
                menu: cx.menu.take(),
                mode: cx.permission_mode.as_deref(),
                mode_picker: cx.mode_picker.take(),
                model_picker: cx.model_picker.take(),
                usage_meter: cx.usage_meter.take(),
                session_controls: cx.session_controls.take(),
                setup_controls: None,
                draft_error: None,
                suggestion: cx.suggestion,
                focused: cx.focused,
                editing: cx.editing,
                drop_target: cx.drop_target,
                joined: cx.decision_joined,
            },
        )
        .into_any_element(),
    )
}

/// WP-D · the L2 cell's compact Composer, or a Subagent's footer. An L2 cell
/// keeps its Composer: the operator types into a small Pane as into a big
/// one. No menu, picker or band at this size; the keys still work.
fn l2_composer(cx: &mut PaneCtx) -> Option<Div> {
    let composer = cx
        .transcript
        .filter(|_| cx.view.is_main())
        .map(|transcript| {
            composer_region(
                cx.view,
                Some(transcript),
                ComposerStack {
                    compact: true,
                    grid: cx.grid,
                    decision: cx.decision,
                    queued: std::mem::take(&mut cx.queued),
                    queue_height: cx.queue_height,
                    empty: cx.composer_empty,
                    attachments: cx.attachments.take(),
                    actions: cx.composer_actions.take(),
                    background: cx.background.take(),
                    menu: None,
                    // On a board a non-default mode is the head slot's word;
                    // a Solo cell keeps its status line.
                    mode: cx.permission_mode.as_deref().filter(|_| !cx.grid),
                    mode_picker: None,
                    model_picker: None,
                    usage_meter: None,
                    session_controls: None,
                    setup_controls: None,
                    draft_error: None,
                    suggestion: cx.suggestion,
                    focused: cx.focused,
                    editing: cx.editing,
                    drop_target: cx.drop_target,
                    joined: cx.decision_joined,
                },
            )
        });
    composer.or_else(|| cx.child_footer.take().map(|footer| div().child(footer)))
}

/// The Pane box (§D.1): `--pane` ground, `R_PANE` corners, and a 1px border
/// that is **always in layout** — only ever recoloured — so a state change
/// reflows nothing. `overflow: hidden` clips the children to the radius. The
/// UI face is declared once here; code text inside a Pane (tool arguments
/// and output, diffs, code, the Composer's line) sets the code face where it
/// is drawn.
/// Records the card's bounds into the Pane's geometry each prepaint.
fn record_card(shell: Div, view: &PaneView) -> Div {
    let geometry = view.geometry.clone();
    // The canvas fills the padding box: add back the 1px edge.
    components::on_bounds(shell, move |bounds, _, _| {
        geometry.set(PaneGeometry {
            card: Some(bounds.dilate(px(1.))),
            ..geometry.get()
        })
    })
}

fn pane_shell(edge: gpui::Hsla) -> Div {
    div()
        .relative()
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .min_w_0()
        .bg(rgb(PANE))
        .border_1()
        .border_color(edge)
        .rounded(px(theme::R_PANE))
        .font_family(theme::FONT_UI)
        .overflow_hidden()
}

/// How a resting edge answers the pointer (rule 2.10.2). On a board the
/// hairline blends to `HAIRLINE_STRONG` over the one 150ms hover blend
/// (`motion::hover_blend`, keyed by the Pane) — a click lands here. A state
/// or focus edge never reacts, and in Solo nothing does: a lone Pane is
/// plainly the one the pointer is over.
#[derive(Clone)]
struct HoverEdge {
    /// The Pane's frame id, and the blend's key.
    key: SharedString,
    blend: bool,
}

impl HoverEdge {
    fn of(edge: PaneEdge, show_focus: bool, key: SharedString) -> Self {
        Self {
            key,
            blend: edge == PaneEdge::Rest && show_focus,
        }
    }

    fn ink(&self, edge: PaneEdge) -> gpui::Hsla {
        if self.blend {
            crate::motion::hover_blend(
                &self.key,
                rgba(HAIRLINE).into(),
                rgba(HAIRLINE_STRONG).into(),
            )
        } else {
            edge.ink()
        }
    }
}

/// What a Pane's 1px edge says, by precedence: a closed Session beats a
/// Decision, a Decision beats focus, and a Pane with none of them rests on
/// the hairline. On a board the state edges are alpha (`BLOCKED_EDGE`,
/// `ATTENTION_EDGE`) and only the one answer-target cell wears full
/// `ATTENTION` (C6). In Solo state never recolours the frame (rule 2.2.5):
/// the docked Decision carries it. One colour — focus on an alert Pane is
/// the inset ring `pane_frame` draws, never a second edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaneEdge {
    Blocked,
    Attention,
    /// The waiting cell `y`/`n`/`a` act on.
    AnswerTarget,
    Focused,
    Rest,
}

impl PaneEdge {
    pub(crate) fn of(focused: bool, attention: bool, blocked: bool, solo: bool) -> Self {
        // Operator question Q6 (flagged for confirmation): Solo has no state
        // edge. Reverting it is deleting this branch.
        if solo {
            return if focused {
                PaneEdge::Focused
            } else {
                PaneEdge::Rest
            };
        }
        if blocked {
            PaneEdge::Blocked
        } else if attention {
            PaneEdge::Attention
        } else if focused {
            PaneEdge::Focused
        } else {
            PaneEdge::Rest
        }
    }

    /// The answer target's waiting cell steps up to full ink.
    pub(crate) fn answer_target(self, target: bool) -> Self {
        match self {
            PaneEdge::Attention if target => PaneEdge::AnswerTarget,
            edge => edge,
        }
    }

    pub(crate) fn ink(self) -> gpui::Hsla {
        match self {
            PaneEdge::Blocked => rgba(BLOCKED_EDGE).into(),
            PaneEdge::Attention => rgba(ATTENTION_EDGE).into(),
            PaneEdge::AnswerTarget => rgb(ATTENTION).into(),
            PaneEdge::Focused => rgb(FOCUS_RING).into(),
            PaneEdge::Rest => rgba(HAIRLINE).into(),
        }
    }
}

/// The Pane inside its non-clipping frame: the edge already says focus on a
/// calm Pane, so the frame adds only what the edge cannot — the inset
/// `FOCUS_RING` on a focused alert Pane (2px inside the state edge, UI-21).
/// A ring painted inside the shell's `overflow_hidden()` would be clipped,
/// so it is an absolute sibling here. Unread is not a ring: it breathes on
/// the head dot (`group_head`). The frame carries the hover blend's
/// listener, so the edge knows when the pointer is over its Pane.
fn pane_frame(shell: Div, focused: bool, alert: bool, hover: HoverEdge) -> Stateful<Div> {
    let inset = theme::FOCUS_RING_W * 2.;
    let HoverEdge { key, blend } = hover;
    div()
        .id(gpui::ElementId::Name(key.clone()))
        .relative()
        .flex()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .when(blend, |frame| {
            frame.on_hover(crate::motion::hover_listener(key))
        })
        .child(shell)
        .children((focused && alert).then(|| {
            div()
                .absolute()
                .inset(px(inset))
                .rounded(px(theme::R_PANE - inset))
                .border(px(theme::FOCUS_RING_W))
                .border_color(rgb(FOCUS_RING))
        }))
}

/// This overlay's containing block is the Pane's remaining body slot, not
/// the window. Its complete card (heading, scrollable content and actions)
/// must fit here; it never covers the owning header or the Composer.
fn requests_overlay(requests: AnyElement) -> Div {
    div()
        .absolute()
        .inset_0()
        .flex()
        .flex_col()
        .justify_end()
        .min_h_0()
        .overflow_hidden()
        .child(requests)
}

// ------------------------------------------------------------------ L3 wall

/// The Wall board's cell recipe: 8px padding, 6px gaps, top-anchored —
/// dot · slug name · 5px bar · one 9px status line; alert states carry a
/// 10px colored first line instead of the bar.
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
        focused,
        error,
        usage_meter,
        editing,
        reduce_motion,
        drop_target,
        show_focus,
    } = state;
    // A draft wears the live Pane's edge: the resting hairline (blending up
    // under the pointer on a board) or, beside other Panes, the focus ink.
    // It has no state to announce.
    let framed = focused && show_focus;
    let edge = PaneEdge::of(framed, false, false, !show_focus);
    let key = view.identity.draft().map_or(0, DraftId::get);
    let hover = HoverEdge::of(
        edge,
        show_focus,
        SharedString::from(format!("draft-edge-{key}")),
    );
    let mut shell = record_card(pane_shell(hover.ink(edge)), view);
    if show_focus {
        shell = shell.child(group_head(GroupHead {
            key,
            name: view.name.clone(),
            dot: None,
            unread: false,
            reduce_motion,
            title: None,
            branch: None,
            provider: None,
            slot: None,
            slot_detail: None,
            action: Some(discard),
            expand_question: false,
        }));
    }
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
            attachments,
            actions: composer_actions,
            background: None,
            menu,
            mode: None,
            mode_picker: None,
            model_picker: Some(picker),
            usage_meter,
            session_controls: None,
            setup_controls: Some(band),
            draft_error: error.cloned(),
            // A draft has no conversation yet, so nothing to predict.
            suggestion: None,
            focused,
            editing,
            drop_target,
            joined: false,
        },
    );
    pane_frame(
        shell
            // The body is empty space: the Composer's placeholder says what
            // to do, once (rule 2.11.4).
            .child(
                div()
                    .debug_selector(|| "draft-empty".into())
                    .flex_1()
                    .min_h_0(),
            )
            .children(drop_target.then(crate::prompt_drop::sheet))
            .when(level != Level::Wall, |pane| pane.child(composer)),
        framed,
        false,
        hover,
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

/// Draft setup controls ride the Composer's meta row. In a narrow Pane
/// they give way first: their labels truncate before the model and effort
/// pair or the usage meter loses a pixel. Their focus ring is inset and
/// takes no layout, so their labels start at C1 as the mode word does.
pub fn draft_band() -> Div {
    div()
        .debug_selector(|| "draft-band".into())
        .flex()
        .flex_shrink(1.)
        .min_w_0()
        .items_center()
        .gap(px(theme::PICKER_GAP))
        .h(px(theme::COMPOSER_ROW_H))
}

/// One band chip (project, workspace): the Composer's control chip — the
/// choice and a chevron that says it opens a menu — ringed by the one
/// focus recipe (`components::focused`) while tab rests on it, because the
/// popover opens on ↵ and the chip must say where ↵ will land.
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
        .rounded(px(theme::COMPOSER_CHIP_R))
        .map(|chip| components::focused(chip, focused))
        .hover_raised(format!("band-chip-{slot}"))
        .press_raised()
        .child(
            control_chip(if accent { TEXT } else { TEXT_2 })
                .flex_shrink(1.)
                .min_w_0()
                .child(div().min_w_0().truncate().child(label))
                .child(chip_chevron()),
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
        .rounded(px(theme::COMPOSER_CHIP_R))
        .map(|chip| components::focused(chip, focused))
        .child(control)
}

/// The wall's body (L3), under the one head: the state word hanging at the
/// text column (C1, 36px from the card edge), `WALL_ROW_GAP` under the head
/// rule — `working 12s`, `failing 2`, `needs you · approval`, `done` — and,
/// while work runs to a plan, its meter. Idle says nothing.
fn wall_cell(
    card: &WallCard,
    state: WallState,
    kind: Option<&'static str>,
    transcript: Option<&Transcript>,
) -> Div {
    let signal = wall_signal(state, kind, card, transcript);
    let meter = match state {
        WallState::Working | WallState::Failing => card
            .todos
            .map(|(done, total)| meter(done, total, state == WallState::Working)),
        _ => None,
    };
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .gap(px(theme::WALL_ROW_GAP))
        .pt(px(theme::WALL_ROW_GAP))
        .pl(px(theme::PANE_PAD_X + theme::GUTTER_W))
        .pr(px(theme::PANE_PAD_X))
        .overflow_hidden()
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .children(signal.map(|signal| {
            div()
                .debug_selector(|| "wall-signal".into())
                .flex_shrink_0()
                .w_full()
                .min_w_0()
                .child(head_slot_face(&signal).truncate())
        }))
        .children(meter.map(|meter| div().flex_shrink_0().child(meter)))
}

/// A cell's status dot, one recipe for L2, the wall, the Pane head and the
/// nav (`cockpit::thread_status`): running green, a Decision ochre, a
/// failing suite or a closed Session red, unread the accent, done and idle
/// the idle ink — green never means finished — and a parked Thread hollow.
#[cfg(test)]
pub(crate) fn cell_dot(state: WallState, unread: bool) -> Div {
    crate::cockpit::thread_status(state, unread).dot()
}

/// The Pane head's dot. The main Thread's is the status truth the nav and
/// the cells share (`thread_status`); a subagent tab's is that agent's own
/// transcript state, which unread never touches.
fn head_dot(
    main: bool,
    state: WallState,
    unread: bool,
    subject: Option<Status>,
) -> crate::cockpit::ThreadStatus {
    if main {
        return crate::cockpit::thread_status(state, unread);
    }
    let state = match subject {
        Some(Status::Streaming) => WallState::Working,
        Some(Status::Blocked) => WallState::Decision,
        Some(Status::Closed) => WallState::Blocked,
        _ => WallState::Idle,
    };
    crate::cockpit::thread_status(state, false)
}

/// What a pending request asks of the operator, as the lexicon names it.
fn request_kind(decision: &Decision) -> &'static str {
    if questions_of(decision).is_some() {
        theme::words::QUESTION
    } else {
        theme::words::APPROVAL
    }
}

/// One lexicon word for where a cell stands (rule 2.11.2), shared by the
/// Group head's right slot and the wall's signal line. Each variant is one
/// state; `text` is what it reads, `ink` the one ink it wears.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum HeadSlot {
    /// `needs you · approval` / `needs you · question` — `ATTENTION` on
    /// `needs you` only.
    NeedsYou(&'static str),
    /// `failing 2` (a test run, else CI), or bare `failing`.
    Failing(Option<usize>),
    /// A closed Session, or a turn that ended in an error.
    Failed,
    /// `working 12s`: whole seconds (`progress::live_seconds`).
    Working(String),
    Done,
    Interrupted,
    Parked,
    /// `ctx 32%`: on the focused cell, and on any cell at `USAGE_TIGHT`.
    Context(u32),
    /// A non-default permission mode (`accept edits`).
    Mode(SharedString),
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
            HeadSlot::Context(percent) => format!("ctx {percent}%"),
            HeadSlot::Mode(mode) => mode.to_string(),
        }
    }

    /// The word's ink: colour only where it means something needs you.
    pub(crate) fn ink(&self) -> u32 {
        match self {
            HeadSlot::Failing(_) | HeadSlot::Failed => BLOCKED,
            HeadSlot::Context(percent) if *percent as f32 >= theme::USAGE_TIGHT * 100. => ATTENTION,
            _ => TEXT_MUTED,
        }
    }
}

/// What the slot reads from.
struct SlotFacts<'a> {
    state: WallState,
    /// The first pending request's kind, if any request pends.
    kind: Option<&'static str>,
    card: &'a WallCard,
    /// Failed CI runs on the checkout's PR.
    ci_failing: usize,
    transcript: Option<&'a Transcript>,
    focused: bool,
    mode: Option<SharedString>,
}

/// The Group head's one word, by priority: what needs you, then a failure,
/// then work in progress, then a finished or stopped turn, then the
/// focused cell's context (or any cell's once it runs tight), then a
/// non-default mode. Idle says nothing.
fn head_slot(facts: SlotFacts<'_>) -> Option<HeadSlot> {
    let SlotFacts {
        state,
        kind,
        card,
        ci_failing,
        transcript,
        focused,
        mode,
    } = facts;
    let word = state_word(state, kind, card, transcript);
    if let Some(word @ (HeadSlot::NeedsYou(_) | HeadSlot::Failing(_) | HeadSlot::Failed)) = word {
        return Some(word);
    }
    // A failing CI run on the checkout's PR outranks work in progress.
    if ci_failing > 0 {
        return Some(HeadSlot::Failing(Some(ci_failing)));
    }
    if word.is_some() {
        return word;
    }
    let context = transcript
        .and_then(Transcript::usage)
        .and_then(|usage| {
            usage
                .context_window
                .filter(|window| *window > 0)
                .map(|window| usage.total_tokens as f32 / window as f32)
        })
        .filter(|used| focused || *used >= theme::USAGE_TIGHT)
        .map(|used| HeadSlot::Context((used.clamp(0., 1.) * 100.).round() as u32));
    context.or(mode.map(HeadSlot::Mode))
}

/// The state half of the vocabulary, shared by the head slot and the wall:
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
    (
        crate::cockpit::thread_status(state, unread),
        state_word(state, kind, card, Some(transcript)),
    )
}

/// The wall's line: the state word (a Decision's kind in `TEXT_2`, the one
/// place the wall names it).
fn wall_signal(
    state: WallState,
    kind: Option<&'static str>,
    card: &WallCard,
    transcript: Option<&Transcript>,
) -> Option<HeadSlot> {
    state_word(state, kind, card, transcript)
}

/// A slot word drawn: one lowercase run, tabular, in its ink — `needs you`
/// in `ATTENTION`, its `·` `TEXT_FAINT` and the kind after it quieter.
pub(crate) fn head_slot_face(slot: &HeadSlot) -> Div {
    let face = components::tabular(
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .whitespace_nowrap()
            .font_family(theme::FONT_UI)
            .text_size(px(theme::FS_SM))
            .line_height(px(theme::LH_META)),
    );
    match slot {
        // Where the head is too narrow for both, `· question` drops out
        // whole onto the clipped second line and `needs you` stays: the
        // word is never cut.
        HeadSlot::NeedsYou(kind) => face
            .flex_wrap()
            .flex_shrink(1.)
            .h(px(theme::LH_META))
            .overflow_hidden()
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(rgb(ATTENTION))
                    .child(theme::words::NEEDS_YOU),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .child(
                        div()
                            .px(px(theme::WORD_GAP))
                            .text_color(rgb(TEXT_FAINT))
                            .child("\u{b7}"),
                    )
                    .child(div().text_color(rgb(TEXT_2)).child(*kind)),
            ),
        slot => face.text_color(rgb(slot.ink())).child(slot.text()),
    }
}

// ------------------------------------------------------------- L2 cell

/// The Cockpit board's cell grammar (L2), under the one Group head: one
/// row of readings (plan, diff, files), then the conversation's tail,
/// newest at the bottom, the working line, and the grid's Composer line. A
/// pending Decision swaps the body for the y/n card. Every row sits on the
/// L1 axes: marks in the glyph box at `PANE_PAD_X`, text at C1 (36px).
/// Words, not chips: only a failure's words and the diff's signs carry a
/// hue.
#[allow(clippy::too_many_arguments)]
fn l2_cell(
    view: &PaneView,
    transcript: Option<&Transcript>,
    decision: Option<&Decision>,
    decide: Option<AnyElement>,
    composer: Option<Div>,
    requests: Option<AnyElement>,
    compact_question: bool,
    focused: bool,
    reduce_motion: bool,
    drop_target: bool,
) -> Div {
    // The drop sheet paints before the Composer, so the Composer shows
    // above it; a cell with no Composer takes it last, over everything.
    let sheet = || drop_target.then(crate::prompt_drop::sheet);
    let cell = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .overflow_hidden();
    let Some(transcript) = transcript else {
        return cell.child(parked_body()).children(sheet());
    };

    if let Some(requests) = requests {
        return cell
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(deferred(requests_overlay(requests))),
            )
            .children(sheet())
            .children(composer);
    }

    // A Decision's cell body is the card, keyed like the in-Pane card; the
    // cell keeps its Composer under it (every L2 cell has one). The card
    // holds the keyboard, so y/n answer; a press in the Composer takes
    // typing, and there an empty line's y/n answer as they do at L1.
    if let Some(decision) = decision.filter(|_| !compact_question) {
        let card = l2_decision_body(decision, decide)
            .key_context("Decision")
            .track_focus(&view.decision_focus);
        // The tail takes what the Decision leaves; the Decision never
        // yields (rule 2.8.7), and its head already says what the notice
        // would, so the tail prints only the notice's lead.
        return cell
            .child(l2_tail(transcript, view.text_namespace(), true))
            .child(card)
            .children(sheet())
            .children(composer);
    }

    let read = Instruments::of(transcript);
    let mut body = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(TEXT_MUTED));

    // One row of readings at C1: the plan's meter, the diff and how many
    // files it touched. A failing run is the head's word, not a reading.
    let mut readings = div()
        .flex()
        .flex_shrink_0()
        .min_w_0()
        .overflow_hidden()
        .items_center()
        .gap(px(theme::SPACE_3))
        .pt(px(theme::GAP_ROW))
        .pl(px(theme::PANE_PAD_X + theme::GUTTER_W))
        .pr(px(theme::PANE_PAD_X));
    let mut any = false;
    if let Some(todos) = read.todos.filter(|todos| todos.total > 0) {
        readings = readings.child(meter(
            todos.done,
            todos.total,
            transcript.status() == Status::Streaming,
        ));
        any = true;
    }
    if read.added > 0 || read.removed > 0 {
        readings = readings.child(diff_stat(read.added, read.removed).flex_shrink_0());
        any = true;
    }
    if read.files() > 0 {
        readings = readings.child(components::tabular(div().flex_shrink_0().child(
            SharedString::from(format!(
                "{} file{}",
                read.files(),
                if read.files() == 1 { "" } else { "s" }
            )),
        )));
        any = true;
    }
    if any {
        body = body.child(readings);
    }
    // The tail of the conversation fills what is left, bottom-anchored
    // under the head rule: prompts, answers and tool rows on the L1 gutter
    // grammar, newest at the bottom — what the Thread is saying, not only
    // that it is saying something.
    body = body.child(l2_tail(transcript, view.text_namespace(), false));
    if transcript.status() == Status::Streaming {
        body = body.child(
            div()
                .flex_shrink_0()
                .pt(px(theme::GAP_BLOCK))
                .px(px(theme::PANE_PAD_X))
                .child(working_line(
                    transcript,
                    true,
                    focused,
                    false,
                    reduce_motion,
                )),
        );
    }
    cell.child(body)
        .children(sheet())
        .children(composer.map(|composer| {
            // `GAP_ROW` under the working line, `GAP_BLOCK` under the tail.
            composer.pt(px(if transcript.status() == Status::Streaming {
                theme::GAP_ROW
            } else {
                theme::GAP_BLOCK
            }))
        }))
}

/// How many Blocks an L2 tail reaches back for — more than any cell can
/// show; the reach itself is the cell's height (`l2_tail`).
const L2_TAIL_BLOCKS: usize = 16;

/// One row of the L2 tail: its mark (drawn in the gutter), its line box,
/// and the block that owns it.
struct TailRow {
    id: BlockId,
    /// The row's own line height: what its reach is counted in.
    line: f32,
    /// A Prompt starts a turn: `GAP_BLOCK` above it, `GAP_ROW` otherwise.
    turn: bool,
    element: Div,
}

/// The visible text of an L2 tail row, as it reads and as it copies: what
/// `l2_tail_rows` draws, spelled once so the two cannot disagree.
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

/// The compact tail of a transcript for an L2 cell, rebuilt on the L1
/// gutter grammar: every row is `components::gutter(mark, line_box)` then
/// its text at C1 — a prompt behind `❯` (`FS_UI` `W_LABEL` `TEXT_STRONG`),
/// prose behind the Ferrite mark (Geist `FS_PROSE_SM`/`LH_PROSE_SM`,
/// `TEXT_2`, the mark on the first block of an answer as L1 marks it), a
/// tool row behind its `TOOL_DOT` (`Name(args)`, the args wrapping, never
/// cut), a notice behind an `ATTENTION` dot with only its lead phrase
/// coloured, and a stopped turn in the failure-line grammar. A completed
/// turn leaves no row: the head's `done` says it.
///
/// Reach is the cell's height: rows are measured newest first in the slot
/// they actually get and laid bottom-up, each clamped to the whole lines
/// (of its own line height) that still fit, so the tail never shows half a
/// line under the head rule.
fn l2_tail(transcript: &Transcript, namespace: SharedString, docked: bool) -> Div {
    let rows = l2_tail_rows(transcript, &namespace, docked);
    let selector = format!("l2-tail-{namespace}");
    div()
        .debug_selector(move || selector.clone())
        .flex()
        .flex_1()
        .min_h_0()
        .w_full()
        .px(px(theme::PANE_PAD_X))
        .child(
            canvas(
                move |bounds, window, cx| {
                    let mut remaining = bounds.size.height;
                    let mut visible = Vec::new();
                    for (index, row) in rows.into_iter().rev().enumerate() {
                        // The gap above a row belongs to it, except for the
                        // topmost, which sits against the head rule.
                        let lines = (f32::from(remaining) / row.line).floor().max(0.) as usize;
                        if lines == 0 {
                            break;
                        }
                        let selector = format!("l2-tail-row-{namespace}-{:?}", row.id);
                        let mut element = row
                            .element
                            .line_clamp(lines)
                            .debug_selector(move || selector.clone())
                            .into_any_element();
                        let size = element.layout_as_root(
                            gpui::size(
                                gpui::AvailableSpace::Definite(bounds.size.width),
                                gpui::AvailableSpace::MinContent,
                            ),
                            window,
                            cx,
                        );
                        if size.height > remaining {
                            break;
                        }
                        let origin = point(bounds.left(), bounds.top() + remaining - size.height);
                        element.prepaint_at(origin, window, cx);
                        visible.push(element);
                        let gap = if row.turn {
                            theme::GAP_BLOCK
                        } else {
                            theme::GAP_ROW
                        };
                        remaining -= size.height + px(gap);
                        let _ = index;
                        if remaining <= px(0.) {
                            break;
                        }
                    }
                    visible
                },
                |_, rows, window, cx| {
                    for mut row in rows {
                        row.paint(window, cx);
                    }
                },
            )
            .size_full(),
        )
}

/// The tail's rows, oldest first, each on the L1 gutter grammar.
fn l2_tail_rows(transcript: &Transcript, namespace: &str, docked: bool) -> Vec<TailRow> {
    let blocks = transcript.blocks();
    let tail = &blocks[blocks.len().saturating_sub(L2_TAIL_BLOCKS)..];
    // The live status already presents the current reasoning headline. Keep
    // older reasoning in the tail, and restore this row when the turn ends
    // or progress moves to a different caption.
    let live_reasoning = (transcript.status() == Status::Streaming)
        .then(|| transcript.progress().caption())
        .flatten()
        .and_then(|caption| {
            tail.iter()
                .rev()
                .take_while(|block| !matches!(block.body, Body::Prompt(_)))
                .find_map(|block| match &block.body {
                    Body::Thinking(text) => Some((block.id, text)),
                    _ => None,
                })
                .filter(|(_, text)| ferrite_core::progress::headline(text) == caption)
                .map(|(id, _)| id)
        });
    let mut rows: Vec<TailRow> = Vec::new();
    // L1 marks an answer once, on its first block.
    let mut in_answer = false;
    for block in tail {
        if live_reasoning == Some(block.id) {
            continue;
        }
        let Some(text) = tail_text(&block.body, docked) else {
            continue;
        };
        let prose_like = matches!(
            block.body,
            Body::Paragraph { .. } | Body::Bullet { .. } | Body::Heading { .. } | Body::Code { .. }
        );
        let lead_answer = prose_like && !in_answer;
        in_answer = prose_like;
        let row = |mark: AnyElement, line: f32| {
            div()
                .w_full()
                .flex()
                .items_start()
                .min_w_0()
                .flex_shrink_0()
                .child(components::gutter(mark, line))
        };
        let none = || div().into_any_element();
        let text_selector = format!("l2-tail-text-{namespace}-{:?}", block.id);
        let text_id = move || text_selector.clone();
        let (element, line) = match &block.body {
            Body::Prompt(_) => (
                row(components::prompt_mark(ACCENT), theme::LH_UI).child(
                    div()
                        .debug_selector(text_id)
                        .flex_1()
                        .min_w_0()
                        .text_size(px(theme::FS_UI))
                        .line_height(px(theme::LH_UI))
                        .font_weight(theme::W_LABEL)
                        .text_color(rgb(TEXT_STRONG))
                        .child(SharedString::from(text)),
                ),
                theme::LH_UI,
            ),
            Body::Paragraph { .. } | Body::Bullet { .. } | Body::Heading { .. } => {
                let heading = matches!(block.body, Body::Heading { .. });
                let mark = if lead_answer {
                    icon(
                        icons::FERRITE_MONO,
                        theme::GLYPH_BOX,
                        crate::transcript::ANSWER_MARK_INK,
                    )
                    .into_any_element()
                } else {
                    none()
                };
                (
                    row(mark, theme::LH_PROSE_SM)
                        .child(tail_prose(text, heading).debug_selector(text_id)),
                    theme::LH_PROSE_SM,
                )
            }
            Body::Code { .. } => (
                row(none(), theme::LH_UI).child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .font_family(theme::FONT_CODE)
                        .text_size(px(theme::FS_UI))
                        .line_height(px(theme::LH_UI))
                        .text_color(rgb(TEXT_MUTED))
                        .child(SharedString::from(text)),
                ),
                theme::LH_UI,
            ),
            Body::Tool(tool) => {
                let ink = match tool.state {
                    ToolState::Running => RUNNING,
                    ToolState::Failed(_) => BLOCKED,
                    _ => TEXT_FAINT,
                };
                (
                    row(
                        components::status_dot(ink)
                            .size(px(theme::TOOL_DOT))
                            .into_any_element(),
                        theme::LH_UI,
                    )
                    .child(
                        // `Name(args)`: one mono line, the name in body
                        // ink, wrapping inside the cell — never cut.
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(theme::FONT_CODE)
                            .text_size(px(theme::FS_UI))
                            .line_height(px(theme::LH_UI))
                            .font_weight(theme::W_BODY)
                            .text_color(rgb(TEXT_MUTED))
                            .child(StyledText::new(text).with_highlights(call_highlights(tool))),
                    ),
                    theme::LH_UI,
                )
            }
            Body::Notice(_) => {
                // The lead phrase is the state (`asks 1 question`); the
                // detail wraps to two lines at most, and goes while the
                // Decision is docked in this cell.
                let (lead, detail) = match text.split_once(" \u{b7} ") {
                    Some((lead, detail)) => (lead.to_string(), Some(detail.to_string())),
                    None => (text.clone(), None),
                };
                let mut highlights = vec![(
                    0..lead.len(),
                    HighlightStyle {
                        color: Some(rgb(ATTENTION).into()),
                        ..Default::default()
                    },
                )];
                if detail.is_some() {
                    let seam = lead.len()..lead.len() + " \u{b7} ".len();
                    highlights.push((
                        seam,
                        HighlightStyle {
                            color: Some(rgb(TEXT_FAINT).into()),
                            ..Default::default()
                        },
                    ));
                }
                (
                    row(
                        components::status_dot(ATTENTION)
                            .size(px(theme::TOOL_DOT))
                            .into_any_element(),
                        theme::LH_UI,
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .line_clamp(2)
                            .text_size(px(theme::FS_UI))
                            .line_height(px(theme::LH_UI))
                            .text_color(rgb(TEXT_2))
                            .child(StyledText::new(text).with_highlights(highlights)),
                    ),
                    theme::LH_UI,
                )
            }
            Body::TurnEnd(end) => {
                use ferrite_core::TurnOutcome;
                let (lead, ink) = match end.outcome {
                    TurnOutcome::Interrupted => (theme::words::INTERRUPTED, TEXT_2),
                    _ => (theme::words::FAILED, BLOCKED),
                };
                let mut highlights = separators(&text);
                highlights.insert(
                    0,
                    (
                        0..lead.len(),
                        HighlightStyle {
                            color: Some(rgb(ink).into()),
                            ..Default::default()
                        },
                    ),
                );
                (
                    div().w_full().flex_shrink_0().child(
                        result_line(TEXT_MUTED).child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(StyledText::new(text).with_highlights(highlights)),
                        ),
                    ),
                    theme::LH_UI,
                )
            }
            Body::Meta(_) | Body::Thinking(_) => (
                row(none(), theme::LH_META).child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(theme::FS_SM))
                        .line_height(px(theme::LH_META))
                        .text_color(rgb(TEXT_MUTED))
                        .child(SharedString::from(text)),
                ),
                theme::LH_META,
            ),
        };
        rows.push(TailRow {
            id: block.id,
            line,
            turn: matches!(block.body, Body::Prompt(_)),
            element,
        });
    }
    rows
}

/// An L2 tail's prose (and heading) text at C1: Geist `FS_PROSE_SM` on an
/// `LH_PROSE_SM` line in `TEXT_2` — prose never under 12.5 — a heading in
/// `TEXT_STRONG` at the label weight, never above it.
fn tail_prose(text: String, heading: bool) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_PROSE_SM))
        .line_height(px(theme::LH_PROSE_SM))
        .text_color(rgb(if heading { TEXT_STRONG } else { TEXT_2 }))
        .when(heading, |line| line.font_weight(theme::W_LABEL))
        .child(SharedString::from(text))
}

/// The Cockpit board's Decision cell body: the `◆ approval` head, the
/// subject in runs (`Bash · gh issue close 212`, the command in the code
/// face, soft-wrapping and never cut), where it runs, and — on the one
/// answer-target cell only — the `y allow  n deny  a always` pairs. It
/// takes its natural height, bottom-anchored `GAP_BLOCK` above the
/// Composer line; the transcript's tail above it takes what is left. The
/// keycaps arrive wired from the cockpit (#26), each only where its key
/// would act.
fn l2_decision_body(decision: &Decision, decide: Option<AnyElement>) -> Div {
    div()
        .debug_selector(|| "l2-decision".into())
        .flex()
        .flex_col()
        .flex_shrink_0()
        .justify_end()
        .px(px(theme::PANE_PAD_X))
        .pt(px(theme::GAP_BLOCK))
        .pb(px(theme::GAP_BLOCK))
        .gap(px(theme::DECISION_L2_GAP))
        .font_family(theme::FONT_UI)
        .child(decision::head(decision::kind_word(decision), None, None))
        // The subject and its place read on the text column, under the
        // kind word; `◆` alone holds the glyph column.
        .child(decision_subject_runs(decision).pl(px(theme::GUTTER_W)))
        .children(decision_place(decision).map(|place| {
            div()
                .w_full()
                .pl(px(theme::GUTTER_W))
                .truncate()
                .text_size(px(theme::FS_SM))
                .line_height(px(theme::LH_META))
                .text_color(rgb(TEXT_MUTED))
                .child(place)
        }))
        .children(decide)
}

// ------------------------------------------------------------- the head

/// The one Group head (rule 2.4.6, C3), for L1, L2 and the wall alike: one
/// `PANE_HEAD_H` line closed by a permanent `HAIRLINE` rule the body clips
/// at. In order: the status dot in the glyph box at `PANE_PAD_X` (x = 19,
/// every tier, so a board's dots share one vertical), the title at C1 in
/// `FS_UI` `W_LABEL` `TEXT_STRONG` taking the width it needs (never below
/// `HEAD_TITLE_MIN_W`), the branch in `TEXT_MUTED` only when it is not the
/// default, the provider mark only when it differs from the board's
/// majority, then a fixed right slot with one lexicon word (`HeadSlot`).
/// A draft reserves the glyph box with no dot and carries its × there.
pub(crate) struct GroupHead {
    pub key: u64,
    pub name: SharedString,
    pub dot: Option<crate::cockpit::ThreadStatus>,
    /// Finished while the operator looked elsewhere: the dot breathes in
    /// `ACCENT` on the shared clock until they land on it.
    pub unread: bool,
    pub reduce_motion: bool,
    /// The wired title (drag handle, double-click rename); `None` draws the
    /// name.
    pub title: Option<AnyElement>,
    pub branch: Option<SharedString>,
    pub provider: Option<Provider>,
    pub slot: Option<HeadSlot>,
    /// What the slot's word stands for, one hover away: a working cell's
    /// caption, a failed one's reason.
    pub slot_detail: Option<SharedString>,
    /// A trailing control in the slot's place (a draft's ×).
    pub action: Option<AnyElement>,
    /// The pending question is too big for this body: it answers in
    /// fullscreen (the expand key), which the slot's word stands for.
    pub expand_question: bool,
}

pub(crate) fn group_head(head: GroupHead) -> Div {
    let GroupHead {
        key,
        name,
        dot,
        unread,
        reduce_motion,
        title,
        branch,
        provider,
        slot,
        slot_detail,
        action,
        expand_question,
    } = head;
    let floor = title_floor(&name);
    let mark = match dot {
        Some(dot) if unread && dot.shape == crate::cockpit::DotShape::Solid => {
            components::breathing_dot(dot.ink, reduce_motion)
        }
        Some(dot) => dot.dot().into_any_element(),
        None => div().into_any_element(),
    };
    let slot = slot.map(|slot| match slot {
        HeadSlot::NeedsYou(_) => {
            let face = head_slot_face(&slot).when(expand_question, |face| {
                face.debug_selector(|| "question-expand".into())
            });
            needs_you_door(
                SharedString::from(format!("head-needs-you-{key}")),
                SharedString::from(format!("head-slot-{key}")),
                face,
            )
        }
        slot => {
            let face = head_slot_face(&slot).debug_selector(move || format!("head-slot-{key}"));
            match slot_detail.filter(|detail| !detail.is_empty()) {
                Some(detail) => div()
                    .id(("head-slot", key as usize))
                    .flex_shrink_0()
                    .tooltip(crate::menu::tooltip(detail))
                    .child(face)
                    .into_any_element(),
                None => face.into_any_element(),
            }
        }
    });
    div()
        .debug_selector(move || format!("pane-head-{key}"))
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::PANE_HEAD_H))
        .px(px(theme::PANE_PAD_X))
        .border_b_1()
        .border_color(rgba(HAIRLINE))
        .gap(px(theme::HEAD_GAP))
        .min_w_0()
        .overflow_hidden()
        .font_family(theme::FONT_UI)
        // The left cluster keeps its floor (the glyph box and the title's
        // `HEAD_TITLE_MIN_W`), so a narrow head gives way in the branch and
        // the slot's second word before it starves the title.
        .child(
            div()
                .flex()
                .flex_1()
                .min_w(px(theme::GUTTER_W + floor))
                .overflow_hidden()
                .items_center()
                .child(components::gutter(
                    div()
                        .debug_selector(move || format!("pane-head-dot-{key}"))
                        .flex()
                        .child(mark),
                    theme::LH_UI,
                ))
                .child(
                    div()
                        .debug_selector(move || format!("pane-head-title-{key}"))
                        .flex()
                        .flex_shrink(1.)
                        .min_w(px(floor))
                        .overflow_hidden()
                        .text_size(px(theme::FS_UI))
                        .line_height(px(theme::LH_UI))
                        .font_weight(theme::W_LABEL)
                        .text_color(rgb(TEXT_STRONG))
                        .child(match title {
                            Some(title) => title,
                            None => div().min_w_0().truncate().child(name).into_any_element(),
                        }),
                )
                .children(branch.map(|branch| {
                    div()
                        .debug_selector(move || format!("pane-head-branch-{key}"))
                        .flex()
                        .flex_shrink(theme::HEAD_CHECKOUT_SHRINK)
                        .min_w_0()
                        .overflow_hidden()
                        .items_center()
                        .gap(px(theme::ROW_ICON_GAP))
                        .ml(px(theme::HEAD_GAP))
                        .text_size(px(theme::FS_SM))
                        .line_height(px(theme::LH_META))
                        .text_color(rgb(TEXT_MUTED))
                        .child(icon(icons::BRANCH, theme::ROW_ICON, TEXT_MUTED))
                        .child(div().min_w_0().truncate().child(branch))
                }))
                .children(provider.map(|provider| {
                    let (glyph, ink) = match provider {
                        Provider::Codex => (icons::CODEX, theme::PROVIDER_CODEX),
                        Provider::Claude => (icons::CLAUDE, theme::PROVIDER_CLAUDE),
                    };
                    div()
                        .debug_selector(move || format!("pane-head-provider-{key}"))
                        .flex_shrink_0()
                        .ml(px(theme::HEAD_GAP))
                        .child(icon(glyph, theme::PROVIDER_MARK_SM, ink))
                })),
        )
        .children(slot)
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

/// The head's branch: the checkout's, only when it is not the default. The
/// Project's own default is not recorded yet, so `main` and `master` stand
/// in for it. Before the first checkout read lands, a worktree still names
/// itself (`binding_label`): which Threads can trample one another is the
/// question a board has to answer.
fn head_branch(
    checkout: Option<&BranchStatus>,
    branch: Option<&SharedString>,
    workspace: Option<&WorkspaceBinding>,
) -> Option<SharedString> {
    let name = checkout
        .and_then(|status| status.branch.clone())
        .map(SharedString::from)
        .or_else(|| branch.cloned())
        .or_else(|| match workspace {
            Some(WorkspaceBinding::Worktree { .. }) => Some(binding_label(workspace)),
            _ => None,
        })?;
    (!is_default_branch(&name)).then_some(name)
}

/// Whether a branch is the Project's default (see `head_branch`).
pub(crate) fn is_default_branch(name: &str) -> bool {
    matches!(name, "main" | "master")
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
        .border_color(rgba(HAIRLINE))
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
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

/// What became of a PR, as the head names it: `#48`, `#48 draft`,
/// `#48 merged`, `#48 closed` — in metadata ink, because a PR's fate is not
/// the Thread's state.
fn pr_label(pr: &PullRequest) -> SharedString {
    SharedString::from(match (pr.state, pr.draft) {
        (PrState::Merged, _) => format!("#{} merged", pr.number),
        (PrState::Closed, _) => format!("#{} closed", pr.number),
        (PrState::Open, true) => format!("#{} draft", pr.number),
        (PrState::Open, false) => format!("#{}", pr.number),
    })
}

/// The PR and its CI as one fact: `#48 ●`. The rollup's dot is the one
/// place CI colour appears (`check_ink`); the counts are the card's.
/// Without checks it is the label alone. `open` keeps the `FILL` ground
/// while the card this chip opened is showing.
///
/// Padded and rounded as a chip so its hover face reaches around the
/// glyphs.
fn ci_face(pr: &PullRequest, open: bool) -> Div {
    let face = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::ROW_ICON_GAP))
        .h(px(theme::CHIP_H))
        .px(px(theme::CHIP_PAD_X))
        .rounded(px(theme::R_CHIP))
        .when(open, |face| face.bg(rgb(FILL)))
        .child(pr_label(pr));
    match pr.checks {
        Some(checks) => face.child(check_dot(checks)),
        None => face,
    }
}

/// The PR/CI chip as the control it is: the same face, wearing the hover
/// and press faces every self-grounded control in the app wears, so the
/// one pressable fact in the head says so before it is pressed. It carries
/// its own id — `.active()` needs element identity — and the cockpit adds
/// only the listener.
///
/// `key` is the Thread the chip belongs to, which is what makes the id
/// unique across a board of Panes.
pub fn ci_mark(pr: &PullRequest, key: u64, open: bool) -> Stateful<Div> {
    ci_face(pr, open)
        .id(("ci-mark", key as usize))
        .debug_selector(move || format!("ci-mark-{key}"))
        .tooltip(|window, cx| gpui::component::tooltip::Tooltip::new("CI checks").build(window, cx))
        .map(|mark| {
            let hover = format!("ci-mark-{key}");
            if open {
                mark.hover_carried(hover)
            } else {
                mark.hover_control(hover)
            }
        })
        .press_control()
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
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
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
                .text_color(rgb(if openable { TEXT } else { TEXT_2 }))
                .child(name),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::FS_SM))
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

/// The painted meter (the head, L2 and the wall share it; neither face has
/// `▰▱`): done steps in `TEXT_2`, the rest unlit. `live` lights the
/// step being run in `RUNNING` — the one colour on a meter, because a step
/// in progress is live state. Finishing a plan is not a status: a full
/// meter is all `TEXT_2`, no check and no green.
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
                    rgb(TEXT_2).into()
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
                    .bg(rgb(TEXT_2)),
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
pub fn model_label(model: &str) -> SharedString {
    SharedString::from(ferrite_core::providers::models::display_name(model))
}

/// The Composer's model picker: a control chip with the provider's 12px
/// logomark in its brand colour, the bare
/// model name and a chevron. A busy Session mutes it rather than fading it.
/// Render-only; the cockpit gives it its id and its click.
pub fn model_picker(provider: Option<Provider>, label: SharedString, busy: bool) -> Div {
    let ink = if busy { TEXT_MUTED } else { TEXT_2 };
    let mark = provider.map(|provider| {
        let (glyph, ink) = match provider {
            Provider::Codex => (icons::CODEX, theme::PROVIDER_CODEX),
            Provider::Claude => (icons::CLAUDE, theme::PROVIDER_CLAUDE),
        };
        icon(glyph, theme::PROVIDER_MARK_SM, ink)
    });
    control_chip(ink)
        .children(mark)
        .child(div().flex_shrink_0().child(label))
        .child(chip_chevron())
}

/// The effort chip beside the model picker: the level in force (lowercase
/// in the chip; the menu rows keep their titles) and a chevron.
pub fn effort_picker(label: SharedString, busy: bool) -> Div {
    let ink = if busy { TEXT_MUTED } else { TEXT_2 };
    control_chip(ink)
        .child(div().flex_shrink_0().child(label))
        .child(chip_chevron())
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
/// input, or a diff (a grouped call shows its diff only when opened) — in
/// exactly the window L1 draws. Disclosure cycling, focus validation, and
/// controls all consume this one eligibility rule so an invisible row can
/// never remain keyboard-addressable.
pub fn tool_has_details(tool: &ToolBlock) -> bool {
    tool.output.is_some()
        || tool.structured_result.is_some()
        || !tool.summary.is_empty()
        || !tool.diffs.is_empty()
}

/// One visibility rule for rendering controls, keyboard cycling and focus.
/// Hidden children retain their expansion choice but cannot receive focus.
pub fn rendered_disclosures(view: &PaneView, blocks: &[Block], level: Level) -> Vec<DisclosureId> {
    let mut remaining = rendered_window(blocks, level);
    let mut controls = Vec::new();
    while let Some(block) = remaining.first() {
        if let Some(activity) = ToolActivity::at_start(remaining) {
            let group = DisclosureId::Group(activity.leader().call.clone());
            let expanded = view.tool_state(&group) == DisclosureState::Expanded;
            controls.push(group);
            for block in activity.blocks {
                if let Body::Tool(tool) = &block.body {
                    if (expanded || matches!(tool.state, ToolState::Failed(_)))
                        && tool_has_details(tool)
                    {
                        controls.push(DisclosureId::Tool(tool.call.clone()));
                    }
                }
            }
            remaining = &remaining[activity.blocks.len()..];
            continue;
        }
        match &block.body {
            Body::Tool(tool) if tool_has_details(tool) => {
                controls.push(DisclosureId::Tool(tool.call.clone()))
            }
            Body::Thinking(text) if reasoning_text(text).1.is_some() => {
                controls.push(DisclosureId::Reasoning(block.id))
            }
            _ => {}
        }
        remaining = &remaining[1..];
    }
    controls
}

/// The turn-wide change summary has its own disclosure, not a fabricated tool.
pub fn turn_diff_disclosure(transcript: &Transcript, level: Level) -> Option<DisclosureId> {
    (level == Level::Transcript)
        .then(|| transcript.turn_diff())
        .flatten()
        .map(|diff| DisclosureId::TurnDiff(diff.turn_id.clone()))
}

/// The working line's mark: the Ferrite shards snapping on their 3s
/// timeline, or the assembled mark at rest when the operator asked for
/// reduced motion. Its element id is a constant: the timeline has to
/// survive every re-render of the line.
fn working_mark(reduce_motion: bool) -> AnyElement {
    if reduce_motion {
        div()
            .debug_selector(|| "progress-mark-still".into())
            .child(icons::ferrite_icon(theme::GLYPH_BOX))
            .into_any_element()
    } else {
        div()
            .debug_selector(|| "progress-mark-live".into())
            .child(icons::animated_ferrite_icon(
                theme::GLYPH_BOX,
                "live-progress-indicator",
            ))
            .into_any_element()
    }
}

/// The working line (rule 2.6.2): one row, the animated Ferrite mark in the
/// gutter (at rest under reduced motion), the provider's live caption —
/// `Working` unless it has something better to say — then `(3s · ↓ 312
/// tokens)` in `TEXT_MUTED` at the same size, Claude Code's `✻ Working…
/// (12s · ↓ 1.2k tokens)`. The seconds are whole (`progress::live_seconds`),
/// tabular, so the text changes once a second however often the mark's
/// 33ms tick repaints it. On the focused Pane only (`focused`) it appends
/// `· esc to interrupt`, dropped whole where the row cannot hold it. The
/// caption is what truncates; the facts keep their room. L2 (`compact`)
/// draws the same row without the token count. Command details stay in the
/// tool rows.
fn working_line(
    transcript: &Transcript,
    compact: bool,
    focused: bool,
    received_reasoning_is_visible: bool,
    reduce_motion: bool,
) -> Div {
    let mut facts: Vec<String> = Vec::new();
    if let Some(elapsed) = transcript.turn_elapsed() {
        facts.push(ferrite_core::progress::live_seconds(elapsed));
    }
    let tokens = transcript.turn_output_tokens();
    if tokens > 0 && !compact {
        facts.push(format!("↓ {} tokens", tokens_label(tokens)));
    }
    let progress = transcript.progress();
    let caption = progress.caption().map(|caption| {
        if !compact && received_reasoning_is_visible {
            progress.phase.map(Phase::label).unwrap_or("Working").into()
        } else {
            caption
        }
    });
    let mut row = div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .w_full()
        .min_w_0()
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI));
    if let Some(caption) = caption {
        let selector = format!("progress-caption-{caption}");
        // The caption and its facts share one piece that may take the whole
        // row; the esc hint after it wraps onto the clipped second line —
        // gone, whole — when the row cannot hold both.
        let main = div()
            .flex()
            .items_center()
            .min_w_0()
            .max_w_full()
            .child(
                div()
                    .debug_selector(|| "progress-reasoning".into())
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(TEXT_2))
                    .child(SharedString::from(caption)),
            )
            .when(!facts.is_empty(), |main| {
                main.child(components::tabular(
                    div()
                        .debug_selector(|| "progress-metadata".into())
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .pl(px(theme::WORD_GAP))
                        .text_color(rgb(TEXT_MUTED))
                        .child(SharedString::from(format!("({})", facts.join(" \u{b7} ")))),
                ))
            });
        row = row
            .debug_selector(move || selector.clone())
            // The shard snap is this row's liveness signal, so the text
            // carries no extra opacity pulse.
            .child(components::gutter(
                working_mark(reduce_motion),
                theme::LH_UI,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .flex_1()
                    .min_w_0()
                    .h(px(theme::LH_UI))
                    .overflow_hidden()
                    .child(main)
                    .when(focused, |line| {
                        line.child(
                            div()
                                .debug_selector(|| "progress-esc".into())
                                .flex()
                                .flex_shrink_0()
                                .whitespace_nowrap()
                                .child(
                                    div()
                                        .px(px(theme::SPACE_1_5))
                                        .text_color(rgb(TEXT_FAINT))
                                        .child("\u{b7}"),
                                )
                                .child(div().text_color(rgb(TEXT_MUTED)).child("esc to interrupt")),
                        )
                    }),
            );
    }
    div().w_full().min_w_0().flex_shrink_0().child(row)
}

/// `8.0k`, `12k`, `340` — the token count the way Claude Code prints it.
fn tokens_label(tokens: u64) -> String {
    if tokens >= 10_000 {
        format!("{}k", tokens / 1000)
    } else if tokens >= 1000 {
        format!("{:.1}k", tokens as f64 / 1000.0)
    } else {
        tokens.to_string()
    }
}

fn parked_body() -> Div {
    div()
        .flex()
        .flex_1()
        .min_h_0()
        .items_center()
        .justify_center()
        .text_size(px(theme::FS_SM))
        .text_color(rgb(TEXT_MUTED))
        .child("parked")
}

// --------------------------------------------------------------- Composer

/// The Composer stack's slice of `PaneState`, bundled so `composer_region`
/// stays readable as the states grow.
struct ComposerStack<'a> {
    compact: bool,
    /// A board cell (C4): the Composer is one fixed `COMPOSER_GRID_H` line
    /// with no status row, flat and quiet until its cell holds focus.
    grid: bool,
    decision: Option<&'a Decision>,
    /// Prompts held back while the turn runs, newest first: they pile up
    /// above the line, the latest on top.
    queued: Vec<&'a str>,
    queue_height: f32,
    empty: bool,
    attachments: Option<AnyElement>,
    actions: Option<AnyElement>,
    /// Running background tasks as chips, hung at the right edge of the
    /// same shelf the pending files sit on.
    background: Option<AnyElement>,
    menu: Option<AnyElement>,
    mode: Option<&'a str>,
    /// The mode chip wired to its menu; `None` draws the plain chip.
    mode_picker: Option<AnyElement>,
    /// The Composer's model picker (#25) — drawn in every Pane.
    model_picker: Option<AnyElement>,
    /// Live usage sits immediately beside the model picker.
    usage_meter: Option<AnyElement>,
    session_controls: Option<AnyElement>,
    setup_controls: Option<AnyElement>,
    draft_error: Option<SharedString>,
    /// The follow-up predicted for this Thread's last response, if one has
    /// landed. The idle line shows it verbatim and Tab accepts it.
    suggestion: Option<&'a str>,
    /// Whether this Pane holds the keyboard: the line's debug name.
    focused: bool,
    /// The Composer itself holds the keyboard in the active window: the
    /// `❯` lights to `ACCENT` and the caret shows. Otherwise the `❯` is
    /// `TEXT_MUTED` and there is no caret.
    editing: bool,
    /// Native files hover the Pane: the block's edge is `ACCENT_EDGE`,
    /// saying where they will land.
    drop_target: bool,
    /// A docked Decision sits flush on top and ends in the seam: the block
    /// drops its top edge and top corners, so the two read as one.
    joined: bool,
}

/// The Composer: a raised block in the reading column. Its outer edges are
/// the column's edges and its content sits `BOX_INSET_X` inside them, so
/// its `❯` shares the transcript's glyph box and its text starts at C1.
/// `RAISED`, `COMPOSER_R`, a 1px edge that is always in layout
/// (`COMPOSER_EDGE`, `ACCENT_EDGE` while files hover it),
/// padding `COMPOSER_PAD_T/X/END/B`, rows `COMPOSER_GAP` apart:
///
/// - queued prompts, dim `❯` lines in a bounded scroll viewport;
/// - the input line, `❯` then the editor, growing upward to
///   `composer::MAX_ROWS` rows and then scrolling (Send/Stop end it at L1);
/// - the hint row: setup or mode and the key hints at left, usage, session
///   controls and the model pair at right (Send/Stop at L2).
///
/// The shelf — pending files at left, background tasks at right — floats
/// above the block, its right edge on the Send column. The Pane lays the
/// stack out `flex_shrink_0` below the body, so the transcript gives way.
/// The Decision card is **not** here: it is a sibling of the body. While a
/// Decision pends the block carries the `Decision` key context, so y/n/a
/// answer with the keyboard in the Composer (#23).
fn composer_region(view: &PaneView, transcript: Option<&Transcript>, stack: ComposerStack) -> Div {
    let ComposerStack {
        compact,
        grid,
        decision,
        queued,
        queue_height,
        empty,
        attachments,
        actions,
        background,
        menu,
        mode,
        mode_picker,
        model_picker,
        usage_meter,
        session_controls,
        setup_controls,
        draft_error,
        suggestion,
        focused,
        editing,
        drop_target,
        joined,
    } = stack;
    // On a board only the focused cell's line is live (C4): the others lie
    // flat on the Pane — no ground, the edge held in layout at zero ink, no
    // caret or placeholder, their controls held in layout unseen. The
    // switch is instant: focus moves by keyboard.
    let live = !grid || focused || drop_target;
    let (pad_t, pad_b) = if grid {
        (theme::COMPOSER_GRID_PAD_Y, theme::COMPOSER_GRID_PAD_Y)
    } else {
        (theme::COMPOSER_PAD_T, theme::COMPOSER_PAD_B)
    };
    let block = if live && joined {
        // The block's own top edge is the seam under the Decision: it stays
        // in layout, so a Decision arriving never moves the line.
        composer_box(composer_edge(drop_target)).rounded_t(px(0.))
    } else if live {
        composer_box(composer_edge(drop_target))
    } else {
        div()
            .border(px(theme::COMPOSER_EDGE_W))
            .border_color(rgba(TRANSPARENT))
            .rounded(px(theme::COMPOSER_R))
    };
    let geometry = view.geometry.clone();
    // The canvas fills the padding box: add back the edge held in layout.
    let mut block = components::on_bounds(block, move |bounds, _, _| {
        geometry.set(PaneGeometry {
            composer: Some(bounds.dilate(px(theme::COMPOSER_EDGE_W))),
            ..geometry.get()
        })
    });
    block = block
        .debug_selector(|| "composer-block".into())
        .when(drop_target, |block| {
            block.debug_selector(|| "composer-drop-target".into())
        })
        .when(!live, |block| {
            block.debug_selector(|| "composer-flat".into())
        })
        .relative()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(px(theme::COMPOSER_GAP))
        .min_w_0()
        .pt(px(pad_t))
        .pl(px(if compact {
            theme::COMPOSER_PAD_X_L2
        } else {
            theme::COMPOSER_PAD_X
        }))
        .pr(px(theme::COMPOSER_PAD_END))
        .pb(px(pad_b))
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(TEXT_2))
        .when(decision.is_some(), |block| block.key_context("Decision"));
    if let Some(error) = draft_error {
        block = block.child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .text_size(px(theme::FS_SM))
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
        block =
            block.child(
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
    // per visual row, so the line height here IS the row pitch. The idle
    // placeholder overlays its first row in every Pane whose line is empty,
    // focused or not: it carries a follow-up read off the last response,
    // and that is worth reading with the cursor already in the box.
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
        // The input is a terminal line: the code face, placeholder too.
        .font_family(theme::FONT_CODE)
        .font_weight(theme::W_BODY)
        .line_height(px(theme::COMPOSER_ROW_H))
        .text_color(rgb(TEXT))
        .child(view.composer.clone());
    if empty && live {
        // The Composer paints its own caret at the line origin, so the
        // ghost reserves the same caret inset in either focus state.
        // The hint is never cut: an accept key stays whole and the ghost
        // gives way to it; a pointer to the `/` menu wraps onto the
        // clipped second line — gone — when the row has no room for it.
        // A compact (L2) line has no room for a hint beside its ghost; the
        // `/` menu is one key away all the same.
        let ghost = placeholder(
            decision.is_some(),
            setup_controls.is_some(),
            transcript,
            suggestion,
        );
        line = line.child(ghost_row(ghost, compact));
    }
    // The `❯` is always in layout, so the text origin never moves with
    // focus. It hangs centred on the first row while the line grows.
    let line_selector = format!(
        "composer-{}-{}",
        if live { "live" } else { "flat" },
        view.text_namespace()
    );
    let mut input = div()
        .debug_selector(move || line_selector.clone())
        .flex()
        .items_start()
        .min_h(px(theme::COMPOSER_ROW_H))
        .min_w_0()
        .child(
            components::gutter(
                components::prompt_mark(prompt_ink(editing)),
                theme::COMPOSER_ROW_H,
            )
            .debug_selector(|| "composer-mark".into()),
        )
        .child(line);
    // The box's one row: `❯` and the line at left; the model pair and the
    // send control at right, on the first line's box however the line
    // grows (L2 has no model pair, only the send control).
    // On a board the status row does not exist: a draft's setup chips and
    // the session controls ride this row beside the model pair.
    let (row_setup, setup_controls) = if grid {
        (setup_controls, None)
    } else {
        (None, setup_controls)
    };
    let (row_session, session_controls) = if grid {
        (session_controls, None)
    } else {
        (None, session_controls)
    };
    input = input.child(
        div()
            .debug_selector(|| "composer-controls".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(theme::PICKER_GAP))
            .h(px(theme::COMPOSER_ROW_H))
            .ml(px(theme::SPACE_2))
            // An unfocused cell's controls keep their boxes, unseen and
            // unpressable: nothing reflows when focus arrives.
            .when(!live, |controls| controls.opacity(0.))
            .children(row_setup)
            .children(row_session.map(|controls| div().flex_shrink_0().child(controls)))
            .children(model_picker.map(|picker| div().flex_shrink_0().child(picker)))
            .children(actions.map(|actions| div().flex_shrink_0().child(actions))),
    );
    block = block.child(input);
    // The popover paints above the stack — deferred, so it escapes the
    // Pane's clip and draws over the transcript (#24).
    if let Some(menu) = menu {
        block = block.child(deferred(
            div()
                .absolute()
                .bottom(relative(1.))
                .left_0()
                .right_0()
                .mb(px(theme::FLOAT_OFFSET))
                .child(menu),
        ));
    }

    // The status line (rule 2.6.6), under the box and outside it, quiet
    // `FS_SM` `TEXT_MUTED`: a draft's setup chips and the Session's mode
    // word at left, the session controls and `ctx 32%` at right. It is
    // present in every state — a Decision included — at a fixed `LH_META`,
    // reserved when empty, so the Composer never moves. Its chips are
    // `CHIP_H`: they hang 2px into the air above and below the line, inside
    // a clip that is theirs. No key hints: the placeholder carries one, the
    // controls' tooltips name their keys, and every binding works whether
    // or not it is written down.
    let mut meta = div()
        .flex()
        .items_center()
        .gap(px(theme::SPACE_2))
        .h(px(theme::CHIP_H))
        .mt(px(-(theme::CHIP_H - theme::COMPOSER_META_H) / 2.))
        .pl(px(theme::COMPOSER_META_START))
        .pr(px(theme::COMPOSER_META_END))
        .min_w_0()
        .overflow_hidden()
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(TEXT_MUTED));
    if let Some(setup) = setup_controls {
        meta = meta.child(setup);
    }
    // The word is the live Session's permission mode, so it rides every
    // Pane whose Session has announced one other than the default — a
    // pending Decision too: the mode is what the answer will run under. A
    // closed Session has no mode to be in (its word is None). L2 draws it
    // plain (no menu).
    if let Some(mode) = mode.filter(|_| !grid) {
        let key = view.thread().map_or(0, ThreadId::get);
        meta = meta.child(
            div()
                .debug_selector(move || format!("composer-mode-{key}"))
                .flex_shrink_0()
                .child(match mode_picker {
                    Some(picker) => picker,
                    None => mode_chip(mode, false).into_any_element(),
                }),
        );
    }
    meta = meta.child(div().flex_1());
    if let Some(session_controls) = session_controls {
        meta = meta.child(div().flex_shrink_0().child(session_controls));
    }
    if let Some(meter) = usage_meter {
        meta = meta.child(div().flex_shrink_0().child(meter));
    }
    // A board has no status row (C4): ctx and a non-default mode are its
    // head's word.
    let meta = (!grid).then(|| {
        div()
            .debug_selector(|| "composer-meta".into())
            .flex_shrink_0()
            .h(px(theme::COMPOSER_META_H))
            .mt(px(theme::COMPOSER_META_GAP))
            .min_w_0()
            .child(meta)
    });
    let stack = div()
        .flex()
        .flex_col()
        .min_w_0()
        .when(attachments.is_some() || background.is_some(), |stack| {
            // The shelf floats `SHELF_GAP` clear of the block: pending
            // files from its outer left edge, the background chips at right
            // on the Send column. The chips give way first — they cut
            // their labels, the files do not.
            stack.child(
                div()
                    .debug_selector(|| "composer-shelf".into())
                    .flex()
                    .items_end()
                    .gap(px(theme::SPACE_2))
                    .min_w_0()
                    .pl(px(0.))
                    .pr(px(theme::COMPOSER_CONTROL_INSET))
                    .pb(px(theme::SHELF_GAP))
                    .when_some(attachments, |shelf, attachments| {
                        shelf.child(div().flex_1().min_w_0().child(attachments))
                    })
                    .when_some(background, |shelf, chips| {
                        shelf.child(div().ml_auto().min_w_0().max_w_full().child(chips))
                    }),
            )
        })
        .child(block)
        .children(meta);
    if compact {
        div()
            .flex_shrink_0()
            .min_w_0()
            .px(px(theme::COMPOSER_INSET_L2))
            .pb(px(theme::COMPOSER_INSET_L2))
            .child(stack)
    } else {
        div()
            .flex_shrink_0()
            .min_w_0()
            .px(px(theme::PANE_PAD_X))
            .pb(px(theme::COMPOSER_INSET_B))
            .child(components::reading_column(stack))
    }
}

/// The input line's `❯`: `ACCENT` while the keyboard is in the line and
/// `TEXT_MUTED` whenever keys would land elsewhere. A Decision's reply line
/// lights the same accent: the accent is the prompt's, never a state's.
fn prompt_ink(editing: bool) -> u32 {
    if editing {
        ACCENT
    } else {
        TEXT_MUTED
    }
}

/// The block's edge (as `0xRRGGBBAA`): `ACCENT_EDGE` while native files
/// hover it, saying where they will land, and the resting `COMPOSER_EDGE`
/// otherwise. Focus is the Pane ring, the accent `❯` and the caret; state
/// never recolours the block.
fn composer_edge(drop_target: bool) -> u32 {
    if drop_target {
        theme::ACCENT_EDGE
    } else {
        theme::COMPOSER_EDGE
    }
}

/// The Composer's box: the raised block with its always-in-layout edge,
/// cornered `COMPOSER_R` — concentric with the pill controls it holds
/// (§ theme "Concentric radii"). The Subagent footer draws the same box.
pub fn composer_box(edge: u32) -> Div {
    components::raised_edged(edge).rounded(px(theme::COMPOSER_R))
}

/// A Composer control on the hint row (§ theme "Composer controls"): the
/// model, effort and mode pickers and the session `•••` share it. No ground
/// at rest; the button it rides in (`composer_control`, `draft_picker`) or
/// its own id'd wrapper wears the one hover blend. Label in `ink`, an
/// optional chevron. Render-only; the cockpit wires it.
fn control_chip(ink: u32) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::PICKER_GAP))
        .h(px(theme::CHIP_H))
        .px(px(theme::PICKER_PAD_X))
        .rounded(px(theme::COMPOSER_CHIP_R))
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(ink))
}

/// A control chip's menu chevron.
fn chip_chevron() -> gpui::Svg {
    icon(icons::CHEVRON_DOWN, theme::ICON_CHEVRON_SM, TEXT_MUTED)
}

/// The status line's mode word (C17): Geist `FS_SM` `W_BODY` `TEXT_MUTED`
/// on the control-chip recipe, a value word like `ctx 32%`. When it opens a
/// menu its chevron shows only under the pointer or keyboard focus, so at
/// rest the line reads as words, not controls. Hidden at the default: the
/// mode stays reachable in the session controls card (`•••`).
pub fn mode_chip(mode: &str, menu: bool) -> Div {
    control_chip(TEXT_MUTED)
        .group("mode-chip")
        .font_family(theme::FONT_UI)
        .font_weight(theme::W_BODY)
        .child(mode.to_owned())
        .when(menu, |chip| {
            chip.child(
                div()
                    .flex()
                    .opacity(0.)
                    .group_hover("mode-chip", |chevron| chevron.opacity(1.))
                    .child(chip_chevron()),
            )
        })
}

/// The button a Composer chip rides in (model, effort, mode, session
/// `•••`): no padding of its own and the chip's pill radius, so the kit's
/// hover, pressed and focus faces fill exactly the chip's shape.
pub fn composer_control(
    id: impl Into<gpui::ElementId>,
    cx: &gpui::App,
) -> gpui::component::button::Button {
    chip_button(id, cx)
        .p_0()
        .h_auto()
        .rounded(px(theme::COMPOSER_CHIP_R))
}

/// A Composer chip's button: no ground at rest, `HOVER_RAISED` under the
/// pointer over the one 150ms blend, `FILL_HOVER` pressed at once.
fn chip_button(id: impl Into<gpui::ElementId>, cx: &gpui::App) -> gpui::component::button::Button {
    components::faded_button(
        id,
        gpui::rgba(theme::TRANSPARENT).into(),
        rgb(theme::HOVER_RAISED).into(),
        rgb(theme::FILL_HOVER).into(),
        rgb(TEXT_2).into(),
        cx,
    )
}

/// The session-controls trigger: `•••` on the control-chip recipe.
pub fn session_chip() -> Div {
    control_chip(TEXT_MUTED).child("•••")
}

/// The idle line's ghost (§D.7): a ladder of rungs, longest first, of
/// which the line shows the first that fits — never a word cut in half.
/// The fullest rung carries the one key hint the Composer writes down
/// (its controls' tooltips name theirs) after a `TEXT_FAINT` `·`.
///
/// A predicted follow-up is already in the operator's voice and already
/// filtered, so it is shown verbatim — a draft of their next prompt, not a
/// description of one — with `⇥ accept`, which is always kept whole: an
/// accept key nobody knows about is the same as no accept key, so the
/// prediction's own words give way to it. The resting line points at the
/// `/` menu, where everything else lives, and that pointer drops out whole
/// first where the row cannot hold it.
#[derive(Clone, Debug, PartialEq)]
struct Ghost {
    /// The shortest rung's words, before its ellipsis.
    head: SharedString,
    /// The words a fuller rung adds after `head`; they drop out whole.
    more: Option<&'static str>,
    /// A verbatim prediction: no ellipsis, and its words give way to the
    /// accept hint rather than the other way round.
    verbatim: bool,
    hint: Option<(&'static str, &'static str)>,
}

impl Ghost {
    fn ladder(head: &'static str, more: Option<&'static str>, hint: bool) -> Self {
        Self {
            head: head.into(),
            more,
            verbatim: false,
            hint: hint.then_some(("/", "for commands")),
        }
    }

    /// Whether the hint stays whole whatever the width (`⇥ accept`).
    fn keeps_hint(&self) -> bool {
        self.verbatim
    }

    /// Every rung the line can show, longest first: the texts `ghost_row`'s
    /// row-fit check picks from, spelled out for the tests.
    #[cfg(test)]
    fn rungs(&self) -> Vec<String> {
        let hint = |text: &str| match self.hint {
            Some((key, verb)) => format!("{text} \u{b7} {key} {verb}"),
            None => text.to_owned(),
        };
        if self.verbatim {
            return vec![hint(&self.head)];
        }
        let short = format!("{}\u{2026}", self.head);
        let full = match self.more {
            Some(more) => format!("{}{more}\u{2026}", self.head),
            None => short.clone(),
        };
        let mut rungs = Vec::new();
        if self.hint.is_some() {
            rungs.push(hint(&full));
        }
        rungs.push(full);
        if self.more.is_some() {
            rungs.push(short);
        }
        rungs
    }
}

/// The ghost for this line: a Decision's reply, a dead Session's revival,
/// a landed prediction, a draft's first prompt, or steering a live Thread.
fn placeholder(
    pending: bool,
    draft: bool,
    transcript: Option<&Transcript>,
    suggestion: Option<&str>,
) -> Ghost {
    if draft {
        return Ghost::ladder("Start a thread", None, true);
    }
    match followup::suggest(pending, transcript, suggestion) {
        // A docked Decision owns the block above this line; the line itself
        // still steers, so it says so (rule 2.8.1: one input line).
        Followup::Decision => Ghost::ladder("Steer", Some(" this Thread"), false),
        Followup::Revive => Ghost::ladder("Revive", Some(" and continue"), false),
        Followup::Suggested(text) => Ghost {
            head: SharedString::from(text),
            more: None,
            verbatim: true,
            hint: Some(("\u{21e5}", "accept")),
        },
        Followup::Steer => Ghost::ladder("Steer", Some(" this Thread"), true),
    }
}

/// The ghost drawn: rungs as whole pieces on a clipped, wrapping 20px row,
/// so a piece that does not fit falls to the hidden second line whole and
/// the row shows the longest rung that fits. `head` comes first with its
/// ellipsis hung just past it; `more…` follows on the block's own ground,
/// covering that ellipsis, so the pair reads `head more…`; the hint (`·` in
/// `TEXT_FAINT`, `SPACE_1_5` either side) last. A prediction instead lets
/// its own words wrap away whole before the accept hint, which never goes.
fn ghost_row(ghost: Ghost, compact: bool) -> Div {
    let hint = ghost.hint.filter(|_| !compact).map(|(key, verb)| {
        div()
            .debug_selector(|| "prompt-placeholder-hint".into())
            .flex()
            .flex_shrink_0()
            .child(
                div()
                    .px(px(theme::SPACE_1_5))
                    .text_color(rgb(TEXT_FAINT))
                    .child("\u{b7}"),
            )
            .child(format!("{key} {verb}"))
    });
    let row = div()
        .debug_selector(|| "prompt-placeholder".into())
        .absolute()
        .left(px(theme::CARET_W))
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
                    .overflow_hidden()
                    .whitespace_normal()
                    .child(ghost.head),
            )
            .children(hint);
    }
    let head = match ghost.more {
        // The short rung's ellipsis hangs just past `head`, where `more`
        // (opaque on the block's ground) covers it whenever it fits.
        Some(_) => div().relative().flex_shrink_0().child(ghost.head).child(
            div()
                .absolute()
                .top_0()
                .left(relative(1.))
                .child("\u{2026}"),
        ),
        None => div()
            .flex_shrink_0()
            .child(format!("{}\u{2026}", ghost.head)),
    };
    row.flex_wrap()
        .child(head)
        .children(ghost.more.map(|more| {
            div()
                .flex_shrink_0()
                .h(px(theme::COMPOSER_ROW_H))
                .bg(rgb(RAISED))
                .child(format!("{more}\u{2026}"))
        }))
        .children(hint)
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
    /// Matched byte ranges inside `name`, painted `ACCENT` (never a weight,
    /// so the row never reflows as the cursor moves).
    pub matched: Vec<std::ops::Range<usize>>,
    /// The dimmer text after it: a command's description, or the file's
    /// directory. Empty draws nothing.
    pub detail: SharedString,
    /// Whether `detail` is Ferrite's description (Geist, cut at its end) or
    /// machine text such as a path (Geist Mono, cut at its head so the
    /// useful tail survives).
    pub prose_detail: bool,
    /// A row kept visible but dead (#25's locked provider door): muted ink,
    /// no match highlights, and its pick does nothing but dismiss.
    pub inert: bool,
}

/// The Composer menus' popover: the one floating surface at the Composer's
/// own width, capped at `MENU_MAX_H` (its row list scrolls past that).
pub fn menu_popover() -> Div {
    components::floating_surface()
        .w_full()
        .max_h(px(theme::MENU_MAX_H))
}

/// A `/` or `@` row in the shared menu grammar: the name with its matches
/// in the accent, the detail muted in its own column (`label_w` aligns the
/// slash commands' descriptions), `↵` on the cursor row.
pub fn menu_row(
    id: impl Into<gpui::ElementId>,
    row: &MenuRow,
    cursor: bool,
    label_w: Option<f32>,
) -> Stateful<Div> {
    // A description or a path cut at the popover's width keeps its whole
    // text one hover away.
    let detail = (!row.detail.is_empty()).then(|| row.detail.clone());
    components::menu_row(id, &menu_item(row, cursor, label_w), cursor, false)
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

/// The Composer's height less its editor rows and queue: the inset below
/// it, the block's two edges and padding, and — in Solo — the status row
/// under the block with its gap. A board cell's line is `COMPOSER_GRID_H`
/// with its one row, and has no status row. The shelf floats above and is
/// not part of the budget.
fn composer_fixed_height(compact: bool, grid: bool) -> f32 {
    let inset = if compact {
        theme::COMPOSER_INSET_L2
    } else {
        theme::COMPOSER_INSET_B
    };
    if grid {
        return inset + theme::COMPOSER_GRID_H - theme::COMPOSER_ROW_H;
    }
    inset
        + 2. * theme::COMPOSER_EDGE_W
        + theme::COMPOSER_PAD_T
        + theme::COMPOSER_PAD_B
        + theme::COMPOSER_META_GAP
        + theme::COMPOSER_META_H
}

/// Leave the majority of a Pane available for its Thread context. Only the
/// viewport changes: the Composer keeps every character and scrolls to its
/// caret, then reveals more rows again when the Pane grows.
pub(crate) fn composer_row_limit(height: f32, compact: bool, grid: bool, queued: usize) -> usize {
    let fixed = composer_fixed_height(compact, grid);
    let queue = composer_queue_height(height, compact, grid, queued)
        + if queued > 0 { theme::COMPOSER_GAP } else { 0. };
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
    div()
        .debug_selector(move || format!("queued-{index}"))
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::QUEUE_ROW_H))
        .min_w_0()
        .child(components::gutter(
            components::prompt_mark(TEXT_FAINT),
            theme::QUEUE_ROW_H,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(rgb(TEXT_MUTED))
                .child(SharedString::from(held.to_owned())),
        )
        .when(latest, |row| {
            row.child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(theme::SPACE_3))
                    .ml(px(theme::SPACE_2))
                    .child(components::tabular(components::text_meta().child(
                        SharedString::from(if count > 1 {
                            format!("{count} queued")
                        } else {
                            "queued".to_owned()
                        }),
                    )))
                    .when(keys, |row| {
                        row.child(components::key_hints(&[("↑", "edit"), ("⌫", "drop")]))
                    }),
            )
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
/// `$ ` prompt.
pub(crate) fn shell_command(decision: &Decision) -> bool {
    decision.tool_name == "Bash"
        && decision
            .input
            .get("command")
            .and_then(serde_json::Value::as_str)
            .is_some()
}

/// The exact tool input an approval would send. Commands retain their source;
/// other provider input remains inspectable as its JSON value.
pub(crate) fn approval_input(
    decision: &Decision,
    cache: &crate::rich::TextCache,
    id: SharedString,
) -> Option<AnyElement> {
    use gpui::component::scroll::ScrollableElement as _;

    let source = approval_source(decision)?;
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
/// the command itself when the input carries one (machine text, `mono`),
/// else the provider's description. A question is its summary.
struct DecisionSubject {
    tool: Option<SharedString>,
    text: Option<SharedString>,
    mono: bool,
}

fn subject_parts(decision: &Decision) -> DecisionSubject {
    if let Some(questions) = questions_of(decision) {
        return DecisionSubject {
            tool: None,
            text: Some(ferrite_core::questions::summary(questions).into()),
            mono: false,
        };
    }
    let command = decision
        .input
        .get("command")
        .and_then(serde_json::Value::as_str)
        .filter(|command| !command.trim().is_empty());
    let tool = (!decision.tool_name.is_empty()).then(|| decision.tool_name.clone().into());
    let (text, mono) = match command {
        Some(command) => (Some(command.to_string().into()), true),
        None => (
            (!decision.description.is_empty()).then(|| decision.description.clone().into()),
            false,
        ),
    };
    if tool.is_none() && text.is_none() {
        return DecisionSubject {
            tool: None,
            text: Some("unreadable permission request".into()),
            mono: false,
        };
    }
    DecisionSubject { tool, text, mono }
}

/// The Decision's subject as one line of words — `Bash · gh issue close
/// 212`, never a `Bash:` label; the tool's name alone without a
/// description, else the honest unreadable fallback. Every surface that
/// names a Decision in words (the wall alert's tooltip) goes through here.
fn decision_subject(decision: &Decision) -> SharedString {
    let DecisionSubject { tool, text, .. } = subject_parts(decision);
    match (tool, text) {
        (Some(tool), Some(text)) => format!("{tool} \u{b7} {text}").into(),
        (Some(only), None) | (None, Some(only)) => only,
        (None, None) => "unreadable permission request".into(),
    }
}

/// The subject drawn (the L2 Decision cell): the tool in Geist
/// `TEXT_MUTED`, `·` in `TEXT_FAINT`, then the command in the code face at
/// `FS_UI` `TEXT_STRONG` — soft-wrapping, never cut.
fn decision_subject_runs(decision: &Decision) -> Div {
    let DecisionSubject { tool, text, mono } = subject_parts(decision);
    let seam = tool.is_some() && text.is_some();
    div()
        .w_full()
        .min_w_0()
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_baseline()
                .min_w_0()
                .children(tool.map(|tool| {
                    div()
                        .flex_shrink_0()
                        .font_family(theme::FONT_UI)
                        .text_color(rgb(TEXT_MUTED))
                        .child(tool)
                }))
                .when(seam, |line| {
                    line.child(
                        div()
                            .flex_shrink_0()
                            .px(px(theme::SPACE_1_5))
                            .text_color(rgb(TEXT_FAINT))
                            .child("\u{b7}"),
                    )
                })
                .children(text.map(|text| {
                    div()
                        .min_w_0()
                        .whitespace_normal()
                        .when(mono, |run| run.font_family(theme::FONT_CODE))
                        .text_color(rgb(TEXT_STRONG))
                        .child(text)
                })),
        )
}

/// Where an approval would run — `in /work/api` — when the request names
/// its cwd.
fn decision_place(decision: &Decision) -> Option<SharedString> {
    if questions_of(decision).is_some() {
        return None;
    }
    decision
        .input
        .get("cwd")
        .and_then(|cwd| cwd.as_str())
        .map(|cwd| SharedString::from(format!("in {cwd}")))
}

/// The L2 decide keycaps, one constructor per verb, so the cockpit can wire
/// each press without respelling the keycap grammar (#26).
pub fn keycap_allow(verb: bool) -> Stateful<Div> {
    decision::key_action("y allow", "y", "allow", verb)
}
pub fn keycap_deny(verb: bool) -> Stateful<Div> {
    decision::key_action("n deny", "n", "deny", verb).debug_selector(|| "decision-deny".into())
}
pub fn keycap_always(verb: bool) -> Stateful<Div> {
    decision::key_action("a always", "a", "always", verb)
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

/// `+N −N`: a change's size. Only the signs carry the diff hues — the
/// counts are metadata — and it is one text run, so gpui's per-run pixel
/// rounding cannot widen it. Drawn in a tool row's trail and a changed-strip
/// chip.
fn diff_stat(added: usize, removed: usize) -> Div {
    let text = format!("+{added} \u{2212}{removed}");
    let removed_at = format!("+{added} ").len();
    let sign = |at: usize, len: usize, ink: u32| {
        (
            at..at + len,
            HighlightStyle {
                color: Some(rgb(ink).into()),
                ..Default::default()
            },
        )
    };
    let highlights = vec![
        sign(0, 1, RUNNING),
        sign(removed_at, '\u{2212}'.len_utf8(), BLOCKED),
    ];
    components::tabular(
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .text_size(px(theme::FS_SM))
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
/// `BLOCKED`, the `·` in structure ink, then the message in `TEXT_2`, all
/// UI `FS_SM` as one run that wraps, on the rows' inset.
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
        .text_color(rgb(TEXT_2))
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

/// The usage meter's detail card: the meter's own windows, in the meter's
/// own order, each a labelled bar over the reading behind it. Counts are
/// reported values, never estimates. An account window the provider has
/// not reported is left out; when it reports neither, one line says so
/// (`Limits not reported by this provider`) in place of two empty bars.
/// Each block holds the menu rows' inset (`MENU_ROW_PAD_X`) inside the
/// floating surface's `FLOAT_PAD`.
pub fn context_usage(
    usage: ferrite_core::transcript::Usage,
    limits: ferrite_core::transcript::RateLimits,
    details: Option<&ferrite_core::ContextDetails>,
    usage_details: Option<&ferrite_core::UsageDetails>,
    last_cost: Option<f64>,
) -> impl IntoElement {
    fn count_label(count: u64) -> String {
        let digits = count.to_string();
        let mut label = String::new();
        for (index, digit) in digits.chars().enumerate() {
            if index > 0 && (digits.len() - index).is_multiple_of(3) {
                label.push(',');
            }
            label.push(digit);
        }
        label
    }
    let maximum = usage.context_window.filter(|limit| *limit > 0);
    let now = SystemTime::now();
    // One 4px bar, full width: the same track and the same status ink as
    // the meter that opened the card, at a size a card can afford.
    let bar = |fraction: f32| {
        let used = fraction.clamp(0., 1.);
        div()
            .w_full()
            .h(px(theme::USAGE_CARD_BAR_H))
            .rounded(px(theme::USAGE_CARD_BAR_H / 2.))
            .bg(rgba(METER_OFF))
            .child(
                div()
                    .h_full()
                    .w(relative(used))
                    .rounded(px(theme::USAGE_CARD_BAR_H / 2.))
                    .bg(rgb(usage_ink(used))),
            )
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
                .text_size(px(theme::FS_SM))
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
            .gap(px(theme::USAGE_CARD_ROW_GAP))
            .px(px(theme::MENU_ROW_PAD_X))
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
    // The counts behind the context bar, in the card's quietest ink: the
    // bar says how full, this says of what — `64k / 200k tokens`.
    let current = usage.total_tokens;
    let counts = components::tabular(
        div()
            .id("context-usage-counts")
            .text_size(px(theme::FS_SM))
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
    let context = match maximum {
        Some(maximum) => window(
            "Context",
            "context",
            usage.total_tokens as f32 / maximum as f32,
            Some(counts.into_any_element()),
        ),
        // No window to divide by: the count alone, no empty bar.
        None => block()
            .child(heading("Context", div().into_any_element()))
            .child(counts),
    };
    let mut card = div()
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
            components::menu_note("Limits not reported by this provider")
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
    // the left, the reported value right-aligned in tabular digits, every
    // count grouped the same way, and a section head where the scope
    // changes.
    let row = |key: String, value: String| {
        div()
            .flex()
            .justify_between()
            .gap(px(theme::SPACE_3))
            .px(px(theme::MENU_ROW_PAD_X))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(TEXT_MUTED))
                    .child(key),
            )
            .child(components::tabular(div().flex_shrink_0().child(value)))
    };
    if let Some(details) = details {
        let mut section = div()
            .flex()
            .flex_col()
            .gap(px(theme::SPACE_0_5))
            .child(components::menu_section("Context breakdown", None, None));
        if let Some(usable) = details.usable_window {
            section = section.child(
                row("Usable".into(), count_label(usable))
                    .id(SharedString::from(format!("context-usable-{usable}")))
                    .debug_selector(move || format!("context-usable-{usable}")),
            );
        }
        if let Some(threshold) = details.auto_compact_threshold {
            let key = match details.is_auto_compact_enabled {
                Some(true) => "Compacts at",
                Some(false) => "Compaction off at",
                None => "Compaction threshold",
            };
            section = section.child(
                row(key.into(), count_label(threshold))
                    .id(SharedString::from(format!(
                        "context-compaction-{threshold}"
                    )))
                    .debug_selector(move || format!("context-compaction-{threshold}")),
            );
        }
        for (index, category) in details.categories.iter().enumerate() {
            let tokens = category.tokens;
            section = section.child(
                row(sentence_case(&category.name), count_label(tokens))
                    .id(SharedString::from(format!(
                        "context-category-{index}-{tokens}"
                    )))
                    .debug_selector(move || format!("context-category-{index}-{tokens}")),
            );
        }
        card = card.child(section);
    }
    if let Some(details) = usage_details {
        let scope = match details.scope {
            ferrite_core::UsageScope::Message => ("message", "This message"),
            ferrite_core::UsageScope::Turn => ("turn", "This turn"),
            ferrite_core::UsageScope::Session => ("session", "This session"),
        };
        let mut section = div().flex().flex_col().gap(px(theme::SPACE_0_5)).child(
            components::menu_section(scope.1, None, None)
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
                row(label.into(), count_label(count))
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
    // Counts, percentages and the cost tick while the card is open.
    components::tabular(card)
        .max_h(px(theme::MENU_MAX_H))
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

/// A usage reading's ink: neutral `TEXT_2` until the window runs tight
/// (`USAGE_TIGHT`), then `ATTENTION`. There is no `BLOCKED` step: a full
/// window stops nothing until the provider says so. Colour is state; a
/// context half full is not one.
pub fn usage_ink(fraction: f32) -> u32 {
    if fraction >= theme::USAGE_TIGHT {
        ATTENTION
    } else {
        TEXT_2
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

/// The status line's usage readout (rule 2.6.6): text, not a meter — `ctx
/// 32%` in `FS_SM` tabular `TEXT_MUTED`, plus a tight account window
/// (`5h 91%`), inside the control chip that opens the usage card. `None`
/// when there is nothing to read: no reading is ever invented (no `ctx —`).
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
        control_chip(TEXT_MUTED).child(components::tabular(
            div()
                .debug_selector(move || format!("usage-readout-{key}"))
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(theme::SPACE_2))
                .whitespace_nowrap()
                .children(tokens.into_iter().map(|(token, used)| {
                    div()
                        .debug_selector({
                            let token = token.clone();
                            move || format!("usage-token-{token}")
                        })
                        .text_color(rgb(readout_ink(used)))
                        .child(SharedString::from(token))
                })),
        )),
    )
}

/// Which checkout a Thread works in — a worktree's own name, or "main" for
/// the shared one. One line, because an operator running many Threads has to
/// know which of them can trample the others. Shared with the nav's rows
/// (#21), so both surfaces name a binding the same way.
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
    components::menu_row(id, &item, cursor, false)
}

/// A picker's section title: the Provider's logomark in its brand colour
/// (the one place brand colour is allowed) and its name, with an optional
/// note after it. Non-interactive — the arrows skip it.
pub fn picker_section(provider: Provider, note: SharedString) -> Div {
    let (mark, ink, title) = match provider {
        Provider::Codex => (icons::CODEX, theme::PROVIDER_CODEX, "Codex"),
        Provider::Claude => (icons::CLAUDE, theme::PROVIDER_CLAUDE, "Claude"),
    };
    components::menu_section(title, Some((mark, ink)), (!note.is_empty()).then_some(note))
}

/// The popover's key-hint footer, each menu supplying its own verbs.
pub fn popover_footer(hints: &[(&str, &str)]) -> Div {
    components::menu_footer(hints)
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

/// One Block as a transcript row, in the terminal grammar `theme.rs`'s WP-A
/// section states: a drawn mark in the gutter, centred on the first line
/// box, and the text at C1. Rows own no spacing; the list gives each row its
/// gap from the gap table.
///
/// Every text run routes through the selection overlay (#27) — that is what
/// makes it selectable and copyable; the marks, elbows, trails, the `$` and
/// the diff numbers around the runs are chrome, and stay plain. Anything a
/// row registers is mirrored by its `pane/text.rs` collector.
pub(crate) fn render_block(
    block: &Block,
    selection: &TextRuns,
    timings: Option<&HashMap<String, ToolTiming>>,
    expanded: bool,
    disclosure: Option<Disclosure>,
    signal: u32,
    _provider: Option<Provider>,
    preview: &crate::attachment_preview::Preview,
    prompt_actions: Option<AnyElement>,
    reading: ferrite_core::settings::SoloReadingSize,
) -> AnyElement {
    let row = div().w_full().min_w_0().flex_shrink_0();
    // Prose rows read at the answer's size; tool rows and the stamp do not
    // scale.
    let (prose_size, prose_line) = (
        theme::answer_text_size(reading),
        theme::answer_line_height(reading),
    );
    match &block.body {
        // No band and no hover ground: the accent `❯`, the label weight and
        // the strongest ink carry the prompt. Its actions show under the
        // pointer, their box always reserved.
        Body::Prompt(line) => {
            let (text, files) = ferrite_core::prompt_files::split(line.clone());
            let blank = text.is_empty();
            div()
                .debug_selector(|| "transcript-prompt".into())
                .group("sent-prompt")
                .id(SharedString::from(format!("prompt-{:?}", block.id)))
                .relative()
                .flex()
                .items_start()
                .min_w_0()
                .flex_shrink_0()
                .hover_text()
                .on_hover(crate::motion::hover_listener(prompt_hover_key(block.id)))
                // The operator's own words head the turn: the answer's size
                // in the UI face, set apart from the answer under it by the
                // `❯`, the label weight and the strong ink.
                .font_family(theme::FONT_UI)
                .font_weight(theme::W_LABEL)
                .text_size(px(prose_size))
                .line_height(px(prose_line))
                .text_color(rgb(TEXT_STRONG))
                .child(components::gutter(
                    components::prompt_mark(ACCENT),
                    prose_line,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .gap(px(theme::GAP_ROW))
                        .child(
                            div()
                                .flex()
                                .items_start()
                                .w_full()
                                .min_w_0()
                                .gap(px(theme::SPACE_2))
                                .when(!blank, |line| {
                                    line.child(div().flex_1().min_w_0().child(selection.line(
                                        block.id,
                                        text,
                                        Vec::new(),
                                    )))
                                })
                                .when(blank, |line| line.child(div().flex_1()))
                                .children(prompt_actions),
                        )
                        .when(!files.is_empty(), |column| {
                            column
                                .debug_selector(|| "sent-prompt-attachments".into())
                                .child(crate::attachments::Attachments::new(
                                    format!("sent-attachments-{:?}", block.id),
                                    files,
                                    preview,
                                ))
                        }),
                )
                .into_any_element()
        }
        // Fallback prose (a block with no Markdown source) reads as the
        // answer does, at C1.
        Body::Paragraph { spans } => prose_row(row, prose_size, prose_line)
            .child(prose(block.id, spans, selection))
            .into_any_element(),
        Body::Heading { spans, .. } => prose_row(row, prose_size, prose_line)
            .font_weight(theme::W_STRONG)
            .text_color(rgb(TEXT_STRONG))
            .child(prose(block.id, spans, selection))
            .into_any_element(),
        // A fallback list item: Claude's `-` marker right-aligned in the
        // Markdown lists' hang, the text `PROSE_HANG` in.
        Body::Bullet { spans } => {
            let hang = theme::reading_step(theme::PROSE_HANG, prose_size);
            prose_row(row, prose_size, prose_line)
                .relative()
                .child(
                    div()
                        .absolute()
                        .left(px(theme::GUTTER_W))
                        .top_0()
                        .w(px(hang))
                        .pr(px(theme::LIST_MARKER_GAP))
                        .text_right()
                        .text_color(rgb(TEXT_MUTED))
                        .child("-"),
                )
                .child(div().pl(px(hang)).child(prose(block.id, spans, selection)))
                .into_any_element()
        }
        // A blank thought from an older log (redacted thinking, before the
        // fold learned to drop it) draws nothing — not even its margin.
        Body::Thinking(thought) if thought.trim().is_empty() => div().into_any_element(),
        // Reasoning is agent prose one ink down, under `∴`: Geist 14/22,
        // `TEXT_MUTED`, never italic. A short thought shows whole; a long
        // one is its first line with the rest disclosed.
        Body::Thinking(thought) => {
            let (summary, details) = reasoning_text(thought);
            let mark = icon(icons::REASONING, theme::GLYPH_BOX, TEXT_FAINT);
            let reasoning = div()
                .font_family(theme::FONT_UI)
                .text_size(px(prose_size))
                .line_height(px(prose_line))
                .text_color(rgb(TEXT_MUTED));
            let Some(details) = details else {
                // Nothing more was supplied. Keep the whole short block
                // visible, wrapped and selectable without a false disclosure.
                return row
                    .child(
                        gutter_row(mark, prose_line).child(
                            reasoning.flex_1().min_w_0().child(
                                selection
                                    .markdown(block.id, thought.trim().to_owned())
                                    .muted(),
                            ),
                        ),
                    )
                    .into_any_element();
            };
            let (overlay, chevron, targeted) = disclosure_parts(disclosure);
            let header = gutter_row(mark, prose_line)
                .id(SharedString::from(format!("reasoning-row-{:?}", block.id)))
                .group(DISCLOSURE_ROW)
                .relative()
                .items_center()
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
                    div()
                        .min_w_0()
                        .pl(px(theme::GUTTER_W))
                        .child(selection.markdown(block.id, details.clone()).muted()),
                );
            }
            row.child(reasoning.child(body)).into_any_element()
        }
        // A notice: a dot and one line, cut by width at the column's edge
        // with the whole of it one hover away. Only the transcript's latest
        // notice wears the Pane's state (`signal`), and then only on its dot
        // and its lead phrase (`Bash needs approval`, `asks 1 question`):
        // the `·` is `TEXT_FAINT` and the detail `TEXT_2`. While the
        // Decision it announces is docked below, the detail is the card's
        // to say, so only the lead prints (and copies). History stays
        // neutral.
        Body::Notice(text) => {
            let docked = notice_docked(signal);
            let text = notice_text(text, docked).to_string();
            let mark = if signal == TEXT_MUTED {
                TEXT_FAINT
            } else {
                signal
            };
            let mut highlights = separators(&text);
            if signal != TEXT_MUTED {
                let lead = text.split(" \u{b7} ").next().unwrap_or_default().len();
                highlights.insert(
                    0,
                    (
                        0..lead,
                        HighlightStyle {
                            color: Some(rgb(signal).into()),
                            ..Default::default()
                        },
                    ),
                );
            }
            let text = SharedString::from(text);
            row.child(
                gutter_row(
                    components::status_dot(mark).size(px(theme::TOOL_DOT)),
                    theme::LH_UI,
                )
                .text_size(px(theme::FS_UI))
                .line_height(px(theme::LH_UI))
                .text_color(rgb(TEXT_2))
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
            .child(
                result_line(TEXT_MUTED).child(div().flex_1().min_w_0().child(selection.line(
                    block.id,
                    text.clone(),
                    separators(text),
                ))),
            )
            .into_any_element(),
        Body::TurnEnd(end) => row
            .child(turn_end(block.id, end, selection))
            .into_any_element(),
        // Code keeps literal indentation and highlighting without a
        // separate language header or raised container.
        Body::Code {
            language: _,
            source,
            tokens,
        } => row
            .pl(px(theme::GUTTER_W))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .font_family(theme::FONT_CODE)
                    .text_size(px(theme::FS_UI))
                    .line_height(px(theme::LH_CODE))
                    .text_color(rgb(TEXT_2))
                    .children(code_lines(
                        block.id,
                        source,
                        code(source, tokens.as_deref()),
                        selection,
                    )),
            )
            .into_any_element(),
        Body::Tool(tool) => render_tool(
            row, block.id, tool, selection, timings, expanded, disclosure, false,
        ),
    }
}

/// The `group` every disclosable row names, so its trailing chevron shows
/// under the pointer anywhere on the row.
pub(crate) const DISCLOSURE_ROW: &str = "disclosure-row";

/// A transcript row: its mark in the gutter, centred on a `first_line` box,
/// and its text at C1 (the caller's next child, `flex_1 min_w_0`).
fn gutter_row(mark: impl IntoElement, first_line: f32) -> Div {
    div()
        .flex()
        .items_start()
        .w_full()
        .min_w_0()
        .child(components::gutter(mark, first_line))
}

/// Fallback prose at C1, in the answer's face and size.
fn prose_row(row: Div, size: f32, line: f32) -> Div {
    row.pl(px(theme::GUTTER_W))
        .font_family(theme::FONT_UI)
        .text_size(px(size))
        .line_height(px(line))
        .text_color(rgb(TEXT))
}

/// The `⎿` elbow, painted in a `GLYPH_BOX`-wide cell one `line_box` tall:
/// its stem runs from the top of the row box to the first line's centre and
/// turns along it, so it hangs under the call name above whatever the font.
fn elbow(line_box: f32) -> Div {
    div()
        .relative()
        .flex_shrink_0()
        .w(px(theme::GLYPH_BOX))
        .h(px(line_box))
        .child(
            div()
                .absolute()
                .left(px(theme::ELBOW_STEM_X))
                .top_0()
                .w(px(theme::ELBOW_ARM))
                .h(px(line_box / 2.))
                .border_l_1()
                .border_b_1()
                .border_color(rgb(TEXT_FAINT)),
        )
}

/// A row that hangs under the one above it: the elbow at C1, its text at
/// C2. The caller adds the text as the next child.
pub(crate) fn result_line(ink: u32) -> Div {
    elbow_row(theme::LH_UI)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(ink))
}

fn elbow_row(line_box: f32) -> Div {
    div()
        .flex()
        .items_start()
        .gap(px(theme::GUTTER_GAP))
        .w_full()
        .min_w_0()
        .pl(px(theme::GUTTER_W))
        .child(elbow(line_box))
}

/// What a row of output omitted, under the output it belongs to.
pub(crate) fn omitted_line(bytes: usize) -> Div {
    result_line(TEXT_MUTED).child(div().min_w_0().truncate().child(SharedString::from(format!(
        "… {} not kept",
        text::byte_size(bytes)
    ))))
}

/// How a turn ended. A completed turn is a quiet stamp at C1 —
/// `Worked for 38s · 8:53 pm`, metadata ink, no mark. An interrupted or
/// failed one hangs under the turn's last row in the failure-line grammar
/// (`failure_line`): `⎿ failed · 0.1s · API Error: 529 overloaded`, its lead
/// word the only coloured one — `interrupted` in `TEXT_2` (the operator did
/// it; nothing failed), `failed` in `BLOCKED` — and the provider's message
/// in the code face.
fn turn_end(block: BlockId, end: &ferrite_core::transcript::TurnEnd, selection: &TextRuns) -> Div {
    use ferrite_core::TurnOutcome;
    let text = end.text();
    let (lead, ink, message) = match &end.outcome {
        TurnOutcome::Completed => {
            return components::tabular(
                div()
                    .debug_selector(|| "turn-stamp".into())
                    .pl(px(theme::GUTTER_W))
                    .text_size(px(theme::FS_SM))
                    .line_height(px(theme::LH_META))
                    .text_color(rgb(TEXT_MUTED))
                    .child(selection.line(block, text.clone(), separators(&text))),
            );
        }
        TurnOutcome::Interrupted => (theme::words::INTERRUPTED, TEXT_2, None),
        TurnOutcome::Error(_) => (theme::words::FAILED, BLOCKED, turn_end_message(end)),
    };
    let (head, excerpt) = turn_end_runs(&text, message);
    failure_line(block, head, lead.len(), ink, excerpt, selection)
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

/// The `·` seams in a UI line: glyph ink, never a weight. Highlighted in
/// place so the line stays one run and copies back exactly as written.
/// Whether the notice drawn with this signal announces a Decision docked in
/// the same Pane: the live notice wears `ATTENTION` exactly while one waits.
pub(crate) fn notice_docked(signal: u32) -> bool {
    signal == ATTENTION
}

/// A notice as it reads: whole, or only its lead phrase (before the first
/// ` · `) while the Decision it announces is docked below it.
pub(crate) fn notice_text(text: &str, docked: bool) -> &str {
    match text.split_once(" \u{b7} ") {
        Some((lead, _)) if docked => lead,
        _ => text,
    }
}

fn separators(text: &str) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    text.match_indices('\u{b7}')
        .map(|(at, dot)| {
            (
                at..at + dot.len(),
                HighlightStyle {
                    color: Some(rgb(TEXT_FAINT).into()),
                    font_weight: Some(theme::W_BODY),
                    ..Default::default()
                },
            )
        })
        .collect()
}

/// The state colour the transcript's latest notice wears — the Pane's own,
/// so the line and the Pane's edge can never disagree. `TEXT_MUTED` means
/// "no state".
pub(crate) fn signal_color(status: Option<Status>) -> u32 {
    match status {
        Some(Status::Blocked) => ATTENTION,
        Some(Status::Closed) => BLOCKED,
        _ => TEXT_MUTED,
    }
}

/// A tool call's dot: state is the dot, never the name. Settled work
/// recedes (`TEXT_FAINT`), live work is a static green dot (the working
/// line is the one live thing), a failure is `BLOCKED`, a call whose result
/// never came is a hollow ring. Green never means finished.
fn tool_dot_ink(state: &ToolState) -> (u32, DotShape) {
    match state {
        ToolState::Running => (RUNNING, DotShape::Solid),
        ToolState::Ok => (TEXT_FAINT, DotShape::Solid),
        ToolState::Failed(_) => (BLOCKED, DotShape::Solid),
        ToolState::Unavailable => (TEXT_FAINT, DotShape::Ring),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DotShape {
    Solid,
    Ring,
}

fn tool_dot(tool: &ToolBlock) -> AnyElement {
    let (ink, shape) = tool_dot_ink(&tool.state);
    match shape {
        DotShape::Solid => components::status_dot(ink)
            .size(px(theme::TOOL_DOT))
            .into_any_element(),
        DotShape::Ring => components::status_ring(ink)
            .size(px(theme::TOOL_DOT))
            .into_any_element(),
    }
}

/// A call line `Name(args)`, all of it one CLI token in the code face at
/// `W_BODY` (rule 6, the operator's Q1): the name in `TEXT`, the parens and
/// arguments in the line's `TEXT_MUTED`, one selectable line that copies
/// back exactly as it reads.
fn call_highlights(tool: &ToolBlock) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    let name = tool.name.len().min(text::tool_label(tool).len());
    vec![(
        0..name,
        HighlightStyle {
            color: Some(rgb(TEXT).into()),
            ..Default::default()
        },
    )]
}

/// A call's time in its trail: a live call ticks whole seconds from `1s`
/// (`progress::live_seconds`, repainted at 1Hz by the transcript's second
/// clock); a settled one freezes at its whole seconds, shown only from one
/// second up (`DURATION_MIN_MS`) — below that it is noise.
fn trail_duration(
    tool: &ToolBlock,
    timings: Option<&HashMap<String, ToolTiming>>,
) -> Option<String> {
    match timings.and_then(|map| map.get(&tool.call))? {
        ToolTiming::Running(started) => {
            let elapsed = started.elapsed();
            (elapsed.as_secs() >= 1).then(|| ferrite_core::progress::live_seconds(elapsed))
        }
        ToolTiming::Done(total) => (total.as_millis() >= theme::DURATION_MIN_MS)
            .then(|| ferrite_core::progress::settled_duration_label(*total)),
    }
}

/// The word a settled edit's result says, moved into its trail when a diff
/// card follows (`applied`): the card shows what changed, the trail says it
/// landed, and no `⎿ applied` row repeats it.
fn trail_result(tool: &ToolBlock) -> Option<&str> {
    (tool.state == ToolState::Ok && !tool.diffs.is_empty())
        .then_some(tool.result_line.as_deref())
        .flatten()
}

/// One failure-line grammar, for a failed call and a failed or interrupted
/// turn alike: `⎿ failed · 0.1s · API Error: 529 overloaded`. The head —
/// the lead word in its state ink (`BLOCKED` failed, `TEXT_2` interrupted),
/// `·` seams in `TEXT_FAINT`, the duration `TEXT_MUTED` — is Geist, one
/// selectable run with its trailing seam; the excerpt the machine printed is
/// its own run in the code face, `TEXT_MUTED`, soft-wrapping and never cut.
/// The runs sit a word space apart, so a copy reads the line as printed.
fn failure_line(
    block: BlockId,
    head: String,
    lead: usize,
    lead_ink: u32,
    excerpt: Option<String>,
    selection: &TextRuns,
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
    result_line(TEXT_MUTED).child(
        div()
            .flex()
            .items_start()
            .flex_1()
            .min_w_0()
            .child(components::tabular(
                div()
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .font_family(theme::FONT_UI)
                    .child(selection.line(block, head, highlights)),
            ))
            .when_some(excerpt, |line, excerpt| {
                line.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .font_family(theme::FONT_CODE)
                        .text_size(px(theme::FS_UI))
                        .line_height(px(theme::LH_UI))
                        .child(selection.line(block, excerpt, Vec::new())),
                )
            }),
    )
}

/// A failed call's head run: `failed · `, its seam carried so the excerpt
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

/// A tool call: `● Name(args)` with its trail hard right — `applied · +N −M`
/// when a diff card follows, then the time (ticking while live, frozen when
/// done) — and what it produced hanging under it on elbows. Nothing on the
/// row is a pill, a bold weight or a state-coloured word except the one
/// `failed` that says so. The row has no hover ground: under the pointer
/// only its trailing chevron shows.
///
/// Expanded, it shows the input where the call line could not show it
/// whole (`$` first for a command), the output, and any structured result,
/// none of them labelled.
fn render_tool(
    row: Div,
    block: BlockId,
    tool: &ToolBlock,
    selection: &TextRuns,
    timings: Option<&HashMap<String, ToolTiming>>,
    expanded: bool,
    disclosure: Option<Disclosure>,
    in_group: bool,
) -> AnyElement {
    let has_disclosure = disclosure.is_some();
    let (overlay, chevron, targeted) = disclosure_parts(disclosure);
    let call = div()
        .min_w_0()
        .truncate()
        .font_family(theme::FONT_CODE)
        .font_weight(theme::W_BODY)
        .child(selection.line(block, text::tool_label(tool), call_highlights(tool)));
    let mut trail = components::tabular(
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(theme::WORD_GAP))
            .text_size(px(theme::FS_SM))
            .line_height(px(theme::LH_UI))
            .text_color(rgb(TEXT_MUTED)),
    );
    let mut trailing = false;
    let applied = trail_result(tool);
    if let Some(applied) = applied {
        trail = trail
            .child(
                div()
                    .debug_selector(|| "tool-trail-result".into())
                    .flex_shrink_0()
                    .child(selection.line(block, applied.to_owned(), Vec::new())),
            )
            .child(div().text_color(rgb(TEXT_FAINT)).child("\u{b7}"));
        trailing = true;
    }
    if let Some(ToolVerdict::Diff(added, removed)) = tool_verdicts(tool).into_iter().next() {
        trail = trail.child(diff_stat(added, removed));
        trailing = true;
    }
    if let Some(duration) = trail_duration(tool, timings) {
        trail = trail.child(
            div()
                .debug_selector(|| "tool-trail-duration".into())
                .when(trailing, |time| time.pl(px(theme::SPACE_1)))
                .child(SharedString::from(duration)),
        );
        trailing = true;
    }
    let line = gutter_row(tool_dot(tool), theme::LH_UI)
        .id(SharedString::from(format!("tool-row-{}", tool.call)))
        .relative()
        .items_center()
        .gap_0()
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(TEXT_MUTED))
        .when(has_disclosure, |line| line.group(DISCLOSURE_ROW))
        .child(call)
        .children(chevron)
        .child(div().flex_1())
        .when(trailing, |line| line.child(trail.pl(px(theme::SPACE_3))))
        .children(overlay);
    let line = Disclosure::ground(targeted, line);
    let mut card = gpui::component::collapsible::Collapsible::new()
        .w_full()
        .open(expanded)
        .child(line);
    if expanded {
        let mut details = div().flex().flex_col().min_w_0().gap(px(theme::GAP_ROW));
        let mut first = true;
        let mut part = |details: Div, name: &str, text: &str, ink: u32, command: bool| {
            let block = output_block(block, name, text, ink, command, first, selection);
            first = false;
            details.child(block)
        };
        if text::shows_input(tool) {
            details = part(
                details,
                "command",
                &tool.summary,
                TEXT_MUTED,
                ferrite_core::docview::is_command_run(&tool.name),
            );
        }
        if let Some(output) = text::disclosed_output(tool) {
            // Ordinary output stays neutral even when a command failed: the
            // dot and the one `failed` word carry the failure.
            details = part(details, "result", &output.text, TEXT_MUTED, false);
            if output.omitted_bytes > 0 {
                details = details.child(omitted_line(output.omitted_bytes));
            }
        }
        if let Some(structured) = tool.structured_output() {
            details = part(details, "details", &structured.text, TEXT_MUTED, false);
            if structured.omitted_bytes > 0 {
                details = details.child(omitted_line(structured.omitted_bytes));
            }
        }
        card = card.content(details);
    } else if !text::redundant_test_result(tool)
        && applied.is_none()
        && (!in_group || matches!(tool.state, ToolState::Failed(_)))
    {
        if let Some(line) = &tool.result_line {
            let (ink, face) = result_ink(&tool.state);
            card =
                card.child(result_line(ink).font_family(face).child(
                    div().min_w_0().truncate().child(selection.line(
                        block,
                        line.clone(),
                        Vec::new(),
                    )),
                ));
        }
    }
    if tool.state == ToolState::Unavailable {
        card = card.child(result_line(TEXT_MUTED).child(text::NO_RESULT));
    }
    if !expanded {
        if let Some(excerpt) = failed_excerpt(tool) {
            card = card.child(failure_line(
                block,
                failed_head(),
                theme::words::FAILED.len(),
                BLOCKED,
                Some(excerpt.to_owned()),
                selection,
            ));
        }
    }
    if expanded || !in_group {
        for diff in &tool.diffs {
            card = card.child(render_diff(block, diff, selection));
        }
    }
    row.child(card).into_any_element()
}

/// A run of tool calls as one quiet line: the core's own summary, one
/// `TEXT_MUTED` run with tabular figures, ` · N failed` the only state ink,
/// the worst-state dot in the gutter and the chevron trailing. While a
/// member runs the running call hangs under the line; failed members stay
/// previewed under a collapsed group (ADR 0003). Expanded, every member is
/// a tool row on the lone row's axes, a row step under the summary.
pub(crate) fn render_tool_activity_with<S, C>(
    activity: ToolActivity<'_>,
    selection: &TextRuns,
    timings: Option<&HashMap<String, ToolTiming>>,
    expanded: bool,
    disclosure: Option<Disclosure>,
    state: S,
    mut control: C,
) -> AnyElement
where
    S: Fn(&DisclosureId) -> DisclosureState,
    C: FnMut(&DisclosureId) -> Option<Disclosure>,
{
    let (overlay, chevron, targeted) = disclosure_parts(disclosure);
    let call = activity.leader().call.clone();
    let label = text::activity_label(&activity);
    let highlights = separators(&label);
    // The gutter holds the group's worst-state dot; the counts climb while
    // the run is live, so its digits are tabular and the words after them
    // hold still.
    let mut header = components::tabular(gutter_row(
        components::status_dot(group_dot_ink(&activity)).size(px(theme::TOOL_DOT)),
        theme::LH_UI,
    ))
    .id(SharedString::from(format!("tool-group-row-{call}")))
    .group(DISCLOSURE_ROW)
    .relative()
    .items_center()
    .text_size(px(theme::FS_UI))
    .line_height(px(theme::LH_UI))
    .text_color(rgb(TEXT_MUTED))
    .child(div().min_w_0().truncate().child(selection.line(
        activity.blocks[0].id,
        label,
        highlights,
    )));
    if activity.failed > 0 {
        let key = call.clone();
        let failed = format!("{} {}", activity.failed, theme::words::FAILED);
        header = header.child(
            div()
                .debug_selector(move || format!("tool-group-failures-{key}"))
                .flex()
                .flex_shrink_0()
                .whitespace_nowrap()
                .child(
                    div()
                        .px(px(theme::WORD_GAP))
                        .text_color(rgb(TEXT_FAINT))
                        .child("·"),
                )
                .child(
                    div()
                        .text_color(rgb(BLOCKED))
                        .child(SharedString::from(failed)),
                ),
        );
    }
    let header = Disclosure::ground(
        targeted,
        header
            .children(chevron)
            .child(div().flex_1())
            .children(overlay),
    );
    let mut group = gpui::component::collapsible::Collapsible::new()
        .w_full()
        .open(expanded)
        .child(header);
    // Members hang a row step under the summary and under each other, on
    // the same axes as a lone tool row: one run of work, evenly spaced.
    if expanded {
        let mut details = div().flex().flex_col().min_w_0();
        for block in activity.blocks {
            let Body::Tool(tool) = &block.body else {
                continue;
            };
            details = details.child(div().pt(px(theme::GAP_ROW)).child(render_tool(
                div(),
                block.id,
                tool,
                selection,
                timings,
                state(&DisclosureId::Tool(tool.call.clone())) == DisclosureState::Expanded,
                control(&DisclosureId::Tool(tool.call.clone())),
                true,
            )));
        }
        group = group.content(details);
    } else {
        for block in activity.blocks {
            let Body::Tool(tool) = &block.body else {
                continue;
            };
            if matches!(tool.state, ToolState::Failed(_)) {
                group = group.child(div().pt(px(theme::GAP_ROW)).child(render_tool(
                    div(),
                    block.id,
                    tool,
                    selection,
                    timings,
                    state(&DisclosureId::Tool(tool.call.clone())) == DisclosureState::Expanded,
                    control(&DisclosureId::Tool(tool.call.clone())),
                    true,
                )));
            }
        }
    }
    if let Some(tool) = activity
        .blocks
        .iter()
        .rev()
        .find_map(|block| match &block.body {
            Body::Tool(tool) if tool.state == ToolState::Running => Some(tool),
            _ => None,
        })
    {
        let key = call.clone();
        let running = div().debug_selector(move || format!("tool-group-running-{key}"));
        group = group.child(if expanded {
            running.into_any_element()
        } else {
            // One line, never the raw multi-line input: a call line, all
            // mono, the name in body ink and its arguments muted.
            running
                .w_full()
                .min_w_0()
                .child({
                    let label = text::tool_label(tool);
                    let name = tool.name.len().min(label.len());
                    result_line(TEXT_MUTED).child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_family(theme::FONT_CODE)
                            .font_weight(theme::W_BODY)
                            .child(StyledText::new(label).with_highlights(vec![(
                                0..name,
                                HighlightStyle {
                                    color: Some(rgb(TEXT).into()),
                                    ..Default::default()
                                },
                            )])),
                    )
                })
                .into_any_element()
        });
    }
    div()
        .id(SharedString::from(format!("tool-group-{call}")))
        .debug_selector(move || format!("tool-group-{call}"))
        .flex_shrink_0()
        .w_full()
        .child(group)
        .into_any_element()
}

/// A group's gutter dot, inked by its worst member: `BLOCKED` if any call
/// failed, `RUNNING` while one is live, otherwise the settled `TEXT_FAINT`.
fn group_dot_ink(activity: &ToolActivity<'_>) -> u32 {
    let states = activity
        .blocks
        .iter()
        .filter_map(|block| match &block.body {
            Body::Tool(tool) => Some(&tool.state),
            _ => None,
        });
    let mut ink = TEXT_FAINT;
    for state in states {
        match state {
            ToolState::Failed(_) => return BLOCKED,
            ToolState::Running => ink = RUNNING,
            _ => {}
        }
    }
    ink
}

/// The compact row never lays out hard line breaks. The original command
/// stays in the ToolBlock and becomes selectable in the disclosed details.
fn tool_summary_line(tool: &ToolBlock) -> std::borrow::Cow<'_, str> {
    let summary = tool.title.as_deref().unwrap_or(&tool.summary);
    if summary.contains(['\n', '\r']) {
        let first = summary
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("");
        format!("{} …", first.trim()).into()
    } else {
        summary.into()
    }
}

/// A compact result's ink and face: every result line is `TEXT_MUTED` —
/// the dot and the one `failed` word carry the state — and a failed call's
/// line is the machine's own words, in the code face.
fn result_ink(state: &ToolState) -> (u32, &'static str) {
    match state {
        ToolState::Failed(_) => (TEXT_MUTED, theme::FONT_CODE),
        _ => (TEXT_MUTED, theme::FONT_UI),
    }
}

/// A disclosed block of text — a command (`$` first when it is one),
/// output, a structured result. Only the first part of a disclosure hangs
/// on the `⎿` elbow; later parts sit on the same text column (C2) a row
/// step apart. Text draws inline, keeping its whitespace; text past
/// `OUTPUT_INLINE_BYTES` scrolls in one bounded, selectable native control
/// `OUTPUT_MAX_LINES` high, and `… +N lines` under it says how much is out
/// of view.
pub(crate) fn output_block(
    block: BlockId,
    part: &str,
    text: &str,
    ink: u32,
    command: bool,
    first: bool,
    selection: &TextRuns,
) -> Div {
    // Output is machine text: the code face, like the call's arguments.
    let rows = if first {
        elbow_row(theme::LH_CODE)
    } else {
        div()
            .flex()
            .items_start()
            .gap(px(theme::GUTTER_GAP))
            .w_full()
            .min_w_0()
            .pl(px(theme::GUTTER_W + theme::ELBOW_INDENT))
    }
    .font_family(theme::FONT_CODE)
    .text_size(px(theme::FS_UI))
    .line_height(px(theme::LH_CODE))
    .text_color(rgb(ink))
    .when(command, |rows| {
        rows.child(div().flex_shrink_0().text_color(rgb(TEXT_FAINT)).child("$"))
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
                div()
                    .pl(px(theme::GUTTER_W + theme::ELBOW_INDENT))
                    .text_size(px(theme::FS_SM))
                    .line_height(px(theme::LH_META))
                    .text_color(rgb(TEXT_MUTED))
                    .child(SharedString::from(format!("… +{hidden} lines"))),
            );
    }
    rows.child(
        div()
            .flex_1()
            .min_w_0()
            .child(selection.line(block, text.to_string(), Vec::new())),
    )
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

/// A disclosable row's parts, built by the transcript for the row renderer:
/// the click target over the whole row, the chevron laid out after the
/// row's label, and whether the keyboard targets the row.
pub struct Disclosure {
    /// An overlay over the whole row, so the label and the trail toggle it
    /// too. The row adds it last.
    pub overlay: AnyElement,
    /// The trailing chevron (`disclosure_chevron`), `SPACE_1` after the
    /// label.
    pub chevron: AnyElement,
    /// Keyboard cycling has landed on this row: it wears the `HOVER` ground
    /// (at `R_CHIP`) so the operator sees which row Enter will toggle. The
    /// pointer never sets it, and no other row ever wears a ground.
    pub targeted: bool,
}

impl Disclosure {
    /// The row's keyboard-target ground, and its selector.
    fn ground(targeted: bool, row: Stateful<Div>) -> Stateful<Div> {
        row.when(targeted, |row| {
            row.bg(rgb(HOVER))
                .rounded(px(theme::R_CHIP))
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
        }) => (Some(overlay), Some(chevron), targeted),
        None => (None, None, false),
    }
}

/// A disclosure row's click target: an overlay over the whole row, so the
/// label and the trail toggle it too. The row keeps its own mark in the
/// gutter (a tool's dot, a group's worst-state dot, reasoning's `∴`); the
/// disclosure is the trailing chevron (`disclosure_chevron`). Nothing
/// grounds the row under the pointer.
pub fn tool_disclosure_control(
    call: &DisclosureId,
    expanded: bool,
    targeted: bool,
    focus: &FocusHandle,
) -> Div {
    let tooltip = match (call, expanded) {
        (DisclosureId::Reasoning(_), false) => "Show reasoning",
        (DisclosureId::Reasoning(_), true) => "Hide reasoning",
        (DisclosureId::Group(_), false) => "Show tool calls",
        (DisclosureId::Group(_), true) => "Hide tool calls",
        (DisclosureId::TurnDiff(_), false) => "Show turn changes",
        (DisclosureId::TurnDiff(_), true) => "Hide turn changes",
        (_, false) => "Show tool details",
        (_, true) => "Hide tool details",
    };
    // The whole row is the target; its gutter keeps the named
    // `TOOL_DISCLOSURE_HIT` box (with the tooltip) where the row's mark
    // hangs, so the pointer finds the same target it always has.
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
                .w(px(theme::TOOL_DISCLOSURE_HIT))
                .h(px(theme::TOOL_DISCLOSURE_HIT))
                .tooltip(move |window, cx| {
                    gpui::component::tooltip::Tooltip::new(tooltip).build(window, cx)
                }),
        )
}

/// A disclosure's trailing chevron: `ICON_CHEVRON` in `TEXT_FAINT`, its box
/// always reserved so nothing moves when it shows. It shows while the
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

pub struct PromptActions {
    root: Stateful<Div>,
    block: BlockId,
}

impl PromptActions {
    pub fn on_copy(
        mut self,
        listener: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> Self {
        self.root = self.root.child(
            prompt_action("Copy prompt", icons::COPY, format!("copy-{:?}", self.block))
                .on_click(listener),
        );
        self
    }

    pub fn on_resend(
        mut self,
        listener: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> Self {
        self.root = self.root.child(
            prompt_action(
                "Resend prompt",
                icons::RESEND,
                format!("resend-{:?}", self.block),
            )
            .on_click(listener),
        );
        self
    }
}

impl IntoElement for PromptActions {
    type Element = Stateful<Div>;

    fn into_element(self) -> Self::Element {
        self.root
    }
}

/// The hover blend a sent prompt's row drives and its actions read.
pub(crate) fn prompt_hover_key(block: BlockId) -> SharedString {
    SharedString::from(format!("prompt-actions-{block:?}"))
}

/// A sent prompt's Copy and Resend: their box always reserved, blended in
/// over the pointer's 150ms fade while the pointer is on the prompt, never
/// a ground on the prompt itself.
pub fn prompt_actions(block: BlockId) -> PromptActions {
    let key = prompt_hover_key(block);
    let shown = crate::motion::hover_t(&key);
    PromptActions {
        block,
        root: div()
            .id(key)
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(theme::SPACE_0_5))
            // At rest the box is reserved and nothing is painted.
            .when(shown <= 0., |actions| actions.invisible())
            .opacity(shown),
    }
}

fn prompt_action(
    tooltip: &'static str,
    icon_key: &'static str,
    id: impl Into<gpui::ElementId>,
) -> Stateful<Div> {
    let id = id.into();
    let hover = crate::pointer::hover_key(&id);
    div()
        .id(id)
        .debug_selector(move || {
            format!(
                "prompt-action-{}",
                tooltip
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
            )
        })
        .flex()
        .items_center()
        .justify_center()
        .w(px(theme::TOOL_DISCLOSURE_HIT))
        .h(px(theme::TOOL_DISCLOSURE_HIT))
        .rounded(px(theme::R_CHIP))
        // A self-grounded control on the Pane: `HOVER` under the pointer
        // over the one blend, pressed at once.
        .hover_control(hover)
        .press_control()
        .tooltip(move |window, cx| {
            gpui::component::tooltip::Tooltip::new(tooltip).build(window, cx)
        })
        .child(icon(icon_key, theme::ICON_CHEVRON, TEXT_MUTED))
}

/// A diff card at C2 (§ transcript grammar): `RAISED`, `R_CHIP`, no file
/// header — the call above already names the file. Each row is
/// `[number][sign][code]`: the number column as wide as the largest line
/// number needs, the ASCII sign, and the code with its indentation intact.
/// Added and removed rows wear full-bleed washes; a second hunk opens under
/// a `…` row.
///
/// The code cells route through the overlay — their lines copy honestly;
/// the number and sign columns are chrome and never do (#27).
///
/// The card draws at most `HUNK_MAX_ROWS` rows and then names what it left
/// out. A patch is normally a handful of lines, but a written file's patch
/// is the whole file, and the card is a note about a change rather than the
/// change itself. The count it reports is the truth — `Diff::added` counts
/// every line, drawn or not.
fn render_diff(block: BlockId, diff: &Diff, selection: &TextRuns) -> impl IntoElement {
    let (cap, omitted) = hunk_rows(diff.hunks.iter().map(|hunk| hunk.lines.len()).sum());
    let number_w = diff_number_width(diff_max_number(diff, cap));
    let mut lines = div()
        .flex()
        .flex_col()
        .mt(px(theme::HUNK_MARGIN_T))
        // The card hangs at the tool name's x, under the `E` of `Edit(`.
        .ml(px(theme::GUTTER_W))
        .py(px(theme::HUNK_PAD_Y))
        .rounded(px(theme::R_BLOCK))
        .overflow_hidden()
        .bg(rgb(RAISED))
        .font_family(theme::FONT_CODE)
        .text_size(px(theme::FS_UI))
        // A whole-pixel line box: a fractional one rounds each row's origin
        // and height independently, and the added/removed washes can leave a
        // 1px unpainted seam between them.
        .line_height(px(theme::LH_CODE))
        .text_color(rgb(TEXT_MUTED));
    // Two inks per row: the number column is structure (`TEXT_FAINT`), the
    // sign takes its row's code ink.
    let columns = |number: SharedString, sign: &'static str, sign_color: u32| {
        div()
            .flex()
            .items_start()
            .px(px(theme::HUNK_PAD_X))
            .child(components::tabular(
                div()
                    .flex_shrink_0()
                    .w(px(number_w))
                    .text_right()
                    .text_color(rgb(TEXT_FAINT))
                    .child(number),
            ))
            .child(
                div()
                    .flex_shrink_0()
                    .ml(px(theme::DIFF_GAP))
                    .mr(px(theme::DIFF_SIGN_GAP))
                    .w(px(theme::DIFF_SIGN_W))
                    .text_color(rgb(sign_color))
                    .child(sign),
            )
    };
    let mut drawn = 0usize;
    for (index, hunk) in diff.hunks.iter().enumerate() {
        if drawn == cap {
            break;
        }
        if index > 0 {
            lines = lines.child(columns("…".into(), "", TEXT_FAINT));
        }
        let mut old = hunk.old_start;
        let mut new = hunk.new_start;
        for line in &hunk.lines {
            if drawn == cap {
                break;
            }
            drawn += 1;
            let kind = DiffKind::of(line);
            let number = match kind {
                DiffKind::Added => {
                    let n = new;
                    new += 1;
                    n
                }
                DiffKind::Removed => {
                    let n = old;
                    old += 1;
                    n
                }
                DiffKind::Context => {
                    let n = new;
                    old += 1;
                    new += 1;
                    n
                }
            };
            let DiffPaint {
                sign,
                sign_color,
                code_color,
                wash,
            } = kind.paint();
            let body = text::diff_body(line).to_owned();
            // Machine text is never cut: a long line soft-wraps inside its
            // row, the number and sign standing on its first line only and
            // the row's wash under every line of it.
            lines = lines.child(
                columns(SharedString::from(number.to_string()), sign, sign_color)
                    .when_some(wash, |row, wash| row.bg(rgba(wash)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(rgb(code_color))
                            .child(selection.line(block, body, Vec::new())),
                    ),
            );
        }
    }
    // What the cap left out, on the code column — never a silent truncation.
    if omitted > 0 {
        lines = lines.child(
            columns(SharedString::default(), "", TEXT_MUTED).child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(SharedString::from(format!("… +{omitted} lines"))),
            ),
        );
    }
    lines
}

/// How many of a diff's rows the card draws, and how many are left for
/// the omission line to account for. Split out so the arithmetic the card
/// depends on is assertable without a window.
fn hunk_rows(total: usize) -> (usize, usize) {
    let drawn = total.min(theme::HUNK_MAX_ROWS);
    (drawn, total - drawn)
}

/// The largest line number the card will draw.
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

/// The number column: as many mono cells as the largest number has digits
/// (at least two), rounded up to a whole pixel.
fn diff_number_width(max: usize) -> f32 {
    let digits = max.max(1).ilog10() as usize + 1;
    (digits.max(2) as f32 * theme::CODE_CELL).ceil()
}

/// What a unified-diff line is, read from its first byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiffKind {
    Added,
    Removed,
    Context,
}

/// A hunk row's colours: the sign column, the code, and the row's wash.
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

    /// Both CLIs sign a diff in ASCII, `+` and `-`. The code carries the
    /// colour — green added, red removed, lifted a step so a whole line stays
    /// readable on its 8% wash — the sign wears its row's code ink, and a
    /// context line is muted.
    fn paint(self) -> DiffPaint {
        match self {
            Self::Added => DiffPaint {
                sign: "+",
                sign_color: DIFF_ADDED_INK,
                code_color: DIFF_ADDED_INK,
                wash: Some(DIFF_ADDED_WASH),
            },
            Self::Removed => DiffPaint {
                sign: "-",
                sign_color: DIFF_REMOVED_INK,
                code_color: DIFF_REMOVED_INK,
                wash: Some(DIFF_REMOVED_WASH),
            },
            Self::Context => DiffPaint {
                sign: "",
                sign_color: TEXT_MUTED,
                code_color: TEXT_MUTED,
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
        // Inline code keeps its own ink without adding a chip to prose.
        Style::Code => Some(HighlightStyle {
            color: Some(rgb(INLINE_CODE_INK).into()),
            ..Default::default()
        }),
        // `strong` (§E.5): weight 600 in `--text-strong`.
        Style::Bold => Some(HighlightStyle {
            color: Some(rgb(TEXT_STRONG).into()),
            font_weight: Some(W_STRONG),
            ..Default::default()
        }),
        // `a` (§E.6): underlined 1px. The prototype sets it in `--text`
        // over a `--sep` rule; the operator asked for a link to read as
        // one, so ink and underline are the same blue. Inert — paths
        // render, nothing opens.
        Style::Link => Some(HighlightStyle {
            color: Some(rgb(LINK_INK).into()),
            underline: Some(gpui::UnderlineStyle {
                thickness: px(1.),
                color: Some(rgb(LINK_INK).into()),
                wavy: false,
            }),
            ..Default::default()
        }),
    }
}

/// A prose Block's text (§E.1/E.2/E.3): one wrapping run, so a sentence
/// breaks where the prototype's does.
///
/// Inline `code` is the one span that cannot live inside that run — §E.4
/// gives it `padding: 1px 4px` and a 3px radius, and a gpui highlight has
/// neither. A Block that carries one is composed of flex pieces instead,
/// with the chip as its own padded element; a Block that does not — nearly
/// every one — keeps the single run untouched.
fn prose(block: BlockId, spans: &[Span], selection: &TextRuns) -> AnyElement {
    // One shaped run for the whole paragraph, whatever it holds. Inline
    // code used to be its own chip element in a wrapping flex row, which
    // gave it padding and corners — and broke every paragraph that held
    // one: a long text piece became a wrapping box of its own, and the
    // pieces staggered down the column. The code wash is a highlight now;
    // the text wraps as text.
    let (text, highlights) = inline(spans);
    selection.line(block, text, highlights).into_any_element()
}

/// A fenced block's rows, one element per hard line, each carrying that
/// line's slice of the block's highlight runs.
///
/// The indent is drawn as width, not as glyphs. One element per line is not
/// enough on its own: gpui shapes a run of leading U+0020 to zero advance —
/// and U+00A0 in its place shapes to zero too — so every inner line landed
/// flush left however the string was cut. So the leading spaces go into
/// their own box, sized from the code advance (Geist Mono is 600/1000
/// em, `CODE_ADVANCE`), and the code follows in a second run. The spaces are
/// still emitted as a text fragment inside that box, so a copy takes the
/// line back whole; only its painting is guaranteed by the width.
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
    /// The Composer writes down one key hint, in its placeholder: a
    /// prediction's accept key (the one thing about it the box cannot
    /// show), or at rest the `/` menu where everything else lives. A
    /// Decision's line and a dead Session's carry none.
    #[test]
    fn the_placeholder_carries_the_one_key_hint() {
        let live = Transcript::default();
        assert_eq!(
            placeholder(false, false, Some(&live), None).hint,
            Some(("/", "for commands"))
        );
        assert_eq!(placeholder(true, false, Some(&live), None).hint, None);
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
            placeholder(false, false, Some(&answered), Some("Run the tests")).hint,
            Some(("\u{21e5}", "accept"))
        );
        assert_eq!(
            placeholder(false, true, None, Some("Run the tests")).hint,
            Some(("/", "for commands")),
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
                .child(working_line(&self.0, false, false, false, false))
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
                    old_lines: 3,
                    new_start: 1,
                    new_lines: 3,
                    lines: vec![" alpha".into(), "-bravo".into(), "+delta".into()],
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
            // Per-block fixtures show every call; individual tool details
            // retain their own collapsed/expanded state.
            expanded.extend(self.blocks.iter().filter_map(|block| match &block.body {
                Body::Tool(tool) => Some(DisclosureId::Group(tool.call.clone())),
                _ => None,
            }));
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
                turn_diff: None,
                signal_status: Some(Status::Idle),
                timings: HashMap::new(),
                focused: true,
                reading_size: Default::default(),
                selection_scope: gpui::base::TextSelectionScopeId::new(),
                preview: crate::attachment_preview::Preview::new(cx),
                expanded,
                target: None,
                disclosure_focus: cx.focus_handle(),
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
                    turn_diff: None,
                    signal_status: Some(Status::Idle),
                    timings: HashMap::new(),
                    focused: true,
                    reading_size: Default::default(),
                    selection_scope: gpui::base::TextSelectionScopeId::new(),
                    preview: crate::attachment_preview::Preview::new(cx),
                    expanded: HashSet::new(),
                    target: None,
                    disclosure_focus: cx.focus_handle(),
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
                    let rows = decision::approval_rows(decision)
                        .into_iter()
                        .enumerate()
                        .map(|(row_at, row)| {
                            decision::option_row(
                                ("decision-row", at * 16 + row_at),
                                decision::Row {
                                    key: row.key,
                                    label: row.label,
                                    scope: row.scope,
                                    description: None,
                                    recommended: false,
                                    selected: false,
                                    enabled: row.enabled,
                                    quiet: row.verb == decision::Verb::Deny,
                                    enter: false,
                                },
                            )
                            .into_any_element()
                        });
                    decision::card(
                        at as u64,
                        false,
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
                .children(self.decisions.iter().map(|decision| {
                    l2_decision_body(
                        decision,
                        Some(
                            decision::key_actions()
                                .child(keycap_allow(true))
                                .child(keycap_deny(true))
                                .into_any_element(),
                        ),
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
        assert_eq!(cursor(keycap_allow(true)), Some(CursorStyle::PointingHand));
        assert_eq!(cursor(keycap_deny(true)), Some(CursorStyle::PointingHand));
        assert_eq!(
            cursor(keycap_always(false)),
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
        assert!(
            !all.contains("delta") && !all.contains("bravo"),
            "a grouped tool's hidden diff must not join copied text: {all}"
        );
        // The result line registers where it renders (Edit's); Bash's was
        // kept inside its disclosure — so its count never
        // registers. The `└` elbow is chrome and never joins the run.
        assert!(all.contains("applied"), "the result line: {all}");
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
        assert!(
            !all.contains("42 passed"),
            "a redundant success tally stays in its disclosure: {all}"
        );
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
        assert_eq!(empty.todos, None);

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
        assert_eq!(card.todos, Some((3, 4)));
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
        assert_eq!(decision_place(&full), None);
        // A request naming its cwd carries it on the place line (#22 C7).
        let mut placed = decision("Bash", "gh issue close 212");
        placed.input = serde_json::json!({ "command": "gh issue close 212", "cwd": "/work/api" });
        assert_eq!(decision_place(&placed).as_deref(), Some("in /work/api"));
        // No description: the tool's name is the subject.
        let bare = decision("Write", "");
        assert_eq!(decision_subject(&bare).as_ref(), "Write");
        // No tool at all: the honest fallback, on both lines.
        let unreadable = decision("", "");
        assert_eq!(
            decision_subject(&unreadable).as_ref(),
            "unreadable permission request"
        );
        assert_eq!(decision_place(&unreadable), None);
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
        assert_eq!(
            DiffKind::Added.paint(),
            DiffPaint {
                sign: "+",
                sign_color: DIFF_ADDED_INK,
                code_color: DIFF_ADDED_INK,
                wash: Some(DIFF_ADDED_WASH),
            }
        );
        assert_eq!(
            DiffKind::Removed.paint(),
            DiffPaint {
                sign: "-",
                sign_color: DIFF_REMOVED_INK,
                code_color: DIFF_REMOVED_INK,
                wash: Some(DIFF_REMOVED_WASH),
            }
        );
        assert_eq!(
            DiffKind::Context.paint(),
            DiffPaint {
                sign: "",
                sign_color: TEXT_MUTED,
                code_color: TEXT_MUTED,
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
        assert_eq!(code.background_color, None);
        let link = span_style(Style::Link).unwrap();
        assert_eq!(link.color, Some(rgb(LINK_INK).into()));
        assert_eq!(
            link.underline.unwrap().color,
            Some(rgb(LINK_INK).into()),
            "the underline is the link's own ink, not the seam"
        );
        assert!(span_style(Style::Plain).is_none());
    }

    #[test]
    fn a_tool_row_reads_its_outcome_from_its_dot_and_keeps_its_name_neutral() {
        let failed = ToolState::Failed("boom".into());
        // Settled work recedes; live work is a static green dot (only the
        // working line moves); a failure is the blocked dot; a lost result
        // is a hollow ring. Green never means finished.
        assert_eq!(tool_dot_ink(&ToolState::Ok), (TEXT_FAINT, DotShape::Solid));
        assert_eq!(
            tool_dot_ink(&ToolState::Running),
            (RUNNING, DotShape::Solid)
        );
        assert_eq!(tool_dot_ink(&failed), (BLOCKED, DotShape::Solid));
        assert_eq!(
            tool_dot_ink(&ToolState::Unavailable),
            (TEXT_FAINT, DotShape::Ring)
        );
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
            };
            assert_eq!(text::tool_label(&tool), "Bash(cargo test)");
            let highlights = call_highlights(&tool);
            assert_eq!(
                highlights.len(),
                1,
                "one line, one face: only the name is lifted"
            );
            assert_eq!(highlights[0].0, 0..4, "only the name is lifted");
            assert_eq!(
                highlights[0].1,
                HighlightStyle {
                    color: Some(rgb(TEXT).into()),
                    ..Default::default()
                },
                "the name changes ink only: never a weight, never a face"
            );
        }
        assert_eq!(result_ink(&ToolState::Ok), (TEXT_MUTED, theme::FONT_UI));
        assert_eq!(
            result_ink(&failed),
            (TEXT_MUTED, theme::FONT_CODE),
            "the failure's message is machine text; the dot and one word carry the state"
        );
    }

    #[test]
    fn a_diff_keeps_its_indentation_and_sizes_its_number_column_by_digits() {
        assert_eq!(text::diff_body("+    let x = 1;"), "    let x = 1;");
        assert_eq!(text::diff_body("-\tfoo"), "\tfoo");
        assert_eq!(text::diff_body("  indented context"), " indented context");
        assert_eq!(
            text::diff_body("\\ No newline at end of file"),
            "\\ No newline at end of file"
        );
        assert_eq!(text::diff_body(""), "");
        assert_eq!(DiffKind::Removed.paint().sign, "-", "ASCII, like both CLIs");
        let cell = theme::CODE_CELL;
        assert_eq!(
            diff_number_width(7),
            (2. * cell).ceil(),
            "never under two cells"
        );
        assert_eq!(diff_number_width(99), (2. * cell).ceil());
        assert_eq!(diff_number_width(100), (3. * cell).ceil());
        assert_eq!(diff_number_width(10_000), (5. * cell).ceil());
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
        assert_eq!(text::byte_size(1_229), "1.2 KB");
        assert_eq!(text::byte_size(3 * 1024 * 1024), "3.0 MB");
    }

    /// A turn's end leads with the lexicon's own lowercase words, and the
    /// answer's mark is structure ink.
    #[test]
    fn turn_ends_speak_the_lexicon_and_the_answer_mark_is_faint() {
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
            };
            assert_eq!(end.text(), text);
            let (head, message) = turn_end_runs(&end.text(), turn_end_message(&end));
            assert_eq!(
                format!("{head}{}", message.unwrap_or_default()),
                text,
                "the two runs copy back as the line"
            );
        }
        assert_eq!(crate::transcript::ANSWER_MARK_INK, TEXT_FAINT);
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

    /// The Pane's edge says one thing, and state beats focus: a focused
    /// Pane with a Decision keeps its amber edge (focus is then the inset
    /// ring), and a calm unfocused Pane rests on the hairline.
    #[test]
    fn pane_edge_ranks_state_over_focus() {
        assert_eq!(PaneEdge::of(true, true, true, false), PaneEdge::Blocked);
        assert_eq!(PaneEdge::of(false, true, true, false), PaneEdge::Blocked);
        assert_eq!(PaneEdge::of(true, true, false, false), PaneEdge::Attention);
        assert_eq!(PaneEdge::of(false, true, false, false), PaneEdge::Attention);
        assert_eq!(PaneEdge::of(true, false, false, false), PaneEdge::Focused);
        assert_eq!(PaneEdge::of(false, false, false, false), PaneEdge::Rest);
        assert_eq!(PaneEdge::Rest.ink(), rgba(HAIRLINE).into());
        assert_eq!(PaneEdge::Focused.ink(), rgb(FOCUS_RING).into());
        // State edges are alpha on a board (C6); only the answer target is
        // full ochre, and only a waiting cell can be it.
        assert_eq!(PaneEdge::Attention.ink(), rgba(ATTENTION_EDGE).into());
        assert_eq!(PaneEdge::Blocked.ink(), rgba(BLOCKED_EDGE).into());
        assert_eq!(PaneEdge::AnswerTarget.ink(), rgb(ATTENTION).into());
        assert_eq!(
            PaneEdge::of(false, true, false, false).answer_target(true),
            PaneEdge::AnswerTarget
        );
        assert_eq!(
            PaneEdge::of(false, false, true, false).answer_target(true),
            PaneEdge::Blocked
        );
        assert_eq!(
            PaneEdge::of(false, false, false, false).answer_target(true),
            PaneEdge::Rest
        );
        // Solo: whatever the state, the frame is the hairline or focus.
        for focused in [false, true] {
            for attention in [false, true] {
                for blocked in [false, true] {
                    for target in [false, true] {
                        let edge =
                            PaneEdge::of(focused, attention, blocked, true).answer_target(target);
                        assert!(
                            matches!(edge, PaneEdge::Rest | PaneEdge::Focused),
                            "solo {focused} {attention} {blocked} {target}: {edge:?}"
                        );
                    }
                }
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
                if let Some(word) = wall_signal(state, kind, &failing, None) {
                    said.push(word.text());
                }
            }
        }
        assert_eq!(
            wall_signal(WallState::Idle, None, &failing, None),
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
        // The slot's value words: a raw mode id never renders.
        let mode = permission_mode_label("acceptEdits", &[]).unwrap();
        assert_eq!(HeadSlot::Mode(mode).text(), "accept edits");
        assert_eq!(HeadSlot::Context(84).text(), "ctx 84%");
        assert_eq!(HeadSlot::Context(84).ink(), ATTENTION);
        assert_eq!(HeadSlot::Context(32).ink(), TEXT_MUTED);
        assert_eq!(HeadSlot::Working("12s".into()).text(), "working 12s");
    }

    /// The L2 tail's prose is set at the small prose size — never under
    /// 12.5 — and a heading never above the label weight.
    #[test]
    fn the_l2_tail_sets_prose_at_the_small_prose_size() {
        let mut prose = tail_prose("Fixed it.".into(), false);
        assert_eq!(
            prose.style().text.font_size,
            Some(px(theme::FS_PROSE_SM).into())
        );
        assert_eq!(
            prose.style().text.line_height,
            Some(px(theme::LH_PROSE_SM).into())
        );
        assert!(theme::FS_PROSE_SM >= 12.5);
        let mut heading = tail_prose("Result".into(), true);
        assert_eq!(heading.style().text.font_weight, Some(theme::W_LABEL));
    }

    /// What the tail shows is what it copies: one spelling per Body — a
    /// completed turn leaves no row, a docked Decision's notice keeps only
    /// its lead phrase, a call reads as L1 spells it.
    #[test]
    fn the_l2_tail_text_is_its_visible_text() {
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
        });
        assert_eq!(tail_text(&done, false), None, "no `Worked for` row");
        let stopped = TurnEnd {
            outcome: TurnOutcome::Interrupted,
            elapsed_ms: Some(4_100),
            completed_at: None,
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

    /// §D.7: the idle line says what the Pane is waiting on — a Decision, a
    /// live Thread, or a closed Session — and, once a prediction lands, shows
    /// it verbatim as the operator's own next line. It never names the Thread
    /// and never repeats the hints beside it.
    #[test]
    fn the_placeholder_says_what_the_pane_is_waiting_on() {
        let live = Transcript::default();
        assert_eq!(
            placeholder(false, false, Some(&live), None).rungs(),
            [
                "Steer this Thread\u{2026} \u{b7} / for commands",
                "Steer this Thread\u{2026}",
                "Steer\u{2026}",
            ]
        );
        assert_eq!(
            placeholder(true, false, Some(&live), None).rungs(),
            ["Steer this Thread\u{2026}", "Steer\u{2026}"]
        );

        let mut closed = Transcript::default();
        closed.apply(Input::Event(SessionEvent::Closed {
            reason: "the CLI exited".into(),
        }));
        assert_eq!(
            placeholder(false, false, Some(&closed), None).rungs(),
            ["Revive and continue\u{2026}", "Revive\u{2026}"]
        );
        // A draft: what the first prompt does, then the menu pointer.
        assert_eq!(
            placeholder(false, true, None, None).rungs(),
            [
                "Start a thread\u{2026} \u{b7} / for commands",
                "Start a thread\u{2026}",
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
        // A Decision and a dead Session both outrank it: with a Decision
        // docked the line keeps steering, never the prediction.
        assert_eq!(
            placeholder(true, false, Some(&answered), Some("Run the tests")).rungs(),
            ["Steer this Thread\u{2026}", "Steer\u{2026}"]
        );
        assert_eq!(
            placeholder(false, false, Some(&closed), Some("Run the tests")).rungs(),
            ["Revive and continue\u{2026}", "Revive\u{2026}"]
        );

        // No rung is ever a cut word, names a message, or points at the
        // menu anywhere but the fullest rung.
        for ghost in [
            placeholder(false, false, Some(&live), None),
            placeholder(true, false, Some(&live), None),
            placeholder(false, false, Some(&closed), None),
            placeholder(false, true, None, None),
        ] {
            for (index, rung) in ghost.rungs().iter().enumerate() {
                assert!(!rung.contains("message"), "{rung}");
                assert_eq!(
                    rung.contains("commands"),
                    index == 0 && ghost.hint.is_some(),
                    "{rung}"
                );
                assert!(!rung.contains("this\u{2026}"), "{rung}");
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
    /// The `❯` says where keys land: accent only while the line holds the
    /// keyboard — a Decision's reply line included — muted otherwise.
    #[test]
    fn the_prompt_mark_lights_only_while_the_line_holds_the_keyboard() {
        assert_eq!(prompt_ink(true), ACCENT);
        assert_eq!(prompt_ink(false), TEXT_MUTED);
    }

    /// The block's edge answers a file drop and nothing else: `ACCENT_EDGE`
    /// while files hover it, the resting edge otherwise — always 1px,
    /// always in layout.
    #[test]
    fn the_composer_edge_answers_only_a_file_drop() {
        assert_eq!(composer_edge(true), theme::ACCENT_EDGE);
        assert_eq!(composer_edge(false), theme::COMPOSER_EDGE);
    }

    /// The height budget counts exactly what the block draws around its
    /// editor rows: inset, two edges, padding, the gap and the status line
    /// (one `LH_META` line, rule 2.6.6) — and on a board, the grid's one
    /// fixed 32px line with no status row (C4).
    #[test]
    fn the_fixed_height_follows_the_composer_tokens() {
        assert_eq!(theme::COMPOSER_META_H, theme::LH_META);
        let block = 2. * theme::COMPOSER_EDGE_W
            + theme::COMPOSER_PAD_T
            + theme::COMPOSER_PAD_B
            + theme::COMPOSER_META_GAP
            + theme::LH_META;
        assert_eq!(
            composer_fixed_height(false, false),
            theme::COMPOSER_INSET_B + block
        );
        assert_eq!(
            composer_fixed_height(true, false),
            theme::COMPOSER_INSET_L2 + block
        );
        // The grid line: 5 + 20 + 5 inside two 1px edges is 32, and its one
        // row is the editor's own.
        assert_eq!(theme::COMPOSER_GRID_H, 32.);
        assert_eq!(
            theme::COMPOSER_GRID_H,
            2. * theme::COMPOSER_EDGE_W + 2. * theme::COMPOSER_GRID_PAD_Y + theme::COMPOSER_ROW_H
        );
        assert_eq!(
            composer_fixed_height(false, true),
            theme::COMPOSER_INSET_B + theme::COMPOSER_GRID_H - theme::COMPOSER_ROW_H
        );
        assert_eq!(
            composer_fixed_height(true, true),
            theme::COMPOSER_INSET_L2 + theme::COMPOSER_GRID_H - theme::COMPOSER_ROW_H
        );
        assert_eq!(
            theme::BOX_INSET_X,
            theme::COMPOSER_EDGE_W + theme::COMPOSER_PAD_X
        );
        // The L2 box's `❯` lands on the glyph column at x = 16.
        assert_eq!(
            theme::COMPOSER_INSET_L2 + theme::COMPOSER_EDGE_W + theme::COMPOSER_PAD_X_L2,
            theme::PANE_PAD_X
        );
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

    /// Usage is neutral until it runs tight: colour is state.
    #[test]
    fn usage_reads_neutral_until_it_runs_tight() {
        assert_eq!(usage_ink(0.62), TEXT_2);
        assert_eq!(usage_ink(0.79), TEXT_2);
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
