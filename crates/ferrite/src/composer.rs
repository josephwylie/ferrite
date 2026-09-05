//! The Composer: the shell-style prompt box. This is the window half —
//! focus, key actions, and painting; the editing state lives in `Line`.
//!
//! The text soft-wraps at the box's width and the box grows a row per
//! visual line, up to `MAX_ROWS`; past that it scrolls to keep the caret
//! in view. Every pointer and caret question goes through one `Layout`
//! table of visual rows, so wrapped and hard-broken lines read alike.

use std::ops::Range;
use std::path::PathBuf;
use std::time::Duration;

use ferrite_core::prompt_files;

use gpui::prelude::*;
use gpui::{
    actions, div, fill, point, px, relative, rgb, size, App, AvailableSpace, Bounds, ClipboardItem,
    ContentMask, Context, DispatchPhase, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, SharedString,
    Style, TextAlign, Task, TextRun, TextStyle, UTF16Selection, UnderlineStyle, Window,
    WrappedLine,
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
        // A hard line break (shift-enter); enter stays Submit.
        Newline,
        // A visual row up or down. From the first or last row the key
        // propagates, so history recall keeps its single-line meaning.
        Up,
        Down,
    ]
);

/// Half a blink cycle: the caret is solid this long, then hidden this long,
/// the rate every platform's native text field uses.
const BLINK: Duration = Duration::from_millis(500);

/// The most visual rows the box grows to before it scrolls: the prompt
/// grows with its text, but the transcript above keeps most of the Pane.
pub const MAX_ROWS: usize = 8;

/// The line moved — text or cursor. The cockpit listens to keep the `/` and
/// `@` menus following what the operator is typing (#23).
pub struct Edited;

pub struct Composer {
    focus_handle: FocusHandle,
    files_focus: FocusHandle,
    line: Line,
    files: Vec<PathBuf>,
    files_generation: usize,
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
    last_layout: Option<Layout>,
    last_bounds: Option<Bounds<Pixels>>,
    /// The first visual row painted. Zero until the text passes `MAX_ROWS`;
    /// then prepaint slides it so the caret's row stays in view.
    scroll: usize,
    /// The x a row step keeps aiming at across successive ↑/↓ presses —
    /// a short row in between does not lose the column. Any other edit
    /// clears it.
    goal_x: Option<Pixels>,
    /// A press landed in the line and has not been released: moves extend
    /// the selection from where it landed.
    dragging: bool,
    /// The caret's current blink phase. Solid whenever the line is unfocused,
    /// so focusing always lands on a visible caret.
    caret_visible: bool,
    /// The running blink cycle — `None` while the line does not hold focus.
    caret_blink: Option<Task<()>>,
    /// Bumped whenever the cycle is restarted or stopped, so a timer from a
    /// superseded cycle retires instead of toggling the caret behind the
    /// current one.
    caret_epoch: usize,
}

impl EventEmitter<Edited> for Composer {}

impl Composer {
    pub fn new(cx: &mut Context<Self>) -> Self {
        crate::theme::init_components(cx);
        Self {
            focus_handle: cx.focus_handle(),
            files_focus: cx.focus_handle(),
            line: Line::default(),
            files: Vec::new(),
            files_generation: 0,
            mentions: Vec::new(),
            menu_open: false,
            history_available: false,
            last_layout: None,
            last_bounds: None,
            scroll: 0,
            goal_x: None,
            dragging: false,
            caret_visible: true,
            caret_blink: None,
            caret_epoch: 0,
        }
    }

    /// Files stay separate from editable prose until the send/history seam.
    pub fn attachments(
        entity: &Entity<Self>,
        preview: &crate::attachment_preview::Preview,
        cx: &App,
    ) -> Option<gpui::AnyElement> {
        let composer = entity.read(cx);
        if composer.files.is_empty() {
            return None;
        }
        let files = composer.files.clone();
        let focus = composer.files_focus.clone();
        let generation = composer.files_generation;
        let composer = entity.downgrade();
        Some(
            div()
                .id("pending-attachment-tray")
                .debug_selector(|| "pending-attachment-tray".into())
                .track_focus(&focus)
                .tab_stop(true)
                .child(
                    crate::attachments::Attachments::new("prompt-attachments", files, preview)
                        .in_island(generation)
                        .on_remove(move |remove, window, cx| {
                            let _ = composer.update(cx, |composer, cx| {
                                composer.files.retain(|path| path != remove);
                                composer.focus_handle.focus(window, cx);
                                composer.edited(cx);
                            });
                        }),
                )
                .into_any_element(),
        )
    }

