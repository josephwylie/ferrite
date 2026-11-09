//! Bounded, argument-free phase measurements for the packaged feature fixture.
use std::{
    io::Write,
    path::PathBuf,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
pub(super) struct Timer {
    phase: &'static str,
    kind: &'static str,
    started: Instant,
    unix_ms: u128,
    pub outcome: &'static str,
}
impl Timer {
    pub fn start(phase: &'static str, kind: &str) -> Self {
        let kind = match kind {
            "launch" => "launch",
            "tab_list" => "tab_list",
            "tab_switch" => "tab_switch",
            "snapshot" => "snapshot",
            "console" => "console",
            "requests" => "requests",
            "wait" => "wait",
            "click" => "click",
            "type" => "type",
            "fill" => "fill",
            "select" => "select",
            "check" => "check",
            "uncheck" => "uncheck",
            "scroll" => "scroll",
            "scrollintoview" => "scrollintoview",
            _ => "none",
        };
        Self {
            phase,
            kind,
            started: Instant::now(),
            unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            outcome: "failed",
        }
    }
}
impl Drop for Timer {
    fn drop(&mut self) {
        if !std::env::args().any(|argument| argument == "--browser-window-smoke") {
            return;
        }
        let Some(directory) = std::env::var_os("FERRITE_BROWSER_WINDOW_OUTPUT").map(PathBuf::from)
        else {
            return;
        };
        let line=serde_json::json!({"phase":self.phase,"command":self.kind,"started_unix_ms":self.unix_ms,"elapsed_ms":self.started.elapsed().as_secs_f64()*1000.,"outcome":self.outcome}).to_string()+"\n";
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("controller-phases.jsonl"))
        {
            if file
                .metadata()
                .is_ok_and(|metadata| metadata.len() + line.len() as u64 <= 1024 * 1024)
            {
                let _ = file.write_all(line.as_bytes());
            }
        }
    }
}
