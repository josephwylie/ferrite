//! The terminal's grid as an entity: the mono face on the theme's rows,
//! each cell's colours, a block cursor (blue and blinking while the grid
//! holds the keyboard, a hollow outline while it does not), the scrollback
//! under the wheel, a selection by press and drag (a double press takes a
//! word, a triple a line), copy, paste and select all, and the shell's size
//! following the pane's. The grid takes the keyboard as a text field does:
//! no key of the window's reaches it but the command chords.
//!
//! The cockpit decides what the rest of a press means — the menu, the
//! `Attach as context` pill's act — from what the view emits.

use std::time::Duration;

use alacritty_terminal::event::Event;
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::grid::Dimensions as _;
use alacritty_terminal::index::{Point, Side};
use alacritty_terminal::selection::SelectionType;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor};
use ferrite_ui::theme;
use gpui::prelude::*;
use gpui::{
    actions, div, fill, font, point, px, relative, rgb, size, App, Bounds, ClipboardItem, Context,
    DispatchPhase, Element, ElementId, EventEmitter, FocusHandle, Focusable, GlobalElementId,
    Hsla, InspectorElementId, IntoElement, KeyDownEvent, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Render, ScrollWheelEvent, SharedString, Style,
    TextAlign, TextRun, Window,
};

use crate::palette;
use crate::session::Terminal;

/// How often the view looks at what the shell has said.
const POLL_MS: u64 = 16;
/// Polls per blink phase.
const BLINK_BEATS: u64 = theme::TERMINAL_BLINK_MS / POLL_MS;

/// The grid's key context. `Input` keeps the window's own letter keys
/// (`cockpit::Paste`, page keys) out of the shell's way, as any text field
/// does.
pub const KEY_CONTEXT: &str = "Terminal Input";

actions!(terminal, [Copy, Paste, SelectAll, Clear, Attach, Escape, Tab, ShiftTab, Enter]);

/// The cursor's shape (Claude's `Cursor style`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cursor {
    #[default]
    Block,
    Bar,
    Underline,
}

/// What the `⋯` menu sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub cursor: Cursor,
    pub blink: bool,
    pub copy_on_select: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            cursor: Cursor::Block,
            blink: true,
            copy_on_select: false,
        }
    }
}

/// What the grid tells whoever holds it.
#[derive(Clone, Debug, PartialEq)]
pub enum TerminalEvent {
    /// The shell named its window.
    Title(String),
    /// The shell ended.
    Exited,
    /// `Attach as context`: the selected text.
    Attach(String),
    /// A right-click, at this point of the window.
    ContextMenu(gpui::Point<Pixels>),
}

pub struct TerminalView {
    terminal: Terminal,
    focus: FocusHandle,
    settings: Settings,
    blink_on: bool,
    /// A press landed in the grid and has not been released.
    selecting: bool,
    /// Where a single press landed: a drag makes it a selection's start.
    pending: Option<(Point, Side)>,
    /// Wheel pixels not yet worth a line.
    scroll_rest: Pixels,
    /// The selection's last painted rows, where the `Attach` pill hangs.
    selection_rect: Option<Bounds<Pixels>>,
    /// The grid's own bounds as last painted.
    bounds: Option<Bounds<Pixels>>,
    exited: bool,
}

impl EventEmitter<TerminalEvent> for TerminalView {}

