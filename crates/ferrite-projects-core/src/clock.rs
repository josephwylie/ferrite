//! The time: milliseconds since 1970, as the records keep it. A test moves it by hand (`set`), so nothing waits for
//! the real clock and nothing depends on how fast the machine is.
use std::cell::Cell;
use std::time::{SystemTime, UNIX_EPOCH};

thread_local! { static FIXED: Cell<Option<i64>> = const { Cell::new(None) }; }

pub fn now() -> i64 {
    if let Some(t) = FIXED.with(|f| f.get()) { return t; }
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
/// Tests only: the clock stands at `t` on this thread (None: the real one again).
pub fn set(t: Option<i64>) { FIXED.with(|f| f.set(t)); }
/// Tests only: the clock moves on.
pub fn advance(ms: i64) { let t = now(); set(Some(t + ms)); }
pub fn sleep_ms(ms: u64) { std::thread::sleep(std::time::Duration::from_millis(ms)); }
/// A time as JavaScript's toISOString writes it, for log lines.
pub fn iso(ms: i64) -> String { chrono::DateTime::from_timestamp_millis(ms).map(|d| d.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()).unwrap_or_default() }
