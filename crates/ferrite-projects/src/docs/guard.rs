//! The repo's own rules, checked: no source file past its size, every source file says what it holds, a key's chord
//! written only in the keys' table, every test file in the one test program, and no made table out of date. Part of
//! the check (tests/guard.rs runs it); by hand: `herdr-projects docs --guard`.

/// A source file past this many lines is split by area (as crates/flow/src/finish/ was) rather than appended to.
pub const MAX_LINES: usize = 300;
const KEYS_FILE: &str = "crates/flow/src/board/keys.rs";
/// The text box's own editing keys (the chords every text box has) are its own table.
const BOX_KEYS: &str = "crates/board/src/board/textbox/keys.rs";
/// The one test program: every file in tests/ is a module of it.
const TESTS_MAIN: &str = "tests/main.rs";
/// A file made by a script (herdr's palette) is as long as what it is made from.
fn made(text: &str) -> bool { text.lines().take(3).any(|l| l.contains("Do not edit: run the script")) }

pub fn problems() -> Vec<String> {
    let mut out = vec![];
    let files = match super::map::sources() { Ok(f) => f, Err(e) => return vec![e] };
    for f in &files {
        let text = super::read(f);
        let n = text.trim_end_matches('\n').lines().count();
        if n > MAX_LINES && !made(&text) && !f.ends_with(".css") { out.push(format!("{f} has {n} lines, more than {MAX_LINES}: split it by area into a folder of small files (AGENTS.md, \"Where new things go\")")); }
        if let Err(e) = super::map::holds(f) { out.push(e); }
    }
    // A key's chord is written in crates/flow/src/board/keys.rs and nowhere else, so that changing a key is one edit.
    let chord = crate::re!(r#""(ctrl|alt|shift)\+[a-z]"#);
    for f in files.iter().filter(|f| (f.starts_with("src/") || f.starts_with("crates/")) && f.ends_with(".rs") && *f != KEYS_FILE && *f != BOX_KEYS) {
        let mut in_tests = false;
        for (n, l) in super::read(f).lines().enumerate() {
            if l.contains("#[cfg(test)]") { in_tests = true; }
            let code = l.split("//").next().unwrap_or("");
            if !in_tests && chord.is_match(code) { out.push(format!("{f}:{} names a key by its chord: use keys::is(p, K::…) and key_name(\"…\") from {KEYS_FILE}, where every key is defined once", n + 1)); }
        }
    }
    out.extend(tests_left_out(&files, &super::read(TESTS_MAIN)));
    let old = super::tables::stale();
    if !old.is_empty() { out.push(format!("{} {} out of date: run `npm run check` (or `herdr-projects docs`) and commit what it writes (never edit a made table by hand)", old.join(", "), if old.len() > 1 { "are" } else { "is" })); }
    out
}

/// The test files the one test program does not name as a module: cargo never builds or runs them.
pub fn tests_left_out(files: &[String], main: &str) -> Vec<String> {
    files.iter().filter(|f| f.as_str() != TESTS_MAIN).filter_map(|f| f.strip_prefix("tests/")?.strip_suffix(".rs").filter(|m| !m.contains('/')).map(|m| (f, m)))
        .filter(|(_, m)| !main.lines().any(|l| l.trim() == format!("mod {m};")))
        .map(|(f, m)| format!("{f} is not in the one test program, so it is never built or run: add `mod {m};` to {TESTS_MAIN}, in its place in the list"))
        .collect()
}
