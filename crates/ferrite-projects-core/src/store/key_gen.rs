//! The short key the board makes for a repo or folder from its name, when nobody gave it one: `TPW` for
//! team-paperwork, `SKL` for gtm-skills, `CRM` for open-crm, `HOME` for the home folder, `ROOT` for `/`. The same name
//! always makes the same key; a key another repo has is passed over for the next way of writing it, then for the
//! first one with a number (`PRJ2`, `PRJ3`, …).
use std::path::Path;

fn vowel(c: char) -> bool { "aeiou".contains(c) }
/// One word of 8 letters or more is two, where it parts as a word like "paperwork" does: between two consonants that
/// have a vowel on each side, nearest the middle, each half 3 letters or more (`paper` `work`).
fn halves(w: &str) -> Vec<String> {
    let c: Vec<char> = w.chars().collect();
    if c.len() < 8 || !c.iter().all(|x| x.is_ascii_alphabetic()) { return vec![w.to_string()]; }
    let fits = |i: usize| i >= 3 && i + 3 <= c.len() && vowel(c[i - 2]) && !vowel(c[i - 1]) && !vowel(c[i]) && c.get(i + 1).map(|x| vowel(*x)).unwrap_or(false);
    let mid = c.len() / 2;
    match (3..c.len()).filter(|i| fits(*i)).min_by_key(|i| i.abs_diff(mid)) {
        Some(i) => vec![c[..i].iter().collect(), c[i..].iter().collect()],
        None => vec![w.to_string()],
    }
}
/// The words of a name, small letters: parted at anything but a letter or digit, where a small letter meets a capital,
/// and where letters meet digits. Numbers are kept apart: they go at the end of the key.
pub fn words(name: &str) -> (Vec<String>, String) {
    let mut parts: Vec<String> = vec![];
    let mut cur = String::new();
    let mut last: Option<char> = None;
    for ch in name.chars() {
        let cut = match last { None => false, Some(p) => !ch.is_ascii_alphanumeric() || !p.is_ascii_alphanumeric() || (p.is_ascii_lowercase() && ch.is_ascii_uppercase()) || (p.is_ascii_digit() != ch.is_ascii_digit()) };
        if cut && !cur.is_empty() { parts.push(std::mem::take(&mut cur)); }
        if ch.is_ascii_alphanumeric() { cur.push(ch.to_ascii_lowercase()); }
        last = Some(ch);
    }
    if !cur.is_empty() { parts.push(cur); }
    let digits: String = parts.iter().filter(|p| p.bytes().all(|b| b.is_ascii_digit())).cloned().collect();
    let words = parts.into_iter().filter(|p| !p.bytes().all(|b| b.is_ascii_digit())).flat_map(|w| halves(&w)).collect();
    (words, digits)
}
/// A word's first letter and the consonants after it, a letter said twice in a row once: `skills` -> `skls`.
fn bones(w: &str) -> String {
    let mut out = String::new();
    for (i, c) in w.chars().enumerate() { if (i == 0 || !vowel(c)) && !out.ends_with(c) { out.push(c); } }
    out
}
fn first(s: &str, n: usize) -> String { s.chars().take(n).collect() }
/// One word as a key: a short word as it is (`crm`), a longer one by its bones (`skills` -> `skl`), or its first four
/// letters where its bones are too few (`repo`).
fn of_word(w: &str) -> String {
    let b = bones(w);
    if w.len() <= 3 { w.to_string() } else if b.len() >= 3 { first(&b, 3) } else { first(w, 4) }
}
pub fn valid(k: &str) -> bool {
    (2..=5).contains(&k.len()) && k.starts_with(|c: char| c.is_ascii_uppercase()) && k.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) && k != "PR"
}
/// Every key the name could have, the best first, without a number to tell it apart.
pub fn choices(name: &str, folder: &Path, home: &Path) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    if folder == home { out.push("HOME".into()); }
    if folder == Path::new("/") { out.push("ROOT".into()); }
    let (w, digits) = words(name);
    let room = 5usize.saturating_sub(digits.len()).max(1);
    let initials: String = w.iter().filter_map(|x| x.chars().next()).collect();
    let all: String = w.concat();
    let mut base: Vec<String> = vec![];
    match w.len() {
        0 => {}
        1 => base.push(of_word(&w[0])),
        // Two words: the last says what it is (gtm-skills, open-crm); the first is often a group's or a maker's.
        2 => { base.push(of_word(&w[1])); base.push(initials.clone()); }
        _ => base.push(initials.clone()),
    }
    base.push(of_word(&all));
    for x in w.iter().rev() { base.push(of_word(x)); }
    base.extend([first(&bones(&all), 4), first(&all, 3), first(&all, 4), first(&all, 5)]);
    for b in base { out.push(format!("{}{digits}", first(&b, room))); }
    if w.is_empty() { out.push(first(&format!("R{digits}"), 5)); out.push("REPO".into()); }
    let mut seen = std::collections::HashSet::new();
    let out: Vec<String> = out.into_iter().map(|k| k.to_uppercase()).filter(|k| valid(k) && seen.insert(k.clone())).collect();
    if out.is_empty() { vec![format!("{}1", all.to_uppercase())] } else { out } // a name of one letter: `h` -> `H1`
}
/// The key for a repo: its best choice nobody else has; else the best choice with the first number that is free.
pub fn make_key(name: &str, folder: &Path, home: &Path, taken: impl Fn(&str) -> bool) -> String {
    let all = choices(name, folder, home);
    if let Some(k) = all.iter().find(|k| !taken(k)) { return k.clone(); }
    let base = all.first().cloned().unwrap_or_else(|| "REPO".into());
    for n in 2u32.. {
        let n = n.to_string();
        let k = format!("{}{n}", first(&base, 5 - n.len().min(4)));
        if valid(&k) && !taken(&k) { return k; }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(name: &str) -> String { make_key(name, Path::new("/x").join(name).as_path(), Path::new("/home/me"), |_| false) }
    #[test]
    fn keys_from_names() {
        assert_eq!(key("team-paperwork"), "TPW");
        assert_eq!(key("gtm-skills"), "SKL");
        assert_eq!(key("open-crm"), "CRM");
        assert_eq!(key("proj"), "PRJ");
        assert_eq!(key("repo"), "REPO");
        assert_eq!(key("tmp"), "TMP");
        assert_eq!(key("proj-2"), "PRJ2");
        assert_eq!(key("myCoolApp"), "MCA");
        assert_eq!(make_key("me", Path::new("/home/me"), Path::new("/home/me"), |_| false), "HOME");
        assert_eq!(make_key("home", Path::new("/"), Path::new("/home/me"), |_| false), "ROOT");
        assert_eq!(key("h"), "H1");
        assert!(valid(&key("h")) && valid(&key("2024")) && valid(&key("---")));
    }
    #[test]
    fn clashes() {
        assert_eq!(make_key("proj", Path::new("/x/proj"), Path::new("/h"), |k| k == "PRJ"), "PRO");
        assert_eq!(make_key("proj", Path::new("/x/proj"), Path::new("/h"), |k| k != "PRJ2"), "PRJ2");
    }
}
