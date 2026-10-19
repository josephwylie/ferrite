//! Hand-overs: one agent taking over from another that could not go on (`worker.fallback`, `reviewer.fallback`;
//! docs/fallback.md). What is kept on the task, who is next on the list, and the words: why it could not go on, and
//! the one line the row, the task panel and the history say (`codex took over from claude: out of allowance until 17:00`).
//!
//! On the task: `handovers` (every hand-over, oldest first: { at, role, from, to, why }), `on` (the agent on it now,
//! once one took over), `tried` (the agents on it since you last stepped in: none is tried twice), `reviewerOn` (the
//! reviewer that took over, for the later rounds of this task).
use super::settings::values::agent_of;
use super::tasks::{one_line, Task};
use crate::clock::now;
use crate::json::*;
use chrono::{Local, TimeZone, Timelike};
use std::sync::LazyLock;

pub fn handovers(t: &Task) -> Vec<Value> { t.arr("handovers").to_vec() }
pub fn last_handover(t: &Task, role: &str) -> Option<Value> { t.arr("handovers").iter().rev().find(|h| h.s("role") == role).cloned() }
/// Two agents of one program are told apart by their models; otherwise the program's name says who.
fn names(from: &str, to: &str) -> (String, String) {
    let (a, b) = (agent_of(from).kind, agent_of(to).kind);
    if a == b { (from.into(), to.into()) } else { (a, b) }
}
pub fn handover_line(h: &Value) -> String {
    let (from, to) = names(h.s("from"), h.s("to"));
    format!("{to} took over {}from {from}: {}", if h.s("role") == "reviewer" { "the review " } else { "" }, h.s("why"))
}
pub fn log_handover(t: &mut Task, role: &str, from: &str, to: &str, why: &str) -> Value {
    let h = json!({ "at": now(), "role": role, "from": from, "to": to, "why": one_line(why, 200) });
    let mut all = handovers(t);
    all.push(h.clone());
    t.put("handovers", all);
    h
}
/// The agent that is on a task now: the one that took over, else `worker.agent` as it reads (`setting`) when that is
/// still the program on it, else the program alone.
pub fn on_now(t: &Task, setting: &str) -> String {
    if t.b("on") { return t.t("on"); }
    if agent_of(setting).kind == t.s("agent") { setting.to_string() } else { t.t("agent") }
}
/// You stepped in (a reply, a new start): the list may be gone through again, from its start.
pub fn fresh_list(t: &mut Task) {
    t.put("tried", json!([]));
    t.put("reviewerOn", Value::Null);
    if t.at("fallbacks").b("worker") || t.at("fallbacks").b("reviewerLeft") {
        let f = t.obj_mut("fallbacks"); f.del("worker"); f.del("reviewerLeft");
    }
}
/// The next on the list: the first that is not on it now, was not on it since you last stepped in, and can be started
/// here (`usable`: "" or why not). Also why each one before it was passed over.
pub fn next_fallback(list: &[String], on: &str, tried: &[String], usable: impl Fn(&str) -> String) -> (String, Vec<String>) {
    let mut skipped = vec![];
    for agent in list {
        if agent == on || tried.contains(agent) { skipped.push(format!("{agent} was on it already")); continue; }
        let why = usable(agent);
        if !why.is_empty() { skipped.push(why); continue; }
        return (agent.clone(), skipped);
    }
    (String::new(), skipped)
}
pub fn nobody_left(name: &str, skipped: &[String]) -> String { format!("{name}: nobody left to take over ({})", skipped.join("; ")) }

