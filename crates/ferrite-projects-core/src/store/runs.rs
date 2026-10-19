//! A run: one stretch of an agent's work on a task, from the moment it was set going (by you, or by the board with a
//! check's output or a review's findings) to the end of that turn; for a reviewer, one review. Whose run it is, when it
//! began, and the time limit that holds for it. `limit.time` is read when a run starts and holds for that run: kept on
//! the task as `runLimit: { from, time }` by whichever comes first, the first look at the run (limits/tick.rs) or a
//! change of the setting (settings/change.rs), which keeps the value as it was for every run under way.
use super::attempts::all_attempts;
use super::settings::lookup::{lookup, Scope};
use super::settings::places::yours;
use super::tasks::{list, update_wait, Change, ListOpts, Task};
use crate::json::*;

pub fn run_role(t: &Task) -> &'static str {
    match t.s("stage") { "reviewing" | "reviewneeds" => "reviewer", "working" => "agent", _ => "" }
}
pub fn run_began(t: &Task) -> i64 {
    match run_role(t) {
        "reviewer" => t.i("reviewAt"),
        "agent" => t.i("since").max(t.i("workAt")).max(t.at("expectedWake").i("at")),
        _ => 0,
    }
}
pub fn run_time(t: &Task) -> String {
    let from = run_began(t);
    if from == 0 { return "none".into(); }
    if t.at("runLimit").is_object() && t.at("runLimit").i("from") == from { return t.at("runLimit").t("time"); }
    lookup("limit.time", &Scope::task(t), &yours()).map(|f| text(&f.value)).unwrap_or_else(|_| "none".into())
}
/// Keep a run's limit on its task. Returns the limit that holds for the run.
pub fn keep_run_time(t: &Task) -> String {
    let (from, time) = (run_began(t), run_time(t));
    if from != 0 && !(t.at("runLimit").is_object() && t.at("runLimit").i("from") == from) {
        let time2 = time.clone();
        let _ = update_wait(t.s("id"), 2000, move |c| {
            if run_began(c) != from || c.at("runLimit").i("from") == from { return Ok(Change::Keep); }
            c.put("runLimit", json!({ "from": from, "time": time2 }));
            Ok(Change::Write)
        });
    }
    time
}
/// Before `limit.time` changes anywhere: every run under way keeps the limit it started with.
pub fn keep_run_times() {
    let mut all = list(ListOpts::default());
    all.extend(all_attempts());
    for t in all { if !t.b("archivedAt") && run_began(&t) != 0 { keep_run_time(&t); } }
}
