//! Ferrite: the one-time migration of Ferrite's own Coordinator Threads
//! (ADR 0028, removed by ADR 0030) into projects. Ferrite-only; upstream
//! has no such file.
//!
//! Ferrite kept one entry per registered Project in `coordinators.json`
//! beside its Store, the Project's title and roots in `registry.json` and
//! its memory in `memory/project-<id>.md`. Each entry becomes a project
//! under the root through the same writers `new`, `open` and `thread adopt`
//! use, its open workers adopted from the Server's own `thread.list`. The
//! file is renamed `coordinators.json.migrated` before anything is created,
//! so it never runs twice; what fails from there on is a note.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use crate::herdr::Herdr;
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
    let config = profiles::load(&ctx.config_dir)?;
    let socket = store.join("api.sock").to_string_lossy().into_owned();
    let migration = Migration {
        ctx,
        store,
        registry: project::read_json(&store.join("registry.json")).unwrap_or_default(),
        thread_profile: config.new_project_default(Role::Thread),
        coordinator_profile: config.new_project_default(Role::Coordinator),
        prefix: crate::coordinator::current_prefix(&ctx.root)?,
        herdr: Herdr::new(ctx.env.herdr_bin(), &socket, ctx.runner),
        socket,
    };
    // Workers are adopted from the Server's own list, so it answers before
    // anything is created; unreachable, the next start tries again.
    let socket = &migration.socket;
    migration.herdr.agent_list().map_err(|e| anyhow!("Ferrite's Server at {socket} is not reachable: {e}"))?;
    let mut migrated = file.as_os_str().to_owned();
    migrated.push(".migrated");
    std::fs::rename(file, &migrated).with_context(|| format!("could not rename {}", file.display()))?;
    let mut claimed = Vec::new();
    let mut notes = Vec::new();
    for entry in &book.projects {
        let registered = migration.registry.projects.iter().find(|p| p.id == entry.project);
        let name = unclaimed_name(&ctx.root, registered, entry.project, &mut claimed)?;
        notes.extend(migration.entry(entry, &name));
    }
    Ok(notes)
}

/// The entry's project name: its Ferrite title (or `Project <id>`), numbered
/// when a project under the root or an earlier entry already slugs the
/// same, so a Project called `app` beside a project `app` becomes `app-2`.
fn unclaimed_name(root: &Path, registered: Option<&Registered>, id: u64, claimed: &mut Vec<String>) -> Result<String> {
    let base = registered
        .map(|p| p.title.clone())
        .filter(|title| project::slug_from_name(title).is_ok())
        .unwrap_or_else(|| format!("Project {id}"));
    let mut name = base.clone();
    for n in 2.. {
        let slug = project::slug_from_name(&name)?;
        if !claimed.contains(&slug) && !root.join(&slug).exists() {
            claimed.push(slug);
            break;
        }
        name = format!("{base} {n}");
    }
    Ok(name)
}

/// What every entry of one `coordinators.json` is migrated with.
struct Migration<'a> {
    ctx: &'a Ctx<'a>,
    store: &'a Path,
    registry: Registry,
    thread_profile: String,
    coordinator_profile: String,
    prefix: String,
    herdr: Herdr<'a>,
    socket: String,
}

impl Migration<'_> {
    /// One entry, said in notes: what it became, or why not.
    fn entry(&self, entry: &Coordination, name: &str) -> Vec<String> {
        let prefix = &self.prefix;
        let project = match self.create(entry, name) {
            Ok(project) => project,
            Err(error) => {
                // No half-made project.
                if let Ok(slug) = project::slug_from_name(name) {
                    let _ = std::fs::remove_dir_all(self.ctx.root.join(slug));
                }
                return vec![format!("Ferrite's coordinator of {name}: not migrated ({error:#})")];
            }
        };
        let slug = &project.slug;
        let mut notes = vec![format!(
            "Ferrite's coordinator of {name}: migrated to `{slug}`; Thread {} is no longer its coordinator and still holds its ADR 0028 brief; run `{prefix} open {slug}` to start one",
            entry.coordinator
        )];
        for worker in entry.workers.iter().filter(|w| !w.merged.as_ref().is_some_and(|m| m.resolved)) {
            if let Err(error) = adopt_worker(self, &project, worker) {
                notes.push(format!(
                    "  worker Thread {id} of {name}: not migrated ({error:#}); once it runs, `{prefix} thread adopt {slug} --pane {id} --title <title>` adopts it",
                    id = worker.thread
                ));
            }
        }
        notes
    }

    /// The entry's project, everything but its workers.
    fn create(&self, entry: &Coordination, name: &str) -> Result<Project> {
        let repos = self
            .registry
            .projects
            .iter()
            .filter(|p| p.id == entry.project)
            .flat_map(|p| std::iter::once(&p.root).chain(&p.additional_roots))
            .map(|root| Repo { path: root.to_string_lossy().into_owned(), machine: None })
            .collect();
        let project = project::create(&self.ctx.root, name, &entry.goal, repos)?;
        profiles::write_project_defaults(&project, &self.thread_profile, &self.coordinator_profile)?;
        if !entry.instructions.trim().is_empty() {
            write_instructions(&project, &entry.instructions)?;
        }
        write_memory(&project, &self.store.join("memory").join(format!("project-{}.md", entry.project)))?;
        project::write_priming(&project, &self.prefix)?;
        if !entry.follow_prs {
            disable_pr_followup(&project)?;
        }
        // What `open` records, minus a pane: Ferrite's coordinator Thread works
        // in the repository, not the project folder, so it is no coordinator
        // here. The socket lets the ticker follow the adopted workers.
        let cwd = project.canonical_dir().to_string_lossy().into_owned();
        project.update_coordinator(|c| {
            c.socket = self.socket.clone();
            c.cwd = cwd;
        })?;
        Ok(project)
    }
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