impl TerminalView {
    pub fn new(mut terminal: Terminal, settings: Settings, cx: &mut Context<Self>) -> Self {
        // The view looks at what the shell has said every `POLL_MS` (the
        // emulator's thread cannot wake a window's executor), taking all
        // that has come in as one repaint, and blinks the cursor on the
        // same beat.
        let events = terminal.take_events();
        cx.spawn(async move |this, cx| {
            let mut beats = 0;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(POLL_MS))
                    .await;
                beats += 1;
                let alive = this
                    .update(cx, |view, cx| {
                        let mut changed = false;
                        while let Some(event) = events.as_ref().and_then(|events| events.try_recv().ok()) {
                            view.handle(event, cx);
                            changed = true;
                        }
                        if beats % BLINK_BEATS == 0 && view.settings.blink {
                            view.blink_on = !view.blink_on;
                            changed = true;
                        }
                        if changed {
                            cx.notify();
                        }
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();
        Self {
            terminal,
            focus: cx.focus_handle(),
            settings,
            blink_on: true,
            selecting: false,
            pending: None,
            scroll_rest: px(0.),
            selection_rect: None,
            bounds: None,
            exited: false,
        }
    }

    fn handle(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Wakeup => {}
            Event::Title(title) => cx.emit(TerminalEvent::Title(title)),
            Event::PtyWrite(text) => self.terminal.write(text.into_bytes()),
            Event::ChildExit(_) | Event::Exit => {
                self.exited = true;
                cx.emit(TerminalEvent::Exited);
            }
            _ => {}
        }
    }

    pub fn configure(&mut self, settings: Settings, scrollback: usize, cx: &mut Context<Self>) {
        self.settings = settings;
        self.blink_on = true;
        self.terminal.with(|term| {
            term.set_options(alacritty_terminal::term::Config {
                scrolling_history: scrollback,
                ..Default::default()
            })
        });
        cx.notify();
    }

    /// Whether the shell has ended.
    pub fn has_exited(&self) -> bool {
        self.exited
    }

    /// The selected text, if any.
    pub fn selection(&self) -> Option<String> {
        self.terminal.selected_text()
    }

    /// Copy the selection; false with nothing selected.
    pub fn copy(&mut self, cx: &mut Context<Self>) -> bool {
        match self.terminal.selected_text() {
            Some(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                true
            }
            None => false,
        }
    }

    pub fn paste(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.terminal.scroll(Scroll::Bottom);
            self.terminal.paste(&text);
            cx.notify();
        }
    }

    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        self.terminal.select_all();
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.terminal.clear();
        cx.notify();
    }

    /// `Attach as context`: hand the selection to whoever listens, and
    /// let it go.
    pub fn attach_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.terminal.selected_text() {
            cx.emit(TerminalEvent::Attach(text));
            self.terminal.clear_selection();
            cx.notify();
        }
    }

    /// Bytes typed into the shell.
    pub fn write(&mut self, bytes: &'static [u8]) {
        self.terminal.scroll(Scroll::Bottom);
        self.terminal.write(bytes);
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let mode = self.terminal.with(|term| *term.mode());
        if let Some(bytes) = crate::keys::encode(&event.keystroke, mode) {
            self.terminal.scroll(Scroll::Bottom);
            self.terminal.clear_selection();
            self.terminal.write(bytes);
            self.blink_on = true;
            cx.notify();
        }
    }

    /// The grid cell under `position`, and the side of it, from the last
    /// paint.
    fn cell_at(&self, position: gpui::Point<Pixels>, metrics: &Metrics) -> Option<(Point, Side)> {
        let bounds = self.bounds?;
        let x = (position.x - bounds.left()).max(px(0.));
        let y = (position.y - bounds.top()).max(px(0.));
        let (columns, lines) = self.terminal.size();
        let column = ((x / metrics.cell_width) as usize).min(columns as usize - 1);
        let line = ((y / metrics.line_height) as usize).min(lines as usize - 1);
        let side = if x % metrics.cell_width > metrics.cell_width / 2. {
            Side::Right
        } else {
            Side::Left
        };
        let offset = self.terminal.with(|term| term.grid().display_offset());
        Some((Terminal::point(line, column, offset), side))
    }

    fn press(&mut self, event: &MouseDownEvent, metrics: &Metrics, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        match event.button {
            MouseButton::Left => {
                let Some((at, side)) = self.cell_at(event.position, metrics) else {
                    return;
                };
                let kind = match event.click_count {
                    1 => SelectionType::Simple,
                    2 => SelectionType::Semantic,
                    _ => SelectionType::Lines,
                };
                self.selecting = true;
                self.terminal.clear_selection();
                if event.click_count == 1 {
                    // A bare press selects nothing until it drags.
                    self.pending = Some((at, side));
                } else {
                    self.terminal.select_from(kind, at, side);
                }
                cx.notify();
            }
            MouseButton::Right => cx.emit(TerminalEvent::ContextMenu(event.position)),
            _ => {}
        }
    }
}

