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
    canvas, deferred, div, point, pulsating_between, px, relative, rgb, rgba, Animation,
    AnimationExt, AnyElement, Context, Div, Entity, FocusHandle, FontWeight, HighlightStyle,
    PathBuilder, SharedString, Stateful, Styled, StyledText,
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
            name: SharedString::from("new thread"),
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
        self.disclosure.target = Some(call.clone());
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
    pub project_branches: &'a [(SharedString, SharedString)],
    /// Whether the Composer line is empty — what decides the idle
    /// placeholder, read where the cockpit has a `cx` to read it with.
    pub composer_empty: bool,
    /// Queue viewport derived from this Pane's actual available height.
    pub composer_queue_height: f32,
    pub history_available: bool,
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
    /// The header's `ci` mark, wired: a press opens the checks card over
    /// the Pane. `None` where there is no PR, no CI, or no room for the
    /// card — the strip then draws the mark it can draw unwired, or
    /// nothing.
    pub ci: Option<AnyElement>,
    pub activity_attention: Option<AnyElement>,
    pub activity_decisions: Option<AnyElement>,
    /// Questions without enough body space open via a fixed-header action. The
    /// action must remain reachable even when the Composer fills the body.
    pub expand_question: Option<AnyElement>,
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
    /// The failing signal, with the run's own count where it reported one:
    /// `2 failing`, else `failing`.
    pub failing: SharedString,
    /// The plan as (done, total), for the painted meter; `None` without one.
    pub todos: Option<(usize, usize)>,
    /// The working signal: the progress caption (`Thinking`, `Working`).
    pub working: SharedString,
    /// An alert cell's context: the Decision's subject, or the reason the
    /// Session closed. Empty when neither applies.
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
    let failing = match tests {
        Some(Tests::Failed { count: Some(count) }) => {
            SharedString::from(format!("{count} failing"))
        }
        _ => SharedString::from("failing"),
    };
    WallCard {
        tests_failing: matches!(tests, Some(Tests::Failed { .. })),
        failing,
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
    pub needs_queue: bool,
    pub composer_empty: bool,
    pub history_available: bool,
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
        project_branches,
        composer_empty,
        composer_queue_height,
        history_available,
        focused,
        attention,
        wall,
        reduce_motion,
        editing,
        drop_target,
    } = facts;
    let pulse = attention.then(|| view.thread()).flatten();
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
        ci,
        activity_attention,
        mut activity_decisions,
        expand_question,
        question_measurement,
        child_footer,
    } = wiring;
    let has_activity_decisions = activity_decisions.is_some() || expand_question.is_some();
    let subject = thread.and_then(|thread| thread.activity().subject(&view.selected));
    let transcript = subject.as_ref().map(|subject| subject.transcript());
    let decision = if view.is_main() {
        thread.and_then(|thread| thread.pending())
    } else {
        None
    };
    let workspace = thread.and_then(|thread| thread.workspace());
    let timings = subject.as_ref().map(|subject| subject.timings());
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
    let attention_pending =
        thread.is_some_and(|thread| !thread.activity().pending_decisions().is_empty());
    let blocked = state == WallState::Blocked;
    let alert = attention_pending || blocked;
    let edge = PaneEdge::of(focused, attention_pending, blocked);
    let mut shell = pane_shell(edge.ink()).when(edge == PaneEdge::Rest, |shell| shell.hover_edge());
    let mut activity_attention = activity_attention;
    if level != Level::Transcript {
        if let Some(attention) = activity_attention.take() {
            shell = shell.relative().child(
                div()
                    .absolute()
                    .top(px(theme::SPACE_1))
                    .right(px(theme::SPACE_1_5))
                    .child(attention),
            );
        }
    }
    let frame = |shell: Div| pane_frame(shell, focused, alert, pulse, reduce_motion);

    // Far enough away, a Pane is one signal: no header, no transcript,
    // nothing that stops reading at a glance.
    if level == Level::Wall {
        return frame(
            shell
                .child(wall_cell(view, wall, state, focused, title))
                .children(drop_target.then(crate::prompt_drop::sheet)),
        );
    }

    // Requests occupy the space below this Thread's header and above its
    // actual Composer. Keeping the overlay in that flex slot makes it follow
    // multiline drafts and split resizing without escaping into other Panes.
    // (At L2 the cell hangs them itself.)
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
        // Submission guidance follows the same predicate as Submit, including
        // startup and held prompts, independently of this Pane's focus.
        needs_queue: thread.is_some_and(|thread| thread.needs_queue()),
        composer_empty,
        history_available,
        permission_mode: thread.and_then(|thread| {
            thread
                .permission_mode()
                .map(|mode| permission_mode_label(mode, &thread.permission_modes()))
        }),
        suggestion: thread.and_then(|thread| thread.suggestion()),
        received_reasoning_visible,
        reduce_motion,
        editing,
        drop_target,
        starting: thread.is_some_and(|thread| thread.starting()),
        unread: attention,
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
    };

    if level == Level::Instruments {
        let composer = l2_composer(&mut cx);
        let decide = cx.decide.take();
        let activity_decisions = cx.activity_decisions.take();
        return frame(shell.child(l2_cell(
            view,
            transcript,
            decision,
            workspace,
            branch.as_ref(),
            state,
            timings,
            decide,
            title,
            composer,
            activity_decisions.filter(|_| expand_question.is_none()),
            expand_question,
            reduce_motion,
            drop_target,
        )));
    }

    let mut pane = shell.child(pane_head(
        view,
        PaneHeadState {
            branch: branch.as_ref(),
            checkout,
            project_branches,
            status,
            title,
            agents,
            ci,
            attention: activity_attention,
            action: expand_question,
            tasks: l1_tasks(&mut cx),
            unfocused: !focused,
        },
    ));
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
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
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
                    .pb(px(theme::SPACE_1))
                    .bg(rgb(PANE))
                    .child(components::reading_column(
                        div().px(px(theme::BOX_INSET_X)).child(line),
                    )),
            )
            .into_any_element(),
    )
}

/// The working line's shape while a Session starts: the Ferrite mark (still
/// under reduced motion) and `Starting session`.
fn starting_line(reduce_motion: bool) -> Div {
    let mark = working_mark(reduce_motion);
    div()
        .debug_selector(|| "transcript-starting".into())
        .flex()
        .items_center()
        .w_full()
        .min_w_0()
        .font_family(theme::FONT_MONO)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(TEXT_2))
        .child(components::gutter(mark, theme::LH_UI))
        .child(div().min_w_0().truncate().child("Starting session"))
}

/// WP-C · the tasks strip under the head.
fn l1_tasks(cx: &mut PaneCtx) -> Option<AnyElement> {
    // The tasks meter now rides the head's right cluster (one head row).
    let transcript = cx.transcript?;
    let todos = transcript.todos()?;
    Some(
        tasks_strip(
            cx.view.thread().map_or(0, ThreadId::get),
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
    let alert = composer_alert(cx);
    Some(
        composer_region(
            cx.view,
            Some(transcript),
            ComposerStack {
                compact: false,
                decision: cx.decision,
                queued: std::mem::take(&mut cx.queued),
                queue_height: cx.queue_height,
                needs_queue: cx.needs_queue,
                empty: cx.composer_empty,
                attachments: cx.attachments.take(),
                actions: cx.composer_actions.take(),
                background: cx.background.take(),
                history_available: cx.history_available,
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
                alert,
            },
        )
        .into_any_element(),
    )
}

/// WP-D · the L2 cell's compact Composer, or a Subagent's footer. An L2 cell
/// keeps its Composer: the operator types into a small Pane as into a big
/// one. No menu, picker or band at this size; the keys still work.
fn l2_composer(cx: &mut PaneCtx) -> Option<Div> {
    let alert = composer_alert(cx);
    let composer = cx
        .transcript
        .filter(|_| cx.view.is_main())
        .map(|transcript| {
            composer_region(
                cx.view,
                Some(transcript),
                ComposerStack {
                    compact: true,
                    decision: cx.decision,
                    queued: std::mem::take(&mut cx.queued),
                    queue_height: cx.queue_height,
                    needs_queue: cx.needs_queue,
                    empty: cx.composer_empty,
                    attachments: cx.attachments.take(),
                    actions: cx.composer_actions.take(),
                    background: cx.background.take(),
                    history_available: cx.history_available,
                    menu: None,
                    mode: cx.permission_mode.as_deref(),
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
                    alert,
                },
            )
        });
    composer.or_else(|| cx.child_footer.take().map(|footer| div().child(footer)))
}

/// The Pane box (§D.1): `--pane` ground, 8px radius, and a 1px border that
/// is **always in layout** — transparent at rest, amber on a Decision, red
/// when blocked — so a state change reflows nothing. `overflow: hidden`
/// clips the children to the radius. The mono family is declared once
/// here: everything inside a Pane inherits it, everything outside keeps
/// the system sans the root declares.
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
        .font_family(theme::FONT_MONO)
        .overflow_hidden()
}

/// The Pane, plus its neutral focus ring. At rest it follows the border;
/// on an alert Pane it moves two pixels inward, leaving the amber/red edge
/// intact. Both are overlays, so focus never changes layout or board gaps.
/// A ring painted inside the shell's
/// `overflow_hidden()` would be clipped away, so it still lives in a
/// non-clipping wrapper as an absolute overlay. `pulse` names a Thread
/// that finished while the operator looked elsewhere: the same ring
/// breathes until they land on it.
fn focus_wrapper(shell: Div, focused: bool, pulse: Option<ThreadId>, alert: bool) -> Div {
    let ring = || {
        div()
            .absolute()
            .inset(px(if alert { theme::FOCUS_RING_W * 2. } else { 0. }))
            .rounded(px(
                theme::R_PANE - if alert { theme::FOCUS_RING_W * 2. } else { 0. }
            ))
            .border(px(theme::FOCUS_RING_W))
            .border_color(rgb(FOCUS_RING))
    };
    div()
        .relative()
        .flex()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .child(shell)
        .children(focused.then(|| ring().into_any_element()))
        // The unread ring never covers focus or a state edge: those already
        // say "look here", and louder.
        .children(pulse.filter(|_| !focused && !alert).map(|thread| {
            ring()
                .with_animation(
                    ("attention-ring", thread.get() as usize),
                    Animation::new(Duration::from_millis(theme::STATUS_PULSE_MS))
                        .repeat()
                        .with_easing(pulsating_between(theme::PULSE_MIN, 1.0)),
                    |ring, delta| ring.opacity(delta),
                )
                .into_any_element()
        }))
}

/// What a Pane's 1px edge says, by precedence: a closed Session beats a
/// Decision, a Decision beats focus, and a Pane with none of them rests on
/// the hairline. One colour — focus on an alert Pane is the inset ring
/// `pane_frame` draws, never a second edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaneEdge {
    Blocked,
    Attention,
    Focused,
    Rest,
}

