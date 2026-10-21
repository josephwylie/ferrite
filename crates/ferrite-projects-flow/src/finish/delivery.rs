//! Ferrite: recover delivery, not source work. A lost push response is reconciled against the exact remote ref;
//! transport retries are durable, bounded, and claimed through the ordinary pipeline runner.
use crate::clock::now;
use crate::err::Res;
use crate::flow;
use crate::json::*;
use crate::runner::{runner_alive, spawn_runner, Guard};
use crate::store::failures::is_auth_failure;
use crate::store::repos::{branch_of, git_slow};
use crate::store::tasks::{update_wait, Change, Task};
use std::path::Path;

const DELAYS: [i64; 3] = [5_000, 15_000, 45_000];
pub fn timeout(s: &str) -> bool {
    !is_auth_failure(s) && (s.contains("herdr did not answer in time") || s.contains("Connection timed out") || s.contains("Operation timed out"))
}
fn eligible(t: &Task) -> bool {
    if t.b("archivedAt") || t.b("prMerged") || runner_alive(t) || t.b("turnOpen") || t.b("chat") || !t.b("summary") || !flow::task::is_agent_task(t) { return false; }
    let f = t.at("failure");
    let delivery_failure = f.s("stage") == "push" ||
        (f.s("stage") == "merge" && f.s("phase") == "fetch" && t.at("deliveryRetry").is_object());
    (t.s("stage") == "pushing" && t.at("deliveryRetry").b("until")) ||
        (t.s("stage") == "failed" && delivery_failure && !f.b("auth") && timeout(f.s("detail")) && !t.at("deliveryRetry").b("exhausted"))
}
/// Re-enter the normal check/review pipeline, once. The stage transition is the queue reservation, under the task
/// lock; a second tick cannot enqueue it again. No agent is started or prompted here.
pub fn recover(tasks: &[Task], socket: &str) {
    for seen in tasks {
        if seen.at("link").s("socket") != socket || !eligible(seen) || now() < seen.at("deliveryRetry").i("until") { continue; }
        let mut go = false;
        let _ = update_wait(seen.s("id"), 10000, |t| {
            if t.at("link").s("socket") != socket || !eligible(t) || now() < t.at("deliveryRetry").i("until") { return Ok(Change::Keep); }
            let tree = Path::new(t.s("worktree"));
            if !tree.is_dir() || branch_of(tree) != t.s("branch") || !flow::git::dirty(tree).is_empty() { return Ok(Change::Keep); }
            if !t.at("deliveryRetry").is_object() { t.put("deliveryRetry", json!({"head": flow::git::head_of(tree), "tries": 0})); }
            t.put("stage", "checking"); t.put("msg", "Reconciling the branch after a push timeout");
            t.put("failure", Value::Null); go = true;
            Ok(Change::Write)
        });
        if go { spawn_runner("pipeline", seen.s("id"), &[]); }
    }
}
fn remote(tree: &Path, branch: &str) -> Res<String> {
    let reference = format!("refs/heads/{branch}");
    let out = git_slow(tree, &["ls-remote", "--refs", "origin", &reference], 30_000)?;
    Ok(out.lines().filter_map(|l| l.split_once('\t')).find(|(_, r)| *r == reference).map(|(h, _)| h.to_string()).unwrap_or_default())
}
/// True once the exact commit is confirmed at origin; false while a timeout backs off. Never force a retry past
/// another writer: git's lease still applies. A successful response is also checked, rather than trusting a cached ref.
pub fn push(g: &mut Guard, tree: &Path, branch: &str) -> Res<bool> {
    let head = flow::git::head_of(tree);
    if head.is_empty() { crate::fail!("Cannot determine the commit to push"); }
    let result = (|| -> Res<()> {
        if remote(tree, branch)? == head { return Ok(()); }
        g.check()?; // the remote read may have outlasted this turn
        let pushed = git_slow(tree, &["push", "-u", "--force-with-lease", "origin", branch], 120_000);
        g.check()?;
        match pushed {
            Ok(_) => {}
            Err(e) if timeout(&e.msg) => { if remote(tree, branch).ok().as_deref() == Some(&head) { return Ok(()); } return Err(e); }
            Err(e) => return Err(e),
        }
        if remote(tree, branch)? != head { crate::fail!("The remote branch does not match the commit pushed; inspect it before retrying"); }
        Ok(())
    })();
    match result {
        Ok(()) => {
            g.set(|t| { t.put("prHead", head.clone()); t.put("deliveryRetry", Value::Null); })?;
            Ok(true)
        }
        Err(e) if timeout(&e.msg) => {
            if backoff(g, &head, "Push response", &e.msg)? { Ok(false) }
            else { Err(crate::err::Error::new(format!("Delivery transport retries exhausted after {} retries: {}", DELAYS.len(), e.msg))) }
        }
        Err(e) => Err(e),
    }
}

/// Fetch and push are parts of one delivery attempt. A preliminary fetch must consume the same budget instead
/// of stranding the task or starting a fresh retry sequence. True means the runner should yield until the deadline.
pub(super) fn backoff(g: &mut Guard, head: &str, step: &str, detail: &str) -> Res<bool> {
    let t = g.check()?;
    let r = t.at("deliveryRetry");
    let tries = if r.s("head") == head { r.i("tries") } else { 0 };
    if let Some(delay) = DELAYS.get(tries as usize) {
        let t = g.set(|t| {
            t.put("stage", "pushing"); t.put("failure", Value::Null);
            t.put("deliveryRetry", json!({"head": head, "tries": tries + 1, "until": now() + delay, "detail": detail}));
            t.put("msg", format!("{step} timed out; reconciling and retrying in {}s ({} of {})", delay / 1000, tries + 1, DELAYS.len()));
        })?;
        flow::sidebar::tell_sidebar(&t);
        Ok(true)
    } else {
        g.set(|t| { t.obj_mut("deliveryRetry").put("exhausted", true); })?;
        Ok(false)
    }
}