    pub fn focus_target(&self, window: &Window, cx: &App) -> FocusHandle {
        if self.files_focus.contains_focused(window, cx) {
            self.files_focus.clone()
        } else {
            self.focus_handle.clone()
        }
    }

    /// Visit the kit's attachment controls before the cockpit's band/tools.
    /// Leaving this Composer restores its caret and resumes normal traversal.
    pub fn cycle_files(&self, reverse: bool, window: &mut Window, cx: &mut App) -> bool {
        if self.files.is_empty()
            || (!self.focus_handle.is_focused(window)
                && !self.files_focus.contains_focused(window, cx))
        {
            return false;
        }
        if !reverse && self.focus_handle.is_focused(window) {
            self.files_focus.focus(window, cx);
        }
        if reverse {
            window.focus_prev(cx);
        } else {
            window.focus_next(cx);
        }
        if self.files_focus.is_focused(window) || !self.files_focus.contains_focused(window, cx) {
            self.focus_handle.focus(window, cx);
            return false;
        }
        !self.focus_handle.is_focused(window)
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
        let (text, files) = prompt_files::split(text);
        if !files.is_empty() && files != self.files {
            self.files_generation = self.files_generation.wrapping_add(1);
        }
        self.files = files;
        self.line.set(text);
        self.edited(cx);
    }

    /// Hand the line to the Pane and clear it — pills and all.
    pub fn take(&mut self, cx: &mut Context<Self>) -> String {
        let text = self.prompt();
        self.line.take();
        self.files.clear();
        self.mentions.clear();
        self.edited(cx);
        text
    }

    /// Whether the line holds no text — what gives Backspace its
    /// `⌫ unqueue` meaning up in the cockpit.
    pub fn is_empty(&self) -> bool {
        self.line.text().is_empty() && self.files.is_empty()
    }

    pub fn prompt(&self) -> String {
        prompt_files::compose(self.line.text(), &self.files)
    }

    pub fn add_files(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        let before = self.files.len();
        for path in paths {
            if !self.files.contains(path) {
                self.files.push(path.clone());
            }
        }
        if self.files.len() > before {
            self.files_generation = self.files_generation.wrapping_add(1);
        }
        self.edited(cx);
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

    /// How many visual rows the text last painted as — the box's height in
    /// rows, before the `MAX_ROWS` clamp.
    #[cfg(test)]
    pub(crate) fn rows(&self) -> usize {
        self.last_layout.as_ref().map_or(1, Layout::rows)
    }

    /// The visual row the caret sits on, as last painted.
    #[cfg(test)]
    pub(crate) fn caret_row(&self) -> usize {
        self.last_layout
            .as_ref()
            .map_or(0, |layout| layout.row_of(self.line.cursor()))
    }

    fn edited(&mut self, cx: &mut Context<Self>) {
        self.goal_x = None;
        // The caret just moved: show it solid again and restart the cycle, so
        // it is never invisible at the moment the operator is looking for it.
        if self.caret_blink.is_some() {
            self.start_caret_blink(cx);
        }
        cx.emit(Edited);
        cx.notify();
    }

    /// Match the blink cycle to whether this line holds focus. Called from
    /// the element's prepaint, which sees the window's focus each frame.
    fn sync_caret_blink(&mut self, focused: bool, cx: &mut Context<Self>) {
        match (focused, self.caret_blink.is_some()) {
            (true, false) => self.start_caret_blink(cx),
            (false, true) => self.stop_caret_blink(),
            _ => {}
        }
    }

    /// Restart the cycle from the solid phase.
    fn start_caret_blink(&mut self, cx: &mut Context<Self>) {
        self.caret_visible = true;
        self.caret_epoch = self.caret_epoch.wrapping_add(1);
        let epoch = self.caret_epoch;
        self.caret_blink = Some(cx.spawn(async move |composer, cx| {
            loop {
                cx.background_executor().timer(BLINK).await;
                let Some(composer) = composer.upgrade() else {
                    return;
                };
                let current = composer.update(cx, |composer, cx| {
                    if composer.caret_epoch != epoch {
                        return false;
                    }
                    composer.caret_visible = !composer.caret_visible;
                    cx.notify();
                    true
                });
                if !current {
                    return;
                }
            }
        }));
        cx.notify();
    }

    /// Drop the cycle and leave the caret solid for the next focus.
    fn stop_caret_blink(&mut self) {
        self.caret_epoch = self.caret_epoch.wrapping_add(1);
        self.caret_blink = None;
        self.caret_visible = true;
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
            // The draft holds hard newlines now, so pasted ones stay —
            // only the carriage returns go.
            self.line
                .replace(None, &text.replace("\r\n", "\n").replace('\r', "\n"));
            self.edited(cx);
        }
    }

