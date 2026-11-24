//! What a Codex `fileChange` item says it edits, as Hunks path by path:
//! the diff its tool row and its Decision draw before the edit runs. The
//! transcript reads a call's input with it, and the Codex adapter
//! (`providers::codex`) its events.

use serde_json::Value;

use crate::Hunk;

/// Decode Codex's per-file unified diff without assigning a tool identity or
/// inferring files absent from the native `changes` list.
/// A `fileChange` item's `changes` as the diffs they make, path by path —
/// what the item says before it runs (and before its approval is
/// answered), so its row and its Decision can draw the edit asked for.
pub(crate) fn file_change_diffs(item: &Value) -> Vec<(String, Vec<Hunk>)> {
    item.get("changes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|change| {
            let path = change.get("path")?.as_str()?.to_string();
            let hunks = parse_file_change(change);
            (!hunks.is_empty()).then_some((path, hunks))
        })
        .collect()
}

pub(crate) fn parse_file_change(change: &Value) -> Vec<Hunk> {
    let diff = change
        .get("diff")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match change
        .get("kind")
        .and_then(|kind| kind.get("type"))
        .and_then(Value::as_str)
    {
        Some("add") => raw_file_hunk(diff, '+'),
        Some("delete") => raw_file_hunk(diff, '-'),
        Some("update") => parse_unified_diff(diff),
        _ => Vec::new(),
    }
}

fn raw_file_hunk(content: &str, marker: char) -> Vec<Hunk> {
    let lines: Vec<_> = content
        .lines()
        .map(|line| format!("{marker}{line}"))
        .collect();
    if lines.is_empty() {
        return Vec::new();
    }
    let count = lines.len() as u32;
    vec![Hunk {
        old_start: if marker == '-' { 1 } else { 0 },
        old_lines: if marker == '-' { count } else { 0 },
        new_start: if marker == '+' { 1 } else { 0 },
        new_lines: if marker == '+' { count } else { 0 },
        lines,
        section: None,
    }]
}

fn parse_unified_diff(diff: &str) -> Vec<Hunk> {
    let mut hunks = Vec::new();
    let mut current: Option<Hunk> = None;
    for line in diff.lines() {
        if let Some((old_start, old_lines, new_start, new_lines, section)) = parse_hunk_header(line)
        {
            if let Some(hunk) = current.take() {
                hunks.push(hunk);
            }
            current = Some(Hunk {
                old_start,
                old_lines,
                new_start,
                new_lines,
                lines: Vec::new(),
                section,
            });
        } else if current.is_some() && matches!(line.as_bytes().first(), Some(b' ' | b'+' | b'-')) {
            current.as_mut().unwrap().lines.push(line.to_string());
        }
    }
    if let Some(hunk) = current {
        hunks.push(hunk);
    }
    hunks
}

/// `@@ -208,7 +208,11 @@ fn thread_row` → the ranges and the section git's
/// funcname rule put after the second `@@` (none when it found none).
fn parse_hunk_header(line: &str) -> Option<(u32, u32, u32, u32, Option<String>)> {
    let middle = line.strip_prefix("@@ -")?.split_once(" +")?;
    let (old, rest) = middle;
    let (new, tail) = rest.split_once(" @@")?;
    fn range(range: &str) -> Option<(u32, u32)> {
        let (start, count) = range.split_once(',').unwrap_or((range, "1"));
        Some((start.parse().ok()?, count.parse().ok()?))
    }
    let (old_start, old_lines) = range(old)?;
    let (new_start, new_lines) = range(new)?;
    let section = crate::transcript::section_head(tail.trim());
    Some((old_start, old_lines, new_start, new_lines, section))
}
