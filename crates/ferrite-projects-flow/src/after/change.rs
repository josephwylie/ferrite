//! `herdr-projects after <id> [<id>,…] [--clear]`: what a task waits for, said, changed or dropped on a task that
//! exists. A task that waits already goes on waiting for the new list (or starts, when all of it is done); one dropped
//! from waiting goes on to a slot at once. A task an agent is on cannot be made to wait: the wait is before the start.
use super::{after_of, checked, names, pass::refresh, say, standing, STAGE};
use crate::actions::shared::is_agent_stage;
use crate::actions::start::start_queued;
use crate::clock::now;
use crate::err::Res;
use crate::herdr::cli::current_socket;
use crate::json::*;
use crate::runner::runner_alive;
use crate::store::tasks::{assert_unarchived, find, get, update, Change, Task};

/// What a task waits for, in words: each with whether it is done.
pub fn said(t: &Task) -> String {
    let all = after_of(t);
    if all.is_empty() { return format!("{}: waits for nothing", t.s("id")); }
    let (left, broken) = standing(t);
    let each: Vec<String> = all.iter().map(|i| if left.contains(i) { say(i) } else { format!("{} (done)", say(i)) }).collect();
    format!("{}: {} for {}{}", t.s("id"), if t.s("stage") == STAGE { "waits" } else { "is to wait" }, each.join(", "), broken.map(|w| format!(". It waits for you: {w}")).unwrap_or_default())
}
/// `refs`: the tasks to wait for from now (None: show what it waits for; an empty list: wait for nothing).
pub fn change(id: &str, refs: Option<Vec<String>>) -> Res<(Task, String)> {
    let cur = find(id)?;
    let cid = cur.t("id");
    let Some(refs) = refs else { return Ok((cur.clone(), said(&cur))) };
    assert_unarchived(&cur)?;
    let deps = checked(Some(&cid), &refs)?;
    let t = update(&cid, |t| {
        assert_unarchived(t)?;
        let st = t.s("stage").to_string();
        if (is_agent_stage(&st) && st != "queued") || runner_alive(t) { crate::fail!("{cid} already has an agent on it: a task waits before it starts. Stop it first, or say it when you delegate it again."); }
        for k in ["afterLeft", "afterBlocked", "afterDone", "afterEarly"] { t.del(k); }
        if deps.is_empty() {
            t.del("after");
            if st == STAGE { t.put("stage", "queued"); t.put("queuedAt", now()); t.put("msg", ""); }
        } else {
            t.put("after", deps.clone());
            if st == "queued" { t.put("stage", STAGE); }
        }
        Ok(Change::Write)
    })?;
    let socket = if t.at("link").b("socket") { t.at("link").t("socket") } else { current_socket() };
    let started = refresh(&cid) || t.s("stage") == "queued";
    if started { start_queued(&socket); }
    let t = get(&cid).unwrap_or(t);
    let msg = if deps.is_empty() { format!("{cid}: waits for nothing{}", if started { ", so it starts now" } else { "" }) }
        else if t.s("stage") == STAGE { format!("{}; it starts by itself once {} {} done", said(&t), if deps.len() == 1 { "that" } else { "they" }, if deps.len() == 1 { "is" } else { "are" }) }
        else if started { format!("{cid}: everything it waits for ({}) is done, so it starts now", names(&deps)) }
        else { format!("{}, from when it is delegated", said(&t)) };
    Ok((t, msg))
}
