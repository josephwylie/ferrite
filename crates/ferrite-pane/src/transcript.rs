//! Retained, virtualized transcript rendering.
//!
//! Cockpit owns roster and cross-pane commands. This module owns the expensive
//! per-Subject row snapshot, native text, selection document and viewport.

mod find;
#[cfg(any(test, feature = "test-support"))]
pub use find::FindState;
mod rows;
pub use rows::gated;
mod scroll;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Instant, SystemTime};

use ferrite_core::{
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
    file_links::TargetContext,
    hover_card::{CardStat, HoverCard},
    pane::{self, DisclosureId, DisclosureState, Grid},
    preview::Preview,
    rich::TextCache,
    select::{RunText, TextRuns, TranscriptText},
};
use ferrite_ui::{pointer::Pointer, theme};
use std::{cell::Cell, rc::Rc};

/// A row another package appends after the transcript's last (the pending
/// Decision, built by `CockpitView::decision_tail`): `key` names its content
/// (a change re-renders it), `render` builds it.
/// What builds a `TranscriptTail`'s row.
pub(crate) type TailRender = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// A pinned prompt band's place: its turn's row and its offset.
type PinnedBand = Option<(usize, gpui::Pixels)>;

#[derive(Clone)]
pub struct TranscriptTail {
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

/// Where a scene (or a command) asks a transcript to scroll.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(not(feature = "visual-reference"), allow(dead_code))]
pub enum ScrollTarget {
    /// The top of the rows.
    Top,
    /// `offset` pixels below the top of turn `turn`'s prompt band (0 is the
    /// first prompt in the window), its band pinned above.
    TurnBand { turn: usize, offset: f32 },
}

/// The paths a call names, read from the Thread's checkout: inside it,
/// relative (`Read(src/stats.rs)`, `Update(src/stats.rs)`), as Claude Code
/// prints them — Claude reports every
/// path absolute. A path outside the checkout stays whole.
fn relative_paths(input: &mut TranscriptInput) {
    let Some(root) = input.workspace.clone() else {
        return;
    };
    let inside = |path: &str| -> Option<String> {
        std::path::Path::new(path)
            .strip_prefix(&root)
            .ok()
            .filter(|rest| !rest.as_os_str().is_empty())
            .map(|rest| rest.to_string_lossy().into_owned())
    };
    for block in &mut input.blocks {
        let Body::Tool(tool) = &mut block.body else {
            continue;
        };
        // The diffs keep their whole paths: a hunk's section is read off
        // the file on disk (`pane::file_section`).
        if pane::argument_is_path(tool) {
            if let Some(path) = inside(&tool.summary) {
                tool.summary = path;
            }
        }
    }
}

/// Owned input for one selected Subject. Cockpit clones only its retained L1
/// render window when this revision changes; rendering never borrows core.
pub struct TranscriptInput {
    pub thread: ThreadId,
    pub namespace: SharedString,
    pub content_revision: (u64, u64),
    pub display_revision: u64,
    pub blocks: Vec<Block>,
    pub signal_status: Option<Status>,
    pub focused: bool,
    pub reading_size: ferrite_core::settings::ReadingSize,
    pub selection_scope: gpui::base::TextSelectionScopeId,
    pub preview: Preview,
    pub expanded: HashSet<DisclosureId>,
    pub target: Option<DisclosureId>,
    pub disclosure_focus: FocusHandle,
    /// The working row while a turn runs (`pane::working_tail`), after the
    /// last row.
    pub working: Option<TranscriptTail>,
    /// The live reasoning caption the working line names, and whether the
    /// Cockpit took its own row for in view when it chose the line's words:
    /// once laid out, if that is wrong, the transcript says so
    /// (`TranscriptEvent::ReasoningShown`) and the line is chosen again.
    pub working_reasoning: Option<(String, bool)>,
    /// The row after the last (the pending Decision).
    pub tail: Option<TranscriptTail>,
    /// The call a pending Decision gates: hidden while it waits.
    pub pending_call: Option<String>,
    /// The Thread's checkout, which path targets resolve against.
    pub workspace: Option<PathBuf>,
    /// The Pane stands alone (Solo): the first row takes the body's top
    /// padding, which board Panes do not have.
    pub solo: bool,
    /// Whether the model's thinking shows as `∴` rows (Settings'
    /// `show_thinking`, the palette's `show thinking`).
    pub thinking: bool,
    /// When each edit settled, seen live: the hover card's age.
    pub settled_at: BTreeMap<String, SystemTime>,
}

/// Everything a transcript's render depends on besides its blocks and its
/// disclosures, compared every root render before any block is cloned.
#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptKey {
    pub namespace: SharedString,
    pub content_revision: (u64, u64),
    pub display_revision: u64,
    pub focused: bool,
    pub signal_status: Option<Status>,
    pub reading_size: ferrite_core::settings::ReadingSize,
    pub working: Option<u64>,
    pub tail: Option<u64>,
    pub pending_call: Option<String>,
    pub workspace: Option<PathBuf>,
    pub solo: bool,
    pub thinking: bool,
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
            working: self.working.as_ref().map(|working| working.key),
            tail: self.tail.as_ref().map(|tail| tail.key),
            pending_call: self.pending_call.clone(),
            workspace: self.workspace.clone(),
            solo: self.solo,
            thinking: self.thinking,
        }
    }

    /// What the rows are projected with.
    fn shape(&self) -> RowShape {
        RowShape {
            top_pad: if self.solo {
                theme::TX_PAD_T
            } else {
                theme::TX_PAD_T_BOARD
            },
            working: self.working.as_ref().map(|working| working.key),
            tail: self.tail.as_ref().map(|tail| tail.key),
            pending_call: self.pending_call.clone(),
            thinking: self.thinking,
        }
    }
}

