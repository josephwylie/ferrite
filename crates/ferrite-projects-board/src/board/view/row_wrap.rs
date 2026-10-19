//! A row on a narrow screen, wrapped instead of cut: a live task's name and what it says take up to two lines each,
//! and the selected task's name as many as it needs. An automation's row has its trigger under its name there.
use super::fields::{auto_said, fields, is_auto, mark, name_ink, row_title, says_under};
use super::row::{number_lead, number_wide, row, Layout};
use super::row_tall::tall_line;
use crate::board::state::State;
use crate::board::tags::number_cells;
use crate::board::theme::Theme;
use crate::json::*;
use crate::shown::ink::{Ink, Inked};
use crate::shown::queue::queued;
use crate::shown::states::is_live;

/// The most lines a live task's name takes, and the most what it says takes.
const WRAP_LINES: usize = 2;
/// The fewest cells a line of what a task says is worth wrapping in.
const SAY_MIN: usize = 20;
/// A tag wider than this is another machine's long one: what its task says starts right of the mark.
const LONG_TAG: usize = 13;
/// Every mark is one cell wide, in every theme (theme/marks.rs).
const MARK: usize = 1;
fn has_under(r: &Obj) -> bool { is_live(r) || is_auto(r) }
/// Does this row wrap? Live rows, automations and the selected one, on a narrow screen.
pub fn wraps(s: &State, r: &Obj, tall: bool) -> bool { tall && (has_under(r) || r.s("id") == s.sel) }
/// At most `max` lines of a wrapped text: the last one takes what is left and is cut at its end.
fn up_to(lines: Vec<Inked>, max: usize, w: usize) -> Vec<Inked> {
    if lines.len() <= max { return lines; }
    let rest = lines[max - 1..].iter().fold(Inked::new(), |acc, l| if acc.is_empty() { l.clone() } else { acc.add(Inked::plain(" ")).add(l.clone()) });
    lines[..max - 1].iter().cloned().chain(std::iter::once(rest.cut(w))).collect()
}
pub struct Parts { pub indent: usize, pub cells: usize, pub lead: usize, pub name: Vec<Inked>, pub says: Vec<Inked>, pub lone: bool }
/// A wrapping row's name and what it says, in lines without their indent, measured from the width alone.
pub fn wrap_parts(s: &State, r: &Obj, width: usize, cells: Option<usize>) -> Parts {
    let cells = cells.unwrap_or_else(|| number_cells(width, super::row::list_numbering(s, &[r])));
    let lead = width.min(MARK + 2);
    let indent = width.min(lead + number_wide(s, r, cells));
    let room = width.saturating_sub(indent).max(1);
    let title = name_ink(r, &row_title(s, r));
    let name = up_to(title.wrap(room), if r.s("id") == s.sel { usize::MAX } else { WRAP_LINES }, room);
    if !has_under(r) { return Parts { indent, cells, lead, name, says: vec![], lone: false }; }
    let say_lead = if room < SAY_MIN || indent - lead > LONG_TAG { lead } else { indent };
    let say_room = width.saturating_sub(say_lead).max(1);
    let f = fields(s, crate::board::theme::Theme::measure(), r, s.now);
    let said = if is_auto(r) { auto_said(s, r, "all") } else { says_under(s, r, false) };
    // A row with nothing for its last line but a time has the time at the end of what it says instead; a task that
    // waits for a free slot its place in the queue with it (`Q#2  2m`).
    let time = Inked::of(format!("{}{}", f.elapsed, if f.ago { " ago" } else { "" }), Ink::Muted);
    let bare = f.bar.is_empty() && f.eta.is_empty() && f.review.is_empty() && f.pr.is_empty();
    let lone = if is_auto(r) { Inked::of(f.when.clone(), Ink::Muted) } else if bare && f.percent.is_empty() { time } else if bare && queued(r) { f.percent.clone().add(Inked::plain("  ")).add(time) } else { Inked::new() };
    let mut says = up_to(said.wrap(say_room), WRAP_LINES, say_room);
    let lw = lone.width();
    if !lone.is_empty() && is_auto(r) && says.last().map(|l| l.width()).unwrap_or(0) as i64 > say_room as i64 - lw as i64 - 2 { says.push(Inked::new()); }
    if !lone.is_empty() {
        let last = says.len().saturating_sub(1);
        let room2 = say_room.saturating_sub(lw + 2).max(1);
        let line = says.get(last).cloned().unwrap_or_default();
        let line = if line.width() > room2 { line.cut(room2) } else { line };
        let gap = say_room.saturating_sub(line.width() + lw).max(1);
        let joined = line.add(Inked::spaces(gap)).add(lone.clone());
        if says.is_empty() { says.push(joined); } else { says[last] = joined; }
    }
    Parts { indent, cells, lead: say_lead, name, says, lone: !lone.is_empty() }
}
/// The lines a row takes in the list.
pub fn row_height(s: &State, r: &Obj, width: usize, tall: bool) -> usize {
    if !wraps(s, r, tall) { return if is_live(r) { 2 } else { 1 }; }
    let p = wrap_parts(s, r, width, None);
    if has_under(r) { p.name.len() + p.says.len() + if p.lone { 0 } else { 1 } } else { p.name.len() }
}
/// The row as drawn: its name, what it says under it, and for a live task the line of its columns.
pub fn wrapped_lines(s: &State, t: &Theme, r: &Obj, width: usize, lay: &Layout) -> Vec<Inked> {
    let p = wrap_parts(s, r, width, Some(lay.num));
    let first: Vec<Inked> = if p.name.len() > 1 {
        p.name.iter().enumerate().map(|(n, l)| if n > 0 { Inked::spaces(p.indent).add(l.clone()) } else { Inked::plain(" ").add(mark(t, r)).add(Inked::plain(" ")).add(number_lead(s, r, p.cells)).add(l.clone()) }).collect()
    } else { vec![row(s, t, r, width, lay)] };
    if !has_under(r) { return first; }
    let mut out = first;
    out.extend(p.says.iter().map(|l| Inked::spaces(p.lead).add(l.clone())));
    if !p.lone { out.push(tall_line(s, t, r, width, lay)); }
    out
}
