//! Research only as the setting research.only: looked up like every other, the short form and the setting writing the
//! same thing, a default for a repo or for you; a change before a start counting at once and one on a task under way
//! from its next start, and turning it on refused once the repo was changed or a pull request is open.
use crate::common;
use common::board::open;
use common::*;
use serde_json::{json, Value};
use sw::json::*;
use sw::research::is_research;
use sw::store::settings::{change::{set_value, unset_value}, lookup::{lookup, Scope}, places::yours};

const NAME: &str = "research.only";
fn re(r: &str, s: &str) -> bool { regex::Regex::new(r).unwrap().is_match(s) }
fn at(id: &str) -> (Value, String) { let r = lookup(NAME, &Scope::task(&task(id)), &yours()).unwrap(); (r.value, r.from) }
/// What the command says, as it prints it (without the end of the line).
fn run(argv: &[&str]) -> String { let a = sw::cli::flags::parse(&argv.iter().map(|s| s.to_string()).collect::<Vec<_>>()); sw::cli::tasks::command(&a.pos[0], &a).map(|(_, s)| s).unwrap_or_else(|e| panic!("{argv:?}: {}", e.msg)) }
fn run_json(argv: &[&str]) -> Value { let a = sw::cli::flags::parse(&argv.iter().map(|s| s.to_string()).collect::<Vec<_>>()); sw::cli::tasks::command(&a.pos[0], &a).unwrap().0 }
fn delegate_as(id: &str, v: Value) -> sw::err::Res<Obj> { sw::actions::start::delegate(id, vec![(NAME.into(), v)]) }
fn row_research(id: &str) -> bool { is_research(&row(id)) }
fn put(id: &str, fields: Value) { sw::store::tasks::set(id, fields).unwrap(); }

