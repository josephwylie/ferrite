//! The mouse on the board (ADR 0031, Seam 3): upstream's terminal runtime
//! (`board/run/mouse.rs`) for GPUI. The wheel and a click go to the board's
//! `update` (`update/mouse.rs` decides what they do: the chat's message under
//! the pointer scrolls, elsewhere the wheel moves as ↑↓, a click selects a row
//! or folds a heading). Dragging selects the board's own text, line by line,
//! and letting go copies it to the clipboard (where the terminal used OSC 52);
//! a double click selects and copies a word. A double click is upstream's: a
//! second press on the same cell within 400 ms, not the system's click count,
//! which also counts a quick second click a few pixels away.
//!
//! A click on a link's words opens it (`links.rs`) instead.
//!
//! The pointer says where a click lands, as every list in Ferrite does
//! (`pointer.rs`): a row a click would select, or a heading it would fold,
//! takes the row role's hover wash, blended under its words, and the
//! pointing hand; so do a link's words. The selected row, a row while a
//! draft locks who gets it, and every screen a click does nothing on stay
//! as they are.
//!
//! `Selecting` works on cells of the drawn screen (column, line); `Pointer`
//! keeps it with the screen's links and where its cells are, and `listen`
//! turns GPUI's mouse into cells for it.

use std::cell::Cell as Shared;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    canvas, div, px, AnyElement, Bounds, Context, Div, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, ScrollWheelEvent, SharedString,
};
use sw_board::board::state::{Mode, State};
use sw_board::board::update::Event;
use sw_board::shown::ink::{Ink, Inked, Piece};
use unicode_segmentation::UnicodeSegmentation;

use super::links::{self, Link};
use super::BoardView;
use crate::pointer::Pointer as _;
use crate::theme::{CH, ROW};

/// A cell of the screen: its column and its line.
pub type Cell = (usize, usize);

/// What a press or a release comes to.
#[derive(Debug, PartialEq, Eq)]
pub enum Did {
    /// A click on this cell, for the board (a link under it opens instead).
    Click(Cell),
    /// These words were selected, for the clipboard.
    Copy(String),
    Nothing,
}

/// The selection, over the screen as last drawn.
#[derive(Default)]
pub struct Selecting {
    /// The screen's cells, a wide character's second cell empty.
    grid: Vec<Vec<String>>,
    start: Option<Cell>,
    end: Option<Cell>,
    /// The button went down on the board and is not let go yet.
    pressed: bool,
    dragging: bool,
    shown: bool,
    /// The wheel's movement not yet a whole step, in steps (down positive).
    wheel: f32,
    /// The last press: when (the board's clock, ms) and on which cell.
    last: Option<(i64, Cell)>,
}

/// The most between the presses of a double click, in ms.
const DOUBLE_MS: i64 = 400;

impl Selecting {
    /// What the screen holds, for a copy: each character at the cells the
    /// drawer gives it.
    pub fn capture(&mut self, lines: &[Inked]) {
        self.grid = lines
            .iter()
            .map(|line| {
                let mut row = vec![];
                for g in line.text().graphemes(true) {
                    let w = sw_board::text::grapheme_width(g);
                    if w == 0 {
                        continue;
                    }
                    row.push(g.to_string());
                    row.extend(std::iter::repeat_n(String::new(), w - 1));
                }
                row
            })
            .collect();
    }

    fn range(&self) -> Option<(Cell, Cell)> {
        let (a, b) = (self.start?, self.end?);
        Some(if (a.1, a.0) <= (b.1, b.0) {
            (a, b)
        } else {
            (b, a)
        })
    }

    /// Whether a cell is drawn selected (in reverse).
    pub fn selected(&self, x: usize, y: usize) -> bool {
        let Some((a, b)) = self.range().filter(|_| self.shown) else {
            return false;
        };
        (y > a.1 || (y == a.1 && x >= a.0)) && (y < b.1 || (y == b.1 && x <= b.0))
    }

