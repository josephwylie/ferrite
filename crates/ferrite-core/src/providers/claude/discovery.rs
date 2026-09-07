//! Native Claude session discovery without starting or resuming a Session.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{import::Candidate, store::Provider};

const MAX_SCAN_BYTES: u64 = 256 * 1024;

/// List the newest main Claude session files below `root`.
pub fn list(root: &Path, cap: usize) -> io::Result<Vec<Candidate>> {
    if cap == 0 || !root.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    walk(root, &mut paths)?;
    let mut rows = paths
        .into_iter()
        .filter_map(|path| candidate(&path))
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| std::cmp::Reverse(row.modified));
    let mut seen = HashSet::new();
    rows.retain(|row| {
        row.session_id
            .as_ref()
            .is_none_or(|id| seen.insert(id.clone()))
    });
    rows.truncate(cap);
    Ok(rows)
}

fn walk(dir: &Path, paths: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if entry.file_name() != "subagents" {
                walk(&path, paths)?;
            }
        } else if file_type.is_file()
            && path.extension().is_some_and(|ext| ext == "jsonl")
            && !is_subagent(&path)
        {
            paths.push(path);
        }
    }
    Ok(())
}

fn is_subagent(path: &Path) -> bool {
    path.file_stem()
        .is_some_and(|stem| stem.to_string_lossy().starts_with("agent-"))
}

fn candidate(path: &Path) -> Option<Candidate> {
    let modified = fs::metadata(path).and_then(|meta| meta.modified()).ok();
    let text = scan(path).ok()?;
    let mut session_id = None;
    let mut cwd = None;
    let mut custom_title = None;
    let mut summary = None;
    let mut first_prompt = None;
    let mut sidechain = false;
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value["isSidechain"].as_bool() == Some(true) {
            sidechain = true;
        }
        if session_id.is_none() {
            session_id = value["sessionId"]
                .as_str()
                .filter(|value| nonempty(value))
                .map(str::to_owned);
        }
        if cwd.is_none() {
            cwd = value["cwd"]
                .as_str()
                .filter(|value| nonempty(value))
                .map(PathBuf::from);
        }
        if value["type"] == "custom-title" {
            custom_title = value["customTitle"]
                .as_str()
                .filter(|value| nonempty(value))
                .map(str::to_owned);
        }
        if summary.is_none() {
            summary = value["summary"]
                .as_str()
                .filter(|value| nonempty(value))
                .map(str::to_owned);
        }
        if first_prompt.is_none()
            && value["type"] == "user"
            && value["isMeta"].as_bool() != Some(true)
        {
            first_prompt = user_text(&value);
        }
    }
    if sidechain || session_id.is_none() {
        return None;
    }
    Some(Candidate {
        provider: Provider::Claude,
        path: path.to_path_buf(),
        modified,
        title: custom_title.or(summary).or(first_prompt),
        cwd,
        session_id,
    })
}

fn scan(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    if len <= MAX_SCAN_BYTES {
        let mut bytes = Vec::with_capacity(len as usize);
        file.take(MAX_SCAN_BYTES).read_to_end(&mut bytes)?;
        return Ok(String::from_utf8_lossy(&bytes).into_owned());
    }
    let half = MAX_SCAN_BYTES / 2;
    let mut head = vec![0; half as usize];
    file.read_exact(&mut head)?;
    file.seek(SeekFrom::Start(len - half))?;
    let mut tail = vec![0; half as usize];
    file.read_exact(&mut tail)?;
    // Discard partial boundary lines rather than joining unrelated JSON.
    if let Some(end) = head.iter().rposition(|byte| *byte == b'\n') {
        head.truncate(end + 1);
    }
    if let Some(start) = tail.iter().position(|byte| *byte == b'\n') {
        head.extend_from_slice(&tail[start + 1..]);
    }
    Ok(String::from_utf8_lossy(&head).into_owned())
}

fn user_text(value: &Value) -> Option<String> {
    let content = value["message"]["content"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| {
            value["message"]["content"].as_array().map(|blocks| {
                blocks
                    .iter()
                    .filter(|block| block["type"] == "text")
                    .filter_map(|block| block["text"].as_str())
                    .collect::<String>()
            })
        })?;
    let text = content.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn nonempty(value: &str) -> bool {
    !value.trim().is_empty()
}