impl PaneEdge {
    pub(crate) fn of(focused: bool, attention: bool, blocked: bool) -> Self {
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

    pub(crate) fn ink(self) -> gpui::Hsla {
        match self {
            PaneEdge::Blocked => rgb(BLOCKED).into(),
            PaneEdge::Attention => rgb(ATTENTION).into(),
            PaneEdge::Focused => rgb(FOCUS_RING).into(),
            PaneEdge::Rest => rgba(HAIRLINE).into(),
        }
    }
}

/// The Pane inside its non-clipping frame: the edge already says focus on a
/// calm Pane, so the frame adds only what the edge cannot — the inset focus
/// ring on a focused alert Pane (2px inside the amber/red edge, UI-21), and
/// the unread ring that breathes in `ACCENT` over a resting edge until the
/// operator lands on a Thread that finished while they looked elsewhere
/// (held still under reduced motion). Rings painted inside the shell's `overflow_hidden()`
/// would be clipped, so they are absolute siblings here.
fn pane_frame(
    shell: Div,
    focused: bool,
    alert: bool,
    pulse: Option<ThreadId>,
    reduce_motion: bool,
) -> Div {
    let ring = |inset: f32, ink: u32| {
        div()
            .absolute()
            .inset(px(inset))
            .rounded(px(theme::R_PANE - inset))
            .border(px(theme::FOCUS_RING_W))
            .border_color(rgb(ink))
    };
    div()
        .relative()
        .flex()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .child(shell)
        .children(
            (focused && alert)
                .then(|| ring(theme::FOCUS_RING_W * 2., FOCUS_RING).into_any_element()),
        )
        // The unread ring never covers focus or a state edge: those already
        // say "look here", and louder.
        .children(pulse.filter(|_| !focused && !alert).map(|thread| {
            let unread = ring(0., ACCENT);
            if reduce_motion {
                unread.opacity(theme::UNREAD_PULSE_MAX).into_any_element()
            } else {
                unread
                    .with_animation(
                        ("attention-ring", thread.get() as usize),
                        Animation::new(Duration::from_millis(theme::STATUS_PULSE_MS))
                            .repeat()
                            .with_easing(pulsating_between(
                                theme::PULSE_MIN,
                                theme::UNREAD_PULSE_MAX,
                            )),
                        |ring, delta| ring.opacity(delta),
                    )
                    .into_any_element()
            }
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

/// The Decision card's `inset 0 0 0 1px` ring — gpui has no inset
/// box-shadow, so it is an absolute full-size overlay that takes no events
/// and no layout. Its radius must match the card it rings.
fn ring_overlay(color: u32, radius: f32) -> Div {
    div()
        .absolute()
        .inset_0()
        .rounded(px(radius))
        .border_1()
        .border_color(rgba(color))
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
}

/// A draft Pane (#29): an empty transcript area and the Composer wearing
/// the pre-prompt band. Below L1 a draft is a quiet placeholder cell — the
/// band only exists where a Composer does, and nothing is running that the
/// instruments could show.
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
        reduce_motion: _,
        drop_target,
    } = state;
    // A draft wears the live Pane's edge: the resting hairline (stepping up
    // under the pointer) or the focus ink. It has no state to announce.
    let edge = PaneEdge::of(focused, false, false);
    let shell = pane_shell(edge.ink()).when(edge == PaneEdge::Rest, |shell| shell.hover_edge());

    if level != Level::Transcript {
        return focus_wrapper(
            shell
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_h_0()
                        .items_center()
                        .justify_center()
                        .text_size(px(theme::FS_SM))
                        .text_color(rgb(TEXT_MUTED))
                        .child("draft"),
                )
                .child(div().absolute().top(px(2.)).right(px(2.)).child(discard))
                .children(drop_target.then(crate::prompt_drop::sheet)),
            focused,
            None,
            false,
        );
    }

    focus_wrapper(
        shell
            .child(pane_head(
                view,
                PaneHeadState {
                    action: Some(discard),
                    ..Default::default()
                },
            ))
            .child(div().flex().flex_1().min_h_0())
            .children(drop_target.then(crate::prompt_drop::sheet))
            .child(composer_region(
                view,
                None,
                ComposerStack {
                    compact: false,
                    decision: None,
                    queued: Vec::new(),
                    queue_height: 0.,
                    needs_queue: false,
                    empty: composer_empty,
                    attachments,
                    actions: composer_actions,
                    background: None,
                    history_available: false,
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
                    alert: false,
                },
            )),
        focused,
        None,
        false,
    )
}

/// A Draft is disposable state, so its Pane advertises the same close action
/// as cmd-w directly in the header. Live Threads deliberately keep keyboard
/// and context-menu closure instead of adding this control to every Pane.
pub fn draft_close_button(draft: DraftId) -> gpui::component::button::Button {
    components::button(("discard-draft", draft.get() as usize))
        .debug_selector(|| "discard-draft".into())
        .ml_auto()
        .w(px(theme::ICON_BUTTON))
        .h(px(theme::ICON_BUTTON))
        .p_0()
        .tooltip("Discard Draft")
        .child(icon(icons::CLOSE, theme::ICON_BUTTON_GLYPH, TEXT_MUTED))
}

/// Draft setup controls use the same 20px hint row as a live Composer.
pub fn draft_band() -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .min_w_0()
        .items_center()
        .gap(px(theme::PICKER_GAP))
        .h(px(theme::COMPOSER_ROW_H))
}

/// One band chip (project, workspace): the Composer's control chip — the
/// choice and a chevron that says it opens a menu — wrapped in a 1px edge
/// that is always in layout and turns `FOCUS_RING` on tab, because the
/// popover opens on ↵ and the chip must say where ↵ will land.
pub fn band_chip(slot: usize, label: SharedString, accent: bool, focused: bool) -> Stateful<Div> {
    div()
        .id(("band-chip", slot))
        .flex_shrink_0()
        .border_1()
        .border_color(band_edge(focused))
        .rounded(px(theme::R_CONTROL))
        .press_raised()
        .child(
            control_chip(if accent { TEXT } else { TEXT_2 })
                .child(div().flex_shrink_0().child(label))
                .child(chip_chevron()),
        )
}

/// A band control's always-in-layout edge: `FOCUS_RING` while tab rests on
/// it, transparent otherwise.
fn band_edge(focused: bool) -> gpui::Hsla {
    if focused {
        rgb(FOCUS_RING).into()
    } else {
        rgba(TRANSPARENT).into()
    }
}

/// A band chip's text: the choice itself (the chip draws its own chevron).
pub fn band_chip_label(choice: &str) -> SharedString {
    SharedString::from(choice.to_owned())
}

/// A draft's model or effort control: the live Composer's own picker
/// chip, wrapped in the band chip's focus edge so tab still says where ↵
/// will land.
pub fn draft_picker(
    id: &'static str,
    focused: bool,
    control: Div,
) -> gpui::component::button::Button {
    crate::components::button(id)
        .p_0()
        .h_auto()
        .flex()
        .flex_shrink_0()
        .border_1()
        .border_color(band_edge(focused))
        .rounded(px(theme::R_CONTROL))
        .child(control)
}

/// The wall's cell (L3): top-anchored rows in priority order, so a short
/// cell still shows the dot, the title and the signal — an 8px dot and the
/// title, then the signal word, then one detail (the plan's meter, a
/// Decision's subject, why a Session closed). Brightness sorts the cells:
/// a hot cell's title is `TEXT_STRONG`, a quiet one's `TEXT_2`; nothing is
/// dimmed by opacity, so no text drops under the readable floor.
fn wall_cell(
    view: &PaneView,
    card: &WallCard,
    state: WallState,
    focused: bool,
    title: Option<AnyElement>,
) -> Div {
    let hot = focused || cell_is_hot(state);
    let (signal, ink) = cell_signal(state, card);
    // Signals and details hang under the title, past the dot.
    let hang = theme::WALL_DOT + theme::CELL_DOT_GAP;
    let line = |text: SharedString, ink: u32| {
        div()
            .flex_shrink_0()
            .w_full()
            .pl(px(hang))
            .truncate()
            .text_size(px(theme::FS_SM))
            .line_height(px(theme::LH_META))
            .text_color(rgb(ink))
            .child(text)
    };
    let detail = match state {
        WallState::Working | WallState::Failing => card.todos.map(|(done, total)| {
            div()
                .flex_shrink_0()
                .pl(px(hang))
                .text_size(px(theme::FS_SM))
                .line_height(px(theme::LH_META))
                .child(meter(done, total, state == WallState::Working))
        }),
        WallState::Decision if !card.context.is_empty() => Some(line(card.context.clone(), TEXT_2)),
        WallState::Blocked => Some(line(SharedString::from("session closed"), TEXT_MUTED)),
        _ => None,
    };
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap(px(theme::WALL_ROW_GAP))
        .p(px(theme::WALL_PAD))
        .overflow_hidden()
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(theme::CELL_DOT_GAP))
                .min_w_0()
                .child(cell_dot(state).size(px(theme::WALL_DOT)))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(theme::FS_UI))
                        .line_height(px(theme::LH_UI))
                        .font_weight(theme::W_LABEL)
                        .text_color(rgb(if hot { TEXT_STRONG } else { TEXT_2 }))
                        .child(match title {
                            Some(title) => title,
                            None => div().truncate().child(view.name.clone()).into_any_element(),
                        }),
                ),
        )
        .child(line(signal, ink))
        .children(detail)
}

/// A hot cell asks to be read: work under way, a failure, a Decision, a
/// closed Session. Quiet cells (done, idle, parked) step their title down.
fn cell_is_hot(state: WallState) -> bool {
    matches!(
        state,
        WallState::Working | WallState::Failing | WallState::Decision | WallState::Blocked
    )
}

/// A cell's status dot, one recipe for L2 and the wall (and the nav's
/// meaning): running green, a Decision amber, closed red, done and idle the
/// idle ink — green never means finished — and a parked Thread hollow.
fn cell_dot(state: WallState) -> Div {
    match state {
        WallState::Working | WallState::Failing => components::status_dot(RUNNING),
        WallState::Decision => components::status_dot(ATTENTION),
        WallState::Blocked => components::status_dot(BLOCKED),
        WallState::Done | WallState::Idle => components::status_dot(IDLE),
        WallState::Parked => components::status_ring(TEXT_MUTED),
    }
}

/// The wall's signal: what the cell is doing, in words, and the only ink
/// that may carry state. Work in progress reads in the body ink; a failure,
/// a Decision and a closed Session take their state colour on the word
/// alone; done, idle and parked are metadata.
fn cell_signal(state: WallState, card: &WallCard) -> (SharedString, u32) {
    match state {
        WallState::Working => (card.working.clone(), TEXT_2),
        WallState::Failing => (card.failing.clone(), BLOCKED),
        WallState::Decision => (SharedString::from("needs you"), ATTENTION),
        WallState::Blocked => (card.context.clone(), BLOCKED),
        WallState::Done => (SharedString::from("done"), TEXT_MUTED),
        WallState::Idle => (SharedString::from("idle"), TEXT_MUTED),
        WallState::Parked => (SharedString::from("parked"), TEXT_MUTED),
    }
}

// ------------------------------------------------------------- L2 cell

/// The Cockpit board's cell grammar (L2): a 24px header (dot · title ·
/// right meta), then the instruments — the model and checkout, one row of
/// readings (plan, tests, diff, files), and the conversation's tail, newest
/// at the bottom — and the compact Composer. A pending Decision swaps the
/// body for the y/n card. Words, not chips: only a failure's words and the
/// diff's signs carry a hue.
#[allow(clippy::too_many_arguments)]
fn l2_cell(
    view: &PaneView,
    transcript: Option<&Transcript>,
    decision: Option<&Decision>,
    workspace: Option<&WorkspaceBinding>,
    branch: Option<&SharedString>,
    state: WallState,
    _timings: Option<&HashMap<String, ToolTiming>>,
    decide: Option<AnyElement>,
    title: Option<AnyElement>,
    composer: Option<Div>,
    requests: Option<AnyElement>,
    expand_question: Option<AnyElement>,
    reduce_motion: bool,
    drop_target: bool,
) -> Div {
    let compact_question = expand_question.is_some();
    // The drop sheet paints before the Composer, so the Composer shows
    // above it; a cell with no Composer takes it last, over everything.
    let sheet = || drop_target.then(crate::prompt_drop::sheet);
    let mut header = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::CELL_HEADER_H))
        .gap(px(theme::CELL_DOT_GAP))
        .px(px(theme::CELL_PAD))
        .child(cell_dot(state))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(px(theme::FS_UI))
                .line_height(px(theme::LH_UI))
                .font_weight(theme::W_LABEL)
                .text_color(rgb(if cell_is_hot(state) {
                    TEXT_STRONG
                } else {
                    TEXT_2
                }))
                .child(match title {
                    Some(title) => title,
                    None => div().truncate().child(view.name.clone()).into_any_element(),
                }),
        )
        .child(div().flex_1());
    let meta = |text: SharedString| {
        div()
            .flex_shrink_0()
            .text_size(px(theme::FS_SM))
            .line_height(px(theme::LH_META))
            .text_color(rgb(TEXT_MUTED))
            .child(text)
    };
    // The right meta names the Workspace binding — what an operator running
    // many Threads actually needs — and a finished turn's one completion
    // label (muted: green never means finished). A question too big for
    // the cell puts its expander here instead.
    header = if let Some(expand) = expand_question {
        header.child(div().flex_shrink_0().child(expand))
    } else if state == WallState::Done {
        header.child(meta(SharedString::from("done")))
    } else {
        header.child(meta(binding_label(workspace)))
    };

    let cell = div().flex().flex_col().flex_1().min_h_0().min_w_0();
    let Some(transcript) = transcript else {
        return cell.child(header).child(parked_body()).children(sheet());
    };

    if let Some(requests) = requests {
        return cell
            .child(header)
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

    // A Decision's cell body is the card, keyed like the in-Pane card.
    if let Some(decision) = decision.filter(|_| !compact_question) {
        return cell
            .child(header)
            .child(
                l2_decision_body(decision, decide)
                    .key_context("Decision")
                    .track_focus(&view.decision_focus),
            )
            .children(sheet());
    }

    let read = Instruments::of(transcript);
    let mut body = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .px(px(theme::CELL_PAD))
        .pb(px(theme::SPACE_2))
        .gap(px(theme::CELL_ROW_GAP))
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(TEXT_MUTED));

    // The facts the head has no room for at this size: the model serving
    // and the checkout, one muted line — the two things an operator
    // running nine of these asks first.
    let facts: Vec<String> = [
        transcript
            .model()
            .map(ferrite_core::providers::models::display_name),
        branch.map(|branch| branch.to_string()),
    ]
    .into_iter()
    .flatten()
    .filter(|part| !part.is_empty())
    .collect();
    if !facts.is_empty() {
        body = body.child(
            div()
                .w_full()
                .flex_shrink_0()
                .truncate()
                .child(SharedString::from(facts.join(" · "))),
        );
    }

    // One row of readings: the plan's meter, the latest test run, the diff
    // and how many files it touched. Omitted when there is nothing to read.
    let mut readings = div()
        .flex()
        .flex_shrink_0()
        .min_w_0()
        .overflow_hidden()
        .items_center()
        .gap(px(theme::SPACE_3));
    let mut any = false;
    if let Some(todos) = read.todos.filter(|todos| todos.total > 0) {
        readings = readings.child(meter(
            todos.done,
            todos.total,
            transcript.status() == Status::Streaming,
        ));
        any = true;
    }
    match read.tests {
        Some(Tests::Passed { count }) => {
            readings = readings.child(div().flex_shrink_0().child(match count {
                Some(count) => SharedString::from(format!("{count} passed")),
                None => SharedString::from("tests pass"),
            }));
            any = true;
        }
        Some(Tests::Failed { count }) => {
            readings = readings.child(div().flex_shrink_0().text_color(rgb(BLOCKED)).child(
                match count {
                    Some(count) => SharedString::from(format!("{count} failing")),
                    None => SharedString::from("tests failing"),
                },
            ));
            any = true;
        }
        None => {}
    }
    if read.added > 0 || read.removed > 0 {
        readings = readings.child(diff_stat(read.added, read.removed).flex_shrink_0());
        any = true;
    }
    if read.files() > 0 {
        readings = readings.child(div().flex_shrink_0().child(SharedString::from(format!(
            "{} file{}",
            read.files(),
            if read.files() == 1 { "" } else { "s" }
        ))));
        any = true;
    }
    if any {
        body = body.child(readings);
    }
    // The tail of the conversation fills what is left: prompts, answers
    // and tool rows in one compact column, newest at the bottom — what
    // the Thread is saying, not only that it is saying something.
    body = body.child(l2_tail(transcript, view.text_namespace()));
    if transcript.status() == Status::Streaming {
        body = body.child(div().flex_shrink_0().child(working_line(
            transcript,
            true,
            false,
            reduce_motion,
        )));
    }

    // Completion quiets historical content; the editable Composer stays at
    // its normal contrast. The header's "done" is the one completion label.
    if state == WallState::Done {
        body = body.opacity(theme::DONE_CELL_OPACITY);
    }
    cell.child(header)
        .child(body)
        .children(sheet())
        .children(composer)
}

/// How many Blocks an L2 tail reaches back for.
const L2_TAIL_BLOCKS: usize = 16;
/// How many lines one Block may take in the tail before it is cut.
const L2_TAIL_LINES: usize = 4;

