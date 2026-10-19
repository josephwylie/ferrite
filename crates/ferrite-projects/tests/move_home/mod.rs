//! What the tests of the one-time move share (home_folder.rs, home_folder_more.rs): a home folder as an older version
//! left it (data, settings and a report in three places), the move run on it, and every file under a folder read.
#![allow(dead_code)]
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use sw::store::move_home::{move_home, swap_now, Env, Moved};
pub use sw::store::move_home::OLD_NAME;

pub fn put(file: &Path, text: &str, at: Option<i64>) {
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
    if let Some(at) = at { std::fs::File::options().write(true).open(file).unwrap().set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_millis(at as u64)).unwrap(); }
}
pub fn read(f: &Path) -> String { std::fs::read_to_string(f).unwrap_or_default() }
pub fn is_link(p: &Path) -> bool { std::fs::symlink_metadata(p).map(|m| m.file_type().is_symlink()).unwrap_or(false) }
pub fn is_dir(p: &Path) -> bool { std::fs::symlink_metadata(p).map(|m| m.is_dir()).unwrap_or(false) }
pub fn ino(p: &Path) -> u64 { use std::os::unix::fs::MetadataExt; std::fs::metadata(p).unwrap().ino() }
pub fn names(d: &Path) -> Vec<String> { let mut v: Vec<String> = std::fs::read_dir(d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect(); v.sort(); v }
/// Every file under a folder with its text, links followed: what "nothing is lost" is checked against.
pub fn all(dir: &Path) -> BTreeMap<String, String> {
    fn walk(d: &Path, base: &Path, out: &mut BTreeMap<String, String>) {
        for e in std::fs::read_dir(d).unwrap() { let f = e.unwrap().path(); if std::fs::metadata(&f).unwrap().is_dir() { walk(&f, base, out) } else { out.insert(f.strip_prefix(base).unwrap().display().to_string(), read(&f)); } }
    }
    let mut out = BTreeMap::new(); walk(dir, dir, &mut out); out
}
pub fn env(pairs: &[(&str, &str)]) -> Env { pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }
/// The copy of the program that runs the move (the old launcher names it, so it is this board's own).
pub fn bin_of(home: &Path) -> PathBuf { home.join("copy").join("herdr-projects") }
pub fn run_with(home: &Path, env: &Env, swap: &dyn Fn(&Path, &Path) -> bool) -> Moved { move_home(home, env, &bin_of(home), Some(false), 3000, swap) }
pub fn hows(r: &Moved) -> Vec<(&'static str, &str)> { r.moved.iter().map(|(p, how)| (p.what, how.as_str())).collect() }
pub fn nothing(r: &Moved) -> bool { r.moved.is_empty() && r.line.is_empty() && r.stay.is_empty() && !r.busy && !r.other }

pub struct Old { pub home: PathBuf, pub data: PathBuf, pub settings: PathBuf, pub reports: PathBuf, pub to: PathBuf }
static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// A home folder as an older version left it, with the stray file a task in the home folder itself wrote into
/// ~/.herdr-projects (that name is every repo's rule folder). `named`: the program the old launcher names (this one's).
pub fn old_home(base: &Path, named: Option<&str>) -> Old {
    let home = base.join(format!("old-home-{}", N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
    std::fs::create_dir_all(&home).unwrap();
    let home = std::fs::canonicalize(&home).unwrap();
    let (data, settings, reports) = (home.join(".local/state/herdr-superwork"), home.join(".config/herdr-superwork"), home.join(".local/share/herdr-superwork-reports"));
    put(&data.join("tasks/proj/1/task.json"), r#"{"n":1}"#, None);
    put(&data.join("coordinator/default/state.json"), r#"{"session":"abc"}"#, None);
    put(&data.join("coordinator/default/chat/note.txt"), "its folder", None);
    put(&data.join("locks/dGFzaw.lock"), "", None);
    let named = named.map(String::from).unwrap_or_else(|| bin_of(&home).display().to_string());
    put(&data.join("bin").join(OLD_NAME), &format!("#!/bin/sh\nexport HERDR_SUPERWORK_DATA_DIR='{}'\nexec '{named}' \"$@\"\n", data.display()), None);
    put(&settings.join("config.json"), r#"{"max":7}"#, None);
    put(&reports.join("109/index.html"), "<p>report</p>", None);
    put(&home.join(".herdr-projects/closing-1.md"), "a home task's closing note", None);
    let to = home.join(".herdr-projects");
    Old { home, data, settings, reports, to }
}
impl Old {
    pub fn run(&self) -> Moved { run_with(&self.home, &env(&[]), &swap_now) }
    pub fn run_swap(&self, swap: &dyn Fn(&Path, &Path) -> bool) -> Moved { run_with(&self.home, &env(&[]), swap) }
    pub fn run_wait(&self, wait_ms: i64) -> Moved { move_home(&self.home, &env(&[]), &bin_of(&self.home), Some(false), wait_ms, &swap_now) }
}
