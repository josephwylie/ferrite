//! The Files editor (Claude's, `files-code.png`): one file as numbered lines
//! in the code face, the caret's line on a wash with its number bright,
//! soft-wrapped, in syntax colour, and **editable** — it types, selects,
//! cuts, copies, pastes, undoes, indents and saves, by itself after a
//! pause (`Auto save`) or on ⌘S.
//!
//! The text is the Composer's own editing state (`Line`, which holds the
//! selection, the undo steps and the word motions), so every key of the
//! Composer's grammar acts here under the `FileEditor` context
//! (`keymap`); what only a file needs is `editor::Enter` (a new line that
//! keeps the indent), `Tab`, the page keys and `Save`. Up and down step a
//! logical line and keep their column.
//!
//! Only the lines in view are laid out (`gpui::list`). Each line is a
//! `CodeLine`, which paints its own selection and caret over the shaped
//! text and reports its layout back, so a press lands the caret where it
//! fell.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use ferrite_pane::composer as keys;
use ferrite_pane::line::Line;
use ferrite_ui::theme;
use gpui::prelude::*;
use gpui::{
    actions, div, fill, list, point, px, rgb, rgba, size, AnyElement, App, Bounds, ClipboardItem,
    Context, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler, EventEmitter,
    FocusHandle, Focusable, GlobalElementId, HighlightStyle, IntoElement, LayoutId, ListAlignment,
    ListState, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Render, SharedString,
    StyledText, TextLayout, UTF16Selection, Window,
};

actions!(editor, [Enter, Tab, PageUp, PageDown, Save]);

/// How long after the last edit Auto save writes the file.
pub(crate) const AUTO_SAVE_AFTER: Duration = Duration::from_millis(800);
/// How far past the viewport the list lays out rows.
const OVERDRAW: f32 = 400.0;

/// The text changed.
pub(crate) struct Edited;

/// What the Files ⋯ menu sets for every editor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Settings {
    pub wrap: bool,
    pub auto_save: bool,
    pub tab_size: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            wrap: true,
            auto_save: true,
            tab_size: 4,
        }
    }
}

pub(crate) struct Editor {
    path: PathBuf,
    text: Line,
    /// Where each line starts, as byte offsets (`reindex`).
    starts: Vec<usize>,
    language: Option<&'static str>,
    settings: Settings,
    dirty: bool,
    focus: FocusHandle,
    list: ListState,
    /// Each painted line's shaped text, for a press to read its column from.
    layouts: Rc<RefCell<HashMap<usize, TextLayout>>>,
    /// The column ↑ and ↓ keep aiming at, in characters.
    goal_column: Option<usize>,
    dragging: bool,
    caret_bounds: Rc<RefCell<Option<Bounds<Pixels>>>>,
    saving: Option<gpui::Task<()>>,
}

impl EventEmitter<Edited> for Editor {}

