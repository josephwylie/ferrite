//! Older installs on this machine, taken over by the first run of this version (any run: the board, a command, a hook)
//! and looked at again by `herdr-projects doctor` and `install`: the old herdr-projects 0.2's projects set aside and
//! its entries in herdr's and the agent programs' settings taken out; herdr-superwork's home folder moved into this
//! one, its paths fixed, its processes and its plugin replaced by this version's, its extras made this version's.
//! The files are store/old_installs/; what else changes is here. docs/old-installs.md has the whole story.
pub mod herdr_config;
pub mod look;
pub mod live;
pub mod outside;
pub mod toml_text;

use crate::json::*;
use crate::store::files::{bin, read_obj, user_home, write_json};
use crate::store::move_home::{add_line, env_now, linked_worktree, short, swap_now, Env};
use crate::store::old_installs::{legacy, places, superwork, take_files, Lock, Places};
use std::path::{Path, PathBuf};

/// What taking over does beyond files: live.rs for real, a stand-in in the tests.
pub trait Sys {
    /// This version's launcher, as it writes it (flow/send.rs): what the one agents were handed by the old name runs.
    fn launcher(&self) -> Option<String>;
    /// herdr-superwork's keeper and page server, stopped. Returns what was.
    fn stop_old(&self, data: &Path) -> Vec<String>;
    fn start_new(&self, stopped: &[String]);
    fn write_config(&self, file: &Path, next: &str) -> Result<(), String>;
    fn old_plugin_off(&self) -> bool;
}
#[derive(Default, Debug)]
pub struct Report { pub said: Vec<String>, pub left: Vec<String>, pub stay: Option<PathBuf> }

fn legacy_line(moved: &[(String, PathBuf, PathBuf)], home: &Path) -> String {
    let of = |w: &str| moved.iter().find(|m| m.0 == w).map(|m| short(&m.2, home));
    let mut s = match of("projects") { Some(to) => format!("The old herdr-projects (0.2) kept its projects in {}: they are in {to} now, as they were", short(&moved[0].1, home)), None => "The old herdr-projects (0.2) was set aside".into() };
    if let Some(c) = of("settings") { s.push_str(&format!(", its settings in {c}")); }
    s.push_str(". Nothing in them was changed or deleted.");
    s
}
fn config_said(what: &[&str]) -> String { format!("the old herdr-projects' {}", what.join(", ")) }

