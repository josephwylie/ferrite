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
/// home of its own, cut off after `left` requests when that is set.
struct Machine {
    session: Session,
    replies: VecDeque<(u64, io::Result<Reply>)>,
    asked: u64,
    left: Option<u64>,
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
        }
    }
}

impl Exchange for Machine {
    fn ask(&mut self, request: Request) -> io::Result<Asked> {
        if self.left.is_some_and(|left| self.asked >= left) {
            return Err(io::Error::new(io::ErrorKind::ConnectionAborted, "cut off"));
        }
        self.asked += 1;
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
            home: home.clone(),
            dir: self.root.join("settings").join("machines").join(machine),
            conflicts: self.root.join("settings").join("sync-conflicts"),
            allow_removals: false,
            only_if_changed_here: false,
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
        Outcome::Stopped(valve) => panic!("stopped: {valve:?}"),
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
        ".claude/plugins/marketplaces/market/.git/HEAD",
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
    assert_eq!(read(&far, ".codex/auth.json"), None);
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
        let job = world.job(&mine, "far");
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
struct Fake(Mutex<BTreeMap<String, String>>);

impl Keychain for Fake {
    fn get(&self, service: &str) -> io::Result<Option<String>> {
        Ok(self.0.lock().unwrap().get(service).cloned())
    }

    fn set(&self, service: &str, secret: &str) -> io::Result<()> {
        self.0.lock().unwrap().insert(service.into(), secret.into());
        Ok(())
    }

    fn delete(&self, service: &str) -> io::Result<bool> {
        Ok(self.0.lock().unwrap().remove(service).is_some())
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
    assert_eq!(synced.conflicts.len(), 1, "{synced:?}");
    println!("{}", said.join("\n"));
    for line in &said {
        assert!(!line.contains("NEVER-SHOWN"), "{line}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let kept = &synced.conflicts[0].kept;
        assert_eq!(fs::metadata(kept).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::metadata(kept.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
    }
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