/// The compact tail of a transcript for an L2 cell: the newest Blocks as
/// single runs in the transcript's own grammar — a prompt behind its `❯`,
/// prose in Geist, a tool row as its dot and call, a Notice behind an amber
/// dot — each clamped to a few lines. Native layout measures each candidate
/// in the actual remaining slot, then paints only complete rows, newest at
/// the bottom.
fn l2_tail(transcript: &Transcript, namespace: SharedString) -> Div {
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
    let mut rows = Vec::new();
    for block in tail {
        if live_reasoning == Some(block.id) {
            continue;
        }
        let line = |text: String, ink: u32| {
            div()
                .w_full()
                .flex_shrink_0()
                .text_size(px(theme::FS_SM))
                .line_height(px(theme::LH_META))
                .text_color(rgb(ink))
                .child(SharedString::from(text))
        };
        // A dot-led run (tool, notice): the `●` in its own ink, then the
        // name in `TEXT_2` and the rest in metadata ink, as one text run so
        // the line clamp still applies.
        let dotted = |dot: u32, name: &str, rest: &str| {
            let text = if rest.is_empty() {
                format!("● {name}")
            } else {
                format!("● {name} {rest}")
            };
            let dot_end = '●'.len_utf8();
            let name_end = dot_end + 1 + name.len();
            let runs = vec![
                (
                    0..dot_end,
                    HighlightStyle {
                        color: Some(rgb(dot).into()),
                        ..Default::default()
                    },
                ),
                (
                    dot_end..name_end,
                    HighlightStyle {
                        color: Some(rgb(TEXT_2).into()),
                        ..Default::default()
                    },
                ),
            ];
            div()
                .w_full()
                .flex_shrink_0()
                .text_size(px(theme::FS_SM))
                .line_height(px(theme::LH_META))
                .text_color(rgb(TEXT_MUTED))
                .child(StyledText::new(text).with_highlights(runs))
        };
        let prose = |spans: &[Span]| -> String {
            spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>()
                .trim()
                .to_string()
        };
        let drawn = match &block.body {
            Body::Prompt(text) => {
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }
                div()
                    .w_full()
                    .flex()
                    .flex_shrink_0()
                    .gap(px(theme::SPACE_1_5))
                    .text_size(px(theme::FS_SM))
                    .line_height(px(theme::LH_META))
                    .text_color(rgb(TEXT_STRONG))
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .h(px(theme::LH_META))
                            .child(components::prompt_mark(ACCENT)),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .child(SharedString::from(text.to_string())),
                    )
            }
            Body::Paragraph { spans } | Body::Bullet { spans } => {
                let text = prose(spans);
                if text.is_empty() {
                    continue;
                }
                line(text, TEXT_2).font_family(theme::FONT_PROSE)
            }
            Body::Heading { spans, .. } => {
                let text = prose(spans);
                if text.is_empty() {
                    continue;
                }
                line(text, TEXT)
                    .font_family(theme::FONT_PROSE)
                    .font_weight(theme::W_STRONG)
            }
            Body::Code { language, .. } => line(
                format!("```{}", language.as_deref().unwrap_or("")),
                TEXT_MUTED,
            ),
            Body::Tool(tool) => {
                let dot = match tool.state {
                    ToolState::Failed(_) => BLOCKED,
                    _ => TEXT_FAINT,
                };
                dotted(dot, &tool.name, &tool.summary).line_clamp(1)
            }
            Body::Notice(text) => dotted(ATTENTION, text.trim(), ""),
            Body::Meta(text) => line(text.clone(), TEXT_MUTED),
            Body::TurnEnd(end) => line(end.text(), TEXT_MUTED),
            Body::Thinking(text) => {
                line(ferrite_core::progress::headline(text), TEXT_MUTED).line_clamp(1)
            }
        };
        let lines = if matches!(block.body, Body::Tool(_) | Body::Thinking(_)) {
            1
        } else {
            L2_TAIL_LINES
        };
        rows.push((block.id, drawn, lines));
    }
    let selector = format!("l2-tail-{namespace}");
    div()
        .debug_selector(move || selector.clone())
        .flex()
        .flex_1()
        .min_h_0()
        .w_full()
        .child(
            canvas(
                move |bounds, window, cx| {
                    let mut remaining = bounds.size.height;
                    let mut visible = Vec::new();
                    for (id, row, limit) in rows.into_iter().rev() {
                        let available_lines =
                            (f32::from(remaining) / theme::LH_META).floor().max(0.) as usize;
                        if available_lines == 0 {
                            break;
                        }
                        // Only the newest row may use a smaller line clamp. Older
                        // rows are either drawn whole (up to the normal L2 limit)
                        // or omitted. No clipping through a glyph baseline.
                        let lines = if visible.is_empty() {
                            limit.min(available_lines)
                        } else {
                            limit
                        };
                        let selector = format!("l2-tail-row-{namespace}-{id:?}");
                        let mut row = row
                            .line_clamp(lines)
                            .debug_selector(move || selector.clone())
                            .into_any_element();
                        let size = row.layout_as_root(
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
                        row.prepaint_at(origin, window, cx);
                        visible.push(row);
                        remaining -= size.height + px(theme::CELL_TAIL_GAP);
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

/// The Cockpit board's Decision cell body: the `◆ approve` head, the
/// subject (two lines at most), where it runs, and the y/n keycaps — no
/// `a always` at L2. The group hangs directly under the header; a spacer
/// would strand the keycaps on the cell floor (#22 A2). The keycaps arrive
/// wired from the cockpit (#26), each only where its key would act.
fn l2_decision_body(decision: &Decision, decide: Option<AnyElement>) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .p(px(theme::CELL_PAD))
        .gap(px(theme::DECISION_L2_GAP))
        .font_family(theme::FONT_MONO)
        .child(decision::head(decision::kind_word(decision), None, None))
        .child(
            div()
                .w_full()
                .min_w_0()
                .line_clamp(2)
                .text_size(px(theme::FS_UI))
                .line_height(px(theme::LH_UI))
                .text_color(rgb(TEXT_STRONG))
                .child(decision_subject(decision)),
        )
        .children(decision_place(decision).map(|place| {
            div()
                .w_full()
                .truncate()
                .text_size(px(theme::FS_SM))
                .line_height(px(theme::LH_META))
                .text_color(rgb(TEXT_MUTED))
                .child(place)
        }))
        .children(decide)
}

// ---------------------------------------------------------------- L1 pane

/// The Pane head (§D.2): **one 36px row** on the Pane's own plane, closed by
/// the hairline — the one rule drawn inside a Pane. Left: the status dot,
/// the title and the checkout (the branch gives way first when narrow).
/// Then the agent tabs, or a spacer. Right, never shrinking: the tasks
/// meter, the PR and its CI, the attention jump and the head action.
///
/// There is no model chip here (the Composer's picker is the only model
/// surface) and no window controls (park and zoom stay on the keyboard).
#[derive(Default)]
struct PaneHeadState<'a> {
    branch: Option<&'a SharedString>,
    checkout: Option<&'a BranchStatus>,
    project_branches: &'a [(SharedString, SharedString)],
    status: Option<Status>,
    title: Option<AnyElement>,
    agents: Option<AnyElement>,
    ci: Option<AnyElement>,
    attention: Option<AnyElement>,
    action: Option<AnyElement>,
    /// The tasks meter (`l1_tasks`), riding the right cluster.
    tasks: Option<AnyElement>,
    /// Another Pane holds focus: the title steps down from `TEXT_STRONG`
    /// to `TEXT`, the one focus cue that survives a state edge.
    unfocused: bool,
}

fn pane_head(view: &PaneView, state: PaneHeadState<'_>) -> Div {
    let PaneHeadState {
        branch,
        checkout,
        project_branches,
        status,
        title,
        agents,
        ci,
        attention,
        action,
        tasks,
        unfocused,
    } = state;
    // The dot's base is the muted ink — the parked look — and each live
    // state takes its own signal colour. The no-dot ruling is scoped to
    // navigation; a Pane head keeps its dot.
    let dot_color = match status {
        Some(Status::Streaming) => RUNNING,
        Some(Status::Blocked) => ATTENTION,
        Some(Status::Closed) => BLOCKED,
        _ => IDLE,
    };
    let has_agents = agents.is_some();
    let key = view.thread().map_or(0, ThreadId::get);
    let left = div()
        .flex()
        .min_w_0()
        .flex_shrink(1.)
        .overflow_hidden()
        .items_center()
        .gap(px(theme::HEAD_GAP))
        .child(components::status_dot(dot_color))
        .child(
            div()
                .min_w_0()
                .flex_shrink(1.)
                .when(has_agents, |title| title.max_w(relative(0.32)))
                .text_size(px(theme::FS_UI))
                .line_height(px(theme::LH_UI))
                .font_weight(theme::W_LABEL)
                .text_color(rgb(if unfocused { TEXT } else { TEXT_STRONG }))
                .child(match title {
                    Some(title) => title,
                    None => div().truncate().child(view.name.clone()).into_any_element(),
                }),
        )
        .children(
            checkout_strip(checkout, branch, project_branches)
                .map(|checkout| checkout.ml(px(theme::HEAD_CLUSTER_GAP - theme::HEAD_GAP))),
        );
    // The PR is one fact with its CI: wired where the cockpit could wire
    // the card, else drawn flat (below L1, pane-only tests, no checks).
    let pr = checkout
        .and_then(|status| status.pr.as_ref())
        .map(|pr| match ci {
            Some(ci) if pr.checks.is_some() => ci,
            _ => ci_face(pr, false).into_any_element(),
        });
    let right = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::HEAD_CLUSTER_GAP))
        .children(tasks)
        .children(pr)
        .children(attention)
        .children(action);
    div()
        .debug_selector(move || format!("pane-head-{key}"))
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::PANE_HEAD_H))
        .gap(px(theme::HEAD_CLUSTER_GAP))
        .px(px(theme::PANE_PAD_X))
        .border_b_1()
        .border_color(rgba(PANE_HEAD_EDGE))
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(TEXT_MUTED))
        .child(left)
        // The tabs take the free width; without them a growing spacer does
        // — never `ml_auto`, which collapses every gap in the row (taffy
        // hands an auto margin the container's gaps too).
        .child(match agents {
            Some(agents) => agents,
            None => div().flex_1().min_w_0().into_any_element(),
        })
        .child(right)
}

/// The head's checkout (#29): the branch mark and name, then only what is
/// actually true of it — `↑2 ↓1` against its upstream and `±3` of working
/// tree dirt. A branch with no upstream simply has no drift marks: silence
/// here means unknown or absent, never "fine". All of it is metadata ink —
/// drift and dirt are not the Thread's state. A multi-directory Project
/// names each directory's branch: `frontend:feat/header  api:main`.
fn checkout_strip(
    checkout: Option<&BranchStatus>,
    branch: Option<&SharedString>,
    project_branches: &[(SharedString, SharedString)],
) -> Option<Div> {
    // A branch label with no status behind it still deserves the line: the
    // first refresh has simply not landed yet.
    let name: SharedString = match (checkout.and_then(|status| status.branch.as_ref()), branch) {
        (Some(name), _) => SharedString::from(name.clone()),
        (None, Some(name)) => name.clone(),
        (None, None) => return None,
    };
    let branches = if project_branches.is_empty() {
        vec![(None, name)]
    } else {
        project_branches
            .iter()
            .map(|(directory, branch)| (Some(directory.clone()), branch.clone()))
            .collect()
    };
    let mut strip = div()
        .flex()
        .min_w(px(theme::HEAD_CHECKOUT_MIN_W))
        .flex_shrink(theme::HEAD_CHECKOUT_SHRINK)
        .overflow_hidden()
        .items_center()
        .gap(px(theme::ROW_ICON_GAP))
        .child(icon(icons::BRANCH, theme::ROW_ICON, TEXT_FAINT));
    let mut branch_list = div()
        .flex()
        .min_w(px(theme::HEAD_BRANCH_MIN_W))
        .flex_shrink(1.)
        .items_center()
        .gap(px(theme::CHECKOUT_GAP));
    for (index, (directory, branch)) in branches.into_iter().enumerate() {
        branch_list = branch_list.child(
            div()
                .debug_selector(move || format!("project-branch-{index}"))
                .flex()
                .min_w_0()
                .items_center()
                .when_some(directory, |item, directory| {
                    item.child(div().flex_shrink_0().child(directory))
                        .child(div().flex_shrink_0().text_color(rgb(TEXT_FAINT)).child(":"))
                })
                .child(div().min_w_0().truncate().child(branch)),
        );
    }
    strip = strip.child(branch_list);
    let Some(status) = checkout else {
        return Some(strip);
    };
    let marks: Vec<String> = [
        (status.ahead, "↑"),
        (status.behind, "↓"),
        (status.dirty, "±"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, sign)| format!("{sign}{count}"))
    .collect();
    if !marks.is_empty() {
        strip = strip.child(
            div()
                .flex()
                .min_w_0()
                .flex_shrink(theme::HEAD_CHECKOUT_SHRINK * 2.)
                .overflow_hidden()
                .items_center()
                .gap(px(theme::ROW_ICON_GAP))
                .children(marks.into_iter().map(mark)),
        );
    }
    Some(strip)
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

/// A CI rollup's count, as words: how many runs failed while any has, else
/// how many of them have settled. Only the failure is coloured.
fn ci_count(pr: &PullRequest) -> (SharedString, u32) {
    let tally = pr.tally();
    if tally.failing > 0 {
        (
            SharedString::from(format!("{} failed", tally.failing)),
            BLOCKED,
        )
    } else {
        (
            SharedString::from(format!("{}/{}", tally.settled(), tally.total())),
            TEXT_MUTED,
        )
    }
}

/// The PR and its CI as one fact: `#48 ● 5/7`. The rollup's dot is the one
/// place CI colour appears, and legitimately state; the count is words.
/// Without checks it is the label alone. `open` keeps the `FILL` ground
/// while the card this chip opened is showing.
///
/// Padded and rounded as a chip so its hover face reaches around the
/// glyphs. (No negative margin to pull the padding back: taffy then sizes
/// the head's right cluster to nothing and the chip overflows the Pane.)
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
    let Some(checks) = pr.checks else {
        return face;
    };
    let (count, ink) = ci_count(pr);
    face.child(components::status_dot(check_ink(checks)))
        .child(components::tabular(div().text_color(rgb(ink)).child(count)))
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
        .hover_control()
        .press_control()
}

