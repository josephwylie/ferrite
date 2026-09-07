use gpui::Corners;
use std::{
    ops::Range,
    rc::Rc,
    sync::{Arc, Mutex, Weak},
};

use gpui::{
    App, BorderStyle, Bounds, ClickEvent, CursorStyle, Edges, Element, ElementId, GlobalElementId,
    Half, HighlightStyle, Hitbox, HitboxBehavior, InspectorElementId, IntoElement, LayoutId,
    MouseButton, MouseClickEvent, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    SharedString, StyledText, TextLayout, Window, point, px, quad,
};

use crate::{
    GlobalState, TextSelection,
    input::Selection,
    text::TextViewMultiClickKind,
    text::node::LinkMark,
    text::selection::word_range_at,
    text::state::LineSpan,
    text::text_view::{LinkClickHandlerFn, handle_link_click},
    text_selection::TextSelectionDocumentRange,
};

/// A inline element used to render a inline text and support selectable.
///
/// All text in TextView (including the CodeBlock) used this for text rendering.
pub(super) struct Inline {
    id: ElementId,
    text: SharedString,
    links: Rc<Vec<(Range<usize>, LinkMark)>>,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    styled_text: StyledText,
    link_click_handler: Option<Arc<LinkClickHandlerFn>>,

    state: Arc<Mutex<InlineState>>,
}

/// The inline text state, used RefCell to keep the selection state.
#[derive(Debug, Default)]
pub(crate) struct InlineState {
    hovered_index: Option<usize>,
    /// The text that actually rendering, matched with selection.
    pub(super) text: SharedString,
    pub(super) selection: Option<Selection>,
    // Wrapped/custom inline fragments retain offsets into the original run.
    pub(super) fragments: Vec<(Range<usize>, Arc<Mutex<InlineState>>)>,
    source: Option<InlineSource>,
}

#[derive(Clone, Debug)]
struct InlineSource {
    root: Weak<Mutex<InlineState>>,
    offset: usize,
}

impl PartialEq for InlineState {
    fn eq(&self, other: &Self) -> bool {
        self.hovered_index == other.hovered_index
            && self.text == other.text
            && self.selected_range() == other.selected_range()
    }
}

impl InlineState {
    pub(super) fn source_metadata(state: &Arc<Mutex<Self>>) -> (Weak<Mutex<Self>>, usize) {
        state
            .lock()
            .ok()
            .and_then(|state| state.source.clone())
            .map(|source| (source.root, source.offset))
            .unwrap_or_else(|| (Arc::downgrade(state), 0))
    }

    pub(super) fn fragment(
        parent: &Arc<Mutex<Self>>,
        range: Range<usize>,
        text: SharedString,
    ) -> Arc<Mutex<Self>> {
        let (root, offset) = Self::source_metadata(parent);
        let child = Arc::new(Mutex::new(Self {
            text,
            source: Some(InlineSource {
                root,
                offset: offset + range.start,
            }),
            ..Default::default()
        }));
        if let Ok(mut parent) = parent.lock() {
            parent.fragments.push((range, child.clone()));
        }
        child
    }

    pub(super) fn selected_range(&self) -> Option<Range<usize>> {
        if self.fragments.is_empty() {
            return self.selection.as_ref().map(|s| s.start..s.end);
        }
        self.fragments
            .iter()
            .filter_map(|(range, state)| {
                let selected = state.lock().ok()?.selected_range()?;
                Some((range.start + selected.start)..(range.start + selected.end))
            })
            .reduce(|a, b| a.start.min(b.start)..a.end.max(b.end))
    }

    pub(super) fn clear_selection(&mut self) {
        self.selection = None;
        for (_, state) in &self.fragments {
            if let Ok(mut state) = state.lock() {
                state.clear_selection();
            }
        }
    }

    /// Save actually rendered text for selected text to use.
    pub(crate) fn set_text(&mut self, text: SharedString) {
        self.text = text;
    }
}

/// Retains an inline's laid-out glyph geometry for selection updates after the
/// inline has left the painted viewport.
#[derive(Clone)]
pub(super) struct InlineSelectionProjection {
    state: Arc<Mutex<InlineState>>,
    text: SharedString,
    geometry: InlineProjectionGeometry,
    bounds: Bounds<Pixels>,
    source_root: usize,
    source_offset: usize,
    document_ordinal: usize,
}

