//! The one door from the window into `ferrite_projects` (0.2): where its
//! projects live, the snapshot the nav and the bottom bar read, and the
//! configure and doctor actions. Every act is the crate's own function or
//! the same CLI subcommand its popup ran (`popup::run_hp`), off the UI
//! thread.

use std::path::PathBuf;

use ferrite_core::ThreadId;
use ferrite_projects::paths::{self, Ctx, Env};
use ferrite_projects::project::{self, Project, Status};
use ferrite_projects::runner::RealRunner;
use ferrite_projects::thread::{self, Group};
use ferrite_projects::{coordinator, setup, sidebar, ticker};

/// Where Projects keeps its projects and the operator's settings for them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Place {
    pub root: PathBuf,
    pub config_dir: PathBuf,
}

impl Place {
    /// The operator's: `HERDR_PROJECTS_ROOT`, the config file's `root`, or
    /// the default, exactly as the CLI resolves it.
    pub(crate) fn resolve() -> Option<Place> {
        let env = Env::from_process().ok()?;
        let config_dir = env.config_dir();
        let root = paths::resolve_root(None, &env, &config_dir).ok()?;
        Some(Place { root, config_dir })
    }

    /// Run `f` with a context on this place, as the binary builds one.
    pub(crate) fn with_ctx<R>(&self, f: impl FnOnce(&Ctx) -> R) -> Result<R, String> {
        let env = Env::from_process().map_err(|error| format!("{error:#}"))?;
        let runner = RealRunner;
        let ctx = Ctx {
            env: &env,
            root: self.root.clone(),
            config_dir: self.config_dir.clone(),
            runner: &runner,
            detached_ticker: true,
        };
        Ok(f(&ctx))
    }

    /// Run one CLI subcommand of `ferrite-projects` on this place: whether
    /// it succeeded, and its last line.
    pub(crate) fn run(&self, args: &[&str]) -> (bool, String) {
        let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        self.with_ctx(|ctx| ferrite_projects::popup::run_hp(ctx, &args, None))
            .unwrap_or_else(|error| (false, format!("error: {error}")))
    }
}

/// The Thread a herdr-projects pane id names: on this machine, a pane is a
/// Thread and its id is the Thread's (ADR 0030, Seam 1).
pub(crate) fn thread_of(pane: &str, machine: &str) -> Option<ThreadId> {
    machine
        .is_empty()
        .then(|| pane.parse().ok().map(ThreadId::new))
        .flatten()
}

/// What the nav, the bottom bar and the scope read, refreshed every
/// `popup::REFRESH` off the UI thread.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Model {
    /// Whether the ticker runs: without it every recorded state is stale,
    /// so the nav draws its plain rows.
    pub ticker: bool,
    /// `projects: N need you` for the bottom bar, while the ticker runs.
    pub needs_you: Option<String>,
    pub projects: Vec<ProjectEntry>,
}

/// One listed project: its head (the coordinator) and its open threads in
/// the sidebar's order — by need (`Group::rank`), then id.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProjectEntry {
    pub slug: String,
    /// `project::display_name`: the coordinator row's name.
    pub name: String,
    /// `sidebar::project_line`: `2 need you · 3 working`, `paused`, `idle`.
    pub summary: String,
    pub coordinator: Option<ThreadId>,
    /// The coordinator's sub-line (`sidebar::sub_line` of its row state).
    pub coordinator_sub: String,
    pub threads: Vec<ThreadEntry>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThreadEntry {
    /// The project's own id, `t-0002`.
    pub id: String,
    pub thread: Option<ThreadId>,
    pub group: Group,
    /// `sidebar::thread_display`, held to Herdr's 80 columns.
    pub display: String,
    /// `sidebar::sub_line`: what the state line adds, and fresh activity.
    pub sub: String,
}

/// Herdr's cap on an agent's display name.
const DISPLAY_MAX: usize = 80;

