//! Retained, virtualized transcript rendering.
//!
//! Cockpit owns roster and cross-pane commands. This module owns the expensive
//! per-Subject row snapshot, native text, selection document and viewport.

mod rows;
mod scroll;

#[cfg(test)]
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use ferrite_core::{
    cockpit::ToolTiming,
    store::Provider,
    transcript::{Block, BlockId, Body, Status, ToolState},
    ThreadId,
};
use gpui::{
    base::ElementExt, div, list, prelude::*, px, AnyElement, App, Context, Entity, EventEmitter,
    FocusHandle, IntoElement, MouseButton, Render, SharedString, Window,
};

use self::{
    rows::{RowId, RowKind, RowShape, TranscriptRow, TranscriptRows},
    scroll::TranscriptScroll,
};
use crate::{
    attachment_preview::Preview,
    file_links::{PathTarget, TargetContext},
    hover_card::{CardStat, HoverCard},
    pane::{self, DisclosureId, DisclosureState, Grid},
    pointer::Pointer,
    rich::TextCache,
    select::{TextRuns, TranscriptText},
    theme,
};
use std::{cell::Cell, rc::Rc};

/// The answer's gutter mark: the typed `●` in the strongest ink, the
/// prototype's `.g-prose` (the agent spoke).
pub(crate) const ANSWER_MARK_INK: u32 = theme::TEXT_STRONG;

/// Who a transcript row speaks for, as the answer mark counts speakers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Speaker {
    /// Agent prose: an answer's paragraphs, lists, headings and fences.
    Agent,
    /// The operator's prompt, or a machine action (a tool row).
    Other,
}

impl Speaker {
    /// A block's speaker; `None` for the rows that neither speak nor hand
    /// the floor back (reasoning, notices, records, the stamp).
    pub(crate) fn of(body: &Body) -> Option<Self> {
        match body {
            Body::Paragraph { .. }
            | Body::Bullet { .. }
            | Body::Heading { .. }
            | Body::Code { .. } => Some(Self::Agent),
            Body::Prompt(_) | Body::Tool(_) => Some(Self::Other),
            Body::Thinking(_) | Body::Notice(_) | Body::Meta(_) | Body::TurnEnd(_) => None,
        }
    }
}

/// The answer mark's rule, one for every tier (the operator's ruling, Q2):
/// the Ferrite mark is drawn once per speaker change to the agent — on the
/// first prose after a prompt or after a tool row. Consecutive prose wears
/// none; its gutter stays empty and its text keeps the C1 edge. Rows with no
/// speaker are transparent to it.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AnswerMarks {
    agent_has_floor: bool,
}

impl AnswerMarks {
    /// Feed the next row's speaker, oldest first: whether it wears the mark.
    pub(crate) fn next(&mut self, speaker: Option<Speaker>) -> bool {
        match speaker {
            Some(Speaker::Agent) => !std::mem::replace(&mut self.agent_has_floor, true),
            Some(Speaker::Other) => {
                self.agent_has_floor = false;
                false
            }
            None => false,
        }
    }
}

/// A row another package appends after the transcript's last (the pending
/// Decision, built by `CockpitView::decision_tail`): `key` names its content
/// (a change re-renders it), `render` builds it.
/// What builds a `TranscriptTail`'s row.
pub(crate) type TailRender = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// A pinned prompt band's place: its turn's row and its offset.
type PinnedBand = Option<(usize, gpui::Pixels)>;

#[derive(Clone)]
pub(crate) struct TranscriptTail {
    pub key: u64,
    pub render: TailRender,
}

impl std::fmt::Debug for TranscriptTail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TranscriptTail")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

/// What the banner says about its Thread (filled by `sync_transcript`):
/// `Nav rows jitter on stream start`, `claude · opus 5.5 (1M) · medium ·
/// ~/ferrite on dev`, `started 7:18 pm · 2 turns · 41s working`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BannerFacts {
    pub title: SharedString,
    /// The provider, lowercase: `claude`, `codex`.
    pub provider_word: SharedString,
    /// The model as the status line names it (`opus 5.5 (1M)`).
    pub model: Option<SharedString>,
    /// The effort, only when one resolves (R3).
    pub effort: Option<SharedString>,
    /// The checkout, `~`-abbreviated.
    pub workspace: SharedString,
    pub workspace_path: PathBuf,
    pub branch: Option<SharedString>,
    /// The first prompt's send time.
    pub started_at: Option<SharedString>,
    /// How many turns have ended.
    pub turns: usize,
    /// The latest completed turn's settled time (R17).
    pub working: Option<Duration>,
}

/// Where a scene (or a command) asks a transcript to scroll.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(not(feature = "visual-reference"), allow(dead_code))]
pub(crate) enum ScrollTarget {
    /// The banner at the top.
    Top,
    /// `offset` pixels below the top of turn `turn`'s prompt band (0 is the
    /// first prompt in the window), its band pinned above.
    TurnBand { turn: usize, offset: f32 },
}

/// Owned input for one selected Subject. Cockpit clones only its retained L1
/// render window when this revision changes; rendering never borrows core.
pub(crate) struct TranscriptInput {
    pub thread: ThreadId,
    pub namespace: SharedString,
    pub content_revision: (u64, u64),
    pub display_revision: u64,
    pub blocks: Vec<Block>,
    pub signal_status: Option<Status>,
    pub timings: HashMap<String, ToolTiming>,
    /// The Thread's provider: its `✻` at a turn's end wears its colour.
    pub provider: Option<Provider>,
    pub focused: bool,
    pub reading_size: ferrite_core::settings::ReadingSize,
    pub selection_scope: gpui::base::TextSelectionScopeId,
    pub preview: Preview,
    pub expanded: HashSet<DisclosureId>,
    pub target: Option<DisclosureId>,
    pub disclosure_focus: FocusHandle,
    /// The row after the last (the pending Decision).
    pub tail: Option<TranscriptTail>,
    /// The banner heading the rows.
    pub banner: Option<BannerFacts>,
    /// The call a pending Decision gates: hidden while it waits.
    pub pending_call: Option<String>,
    /// The Thread's checkout, which path targets resolve against.
    pub workspace: Option<PathBuf>,
    /// The Pane stands alone (Solo): the banner takes the body's top
    /// padding, which board Panes do not have.
    pub solo: bool,
    /// When each edit settled, seen live: the hover card's age.
    pub settled_at: BTreeMap<String, SystemTime>,
    #[cfg(test)]
    pub disclosure_bounds: Rc<RefCell<HashMap<DisclosureId, gpui::Bounds<gpui::Pixels>>>>,
}

/// Everything a transcript's render depends on besides its blocks and its
/// disclosures, compared every root render before any block is cloned.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptKey {
    pub namespace: SharedString,
    pub content_revision: (u64, u64),
    pub display_revision: u64,
    pub focused: bool,
    pub signal_status: Option<Status>,
    pub reading_size: ferrite_core::settings::ReadingSize,
    pub banner: Option<BannerFacts>,
    pub tail: Option<u64>,
    pub pending_call: Option<String>,
    pub workspace: Option<PathBuf>,
    pub solo: bool,
}

impl TranscriptInput {
    pub(crate) fn key(&self) -> TranscriptKey {
        TranscriptKey {
            namespace: self.namespace.clone(),
            content_revision: self.content_revision,
            display_revision: self.display_revision,
            focused: self.focused,
            signal_status: self.signal_status,
            reading_size: self.reading_size,
            banner: self.banner.clone(),
            tail: self.tail.as_ref().map(|tail| tail.key),
            pending_call: self.pending_call.clone(),
            workspace: self.workspace.clone(),
            solo: self.solo,
        }
    }

