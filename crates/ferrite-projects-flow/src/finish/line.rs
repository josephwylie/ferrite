//! The line: a repo's tasks merge one at a time. One catches up with main, is checked and merges, then the next.
//! Without it every merge moves main under all the others at once: each catches up and runs the whole check, one of
//! them merges, and the rest start over.
use crate::clock::now;
use crate::err::Res;
use crate::json::*;
use crate::runner::{runner_alive, spawn_runner, Guard};
use crate::store::files::with_lock_wait;
use crate::store::repos::folder_of;
use crate::store::tasks::{all, Task};

/// The stages of the last stretch: catching up with main, the check on the result, the push, the merge.
const LAST_STRETCH: [&str; 3] = ["merging", "checking", "pushing"];
/// At the head of the line: the turn is its own, taken by the very runner that is at work on it now.
fn at_head(t: &Task) -> bool { t.b("lineTurn") && t.at("lineTurn") == t.at("runner").at("started") && runner_alive(t) && LAST_STRETCH.contains(&t.s("stage")) && !t.b("archivedAt") }
/// Waiting in the line: it may merge, and another task goes first.
pub fn in_line(t: &Task) -> bool { t.s("stage") == "merging" && t.b("lineAt") && !t.b("autoMerge") && !t.b("archivedAt") && !at_head(t) }
fn num_key(id: &str) -> (String, u64) { let (r, n) = id.split_once('/').unwrap_or((id, "0")); (r.to_string(), n.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)) }
fn by_arrival(a: (i64, &str), b: (i64, &str)) -> std::cmp::Ordering { a.0.cmp(&b.0).then_with(|| num_key(a.1).cmp(&num_key(b.1))) }
fn of_repo(root: &std::path::Path) -> Vec<Task> { all().into_iter().filter(|t| !t.b("archivedAt") && folder_of(t.s("id")) == root).collect() }
/// Asked by the land step before it catches up with main. True: this task goes now, and holds the turn until its
/// runner ends. False: another task of the repo goes first, and this one waits where it is.
pub fn take_turn(id: &str, g: &mut Guard) -> Res<bool> {
    let root = folder_of(id);
    with_lock_wait(&format!("line:{}", root.display()), 10000, || {
        let me = g.check()?;
        if at_head(&me) { return Ok(true); }
        let others: Vec<Task> = of_repo(&root).into_iter().filter(|t| t.s("id") != id).collect();
        let mine_at = if me.b("lineAt") { me.i("lineAt") } else { now() };
        let mut waiting: Vec<&Task> = others.iter().filter(|t| in_line(t)).collect();
        waiting.sort_by(|a, b| by_arrival((a.i("lineAt"), a.s("id")), (b.i("lineAt"), b.s("id"))));
        let first = others.iter().find(|t| at_head(t)).or_else(|| waiting.into_iter().find(|t| by_arrival((t.i("lineAt"), t.s("id")), (mine_at, id)).is_lt()));
        match first {
            None => { g.set(|t| { let s = t.at("runner").at("started").clone(); t.put("lineTurn", s); t.put("lineAt", Value::Null); })?; Ok(true) }
            Some(f) => { let fid = f.t("id"); g.set(|t| { t.put("lineTurn", Value::Null); t.put("lineAt", mine_at); t.put("msg", waiting_msg(&fid)); })?; Ok(false) }
        }
    })
}
pub fn waiting_msg(first: &str) -> String { format!("Waiting to merge: {first} goes first") }
/// The line moves: where nobody is at the head, the task that waited longest is started (by the housekeeping pass).
pub fn line_pass(tasks: &[Obj]) {
    let mut roots: Vec<std::path::PathBuf> = vec![];
    for t in tasks { if in_line(t) { let r = folder_of(t.s("id")); if !roots.contains(&r) { roots.push(r); } } }
    for root in roots {
        let all = of_repo(&root);
        if all.iter().any(at_head) { continue; }
        let mut waiting: Vec<&Task> = all.iter().filter(|t| in_line(t)).collect();
        waiting.sort_by(|a, b| by_arrival((a.i("lineAt"), a.s("id")), (b.i("lineAt"), b.s("id"))));
        if let Some(next) = waiting.first() { if !runner_alive(next) { spawn_runner("land", next.s("id"), &[]); } }
    }
}
