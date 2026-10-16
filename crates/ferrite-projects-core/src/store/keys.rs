//! A repo's short key (`repo.key`: `HSW` for this repo) and what it does to a task's name: its tag reads
//! `HSW-134`, and `HSW-134` names the task wherever its id does. Every repo has one (store/keys_given.rs). Nothing is
//! stored under a key: it is another way to write the id, worked out each time, so changing one breaks nothing, and a
//! key a repo had before still names its tasks: every key that counts is put on record as its repo's for good.
use super::files::*;
use super::repos::known_repos;
use super::settings::{lookup, places, values::NONE};
use crate::json::*;
use std::collections::BTreeMap;
use std::sync::Mutex;

#[derive(Clone, Debug)]
pub struct Said { pub repo: String, pub key: String, pub place: String }

static KEPT: Mutex<Option<(String, Vec<Said>)>> = Mutex::new(None);
static COUNTED: Mutex<Option<(String, Keys)>> = Mutex::new(None);
/// What the keys are worked out from, as it stands on disk: a change to any of it is a fresh look. The files are
/// small and read whole: two edits of the same size within the clock's step look alike by their times.
fn stamp(repos: &Obj) -> String {
    let cfg = places::config_file();
    let own = super::keys_given::record_file();
    let whole = |f: &std::path::Path| read_text(f).unwrap_or_default();
    let mut stamp = vec![data_dir().display().to_string(), cfg.display().to_string(), whole(&cfg), whole(&own)];
    for (name, root) in repos { stamp.push(name.clone()); stamp.push(text(root)); for f in [places::REPO_FILE, places::OLD_REPO_FILE] { stamp.push(whole(&std::path::Path::new(&text(root)).join(f))); } }
    stamp.join("\n")
}
/// Every key that is said, with who says it. Asked on every look at the board, so the answer is kept until one of
/// the files it comes from changes: the list of repos, your settings, a repo's own file, the board's own record of
/// keys (where a stand-in stands, place "standin": store/keys_given.rs).
pub fn said_keys() -> Vec<Said> {
    let repos = known_repos();
    let stamp = stamp(&repos);
    if let Some((s, out)) = KEPT.lock().unwrap().as_ref() { if *s == stamp { return out.clone(); } }
    let mine = places::yours();
    let mut out = vec![];
    for repo in repos.keys() {
        if let Ok(r) = lookup::lookup("repo.key", &lookup::Scope::repo(repo), &mine) {
            let v = text(&r.value);
            if !v.is_empty() && v != NONE { out.push(Said { repo: repo.clone(), key: v, place: r.place.clone() }); }
        }
    }
    for (repo, key) in super::keys_given::stand_ins() { if repos.contains_key(&repo) { out.push(Said { repo, key: text(&key), place: "standin".into() }); } }
    *KEPT.lock().unwrap() = Some((stamp, out.clone()));
    out
}
pub type Keys = BTreeMap<String, String>;
/// The keys that count: repo -> key. A key another repo had is never this one's (an old `HSW-134` keeps its task). A
/// key two repos say is the one of the repo whose entry says it (your entry wins over a file), and among two of the
/// same kind, of the repo the board knew first. The other goes by the stand-in the board gave it, and what it says is
/// kept as it is. What counts is put on record (store/keys_given.rs).
pub fn repo_keys() -> Keys {
    let at = stamp(&known_repos());
    if let Some((s, out)) = COUNTED.lock().unwrap().as_ref() { if *s == at { return out.clone(); } }
    let all = said_keys();
    let had = super::keys_given::had_keys();
    let theirs = |s: &Said| had.get(&s.key).map(|r| text(r) != s.repo).unwrap_or(false);
    let (said, stand): (Vec<&Said>, Vec<&Said>) = all.iter().filter(|s| !theirs(s)).partition(|s| s.place != "standin");
    let mut out = Keys::new();
    for s in &said {
        let same: Vec<&&Said> = said.iter().filter(|x| x.key == s.key).collect();
        let first = same.iter().find(|x| x.place == "repo").or(same.first());
        if first.map(|f| std::ptr::eq(**f, *s)).unwrap_or(false) { out.insert(s.repo.clone(), s.key.clone()); }
    }
    for s in stand { if !out.contains_key(&s.repo) && !out.values().any(|k| *k == s.key) { out.insert(s.repo.clone(), s.key.clone()); } }
    super::keys_given::note_keys(&out);
    *COUNTED.lock().unwrap() = Some((stamp(&known_repos()), out.clone()));
    out
}
fn number_of(id: &str) -> &str { let n = id.rsplit('/').next().unwrap_or(""); if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) { n } else { "" } }
/// `hsw-134` -> `herdr-projects/134`, in any letter case; "" when it is not a key and a number, or no repo has it. With
/// no keys given, every key a repo ever went by counts too, after the ones they go by now.
pub fn id_of_key(reference: &str, keys: Option<&Keys>) -> String {
    let s = reference.trim();
    let Some((k, n)) = s.split_once('-') else { return String::new() };
    let ok_key = k.len() >= 2 && k.len() <= 5 && k.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false) && k.chars().all(|c| c.is_ascii_alphanumeric());
    if !ok_key || n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) { return String::new(); }
    let owned;
    let all = match keys { Some(k) => k, None => { owned = repo_keys(); &owned } };
    let up = k.to_uppercase();
    let repo = all.iter().find(|(_, v)| **v == up).map(|(r, _)| r.clone())
        .or_else(|| if keys.is_none() { super::keys_given::had_keys().get(&up).map(text) } else { None });
    match repo { Some(repo) => format!("{repo}/{}", n.parse::<u64>().unwrap_or(0)), None => String::new() }
}
/// A task's short tag: `HSW-134` with a key, `#134` without. "" for an id with no number.
pub fn tag_of(id: &str, keys: &Keys) -> String {
    let n = number_of(id);
    if n.is_empty() { return String::new(); }
    match keys.get(id.split('/').next().unwrap_or("")) { Some(k) => format!("{k}-{n}"), None => format!("#{n}") }
}
/// A task named in words on the board: `HSW-134` with a key, else its id as it is.
pub fn say_id(id: &str, keys: &Keys) -> String {
    let n = number_of(id);
    match keys.get(id.split('/').next().unwrap_or("")) { Some(k) if !n.is_empty() => format!("{k}-{n}"), _ => id.to_string() }
}
fn word_char(c: char) -> bool { c.is_ascii_alphanumeric() || c == '_' }
/// Every `repo/<number>` in a text that stands alone (no word, slash, dot, plus or dash right before it, no word or
/// slash right after), each replaced by what `f` makes of its number.
pub fn replace_ids(text: &str, repo: &str, f: impl Fn(&str) -> String) -> String {
    let needle = format!("{repo}/");
    let mut out = String::new();
    let mut i = 0;
    while let Some(off) = text[i..].find(&needle) {
        let at = i + off;
        let before_ok = text[..at].chars().last().map(|c| !(word_char(c) || "/.+-".contains(c))).unwrap_or(true);
        let digits: String = text[at + needle.len()..].chars().take_while(|c| c.is_ascii_digit()).collect();
        let end = at + needle.len() + digits.len();
        let after_ok = text[end..].chars().next().map(|c| !(word_char(c) || c == '/')).unwrap_or(true);
        if before_ok && !digits.is_empty() && after_ok {
            out.push_str(&text[i..at]);
            out.push_str(&f(&digits));
            i = end;
        } else {
            out.push_str(&text[i..at + needle.len()]);
            i = at + needle.len();
        }
    }
    out.push_str(&text[i..]);
    out
}
/// Words that name tasks by their ids, as a person reads them: each id of a keyed repo in its short form.
pub fn say_ids(text: &str, keys: &Keys) -> String {
    let mut out = text.to_string();
    for (repo, key) in keys { out = replace_ids(&out, repo, |n| format!("{key}-{n}")); }
    out
}
/// The same for the board's own screen, where a task's name is the tag its row has: every known repo's ids, keyed or
/// not (`proj/14: stopped` -> `#14: stopped`).
pub fn tag_ids(text: &str, repos: &[String], keys: &Keys) -> String {
    let mut out = text.to_string();
    let mut all: Vec<String> = repos.to_vec();
    for k in keys.keys() { if !all.contains(k) { all.push(k.clone()); } }
    for repo in all {
        out = replace_ids(&out, &repo, |n| match keys.get(&repo) { Some(k) => format!("{k}-{n}"), None => format!("#{n}") });
    }
    out
}
/// A task as a commit and a pull request name it: the short name with the id it stands for, or the id alone.
pub fn task_ref(id: &str, keys: &Keys) -> String { let short = say_id(id, keys); if short == id { id.to_string() } else { format!("{short} ({id})") } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_in_words() {
        let mut k = Keys::new();
        k.insert("proj".into(), "HSW".into());
        assert_eq!(say_ids("proj/134: archived, see a/proj/1 and proj/12x", &k), "HSW-134: archived, see a/proj/1 and proj/12x");
        assert_eq!(tag_ids("other/3 stopped", &["other".into()], &k), "#3 stopped");
        assert_eq!(id_of_key("hsw-007", Some(&k)), "proj/7");
        assert_eq!(tag_of("proj/5", &k), "HSW-5");
        assert_eq!(tag_of("x/5", &k), "#5");
    }
}
