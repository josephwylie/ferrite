//! The Panel's Surfaces (ADR 0039, after T3 Code's `rightPanelStore`):
//! each Thread keeps one ordered list of tabs, the one shown, whether its
//! Panel is open, maximized, and its width.
//!
//! A Surface's identity is its kind plus what it shows (`SurfaceId`), and
//! that is the dedupe key: every opener goes through one door, `upsert`,
//! which brings a Surface already open forward and adds one otherwise, so
//! opening anything twice leaves one tab. An agent opens with `reveal`,
//! which is refused once the operator has acted since the revision it read:
//! an agent may reveal a Surface but not take the Panel from a tab the
//! operator just chose.
//!
//! The tabs, the active one, maximized and the width outlive a restart
//! (`Persisted`, a versioned `panel.json` beside `settings.json`); on load
//! every Surface is checked again and one that no longer resolves (its
//! file deleted, its shell or page dead, its kind no longer in this build)
//! is dropped. A Panel is saved by its Thread's `SavedThread`, so Thread 3
//! here and Thread 3 on box keep a Panel each: the window holds those of
//! the machine it is on (`hold`) and writes the others back as it read
//! them.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::{Path, PathBuf};

use ferrite_core::{MachineId, MachineKey, Rehold, SavedThread, ThreadId, ThreadKey};
use serde::{Deserialize, Deserializer, Serialize};

/// What one tab shows. Singletons per Thread, then one per resource.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SurfaceId {
    /// Environment, Context and Connectors: the Panel's old sections.
    Overview,
    Changes,
    Files,
    Background,
    /// A Preview: a file at a line, Markdown, a command's output, a page.
    File(PathBuf),
    /// The Thread's live browser (ADR 0036), its own page tabs inside.
    Browser,
    /// One shell, by the number that names it (`Terminal N`).
    Terminal(u64),
    /// One side chat, by its tab's id (the fork lands after it opens).
    SideChat(u64),
    /// The pull request of the Thread's branch, by its number.
    PullRequest(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Close {
    /// Nothing outlives the tab but its identity: the Surface reads what it
    /// shows from the Thread.
    Hide,
    /// The view is put away; the durable subject (a file on disk) stays, and
    /// reopening builds the view again from it.
    Release,
    /// The resource ends with its tab, and nothing brings it back. Names
    /// what ends, for the menu's `End <what>`.
    End(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Restart {
    /// The tab returns, and builds what it shows from its subject as it is
    /// shown; a subject that is gone says so in the tab.
    Restore,
    /// The resource ended with the window (no process or page survives), so
    /// the tab does not return.
    Ends,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lifecycle {
    pub(crate) close: Close,
    pub(crate) restart: Restart,
}

impl Lifecycle {
    const fn new(close: Close, restart: Restart) -> Self {
        Self { close, restart }
    }

    pub(crate) fn reopenable(self) -> bool {
        !matches!(self.close, Close::End(_))
    }
}

impl SurfaceId {
    pub(crate) fn lifecycle(&self) -> Lifecycle {
        match self {
            Self::Overview
            | Self::Changes
            | Self::Files
            | Self::Background
            | Self::PullRequest(_) => Lifecycle::new(Close::Hide, Restart::Restore),
            Self::File(_) => Lifecycle::new(Close::Release, Restart::Restore),
            Self::Browser => Lifecycle::new(Close::End("page"), Restart::Ends),
            Self::Terminal(_) => Lifecycle::new(Close::End("shell"), Restart::Ends),
            Self::SideChat(_) => Lifecycle::new(Close::End("side chat"), Restart::Ends),
        }
    }
}

/// Which of a pull request's views its Surface shows (T3's `detailTab`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PrTab {
    #[default]
    Summary,
    Guide,
    Timeline,
    Code,
}

impl PrTab {
    pub(crate) const ALL: [Self; 4] = [Self::Summary, Self::Guide, Self::Timeline, Self::Code];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Summary => "Summary",
            Self::Guide => "Guide",
            Self::Timeline => "Timeline",
            Self::Code => "Code",
        }
    }
}

/// One Thread's Panel.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThreadSurfaces {
    pub(crate) open: bool,
    pub(crate) details_inline_open: bool,
    /// Whether the details card's `Previous agents` shows the finished
    /// agents or only their count; folded at first, as T3's is.
    pub(crate) previous_agents_open: bool,
    pub(crate) active: Option<SurfaceId>,
    pub(crate) tabs: Vec<SurfaceId>,
    pub(crate) maximized: bool,
    /// The operator's width; the active kind's floor widens it.
    pub(crate) width: f32,
    /// Each pull request's chosen view, by number: what its Surface
    /// remembers (Summary when unset).
    pub(crate) pr_tabs: BTreeMap<u64, PrTab>,
    /// Bumped by every operator action: what `reveal` checks.
    revision: u64,
    /// Closed tabs that can come back, the last closed at the end.
    closed: Vec<SurfaceId>,
}

