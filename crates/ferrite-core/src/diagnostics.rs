//! What Ferrite says about itself after the fact (ADR 0015): one capped,
//! rotating log under `~/.ferrite/logs`, a crash report per panic beside
//! it, and the text Settings › About copies for a bug report.
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

/// The log's directory under the one `open` is given (`~/.ferrite`).
const LOGS: &str = "logs";
/// The live log's name under `logs/`; the one before it is `ferrite.log.1`.
const LOG: &str = "ferrite.log";
const PREVIOUS: &str = "ferrite.log.1";
/// Past this the live log becomes the previous one: two files, about 2 MB.
const CAP: u64 = 1024 * 1024;
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
    let logs = dir.join(LOGS);
    let log = Log::new(logs.clone(), CAP);
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
/// crash report whole and the end of the log.
pub fn report(dir: &Path, facts: &[(&str, String)]) -> String {
    let dir = dir.join(LOGS);
    let mut text = String::from("Ferrite diagnostics\n");
    for (key, value) in facts {
        text.push_str(&format!("{key}: {value}\n"));
    }
    if let Some(crash) = newest_crash(&dir) {
        let contents = fs::read_to_string(&crash).unwrap_or_default();
        text.push_str(&format!("\n--- {} ---\n{contents}\n", crash.display()));
    }
    let lines: Vec<String> = [PREVIOUS, LOG]
        .iter()
        .filter_map(|name| fs::read_to_string(dir.join(name)).ok())
        .flat_map(|contents| contents.lines().map(str::to_string).collect::<Vec<_>>())
        .collect();
    let tail = &lines[lines.len().saturating_sub(REPORT_LINES)..];
    text.push_str(&format!("\n--- log, last {} lines ---\n", tail.len()));
    for line in tail {
        text.push_str(line);
        text.push('\n');
    }
    text
}

/// The log file pair in one directory, rotated at `cap` bytes.
struct Log {
    dir: PathBuf,
    cap: u64,
    file: Option<File>,
}

impl Log {
    fn new(dir: PathBuf, cap: u64) -> Self {
        Self {
            dir,
            cap,
            file: None,
        }
    }

    /// One timestamped line. Past the cap the live file becomes the
    /// previous one (replacing it) and a fresh one starts.
    fn append(&mut self, line: &str) -> io::Result<()> {
        let path = self.dir.join(LOG);
        let entry = format!("{} {line}\n", stamp("%Y-%m-%d %H:%M:%S%.3f"));
        let full = fs::metadata(&path).is_ok_and(|meta| meta.len() + entry.len() as u64 > self.cap);
        if full {
            self.file = None;
            fs::rename(&path, self.dir.join(PREVIOUS))?;
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

/// `report` written to `crash-<time>.txt` in `dir`.
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
    Ok(path)
}

/// The newest crash report in `dir`: their names sort by time.
fn newest_crash(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("crash-"))
        })
        .max()
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
        let mut log = Log::new(dir.clone(), 200);

        for n in 0..20 {
            log.append(&format!("line {n}")).unwrap();
        }

        let live = fs::read_to_string(dir.join(LOG)).unwrap();
        let previous = fs::read_to_string(dir.join(PREVIOUS)).unwrap();
        assert!(live.len() <= 200 && previous.len() <= 200);
        assert!(live.ends_with("line 19\n"), "{live}");
        assert!(!previous.contains("line 0\n"), "the oldest lines are gone");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
    }

    /// The copied report carries the facts, the newest crash and the end
    /// of the log, oldest line first.
    #[test]
    fn the_report_carries_facts_the_newest_crash_and_the_log() {
        let dir = scratch("report");
        let logs = dir.join(LOGS);
        let mut log = Log::new(logs.clone(), CAP);
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
}
