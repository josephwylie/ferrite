//! Who typed what into a task's agent or reviewer: you (in the board's chat or with `tell`), the Coordinator, or the
//! board itself (its brief, a review's findings, a word to go on). Each text the board types is kept by its words and
//! the time, so that the conversation read back names who said each thing, in its order (flow/talk.rs). What nobody
//! noted was typed into the program's own box, by you.
use super::files::{append_line, dir_of, read_lines};
use crate::clock::now;
use crate::json::*;
use crate::status_words::who::{BOARD, YOU};
use std::path::PathBuf;

/// How much of a text is kept to see that a later text starts with it.
const HEAD: usize = 200;
/// How many of the newest are looked at.
const KEEP: usize = 300;
/// How much later than the conversation has it the board may have noted a text it typed (it notes once herdr has
/// taken it, which can wait for herdr).
const LATE_MS: i64 = 120_000;
/// How much earlier than the conversation has it the board may have typed a text (a program that keeps what is typed
/// while it works writes it down when it takes it). An older text with the same words was somebody else's.
const EARLY_MS: i64 = 300_000;
fn file(id: &str) -> PathBuf { dir_of(id).join("said-by.jsonl") }
fn norm(s: &str) -> String { s.split_whitespace().collect::<Vec<_>>().join(" ") }
fn head(s: &str) -> String { norm(s).chars().take(HEAD).collect() }
/// The whole text, as one number (FNV-1a: the same on every machine and every build).
fn sum(s: &str) -> String { format!("{:016x}", norm(s).bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))) }
#[derive(Clone)]
struct Noted { by: String, head: String, sum: String, at: i64, typed: bool }
/// Oldest first.
fn noted(id: &str) -> Vec<Noted> {
    let all = read_lines(&file(id));
    let from = all.len().saturating_sub(KEEP);
    all[from..].iter().map(|e| Noted { by: e.t("by"), head: e.t("head"), sum: e.t("sum"), at: e.i("at"), typed: e.b("typed") }).filter(|n| !n.head.is_empty()).collect()
}
fn note(id: &str, by: &str, text: &str, typed: bool) {
    let h = head(text);
    if h.is_empty() { return; }
    let mut e = json!({ "by": by, "head": h, "sum": sum(text), "at": now() });
    if typed { e["typed"] = json!(true); }
    let _ = append_line(&file(id), &e);
}
/// Words a person typed for the task's agent: `by` is "you" or "coordinator".
pub fn note_person(id: &str, by: &str, text: &str) { note(id, by, text, false); }
/// A person's words, as they go on: to another task (an attempt of it), or inside the board's own words (a plan sent
/// back with them). Whoever typed `typed` for `from` typed `words` for `to`.
pub fn carry(from: &str, typed: &str, to: &str, words: &str) {
    let s = sum(typed);
    if let Some(n) = noted(from).into_iter().rev().find(|n| !n.typed && n.by != BOARD && n.sum == s) { note(to, &n.by, words, false); }
}
/// Text the board typed into the box of the task's agent or reviewer. Where it starts with what a person typed since
/// the board last typed anything, it is their words on their way in, and stays theirs.
pub fn note_board(id: &str, text: &str) {
    let n = norm(text);
    let theirs = noted(id).into_iter().rev().take_while(|e| !e.typed).find(|e| n.starts_with(e.head.as_str())).map(|e| e.by);
    note(id, theirs.as_deref().unwrap_or(BOARD), text, true);
}
/// Who typed what was sent to a task's agent or reviewer, read once for a whole conversation.
pub struct Senders { id: String, noted: Vec<Noted> }
pub fn senders(id: &str) -> Senders { Senders { id: id.into(), noted: noted(id).into_iter().filter(|n| n.typed).collect() } }
impl Senders {
    /// Who said each text sent, oldest first as (text, when the conversation has it): "you", "coordinator" or
    /// "board". The texts sent and the texts the board typed are paired in the order both happened: as many pairs as
    /// can be, each of the same words and typed close to when the conversation has it (at most `EARLY_MS` before, a
    /// little after), the closest in time where there is a choice. No text typed once stands for two, so the same
    /// words twice are each their own sender's, and a text paired with none was typed into the program's own box:
    /// yours. Not noted (sent before the board kept this): the board's brief, by the line it starts with
    /// (delegate/rules.rs), else yours.
    pub fn of(&self, sent: &[(&str, i64)]) -> Vec<String> {
        let (n, m) = (sent.len(), self.noted.len());
        let sums: Vec<String> = sent.iter().map(|(t, _)| sum(t)).collect();
        // What pairing message i with typed text j costs (how far apart in time), or None where they cannot pair.
        let cost = |i: usize, j: usize| -> Option<i64> {
            let (e, at) = (&self.noted[j], sent[i].1);
            if e.sum != sums[i] { return None; }
            if at <= 0 { return Some((m - j) as i64); } // no time said: the newest
            (e.at <= at + LATE_MS && e.at >= at - EARLY_MS).then(|| (e.at - at).abs())
        };
        // best[i][j]: the most pairs, then the least cost, among the first i texts sent and the first j typed.
        let mut best = vec![vec![(0usize, 0i64); m + 1]; n + 1];
        let better = |a: (usize, i64), b: (usize, i64)| if a.0 != b.0 { a.0 > b.0 } else { a.1 < b.1 };
        for i in 1..=n { for j in 1..=m {
            let mut b = if better(best[i - 1][j], best[i][j - 1]) { best[i - 1][j] } else { best[i][j - 1] };
            if let Some(c) = cost(i - 1, j - 1) { let p = (best[i - 1][j - 1].0 + 1, best[i - 1][j - 1].1 + c); if better(p, b) { b = p; } }
            best[i][j] = b;
        } }
        let mut by: Vec<Option<String>> = vec![None; n];
        let (mut i, mut j) = (n, m);
        while i > 0 && j > 0 {
            let paired = cost(i - 1, j - 1).map(|c| (best[i - 1][j - 1].0 + 1, best[i - 1][j - 1].1 + c) == best[i][j]).unwrap_or(false);
            if paired { by[i - 1] = Some(self.noted[j - 1].by.clone()); i -= 1; j -= 1; }
            else if best[i - 1][j] == best[i][j] { i -= 1; } else { j -= 1; }
        }
        by.into_iter().zip(sent).map(|(b, (text, _))| b.unwrap_or_else(|| if head(text).starts_with(&format!("Work board task {},", self.id)) { BOARD.into() } else { YOU.into() })).collect()
    }
}