    fn newline(&mut self, _: &Newline, _: &mut Window, cx: &mut Context<Self>) {
        self.line.insert_newline();
        self.edited(cx);
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        if !self.step_row(-1, cx) {
            cx.propagate();
        }
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        if !self.step_row(1, cx) {
            cx.propagate();
        }
    }

    /// Move the caret one visual row up or down, keeping its column; false
    /// when there is no such row — the key is then someone else's (history
    /// recall on a one-row draft, or from its first or last row).
    fn step_row(&mut self, delta: isize, cx: &mut Context<Self>) -> bool {
        let Some(layout) = &self.last_layout else {
            return false;
        };
        let cursor = self.line.cursor();
        let target = layout.row_of(cursor) as isize + delta;
        if target < 0 || target >= layout.rows() as isize {
            return false;
        }
        let x = self.goal_x.unwrap_or_else(|| layout.position(cursor).x);
        let offset = layout.offset_in_row(target as usize, x);
        self.line.place_caret(offset);
        self.edited(cx);
        self.goal_x = Some(x);
        true
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

    /// Where a window point falls in the text, as a byte offset — the
    /// nearest row's nearest boundary when it is outside the rows, so a
    /// drag that leaves the box still selects to an end.
    fn offset_at(&self, position: gpui::Point<Pixels>) -> Option<usize> {
        let bounds = self.last_bounds?;
        let layout = self.last_layout.as_ref()?;
        let mut local = position - bounds.origin;
        local.y += layout.line_height * self.scroll;
        Some(layout.offset_at(local))
    }

    /// A press in the box: the caret lands under the pointer, a double
    /// click takes the word, a triple click the whole draft — and the press
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
        let layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        // The IME candidate window hangs off the range's first row.
        let start = layout.position(range.start);
        let end = layout.position(range.end);
        let top = bounds.top() + start.y - layout.line_height * self.scroll;
        Some(Bounds::from_corners(
            point(bounds.left() + start.x, top),
            point(bounds.left() + end.x, top + layout.line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        self.last_bounds?.localize(&point)?;
        let index = self.offset_at(point)?;
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
            .tab_stop(true)
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
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .flex_col()
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

/// One visual row of the wrapped text: the hard line it is cut from, where
/// that line starts in the whole text, and the whole-text byte range the
/// row shows. Rows of one line meet at their ends — a wrap boundary closes
/// one row and opens the next — so a lookup takes the last row that starts
/// at or before an offset: a caret on the boundary paints at the start of
/// the later row, where what it types will land, and the earlier row's
/// trailing space is its last caret stop.
#[derive(Clone, Copy, Debug)]
struct Row {
    line: usize,
    line_start: usize,
    start: usize,
    end: usize,
}

/// The shaped text as last laid out: the hard lines, each wrapped at the
/// box's width, flattened into visual rows. Positions are local to the
/// text's own origin, unscrolled — the element applies `scroll`.
pub(crate) struct Layout {
    lines: Vec<WrappedLine>,
    rows: Vec<Row>,
    line_height: Pixels,
}

impl Layout {
    /// Shape `composer`'s text at `wrap_width` — `None` measures it
    /// unwrapped — in the text `style` the element was requested under.
    fn shape(
        composer: &Composer,
        style: &TextStyle,
        line_height: Pixels,
        wrap_width: Option<Pixels>,
        window: &Window,
    ) -> Self {
        let content = SharedString::from(composer.line.text().to_string());
        let selected = composer.line.selection();
        let pills = pill_ranges(&content, composer.mentions());
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
            composer.line.marked(),
            &pills,
            (!selected.is_empty()).then(|| selected.clone()),
        );
        let font_size = style.font_size.to_pixels(window.rem_size());
        let lines: Vec<WrappedLine> = window
            .text_system()
            .shape_text(content, font_size, &runs, wrap_width, None)
            .map(|lines| lines.into_vec())
            .unwrap_or_default();
        let mut rows = Vec::new();
        let mut line_start = 0;
        for (index, line) in lines.iter().enumerate() {
            let mut start = 0;
            for boundary in line.wrap_boundaries() {
                let end = line.runs()[boundary.run_ix].glyphs[boundary.glyph_ix].index;
                rows.push(Row {
                    line: index,
                    line_start,
                    start: line_start + start,
                    end: line_start + end,
                });
                start = end;
            }
            rows.push(Row {
                line: index,
                line_start,
                start: line_start + start,
                end: line_start + line.len(),
            });
            line_start += line.len() + 1;
        }
        Self {
            lines,
            rows,
            line_height,
        }
    }

    fn rows(&self) -> usize {
        self.rows.len().max(1)
    }

    /// The widest row, unwrapped — what the box measures as when no width
    /// is known.
    fn width(&self) -> Pixels {
        self.lines
            .iter()
            .map(|line| line.width())
            .fold(Pixels::ZERO, Pixels::max)
    }

    /// The last row starting at or before `offset`.
    fn row_of(&self, offset: usize) -> usize {
        self.rows
            .iter()
            .rposition(|row| row.start <= offset)
            .unwrap_or(0)
    }

    /// The last offset a caret on row `index` can stand at: the row's end,
    /// or — when the row wraps into another — before its last character,
    /// since the wrap boundary itself belongs to the next row.
    fn caret_end(&self, index: usize) -> usize {
        let row = self.rows[index];
        let wraps = self
            .rows
            .get(index + 1)
            .is_some_and(|next| next.line == row.line);
        if !wraps {
            return row.end;
        }
        let text = &self.lines[row.line].text;
        let last = text[..row.end - row.line_start]
            .chars()
            .next_back()
            .map_or(0, char::len_utf8);
        (row.end - last).max(row.start)
    }

    /// The x of `offset` within row `index`.
    fn x_in_row(&self, index: usize, offset: usize) -> Pixels {
        let Some(row) = self.rows.get(index) else {
            return Pixels::ZERO;
        };
        let layout = &self.lines[row.line].unwrapped_layout;
        let offset = offset.clamp(row.start, row.end) - row.line_start;
        layout.x_for_index(offset) - layout.x_for_index(row.start - row.line_start)
    }

    /// Where `offset` paints: its row's top-left plus its x.
    fn position(&self, offset: usize) -> gpui::Point<Pixels> {
        let row = self.row_of(offset);
        point(self.x_in_row(row, offset), self.line_height * row)
    }

    /// The boundary nearest `x` on row `index`, clamped to the row's ends.
    fn offset_in_row(&self, index: usize, x: Pixels) -> usize {
        let Some(row) = self.rows.get(index) else {
            return 0;
        };
        let layout = &self.lines[row.line].unwrapped_layout;
        let row_start = row.start - row.line_start;
        let nearest = layout.closest_index_for_x(x + layout.x_for_index(row_start));
        (row.line_start + nearest).clamp(row.start, self.caret_end(index))
    }

    /// The offset under an unscrolled local point: above the rows is the
    /// first, below them the last, and each row clamps to its ends.
    fn offset_at(&self, local: gpui::Point<Pixels>) -> usize {
        let row = if local.y < Pixels::ZERO {
            0
        } else {
            ((local.y / self.line_height) as usize).min(self.rows().saturating_sub(1))
        };
        self.offset_in_row(row, local.x)
    }

    /// The first row of hard line `line`.
    fn first_row_of_line(&self, line: usize) -> usize {
        self.rows
            .iter()
            .position(|row| row.line == line)
            .unwrap_or(0)
    }
}

/// Paints the text itself: the wrapped rows, caret, selection, and any IME
/// mark. Its height follows its width — a row per visual line.
struct LineElement {
    composer: Entity<Composer>,
}

struct PrepaintState {
    layout: Option<Layout>,
    /// The first row painted; rows above it are scrolled off.
    scroll: usize,
    cursor: Option<PaintQuad>,
    selection: Vec<PaintQuad>,
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
        _cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        // The measure runs later, under whatever text style the layout pass
        // is in, so the element's own is captured now.
        let text_style = window.text_style();
        let line_height = window.line_height();
        let composer = self.composer.clone();
        let layout_id =
            window.request_measured_layout(style, move |known, available, window, cx| {
                let width = known.width.or(match available.width {
                    AvailableSpace::Definite(width) => Some(width),
                    _ => None,
                });
                let layout =
                    Layout::shape(composer.read(cx), &text_style, line_height, width, window);
                size(
                    width.unwrap_or_else(|| layout.width()),
                    line_height * layout.rows().min(MAX_ROWS),
                )
            });
        (layout_id, ())
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
        let line_height = window.line_height();
        // The blink cycle runs only while this line holds focus. Prepaint is
        // the seam that sees the window's focus every drawn frame, so a click
        // that lands anywhere else retires the cycle on that same frame.
        let focused = self.composer.read(cx).focus_handle.is_focused(window);
        let caret_visible = self.composer.update(cx, |composer, cx| {
            composer.sync_caret_blink(focused, cx);
            composer.caret_visible
        });
        let composer = self.composer.read(cx);
        let selected = composer.line.selection();
        let cursor = composer.line.cursor();
        let layout = Layout::shape(
            composer,
            &window.text_style(),
            line_height,
            Some(bounds.size.width),
            window,
        );

        // Scroll only as far as it takes to show the caret's row, and never
        // past the last row.
        let visible = ((bounds.size.height / line_height).round() as usize).max(1);
        let caret_row = layout.row_of(cursor);
        let mut scroll = composer.scroll.min(layout.rows().saturating_sub(visible));
        if caret_row < scroll {
            scroll = caret_row;
        } else if caret_row >= scroll + visible {
            scroll = caret_row + 1 - visible;
        }
        let row_top = |row: usize| bounds.top() + line_height * row - line_height * scroll;

        let (selection, cursor) = if selected.is_empty() && focused && caret_visible {
            let at = layout.position(cursor);
            // The Soft caret: a 2 × 14 `--text-2` bar, square, no radius,
            // centred in its row (y = row top + 3 in the 20px row). It blinks
            // on the standard 500ms cycle while the line holds focus.
            let inset = (line_height - px(crate::theme::CARET_H)) / 2.;
            (
                Vec::new(),
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + at.x, row_top(caret_row) + inset),
                        size(px(crate::theme::CARET_W), px(crate::theme::CARET_H)),
                    ),
                    rgb(crate::theme::TEXT_2),
                )),
            )
        } else {
            // One quad per row the selection crosses, from where it enters
            // the row to where it leaves.
            let quads = layout
                .rows
                .iter()
                .enumerate()
                .filter(|(_, row)| row.start < selected.end && selected.start < row.end)
                .map(|(index, row)| {
                    let from = layout.x_in_row(index, selected.start.max(row.start));
                    let to = layout.x_in_row(index, selected.end.min(row.end));
                    fill(
                        Bounds::from_corners(
                            point(bounds.left() + from, row_top(index)),
                            point(bounds.left() + to, row_top(index) + line_height),
                        ),
                        // One selection colour everywhere, whoever paints it —
                        // opaque `#3f3f3f`, with `TEXT_STRONG` runs over it.
                        rgb(crate::theme::SELECTION),
                    )
                })
                .collect();
            (quads, None)
        };

