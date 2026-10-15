//! Ferrite: the one-time migration of Ferrite's own Coordinator Threads
//! (ADR 0028, removed by ADR 0030) into projects. Ferrite-only; upstream
//! has no such file.
//!
//! Ferrite kept one entry per registered Project in `coordinators.json`
//! beside its Store, the Project's title and roots in `registry.json`, its
//! memory in `memory/project-<id>.md`, and each Thread's working directory
//! in the facts of `<id>/log.jsonl`. Each entry becomes a project under the
//! root through the same writers `new`, `open` and `thread adopt` use; the
//! file is then renamed `coordinators.json.migrated`, so a second run finds
//! nothing to do.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::paths::Ctx;
use crate::profiles::{self, Role};
use crate::project::{self, Project, Repo};
use crate::runner::Cmd;
use crate::thread::{self, Kind, Status, Thread};

/// The only `coordinators.json` version Ferrite wrote.
const VERSION: u32 = 1;

/// The memory file a Ferrite coordinator's lessons land in.
const MEMORY_FILE: &str = "memory/ferrite-lessons.md";

const MEMORY_INDEX_LINE: &str = "- [Lessons](memory/ferrite-lessons.md): what workers of Ferrite's coordinator asked the next one to remember\n";

/// `coordinators.json` as Ferrite wrote it.
#[derive(Deserialize)]
struct Book {
    version: u32,
    projects: Vec<Coordination>,
}

#[derive(Deserialize)]
struct Coordination {
    project: u64,
    coordinator: u64,
    #[serde(default)]
    goal: String,
    #[serde(default)]
    instructions: String,
    #[serde(default)]
    follow_prs: bool,
    #[serde(default)]
    workers: Vec<Worker>,
}

#[derive(Deserialize)]
struct Worker {
    thread: u64,
    task: String,
    #[serde(default)]
    merged: Option<Merged>,
}

#[derive(Deserialize)]
struct Merged {
    #[serde(default)]
    resolved: bool,
}

/// The part of Ferrite's `registry.json` a migration reads: each Project's
/// title and roots.
#[derive(Deserialize, Default)]
struct Registry {
    projects: Vec<Registered>,
}

#[derive(Deserialize)]
struct Registered {
    id: u64,
    title: String,
    root: PathBuf,
    #[serde(default)]
    additional_roots: Vec<PathBuf>,
}

/// The part of a Thread's facts a migration reads: the log's first line
/// says them, and every later line that changes them (`facts`, `mark`, a
/// schema-13 `handover`) restates them whole, so the newest one is the truth.
#[derive(Deserialize)]
struct Facts {
    provider: String,
    #[serde(default)]
    workspace: Option<Binding>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Binding {
    Main { checkout: PathBuf },
    Worktree { path: PathBuf },
}

/// A log line that restates the facts.
#[derive(Deserialize)]
struct Restated {
    facts: Facts,
}

impl Facts {
    /// The newest facts in a Thread log. A line that does not parse (a torn
    /// tail) restates nothing.
    fn newest(log: &str) -> Option<Facts> {
        let mut lines = log.lines();
        let header = lines.next()?;
        lines
            .rev()
            .find_map(|line| serde_json::from_str::<Restated>(line).ok())
            .map(|restated| restated.facts)
            .or_else(|| serde_json::from_str(header).ok())
    }

    /// The Thread's workspace binding's directory: what `thread.list`
    /// reports as its `cwd`, so what the adapter gives as its pane's and
    /// agent's `cwd` and what `thread adopt` records. A session project
    /// root below it does not count; an adopted thread whose `cwd` differs
    /// from its pane's is gone to the ticker.
    fn cwd(&self) -> Option<&Path> {
        match self.workspace.as_ref()? {
            Binding::Main { checkout } => Some(checkout),
            Binding::Worktree { path } => Some(path),
        }
    }
}

/// `migrate-ferrite-coordinators <coordinators.json>`: one note per line.
/// No file: nothing to do.
pub fn run(ctx: &Ctx, file: &Path) -> Result<()> {
    for note in migrate(ctx, file)? {
        println!("{note}");
    }
    Ok(())
}

fn migrate(ctx: &Ctx, file: &Path) -> Result<Vec<String>> {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).with_context(|| format!("could not read {}", file.display())),
    };
    let book: Book = serde_json::from_str(&text).with_context(|| format!("{} does not parse", file.display()))?;
    if book.version != VERSION {
        bail!("{}: version {} is not one Ferrite wrote", file.display(), book.version);
    }
    let store = file.parent().context("coordinators.json has no folder")?;
    let registry: Registry = project::read_json(&store.join("registry.json")).unwrap_or_default();
    let config = profiles::load(&ctx.config_dir)?;
    let defaults = (config.new_project_default(Role::Thread), config.new_project_default(Role::Coordinator));
    let prefix = crate::coordinator::current_prefix(&ctx.root)?;
    let mut claimed = Vec::new();
    let mut notes = Vec::new();
    for entry in &book.projects {
        let name = unclaimed_name(registry.projects.iter().find(|p| p.id == entry.project), entry.project, &mut claimed)?;
        notes.extend(migrate_one(ctx, store, &registry, entry, &name, &defaults, &prefix)?);
    }
    let mut migrated = file.as_os_str().to_owned();
    migrated.push(".migrated");
    std::fs::rename(file, &migrated).with_context(|| format!("could not rename {}", file.display()))?;
    Ok(notes)
}