#[derive(Clone, Debug)]
pub enum TranscriptEvent {
    ToggleDisclosure(DisclosureId),
    /// ⌘-click on a path target: open it in a reader pane beside the
    /// Thread (`CockpitView::open_beside`).
    OpenReader {
        path: PathBuf,
        line: Option<u32>,
    },
    /// The live reasoning caption's own row came into or left view, against
    /// what the working line was chosen for (`working_reasoning`).
    ReasoningShown,
}

/// A minimap jump in flight: from one scroll position to another, eased.
#[derive(Clone, Copy, Debug)]
struct Glide {
    from: f32,
    to: f32,
    started: Instant,
    /// The glide ends at the tail and follows it again (jump to latest).
    follow: bool,
}

/// A stable GPUI entity for one Pane Subject's heavy transcript subtree.
pub struct TranscriptView {
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
    /// Where each disclosure control was last laid out, which the tests
    /// aim at.
    #[cfg(any(test, feature = "test-support"))]
    disclosure_bounds: Rc<std::cell::RefCell<HashMap<DisclosureId, gpui::Bounds<gpui::Pixels>>>>,
    /// What prose path targets resolve against: the checkout and the paths
    /// this window's calls named.
    targets: TargetContext,
    /// The hover card this transcript hosts, while a path of its is hovered.
    hover_card: Option<Entity<HoverCard>>,
    /// A minimap jump easing in.
    glide: Option<Glide>,
    /// Room past the last row so a band jump can put its band at the top
    /// of a transcript shorter than that (a board Pane with its first prompt
    /// scrolled away, R2): see `Runway`.
    runway: Rc<Cell<Runway>>,
    /// The agents' visuals in these rows: their live pages (ADR 0013).
    visuals: crate::visual::Shelf,
    /// The list's width as of the last layout (rows can't ask the list
    /// while it lays them out).
    list_w: Rc<Cell<f32>>,
    /// The texts copy and find read, in row order, each with its row.
    texts: Vec<(usize, RunText)>,
    /// The find bar, while it is open (⌘F).
    find: Option<find::TranscriptFind>,
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
                focused: false,
                reading_size: Default::default(),
                selection_scope: gpui::base::TextSelectionScopeId::new(),
                preview,
                expanded: HashSet::new(),
                target: None,
                disclosure_focus: cx.focus_handle(),
                working: None,
                working_reasoning: None,
                tail: None,
                pending_call: None,
                workspace: None,
                solo: false,
                thinking: false,
                settled_at: BTreeMap::new(),
            },
            rich,
            TranscriptText::default(),
            cx,
        )
    }

    pub(crate) fn new(
        mut input: TranscriptInput,
        rich: TextCache,
        selection_source: TranscriptText,
        cx: &mut Context<Self>,
    ) -> Self {
        relative_paths(&mut input);
        let rows = TranscriptRows::new(
            &input.blocks,
            &input.shape(),
            Grid::pane(input.reading_size, input.solo).size,
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
        // Chromium came up (the first visual asked for it): draw the
        // visuals that were waiting.
        cx.observe_global::<crate::visual::engine::Visuals>(|_, cx| cx.notify())
            .detach();
        let weak = cx.entity().downgrade();
        let visuals = crate::visual::Shelf::new(Rc::new(move |id: &str, cx: &mut App| {
            let _ = weak.update(cx, |view: &mut Self, cx| view.visual_height_changed(id, cx));
        }));
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
            pinned_h: Rc::new(Cell::new(gpui::px(0.))),
            pinned_drawn: Rc::new(Cell::new(None)),
            pinned_asked: Rc::new(Cell::new(None)),
            heights: Default::default(),
            #[cfg(any(test, feature = "test-support"))]
            disclosure_bounds: Default::default(),
            targets,
            hover_card: None,
            glide: None,
            runway: Rc::default(),
            visuals,
            list_w: Rc::default(),
            texts: Vec::new(),
            find: None,
        };
        view.register_scope(cx);
        view.sync_members(cx);
        let owner = cx.entity_id();
        cx.on_release(move |view: &mut Self, cx| view.forget_namespace(owner, cx))
            .detach();
        view
    }

    /// Route ⌘-click on this transcript's path targets to
    /// `TranscriptEvent::OpenReader`.
    fn register_scope(&self, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        crate::file_links::register_scope(
            self.input.namespace.clone(),
            cx.entity_id(),
            Rc::new(move |path, line, _window, cx| {
                let _ = weak.update(cx, |_, cx| {
                    cx.emit(TranscriptEvent::OpenReader { path, line });
                });
            }),
            cx,
        );
    }

    /// Nothing under this transcript's namespace can be drawn again: the
    /// transcript was released, or its history regenerated under a new
    /// namespace. Release its ⌘-click route, its laid-out targets and its
    /// native text, unless another transcript has taken the namespace.
    /// A Subject switch keeps both transcripts and their namespaces.
    fn forget_namespace(&self, owner: gpui::EntityId, cx: &mut App) {
        if crate::file_links::release_scope(&self.input.namespace, owner, cx) {
            self.rich.forget_namespace(&self.input.namespace);
        }
    }

    /// Whether this view already shows `key` (the cockpit compares it every
    /// root render before cloning a block).
    pub fn matches(&self, key: &TranscriptKey) -> bool {
        &self.input.key() == key
    }

    /// Reconcile only when Cockpit's revision key changes. Ordinary Cockpit
    /// redraws compare that key without cloning rows or notifying this entity.
    pub fn sync(
        &mut self,
        mut input: TranscriptInput,
        selection_source: TranscriptText,
        cx: &mut Context<Self>,
    ) {
        relative_paths(&mut input);
        let before = self.input.key();
        let after = input.key();
        let namespace_changed = before.namespace != after.namespace;
        let content_changed =
            namespace_changed || before.content_revision != after.content_revision;
        let disclosure_changed =
            self.input.expanded != input.expanded || self.input.target != input.target;
        let reading_changed = before.reading_size != after.reading_size;
        // The answer above the working row drops its actions row while the
        // turn runs: its height changes when the turn starts or ends.
        let working_toggled = before.working.is_some() != after.working.is_some();
        let shape_changed = self.input.shape() != input.shape();
        let display_changed = before != after || disclosure_changed;
        let workspace_changed = before.workspace != after.workspace;
        if namespace_changed {
            self.forget_namespace(cx.entity_id(), cx);
        }
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
        // opening or the top padding and tail changing re-project them too.
        if content_changed || reading_changed || shape_changed {
            // The newest content row: the working row and the Decision
            // stand after it, and do not move it.
            let tail = self
                .rows
                .rows()
                .iter()
                .rev()
                .find(|row| !matches!(row.kind(), RowKind::Working | RowKind::Tail))
                .map(|row| row.id().clone());
            let delta =
                self.rows
                    .reconcile(&self.input.blocks, &self.input.shape(), self.grid().size);
            self.scroll.reconcile(&delta);
            self.note_arrivals(tail, cx);
            let live: HashSet<&str> = self
                .input
                .blocks
                .iter()
                .filter_map(|block| crate::visual::visual_of(block).map(|v| v.id.as_str()))
                .collect();
            self.visuals.retain(&|id| live.contains(id));
        }
        if content_changed || display_changed {
            if disclosure_changed || reading_changed || working_toggled {
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

    /// Stamp the content rows that now follow what was the newest: appended
    /// live. The working row and the Decision after them are not content.
    fn note_arrivals(&mut self, tail: Option<RowId>, cx: &App) {
        let now = cx.background_executor().now();
        let spell = ferrite_ui::motion::FADE_IN.duration();
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
            if !matches!(row.kind(), RowKind::Working | RowKind::Tail) {
                self.arrivals.insert(row.id().clone(), now);
            }
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn arrivals(&self) -> usize {
        self.arrivals.len()
    }

    /// A minimap glide is in flight.
    #[cfg(any(test, feature = "test-support"))]
    pub fn gliding(&self) -> bool {
        self.glide.is_some()
    }

    /// How far a live-appended row is into its entrance, while it is.
    fn arrival(&self, row: &RowId, cx: &App) -> Option<f32> {
        if ferrite_ui::motion::reduced_motion(cx) {
            return None;
        }
        let at = self.arrivals.get(row)?;
        let elapsed = cx
            .background_executor()
            .now()
            .saturating_duration_since(*at);
        (elapsed < ferrite_ui::motion::FADE_IN.duration())
            .then(|| ferrite_ui::motion::FADE_IN.progress_at(elapsed))
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn scroll(&self) -> &TranscriptScroll {
        &self.scroll
    }
    pub fn selection_document(&self) -> gpui::base::TextSelectionDocument {
        self.document.clone()
    }
    pub(crate) fn transcript_focus(&self) -> FocusHandle {
        self.transcript_focus.clone()
    }
    /// Enter or advance the mounted native controls using GPUI's rendered tab
    /// order. The non-tab-stop boundaries keep this walk inside one Subject.
    pub fn cycle_controls(
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
    #[cfg(any(test, feature = "test-support"))]
    pub fn is_following_tail(&self) -> bool {
        self.scroll.is_following_tail()
    }

    /// After a layout: if the live reasoning's own row is in view where the
    /// working line was chosen as if it were not (or the other way round),
    /// ask for the line again.
    fn check_reasoning_shown(&self, cx: &mut Context<Self>) {
        if let Some((caption, assumed)) = &self.input.working_reasoning {
            if self.received_reasoning_is_visible(caption) != *assumed {
                cx.emit(TranscriptEvent::ReasoningShown);
            }
        }
    }

    /// Whether the received reasoning caption's own row is mounted in the
    /// viewport. Cockpit uses this to keep the pinned live caption from
    /// duplicating historical prose.
    pub fn received_reasoning_is_visible(&self, caption: &str) -> bool {
        // The newest content row: the working row and the Decision stand
        // after it.
        let newest = self
            .rows
            .rows()
            .iter()
            .rposition(|row| !matches!(row.kind(), RowKind::Working | RowKind::Tail));
        self.rows.rows().iter().enumerate().any(|(index, row)| {
            // Parent Pane rendering asks before the list's first child has
            // measured. At the followed tail the newest row is nevertheless
            // visible; once geometry exists, use its exact bounds.
            (self.scroll.item_is_visible(index)
                || (self.scroll.is_following_tail() && Some(index) == newest))
                && row.blocks().iter().any(|block| {
                    matches!(&block.body, Body::Thinking(thought)
                        if pane::reasoning_text(thought).0 == caption)
                })
        })
    }

    pub fn scroll_to_bottom(&mut self, cx: &mut Context<Self>) {
        self.glide = None;
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    /// Scroll to `target` (a scene's Solo views): the top,
    /// the live tail, or a turn's band with its output scrolled under it.
    #[cfg_attr(not(feature = "visual-reference"), allow(dead_code))]
    pub fn scroll_to(&mut self, target: ScrollTarget, cx: &mut Context<Self>) {
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
    pub fn reading_prompt(&self) -> Option<String> {
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

    /// The scope this transcript's path targets register under (what
    /// `file_links::laid_out` finds them by): a headless driver points the
    /// mouse at one.
    #[cfg(feature = "visual-reference")]
    pub fn namespace(&self) -> SharedString {
        self.input.namespace.clone()
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
            .with_find(self.find_marks())
    }

    /// Synchronize all logical native text fragments without mounting rows.
    /// This runs outside a draw, including while the window is occluded.
    /// Never construct elements here: GPUI's fallback element arena retains
    /// them indefinitely. Only plain text wrappers may be made and dropped.
    fn sync_members(&mut self, cx: &mut Context<Self>) {
        let selection = self.text_runs();
        let mut rows = Vec::new();
        let (members, texts) = selection.capture_members(|| {
            for (index, row) in self.rows.rows().iter().enumerate() {
                selection.begin_row();
                self.collect_row_text(row, &selection);
                rows.resize(selection.captured(), index);
            }
        });
        self.document.sync_members(members, cx);
        self.texts = rows.into_iter().zip(texts).collect();
        if self.find.is_some() {
            self.refind(cx);
        }
    }

    /// An answer's Markdown with its path targets and issue refs linked
    /// (`file_links::link_targets`): rendered and copied from the same text.
    fn linked(&self, source: &str) -> String {
        crate::file_links::link_targets(source, &self.targets)
    }

    fn collect_row_text(&self, row: &TranscriptRow, selection: &TextRuns) {
        if matches!(row.kind(), RowKind::Tail | RowKind::Working) {
            return;
        }
        if let Some(source) = row.source() {
            let block = &row.blocks()[0];
            let _ = selection.answer(block.markdown_run.unwrap_or(block.id), self.linked(source));
        } else if row.kind() == RowKind::Group {
            pane::collect_group_text(
                row.blocks(),
                self.group_expanded(row),
                |tool| {
                    self.tool_state(DisclosureId::Tool(tool.call.clone()))
                        == DisclosureState::Expanded
                },
                selection,
            );
        } else if let Some(block) = row.blocks().first() {
            let expanded = match &block.body {
                Body::Tool(tool) => self.tool_state(DisclosureId::Tool(tool.call.clone())),
                Body::Thinking(_) => self.tool_state(DisclosureId::Reasoning(block.id)),
                _ => DisclosureState::Collapsed,
            } == DisclosureState::Expanded;
            pane::collect_block_text(block, expanded, selection);
        }
    }

    /// Run only inside a test draw, where GPUI owns the temporary elements.
    /// Compare every logical fragment (including offscreen rows) against the
    /// actual renderer, so disclosure and copy projections cannot drift.
    #[cfg(test)]
    pub(crate) fn assert_text_projection(&self, cx: &mut Context<Self>) {
        let selection = self.text_runs();
        let _ = selection.capture_members(|| {
            for row in self.rows.rows() {
                selection.begin_row();
                self.collect_row_text(row, &selection);
            }
        });
        let logical = self.selection_source.registered(self.input.thread);
        let _ = selection.capture_members(|| {
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
            preview: &self.input.preview,
            grid: self.grid(),
            scope: &self.input.namespace,
            narration: false,
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
        if row.kind() == RowKind::Working {
            return match (&self.input.working, window) {
                (Some(working), Some(window)) => (working.render)(window, cx),
                _ => div().into_any_element(),
            };
        }
        if row.kind() == RowKind::Tail {
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
        let blocks = row.blocks();
        if let Some(source) = row.source() {
            let first = blocks
                .first()
                .expect("markdown row has a block")
                .markdown_run
                .unwrap_or(blocks[0].id);
            let grid = self.grid();
            // Prose on the column's edge. Narration — more work follows it
            // in its turn — reads with the work, a step down and muted; the
            // turn's answer reads bright.
            let narration = row.kind() == RowKind::Answer { narration: true };
            // A turn's answer carries its actions under it (narration is
            // the work's, and copies with it) — once it is written: the
            // answer still streaming above the working row has none, so
            // the working line stands right under its words.
            let streaming = self.input.working.is_some()
                && self
                    .rows
                    .rows()
                    .iter()
                    .rev()
                    .find(|row| !matches!(row.kind(), RowKind::Working | RowKind::Tail))
                    .is_some_and(|last| last.id() == row.id());
            let actions = (!narration && !streaming)
                .then(|| message_actions().child(self.copy_action(row.id(), source.to_owned())));
            return div()
                .id(SharedString::from(format!(
                    "answer-{}-{first:?}",
                    self.input.namespace
                )))
                .flex()
                .items_start()
                .min_w_0()
                .w_full()
                .flex_shrink_0()
                .when(narration, |row| row.text_size(px(grid.activity().size)))
                .when(actions.is_some(), |row| row.flex_col().group(MESSAGE_GROUP))
                .child(
                    // The prose alone, its actions under it apart.
                    div()
                        .debug_selector(|| "transcript-answer".into())
                        .w_full()
                        .flex_1()
                        .min_w_0()
                        .child(
                            selection
                                .answer(first, self.linked(source))
                                .when(narration, |answer| answer.quiet()),
                        ),
                )
                .children(actions.map(|actions| actions.mt(px(theme::MESSAGE_ACTIONS_GAP))))
                .into_any_element();
        }
        let Some(block) = blocks.first() else {
            return div().into_any_element();
        };
        if matches!(&block.body, Body::Thinking(text) if text.trim().is_empty()) {
            return div().into_any_element();
        }
        if let Some(visual) = crate::visual::visual_of(block) {
            return self.visuals.row(
                block.id,
                visual,
                self.content_width(),
                self.input.reading_size,
                selection,
                cx,
            );
        }
        let row_cx = pane::RowCx {
            narration: row.kind() == RowKind::Answer { narration: true },
            ..self.row_cx(selection)
        };
        if row.kind() == RowKind::Group {
            return self.render_group(row, &row_cx, view);
        }
        if let Body::Prompt(line) = &block.body {
            // Its send time and `Copy`, left of the bubble.
            let actions = message_actions()
                .children(block.sent_at.clone().map(|at| {
                    div()
                        .debug_selector(|| "prompt-sent-at".into())
                        .px(px(theme::SPACE_1))
                        .text_size(px(theme::FS_META))
                        .line_height(px(theme::LH_META))
                        .text_color(gpui::rgb(theme::INK_SUBTLE))
                        .child(SharedString::from(at))
                }))
                // Its words, as the bubble shows them: the attachments'
                // trailer is chips, never copied text.
                .child(
                    self.copy_action(row.id(), ferrite_core::prompt_files::split(line.clone()).0),
                );
            return div()
                .group(MESSAGE_GROUP)
                .w_full()
                .min_w_0()
                .child(pane::prompt_row(
                    block,
                    line,
                    &row_cx,
                    Some(actions.into_any_element()),
                ))
                .into_any_element();
        }
        match &block.body {
            Body::Tool(tool) => self.render_call(block.id, tool, &row_cx, view.as_ref()),
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

    /// A row's width (the list's, less the row insets), once laid out.
    fn content_width(&self) -> Option<f32> {
        let width = self.list_w.get();
        (width > 0.).then_some(theme::tx_column(width, self.input.solo))
    }

    /// The transcript's grid: the reading size in Solo, a half step under
    /// it in a Group Pane (`theme::tx_size`).
    fn grid(&self) -> Grid {
        Grid::pane(self.input.reading_size, self.input.solo)
    }

    /// A visual's page changed height: the list re-measures its row (off
    /// screen too, so it never changes height under the reader).
    fn visual_height_changed(&mut self, id: &str, cx: &mut Context<Self>) {
        let index = self.rows.rows().iter().position(|row| {
            row.blocks()
                .first()
                .and_then(crate::visual::visual_of)
                .is_some_and(|visual| visual.id == id)
        });
        if let Some(index) = index {
            self.scroll.list_state().remeasure_items(index..index + 1);
            cx.notify();
        }
    }

    /// One call as its row draws it, alone or in a group: its disclosure
    /// (the whole line the pointer's toggle, its `›`, the keyboard's
    /// target, its folds) when it has details to open.
    fn render_call(
        &self,
        block: BlockId,
        tool: &ferrite_core::transcript::ToolBlock,
        row_cx: &pane::RowCx,
        view: Option<&Entity<Self>>,
    ) -> AnyElement {
        let call = DisclosureId::Tool(tool.call.clone());
        let expanded = self.tool_state(&call) == DisclosureState::Expanded;
        let disclosure = view
            .filter(|_| pane::tool_has_details(tool))
            .map(|view| self.disclosure_control(&call, true, view.clone()));
        pane::render_tool(
            block,
            tool,
            row_cx,
            pane::ToolCx {
                expanded,
                disclosure,
            },
        )
    }

    /// The disclosure naming a group row: its first call's.
    fn group_id(row: &TranscriptRow) -> Option<DisclosureId> {
        row.blocks().first().and_then(|block| match &block.body {
            Body::Tool(tool) => Some(DisclosureId::Group(tool.call.clone())),
            _ => None,
        })
    }

    fn group_expanded(&self, row: &TranscriptRow) -> bool {
        Self::group_id(row).is_some_and(|id| self.tool_state(id) == DisclosureState::Expanded)
    }

    /// A group row (`pane::render_tool_group`): its summary is the
    /// disclosure — the whole line under the pointer, its chevron always
    /// shown — and its calls, open, are each the row they would be alone.
    fn render_group(
        &self,
        row: &TranscriptRow,
        row_cx: &pane::RowCx,
        view: Option<Entity<Self>>,
    ) -> AnyElement {
        let Some(id) = Self::group_id(row) else {
            return div().into_any_element();
        };
        let expanded = self.group_expanded(row);
        let disclosure = view
            .clone()
            .map(|view| self.disclosure_control(&id, true, view));
        let members = move || {
            let drawn: Vec<&ferrite_core::transcript::Block> = if expanded {
                row.blocks().iter().collect()
            } else {
                pane::group_live(row.blocks()).into_iter().collect()
            };
            drawn
                .into_iter()
                .filter_map(|block| match &block.body {
                    Body::Tool(tool) => {
                        Some(self.render_call(block.id, tool, row_cx, view.as_ref()))
                    }
                    _ => None,
                })
                .collect()
        };
        pane::render_tool_group(
            row.blocks(),
            row_cx,
            pane::GroupCx {
                disclosure,
                members: Box::new(members),
            },
        )
    }

    /// Where `call`'s disclosure control was last laid out.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn disclosure_bounds(
        &self,
        call: &DisclosureId,
    ) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.disclosure_bounds.borrow().get(call).copied()
    }

    /// Every disclosure the rows draw, in order: the keyboard's stops. A
    /// call with details, a long thought, a group — and, while a group is
    /// open, its calls; while shut, the call it shows working.
    pub fn disclosures(&self) -> Vec<DisclosureId> {
        let mut stops = Vec::new();
        let call = |stops: &mut Vec<DisclosureId>, block: &ferrite_core::transcript::Block| {
            if let Body::Tool(tool) = &block.body {
                if pane::tool_has_details(tool) {
                    stops.push(DisclosureId::Tool(tool.call.clone()));
                }
            }
        };
        for row in self.rows.rows() {
            match row.kind() {
                RowKind::Group => {
                    let Some(id) = Self::group_id(row) else {
                        continue;
                    };
                    stops.push(id);
                    if self.group_expanded(row) {
                        for block in row.blocks() {
                            call(&mut stops, block);
                        }
                    } else if let Some(block) = pane::group_live(row.blocks()) {
                        call(&mut stops, block);
                    }
                }
                RowKind::Activity { .. } => {
                    if let Some(block) = row.blocks().first() {
                        call(&mut stops, block);
                    }
                }
                RowKind::Reasoning => {
                    if let Some(block) = row.blocks().first() {
                        if matches!(&block.body, Body::Thinking(text) if pane::reasoning_has_details(text))
                        {
                            stops.push(DisclosureId::Reasoning(block.id));
                        }
                    }
                }
                _ => {}
            }
        }
        stops
    }

    /// A reasoning row's disclosure: its header toggles under the pointer,
    /// its chevron turning.
    fn control(
        &self,
        call: &DisclosureId,
        view: Entity<Self>,
        _cx: &mut Context<Self>,
    ) -> pane::Disclosure {
        self.disclosure_control(call, false, view)
    }

    /// A header's disclosure: the whole header toggles under the pointer,
    /// its chevron turning; `shown` keeps the chevron in view (a group's,
    /// whose line is only ever a disclosure) where a reasoning row's shows
    /// under the pointer.
    fn disclosure_control(
        &self,
        call: &DisclosureId,
        shown: bool,
        view: Entity<Self>,
    ) -> pane::Disclosure {
        let call = call.clone();
        let clicked = call.clone();
        let expanded = self.tool_state(&call) == DisclosureState::Expanded;
        let targeted = self.tool_targeted(&call);
        let control = pane::tool_disclosure_control(
            &call,
            expanded,
            targeted,
            &self.input.disclosure_focus,
            self.grid().gutter(),
        )
        // The disclosure overlay fills the rendered header. Keep the handler
        // on it so the chevron, label, and trailing row text share one target.
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            gpui::base::TextSelection::clear(window, cx);
            view.update(cx, |view, cx| {
                view.clear_output_selection(cx);
                cx.emit(TranscriptEvent::ToggleDisclosure(clicked.clone()));
            });
        });
        #[cfg(any(test, feature = "test-support"))]
        let control = {
            let sink = self.disclosure_bounds.clone();
            let measured = call.clone();
            control.on_children_prepainted(move |bounds, _, _| {
                // The hit box is the overlay's last child; a keyboard ring
                // may precede it.
                if let Some(bounds) = bounds.last() {
                    sink.borrow_mut().insert(measured.clone(), *bounds);
                }
            })
        };
        pane::Disclosure {
            overlay: Some(control.into_any_element()),
            chevron: Some(pane::disclosure_chevron(expanded, targeted || shown)),
            targeted,
            focus: None,
        }
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

    /// A message's `Copy` (`components::copy_button`, the message action's
    /// door): `text` to the clipboard, a check for `COPIED_HOLD_MS` after.
    fn copy_action(&self, row: &RowId, text: String) -> AnyElement {
        let id = gpui::ElementId::from(SharedString::from(format!(
            "message-copy-{}-{row:?}",
            self.input.namespace
        )));
        // The hover blend draws through this cached view: ask for its next
        // frame.
        let t = ferrite_ui::motion::hover_t(&ferrite_ui::pointer::hover_key(&id));
        if t > 0. && t < 1. {
            crate::file_links::mark_fading();
        }
        div()
            .debug_selector(|| "message-copy".into())
            .flex_shrink_0()
            .child(ferrite_ui::components::copy_button(id, text))
            .into_any_element()
    }

    /// The pinned prompt: its turn's bubble stuck to the top on a plane in
    /// the card's fill, the rows passing under it into the top fade. A
    /// click glides back to its turn. Wheel gestures scroll the list.
    fn pinned_band(
        &self,
        index: usize,
        shift: gpui::Pixels,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let row = self.rows.get(index)?;
        let block = row.blocks().first()?;
        let Body::Prompt(line) = &block.body else {
            return None;
        };
        let selection = self.text_runs();
        let line_height = self.grid().line;
        let available = f32::from(self.scroll.list_state().viewport_bounds().size.height) / 4.;
        let lines = ((available / line_height) as usize).clamp(1, theme::PINNED_PROMPT_LINES);
        let bubble = pane::pinned_prompt(block, line, &self.row_cx(&selection), lines);
        let measured = self.pinned_h.clone();
        let goal = self.estimated_tops().0.get(index).copied().unwrap_or(0.);
        let weak = cx.entity().downgrade();
        let id = gpui::ElementId::from(SharedString::from(format!(
            "transcript-pinned-{}",
            self.input.namespace
        )));
        let key = ferrite_ui::pointer::hover_key(&id);
        let t = ferrite_ui::motion::hover_t(&key);
        if t > 0. && t < 1. {
            crate::file_links::mark_fading();
        }
        let bubble = bubble.id(id).cursor_pointer().bg(ferrite_ui::motion::mix(
            theme::paint::RAISED.into(),
            theme::paint::SELECTION_HOVER.into(),
            t,
        ));
        let blend = ferrite_ui::motion::hover_listener(key);
        let bubble = bubble
            .on_hover(blend)
            .tooltip(ferrite_ui::menu::tooltip("Back to this prompt"))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                cx.stop_propagation();
                let _ = weak.update(cx, |view, cx| view.glide_to(goal, cx));
            });
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
                            "transcript-pinned-plane-{}",
                            self.input.namespace
                        )))
                        .block_mouse_except_scroll()
                        .cursor_default()
                        .flex()
                        .justify_center()
                        .w_full()
                        .px(px(theme::tx_inset(self.input.solo)))
                        .py(px(theme::PINNED_PROMPT_PAD_Y))
                        .bg(gpui::rgb(theme::CARD))
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .w_full()
                                .max_w(px(theme::TX_COLUMN_MAX_W))
                                .child(bubble),
                        ),
                )
                .child(top_fade())
                .into_any_element(),
        )
    }

    /// How far the rows run on under the transcript's foot, past the
    /// list's own foot room (its padding and runway, which hold no row).
    fn rows_below(&self) -> gpui::Pixels {
        let state = self.scroll.list_state();
        let below = state.max_offset_for_scrollbar().y + state.scroll_px_offset_for_scrollbar().y;
        below - px(theme::body_pad_b(self.input.solo) + self.runway.get().room)
    }

    /// Jump to latest: while the reader is scrolled away from the tail, a
    /// round button over the transcript's foot glides back to it.
    fn jump_to_latest(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.scroll.is_following_tail() || self.glide.is_some_and(|glide| glide.follow) {
            return None;
        }
        // Within a couple of lines of the end there is nothing to jump to.
        if self.rows_below() < px(2. * self.grid().line) {
            return None;
        }
        let id = gpui::ElementId::from(SharedString::from(format!(
            "transcript-latest-{}",
            self.input.namespace
        )));
        let key = ferrite_ui::pointer::hover_key(&id);
        let t = ferrite_ui::motion::hover_t(&key);
        if t > 0. && t < 1. {
            crate::file_links::mark_fading();
        }
        let weak = cx.entity().downgrade();
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(theme::JUMP_LATEST_INSET))
                .flex()
                .justify_center()
                .child(
                    div()
                        .id(id)
                        .debug_selector(|| "jump-to-latest".into())
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(theme::ICON_BUTTON))
                        .rounded_full()
                        .bg(ferrite_ui::motion::mix(
                            theme::paint::RAISED.into(),
                            theme::paint::SELECTION_HOVER.into(),
                            t,
                        ))
                        .shadow(ferrite_ui::components::float_shadow())
                        .cursor_pointer()
                        .on_hover(ferrite_ui::motion::hover_listener(key))
                        .tooltip(ferrite_ui::menu::tooltip("Jump to latest"))
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            let _ = weak.update(cx, |view, cx| view.glide_to_latest(cx));
                        })
                        .child(ferrite_ui::icons::icon(
                            ferrite_ui::icons::ARROW_DOWN,
                            theme::ICON_BUTTON_GLYPH,
                            theme::INK,
                        )),
                )
                .into_any_element(),
        )
    }

    /// Glide to the tail and follow it again; reduced motion jumps.
    fn glide_to_latest(&mut self, cx: &mut Context<Self>) {
        let (tops, total) = self.estimated_tops();
        if ferrite_ui::motion::reduced_motion(cx) {
            self.glide = None;
            self.scroll.scroll_to_bottom();
        } else {
            let view_h = f32::from(self.scroll.list_state().viewport_bounds().size.height);
            self.glide = Some(Glide {
                from: self.scroll_position(&tops, total),
                to: (total - view_h).max(0.),
                started: cx.background_executor().now(),
                follow: true,
            });
        }
        cx.notify();
    }
}

