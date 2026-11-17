//! What Ferrite says about itself after the fact (ADR 0019): a capped,
//! rotating log per process under `~/.ferrite/logs` (the window's and the
//! Server's), a crash report per panic beside them, and the text Settings ›
//! About copies for a bug report, which carries both logs.
//!
//! A Dock launch has no terminal, so anything only printed is lost. Every
//! line goes to stderr as before and, once `open` has named the directory,
//! to the log too. The log is process-wide on purpose: a Session ending on
//! a reader thread and a panic on any thread write to the same file.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// The window's live log under `logs/`.
const LOG: &str = "ferrite.log";
/// The Server's, beside it: most of what a Thread does is said there.
const SERVER_LOG: &str = "server.log";
/// Past this a live log becomes its previous one: two files, about 2 MB.
const CAP: u64 = 1024 * 1024;
/// Crash reports kept; a new one past this removes the oldest.
const CRASHES: usize = 5;
/// How much of the log a copied report carries.
const REPORT_LINES: usize = 200;

/// Write one line to the log (and stderr): `log!("thread {id} ended: {reason}")`.
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::diagnostics::write(format_args!($($arg)*))
    };
}

static SINK: OnceLock<Mutex<Log>> = OnceLock::new();

/// Start logging into `<dir>/logs`, and write a crash report there for
/// every panic. Called once, at launch; before it lines reach stderr only.
pub fn open(dir: &Path) {
    open_as(dir, LOG);
}

/// `open`, for the Server: a log of its own, so its lines and the window's
/// never interleave.
pub fn open_server(dir: &Path) {
    open_as(dir, SERVER_LOG);
}

fn open_as(dir: &Path, name: &'static str) {
    let logs = logs(dir);
    let log = Log::new(logs.clone(), name, CAP);
    if SINK.set(Mutex::new(log)).is_err() {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let report = format!(
            "{info}\nthread: {}\n\n{}",
            thread.name().unwrap_or("unnamed"),
            std::backtrace::Backtrace::force_capture()
        );
        match write_crash(&logs, &report) {
            Ok(path) => write(format_args!("crashed; report in {}", path.display())),
            Err(error) => write(format_args!("crashed; no report written: {error}")),
        }
        previous(info);
    }));
}

/// Where the log and crash reports live under `dir` (`~/.ferrite`).
pub fn logs(dir: &Path) -> PathBuf {
    dir.join("logs")
}

/// What `log!` expands to.
pub fn write(line: fmt::Arguments) {
    eprintln!("ferrite: {line}");
    if let Some(log) = SINK.get() {
        let _ = log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .append(&line.to_string());
    }
}

/// The text "Copy diagnostics" puts on the clipboard: `facts` (version,
/// platform, the CLIs) one per line, then from `<dir>/logs` the newest
/// crash report whole and the end of each process's log.
pub fn report(dir: &Path, facts: &[(&str, String)]) -> String {
    let dir = logs(dir);
    let mut text = String::from("Ferrite diagnostics\n");
    for (key, value) in facts {
        text.push_str(&format!("{key}: {value}\n"));
    }
    if let Some(crash) = newest_crash(&dir) {
        let contents = fs::read_to_string(&crash).unwrap_or_default();
        text.push_str(&format!("\n--- {} ---\n{contents}\n", crash.display()));
    }
    for name in [LOG, SERVER_LOG] {
        let tail = tail(&dir, name);
        text.push_str(&format!("\n--- {name}, last {} lines ---\n", tail.len()));
        for line in tail {
            text.push_str(&line);
            text.push('\n');
        }
    }
    text
}

/// The last `REPORT_LINES` lines of the log `name` in `dir`, oldest first,
/// reaching back into its previous file when the live one is short.
fn tail(dir: &Path, name: &str) -> Vec<String> {
    let mut lines: Vec<String> = [previous(name), name.to_string()]
        .iter()
        .filter_map(|name| fs::read_to_string(dir.join(name)).ok())
        .flat_map(|contents| contents.lines().map(str::to_string).collect::<Vec<_>>())
        .collect();
    lines.drain(..lines.len().saturating_sub(REPORT_LINES));
    lines
}

/// What the log `name` is renamed to when it fills.
fn previous(name: &str) -> String {
    format!("{name}.1")
}

/// The log file pair in one directory, rotated at `cap` bytes.
struct Log {
    dir: PathBuf,
    name: &'static str,
    cap: u64,
    file: Option<File>,
}

impl Log {
    fn new(dir: PathBuf, name: &'static str, cap: u64) -> Self {
        Self {
            dir,
            name,
            cap,
            file: None,
        }
    }

