//! `herdr-projects doctor` (and the end of `install`): older installs looked at again, whatever is left of them taken
//! over now, and where things stand said in a few lines.
use super::{take_over, Sys};
use crate::json::*;
use crate::store::files::read_obj;
use crate::store::move_home::{short, swap_now, Env};
use crate::store::old_installs::{legacy, superwork, Places};
use std::path::Path;

fn day(ms: i64) -> String { crate::clock::iso(ms).get(..10).unwrap_or("").to_string() }
pub fn look(p: &Places, env: &Env, sys: &dyn Sys) -> Vec<String> {
    let r = take_over(p, env, sys, true, 3000, &swap_now);
    let rec = read_obj(&superwork::record_file(p)).unwrap_or_default();
    let s = |x: &str| short(Path::new(x), &p.home);
    let mut out = r.said.clone();
    for (what, v) in rec.o("legacy").cloned().unwrap_or_default() {
        out.push(format!("Old herdr-projects (0.2): its {what} {} in {}, set aside on {}.", if what == "projects" || what == "settings" { "are" } else { "is" }, s(v.s("to")), day(v.i("at"))));
    }
    if rec.o("legacy").map(|l| l.contains_key("projects")).unwrap_or(false) { out.push(format!("  The old version finds its projects there with HERDR_PROJECTS_ROOT={}.", s(rec.at("legacy").at("projects").s("to")))); }
    if env.get("HERDR_PROJECTS_ROOT").map(|v| !v.is_empty()).unwrap_or(false) { out.push("HERDR_PROJECTS_ROOT is set: only the old herdr-projects (0.2) read it. Take it out of your shell's start-up file.".into()); }
    let home = rec.at("home");
    if home.s("state") == "done" {
        out.push(format!("herdr-superwork: moved into {} on {}; {} is a link to it. Remove the link once nothing started before then still runs.", s(home.s("to")), day(home.i("at")), s(home.s("from"))));
    }
    out.extend(r.left.iter().cloned());
    if superwork::due(p) && r.stay.is_some() { out.push(format!("{} is still to be moved: another run is moving it, or the old projects are in the way. Run herdr-projects doctor again.", short(&p.superwork, &p.home))); }
    if legacy::due(p) && r.left.is_empty() { out.push(format!("{} still holds something of the old herdr-projects (0.2).", short(&p.to, &p.home))); }
    if out.is_empty() { out.push("No older install of herdr-projects or herdr-superwork on this machine.".into()); }
    out
}
/// For this machine, as the program runs on it.
pub fn look_here() -> Vec<String> {
    use crate::store::move_home::{env_now, move_note, move_told};
    let (home, env) = (crate::store::files::user_home(), env_now());
    // What this run's start did, when it was not said on the terminal already.
    let note = move_note(&home, &env);
    move_told(&home, &env);
    let mut out: Vec<String> = Some(note).filter(|n| !n.is_empty()).into_iter().collect();
    out.extend(look(&crate::store::old_installs::places(&home, &env), &env, &super::live::Live));
    out
}
