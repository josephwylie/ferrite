//! The keys herdr already uses, its own defaults and what the user's config.toml binds, and so which key the board may
//! take: it never takes one that is in use, and says what that key does today.
use std::collections::BTreeMap;

/// What the first run offers, in order: the first one nothing uses. None of them is a herdr default (0.9.3).
pub const OFFERED: [&str; 5] = ["prefix+a", "prefix+y", "prefix+u", "prefix+i", "prefix+m"];
/// The commented [[keys.command]] example's fields are no actions.
const NOT_ACTIONS: [&str; 6] = ["key", "type", "command", "description", "width", "height"];
pub type Defaults = BTreeMap<String, String>;

/// {action: key} as this herdr binds them: the commented `# action = "key"` lines under [keys] of its default config.
pub fn parse_defaults(out: &str) -> Option<Defaults> {
    let mut lines = out.lines().skip_while(|l| !l.starts_with("[keys]"));
    lines.next()?;
    let part: Vec<&str> = lines.take_while(|l| !(l.starts_with('[') && !l.starts_with("[["))).collect();
    let re = regex::Regex::new(r#"^#\s*([a-z_]+)\s*=\s*"([^"]*)""#).unwrap();
    let mut found = Defaults::new();
    for l in part { if let Some(m) = re.captures(l) { if !m[2].is_empty() && !NOT_ACTIONS.contains(&&m[1]) { found.insert(m[1].into(), m[2].into()); } } }
    (!found.is_empty()).then_some(found)
}
/// Every key herdr 0.9.3 binds itself, kept beside this file. Always counted: herdr's printed defaults leave some out
/// (copy mode, swapping panes), and herdr may not answer at all. A key is never taken blindly.
pub fn built_in() -> Defaults { parse_defaults(include_str!("default-keys.toml")).unwrap_or_default() }
pub fn defaults() -> Defaults {
    let mut d = built_in();
    let out = crate::herdr::cli::run_raw(None, &["--default-config".into()], 8000, "").ok();
    if let Some(more) = out.and_then(|o| parse_defaults(&o)) { d.extend(more); }
    d
}
fn named(p: &str) -> &str { match p { "-" => "minus", "," => "comma", "`" => "backtick", "&" => "ampersand", "return" => "enter", "escape" => "esc", "super" => "cmd", x => x } }
const MODS: [&str; 4] = ["ctrl", "alt", "shift", "cmd"];
/// One spelling per key, so "Prefix+Shift+Ctrl+X" and "prefix+ctrl+shift+x" are seen as the same key.
pub fn norm(key: &str) -> String {
    let low = key.trim().to_lowercase();
    let parts: Vec<String> = low.split('+').map(|p| p.trim()).filter(|p| !p.is_empty()).map(|p| named(p).to_string()).collect();
    let prefix = parts.iter().any(|p| p == "prefix");
    let rest: Vec<&String> = parts.iter().filter(|p| *p != "prefix").collect();
    let mut out: Vec<String> = vec![];
    if prefix { out.push("prefix".into()); }
    for m in MODS { if rest.iter().any(|p| *p == m) { out.push(m.into()); } }
    for p in rest { if !MODS.contains(&p.as_str()) { out.push(p.clone()); } }
    out.join("+")
}
/// "prefix+1..9" is nine keys.
fn each(key: &str) -> Vec<String> {
    match key.strip_suffix("1..9") { Some(head) => (1..=9).map(|i| norm(&format!("{head}{i}"))).collect(), None => vec![norm(key)] }
}
fn words(action: &str) -> String { action.replace('_', " ") }
fn list(v: Option<&toml::Value>) -> Vec<String> {
    match v { Some(toml::Value::String(s)) if !s.is_empty() => vec![s.clone()], Some(toml::Value::Array(a)) => a.iter().filter_map(|x| x.as_str()).filter(|s| !s.is_empty()).map(String::from).collect(), _ => vec![] }
}
#[derive(Clone, Debug)]
pub struct Use { pub by: &'static str, pub what: String }
/// Every key in use, with what it does: key -> {by: "herdr" | "config", what}. herdr's defaults count as the user's
/// config leaves them (an action the user moved or switched off frees its default key), then everything the config
/// binds itself, however it is written. The board's own binding is left out.
pub fn used(text: &str, defs: &Defaults) -> BTreeMap<String, Use> {
    let mut map: BTreeMap<String, Use> = BTreeMap::new();
    let mut put = |key: &str, by: &'static str, what: String| { for k in each(key) { if !k.is_empty() && !map.contains_key(&k) { map.insert(k, Use { by, what: what.clone() }); } } };
    let conf: Option<toml::Table> = text.parse::<toml::Table>().ok();
    let empty = toml::Table::new();
    let keys = conf.as_ref().and_then(|c| c.get("keys")).and_then(|k| k.as_table()).unwrap_or(&empty);
    let mut moved = std::collections::HashSet::new();
    for (action, value) in keys {
        if action == "command" || action == "indexed" { continue; }
        moved.insert(if action == "fullscreen" { "zoom".to_string() } else { action.clone() }); // an older name herdr still reads
        for k in list(Some(value)) { put(&k, "config", words(action)); }
    }
    if let Some(toml::Value::Array(cmds)) = keys.get("command") {
        for c in cmds.iter().filter_map(|c| c.as_table()) {
            if c.get("command").and_then(|v| v.as_str()) == Some("herdr-projects.open") { continue; }
            let what = c.get("description").or_else(|| c.get("command")).and_then(|v| v.as_str()).unwrap_or("a command").to_string();
            for k in list(c.get("key")) { put(&k, "config", what.clone()); }
        }
    }
    // [keys.indexed]: tabs = "ctrl" makes ctrl+1 to ctrl+9 switch tabs.
    if let Some(toml::Value::Table(ix)) = keys.get("indexed") { for (what, mods) in ix { if let Some(m) = mods.as_str().filter(|m| !m.is_empty()) { put(&format!("{m}+1..9"), "config", format!("switch {what}")); } } }
    // A config the reader cannot follow still has its keys found, wherever they stand: better one refusal too many.
    if conf.is_none() {
        let chord = regex::Regex::new(r#"(?i)["']((?:(?:prefix|ctrl|alt|shift|cmd|super)\+)+[^"'\s]+|f\d+)["']"#).unwrap();
        for m in chord.captures_iter(text) { put(&m[1], "config", "a binding".into()); }
    }
    for (action, key) in defs { if !moved.contains(action) || conf.is_none() { put(key, "herdr", words(action)); } }
    map
}
/// May the board take this key? Ok, or why not, naming what the key does today.
pub fn check(key: &str, text: &str, defs: &Defaults, file: &str) -> Result<(), String> {
    let raw = key.trim().to_lowercase();
    if raw.is_empty() || !raw.chars().all(|c| c.is_ascii_alphanumeric() || "+_.-".contains(c)) || raw.ends_with('+') { return Err(format!("\"{key}\" is not a key herdr understands. Try something like prefix+a.")); }
    match used(text, defs).get(&norm(&raw)) {
        None => Ok(()),
        Some(h) if h.by == "herdr" => Err(format!("herdr itself uses {raw} ({}): the board would switch that off. Choose another.", h.what)),
        Some(h) => Err(format!("{raw} is already bound in {file} ({}): the board would replace it. Choose another.", h.what)),
    }
}
/// The key to offer, and why it is not the first choice when it is not.
pub fn suggest(text: &str, defs: &Defaults) -> (String, String) {
    let map = used(text, defs);
    let key = OFFERED.iter().find(|k| !map.contains_key(&norm(k))).map(|s| s.to_string()).unwrap_or_default();
    let Some(first) = map.get(&norm(OFFERED[0])) else { return (key, String::new()) };
    let why = format!("{} is already {} ({})", OFFERED[0], if first.by == "herdr" { "herdr's own key" } else { "bound in your config" }, first.what);
    let note = if key.is_empty() { format!("{why}, and so are the other keys this would offer.") } else { format!("{why}, so this is the next free key.") };
    (key, note)
}
fn pressed(key: &str) -> String { norm(key).split('+').map(|p| match p { "minus" => "-", "comma" => ",", "backtick" => "`", "ampersand" => "&", "cmd" => "super", x => x }.to_string()).collect::<Vec<_>>().join("+") }
/// What a key of herdr's config is as key presses, for a program herdr hands every key to (its popup): "prefix+a" is
/// one of the prefix keys and then `a`; a key without the prefix is itself. (first: the prefix keys, key); None for no
/// key, and when the prefix is not known.
pub fn presses(key: &str, text: &str, defs: &Defaults) -> Option<(Vec<String>, String)> {
    let k = norm(key);
    if k.is_empty() { return None; }
    let Some(rest) = k.strip_prefix("prefix+") else { return Some((vec![], pressed(&k))) };
    let conf: Option<toml::Table> = text.parse::<toml::Table>().ok();
    let set = list(conf.as_ref().and_then(|c| c.get("keys")).and_then(|k| k.as_table()).and_then(|k| k.get("prefix")));
    let first: Vec<String> = if set.is_empty() { defs.get("prefix").map(|p| vec![p.clone()]).unwrap_or_default() } else { set }.iter().map(|p| pressed(p)).collect();
    (!first.is_empty()).then(|| (first, pressed(rest)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_in_use_are_refused() {
        let d = built_in();
        assert_eq!(d.get("prefix").map(String::as_str), Some("ctrl+b"));
        assert!(check("prefix+w", "", &d, "x").is_err());
        assert!(check("prefix+a", "", &d, "x").is_ok());
        assert!(check("Prefix+A", "[[keys.command]]\nkey = \"prefix+a\"\ncommand = \"x\"\n", &d, "x").is_err());
        assert_eq!(suggest("", &d).0, "prefix+a");
        assert_eq!(presses("prefix+a", "", &d), Some((vec!["ctrl+b".to_string()], "a".to_string())));
    }
}
