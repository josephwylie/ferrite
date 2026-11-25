//! The operator's setup synced with a machine's Server through its bridge,
//! on the connection of its own a window's `Cockpit::tree` opens (ADR
//! 0048, ADR 0050): the Setup root is the Server's own home directory, by
//! this Ferrite's table.

use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ferrite_core::chunked::Cancellation;
use ferrite_core::setup_sync::{self, Home, Job, Outcome};
use ferrite_core::tree::{Asked, Exchange, Reply, Request};

use super::remote_operations::machine;
use super::*;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn sync(job: &Job, exchange: &mut dyn Exchange) -> setup_sync::Synced {
    match setup_sync::run(job, exchange, &mut |_| {}, &Cancellation::default()).unwrap() {
        Outcome::Synced(synced) => synced,
        outcome => panic!("{outcome:?}"),
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
    write(&mine.join(".claude/plugins/installed_plugins.json"), r#"{"plugins": {}}"#);
    let job = Job::for_machine(Home::at(mine.clone(), None), &harness.dir.join("settings"), "box");

    let mut exchange = cockpit.tree(cockpit.first_machine()).unwrap()().unwrap();
    let first = sync(&job, &mut *exchange);
    assert_eq!((first.pushed, first.pulled), (4, 0), "{first:?}");
    assert_eq!(std::fs::read_to_string(far.join(".claude/skills/deploy/SKILL.md")).unwrap(), "# Deploy\n");
    assert!(!far.join(".claude/history.jsonl").exists());
    assert!(!far.join(".claude/plugins").exists(), "Claude's own record of its plugins is each side's");
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
    let table = serde_json::json!({ "kind": "setup_table", "table": setup_sync::TABLE });
    assert!(!roots.is_empty() && roots.iter().all(|root| *root == table), "{roots:?}");
}

/// Whether anything holds the home whose lock is at `lock`.
fn held(lock: &Path) -> bool {
    std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
    let file = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(lock).unwrap();
    file.try_lock().is_err()
}

/// A hub's connection, cut as its sync's first write goes out, once it
/// looked at whether the Machine's home was held then.
struct Vanishing {
    there: Box<dyn Exchange>,
    lock: PathBuf,
    held: Option<bool>,
}

impl Exchange for Vanishing {
    fn ask(&mut self, request: Request) -> io::Result<Asked> {
        if matches!(request, Request::Begin { .. }) {
            self.held = Some(held(&self.lock));
            return Err(io::Error::new(io::ErrorKind::ConnectionAborted, "cut off"));
        }
        self.there.ask(request)
    }

    fn answer(&mut self, asked: Asked) -> io::Result<Reply> {
        self.there.answer(asked)
    }
}

/// A hub that vanishes mid-sync: while it synced, the Machine's home was
/// held, so the Machine's own syncs waited for it, and once its connection
/// goes the Server lets the home go.
#[test]
fn a_hub_that_vanishes_mid_sync_leaves_the_machines_home_free() {
    let harness = start("setup-sync-vanishes");
    let far = harness.dir.join("machine-home");
    std::fs::create_dir_all(&far).unwrap();
    let far = dunce::canonicalize(far).unwrap();
    std::env::set_var("HOME", &far);
    std::env::set_var("USERPROFILE", &far);
    std::env::remove_var("CODEX_HOME");
    let lock = ferrite_core::server::settings_dir().join("setup-sync.lock");
    assert!(lock.starts_with(&far), "{lock:?}");
    let sent = Arc::default();
    let mut cockpit = machine(&harness, &sent);
    let mine = harness.dir.join("desktop-home");
    write(&mine.join(".claude/CLAUDE.md"), "Be careful.\n");
    let job = Job::for_machine(Home::at(dunce::canonicalize(mine).unwrap(), None), &harness.dir.join("settings"), "box");

    let there = cockpit.tree(cockpit.first_machine()).unwrap()().unwrap();
    let mut vanishing = Vanishing { there, lock: lock.clone(), held: None };
    let cut = setup_sync::run(&job, &mut vanishing, &mut |_| {}, &Cancellation::default());
    assert!(cut.is_err(), "{cut:?}");
    assert_eq!(vanishing.held, Some(true), "the Machine's home was free while its hub synced");
    drop(vanishing);
    let deadline = Instant::now() + Duration::from_secs(10);
    while held(&lock) {
        assert!(Instant::now() < deadline, "the Machine's home stayed held once its hub vanished");
        std::thread::sleep(Duration::from_millis(20));
    }
}
