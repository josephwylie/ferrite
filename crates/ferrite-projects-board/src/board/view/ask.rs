//! The one question box, drawn: the question in bold, what it is about quietly under it, and the keys that answer it,
//! always in view and never cut. It takes the text box's place until it is answered, and its closing rule is the
//! screen's last line: no hint line is left empty under it. In a short window the detail gives way first.
use crate::board::keys::key_name;
use crate::board::state::State;
use crate::shown::ink::{Ink, Inked};

/// The most lines of a question's detail the box shows: the task panel has it whole.
const DETAIL: usize = 8;
/// The box in at most `room` lines where it can: the detail goes first, then the rules, then the question's later lines;
/// the keys stay.
pub fn ask_lines(s: &State, width: usize, room_rows: usize) -> Vec<Inked> {
    let Some(a) = &s.ask else { return vec![] };
    let rule = Inked::of("─".repeat(width), Ink::Dim);
    let room = width.saturating_sub(4).max(10);
    // A key that is not one of the answers was pressed: the box says which keys it takes.
    let nudge: Vec<Inked> = if a.nudge { vec![Inked::of(format!("press {}:", a.keys.iter().map(|(k, _)| key_name(k)).collect::<Vec<_>>().join(" or ")), Ink::Yellow)] } else { vec![] };
    let pairs: Vec<Inked> = nudge.iter().cloned().chain(a.keys.iter().map(|(k, d)| Inked::of(key_name(k), Ink::Text).add(Inked::of(format!(" = {d}"), Ink::Muted)))).collect();
    let dot = Inked::of(" · ", Ink::Muted);
    // Where the keys do not fit one line they go on to the next, whole.
    let mut key_lines: Vec<Vec<Inked>> = vec![vec![]];
    for p in pairs {
        let first_line = key_lines.len() == 1;
        let l = key_lines.last().unwrap();
        let sep = if !l.is_empty() && !(!nudge.is_empty() && l.len() == 1 && first_line) { 3 } else if !l.is_empty() { 1 } else { 0 };
        let w: usize = l.iter().map(|x| x.width()).sum();
        if !l.is_empty() && w + sep + p.width() > room { key_lines.push(vec![p]); }
        else { let lead = if sep == 3 { dot.clone() } else if sep == 1 { Inked::plain(" ") } else { Inked::new() }; key_lines.last_mut().unwrap().push(lead.add(p)); }
    }
    let keys: Vec<Inked> = key_lines.into_iter().map(Inked::join).collect();
    let mut q = Inked::plain(a.q.clone()).bold().wrap(room);
    let all: Vec<Inked> = if a.detail.is_empty() { vec![] } else { a.detail.split('\n').flat_map(|l| Inked::plain(crate::shown::words::clean(l)).wrap(room)).collect() };
    let one_line = all.is_empty() && q.len() == 1 && keys.len() == 1 && 3 + q[0].width() + 2 + keys[0].width() <= width;
    let keys_n = if one_line { 0 } else { keys.len() };
    let most = DETAIL.min(room_rows.saturating_sub(2 + q.len() + keys_n));
    let detail: Vec<Inked> = (if all.len() > most { all[..most.saturating_sub(1)].iter().cloned().chain((most > 0).then(|| Inked::plain("…"))).collect() } else { all }).into_iter().map(|l| Inked::plain("   ").add(l.recolor(Ink::Soft))).collect();
    let over = (2 + q.len() + detail.len() + keys_n).saturating_sub(room_rows);
    let (top_rule, end_rule) = (over < 1, over < 2);
    q.truncate(q.len().saturating_sub(over.saturating_sub(2)).max(1));
    let mut out = if top_rule { vec![rule.clone()] } else { vec![] };
    for (n, l) in q.iter().enumerate() {
        let lead = if n > 0 { Inked::plain("   ") } else { Inked::plain(" ").add(Inked::of("?", Ink::Yellow)).add(Inked::plain(" ")) };
        out.push(lead.add(l.clone()).add(if one_line { Inked::plain("  ").add(keys[0].clone()) } else { Inked::new() }));
    }
    out.extend(detail);
    if !one_line { out.extend(keys.into_iter().map(|l| Inked::plain("   ").add(l))); }
    if end_rule { out.push(rule); }
    out
}
