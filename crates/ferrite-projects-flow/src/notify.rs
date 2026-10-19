//! Telling you that a task needs you, is ready to approve, or failed: the `board.notify` setting. `bell` rings the
//! terminal bell where a board is open; a command is run once per event, with the task's id, its title and what
//! happened in its environment; `none` does neither.
use crate::clock::now;
use crate::json::*;
use crate::store::attempts::attempts_of;
use crate::store::settings::lookup::setting_text;
use crate::store::tasks::{update, Change};

/// What happened, from where the task stands; "" for a task nobody needs to hear about.
pub fn event_of(t: &Obj) -> &'static str {
    if t.b("archivedAt") { return ""; }
    let st = t.s("stage");
    if ["asks", "reviewneeds", "plan", "pick"].contains(&st) || (st == "stopped" && t.b("limitStop")) || (st == "after" && t.b("afterBlocked")) { return "needs you"; }
    if st == "attempts" { return if attempts_of(t).iter().any(|c| ["asks", "reviewneeds"].contains(&c.s("stage"))) { "needs you" } else { "" }; }
    if st == "failed" || st == "nosummary" { return "failed"; }
    if st == "approval" { return if t.b("failure") || t.b("prFailure") || t.b("syncBlocked") { "needs you" } else { "ready to approve" }; }
    ""
}
fn noted(t: &Obj, event: &str) -> bool { t.at("notice").s("event") == event && (event.is_empty() || t.at("notice").s("stage") == t.s("stage")) }
/// The housekeeping pass: each event is noted on its task once (`notice`), and the command is run for it.
pub fn notify_pass(tasks: &[Obj]) {
    let how = setting_text("board.notify", None);
    for seen in tasks {
        if noted(seen, event_of(seen)) { continue; }
        let mut fresh = false;
        let Ok(t) = update(seen.s("id"), |t| {
            let ev = event_of(t);
            if noted(t, ev) { return Ok(Change::Keep); }
            t.put("notice", if ev.is_empty() { Value::Null } else { json!({ "event": ev, "stage": t.s("stage"), "at": now() }) });
            fresh = !ev.is_empty();
            Ok(Change::Write)
        }) else { continue };
        if !fresh || how == "none" || how == "bell" { continue; }
        let env = [("HERDR_PROJECTS_ID", t.t("id")), ("HERDR_PROJECTS_TITLE", t.t("title")), ("HERDR_PROJECTS_EVENT", t.at("notice").t("event"))];
        crate::runner::detached_sh(&how, &env);
    }
}
