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
    base::ElementExt, div, list, prelude::*, px, relative, App, Context, Entity, EventEmitter,
    FocusHandle, IntoElement, MouseButton, Render, SharedString, Window,
};

use self::{
    rows::{TranscriptRow, TranscriptRows},
    scroll::TranscriptScroll,
};
use crate::{
    attachment_preview::Preview,
    icons,
    pane::{self, DisclosureId, DisclosureState},
    pointer::Pointer,
    rich::TextCache,
    select::{TextRuns, TranscriptText},
    theme,
};

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

    fn display_key(&self) -> (u64, bool, gpui::base::TextSelectionScopeId, Option<Status>) {
        (
            self.display_revision,
            self.focused,
            self.selection_scope,
            self.signal_status,
        )
    }
}

#[derive(Clone, Debug)]
pub(crate) enum TranscriptEvent {
    ToggleDisclosure(DisclosureId),
}

/// A stable GPUI entity for one Pane Subject's heavy transcript subtree.
pub(crate) struct TranscriptView {
    input: TranscriptInput,
    rows: TranscriptRows,
    scroll: TranscriptScroll,
    rich: TextCache,
    selection_source: TranscriptText,
    transcript_focus: FocusHandle,
    document: gpui::base::TextSelectionDocument,
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
                focused: false,
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
        let rows = TranscriptRows::new(&input.blocks, input.turn_diff.as_ref());
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
            scroll,
            rich,
            selection_source,
            transcript_focus: cx.focus_handle(),
            document: gpui::base::TextSelectionDocument::new(scope, cx),
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
        self.input = input;
        self.selection_source = selection_source;
        if content_changed {
            let delta = self
                .rows
                .reconcile(&self.input.blocks, self.input.turn_diff.as_ref());
            self.scroll.reconcile(&delta);
        }
        if content_changed || display_changed {
            if disclosure_changed {
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
    ) -> bool {
        self.input.namespace == namespace
            && self.input.content_revision == content_revision
            && self.input.display_revision == disclosure_revision
            && self.input.focused == focused
            && self.input.signal_status == signal_status
    }
    pub(crate) fn transcript_focus(&self) -> FocusHandle {
        self.transcript_focus.clone()
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
    /// This uses the same row renderer as the viewport, with no control
    /// factory, so copy order cannot diverge from presentation order.
    fn sync_members(&mut self, cx: &mut Context<Self>) {
        let selection = self.text_runs();
        let members = selection.capture_members(|| {
            for row in self.rows.rows() {
                selection.begin_row();
                let _ = self.render_row(row, &selection, None, cx);
            }
        });
        self.document.sync_members(members, cx);
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
            return div()
                .id(SharedString::from(format!(
                    "answer-{}-{first:?}",
                    self.input.namespace
                )))
                .debug_selector(|| "transcript-answer".into())
                .min_w_0()
                .w_full()
                .flex_shrink_0()
                .flex()
                .gap(px(theme::ANSWER_GAP))
                .py(px(theme::ANSWER_PAD_Y))
                .text_size(px(theme::FS_ANSWER))
                .child(
                    // The answer wears Ferrite's mark where Claude Code's
                    // transcript puts its `●`, at rest. The gutter cell keeps
                    // `GUTTER_W` and the mark draws wider out of the flow, so
                    // the overhang eats into the gap instead of moving the
                    // prose; the offset drops it onto the first line's optical
                    // center rather than the row's top.
                    div()
                        .relative()
                        .flex_shrink_0()
                        .w(px(theme::GUTTER_W))
                        .child(
                            div()
                                .absolute()
                                .left(px(0.))
                                .top(px(theme::ANSWER_MARK_TOP))
                                .child(icons::ferrite_icon(theme::ANSWER_MARK)),
                        ),
                )
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
            );
        }
        let Some(block) = blocks.first() else {
            return div().into_any_element();
        };
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
            pane::signal_color(self.input.signal_status),
            None,
            &self.input.preview,
        )
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
        let gutter = div().flex_shrink_0().w(px(theme::GUTTER_W));
        let header = div()
            .id(SharedString::from(format!(
                "turn-diff-row-{}",
                diff.turn_id
            )))
            .relative()
            .flex()
            .items_baseline()
            .min_w_0()
            .gap(px(theme::EVENT_GAP))
            .py(px(theme::EVENT_PAD_Y))
            .text_size(px(theme::FS_MD))
            .line_height(relative(theme::LINE_BODY))
            .text_color(gpui::rgb(theme::TEXT_MUTED))
            .hover(|style| style.text_color(gpui::rgb(theme::TEXT)))
            .active(|style| style.text_color(gpui::rgb(theme::TEXT_STRONG)))
            .child(gutter)
            .child(selection.line(BlockId::TURN_DIFF, "Turn changes", Vec::new()))
            .children(disclosure);
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
                selection,
            ));
            if diff.omitted_bytes > 0 {
                details = details.child(pane::result_line(theme::TEXT_MUTED).child(
                    div().min_w_0().child(format!(
                        "… {} bytes omitted from inline view",
                        diff.omitted_bytes
                    )),
                ));
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
    ) -> gpui::AnyElement {
        let call = call.clone();
        let clicked = call.clone();
        let control = pane::tool_disclosure_control(
            &call,
            self.tool_state(&call) == DisclosureState::Expanded,
            self.tool_targeted(&call),
            &self.input.disclosure_focus,
        )
        // The disclosure overlay fills the rendered header. Keep the handler
        // on it so the arrow, label, and trailing row text share one target.
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            gpui::base::TextSelection::clear(window, cx);
            view.update(cx, |view, cx| {
                view.clear_output_selection(cx);
                cx.emit(TranscriptEvent::ToggleDisclosure(clicked.clone()));
            });
            window.focus(&view.read(cx).tool_focus(), cx);
        });
        #[cfg(test)]
        let control = {
            let sink = self.input.disclosure_bounds.clone();
            let measured = call.clone();
            control.on_children_prepainted(move |bounds, _, _| {
                if let Some(bounds) = bounds.first() {
                    sink.borrow_mut().insert(measured.clone(), *bounds);
                }
            })
        };
        control.into_any_element()
    }
}

