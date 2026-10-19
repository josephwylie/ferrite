//! The map of where things live: which file holds what, each line the first sentence of the comment at the top of that
//! file, folder by folder, and the list of pages in docs/. Written in your own folder only (never in git), printed by
//! `herdr-projects docs --where [folder]`.
use std::process::Command;

pub const WHERE: &str = "docs/where-things-live.md";
fn tracked(dirs: &[&str]) -> Result<Vec<String>, String> {
    let o = Command::new("git").arg("-C").arg(super::root()).args(["ls-files", "--cached", "--others", "--exclude-standard", "--"]).args(dirs).output().map_err(|e| format!("git ls-files failed: {e}"))?;
    if !o.status.success() { return Err(format!("git ls-files failed: {}", String::from_utf8_lossy(&o.stderr))); }
    let mut v: Vec<String> = String::from_utf8_lossy(&o.stdout).lines().filter(|f| !f.is_empty() && super::exists(f)).map(String::from).collect();
    v.sort();
    Ok(v)
}
/// The files the map is made of: the program's code, its page, its scripts, its release.
pub fn sources() -> Result<Vec<String>, String> {
    Ok(tracked(&["src", "crates", "bin", "scripts", "tests", ".github"])?.into_iter().filter(|f| [".rs", ".sh", ".js", ".css", ".yml"].iter().any(|e| f.ends_with(e))).collect())
}
/// What a file holds: the first sentence of the comment at its top.
pub fn holds(file: &str) -> Result<String, String> {
    let text = super::read(file);
    let mut lines: Vec<&str> = text.lines().collect();
    if lines.first().map(|l| l.starts_with("#!")).unwrap_or(false) { lines.remove(0); }
    let mut words = vec![];
    if file.ends_with(".css") {
        for l in lines { let t = l.trim().trim_start_matches("/*").trim(); let end = t.contains("*/"); let t = t.split("*/").next().unwrap_or("").trim(); if !t.is_empty() { words.push(t.to_string()); } if end || t.is_empty() { break; } }
    } else {
        let mark = if file.ends_with(".rs") { "//!" } else if file.ends_with(".js") { "//" } else { "#" };
        for l in lines { let Some(w) = l.strip_prefix(mark) else { break }; let w = w.trim(); if w.is_empty() { break; } words.push(w.to_string()); }
    }
    if words.is_empty() { return Err(format!("{file} does not start with a comment that says what it holds (the map, `herdr-projects docs --where`, is made from it)")); }
    let text = words.join(" ");
    let end = crate::re!(r"\.(\s|$)").find(&text).map(|m| m.start()).unwrap_or(text.len());
    let first = crate::re!(r"\s+").replace_all(&text[..end], " ").trim_end_matches(':').to_string();
    Ok(if first.chars().count() > 220 { let cut: String = first.chars().take(219).collect(); format!("{}…", crate::re!(r"\s+\S*$").replace(&cut, "")) } else { first })
}
fn table(head: &[&str], rows: Vec<Vec<String>>) -> String {
    let mut out = vec![format!("| {} |", head.join(" | ")), format!("|{}|", head.iter().map(|_| "---").collect::<Vec<_>>().join("|"))];
    out.extend(rows.into_iter().map(|r| format!("| {} |", r.iter().map(|c| c.replace('|', "\\|")).collect::<Vec<_>>().join(" | "))));
    out.join("\n")
}
fn dir_of(f: &str) -> String { std::path::Path::new(f).parent().map(|p| p.display().to_string()).unwrap_or_default() }
fn base(f: &str) -> String { std::path::Path::new(f).file_name().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default() }
/// The map. `under`: only the folders at or below this one ("crates/board/src/board"), and then without the list of
/// pages.
pub fn where_page(under: &str) -> Result<String, String> {
    let at = under.trim_start_matches("./").trim_end_matches('/').to_string();
    let files = sources()?;
    let mut dirs: Vec<String> = files.iter().map(|f| dir_of(f)).collect();
    dirs.sort_by_key(|d| (!d.starts_with("src") && !d.starts_with("crates"), d.clone()));
    dirs.dedup();
    dirs.retain(|d| at.is_empty() || *d == at || d.starts_with(&format!("{at}/")));
    let pages: Vec<String> = if at.is_empty() { tracked(&["docs"])?.into_iter().filter(|f| crate::re!(r"^docs/[^/]+\.md$").is_match(f) && f != WHERE).collect() } else { vec![] };
    let title = |f: &str| super::read(f).lines().find_map(|l| l.strip_prefix("# ").map(String::from)).unwrap_or_default();
    let mut out = vec!["# Where things live".to_string(), String::new(),
        "<!-- Made by `herdr-projects docs` from the comment at the top of each file. Not in git: it is written in your".into(),
        "     own folder. Do not edit it: change the file's own comment, then run the command again. -->".into(), String::new(),
        "Which file holds what. Every line is the first sentence of the comment at the top of that file. Printed by".into(),
        "`herdr-projects docs --where [folder]`. Where a new thing goes is in [AGENTS.md](../AGENTS.md); the tests are".into(),
        "beside the code they test (`#[cfg(test)]`) and in `tests/`, one file per area.".into(), String::new()];
    for d in &dirs {
        let rows: Result<Vec<Vec<String>>, String> = files.iter().filter(|f| dir_of(f) == *d).map(|f| Ok(vec![format!("`{}`", base(f)), holds(f)?])).collect();
        out.push(format!("## `{d}/`")); out.push(String::new()); out.push(table(&["File", "Holds"], rows?)); out.push(String::new());
    }
    if !pages.is_empty() { out.push("## The pages: `docs/`".into()); out.push(String::new()); out.push(table(&["Page", "About"], pages.iter().map(|f| vec![format!("[{}]({})", base(f), base(f)), title(f)]).collect())); out.push(String::new()); }
    if dirs.is_empty() { out.push(format!("No folder of source files is at or under `{at}`.")); out.push(String::new()); }
    Ok(out.join("\n"))
}
