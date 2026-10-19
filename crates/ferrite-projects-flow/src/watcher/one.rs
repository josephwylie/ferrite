//! One Watcher call at a time on the whole machine: the turn is a small file in the data folder, so an open board and
//! a keeper that tick at the same moment cannot both start one.
use crate::json::*;
use crate::store::files::{data_dir, marker, read_json, with_lock_wait, write_json};

/// A call that takes longer is stopped.
pub const CALL_MS: i64 = 45000;
fn file() -> std::path::PathBuf { data_dir().join("watcher-call.json") }
/// Somebody's call is under way: its turn was taken, not given back, and it cannot have been stopped for taking too
/// long yet. A process that died with the turn in its hand so holds it for under a minute, never for good.
fn taken(now: i64) -> bool { read_json(&file()).map(|m| now - m.i("at") < CALL_MS + 5000 && now >= m.i("at")).unwrap_or(false) }
/// Take the turn: false when another call runs, in this process or any other.
pub fn take_turn(now: i64) -> bool {
    with_lock_wait("watcher-call", 0, || {
        if taken(now) { return Ok(false); }
        let mut m = marker(); m.put("at", now);
        write_json(&file(), &Value::Object(m))?;
        Ok(true)
    }).unwrap_or(false)
}
pub fn give_turn() { let _ = std::fs::remove_file(file()); }