/// The group a message row names: its actions show under the pointer
/// anywhere on it.
const MESSAGE_GROUP: &str = "transcript-message";

/// A message's actions (`MESSAGE_ACTION`s in a row), laid out at rest and
/// shown under the pointer anywhere on the message.
fn message_actions() -> gpui::Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::SPACE_0_5))
        .h(px(theme::MESSAGE_ACTION))
        .opacity(0.)
        .group_hover(MESSAGE_GROUP, |style| style.opacity(1.))
}

/// The top fade (`EDGE_FADE`): the card's fill fading to nothing, over
/// what scrolls under the top of the transcript or its pinned prompt.
fn top_fade() -> gpui::Div {
    ferrite_ui::components::edge_fade(gpui::rgb(theme::CARD).into(), false)
}

/// The foot fade: the same fill rising from the transcript's foot while a
/// row runs on under it (the reader scrolled away from the tail, or the
/// tail grown past it), so a row cut there fades into the Composer rather
/// than ending on a hard cut.
fn bottom_fade() -> gpui::Div {
    ferrite_ui::components::edge_fade(gpui::rgb(theme::CARD).into(), true)
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
            Self::Prompt => theme::INK_STRONG,
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
            RowKind::Working => return None,
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
            3.0 * self.grid().line
        } else {
            known.iter().sum::<f32>() / known.len() as f32
        };
        let mut tops = Vec::with_capacity(rows.len());
        let mut at = 0.0;
        for row in rows {
            tops.push(at);
            at += heights.get(row.id()).copied().unwrap_or(fallback);
        }
        (tops, at + theme::body_pad_b(self.input.solo))
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
        if ferrite_ui::motion::reduced_motion(cx) {
            self.glide = None;
            self.jump_to(goal, &tops);
        } else {
            self.glide = Some(Glide {
                from: self.scroll_position(&tops, total),
                to: goal,
                started: cx.background_executor().now(),
                follow: false,
            });
        }
        cx.notify();
    }

    /// One frame of a glide in flight.
    fn advance_glide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(glide) = self.glide else {
            return;
        };
        let spec =
            ferrite_ui::motion::MotionSpec::new(theme::MINIMAP_SCROLL_MS, ferrite_ui::motion::EASE);
        let elapsed = cx
            .background_executor()
            .now()
            .saturating_duration_since(glide.started);
        let (tops, _) = self.estimated_tops();
        if elapsed >= spec.duration() {
            self.glide = None;
            if glide.follow {
                self.scroll.scroll_to_bottom();
            } else {
                self.jump_to(glide.to, &tops);
            }
            return;
        }
        let at = ferrite_ui::motion::lerp(glide.from, glide.to, spec.progress_at(elapsed));
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
        let band_t = ferrite_ui::motion::hover_t(&band_key);
        let band_ink = ferrite_ui::motion::mix(
            gpui::rgba(theme::MINIMAP_BAND).into(),
            gpui::rgba(theme::MINIMAP_BAND_HOVER).into(),
            band_t,
        );
        if band_t > 0. && band_t < 1. {
            crate::file_links::mark_fading();
        }
        let blend = ferrite_ui::motion::hover_listener(band_key);
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
        // The working row's spinner and shimmer are loops: the overlay
        // draws them, so their steps rebuild nothing of this transcript.
        ferrite_ui::loops_overlay::begin(window, cx.entity_id());
        self.document.begin_viewport_update(cx);
        crate::file_links::begin_layout(&self.input.namespace, cx);
        // The answers' file links are this transcript's path targets.
        self.rich.set_path_scope(self.input.namespace.clone());
        self.advance_glide(window, cx);
        self.settle_found(window);
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
        let grid = self.grid();
        let solo = self.input.solo;
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
                // the transcript column's inset (`tx_inset`), centres the
                // reading column (`TX_COLUMN_MAX_W` at most), and holds the
                // row's own gap above it (the first row's is the body's top
                // padding: the list's own top padding flickers mid-scroll).
                let gap = row.gap();
                let wrapper = div().w_full().pt(px(gap)).px(px(theme::tx_inset(solo)));
                // A row appended live rises into place; its gap does not
                // move, so the rows above it hold still.
                let wrapper = match arrival {
                    Some(t) => {
                        window.request_animation_frame();
                        ferrite_ui::motion::fade_in_at(wrapper, t)
                    }
                    None => wrapper,
                };
                wrapper
                    .child(
                        div()
                            .w_full()
                            .max_w(px(theme::TX_COLUMN_MAX_W))
                            .mx_auto()
                            .child(element),
                    )
                    .into_any_element()
            },
        )
        // The bottom padding is the list's own: it counts in the scroll
        // extent and the tail follow. A band jump's room rides it.
        .pb(px(
            theme::body_pad_b(self.input.solo) + self.runway.get().room
        ))
        .size_full()
        .min_h_0();
        let scroll = self.scroll.clone();
        let gaps = self.rows.clone();
        let heights = self.heights.clone();
        let weak = cx.entity().downgrade();
        let pinned_drawn = self.pinned_drawn.clone();
        let pinned_asked = self.pinned_asked.clone();
        let pinned_h = self.pinned_h.clone();
        let runway = self.runway.clone();
        let shelf = self.visuals.clone();
        let list_w = self.list_w.clone();
        let reading = self.input.reading_size;
        let inset = theme::tx_inset(solo);
        let pad_b = theme::body_pad_b(solo);
        let check_reasoning = self.input.working_reasoning.is_some();
        let list = div()
            // The wheel takes the scroll back from a minimap glide in
            // flight, rather than the glide overriding it to its end.
            .on_scroll_wheel(cx.listener(|view, _: &gpui::ScrollWheelEvent, _, _| {
                view.glide = None;
            }))
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
                        let room =
                            measured.map_or(0.0, |below| (into + view_h - below - pad_b).max(0.0));
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
                // A pinned band covers the list's top down to its foot; the
                // rows pass under it into its fade.
                let now = pinned_of(&gaps, scroll.list_state(), pinned_h.get());
                let settled = scroll.set_top_mask(now.map(|(_, shift)| pinned_h.get() + shift));
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
                // The visuals near the screen stay live (or open before
                // they scroll in); the far ones may go. After the event: a
                // view's open can wake the engine synchronously.
                if !gaps.visual_rows().is_empty() {
                    let shelf = shelf.clone();
                    let state = scroll.list_state().clone();
                    let measured = heights.clone();
                    let rows = gaps.clone();
                    window.defer(cx, move |window, cx| {
                        let measured = measured.borrow();
                        let height_of = |index: usize| {
                            rows.get(index)
                                .and_then(|row| measured.get(row.id()).copied())
                        };
                        let visuals: Vec<(usize, &crate::visual::Visual)> = rows
                            .visual_rows()
                            .iter()
                            .filter_map(|&index| {
                                let block = rows.get(index)?.blocks().first()?;
                                crate::visual::visual_of(block).map(|visual| (index, visual))
                            })
                            .collect();
                        shelf.reach(&state, &visuals, &height_of, reading, inset, window, cx);
                    });
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
                if (f32::from(width) - list_w.get()).abs() >= 0.5 {
                    // A visual row lays its page out from it: draw again.
                    let had_visuals = !gaps.visual_rows().is_empty();
                    list_w.set(f32::from(width));
                    if had_visuals {
                        let weak = weak.clone();
                        window.defer(cx, move |_, cx| {
                            let _ = weak.update(cx, |_, cx| cx.notify());
                        });
                    }
                }
                // The working line's words depend on whether the live
                // reasoning's own row is in view: check against this layout,
                // while there is a line to choose (a deferred call costs a
                // frame, so an idle transcript asks nothing).
                if check_reasoning {
                    let weak = weak.clone();
                    window.defer(cx, move |_, cx| {
                        let _ = weak.update(cx, |view, cx| view.check_reasoning_shown(cx));
                    });
                }
                if anchored || settled {
                    // Lay this transcript (and the Pane around it) out again
                    // where the scroll settled; nothing else moved.
                    let weak = weak.clone();
                    window.defer(cx, move |_, cx| {
                        let _ = weak.update(cx, |_, cx| cx.notify());
                    });
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
            // One grid for the whole transcript (theme rule 1): Geist at
            // the reading size on its 1.5x line, every row inheriting it;
            // a row laid out on cells sets Geist Mono (`pane::on_cells`).
            .font_family(theme::FONT_UI)
            .text_size(px(grid.size))
            .line_height(px(grid.line))
            .text_color(gpui::rgb(theme::INK))
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
        let pinned = pinned.and_then(|(index, shift)| self.pinned_band(index, shift, cx));
        // Scrolled past the body's top padding: a fade over the padding
        // alone would dim the first row (the list's logical top can name a
        // later row while the rows fit, so read the pixel offset).
        let scrolled = -self.scroll.list_state().scroll_px_offset_for_scrollbar().y
            > px(self.input.shape().top_pad.max(1.));
        // A row runs on under the foot: it fades into the Composer, as
        // Codex's do, rather than ending on a hard cut.
        let cut_below = self.rows_below() > px(0.);
        let latest = self.jump_to_latest(cx);
        let minimap = self.minimap(cx);
        if crate::file_links::take_fading() {
            cx.notify();
        }
        let card = self.hover_card(window, cx);
        let find_bar = self.find_bar();
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
            // What scrolls under the top fades out into the card, under the
            // pinned prompt when there is one.
            .children(
                pinned
                    .is_none()
                    .then_some(scrolled)
                    .filter(|s| *s)
                    .map(|_| {
                        div()
                            .debug_selector(|| "transcript-top-fade".into())
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .child(top_fade())
                    }),
            )
            .children(pinned)
            .children(cut_below.then(|| {
                div()
                    .debug_selector(|| "transcript-bottom-fade".into())
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .child(bottom_fade())
            }))
            .children(latest)
            .children(minimap)
            .children(card)
            .children(find_bar)
            .on_action(cx.listener(|view, _: &crate::find::Next, _, cx| view.find_step(true, cx)))
            .on_action(
                cx.listener(|view, _: &crate::find::Previous, _, cx| view.find_step(false, cx)),
            )
            .on_action(cx.listener(|view, _: &crate::find::Close, _, cx| view.close_find(cx)))
    }
}
