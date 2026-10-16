//! What the board-review tests share: a clock that stands still, a board state with no list of its own, the tasks they
//! draw (one waiting for approval, one at work), and a task drawn as its row (alone or among others) or as its panel,
//! as plain text and in its inks.
#![allow(dead_code)]
use serde_json::{json, Value};
use std::sync::Arc;
use sw::board::state::{Rec, State};
use sw::board::theme::Theme;
use sw::board::view::row::{row_layout, row_lines};
use sw::json::*;
use sw::shown::ink::{Ink, Inked};

pub struct V { pub s: State, pub t: Theme, pub now: i64 }
pub fn has(text: &str, re: &str) -> bool { regex::Regex::new(re).unwrap().is_match(text) }
#[track_caller]
pub fn m(text: &str, re: &str) { assert!(has(text, re), "/{re}/ does not match:\n{text}"); }
#[track_caller]
pub fn no(text: &str, re: &str) { assert!(!has(text, re), "/{re}/ matches:\n{text}"); }
/// A copy of `base` with these fields; a null takes one away.
pub fn with(base: &Obj, more: Value) -> Obj { let mut o = base.clone(); for (k, v) in more.as_object().unwrap() { if v.is_null() { o.remove(k); } else { o.insert(k.clone(), v.clone()); } } o }
impl V {
    /// The clock stands still: "0s ago" stays true however slow the machine is. The board runs in another folder than
    /// the tasks' repo, so their panel names the project.
    pub fn new() -> V { let now = sw::clock::now(); sw::clock::set(Some(now)); let mut s = State::new(Default::default(), Default::default()); s.now = now; s.data.repo_root = "/nowhere".into(); V { s, t: Theme::default_dark(false), now } }
    pub fn base(&self) -> Obj { obj(json!({ "id": "proj/1", "title": "Build a flying game", "state": "blocked", "kind": "permission", "stage": "approval", "agent": "claude", "app": "claude", "at": self.now, "msg": "Approve to merge? Reviewed 3/5, says don't merge. Built **Skylark**: a little plane over endless sunny islands, with clouds." })) }
    pub fn review() -> Value { json!({ "score": 3, "recommend": false, "text": "Crashes." }) }
    pub fn pr(&self) -> Obj { with(&self.base(), json!({ "pr": "https://github.com/a/b/pull/12", "prState": "passing", "review": Self::review() })) }
    pub fn plain(&self) -> Obj { with(&self.base(), json!({ "review": Self::review() })) }
    pub fn working(&self) -> Obj { obj(json!({ "id": "proj/2", "title": "Other", "state": "working", "agent": "claude", "stage": "work", "progress": 50, "msg": "A long sentence about what the other agent is doing right now, in detail.", "at": self.now })) }
    /// The row's lines in their inks, laid out among `list` (only itself when that is empty).
    pub fn inked(&mut self, r: &Obj, width: usize, list: &[Obj]) -> Vec<Inked> {
        let all: Vec<Rec> = (if list.is_empty() { vec![r.clone()] } else { list.to_vec() }).into_iter().map(Arc::new).collect();
        let lay = row_layout(&mut self.s, &self.t, &all, width, &[all.clone()], 200);
        row_lines(&self.s, &self.t, r, width, &lay)
    }
    pub fn row_in(&mut self, r: &Obj, width: usize, list: &[Obj]) -> String { self.inked(r, width, list).iter().map(|l| l.text().trim_end().to_string()).collect::<Vec<_>>().join("\n") }
    pub fn row(&mut self, r: &Obj, width: usize) -> String { self.row_in(r, width, &[]) }
    pub fn top(&mut self, r: &Obj, width: usize, list: &[Obj]) -> String { self.row_in(r, width, list).split('\n').next().unwrap_or("").to_string() }
    /// The panel's lines in their inks: `fit` lines beside the list (0: whole), or full screen. Its settings as the
    /// loader reads them for the selected task.
    pub fn panel_inked(&mut self, r: &Obj, width: usize, full: bool, fit: usize) -> Vec<Inked> { let mut r = with(r, json!({ "owner": "me" })); if !r.has("snap") { let snap = sw::board::data::snap(&r); r.put("snap", snap); } sw::board::view::panel::panel(&mut self.s, &self.t, &r, width, full, fit) }
    pub fn panel(&mut self, r: &Obj, width: usize, full: bool, fit: usize) -> Vec<String> { self.panel_inked(r, width, full, fit).iter().map(|l| l.text().trim_end().to_string()).collect() }
    /// The panel at 80 columns, as text.
    pub fn page(&mut self, r: &Obj, full: bool) -> String { self.panel(r, 80, full, 0).join("\n") }
}
/// The inks the words are drawn in, each piece that holds them.
pub fn inks_of(lines: &[Inked], words: &str) -> Vec<Ink> { lines.iter().flat_map(|l| l.0.iter()).filter(|p| p.t.contains(words)).map(|p| p.c).collect() }
pub fn ink_of(lines: &[Inked], words: &str) -> Option<Ink> { inks_of(lines, words).first().copied() }
/// Every ink that paints some words on these lines.
pub fn all_inks(lines: &[Inked]) -> Vec<Ink> { lines.iter().flat_map(|l| l.0.iter()).filter(|p| !p.t.trim().is_empty()).map(|p| p.c).collect() }
