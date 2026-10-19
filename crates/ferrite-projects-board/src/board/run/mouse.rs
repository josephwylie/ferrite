//! The mouse in a terminal: the wheel and a click go to the board (update/mouse.rs); dragging selects text on the
//! board's own screen, line by line, and letting go copies it (OSC 52), as pi-tui's selection did: a program that asks
//! for the mouse gets all of it, so the board selects itself. A double click selects a word. Links are written over
//! their cells as terminal links (OSC 8), which the terminal opens.
use super::draw::Link;
use crate::board::update::Event;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;
use std::io::Write;

pub enum Did { Event(Event), Copied(usize), Nothing }
#[derive(Default)]
pub struct Selecting { grid: Vec<Vec<String>>, start: Option<(u16, u16)>, end: Option<(u16, u16)>, dragging: bool, shown: bool, last: (i64, u16, u16) }
impl Selecting {
    /// What the screen holds, for a copy.
    pub fn capture(&mut self, buf: &Buffer) {
        let a = buf.area;
        self.grid = (0..a.height).map(|y| (0..a.width).map(|x| buf[(a.x + x, a.y + y)].symbol().to_string()).collect()).collect();
    }
    fn range(&self) -> Option<((u16, u16), (u16, u16))> {
        let (a, b) = (self.start?, self.end?);
        Some(if (a.1, a.0) <= (b.1, b.0) { (a, b) } else { (b, a) })
    }
    fn inside(&self, x: u16, y: u16) -> bool {
        let Some((a, b)) = self.range() else { return false };
        (y > a.1 || (y == a.1 && x >= a.0)) && (y < b.1 || (y == b.1 && x <= b.0))
    }
    /// The highlight, drawn by the board in reverse over what is selected.
    pub fn paint(&self, buf: &mut Buffer) {
        if !self.shown { return; }
        let a = buf.area;
        for y in 0..a.height { for x in 0..a.width { if self.inside(x, y) { let c = &mut buf[(a.x + x, a.y + y)]; let st = c.style().add_modifier(Modifier::REVERSED); c.set_style(st); } } }
    }
    fn text(&self) -> String {
        let Some((a, b)) = self.range() else { return String::new() };
        let mut out = vec![];
        for y in a.1..=b.1 {
            let row = self.grid.get(y as usize).cloned().unwrap_or_default();
            let (from, to) = (if y == a.1 { a.0 as usize } else { 0 }, if y == b.1 { (b.0 as usize + 1).min(row.len()) } else { row.len() });
            out.push(row.get(from..to.max(from)).map(|c| c.concat()).unwrap_or_default().trim_end().to_string());
        }
        out.join("\n")
    }
    fn copy(&self, text: &str) -> usize {
        if text.is_empty() { return 0; }
        let _ = std::io::stdout().write_all(format!("\x1b]52;c;{}\x07", crate::util::base64(text.as_bytes())).as_bytes());
        let _ = std::io::stdout().flush();
        text.chars().count()
    }
    fn word_at(&mut self, x: u16, y: u16) {
        let row = self.grid.get(y as usize).cloned().unwrap_or_default();
        let word = |c: &str| !c.trim().is_empty() && !c.chars().all(|ch| "│─╭╮╰╯▌·".contains(ch));
        if !row.get(x as usize).map(|c| word(c)).unwrap_or(false) { return; }
        let (mut a, mut b) = (x as usize, x as usize);
        while a > 0 && word(&row[a - 1]) { a -= 1; }
        while b + 1 < row.len() && word(&row[b + 1]) { b += 1; }
        self.start = Some((a as u16, y)); self.end = Some((b as u16, y)); self.shown = true;
    }
    pub fn mouse(&mut self, m: &MouseEvent, now: i64) -> Did {
        let (x, y) = (m.column, m.row);
        match m.kind {
            MouseEventKind::ScrollUp => Did::Event(Event::Wheel { x, y, by: -1 }),
            MouseEventKind::ScrollDown => Did::Event(Event::Wheel { x, y, by: 1 }),
            MouseEventKind::Down(MouseButton::Left) => {
                let double = now - self.last.0 < 400 && self.last.1 == x && self.last.2 == y;
                self.last = (now, x, y);
                self.shown = false; self.dragging = false;
                if double { self.word_at(x, y); let n = self.copy(&self.text()); return if n > 0 { Did::Copied(n) } else { Did::Nothing }; }
                self.start = Some((x, y)); self.end = Some((x, y));
                Did::Nothing
            }
            MouseEventKind::Drag(MouseButton::Left) => { if self.start.is_some() { self.end = Some((x, y)); self.dragging = true; self.shown = true; } Did::Nothing }
            MouseEventKind::Up(MouseButton::Left) => {
                if self.dragging { self.dragging = false; self.end = Some((x, y)); let n = self.copy(&self.text()); return if n > 0 { Did::Copied(n) } else { Did::Nothing }; }
                if self.shown { return Did::Nothing; }
                Did::Event(Event::Click { x, y })
            }
            _ => Did::Nothing,
        }
    }
}
/// The links of the frame, written over their own cells as terminal links.
pub fn write_links(links: &[Link]) {
    if links.is_empty() { return; }
    let mut out = std::io::stdout();
    for l in links {
        let _ = crossterm::queue!(out, crossterm::cursor::MoveTo(l.x, l.y), crossterm::style::SetStyle(crossterm::style::ContentStyle { foreground_color: l.style.fg.and_then(ce_color), background_color: l.style.bg.and_then(ce_color), underline_color: None, attributes: attrs(l.style.add_modifier) }));
        let _ = write!(out, "\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\", l.url, l.text);
        let _ = crossterm::queue!(out, crossterm::style::ResetColor, crossterm::style::SetAttribute(crossterm::style::Attribute::Reset));
    }
    let _ = out.flush();
}
fn ce_color(c: ratatui::style::Color) -> Option<crossterm::style::Color> {
    use ratatui::style::Color as R;
    use crossterm::style::Color as C;
    Some(match c { R::Reset => return None, R::Rgb(r, g, b) => C::Rgb { r, g, b }, R::Indexed(n) => C::AnsiValue(n), R::Black => C::Black, R::Red => C::DarkRed, R::Green => C::DarkGreen, R::Yellow => C::DarkYellow, R::Blue => C::DarkBlue, R::Magenta => C::DarkMagenta, R::Cyan => C::DarkCyan, R::Gray => C::Grey, R::DarkGray => C::DarkGrey, R::LightRed => C::Red, R::LightGreen => C::Green, R::LightYellow => C::Yellow, R::LightBlue => C::Blue, R::LightMagenta => C::Magenta, R::LightCyan => C::Cyan, R::White => C::White })
}
fn attrs(m: Modifier) -> crossterm::style::Attributes {
    use crossterm::style::Attribute as A;
    let mut a = crossterm::style::Attributes::default();
    if m.contains(Modifier::BOLD) { a.set(A::Bold); }
    if m.contains(Modifier::UNDERLINED) { a.set(A::Underlined); }
    if m.contains(Modifier::REVERSED) { a.set(A::Reverse); }
    if m.contains(Modifier::DIM) { a.set(A::Dim); }
    a
}
