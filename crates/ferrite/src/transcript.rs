//! Retained, virtualized transcript rendering.
//!
//! Cockpit owns roster and cross-pane commands. This module owns the expensive
//! per-Subject row snapshot, native text, selection document and viewport.

mod rows;
mod scroll;

use std::collections::{HashMap, HashSet};
#[cfg(test)]
use std::{cell::RefCell, rc::Rc};

use ferrite_core::{
    cockpit::ToolTiming,
    transcript::{Block, BlockId, Body, Status, ToolActivity, TurnDiff},
    ThreadId,
};
use gpui::{
    base::ElementExt, div, list, prelude::*, px, App, Context, Entity, EventEmitter, FocusHandle,
    IntoElement, MouseButton, Render, SharedString, Window,
};

use self::{
    rows::{RowId, TranscriptRow, TranscriptRows},
    scroll::TranscriptScroll,
};
use crate::{
    attachment_preview::Preview,
    components, icons,
    pane::{self, DisclosureId, DisclosureState},
    pointer::Pointer,
    rich::TextCache,
    select::{TextRuns, TranscriptText},
    theme,
};

/// The answer's gutter mark: the monochrome Ferrite mark in structure ink,
/// a glyph beside the prose, never brighter than a tool's settled dot.
pub(crate) const ANSWER_MARK_INK: u32 = theme::TEXT_FAINT;

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
    pub focused: bool,
    pub reading_size: ferrite_core::settings::SoloReadingSize,
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
        ferrite_core::settings::SoloReadingSize,
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
    /// Turn-level rows appended at the tail while the operator watched, and
    /// when: a new prompt, the first answer block of a turn and a Decision
    /// summary fade in (`motion::ROW_IN`, opacity only). Every other row is
    /// printed on the frame it arrives (rule 2.10.1); first paint, a history
    /// window growing at its head and a row scrolled back into view never
    /// animate.
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
}

impl EventEmitter<TranscriptEvent> for TranscriptView {}

