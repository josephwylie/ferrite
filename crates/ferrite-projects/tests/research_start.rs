//! Research only is decided when the agent starts: a change while the task waits for a slot, or while the repo's
//! setup still runs, counts for the agent that starts after it, and the repo's file is the one of the commit its
//! worktree starts from.
use crate::common;
use common::*;
use serde_json::{json, Value};
use sw::json::*;
use sw::research::is_research;
use sw::store::settings::{lookup::{lookup, Scope}, places::yours};

const NAME: &str = "research.only";
const RULE: &str = "This task is research only: change nothing in the repo";
fn at(id: &str) -> (Value, String) { let r = lookup(NAME, &Scope::task(&task(id)), &yours()).unwrap(); (r.value, r.from) }
/// The rules the program was started with (its standing rules, in a file).
fn started(id: &str) -> String { read(&sw::store::files::dir_of(id).join("rules.md")) }

#[test]
fn research_only_is_decided_when_the_agent_starts_and_the_repos_file_is_the_one_of_its_commit() {
    let w = world_with(Opts { repo: true, repo_settings: Some(json!({ "research": { "only": true } })), settings: json!({ "board.agents": 1 }), ..Default::default() });
    // The checkout's own copy of the file says nothing (not committed): main still says research only.
    std::fs::write(w.root.join(".herdr-projects.json"), "{}").unwrap();
    w.script(json!({ "worker": vec![json!({ "status": "working" }); 3] }));
    let first = w.add("From the commit");
    let second = sw::actions::filing::add(&w.root, "Changed while it waits", sw::actions::filing::AddOpts { research: Some(false), ..Default::default() }).unwrap().t("id");
    assert_eq!(at(&first), (json!(false), "built in".into()), "before it has a worktree, the file as it stands in the folder");
    delegate(&first); delegate(&second);
    assert_eq!(task(&second).s("stage"), "queued");
    assert!(!is_research(&row(&second)));
    // Waiting for a slot: the change is taken, the label follows at once, and nothing says "from its next start".
    let a = sw::cli::flags::parse(&["set", NAME, "true", "--task", &second].map(String::from));
    assert_eq!(sw::cli::tasks::command("set", &a).unwrap().1, format!("{NAME} for {second}: true  (this task)"));
    assert!(is_research(&row(&second)));
    assert!(!task(&second).has("researchRun"));
    let t = settle(&first);
    assert_eq!((t.t("stage"), t.at("researchRun").clone(), at(&first)), ("working".into(), json!(true), (json!(true), "the repo's file".into())));
    assert!(is_research(&row(&first)));
    assert!(started(&first).contains(RULE));
    // The slot is free: the second starts as what its setting says now.
    sw::actions::talk::stop(&first).unwrap();
    sw::actions::start::start_queued(&w.socket);
    let t = settle(&second);
    assert_eq!((t.t("stage"), t.at("researchRun").clone(), t.has("researchOpen")), ("working".into(), json!(true), false));
    assert!(started(&second).contains(RULE));
}

/// The setup runs in the test's own thread here (the board's jobs do), so the change is made by the setup itself, with
/// the command line, as you would make it from another terminal while it runs; what the board said then is kept.
#[test]
fn real_programs_a_change_while_the_repos_setup_still_runs_counts_for_the_agent_that_starts_after_it() {
    let w = world();
    let bin = env!("CARGO_BIN_EXE_herdr-projects");
    let seen = w.base.join("seen.json");
    w.script(json!({ "worker": [{ "status": "working" }] }));
    let setup = format!("'{bin}' list --json > '{0}.before' && '{bin}' set {NAME} true --task proj/1 > /dev/null && '{bin}' list --json > '{0}'", seen.display());
    let id = sw::actions::filing::add(&w.root, "Set up, then look", sw::actions::filing::AddOpts { settings: vec![("workspace.setup".into(), json!(setup))], ..Default::default() }).unwrap().t("id");
    delegate(&id);
    let t = settle(&id);
    let first = |f: std::path::PathBuf| -> Value { let v: Value = serde_json::from_str(&read(&f)).unwrap_or(Value::Null); v.as_array().and_then(|a| a.iter().find(|r| r.s("id") == "proj/1").cloned()).unwrap_or(Value::Null) };
    let before = first(w.base.join("seen.json.before"));
    let after = first(seen.clone());
    assert_eq!(before.s("stage"), "setup", "{before}");
    assert!(!before.b("research") && after.at("research") == &json!(true), "the row follows at once: {before} {after}");
    assert_eq!((t.t("stage"), t.at("researchRun").clone(), t.has("researchOpen")), ("working".into(), json!(true), false), "{}", t.at("failure"));
    assert!(started(&id).contains(RULE));
}