    /// One timestamped line. Past the cap the live file becomes the
    /// previous one (replacing it) and a fresh one starts.
    fn append(&mut self, line: &str) -> io::Result<()> {
        let path = self.dir.join(self.name);
        let entry = format!("{} {line}\n", stamp("%Y-%m-%d %H:%M:%S%.3f"));
        let full = fs::metadata(&path).is_ok_and(|meta| meta.len() + entry.len() as u64 > self.cap);
        if full {
            self.file = None;
            fs::rename(&path, self.dir.join(previous(self.name)))?;
        }
        if self.file.is_none() {
            fs::create_dir_all(&self.dir)?;
            self.file = Some(OpenOptions::new().create(true).append(true).open(&path)?);
        }
        self.file
            .as_mut()
            .expect("opened")
            .write_all(entry.as_bytes())
    }
}

/// `report` written to `crash-<time>.txt` in `dir`, keeping only the
/// newest `CRASHES`: a panic on every restart must not fill the disk.
fn write_crash(dir: &Path, report: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("crash-{}.txt", stamp("%Y%m%d-%H%M%S%.3f")));
    let header = format!(
        "Ferrite {} on {} {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    fs::write(&path, header + report)?;
    let crashes = crashes(dir);
    for old in &crashes[..crashes.len().saturating_sub(CRASHES)] {
        let _ = fs::remove_file(old);
    }
    Ok(path)
}

/// The newest crash report in `dir`.
fn newest_crash(dir: &Path) -> Option<PathBuf> {
    crashes(dir).pop()
}

/// The crash reports in `dir`, oldest first: their names sort by time.
fn crashes(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut crashes: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("crash-"))
        })
        .collect();
    crashes.sort();
    crashes
}

fn stamp(format: &str) -> String {
    chrono::Local::now().format(format).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ferrite-diagnostics-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// The log keeps two files at most: past the cap the live one becomes
    /// the previous one, and the oldest lines go.
    #[test]
    fn the_log_rotates_at_its_cap() {
        let dir = scratch("rotate");
        let mut log = Log::new(dir.clone(), LOG, 200);

        for n in 0..20 {
            log.append(&format!("line {n}")).unwrap();
        }

        let live = fs::read_to_string(dir.join(LOG)).unwrap();
        let previous = fs::read_to_string(dir.join(previous(LOG))).unwrap();
        assert!(live.len() <= 200 && previous.len() <= 200);
        assert!(live.ends_with("line 19\n"), "{live}");
        assert!(!previous.contains("line 0\n"), "the oldest lines are gone");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
    }

    /// Only the newest crash reports stay; older ones go as new ones come.
    #[test]
    fn crash_reports_are_capped() {
        let dir = scratch("crashes");
        fs::create_dir_all(&dir).unwrap();
        for n in 0..CRASHES + 2 {
            fs::write(dir.join(format!("crash-20261001-09000{n}.000.txt")), "old").unwrap();
        }

        let newest = write_crash(&dir, "panicked at 'boom'").unwrap();

        let kept = crashes(&dir);
        assert_eq!(kept.len(), CRASHES);
        assert_eq!(kept.last(), Some(&newest));
        assert!(!dir.join("crash-20261001-090000.000.txt").exists());
    }

    /// The copied report carries the facts, the newest crash and the end
    /// of the log, oldest line first.
    #[test]
    fn the_report_carries_facts_the_newest_crash_and_the_log() {
        let dir = scratch("report");
        let logs = logs(&dir);
        let mut log = Log::new(logs.clone(), LOG, CAP);
        log.append("thread 3: session ended: exit status 1")
            .unwrap();
        log.append("thread 4: parked").unwrap();
        fs::write(logs.join("crash-20261001-090000.000.txt"), "old crash").unwrap();
        let newest = write_crash(&logs, "panicked at 'boom'").unwrap();

        let report = report(&dir, &[("Version", "0.5.0".into())]);

        assert!(report.contains("Version: 0.5.0\n"), "{report}");
        assert!(report.contains(&newest.display().to_string()), "{report}");
        assert!(report.contains("panicked at 'boom'"), "{report}");
        assert!(!report.contains("old crash"), "only the newest crash");
        let ended = report.find("session ended").unwrap();
        assert!(ended < report.find("parked").unwrap(), "{report}");
    }

    /// The copied report carries the Server's log after the window's, and
    /// reaches into a rotated file: a bug in a Thread is usually told there.
    #[test]
    fn the_report_carries_the_servers_log_beside_the_windows() {
        let dir = scratch("report-server");
        let logs = logs(&dir);
        Log::new(logs.clone(), LOG, CAP)
            .append("window: opened")
            .unwrap();
        let mut server = Log::new(logs.clone(), SERVER_LOG, 120);
        for n in 0..6 {
            server.append(&format!("server: line {n}")).unwrap();
        }
        assert!(logs.join(previous(SERVER_LOG)).exists(), "it rotated");

        let report = report(&dir, &[]);

        let at = |needle: &str| {
            report
                .find(needle)
                .unwrap_or_else(|| panic!("no {needle:?} in\n{report}"))
        };
        assert!(at("--- ferrite.log, last 1 lines ---") < at("window: opened"));
        assert!(at("window: opened") < at("--- server.log, last 6 lines ---"));
        assert!(at("--- server.log") < at("server: line 0"));
        assert!(at("server: line 0") < at("server: line 5"));
    }
}
