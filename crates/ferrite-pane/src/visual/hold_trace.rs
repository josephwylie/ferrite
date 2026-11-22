//! What happened to the operator's held gesture in the packaged boundaries
//! scenario, compiled only with `browser-smoke`: one JSON object per line in
//! `boundary-hold-trace.jsonl` beside the run's other evidence, each with the
//! wall clock it was written at (`unix_ms`).

use std::io::Write as _;
use std::path::PathBuf;

/// The trace's name in the run's output directory.
pub const FILE: &str = "boundary-hold-trace.jsonl";

/// Append `line`, an object, in a packaged smoke run; elsewhere nothing.
pub fn record(mut line: serde_json::Value) {
    let Some(output) = std::env::var_os("FERRITE_BROWSER_WINDOW_OUTPUT").map(PathBuf::from)
    else {
        return;
    };
    let unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    if let Some(fields) = line.as_object_mut() {
        fields.insert("unix_ms".into(), unix_ms.into());
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(output.join(FILE))
    {
        let _ = writeln!(file, "{line}");
    }
}
