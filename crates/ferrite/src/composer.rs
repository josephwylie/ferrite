//! The Composer: one shell-style prompt line. This is the window half —
//! focus, key actions, and painting; the editing state lives in `Line`.

use std::ops::Range;

use gpui::prelude::*;
use gpui::{
    actions, div, fill, point, px, relative, rgb, rgba, App, Bounds, Context, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable,
    GlobalElementId, LayoutId, PaintQuad, Pixels, ShapedLine, SharedString, Style, TextRun,
    UTF16Selection, UnderlineStyle, Window,
};

use crate::line::Line;

actions!(composer, [Backspace, Delete, Left, Right, Home, End, Paste]);

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
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
}

impl EventEmitter<Edited> for Composer {}

impl Composer {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            line: Line::default(),
            mentions: Vec::new(),
            menu_open: false,
            last_layout: None,
            last_bounds: None,
        }
    }

    /// Told by the cockpit as its menu opens or closes over this line.
    pub fn set_menu_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.menu_open != open {
            self.menu_open = open;
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
            reversed: false,
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
            .key_context(if self.menu_open {
                "Composer ComposerMenu"
            } else {
                "Composer"
            })
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
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

/// The line's text runs: the base style, the comp's @-pill (ACCENT ink on
/// the ACCENT_WASH ground) over `pills`, and the IME underline over
/// `marked` — split at every boundary so each run wears exactly its styles.
fn runs_for(
    base: &TextRun,
    len: usize,
    marked: Option<Range<usize>>,
    pills: &[Range<usize>],
) -> Vec<TextRun> {
    let mut cuts = vec![0, len];
    if let Some(marked) = &marked {
        cuts.extend([marked.start, marked.end]);
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
            run.color = rgb(crate::theme::ACCENT).into();
            run.background_color = Some(rgba(crate::theme::ACCENT_WASH).into());
        }
        if marked
            .as_ref()
            .is_some_and(|marked| marked.start <= from && to <= marked.end)
        {
            run.underline = Some(UnderlineStyle {
                color: Some(run.color),
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
        let runs = runs_for(&run, content.len(), marked, &pills);

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(content, font_size, &runs, None);

        let (selection, cursor) = if selected.is_empty() {
            let x = line.x_for_index(cursor);
            (
                None,
                // The comps' block cursor: solid accent, 7px wide.
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + x, bounds.top()),
                        gpui::size(px(crate::theme::CURSOR_W), bounds.bottom() - bounds.top()),
                    ),
                    gpui::rgb(crate::theme::ACCENT),
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
                    // The transcript's selection wash: one selection colour
                    // everywhere, whoever paints it.
                    rgba(crate::theme::SELECTION),
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
        let runs = runs_for(&base, text.len(), Some(8..11), &[5..7]);
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
        assert_eq!(runs_for(&base, 0, None, &[]).len(), 1);
    }
}