impl Model {
    /// Read every listed project's records, as the ticker last wrote them.
    pub(crate) fn load(place: &Place) -> Model {
        let root = place.root.as_path();
        let ticker = ticker::lock_state(root) != ticker::LockState::Free;
        let projects = project::list_slugs(root)
            .into_iter()
            .filter_map(|slug| Project::load(root, &slug).ok())
            .filter(|project| project.status() != Status::Archived)
            .map(|project| ProjectEntry::of(&project))
            .collect();
        Model {
            ticker,
            needs_you: sidebar::needs_you_line(root),
            projects,
        }
    }

    /// The project a Thread belongs to: the one it heads or works for.
    pub(crate) fn project_of(&self, thread: ThreadId) -> Option<&ProjectEntry> {
        self.projects.iter().find(|project| {
            project.coordinator == Some(thread)
                || project.threads.iter().any(|row| row.thread == Some(thread))
        })
    }
}

impl ProjectEntry {
    fn of(project: &Project) -> ProjectEntry {
        let paused = project.status() == Status::Paused;
        let name = project
            .read_project_md()
            .map(|(settings, _)| project::display_name(&settings.name, &project.slug))
            .unwrap_or_else(|_| project.slug.clone());
        let record = project.coordinator();
        let live = coordinator::live(project);
        let pane = live
            .first()
            .map(|pane| pane.pane_id.clone())
            .or_else(|| record.as_ref().map(|record| record.pane_id.clone()))
            .unwrap_or_default();
        let coordinator_sub = live
            .iter()
            .find(|live| live.pane_id == pane)
            .filter(|_| !paused)
            .map(|live| sidebar::sub_line(&coordinator::row_state(live, None).1, ""))
            .unwrap_or_default();
        let mut threads: Vec<ThreadEntry> = thread::list(project)
            .into_iter()
            .filter(|t| t.status != thread::Status::Resolved)
            .map(|t| {
                let group = Group::from_token(&t.last_group).unwrap_or(Group::Working);
                ThreadEntry {
                    thread: thread_of(&t.pane_id, &t.machine),
                    display: sidebar::thread_display(&t)
                        .chars()
                        .take(DISPLAY_MAX)
                        .collect(),
                    sub: if paused {
                        String::new()
                    } else {
                        sidebar::sub_line(&t.state_line, &t.activity)
                    },
                    id: t.id,
                    group,
                }
            })
            .collect();
        threads.sort_by(|a, b| (a.group.rank(), &a.id).cmp(&(b.group.rank(), &b.id)));
        let groups: Vec<Group> = sidebar::recorded_groups(project);
        ProjectEntry {
            slug: project.slug.clone(),
            name,
            summary: sidebar::project_line(&groups, paused),
            coordinator: thread_of(&pane, ""),
            coordinator_sub,
            threads,
        }
    }
}

/// The doctor action's notification: its title and, by whether every
/// required check passed, its body.
pub(crate) const DOCTOR_TITLE: &str = "Projects: doctor";

pub(crate) fn doctor_body(healthy: bool) -> &'static str {
    if healthy {
        "All required checks passed. Details: ferrite-projects doctor"
    } else {
        "Some checks FAILED. Details: ferrite-projects doctor"
    }
}

/// The configure action's notification: what `setup::configure_ferrite`
/// sets (Ferrite draws the rows Herdr's config did).
pub(crate) const CONFIGURED_TITLE: &str = "Projects configured";
pub(crate) const CONFIGURED_BODY: &str = "Progress hooks and the autoproject skill are set.";
pub(crate) const CONFIGURE_FAILED_TITLE: &str = "Projects: configure failed";

/// The configure action: `setup::configure_ferrite` on this place, in this
/// process (the CLI's `configure` with no flags), as its toast: title,
/// words, whether it failed.
pub(crate) fn configure(place: &Place) -> (String, String, bool) {
    configured(place.with_ctx(configure_in).and_then(|done| done))
}

/// `configure` in `ctx`.
pub(super) fn configure_in(ctx: &Ctx) -> Result<Vec<String>, String> {
    setup::configure_ferrite(ctx).map_err(|error| format!("{error:#}"))
}

pub(super) fn configured(done: Result<Vec<String>, String>) -> (String, String, bool) {
    match done {
        Ok(_) => (CONFIGURED_TITLE.into(), CONFIGURED_BODY.into(), false),
        Err(error) => (CONFIGURE_FAILED_TITLE.into(), error, true),
    }
}