/// An open worker becomes what `thread adopt` makes of a running pane, with
/// the same refusals: an adopted thread whose pane is its Thread, in the
/// directory `thread.list` reports, with a thread directory and brief. Its
/// task is the one Ferrite gave it followed by adoption's own, so a worker
/// that already finished reports instead of starting over. A parked Thread
/// is not listed, so not adopted. Nothing is prompted here; the ticker
/// prompts it with the brief once it is ready.
fn adopt_worker(migration: &Migration, project: &Project, worker: &Worker) -> Result<()> {
    let Migration { ctx, herdr, socket, .. } = migration;
    let agent = crate::adopt::adoptable_agent(ctx, herdr, socket, &worker.thread.to_string())?;
    let cwd = &agent.cwd;

    // The working directory's repository and branch, when it is in one.
    let git = |args: &[&str]| -> Option<String> {
        let out = ctx.runner.run(&Cmd::new("git", Duration::from_secs(5)).args(["-C", cwd]).args(args.iter().copied())).ok()?;
        out.success().then(|| out.stdout.trim().to_string()).filter(|s| !s.is_empty())
    };
    let repo = git(&["rev-parse", "--show-toplevel"]).unwrap_or_default();
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]).filter(|b| b != "HEAD").unwrap_or_default();
    let origin = git(&["remote", "get-url", "origin"]).unwrap_or_default();

    let title = worker.task.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("Ferrite worker").to_string();
    let created = thread::allocate(project, |t| {
        t.title = title;
        t.kind = Kind::Adopted;
        t.repo = repo;
        t.branch = branch;
        t.origin = origin;
        t.agent = agent.agent.clone();
        t.agent_name = agent.name.clone();
        t.cwd = cwd.clone();
        t.workspace_id = agent.workspace_id.clone();
        t.tab_id = agent.tab_id.clone();
        t.pane_id = agent.pane_id.clone();
    })?;
    let task = format!("{}\n\n{}", worker.task.trim_end(), crate::adopt::DEFAULT_TASK);
    let briefed = (|| -> Result<String> {
        {
            let _lock = project.lock()?;
            project::write_atomic(&thread::task_path(project, &created.id), task.as_bytes())?;
        }
        let dir = thread::thread_dir(cwd, &project.slug, &created.id);
        let brief = thread::brief_for(project, &Thread { thread_dir: dir.clone(), ..created.clone() }, &task, false)?;
        std::fs::create_dir_all(Path::new(&dir).join("library")).with_context(|| format!("could not create {dir}"))?;
        crate::threads::exclude_from_git(ctx.runner, cwd)?;
        project::write_atomic(&Path::new(&dir).join("brief.md"), brief.as_bytes())?;
        Ok(dir)
    })();
    match briefed {
        Ok(dir) => thread::update(project, &created.id, |t| {
            t.thread_dir = dir;
            t.status = Status::Open;
            t.prompt_pending = true;
            t.last_state = agent.agent_status.clone();
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
    use crate::runner::fake::{FakeRunner, fail, ok};

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

    /// `thread.list`'s answer: Threads 13 and 15 run, 16 is parked.
    fn threads(tree: &Path, checkout: &Path) -> String {
        format!(
            r#"{{"result":{{"threads":[{{"thread":12,"title":"coordinator","provider":"claude","attention":"idle","cwd":"/work/billing"}},{{"thread":13,"title":"worker","provider":"codex","attention":"done","cwd":"{}"}},{{"thread":15,"title":"merger","provider":"codex","attention":"working","cwd":"{}"}}]}}}}"#,
            tree.display(),
            checkout.display()
        )
    }

    /// A Store holding `BOOK`, its registry and one memory file, a config
    /// with default profiles, and a Server answering `thread.list`.
    fn world(home: &Path, runner: &FakeRunner) -> (PathBuf, PathBuf) {
        let store = home.join(".ferrite/threads");
        std::fs::create_dir_all(store.join("memory")).unwrap();
        let file = store.join("coordinators.json");
        std::fs::write(&file, BOOK).unwrap();
        std::fs::write(store.join("registry.json"), REGISTRY).unwrap();
        std::fs::write(store.join("memory/project-3.md"), "- Run cargo check\n").unwrap();
        let tree = home.join("billing-wt-13");
        let checkout = home.join("billing");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::create_dir_all(&checkout).unwrap();
        let config = home.join("config");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("config.toml"), "[defaults]\nthread_profile = \"codex\"\ncoordinator_profile = \"codex\"\n").unwrap();
        runner.on("thread list", ok(&threads(&tree, &checkout)));
        runner.on("git", fail(128, "not a git repository"));
        (file, config)
    }

    #[test]
    fn each_coordinator_becomes_a_project_once() {
        let home = tempfile::tempdir().unwrap();
        let runner = FakeRunner::new();
        let (file, config) = world(home.path(), &runner);
        let store = file.parent().unwrap().to_path_buf();
        let (tree, checkout) = (home.path().join("billing-wt-13"), home.path().join("billing"));
        let env = Env::for_test(home.path(), &[]);
        let ctx = Ctx { env: &env, root: home.path().join("projects"), config_dir: config, runner: &runner, detached_ticker: false };
        project::create(&ctx.root, "taken", "", vec![]).unwrap();

        let notes = migrate(&ctx, &file).unwrap();
        assert_eq!(notes.len(), 5, "{notes:?}");
        assert!(notes[0].contains("migrated to `billing-app`; Thread 12 is no longer its coordinator and still holds its ADR 0028 brief; run `"), "{notes:?}");
        assert!(notes[0].ends_with(" open billing-app` to start one"), "{notes:?}");
        assert!(notes[1].contains("worker Thread 16 of Billing App: not migrated (no agent is detected in pane 16"), "a parked Thread is not adopted: {notes:?}");
        assert!(notes[1].ends_with(" thread adopt billing-app --pane 16 --title <title>` adopts it"), "{notes:?}");
        assert!(notes[2].contains("of taken 2: migrated to `taken-2`"), "an existing project keeps its name: {notes:?}");
        assert!(notes[4].contains("migrated to `billing-app-2`"), "two Projects with one name both survive: {notes:?}");
        assert!(!file.exists() && store.join("coordinators.json.migrated").is_file());
        assert!(Project::load(&ctx.root, "taken").unwrap().coordinator().is_none(), "the existing project is untouched");

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
        let panes: Vec<_> = threads.iter().map(|t| (t.pane_id.as_str(), t.title.as_str(), t.kind, t.status, t.prompt_pending, t.last_state.as_str())).collect();
        assert_eq!(
            panes,
            [
                ("13", "Add invoices", Kind::Adopted, Status::Open, true, "done"),
                ("15", "Merged, not yet cleaned", Kind::Adopted, Status::Open, true, "working"),
            ],
            "as `thread adopt` records them, the state included"
        );
        let cwds: Vec<_> = threads.iter().map(|t| t.cwd.clone()).collect();
        assert_eq!(cwds, [tree.to_string_lossy().into_owned(), checkout.to_string_lossy().into_owned()], "the directory `thread.list` reports");
        assert_eq!((threads[0].agent.as_str(), threads[0].agent_name.as_str()), ("codex", "worker"));
        assert_eq!(threads[0].thread_dir, thread::thread_dir(&threads[0].cwd, "billing-app", &threads[0].id));
        let brief = std::fs::read_to_string(Path::new(&threads[0].thread_dir).join("brief.md")).unwrap();
        assert!(brief.contains("With PDFs."));
        assert_eq!(std::fs::read_to_string(thread::task_path(&billing, &threads[0].id)).unwrap(), format!("Add invoices\n\nWith PDFs.\n\n{}", crate::adopt::DEFAULT_TASK));

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
    fn an_unreachable_server_leaves_it_for_the_next_start() {
        let home = tempfile::tempdir().unwrap();
        let runner = FakeRunner::new();
        runner.on("thread list", fail(1, "connection refused"));
        let (file, config) = world(home.path(), &runner);
        let env = Env::for_test(home.path(), &[]);
        let ctx = Ctx { env: &env, root: home.path().join("projects"), config_dir: config, runner: &runner, detached_ticker: false };
        let error = migrate(&ctx, &file).unwrap_err();
        assert!(format!("{error:#}").contains("is not reachable"), "{error:#}");
        assert!(file.is_file(), "not renamed");
        assert!(project::list_slugs(&ctx.root).is_empty(), "nothing created");
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
