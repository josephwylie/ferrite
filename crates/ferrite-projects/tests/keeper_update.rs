//! [real programs] A keeper whose program an update replaces (cargo puts a new file in the old one's place) starts its
//! successor on the new program itself, with no board open and no agent's hook to do it: also when it first sees the
//! new file while that is still being written. Recorded on five machines: "ending: the plugin was updated", then no
//! keeper until one was started by hand.
use crate::common;
use common::*;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A copy of the program in the world's own folder: the one the test updates.
fn copy(w: &World) -> PathBuf {
    let exe = w.base.join("bin").join("herdr-projects");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_ferrite-projects"), &exe).unwrap(); // Ferrite: the binary is `ferrite-projects`
    exe
}
/// The keeper, started as a hook starts it, on a herdr that is there with nothing in it (so its ticking goes on).
fn start(w: &World, exe: &Path) -> std::process::Child {
    let herdr = w.base.join("bin").join("herdr");
    std::fs::write(&herdr, "#!/bin/sh\necho '{\"workspaces\":[]}'\n").unwrap();
    std::fs::set_permissions(&herdr, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::process::Command::new(exe).arg("keeper").current_dir(&w.root)
        .env_remove("HERDR_PROJECTS_NO_KEEPER").env_remove("HERDR_PROJECTS_INLINE")
        .env("HERDR_BIN_PATH", &herdr).env("HERDR_PROJECTS_STAMP_MS", "100")
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .spawn().unwrap()
}
fn log() -> String { std::fs::read_to_string(sw::store::files::data_dir().join("keeper.log")).unwrap_or_default() }
/// The keepers that started, by their process numbers, in order.
fn started() -> Vec<i64> { log().lines().filter(|l| l.contains(" started (plugin")).filter_map(|l| l.split(' ').nth(1)?.parse().ok()).collect() }
fn until(ms: u64, done: impl Fn() -> bool) -> bool { let end = Instant::now() + Duration::from_millis(ms); while Instant::now() < end { if done() { return true; } std::thread::sleep(Duration::from_millis(20)); } done() }
fn alive(pid: i64) -> bool { unsafe { libc::kill(pid as i32, 0) == 0 } }
/// The new program in the old one's place, as cargo puts it: the old file goes (the running keeper's own file is then
/// "… (deleted)") and a new one takes its path, with a later time.
fn replace(exe: &Path, write: impl FnOnce(&Path)) {
    std::fs::remove_file(exe).unwrap();
    write(exe);
    std::fs::set_permissions(exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::File::options().write(true).open(exe).unwrap().set_modified(std::time::SystemTime::now() + Duration::from_secs(60)).unwrap();
}
/// After the update: the old keeper has gone and its successor, a new process, is the one keeper, beating. It is
/// stopped at the end, as a test leaves nothing running.
fn successor_runs(mut old: std::process::Child) {
    let first = old.id() as i64;
    assert!(until(30000, || started().iter().any(|&p| p != first)), "no successor started: {}", log());
    let next = *started().last().unwrap();
    let st = old.wait().unwrap();
    assert!(st.success(), "{st:?}");
    assert!(until(5000, || sw::keeper::keeper_alive()), "the successor does not beat: {}", log());
    let beat: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(sw::store::files::data_dir().join("keeper.json")).unwrap()).unwrap();
    assert_eq!(beat["pid"], next, "{}", log());
    assert!(alive(next));
    unsafe { libc::kill(next as i32, libc::SIGTERM); }
    assert!(until(10000, || !alive(next)), "the successor did not stop");
    assert!(log().ends_with("ending: a signal\n"), "{}", log());
}

#[test]
fn real_programs_a_keeper_whose_program_is_replaced_starts_its_successor_on_the_new_one() {
    let w = world();
    let exe = copy(&w);
    let old = start(&w, &exe);
    assert!(until(10000, || started().len() == 1), "the keeper did not start: {}", log());
    replace(&exe, |p| { std::fs::copy(env!("CARGO_BIN_EXE_ferrite-projects"), p).unwrap(); }); // Ferrite: the binary is `ferrite-projects`
    successor_runs(old);
    assert!(log().contains("ending: the plugin was updated"), "{}", log());
}

#[test]
fn real_programs_a_keeper_that_sees_its_new_program_half_written_keeps_on_and_starts_the_successor_once_it_is_whole() {
    let w = world();
    let exe = copy(&w);
    let old = start(&w, &exe);
    assert!(until(10000, || started().len() == 1), "the keeper did not start: {}", log());
    let whole = std::fs::read(env!("CARGO_BIN_EXE_ferrite-projects")).unwrap(); // Ferrite: the binary is `ferrite-projects`
    // The linker has written half of the new file and has it open still: it cannot be started yet ("text file busy").
    let mut f = None;
    replace(&exe, |p| { let mut o = std::fs::File::create(p).unwrap(); o.write_all(&whole[..whole.len() / 2]).unwrap(); f = Some(o); });
    assert!(until(30000, || log().contains("the new version did not start")), "the keeper did not try the half-written file: {}", log());
    assert!(alive(old.id() as i64) && started().len() == 1, "the keeper went with no successor: {}", log());
    assert!(until(5000, || sw::keeper::keeper_alive()), "the keeper that kept on does not beat: {}", log());
    // The linker is done: the keeper tries again, on the whole file.
    let mut o = f.take().unwrap();
    o.write_all(&whole[whole.len() / 2..]).unwrap();
    o.set_modified(std::time::SystemTime::now() + Duration::from_secs(120)).unwrap();
    drop(o);
    successor_runs(old);
}
