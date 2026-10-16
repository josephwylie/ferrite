//! Ferrite: what Ferrite changes (README.md, ADR 0031). Its 0.2 (`ferrite-projects` 0.2.34) is taken over as upstream's
//! is, by the names it wrote; the Server's `take-over` says once what that did; `install` and `update` leave the
//! program to Ferrite; and inside Ferrite's app the program agents and hooks run is the shipped one.
use crate::common::{delegate, task, world, world_with, Opts, World};
use crate::takeover_kit::*;
use serde_json::json;
use std::path::{Path, PathBuf};
use sw::json::*;

const HOOK: &str = "'/Applications/Ferrite.app/Contents/MacOS/ferrite-projects' --root /h/.herdr-projects hook --agent claude 2>/dev/null || true";

/// What Ferrite's 0.2 left in a home folder: a project in upstream's root, its settings folder with its own journal
/// and its built-in skill, its hooks under its own name (Copilot's in a file of that name) and its skill's link.
fn ferrite_0_2(home: &Path) {
    put(&home.join(".herdr-projects/app/PROJECT.md"), "+++\nname = \"app\"\n+++\n# Instructions\n");
    put(&home.join(".config/herdr-projects/ferrite-owned.json"), "{}");
    put(&home.join(".config/herdr-projects/skill/autoproject/SKILL.md"), "---\nname: autoproject\n---\n");
    let hook = json!({ "matcher": "*", "hooks": [{ "type": "command", "command": HOOK, "timeout": 10 }] });
    let mine = json!({ "matcher": "Bash", "hooks": [{ "type": "command", "command": "my-own-hook" }] });
    put(&home.join(".claude/settings.json"), &json!({ "hooks": { "SessionStart": [hook.clone()], "PostToolUse": [hook, mine] } }).to_string());
    let copilot = HOOK.replace("--agent claude", "--agent copilot");
    put(&home.join(".copilot/hooks/ferrite-projects.json"), &json!({ "version": 1, "hooks": { "SessionStart": [{ "type": "command", "command": copilot, "timeoutSec": 10 }] } }).to_string());
    std::fs::create_dir_all(home.join(".claude/skills")).unwrap();
    std::os::unix::fs::symlink(home.join(".config/herdr-projects/skill/autoproject"), home.join(".claude/skills/autoproject")).unwrap();
}

