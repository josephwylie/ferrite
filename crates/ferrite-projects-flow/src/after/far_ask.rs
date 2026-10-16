//! Asking another machine about the tasks waited for there, over the machine link (remote.rs): its door's `after`
//! request answers, for each task named, its id and name there, where it stands, what it hands over and what it waits
//! for in turn (for a wait in a circle across machines, after/circle.rs). The pass that asks every machine a waiting
//! task waits on runs as a job of its own (`afterfar`), never in the tick that wants it, so a machine that does not
//! answer holds nothing up: the task goes on waiting and says since when that machine is not reachable.
use super::far::{heard, id_of, is_far, machine, note, note_machine, note_task};
use super::{after_of, handed, stand_of, Stand, STAGE};
use crate::json::*;
use crate::remote::{ask as ask_machine, Machine};
use crate::store::keys::{repo_keys, say_id};
use crate::store::tasks::{find, get, list, ListOpts, Task};
use std::collections::BTreeMap;

/// How long an answer is waited for.
const ASK_MS: u64 = 20000;
/// How often the machines a waiting task waits on are asked.
pub const EVERY_MS: i64 = 15000;

/// The door's answer here, for the machine that asks: each of `refs` as this machine has it.
pub fn door(req: &Value) -> Value {
    let keys = repo_keys();
    let tasks: Vec<Value> = strings(req.at("refs")).iter().map(|r| {
        let d = match find(r) { Ok(d) => d, Err(e) => return json!({ "ref": r, "found": false, "msg": e.msg }) };
        let id = if d.b("attemptOf") { d.t("attemptOf") } else { d.t("id") };
        let (stand, why) = match stand_of(&id) { Stand::Done => ("done", String::new()), Stand::Open => ("open", String::new()), Stand::Broken(w) => ("broken", w) };
        // What it waits for in turn, and so on: every task of this machine reached that way, with what it waits for.
        let (mut edges, mut names, mut todo) = (Obj::new(), Obj::new(), vec![id.clone()]);
        while let Some(n) = todo.pop() {
            if edges.contains_key(&n) { continue; }
            let next = get(&n).map(|t| after_of(&t)).unwrap_or_default();
            todo.extend(next.iter().filter(|x| !is_far(x)).cloned());
            names.put(&n, say_id(&n, &keys));
            edges.put(&n, next);
        }
        // The machines those name, by where this machine reaches them: the asking side knows itself among them by that.
        let mut hosts = Obj::new();
        for raw in edges.values().flat_map(strings).filter(|x| is_far(x)) {
            let label = raw.split(':').next().unwrap_or("").to_string();
            if !hosts.contains_key(&label) { if let Some(m) = machine(&label) { hosts.put(&label, m.target); } }
        }
        json!({ "ref": r, "found": true, "id": id, "name": say_id(&id, &keys), "stand": stand, "why": why, "handed": handed::of(&id), "edges": edges, "names": names, "machines": hosts })
    }).collect();
    json!({ "ok": true, "host": crate::remote::check::hostname(), "tasks": tasks })
}

/// Ask machine `m` about `refs` (as typed there, or ids): each answer, or why there is none, in a few words.
pub fn ask(m: &Machine, refs: &[String]) -> Result<Vec<Value>, String> {
    match ask_machine(m, "after", Some(&json!({ "refs": refs })), ASK_MS) {
        Ok(j) if j.at("tasks").is_array() => Ok(j.arr("tasks").to_vec()),
        Ok(j) if j.s("msg").starts_with("Unknown request") => Err("it runs an older herdr-projects, which cannot answer for its tasks: update it there".into()),
        Ok(j) => Err(if j.s("msg").is_empty() { "it did not answer for its tasks".into() } else { j.t("msg") }),
        Err(why) => Err(why.t("line")),
    }
}
/// One answer as it is kept (after/far.rs).
pub fn kept(a: &Value) -> Value { json!({ "name": a.at("name"), "stand": a.at("stand"), "why": a.at("why"), "handed": a.at("handed") }) }

/// Every task of another machine that a task here waits for, by machine.
fn wanted(tasks: &[Task]) -> BTreeMap<String, Vec<String>> {
    let mut by: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for t in tasks.iter().filter(|t| t.s("stage") == STAGE && !t.b("archivedAt")) {
        for id in after_of(t).into_iter().filter(|i| is_far(i)) {
            let (label, _) = id.split_once(':').unwrap_or_default();
            let v = by.entry(label.to_string()).or_default();
            if !v.contains(&id) { v.push(id); }
        }
    }
    by
}
/// Whether a waiting task here waits on another machine: the pass is then due now and then.
pub fn waits_far(tasks: &[Task]) -> bool { !wanted(tasks).is_empty() }
/// The `afterfar` job: each machine asked about its tasks waited for, and what it said (or that it did not answer)
/// kept. The housekeeping that every job ends with then starts a task whose wait is over.
pub fn far_pass() {
    for (label, ids) in wanted(&list(ListOpts::default())) {
        let locals: Vec<String> = ids.iter().map(|i| i.split_once(':').map(|(_, l)| l.to_string()).unwrap_or_default()).collect();
        let answer = match machine(&label) { Some(m) => ask(&m, &locals), None => Err("it is not a machine the board knows any more (herdr machine list, or the board.machines setting)".into()) };
        let before = heard();
        note(|o| match &answer {
            Err(why) => note_machine(o, &label, Some(why)),
            Ok(list) => {
                note_machine(o, &label, None);
                for (id, a) in ids.iter().zip(list) {
                    if a.b("found") { note_task(o, id, &kept(a)); continue; }
                    // Gone there: removed, as for a task of this machine. Its name is the one it had.
                    let name = before.at("tasks").at(id).t("name");
                    let name = if name.is_empty() { id.split_once(':').map(|(_, l)| l.to_string()).unwrap_or_default() } else { name };
                    note_task(o, id, &json!({ "name": name, "stand": "broken", "why": format!("{name} was removed"), "handed": before.at("tasks").at(id).at("handed") }));
                }
            }
        });
    }
}
/// What a machine said of one of its tasks, as kept for `id`: used when the wait is checked, so a task said to wait
/// for one that is done already starts at once.
pub fn note_answers(label: &str, answers: &[Value]) {
    note(|o| { note_machine(o, label, None); for a in answers.iter().filter(|a| a.b("found")) { note_task(o, &id_of(label, a.s("id")), &kept(a)); } });
}
