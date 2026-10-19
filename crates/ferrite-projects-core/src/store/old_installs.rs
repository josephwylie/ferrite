//! Older installs found on this machine, their files taken over by the first run (flow/take_over.rs does the rest):
//! the old herdr-projects 0.2 kept its projects in ~/.herdr-projects, which is this version's home folder now, so they
//! are set aside whole (old_installs/legacy.rs); and herdr-superwork, this version's working title, kept everything in
//! ~/.herdr-superwork, which becomes ~/.herdr-projects (old_installs/superwork.rs). docs/old-installs.md has the story.
pub mod legacy;
pub mod superwork;

use super::files::{marker, marker_alive, read_json};
use super::move_home::{move_plan, Env};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The plugin's id in herdr, and the one herdr-superwork had.
pub const ID: &str = "herdr-projects";
pub const OLD_ID: &str = "herdr-superwork";

/// Where everything is, for one home folder and one environment (the tests give their own).
#[derive(Clone, Debug)]
pub struct Places {
    /// The user's home folder.
    pub home: PathBuf,
    /// This version's home folder: ~/.herdr-projects.
    pub to: PathBuf,
    /// herdr-superwork's: ~/.herdr-superwork.
    pub superwork: PathBuf,
    /// The old herdr-projects 0.2's own settings: ~/.config/herdr-projects (it never followed XDG_CONFIG_HOME).
    pub old_config: PathBuf,
    /// herdr's config.toml, and the folders herdr keeps each plugin's config and state in.
    pub herdr_config: PathBuf,
    pub plugin_config: PathBuf,
    pub plugin_state: PathBuf,
}
pub fn places(home: &Path, env: &Env) -> Places {
    let get = |k: &str| env.get(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let herdr_config = get("HERDR_CONFIG_PATH").unwrap_or_else(|| get("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config")).join("herdr").join("config.toml"));
    let herdr_dir = herdr_config.parent().map(Path::to_path_buf).unwrap_or_default();
    let state = get("XDG_STATE_HOME").unwrap_or_else(|| home.join(".local").join("state"));
    Places {
        home: home.to_path_buf(),
        to: move_plan(home, env).home,
        superwork: get("HERDR_SUPERWORK_HOME").unwrap_or_else(|| home.join(".herdr-superwork")),
        old_config: home.join(".config").join(ID),
        plugin_config: herdr_dir.join("plugins").join("config"),
        plugin_state: state.join("herdr").join("plugins"),
        herdr_config,
    }
}
/// `<path>-legacy`, or `-legacy-2`… when that is taken: where something of the old version is set aside.
pub fn legacy_name(p: &Path) -> PathBuf {
    let at = |n: usize| PathBuf::from(format!("{}-legacy{}", p.display(), if n < 2 { String::new() } else { format!("-{n}") }));
    (1..).map(at).find(|f| std::fs::symlink_metadata(f).is_err()).expect("a free name")
}

/// A lock file beside the home folder (inside it, it would move with it), held while its value lives. A dead holder's
/// is taken over; a live one's is waited for `wait_ms`, then None.
pub struct Lock(PathBuf);
impl Lock {
    pub fn take(file: PathBuf, wait_ms: i64) -> Option<Lock> {
        let until = crate::clock::now() + wait_ms;
        loop {
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&file) {
                Ok(mut f) => { let _ = std::io::Write::write_all(&mut f, serde_json::to_string(&marker()).unwrap_or_default().as_bytes()); return Some(Lock(file)); }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if !marker_alive(&read_json(&file).unwrap_or(Value::Null)) { let _ = std::fs::remove_file(&file); continue; }
                    if crate::clock::now() >= until { return None; }
                    crate::clock::sleep_ms(20);
                }
                Err(_) => return None,
            }
        }
    }
    pub fn beside(p: &Places, what: &str, wait_ms: i64) -> Option<Lock> { Lock::take(PathBuf::from(format!("{}.{what}.lock", p.to.display())), wait_ms) }
}
impl Drop for Lock { fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); } }
/// The files of both older installs, by one mover at a time: the old projects set aside first, then herdr-superwork's
/// folder moved into the name they leave. Another mover still at it after `wait_ms`: herdr-superwork's folder is used
/// where it is, this time (`busy`).
pub fn take_files(p: &Places, wait_ms: i64, swap: &dyn Fn(&Path, &Path) -> bool) -> (legacy::Aside, superwork::Took) {
    if !legacy::due(p) && !superwork::due(p) { return Default::default(); }
    let Some(_held) = Lock::beside(p, "moving", wait_ms) else { return (Default::default(), superwork::Took { busy: superwork::due(p), ..Default::default() }) };
    let aside = legacy::set_aside(p, wait_ms);
    let took = if aside.busy { superwork::Took { busy: superwork::due(p), ..Default::default() } } else { superwork::take_home(p, swap) };
    (aside, took)
}
