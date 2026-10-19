//! How long a repo's check took the last few times it ran in full on this machine: what its minutes left are worked
//! out from. One list per repo and command: another command is another check. The middle of the last few, so one slow
//! run on a busy machine does not move it.
use super::files::*;
use crate::json::*;
use std::path::{Path, PathBuf};

pub const CHECK_RUNS_KEPT: usize = 5;
fn file() -> PathBuf { data_dir().join("check-runs.json") }
fn key(root: &Path, cmd: &str) -> String { format!("{}\n{}", absolute(root).display(), cmd.trim()) }
/// Milliseconds a full run usually takes, 0 when none was timed yet.
pub fn usual_check(root: &Path, cmd: &str) -> i64 {
    let all = read_obj(&file()).unwrap_or_default();
    let mut runs: Vec<f64> = all.arr(&key(root, cmd)).iter().map(number).filter(|n| n.is_finite() && *n > 0.0).collect();
    runs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if runs.is_empty() { 0 } else { runs[(runs.len() - 1) / 2] as i64 }
}
pub fn note_check_run(root: &Path, cmd: &str, ms: i64) {
    if ms <= 0 { return; }
    let _ = with_lock("check-runs", || {
        let mut all = read_obj(&file()).unwrap_or_default();
        let k = key(root, cmd);
        let mut list: Vec<Value> = all.arr(&k).to_vec();
        list.push(json!(ms));
        let list: Vec<Value> = list.iter().skip(list.len().saturating_sub(CHECK_RUNS_KEPT)).cloned().collect();
        all.shift_remove(&k);
        all.insert(k, Value::Array(list));
        while all.len() > 200 { let k = all.keys().next().cloned().unwrap_or_default(); all.shift_remove(&k); }
        write_json(&file(), &Value::Object(all))
    });
}
