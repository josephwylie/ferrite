//! A task that waits for others to be done (`--after`, docs/after.md): what it waits for, checked when it is said
//! (each task exists, here or on a machine the board knows, and no task ends up waiting for itself), where each of those stands, and the
//! words a waiting task says. Done is the stage the board calls done, however it got there: merged, a research task's
//! report finished, or marked done. The pass that starts a task once its wait is over is after/pass.rs; what its agent
//! is told of the work before it is after/handed.rs.
pub mod change;
pub mod circle;
pub mod far;
pub mod far_ask;
pub mod handed;
pub mod pass;

use crate::err::Res;
use crate::json::*;
use crate::remote::Machine;
use crate::store::keys::{repo_keys, say_id};
use crate::store::tasks::{find, get, Task};
use std::collections::HashMap;

/// What a delegation says about waiting: the tasks named with `--after` (None: what the task had; an empty list or
/// `none`: nothing), and `--now`: start it without waiting.
#[derive(Default, Clone, Debug)]
pub struct Wait { pub after: Option<Vec<String>>, pub now: bool }
/// The stage of a delegated task that waits for others. It holds no slot and nothing runs for it.
pub const STAGE: &str = "after";

/// The tasks a text names: separated by commas or spaces. `none` (or nothing) names none.
pub fn refs_of(text: &str) -> Vec<String> {
    let v: Vec<String> = text.split(|c: char| c == ',' || c.is_whitespace()).filter(|s| !s.is_empty()).map(String::from).collect();
    if v.len() == 1 && v[0].eq_ignore_ascii_case("none") { vec![] } else { v }
}
/// What a task waits for, as kept on it: task ids.
pub fn after_of(t: &Obj) -> Vec<String> { strings(t.at("after")) }
/// A task as a person names it: `HSW-12` where its repo has a short key, else its id.
pub fn say(id: &str) -> String { if far::is_far(id) { far::say(id) } else { say_id(id, &repo_keys()) } }
/// Several tasks in one phrase: "a", "a and b", "a, b and c".
pub fn names(ids: &[String]) -> String {
    let n: Vec<String> = ids.iter().map(|i| say(i)).collect();
    match n.len() { 0 => String::new(), 1 => n[0].clone(), k => format!("{} and {}", n[..k - 1].join(", "), n[k - 1]) }
}
fn listed(ids: &[String]) -> String { ids.iter().map(|i| say(i)).collect::<Vec<_>>().join(", ") }
/// A waiting task's line: "queued after HSW-4, HSW-5", naming the tasks not done yet, and each machine of theirs that
/// does not answer: "queued after vm-str:ELA-3 · waiting for vm-str: not reachable since 10:42". A task delegated
/// (tabbed) with `--after` is queued, as one that waits for a free slot is: it starts by itself once they are done.
pub fn waits_line(left: &[String]) -> String {
    let line = format!("queued after {}", listed(left));
    match far::unreachable(left) { Some(w) => format!("{line} · {w}"), None => line }
}
/// A queued task's line on its row: another machine's row brings the one its own board says (`afterLine`, remote.rs),
/// as the tasks it names are that machine's to name (its keys, its machines).
pub fn row_line(r: &Obj) -> String { r.so("afterLine").map(String::from).unwrap_or_else(|| waits_line(&strings(r.at("afterLeft")))) }
/// A task a row waits for, named as the row's own board names it (`afterNames`, remote.rs), else as this one does.
fn say_on(r: &Obj, id: &str) -> String { r.at("afterNames").so(id).map(String::from).unwrap_or_else(|| say(id)) }
/// What another machine's board is sent of a task's wait, said here for it: its queued line and each task's name.
pub fn sent(t: &mut Obj) {
    if after_of(t).is_empty() { return; }
    let names: Obj = after_of(t).iter().map(|i| (i.clone(), json!(say(i)))).collect();
    if t.s("stage") == STAGE { t.put("afterLine", waits_line(&strings(t.at("afterLeft")))); }
    t.put("afterNames", names);
}
/// "once that is done", "once they are done": when a queued task starts by itself.
fn once(n: usize) -> &'static str { if n == 1 { "once that is done" } else { "once they are done" } }
/// What `list` says of a task in to do (nobody delegated it yet) that is to wait for others: "after HSW-4", naming the
/// ones not done yet (nothing once all are; nothing in any other stage, whose own words `list` says). It is only a
/// condition: such a task never starts by itself, and its row says nothing of it.
pub fn todo_line(t: &Obj) -> String {
    if t.s("stage") != "todo" || after_of(t).is_empty() { return String::new(); }
    let (left, _) = standing(t);
    if left.is_empty() { String::new() } else { format!("after {}", listed(&left)) }
}

