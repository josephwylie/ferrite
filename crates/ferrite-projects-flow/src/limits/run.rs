//! The light part of the limits, with nothing of the agents or the actions in it, so that what only draws a task (the
//! rows, the panel) can ask it: whether a limit is said at all, whose run it is and how long it has taken, the tokens
//! that count, the mark of a stop, and the words for each.
use crate::json::*;
use crate::store::attempts::attempts_of;
use crate::store::runs::run_began;
use crate::store::settings::places::yours;
use crate::store::settings::values::tokens_say;
use crate::store::tasks::{get, Task};
use std::collections::BTreeMap;

const NAMES: [&str; 2] = ["limit.time", "limit.tokens"];
fn task_of(t: &Task) -> Option<Task> { if t.b("attemptOf") { get(t.s("attemptOf")) } else { None } }
/// Does any place say a limit for this task? Asked before the settings are looked up at all, on every drawing.
pub fn limit_said(t: &Task) -> bool {
    if t.at("runLimit").is_object() && t.at("runLimit").s("time") != "none" { return true; }
    let mine = yours();
    let repo = if t.s("repo").is_empty() { t.s("id").split('/').next().unwrap_or("").to_string() } else { t.s("repo").to_string() };
    let parent = task_of(t);
    let says = |p: Option<&Obj>| p.map(|p| p.contains_key("limit") || NAMES.iter().any(|n| p.contains_key(*n))).unwrap_or(false);
    says(t.o("settings")) || says(parent.as_ref().and_then(|p| p.o("settings"))) || says(t.at("carried").o("settings")) || says(Some(&mine.you)) || says(mine.repos.get(&repo))
}
pub fn stopped_at_limit(t: &Task) -> bool { t.s("stage") == "stopped" && t.at("limitStop").is_object() && !t.b("archivedAt") }
/// "1h12m", "45m", "1m21s", "30s": a time as the limits say it. Seconds only while they matter, under ten minutes.
pub fn time_say(span: i64) -> String {
    let s = ((span.max(0) as f64) / 1000.0).round() as i64;
    let (h, m, x) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 { format!("{h}h{}", if m > 0 { format!("{m}m") } else { String::new() }) }
    else if m >= 10 || (m > 0 && x == 0) { format!("{m}m") } else if m > 0 { format!("{m}m{x}s") } else { format!("{s}s") }
}
/// The line the row and herdr's sidebar say.
pub fn stop_line(what: &str, limit: &str) -> String { if what == "time" { format!("stopped at its {limit} limit") } else { format!("stopped at {limit} tokens") } }
/// What was used when it stopped: "after 2h1m", "with 2.1m tokens used".
pub fn used_say(ls: &Value) -> String { if ls.s("what") == "time" { format!("after {}", time_say(ls.i("used"))) } else { format!("with {} tokens used", tokens_say(ls.f("used"))) } }
/// The tokens that count against `limit.tokens`. A task done several times at once is one task to its limit; a
/// conversation counts once, whoever of them holds it.
pub fn used_of(t: &Task) -> f64 {
    let top = task_of(t).unwrap_or_else(|| t.clone());
    let others = if top.arr("attempts").is_empty() { vec![] } else { attempts_of(&top) };
    let mut files: BTreeMap<String, f64> = BTreeMap::new();
    let (mut loose, mut base) = (0.0, 0.0);
    for m in std::iter::once(top.clone()).chain(others).map(|x| if x.s("id") == t.s("id") { t.clone() } else { x }) {
        let own = m.at("spent").o("files").cloned().unwrap_or_default();
        for (file, f) in &own { let e = files.entry(file.clone()).or_insert(0.0); *e = e.max(f.f("n")); }
        if own.is_empty() { loose += m.at("spent").f("total"); }
        base += m.at("spent").f("base");
    }
    (files.values().sum::<f64>() + loose - base).max(0.0)
}
/// How long this run has taken. A run the board carried on with after its limit was raised goes on from the time it
/// had already taken (`runCarry`).
pub fn ran_for(t: &Task, now: i64) -> i64 {
    let from = run_began(t);
    if from == 0 { return 0; }
    (now - from).max(0) + if t.at("runCarry").is_object() && t.at("runCarry").i("from") == from { t.at("runCarry").i("ms") } else { 0 }
}
/// Every turn you start gives a task that a limit stopped its limit once more: the tokens count from here.
pub fn granted(t: &mut Task) {
    if !t.at("limitStop").is_object() { return; }
    if t.at("limitStop").s("what") == "tokens" { let used = used_of(t); let base = t.at("spent").f("base") + used; t.obj_mut("spent").put("base", base); }
    t.put("limitStop", Value::Null);
    t.put("runCarry", Value::Null);
}