#[derive(Clone)]
enum InlineProjectionGeometry {
    Text {
        text_layout: TextLayout,
        line_height: Pixels,
    },
    Atomic,
}

impl InlineSelectionProjection {
    pub(super) fn new(
        state: Arc<Mutex<InlineState>>,
        text: SharedString,
        text_layout: TextLayout,
        line_height: Pixels,
        bounds: Bounds<Pixels>,
    ) -> Self {
        let (root, source_offset) = InlineState::source_metadata(&state);
        Self {
            state,
            text,
            geometry: InlineProjectionGeometry::Text {
                text_layout,
                line_height,
            },
            bounds,
            source_root: root.as_ptr() as usize,
            source_offset,
            document_ordinal: 0,
        }
    }

    pub(super) fn atomic(state: Arc<Mutex<InlineState>>, bounds: Bounds<Pixels>) -> Self {
        let (root, source_offset) = InlineState::source_metadata(&state);
        let text = state
            .lock()
            .map(|state| state.text.clone())
            .unwrap_or_default();
        Self {
            state,
            text,
            geometry: InlineProjectionGeometry::Atomic,
            bounds,
            source_root: root.as_ptr() as usize,
            source_offset,
            document_ordinal: 0,
        }
    }

    pub(super) fn source_root(&self) -> usize {
        self.source_root
    }

    pub(super) fn set_document_ordinal(&mut self, ordinal: usize) {
        self.document_ordinal = ordinal;
    }

    pub(super) fn document_ordinal(&self) -> usize {
        self.document_ordinal
    }

    pub(super) fn project(&self, anchor: Point<Pixels>, cursor: Point<Pixels>) {
        let selection = match &self.geometry {
            InlineProjectionGeometry::Text {
                text_layout,
                line_height,
            } => selection_for_points(&self.text, text_layout, anchor, cursor, *line_height),
            InlineProjectionGeometry::Atomic => {
                let center = self.bounds.center();
                let after = |point: Point<Pixels>| {
                    self.bounds.top() > point.y
                        || (self.bounds.bottom() > point.y && center.x >= point.x)
                };
                (after(anchor) != after(cursor)).then(|| (0..self.text.len()).into())
            }
        };
        if let Ok(mut state) = self.state.lock() {
            state.selection = selection;
        }
    }

    /// Returns this inline's UTF-8 position for a window point.
    pub(super) fn document_position_at(&self, point: Point<Pixels>) -> Option<usize> {
        self.bounds.contains(&point).then_some(())?;
        let offset = match &self.geometry {
            InlineProjectionGeometry::Text { text_layout, .. } => text_layout
                .index_for_position(point)
                .unwrap_or_else(|offset| offset),
            InlineProjectionGeometry::Atomic => {
                if point.x < self.bounds.center().x {
                    0
                } else {
                    self.text.len()
                }
            }
        };
        Some(self.source_offset + offset.min(self.text.len()))
    }

    /// Applies a logical inline/byte range without depending on the layout
    /// that existed when the endpoints were first hit.
    pub(super) fn project_document_range(&self, ordinal: usize, range: TextSelectionDocumentRange) {
        let selection =
            selection_for_document_range(&self.text, ordinal, self.source_offset, range);
        if let Ok(mut state) = self.state.lock() {
            state.selection = selection;
        }
    }
}

fn selection_for_document_range(
    text: &str,
    ordinal: usize,
    source_offset: usize,
    range: TextSelectionDocumentRange,
) -> Option<Selection> {
    const OFFSET_MASK: u64 = u32::MAX as u64;
    let decode = |key: crate::TextSelectionContentKey| {
        (
            (key.value() >> 32) as usize,
            (key.value() & OFFSET_MASK) as usize,
        )
    };
    let start = range.start().map(decode);
    let end = range.end().map(decode);
    let start_ordinal = start.map(|(ordinal, _)| ordinal).unwrap_or(0);
    let end_ordinal = end.map(|(ordinal, _)| ordinal).unwrap_or(usize::MAX);
    if !(start_ordinal..=end_ordinal).contains(&ordinal) {
        return None;
    }
    let start = start
        .filter(|(position_ordinal, _)| *position_ordinal == ordinal)
        .map(|(_, offset)| offset.saturating_sub(source_offset))
        .unwrap_or(0);
    let end = end
        .filter(|(position_ordinal, _)| *position_ordinal == ordinal)
        .map(|(_, offset)| offset.saturating_sub(source_offset))
        .unwrap_or(text.len());
    let start = utf8_boundary_before(text, start.min(text.len()));
    let end = utf8_boundary_before(text, end.min(text.len()));
    (start < end).then(|| (start..end).into())
}

