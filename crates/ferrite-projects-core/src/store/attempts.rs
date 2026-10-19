//! Several attempts at one task, as kept on disk: what `attempts.by` says (who makes each attempt), an attempt's letter
//! and id, and the attempts of a task. An attempt is a small task of its own in a folder beside its task's (`<repo>/12a`
//! for task `<repo>/12`): it has its own agent, worktree, branch and tab and goes through the check like any task, but
//! it is never a row of the board, and the task it belongs to shows it (attempts/ does the rest).
use super::files::*;
use super::tasks::{read_dir_names, Task};
use crate::clock::now;
use crate::err::Res;
use crate::json::*;

/// Five at most: every attempt is a whole agent with its own worktree, holds one of `board.agents`' slots and costs
/// what the task costs when it is done once.
pub const MAX_ATTEMPTS: usize = 5;
pub const LETTERS: [&str; 5] = ["A", "B", "C", "D", "E"];
pub const TOO_MANY: &str = "at most 5: every attempt is a whole agent with its own worktree, holds one of board.agents' slots and costs what the task costs when it is done once";

/// `attempts.by` as kept (a number, or "claude, codex/gpt-5.5, pi") -> the agents, one per attempt. A count means
/// that many of the working agent. One attempt is the task done the usual way: [].
pub fn attempt_agents(by: &Value, worker: &str) -> Vec<String> {
    let s = text(by);
    if by.is_number() || (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())) {
        let n = (number(by) as usize).min(MAX_ATTEMPTS);
        return if n > 1 { vec![worker.to_string(); n] } else { vec![] };
    }
    let names: Vec<String> = s.split(|c: char| c.is_whitespace() || c == ',').filter(|x| !x.is_empty()).take(MAX_ATTEMPTS).map(String::from).collect();
    if names.len() > 1 { names } else { vec![] }
}
pub fn attempt_count(by: &Value) -> usize { attempt_agents(by, "x").len().max(1) }
/// `herdr-projects/12` and `B` -> `herdr-projects/12b`.
pub fn attempt_id(id: &str, letter: &str) -> String { format!("{id}{}", letter.to_lowercase()) }
pub fn is_attempt_id(id: &str) -> bool {
    let Some((repo, n)) = id.split_once('/') else { return false };
    if repo.is_empty() || n.contains('/') || n.len() < 2 { return false; }
    let (digits, last) = n.split_at(n.len() - 1);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) && "abcde".contains(last)
}
fn file_of(id: &str) -> std::path::PathBuf { dir_of(id).join("task.json") }
/// The attempts of a task, in the order of their letters: the sub-tasks as they are on disk now.
pub fn attempts_of(t: &Task) -> Vec<Task> {
    t.arr("attempts").iter().filter_map(|a| read_obj(&file_of(a.s("id")))).filter(|c| c.b("id")).collect()
}
/// Every attempt on disk, of every task.
pub fn all_attempts() -> Vec<Task> {
    let mut out = vec![];
    for repo in read_dir_names(&tasks_dir()) {
        for n in read_dir_names(&tasks_dir().join(&repo)) {
            if !is_attempt_id(&format!("{repo}/{n}")) { continue; }
            if let Some(t) = read_obj(&file_of(&format!("{repo}/{n}"))) { if t.b("id") && t.b("attemptOf") { out.push(t); } }
        }
    }
    out
}
/// A new attempt of a task: saved as queued, with its own program and nothing else of its own.
pub fn create_attempt(task: &Task, letter: &str, agent: &str, extra: Obj) -> Res<Task> {
    let id = attempt_id(task.s("id"), letter);
    let now = now();
    with_lock(&format!("task:{id}"), || {
        let _ = std::fs::remove_dir_all(dir_of(&id)); // an attempt of an earlier round, already cleaned up
        let mut t = obj(json!({ "id": id, "repo": task.at("repo"), "n": format!("{}{}", task.t("n"), letter.to_lowercase()), "root": task.at("root"),
            "title": task.at("title"), "description": task.at("description"), "owner": task.at("owner"), "attemptOf": task.at("id"), "letter": letter }));
        if task.b("automation") { t.put("automation", task.at("automation").clone()); }
        if task.b("carried") { t.put("carried", task.at("carried").clone()); }
        // What the task waited for, handed over to each attempt's agent as to the task's own (after/handed.rs).
        for k in ["afterDone", "afterEarly"] { if task.b(k) { t.put(k, task.at(k).clone()); } }
        for (k, v) in obj(json!({ "stage": "queued", "msg": "", "created": now, "at": now, "since": now, "pinned": false, "archivedAt": null, "settings": { "worker.agent": agent } })) { t.insert(k, v); }
        for (k, v) in extra { t.insert(k, v); }
        write_json(&file_of(&id), &Value::Object(t.clone()))?;
        Ok(t)
    })
}
pub fn remove_attempt(id: &str) { if is_attempt_id(id) { let _ = std::fs::remove_dir_all(dir_of(id)); } }
