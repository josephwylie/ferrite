//! A line for a board that is running, from a command that has no screen of its own (the board key's action): left in
//! the data folder for the board in one pane, which shows it on its message line within a second.
use crate::herdr::cli::session_name;
use crate::json::*;
use crate::store::files::{data_dir, read_json, write_json};

fn file(socket: &str) -> std::path::PathBuf { data_dir().join(format!("board-say-{}.json", session_name(socket))) }
const FRESH_MS: i64 = 15000;
/// `to`: the herdr pane of the board that is to say it.
pub fn say(to: &str, text: &str, kind: &str, socket: &str) { if !to.is_empty() && !text.is_empty() { let _ = write_json(&file(socket), &json!({ "to": to, "text": text, "kind": kind, "at": crate::clock::now() })); } }
/// What was left for the board in pane `me`: taken once. Another board's line is left for it; an old one is dropped.
pub fn heard(me: &str, socket: &str) -> Option<(String, String)> {
    let n = read_json(&file(socket))?;
    let fresh = crate::clock::now() - n.i("at") < FRESH_MS;
    if fresh && (me.is_empty() || n.s("to") != me) { return None; }
    let _ = std::fs::remove_file(file(socket));
    (fresh && n.b("text")).then(|| (n.t("text"), if n.b("kind") { n.t("kind") } else { "yellow".into() }))
}
