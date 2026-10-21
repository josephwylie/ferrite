//! herdr's config.toml read as text, to change one entry and leave every other line as it was (comments, order,
//! spacing): its tables, and in each the entries with where they start and end. Values are read with the toml crate.
use toml::Value;

#[derive(Clone, Debug)]
pub struct Entry { pub key: String, pub start: usize, pub end: usize, pub value: String }
#[derive(Clone, Debug)]
pub struct Section { pub name: String, pub array: bool, pub start: usize, pub end: usize, pub entries: Vec<Entry> }

/// From `at` (just after `=`) to the end of the line the value ends on (its newline included).
fn value_end(t: &[u8], mut i: usize) -> usize {
    let mut depth = 0i32;
    while i < t.len() {
        let rest = &t[i..];
        if rest.starts_with(b"\"\"\"") || rest.starts_with(b"'''") {
            let q = &rest[..3];
            i += 3;
            while i < t.len() && !t[i..].starts_with(q) { if q[0] == b'"' && t[i] == b'\\' { i += 1; } i += 1; }
            i = (i + 3).min(t.len());
            continue;
        }
        match t[i] {
            b'"' | b'\'' => { let q = t[i]; i += 1; while i < t.len() && t[i] != q && t[i] != b'\n' { if q == b'"' && t[i] == b'\\' { i += 1; } i += 1; } i += 1; continue; }
            b'#' => { while i < t.len() && t[i] != b'\n' { i += 1; } continue; }
            b'[' | b'{' => depth += 1,
            b']' | b'}' => depth -= 1,
            b'\n' if depth <= 0 => return i + 1,
            _ => {}
        }
        i += 1;
    }
    t.len()
}
/// Every table, the part before the first one included (name ""), each with its entries.
pub fn sections(text: &str) -> Vec<Section> {
    let t = text.as_bytes();
    let mut out = vec![Section { name: String::new(), array: false, start: 0, end: t.len(), entries: vec![] }];
    let mut i = 0;
    while i < t.len() {
        let line = i;
        while i < t.len() && (t[i] == b' ' || t[i] == b'\t') { i += 1; }
        if i >= t.len() { break; }
        match t[i] {
            b'\n' | b'\r' => { i += 1; }
            b'#' => { while i < t.len() && t[i] != b'\n' { i += 1; } i += 1; }
            b'[' => {
                let array = t[i..].starts_with(b"[[");
                let close = text[i..].find(']').map(|c| i + c).unwrap_or(t.len());
                let name = text[i..close].trim_start_matches('[').trim().to_string();
                if let Some(last) = out.last_mut() { last.end = line; }
                while i < t.len() && t[i] != b'\n' { i += 1; }
                i += 1;
                out.push(Section { name, array, start: line, end: t.len(), entries: vec![] });
            }
            _ => {
                let Some(eq) = text[i..].find('=').map(|e| i + e) else { break };
                let key = text[i..eq].trim().trim_matches('"').to_string();
                let end = value_end(t, eq + 1).min(t.len());
                let value = text[eq + 1..end].trim().to_string();
                if let Some(last) = out.last_mut() { last.entries.push(Entry { key, start: line, end, value }); }
                i = end;
            }
        }
    }
    out
}
/// A value as written (`["a", { token = "b" }]`), read.
pub fn parse(value: &str) -> Option<Value> { toml::from_str::<toml::Table>(&format!("v = {value}")).ok()?.remove("v") }
/// A value written back on one line, as herdr's own examples write it.
pub fn write(v: &Value) -> String {
    match v {
        Value::Array(a) => format!("[{}]", a.iter().map(write).collect::<Vec<_>>().join(", ")),
        Value::Table(t) => if t.is_empty() { "{}".into() } else { format!("{{ {} }}", t.iter().map(|(k, v)| format!("{} = {}", key(k), write(v))).collect::<Vec<_>>().join(", ")) },
        Value::String(s) => serde_json::to_string(s).unwrap_or_default(),
        other => other.to_string(),
    }
}
fn key(k: &str) -> String { if !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') { k.into() } else { serde_json::to_string(k).unwrap_or_default() } }
/// Text with some ranges replaced (each `(start, end, new)`, none overlapping).
pub fn splice(text: &str, mut edits: Vec<(usize, usize, String)>) -> String {
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut out = text.to_string();
    for (s, e, new) in edits { out.replace_range(s..e, &new); }
    out
}
