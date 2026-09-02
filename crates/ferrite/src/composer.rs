//! The Composer: one shell-style prompt line. This is the window half —
//! focus, key actions, and painting; the editing state lives in `Line`.

use std::ops::Range;

use gpui::prelude::*;
use gpui::{
    actions, div, fill, point, px, relative, rgb, App, Bounds, ClipboardItem, Context,
    DispatchPhase, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, GlobalElementId, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, ShapedLine, SharedString, Style, TextRun,
    UTF16Selection, UnderlineStyle, Window,
};

use crate::line::Line;
use crate::pointer::Pointer;

actions!(
    composer,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Home,
        End,
        Paste,
        // Word-wise editing (alt on macOS, ctrl on Windows) and the line
        // halves (cmd-backspace / cmd-delete).
        DeleteWordLeft,
        DeleteWordRight,
        DeleteToStart,
        DeleteToEnd,
        WordLeft,
        WordRight,
        // Shift-motions grow a selection from the caret.
        SelectLeft,
        SelectRight,
        SelectWordLeft,
        SelectWordRight,
        SelectHome,
        SelectEnd,
        SelectAll,
        // The selection's own clipboard verbs. With nothing selected they
        // propagate, so cmd-c still copies a transcript selection.
        Copy,
        Cut,
        Undo,
        Redo,
    ]
);

/// The line moved — text or cursor. The cockpit listens to keep the `/` and
/// `@` menus following what the operator is typing (#23).
pub struct Edited;

pub struct Composer {
    focus_handle: FocusHandle,
    line: Line,
    /// Mention tokens (`@rel/path`) the operator picked from the `@` menu:
    /// any occurrence still standing in the text paints as the comp's
    /// @-pill, whichever provider serves the Thread. Display only — the
    /// wire reads the text itself (Claude's CLI attaches the file, Codex's
    /// send derives its mention items), so a token edited away simply
    /// stops being one.
    mentions: Vec<SharedString>,
    /// A `/` or `@` popover is open over this line (#23). The cockpit owns
    /// the menu; this flag only widens the line's own key context to
    /// `ComposerMenu` — on the FOCUSED node, where gpui's same-depth
    /// tie-break lets the menu's enter/escape rows beat bare Submit and
    /// Interrupt. An ancestor context would lose that tie.
    menu_open: bool,
    /// The focused idle Thread can recall a delivered prompt. The cockpit
    /// derives this alongside the footer hint; this flag only arms the
    /// focused node's key context.
    history_available: bool,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    /// A press landed in the line and has not been released: moves extend
    /// the selection from where it landed.
    dragging: bool,
}

impl EventEmitter<Edited> for Composer {}

