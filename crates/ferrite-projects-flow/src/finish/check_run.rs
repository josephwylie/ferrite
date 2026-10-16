//! Running the repo's check with its progress shown: the file the command reports to, and what is kept on the task while
//! it runs (the row shows that, and herdr's sidebar the row).
use crate::check_progress::{ran_for, read_report, start_check, step_check, KNOWN_LINE};
use crate::clock::now;
use crate::err::Res;
use crate::json::*;
use crate::runner::{sh, Guard, ShOut};
use crate::store::check_runs::{note_check_run, usual_check};
use crate::store::files::dir_of;
use std::path::{Path, PathBuf};

/// The name of the file a check writes its progress to, given to the command in this variable (docs/check-progress.md).
pub const PROGRESS_ENV: &str = "HERDR_PROJECTS_PROGRESS";
pub fn progress_file(id: &str) -> PathBuf { dir_of(id).join("check-progress") }
/// The check itself, followed: what `sh` gave, plus `known` when the command answered at once for code that passed before.
/// `short`: the shorter check of a later round (check_again.rs), said so on the task.
pub fn run_check(g: &mut Guard, id: &str, cmd: &str, short: bool, folder: &Path, env: &[(String, String)], root: &Path, timeout_min: f64) -> Res<(ShOut, bool)> {
    let file = progress_file(id);
    let _ = std::fs::remove_file(&file);
    let check = std::cell::RefCell::new(start_check(now(), usual_check(root, cmd)));
    let seen = std::cell::RefCell::new(String::new());
    g.set(|t| { t.put("msg", if short { "Running the shorter check of a fix round" } else { "Running the check" }); t.put("check", check.borrow().clone()); })?;
    let read = || -> bool {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        if *seen.borrow() == text { return false; }
        *seen.borrow_mut() = text.clone();
        let next = step_check(&check.borrow(), read_report(&text).as_ref(), now());
        if next == *check.borrow() { return false; }
        *check.borrow_mut() = next;
        true
    };
    let mut env = env.to_vec();
    env.push((PROGRESS_ENV.into(), file.display().to_string()));
    let gcell = std::cell::RefCell::new(g);
    let alive = || gcell.borrow().check().is_ok();
    let mut tick = || -> Res<()> { if read() { let c = check.borrow().clone(); gcell.borrow_mut().set(|t| t.put("check", c))?; } Ok(()) };
    let r = sh(cmd, folder, &env, timeout_min, Some(&alive), Some(&mut tick));
    let g = gcell.into_inner();
    read();
    let known = check.borrow().s("state") == "known";
    if r.ok && !r.aborted { if let Some(ran) = ran_for(&check.borrow(), now()) { note_check_run(root, cmd, ran); } }
    let _ = std::fs::remove_file(&file);
    if r.aborted { return Ok((r, false)); }
    // Off the task again; the row says an answer that took no run, and then moves on.
    let ok = r.ok;
    g.set(|t| { t.put("check", Value::Null); if known && ok { t.put("msg", KNOWN_LINE); } })?;
    Ok((r, known))
}
