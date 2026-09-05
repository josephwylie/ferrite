//! File references carried by ordinary prompt text. One spelling survives
//! editing, queueing, history and handover; adapters own wire attachments.
//! Quoted references use JSON strings, so spaces, quotes, Unicode and Windows
//! separators round-trip without shell escaping or a new persisted schema.

use std::path::{Path, PathBuf};

fn token(path: &Path) -> String {
    format!(
        "@{}",
        serde_json::to_string(&path.to_string_lossy()).unwrap()
    )
}

/// The attachment trailer stays ordinary, readable prompt text on disk.
pub fn compose(text: &str, files: &[PathBuf]) -> String {
    let mut prompt = text.to_string();
    for path in files {
        if !prompt.is_empty() {
            prompt.push('\n');
        }
        prompt.push_str(&token(path));
    }
    prompt
}

/// Restore queued/history text. Only our exact trailing absolute references
/// become attachments; inline @ mentions stay editable.
pub fn split(prompt: String) -> (String, Vec<PathBuf>) {
    let mut text = prompt;
    let mut files = Vec::new();
    loop {
        let start = text.rfind('\n').map_or(0, |at| at + 1);
        let line = &text[start..];
        let paths = paths(line, None);
        let [path] = paths.as_slice() else {
            break;
        };
        if token(path) != line {
            break;
        }
        files.push(path.clone());
        text.truncate(start.saturating_sub(1));
    }
    files.reverse();
    (text, files)
}

/// Resolve whole @path / @"path with spaces" tokens, once per distinct path.
/// This does no I/O; a provider decides how to read each referenced file.
pub fn paths(text: &str, cwd: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        rest = rest.trim_start();
        let Some(after) = rest.strip_prefix('@') else {
            rest = &rest[rest.find(char::is_whitespace).unwrap_or(rest.len())..];
            continue;
        };
        let (candidate, used) = if after.starts_with('"') {
            let mut stream = serde_json::Deserializer::from_str(after).into_iter::<String>();
            match stream.next() {
                Some(Ok(path)) => (path, stream.byte_offset()),
                _ => {
                    rest = &after[after.find(char::is_whitespace).unwrap_or(after.len())..];
                    continue;
                }
            }
        } else {
            let used = after.find(char::is_whitespace).unwrap_or(after.len());
            (after[..used].to_string(), used)
        };
        rest = &after[used..];
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            rest = &rest[rest.find(char::is_whitespace).unwrap_or(rest.len())..];
            continue;
        }
        if candidate.is_empty() {
            continue;
        }
        let path = PathBuf::from(candidate);
        let path = if path.is_absolute() {
            path
        } else if let Some(cwd) = cwd {
            cwd.join(path)
        } else {
            continue;
        };
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}

/// Formats both Providers accept as native image input. Other file types
/// remain references the agent can inspect with its own tools.
pub(crate) fn image_type(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_round_trip_and_ignore_prose_or_partial_tokens() {
        let root = std::env::temp_dir();
        let path = root.join("screen shots/quote \"é\" \\ final.png");
        let token = token(&path);
        assert_eq!(
            paths(&format!("inspect {token} {token} @notes.pdf"), Some(&root)),
            [path.clone(), root.join("notes.pdf")]
        );
        for prose in ["", "look at this", "keep\nnewlines\n"] {
            let files = vec![path.clone(), root.join("arbitrary.archive")];
            assert_eq!(split(compose(prose, &files)), (prose.into(), files));
        }
        for text in [
            "email a@b.example",
            "@",
            "@\"unterminated",
            "@\"file\"suffix",
        ] {
            assert!(paths(text, Some(&root)).is_empty(), "{text}");
        }
    }
}