impl ThreadSurfaces {
    /// A Thread's Panel before anything was opened in it: `Overview`, so a
    /// Thread's Panel opens on its environment as it always has.
    pub(crate) fn new(open: bool) -> Self {
        Self {
            open,
            details_inline_open: true,
            previous_agents_open: false,
            active: Some(SurfaceId::Overview),
            tabs: vec![SurfaceId::Overview],
            maximized: false,
            width: 0.,
            pr_tabs: BTreeMap::new(),
            revision: 0,
            closed: Vec::new(),
        }
    }

    /// The view pull request `number`'s Surface shows.
    pub(crate) fn pr_tab(&self, number: u64) -> PrTab {
        self.pr_tabs.get(&number).copied().unwrap_or_default()
    }

    /// The one door: bring `id` forward if it has a tab, else add one at
    /// the end; either way it is shown and the Panel opens. Says whether a
    /// tab was added.
    pub(crate) fn upsert(&mut self, id: SurfaceId) -> bool {
        self.revision += 1;
        self.put(id)
    }

    fn put(&mut self, id: SurfaceId) -> bool {
        let added = !self.tabs.contains(&id);
        if added {
            self.tabs.push(id.clone());
        }
        self.active = Some(id);
        self.open = true;
        added
    }

    /// The operator shows or hides the Panel: an action, which a `reveal`
    /// read before it may not undo.
    pub(crate) fn set_open(&mut self, open: bool) {
        self.revision += 1;
        self.open = open;
    }

    /// The operator maximizes the Panel, or puts it back: an action.
    pub(crate) fn set_maximized(&mut self, maximized: bool) {
        self.revision += 1;
        self.maximized = maximized;
    }

    /// The revision an agent reads before it decides to `reveal`.
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// An agent's open: `upsert`, unless the operator has acted since
    /// `seen`. Not an operator action itself.
    pub(crate) fn reveal(&mut self, id: SurfaceId, seen: u64) -> bool {
        if self.revision != seen {
            return false;
        }
        self.put(id);
        true
    }

    /// Show the tab `id`, if there is one.
    pub(crate) fn activate(&mut self, id: &SurfaceId) {
        if self.tabs.contains(id) {
            self.revision += 1;
            self.active = Some(id.clone());
        }
    }

    /// Close the tab `id`; shown, the tab now in its place (or the one
    /// before, at the end) is shown instead.
    pub(crate) fn close(&mut self, id: &SurfaceId) {
        let Some(at) = self.tabs.iter().position(|tab| tab == id) else {
            return;
        };
        self.revision += 1;
        let gone = self.tabs.remove(at);
        if self.active.as_ref() == Some(&gone) {
            self.active = self
                .tabs
                .get(at.min(self.tabs.len().saturating_sub(1)))
                .cloned();
        }
        self.remember(gone);
    }

    /// Every tab but `id`, which is shown.
    pub(crate) fn close_others(&mut self, id: &SurfaceId) {
        if !self.tabs.contains(id) {
            return;
        }
        self.revision += 1;
        let gone: Vec<SurfaceId> = self.tabs.iter().filter(|tab| *tab != id).cloned().collect();
        self.tabs = vec![id.clone()];
        self.active = Some(id.clone());
        self.remember_all(gone);
    }

    /// The tabs right of `id`; a shown one among them hands the Panel to
    /// `id`.
    pub(crate) fn close_right(&mut self, id: &SurfaceId) {
        let Some(at) = self.tabs.iter().position(|tab| tab == id) else {
            return;
        };
        self.revision += 1;
        let gone = self.tabs.split_off(at + 1);
        if self
            .active
            .as_ref()
            .is_some_and(|active| gone.contains(active))
        {
            self.active = Some(id.clone());
        }
        self.remember_all(gone);
    }

    /// Every tab: the Panel stays on, showing "Open a surface".
    pub(crate) fn close_all(&mut self) {
        self.revision += 1;
        self.active = None;
        let gone = std::mem::take(&mut self.tabs);
        self.remember_all(gone);
    }

    /// Drag reorder: `id` to `index` (clamped); the shown tab stays shown.
    pub(crate) fn move_to(&mut self, id: &SurfaceId, index: usize) {
        let Some(from) = self.tabs.iter().position(|tab| tab == id) else {
            return;
        };
        self.revision += 1;
        let tab = self.tabs.remove(from);
        self.tabs.insert(index.min(self.tabs.len()), tab);
    }

    pub(crate) fn can_reopen(&self) -> bool {
        !self.closed.is_empty()
    }

    /// Bring back the last tab closed that can come back, shown.
    pub(crate) fn reopen_closed(&mut self) -> Option<SurfaceId> {
        let id = self.closed.pop()?;
        self.upsert(id.clone());
        Some(id)
    }