/// A check's ink, the Pane's own status inks: green for a run that passed,
/// red for one that did not, amber while it is still going, and the
/// quietest ink for a run that claims nothing at all.
pub fn check_ink(state: CheckState) -> u32 {
    match state {
        CheckState::Passing => RUNNING,
        CheckState::Failing => BLOCKED,
        CheckState::Pending => ATTENTION,
        CheckState::Skipped => TEXT_MUTED,
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

/// The checks card's column, for the cockpit to fill with `checks_head`
/// and the `check_row`s it has wired. Its own width, because the runs it
/// lists are named by the forge and a job name is longer than a menu row.
pub fn checks_card() -> Div {
    div()
        .flex()
        .flex_col()
        .w(px(theme::CHECKS_CARD_W))
        .p(px(theme::CHECKS_CARD_PAD))
        .font_family(theme::FONT_MONO)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(TEXT))
}

/// The card's heading: the PR by number at the left, and how its runs
/// divide at the right — the counts the head's chip had no room for. Only
/// states with runs in them are named, so the line never reads `0 failed`,
/// and only the failure is coloured. A hairline separates summary from
/// runs: the one rule the card draws.
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
        .min_w_0()
        .truncate()
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(TEXT_MUTED))
        .child(StyledText::new(text).with_highlights(runs));
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .gap(px(theme::EVENT_GAP))
        .h(px(theme::CHECKS_HEAD_H))
        .px(px(theme::CHIP_PAD_X))
        .mb(px(theme::CHECKS_CARD_GAP))
        .border_b_1()
        .border_color(rgba(HAIRLINE))
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
/// `status` rather than being given a heading it does not have.
pub fn checks_group(workflow: Option<&str>, first: bool) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::CHECKS_GROUP_H))
        .px(px(theme::CHIP_PAD_X))
        .when(!first, |group| group.mt(px(theme::CHECKS_GROUP_GAP)))
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(TEXT_MUTED))
        .child(SharedString::from(workflow.unwrap_or("status").to_string()))
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
    div()
        .id(("check-row", index))
        .debug_selector(move || format!("check-row-{index}"))
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::ROW_ICON_GAP))
        .h(px(theme::CHECKS_ROW_H))
        .px(px(theme::CHIP_PAD_X))
        .rounded(px(theme::R_CHIP))
        .child(components::status_dot(check_ink(run.state)))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_color(rgb(if openable { TEXT } else { TEXT_2 }))
                .child(SharedString::from(run.name.clone())),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::FS_SM))
                .text_color(rgb(check_detail_ink(run.state)))
                .child(SharedString::from(run.detail.replace('_', " "))),
        )
        // The card is a raised surface: its rows take the raised faces.
        .when(openable, |row| row.hover_raised().press_raised())
}

/// One mark on the checkout: a short run that never shrinks — these are
/// the facts the checkout exists to carry, and the branch name is what
/// gives way when the Pane is narrow.
fn mark(text: String) -> Div {
    components::tabular(div().flex_shrink_0().child(SharedString::from(text)))
}

/// The head's title, saying it can be renamed: the name in its parent's
/// ink with the hover face every control wears, truncating. Render-only;
/// the cockpit gives it its id and its double-click.
pub fn head_title(name: SharedString) -> Div {
    div()
        .min_w_0()
        .truncate()
        .px(px(theme::CHIP_PAD_X))
        .mx(px(-theme::CHIP_PAD_X))
        .rounded(px(theme::R_CHIP))
        .child(name)
        .hover_control()
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

/// The painted meter (the head, L2 and the wall share it; Geist Mono has
/// no `▰▱`): done steps in `TEXT_2`, the rest unlit. `live` lights the
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
/// logomark (monochrome: this is a control, not a picker row), the bare
/// model name and a chevron. A busy Session mutes it rather than fading it.
/// Render-only; the cockpit gives it its id and its click.
pub fn model_picker(provider: Option<Provider>, label: SharedString, busy: bool) -> Div {
    let ink = if busy { TEXT_MUTED } else { TEXT_2 };
    let mark = provider.map(|provider| {
        let glyph = match provider {
            Provider::Codex => icons::CODEX,
            Provider::Claude => icons::CLAUDE,
        };
        icon(glyph, theme::PROVIDER_MARK_SM, TEXT_MUTED)
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

/// The working line: one row, the animated Ferrite mark in the gutter (at
/// rest under reduced motion), the provider's live caption, then `(6s · ↓ 312 tokens)` in metadata ink —
/// Claude Code's `✻ Thinking… (12s · ↓ 1.2k tokens)`. The caption is what
/// truncates; the facts keep their room. `esc` is shown once, on Stop. L2
/// (`compact`) draws the same row without the token count. Command details
/// stay in the tool rows.
fn working_line(
    transcript: &Transcript,
    compact: bool,
    received_reasoning_is_visible: bool,
    reduce_motion: bool,
) -> Div {
    let mut facts: Vec<String> = Vec::new();
    if let Some(elapsed) = transcript.turn_elapsed() {
        facts.push(components::duration_label(elapsed).to_string());
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
        .font_family(theme::FONT_MONO)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI));
    if let Some(caption) = caption {
        let selector = format!("progress-caption-{caption}");
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
                    .debug_selector(|| "progress-reasoning".into())
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(TEXT_2))
                    .child(SharedString::from(caption)),
            )
            .when(!facts.is_empty(), |row| {
                row.child(components::tabular(
                    div()
                        .debug_selector(|| "progress-metadata".into())
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .pl(px(theme::MONO_CELL))
                        .text_size(px(theme::FS_SM))
                        .text_color(rgb(TEXT_MUTED))
                        .child(SharedString::from(format!("({})", facts.join(" \u{b7} ")))),
                ))
            });
    }
    div().w_full().min_w_0().flex_shrink_0().child(row)
}

pub(crate) fn live_text(row: Div, id: SharedString) -> AnyElement {
    row.with_animation(
        id,
        gpui::Animation::new(Duration::from_millis(theme::STATUS_PULSE_MS))
            .repeat()
            .with_easing(gpui::pulsating_between(0.65, 1.0)),
        |row, opacity| row.opacity(opacity),
    )
    .into_any_element()
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
    decision: Option<&'a Decision>,
    /// Prompts held back while the turn runs, newest first: they pile up
    /// above the line, the latest on top.
    queued: Vec<&'a str>,
    queue_height: f32,
    needs_queue: bool,
    empty: bool,
    attachments: Option<AnyElement>,
    actions: Option<AnyElement>,
    /// Running background tasks as chips, hung at the right edge of the
    /// same shelf the pending files sit on.
    background: Option<AnyElement>,
    history_available: bool,
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
    /// The Pane's own edge is a state colour (a Decision, a blocker). The
    /// block then carries the focus edge itself while `editing`, so focus
    /// never hides behind the amber or red.
    alert: bool,
}

/// The Composer: a raised block in the reading column. Its outer edges are
/// the column's edges and its content sits `BOX_INSET_X` inside them, so
/// its `❯` shares the transcript's glyph box and its text starts at C1.
/// `RAISED`, `R_BLOCK`, a 1px edge that is always in layout (`COMPOSER_EDGE`,
/// `COMPOSER_EDGE_FOCUS` while editing on an alert Pane), padding
/// `COMPOSER_PAD_T/X/B`, rows `COMPOSER_GAP` apart:
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
        decision,
        queued,
        queue_height,
        needs_queue,
        empty,
        attachments,
        mut actions,
        background,
        history_available,
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
        alert,
    } = stack;
    let is_draft = setup_controls.is_some();
    let blocking = decision.is_some_and(Decision::blocks_execution);
    let mut block = components::raised_edged(composer_edge(alert, editing, drop_target))
        .debug_selector(|| "composer-block".into())
        .when(drop_target, |block| {
            block.debug_selector(|| "composer-drop-target".into())
        })
        .relative()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(px(theme::COMPOSER_GAP))
        .min_w_0()
        .pt(px(theme::COMPOSER_PAD_T))
        .px(px(theme::COMPOSER_PAD_X))
        .pb(px(theme::COMPOSER_PAD_B))
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
        block = block.child(
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
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(theme::COMPOSER_GAP))
                                .children(queued.iter().enumerate().map(|(index, held)| {
                                    let namespace = namespace.clone();
                                    div()
                                        .flex_shrink_0()
                                        .debug_selector(move || {
                                            format!("queue-row-{namespace}-{index}")
                                        })
                                        .child(queued_line(held, index, count, empty))
                                })),
                        ),
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
        .line_height(px(theme::COMPOSER_ROW_H))
        .text_color(rgb(TEXT_STRONG))
        .child(view.composer.clone());
    if empty {
        // The Composer paints its own caret at the line origin, so the
        // ghost reserves the same caret inset in either focus state.
        line = line.child(
            div()
                .debug_selector(|| "prompt-placeholder".into())
                .absolute()
                .left(px(theme::CARET_W))
                .right_0()
                .top_0()
                .h(px(theme::COMPOSER_ROW_H))
                .flex()
                .items_center()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_color(rgb(TEXT_MUTED))
                .child(placeholder(decision.is_some(), transcript, suggestion)),
        );
    }
    // The `❯` is always in layout, so the text origin never moves with
    // focus. It hangs centred on the first row while the line grows.
    let mut input = div()
        .flex()
        .items_start()
        .min_h(px(theme::COMPOSER_ROW_H))
        .min_w_0()
        .child(components::gutter(
            components::prompt_mark(prompt_ink(editing, decision.is_some())),
            theme::COMPOSER_ROW_H,
        ))
        .child(line);
    if !compact {
        input = input.children(
            actions
                .take()
                .map(|actions| div().flex_shrink_0().ml(px(theme::SPACE_2)).child(actions)),
        );
    }
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

    // The hint row: setup or mode and the key hints at left; usage,
    // session controls and the model pair at right. The hints give way
    // first — they clip, the controls never do.
    let mut controls = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::SPACE_2))
        .h(px(theme::COMPOSER_ROW_H))
        .min_w_0();
    if let Some(setup) = setup_controls {
        controls = controls.child(setup);
    }
    // The chip is the live Session's permission mode, so it rides every
    // Pane whose Session has announced one and is not blocked: a Decision
    // owns the keyboard until it is answered, and a closed Session has no
    // mode to be in (its chip is None). It is not tied to a turn in
    // flight — the mode is exactly what an operator changes *between*
    // prompts.
    // The plain chip (no menu: L2) rides the hint row's wrap, so in a
    // narrow cell it gives way whole before Send/Stop ever would.
    let mut plain_mode = None;
    if let Some(mode) = mode.filter(|_| !blocking) {
        match mode_picker {
            Some(picker) => controls = controls.child(div().flex_shrink_0().child(picker)),
            None => plain_mode = Some(mode_chip(mode, false).mr(px(theme::SPACE_1))),
        }
    }
    let hints = if compact && empty {
        COMPACT_HINTS
    } else if !empty {
        typing_hints(needs_queue)
    } else {
        composer_hints(
            is_draft,
            history_available,
            followup::suggest(decision.is_some(), transcript, suggestion)
                .acceptable()
                .is_some(),
        )
    };
    controls = controls.child(hint_row(plain_mode, hints));
    if let Some(meter) = usage_meter {
        controls = controls.child(div().flex_shrink_0().child(meter));
    }
    if let Some(session_controls) = session_controls {
        controls = controls.child(div().flex_shrink_0().child(session_controls));
    }
    if let Some(picker) = model_picker {
        controls = controls.child(div().flex_shrink_0().child(picker));
    }
    // L2 has no model/usage controls. Use their row for actions so every
    // line of a small Pane's draft keeps the full editor width.
    controls = controls.children(actions.map(|actions| div().flex_shrink_0().child(actions)));
    let stack = div()
        .flex()
        .flex_col()
        .min_w_0()
        .when(attachments.is_some() || background.is_some(), |stack| {
            // The shelf floats clear of the block, inset to its content
            // edges: pending files at left, the background chips at right
            // on the Send column. The chips give way first — they cut
            // their labels, the files do not.
            stack.child(
                div()
                    .debug_selector(|| "composer-shelf".into())
                    .flex()
                    .items_end()
                    .gap(px(theme::SPACE_2))
                    .min_w_0()
                    .px(px(theme::BOX_INSET_X))
                    .pb(px(theme::SHELF_GAP))
                    .when_some(attachments, |shelf, attachments| {
                        shelf.child(div().flex_1().min_w_0().child(attachments))
                    })
                    .when_some(background, |shelf, chips| {
                        shelf.child(div().ml_auto().min_w_0().max_w_full().child(chips))
                    }),
            )
        })
        .child(block.child(controls));
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

/// The input line's `❯`: `ACCENT` while the keyboard is in the line —
/// `ATTENTION` when the line is a pending Decision's reply channel, the one
/// cue besides the placeholder that what is typed answers it — and
/// `TEXT_MUTED` whenever keys would land elsewhere.
fn prompt_ink(editing: bool, replying: bool) -> u32 {
    match (editing, replying) {
        (false, _) => TEXT_MUTED,
        (true, true) => ATTENTION,
        (true, false) => ACCENT,
    }
}

/// The block's edge (as `0xRRGGBBAA`): the resting `COMPOSER_EDGE`, or the
/// focus ink when the keyboard is in the line on a Pane whose own edge is a
/// state colour — the one case the Pane ring alone could leave in doubt.
fn composer_edge(alert: bool, editing: bool, drop_target: bool) -> u32 {
    if drop_target {
        theme::ACCENT_EDGE
    } else if alert && editing {
        (theme::COMPOSER_EDGE_FOCUS << 8) | 0xff
    } else {
        theme::COMPOSER_EDGE
    }
}

/// Whether a Pane's own edge is a state colour: a Decision pending anywhere
/// in its activity, or the Session closed under it. The same test
/// `render_pane` makes for the edge.
fn composer_alert(cx: &PaneCtx) -> bool {
    let pending = cx
        .thread
        .is_some_and(|thread| !thread.activity().pending_decisions().is_empty());
    pending
        || wall_state(
            cx.transcript,
            cx.decision.is_some_and(Decision::blocks_execution),
            false,
        ) == WallState::Blocked
}

/// A Composer control on the hint row (§ theme "Composer controls"): the
/// model, effort and mode pickers and the session `•••` share it. No ground
/// at rest, `FILL` under the pointer (the hover face on `RAISED`), label in
/// `ink`, an optional chevron. Render-only; the cockpit wires it.
fn control_chip(ink: u32) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::PICKER_GAP))
        .h(px(theme::CHIP_H))
        .px(px(theme::PICKER_PAD_X))
        .rounded(px(theme::R_CONTROL))
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(ink))
        .hover_raised()
}

/// A control chip's menu chevron.
fn chip_chevron() -> gpui::Svg {
    icon(icons::CHEVRON_DOWN, theme::ICON_CHEVRON_SM, TEXT_MUTED)
}

/// The Composer's mode chip: the mode's own word and a chevron when it
/// opens a menu, on the control-chip recipe. `busy` would mute it; the mode
/// is never busy today, so callers pass `false`.
pub fn mode_chip(mode: &str, menu: bool) -> Div {
    control_chip(TEXT_2)
        .child(mode.to_owned())
        .when(menu, |chip| chip.child(chip_chevron()))
}

/// The session-controls trigger: `•••` on the control-chip recipe.
pub fn session_chip() -> Div {
    control_chip(TEXT_MUTED).child("•••")
}