impl Editor {
    pub(crate) fn new(path: &Path, source: String, settings: Settings, cx: &mut Context<Self>) -> Self {
        let mut text = Line::default();
        text.set(source);
        text.place_caret(0);
        let mut editor = Self {
            path: path.to_path_buf(),
            text,
            starts: Vec::new(),
            language: ferrite_core::transcript::language_for_path(path),
            settings,
            dirty: false,
            focus: cx.focus_handle(),
            list: ListState::new(0, ListAlignment::Top, px(OVERDRAW)),
            layouts: Rc::default(),
            goal_column: None,
            dragging: false,
            caret_bounds: Rc::default(),
            saving: None,
        };
        editor.reindex();
        editor.list.reset(editor.starts.len());
        editor
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub(crate) fn configure(&mut self, settings: Settings, cx: &mut Context<Self>) {
        if self.settings != settings {
            self.settings = settings;
            self.list.reset(self.starts.len());
            if self.dirty && settings.auto_save {
                self.schedule_save(cx);
            }
            cx.notify();
        }
    }

    /// Put the caret at the start of `line` (1-based) and show it.
    pub(crate) fn go_to_line(&mut self, line: usize, cx: &mut Context<Self>) {
        let line = line.saturating_sub(1).min(self.starts.len() - 1);
        self.text.place_caret(self.starts[line]);
        self.reveal_caret();
        cx.notify();
    }

    /// Write the file now.
    pub(crate) fn save(&mut self) {
        self.saving = None;
        if self.dirty && std::fs::write(&self.path, self.text.text()).is_ok() {
            self.dirty = false;
        }
    }

    // ------------------------------------------------------------- lines

    fn reindex(&mut self) {
        let text = self.text.text();
        self.starts.clear();
        self.starts.push(0);
        self.starts
            .extend(text.match_indices('\n').map(|(at, _)| at + 1));
    }

    fn line_range(&self, line: usize) -> Range<usize> {
        let text = self.text.text();
        let start = self.starts[line];
        let end = self.starts.get(line + 1).map_or(text.len(), |next| next - 1);
        start..end
    }

    fn line_of(&self, offset: usize) -> usize {
        self.starts.partition_point(|start| *start <= offset) - 1
    }

    fn cursor_line(&self) -> usize {
        self.line_of(self.text.cursor())
    }

    fn column_of(&self, offset: usize) -> usize {
        let start = self.starts[self.line_of(offset)];
        self.text.text()[start..offset].chars().count()
    }

    fn offset_at_column(&self, line: usize, column: usize) -> usize {
        let range = self.line_range(line);
        let text = &self.text.text()[range.clone()];
        let within = text
            .char_indices()
            .nth(column)
            .map_or(text.len(), |(at, _)| at);
        range.start + within
    }

    fn reveal_caret(&self) {
        self.list.scroll_to_reveal_item(self.cursor_line());
    }

    // ------------------------------------------------------------- acts

    /// A motion: the selection or caret moves, the text does not.
    fn moved(&mut self, cx: &mut Context<Self>, keep_column: bool, act: impl FnOnce(&mut Self)) {
        let column = self.goal_column.take();
        act(self);
        if keep_column {
            self.goal_column = column;
        }
        self.reveal_caret();
        cx.notify();
    }

    /// An edit: the text changes, and is saved after a pause.
    fn edit(&mut self, cx: &mut Context<Self>, act: impl FnOnce(&mut Line)) {
        let was = self.starts.len();
        act(&mut self.text);
        self.goal_column = None;
        self.reindex();
        if self.starts.len() != was {
            self.list.reset(self.starts.len());
        } else {
            let line = self.cursor_line();
            self.list.splice(line..line + 1, 1);
        }
        self.dirty = true;
        self.schedule_save(cx);
        self.reveal_caret();
        cx.emit(Edited);
        cx.notify();
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        if !self.settings.auto_save {
            return;
        }
        self.saving = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(AUTO_SAVE_AFTER).await;
            this.update(cx, |editor, _| editor.save()).ok();
        }));
    }

    fn step_lines(&mut self, by: isize, select: bool, cx: &mut Context<Self>) {
        self.moved(cx, true, |editor| {
            let line = editor.cursor_line();
            let current = editor.column_of(editor.text.cursor());
            let column = *editor.goal_column.get_or_insert(current);
            let target = line.saturating_add_signed(by).min(editor.starts.len() - 1);
            let at = if by < 0 && line == 0 {
                0
            } else if by > 0 && line == editor.starts.len() - 1 {
                editor.text.text().len()
            } else {
                editor.offset_at_column(target, column)
            };
            if select {
                editor.text.select_to(at);
            } else {
                editor.text.place_caret(at);
            }
        });
    }

    fn page(&self) -> isize {
        let viewport = self.list.viewport_bounds().size.height;
        ((viewport / px(theme::LH_UI)) as isize - 1).max(1)
    }

    fn line_start(&mut self, select: bool, cx: &mut Context<Self>) {
        self.moved(cx, false, |editor| {
            let at = editor.starts[editor.cursor_line()];
            if select {
                editor.text.select_to(at);
            } else {
                editor.text.place_caret(at);
            }
        });
    }

    fn line_end(&mut self, select: bool, cx: &mut Context<Self>) {
        self.moved(cx, false, |editor| {
            let at = editor.line_range(editor.cursor_line()).end;
            if select {
                editor.text.select_to(at);
            } else {
                editor.text.place_caret(at);
            }
        });
    }

    fn enter(&mut self, cx: &mut Context<Self>) {
        let line = self.line_range(self.cursor_line());
        let indent: String = self.text.text()[line]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        self.edit(cx, |text| text.replace(None, &format!("\n{indent}")));
    }

    fn tab(&mut self, cx: &mut Context<Self>) {
        let column = self.column_of(self.text.cursor());
        let width = self.settings.tab_size;
        let spaces = " ".repeat(width - column % width);
        self.edit(cx, |text| text.replace(None, &spaces));
    }

    fn copy(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(selected) = self.text.selected_text() else {
            return false;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(selected.to_string()));
        true
    }

    fn press(&mut self, line: usize, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        let range = self.line_range(line);
        let column = self
            .layouts
            .borrow()
            .get(&line)
            .map(|layout| layout.index_for_position(event.position).unwrap_or_else(|nearest| nearest))
            .unwrap_or(0);
        let at = range.start + column.min(range.len());
        self.dragging = true;
        self.moved(cx, false, |editor| match event.click_count {
            1 if event.modifiers.shift => editor.text.select_to(at),
            1 => editor.text.place_caret(at),
            2 => editor.text.select_word_at(at),
            _ => {
                editor.text.place_caret(range.start);
                editor.text.select_to(range.end);
            }
        });
    }

    fn drag(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.dragging || event.pressed_button != Some(MouseButton::Left) {
            self.dragging = false;
            return;
        }
        let hit = self.layouts.borrow().iter().find_map(|(line, layout)| {
            let bounds = layout.bounds();
            let (top, bottom) = (bounds.top(), bounds.bottom());
            (event.position.y >= top && event.position.y < bottom).then(|| {
                let column = layout
                    .index_for_position(event.position)
                    .unwrap_or_else(|nearest| nearest);
                (*line, column)
            })
        });
        if let Some((line, column)) = hit {
            let range = self.line_range(line);
            let at = range.start + column.min(range.len());
            self.moved(cx, false, |editor| editor.text.select_to(at));
        }
    }

    // ------------------------------------------------------------- draw

    fn render_line(&self, line: usize, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let range = self.line_range(line);
        let source = &self.text.text()[range.clone()];
        let current = line == self.cursor_line();
        let tokens = ferrite_core::transcript::highlight_tokens(self.language, source);
        let highlights: Vec<(Range<usize>, HighlightStyle)> =
            ferrite_pane::pane::code(source, Some(tokens.as_slice()));
        let selection = self.text.selection();
        let within = (selection.start < range.end + 1 && selection.end > range.start)
            .then(|| {
                selection.start.max(range.start) - range.start
                    ..selection.end.min(range.end + 1) - range.start
            })
            .filter(|within| !within.is_empty());
        let focused = self.focus.is_focused(window);
        let caret = (current && selection.is_empty() && focused)
            .then(|| self.text.cursor() - range.start);
        let code = CodeLine {
            text: StyledText::new(SharedString::from(source.to_string())).with_highlights(highlights),
            selection: within,
            len: source.len(),
            caret,
            line,
            layouts: self.layouts.clone(),
            caret_bounds: self.caret_bounds.clone(),
            input: caret.map(|_| (self.focus.clone(), cx.entity())),
        };
        div()
            .flex()
            .w_full()
            .min_h(px(theme::LH_UI))
            .when(current, |row| row.bg(theme::paint::SELECTED))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |editor, event: &MouseDownEvent, window, cx| {
                    editor.press(line, event, window, cx)
                }),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .justify_end()
                    .w(px(theme::READER_NUMBER_W))
                    .pr(px(theme::READER_NUMBER_GAP))
                    .text_size(px(theme::FS_META))
                    .text_color(rgb(if current {
                        theme::INK_STRONG
                    } else {
                        theme::INK_SUBTLE
                    }))
                    .child(SharedString::from((line + 1).to_string())),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .pr(px(theme::READER_NUMBER_GAP))
                    .when(!self.settings.wrap, |cell| cell.whitespace_nowrap().overflow_hidden())
                    .child(code),
            )
            .into_any_element()
    }
}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