    /// Drop the tab `id` without remembering it: its resource is gone (a
    /// shell exited, a side chat retired, a file deleted).
    pub(crate) fn forget(&mut self, id: &SurfaceId) {
        if let Some(at) = self.tabs.iter().position(|tab| tab == id) {
            self.tabs.remove(at);
            if self.active.as_ref() == Some(id) {
                self.active = self
                    .tabs
                    .get(at.min(self.tabs.len().saturating_sub(1)))
                    .cloned();
            }
        }
        self.closed.retain(|closed| closed != id);
    }

    fn remember_all(&mut self, gone: Vec<SurfaceId>) {
        // The shown one last, so it is the first to come back (T3's
        // `closedInOrder`).
        let active = self.active.clone();
        let (shown, rest): (Vec<_>, Vec<_>) =
            gone.into_iter().partition(|id| active.as_ref() == Some(id));
        for id in rest.into_iter().chain(shown) {
            self.remember(id);
        }
    }

    fn remember(&mut self, id: SurfaceId) {
        if id.lifecycle().reopenable() {
            self.closed.retain(|closed| *closed != id);
            self.closed.push(id);
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum CloseSelection {
    One(SurfaceId),
    Others(SurfaceId),
    Right(SurfaceId),
    All,
}

#[derive(Clone, Debug)]
pub(crate) enum SurfaceAction {
    Open(SurfaceId),
    Reveal { id: SurfaceId, seen: u64 },
    Activate(SurfaceId),
    Close(CloseSelection),
    Forget(SurfaceId),
    Reopen,
    SetOpen(bool),
    ToggleMaximized,
    Resize(f32),
    Move { id: SurfaceId, index: usize },
    ToggleDetails,
    TogglePreviousAgents,
    SelectPrTab { number: u64, tab: PrTab },
}

/// Every Thread's Panel. Mutations keep their persistence revision here.
#[derive(Clone, Default)]
pub(crate) struct Surfaces {
    /// The Panels of the machines the window holds.
    threads: HashMap<ThreadKey, ThreadSurfaces>,
    /// What saved files call each machine the window holds.
    held: HashMap<MachineId, MachineKey>,
    /// The Panels of machines the window does not hold, as saved: written
    /// back as they were read, and held again when the window holds their
    /// machine.
    elsewhere: BTreeMap<SavedThread, ThreadSurfaces>,
    /// What each machine the window let go of was held as, until it holds
    /// that machine again (`Rehold`).
    let_go: HashMap<MachineKey, MachineId>,
    revision: u64,
}

impl Surfaces {
    pub(crate) fn get(&self, thread: ThreadKey) -> Option<&ThreadSurfaces> {
        self.threads.get(&thread)
    }

    /// `thread`'s Panel, made with `open` as its first state.
    fn entry(&mut self, thread: ThreadKey, open: bool) -> &mut ThreadSurfaces {
        self.threads
            .entry(thread)
            .or_insert_with(|| ThreadSurfaces::new(open))
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// Hold the Panels of `machine`, which saved files call `saved_as`,
    /// each by the key `key` gives its Thread there. The machine it was,
    /// when the window let it go.
    pub(crate) fn hold(
        &mut self,
        machine: MachineId,
        saved_as: MachineKey,
        key: impl Fn(ThreadId) -> ThreadKey,
    ) -> Option<Rehold> {
        let was = self.let_go.remove(&saved_as);
        let held: Vec<SavedThread> = self
            .elsewhere
            .keys()
            .filter(|saved| saved.machine == saved_as)
            .cloned()
            .collect();
        for saved in held {
            let surfaces = self.elsewhere.remove(&saved).unwrap();
            self.threads.insert(key(saved.thread), surfaces);
        }
        self.held.insert(machine, saved_as);
        was.map(|was| Rehold { was, now: machine })
    }

    /// The window let go of `machine`: its Panels are kept as they are,
    /// every tab, until the window holds that machine again. The window
    /// did not restart, so a tab whose resource ended with the machine (a
    /// shell) says so in its place; only a restart drops those tabs.
    pub(crate) fn let_go(&mut self, machine: MachineId) {
        let Some(saved_as) = self.held.remove(&machine) else {
            return;
        };
        let gone: Vec<(ThreadKey, ThreadSurfaces)> = self
            .threads
            .extract_if(|key, _| key.machine() == machine)
            .collect();
        for (key, surfaces) in gone {
            let saved = SavedThread {
                machine: saved_as.clone(),
                thread: key.thread(),
            };
            self.elsewhere.insert(saved, surfaces);
        }
        self.let_go.insert(saved_as, machine);
    }

    /// The machines whose Panels are held.
    pub(crate) fn machines(&self) -> impl Iterator<Item = MachineId> + '_ {
        self.held.keys().copied()
    }

    /// `thread` as it is saved; a Thread of a machine not held (a test's)
    /// is this computer's.
    fn saved(&self, thread: ThreadKey) -> SavedThread {
        SavedThread {
            machine: self
                .held
                .get(&thread.machine())
                .cloned()
                .unwrap_or_default(),
            thread: thread.thread(),
        }
    }

    /// Returns false only when an agent's reveal loses to an operator action.
    pub(crate) fn apply(&mut self, thread: ThreadKey, open: bool, action: SurfaceAction) -> bool {
        let existed = self.threads.contains_key(&thread);
        let saved = self.saved(thread);
        let state = self.entry(thread, open);
        let before = PersistedThread::of(saved.clone(), state);
        let mut accepted = true;
        match action {
            SurfaceAction::Open(id) => {
                state.upsert(id);
            }
            SurfaceAction::Reveal { id, seen } => accepted = state.reveal(id, seen),
            SurfaceAction::Activate(id) => state.activate(&id),
            SurfaceAction::Close(selection) => match selection {
                CloseSelection::One(id) => state.close(&id),
                CloseSelection::Others(id) => state.close_others(&id),
                CloseSelection::Right(id) => state.close_right(&id),
                CloseSelection::All => state.close_all(),
            },
            SurfaceAction::Forget(id) => state.forget(&id),
            SurfaceAction::Reopen => {
                state.reopen_closed();
            }
            SurfaceAction::SetOpen(open) => state.set_open(open),
            SurfaceAction::ToggleMaximized => state.set_maximized(!state.maximized),
            SurfaceAction::Resize(width) => {
                if state.maximized {
                    state.set_maximized(false);
                }
                state.width = width;
            }
            SurfaceAction::Move { id, index } => state.move_to(&id, index),
            SurfaceAction::ToggleDetails => state.details_inline_open = !state.details_inline_open,
            SurfaceAction::TogglePreviousAgents => state.previous_agents_open = !state.previous_agents_open,
            SurfaceAction::SelectPrTab { number, tab } => {
                state.pr_tabs.insert(number, tab);
            }
        }
        if !existed || PersistedThread::of(saved, state) != before {
            self.revision += 1;
        }
        accepted
    }

    /// Keep only the Threads `keep` says still exist. The others' tabs come
    /// back, for what each held to be released.
    #[must_use]
    pub(crate) fn retain(&mut self, keep: impl Fn(ThreadKey) -> bool) -> Vec<(ThreadKey, Vec<SurfaceId>)> {
        let gone: Vec<_> = self
            .threads
            .extract_if(|thread, _| !keep(*thread))
            .map(|(thread, state)| (thread, state.tabs))
            .collect();
        if !gone.is_empty() {
            self.revision += 1;
        }
        gone
    }
}

// ------------------------------------------------------------ persistence

const SCHEMA_VERSION: u32 = 1;
pub(crate) const FILE_NAME: &str = "panel.json";

#[derive(PartialEq, Serialize, Deserialize)]
struct Persisted {
    version: u32,
    threads: Vec<PersistedThread>,
}

#[derive(PartialEq, Serialize, Deserialize)]
struct PersistedThread {
    #[serde(deserialize_with = "saved_or_bare")]
    thread: SavedThread,
    open: bool,
    #[serde(default = "details_inline_default")]
    details_inline_open: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    previous_agents_open: bool,
    tabs: Vec<StoredTab>,
    active: Option<StoredTab>,
    maximized: bool,
    width: f32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pr_tabs: BTreeMap<u64, PrTab>,
}

/// A Panel saved before Threads were saved with their machine names its
/// Thread by a bare id: this computer's (ADR 0045).
fn saved_or_bare<'de, D: Deserializer<'de>>(deserializer: D) -> Result<SavedThread, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Saved(SavedThread),
        Bare(ThreadId),
    }
    Ok(match Stored::deserialize(deserializer)? {
        Stored::Saved(saved) => saved,
        Stored::Bare(thread) => SavedThread {
            machine: MachineKey(None),
            thread,
        },
    })
}

/// A stored tab: a Surface this build knows, or one of a kind it no longer
/// has (a retired Surface, as a subagent's drill-in is), kept as written
/// so the shown tab is still found among the tabs, then dropped.
#[derive(PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum StoredTab {
    Known(SurfaceId),
    Retired(serde_json::Value),
}