    /// What the rows are projected with.
    fn shape(&self) -> RowShape {
        RowShape {
            banner: self.banner.is_some(),
            banner_pad: if self.solo { theme::BANNER_PAD_T } else { 0. },
            tail: self.tail.as_ref().map(|tail| tail.key),
            pending_call: self.pending_call.clone(),
            opened_diffs: self
                .expanded
                .iter()
                .filter_map(|id| match id {
                    DisclosureId::Diff(call) => Some(call.clone()),
                    _ => None,
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum TranscriptEvent {
    ToggleDisclosure(DisclosureId),
    /// ⌘-click on a path target: open it in a reader pane beside the
    /// Thread (`CockpitView::open_beside`).
    OpenReader {
        path: PathBuf,
        line: Option<u32>,
    },
}

/// A minimap jump in flight: from one scroll position to another, eased.
#[derive(Clone, Copy, Debug)]
struct Glide {
    from: f32,
    to: f32,
    started: Instant,
}

/// A stable GPUI entity for one Pane Subject's heavy transcript subtree.
pub(crate) struct TranscriptView {
    input: TranscriptInput,
    rows: TranscriptRows,
    /// Rows appended at the tail while the operator watched, and when: they
    /// fade in rising into place (`motion::FADE_IN`). First paint, a history
    /// window growing at its head and a row scrolled back into view never
    /// do.
    arrivals: HashMap<RowId, Instant>,
    scroll: TranscriptScroll,
    rich: TextCache,
    selection_source: TranscriptText,
    transcript_focus: FocusHandle,
    controls_end: FocusHandle,
    document: gpui::base::TextSelectionDocument,
    /// The one-second clock a live tool call's trail ticks on: armed while
    /// any call runs, for the next whole second of its count, and never
    /// faster. Idle, nothing is armed.
    second_tick: Option<gpui::Task<()>>,
    /// The disclosure the pointer last flipped, and to which state: only
    /// its chevron eases; a keyboard toggle turns it at once.
    eased: Option<(DisclosureId, bool)>,
    /// The transcript is wide enough for side-by-side diffs
    /// (`SPLIT_DIFF_MIN_W`), as of the last layout.
    wide: Rc<Cell<bool>>,
    /// The pinned prompt band's height, as of the last layout: the next
    /// turn's band pushes it up by what it overlaps.
    pinned_h: Rc<Cell<gpui::Pixels>>,
    /// The pinned band this frame drew, to notice a layout that moves it.
    pinned_drawn: Rc<Cell<PinnedBand>>,
    /// The position a re-render was last asked for.
    pinned_asked: Rc<Cell<Option<PinnedBand>>>,
    /// Each row's laid-out height, as the list last measured it: the
    /// minimap places its ticks and its view band from these.
    heights: Rc<std::cell::RefCell<HashMap<RowId, f32>>>,
    /// What prose path targets resolve against: the checkout and the paths
    /// this window's calls named.
    targets: TargetContext,
    /// The hover card this transcript hosts, while a path of its is hovered.
    hover_card: Option<Entity<HoverCard>>,
    /// A minimap jump easing in.
    glide: Option<Glide>,
    /// Room past the last row so a band jump can put its band at the top
    /// of a transcript shorter than that (a board Pane with its banner
    /// scrolled away, R2): see `Runway`.
    runway: Rc<Cell<Runway>>,
}

/// The scroll room a band jump adds under the last row: the jump's row and
/// its offset into it (until the list has laid it out), the room, and how
/// tall the rows from the jump's row down were when it was sized — rows
/// arriving later use the room up, so the tail never floats above it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Runway {
    /// The offset into `index`'s row, while the room is being sized.
    jump: Option<f32>,
    index: usize,
    room: f32,
    below: f32,
}

impl EventEmitter<TranscriptEvent> for TranscriptView {}

impl TranscriptView {
    pub(crate) fn empty(
        thread: ThreadId,
        namespace: SharedString,
        preview: Preview,
        rich: TextCache,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new(
            TranscriptInput {
                thread,
                namespace,
                content_revision: (0, 0),
                display_revision: 0,
                blocks: Vec::new(),
                signal_status: None,
                timings: HashMap::new(),
                provider: None,
                focused: false,
                reading_size: Default::default(),
                selection_scope: gpui::base::TextSelectionScopeId::new(),
                preview,
                expanded: HashSet::new(),
                target: None,
                disclosure_focus: cx.focus_handle(),
                tail: None,
                banner: None,
                pending_call: None,
                workspace: None,
                solo: false,
                settled_at: BTreeMap::new(),
                #[cfg(test)]
                disclosure_bounds: Rc::new(RefCell::new(HashMap::new())),
            },
            rich,
            TranscriptText::default(),
            cx,
        )
    }

    pub(crate) fn new(
        input: TranscriptInput,
        rich: TextCache,
        selection_source: TranscriptText,
        cx: &mut Context<Self>,
    ) -> Self {
        let rows = TranscriptRows::new(
            &input.blocks,
            &input.shape(),
            theme::answer_text_size(input.reading_size),
        );
        let scroll = TranscriptScroll::new(rows.len());
        scroll.scroll_to_bottom();
        let scope = if input.focused {
            gpui::base::TextSelectionScopeId::default()
        } else {
            input.selection_scope
        };
        // The hover card follows the pointer between path targets: a
        // transcript hosting one (or asked to by a scene) draws again.
        cx.observe_global::<crate::hover_card::HoverCards>(|view: &mut Self, cx| {
            let involved = crate::hover_card::involves(&view.input.namespace, cx);
            if !involved && view.hover_card.take().is_none() {
                return;
            }
            cx.notify();
        })
        .detach();
        let targets = target_context(&input);
        let mut view = Self {
            input,
            rows,
            arrivals: HashMap::new(),
            scroll,
            rich,
            selection_source,
            transcript_focus: cx.focus_handle(),
            // A tab stop, so the controls walk meets it at the end of the
            // transcript instead of wrapping round the window.
            controls_end: cx.focus_handle().tab_stop(true),
            document: gpui::base::TextSelectionDocument::new(scope, cx),
            second_tick: None,
            eased: None,
            wide: Rc::new(Cell::new(false)),
            pinned_h: Rc::new(Cell::new(gpui::px(0.))),
            pinned_drawn: Rc::new(Cell::new(None)),
            pinned_asked: Rc::new(Cell::new(None)),
            heights: Default::default(),
            targets,
            hover_card: None,
            glide: None,
            runway: Rc::default(),
        };
        view.register_scope(cx);
        view.sync_members(cx);
        view
    }

    /// Route ⌘-click on this transcript's path targets to
    /// `TranscriptEvent::OpenReader`.
    fn register_scope(&self, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        crate::file_links::register_scope(
            self.input.namespace.clone(),
            Rc::new(move |path, line, _window, cx| {
                let _ = weak.update(cx, |_, cx| {
                    cx.emit(TranscriptEvent::OpenReader { path, line });
                });
            }),
            cx,
        );
    }

    /// Whether this view already shows `key` (the cockpit compares it every
    /// root render before cloning a block).
    pub(crate) fn matches(&self, key: &TranscriptKey) -> bool {
        &self.input.key() == key
    }

    /// Reconcile only when Cockpit's revision key changes. Ordinary Cockpit
    /// redraws compare that key without cloning rows or notifying this entity.
    pub(crate) fn sync(
        &mut self,
        input: TranscriptInput,
        selection_source: TranscriptText,
        cx: &mut Context<Self>,
    ) {
        let before = self.input.key();
        let after = input.key();
        let namespace_changed = before.namespace != after.namespace;
        let content_changed =
            namespace_changed || before.content_revision != after.content_revision;
        let disclosure_changed =
            self.input.expanded != input.expanded || self.input.target != input.target;
        let reading_changed = before.reading_size != after.reading_size;
        let shape_changed = self.input.shape() != input.shape();
        let display_changed = before != after || disclosure_changed;
        let workspace_changed = before.workspace != after.workspace;
        self.input = input;
        self.selection_source = selection_source;
        if namespace_changed {
            self.register_scope(cx);
            self.hover_card = None;
        }
        if content_changed || workspace_changed {
            self.targets = target_context(&self.input);
        }
        // The gap table is part of the rows: a reading-size change
        // re-projects them, re-spacing every row whose gap scales; a diff
        // opening or the banner and tail changing re-project them too.
        if content_changed || reading_changed || shape_changed {
            let tail = self.rows.rows().last().map(|row| row.id().clone());
            let delta = self.rows.reconcile(
                &self.input.blocks,
                &self.input.shape(),
                theme::answer_text_size(self.input.reading_size),
            );
            self.scroll.reconcile(&delta);
            self.note_arrivals(tail, cx);
        }
        if content_changed || display_changed {
            if disclosure_changed || reading_changed {
                self.scroll.remeasure_all();
            }
            let scope = if self.input.focused {
                gpui::base::TextSelectionScopeId::default()
            } else {
                self.input.selection_scope
            };
            self.document.set_scope(scope, cx);
            if content_changed || disclosure_changed || shape_changed || workspace_changed {
                self.sync_members(cx);
            }
            cx.notify();
        }
    }

    /// Stamp the rows that now follow what was the tail: appended live.
    fn note_arrivals(&mut self, tail: Option<RowId>, cx: &App) {
        let now = cx.background_executor().now();
        let spell = crate::motion::FADE_IN.duration();
        self.arrivals
            .retain(|_, at| now.saturating_duration_since(*at) < spell);
        let Some(tail) = tail else {
            return;
        };
        let rows = self.rows.rows();
        let Some(at) = rows.iter().rposition(|row| *row.id() == tail) else {
            return;
        };
        for row in &rows[at + 1..] {
            self.arrivals.insert(row.id().clone(), now);
        }
    }

    #[cfg(test)]
    pub(crate) fn arrivals(&self) -> usize {
        self.arrivals.len()
    }

    /// How far a live-appended row is into its entrance, while it is.
    fn arrival(&self, row: &RowId, cx: &App) -> Option<f32> {
        if crate::motion::reduced_motion(cx) {
            return None;
        }
        let at = self.arrivals.get(row)?;
        let elapsed = cx
            .background_executor()
            .now()
            .saturating_duration_since(*at);
        (elapsed < crate::motion::FADE_IN.duration())
            .then(|| crate::motion::FADE_IN.progress_at(elapsed))
    }

    #[cfg(test)]
    pub(crate) fn scroll(&self) -> &TranscriptScroll {
        &self.scroll
    }
    pub(crate) fn selection_document(&self) -> gpui::base::TextSelectionDocument {
        self.document.clone()
    }
    pub(crate) fn transcript_focus(&self) -> FocusHandle {
        self.transcript_focus.clone()
    }
    /// Enter or advance the mounted native controls using GPUI's rendered tab
    /// order. The non-tab-stop boundaries keep this walk inside one Subject.
    pub(crate) fn cycle_controls(
        &self,
        reverse: bool,
        from_edge: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if from_edge {
            window.focus(
                if reverse {
                    &self.controls_end
                } else {
                    &self.transcript_focus
                },
                cx,
            );
        }
        loop {
            let before = window.focused(cx);
            if reverse {
                window.focus_prev(cx);
            } else {
                window.focus_next(cx);
            }
            let Some(focus) = window.focused(cx) else {
                return false;
            };
            if Some(&focus) == before.as_ref() || !self.transcript_focus.contains(&focus, window) {
                return false;
            }
            // The end sentinel: forward, the walk is past the last control;
            // backward, it wrapped round from the first.
            if focus == self.controls_end {
                return false;
            }
            if crate::rich::code_actions_focused(window) {
                return true;
            }
        }
    }

    pub(crate) fn tool_state(&self, call: impl Into<DisclosureId>) -> DisclosureState {
        if self.input.expanded.contains(&call.into()) {
            DisclosureState::Expanded
        } else {
            DisclosureState::Collapsed
        }
    }
    pub(crate) fn tool_targeted(&self, call: impl Into<DisclosureId>) -> bool {
        self.input.target.as_ref() == Some(&call.into())
    }
    fn clear_output_selection(&self, cx: &mut App) {
        self.rich.clear_output_selection(&self.input.namespace, cx)
    }
    #[cfg(test)]
    pub(crate) fn is_following_tail(&self) -> bool {
        self.scroll.is_following_tail()
    }

    /// Whether the received reasoning caption's own row is mounted in the
    /// viewport. Cockpit uses this to keep the pinned live caption from
    /// duplicating historical prose.
    pub(crate) fn received_reasoning_is_visible(&self, caption: &str) -> bool {
        self.rows.rows().iter().enumerate().any(|(index, row)| {
            // Parent Pane rendering asks before the list's first child has
            // measured. At the followed tail the final row is nevertheless
            // the visible row; once geometry exists, use its exact bounds.
            (self.scroll.item_is_visible(index)
                || (self.scroll.is_following_tail() && index + 1 == self.rows.len()))
                && row.blocks().iter().any(|block| {
                    matches!(&block.body, Body::Thinking(thought)
                        if pane::reasoning_text(thought).0 == caption)
                })
        })
    }

    pub(crate) fn scroll_to_bottom(&mut self, cx: &mut Context<Self>) {
        self.glide = None;
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    /// Scroll to `target` (a scene's Solo views): the banner,
    /// the live tail, or a turn's band with its output scrolled under it.
    #[cfg_attr(not(feature = "visual-reference"), allow(dead_code))]
    pub(crate) fn scroll_to(&mut self, target: ScrollTarget, cx: &mut Context<Self>) {
        self.glide = None;
        match target {
            ScrollTarget::Top => {
                self.runway.set(Runway::default());
                self.scroll.scroll_to(0, px(0.))
            }
            ScrollTarget::TurnBand { turn, offset } => {
                let Some((index, row)) = self
                    .rows
                    .rows()
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| row.kind() == RowKind::Prompt)
                    .nth(turn)
                else {
                    return;
                };
                let into = row.gap() + offset;
                // A band brought to the very top gets the room to get
                // there (the room is sized once the list has laid the rows
                // out); any other offset is clamped at the end, as a
                // browser clamps a scroll past it.
                if offset == 0.0 {
                    self.runway.set(Runway {
                        jump: Some(into),
                        index,
                        ..self.runway.get()
                    });
                } else {
                    self.runway.set(Runway::default());
                }
                self.scroll.scroll_to(index, px(into));
            }
        }
        cx.notify();
    }

    /// The prompt the operator is reading: the pinned band's turn, else the
    /// last prompt in the window. What `copy prompt` and `resend prompt`
    /// act on.
    pub(crate) fn reading_prompt(&self) -> Option<String> {
        let pinned = self
            .pinned_prompt()
            .and_then(|(index, _)| self.rows.get(index))
            .and_then(|row| match &row.blocks().first()?.body {
                Body::Prompt(line) => Some(line.clone()),
                _ => None,
            });
        pinned.or_else(|| {
            self.input
                .blocks
                .iter()
                .rev()
                .find_map(|block| match &block.body {
                    Body::Prompt(line) => Some(line.clone()),
                    _ => None,
                })
        })
    }

    /// Open the hover card on the first target naming `path` (a scene's
    /// solo-hover view), at `line` or the Thread's latest change to it.
    #[cfg_attr(not(feature = "visual-reference"), allow(dead_code))]
    pub(crate) fn preview_path(
        &mut self,
        path: &str,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let resolved = match &self.input.workspace {
            Some(root) if !std::path::Path::new(path).is_absolute() => root.join(path),
            _ => PathBuf::from(path),
        };
        let target = PathTarget::new(resolved)
            .at_line(line)
            .shown(SharedString::from(path.to_string()))
            .in_scope(self.input.namespace.clone());
        crate::hover_card::request(target, window, cx);
        cx.notify();
    }

    fn text_runs(&self) -> TextRuns {
        self.selection_source
            .clone()
            .overlay_scoped(
                self.input.thread,
                self.input.namespace.clone(),
                &self.input.blocks,
                self.rich.clone(),
            )
            .with_document(self.document.clone())
    }

    /// Synchronize all logical native text fragments without mounting rows.
    /// This runs outside a draw, including while the window is occluded.
    /// Never construct elements here: GPUI's fallback element arena retains
    /// them indefinitely. Only plain text wrappers may be made and dropped.
    fn sync_members(&mut self, cx: &mut Context<Self>) {
        let selection = self.text_runs();
        let members = selection.capture_members(|| {
            for row in self.rows.rows() {
                selection.begin_row();
                self.collect_row_text(row, &selection);
            }
        });
        self.document.sync_members(members, cx);
    }

    /// An answer's Markdown with its path targets and issue refs linked
    /// (`file_links::link_targets`): rendered and copied from the same text.
    fn linked(&self, source: &str) -> String {
        crate::file_links::link_targets(source, &self.targets)
    }

    fn collect_row_text(&self, row: &TranscriptRow, selection: &TextRuns) {
        match row.kind() {
            RowKind::Banner => {
                if let Some(facts) = &self.input.banner {
                    for line in banner_lines(facts) {
                        let _ = selection.line(BlockId::BANNER, line.text, Vec::new());
                    }
                }
                return;
            }
            RowKind::Tail => return,
            _ => {}
        }
        if let Some(source) = row.source() {
            let block = &row.blocks()[0];
            let _ = selection.answer(block.markdown_run.unwrap_or(block.id), self.linked(source));
        } else if let Some(block) = row.blocks().first() {
            let expanded = match &block.body {
                Body::Tool(tool) => self.tool_state(DisclosureId::Tool(tool.call.clone())),
                Body::Thinking(_) => self.tool_state(DisclosureId::Reasoning(block.id)),
                _ => DisclosureState::Collapsed,
            } == DisclosureState::Expanded;
            pane::collect_block_text(
                block,
                expanded,
                row.diff_shown(),
                self.wide.get(),
                selection,
            );
        }
    }

    /// Run only inside a test draw, where GPUI owns the temporary elements.
    /// Compare every logical fragment (including offscreen rows) against the
    /// actual renderer, so disclosure and copy projections cannot drift.
    #[cfg(test)]
    pub(crate) fn assert_text_projection(&self, cx: &mut Context<Self>) {
        let selection = self.text_runs();
        selection.capture_members(|| {
            for row in self.rows.rows() {
                selection.begin_row();
                self.collect_row_text(row, &selection);
            }
        });
        let logical = self.selection_source.registered(self.input.thread);
        selection.capture_members(|| {
            for row in self.rows.rows() {
                selection.begin_row();
                let _ = self.render_row(row, &selection, None, None, cx);
            }
        });
        assert_eq!(
            logical,
            self.selection_source.registered(self.input.thread),
            "logical copy fragments must match rendered row identities and text"
        );
    }

    /// What a row reads besides its block.
    fn row_cx<'a>(&'a self, selection: &'a TextRuns) -> pane::RowCx<'a> {
        pane::RowCx {
            selection,
            rich: &self.rich,
            timings: Some(&self.input.timings),
            provider: self.input.provider,
            preview: &self.input.preview,
            reading: self.input.reading_size,
            wide: self.wide.get(),
            focused: self.input.focused,
            scope: &self.input.namespace,
            workspace: self.input.workspace.as_deref(),
        }
    }

    fn render_row(
        &self,
        row: &TranscriptRow,
        selection: &TextRuns,
        view: Option<Entity<Self>>,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match row.kind() {
            RowKind::Banner => {
                return match &self.input.banner {
                    Some(facts) => self.render_banner(facts, selection),
                    None => div().into_any_element(),
                };
            }
            RowKind::Tail => {
                return match (&self.input.tail, window) {
                    (Some(tail), Some(window)) => div()
                        .debug_selector(|| "transcript-tail".into())
                        .w_full()
                        .min_w_0()
                        .flex_shrink_0()
                        .child((tail.render)(window, cx))
                        .into_any_element(),
                    _ => div().into_any_element(),
                };
            }
            _ => {}
        }
        let blocks = row.blocks();
        if let Some(source) = row.source() {
            let first = blocks
                .first()
                .expect("markdown row has a block")
                .markdown_run
                .unwrap_or(blocks[0].id);
            let grid = Grid::of(self.input.reading_size);
            let marked = row.answer_mark();
            // The prototype's `.r` with `.g-prose`: the bright `●` in the
            // gutter on the first prose after a speaker change, the answer on
            // the content column, every wrapped line hanging under it.
            return div()
                .id(SharedString::from(format!(
                    "answer-{}-{first:?}",
                    self.input.namespace
                )))
                .debug_selector(|| "transcript-answer".into())
                .flex()
                .items_start()
                .min_w_0()
                .w_full()
                .flex_shrink_0()
                .child(if marked {
                    pane::glyph_gutter(grid, pane::BULLET, ANSWER_MARK_INK)
                        .debug_selector(|| "answer-mark".into())
                } else {
                    div().flex_shrink_0().w(px(grid.gutter()))
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(selection.answer(first, self.linked(source))),
                )
                .into_any_element();
        }
        let Some(block) = blocks.first() else {
            return div().into_any_element();
        };
        if matches!(&block.body, Body::Thinking(text) if text.trim().is_empty()) {
            return div().into_any_element();
        }
        let row_cx = self.row_cx(selection);
        match &block.body {
            Body::Tool(tool) => {
                let call = DisclosureId::Tool(tool.call.clone());
                let expanded = self.tool_state(&call) == DisclosureState::Expanded;
                let disclosure = view.as_ref().map(|view| {
                    self.tool_control(&call, pane::tool_has_details(tool), view.clone())
                });
                let diff_toggle = view
                    .as_ref()
                    .map(|view| self.toggle(DisclosureId::Diff(tool.call.clone()), view.clone()));
                pane::render_tool(
                    block.id,
                    tool,
                    &row_cx,
                    pane::ToolCx {
                        expanded,
                        disclosure,
                        diff: pane::DiffFold {
                            folds: row.diff_folds(),
                            shown: row.diff_shown(),
                            toggle: diff_toggle,
                        },
                    },
                )
            }
            _ => {
                let call = match &block.body {
                    Body::Thinking(text) if pane::reasoning_has_details(text) => {
                        Some(DisclosureId::Reasoning(block.id))
                    }
                    _ => None,
                };
                pane::render_block(
                    block,
                    &row_cx,
                    call.as_ref()
                        .is_some_and(|call| self.tool_state(call) == DisclosureState::Expanded),
                    call.as_ref().and_then(|call| {
                        view.as_ref()
                            .map(|view| self.control(call, view.clone(), cx))
                    }),
                )
            }
        }
    }

    /// The banner (the prototype's `.banner`): the steel mark three rows
    /// tall two cells in, a three-cell gap, the title, the Thread's facts —
    /// its checkout a path target — and its time. Its text is selectable.
    fn render_banner(&self, facts: &BannerFacts, selection: &TextRuns) -> AnyElement {
        let grid = Grid::of(self.input.reading_size);
        let mut column = div().flex().flex_col().min_w_0();
        for (ordinal, line) in banner_lines(facts).into_iter().enumerate() {
            let strong = ordinal == 0;
            let highlights = line
                .path
                .map(|(start, len)| {
                    let from = line
                        .text
                        .char_indices()
                        .nth(start)
                        .map_or(line.text.len(), |(at, _)| at);
                    let to = line
                        .text
                        .char_indices()
                        .nth(start + len)
                        .map_or(line.text.len(), |(at, _)| at);
                    vec![(
                        from..to,
                        gpui::HighlightStyle {
                            color: Some(gpui::rgb(theme::PATH_INK).into()),
                            ..Default::default()
                        },
                    )]
                })
                .unwrap_or_default();
            let overlay = line.path.map(|(start, len)| {
                crate::file_links::path_overlay(
                    PathTarget::new(facts.workspace_path.clone())
                        .shown(facts.workspace.clone())
                        .in_scope(self.input.namespace.clone()),
                    start,
                    len,
                    grid.size,
                    grid.line,
                )
            });
            column = column.child(
                div()
                    .relative()
                    .min_w_0()
                    .truncate()
                    .map(|line| {
                        if strong {
                            line.font_weight(theme::W_STRONG)
                                .text_color(gpui::rgb(theme::TEXT_STRONG))
                        } else {
                            line.text_color(gpui::rgb(theme::TEXT_MUTED))
                        }
                    })
                    .child(selection.line(BlockId::BANNER, line.text, highlights))
                    .children(overlay),
            );
        }
        div()
            .debug_selector(|| "transcript-banner".into())
            .flex()
            .items_start()
            .gap(px(theme::BANNER_GAP_CELLS * grid.cell()))
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .child(
                div()
                    .debug_selector(|| "transcript-banner-mark".into())
                    .child(banner_mark(theme::BANNER_MARK_ROWS * grid.line)),
            )
            .child(column)
            .into_any_element()
    }

    /// A toggle that flips `call`'s disclosure from inside its row (a
    /// fold's `+ N lines`, a diff's `+ show diff`).
    fn toggle(&self, call: DisclosureId, view: Entity<Self>) -> pane::DisclosureToggle {
        let weak = view.downgrade();
        Rc::new(move |window, cx| {
            gpui::base::TextSelection::clear(window, cx);
            let _ = weak.update(cx, |view, cx| {
                view.clear_output_selection(cx);
                view.eased = Some((call.clone(), !view.input.expanded.contains(&call)));
                cx.emit(TranscriptEvent::ToggleDisclosure(call.clone()));
            });
        })
    }

    /// A tool row's disclosure: no chevron, no ground and no cursor change
    /// (the row changes nothing under the pointer); its bullet's cell is the
    /// pointer's toggle when the call has details to disclose; the keyboard
    /// target's ground and focus, and the toggle its fold lines call.
    fn tool_control(
        &self,
        call: &DisclosureId,
        details: bool,
        view: Entity<Self>,
    ) -> pane::Disclosure {
        let targeted = self.tool_targeted(call);
        let toggle = self.toggle(call.clone(), view);
        let overlay = details.then(|| {
            let grid = Grid::of(self.input.reading_size);
            let clicked = toggle.clone();
            let control = div()
                .absolute()
                .top_0()
                .left_0()
                .w(px(grid.gutter()))
                .h(px(grid.line))
                .child(
                    div()
                        .id(SharedString::from(format!("tool-button-{call}")))
                        .size_full(),
                )
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    cx.stop_propagation();
                    clicked(window, cx);
                });
            #[cfg(test)]
            let control = {
                let sink = self.input.disclosure_bounds.clone();
                let measured = call.clone();
                control.on_children_prepainted(move |bounds, _, _| {
                    if let Some(bounds) = bounds.last() {
                        sink.borrow_mut().insert(measured.clone(), *bounds);
                    }
                })
            };
            control.into_any_element()
        });
        pane::Disclosure {
            overlay,
            chevron: None,
            targeted,
            toggle: Some(toggle),
            focus: targeted.then(|| self.input.disclosure_focus.clone()),
        }
    }

    /// A reasoning row's disclosure: its header toggles under the pointer,
    /// its chevron turning.
    fn control(
        &self,
        call: &DisclosureId,
        view: Entity<Self>,
        _cx: &mut Context<Self>,
    ) -> pane::Disclosure {
        let call = call.clone();
        let clicked = call.clone();
        let expanded = self.tool_state(&call) == DisclosureState::Expanded;
        let targeted = self.tool_targeted(&call);
        let toggle = self.toggle(call.clone(), view.clone());
        let control = pane::tool_disclosure_control(
            &call,
            expanded,
            targeted,
            &self.input.disclosure_focus,
            Grid::of(self.input.reading_size).gutter(),
        )
        // The disclosure overlay fills the rendered header. Keep the handler
        // on it so the chevron, label, and trailing row text share one target.
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            gpui::base::TextSelection::clear(window, cx);
            view.update(cx, |view, cx| {
                view.clear_output_selection(cx);
                view.eased = Some((clicked.clone(), !view.input.expanded.contains(&clicked)));
                cx.emit(TranscriptEvent::ToggleDisclosure(clicked.clone()));
            });
        });
        #[cfg(test)]
        let control = {
            let sink = self.input.disclosure_bounds.clone();
            let measured = call.clone();
            control.on_children_prepainted(move |bounds, _, _| {
                // The hit box is the overlay's last child; a keyboard ring
                // may precede it.
                if let Some(bounds) = bounds.last() {
                    sink.borrow_mut().insert(measured.clone(), *bounds);
                }
            })
        };
        let eased = self.eased.as_ref() == Some(&(call.clone(), expanded));
        pane::Disclosure {
            overlay: Some(control.into_any_element()),
            chevron: Some(pane::disclosure_chevron(expanded, targeted, eased)),
            targeted,
            toggle: Some(toggle),
            focus: None,
        }
    }

    /// Arm the one-second clock while a call runs: one notify at the next
    /// whole second of the youngest-rounding live count, so a trail reading
    /// `3s` turns to `4s` on time and no faster.
    fn arm_second_tick(&mut self, cx: &mut Context<Self>) {
        if self.second_tick.is_some() {
            return;
        }
        let now = ferrite_core::clock::instant();
        let next = self
            .input
            .timings
            .values()
            .filter_map(|timing| match timing {
                ToolTiming::Running(started) => {
                    let elapsed = now.saturating_duration_since(*started);
                    Some(Duration::from_secs(elapsed.as_secs() + 1) - elapsed)
                }
                ToolTiming::Done(_) => None,
            })
            .min();
        let Some(next) = next else {
            return;
        };
        self.second_tick = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(next).await;
            let _ = this.update(cx, |view, cx| {
                view.second_tick = None;
                cx.notify();
            });
        }));
    }

    /// The hover card this transcript hosts now, hung under its path.
    fn hover_card(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let Some(hovered) = crate::hover_card::current(&self.input.namespace, cx) else {
            self.hover_card = None;
            return None;
        };
        let reuse = self
            .hover_card
            .as_ref()
            .filter(|card| card.read(cx).shows(&hovered.target))
            .cloned();
        let card = match reuse {
            Some(card) => card,
            None => {
                let shown = hovered.target.display();
                let stat = ferrite_core::transcript::file_stat_in(
                    &self.input.blocks,
                    &self.input.settled_at,
                    &hovered.target.path.display().to_string(),
                )
                .or_else(|| {
                    ferrite_core::transcript::file_stat_in(
                        &self.input.blocks,
                        &self.input.settled_at,
                        shown.as_ref(),
                    )
                });
                let target = hovered.target.clone();
                let card = cx.new(|_| {
                    HoverCard::new(
                        target,
                        stat.and_then(|stat| stat.last_change_line),
                        stat.map(|stat| CardStat {
                            added: stat.added,
                            removed: stat.removed,
                            at: stat.at,
                        }),
                    )
                });
                self.hover_card = Some(card.clone());
                card
            }
        };
        Some(crate::hover_card::hang(card, hovered.anchor, window, cx))
    }
}

