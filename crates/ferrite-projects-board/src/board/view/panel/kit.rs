//! What every part of the task panel is drawn with: wrapped text, headlines, label-and-value rows, and the list of
//! parts with what each gives up where room is short. Each line is marked as what it is (a headline, a fact, the line
//! that goes on under one, text), so a page can lay the same panel out by itself.
use crate::shown::ink::{Ink, Inked};

/// Where every value starts: one column for the whole panel, so the labels of every part line up under each other.
pub const LABEL: usize = 13;
type At = Box<dyn Fn(usize) -> Vec<Inked>>;
/// One part of the panel and how it gives way: `at(n)` its lines in at most n; `pri` the order flexible parts give up
/// lines in; `min` the fewest it keeps while others still can give; `last` its last shape's height (kept longest);
/// `alert` what it needs from you, which gives up lines only after every setting has gone.
pub struct Part { pub lines: Vec<Inked>, pub at: Option<At>, pub pri: i32, pub min: usize, pub last: Option<usize>, pub alert: Option<At>, pub failed: bool, pub name: &'static str }
impl Part { pub fn of(lines: Vec<Inked>) -> Part { Part { lines, at: None, pri: 0, min: 0, last: None, alert: None, failed: false, name: "" } } }
pub struct Kit { pub w: usize, pub room: usize, pub stacked: bool, pub indent: usize, pub parts: Vec<Part> }
fn plain_text(text: &str) -> String {
    let link = crate::re!(r"\[([^\]]+)\]\((https?:[^)\s]+)\)");
    let bold = crate::re!(r"\*\*([^*]+)\*\*");
    bold.replace_all(&link.replace_all(text, "$1 $2"), "$1").into_owned()
}
impl Kit {
    pub fn new(width: usize) -> Kit {
        let w = width.saturating_sub(2).max(10);
        // In a panel too narrow for a value beside its label, the label is a line of its own and the value is under it.
        let stacked = w < LABEL + 16;
        let indent = if stacked { 2 } else { LABEL };
        Kit { w, room: w.saturating_sub(indent).max(1), stacked, indent, parts: vec![] }
    }
    pub fn dim(x: impl Into<String>) -> Inked { Inked::of(x, Ink::Muted) }
    pub fn quiet(x: impl Into<String>) -> Inked { Inked::of(x, Ink::Soft) }
    /// Text wrapped to `room`, its markdown links and bold read as text; each line marked as text.
    pub fn wrap_at(text: &str, room: usize) -> Vec<Inked> {
        plain_text(text).split('\n').flat_map(|l| Inked::plain(crate::shown::words::clean(l)).wrap(room.max(1))).collect()
    }
    pub fn wrap(&self, text: &str) -> Vec<Inked> { Self::wrap_at(text, self.w) }
    /// A headline: bold, on a thin rule that runs to the edge. One that needs your eye is in its state's colour.
    pub fn head(&self, label: &str, ink: Option<Ink>) -> Inked {
        let s = crate::text::cut(label, self.w);
        let left = self.w as i64 - crate::text::width(&s) as i64 - 1;
        let h = match ink { Some(c) => Inked::of(s, c).bold(), None => Inked::plain(s).bold() };
        let h = if left > 0 { h.add(Inked::plain(" ")).add(Inked::of("─".repeat(left as usize), Ink::Rule)) } else { h };
        h.role("head")
    }
    /// Facts on one line, broken only between them: "12m ago" is never split. A line of steps (" → ") that goes on
    /// starts its next line with the arrow.
    pub fn flow(&self, parts: Vec<Inked>, space: Option<usize>, sep: &str) -> Vec<Inked> {
        let space = space.unwrap_or(self.room);
        let mut rows = vec![];
        let mut row = Inked::new();
        for p in parts.into_iter().filter(|p| !p.is_empty()) {
            let next = if row.is_empty() { p.clone() } else { row.clone().add(Self::dim(sep)).add(p.clone()) };
            if !row.is_empty() && next.width() > space { rows.push(row); row = if sep == " · " { p } else { Self::dim(sep.trim_start()).add(p) }; } else { row = next; }
        }
        if !row.is_empty() { rows.push(row); }
        rows
    }
    /// A fact: its label greyed on the left, what it is on the right; a long value wrapped under itself, never cut.
    pub fn fact(&self, label: &str, value: Vec<Inked>) -> Vec<Inked> {
        let cells: Vec<Inked> = value.into_iter().flat_map(|v| v.lines()).flat_map(|l| l.wrap(self.room)).collect();
        let mut out = vec![];
        if self.stacked { out.push(Self::dim(label).role(format!("fact:{label}"))); }
        let first = cells.first().cloned().unwrap_or_default();
        out.push(if self.stacked { Inked::spaces(self.indent).add(first).role("more") } else { Self::dim(format!("{} ", crate::text::pad(label, LABEL - 1))).add(first).role(format!("fact:{label}")) });
        for c in cells.into_iter().skip(1) { out.push(Inked::spaces(self.indent).add(c).role("more")); }
        out
    }
    pub fn fact_text(&self, label: &str, text: &str) -> Vec<Inked> { self.fact(label, Self::wrap_at(text, self.room)) }
    /// A list under a label: each item starts with a dot, and its further lines hang under its first word.
    pub fn items(&self, label: &str, xs: &[String]) -> Vec<Inked> {
        if xs.is_empty() { return vec![]; }
        let lines: Vec<Inked> = xs.iter().flat_map(|x| Self::wrap_at(x, self.room.saturating_sub(2)).into_iter().enumerate().map(|(n, l)| if n > 0 { Inked::plain("  ").add(l) } else { Self::dim("•").add(Inked::plain(" ")).add(l) })).collect();
        self.fact(label, lines)
    }
    /// At most `n` lines of a text; a cut one ends by saying so.
    pub fn first(lines: &[Inked], n: usize, space: usize) -> Vec<Inked> {
        if lines.len() <= n { return lines.to_vec(); }
        if n < 1 { return vec![]; }
        let mut out = lines[..n - 1].to_vec();
        out.push(lines[n - 1].clone().trim_end().add(Inked::plain(" …")).cut(space));
        out
    }
    pub fn part(&mut self, lines: Vec<Inked>) -> usize { self.parts.push(Part::of(lines)); self.parts.len() - 1 }
    /// A headline and a text under it. Short of room it keeps its beginning, or its end with `from_end`.
    pub fn text(&mut self, label: &str, lines: Vec<Inked>, pri: i32, from_end: bool) -> usize {
        let head = self.head(label, None);
        let w = self.w;
        let all = lines.clone();
        let h2 = head.clone();
        let mut p = Part::of(std::iter::once(head).chain(lines.into_iter().map(|l| l.role("text"))).collect());
        p.pri = pri; p.min = 3;
        p.at = Some(Box::new(move |n| if n < 3 { vec![] } else if n > all.len() { std::iter::once(h2.clone()).chain(all.iter().cloned()).collect() } else if from_end { [h2.clone(), Self::dim("…")].into_iter().chain(all[all.len() - (n - 2)..].iter().cloned()).collect() } else { std::iter::once(h2.clone()).chain(Self::first(&all, n - 1, w)).collect() }));
        self.parts.push(p);
        self.parts.len() - 1
    }
    /// What needs your eye: a headline in its state's colour, what it says, and facts under it.
    pub fn alert(&mut self, ink: Ink, label: &str, said: &str, facts: &[(&str, String)]) -> usize {
        let mut lines = vec![self.head(label, Some(ink))];
        if !said.is_empty() { lines.extend(self.wrap(said).into_iter().map(|l| l.role("text"))); }
        for (k, v) in facts { lines.extend(self.fact(k, Self::wrap_at(v, self.room))); }
        let all = lines.clone();
        let w = self.w;
        let mut p = Part::of(lines);
        p.alert = Some(Box::new(move |n| Self::first(&all, n, w)));
        self.parts.push(p);
        self.parts.len() - 1
    }
    /// A part that failed to draw is one red line that says so, in its place: the rest of the panel stays.
    pub fn failed(&mut self, label: &str, why: &str) {
        let mut lines = vec![self.head(label, None)];
        lines.extend(self.wrap(&format!("✗ This part could not be shown: {why}")).into_iter().map(|l| l.recolor(Ink::Red)));
        let mut p = Part::of(lines); p.failed = true;
        self.parts.push(p);
    }
    /// Every line with its margin.
    pub fn out(lines: Vec<Inked>) -> Vec<Inked> { lines.into_iter().map(|l| if l.is_empty() { l } else { let r = l.role_of().map(String::from); let x = Inked::plain(" ").add(l); match r { Some(r) => x.role(r), None => x } }).collect() }
}