        PrepaintState {
            layout: Some(layout),
            scroll,
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
                window.focus(&composer.focus_handle, cx);
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
        let layout = prepaint.layout.take().unwrap();
        let scroll = prepaint.scroll;
        let line_height = layout.line_height;
        // Rows scrolled off the top or bottom are clipped, not skipped: the
        // mask is the box, and a hard line is painted whole.
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for quad in prepaint.selection.drain(..) {
                window.paint_quad(quad);
            }
            for (index, line) in layout.lines.iter().enumerate() {
                let first_row = layout.first_row_of_line(index);
                let top = bounds.top() + line_height * first_row - line_height * scroll;
                let rows = line.wrap_boundaries().len() + 1;
                if top + line_height * rows <= bounds.top() || top >= bounds.bottom() {
                    continue;
                }
                line.paint(
                    point(bounds.left(), top),
                    line_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                )
                .unwrap();
            }
            if focus_handle.is_focused(window) {
                if let Some(cursor) = prepaint.cursor.take() {
                    window.paint_quad(cursor);
                }
            }
        });
        self.composer.update(cx, |composer, _cx| {
            composer.last_layout = Some(layout);
            composer.last_bounds = Some(bounds);
            composer.scroll = scroll;
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

    // ------------------------------------------------------- in a window

    use gpui::{KeyBinding, Modifiers, TestAppContext, VisualTestContext};

    /// A stand-in for the Pane: a fixed-width column holding one focused
    /// Composer, answering Submit and HistoryOlder the way the cockpit
    /// does — by taking the draft, and by counting the recall.
    struct Host {
        composer: Entity<Composer>,
        width: Pixels,
        sent: Vec<String>,
        recalls: usize,
    }

    impl Host {
        fn submit(&mut self, _: &crate::cockpit::Submit, _: &mut Window, cx: &mut Context<Self>) {
            let text = self.composer.update(cx, |composer, cx| composer.take(cx));
            self.sent.push(text);
        }

        fn history_older(
            &mut self,
            _: &crate::cockpit::HistoryOlder,
            _: &mut Window,
            _cx: &mut Context<Self>,
        ) {
            self.recalls += 1;
        }
    }

    impl Render for Host {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(self.width)
                .line_height(px(crate::theme::COMPOSER_ROW_H))
                .on_action(cx.listener(Self::submit))
                .on_action(cx.listener(Self::history_older))
                .child(self.composer.clone())
        }
    }

    /// The test text system draws every character 0.6em wide at the
    /// window's 16px default, so a 200px box wraps after ~20 characters.
    const BOX_W: f32 = 200.;

    fn host(cx: &mut TestAppContext) -> (Entity<Host>, &mut VisualTestContext) {
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("enter", crate::cockpit::Submit, None),
                KeyBinding::new("shift-enter", Newline, Some("Composer")),
                KeyBinding::new("left", Left, None),
                KeyBinding::new("home", Home, None),
                // The production order: history first in the table, the
                // row walk after it, so the walk is tried first.
                KeyBinding::new("up", crate::cockpit::HistoryOlder, Some("ComposerHistory")),
                KeyBinding::new("up", Up, Some("Composer")),
                KeyBinding::new("down", Down, Some("Composer")),
            ]);
        });
        let (host, cx) = cx.add_window_view(|window, cx| {
            let composer = cx.new(Composer::new);
            let focus = composer.read(cx).focus_handle.clone();
            window.focus(&focus, cx);
            Host {
                composer,
                width: px(BOX_W),
                sent: Vec::new(),
                recalls: 0,
            }
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        (host, cx)
    }

    fn composer(host: &Entity<Host>, cx: &mut VisualTestContext) -> Entity<Composer> {
        host.read_with(cx, |host, _| host.composer.clone())
    }

    /// The box's painted bounds and row pitch.
    fn geometry(
        composer: &Entity<Composer>,
        cx: &mut VisualTestContext,
    ) -> (Bounds<Pixels>, Pixels) {
        composer.read_with(cx, |composer, _| {
            (
                composer.last_bounds.unwrap(),
                composer.last_layout.as_ref().unwrap().line_height,
            )
        })
    }

    /// A window point `x` pixels into visual row `row` of the box.
    fn at(
        composer: &Entity<Composer>,
        row: usize,
        x: f32,
        cx: &mut VisualTestContext,
    ) -> gpui::Point<Pixels> {
        let (bounds, pitch) = geometry(composer, cx);
        point(
            bounds.left() + px(x),
            bounds.top() + pitch * row + pitch / 2.,
        )
    }

    const LONG: &str = "one two three four five six seven eight nine ten";

    /// The box is one row tall while the text fits and grows a row per
    /// wrapped line, to `MAX_ROWS`; past that it scrolls to the caret.
    #[gpui::test]
    fn the_caret_is_solid_on_focus_then_blinks_and_typing_resets_it(cx: &mut TestAppContext) {
        let (host, cx) = host(cx);
        let composer = composer(&host, cx);
        cx.run_until_parked();

        let visible = |cx: &mut VisualTestContext| composer.read_with(cx, |c, _| c.caret_visible);

        assert!(visible(cx), "focusing the line must show the caret at once");

        cx.executor().advance_clock(BLINK + Duration::from_millis(10));
        cx.run_until_parked();
        assert!(!visible(cx), "the caret must blink off after half a cycle");

        cx.executor().advance_clock(BLINK + Duration::from_millis(10));
        cx.run_until_parked();
        assert!(visible(cx), "the caret must blink back on");

        // Typing while the caret is hidden must bring it straight back.
        cx.executor().advance_clock(BLINK + Duration::from_millis(10));
        cx.run_until_parked();
        assert!(!visible(cx));
        composer.update(cx, |composer, cx| composer.insert("a", cx));
        assert!(visible(cx), "typing must restart the cycle solid");

        // Losing focus leaves the caret solid and retires the cycle.
        cx.update(|window, cx| window.focus(&cx.focus_handle(), cx));
        cx.run_until_parked();
        composer.read_with(cx, |composer, _| {
            assert!(composer.caret_blink.is_none(), "the cycle must retire");
            assert!(composer.caret_visible);
        });

        // And it stays solid: an unfocused line has no caret to blink.
        cx.executor().advance_clock(BLINK * 4);
        cx.run_until_parked();
        composer.read_with(cx, |composer, _| {
            assert!(composer.caret_blink.is_none());
            assert!(composer.caret_visible);
        });
    }

    #[gpui::test]
    fn the_box_grows_with_its_text_and_scrolls_past_eight_rows(cx: &mut TestAppContext) {
        let (host, cx) = host(cx);
        let composer = composer(&host, cx);
        let (bounds, pitch) = geometry(&composer, cx);
        assert_eq!(bounds.size.width, px(BOX_W));
        assert_eq!(pitch, px(crate::theme::COMPOSER_ROW_H));
        assert_eq!(bounds.size.height, pitch, "empty: one row");

        cx.simulate_input("short");
        let (bounds, _) = geometry(&composer, cx);
        assert_eq!(bounds.size.height, pitch);
        composer.read_with(cx, |composer, _| assert_eq!(composer.rows(), 1));

        cx.simulate_input(" and then some more words to wrap");
        let rows = composer.read_with(cx, |composer, _| composer.rows());
        assert!(rows >= 2, "the text wrapped: {rows} rows");
        let (bounds, _) = geometry(&composer, cx);
        assert_eq!(bounds.size.height, pitch * rows, "a row per visual line");

        // Hard breaks count as rows too, and the box stops at MAX_ROWS.
        for _ in 0..12 {
            cx.simulate_keystrokes("shift-enter");
        }
        let (rows, scroll, caret_row) = composer.read_with(cx, |composer, _| {
            (composer.rows(), composer.scroll, composer.caret_row())
        });
        assert!(rows > MAX_ROWS, "{rows} rows");
        let (bounds, _) = geometry(&composer, cx);
        assert_eq!(bounds.size.height, pitch * MAX_ROWS, "clamped");
        assert_eq!(caret_row, rows - 1, "the caret is on the last row");
        assert_eq!(scroll, rows - MAX_ROWS, "scrolled so the caret's row shows");

        // Home takes the caret back to the top, and the view follows it.
        cx.simulate_keystrokes("home");
        composer.read_with(cx, |composer, _| {
            assert_eq!(composer.caret_row(), 0);
            assert_eq!(composer.scroll, 0);
        });
    }

    /// A click on the second row lands the caret there; shift-click and a
    /// drag select across the wrap; a double click still takes one word.
    #[gpui::test]
    fn the_pointer_places_and_selects_across_wrapped_rows(cx: &mut TestAppContext) {
        let (host, cx) = host(cx);
        let composer = composer(&host, cx);
        cx.simulate_input(LONG);
        let rows = composer.read_with(cx, |composer, _| composer.rows());
        assert!(rows >= 2, "the premise: the text wrapped ({rows} rows)");

        let second_row = at(&composer, 1, 2., cx);
        cx.simulate_click(second_row, Modifiers::none());
        let (cursor, row) =
            composer.read_with(cx, |composer, _| (composer.cursor(), composer.caret_row()));
        assert_eq!(row, 1, "the caret is on the second row");
        assert!(cursor > 0 && cursor < LONG.len());
        assert_eq!(
            &LONG[cursor - 1..cursor],
            " ",
            "a row starts after a wrap's space"
        );
        let second_row_start = cursor;

        // Shift-click from the top extends the selection over the wrap.
        let first_row = at(&composer, 0, 2., cx);
        cx.simulate_click(first_row, Modifiers::none());
        let shift_to = at(&composer, 1, 30., cx);
        cx.simulate_event(MouseDownEvent {
            position: shift_to,
            modifiers: Modifiers::shift(),
            button: MouseButton::Left,
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_mouse_up(second_row, MouseButton::Left, Modifiers::none());
        composer.read_with(cx, |composer, _| {
            let selection = composer.line.selection();
            assert_eq!(selection.start, 0);
            assert!(selection.end > second_row_start, "{selection:?}");
        });

        // A drag does the same from a press, following the pointer.
        cx.simulate_click(first_row, Modifiers::none());
        cx.simulate_mouse_down(first_row, MouseButton::Left, Modifiers::none());
        let drag_to = at(&composer, 1, 60., cx);
        cx.simulate_mouse_move(drag_to, MouseButton::Left, Modifiers::none());
        composer.read_with(cx, |composer, _| {
            let selection = composer.line.selection();
            assert_eq!(selection.start, 0);
            assert!(selection.end > second_row_start, "{selection:?}");
            assert!(composer.dragging);
        });
        cx.simulate_mouse_up(drag_to, MouseButton::Left, Modifiers::none());
        composer.read_with(cx, |composer, _| assert!(!composer.dragging));

        // Double click on the second row: that row's word, no more.
        cx.simulate_event(MouseDownEvent {
            position: second_row,
            modifiers: Modifiers::none(),
            button: MouseButton::Left,
            click_count: 2,
            first_mouse: false,
        });
        composer.read_with(cx, |composer, _| {
            let word = composer.line.selected_text().unwrap();
            assert!(LONG.split(' ').any(|w| w == word), "{word:?}");
            assert_eq!(composer.line.selection().start, second_row_start);
        });
        // Triple: everything.
        cx.simulate_event(MouseDownEvent {
            position: second_row,
            modifiers: Modifiers::none(),
            button: MouseButton::Left,
            click_count: 3,
            first_mouse: false,
        });
        composer.read_with(cx, |composer, _| {
            assert_eq!(composer.line.selected_text(), Some(LONG));
        });
    }

    /// shift-enter breaks the line and grows the box; enter still sends,
    /// newline and all.
    #[gpui::test]
    fn shift_enter_breaks_the_line_and_enter_still_submits(cx: &mut TestAppContext) {
        let (host, cx) = host(cx);
        let composer = composer(&host, cx);
        cx.simulate_input("one");
        cx.simulate_keystrokes("shift-enter");
        cx.simulate_input("two");
        composer.read_with(cx, |composer, _| {
            assert_eq!(composer.text(), "one\ntwo");
            assert_eq!(composer.rows(), 2);
            assert_eq!(composer.caret_row(), 1);
        });
        host.read_with(cx, |host, _| assert!(host.sent.is_empty()));
        cx.simulate_keystrokes("enter");
        host.read_with(cx, |host, _| assert_eq!(host.sent, ["one\ntwo"]));
        composer.read_with(cx, |composer, _| {
            assert!(composer.is_empty());
            assert_eq!(composer.rows(), 1);
        });
    }

    /// ↑ walks the rows of a multi-row draft and only recalls history from
    /// the first row; on a one-row draft it recalls at once, as before.
    #[gpui::test]
    fn up_walks_rows_before_it_recalls_history(cx: &mut TestAppContext) {
        let (host, cx) = host(cx);
        let composer = composer(&host, cx);
        composer.update(cx, |composer, cx| composer.set_history_available(true, cx));

        cx.simulate_input("one row");
        cx.simulate_keystrokes("up");
        host.read_with(cx, |host, _| {
            assert_eq!(host.recalls, 1, "one row: history")
        });
        composer.update(cx, |composer, cx| composer.set("".into(), cx));

        cx.simulate_input(LONG);
        let rows = composer.read_with(cx, |composer, _| composer.rows());
        assert!(rows >= 2);
        let end_x = composer.read_with(cx, |composer, _| {
            composer
                .last_layout
                .as_ref()
                .unwrap()
                .position(LONG.len())
                .x
        });
        cx.simulate_keystrokes("up");
        let (row, cursor) =
            composer.read_with(cx, |composer, _| (composer.caret_row(), composer.cursor()));
        assert_eq!(row, rows - 2, "one row up");
        assert!(cursor < LONG.len());
        host.read_with(cx, |host, _| assert_eq!(host.recalls, 1, "no recall yet"));
        // The column is kept, near enough: the caret's x on the row above
        // is the nearest boundary to where it was.
        let x = composer.read_with(cx, |composer, _| {
            composer
                .last_layout
                .as_ref()
                .unwrap()
                .position(composer.cursor())
                .x
        });
        assert!((x - end_x).abs() < px(10.), "x {x:?} vs {end_x:?}");

        for _ in 1..rows - 1 {
            cx.simulate_keystrokes("up");
        }
        composer.read_with(cx, |composer, _| assert_eq!(composer.caret_row(), 0));
        host.read_with(cx, |host, _| assert_eq!(host.recalls, 1));
        cx.simulate_keystrokes("up");
        host.read_with(cx, |host, _| {
            assert_eq!(host.recalls, 2, "first row: history")
        });
        composer.read_with(cx, |composer, _| {
            assert_eq!(composer.text(), LONG, "the draft stands")
        });

        // ↓ walks back down and stops at the last row.
        for _ in 0..rows {
            cx.simulate_keystrokes("down");
        }
        composer.read_with(cx, |composer, _| assert_eq!(composer.caret_row(), rows - 1));
    }
}
