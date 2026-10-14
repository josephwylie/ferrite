//! The floats' cockpit half (FL-1…FL-9, FL-16, FL-18): the command palette
//! (⌘K, ⌘G), the shortcuts sheet (`?`), the status line's mode cycle (⇧⇥),
//! the notifications list's keys, the palette's veil and the toasts' place.
//!
//! Every float key is an action bound in its float's context (`keymap`)
//! and handled here, at the cockpit's root (`register_palette_actions`):
//! the floats are views that only draw what the cockpit hands them.

use super::*;
use crate::float::{self, FloatPlace};
use crate::palette::{
    Act, Command, Palette, PaletteScope, Row as PaletteRow, Section as PaletteSection,
};
use crate::shortcuts::Shortcuts;

/// What the floats hold between frames: the palette and the shortcuts sheet
/// while they are up, and the focus the notifications list's keys ride.
pub(super) struct Floats {
    pub palette: Option<Entity<Palette>>,
    pub shortcuts: Option<Entity<Shortcuts>>,
    pub notice_focus: FocusHandle,
}

impl Floats {
    pub(super) fn new(cx: &mut Context<CockpitView>) -> Self {
        Self {
            palette: None,
            shortcuts: None,
            notice_focus: cx.focus_handle(),
        }
    }
}

/// A Thread's state as the palette's context word reads it: the lexicon's
/// word for every state that has one — a waiting Thread's what it waits
/// for, as its nav row says — a parked Thread's age, nothing for idle.
fn state_word(row: &nav::ThreadRow, age: SharedString) -> SharedString {
    use crate::theme::words;
    match row.status {
        nav::RowStatus::Working => words::WORKING.into(),
        nav::RowStatus::Background => words::BACKGROUND.into(),
        nav::RowStatus::NeedsYou => match row.tail {
            nav::NavTail::Waiting(kind) => kind.into(),
            _ => words::NEEDS_YOU.into(),
        },
        nav::RowStatus::Failing => words::FAILING.into(),
        nav::RowStatus::Failed => words::FAILED.into(),
        nav::RowStatus::Done => words::DONE.into(),
        nav::RowStatus::Idle => SharedString::default(),
        nav::RowStatus::Parked => age,
    }
}

/// `scope · word`, either part left out when empty.
fn context_words(scope: Option<SharedString>, word: SharedString) -> SharedString {
    match (scope, word.is_empty()) {
        (Some(scope), false) => format!("{scope} \u{b7} {word}").into(),
        (Some(scope), true) => scope,
        (None, false) => word,
        (None, true) => SharedString::default(),
    }
}

