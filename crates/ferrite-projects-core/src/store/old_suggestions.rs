//! Suggested tasks were taken out of the board. What tasks saved by an older version still carry of them is written
//! down once, as one page among the board's reports (the ones that still waited for an answer: task, title,
//! description, who made it), and then removed from the tasks.
use super::files::*;
use super::marks::stamp_file;
use super::tasks::{list, update, Change, ListOpts};
use crate::json::*;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

pub fn old_suggestions_file() -> PathBuf { reports_dir().join("old-suggestions.md") }
const HEAD: &str = "# Suggested tasks that were still open\n\nSuggested tasks were removed from the board. These still waited for an answer when they were taken off their tasks.\nNothing is done with them: add a task yourself for one you still want.\n";
fn entry(t: &Obj, s: &Value) -> String {
    let title = crate::text::squash(if s.s("title").is_empty() { "(no title)" } else { s.s("title") });
    let at = if s.b("at") { chrono::DateTime::from_timestamp_millis(s.i("at")).map(|d| format!(" · {}", d.format("%Y-%m-%d"))).unwrap_or_default() } else { String::new() };
    let body = if !s.s("description").trim().is_empty() { s.s("description").trim().to_string() } else if s.b("where") { format!("Where: {}", s.t("where")) } else { String::new() };
    [format!("## {title}"), format!("On {} ({}) · from {}{at}", t.s("id"), crate::text::squash(t.s("title")), if s.s("by") == "reviewer" { "the reviewer" } else { "the agent" }), body]
        .into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("\n\n")
}
/// Returns how many were written down. Done once: afterwards a stamp says so.
pub fn save_old_suggestions() -> usize {
    let stamp = stamp_file("old-suggestions-saved");
    if stamp.exists() { return 0; }
    let (mut saved, mut left) = (0, false);
    for t in list(ListOpts { include_archived: true, ..Default::default() }) {
        if !t.contains_key("suggestions") { continue; }
        let r = update(t.s("id"), |t| {
            let open: Vec<Value> = t.arr("suggestions").iter().filter(|s| s.s("state") == "open").cloned().collect();
            if !open.is_empty() {
                let file = old_suggestions_file();
                if let Some(d) = file.parent() { fs::create_dir_all(d)?; }
                let head = if file.exists() { "" } else { HEAD };
                let mut f = fs::OpenOptions::new().create(true).append(true).open(&file)?;
                f.write_all(format!("{head}\n{}\n", open.iter().map(|s| entry(t, s)).collect::<Vec<_>>().join("\n\n")).as_bytes())?;
                saved += open.len();
            }
            t.del("suggestions");
            Ok(Change::Write)
        });
        if r.is_err() { left = true; }
    }
    if !left { if let Some(d) = stamp.parent() { let _ = fs::create_dir_all(d); } let _ = fs::write(&stamp, crate::clock::now().to_string()); }
    saved
}
