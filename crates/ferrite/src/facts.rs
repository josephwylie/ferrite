//! What the nav and the Pane head say about a Thread that costs more than
//! an O(1) read: its checkout branch (a `git` call), its Project label (a
//! store peek), a parked Thread's provider (a peek) and the L3 wall card
//! (a walk of every Block). Cached here and refreshed by *moment* — a Pane
//! opened, a Thread streamed or was acted on, the watchdog's tick, the
//! parked set changed — never per frame. Which of the four facts a moment
//! refreshes is this module's knowledge alone, so a new door that opens,
//! parks or changes a Thread names the moment and cannot forget a cache.

use std::collections::HashMap;

use ferrite_core::cockpit::Cockpit;
use ferrite_core::store::Provider;
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::workspace::WorkspaceBinding;
use ferrite_core::ThreadId;
use gpui::SharedString;

use crate::pane::{wall_card, WallCard};

/// One Thread's cached facts. `None` on any of them is honest — the row
/// draws that line empty and keeps its height rather than inventing a word.
#[derive(Default)]
pub struct ThreadFacts {
    /// What the Thread is called: the operator's title, else its first
    /// prompt, else its number — `Cockpit::display_title`, cached by the
    /// same moments as the other facts (a first prompt sent, a rename, a
    /// park) so no frame reads a log to name a row.
    pub name: SharedString,
    /// The branch its effective cwd is actually on, read from git (#29) —
    /// the agent itself may switch branches, which is exactly why the
    /// header reads the repo and not the binding. A cwd outside any
    /// checkout has no text.
    pub branch: Option<SharedString>,
    /// The Project the Thread recorded (#29) — what the nav filter matches
    /// on, so a Thread whose Project is unknown appears under `All
    /// Projects` alone rather than being quietly filed under someone
    /// else's.
    pub project: Option<ProjectId>,
    /// The Project a row names, down the honest ladder (§3.5c): the
    /// registry's title for the recorded Project; else the binding's own
    /// repo leaf — `repo`, never the worktree path, whose leaf is a branch
    /// directory; else nothing at all. Never a placeholder word.
    pub project_label: Option<SharedString>,
    /// The provider the log declared — a parked row's logomark. An open
    /// Thread's provider comes live from the Cockpit.
    pub provider: Option<Provider>,
    /// The wall cell's folded reading — everything the L3 recipe needs that
    /// is not an O(1) transcript read. A frame never walks Blocks at L3.
    pub wall: WallCard,
}

#[derive(Default)]
pub struct Facts {
    threads: HashMap<ThreadId, ThreadFacts>,
    /// The nav's parked Threads, in the Cockpit's stable park order (#21).
    parked: Vec<ThreadId>,
}

impl Facts {
    pub fn get(&self, thread: ThreadId) -> Option<&ThreadFacts> {
        self.threads.get(&thread)
    }

    /// The parked rows the nav draws, in order.
    pub fn parked(&self) -> &[ThreadId] {
        &self.parked
    }

    /// A Thread's Pane opened: everything about it, from scratch.
    pub fn opened(&mut self, cockpit: &Cockpit, thread: ThreadId) {
        self.refresh_slow(cockpit, thread);
        self.refresh_wall(cockpit, thread);
    }

    /// The pump streamed into a Thread: the wall card refolds — this is the
    /// seam that keeps L3 free of per-frame Block walks — and a turn that
    /// just ended may have moved the checkout, the other stated refresh
    /// moment (#29), so the slow facts follow it.
    pub fn streamed(&mut self, cockpit: &Cockpit, thread: ThreadId) {
        self.refresh_wall(cockpit, thread);
        if !cockpit.thread(thread).is_some_and(|open| open.busy()) {
            self.refresh_slow(cockpit, thread);
        }
    }

    /// The operator's own act — a prompt, an interrupt, an answer, a
    /// re-aim — or the watchdog's restart notice changed the transcript:
    /// the wall card refolds.
    pub fn acted(&mut self, cockpit: &Cockpit, thread: ThreadId) {
        self.refresh_wall(cockpit, thread);
    }

    /// The watchdog's tick: the checkout labels ride its slow cadence (#29)
    /// — the agent may have switched branches under a Pane — for every
    /// open Thread.
    pub fn tick(&mut self, cockpit: &Cockpit) {
        for thread in cockpit.threads() {
            self.refresh_slow(cockpit, thread);
        }
    }