/// The entry's project name: its Ferrite title (or `Project <id>`), numbered
/// when an earlier entry of the same file already slugs the same, so two
/// Projects called `app` become `app` and `app-2`. Deterministic, so a rerun
/// after a failure names every entry the same and, since a failed entry
/// leaves no project behind, migrates it then.
fn unclaimed_name(registered: Option<&Registered>, id: u64, claimed: &mut Vec<String>) -> Result<String> {
    let base = registered
        .map(|p| p.title.clone())
        .filter(|title| project::slug_from_name(title).is_ok())
        .unwrap_or_else(|| format!("Project {id}"));
    let mut name = base.clone();
    for n in 2.. {
        let slug = project::slug_from_name(&name)?;
        if !claimed.contains(&slug) {
            claimed.push(slug);
            break;
        }
        name = format!("{base} {n}");
    }
    Ok(name)
}

fn migrate_one(ctx: &Ctx, store: &Path, registry: &Registry, entry: &Coordination, name: &str, defaults: &(String, String), prefix: &str) -> Result<Vec<String>> {
    let slug = project::slug_from_name(name)?;
    if ctx.root.join(&slug).exists() {
        return Ok(vec![format!("Ferrite's coordinator of {name}: `{slug}` already exists, skipped")]);
    }
    let project = match create(ctx, store, registry, entry, name, defaults, prefix) {
        Ok(project) => project,
        Err(error) => {
            // No half-made project: a rerun would skip it as already there.
            let _ = std::fs::remove_dir_all(ctx.root.join(&slug));
            return Err(error);
        }
    };
    let mut notes = vec![format!(
        "Ferrite's coordinator of {name}: migrated to `{}`; Thread {} is no longer its coordinator and still holds its ADR 0028 brief; run `{prefix} open {}` to start one",
        project.slug, entry.coordinator, project.slug
    )];
    for worker in entry.workers.iter().filter(|w| !w.merged.as_ref().is_some_and(|m| m.resolved)) {
        if let Err(error) = adopt_worker(ctx, store, &project, worker) {
            notes.push(format!("  worker Thread {} of {name}: not migrated ({error:#})", worker.thread));
        }
    }
    Ok(notes)
}

/// The entry's project, everything but its workers.
fn create(ctx: &Ctx, store: &Path, registry: &Registry, entry: &Coordination, name: &str, (thread_profile, coordinator_profile): &(String, String), prefix: &str) -> Result<Project> {
    let repos = registry
        .projects
        .iter()
        .filter(|p| p.id == entry.project)
        .flat_map(|p| std::iter::once(&p.root).chain(&p.additional_roots))
        .map(|root| Repo { path: root.to_string_lossy().into_owned(), machine: None })
        .collect();
    let project = project::create(&ctx.root, name, &entry.goal, repos)?;
    profiles::write_project_defaults(&project, thread_profile, coordinator_profile)?;
    if !entry.instructions.trim().is_empty() {
        write_instructions(&project, &entry.instructions)?;
    }
    write_memory(&project, &store.join("memory").join(format!("project-{}.md", entry.project)))?;
    project::write_priming(&project, prefix)?;
    if !entry.follow_prs {
        disable_pr_followup(&project)?;
    }
    // What `open` records, minus a pane: Ferrite's coordinator Thread works
    // in the repository, not the project folder, so it is no coordinator
    // here. The socket lets the ticker follow the adopted workers.
    let socket = store.join("api.sock").to_string_lossy().into_owned();
    let cwd = project.canonical_dir().to_string_lossy().into_owned();
    project.update_coordinator(|c| {
        c.socket = socket;
        c.cwd = cwd;
    })?;
    Ok(project)
}