/// The cell's size in pixels, as the last paint measured it.
#[derive(Clone, Copy)]
struct Metrics {
    cell_width: Pixels,
    line_height: Pixels,
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus.is_focused(window);
        let pill = (!self.terminal_selection_is_empty())
            .then(|| self.selection_rect.zip(self.bounds))
            .flatten();
        let this = cx.entity();
        div()
            .debug_selector(|| "terminal".into())
            .relative()
            .size_full()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, _, cx| view.key_down(event, cx)))
            .on_action(cx.listener(|view, _: &Copy, _, cx| {
                if !view.copy(cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|view, _: &Paste, _, cx| view.paste(cx)))
            .on_action(cx.listener(|view, _: &SelectAll, _, cx| view.select_all(cx)))
            .on_action(cx.listener(|view, _: &Clear, _, cx| view.clear(cx)))
            .on_action(cx.listener(|view, _: &Escape, _, _| view.write(b"\x1b")))
            .on_action(cx.listener(|view, _: &Attach, _, cx| view.attach_selection(cx)))
            .on_action(cx.listener(|view, _: &Tab, _, _| view.write(b"\t")))
            .on_action(cx.listener(|view, _: &ShiftTab, _, _| view.write(b"\x1b[Z")))
            .on_action(cx.listener(|view, _: &Enter, _, _| view.write(b"\r")))
            .child(TerminalElement { view: this, focused })
            .children(pill.map(|(rect, bounds)| {
                // The pill hangs just over the selection, centred on it.
                let at = rect.center().x - bounds.left();
                let top = rect.top() - bounds.top() - px(theme::TERMINAL_PILL_LIFT);
                div()
                    .debug_selector(|| "terminal-attach".into())
                    .absolute()
                    .left(at)
                    .top(top.max(px(0.)))
                    .px(px(theme::SPACE_2))
                    .h(px(theme::PANEL_LINE_BUTTON_H))
                    .flex()
                    .items_center()
                    .rounded(px(theme::R_CONTROL))
                    .bg(theme::paint::RAISED)
                    .text_size(px(theme::FS_META))
                    .text_color(rgb(theme::INK_STRONG))
                    .font_family(theme::FONT_UI)
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .child("Attach as context")
                    .on_mouse_down(MouseButton::Left, cx.listener(|view, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        view.attach_selection(cx);
                    }))
            }))
    }
}

impl TerminalView {
    fn terminal_selection_is_empty(&self) -> bool {
        self.terminal.with(|term| term.selection.as_ref().is_none_or(|s| s.is_empty()))
    }
}

/// One row's text, its runs, and the backgrounds behind it.
struct Row {
    text: String,
    runs: Vec<TextRun>,
    backgrounds: Vec<(usize, usize, Hsla)>,
}

/// What a frame draws, read under the emulator's lock.
struct Frame {
    rows: Vec<Row>,
    cursor: Option<(usize, usize, char, Hsla)>,
    selection: Vec<(usize, usize, usize)>,
    cursor_shape: CursorShape,
}

struct TerminalElement {
    view: gpui::Entity<TerminalView>,
    focused: bool,
}

