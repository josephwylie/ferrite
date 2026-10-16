//! The first run of this version on data an older (Node) version wrote (crates/core/src/store/old_copy.rs): one copy of
//! the data folder as it was, beside it, made once, and its one line for the human. In a scratch HERDR_PROJECTS_HOME.
use std::path::{Path, PathBuf};
use std::sync::MutexGuard;
use sw::store::move_home::OLD_NAME;
use sw::store::old_copy::*;

static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
fn set(k: &str, v: &str) { unsafe { std::env::set_var(k, v) } }
/// A scratch home and board folder; returns the data folder.
fn scratch() -> (PathBuf, PathBuf, MutexGuard<'static, ()>) {
    let one = crate::common::one_at_a_time(); // the environment is the whole test program's
    let base = std::env::temp_dir().join(format!("sw-first-run-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("home")).unwrap();
    let base = std::fs::canonicalize(&base).unwrap();
    set("HOME", &base.join("home").display().to_string());
    set("HERDR_PROJECTS_HOME", &base.join("home/.herdr-projects").display().to_string());
    for (k, v) in [("XDG_STATE_HOME", "home/.state"), ("XDG_CONFIG_HOME", "home/.config"), ("XDG_DATA_HOME", "home/.share")] { set(k, &base.join(v).display().to_string()); }
    for k in ["HERDR_PROJECTS_DATA_DIR", "HERDR_PROJECTS_CONFIG_DIR", "HERDR_PROJECTS_REPORTS_DIR", "HERDR_PROJECTS_INLINE"] { unsafe { std::env::remove_var(k) } }
    (base.clone(), base.join("home/.herdr-projects/data"), one)
}
fn write(p: &Path, text: &str) { std::fs::create_dir_all(p.parent().unwrap()).unwrap(); std::fs::write(p, text).unwrap(); }
fn read(p: &Path) -> String { std::fs::read_to_string(p).unwrap_or_default() }
const NODE_LAUNCHER: &str = "#!/bin/sh\nexport HERDR_SUPERWORK_DATA_DIR='/x/data'\nexec node '/home/u/dev/herdr-superwork/bin/herdr-superwork.js' \"$@\"\n";
/// Data as the Node version left it: a task, its keeper's note, its launcher, a link.
fn node_data(data: &Path, version: Option<&str>) {
    write(&data.join("tasks/proj/1/task.json"), "{\"id\":\"proj/1\",\"title\":\"one\"}");
    write(&data.join("tasks/proj/1/inbox.jsonl"), "{\"a\":1}\n");
    write(&data.join("bin").join(OLD_NAME), NODE_LAUNCHER);
    if let Some(v) = version { write(&data.join("keeper.json"), &format!("{{\"pid\":1,\"version\":\"{v}\"}}")); }
    std::os::unix::fs::symlink("tasks/proj", data.join("latest")).unwrap();
}
/// Every path under a folder with what it holds (a link: where it points), to compare two folders.
fn tree(dir: &Path) -> Vec<(String, String)> {
    let mut out = vec![];
    fn walk(d: &Path, root: &Path, out: &mut Vec<(String, String)>) {
        for e in std::fs::read_dir(d).unwrap().flatten() {
            let (p, m) = (e.path(), std::fs::symlink_metadata(e.path()).unwrap());
            let rel = p.strip_prefix(root).unwrap().display().to_string();
            if m.file_type().is_symlink() { out.push((rel, format!("-> {}", std::fs::read_link(&p).unwrap().display()))); }
            else if m.is_dir() { out.push((rel.clone(), "/".into())); walk(&p, root, out); }
            else { out.push((rel, std::fs::read_to_string(&p).unwrap())); }
        }
    }
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[test]
fn who_wrote_the_data() {
    let (_b, data, _one) = scratch();
    assert_eq!(older_writer(&data), None, "no data at all");
    std::fs::create_dir_all(&data).unwrap();
    assert_eq!(older_writer(&data), None, "an empty folder");
    write(&data.join("keeper.json"), "{\"version\":\"0.2.0\"}");
    assert_eq!(older_writer(&data), Some("0.2.0".into()), "its keeper wrote its version down");
    write(&data.join("keeper.json"), "{\"version\":\"0.3.0\"}");
    assert_eq!(older_writer(&data), None, "this version's keeper");
    write(&data.join("bin").join(OLD_NAME), NODE_LAUNCHER);
    assert_eq!(older_writer(&data), Some("node".into()), "only its launcher says it was Node");
    write(&data.join("bin/herdr-projects"), "#!/bin/sh\nexec '/p/bin/herdr-projects-linux-x86_64' \"$@\"\n");
    assert_eq!(older_writer(&data), None, "this version's launcher");
    assert_eq!(copy_place(&data, "0.2.0"), data.parent().unwrap().join("data-0.2.0"));
}

#[test]
fn the_copy_is_the_data_as_it_was_and_made_once() {
    let (_b, data, _one) = scratch();
    node_data(&data, Some("0.2.0"));
    let before = tree(&data);
    let line = keep_old_copy(&data).unwrap().expect("a copy is made");
    assert_eq!(line, format!("herdr-projects {} uses the data version 0.2.0 kept, unchanged; a copy of it as it was is in ~/.herdr-projects/data-0.2.0.", sw::store::files::VERSION));
    let copy = data.parent().unwrap().join("data-0.2.0");
    assert_eq!(tree(&copy), before, "every file, folder and link as it was");
    assert_eq!(tree(&data), before, "the data itself is not changed");
    assert!(std::fs::symlink_metadata(copy.join("latest")).unwrap().file_type().is_symlink(), "a link stays a link");
    let mut names: Vec<String> = std::fs::read_dir(data.parent().unwrap()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert_eq!(names, vec!["data", "data-0.2.0"], "no spare copy left");
    // Made once: the next run, with the data changed since, leaves the copy as it is.
    write(&data.join("tasks/proj/1/task.json"), "changed");
    assert_eq!(keep_old_copy(&data).unwrap(), None);
    assert_eq!(read(&copy.join("tasks/proj/1/task.json")), "{\"id\":\"proj/1\",\"title\":\"one\"}");
}

#[test]
fn a_copy_already_there_or_half_made_before() {
    let (_b, data, _one) = scratch();
    node_data(&data, None);
    // The keeper wrote no version: named after Node.
    let place = data.parent().unwrap().join("data-node");
    std::fs::create_dir_all(&place).unwrap();
    assert_eq!(keep_old_copy(&data).unwrap(), None, "one is there already: skipped");
    std::fs::remove_dir(&place).unwrap();
    // A run that was cut off while copying left a spare (its process is gone): removed, and the copy made whole.
    let spare = data.parent().unwrap().join("data-node.copying-999999999");
    write(&spare.join("tasks/half"), "half");
    let line = keep_old_copy(&data).unwrap().unwrap();
    assert!(line.contains("the data the older (Node) version kept") && line.ends_with("~/.herdr-projects/data-node."), "{line}");
    assert!(!spare.exists());
    assert!(!place.join("tasks/half").exists() && place.join("tasks/proj/1/task.json").is_file());
}

#[test]
fn the_first_run_makes_it_and_says_it_once() {
    let (b, data, _one) = scratch();
    node_data(&data, Some("0.2.0"));
    let home = b.join("home");
    let env = sw::store::move_home::env_now();
    // The first run is a hook, with nobody reading: the line waits for the board (or a command in a terminal).
    copy_at_start("hook");
    assert!(data.parent().unwrap().join("data-0.2.0/tasks/proj/1/task.json").is_file());
    let note = sw::store::move_home::move_note(&home, &env);
    assert!(note.contains("a copy of it as it was is in ~/.herdr-projects/data-0.2.0."), "{note}");
    // Said once: after that nothing is added, the copy is not made again.
    copy_at_start("hook");
    assert_eq!(sw::store::move_home::move_note(&home, &env), note);
    sw::store::move_home::move_told(&home, &env);
    copy_at_start("list");
    assert_eq!(sw::store::move_home::move_note(&home, &env), "");
}

#[test]
fn the_program_keeps_the_copy_before_its_first_command() {
    let (_b, data, _one) = scratch();
    node_data(&data, Some("0.2.0"));
    let before = tree(&data);
    assert_eq!(sw::cli::main(vec!["version".into()]), 0);
    assert_eq!(tree(&data.parent().unwrap().join("data-0.2.0")), before);
}
