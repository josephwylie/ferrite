//! Taking over older installs on the first run: a fresh machine, the old herdr-projects 0.2 only, herdr-superwork only,
//! and both; each run twice. The text changes in herdr's config, and what is left when something is in the way:
//! take_over_more.rs.
use crate::common::{world_with, Opts};
use crate::takeover_kit::*;
use serde_json::json;
use sw::json::*;
use sw::store::files::read_obj;
use sw::store::old_installs::{legacy, superwork};

#[test]
fn a_fresh_machine_has_nothing_to_take_over_and_no_folder_is_made() {
    let w = world_with(Opts::default());
    let m = machine(&w.base);
    assert!(!sw::take_over::due(&m.p));
    let f = Fake::default();
    let r = m.run(&f);
    assert!(r.said.is_empty() && r.left.is_empty() && r.stay.is_none());
    assert_eq!(std::fs::read_dir(&m.home).unwrap().count(), 0);
    assert!(f.calls.borrow().is_empty());
    assert_eq!(m.doctor(&f), vec!["No older install of herdr-projects or herdr-superwork on this machine.".to_string()]);
    assert_eq!(std::fs::read_dir(&m.home).unwrap().count(), 0);
}

#[test]
fn the_old_herdr_projects_alone_is_set_aside_whole_and_its_entries_outside_are_taken_out() {
    let w = world_with(Opts::default());
    let m = machine(&w.base);
    m.old_rust();
    let (root, cfg) = (all(&m.at(".herdr-projects")), all(&m.at(".config/herdr-projects")));
    assert!(sw::take_over::due(&m.p));
    let f = Fake::default();
    let r = m.run(&f);
    // The projects, the old settings and herdr's state folder for it: set aside whole, nothing in them changed.
    assert_eq!(all(&m.at(".herdr-projects-legacy")), root);
    assert_eq!(all(&m.at(".config/herdr-projects-legacy")), cfg);
    assert!(!exists(&m.at(".config/herdr-projects")) && !exists(&m.p.plugin_state.join("herdr-projects")));
    assert_eq!(read(&m.p.plugin_state.join("herdr-projects-legacy/handoff.json")), "{}");
    // This version's home folder holds only its own record now.
    assert_eq!(std::fs::read_dir(m.at(".herdr-projects")).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect::<Vec<_>>(), vec!["moved.json"]);
    assert_eq!(r.said[0], "The old herdr-projects (0.2) kept its projects in ~/.herdr-projects: they are in ~/.herdr-projects-legacy now, as they were, its settings in ~/.config/herdr-projects-legacy. Nothing in them was changed or deleted.");
    // Its key, its tab bar line and its sidebar rows are gone from herdr's config; herdr's own rows come back; another
    // binding stays.
    let c = read(&m.p.herdr_config);
    assert!(!c.contains("herdr-projects") && !c.contains("$hp_") && !c.contains("ui.sidebar"), "{c}");
    assert!(c.contains("accent = \"blue\"") && c.contains("command = \"other.thing\"") && !c.contains("tab_bar_right"), "{c}");
    // Its hooks: gone, the user's own hook beside them stays; a backup is kept.
    let s: serde_json::Value = serde_json::from_str(&read(&m.at(".claude/settings.json"))).unwrap();
    assert_eq!(s, json!({ "theme": "dark", "hooks": { "PostToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "my-own-hook" }] }] } }));
    assert!(read(&m.at(".claude/settings.json.bak-herdr-projects")).contains(" hook --agent "));
    // Its skill link and the link on the PATH.
    assert!(!exists(&m.at(".claude/skills/autoproject")) && !exists(&m.at(".local/bin/herdr-projects")));
    assert!(r.said.iter().any(|l| l == "Taken out of herdr's config: the old herdr-projects' tab bar, sidebar, key (a backup is beside it)."), "{:?}", r.said);
    assert_eq!(*f.calls.borrow(), vec!["config"]);
    let rec = read_obj(&m.at(".herdr-projects/moved.json")).unwrap();
    assert_eq!((rec.at("takeOver").s("legacy"), rec.at("told")), ("done", &json!(false)));
    assert!(rec.s("line").starts_with("The old herdr-projects (0.2) kept its projects"));
    // A second run finds nothing to do and changes nothing.
    let (home, config) = (all(&m.home), read(&m.p.herdr_config));
    assert!(!sw::take_over::due(&m.p));
    let again = m.run(&Fake::default());
    assert!(again.said.is_empty() && again.left.is_empty());
    assert_eq!((all(&m.home), read(&m.p.herdr_config)), (home, config));
    let d = m.doctor(&Fake::default());
    assert!(d.iter().any(|l| l.starts_with("Old herdr-projects (0.2): its projects are in ~/.herdr-projects-legacy, set aside on ")), "{d:?}");
    assert!(d.iter().any(|l| l.contains("HERDR_PROJECTS_ROOT=~/.herdr-projects-legacy")), "{d:?}");
}

#[test]
fn herdr_superwork_alone_moves_into_the_new_home_with_its_paths_fixed_and_its_extras_made_this_versions() {
    let w = world_with(Opts::default());
    let m = machine(&w.base);
    m.superwork();
    let s = m.at(".herdr-superwork");
    let (old, new) = (s.display().to_string(), m.at(".herdr-projects").display().to_string());
    let before = all(&s);
    let f = Fake { old_running: true, plugin_on: std::cell::RefCell::new(true), ..Default::default() };
    let r = m.run(&f);
    let to = m.at(".herdr-projects");
    // The folder itself moved; the old name is a link to it, so an old path still reaches every file.
    assert!(is_link(&s) && std::fs::canonicalize(&s).unwrap() == to);
    assert!(!is_link(&to) && to.is_dir());
    assert_eq!(all(&s).keys().collect::<Vec<_>>(), all(&to).keys().collect::<Vec<_>>());
    // The paths the board reads back are the new ones; the repo's name in a task's id stays; another folder's path
    // that only starts the same stays.
    let t = read(&to.join("data/tasks/herdr-superwork/7/task.json"));
    assert_eq!(t, format!(r#"{{"id":"herdr-superwork/7","repo":"herdr-superwork","note":"{new}/data/tasks/herdr-superwork/7/closing-note.md","report":"~/.herdr-projects/reports/herdr-superwork/7/","other":"{old}-elsewhere/x"}}"#));
    assert!(read(&to.join("data/tasks/herdr-superwork/7/hook-agent.json")).contains(&format!("'{new}/data/bin/herdr-projects' turn-ended")));
    // What is kept as it was stays so: an artifact, the Coordinator's chat folder, a report, an older copy.
    for f in ["data/tasks/herdr-superwork/7/artifacts/check.txt", "data/coordinator/default/chat/notes.md", "reports/herdr-superwork/7/report.md", "data-0.2.0/keeper.json"] { assert_eq!(read(&to.join(f)), before[f], "{f}"); }
    // The launcher agents were handed by the old name runs this version.
    assert_eq!(read(&to.join("data/bin/herdr-superwork")), LAUNCHER);
    // Its processes: stopped, and this version's started last, once the record says all is done (what they start
    // finds nothing left to take over); its plugin switched off.
    assert_eq!(*f.calls.borrow(), vec![format!("stop {new}/data"), "config".into(), "start keeper,page server".into()]);
    // herdr's config: its blocks are this version's, with the same key; a binding of one of its actions names ours.
    let c = read(&m.p.herdr_config);
    assert!(!c.contains("herdr-superwork"), "{c}");
    assert!(c.contains(&sw::extras::block_text("key", "prefix+a")) && c.contains(&sw::extras::block_text("sidebar", "")) && c.contains("command = \"herdr-projects.coordinator\""), "{c}");
    // Its how-to folders are gone, and from the record of what the plugin wrote; another path there stays.
    for d in [".claude/skills", ".codex/skills"] { assert!(!exists(&m.at(d).join("herdr-superwork"))); }
    let x = read_obj(&to.join("data/extras.json")).unwrap();
    assert_eq!(strings(x.at("skillPaths")), vec![m.at("elsewhere/mine").display().to_string()]);
    assert_eq!(x.at("webUiLinks"), &json!([{ "https": 8751, "now": "http://127.0.0.1:7327" }])); // the web UI's link carries over
    // herdr's folders for the plugin: what herdr-superwork's held is herdr-projects' now.
    assert_eq!(read(&m.p.plugin_config.join("herdr-projects/settings.toml")), "x = 1\n");
    assert!(!exists(&m.p.plugin_config.join("herdr-superwork")) && !exists(&m.p.plugin_state.join("herdr-superwork")));
    // The record: the old moves stay as they were, this one beside them, and the Coordinator's old folder is found.
    let rec = read_obj(&to.join("moved.json")).unwrap();
    assert_eq!(rec.at("data").s("to"), format!("{old}/data"));
    assert_eq!((rec.at("home").s("from"), rec.at("home").s("to"), rec.at("home").s("state")), (old.as_str(), new.as_str(), "done"));
    assert_eq!(rec.at("takeOver").s("superwork"), "done");
    assert!(r.said[0].starts_with("herdr-superwork is herdr-projects now: everything in ~/.herdr-superwork (tasks, settings, reports, the Coordinator's chat) moved into ~/.herdr-projects"), "{:?}", r.said);
    assert!(r.said[1].contains("its keeper and page server restarted on this version, its plugin switched off in herdr, its how-to replaced"), "{:?}", r.said);
    assert_eq!((rec.s("line"), rec.at("told")), (r.said.join(" ").as_str(), &json!(false)));
    // Twice: nothing more happens.
    let (home, config) = (all(&m.home), read(&m.p.herdr_config));
    assert!(!sw::take_over::due(&m.p));
    let f2 = Fake::default();
    let again = m.run(&f2);
    assert!(again.said.is_empty() && f2.calls.borrow().is_empty());
    assert_eq!((all(&m.home), read(&m.p.herdr_config)), (home, config));
    let d = m.doctor(&Fake::default());
    assert!(d.iter().any(|l| l.starts_with("herdr-superwork: moved into ~/.herdr-projects on ") && l.contains("~/.herdr-superwork is a link to it")), "{d:?}");
}

#[test]
fn both_the_old_projects_are_set_aside_first_and_herdr_superwork_takes_the_name_never_mixed() {
    let w = world_with(Opts::default());
    let m = machine(&w.base);
    m.old_rust();
    m.superwork();
    let (root, sw_files) = (all(&m.at(".herdr-projects")), all(&m.at(".herdr-superwork")).into_keys().collect::<Vec<_>>());
    let r = m.run(&Fake::default());
    assert_eq!(all(&m.at(".herdr-projects-legacy")), root);
    let to = m.at(".herdr-projects");
    let now: Vec<String> = all(&to).into_keys().collect();
    assert!(sw_files.iter().all(|f| now.contains(f)));
    assert!(!now.iter().any(|f| f.contains("PROJECT.md") || f.starts_with(".ticker")), "{now:?}");
    assert!(r.said[0].starts_with("The old herdr-projects (0.2) kept its projects") && r.said[1].starts_with("herdr-superwork is herdr-projects now"), "{:?}", r.said);
    // herdr's config: the old version's entries gone, herdr-superwork's made this version's, in one change.
    let c = read(&m.p.herdr_config);
    assert!(!c.contains("herdr-superwork") && !c.contains("open-popup") && !c.contains("$hp_") && c.contains(&sw::extras::block_text("key", "prefix+a")), "{c}");
    let rec = read_obj(&to.join("moved.json")).unwrap();
    assert_eq!((rec.at("takeOver").s("legacy"), rec.at("takeOver").s("superwork")), ("done", "done"));
    assert!(rec.at("legacy").b("projects") && rec.at("home").s("state") == "done");
    let home = all(&m.home);
    let again = m.run(&Fake::default());
    assert!(again.said.is_empty() && !legacy::due(&m.p) && !superwork::due(&m.p));
    assert_eq!(all(&m.home), home);
}
