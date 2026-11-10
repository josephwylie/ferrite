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
//! Thread gone, its file deleted, its shell or page dead) is dropped.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use ferrite_core::ThreadId;
use serde::{Deserialize, Serialize};

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
    /// A subagent's drill-in, by its stored `AgentKey`.
    Agent(String),
}

impl SurfaceId {
    /// Whether a closed tab of this kind can come back: a closed shell or
    /// live page cannot.
    fn reopenable(&self) -> bool {
        !matches!(self, Self::Terminal(_) | Self::Browser)
    }
}

/// One Thread's Panel.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThreadSurfaces {
    pub(crate) open: bool,
    pub(crate) active: Option<SurfaceId>,
    pub(crate) tabs: Vec<SurfaceId>,
    pub(crate) maximized: bool,
    /// The operator's width; the active kind's floor widens it.
    pub(crate) width: f32,
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
            active: Some(SurfaceId::Overview),
            tabs: vec![SurfaceId::Overview],
            maximized: false,
            width: 0.,
            revision: 0,
            closed: Vec::new(),
        }
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
            self.active = self.tabs.get(at.min(self.tabs.len().saturating_sub(1))).cloned();
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
        if self.active.as_ref().is_some_and(|active| gone.contains(active)) {
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
                self.active = self.tabs.get(at.min(self.tabs.len().saturating_sub(1))).cloned();
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
        if id.reopenable() {
            self.closed.retain(|closed| *closed != id);
            self.closed.push(id);
        }
    }
}

/// Every Thread's Panel.
#[derive(Default)]
pub(crate) struct Surfaces {
    threads: HashMap<ThreadId, ThreadSurfaces>,
}

impl Surfaces {
    pub(crate) fn get(&self, thread: ThreadId) -> Option<&ThreadSurfaces> {
        self.threads.get(&thread)
    }

    /// `thread`'s Panel, made with `open` as its first state.
    pub(crate) fn entry(&mut self, thread: ThreadId, open: bool) -> &mut ThreadSurfaces {
        self.threads
            .entry(thread)
            .or_insert_with(|| ThreadSurfaces::new(open))
    }

    pub(crate) fn threads_mut(&mut self) -> impl Iterator<Item = (ThreadId, &mut ThreadSurfaces)> {
        self.threads.iter_mut().map(|(thread, surfaces)| (*thread, surfaces))
    }

    pub(crate) fn remove(&mut self, thread: ThreadId) {
        self.threads.remove(&thread);
    }
}

// ------------------------------------------------------------ persistence

const SCHEMA_VERSION: u32 = 1;
pub(crate) const FILE_NAME: &str = "panel.json";

#[derive(Serialize, Deserialize)]
struct Persisted {
    version: u32,
    threads: Vec<PersistedThread>,
}

#[derive(Serialize, Deserialize)]
struct PersistedThread {
    thread: u64,
    open: bool,
    tabs: Vec<SurfaceId>,
    active: Option<SurfaceId>,
    maximized: bool,
    width: f32,
}

impl Surfaces {
    /// Read `<dir>/panel.json`, keeping only the Surfaces `resolves` still
    /// finds (`migrate`). None there is no Panels; a file of another
    /// version is refused.
    pub(crate) fn load(
        dir: &Path,
        resolves: impl Fn(ThreadId, &SurfaceId) -> bool,
    ) -> io::Result<Self> {
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
        Ok(migrate(persisted, resolves))
    }

    /// Write `<dir>/panel.json`, so that a crash at any moment leaves the
    /// old file or the new one.
    pub(crate) fn save(&self, dir: &Path) -> io::Result<()> {
        let mut threads: Vec<PersistedThread> = self
            .threads
            .iter()
            .map(|(thread, surfaces)| PersistedThread {
                thread: thread.get(),
                open: surfaces.open,
                tabs: surfaces.tabs.clone(),
                active: surfaces.active.clone(),
                maximized: surfaces.maximized,
                width: surfaces.width,
            })
            .collect();
        threads.sort_by_key(|thread| thread.thread);
        let bytes = serde_json::to_vec_pretty(&Persisted {
            version: SCHEMA_VERSION,
            threads,
        })
        .map_err(io::Error::other)?;
        std::fs::create_dir_all(dir)?;
        ferrite_core::store::replace_file(&dir.join(FILE_NAME), &bytes)
    }
}

