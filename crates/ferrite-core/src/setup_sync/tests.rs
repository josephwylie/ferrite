use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use super::*;
use crate::chunked::Cancellation;
use crate::tree::{self, Asked, Exchange, Reply, Request, Session, Side};

/// A sign-in no output of a sync may hold.
const SECRET: &str = "sk-ant-oat01-NEVER-SHOWN-7f3a";

/// A Machine reached in this process: its Server's tree session over a
/// home of its own, cut off after `left` requests when that is set, and
/// `before` each request it takes.
struct Machine {
    session: Session,
    replies: VecDeque<(u64, io::Result<Reply>)>,
    asked: u64,
    left: Option<u64>,
    before: Box<dyn FnMut(&Request)>,
}

impl Machine {
    fn new(home: &Home) -> Self {
        Self::cut_after(home, None)
    }

    fn cut_after(home: &Home, left: Option<u64>) -> Self {
        Self {
            session: Session::with_home(home.clone()),
            replies: VecDeque::new(),
            asked: 0,
            left,
            before: Box::new(|_| {}),
        }
    }
}

impl Exchange for Machine {
    fn ask(&mut self, request: Request) -> io::Result<Asked> {
        if self.left.is_some_and(|left| self.asked >= left) {
            return Err(io::Error::new(io::ErrorKind::ConnectionAborted, "cut off"));
        }
        self.asked += 1;
        (self.before)(&request);
        let reply = self.session.execute(request);
        self.replies.push_back((self.asked, reply));
        Ok(Asked::numbered(self.asked))
    }

    fn answer(&mut self, asked: Asked) -> io::Result<Reply> {
        while let Some((id, reply)) = self.replies.pop_front() {
            if id == asked.id() {
                return reply;
            }
        }
        Err(io::Error::new(io::ErrorKind::ConnectionAborted, "cut off"))
    }
}

struct World {
    root: PathBuf,
}

impl World {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ferrite-setup-sync-{name}"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn home(&self, name: &str) -> Home {
        let dir = self.root.join(name);
        fs::create_dir_all(&dir).unwrap();
        Home::at(dunce::canonicalize(dir).unwrap(), None)
    }

    fn job(&self, home: &Home, machine: &str) -> Job {
        Job {
            dir: self.root.join("settings").join("machines").join(machine),
            names: Names::Any,
            ..Job::for_machine(home.clone(), &self.root.join("settings"), machine)
        }
    }
}

fn put(home: &Home, path: &str, text: &str) {
    let at = path.split('/').fold(home.root().to_path_buf(), |at, part| at.join(part));
    fs::create_dir_all(at.parent().unwrap()).unwrap();
    fs::write(at, text).unwrap();
}

fn read(home: &Home, path: &str) -> Option<String> {
    let at = path.split('/').fold(home.root().to_path_buf(), |at, part| at.join(part));
    fs::read_to_string(at).ok()
}

fn gone(home: &Home, path: &str) {
    let at = path.split('/').fold(home.root().to_path_buf(), |at, part| at.join(part));
    if at.is_dir() {
        fs::remove_dir_all(at).unwrap();
    } else {
        fs::remove_file(at).unwrap();
    }
}

fn aged(home: &Home, path: &str, seconds_ago: u64) {
    let at = path.split('/').fold(home.root().to_path_buf(), |at, part| at.join(part));
    let when = SystemTime::now() - Duration::from_secs(seconds_ago);
    fs::File::options().write(true).open(at).unwrap().set_modified(when).unwrap();
}

/// Every file under `home`, by its path, with what it holds.
fn files(home: &Home) -> BTreeMap<String, String> {
    fn walk(dir: &Path, prefix: &str, into: &mut BTreeMap<String, String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
            if entry.file_type().unwrap().is_dir() {
                walk(&entry.path(), &path, into);
            } else {
                into.insert(path, fs::read_to_string(entry.path()).unwrap_or_default());
            }
        }
    }
    let mut into = BTreeMap::new();
    walk(home.root(), "", &mut into);
    into
}

fn sync(job: &Job, machine: &Home) -> Synced {
    match run(job, &mut Machine::new(machine), &mut |_| {}, &Cancellation::default()).unwrap() {
        Outcome::Synced(synced) => synced,
        outcome => panic!("{outcome:?}"),
    }
}

