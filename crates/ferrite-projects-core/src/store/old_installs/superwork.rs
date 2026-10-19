//! herdr-superwork's home folder, taken over: ~/.herdr-superwork (its data, settings, reports, the Coordinator's chat,
//! the copies an older version kept) becomes ~/.herdr-projects in the three whole steps of the one-time move
//! (move_home.rs: no moment without either path, a cut-off run finished by the next), and ~/.herdr-superwork is left a
//! link to it, so an agent or a process that still holds an old path carries on. Then the paths written inside the
//! board's own files are made the new ones, and so is the command agents were handed.
use super::legacy::old_entries;
use super::Places;
use crate::json::*;
use crate::store::files::{read_obj, write_json};
use crate::store::move_home::{lstat, move_one, real, Part};
use std::fs;
use std::path::{Path, PathBuf};

/// The moved folder's record of its moves (moved.json): written in the folder itself, which is the same file under
/// either name at every step.
pub fn record_file(p: &Places) -> PathBuf { p.to.join("moved.json") }
/// herdr-superwork's folder is there to take over: a real folder, not the link this move leaves.
pub fn due(p: &Places) -> bool { lstat(&p.superwork).map(|m| m.is_dir()).unwrap_or(false) && real(&p.superwork) != real(&p.to) }

#[derive(Default, Debug)]
pub struct Took { pub how: Option<String>, pub busy: bool }
/// The move (old_installs.rs: `take_files` holds the lock). `busy`: not done now (the old projects are still in the
/// way, or a step could not be made): this run works on the folder where it is.
pub fn take_home(p: &Places, swap: &dyn Fn(&Path, &Path) -> bool) -> Took {
    if !due(p) { return Took::default(); }
    if !old_entries(&p.to).is_empty() { return Took { busy: true, ..Default::default() }; } // never mixed with them
    let part = Part { what: "home", from: p.superwork.clone(), to: p.to.clone(), key: "HERDR_PROJECTS_HOME" };
    let file = p.superwork.join("moved.json");
    let mut rec = read_obj(&file).unwrap_or_default();
    let how = move_one(&part, &mut rec, &file, &p.superwork.join("set-aside"), swap).ok().flatten();
    if how.is_none() && due(p) { return Took { busy: true, ..Default::default() }; }
    if how.is_some() {
        let file = record_file(p);
        let mut rec = read_obj(&file).unwrap_or_default();
        let mut t = rec.o("takeOver").cloned().unwrap_or_default();
        t.put("superwork", "due");
        rec.put("takeOver", Value::Object(t));
        let _ = write_json(&file, &Value::Object(rec));
    }
    Took { how, ..Default::default() }
}

/// `text` with every path into the old folder made the same path into the new one; the launcher agents were handed
/// becomes the new one. Only whole names: `~/.herdr-superwork-x` is another folder and stays.
pub fn fix_text(text: &str, p: &Places) -> String {
    let (from, to) = (p.superwork.display().to_string(), p.to.display().to_string());
    let tilde = |x: &Path| x.strip_prefix(&p.home).ok().map(|r| format!("~/{}", r.display()));
    let mut pairs = vec![(format!("{from}/data/bin/herdr-superwork"), format!("{to}/data/bin/herdr-projects")), (from.clone(), to.clone())];
    if let (Some(f), Some(t)) = (tilde(&p.superwork), tilde(&p.to)) { pairs.push((format!("{f}/data/bin/herdr-superwork"), format!("{t}/data/bin/herdr-projects"))); pairs.push((f, t)); }
    let mut out = text.to_string();
    for (a, b) in pairs { out = replace_whole(&out, &a, &b); }
    out
}
fn replace_whole(text: &str, a: &str, b: &str) -> String {
    if !text.contains(a) { return text.to_string(); }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(a) {
        let next = rest[i + a.len()..].chars().next();
        let whole = !next.map(|c| c.is_ascii_alphanumeric() || "._-".contains(c)).unwrap_or(false);
        out.push_str(&rest[..i]);
        out.push_str(if whole { b } else { a });
        rest = &rest[i + a.len()..];
    }
    out.push_str(rest);
    out
}
/// The board's own files in the moved folder, fixed: what it reads back (json, jsonl, a task's rules and notes).
/// Not what is kept as it was: a task's artifacts, the Coordinator's chat folder, reports, the older copies. A
/// changed file keeps its time, which some of the board's looks go by. Returns how many files changed.
pub fn fix_paths(p: &Places) -> usize {
    fn walk(d: &Path, p: &Places, n: &mut usize) {
        for e in fs::read_dir(d).into_iter().flatten().flatten() {
            let f = e.path();
            let Ok(m) = fs::symlink_metadata(&f) else { continue };
            let name = e.file_name().to_string_lossy().into_owned();
            if m.is_dir() { if !["artifacts", "chat", "locks"].contains(&name.as_str()) { walk(&f, p, n); } continue; }
            if !m.is_file() || !(name.ends_with(".json") || name.ends_with(".jsonl") || name.ends_with(".md")) { continue; }
            let Ok(text) = fs::read_to_string(&f) else { continue };
            let next = fix_text(&text, p);
            if next == text { continue; }
            let tmp = PathBuf::from(format!("{}.{}.fix", f.display(), std::process::id()));
            if fs::write(&tmp, &next).is_err() { let _ = fs::remove_file(&tmp); continue; }
            let _ = fs::set_permissions(&tmp, m.permissions());
            if let (Ok(t), Ok(file)) = (m.modified(), fs::File::options().write(true).open(&tmp)) { let _ = file.set_modified(t); }
            if fs::rename(&tmp, &f).is_ok() { *n += 1; } else { let _ = fs::remove_file(&tmp); }
        }
    }
    let mut n = 0;
    for d in ["data", "settings"] { walk(&p.to.join(d), p, &mut n); } // not moved.json: its old moves stay as they were
    n
}
/// herdr's config and state folders for herdr-superwork, taken over into herdr-projects': each thing in them that the
/// new one does not have goes there. An emptied old folder goes. Returns what moved.
pub fn take_plugin_dirs(p: &Places) -> Vec<String> {
    let mut moved = vec![];
    for base in [&p.plugin_config, &p.plugin_state] {
        let (from, to) = (base.join(super::OLD_ID), base.join(super::ID));
        if !lstat(&from).map(|m| m.is_dir()).unwrap_or(false) { continue; }
        for e in fs::read_dir(&from).into_iter().flatten().flatten() {
            let dest = to.join(e.file_name());
            if lstat(&dest).is_some() { continue; }
            if fs::create_dir_all(&to).is_ok() && fs::rename(e.path(), &dest).is_ok() { moved.push(dest.display().to_string()); }
        }
        let _ = fs::remove_dir(&from); // only when empty
    }
    moved
}