    /// The screen's lines with what is selected drawn in reverse, as the
    /// terminal runtime paints its selection over the frame.
    pub fn paint(&self, lines: Vec<Inked>) -> Vec<Inked> {
        if !self.shown {
            return lines;
        }
        lines
            .into_iter()
            .enumerate()
            .map(|(y, line)| {
                let mut out = Inked::new();
                let mut x = 0;
                for mut style in line.0 {
                    let words = std::mem::take(&mut style.t);
                    for g in words.graphemes(true) {
                        let inv = style.inv || self.selected(x, y);
                        x += sw_board::text::grapheme_width(g);
                        let cell = Piece {
                            t: g.to_string(),
                            inv,
                            ..style.clone()
                        };
                        match out.0.last_mut() {
                            Some(last) if same_style(last, &cell) => last.t.push_str(g),
                            _ => out.0.push(cell),
                        }
                    }
                }
                out
            })
            .collect()
    }

    /// The selected words, each line without the spaces at its end.
    fn text(&self) -> String {
        let Some((a, b)) = self.range() else {
            return String::new();
        };
        (a.1..=b.1)
            .map(|y| {
                let row = self.grid.get(y).map(Vec::as_slice).unwrap_or_default();
                let from = if y == a.1 { a.0 } else { 0 }.min(row.len());
                let to = if y == b.1 {
                    (b.0 + 1).min(row.len())
                } else {
                    row.len()
                };
                row[from..to.max(from)].concat().trim_end().to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The word under a cell: its run of characters that are neither space
    /// nor the board's frames and rules.
    fn word_at(&mut self, (x, y): Cell) -> bool {
        let row = self.grid.get(y).cloned().unwrap_or_default();
        let word = |c: &str| !c.trim().is_empty() && !c.chars().all(|ch| "│─╭╮╰╯▌·".contains(ch));
        // A wide character's second cell belongs to it.
        let mut x = x;
        while x > 0 && row.get(x).is_some_and(|c| c.is_empty()) {
            x -= 1;
        }
        if !row.get(x).is_some_and(|c| word(c)) {
            return false;
        }
        let part = |c: &String| c.is_empty() || word(c);
        let (mut a, mut b) = (x, x);
        while a > 0 && part(&row[a - 1]) {
            a -= 1;
        }
        while b + 1 < row.len() && part(&row[b + 1]) {
            b += 1;
        }
        self.start = Some((a, y));
        self.end = Some((b, y));
        self.shown = true;
        true
    }

    /// The left button pressed at `now` (ms): a second press on the same cell
    /// within 400 ms is a double click, which selects the word under it and
    /// copies it; one press starts a selection, or a click if nothing is
    /// dragged.
    pub fn down(&mut self, at: Cell, now: i64) -> Did {
        let double = self
            .last
            .is_some_and(|(then, cell)| cell == at && now - then < DOUBLE_MS);
        self.last = Some((now, at));
        self.shown = false;
        self.dragging = false;
        self.pressed = !double;
        if double {
            return if self.word_at(at) {
                copy_of(self.text())
            } else {
                Did::Nothing
            };
        }
        self.start = Some(at);
        self.end = Some(at);
        Did::Nothing
    }

    /// The pointer moved with the button held: the selection follows it.
    /// Answers whether what is selected changed.
    pub fn drag(&mut self, at: Cell) -> bool {
        if !self.pressed || self.end == Some(at) && self.dragging {
            return false;
        }
        self.end = Some(at);
        self.dragging = true;
        self.shown = true;
        true
    }

    /// The button let go: a drag copies what it selected, a press that did
    /// not move is a click (one after a double click, or off a press
    /// elsewhere, is nothing).
    pub fn up(&mut self, at: Cell) -> Did {
        if !std::mem::take(&mut self.pressed) {
            return Did::Nothing;
        }
        if self.dragging {
            self.dragging = false;
            self.end = Some(at);
            return copy_of(self.text());
        }
        Did::Click(at)
    }

    /// The wheel over a cell, moved by `steps` (a line of movement is one
    /// step; down positive): one wheel event per whole step, as a terminal
    /// sends one per notch, so a faster swipe moves further.
    pub fn wheel(&mut self, (x, y): Cell, steps: f32) -> Vec<Event> {
        if self.wheel.signum() != steps.signum() {
            self.wheel = 0.;
        }
        self.wheel += steps;
        let whole = self.wheel.trunc();
        self.wheel -= whole;
        let by = whole.signum() as i64;
        let cell = |n: usize| n.min(u16::MAX as usize) as u16;
        (0..whole.abs() as usize)
            .map(|_| Event::Wheel {
                x: cell(x),
                y: cell(y),
                by,
            })
            .collect()
    }
}

/// A row of the list a click acts on: its id, its first line on the
/// screen and how many lines it takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Band {
    pub id: String,
    pub y: usize,
    pub lines: usize,
}

/// Whether a click on the list does anything now: `update/mouse.rs`'s
/// `click`, which ignores it on every other screen and while a draft
/// locks who it is for.
fn takes_clicks(s: &State) -> bool {
    let busy = s.leaving
        || s.setup.is_some()
        || s.ask.is_some()
        || !s.paste_text.is_empty()
        || s.menu.is_some()
        || s.full
        || s.auto_open;
    let locked = s.mode == Mode::New && !s.value().trim().is_empty();
    !busy && !locked
}

/// The list's rows a click would act on, as the screen last laid them out
/// (`list_at`, `list_ids`), with the columns they span: every heading (a
/// click folds it, selected or not) and every row but the selected one,
/// where a click does nothing more.
pub fn bands(s: &State) -> (usize, Vec<Band>) {
    let Some((top, wide)) = s.list_at.filter(|_| takes_clicks(s)) else {
        return (0, Vec::new());
    };
    let mut out: Vec<Band> = Vec::new();
    for (at, id) in s.list_ids.iter().enumerate() {
        if id.is_empty() || (*id == s.sel && !sw_board::board::update::heads::is_head(id)) {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.id == *id && last.y + last.lines == top + at => last.lines += 1,
            _ => out.push(Band {
                id: id.clone(),
                y: top + at,
                lines: 1,
            }),
        }
    }
    (wide, out)
}

/// Two pieces drawn alike: everything but their words the same.
fn same_style(a: &Piece, b: &Piece) -> bool {
    Piece {
        t: String::new(),
        ..a.clone()
    } == Piece {
        t: String::new(),
        ..b.clone()
    }
}

fn copy_of(text: String) -> Did {
    if text.is_empty() {
        Did::Nothing
    } else {
        Did::Copy(text)
    }
}

/// Words a drag or a double click selected, on the clipboard, and the board
/// says how many characters went: upstream's "Copied N characters.".
fn copy(s: &mut State, text: &str, cx: &mut gpui::App) {
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.to_string()));
    s.flash(copied(text.chars().count()), Ink::Muted);
}

