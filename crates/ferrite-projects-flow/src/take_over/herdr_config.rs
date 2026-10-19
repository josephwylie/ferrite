//! What older installs left in herdr's config.toml, made right as text (every other line stays as it was):
//!   - the old herdr-projects 0.2 wrote no marked blocks: it changed herdr's own entries. Its key (the
//!     `herdr-projects.open-popup` action), its line in the tab bar (`… needs-you --line`) and its sidebar rows (the
//!     `$hp_…` tokens and the two "cards") are taken out, and herdr's built-in rows come back where it had replaced them;
//!   - herdr-superwork's marked blocks (`# >>> herdr-superwork key`, `… sidebar`) become this version's, with the same
//!     key, and a binding of a `herdr-superwork.<action>` names `herdr-projects.<action>`, the same action now.
use super::toml_text::{parse, sections, splice, write};
use crate::extras::block_text;
use toml::Value;

fn tokens(row: &Value) -> Vec<String> {
    row.as_array().map(|a| a.iter().map(|x| match x { Value::String(s) => s.clone(), Value::Table(t) => t.get("token").and_then(|v| v.as_str()).unwrap_or("").to_string(), _ => String::new() }).collect()).unwrap_or_default()
}
fn old_token(t: &str) -> bool { t.starts_with("$hp") }
/// The old version's "card": one row in place of herdr's two, a token with a rule on its own marker character.
fn card(row: &Value, token: &str, mark: char) -> bool {
    row.as_array().map(|a| a.iter().any(|x| x.get("token").and_then(|v| v.as_str()) == Some(token) && x.get("rules").map(|r| r.to_string().contains(mark)).unwrap_or(false))).unwrap_or(false)
}
fn built_in(spaces: bool) -> Vec<Value> {
    let rows: &[&[&str]] = if spaces { &[&["state_icon", "workspace"], &["branch", "git_status"]] } else { &[&["state_icon", "machine", "workspace", "tab"], &["agent"]] };
    rows.iter().map(|r| Value::Array(r.iter().map(|s| Value::String(s.to_string())).collect())).collect()
}
/// One list of rows without the old version's. None: nothing of it there. Some(None): the entry can go (what is left
/// is herdr's own default, or nothing).
fn rows_without(rows: &Value, spaces: Option<bool>) -> Option<Option<Value>> {
    let a = rows.as_array()?;
    let mut out = vec![];
    let mut changed = false;
    for row in a {
        let ts = tokens(row);
        if !ts.is_empty() && ts.iter().all(|t| old_token(t)) { changed = true; continue; }
        if let Some(sp) = spaces { if card(row, if sp { "workspace" } else { "agent" }, if sp { '\u{2800}' } else { '\u{200B}' }) { out.extend(built_in(sp)); changed = true; continue; } }
        out.push(row.clone());
    }
    if !changed { return None; }
    let gone = out.is_empty() || spaces.map(|sp| out == built_in(sp)).unwrap_or(false);
    Some(if gone { None } else { Some(Value::Array(out)) })
}
/// The old herdr-projects 0.2's entries taken out. Returns the new text and what was taken out.
pub fn undo_legacy(text: &str) -> (String, Vec<&'static str>) {
    let mut edits = vec![];
    let mut said = vec![];
    for s in sections(text) {
        let entry = |k: &str| s.entries.iter().find(|e| e.key == k);
        if s.array && s.name == "keys.command" {
            if entry("command").and_then(|e| parse(&e.value)).and_then(|v| v.as_str().map(String::from)).as_deref() == Some("herdr-projects.open-popup") { edits.push((s.start, s.end, String::new())); said.push("key"); }
            continue;
        }
        let mut left = s.entries.len();
        let mut mine = vec![];
        for e in &s.entries {
            let next = match (s.name.as_str(), e.key.as_str()) {
                ("ui", "tab_bar_right") => parse(&e.value).and_then(|v| {
                    let a = v.as_array()?.clone();
                    let keep: Vec<Value> = a.iter().filter(|x| !x.get("command").and_then(|c| c.as_str()).map(|c| c.contains("herdr-projects") && c.contains("needs-you --line")).unwrap_or(false)).cloned().collect();
                    (keep.len() != a.len()).then(|| { said.push("tab bar"); (!keep.is_empty()).then_some(Value::Array(keep)) })
                }),
                ("ui.sidebar.agents", "rows") => parse(&e.value).and_then(|v| rows_without(&v, Some(false))),
                ("ui.sidebar.spaces", "rows") => parse(&e.value).and_then(|v| rows_without(&v, Some(true))),
                ("ui.sidebar.agents.rows_by_agent", _) => parse(&e.value).and_then(|v| rows_without(&v, None)),
                _ => None,
            };
            let Some(next) = next else { continue };
            if s.name.starts_with("ui.sidebar") && !said.contains(&"sidebar") { said.push("sidebar"); }
            match next { Some(v) => mine.push((e.start, e.end, format!("{} = {}\n", e.key, write(&v)))), None => { left -= 1; mine.push((e.start, e.end, String::new())); } }
        }
        // A sidebar table it emptied goes too: herdr then shows its own rows.
        if left == 0 && !mine.is_empty() && s.name.starts_with("ui.sidebar") { edits.push((s.start, s.end, String::new())); } else { edits.extend(mine); }
    }
    if edits.is_empty() { return (text.to_string(), vec![]); }
    (tidy(&splice(text, edits)), said)
}
/// herdr-superwork's blocks and actions, made this version's. Returns the new text and whether anything changed.
pub fn take_superwork(text: &str) -> (String, bool) {
    let blocks = regex::Regex::new(r"(?ms)^# >>> herdr-superwork (\S+)[^\n]*\n(.*?)^# <<< herdr-superwork \S+[^\n]*(\n|\z)").unwrap();
    let mut next = blocks.replace_all(text, |c: &regex::Captures| {
        let what = &c[1];
        if text.contains(&format!("# >>> herdr-projects {what}")) || !["key", "sidebar"].contains(&what) { return String::new(); } // one of each; the pop-ups are no more
        let key = crate::re!(r#"(?m)^\s*key\s*=\s*"([^"]+)""#).captures(&c[2]).map(|k| k[1].to_string()).unwrap_or_default();
        if what == "key" && key.is_empty() { return String::new(); }
        format!("{}\n", block_text(what, &key))
    }).to_string();
    next = crate::re!(r#"(?m)^(\s*command\s*=\s*["'])herdr-superwork\."#).replace_all(&next, "${1}herdr-projects.").to_string();
    let next = if next == text { next } else { tidy(&next) };
    let changed = next != text;
    (next, changed)
}
fn tidy(text: &str) -> String { crate::re!(r"\n{3,}").replace_all(text, "\n\n").to_string() }