#[test]
fn ferrite_0_2_is_set_aside_and_its_hooks_and_skill_link_taken_out_by_its_own_names() {
    let w = world_with(Opts::default());
    let m = machine(&w.base);
    ferrite_0_2(&m.home);
    let (root, cfg) = (all(&m.at(".herdr-projects")), all(&m.at(".config/herdr-projects")));
    assert!(sw::take_over::due(&m.p), "its journal alone marks its settings folder");
    let r = m.run(&Fake::default());
    assert_eq!(all(&m.at(".herdr-projects-legacy")), root);
    assert_eq!(all(&m.at(".config/herdr-projects-legacy")), cfg);
    let s: serde_json::Value = serde_json::from_str(&read(&m.at(".claude/settings.json"))).unwrap();
    assert_eq!(s, json!({ "hooks": { "PostToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "my-own-hook" }] }] } }));
    assert!(read(&m.at(".claude/settings.json.bak-herdr-projects")).contains(HOOK));
    assert!(!exists(&m.at(".copilot/hooks/ferrite-projects.json")), "its own file goes with its last hook");
    assert!(read(&m.at(".copilot/hooks/ferrite-projects.json.bak-herdr-projects")).contains(" hook --agent copilot"));
    assert!(!exists(&m.at(".claude/skills/autoproject")));
    assert!(r.said.iter().any(|l| l.starts_with("Taken out of the old herdr-projects: its hooks in ~/.claude/settings.json, ~/.copilot/hooks/ferrite-projects.json; its links ~/.claude/skills/autoproject")), "{:?}", r.said);
}

#[test]
fn a_hook_that_is_neither_0_2s_is_left_alone() {
    let w = world_with(Opts::default());
    let m = machine(&w.base);
    put(&m.at(".config/herdr-projects/ferrite-owned.json"), "{}");
    let theirs = json!({ "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "/bin/ferrite-projects turn-ended --id x" }] }] } });
    put(&m.at(".claude/settings.json"), &theirs.to_string());
    m.run(&Fake::default());
    assert_eq!(read(&m.at(".claude/settings.json")), theirs.to_string());
}

/// A copy of the program, run as Ferrite's Server runs it: on a home folder of its own, with nothing of the tests'.
fn run(w: &World, home: &Path, args: &[&str]) -> (i32, String) {
    let exe = w.base.join("bin").join("ferrite-projects");
    if !exe.exists() { std::fs::create_dir_all(exe.parent().unwrap()).unwrap(); std::fs::copy(env!("CARGO_BIN_EXE_herdr-projects"), &exe).unwrap(); }
    let o = std::process::Command::new(&exe).args(args).current_dir(home).env_clear()
        .env("HOME", home).env("PATH", "/usr/bin:/bin").env("HERDR_SOCKET_PATH", "/nonexistent").env("HERDR_BIN_PATH", "/bin/false")
        .stdin(std::process::Stdio::null()).output().unwrap();
    assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into_owned())
}

#[test]
fn real_programs_take_over_says_once_what_was_taken_over() {
    let w = world();
    let home = w.base.join("machine");
    std::fs::create_dir_all(&home).unwrap();
    let home = std::fs::canonicalize(&home).unwrap();
    ferrite_0_2(&home);
    let (_, said) = run(&w, &home, &["take-over"]);
    assert!(said.starts_with("The old herdr-projects (0.2) kept its projects in ~/.herdr-projects: they are in ~/.herdr-projects-legacy now, as they were, its settings in ~/.config/herdr-projects-legacy."), "{said}");
    assert!(said.contains("its hooks in ~/.claude/settings.json, ~/.copilot/hooks/ferrite-projects.json"), "{said}");
    assert_eq!(said.lines().count(), 1, "one line, for one toast");
    assert!(exists(&home.join(".herdr-projects-legacy/app/PROJECT.md")));
    assert_eq!(run(&w, &home, &["take-over"]).1, "", "said once");
    assert_eq!(run(&w, &home, &["take-over", "--json"]).1, "{\"said\":\"\"}\n", "--json says it too");
}

#[test]
fn real_programs_install_and_update_leave_the_program_to_ferrite() {
    let w = world();
    let home = w.base.join("machine");
    std::fs::create_dir_all(&home).unwrap();
    for cmd in ["install", "update"] {
        assert_eq!(run(&w, &home, &[cmd]), (0, format!("{}\n", sw::cli::ferrite::SHIPPED)));
        assert_eq!(run(&w, &home, &[cmd, "--json"]).1, format!("{}\n", json!({ "said": sw::cli::ferrite::SHIPPED })));
    }
}

#[test]
fn the_program_handed_to_agents_is_the_one_ferrite_names() {
    let _w = world();
    unsafe { std::env::set_var("FERRITE_PROJECTS_BIN", "/Applications/Ferrite.app/Contents/MacOS/ferrite-projects") };
    let bin = sw::store::files::bin();
    unsafe { std::env::remove_var("FERRITE_PROJECTS_BIN") };
    assert_eq!(bin, PathBuf::from("/Applications/Ferrite.app/Contents/MacOS/ferrite-projects"));
    assert_ne!(sw::store::files::bin().file_name().unwrap(), "ferrite", "the tests run as themselves");
}

/// Under Ferrite's Server every agent is told herdr's pane environment: `HERDR_PANE_ID`, its Thread's id, which its
/// task's link names as the pane, and `HERDR_SOCKET_PATH`, the Server's API socket (ferrite-core's
/// `server::projects::pane_environment`). So a task's own agent is known as its agent: its "blocked" is its own.
#[test]
fn under_ferrite_a_tasks_agent_is_known_by_its_thread() {
    let w = world();
    let id = w.add("Pick a colour");
    delegate(&id);
    let pane = task(&id).at("link").t("pane");
    unsafe { std::env::set_var("HERDR_PANE_ID", &pane) };
    let caller = sw::cli::caller().map(|(t, role)| (t.t("id"), role));
    let a = sw::cli::flags::parse(&["report", "--state", "blocked", "--msg", "Which colour?"].map(String::from));
    let said = sw::cli::tasks::command("report", &a).map(|(_, said)| said);
    unsafe { std::env::remove_var("HERDR_PANE_ID") };
    assert_eq!(caller, Some((id.clone(), "agent")));
    assert!(said.as_ref().is_ok_and(|s| s.starts_with("Reported: you are waiting, not finished")), "{:?}", said.map_err(|e| e.msg));
    let t = task(&id);
    assert_eq!(t.s("stage"), "asks");
    assert!(t.at("selfBlocked").is_number(), "its own report: the stop after it is no finish");
}