/// The hint row's key hints, on `components::key_hints`' recipe (keys
/// `TEXT_2`, verbs `TEXT_MUTED`, `SPACE_3` between pairs) with one
/// difference: each pair keeps its width, and pairs that do not fit wrap
/// onto a second line the row's height clips away — a narrow row drops
/// whole hints, never half of one. A zero-width lead keeps even the first
/// pair honest: a line always takes one item, and it is the lead. `lead`
/// (the plain mode chip) goes first and gives way the same way.
fn hint_row(lead: Option<Div>, hints: &[(&'static str, &'static str)]) -> Div {
    components::text_meta()
        .flex()
        .flex_1()
        .flex_wrap()
        .min_w_0()
        .h(px(theme::COMPOSER_ROW_H))
        .pt(px((theme::COMPOSER_ROW_H - theme::LH_META) / 2.))
        .overflow_hidden()
        .child(div().flex_shrink_0().w(px(0.)).h(px(theme::LH_META)))
        .children(lead.map(|lead| lead.mt(px((theme::LH_META - theme::CHIP_H) / 2.))))
        .children(hints.iter().map(|(key, verb)| {
            div()
                .flex()
                .flex_shrink_0()
                .mr(px(theme::SPACE_3))
                .gap(px(theme::SPACE_1))
                .whitespace_nowrap()
                .child(div().text_color(rgb(TEXT_2)).child(*key))
                .child(*verb)
        }))
}

/// The L2 hint row with an empty line: the two menus, nothing else fits.
const COMPACT_HINTS: &[(&str, &str)] = &[("@", "files"), ("/", "commands")];

/// The hint row while the line has text: what Enter does now (it queues
/// behind a running turn), and how to break the line instead.
fn typing_hints(needs_queue: bool) -> &'static [(&'static str, &'static str)] {
    if needs_queue {
        &[("↵", "queue"), ("⇧↵", "newline")]
    } else {
        &[("↵", "send"), ("⇧↵", "newline")]
    }
}

/// The hints under an empty line, on the hint row. A showing prediction
/// takes the first slot: it is the only one of these the operator cannot
/// discover by looking at the box, and an accept key nobody knows about is
/// the same as no accept key. `esc` is never here: Stop carries it.
fn composer_hints(
    is_draft: bool,
    history_available: bool,
    suggested: bool,
) -> &'static [(&'static str, &'static str)] {
    match (is_draft, suggested, history_available) {
        (true, _, _) => &[("@", "project files"), ("/import", "session")],
        (false, true, true) => &[("⇥", "accept"), ("↑", "history"), ("@", "files")],
        (false, true, false) => &[("⇥", "accept"), ("@", "files"), ("/", "commands")],
        (false, false, true) => &[("↑", "history"), ("@", "files"), ("/", "commands")],
        (false, false, false) => &[("@", "files"), ("/", "commands")],
    }
}

/// Hints as the row reads them, `key verb` pairs three spaces apart — for
/// tests and copy.
#[cfg(test)]
fn hint_text(hints: &[(&str, &str)]) -> String {
    hints
        .iter()
        .map(|(key, verb)| format!("{key} {verb}"))
        .collect::<Vec<_>>()
        .join("   ")
}

/// The idle line's ghost text (§D.7): the prototype's three, plus the
/// predicted follow-up when one has landed. A prediction is already in the
/// operator's voice and already filtered, so it is shown verbatim — it is a
/// draft of their next prompt, not a description of one, which is what lets
/// Tab accept it. It never names the Thread and never lists the hints; the
/// `.hint` on the same row already does that.
fn placeholder(
    pending: bool,
    transcript: Option<&Transcript>,
    suggestion: Option<&str>,
) -> SharedString {
    match followup::suggest(pending, transcript, suggestion) {
        Followup::Decision => SharedString::from("Reply to the Decision\u{2026}"),
        Followup::Revive => SharedString::from("Revive and continue\u{2026}"),
        Followup::Suggested(text) => SharedString::from(text),
        Followup::Steer => SharedString::from("Steer this Thread\u{2026}"),
    }
}

/// #11: whether this Thread still offers adopting a CLI session — no
/// conversation yet (nothing in the transcript beyond Ferrite's own notices
/// and bookkeeping) and at rest. One predicate for every surface that opens
/// the door — the placeholder hint, the `/` menu's local entry, and the
/// pick that closes the blank Thread — so no two can disagree.
pub fn offers_import(transcript: Option<&Transcript>) -> bool {
    transcript.is_some_and(Transcript::offers_import)
}

/// Display the adapter's label for its native mode; unknown values stay visible.
pub fn permission_mode_label(
    mode: &str,
    choices: &[ferrite_core::PermissionModeChoice],
) -> SharedString {
    choices
        .iter()
        .find(|choice| choice.value == mode)
        .map(|choice| choice.label.clone())
        .unwrap_or_else(|| mode.to_owned())
        .into()
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
    /// Whether `detail` is a description (cut at its end) or a path (cut at
    /// its head, so the useful tail survives). Both draw mono.
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
    components::menu_row(id, &menu_item(row, cursor, label_w), cursor, false)
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
        item = item.detail(detail);
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
pub(crate) fn composer_queue_height(height: f32, compact: bool, count: usize) -> f32 {
    let budget = height * theme::COMPOSER_MAX_PANE_FRACTION
        - composer_fixed_height(compact)
        - theme::COMPOSER_ROW_H;
    let fitting = (budget / (theme::QUEUE_ROW_H + theme::COMPOSER_GAP))
        .floor()
        .max(1.) as usize;
    let rows = count.min(fitting).min(if compact {
        theme::COMPOSER_COMPACT_QUEUE_ROWS
    } else {
        theme::COMPOSER_QUEUE_ROWS
    });
    rows as f32 * theme::QUEUE_ROW_H + rows.saturating_sub(1) as f32 * theme::COMPOSER_GAP
}

/// The Composer's height less its editor rows and queue: the inset below
/// the block, the block's two edges and padding, and the hint row with the
/// gap above it. The shelf floats above and is not part of the budget.
fn composer_fixed_height(compact: bool) -> f32 {
    let inset = if compact {
        theme::COMPOSER_INSET_L2
    } else {
        theme::COMPOSER_INSET_B
    };
    inset
        + 2. * theme::COMPOSER_EDGE_W
        + theme::COMPOSER_PAD_T
        + theme::COMPOSER_PAD_B
        + theme::COMPOSER_GAP
        + theme::COMPOSER_ROW_H
}

/// Leave the majority of a Pane available for its Thread context. Only the
/// viewport changes: the Composer keeps every character and scrolls to its
/// caret, then reveals more rows again when the Pane grows.
pub(crate) fn composer_row_limit(height: f32, compact: bool, queued: usize) -> usize {
    let fixed = composer_fixed_height(compact);
    let queue = composer_queue_height(height, compact, queued)
        + if queued > 0 { theme::COMPOSER_GAP } else { 0. };
    ((height * theme::COMPOSER_MAX_PANE_FRACTION - fixed - queue) / theme::COMPOSER_ROW_H)
        .floor()
        .max(1.)
        .min(crate::composer::MAX_ROWS as f32) as usize
}

/// A prompt written while the agent was still working: a dim `❯` line, in
/// the grammar it will enter the transcript with. `index` counts down the
/// pile from the top; only the top row (0, the latest) carries the count
/// and — while the line is empty, the only time they act — the keys that
/// take it back: ↑ restores it into the line, ⌫ drops it.
fn queued_line(held: &str, index: usize, count: usize, empty: bool) -> impl IntoElement {
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
                    .child(
                        components::text_meta().child(SharedString::from(if count > 1 {
                            format!("{count} queued")
                        } else {
                            "queued".to_owned()
                        })),
                    )
                    .when(empty, |keys| {
                        keys.child(components::key_hints(&[("↑", "edit"), ("⌫", "drop")]))
                    }),
            )
        })
}

/// The exact tool input an approval would send. Commands retain their source;
/// other provider input remains inspectable as its JSON value.
pub(crate) fn approval_input(
    decision: &Decision,
    cache: &crate::rich::TextCache,
    id: SharedString,
) -> Option<AnyElement> {
    use gpui::component::scroll::ScrollableElement as _;

    if questions_of(decision).is_some() {
        return None;
    }
    let source = decision
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
        })?;
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

/// The Decision's subject — what it wants to do, tool-prefixed the comps'
/// way: `Bash: gh issue close 212`; the tool's name alone without a
/// description, else the honest unreadable fallback. Every surface that
/// names a Decision (L1 card, L2 cell, wall alert) goes through here.
fn decision_subject(decision: &Decision) -> SharedString {
    if let Some(questions) = questions_of(decision) {
        return SharedString::from(ferrite_core::questions::summary(questions));
    }
    match (
        decision.tool_name.is_empty(),
        decision.description.is_empty(),
    ) {
        (false, false) => {
            SharedString::from(format!("{}: {}", decision.tool_name, decision.description))
        }
        (true, false) => SharedString::from(decision.description.clone()),
        (false, true) => SharedString::from(decision.tool_name.clone()),
        (true, true) => SharedString::from("unreadable permission request"),
    }
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
pub fn keycap_allow() -> Stateful<Div> {
    decision::key_action("y allow", "y", "allow")
}
pub fn keycap_deny() -> Stateful<Div> {
    decision::key_action("n deny", "n", "deny").debug_selector(|| "decision-deny".into())
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
fn reset_label(resets_at: Option<u64>, span: Duration, now: SystemTime) -> SharedString {
    let Some(resets_at) = resets_at else {
        return SharedString::from("reset not reported");
    };
    let now = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let Some(remaining) = resets_at
        .checked_sub(now)
        .filter(|remaining| *remaining <= span.as_secs())
    else {
        return SharedString::from("reset not reported");
    };
    let label = match remaining {
        0 => "reset not reported".into(),
        1..=59 => "resets in <1m".into(),
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
    SharedString::from(label)
}

/// The usage meter's detail card: the meter's own three windows, in the
/// meter's own order, each a labelled bar over the reading behind it.
/// Counts are reported values, never estimates — a window the provider has
/// not reported keeps its empty track and says so, rather than reading as
/// zero used.
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
    let bar = |fraction: Option<f32>| {
        let used = fraction.unwrap_or(0.).clamp(0., 1.);
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
    // right — the one line that answers the question at a glance.
    let heading = |label: &'static str, value: AnyElement| {
        div()
            .flex()
            .items_baseline()
            .justify_between()
            .gap(px(theme::SPACE_3))
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(rgb(TEXT_MUTED))
                    .child(label),
            )
            .child(value)
    };
    let percent_value = |key: &'static str, fraction: Option<f32>| {
        let percent = fraction.map(|fraction| (fraction.clamp(0., 1.) * 100.).round() as u32);
        div()
            .id(key)
            .debug_selector(move || {
                format!(
                    "context-usage-{key}-{}",
                    percent.map_or("unknown".into(), |n| n.to_string())
                )
            })
            .flex_shrink_0()
            .when(percent.is_none(), |value| value.text_color(rgb(TEXT_MUTED)))
            .child(SharedString::from(
                percent
                    .map(|percent| format!("{percent}%"))
                    .unwrap_or_else(|| "not reported".into()),
            ))
    };
    let count_value = |key: &'static str, count: Option<u64>| {
        div()
            .id(key)
            .debug_selector(move || {
                format!(
                    "context-usage-{key}-{}",
                    count.map_or("unknown".into(), |n| n.to_string())
                )
            })
            .flex_shrink_0()
            .child(SharedString::from(
                count
                    .map(count_label)
                    .unwrap_or_else(|| "not reported".into()),
            ))
    };
    let reset_value = |key: &'static str, resets_at: Option<u64>, span: Duration| {
        div()
            .id(SharedString::from(format!("reset-{key}")))
            .debug_selector(move || {
                format!(
                    "context-usage-{key}-reset-{}",
                    if resets_at.is_some() {
                        "reported"
                    } else {
                        "unknown"
                    }
                )
            })
            .text_color(rgb(TEXT_MUTED))
            .child(reset_label(resets_at, span, now))
            .into_any_element()
    };
    let window = |label: &'static str,
                  key: &'static str,
                  fraction: Option<f32>,
                  detail: Option<AnyElement>| {
        let mut block = div()
            .flex()
            .flex_col()
            .gap(px(theme::USAGE_CARD_ROW_GAP))
            .child(heading(
                label,
                percent_value(key, fraction).into_any_element(),
            ))
            .child(bar(fraction));
        if let Some(detail) = detail {
            block = block.child(detail);
        }
        block
    };
    let context_fraction = maximum.map(|maximum| usage.total_tokens as f32 / maximum as f32);
    // The counts behind the context bar, in the card's quietest ink: the
    // bar says how full, this says of what.
    let counts = div()
        .flex()
        .gap(px(theme::SPACE_1))
        .text_color(rgb(TEXT_MUTED))
        .child(count_value("current", Some(usage.total_tokens)))
        .child("/")
        .child(count_value("maximum", maximum))
        .child("tokens");
    let mut card = div()
        .flex()
        .flex_col()
        .w(px(theme::USAGE_CARD_W))
        .gap(px(theme::USAGE_CARD_GAP))
        .p(px(theme::USAGE_CARD_PAD))
        .text_size(px(theme::FS_SM))
        .text_color(rgb(TEXT))
        .child(window(
            "context",
            "context",
            context_fraction,
            Some(counts.into_any_element()),
        ))
        .child(window(
            "5-hour",
            "five-hour",
            limits.five_hour.map(|limit| limit.used_fraction),
            Some(reset_value(
                "five-hour",
                limits.five_hour.and_then(|limit| limit.resets_at),
                Duration::from_secs(5 * 3_600),
            )),
        ))
        .child(window(
            "weekly",
            "weekly",
            limits.weekly.map(|limit| limit.used_fraction),
            Some(reset_value(
                "weekly",
                limits.weekly.and_then(|limit| limit.resets_at),
                Duration::from_secs(7 * 86_400),
            )),
        ));
    // Everything below the windows is a terminal readout: a quiet key at
    // the left, the reported value right-aligned in tabular digits, every
    // count grouped the same way, and a section head where the scope
    // changes.
    let row = |key: String, value: String| {
        div()
            .flex()
            .justify_between()
            .gap(px(theme::SPACE_3))
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
        let mut section = div().flex().flex_col();
        if let Some(usable) = details.usable_window {
            section = section.child(
                row("usable".into(), count_label(usable))
                    .id(SharedString::from(format!("context-usable-{usable}")))
                    .debug_selector(move || format!("context-usable-{usable}")),
            );
        }
        if let Some(threshold) = details.auto_compact_threshold {
            let key = match details.is_auto_compact_enabled {
                Some(true) => "compacts at",
                Some(false) => "compaction off at",
                None => "compaction threshold",
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
                row(category.name.to_lowercase(), count_label(tokens))
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
            ferrite_core::UsageScope::Message => ("message", "this message"),
            ferrite_core::UsageScope::Turn => ("turn", "this turn"),
            ferrite_core::UsageScope::Session => ("session", "this session"),
        };
        let mut section = div().flex().flex_col().child(
            div()
                .debug_selector(move || format!("usage-scope-{}", scope.0))
                .text_color(rgb(TEXT_2))
                .child(scope.1),
        );
        for (key, label, count) in [
            ("input", "input", details.input_tokens),
            ("cached-input", "cached input", details.cached_input_tokens),
            ("output", "output", details.output_tokens),
            (
                "reasoning-output",
                "reasoning output",
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
                row("last cost".into(), format!("US${cost:.4}"))
                    .debug_selector(move || format!("usage-cost-{cost}")),
            );
        }
        card = card.child(section);
    } else if let Some(cost) = last_cost {
        card = card.child(
            row("last cost".into(), format!("US${cost:.4}"))
                .debug_selector(move || format!("usage-cost-{cost}")),
        );
    }
    card.max_h(px(440.)).overflow_y_scrollbar()
}

