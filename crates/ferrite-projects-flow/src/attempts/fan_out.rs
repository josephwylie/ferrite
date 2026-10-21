//! Starting several attempts: a queued task whose `attempts.by` says more than one gets one small task per attempt,
//! each queued for a slot of its own, and stands as "attempts" until they are through.
use super::adopt::discard;
use crate::clock::now;
use crate::err::Res;
use crate::json::*;
use crate::store::attempts::{attempt_agents, attempts_of, create_attempt, LETTERS};
use crate::store::repos::{can_worktree, folder_of};
use crate::store::settings::lookup::{setting, setting_text};
use crate::store::settings::values::agent_of;
use crate::store::tasks::{get, update_wait, Change, Task};

/// Read now, as the task starts: who makes each attempt ([] when the task is done once). A task in no repo, one that
/// already has its worktree, or one that starts with a plan (split after the plan's yes) is never split here.
pub fn attempts_for(t: &Task) -> Vec<String> {
    if t.b("attemptOf") || t.b("startedOnce") || !can_worktree(&folder_of(t.s("id"))) { return vec![]; }
    if setting_text("plan.approve", Some(t)) != "none" { return vec![]; }
    attempt_agents(&setting("attempts.by", Some(t)), &setting_text("worker.agent", Some(t)))
}
/// The task as it now stands, or None when it is done once. Attempts of an earlier round go first.
pub fn fan_out(id: &str) -> Res<Option<Task>> {
    let Some(cur) = get(id) else { return Ok(None) };
    if cur.s("stage") != "queued" { return Ok(None); }
    let agents = attempts_for(&cur);
    if agents.is_empty() { return Ok(None); }
    for old in attempts_of(&cur) { discard(&cur, &old); }
    let base = if cur.b("queuedAt") { cur.i("queuedAt") } else { now() };
    let mut made = vec![];
    for (n, agent) in agents.iter().enumerate() {
        let k = agent_of(agent).kind;
        made.push(create_attempt(&cur, LETTERS[n], agent, obj(json!({ "queuedAt": base + n as i64, "link": { "socket": cur.at("link").at("socket") }, "agent": k, "app": k })))?);
    }
    Ok(Some(update_wait(id, 2000, |t| {
        if t.s("stage") != "queued" { return Ok(Change::Keep); }
        t.put("stage", "attempts"); t.put("msg", ""); t.put("since", now()); t.put("app", Value::Null);
        t.put("attempts", made.iter().map(|c| json!({ "id": c.at("id"), "letter": c.at("letter"), "agent": c.at("settings").at("worker.agent") })).collect::<Vec<_>>());
        t.put("picked", Value::Null); t.put("comparison", Value::Null); t.del("as");
        let s = t.at("link").at("socket").clone(); t.put("link", json!({ "socket": s }));
        Ok(Change::Write)
    })?))
}