// ---- why it could not go on, in plain words -------------------------------------------------------------------------------
static LIMIT: LazyLock<fancy_regex::Regex> = LazyLock::new(|| fancy_regex::Regex::new(r"(?i)usage limit|usage_limit|(?<!rate.)(?<!rate)limit reached|hit your [\w ]*limit|\bquota\b|billing|credit balance|payment|out of (?:credits?|allowance)").unwrap());
static NO_MODEL: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(?i)not supported|unsupported|does not exist|may not exist|model[^\n]{0,40}not found|unknown (?:provider|model)").unwrap());
fn clock(ms: i64) -> String { Local.timestamp_millis_opt(ms).single().map(|d| format!("{:02}:{:02}", d.hour(), d.minute())).unwrap_or_default() }
/// When the allowance comes back, where the provider's words say so: "resets 5pm", "try again at 5:00 PM", "resets at
/// 17:00", "try again in 3 hours 14 minutes", and the moment itself after a bar ("…|1760367600"). "" when they do not.
pub fn reset_at(words: &str, now: i64) -> String {
    static EPOCH: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"\|(\d{10})\b").unwrap());
    static AMPM: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(?i)(?:resets?|try again|available again|until)\s+(?:at\s+)?(\d{1,2})(?::(\d{2}))?\s*([ap])\.?m\b").unwrap());
    static HHMM: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(?i)(?:resets?|try again|available again|until)\s+(?:at\s+)?(\d{1,2}):(\d{2})\b").unwrap());
    static SPAN: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(?i)(?:resets?|try again|available again)\s+in\s+(?:(\d+)\s*(?:hours?|hrs?|h)\b)?[\s,]*(?:and\s+)?(?:(\d+)\s*(?:minutes?|mins?|m)\b)?").unwrap());
    if let Some(c) = EPOCH.captures(words) { return clock(c[1].parse::<i64>().unwrap_or(0) * 1000); }
    if let Some(c) = AMPM.captures(words).or_else(|| HHMM.captures(words)) {
        let mut h: i64 = c[1].parse().unwrap_or(99);
        let m: i64 = c.get(2).map(|x| x.as_str().parse().unwrap_or(0)).unwrap_or(0);
        let half = c.get(3).map(|x| x.as_str().to_lowercase()).unwrap_or_default();
        if half == "p" && h < 12 { h += 12; }
        if half == "a" && h == 12 { h = 0; }
        return if h < 24 && m < 60 { format!("{h:02}:{m:02}") } else { String::new() };
    }
    if let Some(c) = SPAN.captures(words) {
        let (h, m) = (c.get(1).map(|x| x.as_str().parse::<i64>().unwrap_or(0)), c.get(2).map(|x| x.as_str().parse::<i64>().unwrap_or(0)));
        if h.is_some() || m.is_some() { return clock(now + (h.unwrap_or(0) * 60 + m.unwrap_or(0)) * 60000); }
    }
    String::new()
}
/// Why it could not go on. `passing`: an error that passes by itself, which the board already tried `tries` times.
/// Else the provider's own words decide: a login, the allowance, a model that is not there; anything else as it was.
pub fn why_of(error: &str, auth: bool, passing: bool, reason: &str, tries: i64, now: i64) -> String {
    if auth { return "logged out".into(); }
    if passing { return format!("its {}{}", if reason.is_empty() { "provider failed" } else { reason }, if tries > 1 { format!(", still after {tries} tries") } else { String::new() }); }
    if LIMIT.is_match(error).unwrap_or(false) { let at = reset_at(error, now); return format!("out of allowance{}", if at.is_empty() { at } else { format!(" until {at}") }); }
    if NO_MODEL.is_match(error) { return "its model is not available".into(); }
    let said = one_line(error, 120);
    let said = said.trim_end_matches(|c: char| c == '.' || c.is_whitespace());
    format!("it stopped ({})", if said.is_empty() { "no reason given" } else { said })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn words() {
        assert_eq!(reset_at("Your limit resets 5pm (UTC)", 0), "17:00");
        assert_eq!(reset_at("try again at 12:30 AM", 0), "00:30");
        assert_eq!(why_of("You've hit your usage limit", false, false, "", 0, 0), "out of allowance");
        assert_eq!(why_of("rate limit reached", false, false, "", 0, 0), "it stopped (rate limit reached)");
        assert_eq!(why_of("", true, false, "", 0, 0), "logged out");
        assert_eq!(handover_line(&json!({ "from": "claude", "to": "codex/gpt-5.5", "why": "logged out", "role": "worker" })), "codex took over from claude: logged out");
    }
}