/// A usage reading's ink: neutral `TEXT_2` until the window runs tight,
/// then `ATTENTION`, then `BLOCKED` when it is all but spent. Colour is
/// state; a context half full is not one.
pub fn usage_ink(fraction: f32) -> u32 {
    match fraction {
        fraction if fraction >= theme::USAGE_SPENT => BLOCKED,
        fraction if fraction >= theme::USAGE_TIGHT => ATTENTION,
        _ => TEXT_2,
    }
}

/// The Composer meter's body: a terminal readout — `ctx 62%`, plus the
/// tightest account window once one runs tight (`5h 91%`) — then whichever
/// mark the operator chose (Settings › Appearance): three stacked lines, or
/// three rings in a row. Both marks draw the same three windows in the same
/// fixed order, so the card behind the click explains either one. `context`
/// is `None` where no window has been reported (a draft, a fresh Session):
/// the readout says `ctx —` rather than claiming an empty window.
pub fn usage_meter_body(
    style: ferrite_core::settings::UsageMeterStyle,
    context: Option<f32>,
    limits: ferrite_core::transcript::RateLimits,
) -> Div {
    let mark = match style {
        ferrite_core::settings::UsageMeterStyle::Lines => {
            usage_lines(context.unwrap_or(0.), limits)
        }
        ferrite_core::settings::UsageMeterStyle::Rings => {
            usage_rings(context.unwrap_or(0.), limits)
        }
    };
    control_chip(TEXT_MUTED)
        .gap(px(theme::SPACE_2))
        .child(usage_readout(context, limits))
        .child(mark)
}

/// `ctx 62%`: the context window's use in a fixed-width percent column, and
/// the tightest account window appended only while it runs tight.
fn usage_readout(context: Option<f32>, limits: ferrite_core::transcript::RateLimits) -> Div {
    let percent = |fraction: f32| (fraction.clamp(0., 1.) * 100.).round() as u32;
    let key = context.map_or_else(|| "unknown".to_owned(), |used| percent(used).to_string());
    let worst = [
        ("5h", limits.five_hour.map(|limit| limit.used_fraction)),
        ("wk", limits.weekly.map(|limit| limit.used_fraction)),
    ]
    .into_iter()
    .filter_map(|(name, used)| used.map(|used| (name, used)))
    .filter(|(_, used)| *used >= theme::USAGE_TIGHT)
    .max_by(|a, b| a.1.total_cmp(&b.1));
    components::tabular(
        div()
            .debug_selector(move || format!("usage-readout-{key}"))
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(theme::SPACE_1))
            .child("ctx")
            .child(
                div()
                    .min_w(px(theme::USAGE_READOUT_W))
                    .text_color(rgb(context.map_or(TEXT_MUTED, usage_ink)))
                    .child(SharedString::from(match context {
                        Some(used) => format!("{}%", percent(used)),
                        None => "—".to_owned(),
                    })),
            )
            .children(worst.map(|(name, used)| {
                div().flex().gap(px(theme::SPACE_1)).child(name).child(
                    div()
                        .text_color(rgb(usage_ink(used)))
                        .child(SharedString::from(format!("{}%", percent(used)))),
                )
            })),
    )
}

/// The same three windows as `usage_lines`, drawn as three 14px rings side
/// by side. A window the provider has not reported keeps its unlit track,
/// exactly as its line would.
pub fn usage_rings(context: f32, limits: ferrite_core::transcript::RateLimits) -> Div {
    let ring = |key: &'static str, fraction: Option<f32>| {
        let used = fraction.unwrap_or(0.).clamp(0., 1.);
        let percent = (used * 100.).round() as u32;
        div()
            .id(key)
            .debug_selector(move || format!("usage-ring-{key}-{percent}"))
            .child(usage_ring(used, usage_ink(used)))
    };
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::USAGE_RING_GAP))
        .h(px(theme::CHIP_H))
        .child(ring("context", Some(context)))
        .child(ring(
            "five-hour",
            limits.five_hour.map(|limit| limit.used_fraction),
        ))
        .child(ring(
            "weekly",
            limits.weekly.map(|limit| limit.used_fraction),
        ))
}

/// Three quiet horizontal lines for context, five-hour and weekly usage.
/// The fixed order makes the tiny meter scannable; unknown provider values
/// retain their tracks and are explained as such in the click-through card.
pub fn usage_lines(context: f32, limits: ferrite_core::transcript::RateLimits) -> Div {
    let line = |key: &'static str, fraction: Option<f32>| {
        let used = fraction.unwrap_or(0.).clamp(0., 1.);
        let percent = (used * 100.).round() as u32;
        div()
            .id(key)
            .debug_selector(move || format!("usage-line-{key}-{percent}"))
            .w(px(theme::USAGE_LINE_W))
            .h(px(theme::USAGE_LINE_H))
            .rounded(px(theme::USAGE_LINE_H / 2.))
            .bg(rgba(METER_OFF))
            .child(
                div()
                    .h_full()
                    .w(relative(used))
                    .rounded(px(theme::USAGE_LINE_H / 2.))
                    .bg(rgb(usage_ink(used))),
            )
    };
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .justify_center()
        .gap(px(theme::USAGE_LINE_GAP))
        .h(px(theme::CHIP_H))
        .child(line("context", Some(context)))
        .child(line(
            "five-hour",
            limits.five_hour.map(|limit| limit.used_fraction),
        ))
        .child(line(
            "weekly",
            limits.weekly.map(|limit| limit.used_fraction),
        ))
}

