//! What the nav and the Pane head say about a Thread that costs more than
//! an O(1) read: its checkout branch (a `git` call), its Project label (a
//! store peek), a parked Thread's provider (a peek) and the L3 wall card
//! (a walk of every Block). Cached here and refreshed by *moment* — a Pane
//! opened, a Thread streamed or was acted on, the watchdog's tick, the
//! parked set changed — never per frame. Which of the four facts a moment
//! refreshes is this module's knowledge alone, so a new door that opens,
//! parks or changes a Thread names the moment and cannot forget a cache.

use std::collections::HashMap;
use std::time::SystemTime;

use crate::pane::{wall_card, WallCard};
use ferrite_core::activity::Subject;
use ferrite_core::cockpit::Cockpit;
use ferrite_core::docview::{FileChange, Instruments};
use ferrite_core::store::Provider;
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::workspace::{BranchStatus, WorkspaceBinding};
use ferrite_core::ThreadId;
use gpui::SharedString;

/// One Thread's cached facts. `None` on any of them is honest — the row
/// draws that line empty and keeps its height rather than inventing a word.
#[derive(Default)]
pub struct ThreadFacts {
    /// What the Thread is called: the operator's title, else its first
    /// prompt cut at a word boundary, else `New thread` (`display_name`,
    /// never its number), cached by the
    /// same moments as the other facts (a first prompt sent, a rename, a
    /// park) so no frame reads a log to name a row.
    pub name: SharedString,
    /// The branch its effective cwd is actually on, read from git (#29) —
    /// the agent itself may switch branches, which is exactly why the
    /// header reads the repo and not the binding. A cwd outside any
    /// checkout has no text.
    pub branch: Option<SharedString>,
    /// What the checkout's second header line says (#29): drift against the
    /// upstream, the working tree's dirt, and the branch's PR and CI when
    /// `gh` can answer. Collected on the same slow cadence as the branch
    /// itself, and `None` until it has been.
    pub status: Option<BranchStatus>,
    /// Directory label and branch for every root attached to the Project.
    pub project_branches: Vec<(SharedString, SharedString)>,
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
    /// When the Thread was last written to (#21) — what the nav orders
    /// rows by and what its "40m / 2h / 3d" line says. `None` when the log
    /// cannot be stat'd; such a row claims nothing and sorts last.
    pub last_used: Option<SystemTime>,
    /// The Project's default branch (`workspace::default_branch`), read
    /// once per Project and shared by every Thread in it. `None` until
    /// read, or for a Thread no Project claims: then `main` and `master`
    /// stand in for it.
    pub default_branch: Option<SharedString>,
    /// Subagents observed for this Thread. Retained while parked so
    /// its navigation row keeps the last known count without reopening logs.
    pub subagents: usize,
    /// The wall cell's folded reading — everything the L3 recipe needs that
    /// is not an O(1) transcript read. A frame never walks Blocks at L3.
    pub wall: WallCard,
    /// Files edited anywhere in this Thread, including its subagents, with
    /// their rolled-up diff totals. Folded only when activity changes.
    pub changed_files: Vec<FileChange>,
    main_busy: bool,
    selected_wall: Option<(Subject, WallCard)>,
    /// Whether `branch` has been asked for (a `None` answer included), so a
    /// parked row's checkout costs one `git` call, ever.
    branch_asked: bool,
    /// Whether `subagents` is known: counted while open, or a parked log's
    /// replay has come back.
    subagents_known: bool,
    /// An open Thread's checkout, not known yet: read off the UI thread
    /// (`take_checkout_lookups`) unless `set_branches` answers first.
    branch_wanted: Option<std::path::PathBuf>,
}
impl ThreadFacts {
    /// Whether `branch` is the Project's default, which the nav row and the
    /// titlebar crumb leave unsaid.
    pub fn is_default_branch(&self, branch: &str) -> bool {
        match &self.default_branch {
            Some(default) => branch == default.as_ref(),
            None => matches!(branch, "main" | "master"),
        }
    }

    /// The checkout's branch, only when it says something: not the
    /// Project's default. The live status's branch wins over the cached one.
    pub fn off_default_branch(&self) -> Option<SharedString> {
        self.status
            .as_ref()
            .and_then(|status| status.branch.clone())
            .map(SharedString::from)
            .or_else(|| self.branch.clone())
            .filter(|branch| !self.is_default_branch(branch))
    }