/// Everything, in order; each step is safe to run again. `force`: look at everything (doctor, install), not only
/// at what a move just made due.
pub fn take_over(p: &Places, env: &Env, sys: &dyn Sys, force: bool, wait_ms: i64, swap: &dyn Fn(&Path, &Path) -> bool) -> Report {
    let mut r = Report::default();
    let (aside, took) = take_files(p, wait_ms, swap);
    if aside.busy { r.left.push(format!("{} still holds the old herdr-projects' projects: its ticker did not stop. Run herdr-projects doctor again.", short(&p.to, &p.home))); }
    if took.busy { r.stay = Some(p.superwork.clone()); return r; } // this run works where the folder is
    // The rest by one run at a time: another one at it (a process this one started, say) is left to it.
    let Some(_held) = Lock::beside(p, "taking-over", 0) else { return r };
    let file = superwork::record_file(p);
    let mut rec = read_obj(&file).unwrap_or_default();
    let before = rec.clone();
    let mut t = rec.o("takeOver").cloned().unwrap_or_default();
    if !aside.moved.is_empty() {
        t.put("legacy", "due");
        let mut l = rec.o("legacy").cloned().unwrap_or_default();
        for (what, from, to) in &aside.moved { l.put(what, json!({ "from": from.display().to_string(), "to": to.display().to_string(), "at": crate::clock::now() })); }
        rec.put("legacy", Value::Object(l));
        r.said.push(legacy_line(&aside.moved, &p.home));
    }
    let home_moved = rec.at("home").s("state") == "done";
    if took.how.is_some() { r.said.push(format!("herdr-superwork is herdr-projects now: everything in {} (tasks, settings, reports, the Coordinator's chat) moved into {}, and {} is a link to it, so whatever still runs carries on.", short(&p.superwork, &p.home), short(&p.to, &p.home), short(&p.superwork, &p.home))); }
    // The old version's entries outside its folders.
    let mut text = std::fs::read_to_string(&p.herdr_config).unwrap_or_default();
    let mut config_what: Vec<&str> = vec![];
    if force || t.s("legacy") == "due" {
        let (next, what) = herdr_config::undo_legacy(&text);
        text = next; config_what.extend(what);
        let (hooks, unread) = outside::undo_hooks(p, env);
        let links = outside::undo_links(p, env, &bin());
        let mut gone: Vec<String> = vec![];
        if !hooks.is_empty() { gone.push(format!("its hooks in {}", hooks.iter().map(|f| short(Path::new(f), &p.home)).collect::<Vec<_>>().join(", "))); }
        if !links.is_empty() { gone.push(format!("its links {}", links.iter().map(|f| short(Path::new(f), &p.home)).collect::<Vec<_>>().join(", "))); }
        if !gone.is_empty() { r.said.push(format!("Taken out of the old herdr-projects: {}.", gone.join("; "))); }
        for f in unread { r.left.push(format!("{} may still hold the old herdr-projects' hooks (`… hook --agent …`): it is not plain JSON, so take them out by hand.", short(Path::new(&f), &p.home))); }
        if t.s("legacy") == "due" { t.put("legacy", "done"); }
    }
    let (mut superwork_text, mut restart) = (false, None);
    if (force && home_moved) || t.s("superwork") == "due" {
        let data = p.to.join("data");
        let old_launcher = data.join("bin").join(crate::store::move_home::OLD_NAME);
        if old_launcher.exists() { if let Some(l) = sys.launcher() { if std::fs::read_to_string(&old_launcher).ok().as_deref() != Some(l.as_str()) { let _ = crate::store::files::write_atomic(&old_launcher, l.as_bytes()); } } }
        let stopped = sys.stop_old(&data);
        let off = sys.old_plugin_off();
        let fixed = superwork::fix_paths(p);
        superwork::take_plugin_dirs(p);
        let skills = outside::undo_superwork_skill(p, &outside::skill_dirs(p, env));
        let (next, changed) = herdr_config::take_superwork(&text);
        text = next; superwork_text = changed;
        restart = Some(stopped.clone());
        let mut done = vec![];
        if !stopped.is_empty() { done.push(format!("its {} restarted on this version", stopped.join(" and "))); }
        if off { done.push("its plugin switched off in herdr".into()); }
        if !skills.is_empty() { done.push("its how-to replaced by this version's".into()); }
        if fixed > 0 && t.s("superwork") == "due" { done.push(format!("the paths in {fixed} of the board's files made the new ones")); }
        if !done.is_empty() { r.said.push(format!("herdr-superwork: {}.", done.join(", "))); }
        if t.s("superwork") == "due" { t.put("superwork", "done"); }
    }
    if !config_what.is_empty() || superwork_text {
        match sys.write_config(&p.herdr_config, &text) {
            Ok(()) => { if !config_what.is_empty() { r.said.push(format!("Taken out of herdr's config: the old herdr-projects' {} (a backup is beside it).", config_what.join(", "))); } if superwork_text { r.said.push("herdr-superwork's key and sidebar rows in herdr's config are herdr-projects' now.".into()); } }
            Err(why) => r.left.push(format!("{} still holds {}: {why}", short(&p.herdr_config, &p.home), if config_what.is_empty() { "herdr-superwork's key and sidebar rows".to_string() } else { config_said(&config_what) })),
        }
    }
    if !t.is_empty() { rec.put("takeOver", Value::Object(t)); }
    if !r.said.is_empty() && !force { add_line(&mut rec, &r.said.join(" ")); }
    if rec != before { let _ = write_json(&file, &Value::Object(rec)); }
    // Last, once the record says it is done: what starts now finds nothing left to take over.
    if let Some(stopped) = restart { sys.start_new(&stopped); }
    r
}
/// Anything to take over? A look made at every start, so it only looks at what is there.
pub fn due(p: &Places) -> bool {
    let rec = read_obj(&superwork::record_file(p)).unwrap_or_default();
    superwork::due(p) || legacy::due(p) || rec.at("takeOver").as_object().map(|t| t.values().any(|v| v == "due")).unwrap_or(false)
}
/// Commands that read and change nothing.
const LOOKS: [&str; 5] = ["help", "--skill", "skill", "version", "docs"];
/// At the start of every run, before the one-time move of older folders (store/move_home.rs), whose line says this
/// one's too. Only the copy the board runs from: never a git worktree an agent works in, never the tests.
pub fn at_start(cmd: &str) {
    if crate::runner::inline() || LOOKS.contains(&cmd) || linked_worktree(&bin()) { return; }
    let (home, env) = (user_home(), env_now());
    let p = places(&home, &env);
    if !due(&p) { return; }
    let r = take_over(&p, &env, &live::Live, false, 3000, &swap_now);
    if let Some(h) = r.stay { unsafe { std::env::set_var("HERDR_PROJECTS_HOME", h) }; }
}
