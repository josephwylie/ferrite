//! Packaged feature-fixture gate, compiled only with `browser-smoke`.
//! It delays an actual successful write acknowledgement, never page dispatch
//! or page state, to exercise the Server's real uncertain-reply deadline.
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub(super) fn after_write(stopping: &AtomicBool) {
    let Some(directory) = std::env::var_os("FERRITE_BROWSER_WINDOW_OUTPUT").map(PathBuf::from)
    else {
        return;
    };
    if std::env::var("FERRITE_BROWSER_WINDOW_SCENARIO")
        .ok()
        .as_deref()
        != Some("boundaries")
    {
        return;
    }
    let arm = directory.join("completion-gate-arm");
    if !arm.is_file() || std::fs::remove_file(&arm).is_err() {
        return;
    }
    if std::fs::write(
        directory.join("completion-gate-dispatched"),
        b"actual native write acknowledged before completion gate",
    )
    .is_err()
    {
        return;
    }
    let until = Instant::now() + Duration::from_secs(50);
    while !stopping.load(Ordering::Acquire)
        && Instant::now() < until
        && !directory.join("completion-gate-release").exists()
    {
        std::thread::sleep(Duration::from_millis(20));
    }
}
