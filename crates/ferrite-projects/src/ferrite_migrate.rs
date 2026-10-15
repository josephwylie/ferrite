//! Ferrite: the one-time migration of Ferrite's own Coordinator Threads
//! (ADR 0028, removed by ADR 0030) into projects. Ferrite-only; upstream
//! has no such file.
//!
//! Ferrite kept one entry per registered Project in `coordinators.json`
//! beside its Store, the Project's title and roots in `registry.json`, and
//! its memory in `memory/project-<id>.md`. Each entry becomes a project
//! under the root through the same writers `new`, `open` and `thread adopt`
//! use; the file is then renamed `coordinators.json.migrated`, so a second
//! run finds nothing to do.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::paths::Ctx;
use crate::project::{self, Project, Repo};
use crate::thread::{self, Kind, Status};

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

/// `migrate-ferrite-coordinators <coordinators.json>`: one note per entry.
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
    let mut notes = Vec::new();
    for entry in &book.projects {
        notes.push(migrate_one(ctx, store, &registry, entry)?);
    }
    let mut migrated = file.as_os_str().to_owned();
    migrated.push(".migrated");
    std::fs::rename(file, &migrated).with_context(|| format!("could not rename {}", file.display()))?;
    Ok(notes)
}

fn migrate_one(ctx: &Ctx, store: &Path, registry: &Registry, entry: &Coordination) -> Result<String> {
    let registered = registry.projects.iter().find(|p| p.id == entry.project);
    let name = registered
        .map(|p| p.title.clone())
        .filter(|title| project::slug_from_name(title).is_ok())
        .unwrap_or_else(|| format!("Project {}", entry.project));
    let slug = project::slug_from_name(&name)?;
    if ctx.root.join(&slug).exists() {
        return Ok(format!("Ferrite's coordinator of {name}: `{slug}` already exists, skipped"));
    }
    let repos = registered
        .into_iter()
        .flat_map(|p| std::iter::once(&p.root).chain(&p.additional_roots))
        .map(|root| Repo { path: root.to_string_lossy().into_owned(), machine: None })
        .collect();
    let project = project::create(&ctx.root, &name, &entry.goal, repos)?;
    if !entry.instructions.trim().is_empty() {
        write_instructions(&project, &entry.instructions)?;
    }
    write_memory(&project, &store.join("memory").join(format!("project-{}.md", entry.project)))?;
    project::write_priming(&project, &crate::coordinator::current_prefix(&ctx.root)?)?;
    if !entry.follow_prs {
        crate::settings::routine_toggle(ctx, &project.slug, "pr-followup", Some(false))?;
    }
    let socket = store.join("api.sock").to_string_lossy().into_owned();
    let cwd = project.canonical_dir().to_string_lossy().into_owned();
    project.update_coordinator(|c| {
        c.socket = socket;
        c.tab_id = entry.coordinator.to_string();
        c.pane_id = entry.coordinator.to_string();
        c.cwd = cwd;
    })?;
    for worker in entry.workers.iter().filter(|w| !w.merged.as_ref().is_some_and(|m| m.resolved)) {
        adopt_worker(&project, worker)?;
    }
    Ok(format!("Ferrite's coordinator of {name}: migrated to `{}`", project.slug))
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

/// An open worker becomes an adopted thread whose pane is its Thread: it is
/// already running, so nothing is launched or briefed again.
fn adopt_worker(project: &Project, worker: &Worker) -> Result<()> {
    let id = worker.thread.to_string();
    let title = worker.task.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("Ferrite worker").to_string();
    let created = thread::allocate(project, |t| {
        t.title = title;
        t.kind = Kind::Adopted;
        t.status = Status::Open;
        t.tab_id = id.clone();
        t.pane_id = id;
    })?;
    let _lock = project.lock()?;
    project::write_atomic(&thread::task_path(project, &created.id), worker.task.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Env;
    use crate::runner::fake::FakeRunner;

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
        { "thread": 15, "task": "Merged, not yet cleaned", "merged": { "pr": 8, "head": "def" } }
      ]
    },
    { "project": 4, "coordinator": 20 },
    { "project": 9, "coordinator": 30, "goal": "Unregistered" }
  ]
}"#;

    const REGISTRY: &str = r#"{
  "schema": 2,
  "next_project": 10,
  "projects": [
    { "id": 3, "title": "Billing App", "root": "/work/billing", "additional_roots": ["/work/shared"], "minted": 2 },
    { "id": 4, "title": "taken", "root": "/work/taken", "minted": 0 }
  ],
  "worktrees": []
}"#;

    #[test]
    fn each_coordinator_becomes_a_project_once() {
        let home = tempfile::tempdir().unwrap();
        let store = home.path().join(".ferrite");
        std::fs::create_dir_all(store.join("memory")).unwrap();
        let file = store.join("coordinators.json");
        std::fs::write(&file, BOOK).unwrap();
        std::fs::write(store.join("registry.json"), REGISTRY).unwrap();
        std::fs::write(store.join("memory/project-3.md"), "- Run cargo check\n").unwrap();
        let env = Env::for_test(home.path(), &[]);
        let runner = FakeRunner::new();
        let ctx = Ctx { env: &env, root: home.path().join("projects"), config_dir: home.path().join("config"), runner: &runner, detached_ticker: false };
        project::create(&ctx.root, "taken", "", vec![]).unwrap();

        let notes = migrate(&ctx, &file).unwrap();
        assert_eq!(notes.len(), 3, "{notes:?}");
        assert!(notes[1].contains("`taken` already exists, skipped"), "{notes:?}");
        assert!(!file.exists() && store.join("coordinators.json.migrated").is_file());

        let billing = Project::load(&ctx.root, "billing-app").unwrap();
        let (settings, body) = billing.read_project_md().unwrap();
        assert_eq!((settings.name.as_str(), settings.goal.as_str()), ("Billing App", "Ship billing"));
        assert_eq!(settings.repos.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(), ["/work/billing", "/work/shared"]);
        assert_eq!(body, "# Instructions\n\nNever push to main.\n");
        let index = std::fs::read_to_string(billing.dir().join("MEMORY.md")).unwrap();
        assert!(index.ends_with(MEMORY_INDEX_LINE));
        assert_eq!(std::fs::read_to_string(billing.dir().join(MEMORY_FILE)).unwrap(), "- Run cargo check\n");
        let (routines, broken) = crate::routine::load_all(&billing);
        assert!(broken.is_empty() && routines.iter().any(|r| r.name == "pr-followup" && r.enabled));
        assert!(project::priming_problems(&billing, &crate::coordinator::current_prefix(&ctx.root).unwrap()).is_empty());

        let record = billing.coordinator().unwrap();
        assert_eq!((record.pane_id.as_str(), record.tab_id.as_str()), ("12", "12"));
        assert_eq!(record.socket, store.join("api.sock").to_string_lossy());
        assert_eq!(record.cwd, billing.canonical_dir().to_string_lossy());

        let threads = thread::list(&billing);
        let panes: Vec<_> = threads.iter().map(|t| (t.pane_id.as_str(), t.title.as_str(), t.kind, t.status)).collect();
        assert_eq!(panes, [("13", "Add invoices", Kind::Adopted, Status::Open), ("15", "Merged, not yet cleaned", Kind::Adopted, Status::Open)]);
        assert_eq!(std::fs::read_to_string(thread::task_path(&billing, &threads[0].id)).unwrap(), "Add invoices\n\nWith PDFs.");

        let unregistered = Project::load(&ctx.root, "project-9").unwrap();
        let (settings, body) = unregistered.read_project_md().unwrap();
        assert_eq!(settings.goal, "Unregistered");
        assert!(body.starts_with("# Instructions\n\nStanding instructions"), "the template stays");
        let (routines, _) = crate::routine::load_all(&unregistered);
        assert!(routines.iter().any(|r| r.name == "pr-followup" && !r.enabled), "written, but off");
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