/// One banner line: its text, and where its path target sits in it (in
/// characters), if it has one.
struct BannerLine {
    text: String,
    path: Option<(usize, usize)>,
}

/// The banner's three lines.
fn banner_lines(facts: &BannerFacts) -> [BannerLine; 3] {
    let seam = " \u{b7} ";
    let mut facts_line = String::new();
    for part in [
        Some(&facts.provider_word),
        facts.model.as_ref(),
        facts.effort.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        facts_line.push_str(part);
        facts_line.push_str(seam);
    }
    let start = facts_line.chars().count();
    facts_line.push_str(&facts.workspace);
    let path = (!facts.workspace.is_empty()).then_some((start, facts.workspace.chars().count()));
    if let Some(branch) = &facts.branch {
        facts_line.push_str(" on ");
        facts_line.push_str(branch);
    }
    let mut time = Vec::new();
    if let Some(started) = &facts.started_at {
        time.push(format!("started {started}"));
    }
    time.push(format!(
        "{} turn{}",
        facts.turns,
        if facts.turns == 1 { "" } else { "s" }
    ));
    if let Some(working) = facts.working {
        time.push(format!(
            "{} working",
            ferrite_core::progress::settled_duration_label(working)
        ));
    }
    [
        BannerLine {
            text: facts.title.to_string(),
            path: None,
        },
        BannerLine {
            text: facts_line,
            path,
        },
        BannerLine {
            text: time.join(seam),
            path: None,
        },
    ]
}