/// One importable session as the picker lists it: its title (or first
/// prompt) as a Title would read it, then its project, provider and age.
fn session_row(
    candidate: ferrite_core::import::Candidate,
    into: Option<PaneIdentity>,
    now: std::time::SystemTime,
) -> PaletteRow {
    let name = candidate
        .title
        .as_deref()
        .map(ferrite_core::cockpit::title_from_prompt)
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| {
            candidate
                .path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
    let project = candidate
        .cwd
        .as_deref()
        .and_then(std::path::Path::file_name)
        .map(|name| name.to_string_lossy().into_owned());
    let age = candidate
        .modified
        .map(|at| crate::facts::since_label(at, now).to_string())
        .filter(|age| !age.is_empty());
    let context = project
        .into_iter()
        .chain([provider_label(candidate.provider).to_string()])
        .chain(age)
        .collect::<Vec<_>>()
        .join(" \u{b7} ");
    PaletteRow {
        section: PaletteSection::Sessions,
        name: name.into(),
        context: context.into(),
        status: None,
        detail: None,
        keys: None,
        act: Act::Adopt {
            path: candidate.path,
            into,
        },
    }
}

impl CockpitView {
    /// The floats' hook on the cockpit's root: the palette's, the shortcuts
    /// sheet's, the status line's and the notifications list's actions.
    pub(super) fn register_palette_actions(&self, root: Div, cx: &mut Context<Self>) -> Div {
        use crate::{notifications as bell, palette, shortcuts, status};
        root.on_action(
            cx.listener(|view, _: &palette::Toggle, window, cx| view.toggle_palette(window, cx)),
        )
        .on_action(cx.listener(|view, _: &palette::OpenGroups, window, cx| {
            view.open_palette(PaletteScope::Groups, "", window, cx)
        }))
        .on_action(
            cx.listener(|view, _: &palette::ShowParked, window, cx| view.show_parked(window, cx)),
        )
        .on_action(
            cx.listener(|view, _: &palette::ReviewBranch, window, cx| {
                view.review_branch(window, cx)
            }),
        )
        .on_action(cx.listener(|view, _: &palette::SelectNext, _, cx| view.palette_step(1, cx)))
        .on_action(
            cx.listener(|view, _: &palette::SelectPrevious, _, cx| view.palette_step(-1, cx)),
        )
        .on_action(cx.listener(|view, _: &palette::Confirm, window, cx| {
            view.palette_confirm(false, window, cx)
        }))
        .on_action(cx.listener(|view, _: &palette::Preview, window, cx| {
            view.palette_confirm(true, window, cx)
        }))
        .on_action(cx.listener(|view, _: &palette::Dismiss, _, cx| view.close_palette(cx)))
        .on_action(
            cx.listener(|view, _: &shortcuts::Toggle, window, cx| {
                view.toggle_shortcuts(window, cx)
            }),
        )
        .on_action(
            cx.listener(|view, _: &shortcuts::ScrollUp, _, cx| view.scroll_shortcuts(-1.0, cx)),
        )
        .on_action(
            cx.listener(|view, _: &shortcuts::ScrollDown, _, cx| view.scroll_shortcuts(1.0, cx)),
        )
        .on_action(cx.listener(|view, _: &shortcuts::Dismiss, _, cx| view.close_shortcuts(cx)))
        .on_action(
            cx.listener(|view, _: &status::CycleMode, window, cx| view.cycle_mode(window, cx)),
        )
        .on_action(cx.listener(|view, _: &bell::SelectNext, _, cx| view.step_notices(1, cx)))
        .on_action(cx.listener(|view, _: &bell::SelectPrevious, _, cx| view.step_notices(-1, cx)))
        .on_action(cx.listener(|view, _: &bell::Open, _, cx| view.open_notice_at_cursor(cx)))
        .on_action(cx.listener(|view, _: &bell::Dismiss, _, cx| view.dismiss_notice_at_cursor(cx)))
        .on_action(cx.listener(|view, _: &bell::Close, _, cx| {
            if view.bell.open {
                view.bell.open = false;
                cx.notify();
            }
        }))
    }

    /// Whether a float holds the keyboard this frame (the palette's line,
    /// the shortcuts sheet, the notifications list): render's focus rule
    /// leaves it alone.
    pub(super) fn floats_hold_focus(&self) -> bool {
        self.floats.palette.is_some() || self.floats.shortcuts.is_some() || self.bell.open
    }

    /// Whether the palette or the shortcuts sheet is up (the veil is).
    pub(super) fn sheet_float_open(&self) -> bool {
        self.floats.palette.is_some() || self.floats.shortcuts.is_some()
    }

    /// Close every other surface a float opening replaces.
    fn clear_for_float(&mut self) {
        self.popover = None;
        self.context_menu = None;
        self.session_controls = None;
        self.mode_picker = None;
        self.context_usage = None;
        self.context_checks = None;
        self.changed_files_card = None;
        self.bell.open = false;
        self.floats.shortcuts = None;
    }

    // ------------------------------------------------------------ palette

    /// ⌘K: the palette, open or shut.
    fn toggle_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.floats.palette.is_some() {
            self.close_palette(cx);
        } else {
            self.open_palette(PaletteScope::All, "", window, cx);
        }
    }

    /// Open the palette over the board (FL-1): every Thread, the Projects
    /// and Groups to jump to, then every command (`All`), or the Groups
    /// alone (⌘G), its line seeded with `query` and holding the keyboard.
    pub(crate) fn open_palette(
        &mut self,
        scope: PaletteScope,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = match scope {
            PaletteScope::All => {
                let mut rows = self.palette_thread_rows(window);
                rows.extend(self.palette_project_rows());
                rows.extend(self.palette_group_rows());
                rows.extend(self.palette_command_rows(window, cx));
                rows
            }
            PaletteScope::Groups => self.palette_group_rows(),
            // Listed off the UI thread: `open_import_palette`.
            PaletteScope::Sessions => Vec::new(),
        };
        self.clear_for_float();
        let palette = cx.new(|cx| Palette::new(scope, rows, query, cx));
        let focus = palette.read(cx).input.focus_handle(cx);
        self.floats.palette = Some(palette);
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Shut the palette without touching anything else (esc, the veil): a
    /// turn runs on.
    pub(super) fn close_palette(&mut self, cx: &mut Context<Self>) {
        if self.floats.palette.take().is_some() {
            cx.notify();
        }
    }

    fn palette_step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(palette) = self.floats.palette.clone() {
            palette.update(cx, |palette, cx| palette.step(delta, cx));
        }
    }

    /// ⏎ runs the cursor's row and closes; ⇥ (`preview`) opens a Thread
    /// beside the current Pane and closes, and does nothing on any other
    /// row.
    fn palette_confirm(&mut self, preview: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(palette) = self.floats.palette.clone() else {
            return;
        };
        let Some(row) = palette.read(cx).selected().cloned() else {
            return;
        };
        if preview && !matches!(row.act, Act::Thread(_)) {
            return;
        }
        self.floats.palette = None;
        self.run_palette_act(row.act, preview, window, cx);
        cx.notify();
    }

    /// Run one palette row.
    pub(super) fn run_palette_act(
        &mut self,
        act: Act,
        preview: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match act {
            Act::Thread(thread) if preview => {
                self.open_beside(beside::Beside::Thread(thread), window, cx);
            }
            Act::Thread(thread) => self.land_on_thread(thread, cx),
            Act::Group(group) => self.enter_group(group, cx),
            Act::Project(project) => {
                if let Some(thread) = self.project_lead(project) {
                    self.land_on_thread(thread, cx);
                }
            }
            Act::Command(command) => self.run_command(command, window, cx),
            Act::Adopt { path, into } => self.adopt_file(into, &path, window, cx),
        }
    }

    // ------------------------------------------------------ session import

    /// The import picker (`import a session`, `/import`): the palette over
    /// the CLI sessions there are to resume, newest first and searched like
    /// any palette row — title, project, provider. The listing runs off the
    /// UI thread, so the picker opens at once and fills when it answers.
    /// `into` is the blank Pane a pick takes the place of, if any.
    pub(super) fn open_import_palette(
        &mut self,
        into: Option<PaneIdentity>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_palette(PaletteScope::Sessions, "", window, cx);
        self.discovery_request = self.discovery_request.wrapping_add(1);
        let roots = self.session_file_roots.clone();
        match self.cockpit.discover_sessions(roots, IMPORT_ROWS_MAX) {
            Some(reply) => {
                self.pending_discovery = Some(PendingDiscovery {
                    into,
                    request_id: self.discovery_request,
                    started: std::time::Instant::now(),
                    reply,
                });
                self.show_sessions(Vec::new(), "searching sessions\u{2026}".into(), cx);
            }
            // No listing thread to be had: the plain walk, here.
            None => {
                let found =
                    ferrite_core::import::candidates(&self.session_file_roots, IMPORT_ROWS_MAX);
                self.list_sessions(into, Ok(found), cx);
            }
        }
    }

    /// The listing's answer, once it lands — and only into the picker that
    /// asked for it: one closed or reopened since drops it.
    pub(super) fn poll_session_discovery(&mut self, cx: &mut Context<Self>) {
        use std::sync::mpsc::TryRecvError;
        let Some(pending) = self.pending_discovery.take() else {
            return;
        };
        let current = self.discovery_request == pending.request_id
            && self
                .floats
                .palette
                .as_ref()
                .is_some_and(|palette| palette.read(cx).scope == PaletteScope::Sessions);
        if !current {
            return;
        }
        let found = match pending.reply.try_recv() {
            Ok(found) => found.map_err(|error| error.to_string()),
            Err(TryRecvError::Empty) if pending.started.elapsed() < Duration::from_secs(15) => {
                self.pending_discovery = Some(pending);
                return;
            }
            Err(TryRecvError::Empty) => Err("the listing timed out".into()),
            Err(error) => Err(error.to_string()),
        };
        self.list_sessions(pending.into, found, cx);
    }

    /// Fill the import picker: a row per session, or what the empty list
    /// means.
    fn list_sessions(
        &mut self,
        into: Option<PaneIdentity>,
        found: Result<Vec<ferrite_core::import::Candidate>, String>,
        cx: &mut Context<Self>,
    ) {
        let now = ferrite_core::clock::system_time();
        match found {
            Ok(found) => {
                let roots = self
                    .session_file_roots
                    .iter()
                    .map(|(_, root)| root.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" or ");
                let rows = found
                    .into_iter()
                    .map(|candidate| session_row(candidate, into, now))
                    .collect();
                self.show_sessions(
                    rows,
                    format!("no CLI sessions found under {roots}").into(),
                    cx,
                );
            }
            Err(error) => self.show_sessions(
                Vec::new(),
                format!("could not list sessions: {error}").into(),
                cx,
            ),
        }
    }

    fn show_sessions(
        &mut self,
        rows: Vec<PaletteRow>,
        empty: SharedString,
        cx: &mut Context<Self>,
    ) {
        if let Some(palette) = self.floats.palette.clone() {
            palette.update(cx, |palette, cx| palette.set_rows(rows, empty, cx));
        }
    }

    /// The focused Pane, when it is blank enough for an import to take its
    /// place: a draft with nothing typed, or a Thread that still offers
    /// import.
    pub(super) fn import_target(&self, cx: &gpui::App) -> Option<PaneIdentity> {
        let pane = self.panes.get(self.focused())?;
        let blank = match pane.identity {
            PaneIdentity::Draft(_) => pane.composer.read(cx).is_empty(),
            PaneIdentity::Thread(thread) => {
                pane::offers_import(self.cockpit.thread(thread).map(|open| open.transcript()))
            }
        };
        blank.then_some(pane.identity)
    }

    /// The shown board's Panes in head order: reading order, top to bottom
    /// then left to right (⌘1…⌘9, R5).
    pub(super) fn board_order(&self, window: &Window) -> Vec<usize> {
        let mut rects = self.pane_rects(window);
        rects.sort_by_key(|(_, rect)| (rect.y.round() as i64, rect.x.round() as i64));
        rects.into_iter().map(|(index, _)| index).collect()
    }

    /// The palette's Threads (FL-2): Project by Project in the registry's
    /// order (Threads no Project claims last), each Project's rows in the
    /// nav's tree order — creation order, a Group's members together where
    /// its first member stands, under that member's Project (R14) — then
    /// that Project's parked Threads. Each Thread once.
    pub(super) fn palette_thread_rows(&self, window: &Window) -> Vec<PaletteRow> {
        let board: Vec<ThreadId> = self
            .board_order(window)
            .into_iter()
            .filter_map(|index| self.panes[index].thread())
            .collect();
        let shown_group = match self.cockpit.roster().view() {
            View::Group(group) => self
                .cockpit
                .groups()
                .get(group)
                .map(|group| SharedString::from(group.display_title().to_lowercase())),
            View::Solo => None,
        };
        let project_of = |thread: ThreadId| self.facts.get(thread).and_then(|facts| facts.project);
        let open_threads: Vec<ThreadId> = {
            let mut open = self.cockpit.threads();
            open.sort();
            open
        };
        // The parked Threads the palette offers: those used within the
        // last day. Older ones wait in the nav's parked fold (`show
        // parked`, ⌘⇧P).
        let recent = ferrite_core::clock::system_time();
        let parked: Vec<ThreadId> = self
            .facts
            .parked()
            .iter()
            .copied()
            .filter(|thread| !open_threads.contains(thread))
            .filter(|thread| {
                self.facts.last_used(*thread).is_some_and(|at| {
                    recent
                        .duration_since(at)
                        .is_ok_and(|ago| ago < std::time::Duration::from_secs(24 * 60 * 60))
                })
            })
            .collect();
        // Items in creation order: a loose Thread by itself, a Group by its
        // first member (so it sits where that member was made).
        enum Item {
            Thread(ThreadId),
            Group(Vec<ThreadId>),
        }
        let mut items: Vec<(ThreadId, Option<ProjectId>, Item)> = Vec::new();
        for group in self.cockpit.groups().iter() {
            let members: Vec<ThreadId> = group
                .members
                .iter()
                .copied()
                .filter(|thread| open_threads.contains(thread))
                .collect();
            if let Some(first) = members.first().copied() {
                items.push((first, project_of(first), Item::Group(members)));
            }
        }
        for thread in &open_threads {
            if self.cockpit.groups().of(*thread).is_none() {
                items.push((*thread, project_of(*thread), Item::Thread(*thread)));
            }
        }
        items.sort_by_key(|(first, _, _)| *first);
        let mut projects: Vec<Option<ProjectId>> = self
            .cockpit
            .registry()
            .projects()
            .iter()
            .map(|project| Some(project.id))
            .collect();
        projects.push(None);
        let mut seen: std::collections::HashSet<ThreadId> = std::collections::HashSet::new();
        let mut rows: Vec<PaletteRow> = Vec::new();
        let now = ferrite_core::clock::system_time();
        for project in projects {
            let mut ordered: Vec<ThreadId> = Vec::new();
            for (_, item_project, item) in &items {
                if *item_project != project {
                    continue;
                }
                match item {
                    Item::Thread(thread) => ordered.push(*thread),
                    Item::Group(members) => ordered.extend(members.iter().copied()),
                }
            }
            // A Group's guests from other Projects read under their own.
            let mut guests: Vec<ThreadId> = Vec::new();
            for (_, item_project, item) in &items {
                if let (Item::Group(members), true) = (item, *item_project != project) {
                    guests.extend(
                        members
                            .iter()
                            .copied()
                            .filter(|thread| project_of(*thread) == project),
                    );
                }
            }
            ordered.retain(|thread| project_of(*thread) == project);
            ordered.extend(guests);
            ordered.extend(
                parked
                    .iter()
                    .copied()
                    .filter(|thread| project_of(*thread) == project),
            );
            for thread in ordered {
                if !seen.insert(thread) {
                    continue;
                }
                let row = self.thread_row(thread);
                let age = self
                    .facts
                    .last_used(thread)
                    .map(|at| crate::facts::since_label(at, now))
                    .unwrap_or_default();
                let word = state_word(&row, age);
                let on_board = board.iter().position(|shown| *shown == thread);
                let scope = match (&shown_group, on_board) {
                    (Some(group), Some(_)) => Some(group.clone()),
                    _ => self
                        .facts
                        .get(thread)
                        .and_then(|facts| facts.project_label.clone())
                        .map(|label| SharedString::from(label.to_lowercase())),
                };
                let name = if row.status == nav::RowStatus::Parked {
                    SharedString::from(format!("Parked \u{b7} {}", row.name))
                } else {
                    row.name.clone()
                };
                let keys = on_board
                    .filter(|at| *at < 9)
                    .and_then(|at| focus_rail_action(at + 1))
                    .and_then(crate::components::bound_chord);
                rows.push(PaletteRow {
                    section: PaletteSection::Threads,
                    name,
                    context: context_words(scope, word),
                    status: Some(row.status),
                    detail: self
                        .thread_path(thread)
                        .map(|path| SharedString::from(home_relative(&path))),
                    keys,
                    act: Act::Thread(thread),
                });
            }
        }
        rows
    }

    /// The Projects to jump to (`Act::Project`), in the registry's order,
    /// each with its open Threads' roll-up and how many there are. A
    /// Project with no open Thread has nowhere to land.
    fn palette_project_rows(&self) -> Vec<PaletteRow> {
        self.cockpit
            .registry()
            .projects()
            .iter()
            .filter_map(|project| {
                let rows = self.project_rows(project.id);
                let count = rows.len();
                let status = nav::RowStatus::roll_up(&rows)?;
                let noun = if count == 1 { "thread" } else { "threads" };
                Some(PaletteRow {
                    section: PaletteSection::Projects,
                    name: project.title.to_lowercase().into(),
                    context: format!("{count} {noun}").into(),
                    status: Some(status),
                    detail: None,
                    keys: None,
                    act: Act::Project(project.id),
                })
            })
            .collect()
    }

    /// A Project's open Threads' rows, in creation order.
    fn project_rows(&self, project: ProjectId) -> Vec<nav::ThreadRow> {
        let mut threads = self.cockpit.threads();
        threads.sort();
        threads
            .into_iter()
            .filter(|thread| {
                self.facts.get(*thread).and_then(|facts| facts.project) == Some(project)
            })
            .map(|thread| self.thread_row(thread))
            .collect()
    }

    /// Where jumping to a Project lands: its open Thread that most needs
    /// the operator, the first made among equals.
    fn project_lead(&self, project: ProjectId) -> Option<ThreadId> {
        self.project_rows(project)
            .into_iter()
            .rev()
            .max_by_key(|row| row.status.attention())
            .map(|row| row.thread)
    }

    /// The Groups' rows (⌘G's, and ⌘K's jumps): every Group in creation
    /// order, named, with its members' roll-up and `<project | all
    /// projects> · N threads` after it.
    fn palette_group_rows(&self) -> Vec<PaletteRow> {
        self.cockpit
            .groups()
            .iter()
            .map(|group| {
                let projects: std::collections::BTreeSet<SharedString> = group
                    .members
                    .iter()
                    .filter_map(|thread| {
                        self.facts
                            .get(*thread)
                            .and_then(|facts| facts.project_label.clone())
                    })
                    .collect();
                let scope: SharedString = if projects.len() == 1 {
                    projects
                        .into_iter()
                        .next()
                        .map(|label| SharedString::from(label.to_lowercase()))
                        .unwrap_or_default()
                } else {
                    "all projects".into()
                };
                let count = group.members.len();
                let noun = if count == 1 { "thread" } else { "threads" };
                let members: Vec<nav::ThreadRow> = group
                    .members
                    .iter()
                    .map(|thread| self.thread_row(*thread))
                    .collect();
                PaletteRow {
                    section: PaletteSection::Groups,
                    name: group.display_title().into(),
                    context: format!("{scope} \u{b7} {count} {noun}").into(),
                    status: nav::RowStatus::roll_up(&members),
                    detail: None,
                    keys: None,
                    act: Act::Group(group.id),
                }
            })
            .collect()
    }

    /// The command table (FL-4), in its fixed order; a row whose act is
    /// unavailable right now is left out. Keys come from the key table.
    pub(super) fn palette_command_rows(
        &self,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<PaletteRow> {
        use crate::components::{bound_chord, bound_chord_in};
        let focused = self.focused_thread();
        let open = focused.and_then(|thread| self.cockpit.thread(thread));
        let draft = self
            .panes
            .get(self.focused())
            .is_some_and(|pane| pane.draft().is_some());
        let in_group = matches!(self.cockpit.roster().view(), View::Group(_));
        let parked = !self.parked_threads_all().is_empty();
        let supports =
            |kind: ferrite_core::ControlKind| open.is_some_and(|open| open.supports_control(kind));
        let has_prompt = open.is_some_and(|open| {
            open.transcript()
                .blocks()
                .iter()
                .any(|block| matches!(block.body, ferrite_core::transcript::Body::Prompt(_)))
        });
        let ladder = open.is_some_and(|open| {
            !ferrite_core::providers::models::efforts_for(
                open.provider(),
                open.model(),
                open.models(),
            )
            .is_empty()
        });
        let facts = focused.and_then(|thread| self.facts.get(thread));
        let changed = facts.is_some_and(|facts| !facts.changed_files.is_empty());
        let checks = facts
            .and_then(|facts| facts.status.as_ref())
            .and_then(|status| status.pr.as_ref())
            .is_some_and(|pr| pr.checks.is_some());
        let plan = open.is_some_and(|open| open.transcript().todos().is_some());
        let size = self.prefs.settings.reading_size;
        let mut rows: Vec<PaletteRow> = Vec::new();
        let mut add =
            |shown: bool, name: String, detail: &str, keys: Option<String>, command: Command| {
                if shown {
                    rows.push(PaletteRow {
                        section: PaletteSection::Commands,
                        name: name.into(),
                        context: SharedString::from(detail.to_string()),
                        status: None,
                        detail: None,
                        keys,
                        act: Act::Command(command),
                    });
                }
            };
        let new_thread_t = crate::keymap::bindings(crate::keymap::PLATFORM)
            .into_iter()
            .find(|(keys, action, context)| {
                *action == "cockpit::NewThread" && context.is_none() && keys.ends_with("-t")
            })
            .map(|(keys, _, _)| crate::components::spell_chord(&keys));
        add(
            focused.is_some(),
            "park thread".into(),
            "",
            bound_chord_in("cockpit::CloseThread", Some("ComposerEmpty")),
            Command::ParkThread,
        );
        add(
            parked,
            "show parked".into(),
            "",
            bound_chord("palette::ShowParked"),
            Command::ShowParked,
        );
        add(
            focused.is_some_and(|thread| self.thread_path(thread).is_some()),
            "review branch".into(),
            "against the default branch",
            bound_chord("palette::ReviewBranch"),
            Command::ReviewBranch,
        );
        add(
            focused.is_some_and(|thread| self.thread_path(thread).is_some()),
            "review changes".into(),
            "comment on the diff",
            None,
            Command::ReviewChanges,
        );
        add(
            true,
            "new thread".into(),
            "",
            bound_chord("cockpit::NewThread"),
            Command::NewThread,
        );
        add(
            true,
            "new worktree thread".into(),
            "",
            bound_chord("cockpit::NewWorktreeThread"),
            Command::NewWorktreeThread,
        );
        add(
            true,
            "import a session".into(),
            "/import",
            None,
            Command::ImportSession,
        );
        add(
            in_group,
            "new thread in this group".into(),
            "",
            new_thread_t,
            Command::NewThreadInGroup,
        );
        add(
            focused.is_some_and(|thread| self.cockpit.groups().of(thread).is_none()),
            "new group".into(),
            "",
            bound_chord("cockpit::NewGroup"),
            Command::NewGroup,
        );
        add(
            self.cockpit.groups().iter().next().is_some(),
            "open a group".into(),
            "",
            bound_chord("palette::OpenGroups"),
            Command::OpenGroup,
        );
        add(
            parked,
            "reopen last".into(),
            "",
            bound_chord("cockpit::ReopenThread"),
            Command::ReopenLast,
        );
        let projects: Vec<(ProjectId, String)> = self
            .cockpit
            .registry()
            .projects()
            .iter()
            .map(|project| (project.id, project.title.clone()))
            .collect();
        for (project, title) in &projects {
            add(
                true,
                format!("new thread in {}", title.to_lowercase()),
                "",
                None,
                Command::NewThreadIn(*project),
            );
        }
        add(
            self.nav_filter.is_some(),
            "filter: all projects".into(),
            "",
            None,
            Command::FilterAll,
        );
        for (project, title) in &projects {
            add(
                self.nav_filter != Some(*project),
                format!("filter: {}", title.to_lowercase()),
                "",
                None,
                Command::FilterProject(*project),
            );
        }
        let order = self.prefs.settings.thread_list_order;
        add(
            order != ThreadListOrder::Created,
            "sort: created".into(),
            "",
            None,
            Command::SortCreated,
        );
        add(
            order != ThreadListOrder::Recent,
            "sort: recent".into(),
            "",
            None,
            Command::SortRecent,
        );
        add(
            order != ThreadListOrder::Attention,
            "sort: attention".into(),
            "what needs you first",
            None,
            Command::SortAttention,
        );
        add(true, "new project".into(), "", None, Command::NewProject);
        for (project, title) in &projects {
            add(
                true,
                format!("edit project {}", title.to_lowercase()),
                "",
                None,
                Command::EditProject(*project),
            );
        }
        add(
            true,
            "settings".into(),
            "",
            bound_chord("cockpit::OpenSettings"),
            Command::Settings,
        );
        add(
            true,
            "toggle sidebar".into(),
            "",
            bound_chord("cockpit::ToggleNav"),
            Command::ToggleSidebar,
        );
        add(
            !self.panes.is_empty(),
            "fullscreen pane".into(),
            "",
            bound_chord("cockpit::ToggleFullscreen"),
            Command::FullscreenPane,
        );
        add(
            size.step(1) != size,
            "text larger".into(),
            "",
            bound_chord("cockpit::TextLarger"),
            Command::TextLarger,
        );
        add(
            size.step(-1) != size,
            "text smaller".into(),
            "",
            bound_chord("cockpit::TextSmaller"),
            Command::TextSmaller,
        );
        add(
            size != ferrite_core::settings::ReadingSize::STANDARD,
            "text reset".into(),
            "",
            bound_chord("cockpit::TextReset"),
            Command::TextReset,
        );
        // The model's thinking is off the approved transcript; these put
        // its `∴` rows back, and take them away again.
        let thinking = self.prefs.settings.show_thinking;
        add(
            !thinking,
            "show thinking".into(),
            "",
            None,
            Command::ShowThinking,
        );
        add(
            thinking,
            "hide thinking".into(),
            "",
            None,
            Command::HideThinking,
        );
        // Smart filtering's `done` and `working` sections over the tree.
        let smart = self.prefs.settings.smart_filtering;
        add(
            !smart,
            "smart filtering on".into(),
            "",
            None,
            Command::SmartFilteringOn,
        );
        add(
            smart,
            "smart filtering off".into(),
            "",
            None,
            Command::SmartFilteringOff,
        );
        add(
            true,
            "notifications".into(),
            "",
            bound_chord("cockpit::ToggleNotifications"),
            Command::Notifications,
        );
        add(
            true,
            "shortcuts".into(),
            "",
            bound_chord_in("shortcuts::Toggle", Some("ComposerEmpty")),
            Command::Shortcuts,
        );
        add(
            !self.cockpit.needs_you().is_empty(),
            "next request".into(),
            "",
            bound_chord("cockpit::NextDecision"),
            Command::NextRequest,
        );
        add(
            has_prompt,
            "copy prompt".into(),
            "",
            None,
            Command::CopyPrompt,
        );
        add(
            has_prompt,
            "resend prompt".into(),
            "",
            None,
            Command::ResendPrompt,
        );
        add(
            open.is_some(),
            "model".into(),
            "/model",
            None,
            Command::Model,
        );
        add(ladder, "effort".into(), "/effort", None, Command::Effort);
        add(
            supports(ferrite_core::ControlKind::SetPermissionMode)
                && open.is_some_and(|open| !open.permission_modes().is_empty()),
            "permission mode".into(),
            "",
            bound_chord("status::CycleMode")
                .or_else(|| bound_chord_in("status::CycleMode", Some("Ferrite"))),
            Command::PermissionMode,
        );
        add(
            supports(ferrite_core::ControlKind::RefreshMcp),
            "refresh MCP".into(),
            "",
            None,
            Command::RefreshMcp,
        );
        add(
            supports(ferrite_core::ControlKind::ReconnectMcp)
                && open.is_some_and(|open| !open.transcript().mcp_servers().is_empty()),
            "reconnect MCP".into(),
            "",
            None,
            Command::ReconnectMcp,
        );
        add(
            open.is_some_and(|open| {
                open.supports_control(ferrite_core::ControlKind::BackgroundTasks)
                    || !open.transcript().progress().background().is_empty()
            }),
            "background tasks".into(),
            "",
            None,
            Command::BackgroundTasks,
        );
        add(
            changed,
            "show changes".into(),
            "",
            None,
            Command::ShowChanges,
        );
        add(checks, "show checks".into(), "", None, Command::ShowChecks);
        add(plan, "show plan".into(), "", None, Command::ShowPlan);
        add(
            !self.cli_updates.ready().is_empty(),
            "update CLIs".into(),
            "",
            None,
            Command::UpdateClis,
        );
        add(
            self.ferrite_update.restart_offered(),
            "restart to update Ferrite".into(),
            "",
            None,
            Command::UpdateFerrite,
        );
        add(
            draft,
            "discard draft".into(),
            "",
            bound_chord("cockpit::CloseThread"),
            Command::DiscardDraft,
        );
        let _ = cx;
        rows
    }

    /// Every parked Thread, whatever the nav's filter admits (the palette
    /// is not filtered).
    fn parked_threads_all(&self) -> Vec<ThreadId> {
        self.facts
            .parked()
            .iter()
            .copied()
            .filter(|thread| self.pane_for(*thread).is_none())
            .collect()
    }

    /// Run one command of the table: what its key or its old chrome ran.
    fn run_command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        let thread = self.focused_thread();
        let generation = thread
            .and_then(|thread| self.cockpit.thread(thread))
            .map(|open| open.generation());
        match command {
            Command::ParkThread => self.close_thread(&CloseThread, window, cx),
            Command::ShowParked => self.show_parked(window, cx),
            Command::ReviewBranch => self.review_branch(window, cx),
            Command::ReviewChanges => self.open_beside(
                beside::Beside::Review(ferrite_core::review::Scope::Uncommitted),
                window,
                cx,
            ),
            Command::NewThread => self.new_thread(&NewThread, window, cx),
            Command::NewWorktreeThread => self.new_worktree_thread(&NewWorktreeThread, window, cx),
            Command::NewThreadInGroup => {
                self.open_draft_with_placement(DraftTarget::Main, DraftPlacement::CurrentGroup, cx)
            }
            Command::NewGroup => self.new_group(&NewGroup, window, cx),
            Command::OpenGroup => self.open_palette(PaletteScope::Groups, "", window, cx),
            Command::ReopenLast => self.reopen_thread(&ReopenThread, window, cx),
            Command::NewThreadIn(project) => self.open_draft_in_project(project, cx),
            Command::FilterAll => self.choose_nav_filter(None, cx),
            Command::FilterProject(project) => self.choose_nav_filter(Some(project), cx),
            Command::SortCreated => self.set_thread_order(ThreadListOrder::Created, cx),
            Command::SortRecent => self.set_thread_order(ThreadListOrder::Recent, cx),
            Command::SortAttention => self.set_thread_order(ThreadListOrder::Attention, cx),
            Command::NewProject => self.open_project_creator(cx),
            Command::EditProject(project) => self.open_project_editor(project, cx),
            Command::Settings => self.open_settings(&OpenSettings, window, cx),
            Command::ToggleSidebar => self.toggle_nav_now(cx),
            Command::FullscreenPane => self.toggle_fullscreen(&ToggleFullscreen, window, cx),
            Command::TextLarger => self.step_text_size(1, cx),
            Command::TextSmaller => self.step_text_size(-1, cx),
            Command::TextReset => self.step_text_size(0, cx),
            Command::ShowThinking | Command::HideThinking => {
                let show = command == Command::ShowThinking;
                self.change_settings(|settings| settings.show_thinking = show, cx);
                cx.notify();
            }
            Command::SmartFilteringOn | Command::SmartFilteringOff => {
                let on = command == Command::SmartFilteringOn;
                self.change_settings(|settings| settings.smart_filtering = on, cx);
                cx.notify();
            }
            Command::Notifications => self.toggle_notifications(&ToggleNotifications, window, cx),
            Command::Shortcuts => self.toggle_shortcuts(window, cx),
            Command::NextRequest => self.next_decision(&NextDecision, window, cx),
            Command::CopyPrompt => {
                let index = self.focused();
                self.copy_reading_prompt(index, cx);
            }
            Command::ResendPrompt => {
                let index = self.focused();
                self.resend_reading_prompt(index, cx);
            }
            Command::Model | Command::Effort => {
                if let Some(thread) = thread {
                    if let Some(index) = self.pane_for(thread) {
                        self.focus_pane(index);
                    }
                    self.open_provider_picker(thread, cx);
                }
            }
            Command::PermissionMode => {
                if let (Some(thread), Some(generation)) = (thread, generation) {
                    self.mode_picker = Some((thread, generation));
                }
            }
            Command::RefreshMcp => {
                if let (Some(thread), Some(generation)) = (thread, generation) {
                    self.run_session_control(
                        thread,
                        generation,
                        ferrite_core::SessionControl::RefreshMcp,
                    );
                }
            }
            Command::ReconnectMcp => {
                if let (Some(thread), Some(generation)) = (thread, generation) {
                    let failed: Vec<String> = self
                        .cockpit
                        .thread(thread)
                        .map(|open| {
                            open.transcript()
                                .mcp_servers()
                                .iter()
                                .filter(|server| server.status == ferrite_core::McpStatus::Failed)
                                .map(|server| server.name.clone())
                                .collect()
                        })
                        .unwrap_or_default();
                    if failed.is_empty() {
                        // Nothing has failed: the card lists every server
                        // with its own reconnect.
                        self.session_controls = Some((thread, generation));
                    }
                    for server in failed {
                        self.run_session_control(
                            thread,
                            generation,
                            ferrite_core::SessionControl::ReconnectMcp { server },
                        );
                    }
                }
            }
            Command::BackgroundTasks => {
                if let (Some(thread), Some(generation)) = (thread, generation) {
                    self.session_controls = Some((thread, generation));
                }
            }
            Command::ShowChanges => {
                if let Some(thread) = thread {
                    self.changed_files_card = Some(thread);
                }
            }
            Command::ShowChecks => self.open_checks(window, cx),
            Command::ShowPlan => self.open_plan(window, cx),
            Command::UpdateClis => self.install_cli_updates(cx),
            Command::ImportSession => {
                let into = self.import_target(cx);
                self.open_import_palette(into, window, cx);
            }
            Command::UpdateFerrite => self.restart_to_update(cx),
            Command::DiscardDraft => {
                if let Some(identity) = self.panes.get(self.focused()).map(|pane| pane.identity) {
                    if identity.thread().is_none() {
                        self.close_pane(identity, cx);
                    }
                }
            }
        }
        cx.notify();
    }

    /// ⌘⇧D, `review branch`: the focused Thread's branch against its
    /// Project's default branch, in the Review beside it.
    fn review_branch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_palette(cx);
        self.open_beside(
            beside::Beside::Review(ferrite_core::review::Scope::Branch),
            window,
            cx,
        );
    }

    // ---------------------------------------------------------- shortcuts

    /// `?`: the shortcuts sheet, open or shut.
    pub(super) fn toggle_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.floats.shortcuts.is_some() {
            self.close_shortcuts(cx);
            return;
        }
        self.clear_for_float();
        self.floats.palette = None;
        let sheet = cx.new(Shortcuts::new);
        let focus = sheet.focus_handle(cx);
        self.floats.shortcuts = Some(sheet);
        window.focus(&focus, cx);
        cx.notify();
    }

    fn close_shortcuts(&mut self, cx: &mut Context<Self>) {
        if self.floats.shortcuts.take().is_some() {
            cx.notify();
        }
    }

    fn scroll_shortcuts(&mut self, rows: f32, cx: &mut Context<Self>) {
        if let Some(sheet) = self.floats.shortcuts.clone() {
            sheet.update(cx, |sheet, cx| sheet.scroll_by(rows, cx));
        }
    }

    // --------------------------------------------------------- the status

    /// ⇧⇥ (FL-9): the focused Thread's next permission mode, through the
    /// Session's own control; the mode segment follows its announcement. A
    /// Session that offers no modes keeps ⇧⇥'s old step: back through the
    /// Pane's files, tool calls and code actions.
    pub(super) fn cycle_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let step = self.focused_thread().and_then(|thread| {
            let open = self.cockpit.thread(thread)?;
            if !open.supports_control(ferrite_core::ControlKind::SetPermissionMode) {
                return None;
            }
            let modes: Vec<String> = open
                .permission_modes()
                .into_iter()
                .map(|choice| choice.value)
                .filter(|mode| crate::status::cycles(mode))
                .collect();
            let next = crate::status::next_mode(open.permission_mode(), &modes)?.to_string();
            Some((thread, open.generation(), next))
        });
        match step {
            Some((thread, generation, next)) => {
                self.run_session_control(
                    thread,
                    generation,
                    ferrite_core::SessionControl::SetPermissionMode { mode: next },
                );
                cx.notify();
            }
            None => self.tool_cycle_previous(&ToolCyclePrevious, window, cx),
        }
    }

    // --------------------------------------------------- the notifications

    fn step_notices(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.bell_rows().len();
        if count == 0 || !self.bell.open {
            return;
        }
        self.bell.cursor =
            (self.bell.cursor.min(count - 1) as isize + delta).rem_euclid(count as isize) as usize;
        cx.notify();
    }

    /// ⏎ on the list: open the cursor's Thread (its request), and close.
    fn open_notice_at_cursor(&mut self, cx: &mut Context<Self>) {
        let rows = self.bell_rows();
        let Some(row) = rows.get(self.bell.cursor.min(rows.len().saturating_sub(1))) else {
            return;
        };
        let verb = row.open_verb();
        self.notice_verb(verb, cx);
        self.bell.open = false;
        cx.notify();
    }

    /// ⌫ on the list: dismiss the cursor's row (a request until it changes,
    /// a finished turn with every one folded under it); the cursor stays on
    /// the row that takes its place.
    fn dismiss_notice_at_cursor(&mut self, cx: &mut Context<Self>) {
        let rows = self.bell_rows();
        let Some(row) = rows.get(self.bell.cursor.min(rows.len().saturating_sub(1))) else {
            return;
        };
        let verb = crate::notifications::dismiss_verb(&row.target);
        let folded = row.folded.clone();
        self.notice_verb(verb, cx);
        for id in folded {
            self.notice_verb(crate::notifications::Verb::Dismiss(id), cx);
        }
        let left = self.bell_rows().len();
        self.bell.cursor = self.bell.cursor.min(left.saturating_sub(1));
        cx.notify();
    }

    // ------------------------------------------------------------ drawing

    /// The palette's veil (the board only: the sidebar, the titlebar and
    /// the bottom bar stay as they are) and the palette or the shortcuts
    /// sheet over it, at the palette's geometry: 84 cells, centred on the
    /// board, 56px under its top. A press on the veil closes the float and
    /// nothing else.
    pub(super) fn sheet_floats(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let view: Option<gpui::AnyView> = self
            .floats
            .palette
            .clone()
            .map(Into::into)
            .or_else(|| self.floats.shortcuts.clone().map(Into::into));
        let Some(view) = view else {
            return Vec::new();
        };
        let board = self.board_bounds(window);
        let veil = deferred(
            div()
                .debug_selector(|| "palette-veil".into())
                .absolute()
                .left(px(board.x))
                .top(px(board.y))
                .w(px(board.w))
                .h(px(board.h))
                .bg(rgba(crate::theme::VEIL))
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        view.close_palette(cx);
                        view.close_shortcuts(cx);
                    }),
                ),
        )
        .with_priority(float::VEIL_PRIORITY)
        .into_any_element();
        let anchor = gpui::Bounds::new(
            gpui::point(px(board.x), px(board.y)),
            gpui::size(px(board.w), px(board.h)),
        );
        let float = float::hang(
            view,
            anchor,
            FloatPlace::BoardTop {
                top: crate::theme::PALETTE_TOP,
                width: crate::theme::PALETTE_W,
            },
            window,
            cx,
        );
        vec![veil, float]
    }

    /// The newest standing toast (FL-18), its right edge two cells in from
    /// the window's, its foot 36px over the bottom bar.
    pub(super) fn toast_float(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let target = self.bell.toasts().last()?.alert.target.clone();
        let row = self.alert_row(&target, ferrite_core::clock::system_time())?;
        let viewport = window.viewport_size();
        let anchor = gpui::Bounds::new(
            gpui::point(
                viewport.width - px(crate::theme::TOAST_BOX_RIGHT + crate::theme::TOAST_BOX_W),
                viewport.height - px(crate::theme::STATUS_BAR_H + crate::theme::TOAST_BOTTOM),
            ),
            gpui::size(px(crate::theme::TOAST_BOX_W), px(0.)),
        );
        let handle = self.notice_handle(cx);
        Some(float::hang_element(
            crate::notifications::toast(&row, handle).into_any_element(),
            anchor,
            FloatPlace::Above { gap: 0. },
        ))
    }

    /// The notifications list under the bell (FL-16): its left on the
    /// bell's, 4px under it; a press anywhere but the list or the bell
    /// closes it.
    pub(super) fn notices_float(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.bell.open {
            return None;
        }
        let door = self.bell.door.get()?;
        let rows = self.bell_rows();
        let cursor = self.bell.cursor.min(rows.len().saturating_sub(1));
        let handle = self.notice_handle(cx);
        let bell = self.bell.door.clone();
        let list = crate::notifications::list(&rows, cursor, &self.floats.notice_focus, handle)
            .on_mouse_down_out(cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                if bell
                    .get()
                    .is_some_and(|door| door.contains(&event.position))
                {
                    return;
                }
                if view.bell.open {
                    view.bell.open = false;
                    cx.notify();
                }
            }));
        Some(float::hang_element(
            list.into_any_element(),
            door,
            FloatPlace::Above {
                gap: crate::theme::NOTICE_PANEL_GAP,
            },
        ))
    }
}
