//! What you do to a task a limit stopped: carry on, or take its work as it is. (A reply to its agent carries on too,
//! with your own words: actions/talk.rs.)
use super::finish_line::send_review;
use super::shared::{say, Said};
use super::talk::tell;
use crate::clock::now;
use crate::err::Res;
use crate::flow;
use crate::gate;
use crate::json::*;
use crate::limits::run::{granted, stopped_at_limit, used_say};
use crate::runner::{runner_alive, spawn_runner};
use crate::store::tasks::{assert_unarchived, find, get, update, update_wait, Change, Task};

fn what(ls: &Value) -> String { if ls.s("what") == "time" { format!("{} for one run", ls.s("limit")) } else { format!("{} tokens", ls.s("limit")) } }
fn grant_or_lift(t: &mut Task, raised: bool) { if raised { t.put("limitStop", Value::Null); t.put("runCarry", Value::Null); } else { granted(t); } }
fn busy(t: &Task) -> Res<()> { if !stopped_at_limit(t) || runner_alive(t) { crate::fail!("{} is busy. Try again in a moment.", t.s("id")); } Ok(()) }
/// Carry on from where it stopped. `raised`: the board does it, because the limit was raised or taken away: the run then
/// goes on from the time it had taken. Said by you, the task has its limit once more.
pub fn carry_on(id: &str, raised: bool) -> Res<Said> {
    let cur = find(id)?;
    assert_unarchived(&cur)?;
    let cid = cur.t("id");
    if !stopped_at_limit(&cur) { crate::fail!("{cid} was not stopped at a limit."); }
    let ls = cur.at("limitStop").clone();
    let done = |out: &Said, msg: String| { let t = get(&cid).unwrap_or_else(|| out.task.clone()); say(t, if out.msg.starts_with("queued") { out.msg.clone() } else { msg }) };
    // A turn the board starts because the limit moved does not give it its limit once more: the mark is taken off
    // first, and put back if nothing could be sent.
    let own = |send: &dyn Fn() -> Res<Said>| -> Res<Said> {
        if !raised { return send(); }
        update_wait(&cid, 10000, |t| { t.put("limitStop", Value::Null); Ok(Change::Write) })?;
        send().inspect_err(|_| { let _ = update_wait(&cid, 10000, |t| { if t.s("stage") != "stopped" { return Ok(Change::Keep); } t.put("limitStop", ls.clone()); Ok(Change::Write) }); })
    };
    if ls.b("ready") && ls.b("done") { // nothing to land: it was reviewed, and is done
        let mark = flow::spaces::tidy_mark(&cur, "merged", "");
        let t = update(&cid, |t| { busy(t)?; grant_or_lift(t, raised); t.put("stage", "done"); t.put("tidy", mark.clone().unwrap_or(Value::Null)); t.put("msg", ls.at("done").clone()); t.put("failure", Value::Null); t.put("mergeIntent", Value::Null); t.put("since", now()); Ok(Change::Write) })?;
        flow::sidebar::tell_sidebar(&t);
        spawn_runner("cleanup", &cid, &["--merged"]);
        return Ok(say(t, format!("{cid}: carrying on, it is done")));
    }
    if ls.b("ready") {
        let t = update(&cid, |t| {
            busy(t)?; grant_or_lift(t, raised); t.put("stage", "approval"); t.put("since", now());
            let msg = if t.b("syncBlocked") { crate::sync::behind_msg(t.at("syncBlocked").s("why")) } else { crate::finish::shared::approval_message(t) };
            t.put("msg", msg);
            Ok(Change::Write)
        })?;
        flow::sidebar::tell_sidebar(&t);
        return Ok(say(t, format!("{cid}: carrying on, its work waits for approval")));
    }
    if ls.s("role") == "reviewer" {
        if ls.b("review") && cur.at("review").b("text") && gate::sendable(cur.at("review")) {
            let out = own(&|| send_review(&cid))?;
            return Ok(done(&out, format!("{cid}: carrying on, the review went to the agent")));
        }
        flow::live::assert_live(&cur)?;
        let t = update(&cid, |t| { busy(t)?; grant_or_lift(t, raised); t.put("stage", "checking"); t.put("msg", ""); t.put("since", now()); Ok(Change::Write) })?;
        spawn_runner("pipeline", &cid, &[]);
        return Ok(say(t, format!("{cid}: carrying on, it is reviewed again")));
    }
    let words = flow::send::with_tail(&cur, &format!("You were stopped because this task reached its limit ({}), not because of anything you did. {}: continue the task where you left off.", what(&ls), if raised { "The limit was raised" } else { "The human said to carry on" }));
    let out = own(&|| tell(&cid, &words, false))?;
    // The time the run had taken rides on when it is the limit that moved, not you.
    if raised && ls.s("what") == "time" {
        let _ = update_wait(&cid, 10000, |t| { if t.s("stage") != "working" { return Ok(Change::Keep); } let from = t.i("since").max(t.i("workAt")).max(t.at("expectedWake").i("at")); t.put("runCarry", json!({ "ms": ls.at("used"), "from": from })); Ok(Change::Write) });
    }
    Ok(done(&out, format!("{cid}: told the agent to continue")))
}
/// Take the work as it is: through the check and the review, like any finished turn.
pub fn take_as_is(id: &str) -> Res<Said> {
    let cur = find(id)?;
    let cid = cur.t("id");
    if cur.at("limitStop").s("role") == "reviewer" { return carry_on(&cid, false); }
    flow::live::assert_live(&cur)?;
    // The turn the stop cut short is the one taken here: what herdr counts of it later is not a new turn of yours.
    let a = flow::task::h(&cur).agent_get(cur.at("link").s("pane")).ok().flatten();
    let note = flow::task::note_stat(&cur).map(|n| n.0);
    let t = update(&cid, |t| {
        if !stopped_at_limit(t) || runner_alive(t) { crate::fail!("{} is busy.", t.s("id")); }
        granted(t); t.put("stage", "checking"); t.put("msg", ""); t.put("since", now()); t.put("armedAt", now());
        let h = t.obj_mut("handled");
        if let Some(a) = &a { h.put("comp", a.at("completion_seq").clone()); h.put("seq", a.at("state_change_seq").clone()); }
        if let Some(n) = note { h.put("note", n); }
        h.put("at", now());
        Ok(Change::Write)
    })?;
    spawn_runner("pipeline", &cid, &[]);
    Ok(say(t, format!("{cid}: taking the work as it is ({})", used_say(cur.at("limitStop")))))
}