/// The steel mark, `height` tall, cropped to its shards (the prototype's
/// `viewBox="280 30 700 1130"`), in its steel gradient
/// (`components::steel_mark`), so the banner's text starts its gap after
/// the mark rather than after its box.
fn banner_mark(height: f32) -> gpui::Div {
    crate::components::steel_mark(height)
}

/// What prose path targets resolve against: the checkout, and every path
/// this window's calls named (their arguments and their diffs).
fn target_context(input: &TranscriptInput) -> TargetContext {
    let mut known: Vec<String> = Vec::new();
    for block in &input.blocks {
        let Body::Tool(tool) = &block.body else {
            continue;
        };
        if pane::argument_is_path(tool) && !known.contains(&tool.summary) {
            known.push(tool.summary.clone());
        }
        for diff in &tool.diffs {
            if !known.contains(&diff.path) {
                known.push(diff.path.clone());
            }
        }
    }
    TargetContext {
        workspace: input.workspace.clone(),
        known,
    }
}

/// What a minimap tick marks: a prompt, a failure, a Decision, a pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mark {
    Prompt,
    Failure,
    Decision,
    Pass,
}

impl Mark {
    fn ink(self) -> u32 {
        match self {
            Self::Prompt => theme::ACCENT,
            Self::Failure => theme::BLOCKED,
            Self::Decision => theme::ATTENTION,
            Self::Pass => theme::RUNNING,
        }
    }

