//! A working task's line while the Watcher has written none for this run: the last sentence it wrote, kept and said
//! with its age, or "Just started". Every new run drops the Watcher's guess, and its first look takes about a minute.
//! Until then the row says what the agent was last seen doing, with how long ago that was ("12m ago: Running the
//! check"), so an old sentence is never read as a new one. Never a bare "…".
use crate::json::*;
use crate::shown::words::age;
use crate::store::settings::lookup::setting;

/// How long a run with no sentence yet counts as just started: three looks of the Watcher at its usual pace.
pub const JUST_MS: i64 = 3 * 60000;
/// Whether the line is the board's own plain word (the Watcher is off): the row leaves it out.
pub fn quiet_plain(t: &Obj, role: &str) -> bool { !(role == "agent" && !t.at("lastLine").s("msg").is_empty()) && setting("watcher.agent", None) == json!("none") }
pub fn quiet_line(t: &Obj, role: &str, from: i64, now: i64) -> String {
    let kept = if role == "agent" { t.at("lastLine") } else { &NULL };
    if !kept.s("msg").is_empty() { return format!("{}: {}", if kept.b("at") { format!("{} ago", age(now - kept.i("at"))) } else { "Earlier".into() }, kept.s("msg")); }
    if quiet_plain(t, role) { return if role == "reviewer" { "Under review" } else { "Working" }.into(); }
    if from != 0 && now - from >= JUST_MS { return "No status yet".into(); }
    if role == "reviewer" { "Review just started" } else { "Just started" }.into()
}
