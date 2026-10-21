//! A turn cut off by a passing provider error: the board waits, then tries again by itself
//! (docs/how-work-gets-done.md). What "passing" means is decided where the agent's conversation is read.
use super::tasks::Task;
use crate::json::*;

/// How long it waits before each new try, growing: four more tries over fifteen minutes, then it gives up.
pub fn retry_waits() -> Vec<i64> {
    match std::env::var("HERDR_PROJECTS_RETRY_MS") {
        Ok(v) => v.split(',').filter(|x| !x.trim().is_empty()).map(|x| x.trim().parse().unwrap_or(0)).collect(),
        Err(_) => vec![60000, 120000, 240000, 480000],
    }
}
/// The wait a task is in, or None. Saved as `waiting` on the task: { who, reason, error, until, tries }. The reviewer's
/// holds while the task is under review; the agent's sits on the saved failure of the cut-off turn (`for` is that
/// failure's time), so anything that moves the task on ends it without having to know about it.
pub fn waiting_of(t: &Task) -> Option<&Value> {
    let w = t.at("waiting");
    if !w.is_object() || t.b("archivedAt") { return None; }
    if w.s("who") == "reviewer" { return (t.s("stage") == "reviewing").then_some(w); }
    (w.s("who") == "agent" && t.s("stage") == "failed" && t.b("failure") && t.at("failure").at("at") == w.at("for")).then_some(w)
}
fn span(ms: i64) -> String { if ms >= 60000 { format!("{}m", (ms as f64 / 60000.0).round() as i64) } else { format!("{}s", ((ms as f64 / 1000.0).round() as i64).max(1)) } }
/// What the row says meanwhile: "Reviewer's model is at capacity, trying again in 2m".
pub fn wait_line(w: &Value, now: i64) -> String {
    let left = w.i("until") - now;
    format!("{}'s {}, trying again {}", if w.s("who") == "reviewer" { "Reviewer" } else { "Agent" }, w.s("reason"), if left > 0 { format!("in {}", span(left)) } else { "now".into() })
}
/// What the failure says when the tries are used up, before the provider's own words.
pub fn gave_up(who: &str, reason: &str, tries: i64) -> String {
    let total: i64 = retry_waits().iter().take((tries - 1).max(0) as usize).sum();
    format!("the {who}'s {reason}{}", if tries > 1 { format!(", still after {tries} tries over {}", span(total)) } else { String::new() })
}
