//! Waiting for a task of another machine (docs/after.md): such a task is named by its machine's label before its own
//! id or key (`vm-str:ELA-3`, `vm-str#3`) and kept as `vm-str:ela/3`. What this machine last heard of it, and of the
//! machine itself (not reachable since when), is kept in one file of the data folder, which the waits' own pass reads
//! as it reads a task of this machine. Asking the machine, and its answer there, is after/far_ask.rs.
use super::Stand;
use crate::clock::now;
use crate::json::*;
use crate::remote::{machines, Machine};
use crate::store::files::{data_dir, read_obj, with_lock, write_json};

/// A reference's machine and what it names there: `vm-str:ELA-3` is ("vm-str", "ELA-3"), `vm-str#3` ("vm-str", "#3").
/// None for a task of this machine, whose ids and keys never hold a `:` or a `#` after something.
pub fn split(r: &str) -> Option<(String, String)> {
    let at = r.find([':', '#'])?;
    let (m, rest) = (&r[..at], &r[at..]);
    let rest = rest.strip_prefix(':').unwrap_or(rest);
    (!m.is_empty() && !rest.is_empty() && rest != "#").then(|| (m.to_string(), rest.to_string()))
}
/// A task of another machine, as kept: `<label>:<its id there>`.
pub fn is_far(id: &str) -> bool { id.contains(':') }
pub fn id_of(label: &str, local: &str) -> String { format!("{label}:{local}") }
/// The machine of the board's own that a label names (case does not matter).
pub fn machine(label: &str) -> Option<Machine> { machines().into_iter().find(|m| m.label.eq_ignore_ascii_case(label)) }

fn file() -> std::path::PathBuf { data_dir().join("after-far.json") }
/// What this machine last heard: `tasks` (each far task's name there, where it stands, what it hands over) and
/// `machines` (each one asked: reachable, or since when not and why).
pub fn heard() -> Obj { read_obj(&file()).unwrap_or_default() }
/// A change to what was heard, under its lock.
pub fn note(f: impl FnOnce(&mut Obj)) {
    let _ = with_lock("after-far", || { let mut o = heard(); f(&mut o); write_json(&file(), &Value::Object(o)) });
}
/// What a machine answered of one of its tasks, kept: `a` as after/far_ask.rs reads it.
pub fn note_task(o: &mut Obj, id: &str, a: &Value) {
    let mut e = obj(a.clone());
    e.put("at", now());
    o.obj_mut("tasks").put(id, Value::Object(e));
}
/// A machine answered (`why` None), or did not: it is not reachable from the first time it did not answer on.
pub fn note_machine(o: &mut Obj, label: &str, why: Option<&str>) {
    let m = o.obj_mut("machines");
    let since = m.at(label).i("since");
    m.put(label, match why { None => json!({ "at": now() }), Some(w) => json!({ "at": now(), "since": if since > 0 { since } else { now() }, "why": w }) });
}

/// A far task as a person names it: `vm-str:ELA-3` where its machine said its key.
pub fn say(id: &str) -> String {
    let (label, local) = id.split_once(':').unwrap_or(("", id));
    let h = heard();
    let name = h.at("tasks").at(id).t("name");
    id_of(label, if name.is_empty() { local } else { &name })
}
/// Where a far task stands, as last heard: open until its machine says otherwise.
pub fn stand(id: &str) -> Stand {
    let h = heard();
    let e = h.at("tasks").at(id);
    let label = id.split(':').next().unwrap_or("");
    match e.s("stand") {
        "done" => Stand::Done,
        "broken" => Stand::Broken(format!("{label}:{}", e.s("why"))),
        _ => Stand::Open,
    }
}
/// What a far task hands over when the wait ends, as `handed::of` has it for one of this machine.
pub fn handed(id: &str) -> Value {
    let h = heard();
    let e = h.at("tasks").at(id);
    if !e.at("handed").is_object() { return json!({ "id": id, "done": false, "machine": id.split(':').next() }); }
    let mut o = obj(e.at("handed").clone());
    o.put("id", id); o.put("done", e.s("stand") == "done"); o.put("machine", id.split(':').next().unwrap_or(""));
    Value::Object(o)
}
/// A clock time as the row says it: `10:42`, this machine's time zone.
pub fn clock(ms: i64) -> String { chrono::DateTime::from_timestamp_millis(ms).map(|d| d.with_timezone(&chrono::Local).format("%H:%M").to_string()).unwrap_or_default() }
/// The machines of `left` that did not answer, said: "waiting for vm-str: not reachable since 10:42".
pub fn unreachable(left: &[String]) -> Option<String> {
    if !left.iter().any(|i| is_far(i)) { return None; }
    let h = heard();
    let mut seen: Vec<String> = vec![];
    for id in left.iter().filter(|i| is_far(i)) {
        let label = id.split(':').next().unwrap_or("").to_string();
        let since = h.at("machines").at(&label).i("since");
        if since > 0 && !seen.iter().any(|s| s.starts_with(&format!("{label}:"))) { seen.push(format!("{label}: not reachable since {}", clock(since))); }
    }
    (!seen.is_empty()).then(|| format!("waiting for {}", seen.join(", ")))
}