    pub fn wall_for(&self, subject: &Subject) -> Option<&WallCard> {
        match subject {
            Subject::Main => Some(&self.wall),
            _ => self
                .selected_wall
                .as_ref()
                .filter(|(selected, _)| selected == subject)
                .map(|(_, card)| card),
        }
    }
}

pub struct Facts {
    threads: HashMap<ThreadId, ThreadFacts>,
    /// Each Project's default branch, read once (a `git` call or two) the
    /// first time a Thread of it is refreshed, never from a frame.
    default_branches: HashMap<ProjectId, SharedString>,
    /// The nav's parked Threads, in the Cockpit's stable park order (#21).
    parked: Vec<ThreadId>,
    /// Whether an untitled Thread is named from its first prompt (a
    /// setting), else by its number.
    auto_title: bool,
}

impl Default for Facts {
    fn default() -> Self {
        Self::with_auto_title(true)
    }
}

impl Facts {
    pub fn with_auto_title(auto_title: bool) -> Self {
        Self {
            threads: HashMap::new(),
            default_branches: HashMap::new(),
            parked: Vec::new(),
            auto_title,
        }
    }

    /// Change the naming rule; answers whether it changed.
    pub fn set_auto_title(&mut self, auto_title: bool) -> bool {
        let changed = self.auto_title != auto_title;
        self.auto_title = auto_title;
        changed
    }

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
    /// moment (#29), so the slow facts follow it (the checkout itself on
    /// the view's background refresh).
    pub fn streamed(&mut self, cockpit: &Cockpit, thread: ThreadId) {
        let was_busy = self
            .threads
            .get(&thread)
            .is_some_and(|facts| facts.main_busy);
        let busy = cockpit.thread(thread).is_some_and(|open| open.busy());
        self.refresh_wall(cockpit, thread);
        if was_busy && !busy {
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
            self.refresh_metadata(cockpit, thread);
        }
    }

    /// Adopt checkout labels and their status, collected away from the UI
    /// thread. The two travel together because one `git status` answers
    /// both, and a branch name without its drift would draw a header that
    /// contradicts itself for a tick.
    pub fn set_branches(&mut self, branches: Vec<(ThreadId, Option<BranchStatus>)>) {
        for (thread, status) in branches {
            let facts = self.threads.entry(thread).or_default();
            facts.branch = status
                .as_ref()
                .and_then(|status| status.branch.clone())
                .map(SharedString::from);
            facts.status = status;
            facts.branch_wanted = None;
        }
    }

    pub fn set_project_branches(
        &mut self,
        branches: Vec<(ThreadId, Vec<(SharedString, SharedString)>)>,
    ) {
        for (thread, project_branches) in branches {
            self.threads.entry(thread).or_default().project_branches = project_branches;
        }
    }

