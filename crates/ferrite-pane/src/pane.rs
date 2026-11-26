//! One Pane: the visible cell for one Thread. Header, transcript, Composer.
//! Rendering only — everything it
//! shows is folded in core, and every key it answers to belongs to the
//! cockpit above it.
//!
//! A Pane is the window's plane (theme WP-C, SPEC §5.3): square, flush
//! with the chrome's hairlines, with no head (the nav's row names the
//! Thread); top to bottom the subagent strip, the transcript body (its
//! Decision is the transcript's tail), the Composer.

mod agents;
mod background_work;
mod composer;
mod menus;
mod output;
mod text;
mod usage;
mod working;
mod work_row;
mod wrap_up;
pub(crate) use agents::{is_agent, render_agent, render_agent_card, AgentCardCx, AgentRowCx};
pub use agents::{AgentClock, AgentFace, AgentFaces, AgentRun, Elsewhere};
pub use background_work::{
    background_work_line, inline_action, inline_button, now_line, present as present_background_work,
    BackgroundTask, NowLine, WorkKind,
};
#[cfg(any(test, feature = "test-support"))]
pub use composer::model_label;
pub use composer::{
    attach_button, composer_footer, composer_row_limit, env_machine, env_strip, env_word,
    mode_chip, model_effort_chip, offers_import, ChipMark, ModelChip, permission_mode_label, queued_close, queued_count,
    primary_actions, queued_edit, queued_send_now, queued_sends_now, queued_takes_back, send_button,
    stop_button,
    toolbar_chip, ComposerControls, PrimaryActions,
};
use composer::{composer_region, env_face, ComposerStack};
pub use menus::{menu_item, menu_popover, menu_row, picker_row, MenuRow};
use output::{call_body, diff_rows, hunk_rows, render_diff, running_tests, DiffRow};
pub use output::{diff_number_cells, test_elbow, DiffKind, DiffPaint, TestElbow};
pub(crate) use text::{collect_block_text, collect_group_text, group_live, group_tools, is_agent_card};
pub use usage::{
    compact_count, context_meter_segments, context_segments, context_summary, context_usage, ctx_meter_ink,
    plan_usage_fraction, plan_usage_popover, plan_usage_ring, usage_ink,
};
pub(crate) use working::{turn_fold, FoldFace};
pub use working::{background_clock, background_link, working_tail, WorkingFacts, WorkingTail};
pub(crate) use wrap_up::{render_wrap_up, Face as WrapUpFace};
#[cfg(test)]
use {
    composer::{composer_fixed_height, composer_queue_height, placeholder, queued_preview},
    output::{changed_words, result_highlights},
    usage::{category_ink, reset_label},
    working::starting_line,
};

use ferrite_core::activity::{AgentStatus, Subject};
use ferrite_client::cockpit::ThreadView;
pub use ferrite_core::digest::{Turn, WallCard, WallLine};
use ferrite_core::checkout::WorkspaceBinding;
use ferrite_core::docview::{is_test_run, passed_count};
use ferrite_core::followup::{self, Followup};
use ferrite_core::progress::Phase;
use ferrite_core::pull_request::{Check, CheckState, PullRequest};
use ferrite_core::roster::{DraftId, PaneIdentity};
use ferrite_core::transcript::{
    Block, BlockId, Body, Diff, Span, Status, Style, Token, ToolBlock, ToolState, Transcript,
};
use ferrite_core::{Decision, ThreadId, ThreadKey};
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


/// One Pane's view state: what the window owns per Pane. Everything it
/// shows lives in core; this is the keyboard, the scrollback position, and
/// its cached strings.
pub struct PaneView {
    /// Which Pane of the roster this is: a live Thread, or a draft still
    /// choosing its provider and CWD (#29) — no Thread, no Session, nothing
    /// durable. The roster's own identity, so the two can never disagree.
    pub identity: PaneIdentity,
    /// The draft's choices, while it is one.
    draft: Option<DraftBinding>,
    /// The Thread's slug name — `thread-NN` until display names exist
    /// (sidebar-and-impl §4.2 #8) — or the draft's placeholder title.
    /// Built once, never formatted per frame.
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
    /// The Thread details card over the transcript (`details::card`).
    pub details: crate::details::DetailsCard,
}

/// A Pane's last laid-out card and Composer bounds, recorded in prepaint.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PaneGeometry {
    pub card: Option<gpui::Bounds<gpui::Pixels>>,
    /// The Composer's box.
    pub composer: Option<gpui::Bounds<gpui::Pixels>>,
    /// The Composer's whole stack: what stands over its box (the band, the
    /// queue, the shelf), the box, and its footer. None while the Pane has
    /// no Composer (a subagent's transcript runs to its foot).
    pub stack: Option<gpui::Bounds<gpui::Pixels>>,
    /// What the operator answers at the transcript's foot, as last drawn.
    pub answers: Answers,
}

/// What the operator answers at a transcript's foot, as last drawn: a
/// Decision's cards, or a failed turn's Retry. Cleared as the row after
/// the transcript's last draws again (`cockpit::transcript_tail`), and
/// filled as what it holds is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Answers {
    /// The cards, or the Retry.
    pub cards: Option<gpui::Bounds<gpui::Pixels>>,
    /// Their buttons, Allow and Deny: what must stay in sight. None where
    /// the whole card is.
    pub buttons: Option<gpui::Bounds<gpui::Pixels>>,
}

impl Answers {
    /// `cards` joins what is answered.
    pub fn with_cards(self, cards: gpui::Bounds<gpui::Pixels>) -> Self {
        Self { cards: Some(self.cards.map_or(cards, |had| had.union(&cards))), ..self }
    }

    /// `buttons` join the buttons.
    pub fn with_buttons(self, buttons: gpui::Bounds<gpui::Pixels>) -> Self {
        Self { buttons: Some(self.buttons.map_or(buttons, |had| had.union(&buttons))), ..self }
    }
}

/// View ownership stays with a Subject even while its transcript is hidden.
struct TranscriptViewport {
    generation: u64,
    selection_scope: gpui::base::TextSelectionScopeId,
    disclosure: ToolDisclosure,
}

/// Disclosure identities preserve choices while content streams or Subjects
/// switch: a call's details (its output fold, its diffs), a long thought,
/// a group of calls, named by its first call, a settled turn's fold,
/// named by the prompt it stands under (a steer's segment by the steer's),
/// and the running turn's thinking at its tail, with no call to name it,
/// by its first thought (`rows::live_anchor`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DisclosureId {
    Tool(String),
    Reasoning(BlockId),
    Group(String),
    Turn(BlockId),
    Live(BlockId),
    /// A long prompt's clamp, opened (`pane::prompt_clamps`), or a wake's
    /// row, opened to its whole text.
    Prompt(BlockId),
    /// A changed-files card's `Expand all`, by its turn's end.
    Changes(BlockId),
    /// One folder of a changed-files card, toggled from where `Expand
    /// all` leaves it.
    Folder(BlockId, String),
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

/// The band's chips, in tab order: the model (its effort a submenu
/// there), the environment while there is more than one, the Project, the
/// Workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandChip {
    Provider,
    Environment,
    Project,
    Workspace,
}

impl BandChip {
    /// Where tab goes next: across the band, then back to the prompt. The
    /// environment chip is passed over while it is not shown.
    pub fn next(current: Option<BandChip>, environment: bool) -> Option<BandChip> {
        match current {
            None => Some(BandChip::Provider),
            Some(BandChip::Provider) if environment => Some(BandChip::Environment),
            Some(BandChip::Provider | BandChip::Environment) => Some(BandChip::Project),
            Some(BandChip::Project) => Some(BandChip::Workspace),
            Some(BandChip::Workspace) => None,
        }
    }
}

