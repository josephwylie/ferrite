//! The one-time move from the old settings: a run that finds your file in the old shape converts your config.json (a
//! copy of the old file is kept beside it as config.old.json), each task's own settings and what each task took from
//! its repo's file. After it the old names are gone. A task under way keeps the choices it had.
use super::places::config_file;
use super::values::check;
use crate::json::*;
use crate::store::files::*;
use crate::store::tasks::read_dir_names;
use std::path::PathBuf;

const OFF: [&str; 4] = ["false", "off", "no", "none"];
const ON: [&str; 4] = ["true", "on", "yes", "auto"];
fn off(v: &Value) -> bool { *v == json!(false) || OFF.contains(&text(v).to_lowercase().as_str()) }
fn on(v: &Value) -> bool { *v == json!(true) || ON.contains(&text(v).to_lowercase().as_str()) }
fn has(v: &Value) -> bool { !(v.is_null() || *v == json!("")) }
fn with_model(kind: &str, model: &Value) -> String { if has(model) { format!("{kind}/{}", text(model)) } else { kind.to_string() } }

/// Old names in, new names out. `builtin`: the old built-in programs, for a model that was set without its program.
pub fn moved_values(o: &Obj, builtin: &[(&str, String)]) -> Obj {
    let mut out = Obj::new();
    let b = |k: &str| builtin.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone()).unwrap_or_default();
    let mut put = |name: &str, v: Value, out: &mut Obj| { if let Ok(x) = check(name, &v, "") { out.insert(name.into(), x); } };
    let agent = |name: &str, kind: &Value, model: &Value, fallback: &str, out: &mut Obj, put: &mut dyn FnMut(&str, Value, &mut Obj)| {
        if has(kind) && !off(kind) { put(name, json!(with_model(&text(kind).to_lowercase(), model)), out); }
        else if has(model) && !fallback.is_empty() { put(name, json!(with_model(fallback, model)), out); }
    };
    agent("worker.agent", o.at("agent"), o.at("model"), &b("agent"), &mut out, &mut put);
    if has(o.at("effort")) { put("worker.effort", o.at("effort").clone(), &mut out); }
    if o.contains_key("fast") { put("worker.fast", json!(on(o.at("fast"))), &mut out); }
    if has(o.at("reviewer")) && off(o.at("reviewer")) { put("review.when", json!("never"), &mut out); } else { agent("reviewer.agent", o.at("reviewer"), o.at("reviewerModel"), &b("reviewer"), &mut out, &mut put); }
    if has(o.at("reviewerEffort")) { put("reviewer.effort", o.at("reviewerEffort").clone(), &mut out); }
    if o.contains_key("reviewerFast") { put("reviewer.fast", json!(on(o.at("reviewerFast"))), &mut out); }
    if has(o.at("coordinator")) && off(o.at("coordinator")) { put("coordinator.agent", json!("none"), &mut out); } else { agent("coordinator.agent", o.at("coordinator"), o.at("coordinatorModel"), &b("coordinator"), &mut out, &mut put); }
    if has(o.at("coordinatorEffort")) { put("coordinator.effort", o.at("coordinatorEffort").clone(), &mut out); }
    let w_agent = if o.contains_key("watcherAgent") { o.at("watcherAgent") } else { o.at("helper") };
    let w_model = if o.contains_key("watcherModel") { o.at("watcherModel") } else { o.at("helperModel") };
    if has(o.at("watcher")) && off(o.at("watcher")) { put("watcher.agent", json!("none"), &mut out); }
    else if has(w_agent) || has(w_model) {
        let kind = if has(w_agent) { text(w_agent).to_lowercase() } else { "claude".into() };
        let model = if has(w_model) { w_model.clone() } else if has(w_agent) { json!("") } else { json!("claude-haiku-5-5") };
        put("watcher.agent", json!(with_model(&kind, &model)), &mut out);
    }
    if has(o.at("watcherEvery")) { put("watcher.every", json!(format!("{}s", o.f("watcherEvery").round() as i64)), &mut out); }
    if has(o.at("review")) && !out.contains_key("review.when") {
        let w = match text(o.at("review")).to_lowercase().as_str() { "all" | "always" | "everything" => "always", "none" | "off" | "false" | "no" | "never" => "never", _ => "repos" };
        put("review.when", json!(w), &mut out);
    }
    // The old gate: a severity, or "off" (and the older reviewAuto: false), which sent nothing back by itself.
    let gate = if has(o.at("gate")) { text(o.at("gate")).to_lowercase() } else if o.contains_key("reviewAuto") { if on(o.at("reviewAuto")) { "major".into() } else { "off".into() } } else { String::new() };
    if gate == "off" || gate == "manual" || OFF.contains(&gate.as_str()) { put("review.rounds", json!(0), &mut out); } else if !gate.is_empty() { put("review.mustFix", json!(gate), &mut out); }
    if has(o.at("rounds")) && !out.contains_key("review.rounds") { put("review.rounds", json!(o.f("rounds")), &mut out); }
    if o.contains_key("artifacts") { put("review.evidence", json!(on(o.at("artifacts"))), &mut out); }
    if has(o.at("base")) { put("workspace.from", json!(if o.s("base") == "head" { "here" } else { "main" }), &mut out); }
    if o.contains_key("cleanup") { put("workspace.cleanup", json!(on(o.at("cleanup"))), &mut out); }
    if o.contains_key("pr") { put("merge.pr", json!(on(o.at("pr"))), &mut out); }
    if has(o.at("merge")) { put("merge.method", o.at("merge").clone(), &mut out); }
    if o.contains_key("autoMerge") && !o.at("autoMerge").is_null() { put("merge.by", json!(if on(o.at("autoMerge")) { "itself" } else { "you" }), &mut out); }
    if has(o.at("max")) { put("board.agents", json!(o.f("max") as i64), &mut out); }
    if has(o.at("machines")) {
        let names: Vec<String> = text(o.at("machines")).split(|c: char| c.is_whitespace() || c == ',').filter(|x| !x.is_empty()).map(String::from).collect();
        let v = if names.len() == 1 && off(&json!(names[0])) { "none".to_string() } else {
            names.iter().map(|n| if has(o.at("remoteUser")) && !n.contains('@') && n != "auto" { format!("{}@{n}", o.t("remoteUser")) } else { n.clone() }).collect::<Vec<_>>().join(", ") };
        put("board.machines", json!(v), &mut out);
    }
    if has(o.at("theme")) { put("board.theme", o.at("theme").clone(), &mut out); }
    if has(o.at("webUi")) { put("board.webUi", json!(if off(o.at("webUi")) { "none" } else { "auto" }), &mut out); }
    out
}
fn nested(flat: &Obj) -> Obj {
    let mut out = Obj::new();
    for (name, v) in flat { let (a, b) = name.split_once('.').unwrap_or((name, "")); out.obj_mut(a).put(b, v.clone()); }
    out
}
/// The repo's old file, as a task took it: setup, check, teardown, include.
fn moved_facts(cfg: &Value) -> Obj {
    let mut f = Obj::new();
    if cfg.b("setup") { f.put("workspace.setup", cfg.t("setup")); }
    if cfg.b("check") { f.put("check.command", cfg.t("check")); }
    if cfg.b("teardown") { f.put("workspace.teardown", cfg.t("teardown")); }
    if !cfg.arr("include").is_empty() { f.put("workspace.include", cfg.arr("include").iter().map(text).collect::<Vec<_>>()); }
    f
}
const OLD_KEYS: [&str; 35] = ["agent", "model", "effort", "fast", "reviewerModel", "reviewerEffort", "reviewerFast", "coordinatorModel", "coordinatorEffort", "watcherAgent", "watcherModel", "watcherEvery", "helper", "helperModel", "gate", "reviewAuto", "rounds", "artifacts", "serve", "remerge", "worktree", "base", "pr", "autoMerge", "cleanup", "machines", "remoteUser", "max", "skill", "trust", "agentArgs", "theme", "ownTerminal", "webUi", "bell"];
fn is_old(file: &Value) -> bool {
    let Some(o) = file.as_object() else { return false };
    OLD_KEYS.iter().any(|k| o.contains_key(*k)) || ["reviewer", "coordinator", "watcher", "review", "merge"].iter().any(|k| o.get(*k).map(|v| !v.is_object()).unwrap_or(false))
}
/// Your old config.json as the new one, or None when it is not in the old shape.
pub fn moved_config(file: &Value) -> Option<Obj> {
    if !is_old(file) { return None; }
    Some(nested(&moved_values(file.as_object()?, &[("agent", "claude".into()), ("reviewer", "codex".into()), ("coordinator", "claude".into())])))
}
/// One task in the old shape (`own`, `repoCfg`) as the new one, or None when there is nothing to move. `old`: your old
/// config, which a task's merging by itself depended on.
pub fn moved_task(t: &Obj, old: &Obj) -> Option<Obj> {
    if !["own", "repoCfg", "worktreeMode", "base"].iter().any(|k| t.contains_key(*k)) { return None; }
    let own = t.o("own").cloned().unwrap_or_default();
    let agent = if t.b("agent") { t.t("agent") } else if old.b("agent") { old.t("agent") } else { "claude".into() };
    let reviewer = if old.b("reviewer") && !off(old.at("reviewer")) { old.t("reviewer") } else { "codex".into() };
    let said = moved_values(&own, &[("agent", agent), ("reviewer", reviewer)]);
    let mut settings: Obj = said.into_iter().chain(t.o("settings").cloned().unwrap_or_default()).filter(|(n, _)| !(n.starts_with("board.") || n.starts_with("coordinator.") || n.starts_with("watcher."))).collect();
    if (t.s("base") == "head" || t.s("base") == "fresh") && !settings.contains_key("workspace.from") { settings.put("workspace.from", if t.s("base") == "head" { "here" } else { "main" }); }
    // The repo's own file could switch merging on by itself where you had chosen nothing. It no longer can; a task
    // that ran on that keeps it, as its own choice.
    if t.at("repoCfg").s("land") == "auto" && own.at("autoMerge").is_null() && old.at("autoMerge").is_null() && t.b("startedOnce") && !settings.contains_key("merge.by") { settings.put("merge.by", "itself"); }
    let mut next = t.clone();
    next.put("settings", settings);
    if own.at("research") == &json!(true) { next.put("research", true); }
    if t.contains_key("repoCfg") { next.put("repoFile", moved_facts(t.at("repoCfg"))); }
    for k in ["own", "repoCfg", "worktreeMode", "base"] { next.del(k); }
    Some(next)
}
fn kept_file() -> PathBuf { let f = config_file(); f.with_file_name("config.old.json") }
/// A task an older version wrote is converted when it is read.
pub fn moved_on_read(t: Obj) -> Obj {
    if !["own", "repoCfg", "worktreeMode", "base"].iter().any(|k| t.contains_key(*k)) { return t; }
    let old = if t.at("repoCfg").s("land") == "auto" { read_obj(&kept_file()).unwrap_or_default() } else { Obj::new() };
    moved_task(&t, &old).unwrap_or(t)
}
/// The move itself. Only a config.json still in the old shape is converted, and with it every task.
pub fn move_settings() -> bool {
    let Some(file) = read_json(&config_file()) else { return false };
    if !is_old(&file) { return false; }
    let mut moved = false;
    let _ = with_lock_wait("settings-move", 10000, || {
        let Some(old) = read_json(&config_file()) else { return Ok(()) };
        if !is_old(&old) { return Ok(()); }
        let old_obj = old.as_object().cloned().unwrap_or_default();
        for repo in read_dir_names(&tasks_dir()) {
            for n in read_dir_names(&tasks_dir().join(&repo)).into_iter().filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())) {
                let f = tasks_dir().join(&repo).join(&n).join("task.json");
                with_task_lock(&format!("{repo}/{n}"), 10000, || { if let Some(t) = read_obj(&f) { if let Some(next) = moved_task(&t, &old_obj) { write_json(&f, &Value::Object(next))?; } } Ok(()) })?;
            }
        }
        // Your file last: until it is written the move is not done, and is done again from the start.
        std::fs::copy(config_file(), kept_file())?;
        write_json(&config_file(), &Value::Object(moved_config(&old).unwrap_or_default()))?;
        moved = true;
        Ok(())
    });
    moved
}