/// The body under PROJECT.md's front matter becomes the standing instructions.
fn write_instructions(project: &Project, instructions: &str) -> Result<()> {
    let text = std::fs::read_to_string(project.project_md())?;
    let (front, _) = text.split_once("\n+++\n").context("PROJECT.md has no front matter")?;
    let text = format!("{front}\n+++\n\n# Instructions\n\n{}\n", instructions.trim());
    project::parse_project_md(&text)?;
    project::write_atomic(&project.project_md(), text.as_bytes())
}

/// Ferrite's memory file, one `- ` lesson per line, becomes one memory file
/// listed in MEMORY.md. Nothing remembered: nothing written.
fn write_memory(project: &Project, ferrite_memory: &Path) -> Result<()> {
    let lessons = std::fs::read_to_string(ferrite_memory).unwrap_or_default();
    if lessons.trim().is_empty() {
        return Ok(());
    }
    project::write_atomic(&project.dir().join(MEMORY_FILE), lessons.as_bytes())?;
    let index = project.dir().join("MEMORY.md");
    let mut text = std::fs::read_to_string(&index)?;
    text.push_str(MEMORY_INDEX_LINE);
    project::write_atomic(&index, text.as_bytes())
}

/// The default routine stays, switched off: the coordinator did not follow PRs.
fn disable_pr_followup(project: &Project) -> Result<()> {
    let path = project.dir().join(project::PR_FOLLOWUP);
    let text = std::fs::read_to_string(&path)?.replacen("\nenabled = true\n", "\nenabled = false\n", 1);
    crate::routine::parse("pr-followup", &text)?;
    project::write_atomic(&path, text.as_bytes())
}