/// Where a task that is waited for stands: done, not yet, or never by itself, with the reason.
#[derive(Debug, PartialEq)]
pub enum Stand { Done, Open, Broken(String) }
pub fn stand_of(id: &str) -> Stand {
    if far::is_far(id) { return far::stand(id); }
    let Some(d) = get(id) else { return Stand::Broken(format!("{} was removed", say(id))) };
    if d.s("stage") == "done" { return Stand::Done; }
    if d.b("archivedAt") { return Stand::Broken(format!("{} was archived without being done", say(id))); }
    match d.s("stage") {
        "failed" | "nosummary" => Stand::Broken(format!("{} failed", say(id))),
        "stopped" => Stand::Broken(format!("{} was stopped", say(id))),
        "todo" if d.b("stoppedWaiting") => Stand::Broken(format!("{} was stopped", say(id))), // stopped while it waited itself
        _ => Stand::Open,
    }
}
/// What a task still waits for, and the first reason it cannot go on by itself.
pub fn standing(t: &Obj) -> (Vec<String>, Option<String>) {
    let (mut left, mut broken) = (vec![], None);
    for id in after_of(t) {
        match stand_of(&id) { Stand::Done => {}, Stand::Open => left.push(id), Stand::Broken(w) => { left.push(id); broken.get_or_insert(w); } }
    }
    (left, broken)
}

/// The tasks `refs` names, as ids, for task `me` to wait for (None: a task being added, which nothing waits for yet).
/// A task of another machine is named by its machine first (`vm-str:ELA-3`) and asked for there. Refused, and nothing
/// changed, for a task or a machine that does not exist, a machine that does not answer, or a wait in a circle (or
/// one that cannot be followed to its end to know).
pub fn checked(me: Option<&str>, refs: &[String]) -> Res<Vec<String>> {
    let far_refs: Vec<(String, String, String)> = refs.iter().filter_map(|r| far::split(r).filter(|(p, _)| !circle::is_me(p)).map(|(p, rest)| (r.clone(), p, rest))).collect();
    let ms = if far_refs.is_empty() { vec![] } else { crate::remote::machines() };
    // Each machine asked once, about all of its tasks named.
    let mut answers: HashMap<String, Value> = HashMap::new();
    let mut by: Vec<(Machine, Vec<(String, String)>)> = vec![];
    for (r, p, rest) in &far_refs {
        let Some(m) = ms.iter().find(|m| m.label.eq_ignore_ascii_case(p)) else { crate::fail!("{r}: {p} is not a machine the board knows (ferrite machine list, or the board.machines setting): it cannot be waited for. Nothing was changed.") }; // Ferrite: herdr's machine list is Ferrite's.
        match by.iter_mut().find(|(x, _)| x.label == m.label) { Some((_, v)) => v.push((r.clone(), rest.clone())), None => by.push((m.clone(), vec![(r.clone(), rest.clone())])) }
    }
    for (m, named) in &by {
        let locals: Vec<String> = named.iter().map(|(_, l)| l.clone()).collect();
        let got = far_ask::ask(m, &locals).map_err(|why| crate::err::Error::new(format!("{} did not answer ({why}), so {} cannot be checked: try again once it does. Nothing was changed.", m.label, named.iter().map(|(r, _)| r.as_str()).collect::<Vec<_>>().join(", "))))?;
        for ((r, l), a) in named.iter().zip(got) {
            if !a.b("found") { crate::fail!("No task {l} on {}: {r} cannot be waited for. Nothing was changed.", m.label); }
            answers.insert(r.clone(), json!({ "label": m.label, "a": a }));
        }
    }
    let mut out: Vec<String> = vec![];
    for r in refs {
        let id = if let Some(x) = answers.get(r) { far::id_of(x.s("label"), x.at("a").s("id")) } else {
            let local = far::split(r).filter(|(p, _)| circle::is_me(p)).map(|(_, rest)| rest).unwrap_or_else(|| r.clone());
            let d = find(&local).map_err(|e| crate::err::Error::new(format!("{}: it cannot be waited for. Nothing was changed.", e.msg)))?;
            if d.b("attemptOf") { d.t("attemptOf") } else { d.t("id") } // an attempt is its task's
        };
        if !out.contains(&id) { out.push(id); }
    }
    if let Some(me) = me {
        let mut g = circle::Graph::new(me, &out, ms);
        for x in answers.values() { g.take_in(x.s("label"), x.at("a")); }
        let found = g.circle(me).map_err(|why| crate::err::Error::new(format!("{} cannot wait for that: whether tasks would wait on each other cannot be checked, because {why}. Nothing was changed.", say(me))))?;
        if let Some(path) = found {
            let p: Vec<String> = path.iter().map(|i| g.say(i)).collect();
            crate::fail!("{} cannot wait for that: tasks would wait on each other ({}). Nothing was changed.", say(me), p.join(" → "));
        }
    }
    for (m, _) in &by { far_ask::note_answers(&m.label, &answers.values().filter(|x| x.s("label") == m.label).map(|x| x.at("a").clone()).collect::<Vec<_>>()); }
    Ok(out)
}

