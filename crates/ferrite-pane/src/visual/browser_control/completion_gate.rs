//! Packaged feature-fixture gate, compiled only with `browser-smoke`.
//! It holds native attachment or an actual write acknowledgement to exercise
//! both sides of the Server's dispatch authorization deadline.
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub(super) fn after_write(stopping: &AtomicBool) {
    wait(
        stopping,
        "completion-gate",
        "dispatched",
        b"actual native write acknowledged before completion gate",
    );
}

pub(super) fn before_attachment(stopping: &AtomicBool) {
    wait(
        stopping,
        "dispatch-gate",
        "waiting",
        b"native worker reserved before attachment and dispatch authorization",
    );
}

fn wait(stopping: &AtomicBool, name: &str, ready: &str, message: &[u8]) {
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
    let arm = directory.join(format!("{name}-arm"));
    if !arm.is_file() || std::fs::remove_file(&arm).is_err() {
        return;
    }
    if std::fs::write(directory.join(format!("{name}-{ready}")), message).is_err() {
        return;
    }
    let until = Instant::now() + Duration::from_secs(50);
    while !stopping.load(Ordering::Acquire)
        && Instant::now() < until
        && !directory.join(format!("{name}-release")).exists()
    {
        std::thread::sleep(Duration::from_millis(20));
    }
}
