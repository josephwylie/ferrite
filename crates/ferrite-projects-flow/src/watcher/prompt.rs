//! What the Watcher is asked and how its answer is read: one prompt for every agent in the call, one line and one
//! percent each.
use crate::json::*;
use crate::state::began;
use crate::store::tasks::one_line;
use std::collections::BTreeMap;

/// With a program that takes one (agents/kinds.rs), this replaces its own long instructions for coding work.
pub const SYSTEM: &str = "You write short status lines for a dashboard that shows coding agents at work. You reply with JSON only.";
/// One agent in a call: the task, whom the Watcher follows on it, and its last screen lines.
pub struct Look<'a> { pub t: &'a Obj, pub role: &'a str, pub lines: String }

fn spent_of(t: &Obj, role: &str, now: i64) -> String {
    let from = began(t, role);
    let mins = if from > 0 { ((now - from) as f64 / 60000.0).round().max(0.0) as i64 } else { 0 };
    match mins { 0 => "under a minute".into(), 1 => "1 minute".into(), m => format!("{m} minutes") }
}
fn part(look: &Look, n: usize, now: i64) -> String {
    let (t, role) = (look.t, look.role);
    let about: String = one_line(t.s("description"), 3000).chars().take(800).collect();
    let mut out = vec![format!("--- {n} ---"),
        if role == "reviewer" { "This one is an adversarial reviewer checking finished work. Estimate progress through this review only, not through the agent's task.".into() } else { "This one is a coding agent working on a task.".to_string() },
        format!("Task: {}", t.s("title"))];
    if !about.is_empty() { out.push("What the task asks for:".into()); out.push(about); }
    out.push(format!("The {} for {} so far.", if role == "reviewer" { "reviewer has been on this review" } else { "agent has been on this" }, spent_of(t, role, now)));
    out.push(if t.at("watch").s("role") == role {
        format!("Your previous guess was {} percent. Keep it unless something on the screen is a reason to change it. Raise it as work gets done. Lower it when new work shows up: {}.", t.at("watch").t("progress"),
            if role == "reviewer" { "more to check than it looked" } else { "a failing test, a merge conflict, review findings to fix, a bigger job than it looked" })
    } else { "This is your first look at it.".into() });
    out.push(format!("The last lines on the {role}'s terminal right now:"));
    out.push(look.lines.clone());
    out.join("\n")
}
pub fn watcher_prompt(looks: &[Look], now: i64) -> String {
    let many = looks.len() > 1;
    let mut out = vec![
        if many { format!("You are watching {} agents at work, numbered below. Write the status of each for a dashboard.", looks.len()) } else { "You are watching an agent at work. Write its status for a dashboard.".into() },
        "The percent is the share of the whole job's time that is already spent: the dashboard works out the minutes left from it and the time spent so far, so do not guess high while much is left to do.".into(),
    ];
    for (i, l) in looks.iter().enumerate() { out.push(part(l, i + 1, now)); }
    out.push("--- end ---".into());
    out.push("Each status line is written like these examples: \"Updating file locations and running checks\", \"Waiting for the PR review on GitHub\", \"Fixing the two failing tests\".".into());
    out.push(format!("Reply with only this JSON and nothing else{}: [{{\"n\": <the number above>, \"msg\": \"<what it is doing right now: at most 8 plain words, present tense, no tool or command names>\", \"progress\": <whole number 0 to 99: your best guess of how far through its current assignment it is>}}]", if many { ", one entry for each number above" } else { "" }));
    out.join("\n")
}
/// The answer as n -> {msg, progress}. A single object is taken as the answer for the only agent asked about.
pub fn read_answer(out: &str, count: usize) -> BTreeMap<usize, Value> {
    let mut found = BTreeMap::new();
    let mut list: Option<Vec<Value>> = None;
    for (open, close) in [('[', ']'), ('{', '}')] {
        let (Some(a), Some(b)) = (out.find(open), out.rfind(close)) else { continue };
        if b <= a { continue; }
        if let Ok(v) = serde_json::from_str::<Value>(&out[a..=b]) { list = Some(match v { Value::Array(a) => a, v => vec![v] }); break; }
    }
    for j in list.unwrap_or_default() {
        if !j.is_object() || !j.b("msg") { continue; }
        let n = if j.at("n").is_null() && count == 1 { 1 } else { j.f("n").round() as i64 };
        if !(n >= 1 && n as usize <= count) || found.contains_key(&(n as usize)) { continue; }
        let p = j.f("progress"); let p = if p.is_nan() { 0 } else { (p.round() as i64).clamp(0, 99) };
        found.insert(n as usize, json!({ "msg": one_line(&j.t("msg"), 120), "progress": p }));
    }
    found
}