/// An open worker becomes what `thread adopt` makes of a running pane: an
/// adopted thread whose pane is its Thread, recorded in its working
/// directory, with a thread directory and brief. Its task is the one Ferrite
/// gave it followed by adoption's own, so a worker that already finished
/// reports instead of starting over. Nothing is launched; the ticker prompts
/// it with the brief once it is ready, as for any adoption.
fn adopt_worker(ctx: &Ctx, store: &Path, project: &Project, worker: &Worker) -> Result<()> {
    let log = store.join(worker.thread.to_string()).join("log.jsonl");
    let text = std::fs::read_to_string(&log).with_context(|| format!("{} is gone", log.display()))?;
    let facts = Facts::newest(&text).with_context(|| format!("{} has no header", log.display()))?;
    let cwd = facts.cwd().with_context(|| format!("{} records no working directory", log.display()))?.to_string_lossy().into_owned();

    // The working directory's repository and branch, when it is in one.
    let git = |args: &[&str]| -> Option<String> {
        let out = ctx.runner.run(&Cmd::new("git", Duration::from_secs(5)).args(["-C", &cwd]).args(args.iter().copied())).ok()?;
        out.success().then(|| out.stdout.trim().to_string()).filter(|s| !s.is_empty())
    };
    let repo = git(&["rev-parse", "--show-toplevel"]).unwrap_or_default();
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]).filter(|b| b != "HEAD").unwrap_or_default();
    let origin = git(&["remote", "get-url", "origin"]).unwrap_or_default();

    let id = worker.thread.to_string();
    let title = worker.task.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("Ferrite worker").to_string();
    let created = thread::allocate(project, |t| {
        t.title = title;
        t.kind = Kind::Adopted;
        t.repo = repo;
        t.branch = branch;
        t.origin = origin;
        t.agent = facts.provider.clone();
        t.agent_name = facts.title.clone().unwrap_or_default();
        t.cwd = cwd.clone();
        t.tab_id = id.clone();
        t.pane_id = id;
    })?;
    let task = format!("{}\n\n{}", worker.task.trim_end(), crate::adopt::DEFAULT_TASK);
    let briefed = (|| -> Result<String> {
        {
            let _lock = project.lock()?;
            project::write_atomic(&thread::task_path(project, &created.id), task.as_bytes())?;
        }
        let dir = thread::thread_dir(&cwd, &project.slug, &created.id);
        let brief = thread::brief_for(project, &Thread { thread_dir: dir.clone(), ..created.clone() }, &task, false)?;
        std::fs::create_dir_all(Path::new(&dir).join("library")).with_context(|| format!("could not create {dir}"))?;
        crate::threads::exclude_from_git(ctx.runner, &cwd)?;
        project::write_atomic(&Path::new(&dir).join("brief.md"), brief.as_bytes())?;
        Ok(dir)
    })();
    match briefed {
        Ok(dir) => thread::update(project, &created.id, |t| {
            t.thread_dir = dir;
            t.status = Status::Open;
            t.prompt_pending = true;
            t.last_state_change = project::now();
        })?,
        Err(error) => {
            let message = format!("{error:#}");
            thread::update(project, &created.id, |t| {
                t.status = Status::Failed;
                t.error = message;
            })?;
            return Err(error);
        }
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Env;
    use crate::runner::fake::{FakeRunner, fail};

    const BOOK: &str = r#"{
  "version": 1,
  "projects": [
    {
      "project": 3,
      "coordinator": 12,
      "goal": "Ship billing",
      "instructions": "Never push to main.",
      "follow_prs": true,
      "workers": [
        { "thread": 13, "task": "Add invoices\n\nWith PDFs.", "sent": ["remark:IC_1"] },
        { "thread": 14, "task": "Old work", "merged": { "pr": 7, "head": "abc", "resolved": true } },
        { "thread": 15, "task": "Merged, not yet cleaned", "merged": { "pr": 8, "head": "def" } },
        { "thread": 16, "task": "Its Thread was deleted" }
      ]
    },
    { "project": 4, "coordinator": 20 },
    { "project": 9, "coordinator": 30, "goal": "Unregistered" },
    { "project": 5, "coordinator": 40, "goal": "Same name" }
  ]
}"#;

    const REGISTRY: &str = r#"{
  "schema": 2,
  "next_project": 10,
  "projects": [
    { "id": 3, "title": "Billing App", "root": "/work/billing", "additional_roots": ["/work/shared"], "minted": 2 },
    { "id": 4, "title": "taken", "root": "/work/taken", "minted": 0 },
    { "id": 5, "title": "billing app", "root": "/work/billing-2", "minted": 0 }
  ],
  "worktrees": []
}"#;

    fn header(workspace: &str, root: &str) -> String {
        format!(r#"{{"schema":13,"provider":"codex","workspace":{workspace},"session_project_root":{root},"model":null,"project_id":3,"title":"worker","effort":null}}"#) + "\n{}\n"
    }

    #[test]
    fn each_coordinator_becomes_a_project_once() {
        let home = tempfile::tempdir().unwrap();
        let store = home.path().join(".ferrite/threads");
        std::fs::create_dir_all(store.join("memory")).unwrap();
        let file = store.join("coordinators.json");
        std::fs::write(&file, BOOK).unwrap();
        std::fs::write(store.join("registry.json"), REGISTRY).unwrap();
        std::fs::write(store.join("memory/project-3.md"), "- Run cargo check\n").unwrap();
        let tree = home.path().join("billing-wt-13");
        let checkout = home.path().join("billing");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::create_dir_all(checkout.join("app")).unwrap();
        for (id, log) in [
            (13, header(&format!(r#"{{"kind":"worktree","repo":"/work/billing","path":"{}"}}"#, tree.display()), "null")),
            (15, header(&format!(r#"{{"kind":"main","checkout":"{}"}}"#, checkout.display()), &format!(r#""{}""#, checkout.join("app").display()))),
        ] {
            std::fs::create_dir_all(store.join(id.to_string())).unwrap();
            std::fs::write(store.join(id.to_string()).join("log.jsonl"), log).unwrap();
        }
        let config = home.path().join("config");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("config.toml"), "[defaults]\nthread_profile = \"codex\"\ncoordinator_profile = \"codex\"\n").unwrap();
        let env = Env::for_test(home.path(), &[]);
        let runner = FakeRunner::new();
        runner.on("git", fail(128, "not a git repository"));
        let ctx = Ctx { env: &env, root: home.path().join("projects"), config_dir: config, runner: &runner, detached_ticker: false };
        project::create(&ctx.root, "taken", "", vec![]).unwrap();

        let notes = migrate(&ctx, &file).unwrap();
        assert_eq!(notes.len(), 5, "{notes:?}");
        assert!(notes[0].contains("migrated to `billing-app`; Thread 12 is no longer its coordinator and still holds its ADR 0028 brief; run `"), "{notes:?}");
        assert!(notes[0].ends_with(" open billing-app` to start one"), "{notes:?}");
        assert!(notes[1].contains("worker Thread 16 of Billing App: not migrated"), "{notes:?}");
        assert!(notes[2].contains("`taken` already exists, skipped"), "{notes:?}");
        assert!(notes[4].contains("migrated to `billing-app-2`"), "two Projects with one name both survive: {notes:?}");
        assert!(!file.exists() && store.join("coordinators.json.migrated").is_file());

        let billing = Project::load(&ctx.root, "billing-app").unwrap();
        let (settings, body) = billing.read_project_md().unwrap();
        assert_eq!((settings.name.as_str(), settings.goal.as_str()), ("Billing App", "Ship billing"));
        assert_eq!(settings.repos.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(), ["/work/billing", "/work/shared"]);
        assert_eq!((settings.thread_profile.as_str(), settings.coordinator_profile.as_str()), ("codex", "codex"), "the configured defaults, as `new` writes them");
        assert_eq!(body, "# Instructions\n\nNever push to main.\n");
        let index = std::fs::read_to_string(billing.dir().join("MEMORY.md")).unwrap();
        assert!(index.ends_with(MEMORY_INDEX_LINE));
        assert_eq!(std::fs::read_to_string(billing.dir().join(MEMORY_FILE)).unwrap(), "- Run cargo check\n");
        let (routines, broken) = crate::routine::load_all(&billing);
        assert!(broken.is_empty() && routines.iter().any(|r| r.name == "pr-followup" && r.enabled));
        assert!(project::priming_problems(&billing, &crate::coordinator::current_prefix(&ctx.root).unwrap()).is_empty());

        let record = billing.coordinator().unwrap();
        assert!(record.pane_id.is_empty() && record.tab_id.is_empty(), "the old coordinator Thread works in its repo, not here");
        assert_eq!(record.socket, store.join("api.sock").to_string_lossy());
        assert_eq!(record.cwd, billing.canonical_dir().to_string_lossy());

        let threads = thread::list(&billing);
        let panes: Vec<_> = threads.iter().map(|t| (t.pane_id.as_str(), t.title.as_str(), t.kind, t.status, t.prompt_pending)).collect();
        assert_eq!(
            panes,
            [
                ("13", "Add invoices", Kind::Adopted, Status::Open, true),
                ("15", "Merged, not yet cleaned", Kind::Adopted, Status::Open, true),
            ]
        );
        let cwds: Vec<_> = threads.iter().map(|t| t.cwd.clone()).collect();
        assert_eq!(cwds, [tree.to_string_lossy().into_owned(), checkout.to_string_lossy().into_owned()], "the binding's directory, as `thread.list` reports it, not the session project root");
        assert_eq!((threads[0].agent.as_str(), threads[0].agent_name.as_str()), ("codex", "worker"));
        assert_eq!(threads[0].thread_dir, thread::thread_dir(&threads[0].cwd, "billing-app", &threads[0].id));
        let brief = std::fs::read_to_string(Path::new(&threads[0].thread_dir).join("brief.md")).unwrap();
        assert!(brief.contains("With PDFs."));
        assert_eq!(std::fs::read_to_string(thread::task_path(&billing, &threads[0].id)).unwrap(), "Add invoices\n\nWith PDFs.");

        let unregistered = Project::load(&ctx.root, "project-9").unwrap();
        let (settings, body) = unregistered.read_project_md().unwrap();
        assert_eq!(settings.goal, "Unregistered");
        assert!(body.starts_with("# Instructions\n\nStanding instructions"), "the template stays");
        let (routines, broken) = crate::routine::load_all(&unregistered);
        assert!(broken.is_empty() && routines.iter().any(|r| r.name == "pr-followup" && !r.enabled), "written, but off");
        assert!(thread::list(&unregistered).is_empty());

        assert!(migrate(&ctx, &file).unwrap().is_empty(), "nothing left to do");
    }

    #[test]
    fn no_file_is_nothing_to_do() {
        let home = tempfile::tempdir().unwrap();
        let env = Env::for_test(home.path(), &[]);
        let runner = FakeRunner::new();
        let ctx = Ctx { env: &env, root: home.path().join("projects"), config_dir: home.path().join("config"), runner: &runner, detached_ticker: false };
        assert!(migrate(&ctx, &home.path().join("coordinators.json")).unwrap().is_empty());
        assert!(!ctx.root.exists());
    }
}
