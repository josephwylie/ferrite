//! ADR 0019: `diagnostics::open` installs the panic hook. Its own test
//! binary, because the hook and the log are process-wide.

use std::fs;

#[test]
fn a_panic_after_open_leaves_a_crash_report_and_a_log_line() {
    let dir = std::env::temp_dir().join(format!("ferrite-crash-report-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    ferrite_core::diagnostics::open(&dir);

    let panicked = std::thread::Builder::new()
        .name("doomed".into())
        .spawn(|| panic!("boom"))
        .unwrap()
        .join();

    assert!(panicked.is_err());
    let logs = ferrite_core::diagnostics::logs(&dir);
    let crash = fs::read_dir(&logs)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("crash-")
        })
        .expect("a crash report");
    let report = fs::read_to_string(crash).unwrap();
    assert!(
        report.contains("boom") && report.contains("thread: doomed"),
        "{report}"
    );
    let log = fs::read_to_string(logs.join("ferrite.log")).unwrap();
    assert!(log.contains("crashed; report in"), "{log}");
}