macro_rules! on {
    ($root:expr, $cx:expr, $($action:ty => |$editor:ident, $cx2:ident| $body:expr),+ $(,)?) => {{
        let root = $root;
        $(let root = root.on_action($cx.listener(|$editor, _: &$action, _, $cx2| $body));)+
        root
    }};
}

impl Render for Editor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity();
        let root = div()
            .debug_selector(|| "editor".into())
            .key_context(ferrite_ui::keymap::FILE_EDITOR)
            .track_focus(&self.focus)
            .size_full()
            .font_family(theme::FONT_CODE)
            .text_size(px(theme::FS_READER_CODE))
            .line_height(px(theme::LH_UI))
            .text_color(rgb(theme::SYN_PLAIN))
            .on_mouse_move(cx.listener(|editor, event: &MouseMoveEvent, _, cx| editor.drag(event, cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(|editor, _, _, _| editor.dragging = false))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|editor, _, _, _| editor.dragging = false));
        let root = on!(root, cx,
            keys::Backspace => |e, cx| e.edit(cx, Line::backspace),
            keys::Delete => |e, cx| e.edit(cx, Line::delete),
            keys::DeleteWordLeft => |e, cx| e.edit(cx, Line::delete_word_left),
            keys::DeleteWordRight => |e, cx| e.edit(cx, Line::delete_word_right),
            keys::DeleteToStart => |e, cx| e.edit(cx, Line::delete_to_start),
            keys::DeleteToEnd => |e, cx| e.edit(cx, Line::delete_to_end),
            keys::Newline => |e, cx| e.edit(cx, Line::insert_newline),
            keys::Undo => |e, cx| {
                if e.text.undo() {
                    e.edit(cx, |_| {});
                }
            },
            keys::Redo => |e, cx| {
                if e.text.redo() {
                    e.edit(cx, |_| {});
                }
            },
            keys::Left => |e, cx| e.moved(cx, false, |e| e.text.move_left()),
            keys::Right => |e, cx| e.moved(cx, false, |e| e.text.move_right()),
            keys::WordLeft => |e, cx| e.moved(cx, false, |e| e.text.move_word_left()),
            keys::WordRight => |e, cx| e.moved(cx, false, |e| e.text.move_word_right()),
            keys::SelectLeft => |e, cx| e.moved(cx, false, |e| e.text.select_left()),
            keys::SelectRight => |e, cx| e.moved(cx, false, |e| e.text.select_right()),
            keys::SelectWordLeft => |e, cx| e.moved(cx, false, |e| e.text.select_word_left()),
            keys::SelectWordRight => |e, cx| e.moved(cx, false, |e| e.text.select_word_right()),
            keys::SelectAll => |e, cx| e.moved(cx, false, |e| e.text.select_all()),
            keys::Home => |e, cx| e.line_start(false, cx),
            keys::End => |e, cx| e.line_end(false, cx),
            keys::SelectHome => |e, cx| e.line_start(true, cx),
            keys::SelectEnd => |e, cx| e.line_end(true, cx),
            keys::Up => |e, cx| e.step_lines(-1, false, cx),
            keys::Down => |e, cx| e.step_lines(1, false, cx),
            PageUp => |e, cx| e.step_lines(-e.page(), false, cx),
            PageDown => |e, cx| e.step_lines(e.page(), false, cx),
            Save => |e, _cx| e.save(),
        );
        root.on_action(cx.listener(|editor, _: &Enter, _, cx| editor.enter(cx)))
            .on_action(cx.listener(|editor, _: &Tab, _, cx| editor.tab(cx)))
            .on_action(cx.listener(|editor, _: &keys::Copy, _, cx| {
                if !editor.copy(cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|editor, _: &keys::Cut, _, cx| {
                if editor.copy(cx) {
                    editor.edit(cx, Line::backspace);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|editor, _: &keys::Paste, _, cx| {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    editor.edit(cx, |line| line.replace(None, &text));
                }
            }))
            .child(
                list(self.list.clone(), move |line, window, cx| {
                    this.update(cx, |editor, cx| editor.render_line(line, window, cx))
                })
                .size_full()
                .py(px(theme::HALF_ROW)),
            )
    }
}

/// The platform's text input into the line holding the caret.
impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.utf8(&range);
        actual.replace(self.utf16(&range));
        Some(self.text.text()[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.utf16(&self.text.selection()),
            reversed: self.text.reversed(),
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.text.marked().map(|range| self.utf16(&range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.text.unmark();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range.map(|range| self.utf8(&range));
        self.edit(cx, |line| line.replace(range, text));
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range.map(|range| self.utf8(&range));
        let selection = selection.map(|range| self.utf8(&range));
        self.edit(cx, |line| line.replace_and_mark(range, text, selection));
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        *self.caret_bounds.borrow()
    }

    fn character_index_for_point(
        &mut self,
        _: gpui::Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

impl Editor {
    fn utf8(&self, range: &Range<usize>) -> Range<usize> {
        self.text.offset_from_utf16(range.start)..self.text.offset_from_utf16(range.end)
    }

    fn utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.text.offset_to_utf16(range.start)..self.text.offset_to_utf16(range.end)
    }
}

/// One line's text with its selection and caret painted over it.
struct CodeLine {
    text: StyledText,
    /// The selected bytes of the line (a selected newline counts one past
    /// its end).
    selection: Option<Range<usize>>,
    /// The line's length in bytes.
    len: usize,
    caret: Option<usize>,
    line: usize,
    layouts: Rc<RefCell<HashMap<usize, TextLayout>>>,
    caret_bounds: Rc<RefCell<Option<Bounds<Pixels>>>>,
    /// The editor and its focus, while this line holds the caret: it takes
    /// the platform's text input.
    input: Option<(FocusHandle, Entity<Editor>)>,
}

impl IntoElement for CodeLine {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for CodeLine {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        self.text.request_layout(id, inspector, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.text.prepaint(id, inspector, bounds, &mut (), window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let layout = self.text.layout().clone();
        self.layouts.borrow_mut().insert(self.line, layout.clone());
        let line_height = layout.line_height();
        if let Some(selection) = &self.selection {
            let at = |index: usize| {
                layout
                    .position_for_index(index.min(self.len))
                    .unwrap_or(bounds.origin)
            };
            let (from, to) = (at(selection.start), at(selection.end));
            // A selected newline washes a little past the line's end.
            let end_pad = if selection.end > self.len {
                px(theme::CH)
            } else {
                px(0.)
            };
            let mut top = from.y;
            while top <= to.y {
                let left = if top == from.y { from.x } else { bounds.left() };
                let right = if top == to.y { to.x + end_pad } else { bounds.right() };
                window.paint_quad(fill(
                    Bounds::from_corners(point(left, top), point(right, top + line_height)),
                    rgba(theme::COMPOSER_SELECTION),
                ));
                top += line_height;
            }
        }
        self.text
            .paint(id, inspector, bounds, &mut (), &mut (), window, cx);
        if let Some(caret) = self.caret {
            let at = layout.position_for_index(caret).unwrap_or(bounds.origin);
            let block = Bounds::new(at, size(px(theme::CARET_W), line_height));
            window.paint_quad(fill(block, rgb(theme::CARET)));
            *self.caret_bounds.borrow_mut() = Some(block);
        }
        if let Some((focus, editor)) = self.input.take() {
            window.handle_input(&focus, ElementInputHandler::new(bounds, editor), cx);
        }
    }
}

#[cfg(test)]
impl Editor {
    pub(crate) fn text(&self) -> &str {
        self.text.text()
    }

    pub(crate) fn caret(&self) -> usize {
        self.text.cursor()
    }

    /// What typing `text` at the caret does.
    pub(crate) fn type_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.edit(cx, |line| line.replace(None, text));
    }
}