#[test]
fn one_setting_looked_up_like_every_other_the_short_form_and_the_setting_write_the_same_thing() {
    let w = world();
    let e = sw::store::settings_list::entry_of(NAME).unwrap();
    assert_eq!((e.kind.name(), e.builtin.clone(), e.label.as_str()), ("work", json!(false), "Research only"));
    let plain = w.add("Fix the page");
    assert_eq!(at(&plain), (json!(false), "built in".into()));
    assert!(!row_research(&plain));
    // The short form, and the setting by its name, on add: one source of truth, on the task.
    let root = w.root_str();
    assert_eq!(run(&["add", "Compare", "the", "parsers", "--research", "--repo", &root]), "proj/2: added as research only");
    assert_eq!(run(&["add", "Audit", "the", "build", "--research.only", "true", "--repo", &root]), "proj/3: added as research only");
    for id in ["proj/2", "proj/3"] { assert_eq!((task(id).at("settings").clone(), task(id).has("research")), (json!({ NAME: true }), false)); assert_eq!(at(id), (json!(true), "this task".into())); assert!(row_research(id)); }
    // set, unset and show with --task: the place is named each time.
    assert_eq!(run(&["set", NAME, "yes", "--task", &plain]), format!("{NAME} for proj/1: true  (this task)"));
    assert!(re(r"\n\* research\.only +true +this task\n", &run(&["show", &plain])));
    assert_eq!(run_json(&["show", &plain, "--json"]).arr("settings").iter().find(|r| r.s("name") == NAME).unwrap().s("from"), "this task");
    assert_eq!(run(&["unset", NAME, "--task", &plain]), format!("{NAME} for proj/1: false  (built in)"));
    assert!(re(r"\n {2}research\.only +false +built in\n", &run(&["show", &plain])));
    // A default for the repo: every new task there is research only unless the task says otherwise, and the row says so.
    assert_eq!(run(&["set", NAME, "true", "--repo", "proj"]), format!("{NAME} for proj: true  (your entry for proj)"));
    assert!(re(r"\n\* research\.only +true +your entry for proj\n", &run(&["show", "--repo", "proj"])));
    assert_eq!(run(&["add", "Look", "into", "the", "logs", "--repo", &root]), "proj/4: added as research only");
    assert_eq!(run(&["add", "Fix", "the", "logs", "--no-research", "--repo", &root]), "proj/5: added");
    assert_eq!(at("proj/4"), (json!(true), "your entry for proj".into()));
    assert!(task("proj/4").at("settings").as_object().map(|o| o.is_empty()).unwrap_or(true));
    assert_eq!(at("proj/5"), (json!(false), "this task".into()));
    assert_eq!(at(&plain), (json!(true), "your entry for proj".into()), "not started yet: it follows the default too");
    let mut s = open(&w.root, 40);
    let text = s.screen(120).join("\n");
    assert!(text.contains("Look into the logs [research]") && text.contains("Fix the page [research]") && !text.contains("Fix the logs [research]"), "{text}");
    let with_tree = |id: &str, n: &str| { let mut t = task(id); t.put("worktree", "/tmp/x"); t.put("branch", format!("work/{n}-x")); sw::delegate::rules::rules(&t) };
    assert!(with_tree("proj/4", "4").contains("This task is research only: change nothing in the repo"));
    assert!(!with_tree("proj/5", "5").contains("research only"));
    // For you, with no repo named; then nobody says it and it is built in again.
    run(&["unset", NAME, "--repo", "proj"]);
    assert_eq!(run(&["set", NAME, "on"]), format!("{NAME}: true  (you)"));
    assert_eq!(at("proj/4"), (json!(true), "you".into()));
    assert!(re(r"\n\* research\.only +true +you\n", &run(&["show"])));
    assert_eq!(run(&["unset", NAME]), format!("{NAME}: false  (built in)"));
    assert_eq!(at("proj/4"), (json!(false), "built in".into()));
    // A value that is refused adds no task.
    let before = sw::store::tasks::all().len();
    let e = sw::actions::filing::add(&w.root, "Wrong", sw::actions::filing::AddOpts { settings: vec![(NAME.into(), json!("maybe"))], ..Default::default() }).unwrap_err().msg;
    assert!(e.contains("research.only is a switch: true or false"), "{e}");
    assert_eq!(sw::store::tasks::all().len(), before);
    // On delegate, by its name and by the short form (the command line writes `--research` as the setting).
    delegate_as(&plain, json!("true")).unwrap();
    assert_eq!(task(&plain).at("settings").at(NAME), &json!(true));
    assert!(is_research(&task(&plain)));
    delegate_as("proj/2", json!(false)).unwrap();
    assert_eq!(task("proj/2").at("settings").at(NAME), &json!(false));
    assert!(!is_research(&task("proj/2")));
}