impl PersistedThread {
    fn of(thread: SavedThread, surfaces: &ThreadSurfaces) -> Self {
        Self {
            thread,
            open: surfaces.open,
            details_inline_open: surfaces.details_inline_open,
            previous_agents_open: surfaces.previous_agents_open,
            tabs: surfaces.tabs.iter().cloned().map(StoredTab::Known).collect(),
            active: surfaces.active.clone().map(StoredTab::Known),
            maximized: surfaces.maximized,
            width: surfaces.width,
            pr_tabs: surfaces.pr_tabs.clone(),
        }
    }
}

fn details_inline_default() -> bool {
    true
}

impl Surfaces {
    /// Read `<dir>/panel.json`, keeping only the Surfaces `resolves` still
    /// finds (`restored`), every Panel as saved until the window `hold`s
    /// its machine's. None there is no Panels; a file of another version
    /// is refused.
    pub(crate) fn load(dir: &Path, resolves: impl Fn(&SurfaceId) -> bool) -> io::Result<Self> {
        let bytes = match std::fs::read(dir.join(FILE_NAME)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error),
        };
        let persisted: Persisted = serde_json::from_slice(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if persisted.version != SCHEMA_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("panel version {} is not supported", persisted.version),
            ));
        }
        let elsewhere = persisted
            .threads
            .into_iter()
            .map(|stored| (stored.thread.clone(), restored(stored, &resolves)))
            .collect();
        Ok(Self {
            elsewhere,
            ..Self::default()
        })
    }

    fn persisted(&self) -> Persisted {
        let here = self
            .threads
            .iter()
            .map(|(key, surfaces)| PersistedThread::of(self.saved(*key), surfaces));
        let elsewhere = self
            .elsewhere
            .iter()
            .map(|(saved, surfaces)| PersistedThread::of(saved.clone(), surfaces));
        let mut threads: Vec<PersistedThread> = here.chain(elsewhere).collect();
        threads.sort_by(|a, b| a.thread.cmp(&b.thread));
        Persisted {
            version: SCHEMA_VERSION,
            threads,
        }
    }

    /// This computer's `thread`'s Panel as `<dir>/panel.json` saved it.
    #[cfg(test)]
    pub(crate) fn saved_in(dir: &Path, thread: ThreadId) -> Option<ThreadSurfaces> {
        let saved = SavedThread {
            machine: MachineKey(None),
            thread,
        };
        Self::load(dir, |_| true).unwrap().elsewhere.remove(&saved)
    }

    /// Write `<dir>/panel.json`, so that a crash at any moment leaves the
    /// old file or the new one.
    pub(crate) fn save(&self, dir: &Path) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(&self.persisted()).map_err(io::Error::other)?;
        std::fs::create_dir_all(dir)?;
        ferrite_core::disk::replace_file(&dir.join(FILE_NAME), &bytes)
    }
}