/// What a delegated task that waits says it does: "is queued after a and starts by itself once that is done", or why
/// it waits for you. None for a task that does not wait.
pub fn waiting_words(t: &Obj) -> Option<String> {
    if t.s("stage") != STAGE { return None; }
    let left = strings(t.at("afterLeft"));
    Some(match t.so("afterBlocked") { Some(w) => format!("waits for you: {w}"), None => format!("is {} and starts by itself {}", waits_line(&left), once(left.len())) })
}
/// The task panel's line about waiting, from the task alone: its label ("Starts after", "Waited for", "To wait for" on
/// one not delegated yet) and each task with whether it is done.
pub fn panel_line(r: &Obj) -> Option<(&'static str, String)> {
    let all = after_of(r);
    if r.s("stage") == STAGE {
        let left = strings(r.at("afterLeft"));
        return Some(("Starts after", all.iter().map(|i| if left.contains(i) || !r.has("afterLeft") { say_on(r, i) } else { format!("{} (done)", say_on(r, i)) }).collect::<Vec<_>>().join(", ")));
    }
    let done = r.arr("afterDone");
    if !done.is_empty() { return Some(("Waited for", done.iter().map(|d| format!("{} ({})", say_on(r, d.s("id")), if d.b("done") { "done" } else { "not done" })).collect::<Vec<_>>().join(", "))); }
    (!all.is_empty()).then(|| ("To wait for", all.iter().map(|i| say_on(r, i)).collect::<Vec<_>>().join(", ")))
}
/// Said to a delegation of a task that already waits: it starts by itself, or early when asked.
pub fn still_waits(t: &Task) -> String {
    let id = t.t("id");
    let (left, broken) = standing(t);
    let now = match broken { Some(w) => format!("{} waits for you: {w}.", say(&id)), None => format!("{} is still {}: it starts by itself {}.", say(&id), waits_line(&left), once(left.len())) };
    format!("{now} To start it now anyway: herdr-projects delegate {id} --now. To stop it waiting: herdr-projects after {id} --clear")
}
/// What a task's wait comes to as its delegation is recorded (actions/start.rs), with `t` already queued: it waits
/// (stage "after", its line), or it goes on to a slot with what it waited for handed over.
pub fn on_delegate(t: &mut Task, now: bool) {
    for k in ["afterLeft", "afterBlocked", "afterDone", "afterEarly", "stoppedWaiting"] { t.del(k); }
    if after_of(t).is_empty() { t.del("after"); return; }
    let (left, broken) = standing(t);
    if now || left.is_empty() { handed::hand_over(t, now && !left.is_empty()); return; }
    t.put("stage", STAGE);
    t.put("msg", broken.clone().unwrap_or_else(|| waits_line(&left)));
    t.put("afterLeft", left);
    t.put("afterBlocked", broken.map(Value::from).unwrap_or(Value::Null));
}