/// Whether the row at `index` is turn-level and enters on `ROW_IN` when it
/// is appended live: a prompt, the first answer block of its turn, or a
/// Decision summary (`asks …` / `… needs approval`). Everything else is
/// printed (rule 2.10.1).
fn enters(rows: &[std::rc::Rc<TranscriptRow>], index: usize) -> bool {
    let row = &rows[index];
    let body = row.blocks().first().map(|block| &block.body);
    match (row.id(), body) {
        (_, Some(Body::Prompt(_))) => true,
        (RowId::Markdown(_), _) => !rows[..index]
            .iter()
            .rev()
            .take_while(|earlier| {
                !matches!(
                    earlier.blocks().first().map(|block| &block.body),
                    Some(Body::Prompt(_))
                )
            })
            .any(|earlier| matches!(earlier.id(), RowId::Markdown(_))),
        (_, Some(Body::Notice(text))) => {
            text.starts_with("asks ") || text.contains(" needs approval")
        }
        _ => false,
    }
}

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

    /// Stamp the turn-level rows that now follow what was the tail: appended
    /// live. Tool rows, output and later paragraphs are printed, not staged.
    fn note_arrivals(&mut self, tail: Option<RowId>, cx: &App) {
        let now = cx.background_executor().now();
        let spell = crate::motion::ROW_IN.duration();
        self.arrivals
            .retain(|_, at| now.saturating_duration_since(*at) < spell);
        let Some(tail) = tail else {
            return;
        };
        let rows = self.rows.rows();
        let Some(at) = rows.iter().rposition(|row| *row.id() == tail) else {
            return;
        };
        for index in at + 1..rows.len() {
            if enters(rows, index) {
                self.arrivals.insert(rows[index].id().clone(), now);
            }
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
        (elapsed < crate::motion::ROW_IN.duration())
            .then(|| crate::motion::ROW_IN.progress_at(elapsed))
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
        reading_size: ferrite_core::settings::SoloReadingSize,
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
            pane::collect_block_text(block, expanded, signal, selection);
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
            let answer_size = theme::answer_text_size(self.input.reading_size);
            let line_height = theme::answer_line_height(self.input.reading_size);
            // The mark centres on the first line box: a leading heading's
            // own, taller box, or the prose line at this reading size.
            let first_line = match &blocks[0].body {
                Body::Heading { level, .. } => {
                    crate::rich::heading_line_height(*level, answer_size)
                }
                _ => line_height,
            };
            let marked = row.answer_mark();
            return div()
                .id(SharedString::from(format!(
                    "answer-{}-{first:?}",
                    self.input.namespace
                )))
                .debug_selector(|| "transcript-answer".into())
                .min_w_0()
                .w_full()
                .flex_shrink_0()
                .relative()
                // A fixed gutter needs no flex sizing. Giving Markdown the
                // remaining block width avoids intrinsic-size passes over the
                // entire growing document before its final wrapped layout.
                .pl(px(theme::GUTTER_W))
                .text_size(px(answer_size))
                .line_height(px(line_height))
                .when(marked, |answer| {
                    // The monochrome Ferrite mark, centred on the first line
                    // box at every reading size, a leading heading included.
                    answer.child(
                        components::gutter(
                            components::glyph_box(icons::icon(
                                icons::FERRITE_MONO,
                                theme::GLYPH_BOX,
                                ANSWER_MARK_INK,
                            ))
                            .debug_selector(|| "answer-mark".into()),
                            first_line,
                        )
                        .absolute()
                        .left_0()
                        .top_0(),
                    )
                })
                .child(selection.answer(first, source.to_owned()))
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
            None,
            &self.input.preview,
            view.map(|view| self.prompt_actions(block, view)),
            self.input.reading_size,
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
        // The group recipe: a muted line at C1, the chevron trailing it, no
        // hover ground; the keyboard target alone is grounded.
        let header = div()
            .id(SharedString::from(format!(
                "turn-diff-row-{}",
                diff.turn_id
            )))
            .group(pane::DISCLOSURE_ROW)
            .relative()
            .flex()
            .items_center()
            .min_w_0()
            .pl(px(theme::GUTTER_W))
            .text_size(px(theme::FS_UI))
            .line_height(px(theme::LH_UI))
            .text_color(gpui::rgb(theme::TEXT_MUTED))
            .when(targeted, |header| {
                header
                    .bg(gpui::rgb(theme::HOVER))
                    .rounded(px(theme::R_CHIP))
                    .debug_selector(|| "tool-disclosure-keyboard-target".into())
            })
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
            ));
            if diff.omitted_bytes > 0 {
                details = details.child(pane::omitted_line(diff.omitted_bytes));
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
        let control =
            pane::tool_disclosure_control(&call, expanded, targeted, &self.input.disclosure_focus)
                // The disclosure overlay fills the rendered header. Keep the handler
                // on it so the chevron, label, and trailing row text share one target.
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    cx.stop_propagation();
                    gpui::base::TextSelection::clear(window, cx);
                    view.update(cx, |view, cx| {
                        view.clear_output_selection(cx);
                        view.eased =
                            Some((clicked.clone(), !view.input.expanded.contains(&clicked)));
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

impl Render for TranscriptView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.arm_second_tick(cx);
        self.document.begin_viewport_update(cx);
        let rows = self.rows.clone();
        let selection = self.text_runs();
        let view = cx.entity();
        if self.scroll.is_following_tail() {
            self.scroll.scroll_to_bottom();
        }
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
                // resolve against and shrinks to its text. The wrapper is
                // also the reading column — gpui lays list items at the
                // list's full width, so the column lives in the row — and
                // carries the row's own gap above it (the first row's is
                // the body's top padding: the list's own top padding
                // flickers mid-scroll).
                let gap = row.gap();
                let wrapper = div().w_full().px(px(theme::PANE_PAD_X));
                // A turn-level row appended live fades in where it lands:
                // opacity only, so nothing above or below it moves.
                let wrapper = match arrival {
                    Some(t) => {
                        window.request_animation_frame();
                        crate::motion::row_in_at(wrapper, t)
                    }
                    None => wrapper,
                };
                wrapper
                    .child(components::reading_column(
                        div().px(px(theme::BOX_INSET_X)).pt(px(gap)).child(element),
                    ))
                    .into_any_element()
            },
        )
        // The bottom padding is the list's own: it counts in the scroll
        // extent and the tail follow, and the working line overlays it.
        .pb(px(theme::BODY_PAD_B))
        .size_full()
        .min_h_0();
        let scroll = self.scroll.clone();
        let list = div()
            .on_children_prepainted(move |_, window, cx| {
                if scroll.did_layout() {
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
            .text_size(px(theme::FS_UI))
            .line_height(px(theme::LH_UI))
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
        // A Thread with nothing in it yet shows nothing: the Composer's
        // placeholder says what to do, once (rule 2.11.4).
        div()
            .relative()
            .flex()
            .flex_col()
            .min_w_0()
            .size_full()
            .min_h_0()
            .child(list)
            .child(crate::components::scrollbar(
                SharedString::from(format!("transcript-scrollbar-{}", self.input.namespace)),
                self.scroll.list_state(),
            ))
    }
}
