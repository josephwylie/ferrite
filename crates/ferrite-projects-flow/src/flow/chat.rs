//! A chat with the agent of a task whose work is reviewed: what the last review covered is kept, so a chat that
//! changes no code changes nothing else either (docs/how-work-gets-done.md, "Chatting with a finished task"). The
//! task's `chat` record: when it began and when you last spoke (`at`, `last`), how (`via`: board or terminal), the
//! commit the review covered (`head`), the task as it stood (`from`), `during` while a review that was under way when
//! it began goes on, `held` once the agent's turn ended with changes (until you are done), and `left` when you left it.
use crate::clock::now;
use crate::json::*;
use crate::store::tasks::Task;
use std::path::Path;

/// What a chat that changed no code puts back: the task as it stood, its review and its counts.
const KEPT: [&str; 16] = ["stage", "msg", "kind", "review", "requireApproval", "mergeIntent", "tidy", "checkFails", "reviewRounds", "prRounds", "syncRepairs", "mergeFailed", "failure", "since", "syncBlocked", "watch"];
/// The stages in which finished work stands as it was reviewed.
const AT_REST: [&str; 2] = ["approval", "done"];
const UNDER_REVIEW: [&str; 2] = ["reviewing", "reviewneeds"];
/// The row of a task whose chat changed its code, until you are done.
pub const HELD: &str = "Changes made · review starts when you're done";
/// The row of a task whose chat changed nothing, after its review's result.
pub const KEPT_LINE: &str = "Chatting · no changes, review kept";

/// What a human turn that begins does to the task. `During`: a review under way goes on, the stage stays. `Rest`: the
/// turn re-arms the task as always, with what to put back kept. `No`: as always, nothing kept.
#[derive(PartialEq, Debug)]
pub enum Began { During, Rest, No }

/// The code as the review left it: on that commit, with nothing uncommitted.
pub fn same_code(t: &Obj, head: &str) -> bool {
    let tree = crate::sync::tree_of(t);
    if tree.is_empty() || head.is_empty() { return false; }
    let p = Path::new(&tree);
    crate::flow::git::head_of(p) == head && crate::flow::git::dirty(p).is_empty()
}
fn snapshot(t: &Task) -> Value { Value::Object(KEPT.iter().filter(|k| t.has(**k)).map(|k| (k.to_string(), t.at(k).clone())).collect()) }
/// The commit the work stands reviewed at: the reviewed one, or (work with nothing reviewed, done) the one it is on.
fn rest_head(t: &Task) -> String {
    if t.b("reviewedHead") { return t.t("reviewedHead"); }
    let tree = crate::sync::tree_of(t);
    if tree.is_empty() || !crate::flow::git::dirty(Path::new(&tree)).is_empty() { return String::new(); }
    crate::flow::git::head_of(Path::new(&tree))
}
/// A human turn begins: your words (`via`: "board", or "terminal" for the agent's own pane), or a turn of yours seen
/// only as it ended (`via` ""). `running`: a review runner is at work on the task right now.
pub fn begin(t: &mut Task, via: &str, running: bool) -> Began {
    let n = now();
    t.del("chatKept");
    if t.at("chat").is_object() {
        let at_rest = AT_REST.contains(&t.s("stage"));
        // The review that was under way when the chat began is over: the chat keeps what it left.
        if t.at("chat").b("during") && !running && at_rest {
            let (from, head) = (snapshot(t), rest_head(t));
            let c = t.obj_mut("chat"); c.del("during"); c.put("from", from); c.put("head", head);
        }
        let c = t.obj_mut("chat");
        c.del("held"); c.del("left");
        if !via.is_empty() { c.put("last", n); }
        if via == "board" { c.put("via", via); } // what herdr sees of words from the board is no chat in the terminal
        return if c.b("during") && running && UNDER_REVIEW.contains(&t.s("stage")) { Began::During } else { Began::Rest };
    }
    let via = if via.is_empty() { "terminal" } else { via };
    if crate::sync::tree_of(t).is_empty() { return Began::No; }
    if running && UNDER_REVIEW.contains(&t.s("stage")) {
        let head = crate::flow::git::head_of(Path::new(&crate::sync::tree_of(t)));
        t.put("chat", json!({ "at": n, "last": n, "via": via, "head": head, "during": true }));
        return Began::During;
    }
    // Approved and waiting to merge (for GitHub's checks): the approval goes with any chat, as the merge it waited
    // for is called off; a change the chat makes is still reviewed only once you are done.
    let waits_to_merge = t.s("stage") == "merging" && !running;
    if !AT_REST.contains(&t.s("stage")) && !waits_to_merge { return Began::No; }
    let head = rest_head(t);
    if head.is_empty() { return Began::No; }
    if waits_to_merge {
        t.put("chat", json!({ "at": n, "last": n, "via": via, "head": head }));
        return Began::Rest;
    }
    let from = snapshot(t);
    t.put("chat", json!({ "at": n, "last": n, "via": via, "head": head, "from": from }));
    Began::Rest
}
/// The task as it stood before the chat, put back: its stage, its review, its counts. The chat is over.
pub fn put_back(t: &mut Task) {
    let from = t.at("chat").at("from").as_object().cloned().unwrap_or_default();
    for k in KEPT { match from.get(k) { Some(v) => t.put(k, v.clone()), None => t.del(k) } }
    t.del("chat");
    t.put("chatKept", now());
}
/// A review under way when the chat began goes on: the agent's turn changed nothing it looks at.
pub fn review_goes_on(t: &Obj) -> bool { t.at("chat").b("during") && UNDER_REVIEW.contains(&t.s("stage")) && same_code(t, t.at("chat").s("head")) }
/// The chat began while a review was under way, and the agent's turn in it is not handled yet: what the reviewer
/// changed is not put back, it may be the agent's.
pub fn during(t: &Obj) -> bool { t.at("chat").b("during") }
/// The agent's turn in the chat ended with changes, and the board waits for you to be done.
pub fn held(t: &Obj) -> bool { t.s("stage") == "working" && t.at("chat").b("held") }
