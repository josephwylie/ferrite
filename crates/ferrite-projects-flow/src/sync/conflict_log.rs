//! A record of conflicts with main: one line per conflict, appended to `conflicts.jsonl` in the data folder once the
//! conflict is repaired and the board knows whether a new review followed. A line: { at, task, files, minutes,
//! repairs, review }: when it was found (ISO time), the files that conflicted (every round added up), the minutes from
//! found to up to date again, how often the agent was sent back, and whether the repaired work was reviewed anew.
use crate::clock::now;
use crate::json::*;
use crate::store::files::{append_line, data_dir, read_lines};
use std::path::PathBuf;

pub fn conflict_log() -> PathBuf { data_dir().join("conflicts.jsonl") }
fn files(r: &str) -> Vec<String> { r.lines().map(|f| f.trim().to_string()).filter(|f| !f.is_empty()).collect() }
/// A conflict was found: a repair that conflicts again is the same conflict, counted once from when it was first found.
pub fn conflict_found(t: &mut Obj, conflicting: &str) {
    let was = if t.at("conflict").is_object() && !t.at("conflict").b("repairedAt") { Some(t.at("conflict").clone()) } else { None };
    let mut all: Vec<String> = was.as_ref().map(|w| strings(w.at("files"))).unwrap_or_default();
    for f in files(conflicting) { if !all.contains(&f) { all.push(f); } }
    let at = was.as_ref().filter(|w| w.b("at")).map(|w| w.i("at")).unwrap_or_else(now);
    t.put("conflict", json!({ "at": at, "files": all, "repairs": was.map(|w| w.i("repairs")).unwrap_or(0) + 1 }));
}
/// The branch is up to date with main again. Nothing where no conflict is open.
pub fn conflict_repaired(t: &mut Obj) { if t.at("conflict").is_object() && !t.at("conflict").b("repairedAt") { t.obj_mut("conflict").put("repairedAt", now()); } }
/// The repaired work reached its review, or kept the one it had: the line is written and the task forgets the conflict.
pub fn conflict_logged(t: &mut Obj, reviewed: bool) -> Option<Value> {
    let c = t.at("conflict").clone();
    if !c.b("repairedAt") { return None; }
    let at = chrono::DateTime::from_timestamp_millis(c.i("at")).map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)).unwrap_or_default();
    let minutes = (((c.i("repairedAt") - c.i("at")) as f64 / 6000.0).round() / 10.0).max(0.0);
    let line = json!({ "at": at, "task": t.at("id"), "files": c.at("files"), "minutes": minutes, "repairs": c.at("repairs"), "review": reviewed });
    let _ = append_line(&conflict_log(), &line);
    t.put("conflict", Value::Null);
    Some(line)
}
/// Every line of the record, oldest first.
pub fn conflicts() -> Vec<Value> { read_lines(&conflict_log()) }
