//! What older installs put in other programs' folders, taken out: the old herdr-projects 0.2's hooks in each agent
//! program's settings (a command `… herdr-projects --root … hook --agent …`), its skill (a link named `autoproject`
//! to its plugin folder) and the link it put on the PATH; and herdr-superwork's how-to (a `herdr-superwork` folder in
//! each agent program's skills folder). Only what those versions wrote: a file is changed only where it holds their
//! entries, and is backed up first.
use crate::json::*;
use crate::store::files::read_obj;
use crate::store::move_home::{lstat, Env};
use crate::store::old_installs::Places;
use serde_json::Map;
use std::fs;
use std::path::{Path, PathBuf};

fn dir_of(p: &Places, env: &Env, var: &str, rest: &str) -> PathBuf { env.get(var).filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| p.home.join(rest)) }
/// The settings files the old version put its hooks in, and its own file for Copilot.
pub fn hook_files(p: &Places, env: &Env) -> (Vec<PathBuf>, PathBuf) {
    (vec![dir_of(p, env, "CLAUDE_CONFIG_DIR", ".claude").join("settings.json"), dir_of(p, env, "CODEX_HOME", ".codex").join("hooks.json"), p.home.join(".factory/settings.json"), p.home.join(".gemini/settings.json")],
     dir_of(p, env, "COPILOT_HOME", ".copilot").join("hooks").join("herdr-projects.json"))
}
fn old_hook(v: &Value) -> bool { v.get("command").and_then(|c| c.as_str()).map(|c| c.contains("herdr-projects") && c.contains(" hook --agent ")).unwrap_or(false) }
/// A settings value without the old hooks: an entry that held only those goes, and so does a list or a table they
/// leave empty. (value, changed); None: it goes.
fn strip(v: &Value) -> (Option<Value>, bool) {
    if old_hook(v) { return (None, true); }
    match v {
        Value::Array(a) => {
            let mut ch = false;
            let out: Vec<Value> = a.iter().filter_map(|x| { let (y, c) = strip(x); ch |= c; y }).collect();
            if ch && out.is_empty() { (None, true) } else { (Some(Value::Array(out)), ch) }
        }
        Value::Object(o) => {
            let (out, ch) = strip_map(o);
            if ch && (out.is_empty() || (o.contains_key("hooks") && !out.contains_key("hooks"))) { (None, true) } else { (Some(Value::Object(out)), ch) }
        }
        _ => (Some(v.clone()), false),
    }
}
fn strip_map(o: &Map<String, Value>) -> (Map<String, Value>, bool) {
    let mut ch = false;
    let mut out = Map::new();
    for (k, x) in o { let (y, c) = strip(x); ch |= c; if let Some(y) = y { out.insert(k.clone(), y); } }
    (out, ch)
}
fn backup(file: &Path) { let _ = fs::copy(file, format!("{}.bak-herdr-projects", file.display())); }
/// The old hooks taken out of every file. Returns the files changed, and those that could not be read (not JSON).
pub fn undo_hooks(p: &Places, env: &Env) -> (Vec<String>, Vec<String>) {
    let (files, copilot) = hook_files(p, env);
    let (mut done, mut unread) = (vec![], vec![]);
    for f in files.iter().chain(std::iter::once(&copilot)) {
        let Ok(text) = fs::read_to_string(f) else { continue };
        if !text.contains(" hook --agent ") { continue; }
        let Ok(Value::Object(o)) = serde_json::from_str::<Value>(&text) else { unread.push(f.display().to_string()); continue };
        let (out, ch) = strip_map(&o);
        if !ch { continue; }
        backup(f);
        let ok = if f == &copilot && !out.contains_key("hooks") { fs::remove_file(f).is_ok() } else { fs::write(f, format!("{}\n", serde_json::to_string_pretty(&Value::Object(out)).unwrap_or_default())).is_ok() };
        if ok { done.push(f.display().to_string()); }
    }
    (done, unread)
}
/// The old version's skill: a link named `autoproject` to its plugin's `skill/autoproject`. And the link it put on
/// the PATH, when it points into herdr's plugins and not at this program. Returns what was removed.
pub fn undo_links(p: &Places, env: &Env, this: &Path) -> Vec<String> {
    let mut gone = vec![];
    for l in [dir_of(p, env, "CLAUDE_CONFIG_DIR", ".claude").join("skills/autoproject"), p.home.join(".agents/skills/autoproject")] {
        if fs::read_link(&l).map(|t| t.ends_with("skill/autoproject")).unwrap_or(false) && fs::remove_file(&l).is_ok() { gone.push(l.display().to_string()); }
    }
    let on_path = dir_of(p, env, "XDG_BIN_HOME", ".local/bin").join("herdr-projects");
    if let Ok(t) = fs::read_link(&on_path) {
        let ours = fs::canonicalize(&on_path).ok().zip(fs::canonicalize(this).ok()).map(|(a, b)| a == b).unwrap_or(false);
        if t.to_string_lossy().contains("/plugins/") && !ours && fs::remove_file(&on_path).is_ok() { gone.push(on_path.display().to_string()); }
    }
    gone
}
/// Each agent program's own skills folder (agents/homes.rs, for this environment).
pub fn skill_dirs(p: &Places, env: &Env) -> Vec<PathBuf> {
    [("CLAUDE_CONFIG_DIR", ".claude"), ("CODEX_HOME", ".codex"), ("PI_CODING_AGENT_DIR", ".pi/agent")].iter().map(|(v, d)| dir_of(p, env, v, d).join("skills")).collect()
}
/// herdr-superwork's how-to folders: the ones it wrote down, and one in each agent program's skills folder that holds
/// only its SKILL.md. Taken out of the moved extras.json too. Returns what was removed.
pub fn undo_superwork_skill(p: &Places, skill_dirs: &[PathBuf]) -> Vec<String> {
    let file = p.to.join("data").join("extras.json");
    let mut rec = read_obj(&file).unwrap_or_default();
    let listed = strings(rec.at("skillPaths"));
    // Only inside this home folder: a path written down on another machine (a copied board) is not this one's.
    let mut dirs: Vec<PathBuf> = listed.iter().filter(|d| d.ends_with("/herdr-superwork")).map(PathBuf::from).filter(|d| d.starts_with(&p.home)).collect();
    dirs.extend(skill_dirs.iter().map(|d| d.join("herdr-superwork")));
    let mut gone = vec![];
    for d in dirs {
        if gone.contains(&d.display().to_string()) || !lstat(&d).map(|m| m.is_dir()).unwrap_or(false) { continue; }
        let only = fs::read_dir(&d).map(|r| r.flatten().all(|e| e.file_name() == "SKILL.md")).unwrap_or(false);
        let named = fs::read_to_string(d.join("SKILL.md")).map(|t| t.contains("name: herdr-superwork")).unwrap_or(false);
        if only && named && fs::remove_dir_all(&d).is_ok() { gone.push(d.display().to_string()); }
    }
    let keep: Vec<String> = listed.into_iter().filter(|d| !d.ends_with("/herdr-superwork")).collect();
    if rec.b("skillPaths") && keep.len() != strings(rec.at("skillPaths")).len() { rec.put("skillPaths", json!(keep)); let _ = crate::store::files::write_json(&file, &Value::Object(rec)); }
    gone
}