/// The context ring (§G.10): a 14px box holding a 5.4px-radius, 2px-stroke
/// circle — a `--meter-off` track under an arc that sweeps clockwise from
/// 12 o'clock with the used fraction of the window.
///
/// The header stays compact; its caller wires the token card on click.
///
/// `PathBuilder::arc_to` draws the real arc — gpui 0.2.2 has an arc
/// primitive, whatever the old comment here claimed.
/// The ring takes its ink from the caller: the meter's three rings wear
/// the same status inks its lines do, so a budget reads the same whichever
/// mark the operator picked.
pub fn usage_ring(fraction: f32, ink: u32) -> Div {
    // A full ring's seam would degenerate the arc; one part in a thousand
    // is invisible at 14px.
    let fraction = fraction.clamp(0.0, 1.0).min(0.999);
    div()
        .relative()
        .flex_shrink_0()
        .w(px(theme::USAGE_RING_D))
        .h(px(theme::USAGE_RING_D))
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    // The circle the prototype draws is `USAGE_RING_R` /
                    // `USAGE_RING_W`; these are what gpui has to be *asked*
                    // for to land on it. lyon's arc approximation pulls the
                    // curve inward by ~0.32px and the stroke rasterises
                    // ~0.5px thin, so the ink measured 12.0px across where
                    // the prototype measures 12.7px. The compensation lives
                    // here, at the rasteriser, and never in theme.rs.
                    const ARC_R: f32 = theme::USAGE_RING_R + 0.15;
                    const ARC_W: f32 = theme::USAGE_RING_W + 0.25;
                    let radius = px(ARC_R);
                    let centre = bounds.center();
                    let sweep = fraction * std::f32::consts::TAU;
                    let start = -std::f32::consts::FRAC_PI_2;
                    let at = |angle: f32| {
                        point(
                            centre.x + radius * angle.cos(),
                            centre.y + radius * angle.sin(),
                        )
                    };
                    // The caps are quads, not paths: they rasterise exactly,
                    // so they sit on the true centreline at the true radius.
                    let cap_at = |angle: f32| {
                        point(
                            centre.x + px(theme::USAGE_RING_R) * angle.cos(),
                            centre.y + px(theme::USAGE_RING_R) * angle.sin(),
                        )
                    };
                    let stroke = |from: f32, to: f32, large: bool| {
                        let mut arc = PathBuilder::stroke(px(ARC_W));
                        arc.move_to(at(from));
                        arc.arc_to(point(radius, radius), px(0.), large, true, at(to));
                        arc.build().ok()
                    };
                    // The unlit track is the same circle as the used arc —
                    // painted, not a bordered box, because gpui rounds a
                    // box's inset to a whole pixel and the ring's radius is
                    // 5.4. Drawn as two halves; a closed circle would
                    // degenerate the arc.
                    if let Some(path) = stroke(start, start + std::f32::consts::PI, false) {
                        window.paint_path(path, rgba(METER_OFF));
                    }
                    if let Some(path) = stroke(
                        start + std::f32::consts::PI,
                        start + std::f32::consts::TAU - 0.001,
                        false,
                    ) {
                        window.paint_path(path, rgba(METER_OFF));
                    }
                    if fraction <= 0.0 {
                        return;
                    }
                    if let Some(path) = stroke(start, start + sweep, fraction > 0.5) {
                        window.paint_path(path, rgb(ink));
                    }
                    // `.used` carries `stroke-linecap: round`; lyon's
                    // default is butt and gpui 0.2.2 re-exports no
                    // `LineCap`, so each cap is painted as its own disc of
                    // the stroke's radius.
                    let cap = px(theme::USAGE_RING_W / 2.0);
                    for angle in [start, start + sweep] {
                        let end = cap_at(angle);
                        window.paint_quad(
                            gpui::fill(
                                gpui::Bounds::new(
                                    point(end.x - cap, end.y - cap),
                                    gpui::size(cap * 2., cap * 2.),
                                ),
                                rgb(ink),
                            )
                            .corner_radii(gpui::Corners::all(cap)),
                        );
                    }
                },
            )
            .absolute()
            .inset_0(),
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
    disclosure: Option<AnyElement>,
    signal: u32,
    _provider: Option<Provider>,
    preview: &crate::attachment_preview::Preview,
    prompt_actions: Option<AnyElement>,
    reduce_motion: bool,
) -> AnyElement {
    let row = div().w_full().min_w_0().flex_shrink_0();
    match &block.body {
        // No band: the accent `❯` and the brightest mono ink carry the
        // prompt. The hover wash bleeds past the text without moving it.
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
                .mx(px(-theme::PROMPT_HOVER_BLEED))
                .my(px(-theme::PROMPT_HOVER_BLEED))
                .p(px(theme::PROMPT_HOVER_BLEED))
                .rounded(px(theme::R_CHIP))
                .hover_row()
                .hover_text()
                .font_family(theme::FONT_MONO)
                .text_size(px(theme::FS_UI))
                .line_height(px(theme::LH_UI))
                .text_color(rgb(TEXT_STRONG))
                .child(components::gutter(
                    components::prompt_mark(ACCENT),
                    theme::LH_UI,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .gap(px(theme::SPACE_1_5))
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
        Body::Paragraph { spans } => prose_row(row)
            .child(prose(block.id, spans, selection))
            .into_any_element(),
        Body::Heading { spans, .. } => prose_row(row)
            .font_weight(theme::W_STRONG)
            .text_color(rgb(TEXT_STRONG))
            .child(prose(block.id, spans, selection))
            .into_any_element(),
        // A fallback list item: Claude's `- ` marker at C1, the text hung
        // `UL_INDENT` in.
        Body::Bullet { spans } => prose_row(row)
            .relative()
            .child(
                div()
                    .absolute()
                    .left(px(theme::GUTTER_W))
                    .top_0()
                    .text_color(rgb(TEXT_MUTED))
                    .child("-"),
            )
            .child(
                div()
                    .pl(px(theme::UL_INDENT))
                    .child(prose(block.id, spans, selection)),
            )
            .into_any_element(),
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
                .font_family(theme::FONT_PROSE)
                .text_size(px(theme::FS_PROSE))
                .line_height(px(theme::LH_PROSE))
                .text_color(rgb(TEXT_MUTED));
            let Some(details) = details else {
                // Nothing more was supplied. Keep the whole short block
                // visible, wrapped and selectable without a false disclosure.
                return row
                    .child(
                        gutter_row(mark, theme::LH_PROSE).child(
                            reasoning.flex_1().min_w_0().child(
                                selection
                                    .markdown(block.id, thought.trim().to_owned())
                                    .muted(),
                            ),
                        ),
                    )
                    .into_any_element();
            };
            let header = gutter_row(mark, theme::LH_PROSE)
                .id(SharedString::from(format!("reasoning-row-{:?}", block.id)))
                .group(DISCLOSURE_ROW)
                .relative()
                .items_center()
                .pr(px(theme::TOOL_DISCLOSURE_HIT))
                .rounded(px(theme::R_CHIP))
                .hover_row()
                .child(
                    div()
                        .debug_selector(|| "reasoning-summary".into())
                        .min_w_0()
                        .truncate()
                        .child(SharedString::from(summary)),
                )
                .children(disclosure);
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
        // A notice: a dot and one mono line. Only the transcript's latest
        // notice wears the Pane's state (`signal`); history stays neutral.
        Body::Notice(text) => {
            let (mark, ink) = if signal == TEXT_MUTED {
                (TEXT_FAINT, TEXT_2)
            } else {
                (signal, signal)
            };
            row.child(
                gutter_row(
                    components::status_dot(mark).size(px(theme::TOOL_DOT)),
                    theme::LH_UI,
                )
                .text_size(px(theme::FS_UI))
                .line_height(px(theme::LH_UI))
                .text_color(rgb(ink))
                .child(div().flex_1().min_w_0().child(selection.line(
                    block.id,
                    text.clone(),
                    separators(text),
                ))),
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
            row,
            block.id,
            tool,
            selection,
            timings,
            expanded,
            disclosure,
            false,
            reduce_motion,
        ),
    }
}

/// The `group` every disclosable row names, so its trailing chevron shows
/// under the pointer anywhere on the row.
const DISCLOSURE_ROW: &str = "disclosure-row";

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

/// Fallback prose at C1, in the answer's face.
fn prose_row(row: Div) -> Div {
    row.pl(px(theme::GUTTER_W))
        .font_family(theme::FONT_PROSE)
        .text_size(px(theme::FS_PROSE))
        .line_height(px(theme::LH_PROSE))
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
/// failed one hangs under the turn's last row as an elbow result, its first
/// word the only coloured one: `Interrupted` in `TEXT_2` (the operator did
/// it; nothing failed), `Failed` in `BLOCKED`.
fn turn_end(block: BlockId, end: &ferrite_core::transcript::TurnEnd, selection: &TextRuns) -> Div {
    use ferrite_core::TurnOutcome;
    let text = end.text();
    let mut highlights = separators(&text);
    match &end.outcome {
        TurnOutcome::Completed => {
            return components::tabular(
                div()
                    .debug_selector(|| "turn-stamp".into())
                    .pl(px(theme::GUTTER_W))
                    .text_size(px(theme::FS_SM))
                    .line_height(px(theme::LH_META))
                    .text_color(rgb(TEXT_MUTED))
                    .child(selection.line(block, text.clone(), highlights)),
            );
        }
        TurnOutcome::Interrupted | TurnOutcome::Error(_) => {
            let lead = text.split(" \u{b7} ").next().unwrap_or_default().len();
            let ink = if matches!(end.outcome, TurnOutcome::Interrupted) {
                TEXT_2
            } else {
                BLOCKED
            };
            highlights.insert(
                0,
                (
                    0..lead,
                    HighlightStyle {
                        color: Some(rgb(ink).into()),
                        ..Default::default()
                    },
                ),
            );
        }
    }
    components::tabular(
        result_line(TEXT_MUTED)
            .debug_selector(|| "turn-stamp".into())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(selection.line(block, text.clone(), highlights)),
            ),
    )
}

/// The `·` seams in a mono line: glyph ink, never a weight. Highlighted in
/// place so the line stays one run and copies back exactly as written.
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
/// recedes (`TEXT_FAINT`), live work is green and breathes, a failure is
/// `BLOCKED`, a call whose result never came is a hollow ring. Green never
/// means finished.
fn tool_dot_ink(state: &ToolState) -> (u32, DotShape) {
    match state {
        ToolState::Running => (RUNNING, DotShape::Pulsing),
        ToolState::Ok => (TEXT_FAINT, DotShape::Solid),
        ToolState::Failed(_) => (BLOCKED, DotShape::Solid),
        ToolState::Unavailable => (TEXT_FAINT, DotShape::Ring),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DotShape {
    Solid,
    Pulsing,
    Ring,
}

fn tool_dot(tool: &ToolBlock, reduce_motion: bool) -> AnyElement {
    let (ink, shape) = tool_dot_ink(&tool.state);
    match shape {
        DotShape::Solid => components::status_dot(ink)
            .size(px(theme::TOOL_DOT))
            .into_any_element(),
        DotShape::Ring => components::status_ring(ink)
            .size(px(theme::TOOL_DOT))
            .into_any_element(),
        DotShape::Pulsing => components::pulsing_dot(
            SharedString::from(format!("tool-dot-{}", tool.call)),
            ink,
            RUNNING_HALO,
            reduce_motion,
        ),
    }
}

/// A call's name and arguments as one line, `Name(args)` with the parens
/// touching: the name in body ink, the rest muted.
fn call_highlights(tool: &ToolBlock) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    vec![(
        0..tool.name.len(),
        HighlightStyle {
            color: Some(rgb(TEXT).into()),
            ..Default::default()
        },
    )]
}

/// A settled call's time, from one second up; below that it is noise.
fn settled_duration(
    tool: &ToolBlock,
    timings: Option<&HashMap<String, ToolTiming>>,
) -> Option<Duration> {
    timings
        .and_then(|map| map.get(&tool.call))
        .and_then(|timing| match timing {
            ToolTiming::Done(total) => Some(*total),
            ToolTiming::Running(_) => None,
        })
        .filter(|total| total.as_millis() >= theme::DURATION_MIN_MS)
}

/// A tool call: `● Name(args)` with its trail hard right — the diff stat,
/// then the time — and what it produced hanging under it on elbows. Nothing
/// on the row is a pill, a bold weight or a state-coloured word except the
/// one `failed` that says so.
///
/// Expanded, it shows `⎿ $ command` (the exact input, `$` for a command
/// runner), the output, and any structured result, none of them labelled.
fn render_tool(
    row: Div,
    block: BlockId,
    tool: &ToolBlock,
    selection: &TextRuns,
    timings: Option<&HashMap<String, ToolTiming>>,
    expanded: bool,
    disclosure: Option<AnyElement>,
    in_group: bool,
    reduce_motion: bool,
) -> AnyElement {
    let has_disclosure = disclosure.is_some();
    let call = div().flex_1().min_w_0().truncate().child(selection.line(
        block,
        text::tool_label(tool),
        call_highlights(tool),
    ));
    let mut trail = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::SPACE_2))
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_UI));
    let mut trailing = false;
    if let Some(ToolVerdict::Diff(added, removed)) = tool_verdicts(tool).into_iter().next() {
        trail = trail.child(diff_stat(added, removed));
        trailing = true;
    }
    if let Some(total) = settled_duration(tool, timings) {
        trail = trail.child(components::tabular(
            div()
                .text_color(rgb(TEXT_MUTED))
                .child(components::duration_label(total)),
        ));
        trailing = true;
    }
    let line = gutter_row(tool_dot(tool, reduce_motion), theme::LH_UI)
        .id(SharedString::from(format!("tool-row-{}", tool.call)))
        .relative()
        .items_center()
        .gap_0()
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(TEXT_MUTED))
        .rounded(px(theme::R_CHIP))
        .when(has_disclosure, |line| {
            line.group(DISCLOSURE_ROW)
                .pr(px(theme::TOOL_DISCLOSURE_HIT))
                .hover_row()
        })
        .child(call)
        .when(trailing, |line| line.child(trail.pl(px(theme::SPACE_3))))
        .children(disclosure);
    let mut card = gpui::component::collapsible::Collapsible::new()
        .w_full()
        .open(expanded)
        .child(line);
    if expanded {
        let mut details = div().flex().flex_col().min_w_0();
        if text::shows_input(tool) {
            details = details.child(output_block(
                block,
                "command",
                &tool.summary,
                TEXT_2,
                ferrite_core::docview::is_command_run(&tool.name),
                selection,
            ));
        }
        if let Some(output) = &tool.output {
            // Ordinary output stays neutral even when a command failed: the
            // dot and the one `failed` word carry the failure.
            details = details.child(output_block(
                block,
                "result",
                &output.text,
                TEXT_MUTED,
                false,
                selection,
            ));
            if output.omitted_bytes > 0 {
                details = details.child(omitted_line(output.omitted_bytes));
            }
        }
        if let Some(structured) = tool.structured_output() {
            details = details.child(output_block(
                block,
                "details",
                &structured.text,
                TEXT_MUTED,
                false,
                selection,
            ));
            if structured.omitted_bytes > 0 {
                details = details.child(omitted_line(structured.omitted_bytes));
            }
        }
        card = card.content(details);
    } else if !text::redundant_test_result(tool)
        && (!in_group || matches!(tool.state, ToolState::Failed(_)))
    {
        if let Some(line) = &tool.result_line {
            card =
                card.child(result_line(result_ink(&tool.state)).child(
                    div().min_w_0().truncate().child(selection.line(
                        block,
                        line.clone(),
                        Vec::new(),
                    )),
                ));
        }
    }
    if tool.state == ToolState::Unavailable {
        card = card.child(result_line(TEXT_MUTED).child("Result unavailable"));
    }
    if !expanded {
        if let ToolState::Failed(message) = &tool.state {
            let first = message
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("");
            if !first.is_empty() && tool.result_line.as_deref() != Some(first) {
                card = card.child(
                    result_line(TEXT_2)
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(rgb(BLOCKED))
                                .child("failed"),
                        )
                        .child(div().flex_shrink_0().text_color(rgb(TEXT_FAINT)).child("·"))
                        .child(div().min_w_0().truncate().child(selection.line(
                            block,
                            first.to_owned(),
                            Vec::new(),
                        ))),
                );
            }
        }
    }
    if expanded || !in_group {
        for diff in &tool.diffs {
            card = card.child(render_diff(block, diff, selection));
        }
    }
    row.child(card).into_any_element()
}

