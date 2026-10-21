//! The automations' commands: add, list, show, change, set and unset with --automation, on, off, run, firings, remove;
//! all with --json; and what each refuses, with nothing made.
use crate::common;
use common::*;
use serde_json::json;
use sw::json::*;
use sw::store::automations::{automation, automations, firings_of};

/// The command run in this process, in the world's repo, as the program prints it (cli/mod.rs: run).
fn run(w: &World, args: &[&str]) -> Result<String, String> {
    let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let a = sw::cli::flags::parse(&argv);
    let cmd = a.pos.first().cloned().unwrap_or_default();
    if cmd == "help" { return Ok(sw::cli::help::HELP.to_string()); }
    let was = std::env::current_dir().unwrap();
    std::env::set_current_dir(&w.root).unwrap();
    let r = sw::cli::flags::refuse_unknown(&cmd, &a).and_then(|_| sw::cli::tasks::command(&cmd, &a));
    std::env::set_current_dir(was).unwrap();
    match r {
        Ok((v, said)) => Ok(format!("{}\n", if a.flags.b("json") { serde_json::to_string_pretty(&v).unwrap() } else { said })),
        Err(e) => Err(e.msg),
    }
}
fn out(w: &World, args: &[&str]) -> String { run(w, args).unwrap_or_else(|e| panic!("{args:?}: {e}")) }
fn fails(w: &World, args: &[&str], re: &str) { let e = run(w, args).expect_err(re); assert!(regex::Regex::new(re).unwrap().is_match(&e), "{args:?}: {e}"); }
fn like(text: &str, re: &str) { assert!(regex::Regex::new(re).unwrap().is_match(text), "{re}\n{text}"); }

