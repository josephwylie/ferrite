//! Ferrite: detached jobs must be reaped by the long-lived keeper or window.
use std::path::PathBuf;
use std::time::{Duration, Instant};

struct Home(PathBuf);

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn gone(pid: u32) -> bool {
    let result = unsafe { libc::kill(pid as i32, 0) };
    result == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

fn until(done: impl Fn() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        if done() { return true; }
        std::thread::sleep(Duration::from_millis(10));
    }
    done()
}

#[test]
fn detached_jobs_are_reaped_while_their_parent_stays_alive() {
    use std::os::unix::fs::PermissionsExt;
    let home = Home(std::env::temp_dir().join(format!("ferrite-child-reaping-{}", std::process::id())));
    std::fs::create_dir_all(&home.0).unwrap();
    let bin = home.0.join("job");
    std::fs::write(&bin, "#!/bin/sh\ncase \"$1\" in\n  hold) exec /bin/sleep 2 ;;\n  fail) exit 7 ;;\n  serve) printf '{\"pid\":%s,\"port\":9876}' \"$$\" > \"$FERRITE_CHILD_SERVE_FILE\" ;;\nesac\nexit 0\n").unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    unsafe {
        std::env::set_var("HERDR_PROJECTS_HOME", &home.0);
        std::env::set_var("HERDR_PROJECTS_DATA_DIR", home.0.join("data"));
        std::env::set_var("HERDR_PROJECTS_ENV_OK", "1");
        std::env::set_var("FERRITE_PROJECTS_BIN", &bin);
        std::env::set_var("FERRITE_CHILD_SERVE_FILE", home.0.join("data/serve.json"));
        std::env::remove_var("HERDR_PROJECTS_INLINE");
    }

    // An unfinished job must not block the caller or be stopped by returning from spawn.
    let before = Instant::now();
    let slow = sw_flow::housekeeping::spawn_loose(&["hold"]);
    assert_ne!(slow, 0);
    assert!(before.elapsed() < Duration::from_secs(1));
    assert!(!gone(slow), "a running detached job must be left running");

    let mut pids = Vec::new();
    for _ in 0..8 {
        pids.push(sw_flow::runner::spawn_runner("finish", "-", &[]));
        pids.push(sw_flow::housekeeping::spawn_loose(&["run", "prpoll", "-"]));
        pids.push(sw_flow::housekeeping::spawn_loose(&["fail"]));
    }
    assert!(pids.iter().all(|&pid| pid != 0), "every job must start");

    let notifications = home.0.join("notifications");
    for _ in 0..8 {
        sw_flow::runner::detached_sh("printf '%s\\n' \"$$\" >> \"$FERRITE_CHILD_PIDS\"", &[("FERRITE_CHILD_PIDS", notifications.display().to_string())]);
    }
    assert!(until(|| std::fs::read_to_string(&notifications).unwrap_or_default().lines().count() == 8));
    pids.extend(std::fs::read_to_string(&notifications).unwrap().lines().map(|s| s.parse::<u32>().unwrap()));

    let server = sw_flow::review_page::ensure_server();
    pids.push(server["pid"].as_u64().expect("the test page server must start") as u32);

    let reaped = until(|| pids.iter().all(|&pid| gone(pid)));
    let remaining: Vec<_> = pids.into_iter().filter(|&pid| !gone(pid)).collect();
    assert!(reaped, "finished background jobs still occupy process slots: {remaining:?}");
    assert!(until(|| gone(slow)), "the long job must also be reaped when it ends");
    unsafe { std::env::set_var("FERRITE_PROJECTS_BIN", home.0.join("missing")); }
    assert_eq!(sw_flow::housekeeping::spawn_loose(&["run"]), 0, "a failed spawn must still be reported");
}