/// A stored Panel as it still resolves: a Surface `resolves` refuses is
/// dropped, and a shown one dropped hands the Panel to the nearest tab left
/// (or none).
fn restored(stored: PersistedThread, resolves: impl Fn(&SurfaceId) -> bool) -> ThreadSurfaces {
    let mut tabs: Vec<SurfaceId> = Vec::new();
    // Where the shown tab stood among those kept, kept or not.
    let mut shown_at = None;
    for tab in stored.tabs {
        if stored.active.as_ref() == Some(&tab) {
            shown_at = Some(tabs.len());
        }
        let StoredTab::Known(id) = tab else {
            continue;
        };
        if resolves(&id) && !tabs.contains(&id) {
            tabs.push(id);
        }
    }
    let active = shown_at.and_then(|at| tabs.get(at.min(tabs.len().saturating_sub(1))).cloned());
    let mut pr_tabs = stored.pr_tabs;
    pr_tabs.retain(|number, _| tabs.contains(&SurfaceId::PullRequest(*number)));
    ThreadSurfaces {
        open: stored.open,
        details_inline_open: stored.details_inline_open,
        previous_agents_open: stored.previous_agents_open,
        active,
        tabs,
        maximized: stored.maximized,
        width: stored.width,
        pr_tabs,
        revision: 0,
        closed: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> SurfaceId {
        SurfaceId::File(PathBuf::from(name))
    }

    #[test]
    fn upsert_of_an_open_id_activates_it_and_adds_no_tab() {
        let mut panel = ThreadSurfaces::new(false);
        assert!(panel.upsert(SurfaceId::Changes), "a new id is added");
        assert_eq!(panel.tabs, [SurfaceId::Overview, SurfaceId::Changes]);
        assert_eq!(panel.active, Some(SurfaceId::Changes));
        assert!(panel.open, "and the Panel opens");
        panel.activate(&SurfaceId::Overview);
        assert!(!panel.upsert(SurfaceId::Changes), "already open");
        assert_eq!(panel.tabs, [SurfaceId::Overview, SurfaceId::Changes]);
        assert_eq!(panel.active, Some(SurfaceId::Changes));
    }

    #[test]
    fn a_second_file_is_a_second_tab_and_the_first_stays() {
        let mut panel = ThreadSurfaces::new(true);
        panel.upsert(file("a.rs"));
        panel.upsert(file("b.rs"));
        assert_eq!(
            panel.tabs,
            [SurfaceId::Overview, file("a.rs"), file("b.rs")]
        );
        assert_eq!(panel.active, Some(file("b.rs")));
        panel.upsert(file("a.rs"));
        assert_eq!(panel.tabs.len(), 3, "deduped by path");
        assert_eq!(panel.active, Some(file("a.rs")));
    }

    #[test]
    fn reveal_is_refused_after_an_operator_action() {
        let mut panel = ThreadSurfaces::new(true);
        let seen = panel.revision();
        assert!(
            panel.reveal(SurfaceId::Changes, seen),
            "nothing happened since"
        );
        assert_eq!(panel.active, Some(SurfaceId::Changes));
        assert_eq!(panel.revision(), seen, "a reveal is not the operator's");

        let seen = panel.revision();
        panel.activate(&SurfaceId::Overview);
        assert!(
            !panel.reveal(SurfaceId::Files, seen),
            "the operator chose Overview"
        );
        assert_eq!(panel.active, Some(SurfaceId::Overview));
        assert!(!panel.tabs.contains(&SurfaceId::Files));

        for act in [
            |panel: &mut ThreadSurfaces| panel.close(&SurfaceId::Changes),
            |panel: &mut ThreadSurfaces| panel.move_to(&SurfaceId::Overview, 1),
            |panel: &mut ThreadSurfaces| {
                panel.upsert(SurfaceId::Background);
            },
        ] {
            let mut panel = ThreadSurfaces::new(true);
            panel.upsert(SurfaceId::Changes);
            let seen = panel.revision();
            act(&mut panel);
            assert!(!panel.reveal(SurfaceId::Files, seen));
        }
    }

    #[test]
    fn operator_precedence_is_independent_of_the_persistence_revision() {
        let thread = here(ThreadId::new(1));
        let mut surfaces = Surfaces::default();
        surfaces.apply(thread, true, SurfaceAction::Open(SurfaceId::Overview));
        let saved = surfaces.revision();
        let seen = surfaces.get(thread).unwrap().revision();
        surfaces.apply(thread, true, SurfaceAction::Activate(SurfaceId::Overview));
        assert_eq!(surfaces.revision(), saved, "the persisted tabs are unchanged");
        assert!(!surfaces.apply(
            thread,
            true,
            SurfaceAction::Reveal { id: SurfaceId::Files, seen },
        ));
        assert_eq!(surfaces.get(thread).unwrap().active, Some(SurfaceId::Overview));
        assert_eq!(surfaces.revision(), saved, "a refused reveal needs no save");
    }

    #[test]
    fn reopen_closed_skips_terminals_and_live_browser_tabs() {
        let mut panel = ThreadSurfaces::new(true);
        panel.upsert(SurfaceId::Changes);
        panel.upsert(SurfaceId::Terminal(1));
        panel.upsert(SurfaceId::Browser);
        panel.close(&SurfaceId::Changes);
        panel.close(&SurfaceId::Terminal(1));
        panel.close(&SurfaceId::Browser);
        assert_eq!(panel.reopen_closed(), Some(SurfaceId::Changes));
        assert_eq!(panel.active, Some(SurfaceId::Changes));
        assert_eq!(
            panel.reopen_closed(),
            None,
            "a shell or a page cannot come back"
        );
    }

    #[test]
    fn closing_the_shown_tab_shows_its_neighbour() {
        let mut panel = ThreadSurfaces::new(true);
        panel.upsert(file("a"));
        panel.upsert(file("b"));
        panel.activate(&file("a"));
        panel.close(&file("a"));
        assert_eq!(panel.active, Some(file("b")), "the tab now in its place");
        panel.close(&file("b"));
        assert_eq!(
            panel.active,
            Some(SurfaceId::Overview),
            "at the end, the one before"
        );
        panel.close(&SurfaceId::Overview);
        assert_eq!(panel.active, None, "Open a surface");
        assert!(panel.open, "the Panel stays on");
    }

    #[test]
    fn close_others_close_right_and_move_to_keep_the_active_id() {
        let tabs = || {
            let mut panel = ThreadSurfaces::new(true);
            for name in ["a", "b", "c"] {
                panel.upsert(file(name));
            }
            panel
        };

        let mut panel = tabs();
        panel.close_others(&file("b"));
        assert_eq!(panel.tabs, [file("b")]);
        assert_eq!(panel.active, Some(file("b")));

        let mut panel = tabs();
        panel.activate(&file("a"));
        panel.close_right(&file("b"));
        assert_eq!(panel.tabs, [SurfaceId::Overview, file("a"), file("b")]);
        assert_eq!(
            panel.active,
            Some(file("a")),
            "the shown tab was left of it"
        );
        let mut panel = tabs();
        panel.close_right(&file("a"));
        assert_eq!(
            panel.active,
            Some(file("a")),
            "the shown tab went: `a` takes it"
        );
        assert_eq!(
            panel.reopen_closed(),
            Some(file("c")),
            "the shown one comes back first"
        );

        let mut panel = tabs();
        panel.move_to(&file("c"), 0);
        assert_eq!(
            panel.tabs,
            [file("c"), SurfaceId::Overview, file("a"), file("b")]
        );
        assert_eq!(panel.active, Some(file("c")));
        panel.move_to(&file("c"), 99);
        assert_eq!(panel.tabs.last(), Some(&file("c")), "clamped to the end");

        let mut panel = tabs();
        panel.close_all();
        assert!(panel.tabs.is_empty());
        assert_eq!(panel.active, None);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ferrite-surfaces-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// `thread` on this computer.
    fn here(thread: ThreadId) -> ThreadKey {
        ThreadKey::fixture(0, thread.get())
    }

    /// `dir`'s Panels, as a window holding this computer holds them.
    fn held(dir: &Path) -> Surfaces {
        let mut surfaces = Surfaces::load(dir, |_| true).unwrap();
        surfaces.hold(here(ThreadId::new(1)).machine(), MachineKey(None), here);
        surfaces
    }

    #[test]
    fn tabs_active_maximized_and_width_survive_a_restart() {
        let dir = scratch("roundtrip");
        let thread = here(ThreadId::new(7));
        let mut surfaces = Surfaces::default();
        let panel = surfaces.entry(thread, true);
        panel.upsert(file("a.rs"));
        panel.upsert(SurfaceId::Changes);
        panel.upsert(SurfaceId::PullRequest(48));
        panel.pr_tabs.insert(48, PrTab::Timeline);
        panel.activate(&file("a.rs"));
        panel.maximized = true;
        panel.width = 512.;
        panel.details_inline_open = false;
        panel.previous_agents_open = true;
        surfaces.save(&dir).unwrap();

        let loaded = held(&dir);
        let panel = loaded.get(thread).expect("the Thread's Panel");
        assert_eq!(
            panel.tabs,
            [
                SurfaceId::Overview,
                file("a.rs"),
                SurfaceId::Changes,
                SurfaceId::PullRequest(48)
            ]
        );
        assert_eq!(panel.pr_tab(48), PrTab::Timeline, "a PR keeps its view");
        assert_eq!(panel.pr_tab(7), PrTab::Summary);
        assert_eq!(panel.active, Some(file("a.rs")));
        assert!(panel.maximized && panel.open);
        assert_eq!(panel.width, 512.);
        assert!(!panel.details_inline_open);
        assert!(panel.previous_agents_open, "opened Previous agents stay open");
    }

    #[test]
    fn details_inline_visibility_defaults_open_and_is_saved_per_thread() {
        let dir = scratch("details-inline");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE_NAME), br#"{"version":1,"threads":[{"thread":7,"open":false,"tabs":["overview"],"active":"overview","maximized":false,"width":0}]}"#).unwrap();
        let mut surfaces = held(&dir);
        let first = here(ThreadId::new(7));
        let second = here(ThreadId::new(8));
        assert!(
            surfaces.get(first).unwrap().details_inline_open,
            "older files start inline context visible"
        );
        surfaces.entry(first, false).details_inline_open = false;
        assert!(surfaces.entry(second, false).details_inline_open);
        surfaces.save(&dir).unwrap();
        let restored = held(&dir);
        assert!(!restored.get(first).unwrap().details_inline_open);
        assert!(restored.get(second).unwrap().details_inline_open);
    }

    #[test]
    fn a_file_of_another_version_is_refused() {
        let dir = scratch("version");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE_NAME), br#"{"version": 99, "threads": []}"#).unwrap();
        let error = Surfaces::load(&dir, |_| true).err().expect("refused");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(Surfaces::load(&scratch("missing"), |_| true)
            .unwrap()
            .persisted()
            .threads
            .is_empty());
    }

    #[test]
    fn a_surface_that_no_longer_resolves_is_dropped_and_the_active_falls_back() {
        let dir = scratch("migrate");
        let kept = here(ThreadId::new(1));
        let mut surfaces = Surfaces::default();
        let panel = surfaces.entry(kept, true);
        for name in ["a", "deleted", "c"] {
            panel.upsert(file(name));
        }
        panel.activate(&file("deleted"));
        let lone = here(ThreadId::new(3));
        surfaces.entry(lone, true).close_all();
        surfaces.entry(lone, true).upsert(file("deleted"));
        surfaces.save(&dir).unwrap();

        let mut loaded = Surfaces::load(&dir, |id| *id != file("deleted")).unwrap();
        loaded.hold(here(ThreadId::new(1)).machine(), MachineKey(None), here);
        let panel = loaded.get(kept).unwrap();
        assert_eq!(panel.tabs, [SurfaceId::Overview, file("a"), file("c")]);
        assert_eq!(panel.active, Some(file("c")), "the nearest tab left");
        let panel = loaded.get(lone).unwrap();
        assert!(panel.tabs.is_empty());
        assert_eq!(panel.active, None, "nothing left to show");
    }

    /// A tab of a kind this build no longer has (the retired subagent
    /// drill-in) is dropped on load; the file and its other tabs load, and
    /// a shown one hands the Panel to its nearest neighbour.
    #[test]
    fn a_retired_tab_kind_is_dropped_and_every_other_tab_loads() {
        let dir = scratch("retired");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(FILE_NAME),
            br#"{"version":1,"threads":[
                {"thread":1,"open":true,"tabs":["overview",{"agent":"claude:s:toolu_a"},{"file":"a.rs"}],"active":{"agent":"claude:s:toolu_a"},"maximized":false,"width":420},
                {"thread":2,"open":false,"tabs":["changes",{"pull_request":48}],"active":"changes","maximized":true,"width":300}
            ]}"#,
        )
        .unwrap();
        let loaded = held(&dir);
        let first = loaded.get(here(ThreadId::new(1))).unwrap();
        assert_eq!(first.tabs, [SurfaceId::Overview, file("a.rs")]);
        assert_eq!(first.active, Some(file("a.rs")), "the nearest tab left");
        let second = loaded.get(here(ThreadId::new(2))).unwrap();
        assert_eq!(second.tabs, [SurfaceId::Changes, SurfaceId::PullRequest(48)]);
        assert_eq!(second.active, Some(SurfaceId::Changes));
        assert!(second.maximized);
    }

    /// Thread 3 here and Thread 3 on box are two Threads: each keeps its
    /// own Panel, held while the window holds its machine and written back
    /// as it was while the window does not.
    #[test]
    fn a_panel_keeps_two_machines_threads_apart() {
        let dir = scratch("two-machines");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(FILE_NAME),
            br#"{"version":1,"threads":[
                {"thread":{"machine":null,"thread":3},"open":true,"tabs":["overview","changes"],"active":"changes","maximized":false,"width":420},
                {"thread":{"machine":"me@box","thread":3},"open":false,"tabs":["files"],"active":"files","maximized":true,"width":300}
            ]}"#,
        )
        .unwrap();
        let mut surfaces = held(&dir);
        let (mine, boxes) = (here(ThreadId::new(3)), ThreadKey::fixture(1, 3));
        assert_eq!(surfaces.get(mine).unwrap().active, Some(SurfaceId::Changes));
        assert!(surfaces.get(boxes).is_none(), "box is not held yet");
        surfaces.apply(mine, true, SurfaceAction::Open(SurfaceId::Background));

        let on_box = |thread: ThreadId| ThreadKey::fixture(1, thread.get());
        surfaces.hold(boxes.machine(), MachineKey(Some("me@box".into())), on_box);
        let panel = surfaces.get(boxes).expect("box's Panel, as box left it");
        assert_eq!(panel.tabs, [SurfaceId::Files]);
        assert!(panel.maximized && !panel.open);
        assert_eq!(
            surfaces.get(mine).unwrap().active,
            Some(SurfaceId::Background),
            "both held at once"
        );
        surfaces.apply(boxes, true, SurfaceAction::Open(SurfaceId::Changes));

        surfaces.let_go(boxes.machine());
        assert!(surfaces.get(boxes).is_none(), "box let go");
        surfaces.save(&dir).unwrap();
        let mut loaded = held(&dir);
        assert_eq!(
            loaded.get(mine).unwrap().active,
            Some(SurfaceId::Background),
            "this computer's Panel saved"
        );
        loaded.hold(boxes.machine(), MachineKey(Some("me@box".into())), on_box);
        assert_eq!(
            loaded.get(boxes).unwrap().active,
            Some(SurfaceId::Changes),
            "box's saved as it was let go"
        );
    }

    /// A Panel saved before Threads were saved with their machine names
    /// its Thread by a bare id: it reads as this computer's, and is written
    /// back with its machine.
    #[test]
    fn a_panel_saved_before_machines_reads_as_this_computers() {
        let dir = scratch("bare-ids");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE_NAME), br#"{"version":1,"threads":[{"thread":7,"open":true,"tabs":["overview","changes"],"active":"changes","maximized":false,"width":0}]}"#).unwrap();
        let surfaces = held(&dir);
        let panel = surfaces.get(here(ThreadId::new(7))).expect("this computer's Thread 7");
        assert_eq!(panel.active, Some(SurfaceId::Changes));
        surfaces.save(&dir).unwrap();
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join(FILE_NAME)).unwrap()).unwrap();
        assert_eq!(
            written["threads"][0]["thread"],
            serde_json::json!({"machine": null, "thread": 7})
        );
    }
}
