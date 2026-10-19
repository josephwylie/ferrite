//! Automations on disk: the list (a saved task plus a trigger, each), every firing written down, and which board events
//! were seen already. All of it is one folder in the data folder, `automations/`, and belongs to this machine. What an
//! automation is and does: automations/, docs/automations.md.
use super::files::*;
use super::old_research::research_moved_automation;
use crate::err::Res;
use crate::json::*;
use std::path::PathBuf;

pub fn automations_dir() -> PathBuf { data_dir().join("automations") }
fn list_file() -> PathBuf { automations_dir().join("automations.json") }
fn firings_file(id: &str) -> PathBuf { automations_dir().join("firings").join(format!("{id}.jsonl")) }
fn events_file() -> PathBuf { automations_dir().join("events.json") }
pub fn is_id(id: &str) -> bool { id.len() == 8 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
/// The most firings kept for one automation: older ones go, the newest stay.
pub const FIRINGS_KEPT: usize = 200;

// ---- the list -------------------------------------------------------------------------------------------------------------
pub fn automations() -> Vec<Obj> {
    read_obj(&list_file()).map(|j| j.arr("list").iter().filter_map(|a| a.as_object()).filter(|a| is_id(a.s("id"))).cloned().map(research_moved_automation).collect()).unwrap_or_default()
}
pub fn has_automations() -> bool { list_file().exists() }
/// One automation by its name (any case) or its id.
pub fn automation(reference: &str) -> Option<Obj> {
    let want = reference.trim().to_lowercase();
    if want.is_empty() { return None; }
    automations().into_iter().find(|a| a.s("id") == want || a.s("name").to_lowercase() == want)
}
/// The one way the list changes: a short read-modify-write under its lock. `f` gets the list and changes it in place.
pub fn change_automations<T>(f: impl FnOnce(&mut Vec<Obj>) -> Res<T>) -> Res<T> {
    with_lock_wait("automations", 5000, || {
        let mut list = automations();
        let out = f(&mut list)?;
        write_json(&list_file(), &json!({ "list": list }))?;
        Ok(out)
    })
}
/// One automation changed in place; None when it is gone.
pub fn change_automation(id: &str, f: impl FnOnce(&mut Obj)) -> Res<Option<Obj>> {
    change_automations(|list| Ok(list.iter_mut().find(|a| a.s("id") == id).map(|a| { f(a); a.clone() })))
}

// ---- its firings: one line each, the newest last on disk -------------------------------------------------------------------
pub fn firings_of(id: &str) -> Vec<Value> { if is_id(id) { read_lines(&firings_file(id)) } else { vec![] } }
/// A firing is written down. The same skip again and again is one line with a count.
pub fn note_firing(id: &str, entry: Obj) -> Res<Value> {
    with_lock_wait("automation-firings", 5000, || {
        let mut all = firings_of(id);
        let same_skip = entry.b("skipped") && all.last().map(|l| l.at("skipped") == entry.at("skipped")).unwrap_or(false);
        if same_skip {
            let last = all.last_mut().and_then(|l| l.as_object_mut()).expect("there is a last firing");
            let n = if last.b("n") { last.i("n") } else { 1 } + 1;
            for (k, v) in entry { last.insert(k, v); }
            last.put("n", n);
        } else { all.push(Value::Object(entry)); }
        let kept: Vec<Value> = all.iter().skip(all.len().saturating_sub(FIRINGS_KEPT)).cloned().collect();
        let body: String = kept.iter().map(|e| format!("{}\n", serde_json::to_string(e).unwrap_or_default())).collect();
        write_atomic(&firings_file(id), body.as_bytes())?;
        Ok(kept.last().cloned().unwrap_or(Value::Null))
    })
}
pub fn drop_firings(id: &str) { if is_id(id) { let _ = std::fs::remove_file(firings_file(id)); } }

// ---- board events seen already: { "<task id>": ["added", "merged:17…", …] }, or None before the first look --------------
pub fn events_seen() -> Option<Obj> { read_obj(&events_file()) }
pub fn save_events_seen(seen: &Obj) -> Res<()> { write_json(&events_file(), &Value::Object(seen.clone())) }
pub fn drop_events_seen() { let _ = std::fs::remove_file(events_file()); }
