//! Retained, virtualized transcript rendering.
//!
//! Cockpit owns roster and cross-pane commands. This module owns the expensive
//! per-Subject row snapshot, native text, selection document and viewport.

mod rows;
mod scroll;

#[cfg(test)]
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use ferrite_core::{
    cockpit::ToolTiming,
    store::Provider,
    transcript::{Block, BlockId, Body, Status, ToolActivity, ToolState, TurnDiff},
    ThreadId,
};
use gpui::{
    base::ElementExt, div, list, prelude::*, px, App, Context, Entity, EventEmitter, FocusHandle,
    IntoElement, MouseButton, Render, SharedString, Window,
};

use self::{
    rows::{RowId, RowKind, TranscriptRow, TranscriptRows},
    scroll::TranscriptScroll,
};
use crate::{
    attachment_preview::Preview,
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
    /// The operator's prompt, or a machine action (a tool row or group).
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
/// first prose after a prompt or after a tool or group row. Consecutive
/// prose wears none; its gutter stays empty and its text keeps the C1 edge.
/// Rows with no speaker are transparent to it.
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

/// Owned input for one selected Subject. Cockpit clones only its retained L1
/// render window when this revision changes; rendering never borrows core.
pub(crate) struct TranscriptInput {
    pub thread: ThreadId,
    pub namespace: SharedString,
    pub content_revision: (u64, u64),
    pub display_revision: u64,
    pub blocks: Vec<Block>,
    /// The turn-wide native change summary, separate from provider tool calls.
    pub turn_diff: Option<TurnDiff>,
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
    #[cfg(test)]
    pub disclosure_bounds: Rc<RefCell<HashMap<DisclosureId, gpui::Bounds<gpui::Pixels>>>>,
}

impl TranscriptInput {
    fn content_key(&self) -> (&SharedString, (u64, u64), Option<&TurnDiff>) {
        (
            &self.namespace,
            self.content_revision,
            self.turn_diff.as_ref(),
        )
    }

    fn display_key(
        &self,
    ) -> (
        u64,
        bool,
        gpui::base::TextSelectionScopeId,
        Option<Status>,
        ferrite_core::settings::ReadingSize,
    ) {
        (
            self.display_revision,
            self.focused,
            self.selection_scope,
            self.signal_status,
            self.reading_size,
        )
    }
}

#[derive(Clone, Debug)]
pub(crate) enum TranscriptEvent {
    ToggleDisclosure(DisclosureId),
    CopyPrompt(String),
    ResendPrompt(String),
}

/// A stable GPUI entity for one Pane Subject's heavy transcript subtree.
pub(crate) struct TranscriptView {
    input: TranscriptInput,
    rows: TranscriptRows,
    /// Rows appended at the tail while the operator watched, and when: they
    /// fade in rising into place (`motion::FADE_IN`). First paint, a history
    /// window growing at its head and a row scrolled back into view never
    /// do.
    arrivals: HashMap<RowId, std::time::Instant>,
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
    pinned_drawn: Rc<Cell<Option<(usize, gpui::Pixels)>>>,
    /// The position a re-render was last asked for.
    pinned_asked: Rc<Cell<Option<Option<(usize, gpui::Pixels)>>>>,
    /// Each row's laid-out height, as the list last measured it: the
    /// minimap places its ticks and its view band from these.
    heights: Rc<std::cell::RefCell<HashMap<RowId, f32>>>,
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
                turn_diff: None,
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
            input.turn_diff.as_ref(),
            theme::answer_text_size(input.reading_size),
        );
        let scroll = TranscriptScroll::new(rows.len());
        scroll.scroll_to_bottom();
        let scope = if input.focused {
            gpui::base::TextSelectionScopeId::default()
        } else {
            input.selection_scope
        };
        let mut view = Self {
            input,
            rows,
            arrivals: HashMap::new(),
            scroll,
            rich,
            selection_source,
            transcript_focus: cx.focus_handle(),
            controls_end: cx.focus_handle(),
            document: gpui::base::TextSelectionDocument::new(scope, cx),
            second_tick: None,
            eased: None,
            wide: Rc::new(Cell::new(false)),
            pinned_h: Rc::new(Cell::new(gpui::px(0.))),
            pinned_drawn: Rc::new(Cell::new(None)),
            pinned_asked: Rc::new(Cell::new(None)),
            heights: Default::default(),
        };
        view.sync_members(cx);
        view
    }

    /// Reconcile only when Cockpit's revision key changes. Ordinary Cockpit
    /// redraws compare that key without cloning rows or notifying this entity.
    pub(crate) fn sync(
        &mut self,
        input: TranscriptInput,
        selection_source: TranscriptText,
        cx: &mut Context<Self>,
    ) {
        let content_changed = self.input.content_key() != input.content_key();
        let display_changed = self.input.display_key() != input.display_key();
        let disclosure_changed = self.input.expanded != input.expanded;
        let reading_changed = self.input.reading_size != input.reading_size;
        self.input = input;
        self.selection_source = selection_source;
        // The gap table is part of the rows: a reading-size change
        // re-projects them, re-spacing every row whose gap scales.
        if content_changed || reading_changed {
            let tail = self.rows.rows().last().map(|row| row.id().clone());
            let delta = self.rows.reconcile(
                &self.input.blocks,
                self.input.turn_diff.as_ref(),
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
            if content_changed || disclosure_changed {
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
    pub(crate) fn matches_key(
        &self,
        namespace: &str,
        content_revision: (u64, u64),
        disclosure_revision: u64,
        focused: bool,
        signal_status: Option<Status>,
        reading_size: ferrite_core::settings::ReadingSize,
    ) -> bool {
        self.input.namespace == namespace
            && self.input.content_revision == content_revision
            && self.input.display_revision == disclosure_revision
            && self.input.focused == focused
            && self.input.signal_status == signal_status
            && self.input.reading_size == reading_size
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
            if crate::rich::code_actions_focused(window) {
                return true;
            }
        }
    }

    pub(crate) fn tool_focus(&self) -> FocusHandle {
        self.input.disclosure_focus.clone()
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

    pub(crate) fn scroll_to_bottom(&self, cx: &mut Context<Self>) {
        self.scroll.scroll_to_bottom();
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

    fn collect_row_text(&self, row: &TranscriptRow, selection: &TextRuns) {
        if let Some(diff) = row.turn_diff() {
            let _ = selection.line(BlockId::TURN_DIFF, "Turn changes", Vec::new());
            if self.tool_state(DisclosureId::TurnDiff(diff.turn_id.clone()))
                == DisclosureState::Expanded
            {
                pane::collect_output_text(BlockId::TURN_DIFF, "turn-diff", &diff.diff, selection);
            }
        } else if let Some(source) = row.source() {
            let block = &row.blocks()[0];
            let _ = selection.answer(block.markdown_run.unwrap_or(block.id), source.to_owned());
        } else if let Some(activity) = ToolActivity::at_start(row.blocks()) {
            let expanded = self.tool_state(DisclosureId::Group(activity.leader().call.clone()))
                == DisclosureState::Expanded;
            pane::collect_activity_text(
                activity,
                expanded,
                |call| self.tool_state(call),
                self.wide.get(),
                selection,
            );
        } else if let Some(block) = row.blocks().first() {
            let expanded = match &block.body {
                Body::Tool(tool) => self.tool_state(DisclosureId::Tool(tool.call.clone())),
                Body::Thinking(_) => self.tool_state(DisclosureId::Reasoning(block.id)),
                _ => DisclosureState::Collapsed,
            } == DisclosureState::Expanded;
            let signal = if row.live_notice() {
                pane::signal_color(self.input.signal_status)
            } else {
                theme::TEXT_MUTED
            };
            pane::collect_block_text(block, expanded, signal, self.wide.get(), selection);
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
                let _ = self.render_row(row, &selection, None, cx);
            }
        });
        assert_eq!(
            logical,
            self.selection_source.registered(self.input.thread),
            "logical copy fragments must match rendered row identities and text"
        );
    }

    fn render_row(
        &self,
        row: &TranscriptRow,
        selection: &TextRuns,
        view: Option<Entity<Self>>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if let Some(diff) = row.turn_diff() {
            return self.render_turn_diff(diff, selection, view, cx);
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
                        .child(selection.answer(first, source.to_owned())),
                )
                .into_any_element();
        }
        if let Some(activity) = ToolActivity::at_start(blocks) {
            let group = DisclosureId::Group(activity.leader().call.clone());
            return pane::render_tool_activity_with(
                activity,
                selection,
                Some(&self.input.timings),
                self.tool_state(&group) == DisclosureState::Expanded,
                view.as_ref()
                    .map(|view| self.control(&group, view.clone(), cx)),
                |call| self.tool_state(call),
                |call| {
                    view.as_ref()
                        .map(|view| self.control(call, view.clone(), cx))
                },
                self.input.reading_size,
                self.wide.get(),
            );
        }
        let Some(block) = blocks.first() else {
            return div().into_any_element();
        };
        // A sent prompt's actions blend in under the pointer; this view is
        // cached, so while the blend is mid-flight it renders again next
        // frame.
        if matches!(block.body, Body::Prompt(_)) {
            let shown = crate::motion::hover_t(&pane::prompt_hover_key(block.id));
            if shown > 0. && shown < 1. {
                cx.notify();
            }
        }
        if matches!(&block.body, Body::Thinking(text) if text.trim().is_empty()) {
            return div().into_any_element();
        }
        let call = match &block.body {
            Body::Tool(tool) if pane::tool_has_details(tool) => {
                Some(DisclosureId::Tool(tool.call.clone()))
            }
            Body::Thinking(text) if pane::reasoning_has_details(text) => {
                Some(DisclosureId::Reasoning(block.id))
            }
            _ => None,
        };
        pane::render_block(
            block,
            selection,
            Some(&self.input.timings),
            call.as_ref()
                .is_some_and(|call| self.tool_state(call) == DisclosureState::Expanded),
            call.as_ref().and_then(|call| {
                view.as_ref()
                    .map(|view| self.control(call, view.clone(), cx))
            }),
            if row.live_notice() {
                pane::signal_color(self.input.signal_status)
            } else {
                theme::TEXT_MUTED
            },
            self.input.provider,
            &self.input.preview,
            view.map(|view| self.prompt_actions(block, view)),
            self.input.reading_size,
            self.wide.get(),
        )
    }

    fn prompt_actions(&self, block: &Block, view: Entity<Self>) -> gpui::AnyElement {
        let Body::Prompt(prompt) = &block.body else {
            return div().into_any_element();
        };
        let copy = prompt.clone();
        let resend = prompt.clone();
        let copy_view = view.clone();
        pane::prompt_actions(block.id)
            .on_copy(move |_, _, cx| {
                cx.stop_propagation();
                copy_view.update(cx, |_, cx| {
                    cx.emit(TranscriptEvent::CopyPrompt(copy.clone()))
                });
            })
            .on_resend(move |_, _, cx| {
                cx.stop_propagation();
                view.update(cx, |_, cx| {
                    cx.emit(TranscriptEvent::ResendPrompt(resend.clone()))
                });
            })
            .into_any_element()
    }

    fn render_turn_diff(
        &self,
        diff: &TurnDiff,
        selection: &TextRuns,
        view: Option<Entity<Self>>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let call = DisclosureId::TurnDiff(diff.turn_id.clone());
        let expanded = self.tool_state(&call) == DisclosureState::Expanded;
        let disclosure = view
            .as_ref()
            .map(|view| self.control(&call, view.clone(), cx));
        let targeted = disclosure.as_ref().is_some_and(|parts| parts.targeted);
        let (overlay, chevron) = match disclosure {
            Some(parts) => (Some(parts.overlay), Some(parts.chevron)),
            None => (None, None),
        };
        // The group recipe: a muted `●` line, the chevron trailing it, no
        // hover ground; the keyboard target alone is grounded.
        let grid = Grid::of(self.input.reading_size);
        let header = div()
            .id(SharedString::from(format!(
                "turn-diff-row-{}",
                diff.turn_id
            )))
            .group(pane::DISCLOSURE_ROW)
            .relative()
            .flex()
            .items_start()
            .min_w_0()
            .text_color(gpui::rgb(theme::TEXT_MUTED))
            .when(targeted, |header| {
                header
                    .bg(theme::paint::HOVER)
                    .debug_selector(|| "tool-disclosure-keyboard-target".into())
            })
            .child(pane::glyph_gutter(grid, pane::BULLET, theme::TEXT_MUTED))
            .child(selection.line(BlockId::TURN_DIFF, "Turn changes", Vec::new()))
            .children(chevron)
            .children(overlay);
        let mut card = gpui::component::collapsible::Collapsible::new()
            .w_full()
            .open(expanded)
            .child(header);
        if expanded {
            let mut details = div().flex().flex_col().min_w_0().child(pane::output_block(
                BlockId::TURN_DIFF,
                "turn-diff",
                &diff.diff,
                theme::TEXT_MUTED,
                false,
                true,
                selection,
                grid,
            ));
            if diff.omitted_bytes > 0 {
                details = details.child(pane::omitted_line(diff.omitted_bytes, grid));
            }
            card = card.content(details);
        }
        div()
            .id(SharedString::from(format!("turn-diff-{}", diff.turn_id)))
            .debug_selector(|| "turn-diff".into())
            .flex_shrink_0()
            .w_full()
            .child(card)
            .into_any_element()
    }

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
        let toggle_view = view.downgrade();
        let toggled = call.clone();
        let toggle: pane::DisclosureToggle = Rc::new(move |window, cx| {
            gpui::base::TextSelection::clear(window, cx);
            let _ = toggle_view.update(cx, |view, cx| {
                view.clear_output_selection(cx);
                view.eased = Some((toggled.clone(), !view.input.expanded.contains(&toggled)));
                cx.emit(TranscriptEvent::ToggleDisclosure(toggled.clone()));
            });
        });
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
            overlay: control.into_any_element(),
            chevron: pane::disclosure_chevron(expanded, targeted, eased),
            targeted,
            toggle: Some(toggle),
        }
    }

    /// Arm the one-second clock while a call runs: one notify at the next
    /// whole second of the youngest-rounding live count, so a trail reading
    /// `3s` turns to `4s` on time and no faster.
    fn arm_second_tick(&mut self, cx: &mut Context<Self>) {
        if self.second_tick.is_some() {
            return;
        }
        let next = self
            .input
            .timings
            .values()
            .filter_map(|timing| match timing {
                ToolTiming::Running(started) => {
                    let elapsed = started.elapsed();
                    Some(std::time::Duration::from_secs(elapsed.as_secs() + 1) - elapsed)
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
    /// or turn, a Decision (its record, or the live notice of one waiting),
    /// a passing check.
    pub(crate) fn of(row: &TranscriptRow, signal: u32) -> Option<Self> {
        if row.kind() == RowKind::Prompt {
            return Some(Self::Prompt);
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
                Body::Notice(_) if row.live_notice() && signal == theme::ATTENTION => {
                    return Some(Self::Decision)
                }
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

    /// The pinned prompt band (the prototype's sticky `.prompt`): a plain
    /// echo of the turn's band, laid over the top of the list on the plane
    /// so it hides what scrolls under it. A click scrolls back to the
    /// prompt itself.
    fn pinned_band(
        &self,
        index: usize,
        shift: gpui::Pixels,
        cx: &Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let row = self.rows.get(index)?;
        let Body::Prompt(line) = &row.blocks().first()?.body else {
            return None;
        };
        let grid = Grid::of(self.input.reading_size);
        let (text, files) = ferrite_core::prompt_files::split(line.clone());
        let names = files
            .iter()
            .map(|path| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("  ");
        let state = self.scroll.list_state().clone();
        let gap = row.gap();
        let measured = self.pinned_h.clone();
        let entity = cx.entity().downgrade();
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
                        .debug_selector(|| "transcript-pinned-prompt".into())
                        .occlude()
                        .cursor_pointer()
                        .w_full()
                        // The band over the plane: together they hide the
                        // rows scrolling under it, glass or not.
                        .bg(theme::paint::PLANE)
                        .child(
                            div()
                                .flex()
                                .items_start()
                                .w_full()
                                .min_w_0()
                                .bg(theme::paint::BAND)
                                .py(px(grid.half()))
                                .pl(px(theme::TX_PAD_L))
                                .pr(px(theme::TX_PAD_R))
                                .text_color(gpui::rgb(theme::TEXT_STRONG))
                                .child(pane::mark_gutter(
                                    grid,
                                    crate::icons::icon(
                                        crate::icons::PROMPT,
                                        grid.mark(),
                                        theme::ACCENT,
                                    ),
                                ))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .flex_1()
                                        .min_w_0()
                                        .when(!text.is_empty(), |column| {
                                            column.child(SharedString::from(text))
                                        })
                                        .when(!names.is_empty(), |column| {
                                            column.child(
                                                div()
                                                    .truncate()
                                                    .text_color(gpui::rgb(theme::PATH_INK))
                                                    .child(SharedString::from(names)),
                                            )
                                        }),
                                ),
                        )
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            state.scroll_to(gpui::ListOffset {
                                item_ix: index,
                                offset_in_item: px(gap),
                            });
                            let _ = entity.update(cx, |_, cx| cx.notify());
                        }),
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

    /// The minimap (the WP-A minimap tokens): a tick per prompt, failure,
    /// Decision and pass, the viewport as a translucent band, shown only
    /// while the pointer is on the transcript. A click on a tick jumps to its
    /// row; anywhere else on the rail centres the view on that point.
    fn minimap(&self, group: SharedString, cx: &Context<Self>) -> Option<gpui::AnyElement> {
        let rows = self.rows.rows();
        if rows.is_empty() {
            return None;
        }
        let state = self.scroll.list_state().clone();
        let viewport = state.viewport_bounds();
        let view_h = f32::from(viewport.size.height);
        let (tops, total) = self.estimated_tops();
        if view_h <= 0. || total <= view_h + 1. {
            return None;
        }
        let signal = pane::signal_color(self.input.signal_status);
        let top = state.logical_scroll_top();
        let scrolled = if self.scroll.is_following_tail() {
            total - view_h
        } else {
            tops.get(top.item_ix).copied().unwrap_or(total) + f32::from(top.offset_in_item)
        };
        let band_top = (scrolled / total).clamp(0., 1.);
        let band_h = (view_h / total).clamp(0., 1.);
        let entity = cx.entity().downgrade();
        let tops = Rc::new(tops);
        let ticks = rows.iter().enumerate().filter_map(|(index, row)| {
            let mark = Mark::of(row, signal)?;
            let at = tops[index] / total;
            let state = state.clone();
            let entity = entity.clone();
            let gap = row.gap();
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
                            .bg(gpui::rgb(mark.ink())),
                    )
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        state.scroll_to(gpui::ListOffset {
                            item_ix: index,
                            offset_in_item: px(gap),
                        });
                        let _ = entity.update(cx, |_, cx| cx.notify());
                    }),
            )
        });
        let jump_tops = tops.clone();
        let jump_entity = entity.clone();
        let rail = SharedString::from(format!("transcript-minimap-{}", self.input.namespace));
        Some(
            div()
                .id(rail.clone())
                .group(rail.clone())
                .debug_selector(|| "transcript-minimap".into())
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .w(px(theme::MINIMAP_W))
                .cursor_pointer()
                .opacity(0.)
                .group_hover(group, |style| style.opacity(1.))
                .child(
                    div()
                        .id("minimap-view")
                        .absolute()
                        .left(px(theme::MINIMAP_BAND_INSET))
                        .right(px(theme::MINIMAP_BAND_INSET))
                        .top(gpui::relative(band_top))
                        .h(gpui::relative(band_h))
                        .min_h(px(theme::MINIMAP_BAND_MIN_H))
                        .bg(theme::paint::HOVER)
                        .group_hover(rail, |style| style.bg(theme::paint::SELECTION)),
                )
                .children(ticks)
                .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                    cx.stop_propagation();
                    let viewport = state.viewport_bounds();
                    let height = f32::from(viewport.size.height).max(1.);
                    let fraction =
                        (f32::from(event.position.y - viewport.top()) / height).clamp(0., 1.);
                    let goal = (fraction * total - view_h / 2.).max(0.);
                    let index = jump_tops.iter().rposition(|top| *top <= goal).unwrap_or(0);
                    state.scroll_to(gpui::ListOffset {
                        item_ix: index,
                        offset_in_item: px(goal - jump_tops[index]),
                    });
                    let _ = jump_entity.update(cx, |_, cx| cx.notify());
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.arm_second_tick(cx);
        self.document.begin_viewport_update(cx);
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
                    (
                        view.render_row(&row, &selection, Some(cx.entity()), cx),
                        view.arrival(row.id(), cx),
                    )
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
        // extent and the tail follow, and the working line overlays it.
        .pb(px(theme::BODY_PAD_B))
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
        let list = div()
            .on_children_prepainted(move |_, window, cx| {
                let anchored = scroll.did_layout();
                let settled =
                    scroll.settle_top(|index| gaps.get(index).map_or(0., |row| row.gap()));
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
                let now = pinned_of(&gaps, scroll.list_state(), pinned_h.get());
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
        let group = SharedString::from(format!("transcript-body-{}", self.input.namespace));
        let pinned = pinned.and_then(|(index, shift)| self.pinned_band(index, shift, cx));
        let minimap = self.minimap(group.clone(), cx);
        // A Thread with nothing in it yet shows nothing: the Composer's
        // placeholder says what to do, once (rule 2.11.4).
        div()
            .group(group)
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
            // The first visible row is never cut under the head rule: a
            // cut row's short remnant lies under the plane, so the body
            // reads from its first whole row (`settle_top`).
            .children((self.scroll.top_mask() > px(0.)).then(|| {
                div()
                    .debug_selector(|| "transcript-top-mask".into())
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(self.scroll.top_mask())
                    .bg(theme::paint::PLANE)
            }))
            .children(pinned)
            .children(minimap)
    }
}