    /// What a row is worth a tick for, if anything: a prompt, a failed call
    /// or turn, a Decision (its record, or the pending one at the tail), a
    /// passing check.
    pub(crate) fn of(row: &TranscriptRow) -> Option<Self> {
        match row.kind() {
            RowKind::Prompt => return Some(Self::Prompt),
            RowKind::Tail => return Some(Self::Decision),
            _ => {}
        }
        let mut pass = false;
        for block in row.blocks() {
            match &block.body {
                Body::Tool(tool) => match &tool.state {
                    ToolState::Failed(_) => return Some(Self::Failure),
                    ToolState::Ok if ferrite_core::docview::is_test_run(tool) => pass = true,
                    _ => {}
                },
                Body::TurnEnd(end) => {
                    if matches!(end.outcome, ferrite_core::TurnOutcome::Error(_)) {
                        return Some(Self::Failure);
                    }
                }
                Body::Meta(_) => return Some(Self::Decision),
                _ => {}
            }
        }
        pass.then_some(Self::Pass)
    }
}

impl TranscriptView {
    /// The prompt band the top of the viewport reads under, and how far the
    /// next turn's band pushes it up: `None` while the band is in place (or
    /// no prompt heads the rows in view).
    fn pinned_prompt(&self) -> Option<(usize, gpui::Pixels)> {
        pinned_of(&self.rows, self.scroll.list_state(), self.pinned_h.get())
    }

