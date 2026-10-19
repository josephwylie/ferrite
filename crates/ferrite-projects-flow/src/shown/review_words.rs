//! A review in a few words: its result, its colour, which round it was.
use super::ink::{Ink, Inked};
use super::states::{is_agent, reviewing};
use super::words::{landed, ordinal};
use crate::gate;
use crate::json::*;
use crate::store::reviews::{review_number, review_result, review_round};

/// A review's result is said in words; its colour only repeats it. One from before the gate: its advice, as it was.
pub fn result_ink(rv: &Value) -> Ink {
    if gate::is_tagged(rv) { return if rv.b("pass") { Ink::Green } else { Ink::Red }; }
    match rv.at("recommend").as_bool() { Some(true) => Ink::Green, Some(false) => Ink::Red, None => Ink::Soft }
}
/// In the task panel's heading: nothing on the first review; from the second on, in words: "2nd review".
pub fn round_words(r: &Obj) -> String { if review_round(r) >= 2 { format!("{} review", ordinal(review_round(r))) } else { String::new() } }
/// On a row: which review a task is on or last had: "R#2". Nothing before the first.
pub fn review_tag(r: &Obj) -> String { let n = review_number(r); if n > 0 { format!("R#{n}") } else { String::new() } }
/// Its colour alone says how that review went: green passed; red failed (and stays so while the agent fixes it); the
/// review blue while the reviewer is on it. Once merged it goes quiet, but for one merged although its review failed.
pub fn review_tag_ink(r: &Obj) -> Ink {
    if reviewing(r) { Ink::Review } else if review_result(r) == "fail" { Ink::Red } else if landed(r) { Ink::Muted } else if review_result(r) == "pass" { Ink::Green } else { Ink::Soft }
}
pub fn review_tag_inked(r: &Obj) -> Inked { Inked::of(review_tag(r), review_tag_ink(r)) }
/// In the task panel a merged task's reviews are quiet the same way: all but its last one, when that one failed.
pub fn quiet_review(r: &Obj, e: &Value, last: bool) -> bool { landed(r) && !(last && (e.b("error") || result_ink(e) == Ink::Red)) }
/// Strip only board-generated review metadata; task descriptions and agent wording stay intact.
pub fn review_note(r: &Obj) -> String {
    if !is_agent(r) { return r.t("msg"); }
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"^(Approve to merge\?\s*)?(?:Reviewed [1-5]/5(?:, says (?:don't )?(?:merge|accept))?|Review: (?:Pass|Fail)(?: · [a-z0-9, -]+)?)\.\s*").unwrap());
    RE.replace(r.s("msg"), "$1").into_owned()
}
/// One review in a few words: "pass" in a list of them, "Pass · 2 minor" on its own. `quiet` takes the colour away.
pub fn verdict(e: &Value, long: bool, quiet: bool) -> Inked {
    let c = if quiet { Ink::Muted } else if e.b("error") { Ink::Red } else { result_ink(e) };
    if e.b("error") { return Inked::of(if long { "No verdict" } else { "no verdict" }, c); }
    if gate::is_tagged(e) {
        let w = gate::word(e);
        let mut out = Inked::of(if long { w.to_string() } else { w.to_lowercase() }, c);
        let t = gate::tally(e);
        if long && !t.is_empty() { out.push(format!(" · {t}"), Ink::Plain); }
        return out;
    }
    let mut out = Inked::of(format!("{}/5", e.t("score")), c);
    match e.at("recommend").as_bool() { Some(true) => { out.push(if long { ", says merge" } else { " yes" }, Ink::Plain); } Some(false) => { out.push(if long { ", says don't merge" } else { " no" }, Ink::Plain); } None => {} }
    out
}
/// The mark before a review's result: how it went, said without its colour. A dash: it gave no verdict.
pub fn verdict_mark(e: &Value, quiet: bool) -> Inked {
    let how = if e.b("error") { "none" } else if gate::is_tagged(e) { if e.b("pass") { "pass" } else { "fail" } } else { match e.at("recommend").as_bool() { Some(true) => "pass", Some(false) => "fail", None => "none" } };
    Inked::of(match how { "pass" => "✓", "fail" => "✗", _ => "–" }, if quiet { Ink::Muted } else if e.b("error") { Ink::Red } else { result_ink(e) })
}