impl IntoElement for TerminalElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = (Metrics, Frame, Bounds<Pixels>);

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let pad = point(px(theme::TERMINAL_PAD_X), px(theme::TERMINAL_PAD_Y));
        let grid = Bounds::new(
            bounds.origin + pad,
            size(bounds.size.width - pad.x * 2., bounds.size.height - pad.y),
        );
        let face = font(theme::FONT_CODE);
        let font_size = px(theme::FS_TERMINAL);
        let font_id = window.text_system().resolve_font(&face);
        let cell_width = window
            .text_system()
            .em_advance(font_id, font_size)
            .unwrap_or(px(7.));
        let metrics = Metrics {
            cell_width,
            line_height: px(theme::LH_TERMINAL),
        };
        let columns = ((grid.size.width / cell_width) as u16).max(2);
        let lines = ((grid.size.height / metrics.line_height) as u16).max(1);
        let scheme = self.view.update(cx, |view, _| {
            view.terminal.resize(columns, lines, f32::from(cell_width) as u16, theme::LH_TERMINAL as u16);
            view.bounds = Some(grid);
            view.terminal.with(|term| frame(term, &face))
        });
        (metrics, scheme, grid)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        (metrics, frame, grid): &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (metrics, grid) = (*metrics, *grid);
        let at = |line: usize, column: usize| {
            point(
                grid.left() + metrics.cell_width * column as f32,
                grid.top() + metrics.line_height * line as f32,
            )
        };
        let cell = |line: usize, column: usize, columns: usize| {
            Bounds::new(at(line, column), size(metrics.cell_width * columns as f32, metrics.line_height))
        };
        for (line, from, to) in &frame.selection {
            window.paint_quad(fill(cell(*line, *from, to - from), rgb(theme::TERMINAL_SELECTION)));
        }
        let font_size = px(theme::FS_TERMINAL);
        for (line, row) in frame.rows.iter().enumerate() {
            for (from, to, color) in &row.backgrounds {
                window.paint_quad(fill(cell(line, *from, to - from), *color));
            }
            if row.text.trim().is_empty() {
                continue;
            }
            let shaped = window.text_system().shape_line(
                SharedString::from(row.text.clone()),
                font_size,
                &row.runs,
                None,
            );
            let _ = shaped.paint(at(line, 0), metrics.line_height, TextAlign::Left, None, window, cx);
        }
        // The selection's rows, for the pill.
        let rect = frame
            .selection
            .iter()
            .map(|(line, from, to)| cell(*line, *from, to - from))
            .reduce(|a, b| a.union(&b));
        let blink = self.view.read(cx).blink_on;
        let shape = self.view.read(cx).settings.cursor;
        if let Some((line, column, ch, ink)) = &frame.cursor {
            let block = cell(*line, *column, 1);
            let lit = !self.focused || blink || !self.view.read(cx).settings.blink;
            if frame.cursor_shape != CursorShape::Hidden && lit {
                let cursor = rgb(theme::TERMINAL_CURSOR);
                let edge = px(1.);
                if !self.focused {
                    for side in [
                        Bounds::new(block.origin, size(block.size.width, edge)),
                        Bounds::new(point(block.left(), block.bottom() - edge), size(block.size.width, edge)),
                        Bounds::new(block.origin, size(edge, block.size.height)),
                        Bounds::new(point(block.right() - edge, block.top()), size(edge, block.size.height)),
                    ] {
                        window.paint_quad(fill(side, cursor));
                    }
                } else {
                    let (shape_bounds, over) = match shape {
                        Cursor::Block => (block, true),
                        Cursor::Bar => (Bounds::new(block.origin, size(px(theme::CARET_W), block.size.height)), false),
                        Cursor::Underline => (
                            Bounds::new(point(block.left(), block.bottom() - px(theme::CARET_W)), size(block.size.width, px(theme::CARET_W))),
                            false,
                        ),
                    };
                    window.paint_quad(fill(shape_bounds, cursor));
                    if over && !ch.is_whitespace() {
                        let run = TextRun {
                            len: ch.len_utf8(),
                            font: font(theme::FONT_CODE),
                            color: *ink,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let shaped = window.text_system().shape_line(SharedString::from(ch.to_string()), font_size, &[run], None);
                        let _ = shaped.paint(block.origin, metrics.line_height, TextAlign::Left, None, window, cx);
                    }
                }
            }
        }
        // The pill follows the selection a frame behind it.
        self.view.update(cx, |view, cx| {
            if view.selection_rect != rect {
                view.selection_rect = rect;
                cx.notify();
            }
        });
        self.register_input(bounds, metrics, window);
    }
}

impl TerminalElement {
    /// The pointer's acts inside the grid: a press, a drag, its release,
    /// and the wheel.
    fn register_input(&self, bounds: Bounds<Pixels>, metrics: Metrics, window: &mut Window) {
        let view = self.view.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble && bounds.contains(&event.position) {
                view.update(cx, |view, cx| view.press(event, &metrics, window, cx));
            }
        });
        let view = self.view.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            view.update(cx, |view, cx| {
                if !view.selecting || event.pressed_button != Some(MouseButton::Left) {
                    return;
                }
                if let Some((at, side)) = view.cell_at(event.position, &metrics) {
                    if let Some((from, from_side)) = view.pending.take() {
                        view.terminal.select_from(SelectionType::Simple, from, from_side);
                    }
                    view.terminal.select_to(at, side);
                    cx.notify();
                }
            });
        });
        let view = self.view.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
            if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
                return;
            }
            view.update(cx, |view, cx| {
                if std::mem::take(&mut view.selecting) {
                    view.pending = None;
                    if view.settings.copy_on_select {
                        view.copy(cx);
                    }
                    cx.notify();
                }
            });
        });
        let view = self.view.clone();
        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, _, cx| {
            if phase != DispatchPhase::Bubble || !bounds.contains(&event.position) {
                return;
            }
            view.update(cx, |view, cx| {
                let delta = event.delta.pixel_delta(metrics.line_height).y;
                view.scroll_rest += delta;
                let lines = (view.scroll_rest / metrics.line_height) as i32;
                if lines != 0 {
                    view.scroll_rest -= metrics.line_height * lines as f32;
                    view.terminal.scroll(Scroll::Delta(lines));
                    cx.notify();
                }
            });
        });
    }
}

