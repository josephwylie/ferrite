use std::collections::{BTreeMap, VecDeque};
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
        ".claude/plugins/cache/market/tool/1.0/plugin.json",
        ".claude/plugins/marketplaces/market/README.md",
        ".claude/plugins/marketplaces/market/.git/HEAD",
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
        ".codex/skills/.system/bundled/SKILL.md",
        ".codex/sessions/one.jsonl",
    ] {
        assert!(!landed.contains_key(path), "{path} is not the setup");
    }
    assert_eq!(read(&far, ".claude/projects/y/session.jsonl").as_deref(), Some("far\n"));

    let plugins: serde_json::Value = serde_json::from_str(&read(&far, ".claude/plugins/installed_plugins.json").unwrap()).unwrap();
    let installed = far.root().join(".claude").join("plugins").join("cache").join("market").join("tool").join("1.0");
    assert_eq!(plugins["plugins"]["tool@market"][0]["installPath"], *installed.to_string_lossy(), "in the Machine's home");
    assert_eq!(read(&far, ".claude/settings.json"), read(&mine, ".claude/settings.json"));
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
    assert!(read(&far, ".claude.json").is_some_and(|json| !json.contains("mcpServers")), "the key goes, the file stays");
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
        let read = machine.call(Request::Read { root: tree::Root::Setup, path: path.into(), offset: 0 });
        assert!(read.is_err(), "{path}");
        let remove = machine.call(Request::Remove { root: tree::Root::Setup, path: path.into() });
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
fn losing_every_skill_stops_though_the_plugins_outnumber_them_and_what_goes_here_is_kept_in_the_trash() {
    let world = World::new("valve-item");
    let (mine, far) = (world.home("here"), world.home("machine"));
    seed(&mine);
    for at in 0..60 {
        put(&mine, &format!(".claude/plugins/cache/big/{at}.js"), "plugin\n");
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
    assert_eq!(kept(".claude/commands/a.md").as_deref(), Some("a\n"));
    assert_eq!(kept(".claude/commands/b.md").as_deref(), Some("b\n"));
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
        let removed = Machine::new(&far).call(Request::Remove { root: tree::Root::Setup, path: path.into() });
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
fn a_marketplace_is_seeded_whole_to_a_side_without_it_and_otherwise_left_alone() {
    let world = World::new("seed");
    let (mine, far) = (world.home("here"), world.home("machine"));
    put(&mine, ".claude/plugins/marketplaces/shared/.git/HEAD", "ref: refs/heads/main\n");
    put(&mine, ".claude/plugins/marketplaces/shared/README.md", "Shared.\r\n");
    put(&mine, ".claude/plugins/marketplaces/shared/plugins/new.json", "{}\r\n");
    for at in 0..40 {
        put(&mine, &format!(".claude/plugins/marketplaces/mine/plugins/{at:02}.json"), "{}\n");
    }
    put(&mine, ".claude/plugins/store/here.lock", "1");
    put(&far, ".claude/plugins/marketplaces/shared/.git/HEAD", "ref: refs/heads/old\n");
    put(&far, ".claude/plugins/marketplaces/shared/README.md", "Shared.\n");
    fs::create_dir_all(at(&far, ".claude/plugins/store/there.lock")).unwrap();
    let job = world.job(&mine, "far");
    let seeded = |home: &Home| files(home).keys().filter(|path| path.contains("/marketplaces/mine/")).count();

    let cut = run(&job, &mut Machine::cut_after(&far, Some(12)), &mut |_| {}, &Cancellation::default());
    assert!(cut.is_err(), "{cut:?}");
    assert!((1..40).contains(&seeded(&far)), "cut short: {}", seeded(&far));
    let synced = sync(&job, &far);
    assert_eq!(synced.conflicts, [], "{synced:?}");
    assert_eq!(seeded(&far), 40, "a seed cut short is finished");
    assert_eq!(read(&far, ".claude/plugins/marketplaces/shared/README.md").as_deref(), Some("Shared.\n"), "its own clone is left alone");
    assert_eq!(read(&far, ".claude/plugins/marketplaces/shared/plugins/new.json"), None);
    assert_eq!(read(&mine, ".claude/plugins/marketplaces/shared/.git/HEAD").as_deref(), Some("ref: refs/heads/main\n"));
    assert!(!at(&far, ".claude/plugins/store/here.lock").exists() && !at(&mine, ".claude/plugins/store/there.lock").exists());

    put(&far, ".claude/plugins/marketplaces/mine/plugins/00.json", "{\"updated\": true}\n");
    assert_eq!(sync(&job, &far), Synced::default(), "each side keeps its clone up itself");
    assert_eq!(read(&mine, ".claude/plugins/marketplaces/mine/plugins/00.json").as_deref(), Some("{}\n"));
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
        .filter(|sync| sync.path().join(".claude").join("CLAUDE.md").exists())
        .count();
    assert_eq!(copies, 1, "one kept copy");
    assert_eq!(fs::read_to_string(&synced.conflicts[0].kept).unwrap(), "Edited here, earlier.\n");
}

#[test]
fn codex_config_new_from_a_side_without_bits_is_owner_only() {
    let world = World::new("config-bits");
    let home = world.home("here");
    assert_eq!(home.receive(".codex/config.toml", None).unwrap().mode, Some(0o600));
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
            .map(|path| (path.to_string(), tree::Entry::File { size: 0, sha256: String::new(), mode: None, modified: None }))
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