/// The mouse's side of the board view: the selection, the links of the
/// screen last drawn, and where its cells are on the window.
#[derive(Default)]
pub(crate) struct Pointer {
    selecting: Selecting,
    links: Vec<Link>,
    /// The cells' box, as last painted (`layer`).
    bounds: Rc<Shared<Option<Bounds<Pixels>>>>,
}

impl Pointer {
    /// The lines about to be drawn: kept for a copy and their links, and
    /// answered with what is selected in reverse.
    pub(crate) fn frame(&mut self, lines: Vec<Inked>) -> Vec<Inked> {
        self.links = links::links(&lines);
        self.selecting.capture(&lines);
        self.selecting.paint(lines)
    }

    /// An empty layer over the cells that notes where they are on the
    /// window as it is painted.
    pub(crate) fn layer(&self) -> impl IntoElement {
        let bounds = self.bounds.clone();
        canvas(move |at, _, _| bounds.set(Some(at)), |_, _, _, _| {})
            .absolute()
            .size_full()
    }

    /// What lies under the board's words and says where a click lands:
    /// each row a click acts on, washed under the pointer (the row role),
    /// and each link's words, the pointing hand. Drawn before the lines,
    /// so the wash sits under their words.
    pub(crate) fn grounds(&self, s: &State) -> Vec<AnyElement> {
        let at = |x: usize, y: usize, width: usize, lines: usize| {
            div()
                .absolute()
                .left(px(x as f32 * CH))
                .top(px(y as f32 * ROW))
                .w(px(width as f32 * CH))
                .h(px(lines as f32 * ROW))
        };
        let (wide, bands) = bands(s);
        let rows = bands.into_iter().map(|band| {
            let id = SharedString::from(format!("board-row-{}", band.id));
            let key = SharedString::from(format!("hover:{id}"));
            let selector = id.clone();
            at(0, band.y, wide, band.lines)
                .id(id)
                .debug_selector(move || selector.to_string())
                .hover_row(key)
                .into_any_element()
        });
        let links = self.links.iter().enumerate().map(|(n, link)| {
            at(link.x, link.y, link.width, 1)
                .id(("board-link", n))
                .cursor_pointer()
                .into_any_element()
        });
        rows.chain(links).collect()
    }