#[test]
fn the_commands_add_list_show_change_set_unset_on_off_run_firings_remove_all_with_json() {
    let w = world();
    like(&out(&w, &["automations"]), r"^No automations\. Make one: herdr-projects automations add");
    assert_eq!(out(&w, &["automations", "add", "Old dependencies", "--prompt", "Look for dependencies that are out of date.", "--schedule", "mon 09:00", "--reviewer.agent", "claude", "--research"]),
        "Added the automation \"Old dependencies\": mon 09:00, in proj. Each firing adds a task and puts an agent on it.\n");
    let a = automation("old dependencies").unwrap();
    // --research is short for the setting
    assert_eq!((a.at("trigger").s("cron"), a.at("settings"), a.at("research"), a.at("on"), a.s("repo")), ("0 9 * * 1", &json!({ "reviewer.agent": "claude", "research.only": true }), &json!(null), &json!(true), "proj"));
    like(&out(&w, &["automations", "list"]), r"^on {3}Old dependencies {2}· {2}mon 09:00 {2}· {2}proj {2}· {2}never fired {2}· {2}next in \d+[smhd]\n$");
    let listed: serde_json::Value = serde_json::from_str(&out(&w, &["automations", "--json"])).unwrap();
    assert_eq!((listed[0].s("name"), listed[0].s("trigger"), listed[0].at("on"), listed[0].at("settings"), listed[0].at("research")), ("Old dependencies", "mon 09:00", &json!(true), &json!({ "reviewer.agent": "claude", "research.only": true }), &json!(true)));
    // refused, and nothing is made: no trigger, two triggers, a board setting, a wrong value, a name that is taken
    fails(&w, &["automations", "add", "X", "--prompt", "p"], r#"needs one trigger: --schedule "every 2h" or --when merged\."#);
    fails(&w, &["automations", "add", "X", "--prompt", "p", "--schedule", "every 2h", "--when", "merged"], "one trigger: --schedule or --when, not both");
    fails(&w, &["automations", "add", "X", "--prompt", "p", "--when", "merged", "--board.agents", "9"], r"board\.agents is a board setting: it is yours only and cannot be set on an automation");
    fails(&w, &["automations", "add", "X", "--prompt", "p", "--when", "merged", "--merge.by", "nobody"], r"merge\.by");
    fails(&w, &["automations", "add", "old DEPENDENCIES", "--prompt", "p", "--when", "merged"], r#"There is an automation "Old dependencies" already"#);
    fails(&w, &["automations", "add", "X", "--when", "merged"], "needs a prompt");
    fails(&w, &["automations", "show", "nothing"], r#"No automation "nothing"\. There is: Old dependencies\."#);
    assert_eq!(automations().len(), 1);
    // its settings: the same three commands a task has
    assert_eq!(out(&w, &["set", "merge.by", "itself", "--automation", "Old dependencies"]), "merge.by for \"Old dependencies\": itself  (this automation)\nIt applies from its next firing.\n");
    like(&out(&w, &["show", "--automation", "Old dependencies"]), r#"^"Old dependencies": every setting its next task will have[\s\S]*\n\* reviewer\.agent +claude +this automation\n[\s\S]*\n\* merge\.by +itself +this automation\n[ ▌] {1}merge\.method +squash +built in\n"#);
    assert_eq!(out(&w, &["unset", "merge.by", "--automation", "Old dependencies"]), "merge.by for \"Old dependencies\": you  (built in)\nIt applies from its next firing.\n");
    fails(&w, &["set", "board.agents", "2", "--automation", "Old dependencies"], "cannot be set on an automation");
    fails(&w, &["set", "merge.by", "you", "--automation", "Old dependencies", "--task", "1"], "one of --task, --repo and --automation");
    fails(&w, &["set", "worker.effort", "ultra", "--automation", "Old dependencies"], r"worker\.effort");
    assert_eq!(automation(a.s("id")).unwrap().at("settings"), &json!({ "reviewer.agent": "claude", "research.only": true }));
    // research only is one of those settings: shown with its place, set and unset like the others
    like(&out(&w, &["show", "--automation", "Old dependencies"]), r"\n\* research\.only +true +this automation\n");
    assert_eq!(out(&w, &["unset", "research.only", "--automation", "Old dependencies"]), "research.only for \"Old dependencies\": false  (built in)\nIt applies from its next firing.\n");
    assert_eq!(out(&w, &["set", "research.only", "true", "--automation", "Old dependencies"]), "research.only for \"Old dependencies\": true  (this automation)\nIt applies from its next firing.\n");
    // change: name, prompt, trigger
    like(&out(&w, &["automations", "change", "Old dependencies", "--name", "Dependencies", "--when", "merged", "--every-repo", "--no-research"]), r#"^Changed "Dependencies": when a task is merged, in every repo\. It applies from its next firing"#);
    let now = automation(a.s("id")).unwrap();
    assert_eq!((now.at("trigger"), now.at("settings").at("research.only"), now.i("nextAt")), (&json!({ "kind": "event", "event": "merged", "everyRepo": true }), &json!(false), 0));
    fails(&w, &["automations", "change", "Dependencies"], "Nothing to change");
    // run now, its firings, off and on
    assert_eq!(out(&w, &["automations", "run", "Dependencies"]), "\"Dependencies\" fired: proj/1 was added and an agent is on it.\n");
    assert_eq!(out(&w, &["automations", "run", "Dependencies"]), "\"Dependencies\" did not fire: its last task, proj/1, is still being worked on.\n");
    like(&out(&w, &["automations", "firings", "Dependencies"]), r"^\d+s ago +run now {2}→ {2}skipped: its last task, PRJ-1, is still being worked on\n\d+s ago +run now {2}→ {2}made PRJ-1\n$");
    let f: serde_json::Value = serde_json::from_str(&out(&w, &["automations", "firings", "Dependencies", "--json"])).unwrap();
    assert_eq!(f.as_array().unwrap().iter().map(|f| if f.b("task") { f.t("task") } else { "skipped".into() }).collect::<Vec<_>>(), ["skipped", "proj/1"]);
    let shown = out(&w, &["automations", "show", "Dependencies"]);
    like(&shown, r"^Dependencies {2}\(on\)\n[ ▌] {1}trigger {3}when a task is merged, in every repo\n[ ▌] {1}repo {6}proj ");
    like(&shown, r"\n[ ▌] {1}settings {2}reviewer\.agent claude · research\.only false\n[ ▌] {1}prompt\n[ ▌] {3}Look for dependencies that are out of date\.\n[ ▌] {1}firings\n[ ▌] {3}\d+s ago");
    assert_eq!(out(&w, &["automations", "off", "Dependencies"]), "\"Dependencies\" is off. It never fires while it is off, and keeps everything.\n");
    fails(&w, &["automations", "run", "Dependencies"], r#""Dependencies" is off, and off it never fires\. Turn it on first"#);
    assert_eq!(out(&w, &["automations", "on", "Dependencies"]), "\"Dependencies\" is on.\n");
    assert_eq!(out(&w, &["automations", "remove", "Dependencies"]), "Removed the automation \"Dependencies\". The tasks it made stay.\n");
    assert!(automations().is_empty()); assert!(firings_of(a.s("id")).is_empty()); assert_eq!(sw::store::tasks::list(Default::default()).len(), 1);
    like(&out(&w, &["help"]), r"herdr-projects automations \[add\|show\|change\|on\|off\|run\|firings\|remove\]");
    like(&out(&w, &["automations", "help"]), "--schedule {3}every 2h · daily 09:00");
}