    /// The pinned prompt band (the prototype's sticky `.prompt`): exactly
    /// the turn's band — its `❯`, its words, its attachment chip and its
    /// send time — laid over the top of the list on the plane so it hides
    /// what scrolls under it. It answers nothing under the pointer.
    fn pinned_band(&self, index: usize, shift: gpui::Pixels) -> Option<AnyElement> {
        let row = self.rows.get(index)?;
        let block = row.blocks().first()?;
        let Body::Prompt(line) = &block.body else {
            return None;
        };
        let selection = self.text_runs();
        let band = pane::prompt_row(block, line, &self.row_cx(&selection), true);
        let measured = self.pinned_h.clone();
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(shift)
                .on_children_prepainted(move |bounds, _, _| {
                    if let Some(bounds) = bounds.first() {
                        measured.set(bounds.size.height);
                    }
                })
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "transcript-pinned-{}",
                            self.input.namespace
                        )))
                        .occlude()
                        .cursor_default()
                        .w_full()
                        // The band over an opaque plane: together they hide
                        // the rows scrolling under it, glass or not.
                        .bg(gpui::rgb(theme::PLANE))
                        .child(band),
                )
                .into_any_element(),
        )
    }

    /// Every row's estimated top and the whole transcript's height, from the
    /// heights the list last measured (an unmeasured row counts as the
    /// measured average).
    fn estimated_tops(&self) -> (Vec<f32>, f32) {
        let heights = self.heights.borrow();
        let rows = self.rows.rows();
        let known: Vec<f32> = rows
            .iter()
            .filter_map(|row| heights.get(row.id()).copied())
            .collect();
        let fallback = if known.is_empty() {
            3.0 * Grid::of(self.input.reading_size).line
        } else {
            known.iter().sum::<f32>() / known.len() as f32
        };
        let mut tops = Vec::with_capacity(rows.len());
        let mut at = 0.0;
        for row in rows {
            tops.push(at);
            at += heights.get(row.id()).copied().unwrap_or(fallback);
        }
        (tops, at + theme::BODY_PAD_B)
    }

    /// Where the viewport's top is, in the estimated tops' space.
    fn scroll_position(&self, tops: &[f32], total: f32) -> f32 {
        let state = self.scroll.list_state();
        let view_h = f32::from(state.viewport_bounds().size.height);
        if self.scroll.is_following_tail() {
            return (total - view_h).max(0.);
        }
        let top = state.logical_scroll_top();
        tops.get(top.item_ix).copied().unwrap_or(total) + f32::from(top.offset_in_item)
    }

    /// Hold the viewport's top at `at` in the estimated tops' space.
    fn jump_to(&self, at: f32, tops: &[f32]) {
        if tops.is_empty() {
            return;
        }
        let index = tops.iter().rposition(|top| *top <= at).unwrap_or(0);
        self.scroll.scroll_to(index, px((at - tops[index]).max(0.)));
    }

    /// Scroll to `goal` (estimated tops' space), easing over
    /// `MINIMAP_SCROLL_MS`; reduced motion jumps.
    fn glide_to(&mut self, goal: f32, cx: &mut Context<Self>) {
        let (tops, total) = self.estimated_tops();
        let goal = goal.clamp(0., total);
        if crate::motion::reduced_motion(cx) {
            self.glide = None;
            self.jump_to(goal, &tops);
        } else {
            self.glide = Some(Glide {
                from: self.scroll_position(&tops, total),
                to: goal,
                started: cx.background_executor().now(),
            });
        }
        cx.notify();
    }

    /// One frame of a glide in flight.
    fn advance_glide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(glide) = self.glide else {
            return;
        };
        let spec = crate::motion::MotionSpec::new(theme::MINIMAP_SCROLL_MS, crate::motion::EASE);
        let elapsed = cx
            .background_executor()
            .now()
            .saturating_duration_since(glide.started);
        let (tops, _) = self.estimated_tops();
        if elapsed >= spec.duration() {
            self.glide = None;
            self.jump_to(glide.to, &tops);
            return;
        }
        let at = crate::motion::lerp(glide.from, glide.to, spec.progress_at(elapsed));
        self.jump_to(at, &tops);
        window.request_animation_frame();
    }

    /// The minimap (the WP-A minimap tokens): a tick per prompt, failure,
    /// Decision and pass, the viewport as a translucent band while the
    /// transcript overflows, shown while the pointer is on the Pane. A click
    /// on a tick glides to its row; anywhere else on the rail centres the
    /// view on that point.
    fn minimap(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let rows = self.rows.rows();
        if rows.is_empty() {
            return None;
        }
        let state = self.scroll.list_state().clone();
        let viewport = state.viewport_bounds();
        let view_h = f32::from(viewport.size.height);
        let (tops, total) = self.estimated_tops();
        if view_h <= 0. || total <= 0. {
            return None;
        }
        let overflows = total > view_h + 1.;
        let scrolled = self.scroll_position(&tops, total);
        let band_top = (scrolled / total).clamp(0., 1.);
        let band_h = (view_h / total).clamp(0., 1.);
        let entity = cx.entity().downgrade();
        let tops = Rc::new(tops);
        let ticks: Vec<_> = rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                let mark = Mark::of(row)?;
                let at = tops[index] / total;
                let entity = entity.clone();
                let goal = tops[index] + row.gap();
                Some(
                    div()
                        .id(SharedString::from(format!("minimap-tick-{index}")))
                        .debug_selector(move || format!("minimap-tick-{mark:?}"))
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(gpui::relative(at))
                        .h(px(theme::MINIMAP_TICK_H + 2. * theme::MINIMAP_TICK_INSET))
                        .mt(px(-theme::MINIMAP_TICK_INSET))
                        .flex()
                        .items_center()
                        .px(px(theme::MINIMAP_TICK_INSET))
                        .child(
                            div()
                                .w_full()
                                .h(px(theme::MINIMAP_TICK_H))
                                .rounded(px(theme::MINIMAP_TICK_R))
                                .bg(gpui::rgb(mark.ink())),
                        )
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            let _ = entity.update(cx, |view, cx| view.glide_to(goal, cx));
                        }),
                )
            })
            .collect();
        let rail = SharedString::from(format!("transcript-minimap-{}", self.input.namespace));
        let band_key = SharedString::from(format!("{rail}-band"));
        let band_t = crate::motion::hover_t(&band_key);
        let band_ink = crate::motion::mix(
            gpui::rgba(theme::MINIMAP_BAND).into(),
            gpui::rgba(theme::MINIMAP_BAND_HOVER).into(),
            band_t,
        );
        if band_t > 0. && band_t < 1. {
            crate::file_links::mark_fading();
        }
        let blend = crate::motion::hover_listener(band_key);
        let hover_entity = entity.clone();
        let jump_entity = entity;
        Some(
            div()
                .id(rail)
                .debug_selector(|| "transcript-minimap".into())
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .w(px(theme::MINIMAP_W))
                .cursor_pointer()
                .opacity(0.)
                .group_hover(pane::PANE_GROUP, |style| style.opacity(1.))
                .on_hover(move |over, window, cx| {
                    blend(over, window, cx);
                    let _ = hover_entity.update(cx, |_, cx| cx.notify());
                })
                .when(overflows, |rail| {
                    rail.child(
                        div()
                            .id("minimap-view")
                            .absolute()
                            .left(px(theme::MINIMAP_BAND_INSET))
                            .right(px(theme::MINIMAP_BAND_INSET))
                            .top(gpui::relative(band_top))
                            .h(gpui::relative(band_h))
                            .min_h(px(theme::MINIMAP_BAND_MIN_H))
                            .rounded(px(theme::MINIMAP_BAND_R))
                            .bg(band_ink),
                    )
                })
                .children(ticks)
                .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                    cx.stop_propagation();
                    let viewport = state.viewport_bounds();
                    let height = f32::from(viewport.size.height).max(1.);
                    let fraction =
                        (f32::from(event.position.y - viewport.top()) / height).clamp(0., 1.);
                    let goal = (fraction * total - view_h / 2.).max(0.);
                    let _ = jump_entity.update(cx, |view, cx| view.glide_to(goal, cx));
                })
                .into_any_element(),
        )
    }

    /// The transcript crossed `SPLIT_DIFF_MIN_W`: diffs lay out again, side
    /// by side or unified, and their copy order follows.
    fn width_changed(&mut self, wide: bool, cx: &mut Context<Self>) {
        if self.wide.get() == wide {
            return;
        }
        self.wide.set(wide);
        self.sync_members(cx);
        self.scroll.remeasure_all();
        cx.notify();
    }
}