    /// The parked set changed — a park, a revive, an import, a rename: the
    /// nav's parked rows are rebuilt. Each costs a `Store::peek`, one header
    /// line off disk, and the strings built here are what every frame after
    /// reuses. An unreadable log still gets a row — the Thread exists, and
    /// a nav that hides it would hide the problem — it just claims nothing
    /// it cannot know.
    ///
    /// What costs more than a peek — a `git` call for the checkout, a replay
    /// of the whole log for the subagent count — is not done here: it is
    /// returned, once per Thread, for the caller to run off the UI thread
    /// and hand back through `parked_looked_up`. A launch with dozens of
    /// parked Threads would otherwise hold the first frame for seconds.
    pub fn parked_changed(&mut self, cockpit: &Cockpit) -> ParkedLookups {
        let ordered = cockpit.parked_in_order().unwrap_or_default();
        let mut lookups = ParkedLookups::default();
        for thread in &ordered {
            let last_used = cockpit.last_used(*thread);
            let facts = self.threads.entry(*thread).or_default();
            facts.last_used = last_used;
            facts.name = display_name(cockpit, *thread, self.auto_title);
            let Ok(meta) = cockpit.peek(*thread) else {
                facts.provider = None;
                facts.project = None;
                facts.project_label = None;
                continue;
            };
            facts.provider = Some(meta.provider);
            facts.project = meta.project_id;
            let checkout = match meta.workspace.as_ref() {
                Some(WorkspaceBinding::Worktree { repo, .. }) => Some(repo.as_path()),
                Some(WorkspaceBinding::Main { checkout }) => Some(checkout.as_path()),
                None => None,
            };
            facts.default_branch =
                default_branch_of(&mut self.default_branches, meta.project_id, checkout);
            facts.project_label = project_label(cockpit, meta.project_id, meta.workspace.as_ref());
            if !facts.subagents_known {
                facts.subagents_known = true;
                lookups.subagents.push(*thread);
            }
            // The checkout, for a parked Thread, in the order that costs
            // least: the registry already knows a worktree's branch, and a
            // main checkout is asked `git` exactly once, ever.
            if facts.branch.is_none() && !facts.branch_asked {
                facts.branch_asked = true;
                let checkout = match meta.workspace {
                    // A worktree the agent made (followed into, never
                    // registered) is asked git, once, like a main checkout.
                    Some(WorkspaceBinding::Worktree { path, .. }) => {
                        match cockpit.registry().branch_for(&path) {
                            Some(branch) => {
                                facts.branch = Some(SharedString::from(branch.to_string()));
                                None
                            }
                            None => Some(path),
                        }
                    }
                    Some(WorkspaceBinding::Main { checkout }) => Some(checkout),
                    None => None,
                };
                if let Some(checkout) = checkout {
                    lookups.branches.push((*thread, checkout));
                }
            }
        }
        self.parked = ordered;
        lookups
    }

    /// The answers to `parked_changed`'s lookups, back from off the UI
    /// thread. A Thread opened meanwhile keeps what its open Pane read —
    /// that answer is the newer one.
    pub fn parked_looked_up(&mut self, cockpit: &Cockpit, answers: ParkedAnswers) {
        for (thread, branch) in answers.branches {
            let facts = self.threads.entry(thread).or_default();
            if facts.branch.is_none() {
                facts.branch = branch.map(SharedString::from);
            }
        }
        for (thread, count) in answers.subagents {
            if cockpit.thread(thread).is_none() {
                self.threads.entry(thread).or_default().subagents = count;
            }
        }
    }

    /// The Project and its default branch — a peek, and `git` once per
    /// Project — nowhere near a frame. The checkout label is asked for
    /// here and read off the UI thread (`take_checkout_lookups`).
    fn refresh_slow(&mut self, cockpit: &Cockpit, thread: ThreadId) {
        let open = cockpit.thread(thread);
        let cwd = ferrite_core::workspace::effective_cwd(
            open.and_then(|open| open.session_project_root()),
            open.and_then(|open| open.workspace()),
        )
        .map(std::path::Path::to_path_buf);
        let (project, project_label) = match cockpit.peek(thread) {
            Ok(meta) => (
                meta.project_id,
                project_label(cockpit, meta.project_id, meta.workspace.as_ref()),
            ),
            Err(_) => (None, None),
        };
        let default_branch = default_branch_of(&mut self.default_branches, project, cwd.as_deref());
        let name = display_name(cockpit, thread, self.auto_title);
        let last_used = cockpit.last_used(thread);
        let facts = self.threads.entry(thread).or_default();
        facts.last_used = last_used;
        facts.default_branch = default_branch;
        // The checkout is `git`'s to say, 50-400ms a call on Windows: asked
        // here, on the UI thread, it held a launch for seconds (one call per
        // open Pane) and every turn's end for a beat. A known checkout
        // follows the view's periodic branch refresh instead. Unit tests
        // read it inline: the late answer's redraw would blank the cached
        // transcripts' test selectors (see `motion::live`).
        #[cfg(not(test))]
        {
            facts.branch_wanted = if facts.branch.is_none() { cwd } else { None };
        }
        #[cfg(test)]
        {
            facts.branch = cwd
                .as_deref()
                .and_then(ferrite_core::workspace::checkout_branch)
                .map(SharedString::from);
        }
        facts.branch_asked = true;
        facts.project = project;
        facts.project_label = project_label;
        facts.name = name;
    }

