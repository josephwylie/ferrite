//! The operator's setup synced with a machine's Server through its bridge,
//! on the connection of its own a window's `Cockpit::tree` opens (ADR
//! 0048): the Setup root is the Server's own home directory.

use ferrite_core::chunked::Cancellation;
use ferrite_core::setup_sync::{self, Home, Job, Outcome};
use ferrite_core::tree::Exchange;

use super::remote_operations::machine;
use super::*;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn sync(job: &Job, exchange: &mut dyn Exchange) -> setup_sync::Synced {
    match setup_sync::run(job, exchange, &mut |_| {}, &Cancellation::default()).unwrap() {
        Outcome::Synced(synced) => synced,
        Outcome::Stopped(valve) => panic!("stopped: {valve:?}"),
    }
}

#[test]
fn a_setup_sync_lands_in_the_machines_home_and_comes_back_from_it() {
    let harness = start("setup-sync");
    let far = harness.dir.join("machine-home");
    std::fs::create_dir_all(&far).unwrap();
    let far = dunce::canonicalize(far).unwrap();
    // The Server runs in this process: its home directory is the
    // machine's.
    std::env::set_var("HOME", &far);
    std::env::set_var("USERPROFILE", &far);
    std::env::remove_var("CODEX_HOME");
    let sent = Arc::default();
    let mut cockpit = machine(&harness, &sent);

    let mine = dunce::canonicalize({
        let mine = harness.dir.join("desktop-home");
        std::fs::create_dir_all(&mine).unwrap();
        mine
    })
    .unwrap();
    write(&mine.join(".claude/skills/deploy/SKILL.md"), "# Deploy\n");
    write(&mine.join(".claude/CLAUDE.md"), "Be careful.\n");
    write(&mine.join(".claude/.credentials.json"), r#"{"claudeAiOauth": {}}"#);
    write(&mine.join(".claude/history.jsonl"), "not the setup\n");
    let install = mine.join(".claude").join("plugins").join("cache").join("tool");
    write(
        &mine.join(".claude/plugins/installed_plugins.json"),
        &format!(r#"{{"installPath": {}}}"#, serde_json::to_string(&install.to_string_lossy()).unwrap()),
    );
    let job = Job::for_machine(Home::at(mine.clone(), None), &harness.dir.join("settings"), "box");

    let mut exchange = cockpit.tree(cockpit.first_machine()).unwrap()().unwrap();
    let first = sync(&job, &mut *exchange);
    assert_eq!((first.pushed, first.pulled), (5, 0), "{first:?}");
    assert_eq!(std::fs::read_to_string(far.join(".claude/skills/deploy/SKILL.md")).unwrap(), "# Deploy\n");
    assert!(!far.join(".claude/history.jsonl").exists());
    let plugins: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(far.join(".claude/plugins/installed_plugins.json")).unwrap()).unwrap();
    let there = far.join(".claude").join("plugins").join("cache").join("tool");
    assert_eq!(plugins["installPath"], *there.to_string_lossy());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let bits = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(bits(&far.join(".claude/.credentials.json")), 0o600);
        assert_eq!(bits(&far.join(".claude")), 0o700);
    }

    write(&far.join(".claude/commands/made-there.md"), "From the Machine.\n");
    let back = sync(&job, &mut *exchange);
    assert_eq!((back.pushed, back.pulled), (0, 1), "{back:?}");
    assert_eq!(std::fs::read_to_string(mine.join(".claude/commands/made-there.md")).unwrap(), "From the Machine.\n");
    assert!(job.dir.join(setup_sync::BASE).exists());

    let roots: Vec<serde_json::Value> = sent.lock().unwrap()
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .filter(|message| message["op"] == "tree" && message["request"]["action"] == "scan")
        .map(|message| message["request"]["root"].clone())
        .collect();
    assert!(!roots.is_empty() && roots.iter().all(|root| *root == serde_json::json!({ "kind": "setup" })), "{roots:?}");
}