/// Read the visible grid: each row's text with a run per change of ink and
/// style, the backgrounds, the cursor and the selection's rows.
fn frame(term: &alacritty_terminal::term::Term<crate::session::Listener>, face: &gpui::Font) -> Frame {
    let content = term.renderable_content();
    let (columns, lines) = (term.columns(), term.screen_lines());
    let offset = content.display_offset as i32;
    let mut rows: Vec<Row> = (0..lines)
        .map(|_| Row { text: String::new(), runs: Vec::new(), backgrounds: Vec::new() })
        .collect();
    // The cell most recently drawn in a row, to pad skipped columns.
    let mut filled = vec![0usize; lines];
    for indexed in content.display_iter {
        let line = indexed.point.line.0 + offset;
        let (Ok(line), column) = (usize::try_from(line), indexed.point.column.0) else {
            continue;
        };
        if line >= lines || column >= columns {
            continue;
        }
        let cell = indexed.cell;
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        let row = &mut rows[line];
        // Blank columns the iterator skipped are drawn as spaces.
        for _ in filled[line]..column {
            push(row, ' ', face, palette::foreground(Color::Named(NamedColor::Foreground)), Flags::empty());
        }
        let inverse = cell.flags.contains(Flags::INVERSE);
        let (fg, bg) = if inverse { (cell.bg, cell.fg) } else { (cell.fg, cell.bg) };
        let (mut ink, ground) = (palette::foreground(fg), palette::background(bg).or_else(|| inverse.then(|| palette::foreground(cell.fg))));
        if cell.flags.contains(Flags::DIM) {
            ink.a *= 0.6;
        }
        if cell.flags.contains(Flags::HIDDEN) {
            ink.a = 0.;
        }
        let width = if cell.flags.contains(Flags::WIDE_CHAR) { 2 } else { 1 };
        if let Some(ground) = ground {
            match row.backgrounds.last_mut() {
                Some((_, to, color)) if *to == column && *color == ground => *to += width,
                _ => row.backgrounds.push((column, column + width, ground)),
            }
        }
        push(row, cell.c, face, ink, cell.flags);
        filled[line] = column + width;
    }
    let cursor = {
        let at = content.cursor.point;
        let line = at.line.0 + offset;
        usize::try_from(line).ok().filter(|line| *line < lines).map(|line| {
            let cell = &term.grid()[at];
            (line, at.column.0, cell.c, rgb(theme::TERMINAL_CURSOR_INK).into())
        })
    };
    let mut selection = Vec::new();
    if let Some(range) = content.selection {
        for line in 0..lines as i32 {
            let grid_line = alacritty_terminal::index::Line(line - offset);
            if grid_line < range.start.line || grid_line > range.end.line {
                continue;
            }
            let from = if grid_line == range.start.line || range.is_block { range.start.column.0 } else { 0 };
            let to = if grid_line == range.end.line || range.is_block { range.end.column.0 + 1 } else { columns };
            selection.push((line as usize, from.min(columns), to.min(columns)));
        }
    }
    Frame { rows, cursor, selection, cursor_shape: content.cursor.shape }
}

fn push(row: &mut Row, ch: char, face: &gpui::Font, color: Hsla, flags: Flags) {
    row.text.push(ch);
    let mut font = face.clone();
    if flags.contains(Flags::BOLD) {
        font = font.bold();
    }
    if flags.contains(Flags::ITALIC) {
        font = font.italic();
    }
    let underline = flags.intersects(Flags::ALL_UNDERLINES).then(|| gpui::UnderlineStyle {
        thickness: px(1.),
        color: Some(color),
        wavy: flags.contains(Flags::UNDERCURL),
    });
    let strikethrough = flags.contains(Flags::STRIKEOUT).then(|| gpui::StrikethroughStyle {
        thickness: px(1.),
        color: Some(color),
    });
    let len = ch.len_utf8();
    match row.runs.last_mut() {
        Some(last)
            if last.color == color
                && last.font == font
                && last.underline == underline
                && last.strikethrough == strikethrough =>
        {
            last.len += len
        }
        _ => row.runs.push(TextRun {
            len,
            font,
            color,
            background_color: None,
            underline,
            strikethrough,
        }),
    }
}

