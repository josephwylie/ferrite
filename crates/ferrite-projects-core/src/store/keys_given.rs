//! Every repo and folder has a short key: the board gives one (store/key_gen.rs) to a repo the moment it first sees it,
//! to every repo it knew without one when this version first runs (once, and the board says which), and again to a
//! repo whose key was taken away. A key given is saved as your entry for the repo, like one you set; a key you or the
//! repo's file gave is never touched: a repo whose key another repo has too, or had, goes by a stand-in kept here
//! instead. Every key a repo ever went by is kept here too, however it changed (`set`, your file, the repo's file):
//! an old `HSW-134` still names its task, and no other repo may take that key.
use super::files::*;
use super::key_gen::make_key;
use super::keys::{repo_keys, said_keys};
use super::repos::known_repos;
use super::settings::places::write_yours;
use crate::json::*;
use std::path::{Path, PathBuf};

/// `{ "upgraded": true, "had": { "HS": "herdr-projects" }, "standIn": { "beta": "BETA" }, "untold": [["exedev", "HOME"]] }`.
pub fn record_file() -> PathBuf { data_dir().join("repo-keys.json") }
fn file() -> PathBuf { record_file() }
fn read() -> Obj { read_obj(&file()).unwrap_or_default() }
fn write(o: Obj) { let _ = write_json(&file(), &Value::Object(o)); }

/// Every key a repo ever went by: key -> repo, the first repo that had it, for good.
pub fn had_keys() -> Obj { read().o("had").cloned().unwrap_or_default() }
/// The keys repos go by now, put on record: a key is its first repo's for good.
pub fn note_keys(now: &super::keys::Keys) {
    if now.iter().all(|(r, k)| had_keys().get(k).map(text).as_deref() == Some(r.as_str())) { return; }
    let _ = with_lock("repo-keys", || {
        let mut o = read();
        let had = o.obj_mut("had");
        for (r, k) in now { if !had.contains_key(k) { had.insert(k.clone(), json!(r)); } }
        write(o);
        Ok(())
    });
}
/// The keys the board gave repos whose own key another repo has too: repo -> key.
pub fn stand_ins() -> Obj { read().o("standIn").cloned().unwrap_or_default() }
/// Gives a key to every repo that has none that counts (`only`: to that one repo), and returns them: (repo, key).
pub fn give_keys(only: Option<&str>) -> Vec<(String, String)> {
    with_lock("repo-keys", || {
        let have = repo_keys();
        let repos = known_repos();
        let said = said_keys();
        // A stand-in its repo no longer goes by (its own key counts again) is let go.
        let stale: Vec<String> = said.iter().filter(|s| s.place == "standin" && have.get(&s.repo) != Some(&s.key)).map(|s| s.repo.clone()).collect();
        if !stale.is_empty() { let mut o = read(); for r in &stale { o.obj_mut("standIn").shift_remove(r); } write(o); }
        if repos.keys().all(|r| have.contains_key(r) || only.map(|o| o != r).unwrap_or(false)) { return Ok(vec![]); }
        let said = said_keys();
        // Taken: every key a repo says or once had, and whose it is.
        let mut taken: Vec<(String, String)> = said.iter().map(|s| (s.key.clone(), s.repo.clone())).collect();
        for (k, r) in had_keys() { taken.push((k, text(&r))); }
        let home = user_home();
        let mut given = vec![];
        for (repo, root) in &repos {
            if have.contains_key(repo) || only.map(|o| o != repo).unwrap_or(false) { continue; }
            let key = make_key(repo, Path::new(&text(root)), &home, |k| taken.iter().any(|(t, r)| t == k && r != repo));
            // A repo that says a key of its own (one another repo has too) keeps it as it is: it goes by a stand-in.
            if said.iter().any(|s| s.repo == *repo && s.place != "standin") { let mut o = read(); o.obj_mut("standIn").put(repo, key.clone()); write(o); }
            else { write_yours("repo.key", Some(&json!(key)), repo)?; }
            taken.push((key.clone(), repo.clone()));
            given.push((repo.clone(), key));
        }
        Ok(given)
    }).unwrap_or_default()
}
fn tell(given: &[(String, String)]) {
    if given.is_empty() { return; }
    let _ = with_lock("repo-keys", || {
        let mut o = read();
        let mut untold = o.arr("untold").to_vec();
        for (r, k) in given {
            match untold.iter_mut().find(|p| p.get(0).and_then(|x| x.as_str()) == Some(r.as_str())) { Some(p) => *p = json!([r, k]), None => untold.push(json!([r, k])) }
        }
        o.put("untold", untold);
        write(o);
        Ok(())
    });
}
/// The first run of this version: every repo the board knew without a key gets one, once. Said by the board.
pub fn keys_at_start() {
    if read().b("upgraded") { return; }
    let given = give_keys(None);
    tell(&given);
    let _ = with_lock("repo-keys", || { let mut o = read(); o.put("upgraded", true); write(o); Ok(()) });
}
/// The board opening: a repo whose key was taken away by hand gets one again; then what was given and not yet said,
/// as one line, said once.
pub fn keys_note() -> String {
    keys_at_start();
    tell(&give_keys(None));
    let mut said = String::new();
    let _ = with_lock("repo-keys", || {
        let mut o = read();
        let (all, have) = (said_keys(), repo_keys());
        // A stand-in says why: the repo's own key is another repo's too, and stays as it is.
        let why = |repo: &str| all.iter().find(|s| s.repo == repo && s.place != "standin").and_then(|s| have.iter().find(|(r, k)| **k == s.key && *r != repo).map(|(r, _)| format!(" (its own {} is {r}'s too)", s.key))).unwrap_or_default();
        let untold: Vec<String> = o.arr("untold").iter().map(|p| { let p = strings(p); let repo = p.first().cloned().unwrap_or_default(); format!("{} for {repo}{}", p.get(1).cloned().unwrap_or_default(), why(&repo)) }).collect();
        if untold.is_empty() { return Ok(()); }
        said = format!("Every repo and folder has a short key now: {}. Change one: herdr-projects set repo.key <KEY> --repo <name>", untold.join(", "));
        o.shift_remove("untold");
        write(o);
        Ok(())
    });
    said
}