/// A setup like the operator's, on this computer.
fn seed(home: &Home) {
    put(home, ".claude/skills/deploy/SKILL.md", "# Deploy\n");
    put(home, ".claude/skills/deploy/scripts/run.sh", "#!/bin/sh\necho deploy\n");
    put(home, ".claude/commands/review.md", "Review the diff.\n");
    put(home, ".claude/agents/helper.md", "A helper.\n");
    put(home, ".claude/rules/style.md", "Short sentences.\n");
    put(home, ".claude/CLAUDE.md", "Be careful.\n");
    put(home, ".claude/settings.json", r#"{"model": "opus", "statusLine": {"command": "~/bin/status"}}"#);
    let install = format!("{}/.claude/plugins/cache/market/tool/1.0", home.root().display());
    put(
        home,
        ".claude/plugins/installed_plugins.json",
        &format!(r#"{{"plugins": {{"tool@market": [{{"installPath": {}}}]}}}}"#, serde_json::to_string(&install).unwrap()),
    );
    put(home, ".claude/plugins/cache/market/tool/1.0/plugin.json", "{}");
    put(home, ".claude/plugins/marketplaces/market/.git/HEAD", "ref: refs/heads/main\n");
    put(home, ".claude/plugins/marketplaces/market/README.md", "Market.\n");
    put(home, ".claude/.credentials.json", &format!(r#"{{"claudeAiOauth": {{"accessToken": "{SECRET}"}}}}"#));
    put(home, ".claude.json", r#"{"numStartups": 9, "mcpServers": {"docs": {"command": "docs-mcp"}}, "projects": {"x": {}}}"#);
    put(home, ".claude/projects/x/session.jsonl", "{}\n");
    put(home, ".claude/history.jsonl", "{}\n");
    put(home, ".codex/AGENTS.md", "Codex rules.\n");
    put(home, ".codex/config.toml", "model = \"gpt\"\n");
    put(home, ".codex/prompts/fix.md", "Fix it.\n");
    put(home, ".codex/skills/lint/SKILL.md", "# Lint\n");
    put(home, ".codex/skills/.system/bundled/SKILL.md", "# Bundled\n");
    put(home, ".codex/auth.json", &format!(r#"{{"tokens": {{"access_token": "{SECRET}"}}}}"#));
    put(home, ".codex/sessions/one.jsonl", "{}\n");
}

#[test]
fn the_first_sync_lands_everything_and_nothing_outside_the_table() {
    let world = World::new("first");
    let (mine, far) = (world.home("here"), world.home("machine"));
    seed(&mine);
    put(&far, ".claude.json", r#"{"userID": "far", "projects": {"y": {}}}"#);
    put(&far, ".claude/projects/y/session.jsonl", "far\n");

    let synced = sync(&world.job(&mine, "far"), &far);
    assert_eq!(synced.problems, []);
    let landed = files(&far);
    for path in [
        ".claude/skills/deploy/SKILL.md",
        ".claude/skills/deploy/scripts/run.sh",
        ".claude/commands/review.md",
        ".claude/agents/helper.md",
        ".claude/rules/style.md",
        ".claude/CLAUDE.md",
        ".codex/AGENTS.md",
        ".codex/config.toml",
        ".codex/prompts/fix.md",
        ".codex/skills/lint/SKILL.md",
    ] {
        assert_eq!(landed.get(path), read(&mine, path).as_ref(), "{path}");
    }
    for path in [
        ".claude/history.jsonl",
        ".claude/projects/x/session.jsonl",
        ".claude/plugins/installed_plugins.json",
        ".claude/plugins/cache/market/tool/1.0/plugin.json",
        ".claude/plugins/marketplaces/market/README.md",
        ".claude/plugins/marketplaces/market/.git/HEAD",
        ".codex/skills/.system/bundled/SKILL.md",
        ".codex/sessions/one.jsonl",
    ] {
        assert!(!landed.contains_key(path), "{path} is not the setup");
    }
    assert_eq!(read(&far, ".claude/projects/y/session.jsonl").as_deref(), Some("far\n"));

    let settings = |home: &Home| serde_json::from_str::<serde_json::Value>(&read(home, ".claude/settings.json").unwrap()).unwrap();
    assert_eq!(settings(&far), settings(&mine));
    let claude: serde_json::Value = serde_json::from_str(&read(&far, ".claude.json").unwrap()).unwrap();
    assert_eq!(claude["mcpServers"]["docs"]["command"], "docs-mcp");
    assert_eq!(claude["userID"], "far", "the rest of it is the Machine's own");
    assert!(claude.get("numStartups").is_none() && claude["projects"].get("x").is_none());
    assert!(read(&far, ".claude/.credentials.json").unwrap().contains(SECRET));
    assert!(read(&far, ".codex/auth.json").unwrap().contains(SECRET));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let bits = |path: &str| fs::metadata(far.root().join(path)).unwrap().permissions().mode() & 0o777;
        assert_eq!(bits(".claude/.credentials.json"), 0o600);
        assert_eq!(bits(".claude"), 0o700);
        assert_eq!(bits(".codex/auth.json"), 0o600);
        assert_eq!(bits(".codex"), 0o700);
    }

    let again = sync(&world.job(&mine, "far"), &far);
    assert_eq!(again, Synced::default(), "nothing differs now");
}

#[test]
fn an_edit_an_addition_and_a_removal_on_either_side_reach_the_other() {
    let world = World::new("both-ways");
    let (mine, far) = (world.home("here"), world.home("machine"));
    seed(&mine);
    put(&mine, ".claude/commands/ship.md", "Ship it.\n");
    put(&mine, ".claude/rules/tone.md", "Plain words.\n");
    let job = world.job(&mine, "far");
    sync(&job, &far);

    put(&mine, ".claude/skills/deploy/SKILL.md", "# Deploy, edited here\n");
    put(&far, ".claude/skills/far-made/SKILL.md", "# Made on the Machine\n");
    gone(&mine, ".claude/commands/review.md");
    gone(&far, ".claude/rules/style.md");
    put(&far, ".codex/config.toml", "model = \"other\"\n");
    let synced = sync(&job, &far);
    assert_eq!((synced.pushed, synced.pulled, synced.removed_here, synced.removed_there), (1, 3, 1, 1), "{synced:?}");

    assert_eq!(read(&far, ".claude/skills/deploy/SKILL.md").as_deref(), Some("# Deploy, edited here\n"));
    assert_eq!(read(&mine, ".claude/skills/far-made/SKILL.md").as_deref(), Some("# Made on the Machine\n"));
    assert_eq!(read(&far, ".claude/commands/review.md"), None);
    assert_eq!(read(&mine, ".claude/rules/style.md"), None);
    assert_eq!(read(&mine, ".codex/config.toml").as_deref(), Some("model = \"other\"\n"));
    assert_eq!(sync(&job, &far), Synced::default());
}

#[test]
fn of_two_edits_the_newer_wins_and_the_other_is_kept_aside() {
    let world = World::new("conflict");
    let (mine, far) = (world.home("here"), world.home("machine"));
    seed(&mine);
    let job = world.job(&mine, "far");
    sync(&job, &far);

    put(&mine, ".claude/CLAUDE.md", "Edited here, earlier.\n");
    aged(&mine, ".claude/CLAUDE.md", 60);
    put(&far, ".claude/CLAUDE.md", "Edited on the Machine, later.\n");
    put(&mine, ".claude/agents/helper.md", "Edited here, later.\n");
    put(&far, ".claude/agents/helper.md", "Edited on the Machine, earlier.\n");
    aged(&far, ".claude/agents/helper.md", 60);
    let synced = sync(&job, &far);

    assert_eq!(read(&mine, ".claude/CLAUDE.md").as_deref(), Some("Edited on the Machine, later.\n"));
    assert_eq!(read(&far, ".claude/agents/helper.md").as_deref(), Some("Edited here, later.\n"));
    let kept: Vec<_> = synced.conflicts.iter().map(|kept| (kept.path.as_str(), kept.winner)).collect();
    assert_eq!(kept, [(".claude/CLAUDE.md", Side::There), (".claude/agents/helper.md", Side::Here)]);
    let losers: Vec<_> = synced.conflicts.iter().map(|kept| fs::read_to_string(&kept.kept).unwrap()).collect();
    assert_eq!(losers, ["Edited here, earlier.\n", "Edited on the Machine, earlier.\n"]);
    assert!(synced.conflicts.iter().all(|kept| kept.kept.starts_with(&job.conflicts)));
    assert_eq!(sync(&job, &far), Synced::default());
}

#[test]
fn a_sync_that_would_remove_most_of_a_side_stops_until_told_to_go_on() {
    let world = World::new("valve");
    let (mine, far) = (world.home("here"), world.home("machine"));
    seed(&mine);
    let mut job = world.job(&mine, "far");
    sync(&job, &far);
    put(&far, ".claude/projects/y/session.jsonl", "the Machine's own\n");
    let before = files(&far);

    gone(&mine, ".claude");
    gone(&mine, ".codex");
    put(&mine, ".claude.json", r#"{"numStartups": 10}"#);
    let outcome = run(&job, &mut Machine::new(&far), &mut |_| {}, &Cancellation::default()).unwrap();
    let Outcome::Stopped(valve) = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(valve.side, Side::There);
    assert!(valve.removing * 2 > valve.holding, "{valve:?}");
    assert_eq!(files(&far), before, "nothing was removed");

    job.allow_removals = true;
    sync(&job, &far);
    assert_eq!(read(&far, ".claude/CLAUDE.md"), None);
    assert!(read(&far, ".codex/auth.json").is_some(), "a sign-in is never removed");
    assert_eq!(read(&mine, ".codex/auth.json"), read(&far, ".codex/auth.json"), "and comes back here");
    assert_eq!(read(&far, ".claude/projects/y/session.jsonl").as_deref(), Some("the Machine's own\n"), "not the setup");
    let claude: serde_json::Value = serde_json::from_str(&read(&far, ".claude.json").expect("the file stays")).unwrap();
    assert!(claude["mcpServers"].get("docs").is_none(), "the key goes: {claude}");
}

#[test]
fn a_sync_cut_short_at_any_request_is_finished_by_the_next() {
    let mut cut = 0;
    loop {
        let world = World::new(&format!("cut-{cut}"));
        let (mine, far) = (world.home("here"), world.home("machine"));
        seed(&mine);
        let job = Job {
            allow_removals: true,
            ..world.job(&mine, "far")
        };
        sync(&job, &far);
        put(&mine, ".claude/skills/deploy/SKILL.md", "# Edited here\n");
        put(&mine, ".claude/skills/new/SKILL.md", "# New here\n");
        put(&far, ".claude/skills/far/SKILL.md", "# New there\n");
        put(&far, ".claude/settings.json", r#"{"model": "sonnet"}"#);
        gone(&mine, ".claude/commands");
        gone(&far, ".codex/prompts/fix.md");
        put(&mine, ".claude/CLAUDE.md", "Here's.\n");
        aged(&mine, ".claude/CLAUDE.md", 60);
        put(&far, ".claude/CLAUDE.md", "There's.\n");

        let first = run(&job, &mut Machine::cut_after(&far, Some(cut)), &mut |_| {}, &Cancellation::default());
        let finished = sync(&job, &far);
        let (here_now, there_now) = (files(&mine), files(&far));
        let setup = |files: &BTreeMap<String, String>| -> BTreeMap<String, String> {
            files
                .iter()
                .filter(|(path, _)| path.starts_with(".claude/skills") || path.starts_with(".claude/commands") || path.starts_with(".codex/prompts"))
                .map(|(path, text)| (path.clone(), text.clone()))
                .collect()
        };
        assert_eq!(setup(&here_now), setup(&there_now), "cut after {cut}: {finished:?}");
        assert_eq!(here_now.get(".claude/skills/new/SKILL.md").map(String::as_str), Some("# New here\n"));
        assert_eq!(here_now.get(".claude/skills/far/SKILL.md").map(String::as_str), Some("# New there\n"));
        assert!(!here_now.contains_key(".claude/commands/review.md") && !there_now.contains_key(".codex/prompts/fix.md"));
        assert_eq!(read(&mine, ".claude/CLAUDE.md").as_deref(), Some("There's.\n"), "cut after {cut}");
        assert_eq!(read(&far, ".claude/CLAUDE.md").as_deref(), Some("There's.\n"));
        assert_eq!(read(&mine, ".claude/settings.json").as_deref(), Some(r#"{"model": "sonnet"}"#));
        assert_eq!(sync(&job, &far), Synced::default(), "cut after {cut}");
        if first.is_ok() {
            break;
        }
        cut += 1;
    }
}

#[test]
fn the_hub_carries_one_machines_change_to_another() {
    let world = World::new("hub");
    let (mine, one, other) = (world.home("here"), world.home("one"), world.home("other"));
    seed(&mine);
    let (to_one, to_other) = (world.job(&mine, "one"), world.job(&mine, "other"));
    sync(&to_one, &one);
    sync(&to_other, &other);

    put(&one, ".claude/commands/from-one.md", "From the first Machine.\n");
    sync(&to_one, &one);
    sync(&to_other, &other);
    assert_eq!(read(&other, ".claude/commands/from-one.md").as_deref(), Some("From the first Machine.\n"));

    gone(&other, ".claude/commands/from-one.md");
    sync(&to_other, &other);
    sync(&to_one, &one);
    assert_eq!(read(&mine, ".claude/commands/from-one.md"), None);
    assert_eq!(read(&one, ".claude/commands/from-one.md"), None);
    assert_eq!(sync(&to_one, &one), Synced::default());
    assert_eq!(sync(&to_other, &other), Synced::default());
}

/// macOS's Keychain, as a map.
#[derive(Default)]
struct Fake(Mutex<BTreeMap<String, String>>, std::sync::atomic::AtomicUsize);

impl Fake {
    /// How many times a sync read it.
    fn reads(&self) -> usize {
        self.1.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Keychain for Fake {
    fn get(&self, service: &str) -> io::Result<Option<String>> {
        self.1.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(self.0.lock().unwrap().get(service).cloned())
    }

    fn set(&self, service: &str, secret: &str) -> io::Result<()> {
        self.0.lock().unwrap().insert(service.into(), secret.into());
        Ok(())
    }
}

#[test]
fn a_keychain_sign_in_goes_to_a_file_and_comes_back_to_the_keychain() {
    let world = World::new("keychain");
    let keychain = Arc::new(Fake::default());
    let mine = world.home("here").with_keychain(keychain.clone());
    let far = world.home("machine");
    let signed_in = format!(r#"{{"claudeAiOauth": {{"accessToken": "{SECRET}"}}}}"#);
    keychain.set(CLAUDE_KEYCHAIN, &signed_in).unwrap();
    let job = world.job(&mine, "far");
    sync(&job, &far);
    assert_eq!(read(&far, ".claude/.credentials.json"), Some(signed_in));
    assert_eq!(read(&mine, ".claude/.credentials.json"), None, "this computer keeps it in its Keychain");

    put(&far, ".claude/.credentials.json", r#"{"claudeAiOauth": {"accessToken": "refreshed"}}"#);
    sync(&job, &far);
    assert_eq!(keychain.get(CLAUDE_KEYCHAIN).unwrap().as_deref(), Some(r#"{"claudeAiOauth": {"accessToken": "refreshed"}}"#));
    assert_eq!(read(&mine, ".claude/.credentials.json"), None);
}

#[test]
fn a_sign_in_is_never_in_an_error_or_an_outcome() {
    let world = World::new("secret");
    let (mine, far) = (world.home("here"), world.home("machine"));
    seed(&mine);
    let job = world.job(&mine, "far");
    sync(&job, &far);
    let mut said = Vec::new();

    put(&mine, ".claude/.credentials.json", &format!(r#"{{"accessToken": "{SECRET}-2"}}"#));
    gone(&far, ".claude/.credentials.json");
    fs::create_dir_all(far.root().join(".claude/.credentials.json/in-the-way")).unwrap();
    match run(&job, &mut Machine::new(&far), &mut |_| {}, &Cancellation::default()) {
        Ok(outcome) => said.push(format!("{outcome:?} {}", serde_json::to_string(&outcome).unwrap())),
        Err(error) => said.push(format!("{error} {error:?}")),
    }
    gone(&far, ".claude/.credentials.json");

    put(&mine, ".claude/.credentials.json", &format!(r#"{{"accessToken": "{SECRET}-3"}}"#));
    put(&far, ".claude/.credentials.json", &format!(r#"{{"accessToken": "{SECRET}-4"}}"#));
    match run(&job, &mut Machine::cut_after(&far, Some(1)), &mut |_| {}, &Cancellation::default()) {
        Ok(outcome) => said.push(format!("{outcome:?}")),
        Err(error) => said.push(format!("{error} {error:?}")),
    }
    let synced = sync(&job, &far);
    said.push(format!("{synced:?} {}", serde_json::to_string(&synced).unwrap()));
    println!("{}", said.join("\n"));
    for line in &said {
        assert!(!line.contains("NEVER-SHOWN"), "{line}");
    }
    assert_eq!(synced.conflicts, [], "a sign-in's loser is never kept");
    assert!(!job.conflicts.exists());
    assert_eq!(read(&mine, ".claude/.credentials.json"), read(&far, ".claude/.credentials.json"), "the newer is on both");
}

#[test]
fn a_json_file_that_does_not_read_is_held_and_the_rest_syncs() {
    let world = World::new("torn");
    let (mine, far) = (world.home("here"), world.home("machine"));
    seed(&mine);
    let job = world.job(&mine, "far");
    sync(&job, &far);
    put(&far, ".claude/settings.json", r#"{"model": "#);
    put(&mine, ".claude/settings.json", r#"{"model": "haiku"}"#);
    put(&mine, ".claude/CLAUDE.md", "Still syncs.\n");
    let synced = sync(&job, &far);
    assert_eq!(read(&far, ".claude/settings.json").as_deref(), Some(r#"{"model": "#));
    assert_eq!(read(&far, ".claude/CLAUDE.md").as_deref(), Some("Still syncs.\n"));
    assert_eq!(synced.problems.iter().map(|problem| (problem.path.as_str(), problem.side)).collect::<Vec<_>>(), [(".claude/settings.json", Side::There)]);
}

#[cfg(unix)]
#[test]
fn a_linked_skill_is_sent_as_its_contents_and_never_written_through() {
    let world = World::new("linked");
    let (mine, far) = (world.home("here"), world.home("machine"));
    put(&mine, "elsewhere/shared/SKILL.md", "# Shared\n");
    put(&mine, ".claude/skills/own/SKILL.md", "# Own\n");
    std::os::unix::fs::symlink(mine.root().join("elsewhere/shared"), mine.root().join(".claude/skills/shared")).unwrap();
    let job = world.job(&mine, "far");
    sync(&job, &far);
    assert_eq!(read(&far, ".claude/skills/shared/SKILL.md").as_deref(), Some("# Shared\n"));
    assert!(!fs::symlink_metadata(far.root().join(".claude/skills/shared")).unwrap().file_type().is_symlink());

    put(&far, ".claude/skills/shared/SKILL.md", "# Edited on the Machine\n");
    let synced = sync(&job, &far);
    assert_eq!(read(&mine, "elsewhere/shared/SKILL.md").as_deref(), Some("# Shared\n"), "never written through");
    assert_eq!(synced.held.get(".claude/skills/shared/SKILL.md"), Some(&Hold::Linked(Side::Here)));
}

#[test]
fn a_path_outside_the_table_is_refused_by_the_far_side() {
    let world = World::new("outside");
    let far = world.home("machine");
    put(&far, ".claude/history.jsonl", "private\n");
    let mut machine = Machine::new(&far);
    for path in [".claude/history.jsonl", ".claude/skills", ".codex/skills/.system/x", ".claude/plugins/m/.git/HEAD", ".ssh/id_rsa"] {
        let read = machine.call(Request::Read { root: tree::Root::SETUP, path: path.into(), offset: 0 });
        assert!(read.is_err(), "{path}");
        let remove = machine.call(Request::Remove { root: tree::Root::SETUP, path: path.into() });
        assert!(remove.is_err(), "{path}");
    }
    assert_eq!(read(&far, ".claude/history.jsonl").as_deref(), Some("private\n"));
}

/// A link at `at` to the folder `target`, there or not: a symlink, or on
/// Windows a junction, which needs no privilege.
fn link_dir(target: &Path, at: &Path) {
    fs::create_dir_all(at.parent().unwrap()).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, at).unwrap();
    #[cfg(windows)]
    {
        let made = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(at).arg(target).output().unwrap();
        assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    }
}

fn at(home: &Home, path: &str) -> PathBuf {
    path.split('/').fold(home.root().to_path_buf(), |at, part| at.join(part))
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
}

fn stopped(job: &Job, machine: &Home) -> Valve {
    match run(job, &mut Machine::new(machine), &mut |_| {}, &Cancellation::default()).unwrap() {
        Outcome::Stopped(valve) => valve,
        outcome => panic!("{outcome:?}"),
    }
}

#[test]
fn a_pull_of_hundreds_of_files_lands_whole() {
    let world = World::new("many");
    let (mine, far) = (world.home("here"), world.home("machine"));
    for at in 0..300 {
        put(&far, &format!(".claude/skills/many/{at:03}.md"), &format!("file {at}\n"));
    }
    let job = world.job(&mine, "far");
    let synced = sync(&job, &far);
    assert_eq!(synced.pulled, 301, "{synced:?}");
    assert_eq!(files(&mine), files(&far));

    for name in ["SKILL.md", "a.md", "b.md", "c.md", "d.md"] {
        put(&far, &format!(".claude/skills/five/{name}"), name);
    }
    assert_eq!(sync(&job, &far).pulled, 6);
    assert_eq!(read(&mine, ".claude/skills/five/d.md").as_deref(), Some("d.md"));
}

#[test]
fn a_sync_a_change_here_started_carries_what_changed_there_too() {
    let world = World::new("here-and-there");
    let (mine, far) = (world.home("here"), world.home("machine"));
    seed(&mine);
    sync(&world.job(&mine, "far"), &far);
    let local = Job {
        only_if_changed_here: true,
        read_sign_ins: false,
        ..world.job(&mine, "far")
    };
    put(&mine, ".claude/CLAUDE.md", "Edited here.\n");
    put(&far, ".claude/commands/there.md", "Made there.\n");
    let synced = sync(&local, &far);
    assert_eq!((synced.pushed, synced.pulled), (1, 1), "{synced:?}");
    assert_eq!(read(&far, ".claude/CLAUDE.md").as_deref(), Some("Edited here.\n"));
    assert_eq!(read(&mine, ".claude/commands/there.md").as_deref(), Some("Made there.\n"));

    put(&far, ".claude/commands/later.md", "Later.\n");
    assert_eq!(sync(&local, &far), Synced::default(), "nothing changed here: the check there is the clock's");
    assert_eq!(read(&mine, ".claude/commands/later.md"), None);
}

#[test]
fn losing_every_skill_stops_though_other_skills_outnumber_them_and_what_goes_here_is_kept_in_the_trash() {
    let world = World::new("valve-item");
    let (mine, far) = (world.home("here"), world.home("machine"));
    seed(&mine);
    for at in 0..60 {
        put(&mine, &format!(".agents/skills/big/{at}.md"), "skill\n");
    }
    put(&mine, ".claude/commands/a.md", "a\n");
    put(&mine, ".claude/commands/b.md", "b\n");
    let mut job = world.job(&mine, "far");
    sync(&job, &far);
    let before = files(&mine);

    gone(&far, ".claude/skills");
    let valve = stopped(&job, &far);
    assert_eq!(
        valve,
        Valve { side: Side::Here, item: ".claude/skills".into(), removing: 2, holding: 2, vanished: true }
    );
    assert_eq!(files(&mine), before, "nothing was removed");

    put(&far, ".claude/skills/deploy/SKILL.md", "# Deploy\n");
    put(&far, ".claude/skills/deploy/scripts/run.sh", "#!/bin/sh\necho deploy\n");
    gone(&far, ".claude/commands/a.md");
    gone(&far, ".claude/commands/b.md");
    let valve = stopped(&job, &far);
    assert_eq!((valve.item.as_str(), valve.removing, valve.holding, valve.vanished), (".claude/commands", 2, 3, false));

    let old = job.trash.join("20200101-000000-000");
    fs::create_dir_all(old.join(".claude")).unwrap();
    job.allow_removals = true;
    let synced = sync(&job, &far);
    assert_eq!(read(&mine, ".claude/commands/a.md"), None);
    let trashed = synced.trashed.expect("a trash folder");
    assert!(trashed.starts_with(&job.trash));
    let kept = |path: &str| fs::read_to_string(path.split('/').fold(trashed.clone(), |at, part| at.join(part))).ok();
    assert_eq!(kept("claude/commands/a.md").as_deref(), Some("a\n"), "in a folder a file browser shows");
    assert_eq!(kept("claude/commands/b.md").as_deref(), Some("b\n"));
    assert!(!old.exists(), "a sync's trash older than 14 days is pruned");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&trashed).unwrap().permissions().mode() & 0o777, 0o700);
    }
}

#[test]
fn an_item_behind_a_link_to_nothing_removes_nothing_and_says_why() {
    let world = World::new("dangling");
    let (mine, far) = (world.home("here"), world.home("machine"));
    seed(&mine);
    let job = world.job(&mine, "far");
    sync(&job, &far);
    let before = files(&mine);

    gone(&far, ".claude/skills");
    link_dir(&far.root().join("unmounted").join("skills"), &at(&far, ".claude/skills"));
    gone(&far, ".codex/prompts");
    link_dir(&far.root().join("unmounted").join("prompts"), &at(&far, ".codex/prompts"));
    let synced = sync(&job, &far);
    assert_eq!(files(&mine), before, "{synced:?}");
    let held: Vec<_> = synced.problems.iter().map(|problem| (problem.path.as_str(), problem.side)).collect();
    assert_eq!(held, [(".claude/skills", Side::There), (".codex/prompts", Side::There)]);
    assert!(synced.problems[0].error.contains("a link to something not there"), "{}", synced.problems[0].error);
}

#[test]
fn removing_a_linked_folder_takes_the_link_never_what_it_points_at_both_ways() {
    let world = World::new("link-removal");
    let (mine, far) = (world.home("here"), world.home("machine"));
    for name in ["a", "b", "c"] {
        put(&mine, &format!(".claude/skills/{name}/SKILL.md"), name);
    }
    put(&mine, "elsewhere/foo/SKILL.md", "# Foo\n");
    link_dir(&at(&mine, "elsewhere/foo"), &at(&mine, ".claude/skills/foo"));
    put(&far, "elsewhere/bar/SKILL.md", "# Bar\n");
    link_dir(&at(&far, "elsewhere/bar"), &at(&far, ".claude/skills/bar"));
    let job = world.job(&mine, "far");
    sync(&job, &far);
    assert_eq!(read(&far, ".claude/skills/foo/SKILL.md").as_deref(), Some("# Foo\n"));
    assert_eq!(read(&mine, ".claude/skills/bar/SKILL.md").as_deref(), Some("# Bar\n"));
    assert!(!is_link(&at(&far, ".claude/skills/foo")) && !is_link(&at(&mine, ".claude/skills/bar")));

    gone(&far, ".claude/skills/foo");
    gone(&mine, ".claude/skills/bar");
    let synced = sync(&job, &far);
    assert_eq!((synced.removed_here, synced.removed_there, synced.held.len()), (1, 1, 0), "{synced:?}");
    assert!(!is_link(&at(&mine, ".claude/skills/foo")) && !at(&mine, ".claude/skills/foo").exists());
    assert!(!is_link(&at(&far, ".claude/skills/bar")) && !at(&far, ".claude/skills/bar").exists());
    assert_eq!(read(&mine, "elsewhere/foo/SKILL.md").as_deref(), Some("# Foo\n"), "what the link pointed at stays");
    assert_eq!(read(&far, "elsewhere/bar/SKILL.md").as_deref(), Some("# Bar\n"));
    assert_eq!(sync(&job, &far), Synced::default(), "converged");
}

#[test]
fn two_sign_ins_with_none_agreed_wait_for_the_operator() {
    let world = World::new("sign-ins");
    let (mine, far) = (world.home("here"), world.home("machine"));
    seed(&mine);
    gone(&mine, ".codex/auth.json");
    let (theirs, codex) = (r#"{"claudeAiOauth": {"account": "far"}}"#, r#"{"tokens": {"account": "far"}}"#);
    put(&far, ".claude/.credentials.json", theirs);
    put(&far, ".codex/auth.json", codex);
    let mine_before = read(&mine, ".claude/.credentials.json");
    let job = world.job(&mine, "far");
    for _ in 0..2 {
        let synced = sync(&job, &far);
        assert_eq!(synced.held.get(".claude/.credentials.json"), Some(&Hold::SignIn), "{synced:?}");
        assert_eq!(read(&mine, ".claude/.credentials.json"), mine_before);
        assert_eq!(read(&far, ".claude/.credentials.json").as_deref(), Some(theirs));
    }
    assert_eq!(read(&mine, ".codex/auth.json").as_deref(), Some(codex), "a side with none takes the other's");

    let chosen = Job {
        sign_ins: Some(Side::There),
        ..world.job(&mine, "far")
    };
    sync(&chosen, &far);
    assert_eq!(read(&mine, ".claude/.credentials.json").as_deref(), Some(theirs));
    assert_eq!(sync(&job, &far), Synced::default());
}

#[test]
fn a_sign_in_read_mid_write_waits_for_the_next_sync_unsaid() {
    let world = World::new("torn-sign-in");
    let (mine, far) = (world.home("here"), world.home("machine"));
    seed(&mine);
    let job = world.job(&mine, "far");
    sync(&job, &far);
    let before = read(&far, ".claude/.credentials.json");
    put(&mine, ".claude/.credentials.json", &format!(r#"{{"claudeAiOauth": {{"accessToken": "{SECRET}-torn"#));
    let synced = sync(&job, &far);
    assert_eq!(read(&far, ".claude/.credentials.json"), before);
    assert_eq!(synced, Synced::default(), "held, and not named");
}

/// Each file under `dir` that holds a sign-in's secret.
fn secrets(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(secrets(&path));
        } else if fs::read_to_string(&path).is_ok_and(|text| text.contains("NEVER-SHOWN")) {
            found.push(path);
        }
    }
    found
}

/// Signing out on one side while the other is signed in signs it back in
/// at the next sync: no stop, no trash, either side, either agent.
#[test]
fn signing_out_on_either_side_signs_it_back_in_and_nothing_stops() {
    let world = World::new("sign-out");
    let keychain = Arc::new(Fake::default());
    let (mine, far) = (world.home("here").with_keychain(keychain.clone()), world.home("machine"));
    seed(&mine);
    gone(&mine, ".claude/.credentials.json");
    let claude = format!(r#"{{"claudeAiOauth": {{"accessToken": "{SECRET}"}}}}"#);
    keychain.set(CLAUDE_KEYCHAIN, &claude).unwrap();
    let job = world.job(&mine, "far");
    sync(&job, &far);
    let codex = read(&mine, ".codex/auth.json");

    gone(&far, ".claude/.credentials.json");
    gone(&far, ".codex/auth.json");
    let synced = sync(&job, &far);
    assert_eq!((synced.pushed, synced.removed_here), (2, 0), "{synced:?}");
    assert_eq!(read(&far, ".claude/.credentials.json"), Some(claude.clone()), "signed back in there");
    assert_eq!(read(&far, ".codex/auth.json"), codex);

    keychain.0.lock().unwrap().clear();
    gone(&mine, ".codex/auth.json");
    let synced = sync(&job, &far);
    assert_eq!((synced.pulled, synced.removed_there), (2, 0), "{synced:?}");
    assert_eq!(keychain.get(CLAUDE_KEYCHAIN).unwrap(), Some(claude), "signed back in here");
    assert_eq!(read(&mine, ".codex/auth.json"), codex);
    assert_eq!(sync(&job, &far), Synced::default());

    keychain.0.lock().unwrap().clear();
    for path in [".claude/.credentials.json", ".codex/auth.json"] {
        gone(&far, path);
    }
    gone(&mine, ".codex/auth.json");
    assert_eq!(sync(&job, &far), Synced::default(), "signed out on both sides is nothing");
    assert_eq!(sync(&job, &far), Synced::default());
    assert!(!job.trash.exists() && !job.conflicts.exists());
}

/// No code path writes a sign-in to the trash or to a conflict copy: the
/// store refuses to trash or remove one, here or asked by the hub, and the
/// syncs that remove most and keep losers aside leave none there.
#[test]
fn no_sign_in_is_ever_put_in_the_trash_or_kept_as_a_conflict() {
    let world = World::new("sign-in-kept-nowhere");
    let keychain = Arc::new(Fake::default());
    let (mine, far) = (world.home("here"), world.home("machine"));
    let keyed = world.home("keyed").with_keychain(keychain.clone());
    keychain.set(CLAUDE_KEYCHAIN, &format!(r#"{{"accessToken": "{SECRET}"}}"#)).unwrap();
    seed(&mine);
    put(&keyed, ".codex/auth.json", &format!(r#"{{"tokens": "{SECRET}"}}"#));
    let by_hand = world.root.join("trash-by-hand");
    for home in [&mine, &keyed] {
        for path in [".claude/.credentials.json", ".codex/auth.json"] {
            assert!(home.trash(path, &by_hand).is_err(), "{path}");
            assert!(home.remove(path).is_err(), "{path}");
        }
    }
    assert!(keychain.get(CLAUDE_KEYCHAIN).unwrap().is_some() && read(&mine, ".claude/.credentials.json").is_some());
    assert!(!by_hand.exists());

    let job = Job {
        allow_removals: true,
        ..world.job(&mine, "far")
    };
    sync(&job, &far);
    for path in [".claude/.credentials.json", ".codex/auth.json"] {
        let removed = Machine::new(&far).call(Request::Remove { root: tree::Root::SETUP, path: path.into() });
        assert!(removed.is_err(), "the Machine refuses too: {path}");
    }

    gone(&far, ".claude");
    gone(&far, ".codex");
    let synced = sync(&job, &far);
    assert!(synced.trashed.is_some(), "the rest of this side went to the trash: {synced:?}");
    gone(&mine, ".claude");
    gone(&mine, ".codex");
    sync(&job, &far);
    for path in [".claude/.credentials.json", ".codex/auth.json"] {
        assert!(read(&mine, path).is_some_and(|text| text.contains("NEVER-SHOWN")), "{path} came back here");
    }

    put(&mine, ".claude/.credentials.json", &format!(r#"{{"accessToken": "{SECRET}-here"}}"#));
    aged(&mine, ".claude/.credentials.json", 60);
    put(&far, ".claude/.credentials.json", &format!(r#"{{"accessToken": "{SECRET}-there"}}"#));
    let synced = sync(&job, &far);
    assert_eq!(synced.conflicts, [], "{synced:?}");
    assert_eq!(read(&mine, ".claude/.credentials.json"), read(&far, ".claude/.credentials.json"));
    assert_eq!(secrets(&job.trash), Vec::<PathBuf>::new());
    assert_eq!(secrets(&job.conflicts), Vec::<PathBuf>::new());
}

#[test]
fn hooks_output_styles_and_the_status_line_sync_and_claude_config_dir_moves_claude() {
    let world = World::new("table");
    let (mine, far) = (world.home("here"), world.home("machine"));
    put(&mine, ".claude/hooks/check.js", "console.log(1)\n");
    put(&mine, ".claude/output-styles/terse.md", "Be terse.\n");
    put(&mine, ".claude/statusline.sh", "#!/bin/sh\necho ok\n");
    put(&mine, ".codex/rules/default.rules", "prefix_rule(pattern = [\"ls\"])\n");
    put(&mine, ".claude.json", r#"{"mcpServers": {"docs": {"command": "docs-mcp"}}}"#);
    let moved = far.clone().with_claude(far.root().join("work-claude"));
    sync(&world.job(&mine, "far"), &moved);
    let there = |path: &str| fs::read_to_string(path.split('/').fold(far.root().join("work-claude"), |at, part| at.join(part))).ok();
    assert_eq!(there("hooks/check.js").as_deref(), Some("console.log(1)\n"));
    assert_eq!(there("output-styles/terse.md").as_deref(), Some("Be terse.\n"));
    assert_eq!(there("statusline.sh").as_deref(), Some("#!/bin/sh\necho ok\n"));
    assert!(there(".claude.json").is_some_and(|json| json.contains("docs-mcp")), "in CLAUDE_CONFIG_DIR");
    assert!(!far.root().join(".claude").exists() && !far.root().join(".claude.json").exists());
    assert_eq!(read(&far, ".codex/rules/default.rules").as_deref(), Some("prefix_rule(pattern = [\"ls\"])\n"));
}

#[test]
fn a_conflict_cut_short_keeps_one_copy() {
    let world = World::new("kept-once");
    let (mine, far) = (world.home("here"), world.home("machine"));
    seed(&mine);
    let job = world.job(&mine, "far");
    sync(&job, &far);
    put(&mine, ".claude/CLAUDE.md", "Edited here, earlier.\n");
    aged(&mine, ".claude/CLAUDE.md", 60);
    put(&far, ".claude/CLAUDE.md", "Edited on the Machine, later.\n");
    for at in 0..600 {
        put(&mine, &format!(".claude/skills/bulk/{at}.md"), "bulk\n");
    }
    let cut = run(&job, &mut Machine::cut_after(&far, Some(40)), &mut |_| {}, &Cancellation::default());
    assert!(cut.is_err(), "{cut:?}");
    let synced = sync(&job, &far);
    assert_eq!(synced.conflicts.len(), 1, "{synced:?}");
    let copies = fs::read_dir(&job.conflicts)
        .unwrap()
        .flatten()
        .filter(|sync| sync.path().join("claude").join("CLAUDE.md").exists())
        .count();
    assert_eq!(copies, 1, "one kept copy");
    assert_eq!(fs::read_to_string(&synced.conflicts[0].kept).unwrap(), "Edited here, earlier.\n");
}

#[test]
fn codex_config_new_from_a_side_without_bits_is_owner_only() {
    let world = World::new("config-bits");
    let home = world.home("here");
    assert_eq!(home.receive(".codex/config.toml#model", None).unwrap().mode, Some(0o600));
    assert_eq!(home.receive(".codex/AGENTS.md", None).unwrap().mode, None, "instructions take the umask");
}

#[test]
fn a_name_this_computer_cannot_hold_is_held_alone() {
    let paths = [
        ".claude/skills/a/aux.md",
        ".claude/skills/b?/x.md",
        ".claude/skills/c./x.md",
        ".claude/skills/Case.md",
        ".claude/skills/case.md",
        ".claude/skills/ok.md",
        ".claude/skills/Mine.md",
    ];
    let manifest = |paths: &[&str]| tree::Manifest {
        entries: paths
            .iter()
            .map(|path| (path.to_string(), tree::Entry::File { size: 0, sha256: String::new(), mode: None, modified: None, text: None }))
            .collect(),
        ..tree::Manifest::default()
    };
    let here = manifest(&[".claude/skills/mine.md"]);
    let mut far = manifest(&paths);
    super::run::unholdable(&mut far, &here, Names::Windows);
    assert_eq!(
        far.problems.keys().map(String::as_str).collect::<Vec<_>>(),
        [".claude/skills/Mine.md", ".claude/skills/a/aux.md", ".claude/skills/b?/x.md", ".claude/skills/c./x.md", ".claude/skills/case.md"]
    );
    let mut case_only = manifest(&paths[3..5]);
    super::run::unholdable(&mut case_only, &here, Names::Any);
    assert!(case_only.problems.is_empty(), "Linux holds both");
}

#[cfg(unix)]
#[test]
fn a_machine_name_windows_cannot_hold_does_not_stop_the_rest() {
    let world = World::new("names");
    let (mine, far) = (world.home("here"), world.home("machine"));
    put(&far, ".claude/skills/what?/SKILL.md", "# What\n");
    put(&far, ".claude/skills/good/SKILL.md", "# Good\n");
    let job = Job { names: Names::Windows, ..world.job(&mine, "far") };
    let synced = sync(&job, &far);
    assert_eq!(read(&mine, ".claude/skills/good/SKILL.md").as_deref(), Some("# Good\n"));
    assert_eq!(synced.problems.iter().map(|problem| problem.path.as_str()).collect::<Vec<_>>(), [".claude/skills/what?"]);
}

#[test]
fn a_sync_that_finds_another_under_way_is_busy_not_failed() {
    let world = World::new("busy");
    let (mine, far) = (world.home("here"), world.home("machine"));
    let job = world.job(&mine, "far");
    fs::create_dir_all(&job.dir).unwrap();
    let held = fs::File::create(job.dir.join("setup-sync.lock")).unwrap();
    held.lock().unwrap();
    let outcome = run(&job, &mut Machine::new(&far), &mut |_| {}, &Cancellation::default()).unwrap();
    assert_eq!(outcome, Outcome::Busy);
}

#[test]
fn a_temporary_file_a_crash_left_beside_the_settings_is_swept() {
    let world = World::new("sweep");
    let (mine, far) = (world.home("here"), world.home("machine"));
    put(&mine, ".claude/settings.json", "{}");
    let left = |dir: PathBuf, name: &str, minutes: u64| {
        let path = dir.join(name);
        fs::write(&path, "torn").unwrap();
        let when = SystemTime::now() - Duration::from_secs(minutes * 60);
        fs::File::options().write(true).open(&path).unwrap().set_modified(when).unwrap();
        path
    };
    let stale = left(mine.root().join(".claude"), ".ferrite-1-2-3.part", 11);
    let fresh = left(mine.root().join(".claude"), &format!(".ferrite-{}-5-6.part", std::process::id()), 0);
    let in_home = left(mine.root().to_path_buf(), ".ferrite-7-8-9.part", 11);
    sync(&world.job(&mine, "far"), &far);
    assert!(!stale.exists() && !in_home.exists());
    assert!(fresh.exists(), "a write may still be under way");
}

#[test]
fn a_sync_a_change_here_started_never_reads_the_keychain() {
    let world = World::new("keychain-reads");
    let keychain = Arc::new(Fake::default());
    let mine = world.home("here").with_keychain(keychain.clone());
    let far = world.home("machine");
    let (first, refreshed) = (r#"{"claudeAiOauth": {"accessToken": "first"}}"#, r#"{"claudeAiOauth": {"accessToken": "refreshed"}}"#);
    keychain.set(CLAUDE_KEYCHAIN, first).unwrap();
    put(&mine, ".claude/CLAUDE.md", "Be careful.\n");
    sync(&world.job(&mine, "far"), &far);
    assert!(keychain.reads() > 0);
    assert_eq!(read(&far, ".claude/.credentials.json").as_deref(), Some(first));

    let local = Job {
        only_if_changed_here: true,
        read_sign_ins: false,
        ..world.job(&mine, "far")
    };
    let reads = keychain.reads();
    put(&mine, ".claude/CLAUDE.md", "Edited.\n");
    put(&far, ".claude/.credentials.json", refreshed);
    let synced = sync(&local, &far);
    assert_eq!(keychain.reads(), reads, "{synced:?}");
    assert_eq!(read(&far, ".claude/CLAUDE.md").as_deref(), Some("Edited.\n"));
    assert_eq!(keychain.0.lock().unwrap().get(CLAUDE_KEYCHAIN).map(String::as_str), Some(first));

    sync(&world.job(&mine, "far"), &far);
    assert_eq!(keychain.0.lock().unwrap().get(CLAUDE_KEYCHAIN).map(String::as_str), Some(refreshed));
}

#[test]
fn a_file_that_changes_while_it_crosses_is_left_to_the_next_sync() {
    let world = World::new("changed-crossing");
    let (mine, far) = (world.home("here"), world.home("machine"));
    seed(&mine);
    let job = world.job(&mine, "far");
    sync(&job, &far);
    let agreed = || -> tree::Manifest { serde_json::from_slice(&fs::read(job.dir.join(BASE)).unwrap()).unwrap() };
    let edited = ".claude/skills/deploy/SKILL.md";
    for (round, writer, side) in [(0, &mine, Side::Here), (1, &far, Side::There)] {
        let added = format!(".claude/skills/fresh-{round}/SKILL.md");
        let before = agreed();
        put(writer, edited, &format!("edited {round}
"));
        put(writer, &added, "added
");
        let mut machine = Machine::new(&far);
        let (changing, again, newest) = (writer.clone(), added.clone(), format!("changed while it crossed {round}
"));
        machine.before = Box::new(move |request| {
            if let Request::Begin { path, .. } | Request::Read { path, .. } = request {
                if *path == edited || *path == again {
                    put(&changing, path, &newest);
                }
            }
        });
        match run(&job, &mut machine, &mut |_| {}, &Cancellation::default()) {
            Ok(Outcome::Synced(synced)) => {
                let left: Vec<_> = synced.problems.iter().map(|problem| (problem.path.as_str(), problem.side)).collect();
                assert_eq!(left, [(edited, side), (added.as_str(), side)]);
            }
            Err(error) => {
                assert_eq!(side, Side::Here, "only a send may stop at it: {error}");
                let stuck = error.get_ref().and_then(|inner| inner.downcast_ref::<tree::Stuck>()).expect("it names where");
                assert!([edited, added.as_str()].contains(&stuck.path.as_str()), "{error}");
                assert_eq!(stuck.side, Side::Here, "{error}");
            }
            outcome => panic!("{outcome:?}"),
        }
        let base = agreed();
        assert_eq!(base.entries.get(edited), before.entries.get(edited), "round {round}");
        assert_eq!(base.entries.get(&added), None, "round {round}");

        sync(&job, &far);
        assert_eq!(read(&mine, edited), read(&far, edited), "round {round}");
        assert_eq!(read(&mine, edited), Some(format!("changed while it crossed {round}
")), "round {round}");
        assert!(read(&mine, &added).is_some() && read(&mine, &added) == read(&far, &added), "round {round}");
        assert_eq!(sync(&job, &far), Synced::default(), "round {round}");
    }
}

/// `~/.claude/skills/<name>` as a skill installer leaves it: a link into
/// `~/.agents/skills/<name>`, relative on Unix, a junction on Windows.
fn link_skill(home: &Home, name: &str) {
    let at = at(home, &format!(".claude/skills/{name}"));
    #[cfg(unix)]
    {
        fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(format!("../../.agents/skills/{name}"), &at).unwrap();
    }
    #[cfg(windows)]
    link_dir(&home.root().join(".agents").join("skills").join(name), &at);
}

/// Every file under `home/path`, by its path from there, with its bytes.
fn bytes_under(home: &Home, path: &str) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, prefix: &str, into: &mut BTreeMap<String, Vec<u8>>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = format!("{prefix}/{name}");
            if entry.path().is_dir() {
                walk(&entry.path(), &path, into);
            } else {
                into.insert(path, fs::read(entry.path()).unwrap());
            }
        }
    }
    let mut into = BTreeMap::new();
    walk(&at(home, path), path, &mut into);
    into
}

/// What a home's Claude and Codex have on: the keys of `enabledPlugins`
/// set true, and each `[plugins.*]` table of Codex's config.
fn plugins_on(home: &Home) -> (BTreeSet<String>, BTreeSet<String>) {
    let settings: serde_json::Value = serde_json::from_str(&read(home, ".claude/settings.json").unwrap()).unwrap();
    let claude = settings["enabledPlugins"]
        .as_object()
        .map(|on| on.iter().filter(|(_, on)| **on == true).map(|(id, _)| id.clone()).collect())
        .unwrap_or_default();
    let config = read(home, ".codex/config.toml").unwrap();
    let codex = config
        .lines()
        .filter_map(|line| line.strip_prefix("[plugins.\"")?.strip_suffix("\"]"))
        .map(str::to_string)
        .collect();
    (claude, codex)
}

/// A home shaped like the operator's Windows computer's (`windows`) or
/// Linux Machine's: its agents' plugin downloads and bookkeeping with that
/// side's line ends and paths, Codex's config holding that side's own
/// places, Claude's account sync under `skills/synced`, and skills in
/// `~/.agents/skills` linked from `~/.claude/skills`.
fn shaped(home: &Home, windows: bool) {
    let (end, user, model, own_skill, own_plugin) = match windows {
        true => ("\r\n", r"C:\\Users\\win", "gpt-a", "made-on-windows", "pstack@cursor-plugins"),
        false => ("\n", "/home/ubuntu", "gpt-b", "made-on-linux", "sentry@cursor-plugins"),
    };
    let sep = if windows { r"\\" } else { "/" };
    let lines = |text: &[&str]| text.iter().map(|line| format!("{line}{end}")).collect::<String>();
    put(home, ".claude/plugins/cache/cursor-plugins/pstack/0.15.48/README.md", &lines(&["# pstack", "", "Skills."]));
    put(home, ".claude/plugins/cache/cursor-plugins/pstack/0.15.48/skills/a/SKILL.md", &lines(&["---", "name: a", "---"]));
    put(home, ".claude/plugins/marketplaces/cursor-plugins/README.md", &lines(&["# Cursor plugins"]));
    put(home, ".claude/plugins/marketplaces/cursor-plugins/.git/HEAD", "ref: refs/heads/main\n");
    let cache = format!("{user}{sep}.claude{sep}plugins{sep}cache{sep}cursor-plugins{sep}pstack{sep}0.15.48");
    put(home, ".claude/plugins/installed_plugins.json", &format!(r#"{{"version": 2, "plugins": {{"pstack@cursor-plugins": [{{"scope": "user", "installPath": "{cache}", "installedAt": "{model}"}}]}}}}"#));
    put(home, ".claude/plugins/known_marketplaces.json", &format!(r#"{{"cursor-plugins": {{"installLocation": "{user}{sep}m", "lastUpdated": "{model}"}}}}"#));
    put(home, ".claude/plugins/.last_inuse_sweep", model);
    put(home, ".claude/plugins/plugin-directory-cache-v2.json", &format!(r#"{{"at": "{model}"}}"#));
    put(home, ".claude/plugins/store/cc-plugin.json", &format!(r#"{{"at": "{model}"}}"#));
    put(home, ".claude/plugins/data/pstack/state.json", &format!(r#"{{"at": "{model}"}}"#));
    put(home, ".claude/plugins/synced/manifest.json", &format!(r#"{{"round": "{model}"}}"#));
    put(home, ".claude/skills/synced/manifest.json", &format!(r#"{{"round": "{model}"}}"#));
    put(home, ".claude/skills/synced/.last-complete-round", model);
    put(home, ".claude/skills/synced/.bucket-7f", model);
    put(
        home,
        ".claude/settings.json",
        &format!(
            r#"{{"model": "opus", "enabledPlugins": {{"{own_plugin}": true}}, "extraKnownMarketplaces": {{"cursor-plugins": {{"source": {{"source": "github", "repo": "cursor/plugins"}}}}}}}}"#
        ),
    );
    let place = |at: &str| if windows { format!(r"C:\\Users\\win\\{at}") } else { format!("/home/ubuntu/{at}") };
    put(
        home,
        ".codex/config.toml",
        &format!(
            "# Codex, set up by hand\nmodel = \"{model}\" # the one I like\nnotify = [\"{notify}\"]\n\n[projects.\"{project}\"]\ntrust_level = \"trusted\"\n\n[[skills.config]]\npath = \"{skill}\"\nenabled = true\n\n[plugins.\"{own_plugin}\"]\nenabled = true\n\n[marketplaces.cursor-plugins]\nsource_type = \"git\"\nsource = \"https://github.com/cursor/plugins.git\"\n",
            notify = place("bin/notify"),
            project = place("code"),
            skill = place(".agents/skills/shared"),
        ),
    );
    put(home, ".agents/skills/shared/SKILL.md", &lines(&["# Shared", "", "On both."]));
    put(home, &format!(".agents/skills/{own_skill}/SKILL.md"), &lines(&[&format!("# Made on {model}")]));
    put(home, ".agents/.skill-lock.json", &format!(r#"{{"at": "{model}"}}"#));
    link_skill(home, "shared");
    link_skill(home, own_skill);
    put(home, ".claude/CLAUDE.md", &lines(&["Be careful."]));
}

/// The first real run of Setup sync, between a Windows computer and a
/// Linux Machine, as a test: what each side's agents keep for themselves
/// never moves or conflicts, Codex's own places stay each side's, what the
/// operator authored and chose reaches both, and a third Machine gets only
/// that.
#[test]
fn a_windows_home_and_a_linux_home_share_what_the_operator_chose_and_nothing_else() {
    let world = World::new("acceptance");
    let (windows, linux) = (world.home("windows"), world.home("linux"));
    shaped(&windows, true);
    shaped(&linux, false);
    let kept_own = |home: &Home| {
        let mut own = bytes_under(home, ".claude/plugins");
        own.extend(bytes_under(home, ".claude/skills/synced"));
        own.extend(bytes_under(home, ".agents/.skill-lock.json"));
        own
    };
    let (windows_own, linux_own) = (kept_own(&windows), kept_own(&linux));
    let config = |home: &Home| read(home, ".codex/config.toml").unwrap();
    let job = world.job(&windows, "linux");

    let first = sync(&job, &linux);
    assert_eq!(first.conflicts, [], "{first:?}");
    assert_eq!(kept_own(&windows), windows_own, "nothing of the agents' own moved here");
    assert_eq!(kept_own(&linux), linux_own, "nor on the Machine");
    for (home, theirs, own) in [(&windows, "/home/ubuntu", r"C:\\Users\\win"), (&linux, r"C:\\Users\\win", "/home/ubuntu")] {
        let config = config(home);
        assert!(!config.contains(theirs), "{config}");
        for own in [format!("notify = [\"{own}"), format!("[projects.\"{own}"), format!("path = \"{own}")] {
            assert!(config.contains(&own), "{own} is still in\n{config}");
        }
        assert!(config.starts_with("# Codex, set up by hand\nmodel = \"gpt-a\" # the one I like\n"), "this computer's model, the comments kept:\n{config}");
    }
    for home in [&windows, &linux] {
        for skill in ["shared", "made-on-windows", "made-on-linux"] {
            let text = read(home, &format!(".claude/skills/{skill}/SKILL.md")).unwrap_or_default();
            assert!(text.starts_with("# "), "{skill} is usable on {}", home.root().display());
            assert_eq!(read(home, &format!(".agents/skills/{skill}/SKILL.md")), Some(text));
        }
    }
    let (claude, codex) = plugins_on(&windows);
    assert_eq!((claude.len(), codex.len()), (2, 2), "{claude:?} {codex:?}");
    assert_eq!(plugins_on(&linux), (claude, codex));
    assert_eq!(sync(&job, &linux), Synced::default(), "a second sync moves nothing");

    put(&windows, ".agents/skills/made-on-linux/SKILL.md", "# Edited on Windows\r\n");
    put(&linux, ".agents/skills/made-on-windows/SKILL.md", "# Edited on Linux\n");
    let edited = sync(&job, &linux);
    assert_eq!((edited.pushed, edited.pulled, edited.conflicts.len()), (1, 1, 0), "{edited:?}");
    assert_eq!(read(&linux, ".claude/skills/made-on-linux/SKILL.md").as_deref(), Some("# Edited on Windows\r\n"));
    assert_eq!(read(&windows, ".claude/skills/made-on-windows/SKILL.md").as_deref(), Some("# Edited on Linux\n"));

    let third = world.home("third");
    sync(&world.job(&windows, "third"), &third);
    let landed: Vec<String> = files(&third).into_keys().collect();
    for path in &landed {
        let derived = [".claude/plugins", ".claude/skills/synced", ".agents/.skill-lock.json", ".claude/projects"];
        assert!(!derived.iter().any(|derived| path.starts_with(derived)), "{path} reached the third home");
    }
    assert!(landed.contains(&".agents/skills/made-on-linux/SKILL.md".to_string()), "{landed:?}");
    let config = config(&third);
    for own in ["notify", "projects", "skills", "/home/ubuntu", r"C:\\"] {
        assert!(!config.contains(own), "{own} reached the third home:\n{config}");
    }
    assert!(config.contains("model = \"gpt-a\""), "{config}");
    assert_eq!(plugins_on(&third), plugins_on(&windows));
}

#[cfg(unix)]
#[test]
fn every_folder_a_sync_makes_is_the_owners_alone_whatever_the_umask() {
    use std::os::unix::fs::PermissionsExt;
    // SAFETY: nextest runs each test in a process of its own.
    unsafe { libc::umask(0o022) };
    let world = World::new("owner-only-folders");
    let (mine, far) = (world.home("here"), world.home("machine"));
    seed(&mine);
    put(&far, ".codex/prompts/deep/plan.md", "Plan it.
");
    sync(&world.job(&mine, "far"), &far);
    let bits = |home: &Home, path: &str| fs::metadata(at(home, path)).unwrap().permissions().mode() & 0o777;
    for path in [
        ".claude/skills",
        ".claude/skills/deploy/scripts",
        ".claude/plugins/cache/market/tool/1.0",
        ".claude/plugins/marketplaces/market/.git",
    ] {
        assert_eq!(bits(&far, path), 0o700, "{path} on the Machine");
    }
    assert_eq!(bits(&mine, ".codex/prompts/deep"), 0o700, "pulled here");
}

#[test]
fn a_plugin_and_a_server_each_side_added_are_kept_on_both() {
    let world = World::new("each");
    let (mine, far) = (world.home("here"), world.home("machine"));
    put(&mine, ".claude/settings.json", r#"{"enabledPlugins": {"pstack@cursor-plugins": true}}"#);
    put(&far, ".claude/settings.json", r#"{"enabledPlugins": {"sentry@cursor-plugins": true}}"#);
    put(&mine, ".claude.json", r#"{"mcpServers": {"docs": {"command": "docs-mcp"}}}"#);
    put(&far, ".claude.json", r#"{"userID": "far", "mcpServers": {"search": {"command": "search-mcp"}}}"#);
    put(&mine, ".codex/config.toml", "[plugins.\"pstack@cursor-plugins\"]\nenabled = true\n");
    put(&far, ".codex/config.toml", "[plugins.\"sentry@cursor-plugins\"]\nenabled = true\n");
    let job = world.job(&mine, "far");
    let synced = sync(&job, &far);
    assert_eq!((synced.conflicts.len(), synced.replaced.len()), (0, 0), "{synced:?}");
    let both: BTreeSet<String> = ["pstack@cursor-plugins", "sentry@cursor-plugins"].map(String::from).into();
    for home in [&mine, &far] {
        assert_eq!(plugins_on(home), (both.clone(), both.clone()), "{}", home.root().display());
        let claude: serde_json::Value = serde_json::from_str(&read(home, ".claude.json").unwrap()).unwrap();
        let servers: Vec<&String> = claude["mcpServers"].as_object().unwrap().keys().collect();
        assert_eq!(servers.len(), 2, "{claude}");
    }
    assert!(read(&far, ".claude.json").unwrap().starts_with(r#"{"userID": "far", "mcpServers": {"search""#), "the rest of it as it was");
    assert_eq!(sync(&job, &far), Synced::default());
}

#[test]
fn a_preference_the_sides_hold_differently_takes_this_computers_at_the_first_sync_and_the_newer_after() {
    let world = World::new("whole");
    let (mine, far) = (world.home("here"), world.home("machine"));
    put(&mine, ".claude/settings.json", "{\n  \"model\": \"opus\"\n}\n");
    aged(&mine, ".claude/settings.json", 60);
    put(&far, ".claude/settings.json", "{\n  \"model\": \"sonnet\",\n  \"theme\": \"dark\"\n}\n");
    put(&mine, ".codex/config.toml", "model = \"gpt-a\"\n");
    put(&far, ".codex/config.toml", "# Set up by hand\nmodel = \"gpt-b\" # kept\n");
    let job = world.job(&mine, "far");
    let synced = sync(&job, &far);
    assert_eq!(synced.conflicts, [], "no copy is kept of a key");
    let replaced: Vec<_> = synced.replaced.iter().map(|key| (key.path.as_str(), key.winner, key.was.as_str())).collect();
    assert_eq!(
        replaced,
        [(".claude/settings.json#model", Side::Here, "\"sonnet\""), (".codex/config.toml#model", Side::Here, "\"gpt-b\"")],
        "this computer's, though the Machine's is newer"
    );
    assert!(!job.conflicts.exists());
    assert_eq!(read(&far, ".claude/settings.json").as_deref(), Some("{\n  \"model\": \"opus\",\n  \"theme\": \"dark\"\n}\n"));
    assert_eq!(read(&mine, ".claude/settings.json").as_deref(), Some("{\n  \"model\": \"opus\",\n  \"theme\": \"dark\"\n}\n"));
    assert_eq!(read(&far, ".codex/config.toml").as_deref(), Some("# Set up by hand\nmodel = \"gpt-a\" # kept\n"));
    assert_eq!(sync(&job, &far), Synced::default());

    put(&mine, ".claude/settings.json", "{\n  \"model\": \"haiku\",\n  \"theme\": \"dark\"\n}\n");
    aged(&mine, ".claude/settings.json", 60);
    put(&far, ".claude/settings.json", "{\n  \"model\": \"sonnet\",\n  \"theme\": \"dark\"\n}\n");
    let later = sync(&job, &far);
    let replaced: Vec<_> = later.replaced.iter().map(|key| (key.path.as_str(), key.winner, key.was.as_str())).collect();
    assert_eq!(replaced, [(".claude/settings.json#model", Side::There, "\"haiku\"")], "the newer, once there is a base");
    assert!(read(&mine, ".claude/settings.json").unwrap().contains("\"sonnet\""));
}

#[test]
fn a_file_whose_text_differs_only_in_line_ends_is_the_same_on_both_sides() {
    let world = World::new("line-ends");
    let (mine, far) = (world.home("here"), world.home("machine"));
    put(&mine, ".claude/CLAUDE.md", "Be careful.\r\nBe kind.\r\n");
    put(&far, ".claude/CLAUDE.md", "Be careful.\nBe kind.\n");
    put(&mine, ".agents/skills/x/SKILL.md", "# X\r\n");
    put(&far, ".agents/skills/x/SKILL.md", "# X\n");
    let job = world.job(&mine, "far");
    assert_eq!(sync(&job, &far), Synced::default(), "nothing to move, and no conflict");
    assert_eq!(read(&mine, ".claude/CLAUDE.md").as_deref(), Some("Be careful.\r\nBe kind.\r\n"), "each side's bytes as they were");
    assert_eq!(read(&far, ".claude/CLAUDE.md").as_deref(), Some("Be careful.\nBe kind.\n"));

    put(&far, ".claude/CLAUDE.md", "Be careful.\nBe kinder.\n");
    let synced = sync(&job, &far);
    assert_eq!((synced.pulled, synced.conflicts.len()), (1, 0), "{synced:?}");
    assert_eq!(read(&mine, ".claude/CLAUDE.md").as_deref(), Some("Be careful.\nBe kinder.\n"), "sent as it is");
    assert_eq!(sync(&job, &far), Synced::default());
}

#[test]
fn what_an_older_table_synced_is_never_removed_by_this_one() {
    use sha2::Digest as _;
    let world = World::new("old-base");
    let (mine, far) = (world.home("here"), world.home("machine"));
    put(&far, ".claude/plugins/installed_plugins.json", "{}\n");
    put(&far, ".claude/plugins/cache/m/p/SKILL.md", "# P\n");
    put(&far, ".claude/skills/synced/manifest.json", "{}\n");
    for home in [&mine, &far] {
        put(home, ".claude/settings.json", r#"{"model": "opus"}"#);
        put(home, ".codex/config.toml", "model = \"gpt\"\n");
    }
    let job = world.job(&mine, "far");
    let whole = |path: &str| {
        let bytes = read(&far, path).unwrap();
        let entry = tree::Entry::File {
            size: bytes.len() as u64,
            sha256: crate::chunked::hex(&sha2::Sha256::digest(bytes.as_bytes())),
            mode: None,
            modified: None,
            text: None,
        };
        (path.to_string(), entry)
    };
    let first = tree::Manifest {
        entries: [
            ".claude/plugins/installed_plugins.json",
            ".claude/plugins/cache/m/p/SKILL.md",
            ".claude/skills/synced/manifest.json",
            ".claude/settings.json",
            ".codex/config.toml",
        ]
        .map(whole)
        .into_iter()
        .collect(),
        ..tree::Manifest::default()
    };
    fs::create_dir_all(&job.dir).unwrap();
    fs::write(job.dir.join(BASE), serde_json::to_vec(&first).unwrap()).unwrap();
    let before = files(&far);

    let synced = sync(&job, &far);
    assert_eq!((synced.removed_here, synced.removed_there, synced.held.len()), (0, 0, 0), "{synced:?}");
    assert_eq!(files(&far), before, "the Machine's own, as it was");
    assert_eq!(read(&mine, ".claude/plugins/installed_plugins.json"), None, "and never brought here");
}

/// A Machine whose Ferrite syncs by another table: it refuses the Setup
/// root, as 0.4.0's Server refuses a root it does not know.
struct Refusing;

impl Exchange for Refusing {
    fn ask(&mut self, _: Request) -> io::Result<Asked> {
        Ok(Asked::numbered(1))
    }

    fn answer(&mut self, _: Asked) -> io::Result<Reply> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "This server does not know that kind of root"))
    }
}

#[test]
fn two_ferrites_whose_tables_differ_move_nothing() {
    let world = World::new("tables");
    let (mine, far) = (world.home("here"), world.home("machine"));
    seed(&mine);
    put(&far, ".claude/CLAUDE.md", "The Machine's.\n");
    let job = world.job(&mine, "far");
    let (here, there) = (files(&mine), files(&far));
    let refused = run(&job, &mut Refusing, &mut |_| {}, &Cancellation::default()).unwrap_err();
    assert_eq!(refused.kind(), io::ErrorKind::Unsupported, "{refused}");
    assert!(!job.dir.join(BASE).exists());

    let scan = |root: tree::Root| Session::with_home(far.clone()).execute(Request::Scan { root, skip: Vec::new(), after: None });
    let newer = scan(tree::Root::Setup { table: TABLE + 1 }).unwrap_err();
    assert_eq!(newer.kind(), io::ErrorKind::Unsupported, "{newer}");
    for older in [tree::Root::Setup { table: TABLE - 1 }, tree::Root::FirstSetup] {
        let refused = scan(older).unwrap_err();
        assert!(refused.to_string().starts_with("Update Ferrite on this computer to sync your setup"), "{refused}");
    }
    let write = Request::Begin { write: 1, root: tree::Root::FirstSetup, path: ".claude/CLAUDE.md".into(), size: 1, mode: None };
    assert!(Session::with_home(far.clone()).execute(write).is_err());
    assert_eq!((files(&mine), files(&far)), (here, there), "nothing moved");
}

/// A Server from before the table, main since #494, reads a Setup root by
/// one as a root it does not know, refuses it as `unsupported`, and this
/// window reads that as "Update Ferrite on <name>".
#[test]
fn a_server_from_before_the_table_reads_a_setup_root_by_one_as_a_root_it_does_not_know() {
    #[derive(Debug, PartialEq, serde::Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum Shipped {
        Path {
            #[allow(dead_code)]
            path: PathBuf,
        },
        Setup,
        #[serde(other)]
        Unknown,
    }
    let sent = serde_json::to_value(tree::Root::SETUP).unwrap();
    assert_eq!(serde_json::from_value::<Shipped>(sent).unwrap(), Shipped::Unknown);
    let first: tree::Root = serde_json::from_str(r#"{"kind":"setup"}"#).unwrap();
    assert_eq!(first, tree::Root::FirstSetup, "and what that Server's hub sends is refused here");
}

/// Claude's installer as a test runs it: each run said in `calls`, a
/// marketplace added and a plugin installed by writing Claude's own record
/// of it, and the plugin `broken` not found.
struct Claude {
    dir: PathBuf,
    calls: Arc<Mutex<Vec<String>>>,
}

impl Installer for Claude {
    fn run(&self, words: &[&str]) -> io::Result<()> {
        self.calls.lock().unwrap().push(words.join(" "));
        let record = |name: &str, key: &str| {
            let at = self.dir.join("plugins").join(name);
            let mut record: serde_json::Value = fs::read(&at).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
            let into = if name == "installed_plugins.json" { &mut record["plugins"] } else { &mut record };
            into[key] = serde_json::json!({});
            fs::create_dir_all(at.parent().unwrap()).unwrap();
            fs::write(at, record.to_string()).unwrap();
        };
        match words {
            ["marketplace", "add", "cursor/plugins"] => record("known_marketplaces.json", "cursor-plugins"),
            ["install", plugin, "--json"] if plugin.starts_with("broken@") => {
                return Err(io::Error::other("Plugin \"broken\" not found in marketplace \"cursor-plugins\"."));
            }
            ["install", plugin, "--json"] => record("installed_plugins.json", plugin),
            words => panic!("{words:?}"),
        }
        Ok(())
    }
}

#[test]
fn a_plugin_a_sync_turns_on_is_installed_once_and_one_that_will_not_install_is_said() {
    let world = World::new("installs");
    let mine = world.home("here");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let far = world.home("machine");
    let installs = Installs::new(Claude { dir: far.root().join(".claude"), calls: calls.clone() });
    let far = far.with_installs(installs.clone());
    put(
        &mine,
        ".claude/settings.json",
        r#"{"enabledPlugins": {"pstack@cursor-plugins": true, "broken@cursor-plugins": true, "off@cursor-plugins": false},
            "extraKnownMarketplaces": {"cursor-plugins": {"source": {"source": "github", "repo": "cursor/plugins"}}}}"#,
    );
    let job = world.job(&mine, "far");
    let synced = sync(&job, &far);
    let said: Vec<_> = synced.problems.iter().map(|problem| (problem.path.as_str(), problem.side, problem.error.as_str())).collect();
    let broken = (".claude/plugins/broken@cursor-plugins", Side::There, "Plugin \"broken\" not found in marketplace \"cursor-plugins\".");
    assert_eq!(said, [broken], "said by the sync that turned it on");
    assert_eq!(installing(&synced.problems[0].path), Some("broken@cursor-plugins"));
    let ran = |words: &str| calls.lock().unwrap().iter().filter(|call| *call == words).count();
    assert_eq!((ran("marketplace add cursor/plugins"), ran("install pstack@cursor-plugins --json")), (1, 1));
    assert_eq!(ran("install off@cursor-plugins --json"), 0, "a plugin turned off is not installed");

    let before = calls.lock().unwrap().len();
    installs.changed(&far.root().join(".claude"));
    installs.wait(Duration::from_secs(10));
    let again: Vec<String> = calls.lock().unwrap()[before..].to_vec();
    assert_eq!(again, ["install broken@cursor-plugins --json"], "what is installed is not installed again");
    let quiet = sync(&job, &far);
    assert_eq!(quiet.changed(), 0, "{quiet:?}");
    assert_eq!(quiet.problems.len(), 1, "said until a later write installs it");
}