impl PaneView {
    pub fn new<T: 'static>(key: ThreadKey, cx: &mut Context<T>) -> Self {
        // The transcript and the Pane's names are its machine's own:
        // its key is what tells it apart from another machine's.
        let thread = key.thread();
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
            identity: PaneIdentity::Thread(key),
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
            details: crate::details::DetailsCard::new(cx),
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
            details: crate::details::DetailsCard::new(cx),
        }
    }

    /// The Thread this Pane shows, or None while it is still a draft.
    pub fn thread(&self) -> Option<ThreadKey> {
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
    pub fn adopt_thread(&mut self, thread: ThreadKey) {
        self.identity = PaneIdentity::Thread(thread);
        self.draft = None;
        self.name = SharedString::from(format!("thread-{:02}", thread.thread()));
    }

    pub fn is_main(&self) -> bool {
        self.selected == Subject::Main
    }

    pub fn text_namespace(&self) -> SharedString {
        let thread = self.thread().map_or(0, |key| key.thread().get());
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
        let thread = self.thread()?.thread();
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
        let thread = self.thread().map_or(0, |key| key.thread().get());
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

    pub fn transcript_disclosure_snapshot(&self) -> (HashSet<DisclosureId>, Option<DisclosureId>) {
        // The retained renderer receives a snapshot; its display revision
        // changes for every state transition above.
        (
            self.disclosure.expanded.clone(),
            self.disclosure.target.clone(),
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
    pub thread: ThreadView<'a>,
    /// Whether the Composer line is empty — what decides the idle
    /// placeholder, read where the cockpit has a `cx` to read it with.
    pub composer_empty: bool,
    pub focused: bool,
    /// This Thread finished and the operator has not seen it since (an
    /// unread Notice): its head's mark is the done check until they do.
    pub unread: bool,
    /// The operator asked the system for reduced motion (`cx.reduce_motion()`).
    pub reduce_motion: bool,
    /// The Composer's own focus handle holds focus in the active window.
    pub editing: bool,
    /// Native files are dragged over this Pane: the drop sheet covers it
    /// and the Composer, drawn above the sheet, wears the strong edge.
    pub drop_target: bool,
    /// The Pane is the window's plane, on the
    /// transcript's black ground; drawn inside another surface (the
    /// Factory's), it takes that surface's `CARD` ground instead.
    pub plane: bool,
    /// This Pane's docked Decision merges into its live Composer (one
    /// block, rule 2.8.1): the Composer drops its top edge and corners.
    pub decision_joined: bool,
}

/// The click-wired elements only the cockpit can build — gpui listeners
/// are made with its own `Context` — and the Pane only places.
#[derive(Default)]
pub struct PaneWiring {
    /// The retained transcript. Its cached entity owns native text and
    /// row layout; the Pane only places the allocated viewport.
    pub transcript: Option<AnyElement>,
    pub attachments: Option<AnyElement>,
    /// The open `/` or `@` popover for this Pane's Composer, rows wired to
    /// their picks in the cockpit and hung above the input line here (#23).
    pub menu: Option<AnyElement>,
    pub activity_decisions: Option<AnyElement>,
    /// The Composer's controls, wired in the cockpit (SPEC §5.5).
    pub toolbar: Option<ComposerControls>,
    /// An empty Thread's hero (`Hero`): it stands centred over its body
    /// until the first prompt.
    pub start: Option<Hero>,
    /// The Thread details card, laid over the body's top right.
    pub details: Option<AnyElement>,
    pub details_lane_width: f32,
    /// What the Thread offers when its provider lost the conversation it
    /// saved (`resume_offer`), the first row of the Composer's stack.
    pub resume_offer: Option<AnyElement>,
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

/// Everything the `render_pane` slots read for one frame, gathered once by the
/// skeleton. **Foundation-owned:** only the integrator adds a field, so no
/// package edits this struct. The owned wiring pieces are `Option`s that the
/// slot named in their doc `take()`s.
pub(crate) struct PaneCtx<'a> {
    pub view: &'a PaneView,
    #[allow(dead_code)]
    pub thread: ThreadView<'a>,
    pub transcript: &'a Transcript,
    pub focused: bool,
    /// The Main Subject's pending Decision.
    pub decision: Option<&'a Decision>,
    /// Activity requests (or a question expander) exist for this Pane, so
    /// the plain Decision card steps aside.
    pub has_activity_decisions: bool,
    pub queued: &'a [ferrite_core::prompt_queue::QueueItem],
    pub composer_empty: bool,
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
    // ---- owned wiring
    /// `composer_slot`.
    pub attachments: Option<AnyElement>,
    pub menu: Option<AnyElement>,
    /// `dock_slot`: activity requests that were not docked in the body.
    pub activity_decisions: Option<AnyElement>,
    /// The docked Decision merges into this Pane's Composer (one block).
    pub decision_joined: bool,
    /// `composer_slot`: the Solo box's controls.
    pub toolbar: Option<ComposerControls>,
    /// `PaneWiring::resume_offer`, for `composer_slot`.
    pub resume_offer: Option<AnyElement>,
}

/// The group name every Pane root carries (`.group(PANE_GROUP)`): what a
/// child styles itself by while the pointer is anywhere over its Pane (the
/// minimap's `group_hover`, the prototype's `.pane:hover .mm`).
pub const PANE_GROUP: &str = "pane";

pub fn render_pane(
    view: &PaneView,
    facts: PaneFacts<'_>,
    wiring: PaneWiring,
) -> impl IntoElement {
    let PaneFacts {
        thread,
        composer_empty,
        focused,
        unread,
        reduce_motion,
        editing,
        drop_target,
        plane,
        decision_joined,
    } = facts;
    let PaneWiring {
        transcript: retained_transcript,
        attachments,
        menu,
        activity_decisions,
        toolbar,
        start,
        details,
        details_lane_width,
        resume_offer,
    } = wiring;
    let has_activity_decisions = activity_decisions.is_some();
    // The Decision is the transcript's tail now (`decision_tail`): no card
    // is laid over the body or docked above the Composer at this level.
    let _ = activity_decisions;
    let key = view.thread().map_or(0, |key| key.thread().get());
    let shell = record_card(pane_shell(plane), view);
    // A selected subagent the Activity no longer holds has no transcript
    // to draw: the Pane stays an empty shell for the frame.
    let Some(subject) = thread.activity().subject(&view.selected) else {
        return pane_frame(shell, key);
    };
    let transcript = subject.transcript();
    let decision = if view.is_main() { thread.pending() } else { None };
    let workspace = thread.workspace();

    let mut cx = PaneCtx {
        view,
        thread,
        transcript,
        focused,
        decision,
        has_activity_decisions,
        queued: thread.queue(),
        composer_empty,
        suggestion: thread.suggestion(),
        reduce_motion,
        editing,
        drop_target,
        unread,
        attachments,
        menu,
        activity_decisions: None,
        decision_joined,
        toolbar,
        resume_offer,
    };

    // A Pane has no head: the nav's row names the Thread (C2, ADR 0014),
    // and the body starts at the Pane's own top edge, a subagent's too.
    let mut pane = shell;
    view.rich
        .file_context(workspace.map(WorkspaceBinding::cwd), &view.preview);
    // A child's tab has no Composer to carry the `Decision` key
    // context: while that child's request pends, its body does, and
    // with no Composer there the answer letters bind too.
    let child_request = !view.is_main()
        && thread
            .pending_decisions()
            .iter()
            .any(|request| request.subject.as_ref() == Some(&view.selected));
    let details_canvas = view.details.canvas.clone();
    pane = pane.child(components::on_bounds(
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
            .when(child_request, |body| {
                body.key_context("Decision SubagentDecision")
            })
            .child(
                div().flex().flex_col().flex_1().min_h_0().min_w_0()
                    .pr(px(details_lane_width))
                    .child(retained_transcript.expect("the transcript entity is wired by CockpitView")),
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
            .children(details),
        // The card is placed on the bounds the frame before measured:
        // a change redraws this Pane, and no other part.
        move |bounds, window, cx| {
            if details_canvas.replace(Some(bounds)) != Some(bounds) {
                components::redraw_drawn_view(window, cx);
            }
        },
    ));
    // The order is head · body · progress · dock · composer.
    pane = pane.children(dock_slot(&mut cx));
    // The drop sheet covers the Pane beneath the Composer, which
    // paints after it and so stays in view, edged in `INK_STRONG`.
    pane = pane.children(drop_target.then(crate::prompt_drop::sheet));
    pane = pane.children(composer_slot(&mut cx));
    pane_frame(pane, key)
}

// ---------------------------------------------------------- render_pane slots
// Each slot's body belongs to one package; its signature and `PaneCtx` are
// the integrator's.

/// WP-F · between the body and the Composer. Every request — Main's
/// plain approval included — is the one Decision card in the body's
/// requests overlay (`activity_decisions`), and a failed send is that
/// card's own error line, so nothing docks here; the slot keeps any
/// activity requests it is handed undocked.
fn dock_slot(cx: &mut PaneCtx) -> Vec<AnyElement> {
    if !cx.has_activity_decisions {
        return Vec::new();
    }
    cx.activity_decisions.take().into_iter().collect()
}

/// WP-D · the Composer: Main's alone. A subagent takes no prompts, so
/// its transcript runs to the Pane's foot.
fn composer_slot(cx: &mut PaneCtx) -> Option<AnyElement> {
    if !cx.view.is_main() {
        let geometry = &cx.view.geometry;
        geometry.set(PaneGeometry {
            stack: None,
            ..geometry.get()
        });
        return None;
    }
    Some(
        composer_region(
            cx.view,
            Some(cx.transcript),
            ComposerStack {
                decision: cx.decision,
                queued: cx.queued,
                empty: cx.composer_empty,
                attachments: cx.attachments.take(),
                menu: cx.menu.take(),
                draft: false,
                draft_error: None,
                suggestion: cx.suggestion,
                focused: cx.focused,
                drop_target: cx.drop_target,
                joined: cx.decision_joined,
                toolbar: cx.toolbar.take(),
                offer: cx.resume_offer.take(),
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

/// The Pane on the plane (theme rule 2, SPEC §5.3): square, its content
/// flush to its own edges, on the transcript's ground when it is the
/// window's plane (`plane`), else its host's `CARD`. `overflow: hidden`
/// clips the children to the box.
fn pane_shell(plane: bool) -> Div {
    div()
        .relative()
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .min_w_0()
        .bg(rgb(theme::tx_ground(plane)))
        .font_family(theme::FONT_UI)
        .overflow_hidden()
}

/// The Pane root (F-1): every Pane root is `PANE_GROUP`.
fn pane_frame(pane: Div, key: u64) -> Stateful<Div> {
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
}

/// A card's 1px ring in `ink`, just outside its box
/// (`components::outer_ring`). The cockpit rings a reader's card with it.
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
    /// The open `/` or `@` menu, hung above the Composer like every menu.
    pub menu: Option<AnyElement>,
    pub composer_empty: bool,
    pub focused: bool,
    /// A failed bootstrap's words, shown where the band is.
    pub error: Option<&'a SharedString>,
    /// The operator asked for reduced motion.
    pub reduce_motion: bool,
    /// Native files hover the draft (`PaneFacts::drop_target`).
    pub drop_target: bool,
    /// The box's controls (`PaneWiring::toolbar`).
    pub toolbar: Option<ComposerControls>,
    /// The hero (`Hero`), its project word wired in the cockpit to the
    /// Project picker.
    pub start: Option<Hero>,
}

/// What a draft is called until its first send names the Thread.
pub const DRAFT_TITLE: &str = "New thread";

/// A draft Pane (#29): an empty transcript area and the Composer wearing
/// the pre-prompt band. It has no head: cmd-w discards it.
pub fn render_draft(view: &PaneView, state: DraftState<'_>) -> impl IntoElement {
    let DraftState {
        attachments,
        menu,
        composer_empty,
        focused,
        error,
        reduce_motion,
        drop_target,
        toolbar,
        start,
    } = state;
    let key = view.identity.draft().map_or(0, DraftId::get);
    let shell = record_card(pane_shell(true), view);
    let _ = reduce_motion;
    let composer = composer_region(
        view,
        None,
        ComposerStack {
            decision: None,
            queued: &[],
            empty: composer_empty,
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
            offer: None,
        },
    );
    // Codex's home: at the body's centre the hero, and the Composer at the
    // foot, where the Thread it becomes keeps it: two rows tall, Codex's,
    // until the first send makes it Claude's one-row box.
    let card = start.map(start_hero);
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
            .child(composer),
        u64::MAX - key,
    )
}

/// What an empty Thread's or a draft's hero asks: a Project's word
/// (`hero_project`, wired in the cockpit), or none for a scratch Thread,
/// which has no Project to build in.
pub enum Hero {
    Project(AnyElement),
    Scratch,
}

/// An empty Thread's or a draft's hero, Codex's home: Ferrite's mark in
/// `HERO_MARK_INK`, then `What should we build in <project>?` at the home's
/// display size (`What should we work on?` for a scratch Thread). Under
/// `HERO_SHORT_WINDOW` of window height the lead words give way and the
/// project word stands alone (T3's `max-height: 28rem`).
pub fn start_hero(hero: Hero) -> StartHero {
    StartHero { hero }
}

#[derive(IntoElement)]
pub struct StartHero {
    hero: Hero,
}

impl RenderOnce for StartHero {
    fn render(self, window: &mut gpui::Window, _: &mut gpui::App) -> impl IntoElement {
        let short = f32::from(window.viewport_size().height) < theme::HERO_SHORT_WINDOW;
        let title = div()
            .flex()
            .flex_wrap()
            .justify_center()
            .items_baseline()
            .max_w_full()
            .font_family(theme::FONT_UI)
            .font_weight(theme::W_BODY)
            .text_size(px(theme::FS_HOME_TITLE))
            .line_height(px(theme::LH_HOME_TITLE))
            .text_color(rgb(INK_STRONG));
        let title = match self.hero {
            Hero::Project(project) => title
                .when(!short, |title| title.child("What should we build in\u{a0}"))
                .child(project)
                .when(!short, |title| title.child("?")),
            Hero::Scratch => title.child("What should we work on?"),
        };
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
            .child(title)
    }
}

/// The hero's project word, in `W_LABEL` (T3's `font-medium`): the Project's
/// name, and while it can still change (a draft's) a switcher: the word in
/// muted 60% over a 1px rule at muted 35%, both lifting under the pointer
/// (T3's dotted rule; GPUI has no dotted border, so it is solid, as the
/// prose links' hover underline is). The cockpit wires its press.
pub fn hero_project(label: SharedString, switcher: bool) -> Stateful<Div> {
    let muted = |alpha: u32| gpui::rgba((INK_SUBTLE << 8) | alpha);
    div()
        .id("start-hero-project")
        .debug_selector(|| "start-hero-project".into())
        .min_w_0()
        .truncate()
        .font_weight(theme::W_LABEL)
        .when(switcher, |word| {
            word.cursor_pointer()
                .border_b_1()
                .border_color(muted(theme::HERO_SWITCHER_RULE))
                .text_color(muted(theme::HERO_SWITCHER_INK))
                .hover(|word| {
                    word.border_color(muted(theme::HERO_SWITCHER_RULE_HOVER))
                        .text_color(muted(theme::HERO_SWITCHER_INK_HOVER))
                })
        })
        .child(label)
}

/// A draft's Project or Workspace chip on the strip over the box
/// (`env_strip`, Codex's): its mark and the choice, ringed by the one focus
/// recipe (`components::focused`) while tab rests on it, because the
/// popover opens on ↵ and the chip must say where ↵ will land. In a narrow
/// Pane its label cuts first.
pub fn band_chip(
    slot: usize,
    mark: ChipMark,
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

/// One lexicon word for where a Thread stands (rule 2.11.2), as the nav
/// row reads it (`mark::NavTail::of`). Each variant is one state.
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

/// The state vocabulary the nav reads:
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
/// Thread's dot — the nav, the rail — reads
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

/// The Pane's offer when its provider no longer holds the conversation the
/// Thread saved: what happened and what the way on costs, above the one
/// button that takes it (`action`). The Composer's column holds it.
pub fn resume_offer(key: u64, line: SharedString, action: gpui::component::button::Button) -> AnyElement {
    div()
        .debug_selector(move || format!("resume-offer-{key}"))
        .flex()
        .flex_col()
        .min_w_0()
        .gap(px(theme::HALF_ROW))
        .pb(px(theme::SPACE_4))
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(INK_SUBTLE))
        .child(line)
        .child(div().flex().child(action))
        .into_any_element()
}

// ------------------------------------------------------------- tile text

// ------------------------------------------------------------- the head

/// The failure strip (SPEC §5.9 item 4), the transcript's tail row under
/// a failed turn's failure line:
/// `FAILURE_STRIP_H` at `R_CARD` on `FAILURE_WASH`, an `x-circle` in
/// `BLOCKED`, why it failed (`API Error: 529 overloaded`) in `INK_STRONG`, cut to
/// one line, and at the right its wired presses (`strip_button`): `Retry`,
/// which a closed Session goes without, and the sign-in of a CLI that is
/// not signed in on another Machine.
pub fn failure_strip(key: u64, words: SharedString, presses: Option<AnyElement>) -> Div {
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
        .children(presses)
}

/// A press on the strip, `Retry` among them: `RETRY_H` at `R_CONTROL` on
/// `SELECTED`, padded `RETRY_PAD_X`, `label` at `W_LABEL` in `INK_STRONG`.
/// `name` and `key` make its selector (`failure-retry-7`). The caller wires
/// the press.
pub fn strip_button(
    name: &'static str,
    key: u64,
    label: impl Into<SharedString>,
    cx: &gpui::App,
) -> gpui::component::button::Button {
    components::faded_button(
        (name, key as usize),
        theme::paint::SELECTED.into(),
        theme::paint::SELECTION_HOVER.into(),
        theme::paint::PRESS.into(),
        rgb(INK_STRONG).into(),
        cx,
    )
    .debug_selector(move || format!("{name}-{key}"))
    .flex_shrink_0()
    .h(px(theme::RETRY_H))
    .px(px(theme::RETRY_PAD_X))
    .rounded(px(theme::R_CONTROL))
    .child(
        components::text_ui()
            .text_size(px(theme::FS_LABEL))
            .font_weight(theme::W_LABEL)
            .text_color(rgb(INK_STRONG))
            .child(label.into()),
    )
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

/// How many of a transcript's newest Blocks a Pane draws.
const RENDERED_BLOCKS: usize = 200;

/// The rendered tail of a transcript — the window `body` draws and the
/// selection overlay resolves against (#27). One function, two callers, so
/// the wash can never resolve against a different window than is drawn.
pub fn rendered_window(blocks: &[Block]) -> &[Block] {
    let tail = blocks.len().saturating_sub(RENDERED_BLOCKS);
    &blocks[tail..]
}

/// Tool rows with something to disclose — output, a structured result, an
/// input, or a diff — in exactly the window the Pane draws. Disclosure cycling,
/// focus validation, and controls all consume this one eligibility rule so
/// an invisible row can never remain keyboard-addressable.
pub fn tool_has_details(tool: &ToolBlock) -> bool {
    tool.output.is_some()
        || tool.structured_preview.is_some()
        || !tool.summary.is_empty()
        || !tool.diffs.is_empty()
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

    /// The grid of a Solo (`solo`) or a side chat's transcript
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
/// its diffs) are open, whether it stands in a group's list, the keyboard's
/// target and toggle, and when it began.
pub(crate) struct ToolCx {
    pub expanded: bool,
    pub grouped: bool,
    pub disclosure: Option<Disclosure>,
    /// Its work running in the background: `View live output ▸`, pressed
    /// to open the Background tasks pane (`working::background_link`).
    pub live: Option<AnyElement>,
    /// Its block's stamp (`Block::at`): the row's hover time.
    pub at: Option<String>,
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
    live: bool,
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
        Body::Prompt(text) => match &block.wake {
            Some(wake) => wake_row(block, wake, text, row_cx, expanded, disclosure),
            None => prompt_row(block, text, row_cx, PromptFace::default(), None),
        },
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
        // A thought (T3's): the brain, the thought flattened to one line
        // and cut, always with its chevron. Open, the row reads `Thinking`
        // while it is `live`, then `Thought` (`thought_label`), and the whole thought stands under it past the icon gutter, at
        // full ink, at most `THOUGHT_BODY_MAX_H` tall and scrolling in
        // itself.
        Body::Thinking(thought) => {
            let (overlay, _, targeted) = disclosure_parts(disclosure);
            let label = thought_label(thought, expanded, live);
            let header = work_row::work_row(work_row::WorkRow {
                key: SharedString::from(format!("reasoning-row-{:?}", block.id)),
                icon: ferrite_core::transcript::tools::ToolIcon::Brain,
                failed: false,
                error: false,
                label: div()
                    .debug_selector(|| "reasoning-summary".into())
                    .min_w_0()
                    .truncate()
                    .child(label)
                    .into_any_element(),
                ink: work_row::tool_ink(),
                at: block.at.clone(),
                slot: work_row::Slot::Chevron { open: expanded },
                listed: false,
                press: overlay,
            })
            .group(DISCLOSURE_ROW);
            let header = Disclosure::ground(targeted, header);
            let body = expanded.then(|| {
                div()
                    .id(SharedString::from(format!("reasoning-body-{:?}", block.id)))
                    .debug_selector(|| "reasoning-body".into())
                    .ml(px(theme::WORK_DETAIL_INDENT))
                    .px(px(theme::WORK_ROW_PAD_X))
                    .py(px(theme::SPACE_1))
                    .max_h(px(theme::THOUGHT_BODY_MAX_H))
                    .overflow_y_scroll()
                    .text_color(rgb(INK_STRONG))
                    .child(selection.markdown(block.id, thought.trim().to_owned()))
            });
            row.child(header).children(body).into_any_element()
        }
        // A notice: an update's work row (T3's), its words `INK_STRONG` at
        // `WORK_UPDATE_ALPHA`, cut at the column's edge with the whole of
        // it one hover away. A Decision never leaves one (it is its own row,
        // at the tail).
        Body::Notice(whole) => {
            let text = notice_line(whole);
            let highlights = separators(&text);
            let text = SharedString::from(text);
            let label = div()
                .id(SharedString::from(format!("notice-{:?}", block.id)))
                .debug_selector({
                    let id = block.id;
                    move || format!("notice-{id:?}")
                })
                .flex_shrink(1.)
                .min_w_0()
                .tooltip(ferrite_ui::menu::tooltip(whole.clone()))
                .child(edge_cut(
                    theme::WORK_ROW_LINE,
                    INK_STRONG,
                    selection.line(block.id, text, highlights),
                ));
            row.child(work_row::work_row(work_row::WorkRow {
                key: SharedString::from(format!("notice-row-{:?}", block.id)),
                icon: ferrite_core::transcript::tools::ToolIcon::Zap,
                failed: false,
                error: false,
                label: label.into_any_element(),
                ink: work_row::update_ink(),
                at: block.at.clone(),
                slot: work_row::Slot::Empty,
                listed: false,
                press: None,
            }))
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
        Body::Compaction(compaction) => crate::transcript::compaction_divider(compaction)
            .into_any_element(),
        Body::Plan(steps) => row
            .child(working::plan_card(steps, grid, |_, words| {
                selection
                    .line(block.id, words.to_owned(), Vec::new())
                    .into_any_element()
            }))
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
                at: block.at.clone(),
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
    let words = ferrite_core::replies::split(line).1;
    let (text, files, _) = ferrite_core::browser_annotations::split(String::from(words));
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

/// A skill in a sent prompt, the Composer's token (T3's
/// `SkillInlineText`): the cube (`icons::BOX`, 1em) then the name at
/// `W_LABEL` in `SKILL_INK`, on no ground, underlined under the pointer,
/// and a card above it: the full name in `INK_SUBTLE`, the catalogue's
/// description, and `View instructions` where the skill names its
/// SKILL.md, which a click opens.
pub(crate) fn skill_token(
    label: &str,
    skill: &ferrite_core::SessionCommand,
    window: &mut gpui::Window,
) -> (gpui::Size<gpui::Pixels>, AnyElement) {
    let label = label.trim_start_matches(['/', '$']).to_string();
    let mut style = window.text_style();
    style.font_weight = theme::W_LABEL;
    let size = style.font_size.to_pixels(window.rem_size());
    let shaped = window.text_system().shape_line(
        SharedString::from(label.clone()),
        size,
        &[style.to_run(label.len())],
        None,
    );
    let icon_side = f32::from(size);
    let gap = icon_side * theme::TOKEN_ICON_GAP_EM;
    let width = shaped.width() + px(icon_side + gap);
    let line = window.line_height();
    let name = skill.name.clone();
    let mut body = skill.description.clone();
    if skill.path.is_some() {
        body.push_str("\n\nView instructions");
    }
    let instructions = skill.path.clone().map(std::path::PathBuf::from);
    let element = div()
        .id(SharedString::from(format!("prompt-skill-{name}")))
        .debug_selector({
            let name = name.clone();
            move || format!("prompt-skill-{name}")
        })
        .flex()
        .items_center()
        .gap(px(gap))
        .w(width)
        .h(line)
        .font_weight(theme::W_LABEL)
        .text_color(rgb(theme::SKILL_INK))
        .whitespace_nowrap()
        .hover(|style| style.underline())
        .when_some(instructions, |token, path| {
            token
                .cursor_pointer()
                .on_click(move |_, _, cx| cx.open_with_system(&path))
        })
        .tooltip(ferrite_ui::menu::tooltip_card(
            format!("${name}"),
            body,
            theme::SKILL_TIP_W,
        ))
        .child(ferrite_ui::icons::icon_stroked(
            ferrite_ui::icons::BOX,
            icon_side,
            theme::SKILL_ICON_STROKE,
            theme::SKILL_INK,
        ))
        .child(SharedString::from(label))
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
/// How a prompt's bubble sits: its joins to the prompts right above and
/// below it (`rows::Joins`, those corners tightening to
/// `PROMPT_BUBBLE_JOIN_RADIUS`), and a long prompt's clamp (`prompt_clamps`)
/// with the toggle under it that opens and shuts it.
#[derive(Default)]
pub(crate) struct PromptFace {
    pub above: bool,
    pub below: bool,
    pub clamp: Option<PromptClamp>,
    /// Where each of its Quote rows was cited from, by row.
    pub sources: Vec<Option<crate::attachments::QuoteSource>>,
}

/// A long prompt's clamp: whether it is open, and its toggle.
pub(crate) struct PromptClamp {
    pub open: bool,
    pub toggle: AnyElement,
}

/// Whether a prompt's words clamp (T3's `shouldCollapseUserMessage`):
/// over `PROMPT_CLAMP_CHARS` characters or `PROMPT_CLAMP_LINES` lines of
/// what its bubble shows.
pub(crate) fn prompt_clamps(line: &str) -> bool {
    let words = ferrite_core::prompt_files::split(line.to_owned()).0;
    let words = ferrite_core::replies::split(&words).1;
    !words.trim().is_empty()
        && (words.chars().count() > theme::PROMPT_CLAMP_CHARS
            || words.split('\n').count() > theme::PROMPT_CLAMP_LINES)
}

/// A wake row's icon, as T3's `workEntryIconName` picks a notification's:
/// the alert for a failure, else its source's.
fn wake_icon(wake: &ferrite_core::wake::WakeNote) -> ferrite_core::transcript::tools::ToolIcon {
    use ferrite_core::transcript::tools::ToolIcon;
    use ferrite_core::wake::{Outcome, Source};
    match (wake.outcome, wake.source) {
        (Outcome::Failed, _) => ToolIcon::Alert,
        (_, Source::PrWatch) => ToolIcon::Eye,
        (_, Source::Delegates) => ToolIcon::Bot,
    }
}

/// A prompt the Server sent on its own (T3's notification work row,
/// `WorkEntryLogRow`): never the operator's bubble. Its summary on one line
/// (`#412: checks failed, new comments`) in an update's ink, by the eye of
/// a watch or the bot of Delegates (T3's `workEntryIconName`), or the alert
/// when the news is a failure; opened, the whole text it woke the agent
/// with, past the icon gutter.
fn wake_row(
    block: &Block,
    wake: &ferrite_core::wake::WakeNote,
    text: &str,
    row_cx: &RowCx,
    expanded: bool,
    disclosure: Option<Disclosure>,
) -> AnyElement {
    let selection = row_cx.selection;
    let (overlay, _, targeted) = disclosure_parts(disclosure);
    let summary = SharedString::from(wake.summary.clone());
    let header = work_row::work_row(work_row::WorkRow {
        key: SharedString::from(format!("wake-row-{:?}", block.id)),
        icon: wake_icon(wake),
        failed: false,
        error: false,
        label: div()
            .debug_selector(|| "wake-summary".into())
            .min_w_0()
            .truncate()
            .child(selection.line(block.id, summary, Vec::new()))
            .into_any_element(),
        ink: work_row::update_ink(),
        at: block.sent_at.clone().or_else(|| block.at.clone()),
        slot: work_row::Slot::Chevron { open: expanded },
        listed: false,
        press: overlay,
    })
    .group(DISCLOSURE_ROW);
    let header = Disclosure::ground(targeted, header);
    let body = expanded.then(|| {
        div()
            .debug_selector(|| "wake-body".into())
            .ml(px(theme::WORK_DETAIL_INDENT))
            .px(px(theme::WORK_ROW_PAD_X))
            .py(px(theme::SPACE_1))
            .text_color(rgb(INK_STRONG))
            .child(selection.markdown(block.id, text.trim().to_owned()))
    });
    div()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .child(header)
        .children(body)
        .into_any_element()
}

pub(crate) fn prompt_row(
    block: &Block,
    line: &str,
    row_cx: &RowCx,
    face: PromptFace,
    actions: Option<AnyElement>,
) -> AnyElement {
    let (words, other_files, annotations) =
        ferrite_core::browser_annotations::split(line.to_owned());
    let visible = ferrite_core::prompt_files::compose(&words, &other_files);
    let (text, files) = prompt_text(&visible, row_cx.skills);
    let gallery = prompt_gallery(&files);
    // The passages it replied to, as their Quote rows (Claude's).
    let mut sources = face.sources.into_iter();
    let quotes: Vec<_> = ferrite_core::replies::split(line)
        .0
        .into_iter()
        .enumerate()
        .map(|(index, reply)| {
            let source = sources.next().flatten();
            div()
                .max_w(relative(theme::PROMPT_BUBBLE_MAX_W))
                .min_w_0()
                .child(crate::attachments::quote_row(index, reply, source))
        })
        .collect();
    let clamped = face.clamp.as_ref().is_some_and(|clamp| !clamp.open);
    let bubble = prompt_words(block, text, &gallery, row_cx, None).map(|words| {
        let join = px(theme::PROMPT_BUBBLE_JOIN_RADIUS);
        prompt_bubble()
            .debug_selector(|| "transcript-prompt".into())
            .when(face.above, |bubble| bubble.rounded_tr(join))
            .when(face.below, |bubble| bubble.rounded_br(join))
            .when(clamped, |bubble| bubble.relative())
            .child(
                div()
                    .min_w_0()
                    .when(clamped, |words| {
                        words
                            .debug_selector(|| "transcript-prompt-clamped".into())
                            .max_h(px(theme::PROMPT_CLAMP_H))
                            .overflow_hidden()
                    })
                    .child(words),
            )
            .when(clamped, |bubble| {
                bubble.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom(px(theme::PROMPT_BUBBLE_PAD_Y))
                        .child(ferrite_ui::components::fade(
                            theme::paint::PROMPT_BUBBLE.into(),
                            true,
                            theme::PROMPT_CLAMP_FADE,
                            &[(0., 0.), (1., 1.)],
                        )),
                )
            })
    });
    // A clamped bubble and its toggle stand `PROMPT_CLAMP_TOGGLE_GAP` apart.
    let bubble = match face.clamp {
        Some(clamp) => Some(
            div()
                .flex()
                .flex_col()
                .items_end()
                .gap(px(theme::PROMPT_CLAMP_TOGGLE_GAP))
                .w_full()
                .min_w_0()
                .children(bubble)
                .child(clamp.toggle)
                .into_any_element(),
        ),
        None => bubble.map(IntoElement::into_any_element),
    };
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
        .when(!annotations.is_empty(), |message| {
            message.child(
                crate::attachments::Attachments::new(
                    "sent-browser-annotations",
                    vec![],
                    row_cx.preview,
                )
                .annotations(annotations),
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
        .gap(px(theme::PROMPT_ACTIONS_GAP))
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
    let annotation_count = ferrite_core::browser_annotations::split(line.to_owned())
        .2
        .len();
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
                .children(words)
                .when(annotation_count > 0, |words| {
                    words.child(format!(
                        "{} browser annotation{}",
                        annotation_count,
                        if annotation_count == 1 { "" } else { "s" }
                    ))
                }),
        )
}

/// The prompt's bubble (T3's): `paint::PROMPT_BUBBLE` at
/// `PROMPT_BUBBLE_RADIUS`, shrinking to its words in Geist `INK_STRONG` at body
/// weight, never wider than `PROMPT_BUBBLE_MAX_W` of the column. One face
/// for the echo in its row and its pinned copy.
fn prompt_bubble() -> Div {
    div()
        .max_w(relative(theme::PROMPT_BUBBLE_MAX_W))
        .min_w_0()
        .py(px(theme::PROMPT_BUBBLE_PAD_Y))
        .px(px(theme::PROMPT_BUBBLE_PAD_X))
        .bg(theme::paint::PROMPT_BUBBLE)
        .rounded(px(theme::PROMPT_BUBBLE_RADIUS))
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
    let local = preview.attachment_local(path);
    let size = match local.as_deref().and_then(image_facts).and_then(|facts| facts.size) {
        Some((w, h)) if w > 0 && h > 0 => {
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
    crate::attachments::remote_thumbnail(
        SharedString::from(format!("sent-image-{block:?}-{index}")),
        path,
        preview,
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
        if let Some((width, height)) = facts.size {
            caption.push(format!("{width}\u{d7}{height}"));
        }
        caption.push(text::byte_size(facts.bytes as usize));
    }
    caption.join(" \u{b7} ")
}

/// What an image caption says about the file: its size on disk and, when
/// its header is one the reader knows, its pixel size. Without a pixel size
/// the image is still drawn: the decoder, not this reader, decides whether
/// it can be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ImageFacts {
    pub size: Option<(u32, u32)>,
    pub bytes: u64,
}

/// A file as it stood when its facts were read: a changed file reads again.
type Stamp = (u64, Option<std::time::SystemTime>);

thread_local! {
    /// Image headers are read once per version of a file: a caption is
    /// drawn every frame its row is on screen.
    static IMAGE_FACTS: std::cell::RefCell<HashMap<std::path::PathBuf, (Stamp, ImageFacts)>> =
        std::cell::RefCell::new(HashMap::new());
}

/// An image file's facts, or `None` when it cannot be read: missing, or not
/// a file.
pub(crate) fn image_facts(path: &std::path::Path) -> Option<ImageFacts> {
    let meta = std::fs::metadata(path).ok().filter(|meta| meta.is_file())?;
    let stamp = (meta.len(), meta.modified().ok());
    IMAGE_FACTS.with(|cache| {
        if let Some((seen, facts)) = cache.borrow().get(path) {
            if *seen == stamp {
                return Some(*facts);
            }
        }
        let facts = read_image_facts(path, meta.len())?;
        cache
            .borrow_mut()
            .insert(path.to_path_buf(), (stamp, facts));
        Some(facts)
    })
}

fn read_image_facts(path: &std::path::Path, bytes: u64) -> Option<ImageFacts> {
    use std::io::Read as _;
    let mut head = Vec::with_capacity(64 * 1024);
    std::fs::File::open(path)
        .ok()?
        .take(64 * 1024)
        .read_to_end(&mut head)
        .ok()?;
    Some(ImageFacts {
        size: image_size(&head),
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
/// back as it reads: an interrupted one is a muted activity line, all
/// `INK_SUBTLE` (the operator did it; nothing failed); a failed one leads
/// with `failed` in `BLOCKED`. A completed turn has no row at its end: its
/// fold says how long it worked.
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
    components::tabular(activity_line(row_cx.grid(), None, words))
        .debug_selector(|| "turn-stamp".into())
        .into_any_element()
}

/// A notice's words on the one line its row has: a provider's failure
/// arrives as a reason and its stderr on lines of their own, which a
/// `WORK_ROW_LINE` box would clip through. The lines read as `a · b`, the
/// row's own seam; the tooltip keeps the whole.
fn notice_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" \u{b7} ")
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

/// A thought's row label: shut, the thought on one line; open, `Thinking`
/// while it is still being thought (`live`), else `Thought` (T3's).
fn thought_label(thought: &str, expanded: bool, live: bool) -> SharedString {
    match (expanded, live) {
        (false, _) => ferrite_core::progress::one_line(thought, usize::MAX).into(),
        (true, true) => "Thinking".into(),
        (true, false) => "Thought".into(),
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

/// A tool call as T3 Code's work row draws it (`work_row`): its kind's icon
/// (`tools::policy`, tinted `TOOL_ERROR_ICON` when it failed), its label in
/// the app's words (`text::call_line`: a command's text, an edit's path,
/// `Read src/x.rs`), its time under the pointer, and its chevron slot. No
/// result trails it and no `Failed` word: what came of a call is in its
/// open body. Under it while it runs: a suite's bar.
///
/// Open, a call shows its body (`call_body`): its input, a failed
/// command's `Exit code N`, what it printed as plain mono, then its diffs,
/// on the column with no card. Opening and shutting snap. The whole row is
/// the pointer's toggle; the keyboard's disclosure target wears
/// `paint::RAISED`.
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
        grouped,
        disclosure,
        live,
        at,
    } = state;
    let (overlay, _, targeted) = disclosure_parts(disclosure);
    let slot = if overlay.is_some() {
        work_row::Slot::Chevron { open: expanded }
    } else {
        work_row::Slot::Empty
    };
    let run = |markdown: String| {
        selection
            .markdown(block, markdown)
            .ink(INK_SUBTLE)
            .activity()
    };
    // One run on one line at its own width, cut with `…` at the row's
    // edge; a call on a file has its path in a run of its own after the
    // verb, which is chrome.
    let cut = |markdown: String| {
        div()
            .debug_selector(|| "tool-call".into())
            .flex_shrink(1.)
            .min_w_0()
            .child(edge_cut(theme::WORK_ROW_LINE, INK_SUBTLE, run(markdown)))
    };
    let label = match text::call_pieces(tool) {
        text::CallPieces::Whole(call) => div().flex().min_w_0().child(cut(call)),
        text::CallPieces::File { verb, path } => div()
            .flex()
            .items_center()
            .min_w_0()
            .child(div().flex_shrink_0().whitespace_nowrap().child(verb))
            .child(cut(path)),
    };
    let row = work_row::work_row(work_row::WorkRow {
        key: SharedString::from(format!("tool-row-{}", tool.call)),
        icon: ferrite_core::transcript::tools::policy(&tool.name).icon,
        failed: text::call_failed(tool),
        error: false,
        label: label.into_any_element(),
        ink: work_row::tool_ink(),
        at,
        slot,
        listed: grouped,
        press: overlay,
    })
    .group(DISCLOSURE_ROW)
    .children(live);
    let mut card = div()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .child(target_ground(targeted, row));
    if let Some(progress) = tool.progress.filter(|_| tool.state == ToolState::Running) {
        card = card.child(running_tests(&tool.call, progress, grid));
    }
    if let Some(path) = shown_image(tool) {
        // The picture the call read, under its row whether the row is open
        // or shut (Claude's); a click opens it in the lightbox.
        let gallery: std::sync::Arc<[std::path::PathBuf]> = [path.clone()].into();
        let picture = inline_image(block, 0, &path, &gallery, row_cx.preview);
        card = card.child(div().mt(px(theme::PROMPT_IMAGE_GAP)).child(picture));
    }
    if expanded {
        if let Some(body) = call_body(block, tool, grid, selection) {
            card = card.child(body.mt(px(theme::WORK_ROW_PAD_Y)).pb(px(theme::SPACE_2)));
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
/// toggle over its header, how its members draw — every member while open;
/// while shut, the call still working (`text::group_live`) — and, open,
/// the list's scroll (`GroupList`). Members draw after the header, so
/// their text registers after its own.
pub(crate) struct GroupCx<'a> {
    pub disclosure: Option<Disclosure>,
    pub members: Box<dyn FnOnce() -> Vec<AnyElement> + 'a>,
    /// Open: the list's scroll, and whether a member is opened in it.
    pub list: Option<(&'a GroupList, bool)>,
    /// The transcript's height, for the list's cap (half of it at most).
    pub view_h: f32,
    /// The transcript's ground (`theme::tx_ground`), for the list's fades.
    pub ground: u32,
}

/// An open group's list scroll, kept by the transcript per group, so its
/// offset holds across re-projection, scrolling away and thread switches.
#[derive(Default)]
pub(crate) struct GroupList {
    handle: gpui::ScrollHandle,
    /// The content height the last frame laid out, and whether this frame
    /// laid out the same: a list still growing under its cap asks for
    /// another frame (`GroupList::settled`).
    seen_h: std::cell::Cell<f32>,
    steady: std::cell::Cell<bool>,
}

impl GroupList {
    /// Whether the last frame measured the list's content as the one before
    /// did, so its cap needs no further frame.
    pub(crate) fn settled(&self) -> bool {
        self.steady.get()
    }
}

/// A run of work under one summary (`text::group_summary`), T3 Code's
/// group: its header a work row (the group's icon, the summary, its time
/// under the pointer, no chevron: the whole row is the toggle), and, open,
/// its members as plain work rows touching in a list — no card, no rules —
/// at most `WORK_LIST_MAX_H` or half the transcript tall (while a member is
/// opened in it, that plus the member's detail, never past the transcript's
/// height, so an open member is shown whole yet never unbounded), scrolling
/// in itself, `WORK_LIST_FADE` fades at whichever edge has rows past it,
/// pinned to its end while it is there. Shut, a working call alone stands
/// under it, so a live suite's bar stays in view. Opening and shutting snap.
pub(crate) fn render_tool_group(members: &[Block], row_cx: &RowCx, state: GroupCx) -> AnyElement {
    let GroupCx {
        disclosure,
        members: draw_members,
        list,
        view_h,
        ground,
    } = state;
    let Some(leader) = members.first() else {
        return div().into_any_element();
    };
    let leader_key = text::group_tools(members)
        .next()
        .map_or_else(|| format!("{:?}", leader.id), |tool| tool.call.clone());
    let (overlay, targeted, focus) = match disclosure {
        Some(Disclosure {
            overlay,
            targeted,
            focus,
            ..
        }) => (overlay, targeted, focus),
        None => (None, false, None),
    };
    let summary = text::group_summary(members);
    let label = div()
        .debug_selector(|| "tool-group-summary".into())
        .flex_shrink(1.)
        .min_w_0()
        .child(edge_cut(
            theme::WORK_ROW_LINE,
            INK_SUBTLE,
            row_cx
                .selection
                .line(leader.id, summary.clone(), separators(&summary)),
        ));
    let mut header = work_row::work_row(work_row::WorkRow {
        key: SharedString::from(format!("tool-group-{leader_key}")),
        icon: work_row::group_icon(members),
        failed: false,
        error: false,
        label: label.into_any_element(),
        ink: work_row::tool_ink(),
        at: leader.at.clone(),
        slot: work_row::Slot::None,
        listed: false,
        press: overlay,
    })
    .group(DISCLOSURE_ROW);
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
    let calls = drawn.len();
    let members = (!drawn.is_empty()).then(|| {
        let rows = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .children(drawn);
        match list {
            None => rows.into_any_element(),
            Some((list, opened)) => group_list(&leader_key, list, opened, view_h, ground, calls, rows),
        }
    });
    div()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .text_color(rgb(INK_SUBTLE))
        .child(header)
        .children(members)
        .into_any_element()
}

/// An open group's list (T3's `ExpandedWorkGroupEntries`): its rows in a
/// box that scrolls in itself past its cap, fading at an edge only while
/// rows lie past it, and following its end while it is there.
fn group_list(
    key: &str,
    list: &GroupList,
    opened: bool,
    view_h: f32,
    ground: u32,
    calls: usize,
    rows: Div,
) -> AnyElement {
    let handle = &list.handle;
    let scrolled = -f32::from(handle.offset().y);
    let max = f32::from(handle.max_offset().y);
    if max > 0. && scrolled >= max - 1. {
        handle.scroll_to_bottom();
    }
    let content_h = max + f32::from(handle.bounds().size.height);
    list.steady.set((content_h - list.seen_h.get()).abs() < 0.5);
    list.seen_h.set(content_h);
    let half = if view_h > 0. { view_h / 2. } else { f32::MAX };
    let base = theme::WORK_LIST_MAX_H.min(half);
    let cap = if opened {
        let detail = (content_h - calls as f32 * theme::WORK_ROW_H).max(0.);
        let most = if view_h > 0. { view_h } else { f32::MAX };
        (base + detail).min(most)
    } else {
        base
    };
    let fade = |rising: bool| {
        components::fade(
            rgb(ground).into(),
            rising,
            theme::WORK_LIST_FADE,
            &[(0., 1.), (1., 0.)],
        )
        .absolute()
        .left_0()
        .right_0()
    };
    div()
        .debug_selector(|| "tool-group-members".into())
        .on_scroll_wheel({
            let handle = handle.clone();
            move |_, _, cx| {
                if handle.max_offset().y > px(0.) {
                    cx.stop_propagation();
                }
            }
        })
        .relative()
        .w_full()
        .min_w_0()
        .pb(px(theme::WORK_LIST_PAD_B))
        .child(
            div()
                .id(SharedString::from(format!("tool-group-list-{key}")))
                .w_full()
                .min_w_0()
                .max_h(px(cap))
                .overflow_y_scroll()
                .track_scroll(handle)
                .child(rows),
        )
        .when(scrolled > 1., |list| {
            list.child(
                fade(false)
                    .debug_selector(|| "tool-group-fade-top".into())
                    .top_0(),
            )
        })
        .when(max - scrolled > 1., |list| {
            list.child(
                fade(true)
                    .debug_selector(|| "tool-group-fade-bottom".into())
                    .bottom(px(theme::WORK_LIST_PAD_B)),
            )
        })
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
    use ferrite_core::provider::Provider;
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
            "Run the tests \u{21e5} Tab"
        );
        assert_eq!(
            placeholder(false, true, None, Some("Run the tests")).text(),
            "Ask anything. @ files, $ skills, / commands",
            "a draft has no conversation to predict from"
        );
    }
    use ferrite_core::transcript::{Input, Lexer};
    use ferrite_core::{Hunk, SessionEvent, ToolResult, TurnOutcome};
    use gpui::{size, TestAppContext};
    use std::sync::Arc;

    struct StartingAndWorking(Transcript);

    impl Render for StartingAndWorking {
        fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
            let line = |selector: &'static str, line: AnyElement| {
                div().debug_selector(move || selector.into()).child(line)
            };
            let working = |solo| {
                working_tail(WorkingFacts {
                    transcript: &self.0,
                    thread: ThreadId::new(1),
                    starting: false,
                    solo,
                    focused: false,
                    reduce_motion: true,
                })
                .expect("a running turn")
                .live(&[], None)
            };
            div()
                .w_full()
                .flex()
                .flex_col()
                .child(line("solo-starting", starting_line(true, true).into_any_element()))
                .child(line("solo-working", working(true)))
                .child(line("group-starting", starting_line(true, false).into_any_element()))
                .child(line("group-working", working(false)))
        }
    }

    /// A work row's label as `render_tool` cuts it: an activity run in
    /// `edge_cut`, its words' shaped width beside it.
    struct CutLabel {
        rich: crate::rich::TextCache,
        words: &'static str,
        shaped: std::rc::Rc<std::cell::Cell<gpui::Pixels>>,
    }

    impl Render for CutLabel {
        fn render(&mut self, window: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
            let size = px(theme::FS_UI);
            let style = gpui::TextStyle {
                font_family: theme::FONT_UI.into(),
                font_size: size.into(),
                ..window.text_style()
            };
            let run = style.to_run(self.words.len());
            let shaped = window
                .text_system()
                .shape_line(self.words.into(), size, &[run], None)
                .width;
            self.shaped.set(shaped);
            let markdown = crate::rich::Markdown::new(
                "cut-label",
                text::escape_words(self.words),
                self.rich.clone(),
            )
            .activity();
            div().w_full().flex().text_size(size).child(
                div()
                    .debug_selector(|| "cut-label".into())
                    .flex_shrink(1.)
                    .min_w_0()
                    .child(edge_cut(theme::WORK_ROW_LINE, INK_SUBTLE, markdown)),
            )
        }
    }

    /// A thought's row reads its words shut; open, `Thinking` while it is
    /// still being thought, `Thought` once it has settled.
    #[test]
    fn an_open_thoughts_row_reads_thinking_until_it_settles() {
        assert_eq!(thought_label("weighing it up", false, true).as_ref(), "weighing it up");
        assert_eq!(thought_label("weighing it up", true, true).as_ref(), "Thinking");
        assert_eq!(thought_label("weighing it up", true, false).as_ref(), "Thought");
    }

    /// A label that fits draws whole: the box `edge_cut` clips to is as
    /// wide as the words the run paints, so `ls -la` never draws `ls -l`.
    #[gpui::test]
    fn a_work_rows_label_that_fits_keeps_its_last_glyph(cx: &mut TestAppContext) {
        let shaped = std::rc::Rc::new(std::cell::Cell::new(px(0.)));
        let (_, cx) = cx.add_window_view({
            let shaped = shaped.clone();
            move |_, cx| {
                gpui::component::init(cx);
                CutLabel {
                    rich: crate::rich::TextCache::default(),
                    words: "ls -la",
                    shaped,
                }
            }
        });
        cx.simulate_resize(size(px(640.), px(400.)));
        cx.run_until_parked();
        let label = cx.debug_bounds("cut-label").expect("the label");
        assert!(
            label.size.width + px(1.) >= shaped.get(),
            "the box ({:?}) holds the words ({:?})",
            label.size.width,
            shaped.get()
        );
    }

    /// A provider that fails to start leaves its reason and its stderr on
    /// separate lines. The notice is one work row, so its words must sit
    /// whole inside it rather than centred and clipped through top and bottom.
    #[gpui::test]
    fn a_multi_line_notice_draws_whole_inside_its_row(cx: &mut TestAppContext) {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Notice(
            "could not start: claude CLI exited: exit status: 1\nstderr: No conversation found".into(),
        ));
        let blocks: Vec<Block> = transcript.blocks().to_vec();
        let id = blocks[0].id;
        let (_view, cx) = cx.add_window_view(|_, cx| {
            gpui::component::init(cx);
            shows_blocks(blocks.clone(), cx)
        });
        cx.simulate_resize(size(px(520.), px(400.)));
        cx.run_until_parked();
        let row = cx
            .debug_bounds(Box::leak(format!("notice-{id:?}").into_boxed_str()))
            .expect("the notice's row");
        // A glyph run is a hair taller than its box, so the bound is a line
        // of slack, not a second line's worth.
        let words = cx
            .update(|_, cx| {
                crate::rich::testing::bounds(&format!("literal-pane-block-test-{id:?}-0"), 0, cx)
            })
            .expect("the notice's run");
        assert!(
            words.top() >= row.top() - px(2.) && words.bottom() <= row.bottom() + px(2.),
            "the words {words:?} lie inside the row {row:?}"
        );
    }

    /// A wake's icon is its source's, as T3's: a watch's eye, Delegates'
    /// bot, and the alert for either's failure.
    #[test]
    fn a_wake_wears_its_sources_icon() {
        use ferrite_core::transcript::tools::ToolIcon;
        use ferrite_core::wake::{Outcome, Source, WakeNote};
        let note = |outcome, source| WakeNote {
            summary: String::new(),
            outcome,
            source,
        };
        assert_eq!(wake_icon(&note(Outcome::Updated, Source::PrWatch)), ToolIcon::Eye);
        assert_eq!(wake_icon(&note(Outcome::Completed, Source::Delegates)), ToolIcon::Bot);
        assert_eq!(wake_icon(&note(Outcome::Failed, Source::Delegates)), ToolIcon::Alert);
    }

    /// A wake the Server sent is T3's notification row, never the
    /// operator's bubble: its summary by the alert of a failure, and its
    /// whole text only once it is opened.
    #[gpui::test]
    fn a_wake_draws_as_a_notice_row_that_opens_to_its_text(cx: &mut TestAppContext) {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Wake(ferrite_core::wake::Wake {
            text: "Update on pull request #412 (https://github.com/o/r/pull/412), which Ferrite is watching for you:\n- Checks failed on abc1234:\n  - test".into(),
            outcome: ferrite_core::wake::Outcome::Failed,
            summary: "#412: checks failed, new comments".into(),
            source: ferrite_core::wake::Source::PrWatch,
        }));
        let blocks: Vec<Block> = transcript.blocks().to_vec();
        let wake = blocks.iter().find(|block| block.wake.is_some()).unwrap().id;
        let (view, cx) = cx.add_window_view(|_, cx| {
            gpui::component::init(cx);
            shows_blocks(blocks.clone(), cx)
        });
        cx.simulate_resize(size(px(640.), px(400.)));
        cx.run_until_parked();
        let row = Box::leak(format!("wake-row-{wake:?}").into_boxed_str());
        assert!(cx.debug_bounds(row).is_some(), "the wake's row");
        assert!(cx.debug_bounds("wake-summary").is_some());
        assert!(cx.debug_bounds("wake-body").is_none(), "shut, it is one line");
        assert!(
            cx.debug_bounds("transcript-prompt").is_none(),
            "no bubble: the operator sent nothing"
        );

        view.update(cx, |view, cx| {
            view.wakes_open = true;
            cx.notify();
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("wake-body").is_some(), "opened, its whole text");
    }

    /// `Starting` holds the working line's own row, Solo's and a side
    /// chat's, so the line never moves when the turn's words replace it.
    #[gpui::test]
    fn the_starting_line_stands_on_the_working_line_grid(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, cx| {
            gpui::component::init(cx);
            let mut transcript = Transcript::default();
            transcript.apply(Input::Prompt("Build".into()));
            StartingAndWorking(transcript)
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
                        .child(model_effort_chip(ModelChip {
                            mark: ferrite_ui::provider::mark(Provider::Claude),
                            model: "Sonnet 4.5".into(),
                            effort: None,
                            loading,
                            open: false,
                            group: selector.into(),
                        }))
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
        assert!(
            shimmering("starting-chip", cx),
            "the starting chip shimmers"
        );
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
        /// Every wake's row opened to its text.
        wakes_open: bool,
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
                // Every turn's fold open: these draw each block's own row.
                .chain(self.blocks.iter().filter_map(|block| {
                    matches!(block.body, Body::Prompt(_)).then_some(DisclosureId::Turn(block.id))
                }))
                .collect();
            if self.reasoning_expanded {
                expanded.extend(self.blocks.iter().filter_map(|block| {
                    matches!(&block.body, Body::Thinking(_))
                        .then_some(DisclosureId::Reasoning(block.id))
                }));
            }
            if self.wakes_open {
                expanded.extend(self.blocks.iter().filter_map(|block| {
                    block.wake.is_some().then_some(DisclosureId::Prompt(block.id))
                }));
            }
            let (presentation, body) = (
                crate::transcript::TranscriptPresentation {
                    disclosure_focus: cx.focus_handle(),
                    selection_scope: gpui::base::TextSelectionScopeId::new(),
                    thread: self.thread,
                    namespace: "pane-block-test".into(),
                    content_revision: (0, 0),
                    display_revision: self.display_revision,
                    signal_status: Some(Status::Idle),
                    focused: true,
                    reading_size: Default::default(),
                    working: None,
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
                    cited: Default::default(),
                    note: None,
                    earlier: None,
                    parent: None,
                    queued: 0,
                },
                crate::transcript::TranscriptBody {
                    blocks: self.blocks.clone(),
                    preview: crate::preview::Preview::new(cx),
                    expanded,
                    target: None,
                },
            );
            let selection = self.selection.clone();
            self.transcript.update(cx, |transcript, cx| {
                transcript.reconcile(presentation, || body, selection, cx);
                transcript.assert_text_projection(cx);
            });
            self.transcript.clone()
        }
    }

    fn shows_blocks(blocks: Vec<Block>, cx: &mut Context<ShowsBlocks>) -> ShowsBlocks {
        let thread = ThreadId::new(1);
        let selection = crate::select::TranscriptText::default();
        let transcript = cx.new(|cx| {
            crate::transcript::TranscriptView::empty(
                thread,
                "pane-block-test".into(),
                crate::preview::Preview::new(cx),
                crate::rich::TextCache::default(),
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
            wakes_open: false,
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
                Body::Compaction(_) => "compaction",
                Body::Plan(_) => "plan",
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

    /// A call's work row is one row tall, however long or many-lined its
    /// command:
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
        let tall = px(theme::WORK_ROW_H + 2. * theme::WORK_ROW_PAD_Y);
        let line = px(theme::WORK_ROW_LINE);
        let rows = [
            cx.debug_bounds("tool-row-multi")
                .expect("the heredoc's call line"),
            cx.debug_bounds("tool-row-long")
                .expect("the long command's call line"),
        ];
        cx.update(|_, cx| {
            for ((row, call), answer) in rows.iter().zip(calls).zip(answers) {
                assert!(
                    (row.size.height - tall).abs() <= px(0.5),
                    "one row tall: {row:?} / {tall:?}"
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
            ("g2", "grep -n adelaide notes.md", "", false),
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
        assert!(shut.contains("Ran 2 commands and read 1 file"), "{shut}");
        // Shut, it draws its working call alone: a call on a file copies as
        // its path, its verb being chrome.
        assert!(
            shut.contains("[notes\\.md](<ferrite-path:notes.md>)"),
            "{shut}"
        );
        assert!(!shut.contains("python3 match\\.py"), "{shut}");
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
            "python3 match\\.py",
            "grep \\-n adelaide",
            "[notes\\.md](<ferrite-path:notes.md>)",
        ] {
            assert!(open.contains(text), "{text}: {open}");
        }
        // A shut call previews nothing it printed.
        assert!(!open.contains("12 matched"), "{open}");
        // The calls are work rows on the summary's edge, under it, no card
        // and no rules: listed, they touch, `WORK_ROW_H` apart.
        let summary = cx.debug_bounds("tool-group-g1").unwrap();
        let list = cx.debug_bounds("tool-group-members").unwrap();
        let member = cx.debug_bounds("tool-row-g1").unwrap();
        assert!((member.left() - summary.left()).abs() <= px(1.0), "{member:?} {summary:?}");
        assert!(list.top() >= summary.bottom(), "the list hangs under it");
        let second = cx.debug_bounds("tool-row-g2").unwrap();
        assert!(
            ((second.top() - member.top()) - px(theme::WORK_ROW_H)).abs() <= px(0.5),
            "rows {} apart: {member:?} {second:?}",
            theme::WORK_ROW_H
        );
        assert!(cx.debug_bounds("tool-group-fade-top").is_none());
        assert!(cx.debug_bounds("tool-group-fade-bottom").is_none());

        // A call opened in the list draws Claude's body, no box of its own:
        // its `$ cmd` in a well under its line, what it printed under that.
        // A success says nothing.
        view.update(cx, |view, cx| {
            view.expanded.insert("g1".into());
            cx.notify();
        });
        cx.run_until_parked();
        assert!(copied(&view, cx).contains("12 matched"));
        let line = cx.debug_bounds("tool-row-g1").unwrap();
        let well = cx.debug_bounds("tool-input").unwrap();
        let output = cx.debug_bounds("tool-output").unwrap();
        assert!(well.top() >= line.bottom(), "{line:?} {well:?}");
        assert!(output.top() > well.bottom(), "{well:?} {output:?}");
        assert!(cx.debug_bounds("tool-card").is_none(), "no card in a card");
        assert!(cx.debug_bounds("tool-exit").is_none(), "a success is quiet");
    }

    /// Opening one call in a long group grows the group by that call's
    /// detail, no further: the other calls stay in the list's scroll, and
    /// shutting the call restores the list.
    #[gpui::test]
    fn an_opened_call_grows_its_group_by_its_detail_only(cx: &mut TestAppContext) {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("sweep".into()));
        for n in 0..60 {
            transcript.apply(Input::Event(SessionEvent::ToolStarted {
                id: format!("c{n}"),
                name: "Bash".into(),
                input: serde_json::json!({ "command": format!("grep -n item{n} notes.md") }),
            }));
            transcript.apply(Input::Event(SessionEvent::ToolCompleted {
                id: format!("c{n}"),
                output: "first\nsecond".into(),
                is_error: false,
                result: ferrite_core::ToolResult::Opaque,
            }));
        }
        let blocks: Vec<Block> = transcript.blocks().to_vec();
        let (view, cx) = cx.add_window_view(|_, cx| {
            gpui::component::init(cx);
            shows_blocks(blocks, cx)
        });
        cx.simulate_resize(size(px(900.), px(900.)));
        view.update(cx, |view, cx| {
            view.groups.insert("c0".into());
            cx.notify();
        });
        cx.run_until_parked();
        let height = |cx: &mut gpui::VisualTestContext| {
            cx.debug_bounds("tool-group-members").unwrap().size.height
        };
        let shut = height(cx);
        assert!(
            shut <= px(theme::WORK_LIST_MAX_H + theme::WORK_LIST_PAD_B),
            "an open group is capped: {shut:?}"
        );
        view.update(cx, |view, cx| {
            view.expanded.insert("c0".into());
            cx.notify();
        });
        for _ in 0..3 {
            cx.run_until_parked();
            cx.update(|window, _| window.refresh());
        }
        let opened = height(cx);
        let rows = px(theme::WORK_ROW_H * 60.);
        assert!(opened > shut, "its detail shows: {shut:?} then {opened:?}");
        assert!(
            opened < shut + px(300.) && opened < rows / 2.,
            "the other calls stay in the scroll: {shut:?} then {opened:?}"
        );
        view.update(cx, |view, cx| {
            view.expanded.remove("c0");
            cx.notify();
        });
        for _ in 0..3 {
            cx.run_until_parked();
            cx.update(|window, _| window.refresh());
        }
        assert_eq!(height(cx), shut, "shutting the call restores the list");
    }

    /// A call opened alone draws the same body under its row, no card; a
    /// failed command says its exit code over what it printed.
    #[gpui::test]
    fn a_failed_command_opened_alone_says_its_exit_over_its_output(cx: &mut TestAppContext) {
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
        let well = cx.debug_bounds("tool-input").unwrap();
        let exit = cx.debug_bounds("tool-exit").unwrap();
        let output = cx.debug_bounds("tool-output").unwrap();
        assert!(cx.debug_bounds("tool-card").is_none());
        assert!(well.top() >= line.bottom(), "{line:?} {well:?}");
        assert!(
            well.bottom() <= exit.top() && exit.bottom() <= output.top(),
            "{well:?} {exit:?} {output:?}"
        );
    }

    #[test]
    fn a_shut_group_keeps_only_its_working_call_in_view() {
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
            text::group_live(blocks, &AgentFaces::default()).first().map(|block| block.id),
            Some(blocks[5].id)
        );
        assert!(text::group_live(&blocks[..5], &AgentFaces::default()).is_empty());
    }

    #[gpui::test]
    fn every_block_kind_registers_its_selectable_text(cx: &mut TestAppContext) {
        let transcript = every_kind();
        let instruments = Instruments::of(&transcript);
        let blocks: Vec<Block> = transcript.blocks().to_vec();
        let ids: Vec<ferrite_core::transcript::BlockId> = blocks
            .iter()
            // A completed turn's end draws no row (`text::turn_stamp`).
            .filter(|block| !matches!(&block.body, Body::TurnEnd(end) if text::turn_stamp(end).is_none()))
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
                matches!(&block.body, Body::Thinking(text) if !text.trim().is_empty())
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
            all.contains("cargo test"),
            "a command's row is its command: {all}"
        );
        // An edit's diff shows only once its call is opened.
        assert_eq!(
            collapsed_and_expanded_count(&runs, "delta"),
            0,
            "the edit's diff waits for its call to open: {all}"
        );
        // A shut call registers its row alone: no verdict trails it (a
        // failure is its icon's tint, chrome), and what it printed waits
        // for it to open, below. No `└` elbow joins the run.
        assert!(
            !all.contains("applied"),
            "a shut call previews nothing: {all}"
        );
        assert!(!all.contains("42 passed"), "no verdict: {all}");
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
            findings: Vec::new(),
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
        // A command reads as itself and an edit as its path (T3's); any
        // other call is its verb in the app's words, then its argument.
        for (name, verb) in [
            ("Bash", ""),
            ("Read", "Read "),
            ("Edit", ""),
            ("Write", ""),
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
                verb: String::new(),
                argument: "cargo test".into(),
            }
        );
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
        // A read of the checkout's file by its real path, the Thread in the
        // checkout through a symlink, reads relative (`Read f01.txt`).
        #[cfg(unix)]
        {
            let (real, link) = crate::file_links::tests::symlinked_checkout("call-line");
            let mut read = tool("Read", &real.join("f01.txt").to_string_lossy(), ToolState::Ok);
            crate::transcript::relative_call(&mut read, &link);
            assert_eq!(
                text::call_line(&read),
                text::CallLine {
                    verb: "Read ".into(),
                    argument: "f01.txt".into(),
                }
            );
        }
        // The provider's description is the working line's, not the row's.
        let described = ToolBlock {
            caption: None,
            background: None,
            image: false,
            findings: Vec::new(),
            title: Some("List a directory that doesn't exist".into()),
            ..tool("Bash", "ls /definitely-missing-dir", ToolState::Ok)
        };
        assert_eq!(
            text::call_line(&described),
            text::CallLine {
                verb: String::new(),
                argument: "ls /definitely-missing-dir".into(),
            }
        );
        // One Markdown run that reads exactly as written: a path a file link.
        let read = text::CallLine {
            verb: "Read ".into(),
            argument: "src/a_b.rs".into(),
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
            findings: Vec::new(),
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
    /// finished one says nothing at its end (its fold says how long it
    /// worked, however quick).
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
        use text::turn_stamp;
        assert_eq!(turn_stamp(&end(Completed, 100)), None);
        assert_eq!(turn_stamp(&end(Completed, 322_000)), None);
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

    /// What the nav row's state word reads from each state: a pending
    /// request is what needs you, a red suite counts its failures, and
    /// idle, or a finish already seen, says nothing.
    #[test]
    fn each_state_says_its_word() {
        use theme::words;
        let failing = WallCard {
            failing_count: Some(2),
            ..Default::default()
        };
        let word = |state, kind, unread| state_word(state, kind, &failing, None, unread);
        assert_eq!(word(WallState::Idle, None, false), None, "idle says nothing");
        assert_eq!(
            word(WallState::Done, None, false),
            None,
            "a finish already seen says nothing"
        );
        assert_eq!(
            word(WallState::Done, None, true),
            Some(HeadSlot::Done),
            "a finish not yet seen is done"
        );
        assert_eq!(
            word(WallState::Failing, None, false),
            Some(HeadSlot::Failing(Some(2)))
        );
        assert_eq!(
            word(WallState::Decision, None, false),
            Some(HeadSlot::NeedsYou(words::APPROVAL))
        );
        assert_eq!(
            word(WallState::Working, Some(words::QUESTION), false),
            Some(HeadSlot::NeedsYou(words::QUESTION)),
            "a request pending anywhere needs you"
        );
        assert_eq!(word(WallState::Blocked, None, false), Some(HeadSlot::Failed));
        assert_eq!(word(WallState::Parked, None, false), Some(HeadSlot::Parked));
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
            "Ask anything. @ files, $ skills, / commands",
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
            "Ask anything. @ files, $ skills, / commands"
        );
        assert_eq!(
            placeholder(true, false, Some(&live), None).text(),
            "Ask anything. @ files, $ skills, / commands"
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
            "Ask anything. @ files, $ skills, / commands"
        );
        assert_eq!(
            placeholder(false, true, None, None).text(),
            "Ask anything. @ files, $ skills, / commands"
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
            "Ask anything. @ files, $ skills, / commands"
        );
        assert_eq!(
            placeholder(false, false, Some(&closed), Some("Run the tests")).text(),
            "Ask anything. @ files, $ skills, / commands"
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
    /// editor rows, at every level, focused or not: the input's 14 + 8, the
    /// 42px toolbar, the 14px inset.
    #[test]
    fn the_fixed_height_follows_the_composer_tokens() {
        // The row's 7 + 5 of air above and below one line (a 44 box), then
        // the 41px footer down to the card's foot.
        assert_eq!(composer_fixed_height(), 2. * (7. + 5.) + 41.);
        // A tall Pane shows every editor row; a short one keeps its
        // majority.
        assert_eq!(composer_row_limit(900., 0), crate::composer::MAX_ROWS);
        let limit = composer_row_limit(300., 0) as f32;
        assert!(
            composer_fixed_height() + limit * theme::COMPOSER_ROW_H
                <= 300. * theme::COMPOSER_MAX_PANE_FRACTION
        );
        // The queue is a 44px row per message and its 8 gap.
        assert_eq!(composer_queue_height(900., 0), 0.);
        // A row per message, up to the rows it shows; more scroll.
        assert_eq!(
            composer_queue_height(900., 1),
            theme::QUEUE_CARD_H + theme::QUEUE_CARD_GAP
        );
        assert_eq!(
            composer_queue_height(900., 2),
            2. * theme::QUEUE_CARD_H + theme::QUEUE_CARD_GAP
        );
        assert_eq!(
            composer_queue_height(900., 9),
            theme::QUEUE_ROWS_SHOWN as f32 * theme::QUEUE_CARD_H + theme::QUEUE_CARD_GAP
        );
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

    /// Claude's breakdown, as the owner's panel read it with no window
    /// reported: each share is of the 400k window, as `/context` shows it,
    /// which Free space completes; never of the categories alone.
    #[test]
    fn context_shares_are_of_the_window_free_space_completes() {
        let categories: Vec<_> = [
            ("System prompt", 1_800),
            ("System tools", 8_500),
            ("MCP tools", 2_500),
            ("MCP server instructions", 1_900),
            ("Memory files", 300),
            ("Skills", 8_900),
            ("Messages", 109_000),
            ("Autocompact buffer", 33_000),
            ("Free space", 234_100),
            ("MCP tools (deferred)", 21_000),
            ("System tools (deferred)", 33_000),
        ]
        .into_iter()
        .map(|(name, tokens)| ferrite_core::ContextCategory {
            name: name.into(),
            tokens,
        })
        .collect();
        assert_eq!(usage::context_window(None, &categories), Some(400_000));
        assert_eq!(
            usage::context_window(Some(1_000_000), &categories),
            Some(1_000_000),
            "a reported window stands"
        );
        assert_eq!(
            usage::context_window(None, &categories[..8]),
            None,
            "without Free space the window is unknown, never guessed"
        );
        let segments = context_segments(&categories, 400_000);
        let messages = segments[6].1;
        assert!((messages - 0.2725).abs() < 1e-4, "{messages}");
        assert!(segments.iter().map(|(_, share)| share).sum::<f32>() <= 1.);
    }

    // (end WP-D)

    // ---- WP-E tests (append above the end line)
    // (end WP-E)

    // ---- WP-F tests (append above the end line)
    // (end WP-F)

    // ---- WP-G tests (append above the end line)
    // (end WP-G)
}