    /// The cell under a point of the window: one off the board is the
    /// nearest edge cell, so a drag that leaves it still selects to an end.
    fn cell(&self, at: Point<Pixels>) -> Option<Cell> {
        let origin = self.bounds.get()?.origin;
        let along = |by: Pixels, size: f32| (f32::from(by) / size).floor().max(0.) as usize;
        Some((along(at.x - origin.x, CH), along(at.y - origin.y, ROW)))
    }
}

/// The board view's mouse: press, drag, release and the wheel, each turned
/// into its cell for `Selecting`, and what that comes to done.
pub(crate) fn listen(surface: Div, cx: &mut Context<BoardView>) -> Div {
    surface
        .on_mouse_down(MouseButton::Left, cx.listener(BoardView::mouse_down))
        .on_mouse_move(cx.listener(BoardView::mouse_drag))
        .on_mouse_up(MouseButton::Left, cx.listener(BoardView::mouse_up))
        .on_mouse_up_out(MouseButton::Left, cx.listener(BoardView::mouse_up))
        .on_scroll_wheel(cx.listener(BoardView::wheel))
}

impl BoardView {
    fn mouse_down(&mut self, event: &MouseDownEvent, _: &mut gpui::Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let Some(at) = self.pointer.cell(event.position) else {
            return;
        };
        let did = self.pointer.selecting.down(at, sw_board::clock::now());
        self.did(did, cx);
    }

    fn mouse_drag(&mut self, event: &MouseMoveEvent, _: &mut gpui::Window, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let Some(at) = self.pointer.cell(event.position) else {
            return;
        };
        if self.pointer.selecting.drag(at) {
            cx.notify();
        }
    }

    fn mouse_up(&mut self, event: &MouseUpEvent, _: &mut gpui::Window, cx: &mut Context<Self>) {
        let Some(at) = self.pointer.cell(event.position) else {
            return;
        };
        let did = self.pointer.selecting.up(at);
        self.did(did, cx);
    }

    fn wheel(&mut self, event: &ScrollWheelEvent, _: &mut gpui::Window, cx: &mut Context<Self>) {
        let Some(at) = self.pointer.cell(event.position) else {
            return;
        };
        cx.stop_propagation();
        // Content moving down is the wheel turned up: upstream's `by` -1.
        let steps = -f32::from(event.delta.pixel_delta(px(ROW)).y) / ROW;
        for step in self.pointer.selecting.wheel(at, steps) {
            self.feed(step, cx);
        }
    }

    /// A click opens the link under it, else goes to the board; selected
    /// words go to the clipboard.
    fn did(&mut self, did: Did, cx: &mut Context<Self>) {
        match did {
            Did::Click((x, y)) => match links::at(&self.pointer.links, x, y) {
                Some(link) => links::open(&link.url.clone(), cx),
                None => {
                    let cell = |n: usize| n.min(u16::MAX as usize) as u16;
                    self.feed(
                        Event::Click {
                            x: cell(x),
                            y: cell(y),
                        },
                        cx,
                    );
                }
            },
            Did::Copy(text) => {
                if let Some(state) = self.state() {
                    copy(state, &text, cx);
                }
            }
            Did::Nothing => {}
        }
        cx.notify();
    }
}