fn utf8_boundary_before(text: &str, mut offset: usize) -> usize {
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

impl Inline {
    pub(super) fn new(
        id: impl Into<ElementId>,
        state: Arc<Mutex<InlineState>>,
        links: Vec<(Range<usize>, LinkMark)>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
        link_click_handler: Option<Arc<LinkClickHandlerFn>>,
    ) -> Self {
        let text = state
            .lock()
            .map(|state| state.text.clone())
            .unwrap_or_default();

        Self {
            id: id.into(),
            links: Rc::new(links),
            highlights,
            text: text.clone(),
            styled_text: StyledText::new(text),
            link_click_handler,
            state,
        }
    }

    /// Get link at given mouse position.
    fn link_for_position(
        layout: &TextLayout,
        links: &Vec<(Range<usize>, LinkMark)>,
        position: Point<Pixels>,
    ) -> Option<LinkMark> {
        let offset = layout.index_for_position(position).ok()?;
        for (range, link) in links.iter() {
            if range.contains(&offset) {
                return Some(link.clone());
            }
        }

        None
    }

    /// Paint selected bounds for debug.
    #[allow(unused)]
    fn paint_selected_bounds(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        window.paint_quad(gpui::PaintQuad {
            bounds,
            background: gpui::hsla(0.58, 0.85, 0.62, 0.01).into(),
            corner_radii: Corners::default(),
            border_color: gpui::transparent_black(),
            border_style: BorderStyle::default(),
            border_widths: gpui::Edges::all(px(0.)),
        });
    }

    fn layout_selections(
        &self,
        text_layout: &TextLayout,
        bounds: &Bounds<Pixels>,
        document_range: Option<TextSelectionDocumentRange>,
        ordinal: usize,
        source_offset: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> (bool, bool, Option<Selection>) {
        let Some(text_view_state) = GlobalState::global(cx).text_view_state() else {
            return (false, false, None);
        };

        let text_view_state = text_view_state.read(cx);
        let is_selectable = text_view_state.is_selectable();
        if !is_selectable {
            return (false, false, None);
        }

        if text_view_state.is_all_selected() {
            return (is_selectable, true, Some((0..self.text.len()).into()));
        }

        if let Some(selection) = text_view_state.multi_click_selection() {
            return (
                is_selectable,
                true,
                selection_for_multi_click(
                    &self.text,
                    text_layout,
                    *bounds,
                    selection.pos,
                    selection.kind,
                )
                .map(Selection::from),
            );
        }

        if let Some(document_range) = document_range {
            return (
                is_selectable,
                true,
                selection_for_document_range(&self.text, ordinal, source_offset, document_range),
            );
        }

        let Some((selection_start, selection_end)) = text_view_state.selection_points(cx) else {
            return (is_selectable, false, None);
        };
        let line_height = window.line_height();

        (
            true,
            true,
            selection_for_points(
                &self.text,
                text_layout,
                selection_start,
                selection_end,
                line_height,
            ),
        )
    }

    fn text_line_bounds(
        &self,
        text_layout: &TextLayout,
        line_height: Pixels,
        mask_bounds: Bounds<Pixels>,
    ) -> Vec<Bounds<Pixels>> {
        // Ferrite: only visible text can start a selection. Avoid walking every
        // glyph in clipped paragraphs; selection/copy still use layout_selections.
        let visible = text_layout.bounds().intersect(&mask_bounds);
        if visible.size.width <= px(0.) || visible.size.height <= px(0.) {
            return Vec::new();
        }

        let mut line_bounds = Vec::new();
        let mut current_line_y = None;
        let mut current_bounds: Option<Bounds<Pixels>> = None;
        let mut offset = 0;

        for c in self.text.chars() {
            let next_offset = offset + c.len_utf8();
            let Some(pos) = text_layout.position_for_index(offset) else {
                offset = next_offset;
                continue;
            };

            let mut char_width = line_height.half();
            if let Some(next_pos) = text_layout.position_for_index(next_offset) {
                if next_pos.y == pos.y {
                    char_width = next_pos.x - pos.x;
                }
            }

            let bounds = Bounds::from_corners(pos, point(pos.x + char_width, pos.y + line_height))
                .intersect(&mask_bounds);
            if bounds.size.width > px(0.) && bounds.size.height > px(0.) {
                if current_line_y == Some(pos.y) {
                    if let Some(current) = current_bounds.as_mut() {
                        *current = current.union(&bounds);
                    }
                } else {
                    if let Some(current) = current_bounds.take() {
                        line_bounds.push(current);
                    }
                    current_line_y = Some(pos.y);
                    current_bounds = Some(bounds);
                }
            }

            offset = next_offset;
        }

        if let Some(current) = current_bounds {
            line_bounds.push(current);
        }

        line_bounds
    }

    /// Paint the selection background.
    fn paint_selection(
        selection: &Selection,
        text_layout: &TextLayout,
        bounds: &Bounds<Pixels>,
        window: &mut Window,
        color: gpui::Hsla,
    ) {
        let mut start = selection.start;
        let mut end = selection.end;
        if end < start {
            std::mem::swap(&mut start, &mut end);
        }
        let Some(start_position) = text_layout.position_for_index(start) else {
            return;
        };
        let Some(end_position) = text_layout.position_for_index(end) else {
            return;
        };

        let line_height = text_layout.line_height();
        if start_position.y == end_position.y {
            window.paint_quad(quad(
                Bounds::from_corners(
                    start_position,
                    point(end_position.x, end_position.y + line_height),
                ),
                px(0.),
                color,
                Edges::default(),
                gpui::transparent_black(),
                BorderStyle::default(),
            ));
        } else {
            window.paint_quad(quad(
                Bounds::from_corners(
                    start_position,
                    point(bounds.right(), start_position.y + line_height),
                ),
                px(0.),
                color,
                Edges::default(),
                gpui::transparent_black(),
                BorderStyle::default(),
            ));

            if end_position.y > start_position.y + line_height {
                window.paint_quad(quad(
                    Bounds::from_corners(
                        point(bounds.left(), start_position.y + line_height),
                        point(bounds.right(), end_position.y),
                    ),
                    px(0.),
                    color,
                    Edges::default(),
                    gpui::transparent_black(),
                    BorderStyle::default(),
                ));
            }

            window.paint_quad(quad(
                Bounds::from_corners(
                    point(bounds.left(), end_position.y),
                    point(end_position.x, end_position.y + line_height),
                ),
                px(0.),
                color,
                Edges::default(),
                gpui::transparent_black(),
                BorderStyle::default(),
            ));
        }
    }
}

impl IntoElement for Inline {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Inline {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_element_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let text_style = window.text_style();

        let mut runs = Vec::new();
        let mut ix = 0;
        for (range, highlight) in self.highlights.iter() {
            if ix < range.start {
                runs.push(text_style.clone().to_run(range.start - ix));
            }
            runs.push(text_style.clone().highlight(*highlight).to_run(range.len()));
            ix = range.end;
        }
        if ix < self.text.len() {
            runs.push(text_style.to_run(self.text.len() - ix));
        }

        self.styled_text = StyledText::new(self.text.clone()).with_runs(runs);
        let (layout_id, _) =
            self.styled_text
                .request_layout(global_element_id, inspector_id, window, cx);

        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.styled_text
            .prepaint(id, inspector_id, bounds, &mut (), window, cx);

        // Report this element's laid-out extent so an ancestor TextView with
        // `max_lines` can snap its clip to a whole-line boundary. The state
        // stack only holds an entry during prepaint when that view set
        // `max_lines`, so this is a no-op otherwise.
        if let Some(text_view_state) = GlobalState::global(cx).text_view_state().cloned() {
            let state = text_view_state.read(cx);
            if state.max_lines.is_some()
                && let Ok(mut line_spans) = state.line_spans.lock()
            {
                line_spans.push(LineSpan {
                    top: bounds.top(),
                    bottom: bounds.bottom(),
                    line_height: window.line_height(),
                });
            }
        }

        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        hitbox
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let current_view = window.current_view();
        let hitbox = prepaint;
        let (source_root, source_offset) = InlineState::source_metadata(&self.state);
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        let text_layout = self.styled_text.layout().clone();
        self.styled_text
            .paint(global_id, None, bounds, &mut (), &mut (), window, cx);

        let (document_range, ordinal, source_offset) = GlobalState::global(cx)
            .text_view_state()
            .map(|state| {
                let state = state.read(cx);
                (
                    state.selection_adapter.document_range(cx),
                    state
                        .selection_adapter
                        .document_ordinal_for_source(source_root.as_ptr() as usize),
                    source_offset,
                )
            })
            .unwrap_or((None, 0, 0));

        // layout selections
        let (is_selectable, is_selection, selection) = self.layout_selections(
            &text_layout,
            &bounds,
            document_range,
            ordinal,
            source_offset,
            window,
            cx,
        );

        state.selection = selection;

        if is_selection || is_selectable {
            window.set_cursor_style(CursorStyle::IBeam, &hitbox);
        }

        // link cursor pointer
        let mouse_position = window.mouse_position();
        if let Some(_) = Self::link_for_position(&text_layout, &self.links, mouse_position) {
            window.set_cursor_style(CursorStyle::PointingHand, &hitbox);
        }

        if let Some(selection) = &state.selection {
            let color = GlobalState::global(cx)
                .text_view_state()
                .map(|state| state.read(cx).text_view_style.selection())
                .unwrap_or_else(|| crate::Theme::global(cx).tokens.colors.selection);
            Self::paint_selection(selection, &text_layout, &bounds, window, color);
        }

        // Projection construction reads the same state for its logical source
        // metadata, so release the paint-time guard first.
        let hovered_index = state.hovered_index;
        drop(state);

        if is_selectable {
            if let Some(text_view_state) = GlobalState::global(cx).text_view_state().cloned() {
                let text_bounds = self.text_line_bounds(
                    &text_layout,
                    text_layout.line_height(),
                    window.content_mask().bounds,
                );
                let projection = InlineSelectionProjection::new(
                    self.state.clone(),
                    self.text.clone(),
                    text_layout.clone(),
                    window.line_height(),
                    bounds,
                );
                text_view_state.update(cx, |state, _| {
                    state.selection_adapter.register_inline(text_bounds);
                    state.selection_adapter.register_projection(projection);
                });
            }

            window.on_mouse_event({
                let hitbox = hitbox.clone();
                let text_layout = text_layout.clone();
                let inline_state = self.state.clone();
                let text = self.text.clone();
                let text_view_state = GlobalState::global(cx).text_view_state().cloned();
                move |event: &MouseDownEvent, phase, window, cx| {
                    if !phase.bubble()
                        || !hitbox.is_hovered(window)
                        || event.button != MouseButton::Left
                    {
                        return;
                    }

                    let kind = match event.click_count {
                        2 => TextViewMultiClickKind::Word,
                        3 => TextViewMultiClickKind::Paragraph,
                        _ => return,
                    };

                    let Some(range) = selection_for_multi_click(
                        &text,
                        &text_layout,
                        hitbox.bounds,
                        event.position,
                        kind,
                    ) else {
                        return;
                    };

                    let selected_text = text[range.clone()].to_string();

                    // This renderer owns multi-click selection. Prevent the
                    // window selection layer from handling the same press.
                    GlobalState::suppress_text_selection(cx);

                    if let Ok(mut inline_state) = inline_state.lock() {
                        inline_state.selection = Some(range.into());
                    }
                    if let Some(text_view_state) = &text_view_state {
                        text_view_state.update(cx, |state, cx| {
                            state.set_multi_click_selection(
                                event.position,
                                kind,
                                selected_text,
                                cx,
                            );
                        });
                    }
                    cx.notify(current_view);
                }
            });
        }

        // mouse move, update hovered link
        window.on_mouse_event({
            let hitbox = hitbox.clone();
            let text_layout = text_layout.clone();
            let mut hovered_index = hovered_index;
            move |event: &MouseMoveEvent, phase, window, cx| {
                if !phase.bubble() || !hitbox.is_hovered(window) {
                    return;
                }

                let current = hovered_index;
                let updated = text_layout.index_for_position(event.position).ok();
                //  notify update when hovering over different links
                if current != updated {
                    hovered_index = updated;
                    cx.notify(current_view);
                }
            }
        });

        if !is_selection {
            // click to open link
            window.on_mouse_event({
                let links = self.links.clone();
                let text_layout = text_layout.clone();
                let hitbox = hitbox.clone();
                let text_view_state = GlobalState::global(cx).text_view_state().cloned();
                let link_click_handler = self.link_click_handler.clone();

                move |event: &MouseUpEvent, phase, window, cx| {
                    if !phase.bubble() || !hitbox.is_hovered(window) {
                        return;
                    }
                    if text_view_state
                        .as_ref()
                        .is_some_and(|state| state.read(cx).has_selection(cx))
                    {
                        return;
                    }

                    if let Some(link) =
                        Self::link_for_position(&text_layout, &links, event.position)
                    {
                        TextSelection::end(window, cx);
                        cx.stop_propagation();
                        let click = ClickEvent::Mouse(MouseClickEvent {
                            down: MouseDownEvent {
                                button: event.button,
                                position: event.position,
                                modifiers: event.modifiers,
                                click_count: event.click_count,
                                first_mouse: false,
                            },
                            up: event.clone(),
                        });
                        handle_link_click(&link_click_handler, link.url, click, window, cx);
                    }
                }
            });
        }
    }
}

/// Computes selection from the complete laid-out glyph band, regardless of
/// which portion of the inline was visible while it was painted.
fn selection_for_points(
    text: &str,
    text_layout: &TextLayout,
    selection_start: Point<Pixels>,
    selection_end: Point<Pixels>,
    line_height: Pixels,
) -> Option<Selection> {
    // Every glyph in a painted element has a valid position even when an
    // ancestor clips it. Copy derives from InlineState.selection, so clipping
    // this walk would drop selected, scrolled-out text.
    let mut selection: Option<Selection> = None;
    let mut offset = 0;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let Some(pos) = text_layout.position_for_index(offset) else {
            offset += c.len_utf8();
            continue;
        };

        let next_offset = offset + c.len_utf8();
        let mut char_width = line_height.half();
        if let Some(next_pos) = text_layout.position_for_index(next_offset) {
            if next_pos.y == pos.y {
                char_width = next_pos.x - pos.x;
            }
        }

        if point_in_text_selection(pos, char_width, selection_start, selection_end, line_height) {
            if selection.is_none() {
                selection = Some((offset..offset).into());
            }

            if let Some(selection) = selection.as_mut() {
                selection.end = next_offset;
            }
        }

        offset = next_offset;
    }

    selection
}

fn selection_for_multi_click(
    text: &str,
    text_layout: &TextLayout,
    bounds: Bounds<Pixels>,
    pos: Point<Pixels>,
    kind: TextViewMultiClickKind,
) -> Option<std::ops::Range<usize>> {
    if !bounds.contains(&pos) {
        return None;
    }

    let offset = text_layout.index_for_position(pos).ok()?;

    match kind {
        TextViewMultiClickKind::Word => word_range_at(text, offset),
        // Known limitation: a paragraph maps to a single Inline run here. When a
        // paragraph embeds an inline image it is split into multiple Inline runs,
        // so triple-click only selects the run on the clicked side of the image.
        TextViewMultiClickKind::Paragraph => (!text.is_empty()).then_some(0..text.len()),
    }
}

/// Check if a `pos` is within a `bounds`, considering multi-line selections.
fn point_in_text_selection(
    pos: Point<Pixels>,
    char_width: Pixels,
    selection_start: Point<Pixels>,
    selection_end: Point<Pixels>,
    line_height: Pixels,
) -> bool {
    let point_in_line = |point: Point<Pixels>| point.y >= pos.y && point.y < pos.y + line_height;
    let top = selection_start.y.min(selection_end.y);
    let bottom = selection_start.y.max(selection_end.y);
    let x = pos.x + char_width.half();

    // Out of the vertical bounds
    if pos.y + line_height <= top || pos.y > bottom {
        return false;
    }

    // Treat the selection as single-line when both drag points fall within the
    // same rendered line, even if their y coordinates differ inside that line.
    if point_in_line(selection_start) && point_in_line(selection_end) {
        let left = selection_start.x.min(selection_end.x);
        let right = selection_start.x.max(selection_end.x);
        return x >= left && x <= right;
    }

    let (top_point, bottom_point) = if selection_start.y < selection_end.y {
        (selection_start, selection_end)
    } else {
        (selection_end, selection_start)
    };
    let is_top_line = point_in_line(top_point);
    let is_bottom_line = point_in_line(bottom_point);

    if is_top_line {
        return x >= top_point.x;
    } else if is_bottom_line {
        return x <= bottom_point.x;
    } else {
        return true;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        InlineSelectionProjection, InlineState, point_in_text_selection,
        selection_for_document_range,
    };
    use crate::{TextSelectionContentKey, text_selection::TextSelectionDocumentRange};
    use gpui::{Bounds, point, px};
    use std::sync::{Arc, Mutex};

    #[test]
    fn document_range_preserves_utf8_bytes_across_reflow() {
        let range = TextSelectionDocumentRange::new(
            Some(TextSelectionContentKey::new(1)),
            Some(TextSelectionContentKey::new(5)),
        );

        // The range is logical byte offsets, so a replacement TextLayout does
        // not affect the selected UTF-8 span.
        let selection = selection_for_document_range("aébc", 0, 0, range).unwrap();
        assert_eq!(selection.start, 1);
        assert_eq!(selection.end, 5);
        assert!(selection_for_document_range("aébc", 1, 0, range).is_none());
    }

    #[test]
    fn document_range_uses_original_source_offsets_for_wrapped_fragments() {
        let range = TextSelectionDocumentRange::new(
            Some(TextSelectionContentKey::new(1)),
            Some(TextSelectionContentKey::new(4)),
        );

        // Reflow splits `aébc` after `é`. Each visual fragment keeps its
        // original byte offset, so both receive their respective selection.
        let first = selection_for_document_range("aé", 0, 0, range).unwrap();
        assert_eq!(first.start, 1);
        assert_eq!(first.end, 3);
        let second = selection_for_document_range("bc", 0, 3, range).unwrap();
        assert_eq!(second.start, 0);
        assert_eq!(second.end, 1);
    }

    #[test]
    fn atomic_projection_uses_its_original_source_offset() {
        let root = Arc::new(Mutex::new(InlineState {
            text: "before link after".into(),
            ..Default::default()
        }));
        let card = InlineState::fragment(&root, 7..11, "link".into());
        let projection = InlineSelectionProjection::atomic(
            card.clone(),
            Bounds::from_corners(point(px(0.), px(0.)), point(px(100.), px(20.))),
        );
        let range = TextSelectionDocumentRange::new(
            Some(TextSelectionContentKey::new(8)),
            Some(TextSelectionContentKey::new(10)),
        );

        projection.project_document_range(0, range);
        assert_eq!(card.lock().unwrap().selected_range(), Some(1..3));
    }

    #[test]
    fn unbounded_document_range_covers_later_source_fragments() {
        let range = TextSelectionDocumentRange::new(None, None);
        let selection = selection_for_document_range("later", 3, 64, range).unwrap();

        assert_eq!(selection.start..selection.end, 0..5);
    }

    #[test]
    fn test_point_in_text_selection() {
        let line_height = px(20.);
        let char_width = px(10.);
        let start = point(px(50.), px(50.));
        let end = point(px(150.), px(150.));

        // First line but haft line height, true
        // | p --------|
        // | selection |
        // |-----------|
        assert!(point_in_text_selection(
            point(px(50.), px(40.)),
            char_width,
            start,
            end,
            line_height
        ));

        // First line in selection, true
        // | p --------|
        // | selection |
        // |-----------|
        assert!(point_in_text_selection(
            point(px(50.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));
        // First line, but left out of selection, false
        // p |-----------|
        //   | selection |
        //   |-----------|
        assert!(!point_in_text_selection(
            point(px(40.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));
        // First line but right out of selection, true
        // |-----------| p
        // | selection |
        // |-----------|
        assert!(point_in_text_selection(
            point(px(160.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));

        // Middle line in selection, true
        // |-----------|
        // |     p     |
        // |-----------|
        assert!(point_in_text_selection(
            point(px(100.), px(70.)),
            char_width,
            start,
            end,
            line_height
        ));
        // Middle line, but left out of selection, true
        //   |-----------|
        // p | selection |
        //   |-----------|
        assert!(point_in_text_selection(
            point(px(40.), px(70.)),
            char_width,
            start,
            end,
            line_height
        ));
        // Middle line, but right out of selection, true
        // |-----------|
        // | selection | p
        // |-----------|
        assert!(point_in_text_selection(
            point(px(160.), px(70.)),
            char_width,
            start,
            end,
            line_height
        ));

        // Last line in selection, true
        // |-----------|
        // | selection |
        // |------- p -|
        assert!(point_in_text_selection(
            point(px(100.), px(140.)),
            char_width,
            start,
            end,
            line_height
        ));
        // Last line, but left out of selection, true
        //
        //   |-----------|
        //   | selection |
        // p |-----------|
        assert!(point_in_text_selection(
            point(px(40.), px(140.)),
            char_width,
            start,
            end,
            line_height
        ));
        // Last line, but right out of selection, false
        // |-----------|
        // | selection |
        // |-----------| p
        assert!(!point_in_text_selection(
            point(px(160.), px(140.)),
            char_width,
            start,
            end,
            line_height
        ));

        // Out of vertical bounds (top), false
        //       p
        // |-----------|
        // | selection |
        // |-----------|
        assert!(!point_in_text_selection(
            point(px(100.), px(20.)),
            char_width,
            start,
            end,
            line_height
        ));
        // Out of vertical bounds (bottom), false
        // |-----------|
        // | selection |
        // |-----------|
        //       p
        assert!(!point_in_text_selection(
            point(px(100.), px(160.)),
            char_width,
            start,
            end,
            line_height
        ));
    }

    #[test]
    fn test_point_in_text_selection_reversed_drag_direction() {
        let line_height = px(20.);
        let char_width = px(10.);

        // Mouse down on lower line then drag upward to x=150.
        // Top line should follow current mouse x, bottom line should keep anchor x.
        let start = point(px(80.), px(150.));
        let end = point(px(150.), px(50.));

        // On top line, selection starts from top cursor x (150), so x=140 should be excluded.
        assert!(!point_in_text_selection(
            point(px(140.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));
        assert!(point_in_text_selection(
            point(px(150.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));

        // On bottom line, selection ends at anchor x (80), so x=90 should be excluded.
        assert!(point_in_text_selection(
            point(px(75.), px(140.)),
            char_width,
            start,
            end,
            line_height
        ));
        assert!(!point_in_text_selection(
            point(px(80.), px(140.)),
            char_width,
            start,
            end,
            line_height
        ));
    }

    #[test]
    fn test_point_in_text_selection_same_visual_line_with_different_y() {
        let line_height = px(20.);
        let char_width = px(10.);
        let start = point(px(100.), px(55.));
        let end = point(px(60.), px(58.));

        assert!(!point_in_text_selection(
            point(px(40.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));
        assert!(point_in_text_selection(
            point(px(70.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));
        assert!(!point_in_text_selection(
            point(px(110.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));
    }

    #[test]
    fn test_point_in_text_selection_same_visual_line_with_reversed_y() {
        let line_height = px(20.);
        let char_width = px(10.);
        let start = point(px(60.), px(58.));
        let end = point(px(100.), px(55.));

        assert!(!point_in_text_selection(
            point(px(40.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));
        assert!(point_in_text_selection(
            point(px(70.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));
        assert!(!point_in_text_selection(
            point(px(110.), px(50.)),
            char_width,
            start,
            end,
            line_height
        ));
    }
}