/// The prompt band the top of the viewport reads under (see
/// `TranscriptView::pinned_prompt`), from the list's last layout: `None`
/// before the list has laid out, while the band is in place, or when no
/// prompt heads the rows in view.
fn pinned_of(
    rows: &TranscriptRows,
    state: &gpui::ListState,
    pinned_h: gpui::Pixels,
) -> Option<(usize, gpui::Pixels)> {
    let rows = rows.rows();
    let viewport = state.viewport_bounds();
    if rows.is_empty() || viewport.size.height <= px(0.) {
        return None;
    }
    let top = state.logical_scroll_top();
    // Past the last row is the tail's sentinel before layout resolves it.
    if top.item_ix >= rows.len() {
        return None;
    }
    let prompt = rows[..=top.item_ix]
        .iter()
        .rposition(|row| row.kind() == RowKind::Prompt)?;
    if prompt == top.item_ix && top.offset_in_item <= px(rows[prompt].gap()) {
        return None;
    }
    let mut shift = px(0.);
    if let Some(next) = (top.item_ix + 1..rows.len()).find(|ix| rows[*ix].kind() == RowKind::Prompt)
    {
        if let Some(bounds) = state.bounds_for_item(next) {
            let band_top = bounds.top() + px(rows[next].gap()) - viewport.top();
            if band_top < pinned_h {
                shift = band_top - pinned_h;
            }
        }
    }
    Some((prompt, shift))
}

