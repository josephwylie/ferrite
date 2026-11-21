//! A test program's temporary folder is its own and goes when the program
//! does. Before `main`, the program points `TMPDIR`, `TMP` and `TEMP` at
//! `ferrite-tests/<pid>` under `/tmp` (Windows: the system's temporary
//! folder), so every `temp_dir()` the tests
//! and the code they run reach lands there; it is removed at exit, a failed
//! test's included. A program that dies without exiting leaves it to the next
//! test program, which removes each such folder no live program holds.
//!
//! Each test program includes this file (`#[path]`): a constructor in a
//! dependency is dropped by the linker.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// Set in a test program and so in what it starts, which share its folder.
const OWNED: &str = "FERRITE_TEST_TEMP";
/// Locked for the owning program's life.
const HELD: &str = ".held";

static ROOT: Mutex<Option<(PathBuf, File)>> = Mutex::new(None);

#[ctor::ctor(unsafe)]
fn isolate() {
    if std::env::var_os(OWNED).is_some() {
        return;
    }
    // Short on Unix: a socket a test makes under it must fit macOS's 104-byte
    // SUN_LEN, and macOS's own temporary folder takes 48 of them.
    let system = if cfg!(unix) { PathBuf::from("/tmp") } else { std::env::temp_dir() };
    let parent = system.join("ferrite-tests");
    sweep(&parent);
    let root = parent.join(std::process::id().to_string());
    let _ = std::fs::remove_dir_all(&root);
    let Ok(held) = std::fs::create_dir_all(&root).and_then(|()| File::create(root.join(HELD))) else {
        return;
    };
    if held.lock().is_err() {
        return;
    }
    for name in ["TMPDIR", "TMP", "TEMP", OWNED] {
        std::env::set_var(name, &root);
    }
    *ROOT.lock().unwrap() = Some((root, held));
    unsafe { libc::atexit(remove) };
}

extern "C" fn remove() {
    if let Some((root, held)) = ROOT.lock().ok().and_then(|mut root| root.take()) {
        drop(held);
        // Best effort: Windows keeps a folder a straggling child still has open.
        let _ = std::fs::remove_dir_all(root);
    }
}

/// Removes the folders of programs that died without exiting. One younger
/// than a minute may be a program's that has not taken its lock yet.
fn sweep(parent: &Path) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let settled = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .is_ok_and(|at| at.elapsed().is_ok_and(|age| age > Duration::from_secs(60)));
        let free = || File::open(entry.path().join(HELD)).map_or(true, |held| held.try_lock().is_ok());
        if settled && free() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}