impl Composer {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            line: Line::default(),
            mentions: Vec::new(),
            menu_open: false,
            history_available: false,
            last_layout: None,
            last_bounds: None,
            dragging: false,
        }
    }

    /// Told by the cockpit as its menu opens or closes over this line.
    pub fn set_menu_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.menu_open != open {
            self.menu_open = open;
            cx.notify();
        }
    }

    pub fn set_history_available(&mut self, available: bool, cx: &mut Context<Self>) {
        if self.history_available != available {
            self.history_available = available;
            cx.notify();
        }
    }

    /// Put a line back into the Composer, ready to edit at its end.
    pub fn set(&mut self, text: String, cx: &mut Context<Self>) {
        self.line.set(text);
        self.edited(cx);
    }

    /// Hand the line to the Pane and clear it — pills and all.
    pub fn take(&mut self, cx: &mut Context<Self>) -> String {
        let text = self.line.take();
        self.mentions.clear();
        self.edited(cx);
        text
    }

    /// Whether the line holds no text — what gives Backspace its
    /// `⌫ unqueue` meaning up in the cockpit.
    pub fn is_empty(&self) -> bool {
        self.line.text().is_empty()
    }

    pub fn text(&self) -> &str {
        self.line.text()
    }

    /// The caret, as a byte offset — where the menus read their filter up to.
    pub fn cursor(&self) -> usize {
        self.line.cursor()
    }

    /// Replace `range` with `text`, leaving the caret after it — how a menu
    /// pick lands without sending the caret to the end of the line.
    pub fn splice(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        self.line.replace(Some(range), text);
        self.edited(cx);
    }

    /// Type `text` at the caret — the letter path of a y/n/a key pressed
    /// while the line already holds a sentence.
    pub fn insert(&mut self, text: &str, cx: &mut Context<Self>) {
        self.line.replace(None, text);
        self.edited(cx);
    }

    /// Remember a picked `@` token so its occurrences paint as pills.
    pub fn stage_mention(&mut self, token: SharedString, cx: &mut Context<Self>) {
        if !self.mentions.contains(&token) {
            self.mentions.push(token);
        }
        cx.notify();
    }

    /// The staged pill tokens, as picked — display state only.
    pub fn mentions(&self) -> &[SharedString] {
        &self.mentions
    }

    fn edited(&mut self, cx: &mut Context<Self>) {
        cx.emit(Edited);
        cx.notify();
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        // On an empty line the key means `⌫ unqueue` (PromptBox state 04):
        // it bubbles to the cockpit, which owns the queue.
        if self.line.text().is_empty() {
            cx.propagate();
            return;
        }
        self.line.backspace();
        self.edited(cx);
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        self.line.delete();
        self.edited(cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.line.move_left();
        self.edited(cx);
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.line.move_right();
        self.edited(cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.line.move_home();
        self.edited(cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.line.move_end();
        self.edited(cx);
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.line.replace(None, &text.replace('\n', " "));
            self.edited(cx);
        }
    }

    fn delete_word_left(&mut self, _: &DeleteWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.line.text().is_empty() {
            cx.propagate();
            return;
        }
        self.line.delete_word_left();
        self.edited(cx);
    }

    fn delete_word_right(&mut self, _: &DeleteWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.line.delete_word_right();
        self.edited(cx);
    }

    fn delete_to_start(&mut self, _: &DeleteToStart, _: &mut Window, cx: &mut Context<Self>) {
        if self.line.text().is_empty() {
            cx.propagate();
            return;
        }
        self.line.delete_to_start();
        self.edited(cx);
    }

    fn delete_to_end(&mut self, _: &DeleteToEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.line.delete_to_end();
        self.edited(cx);
    }

    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.line.move_word_left();
        self.edited(cx);
    }

    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.line.move_word_right();
        self.edited(cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.line.select_left();
        self.edited(cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.line.select_right();
        self.edited(cx);
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.line.select_word_left();
        self.edited(cx);
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.line.select_word_right();
        self.edited(cx);
    }

    fn select_home(&mut self, _: &SelectHome, _: &mut Window, cx: &mut Context<Self>) {
        self.line.select_home();
        self.edited(cx);
    }

    fn select_end(&mut self, _: &SelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.line.select_end();
        self.edited(cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.line.select_all();
        self.edited(cx);
    }

    /// Copy the line's own selection; with none, let the key reach the
    /// cockpit, whose cmd-c copies the transcript selection.
    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        match self.line.selected_text() {
            Some(text) => cx.write_to_clipboard(ClipboardItem::new_string(text.to_string())),
            None => cx.propagate(),
        }
    }

    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.line.selected_text() else {
            cx.propagate();
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        self.line.replace(None, "");
        self.edited(cx);
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if self.line.undo() {
            self.edited(cx);
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if self.line.redo() {
            self.edited(cx);
        }
    }

    /// Where a window point falls in the line, as a byte offset — past the
    /// end when it is right of the text.
    fn offset_at(&self, position: gpui::Point<Pixels>) -> Option<usize> {
        let bounds = self.last_bounds?;
        let layout = self.last_layout.as_ref()?;
        let x = position.x - bounds.left();
        Some(layout.index_for_x(x).unwrap_or(self.line.text().len()))
    }

    /// A press in the line: the caret lands under the pointer, a double
    /// click takes the word, a triple click the whole line — and the press
    /// arms a drag that extends from there.
    pub(crate) fn press(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(offset) = self.offset_at(event.position) else {
            return;
        };
        match event.click_count {
            1 => {
                if event.modifiers.shift {
                    self.line.select_to(offset);
                } else {
                    self.line.place_caret(offset);
                }
            }
            2 => self.line.select_word_at(offset),
            _ => self.line.select_all(),
        }
        self.dragging = event.click_count == 1;
        self.edited(cx);
    }

    fn drag(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.dragging || !event.dragging() {
            return;
        }
        if let Some(offset) = self.offset_at(event.position) {
            self.line.select_to(offset);
            self.edited(cx);
        }
    }

    fn release(&mut self, _: &MouseUpEvent, _cx: &mut Context<Self>) {
        self.dragging = false;
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.line.offset_from_utf16(range.start)..self.line.offset_from_utf16(range.end)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.line.offset_to_utf16(range.start)..self.line.offset_to_utf16(range.end)
    }
}

impl EntityInputHandler for Composer {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.line.text()[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.line.selection()),
            reversed: self.line.reversed(),
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.line.marked().map(|range| self.range_to_utf16(&range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.line.unmark();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16.map(|range| self.range_from_utf16(&range));
        self.line.replace(range, new_text);
        self.edited(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16.map(|range| self.range_from_utf16(&range));
        let selection = new_selected_range_utf16.map(|range| self.range_from_utf16(&range));
        self.line.replace_and_mark(range, new_text, selection);
        self.edited(cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(range.start),
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(range.end),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let index = self
            .last_layout
            .as_ref()?
            .index_for_x(point.x - line_point.x)?;
        Some(self.line.offset_to_utf16(index))
    }
}

impl Focusable for Composer {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Composer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_1()
            .min_w_0()
            // Editable text speaks the I-beam like the transcript's runs do
            // (#26's text role, #27) — an arrow over the one line the
            // operator types into would say the opposite of the truth.
            .hover_text()
            .key_context(match (self.history_available, self.menu_open) {
                (true, true) => "Composer ComposerHistory ComposerMenu",
                (true, false) => "Composer ComposerHistory",
                (false, true) => "Composer ComposerMenu",
                (false, false) => "Composer",
            })
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::delete_word_left))
            .on_action(cx.listener(Self::delete_word_right))
            .on_action(cx.listener(Self::delete_to_start))
            .on_action(cx.listener(Self::delete_to_end))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .child(LineElement {
                composer: cx.entity(),
            })
    }
}

/// Where staged mention tokens still stand whole in the text — start-of-line
/// or whitespace on both sides, exactly the boundaries the wire's own
/// `@`-token rule reads — sorted, overlaps dropped. These byte ranges paint
/// as the comp's @-pill.
fn pill_ranges(text: &str, mentions: &[SharedString]) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for token in mentions {
        let mut from = 0;
        while let Some(found) = text[from..].find(token.as_ref()) {
            let start = from + found;
            let end = start + token.len();
            let before = start == 0 || text[..start].ends_with(char::is_whitespace);
            let after = end == text.len() || text[end..].starts_with(char::is_whitespace);
            if before && after {
                ranges.push(start..end);
                from = end;
            } else {
                from = start + 1;
            }
        }
    }
    ranges.sort_by_key(|range| range.start);
    ranges.dedup_by(|next, kept| next.start < kept.end);
    ranges
}

/// The line's text runs: the base style, the @-pill (`TEXT` ink on the
/// opaque `SELECTION` ground) over `pills`, the selection's own `TEXT_STRONG`
/// ink over `selected` — the selection quad is opaque `#3f3f3f` and the
/// shaped line paints straight over it — and the IME underline over `marked`.
/// Split at every boundary so each run wears exactly its styles.
///
/// The prototype draws no mention pill; `TEXT` on `SELECTION` are the
/// nearest tokens it does define, so no new value is invented here.
fn runs_for(
    base: &TextRun,
    len: usize,
    marked: Option<Range<usize>>,
    pills: &[Range<usize>],
    selected: Option<Range<usize>>,
) -> Vec<TextRun> {
    let mut cuts = vec![0, len];
    if let Some(marked) = &marked {
        cuts.extend([marked.start, marked.end]);
    }
    if let Some(selected) = &selected {
        cuts.extend([selected.start, selected.end]);
    }
    for pill in pills {
        cuts.extend([pill.start, pill.end]);
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut runs = Vec::new();
    for window in cuts.windows(2) {
        let (from, to) = (window[0], window[1]);
        if to <= from {
            continue;
        }
        let mut run = TextRun {
            len: to - from,
            ..base.clone()
        };
        if pills
            .iter()
            .any(|pill| pill.start <= from && to <= pill.end)
        {
            run.color = rgb(crate::theme::TEXT).into();
            run.background_color = Some(rgb(crate::theme::SELECTION).into());
        }
        if selected
            .as_ref()
            .is_some_and(|selected| selected.start <= from && to <= selected.end)
        {
            run.color = rgb(crate::theme::TEXT_STRONG).into();
        }
        if marked
            .as_ref()
            .is_some_and(|marked| marked.start <= from && to <= marked.end)
        {
            run.underline = Some(UnderlineStyle {
                color: Some(rgb(crate::theme::SEP).into()),
                thickness: px(1.),
                wavy: false,
            });
        }
        runs.push(run);
    }
    if runs.is_empty() {
        runs.push(base.clone());
    }
    runs
}

/// Paints the line itself: shaped text, caret, and any IME selection.
struct LineElement {
    composer: Entity<Composer>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for LineElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for LineElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let composer = self.composer.read(cx);
        let content = SharedString::from(composer.line.text().to_string());
        let selected = composer.line.selection();
        let cursor = composer.line.cursor();
        let marked = composer.line.marked();
        let pills = pill_ranges(&content, composer.mentions());
        let style = window.text_style();

        let run = TextRun {
            len: content.len(),
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = runs_for(
            &run,
            content.len(),
            marked,
            &pills,
            (!selected.is_empty()).then(|| selected.clone()),
        );

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(content, font_size, &runs, None);

        let (selection, cursor) = if selected.is_empty() {
            let x = line.x_for_index(cursor);
            // The Soft caret: a 2 × 14 `--text-2` bar, square, no radius, no
            // blink. The prototype centres it in the 20px prompt row (y = row
            // top + 3); this element is the shaped line's own box, centred in
            // that row by `items_center`, so centring in `bounds` lands on the
            // same pixel whatever the line height resolves to.
            let inset = ((bounds.bottom() - bounds.top()) - px(crate::theme::CARET_H)) / 2.;
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + x, bounds.top() + inset),
                        gpui::size(px(crate::theme::CARET_W), px(crate::theme::CARET_H)),
                    ),
                    rgb(crate::theme::TEXT_2),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(selected.start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(selected.end),
                            bounds.bottom(),
                        ),
                    ),
                    // One selection colour everywhere, whoever paints it —
                    // opaque `#3f3f3f`, with `TEXT_STRONG` runs over it.
                    rgb(crate::theme::SELECTION),
                )),
                None,
            )
        };

        PrepaintState {
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.composer.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.composer.clone()),
            cx,
        );
        // The pointer edits the caret too: a press lands it, a drag grows
        // the selection, a release ends the drag wherever it went.
        let composer = self.composer.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble
                || event.button != MouseButton::Left
                || !bounds.contains(&event.position)
            {
                return;
            }
            composer.update(cx, |composer, cx| {
                window.focus(&composer.focus_handle);
                composer.press(event, cx);
            });
        });
        let composer = self.composer.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            composer.update(cx, |composer, cx| composer.drag(event, cx));
        });
        let composer = self.composer.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            composer.update(cx, |composer, cx| composer.release(event, cx));
        });
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let line = prepaint.line.take().unwrap();
        line.paint(bounds.origin, window.line_height(), window, cx)
            .unwrap();
        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }
        self.composer.update(cx, |composer, _cx| {
            composer.last_layout = Some(line);
            composer.last_bounds = Some(bounds);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared(tokens: &[&str]) -> Vec<SharedString> {
        tokens
            .iter()
            .map(|t| SharedString::from(t.to_string()))
            .collect()
    }

    /// #23: a picked mention paints as a pill exactly while its token still
    /// stands whole in the text — the same boundary rule the wire reads.
    #[test]
    #[allow(clippy::single_range_in_vec_init)] // assertions compare literal ranges
    fn pill_ranges_cover_whole_tokens_only() {
        let mentions = shared(&["@docs/notes.txt"]);
        assert_eq!(pill_ranges("read @docs/notes.txt now", &mentions), [5..20]);
        // Edited into a longer word, it is no longer the token.
        assert!(pill_ranges("read @docs/notes.txtx", &mentions).is_empty());
        assert!(pill_ranges("x@docs/notes.txt", &mentions).is_empty());
        // At the very ends of the line it still counts.
        assert_eq!(pill_ranges("@docs/notes.txt", &mentions), [0..15]);
        // Gone from the text, gone from the paint.
        assert!(pill_ranges("plain prose", &mentions).is_empty());
    }

    #[test]
    #[allow(clippy::single_range_in_vec_init)]
    fn overlapping_pill_candidates_keep_the_first() {
        let mentions = shared(&["@a", "@a"]);
        assert_eq!(pill_ranges("@a", &mentions), [0..2]);
    }

    /// The run splitter hands the shaper exactly the text's length, pill
    /// styling only inside pill ranges, and the IME underline only inside
    /// the marked range.
    #[test]
    #[allow(clippy::single_range_in_vec_init)]
    fn runs_split_at_pill_and_marked_boundaries() {
        let base = TextRun {
            len: 0,
            font: gpui::font(crate::theme::FONT_MONO),
            color: gpui::white(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let text = "read @a now";
        let runs = runs_for(&base, text.len(), Some(8..11), &[5..7], None);
        let lens: Vec<usize> = runs.iter().map(|run| run.len).collect();
        assert_eq!(lens.iter().sum::<usize>(), text.len());
        assert_eq!(lens, [5, 2, 1, 3]);
        assert!(
            runs[1].background_color.is_some(),
            "the pill wears the wash"
        );
        assert!(runs[0].background_color.is_none());
        assert!(runs[3].underline.is_some(), "the mark wears the underline");
        assert!(runs[2].underline.is_none());

        // An empty line still hands the shaper one (empty) run.
        assert_eq!(runs_for(&base, 0, None, &[], None).len(), 1);
    }

    /// The selection quad is opaque `#3f3f3f` and the shaped line paints over
    /// it, so every covered run carries the strong ink instead of the base.
    #[test]
    fn selected_runs_take_the_strong_ink() {
        let base = TextRun {
            len: 0,
            font: gpui::font(crate::theme::FONT_MONO),
            color: rgb(crate::theme::TEXT_2).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = runs_for(&base, 11, None, &[], Some(5..7));
        let lens: Vec<usize> = runs.iter().map(|run| run.len).collect();
        assert_eq!(lens, [5, 2, 4]);
        assert_eq!(runs[1].color, rgb(crate::theme::TEXT_STRONG).into());
        assert_eq!(runs[0].color, base.color);
    }
}