fn copied(n: usize) -> String {
    format!("Copied {n} character{}.", if n == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(lines: &[&str]) -> Selecting {
        let mut s = Selecting::default();
        s.capture(&lines.iter().map(|l| Inked::plain(*l)).collect::<Vec<_>>());
        s
    }

    /// Two presses on one cell, 100 ms apart, a second after the last press.
    fn double(s: &mut Selecting, at: Cell) -> Did {
        let now = s.last.map_or(0, |(then, _)| then + 1000);
        s.down(at, now);
        s.down(at, now + 100)
    }

    #[test]
    fn a_press_and_release_on_one_cell_is_a_click() {
        let mut s = screen(&["  ● #12 Fix the title"]);
        assert_eq!(s.down((4, 0), 0), Did::Nothing);
        assert_eq!(s.up((4, 0)), Did::Click((4, 0)));
        assert!(!s.selected(4, 0), "a click selects no text");
        assert_eq!(
            s.up((4, 0)),
            Did::Nothing,
            "a release with no press on the board is nothing"
        );
        assert!(
            !s.drag((6, 0)),
            "nor is a move with the button down from elsewhere"
        );
    }

    #[test]
    fn a_drag_selects_line_by_line_and_letting_go_copies_without_trailing_spaces() {
        let mut s = screen(&[
            " herdr-projects  repo · 2 to do   ",
            "  ○ #1 First task    ",
            "  ○ #2 Second",
        ]);
        s.down((17, 0), 0);
        assert!(s.drag((5, 1)));
        assert!(s.selected(30, 0) && s.selected(0, 1) && s.selected(5, 1));
        assert!(!s.selected(16, 0) && !s.selected(6, 1));
        assert_eq!(
            s.up((8, 2)),
            Did::Copy("repo · 2 to do\n  ○ #1 First task\n  ○ #2 Se".into())
        );
        // Dragged upward, the same words.
        let mut s = screen(&["one two", "three four"]);
        s.down((4, 1), 0);
        s.drag((4, 0));
        assert_eq!(s.up((4, 0)), Did::Copy("two\nthree".into()));
    }

    #[test]
    fn a_double_click_copies_a_word_and_never_the_frames_around_it() {
        let mut s = screen(&["│ ❯ fix─the│title │"]);
        assert_eq!(double(&mut s, (6, 0)), Did::Copy("fix".into()));
        assert!(s.selected(4, 0) && s.selected(6, 0) && !s.selected(7, 0));
        assert_eq!(
            s.up((6, 0)),
            Did::Nothing,
            "the release after a double click is no click"
        );
        assert_eq!(double(&mut s, (13, 0)), Did::Copy("title".into()));
        assert_eq!(double(&mut s, (11, 0)), Did::Nothing, "a rule is no word");
        assert_eq!(double(&mut s, (0, 0)), Did::Nothing, "a frame is no word");
        assert_eq!(
            double(&mut s, (3, 1)),
            Did::Nothing,
            "below the screen there is nothing"
        );
    }

    #[test]
    fn what_is_selected_is_drawn_in_reverse_and_keeps_its_inks() {
        let mut s = Selecting::default();
        let lines = vec![Inked::plain(" ")
            .add(Inked::of("PR#60", Ink::Green))
            .add(Inked::plain(" ok"))];
        s.capture(&lines);
        assert_eq!(
            s.paint(lines.clone()),
            lines,
            "nothing selected, nothing changes"
        );
        s.down((3, 0), 0);
        s.drag((7, 0));
        let painted = s.paint(lines);
        let parts: Vec<(&str, Ink, bool)> = painted[0]
            .0
            .iter()
            .map(|p| (p.t.as_str(), p.c, p.inv))
            .collect();
        assert_eq!(
            parts,
            [
                (" ", Ink::Plain, false),
                ("PR", Ink::Green, false),
                ("#60", Ink::Green, true),
                (" o", Ink::Plain, true),
                ("k", Ink::Plain, false)
            ]
        );
    }

    #[test]
    fn a_double_click_is_two_presses_on_one_cell_within_400_ms() {
        let mut s = screen(&["  ● #12 Fix the title"]);
        s.down((9, 0), 0);
        assert_eq!(
            s.down((10, 0), 100),
            Did::Nothing,
            "a quick press on the next cell is a press of its own"
        );
        assert_eq!(
            s.down((10, 0), 500),
            Did::Nothing,
            "nor is one 400 ms after the last"
        );
        assert_eq!(s.down((10, 0), 899), Did::Copy("Fix".into()));
    }

    #[test]
    fn a_double_click_on_a_wide_character_copies_the_whole_word() {
        let mut s = screen(&["  日本語 x"]);
        assert_eq!(double(&mut s, (5, 0)), Did::Copy("日本語".into()));
    }

    #[test]
    fn the_wheel_sends_one_step_per_whole_line_and_keeps_the_rest() {
        let mut s = Selecting::default();
        assert!(s.wheel((3, 7), 0.5).is_empty());
        let steps = s.wheel((3, 7), 2.6);
        assert_eq!(steps.len(), 3);
        assert!(steps
            .iter()
            .all(|e| matches!(e, Event::Wheel { x: 3, y: 7, by: 1 })));
        // Turning back starts over: what was left of the other way is dropped.
        let up = s.wheel((3, 7), -1.2);
        assert!(matches!(up.as_slice(), [Event::Wheel { by: -1, .. }]));
    }

    #[test]
    fn the_copied_message_counts_characters() {
        assert_eq!(copied(1), "Copied 1 character.");
        assert_eq!(copied(12), "Copied 12 characters.");
    }
}