    /// Refresh everything except the checkout label. This path stays in the
    /// pump, so it must never launch Git.
    fn refresh_metadata(&mut self, cockpit: &Cockpit, thread: ThreadId) {
        let (project, project_label) = match cockpit.peek(thread) {
            Ok(meta) => (
                meta.project_id,
                project_label(cockpit, meta.project_id, meta.workspace.as_ref()),
            ),
            Err(_) => (None, None),
        };
        let name = display_name(cockpit, thread, self.auto_title);
        let last_used = cockpit.last_used(thread);
        let facts = self.threads.entry(thread).or_default();
        facts.last_used = last_used;
        facts.project = project;
        facts.project_label = project_label;
        facts.name = name;
    }

    /// The open Threads' checkouts still unknown, for the caller to read
    /// off the UI thread and hand back through `parked_looked_up`.
    pub fn take_checkout_lookups(&mut self) -> ParkedLookups {
        ParkedLookups {
            branches: self
                .threads
                .iter_mut()
                .filter_map(|(thread, facts)| Some((*thread, facts.branch_wanted.take()?)))
                .collect(),
            subagents: Vec::new(),
        }
    }

    /// The name alone — after a first prompt or a rename, the one fact
    /// that moved.
    pub fn renamed(&mut self, cockpit: &Cockpit, thread: ThreadId) {
        let name = display_name(cockpit, thread, self.auto_title);
        self.threads.entry(thread).or_default().name = name;
    }

    /// What a Thread is called, from the cache; `New thread` until a moment
    /// has named it (never its number).
    pub fn name(&self, thread: ThreadId) -> SharedString {
        self.threads
            .get(&thread)
            .filter(|facts| !facts.name.is_empty())
            .map(|facts| facts.name.clone())
            .unwrap_or_else(|| SharedString::from(NEW_THREAD))
    }

    /// Only the selected child needs a wall projection. Refresh at selection
    /// and native observations, keeping rendering free of transcript scans.
    pub fn selected(&mut self, cockpit: &Cockpit, thread: ThreadId, subject: &Subject) {
        let card = if *subject == Subject::Main {
            None
        } else {
            cockpit
                .thread(thread)
                .and_then(|open| open.activity().subject(subject))
                .map(|view| (subject.clone(), wall_card(Some(view.transcript()), None)))
        };
        self.threads.entry(thread).or_default().selected_wall = card;
    }

    /// When a Thread was last used, from the cache.
    pub fn last_used(&self, thread: ThreadId) -> Option<SystemTime> {
        self.threads.get(&thread).and_then(|facts| facts.last_used)
    }

    /// Refold one Thread's wall card, wherever its transcript can change.
    fn refresh_wall(&mut self, cockpit: &Cockpit, thread: ThreadId) {
        let open = cockpit.thread(thread);
        let card = wall_card(
            open.map(|open| open.transcript()),
            open.and_then(|open| open.pending()),
        );
        let last_used = cockpit.last_used(thread);
        let changed_files = open.map(changed_files);
        let facts = self.threads.entry(thread).or_default();
        facts.wall = card;
        if let Some(changed_files) = changed_files {
            facts.changed_files = changed_files;
        }
        if open.is_some() {
            facts.subagents = cockpit.subagent_count(thread).unwrap_or_default();
            facts.subagents_known = true;
        }
        // The wall refolds on exactly the moments that append to the log —
        // a stream, a prompt, an act — so recency rides it rather than
        // needing a moment of its own.
        if last_used.is_some() {
            facts.last_used = last_used;
        }
        facts.main_busy = open.is_some_and(|open| open.busy());
    }
}

/// A Project's default branch from the cache, read from `checkout` the
/// first time the Project is seen. A Thread no Project claims has none.
fn default_branch_of(
    cache: &mut HashMap<ProjectId, SharedString>,
    project: Option<ProjectId>,
    checkout: Option<&std::path::Path>,
) -> Option<SharedString> {
    let project = project?;
    if let Some(known) = cache.get(&project) {
        return Some(known.clone());
    }
    let read = SharedString::from(ferrite_core::workspace::default_branch(checkout?));
    cache.insert(project, read.clone());
    Some(read)
}