/// The stored Panels as they still resolve: a Surface `resolves` refuses is
/// dropped, and a shown one dropped hands the Panel to the nearest tab left
/// (or none).
fn migrate(persisted: Persisted, resolves: impl Fn(ThreadId, &SurfaceId) -> bool) -> Surfaces {
    let mut threads = HashMap::new();
    for stored in persisted.threads {
        let thread = ThreadId::new(stored.thread);
        if !resolves(thread, &SurfaceId::Overview) {
            continue;
        }
        let mut tabs: Vec<SurfaceId> = Vec::new();
        // Where the shown tab stood among those kept, kept or not.
        let mut shown_at = None;
        for id in stored.tabs {
            if stored.active.as_ref() == Some(&id) {
                shown_at = Some(tabs.len());
            }
            if resolves(thread, &id) && !tabs.contains(&id) {
                tabs.push(id);
            }
        }
        let active = shown_at.and_then(|at| tabs.get(at.min(tabs.len().saturating_sub(1))).cloned());
        threads.insert(
            thread,
            ThreadSurfaces {
                open: stored.open,
                active,
                tabs,
                maximized: stored.maximized,
                width: stored.width,
                revision: 0,
                closed: Vec::new(),
            },
        );
    }
    Surfaces { threads }
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
        assert_eq!(panel.tabs, [SurfaceId::Overview, file("a.rs"), file("b.rs")]);
        assert_eq!(panel.active, Some(file("b.rs")));
        panel.upsert(file("a.rs"));
        assert_eq!(panel.tabs.len(), 3, "deduped by path");
        assert_eq!(panel.active, Some(file("a.rs")));
    }

    #[test]
    fn reveal_is_refused_after_an_operator_action() {
        let mut panel = ThreadSurfaces::new(true);
        let seen = panel.revision();
        assert!(panel.reveal(SurfaceId::Changes, seen), "nothing happened since");
        assert_eq!(panel.active, Some(SurfaceId::Changes));
        assert_eq!(panel.revision(), seen, "a reveal is not the operator's");

        let seen = panel.revision();
        panel.activate(&SurfaceId::Overview);
        assert!(!panel.reveal(SurfaceId::Files, seen), "the operator chose Overview");
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
        assert_eq!(panel.reopen_closed(), None, "a shell or a page cannot come back");
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
        assert_eq!(panel.active, Some(SurfaceId::Overview), "at the end, the one before");
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
        assert_eq!(panel.active, Some(file("a")), "the shown tab was left of it");
        let mut panel = tabs();
        panel.close_right(&file("a"));
        assert_eq!(panel.active, Some(file("a")), "the shown tab went: `a` takes it");
        assert_eq!(panel.reopen_closed(), Some(file("c")), "the shown one comes back first");

        let mut panel = tabs();
        panel.move_to(&file("c"), 0);
        assert_eq!(panel.tabs, [file("c"), SurfaceId::Overview, file("a"), file("b")]);
        assert_eq!(panel.active, Some(file("c")));
        panel.move_to(&file("c"), 99);
        assert_eq!(panel.tabs.last(), Some(&file("c")), "clamped to the end");

        let mut panel = tabs();
        panel.close_all();
        assert!(panel.tabs.is_empty());
        assert_eq!(panel.active, None);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ferrite-surfaces-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn tabs_active_maximized_and_width_survive_a_restart() {
        let dir = scratch("roundtrip");
        let thread = ThreadId::new(7);
        let mut surfaces = Surfaces::default();
        let panel = surfaces.entry(thread, true);
        panel.upsert(file("a.rs"));
        panel.upsert(SurfaceId::Changes);
        panel.activate(&file("a.rs"));
        panel.maximized = true;
        panel.width = 512.;
        surfaces.save(&dir).unwrap();

        let loaded = Surfaces::load(&dir, |_, _| true).unwrap();
        let panel = loaded.get(thread).expect("the Thread's Panel");
        assert_eq!(panel.tabs, [SurfaceId::Overview, file("a.rs"), SurfaceId::Changes]);
        assert_eq!(panel.active, Some(file("a.rs")));
        assert!(panel.maximized && panel.open);
        assert_eq!(panel.width, 512.);
    }

    #[test]
    fn a_file_of_another_version_is_refused() {
        let dir = scratch("version");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE_NAME), br#"{"version": 99, "threads": []}"#).unwrap();
        let error = Surfaces::load(&dir, |_, _| true).err().expect("refused");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(Surfaces::load(&scratch("missing"), |_, _| true).unwrap().threads.is_empty());
    }

    #[test]
    fn a_surface_that_no_longer_resolves_is_dropped_and_the_active_falls_back() {
        let dir = scratch("migrate");
        let (kept, gone) = (ThreadId::new(1), ThreadId::new(2));
        let mut surfaces = Surfaces::default();
        let panel = surfaces.entry(kept, true);
        for name in ["a", "deleted", "c"] {
            panel.upsert(file(name));
        }
        panel.activate(&file("deleted"));
        surfaces.entry(gone, true).upsert(SurfaceId::Changes);
        let lone = ThreadId::new(3);
        surfaces.entry(lone, true).close_all();
        surfaces.entry(lone, true).upsert(file("deleted"));
        surfaces.save(&dir).unwrap();

        let loaded = Surfaces::load(&dir, |thread, id| {
            thread != gone && *id != file("deleted")
        })
        .unwrap();
        assert!(loaded.get(gone).is_none(), "a deleted Thread's Panel goes");
        let panel = loaded.get(kept).unwrap();
        assert_eq!(panel.tabs, [SurfaceId::Overview, file("a"), file("c")]);
        assert_eq!(panel.active, Some(file("c")), "the nearest tab left");
        let panel = loaded.get(lone).unwrap();
        assert!(panel.tabs.is_empty());
        assert_eq!(panel.active, None, "nothing left to show");
    }
}