impl Render for TranscriptView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.arm_second_tick(cx);
        self.document.begin_viewport_update(cx);
        crate::file_links::begin_layout(&self.input.namespace, cx);
        // The answers' file links are this transcript's path targets.
        self.rich.set_path_scope(self.input.namespace.clone());
        self.advance_glide(window, cx);
        let rows = self.rows.clone();
        let selection = self.text_runs();
        let view = cx.entity();
        // The pinned band reads the last layout, before a tail follow
        // resets the list to its end sentinel for the next one.
        let pinned = self.pinned_prompt();
        self.pinned_drawn.set(pinned);
        if self.scroll.is_following_tail() {
            self.scroll.scroll_to_bottom();
        }
        let grid = Grid::of(self.input.reading_size);
        let list = list(
            self.scroll.list_state().clone(),
            move |index, window, cx| {
                let Some(row) = rows.get(index).cloned() else {
                    return div().into_any_element();
                };
                let (element, arrival) = view.update(cx, |view, cx| {
                    selection.begin_row();
                    let element =
                        view.render_row(&row, &selection, Some(cx.entity()), Some(window), cx);
                    // A path underline mid-fade: draw again next frame.
                    if crate::file_links::take_fading() {
                        cx.notify();
                    }
                    (element, view.arrival(row.id(), cx))
                });
                // Every row is wrapped: a list item is laid out as its own
                // root, where a bare row's `w_full` has no parent width to
                // resolve against and shrinks to its text. The wrapper holds
                // the row's inset (two cells left, three right; a prompt band
                // runs full width and insets its own text) and the row's own
                // gap above it (the first row's is the body's top padding:
                // the list's own top padding flickers mid-scroll).
                let gap = row.gap();
                let band = row.kind() == RowKind::Prompt;
                let wrapper = div().w_full().pt(px(gap)).when(!band, |wrapper| {
                    wrapper.pl(px(theme::TX_PAD_L)).pr(px(theme::TX_PAD_R))
                });
                // A row appended live rises into place; its gap does not
                // move, so the rows above it hold still.
                let wrapper = match arrival {
                    Some(t) => {
                        window.request_animation_frame();
                        crate::motion::fade_in_at(wrapper, t)
                    }
                    None => wrapper,
                };
                wrapper.child(element).into_any_element()
            },
        )
        // The bottom padding is the list's own: it counts in the scroll
        // extent and the tail follow. A band jump's room rides it.
        .pb(px(theme::BODY_PAD_B + self.runway.get().room))
        .size_full()
        .min_h_0();
        let scroll = self.scroll.clone();
        let gaps = self.rows.clone();
        let heights = self.heights.clone();
        let wide = self.wide.clone();
        let weak = cx.entity().downgrade();
        let pinned_drawn = self.pinned_drawn.clone();
        let pinned_asked = self.pinned_asked.clone();
        let pinned_h = self.pinned_h.clone();
        let runway = self.runway.clone();
        let line = grid.line;
        let half = grid.half();
        let list = div()
            .on_children_prepainted(move |_, window, cx| {
                let anchored = scroll.did_layout();
                // A band jump's room: enough under the last row for the
                // band to reach the top, used up as rows arrive under it.
                {
                    let state = scroll.list_state();
                    let mut now = runway.get();
                    // The rows' height from `from` down, when the list laid
                    // them all out (a row past the viewport means there is
                    // height enough already).
                    let below = |from: usize| -> Option<f32> {
                        let mut total = 0.0;
                        for index in from..gaps.len() {
                            total += f32::from(state.bounds_for_item(index)?.size.height);
                        }
                        Some(total)
                    };
                    if let Some(into) = now.jump {
                        let measured = below(now.index);
                        let view_h = f32::from(state.viewport_bounds().size.height);
                        let room = measured.map_or(0.0, |below| {
                            (into + view_h - below - theme::BODY_PAD_B).max(0.0)
                        });
                        let settled = (room - now.room).abs() < 0.5;
                        now = Runway {
                            jump: (!settled).then_some(into),
                            index: now.index,
                            room,
                            below: measured.unwrap_or(now.below),
                        };
                        runway.set(now);
                        if !settled {
                            state.scroll_to(gpui::ListOffset {
                                item_ix: now.index,
                                offset_in_item: px(into),
                            });
                            let weak = weak.clone();
                            window.defer(cx, move |_, cx| {
                                let _ = weak.update(cx, |_, cx| cx.notify());
                            });
                        }
                    } else if now.room > 0.0 {
                        match below(now.index) {
                            Some(grown) if grown > now.below + 0.5 => {
                                now.room = (now.room - (grown - now.below)).max(0.0);
                                now.below = grown;
                                runway.set(now);
                            }
                            Some(_) => {}
                            // The rows under it outgrew the viewport.
                            None => runway.set(Runway::default()),
                        }
                    }
                }
                // A pinned band hides the list's top down to its foot, and
                // the row passing under the foot down to its next whole
                // line.
                let now = pinned_of(&gaps, scroll.list_state(), pinned_h.get());
                let band = now.map(|(_, shift)| pinned_h.get() + shift);
                let settled = scroll.settle_top(
                    band,
                    |index| {
                        gaps.get(index).map_or(0., |row| {
                            row.gap()
                                + if row.kind() == RowKind::Prompt {
                                    half
                                } else {
                                    0.
                                }
                        })
                    },
                    line,
                );
                // What the list measured feeds the minimap's estimates.
                {
                    let state = scroll.list_state();
                    let mut heights = heights.borrow_mut();
                    let start = state.logical_scroll_top().item_ix;
                    for index in start..gaps.len() {
                        let Some(bounds) = state.bounds_for_item(index) else {
                            break;
                        };
                        if let Some(row) = gaps.get(index) {
                            heights.insert(row.id().clone(), f32::from(bounds.size.height));
                        }
                    }
                    if heights.len() > 2 * gaps.len() + 64 {
                        let live: HashSet<_> = gaps.rows().iter().map(|row| row.id()).collect();
                        heights.retain(|id, _| live.contains(id));
                    }
                }
                // The band pinned at render came from the previous layout:
                // when this layout moves it, render again.
                let drawn = pinned_drawn.get();
                let moved = match (now, drawn) {
                    (Some((a, x)), Some((b, y))) => a != b || (x - y).abs() > px(0.5),
                    (None, None) => false,
                    _ => true,
                };
                // Ask once per position: a layout that keeps moving the band
                // (it cannot, but a loop must never be possible) waits for the
                // next ordinary render.
                if moved && pinned_asked.get() != Some(now) {
                    pinned_asked.set(Some(now));
                    let weak = weak.clone();
                    window.defer(cx, move |_, cx| {
                        let _ = weak.update(cx, |_, cx| cx.notify());
                    });
                }
                let width = scroll.list_state().viewport_bounds().size.width;
                let is_wide = width >= px(theme::SPLIT_DIFF_MIN_W);
                if width > px(0.) && is_wide != wide.get() {
                    let weak = weak.clone();
                    window.defer(cx, move |_, cx| {
                        let _ = weak.update(cx, |view, cx| view.width_changed(is_wide, cx));
                    });
                }
                if anchored || settled {
                    window.defer(cx, |window, _| window.refresh());
                }
            })
            .id(SharedString::from(format!(
                "transcript-{}",
                self.input.namespace
            )))
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .w_full()
            .min_h_0()
            // One grid for the whole transcript (theme rule 1): the reading
            // size on its 1.5x line, every row inheriting it.
            .font_family(theme::FONT_UI)
            .text_size(px(grid.size))
            .line_height(px(grid.line))
            .text_color(gpui::rgb(theme::TEXT))
            .hover_text()
            .track_focus(&self.transcript_focus)
            .child(list)
            .child(
                div()
                    .id("transcript-controls-end")
                    .track_focus(&self.controls_end),
            )
            .text_selection_scope(if self.input.focused {
                gpui::base::TextSelectionScopeId::default()
            } else {
                self.input.selection_scope
            });
        let pinned = pinned.and_then(|(index, shift)| self.pinned_band(index, shift));
        let minimap = self.minimap(cx);
        if crate::file_links::take_fading() {
            cx.notify();
        }
        let card = self.hover_card(window, cx);
        div()
            .relative()
            .flex()
            .flex_col()
            .min_w_0()
            .size_full()
            .min_h_0()
            .font_family(theme::FONT_UI)
            .text_size(px(grid.size))
            .line_height(px(grid.line))
            .child(list)
            // The band and the row passing under its foot, down to its next
            // whole line: under an opaque plane (a 94% glass plane would let
            // a cut glyph ghost through), so no partial line shows
            // (`settle_top`).
            .children((self.scroll.top_mask() > px(0.)).then(|| {
                div()
                    .debug_selector(|| "transcript-top-mask".into())
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(self.scroll.top_mask())
                    .bg(gpui::rgb(theme::PLANE))
            }))
            .children(pinned)
            .children(minimap)
            .children(card)
    }
}
