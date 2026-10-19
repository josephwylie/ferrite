//! The settings' commands: set, unset and show, for you, for your entry for a repo, or for one task.
use crate::err::Res;
use crate::json::*;
use crate::store::repos::known_repos;
use crate::store::settings::change::{set_value, unset_value};
use crate::store::settings::lookup::Scope;
use crate::store::settings::show::{problems, rows, show_text};
use crate::store::settings::values::say;
use crate::store::tasks::{find, get};

/// Which of you, a repo or a task a command means.
pub struct Where { pub task: Option<String>, pub repo: Option<String>, pub automation: Option<String> }
/// Returns what the command line prints: the value for --json, and the text.
pub fn settings_command(cmd: &str, pos: &[String], w: &Where, value: Option<Value>) -> Res<(Value, String)> {
    if [&w.task, &w.repo, &w.automation].iter().filter(|x| x.is_some()).count() > 1 { crate::fail!("{cmd} takes one of --task, --repo and --automation, not several."); }
    if let Some(a) = &w.automation { return crate::automations::command::settings_for(cmd, pos, a, value); }
    let id = if cmd == "show" { w.task.clone().or_else(|| pos.get(1).cloned()) } else { w.task.clone() };
    let task = match &id { Some(id) => Some(find(id)?), None => None };
    let scope = match (&task, &w.repo) { (Some(t), _) => Scope::task(t), (None, Some(r)) => Scope::repo(r), _ => Scope::you() };
    let place = match (&task, &w.repo) { (Some(t), _) => format!(" for {}", t.s("id")), (None, Some(r)) => format!(" for {r}"), _ => String::new() };
    if cmd == "show" {
        if !scope.repo.is_empty() && !known_repos().contains_key(&scope.repo) {
            let names: Vec<String> = known_repos().keys().cloned().collect();
            crate::fail!("No repo \"{}\" on the board. Known: {}.", scope.repo, if names.is_empty() { "none yet".into() } else { names.join(", ") });
        }
        let list: Vec<Value> = rows(&scope).iter().map(|r| r.json()).collect();
        return Ok((json!({ "settings": list, "problems": problems(task.as_ref()) }), show_text(&scope)));
    }
    let Some(name) = pos.get(1) else { crate::fail!("{cmd} needs a setting's name. `herdr-projects show` lists them all.") };
    let r = if cmd == "set" {
        let Some(v) = value else { crate::fail!("set needs a value: herdr-projects set {name} <value>. To take your answer away: herdr-projects unset {name}") };
        set_value(name, &v, &scope)?
    } else { unset_value(name, &scope)? };
    let fresh = task.as_ref().and_then(|t| get(t.s("id")));
    let scope2 = match &fresh { Some(t) => Scope::task(t), None => scope.clone() };
    let row = rows(&scope2).into_iter().find(|x| x.name == r.name);
    let mut note = row.map(|row| if !row.note.is_empty() { row.note } else if !row.later.is_empty() { format!("It {}.", row.later) } else { String::new() }).unwrap_or_default();
    // A repo is never left without a key: unset goes back to the repo's file's, or to one made from its name.
    if cmd == "unset" && r.name == "repo.key" && note.is_empty() {
        let r2 = w.repo.clone().unwrap_or_default();
        note = if r.place == "file" { format!("A repo always has a key: {r2} is back to the one its own file says.") }
            else { format!("A repo always has a key: {r2} is back to the one the board made from its name. Another: herdr-projects set repo.key <KEY> --repo {r2}") };
    }
    let text = format!("{}{place}: {}  ({}){}", r.name, say(&r.value), r.from, if note.is_empty() { String::new() } else { format!("\n{note}") });
    let mut j = obj(json!({ "name": r.name, "value": r.value, "from": r.from }));
    if !note.is_empty() { j.put("note", note); }
    Ok((Value::Object(j), text))
}
