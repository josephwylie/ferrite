//! What an agent was told: every prompt in its saved conversation, oldest first, where the program's file can be read.
use super::read_turn::when;
use super::talk::unwrap_paste;
use crate::json::*;
use std::path::Path;

/// Claude Code: a prompt is a user entry with text of its own: not a tool's result, not a note the program wrote
/// itself, not a helper agent's. None when a line cannot be read (it may hide a prompt).
fn claude_prompts(file: &Path) -> Option<Vec<(i64, String)>> {
    let text = std::fs::read_to_string(file).ok()?;
    let mut out = vec![];
    for line in text.split('\n') {
        if line.trim().is_empty() { continue; }
        let e: Value = serde_json::from_str(line).ok()?;
        if e.s("type") != "user" || !e.at("message").is_object() || e.b("isMeta") || e.b("isSidechain") || e.b("isCompactSummary") { continue; }
        let c = e.at("message").at("content");
        let t = match c { Value::String(s) => s.clone(), _ => c.as_array().map(|a| a.iter().filter(|x| x.s("type") == "text").map(|x| x.t("text")).collect::<Vec<_>>().join("\n")).unwrap_or_default() };
        let t = unwrap_paste(t.trim());
        if !t.is_empty() { out.push((when(e.at("timestamp")), t)); }
    }
    Some(out)
}
/// None when nothing can be said: a kind with no reader, no file, a file that cannot be read.
pub fn prompts_of(kind: &str, file: Option<&Path>) -> Option<Vec<(i64, String)>> {
    match (kind, file) { ("claude", Some(f)) => claude_prompts(f), _ => None }
}