impl Render for TranscriptView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.document.begin_viewport_update(cx);
        let rows = self.rows.clone();
        let row_count = rows.len();
        let selection = self.text_runs();
        let view = cx.entity();
        if self.scroll.is_following_tail() {
            self.scroll.scroll_to_bottom();
        }
        let list = list(
            self.scroll.list_state().clone(),
            move |index, _window, cx| {
                let Some(row) = rows.get(index).cloned() else {
                    return div().into_any_element();
                };
                let row = view.update(cx, |view, cx| {
                    selection.begin_row();
                    view.render_row(&row, &selection, Some(cx.entity()), cx)
                });
                // Every row is wrapped, last one included: a list item is
                // laid out as its own root, where a bare row's `w_full`
                // has no parent width to resolve against and shrinks to
                // its text. Only the gap below differs — the last row
                // carries none, so the stack ends on the body padding.
                div()
                    .w_full()
                    .when(index + 1 < row_count, |row| {
                        row.pb(px(theme::BLOCK_GAP))
                    })
                    .child(row)
                    .into_any_element()
            },
        )
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
            .px(px(theme::PANE_PAD_X))
            .pt(px(theme::BODY_PAD_T))
            .pb(px(theme::BODY_PAD_B))
            .text_size(px(theme::FS_MD))
            .line_height(relative(theme::LINE_BODY))
            .text_color(gpui::rgb(theme::TEXT_2))
            .hover_text()
            .track_focus(&self.transcript_focus)
            .child(list)
            .text_selection_scope(if self.input.focused {
                gpui::base::TextSelectionScopeId::default()
            } else {
                self.input.selection_scope
            });
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
