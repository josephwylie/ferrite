//! The screen into cells: each written character at the cells the board measured for it (text.rs), each piece in the
//! colour its ink names in herdr's theme. With colour off (NO_COLOR) nothing is painted: bold, reverse and the marks
//! still say everything. Links are kept with their cells, so the runtime can make them clickable (OSC 8).
use crate::board::theme::Theme;
use crate::shown::ink::{Ink, Inked, Piece};
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier, Style};
use unicode_segmentation::UnicodeSegmentation;

/// A link on the screen: where its words are and where they go.
#[derive(Clone, Debug, PartialEq)]
pub struct Link { pub x: u16, pub y: u16, pub text: String, pub url: String, pub style: Style }
fn style_of(t: &Theme, p: &Piece) -> Style {
    let mut s = Style::default();
    if !t.plain {
        let fg = if p.quiet { t.quiet(p.c) } else { t.ink(if p.c == Ink::Plain { Ink::Text } else { p.c }) };
        if fg.color() != Color::Reset { s = s.fg(fg.color()); }
        if let Some(bg) = p.bg { let c = t.ink(bg).color(); if c != Color::Reset { s = s.bg(c); } }
    }
    if p.bold { s = s.add_modifier(Modifier::BOLD); }
    if p.dim && !t.plain { s = s.add_modifier(Modifier::DIM); }
    if p.under { s = s.add_modifier(Modifier::UNDERLINED); }
    if p.inv { s = s.add_modifier(Modifier::REVERSED); }
    s
}
/// Draw the lines into the buffer, from its top left. Answers the links that were drawn.
pub fn draw(t: &Theme, lines: &[Inked], buf: &mut Buffer) -> Vec<Link> {
    let area = buf.area;
    let mut links = vec![];
    // Whatever the board leaves uncoloured is herdr's text colour on the terminal's own background.
    let base = if t.plain { Style::default() } else { match t.ink(Ink::Text).color() { Color::Reset => Style::default(), c => Style::default().fg(c) } };
    for (y, line) in lines.iter().enumerate().take(area.height as usize) {
        let y = area.y + y as u16;
        let mut x = area.x;
        let right = area.x + area.width;
        // A line's background (the selected row, the side panel) runs to its end only where its own pieces have one.
        for p in &line.0 {
            let st = style_of(t, p);
            let mut link_text = String::new();
            let start = x;
            for g in p.t.graphemes(true) {
                let w = crate::text::grapheme_width(g) as u16;
                if w == 0 { continue; }
                if x + w > right { break; }
                let cell = &mut buf[(x, y)];
                cell.set_symbol(g).set_style(base.patch(st));
                for k in 1..w { buf[(x + k, y)].set_symbol("").set_style(base.patch(st)); }
                if p.link.is_some() { link_text.push_str(g); }
                x += w;
            }
            if let Some(url) = &p.link { if !link_text.is_empty() { links.push(Link { x: start, y, text: link_text, url: url.clone(), style: base.patch(st) }); } }
        }
        while x < right { buf[(x, y)].set_symbol(" ").set_style(base); x += 1; }
    }
    links
}
/// The screen as text, one line per row: what a snapshot of a frame compares. `colour`: each piece's ink by name.
pub fn text_of(lines: &[Inked], colour: bool) -> String {
    lines.iter().map(|l| if colour { l.0.iter().map(|p| if p.t.trim().is_empty() || p.c == Ink::Plain && !p.bold && !p.inv && p.bg.is_none() { p.t.clone() } else { format!("[{}{}{}{}|{}]", p.c.name(), if p.bold { "+b" } else { "" }, if p.inv { "+r" } else { "" }, p.bg.map(|b| format!("@{}", b.name())).unwrap_or_default(), p.t) }).collect::<String>() } else { l.text() }.trim_end().to_string()).collect::<Vec<_>>().join("\n")
}