/// A run of tool calls as one quiet line: the core's own summary in
/// `TEXT_MUTED` with its counts one step up, ` · N failed` the only state
/// ink, the chevron trailing. While a member runs the gutter breathes and
/// the running call hangs under the line; failed members stay previewed
/// under a collapsed group (ADR 0003). Expanded, every member is a tool row
/// one gutter in.
pub(crate) fn render_tool_activity_with<S, C>(
    activity: ToolActivity<'_>,
    selection: &TextRuns,
    timings: Option<&HashMap<String, ToolTiming>>,
    expanded: bool,
    disclosure: Option<AnyElement>,
    state: S,
    mut control: C,
    reduce_motion: bool,
) -> AnyElement
where
    S: Fn(&DisclosureId) -> DisclosureState,
    C: FnMut(&DisclosureId) -> Option<AnyElement>,
{
    let call = activity.leader().call.clone();
    let label = text::activity_label(&activity);
    let mut highlights = separators(&label);
    highlights.extend(
        label
            .match_indices(|character: char| character.is_ascii_digit())
            .map(|(at, digit)| {
                (
                    at..at + digit.len(),
                    HighlightStyle {
                        color: Some(rgb(TEXT_2).into()),
                        ..Default::default()
                    },
                )
            }),
    );
    highlights.sort_by_key(|(range, _)| range.start);
    let mark = if activity.running > 0 {
        components::pulsing_dot(
            SharedString::from(format!("live-group-{call}")),
            RUNNING,
            RUNNING_HALO,
            reduce_motion,
        )
    } else {
        div().into_any_element()
    };
    let mut header = gutter_row(mark, theme::LH_UI)
        .id(SharedString::from(format!("tool-group-row-{call}")))
        .group(DISCLOSURE_ROW)
        .relative()
        .items_center()
        .pr(px(theme::TOOL_DISCLOSURE_HIT))
        .rounded(px(theme::R_CHIP))
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(TEXT_MUTED))
        .hover_row()
        .child(div().min_w_0().truncate().child(selection.line(
            activity.blocks[0].id,
            label,
            highlights,
        )));
    if activity.failed > 0 {
        let key = call.clone();
        let failed = format!("{} failed", activity.failed);
        header = header.child(
            div()
                .debug_selector(move || format!("tool-group-failures-{key}"))
                .flex()
                .flex_shrink_0()
                .whitespace_nowrap()
                .child(
                    div()
                        .px(px(theme::MONO_CELL))
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
    let header = header.children(disclosure);
    let mut group = gpui::component::collapsible::Collapsible::new()
        .w_full()
        .open(expanded)
        .child(header);
    if expanded {
        let mut details = div().flex().flex_col().min_w_0().pl(px(theme::GUTTER_W));
        for (index, block) in activity.blocks.iter().enumerate() {
            let Body::Tool(tool) = &block.body else {
                continue;
            };
            details = details.child(
                div()
                    .when(index > 0, |member| member.pt(px(theme::GAP_TOOL)))
                    .child(render_tool(
                        div(),
                        block.id,
                        tool,
                        selection,
                        timings,
                        state(&DisclosureId::Tool(tool.call.clone())) == DisclosureState::Expanded,
                        control(&DisclosureId::Tool(tool.call.clone())),
                        true,
                        reduce_motion,
                    )),
            );
        }
        group = group.content(details);
    } else {
        for block in activity.blocks {
            let Body::Tool(tool) = &block.body else {
                continue;
            };
            if matches!(tool.state, ToolState::Failed(_)) {
                group = group.child(div().pl(px(theme::GUTTER_W)).child(render_tool(
                    div(),
                    block.id,
                    tool,
                    selection,
                    timings,
                    state(&DisclosureId::Tool(tool.call.clone())) == DisclosureState::Expanded,
                    control(&DisclosureId::Tool(tool.call.clone())),
                    true,
                    reduce_motion,
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
            // One line, never the raw multi-line input.
            running
                .w_full()
                .min_w_0()
                .child(
                    result_line(TEXT_2).child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(SharedString::from(text::tool_label(tool))),
                    ),
                )
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

/// A compact result's ink: `TEXT_2` for a failed call's line, the brightest
/// thing under the row so it gets read; muted otherwise. The dot and the
/// word `failed` carry the state.
fn result_ink(state: &ToolState) -> u32 {
    match state {
        ToolState::Failed(_) => TEXT_2,
        _ => TEXT_MUTED,
    }
}

/// A disclosed block of text under its elbow at C2 — a command (`$` first
/// when it is one), output, a structured result. Text draws inline, keeping
/// its whitespace; text past `OUTPUT_INLINE_BYTES` scrolls in one bounded,
/// selectable native control `OUTPUT_MAX_LINES` high, and `… +N lines` under
/// it says how much is out of view.
pub(crate) fn output_block(
    block: BlockId,
    part: &str,
    text: &str,
    ink: u32,
    command: bool,
    selection: &TextRuns,
) -> Div {
    let rows = elbow_row(theme::LH_CODE)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_CODE))
        .text_color(rgb(ink))
        .when(command, |rows| {
            rows.child(div().flex_shrink_0().text_color(rgb(TEXT_MUTED)).child("$"))
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

/// A disclosure row's click target: an overlay over the whole row, so the
/// label and the trail toggle it too, with the chevron **trailing** in a
/// `TOOL_DISCLOSURE_HIT` box at the row's right edge (the row reserves that
/// room). The chevron shows while the pointer is on the row, while the row
/// is open or keyboard-targeted, and always on a collapsed group — the one
/// row whose details are the point. A keyboard target draws the
/// `FOCUS_RING` round the whole row.
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
    let shown = expanded || targeted || matches!(call, DisclosureId::Group(_));
    let control = div()
        .id(SharedString::from(format!("tool-button-{call}")))
        .flex()
        .items_center()
        .justify_center()
        .w(px(theme::TOOL_DISCLOSURE_HIT))
        .h(px(theme::TOOL_DISCLOSURE_HIT))
        .tooltip(move |window, cx| {
            gpui::component::tooltip::Tooltip::new(tooltip).build(window, cx)
        })
        .when(!shown, |control| {
            control
                .invisible()
                .group_hover(DISCLOSURE_ROW, |style| style.visible())
        })
        .child(icon(
            if expanded {
                icons::CHEVRON_DOWN
            } else {
                icons::CHEVRON_RIGHT
            },
            theme::DISCLOSURE_CHEVRON,
            TEXT_MUTED,
        ));
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_end()
        .cursor_pointer()
        // Keyboard cycling outlines the complete disclosure row so the
        // operator can see which row Enter will toggle.
        // The pointer never triggers it.
        .when(targeted, |control| {
            control
                .track_focus(focus)
                .key_context("ToolDisclosure")
                .child(
                    ring_overlay(FOCUS_RING, theme::R_CHIP)
                        .border_color(rgb(FOCUS_RING))
                        .debug_selector(|| "tool-disclosure-keyboard-target".into()),
                )
        })
        .child(
            div()
                .flex_shrink_0()
                .w(px(theme::TOOL_DISCLOSURE_HIT))
                .h(px(theme::TOOL_DISCLOSURE_HIT))
                .child(control),
        )
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

pub fn prompt_actions(block: BlockId) -> PromptActions {
    PromptActions {
        block,
        root: div()
            .id(SharedString::from(format!("prompt-actions-{block:?}")))
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(theme::SPACE_0_5))
            .invisible()
            .group_hover("sent-prompt", |style| style.visible()),
    }
}

fn prompt_action(
    tooltip: &'static str,
    icon_key: &'static str,
    id: impl Into<gpui::ElementId>,
) -> Stateful<Div> {
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
        // The prompt row under it already wears `HOVER`: the button steps
        // up to `FILL` so it still reads as a button.
        .hover_raised()
        .press_raised()
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
        .ml(px(theme::GUTTER_W + theme::ELBOW_INDENT))
        .py(px(theme::HUNK_PAD_Y))
        .rounded(px(theme::R_CHIP))
        .overflow_hidden()
        .bg(rgb(RAISED))
        .text_size(px(theme::FS_UI))
        // A whole-pixel line box: a fractional one rounds each row's origin
        // and height independently, and the added/removed washes can leave a
        // 1px unpainted seam between them.
        .line_height(px(theme::LH_CODE))
        .text_color(rgb(TEXT_MUTED));
    let columns = |number: SharedString, sign: &'static str, sign_color: u32| {
        div()
            .flex()
            .px(px(theme::HUNK_PAD_X))
            .child(components::tabular(
                div()
                    .flex_shrink_0()
                    .w(px(number_w))
                    .text_right()
                    .text_color(rgb(TEXT_MUTED))
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
            lines = lines.child(columns("…".into(), "", TEXT_MUTED));
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
            lines = lines.child(
                columns(SharedString::from(number.to_string()), sign, sign_color)
                    .when_some(wash, |row, wash| row.bg(rgba(wash)))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .whitespace_nowrap()
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
    (digits.max(2) as f32 * theme::MONO_CELL).ceil()
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
    /// colour — green added, red removed, a step lighter than the sign so a
    /// whole line stays readable on its wash — and a context line is muted.
    fn paint(self) -> DiffPaint {
        match self {
            Self::Added => DiffPaint {
                sign: "+",
                sign_color: RUNNING,
                code_color: DIFF_ADDED_INK,
                wash: Some(RUNNING_WASH),
            },
            Self::Removed => DiffPaint {
                sign: "-",
                sign_color: BLOCKED,
                code_color: DIFF_REMOVED_INK,
                wash: Some(BLOCKED_WASH),
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
            font_weight: Some(FontWeight::SEMIBOLD),
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
/// their own box, sized from the mono advance (JetBrains Mono is 600/1000
/// em, the 0.6 below), and the code follows in a second run. The spaces are
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
    #[test]
    fn footer_advertises_history_only_when_the_context_is_armed() {
        assert_eq!(
            hint_text(composer_hints(false, true, false)),
            "↑ history   @ files   / commands"
        );
        assert_eq!(
            hint_text(composer_hints(false, false, false)),
            "@ files   / commands"
        );
        assert_eq!(
            hint_text(composer_hints(true, true, false)),
            "@ project files   /import session",
            "drafts never advertise Thread history"
        );
        // Stop carries `esc`; no hint row ever repeats it.
        for (draft, history, suggested) in [(false, true, true), (true, false, false)] {
            assert!(!hint_text(composer_hints(draft, history, suggested)).contains("esc"));
        }
        assert!(!hint_text(typing_hints(true)).contains("esc"));
    }

    /// A prediction the operator can accept must say so: the key is the one
    /// thing about it the box itself cannot show.
    #[test]
    fn footer_advertises_the_accept_key_while_a_prediction_shows() {
        assert!(hint_text(composer_hints(false, true, true)).starts_with("⇥ accept"));
        assert!(hint_text(composer_hints(false, false, true)).starts_with("⇥ accept"));
        assert!(
            !hint_text(composer_hints(true, true, true)).contains("accept"),
            "a draft has no conversation to predict from"
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
                .child(working_line(&self.0, false, false, false))
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
                .font_family(crate::theme::FONT_MONO)
                .text_size(px(12.))
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
                                    description: None,
                                    recommended: false,
                                    selected: false,
                                    enabled: row.enabled,
                                    prose: false,
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
                            approval_input(decision, &self.cache, "decision-reference".into())
                                .map(|input| decision::well(input).into_any_element()),
                        )
                        .chain(rows),
                    )
                }))
                .children(self.decisions.iter().map(|decision| {
                    l2_decision_body(
                        decision,
                        Some(
                            decision::key_actions()
                                .child(keycap_allow())
                                .child(keycap_deny())
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
        assert_eq!(cursor(keycap_allow()), Some(CursorStyle::PointingHand));
        assert_eq!(cursor(keycap_deny()), Some(CursorStyle::PointingHand));
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
        assert_eq!(red.failing.as_ref(), "2 failing");

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
            "Bash: gh issue close 212"
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
        assert_eq!(decision_subject(&full).as_ref(), "Bash: gh issue close 212");
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
            permission_mode_label("opaque-mode", &choices).as_ref(),
            "Ask for changes"
        );
        assert_eq!(
            permission_mode_label("unknown", &choices).as_ref(),
            "unknown"
        );
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
                sign_color: RUNNING,
                code_color: DIFF_ADDED_INK,
                wash: Some(RUNNING_WASH),
            }
        );
        assert_eq!(
            DiffKind::Removed.paint(),
            DiffPaint {
                sign: "-",
                sign_color: BLOCKED,
                code_color: DIFF_REMOVED_INK,
                wash: Some(BLOCKED_WASH),
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
        // Settled work recedes; live work is green and breathes; a failure
        // is the blocked dot; a lost result is a hollow ring. Green never
        // means finished.
        assert_eq!(tool_dot_ink(&ToolState::Ok), (TEXT_FAINT, DotShape::Solid));
        assert_eq!(
            tool_dot_ink(&ToolState::Running),
            (RUNNING, DotShape::Pulsing)
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
            assert_eq!(highlights.len(), 1);
            assert_eq!(highlights[0].0, 0..4, "only the name is lifted");
            assert_eq!(highlights[0].1.color, Some(rgb(TEXT).into()));
            assert_eq!(highlights[0].1.font_weight, None, "names are never bold");
        }
        assert_eq!(result_ink(&ToolState::Ok), TEXT_MUTED);
        assert_eq!(
            result_ink(&failed),
            TEXT_2,
            "the failure's message is read; the dot and one word carry the state"
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
        let cell = theme::MONO_CELL;
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

    #[test]
    fn durations_read_at_the_comps_grammar() {
        assert_eq!(
            components::duration_label(Duration::from_millis(340)).as_ref(),
            "0.3s"
        );
        assert_eq!(
            components::duration_label(Duration::from_millis(8_200)).as_ref(),
            "8.2s"
        );
        assert_eq!(
            components::duration_label(Duration::from_secs(42)).as_ref(),
            "42s"
        );
        assert_eq!(
            components::duration_label(Duration::from_secs(134)).as_ref(),
            "2m14s"
        );
    }

    #[test]
    fn rate_limit_resets_read_as_compact_countdowns() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        let after = |seconds: u64| Some(1_000_000 + seconds);
        let week = Duration::from_secs(7 * 86_400);

        assert_eq!(reset_label(None, week, now).as_ref(), "reset not reported");
        assert_eq!(reset_label(after(45), week, now).as_ref(), "resets in <1m");
        assert_eq!(
            reset_label(after(42 * 60), week, now).as_ref(),
            "resets in 42m"
        );
        assert_eq!(
            reset_label(after(3 * 3_600 + 14 * 60), week, now).as_ref(),
            "resets in 3h 14m"
        );
        assert_eq!(
            reset_label(after(4 * 86_400 + 2 * 3_600), week, now).as_ref(),
            "resets in 4d 2h"
        );
        assert_eq!(
            reset_label(Some(999_999), week, now).as_ref(),
            "reset not reported",
            "an elapsed or relative provider timestamp must not underflow"
        );
        assert_eq!(
            reset_label(after(8 * 86_400), week, now).as_ref(),
            "reset not reported",
            "a value outside the window span is not guessed to be Unix seconds"
        );
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
        assert_eq!(PaneEdge::of(true, true, true), PaneEdge::Blocked);
        assert_eq!(PaneEdge::of(false, true, true), PaneEdge::Blocked);
        assert_eq!(PaneEdge::of(true, true, false), PaneEdge::Attention);
        assert_eq!(PaneEdge::of(false, true, false), PaneEdge::Attention);
        assert_eq!(PaneEdge::of(true, false, false), PaneEdge::Focused);
        assert_eq!(PaneEdge::of(false, false, false), PaneEdge::Rest);
        assert_eq!(PaneEdge::Rest.ink(), rgba(HAIRLINE).into());
        assert_eq!(PaneEdge::Focused.ink(), rgb(FOCUS_RING).into());
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
            placeholder(false, Some(&live), None),
            "Steer this Thread\u{2026}"
        );
        assert_eq!(
            placeholder(true, Some(&live), None),
            "Reply to the Decision\u{2026}"
        );

        let mut closed = Transcript::default();
        closed.apply(Input::Event(SessionEvent::Closed {
            reason: "the CLI exited".into(),
        }));
        assert_eq!(
            placeholder(false, Some(&closed), None),
            "Revive and continue\u{2026}"
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
            placeholder(false, Some(&answered), Some("Run the tests")),
            "Run the tests"
        );
        // A Decision and a dead Session both outrank it.
        assert_eq!(
            placeholder(true, Some(&answered), Some("Run the tests")),
            "Reply to the Decision\u{2026}"
        );
        assert_eq!(
            placeholder(false, Some(&closed), Some("Run the tests")),
            "Revive and continue\u{2026}"
        );

        for line in [
            placeholder(false, Some(&live), None),
            placeholder(true, Some(&live), None),
            placeholder(false, Some(&closed), None),
        ] {
            assert!(!line.contains("message"), "{line}");
            assert!(!line.contains("commands"), "{line}");
        }
    }

    // ---- WP-A tests (append above the end line)
    // (end WP-A)

    // ---- WP-B tests (append above the end line)
    // (end WP-B)

    // ---- WP-C tests (append above the end line)
    // (end WP-C)

    // ---- WP-D tests (append above the end line)
    /// With text in the line the row says what Enter does now: behind a
    /// running turn it queues, not sends.
    #[test]
    fn typing_hints_say_whether_enter_sends_or_queues() {
        assert_eq!(hint_text(typing_hints(false)), "↵ send   ⇧↵ newline");
        assert_eq!(hint_text(typing_hints(true)), "↵ queue   ⇧↵ newline");
    }

    /// The `❯` says where keys land: accent only while the line holds the
    /// keyboard, attention when that line answers a Decision.
    #[test]
    fn the_prompt_mark_lights_only_while_the_line_holds_the_keyboard() {
        assert_eq!(prompt_ink(true, false), ACCENT);
        assert_eq!(prompt_ink(true, true), ATTENTION);
        assert_eq!(prompt_ink(false, false), TEXT_MUTED);
        assert_eq!(prompt_ink(false, true), TEXT_MUTED);
    }

    /// The block draws its own focus edge only where the Pane's edge is a
    /// state colour and the keyboard is in the line (P0-8); otherwise the
    /// resting edge, always 1px, always in layout.
    #[test]
    fn the_composer_edge_carries_focus_only_on_an_alert_pane() {
        assert_eq!(
            composer_edge(true, true, false),
            (theme::FOCUS_RING << 8) | 0xff
        );
        for (alert, editing) in [(false, false), (false, true), (true, false)] {
            assert_eq!(composer_edge(alert, editing, false), theme::COMPOSER_EDGE);
        }
    }

    /// The height budget counts exactly what the block draws around its
    /// editor rows: inset, two edges, padding, the gap and the hint row.
    #[test]
    fn the_fixed_height_follows_the_composer_tokens() {
        let block = 2. * theme::COMPOSER_EDGE_W
            + theme::COMPOSER_PAD_T
            + theme::COMPOSER_PAD_B
            + theme::COMPOSER_GAP
            + theme::COMPOSER_ROW_H;
        assert_eq!(
            composer_fixed_height(false),
            theme::COMPOSER_INSET_B + block
        );
        assert_eq!(
            composer_fixed_height(true),
            theme::COMPOSER_INSET_L2 + block
        );
        assert_eq!(
            theme::BOX_INSET_X,
            theme::COMPOSER_EDGE_W + theme::COMPOSER_PAD_X
        );
        // A tall Pane shows every editor row; a short one keeps its majority.
        assert_eq!(
            composer_row_limit(900., false, 0),
            crate::composer::MAX_ROWS
        );
        let limit = composer_row_limit(300., false, 0) as f32;
        assert!(
            composer_fixed_height(false) + limit * theme::COMPOSER_ROW_H
                <= 300. * theme::COMPOSER_MAX_PANE_FRACTION
        );
    }

    /// Usage is neutral until it runs tight: colour is state.
    #[test]
    fn usage_reads_neutral_until_it_runs_tight() {
        assert_eq!(usage_ink(0.62), TEXT_2);
        assert_eq!(usage_ink(theme::USAGE_TIGHT), ATTENTION);
        assert_eq!(usage_ink(theme::USAGE_SPENT), BLOCKED);
    }
    // (end WP-D)

    // ---- WP-E tests (append above the end line)
    // (end WP-E)

    // ---- WP-F tests (append above the end line)
    // (end WP-F)

    // ---- WP-G tests (append above the end line)
    // (end WP-G)
}