fn changed_files(open: ferrite_core::cockpit::ThreadView<'_>) -> Vec<FileChange> {
    let activity = open.activity();
    let mut changed = Vec::<FileChange>::new();
    let transcripts = std::iter::once(activity.main().transcript()).chain(
        activity
            .children()
            .into_iter()
            .map(|agent| agent.transcript()),
    );
    for transcript in transcripts {
        for file in Instruments::of(transcript).changed {
            match changed.iter_mut().find(|changed| changed.path == file.path) {
                Some(changed) => {
                    changed.added += file.added;
                    changed.removed += file.removed;
                }
                None => changed.push(file),
            }
        }
    }
    changed
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

/// How long ago, in the nav's own shorthand: `1m`, `40m`, `2h`, `3d`, `1w`,
/// then `12mo` and `2y`. One unit, never two — the row has a line's tail to
/// spend, and "2h" is the whole answer at a glance. Under a minute it says
/// nothing at all (C10: never `now`), and callers keep the slot's width so
/// the first minute moves nothing. A time in the future (a clock that
/// moved) says nothing rather than a negative.
pub fn since_label(last_used: SystemTime, now: SystemTime) -> SharedString {
    let secs = now.duration_since(last_used).unwrap_or_default().as_secs();
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    const WEEK: u64 = 7 * DAY;
    // A month is the mean Gregorian month, and a year twelve of them: the
    // row says "3mo", not a date, so the calendar's irregularity is below
    // what it claims.
    const MONTH: u64 = 2_629_746;
    const YEAR: u64 = 12 * MONTH;
    let text = match secs {
        s if s < MINUTE => String::new(),
        s if s < HOUR => format!("{}m", s / MINUTE),
        s if s < DAY => format!("{}h", s / HOUR),
        s if s < WEEK => format!("{}d", s / DAY),
        s if s < MONTH => format!("{}w", s / WEEK),
        s if s < YEAR => format!("{}mo", s / MONTH),
        s => format!("{}y", s / YEAR),
    };
    SharedString::from(text)
}

/// A Thread with no title and no prompt yet.
pub const NEW_THREAD: &str = "New thread";

/// What a Thread is called (rule 2.11): its real title when it has one;
/// before that, the first prompt cut at a word boundary (`auto`, the
/// operator's setting); with no prompt, `New thread`. Never `thread-{id}`.
fn display_name(cockpit: &Cockpit, thread: ThreadId, auto: bool) -> SharedString {
    let shown = cockpit.display_title(thread, auto);
    let titled = cockpit
        .thread(thread)
        .is_some_and(|open| open.title().is_some())
        || cockpit.peek(thread).is_ok_and(|meta| meta.title.is_some());
    if titled {
        return shown.into();
    }
    if shown == format!("thread-{}", thread.get()) {
        return NEW_THREAD.into();
    }
    at_word_boundary(&shown).into()
}

/// A provisional title cut short (`…`) ends on a whole word: the partial
/// word the character cut left behind is dropped.
fn at_word_boundary(title: &str) -> String {
    let Some(cut) = title.strip_suffix('\u{2026}') else {
        return title.to_string();
    };
    match cut.trim_end().rsplit_once(' ') {
        Some((words, _)) if !words.trim_end().is_empty() => {
            format!("{}\u{2026}", words.trim_end())
        }
        _ => title.to_string(),
    }
}

/// The reads too slow for the UI thread: a parked row's
/// (`Facts::parked_changed`), and an open Thread's checkout
/// (`Facts::take_checkout_lookups`).
#[derive(Default)]
pub struct ParkedLookups {
    /// Parked Threads whose checkout branch only `git` can say.
    pub branches: Vec<(ThreadId, std::path::PathBuf)>,
    /// Parked Threads whose subagent count needs a replay of their log.
    pub subagents: Vec<ThreadId>,
}

impl ParkedLookups {
    pub fn is_empty(&self) -> bool {
        self.branches.is_empty() && self.subagents.is_empty()
    }

    /// Run the lookups: blocking `git` calls and whole-log reads, so call
    /// this from a background task.
    pub fn run(self, logs: &ferrite_core::cockpit::LogReader) -> ParkedAnswers {
        ParkedAnswers {
            branches: self
                .branches
                .into_iter()
                .map(|(thread, checkout)| {
                    (thread, ferrite_core::workspace::checkout_branch(&checkout))
                })
                .collect(),
            subagents: self
                .subagents
                .into_iter()
                .map(|thread| (thread, logs.subagent_count(thread).unwrap_or_default()))
                .collect(),
        }
    }
}

/// What `ParkedLookups::run` found, for `Facts::parked_looked_up`.
pub struct ParkedAnswers {
    branches: Vec<(ThreadId, Option<String>)>,
    subagents: Vec<(ThreadId, usize)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn ago(secs: u64) -> SharedString {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10 * 365 * 24 * 3600);
        since_label(now - Duration::from_secs(secs), now)
    }

    /// A provisional title never ends on half a word, and a whole title is
    /// left alone.
    #[test]
    fn a_provisional_title_is_cut_at_a_word_boundary() {
        assert_eq!(
            at_word_boundary("Fix the flaky scrollbar fade in the transcri\u{2026}"),
            "Fix the flaky scrollbar fade in the\u{2026}"
        );
        assert_eq!(at_word_boundary("Fix the fade"), "Fix the fade");
        assert_eq!(
            at_word_boundary("Supercalifragilistic\u{2026}"),
            "Supercalifragilistic\u{2026}",
            "one long word keeps its cut"
        );
        assert_eq!(NEW_THREAD, "New thread");
        let facts = Facts::default();
        assert_eq!(facts.name(ThreadId::new(7)), NEW_THREAD, "never thread-7");
    }

    #[test]
    fn the_shorthand_climbs_one_unit_at_a_time() {
        assert_eq!(ago(0), "");
        assert_eq!(ago(59), "");
        assert_eq!(ago(60), "1m");
        let then = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        assert_eq!(since_label(then, then + Duration::from_secs(90)), "1m");
        assert_eq!(ago(40 * 60), "40m");
        assert_eq!(ago(2 * 3600), "2h");
        assert_eq!(ago(3 * 24 * 3600), "3d");
        assert_eq!(ago(7 * 24 * 3600), "1w");
        assert_eq!(ago(62 * 24 * 3600), "2mo");
        assert_eq!(ago(800 * 24 * 3600), "2y");
    }

    /// A launch with dozens of parked Threads drew an empty window for
    /// seconds: every row ran `git` and replayed its log on the UI thread.
    /// Rebuilding the rows now only hands those reads back, once per
    /// Thread, for the caller to run elsewhere.
    #[test]
    fn parked_rows_hand_back_their_slow_reads_once() {
        let dir = std::env::temp_dir().join(format!(
            "ferrite-facts-{}-parked-lookups",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let store = ferrite_core::store::Store::open(&dir).unwrap();
        let checkout = std::env::current_dir().unwrap();
        let (thread, writer) = store
            .create(
                Provider::Claude,
                None,
                WorkspaceBinding::Main {
                    checkout: checkout.clone(),
                },
            )
            .unwrap();
        drop(writer);
        let cockpit = Cockpit::new(store, Box::new(crate::demo::Spawn::new(false)));
        let mut facts = Facts::default();

        let lookups = facts.parked_changed(&cockpit);
        assert_eq!(facts.parked(), &[thread]);
        assert_eq!(lookups.branches, vec![(thread, checkout.clone())]);
        assert_eq!(lookups.subagents, vec![thread]);
        assert_eq!(facts.get(thread).unwrap().branch, None, "no git ran here");

        assert!(
            facts.parked_changed(&cockpit).is_empty(),
            "a rebuilt parked set asks nothing it already asked"
        );

        let answers = lookups.run(&cockpit.log_reader());
        facts.parked_looked_up(&cockpit, answers);
        assert_eq!(
            facts.get(thread).unwrap().branch,
            ferrite_core::workspace::checkout_branch(&checkout).map(SharedString::from),
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A clock that moved backwards must not print a negative age.
    #[test]
    fn a_future_stamp_says_nothing() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        assert_eq!(since_label(now + Duration::from_secs(500), now), "");
    }

    /// The Project's default is left unsaid; anything else is named. With
    /// no default read, `main` and `master` stand in for it.
    #[test]
    fn only_a_branch_off_the_default_is_named() {
        let mut facts = ThreadFacts {
            branch: Some("feat/nav".into()),
            ..ThreadFacts::default()
        };
        assert_eq!(facts.off_default_branch().as_deref(), Some("feat/nav"));
        facts.branch = Some("master".into());
        assert_eq!(facts.off_default_branch(), None);
        facts.default_branch = Some("trunk".into());
        assert_eq!(facts.off_default_branch().as_deref(), Some("master"));
        facts.branch = Some("trunk".into());
        assert_eq!(facts.off_default_branch(), None);
    }
}