    /// The parked set changed — a park, a revive, an import, a rename: the
    /// nav's parked rows are rebuilt. Each costs a `Store::peek`, one header
    /// line off disk, and the strings built here are what every frame after
    /// reuses. An unreadable log still gets a row — the Thread exists, and
    /// a nav that hides it would hide the problem — it just claims nothing
    /// it cannot know.
    pub fn parked_changed(&mut self, cockpit: &Cockpit) {
        let ordered = cockpit.parked_in_order().unwrap_or_default();
        for thread in &ordered {
            let facts = self.threads.entry(*thread).or_default();
            facts.name = SharedString::from(cockpit.display_title(*thread, true));
            let Ok(meta) = cockpit.peek(*thread) else {
                facts.provider = None;
                facts.project = None;
                facts.project_label = None;
                continue;
            };
            facts.provider = Some(meta.provider);
            facts.project = meta.project_id;
            facts.project_label = project_label(cockpit, meta.project_id, meta.workspace.as_ref());
            // The checkout, for a parked Thread, in the order that costs
            // least: the registry already knows a worktree's branch, and a
            // main checkout is asked `git` exactly once, ever.
            if facts.branch.is_none() {
                facts.branch = match meta.workspace.as_ref() {
                    Some(WorkspaceBinding::Worktree { path, .. }) => cockpit
                        .registry()
                        .branch_for(path)
                        .map(|branch| SharedString::from(branch.to_string())),
                    Some(WorkspaceBinding::Main { checkout }) => {
                        ferrite_core::workspace::checkout_branch(checkout).map(SharedString::from)
                    }
                    None => None,
                };
            }
        }
        self.parked = ordered;
    }

    /// The checkout label and the Project — a `git` call and a peek —
    /// nowhere near a frame.
    fn refresh_slow(&mut self, cockpit: &Cockpit, thread: ThreadId) {
        let open = cockpit.thread(thread);
        let cwd = ferrite_core::workspace::effective_cwd(
            open.and_then(|open| open.session_project_root()),
            open.and_then(|open| open.workspace()),
        )
        .map(std::path::Path::to_path_buf);
        let branch = cwd
            .and_then(|cwd| ferrite_core::workspace::checkout_branch(&cwd))
            .map(SharedString::from);
        let (project, project_label) = match cockpit.peek(thread) {
            Ok(meta) => (
                meta.project_id,
                project_label(cockpit, meta.project_id, meta.workspace.as_ref()),
            ),
            Err(_) => (None, None),
        };
        let name = SharedString::from(cockpit.display_title(thread, true));
        let facts = self.threads.entry(thread).or_default();
        facts.branch = branch;
        facts.project = project;
        facts.project_label = project_label;
        facts.name = name;
    }

    /// The name alone — after a first prompt or a rename, the one fact
    /// that moved.
    pub fn renamed(&mut self, cockpit: &Cockpit, thread: ThreadId) {
        let name = SharedString::from(cockpit.display_title(thread, true));
        self.threads.entry(thread).or_default().name = name;
    }

    /// What a Thread is called, from the cache; its number until a moment
    /// has named it.
    pub fn name(&self, thread: ThreadId) -> SharedString {
        self.threads
            .get(&thread)
            .filter(|facts| !facts.name.is_empty())
            .map(|facts| facts.name.clone())
            .unwrap_or_else(|| SharedString::from(format!("thread-{}", thread.get())))
    }

    /// Refold one Thread's wall card, wherever its transcript can change.
    fn refresh_wall(&mut self, cockpit: &Cockpit, thread: ThreadId) {
        let open = cockpit.thread(thread);
        let card = wall_card(
            open.map(|open| open.transcript()),
            open.and_then(|open| open.pending()),
        );
        self.threads.entry(thread).or_default().wall = card;
    }
}

fn project_label(
    cockpit: &Cockpit,
    project: Option<ProjectId>,
    workspace: Option<&WorkspaceBinding>,
) -> Option<SharedString> {
    if let Some(title) = project
        .and_then(|id| cockpit.registry().project(id))
        .map(|project| SharedString::from(project.title.clone()))
    {
        return Some(title);
    }
    let leaf = match workspace? {
        WorkspaceBinding::Main { checkout } => checkout.file_name(),
        WorkspaceBinding::Worktree { repo, .. } => repo.file_name(),
    }?;
    Some(SharedString::from(leaf.to_string_lossy().to_string()))
}