#[test]
fn a_change_before_a_start_counts_at_once_one_under_way_from_the_next_start_and_turning_it_on_is_refused_once_changed() {
    let w = world();
    // Not started: at once, and the label follows.
    let todo = w.add("Not started");
    set_value(NAME, &json!(true), &Scope::task(&task(&todo))).unwrap(); assert!(row_research(&todo));
    set_value(NAME, &json!(false), &Scope::task(&task(&todo))).unwrap(); assert!(!row_research(&todo));
    // Under way as an ordinary task, nothing changed yet: saved, and it counts from the next start.
    w.script(json!({ "worker": vec![json!({ "status": "working" }); 4] }));
    let id = w.add("Under way");
    delegate(&id);
    assert_eq!(settle(&id).s("stage"), "working");
    assert_eq!(task(&id).at("researchRun"), &json!(false));
    assert_eq!(run(&["set", NAME, "true", "--task", &id]), format!("{NAME} for {id}: true  (this task)\nIt counts from its next start: this run is not research only."));
    assert_eq!(at(&id), (json!(true), "this task".into()));
    assert!(!is_research(&task(&id)) && !row_research(&id));
    assert!(re(r"\n\* research\.only +true +this task · counts from its next start: this run is not research only\n", &run(&["show", &id])));
    let said = |id: &str| prompts(&w, task(id).at("link").s("pane")).join("\n---\n");
    assert!(!said(&id).contains("research only"));
    // The next start: it is research only, the row says so, and its agent is told the rules as they are now.
    sw::actions::talk::stop(&id).unwrap(); delegate(&id);
    let t = settle(&id);
    assert_eq!(t.at("researchRun"), &json!(true));
    assert!(row_research(&id) && !t.b("researchTurned"));
    assert!(re(r"This task is research only from now on\. These rules replace the ones you were given\.[\s\S]*This task is research only: change nothing in the repo", &said(&id)), "{}", said(&id));
    assert!(!sw::store::settings::show::show_text(&Scope::task(&t)).contains("counts from its next start"));
    // Turning it off is never refused, and counts from the next start as well.
    assert_eq!(set_value(NAME, &json!(false), &Scope::task(&task(&id))).unwrap().value, json!(false));
    assert_eq!(sw::store::settings::show::rows(&Scope::task(&task(&id))).into_iter().find(|r| r.name == NAME).unwrap().later, "counts from its next start: this run is research only");
    assert!(is_research(&task(&id)));
    sw::actions::talk::stop(&id).unwrap(); delegate(&id);
    assert!(!is_research(&task(&id)) && !row_research(&id), "waiting to start, it follows the setting already");
    let mut t = task(&id);
    sw::research::starts_as(&mut t).unwrap(); // what the job that starts its agent does, without running one more agent here
    assert_eq!((t.at("researchRun"), t.has("researchOpen")), (&json!(false), false));
    assert!(sw::delegate::rules::turned_words(&t).starts_with("This task is no longer research only: its result is a change to the repo"));
    sw::runner::JOBS.with(|j| j.borrow_mut().clear());
    // A task that was started as an ordinary one, in a checkout written down as it stood then (no agent is run for
    // this). Its agent has changed the repo: turning it on cannot be honoured, and nothing is saved.
    let own = w.add("Started before");
    put(&own, json!({ "startedOnce": true, "researchRun": false, "startMark": sw::research::start_mark(&w.root), "settings": { NAME: false } }));
    std::fs::write(w.root.join("new.txt"), "work\n").unwrap();
    let refused = r"research\.only cannot be turned on for proj/3: its agent has already changed the repo \(1 change in the repo: new\.txt\), and a research-only task changes nothing in it\. Leave proj/3 as it is and add a new task as research only, or undo the changes first\.";
    assert!(re(refused, &set_value(NAME, &json!(true), &Scope::task(&task(&own))).unwrap_err().msg));
    assert_eq!(task(&own).at("settings").at(NAME), &json!(false));
    // Taking its own "false" away would turn it on by a default: refused the same way, and so is a start.
    set_value(NAME, &json!(true), &Scope::repo("proj")).unwrap();
    assert!(!is_research(&task(&own)) && !row_research(&own), "a default never changes a task that was started");
    assert!(re(refused, &unset_value(NAME, &Scope::task(&task(&own))).unwrap_err().msg));
    sw::store::tasks::patch(&own, |x| { x.obj_mut("settings").shift_remove(NAME); }).unwrap();
    let e = sw::actions::start::delegate(&own, vec![]).unwrap_err().msg;
    assert!(re(r"already changed the repo[\s\S]*To start it as it was: herdr-projects set research\.only false --task proj/3", &e), "{e}");
    assert!(re(refused, &delegate_as(&own, json!(true)).unwrap_err().msg));
    assert_eq!(task(&own).s("stage"), "todo", "a refused start changes nothing");
    // A pull request that is open.
    std::fs::remove_file(w.root.join("new.txt")).unwrap();
    put(&own, json!({ "pr": "https://github.com/o/r/pull/7" }));
    assert!(re(r"cannot be turned on for proj/3: its pull request is open \(https://github\.com/o/r/pull/7\), and a research-only task opens none", &set_value(NAME, &json!(true), &Scope::task(&task(&own))).unwrap_err().msg));
    put(&own, json!({ "prMerged": true }));
    assert_eq!(set_value(NAME, &json!(true), &Scope::task(&task(&own))).unwrap().value, json!(true));
}
