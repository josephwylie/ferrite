//! "Research only" was a switch of its own on a task and on an automation (`research: true`). It is the setting
//! `research.only` now, and what was saved with the old switch is read in the new shape: a task or an automation that
//! had it on says the setting as its own, and a task that was started keeps what it was started as (`researchRun`).
//! Done when it is read, and written with the next change to it; converting twice changes nothing.
use crate::json::*;

pub const RESEARCH: &str = "research.only";

/// A task as it is now.
pub fn research_moved(mut t: Obj) -> Obj {
    let old = t.contains_key("research");
    let started = t.b("startedOnce") && !t.at("researchRun").is_boolean() && !t.b("researchOpen");
    if !old && !started { return t; }
    let was = t.at("research") == &json!(true);
    // Its own answer since then wins over the old switch.
    if was && t.at("settings").at(RESEARCH).is_null() && !t.at("settings").as_object().map(|s| s.contains_key(RESEARCH)).unwrap_or(false) { t.obj_mut("settings").put(RESEARCH, true); }
    // Under way before this version: it goes on as what it was, whatever a default says from now on.
    if started { t.put("researchRun", was); }
    t.del("research");
    t
}
/// An automation as it is now: the old switch is one of its own settings.
pub fn research_moved_automation(mut a: Obj) -> Obj {
    if !a.contains_key("research") { return a; }
    let was = a.at("research") == &json!(true);
    a.del("research");
    if was && !a.at("settings").as_object().map(|s| s.contains_key(RESEARCH)).unwrap_or(false) { a.obj_mut("settings").put(RESEARCH, true); }
    a
}
