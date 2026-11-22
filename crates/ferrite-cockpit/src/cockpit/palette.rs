//! The floats' cockpit half (FL-1…FL-9, FL-16, FL-18): the command palette
//! (⌘K), the shortcuts sheet (`?`), the status line's mode cycle (⇧⇥),
//! the notifications list's keys and the palette's veil.
//!
//! The palette is a view of its own (ADR 0041, `crate::palette`): it
//! keeps its line, its rows, its tab and its cursor, and answers its own
//! keys. Here the cockpit builds its rows as it opens (`palette_inputs`)
//! and hears what it chose (`PaletteEvent`), running the row, since a row
//! can touch anything in the window. The shortcuts sheet's keys and ⌘K,
//! ⌘⇧P, ⌘⇧D and ⇧⇥ are handled here, at the cockpit's root
//! (`register_palette_actions`).

use super::*;
use crate::float::{self, FloatPlace};
use crate::palette::{
    Act, Command, Palette, PaletteEvent, PaletteScope, Row as PaletteRow,
    Section as PaletteSection,
};
use crate::shortcuts::Shortcuts;

/// What the floats hold between frames: the palette and the shortcuts sheet
/// while they are up.
pub(super) struct Floats {
    pub palette: Option<Entity<Palette>>,
    pub shortcuts: Option<Entity<Shortcuts>>,
    /// The palette or the shortcuts sheet on its way out (`sheet_floats`).
    pub exit: std::cell::RefCell<ferrite_ui::motion::Exits<gpui::AnyView>>,
}

impl Floats {
    pub(super) fn new() -> Self {
        Self {
            palette: None,
            shortcuts: None,
            exit: Default::default(),
        }
    }
}

/// A Thread's state as the palette's context word reads it: the lexicon's
/// word for every state that has one — a waiting Thread's what it waits
/// for, as its nav row says — a parked Thread's age, nothing for idle.
fn state_word(row: &nav::ThreadRow, age: SharedString) -> SharedString {
    use ferrite_ui::theme::words;
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
        .chain([ferrite_ui::provider::word(candidate.provider).to_string()])
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
        used: None,
        act: Act::Adopt {
            path: candidate.path,
            into,
        },
    }
}

impl CockpitView {
    /// The floats' hook on the cockpit's root: the palette's, the shortcuts
    /// sheet's and the status line's actions.
    pub(super) fn register_palette_actions(&self, root: Div, cx: &mut Context<Self>) -> Div {
        use crate::{palette, shortcuts, status};
        root.on_action(
            cx.listener(|view, _: &palette::Toggle, window, cx| view.toggle_palette(window, cx)),
        )
        .on_action(
            cx.listener(|view, _: &palette::ShowParked, window, cx| view.show_parked(window, cx)),
        )
        .on_action(
            cx.listener(|view, _: &palette::ReviewBranch, window, cx| {
                view.review_branch(window, cx)
            }),
        )
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
    }

    /// Whether a float holds the keyboard this frame (the palette's line,
    /// the shortcuts sheet, the notifications list): render's focus rule
    /// leaves it alone.
    pub(super) fn floats_hold_focus(&self) -> bool {
        self.floats.palette.is_some() || self.floats.shortcuts.is_some()
    }

    /// Whether the palette or the shortcuts sheet is up (the veil is).
    pub(super) fn sheet_float_open(&self) -> bool {
        self.floats.palette.is_some() || self.floats.shortcuts.is_some()
    }

    /// Close every other surface a float opening replaces.
    pub(super) fn clear_for_float(&mut self, cx: &mut Context<Self>) {
        self.close_composer_menu(cx);
        self.close_popup(cx);
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
    /// to jump to, then every command (`All`), or the sessions to import,
    /// its line seeded with `query` and holding the keyboard.
    pub(crate) fn open_palette(
        &mut self,
        scope: PaletteScope,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = self.palette_inputs(scope, window, cx);
        self.clear_for_float(cx);
        let palette = cx.new(|cx| Palette::new(scope, rows, query, cx));
        cx.subscribe_in(&palette, window, Self::on_palette_event).detach();
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

    /// What the palette lists as it opens, in the order it draws them:
    /// every Thread, the Projects and their Factories, then every command
    /// (`All`). The import picker opens empty and fills when its listing
    /// answers (`open_import_palette`).
    fn palette_inputs(
        &self,
        scope: PaletteScope,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<PaletteRow> {
        match scope {
            PaletteScope::All => {
                let mut rows = self.palette_thread_rows(cx);
                rows.extend(self.palette_project_rows(cx));
                rows.extend(self.palette_factory_rows(cx));
                rows.extend(self.palette_command_rows(window, cx));
                rows
            }
            PaletteScope::Sessions => Vec::new(),
        }
    }

    /// What the palette chose: ⏎ shuts it and runs the row; esc shuts it.
    fn on_palette_event(
        &mut self,
        _: &Entity<Palette>,
        event: &PaletteEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PaletteEvent::Run(act) => {
                self.floats.palette = None;
                self.run_palette_act(act.clone(), window, cx);
                cx.notify();
            }
            PaletteEvent::Dismiss => self.close_palette(cx),
        }
    }

    /// Run one palette row.
    pub(super) fn run_palette_act(
        &mut self,
        act: Act,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match act {
            Act::Thread(thread) => self.land_on_thread(thread, cx),
            Act::Project(project) => {
                if let Some(thread) = self.project_lead(project, cx) {
                    self.land_on_thread(thread, cx);
                }
            }
            Act::Factory(project) => self.open_factory(project, cx),
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
        let roots = self.session_file_roots.clone();
        let asked = self.import_machine(into, cx).and_then(|machine| {
            self.cockpit
                .update(cx, |cockpit, _| cockpit.discover_sessions(machine, roots, IMPORT_ROWS_MAX))
        });
        match asked {
            Some(reply) => {
                if let Some(palette) = self.floats.palette.clone() {
                    palette.update(cx, |palette, _| palette.listen(into, reply));
                }
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

    /// The listing's answer, once it lands, into the picker that asked for
    /// it: the listing lives on that picker, so one closed or reopened
    /// since has dropped it.
    pub(super) fn poll_session_discovery(&mut self, cx: &mut Context<Self>) {
        let Some(palette) = self
            .floats
            .palette
            .clone()
            .filter(|palette| palette.read(cx).listing.is_some())
        else {
            return;
        };
        if let Some((into, found)) = palette.update(cx, |palette, _| palette.listed()) {
            self.list_sessions(into, found, cx);
        }
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
                    format!("No CLI sessions found under {roots}").into(),
                    cx,
                );
            }
            Err(error) => self.show_sessions(
                Vec::new(),
                format!("Could not list sessions: {error}").into(),
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
        let pane = self.panes.get(self.focused(cx))?;
        let blank = match pane.identity {
            PaneIdentity::Draft(_) => pane.composer.read(cx).is_empty(),
            PaneIdentity::Thread(thread) => {
                pane::offers_import(self.cockpit.read(cx).thread(thread).map(|open| open.transcript()))
            }
        };
        blank.then_some(pane.identity)
    }

    /// The palette's Threads (FL-2): Project by Project in the registry's
    /// order (Threads no Project claims last), each Project's open Threads
    /// in creation order, then its parked Threads. Each Thread once.
    pub(super) fn palette_thread_rows(&self, cx: &gpui::App) -> Vec<PaletteRow> {
        let order = self.thread_order(cx);
        let project_of = |thread: ThreadKey| self.facts.get(thread).and_then(|facts| facts.project);
        let open_threads: Vec<ThreadKey> = {
            let mut open = self.cockpit.read(cx).threads();
            let side_chat = self.side_chat.read(cx);
            open.retain(|thread| !side_chat.owns(*thread) && self.cockpit.read(cx).operators(*thread));
            open.sort();
            open
        };
        // The parked Threads the palette offers: those used within the
        // last day. Older ones wait in the nav's parked fold (`show
        // parked`, ⌘⇧P).
        let recent = ferrite_core::clock::system_time();
        let parked: Vec<ThreadKey> = self
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
        let mut projects: Vec<Option<ProjectKey>> = self
            .every_project(cx)
            .into_iter()
            .map(|(key, _, _)| Some(key))
            .collect();
        projects.push(None);
        let mut seen: std::collections::HashSet<ThreadKey> = std::collections::HashSet::new();
        let mut rows: Vec<PaletteRow> = Vec::new();
        let now = ferrite_core::clock::system_time();
        for project in projects {
            let mut ordered: Vec<ThreadKey> = open_threads
                .iter()
                .copied()
                .filter(|thread| project_of(*thread) == project)
                .collect();
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
                let row = self.thread_row(thread, cx);
                let age = self
                    .facts
                    .last_used(thread)
                    .map(|at| crate::facts::since_label(at, now))
                    .unwrap_or_default();
                let word = state_word(&row, age);
                let ordinal = order.iter().position(|listed| *listed == thread);
                let scope = self
                    .facts
                    .get(thread)
                    .and_then(|facts| facts.project_label.clone());
                let name = if row.status == nav::RowStatus::Parked {
                    SharedString::from(format!("Parked \u{b7} {}", row.name))
                } else {
                    row.name.clone()
                };
                let keys = ordinal
                    .filter(|at| *at < 9)
                    .and_then(|at| focus_thread_action(at + 1))
                    .and_then(ferrite_ui::components::bound_chord);
                rows.push(PaletteRow {
                    section: PaletteSection::Threads,
                    name,
                    context: context_words(scope, word),
                    status: Some(row.status),
                    detail: self
                        .thread_path(thread, cx)
                        .map(|path| SharedString::from(home_relative(&path))),
                    keys,
                    used: self.facts.last_used(thread),
                    act: Act::Thread(thread),
                });
            }
        }
        rows
    }

    /// The Projects to jump to (`Act::Project`), in the registry's order,
    /// each with its open Threads' roll-up and how many there are. A
    /// Project with no open Thread has nowhere to land.
    fn palette_project_rows(&self, cx: &gpui::App) -> Vec<PaletteRow> {
        self.every_project(cx)
            .into_iter()
            .filter_map(|(key, _, title)| {
                let rows = self.project_rows(key, cx);
                let count = rows.len();
                let status = nav::roll_up(&rows)?;
                let noun = if count == 1 { "thread" } else { "threads" };
                Some(PaletteRow {
                    section: PaletteSection::Projects,
                    name: title.into(),
                    context: format!("{count} {noun}").into(),
                    status: Some(status),
                    detail: None,
                    keys: None,
                    used: None,
                    act: Act::Project(key),
                })
            })
            .collect()
    }

    /// Every Project's Factory (`Act::Factory`), `Factory: <Project>`,
    /// an empty one's too: the nav lists only the busy ones.
    fn palette_factory_rows(&self, cx: &gpui::App) -> Vec<PaletteRow> {
        let cockpit = self.cockpit.read(cx);
        self.every_project(cx)
            .into_iter()
            .map(|(key, _, title)| {
                let tasks = cockpit.factory(key.machine()).on_board(key.project()).count();
                PaletteRow {
                    section: PaletteSection::Projects,
                    name: format!("Factory: {title}").into(),
                    context: match tasks {
                        0 => "No tasks".to_string(),
                        1 => "1 task".to_string(),
                        n => format!("{n} tasks"),
                    }
                    .into(),
                    status: None,
                    detail: None,
                    keys: None,
                    used: None,
                    act: Act::Factory(key),
                }
            })
            .collect()
    }

    /// A Project's open Threads' rows, in creation order.
    fn project_rows(&self, project: ProjectKey, cx: &gpui::App) -> Vec<nav::ThreadRow> {
        let mut threads = self.cockpit.read(cx).threads();
        threads.sort();
        threads
            .into_iter()
            .filter(|thread| !self.side_chat.read(cx).owns(*thread))
            .filter(|thread| {
                self.facts.get(*thread).and_then(|facts| facts.project) == Some(project)
            })
            .map(|thread| self.thread_row(thread, cx))
            .collect()
    }

    /// Where jumping to a Project lands: its open Thread that most needs
    /// the operator, the first made among equals.
    fn project_lead(&self, project: ProjectKey, cx: &gpui::App) -> Option<ThreadKey> {
        self.project_rows(project, cx)
            .into_iter()
            .rev()
            .max_by_key(|row| row.status.attention())
            .map(|row| row.key)
    }

    /// The command table (FL-4), in its fixed order; a row whose act is
    /// unavailable right now is left out. Keys come from the key table.
    pub(super) fn palette_command_rows(
        &self,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<PaletteRow> {
        use ferrite_ui::components::{bound_chord, bound_chord_in};
        let focused = self.focused_thread(cx);
        let open = focused.and_then(|thread| self.cockpit.read(cx).thread(thread));
        let draft = self
            .panes
            .get(self.focused(cx))
            .is_some_and(|pane| pane.draft().is_some());
        let parked = !self.parked_threads_all().is_empty();
        let supports =
            |kind: ferrite_core::ControlKind| open.is_some_and(|open| open.supports_control(kind));
        let has_prompt = open.is_some_and(|open| {
            open.transcript()
                .blocks()
                .iter()
                .any(|block| matches!(block.body, ferrite_core::transcript::Body::Prompt(_)))
        });
        // Rewind and Fork where the agent can cut now (`rewind_gate`), and
        // a Rewind only to a prompt it knows by id.
        let cuttable = open
            .as_ref()
            .and_then(super::transcript_glue::rewind_gate)
            .is_some_and(|allowed| allowed.is_ok());
        let rewindable =
            cuttable && focused.is_some_and(|thread| self.last_prompt(thread, cx).is_some());
        let ladder = open.is_some_and(|open| {
            !ferrite_core::providers::models::efforts_for(
                open.provider(),
                open.model(),
                open.models(),
            )
            .is_empty()
        });
        let facts = focused.and_then(|thread| self.facts.get(thread));
        // The changes show in the panel, so only where it has a Thread.
        let changed = facts.is_some_and(|facts| !facts.changed_files.is_empty())
            && self.panel_candidate(cx).is_some();
        let pr = facts.and_then(|facts| facts.pr.reading.as_ref());
        let checks = pr.is_some_and(|pr| pr.checks.is_some());
        let watch = focused.and_then(|thread| self.pr_watch_target(thread, cx));
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
                        used: None,
                        act: Act::Command(command),
                    });
                }
            };
        add(
            focused.is_some(),
            "Park thread".into(),
            "",
            bound_chord("cockpit::CloseThread"),
            Command::ParkThread,
        );
        add(
            parked,
            "Show parked".into(),
            "",
            bound_chord("palette::ShowParked"),
            Command::ShowParked,
        );
        add(
            focused.is_some_and(|thread| self.thread_path(thread, cx).is_some()),
            "Review branch".into(),
            "against the default branch",
            bound_chord("palette::ReviewBranch"),
            Command::ReviewBranch,
        );
        add(
            focused.is_some_and(|thread| self.thread_path(thread, cx).is_some()),
            "Review changes".into(),
            "comment on the diff",
            None,
            Command::ReviewChanges,
        );
        add(
            true,
            "New thread".into(),
            "",
            bound_chord("cockpit::NewThread"),
            Command::NewThread,
        );
        add(
            true,
            "New worktree thread".into(),
            "",
            bound_chord("cockpit::NewWorktreeThread"),
            Command::NewWorktreeThread,
        );
        add(
            true,
            "Import a session".into(),
            "/import",
            None,
            Command::ImportSession,
        );
        add(
            parked,
            "Reopen last".into(),
            "",
            bound_chord("cockpit::ReopenThread"),
            Command::ReopenLast,
        );
        let projects: Vec<(ProjectKey, String)> = self
            .every_project(cx)
            .into_iter()
            .map(|(key, _, title)| (key, title))
            .collect();
        for (project, title) in &projects {
            add(
                true,
                format!("New thread in {title}"),
                "",
                None,
                Command::NewThreadIn(*project),
            );
        }
        add(
            self.nav_filter.is_some(),
            "Filter: all projects".into(),
            "",
            None,
            Command::FilterAll,
        );
        for (project, title) in &projects {
            add(
                self.nav_filter != Some(*project),
                format!("Filter: {title}"),
                "",
                None,
                Command::FilterProject(*project),
            );
        }
        add(true, "New project".into(), "", None, Command::NewProject);
        for (project, title) in &projects {
            add(
                true,
                format!("Edit project {title}"),
                "",
                None,
                Command::EditProject(*project),
            );
        }
        add(
            true,
            "Settings".into(),
            "",
            bound_chord("cockpit::OpenSettings"),
            Command::Settings,
        );
        add(true, "Pull requests".into(), "", None, Command::PullRequests);
        add(
            true,
            "Toggle sidebar".into(),
            "",
            bound_chord("cockpit::ToggleNav"),
            Command::ToggleSidebar,
        );
        add(
            self.panel_candidate(cx).is_some(),
            "Toggle panel".into(),
            "",
            bound_chord("cockpit::TogglePanel"),
            Command::TogglePanel,
        );
        add(
            self.focused_thread(cx).is_some(),
            "Toggle thread details".into(),
            "",
            bound_chord("cockpit::ToggleDetails"),
            Command::ToggleDetails,
        );
        // The Panel's own (ADR 0039): T3's `+` menu, close and reopen
        // chords are Ferrite's ⌘T/⌘W/⌘⇧T, so these are the palette's.
        add(
            focused.is_some(),
            "Open a surface".into(),
            "the Panel's + menu",
            None,
            Command::SurfaceMenu,
        );
        add(
            focused
                .is_some_and(|thread| self.active_surface(thread).is_some() && self.panel_open(thread)),
            "Close surface".into(),
            "the Panel's shown tab",
            None,
            Command::CloseSurface,
        );
        add(
            focused.is_some_and(|thread| {
                self.panel
                    .surfaces
                    .get(thread)
                    .is_some_and(|state| state.can_reopen())
            }),
            "Reopen closed surface".into(),
            "",
            None,
            Command::ReopenSurface,
        );
        add(
            size.step(1) != size,
            "Text larger".into(),
            "",
            bound_chord("cockpit::TextLarger"),
            Command::TextLarger,
        );
        add(
            size.step(-1) != size,
            "Text smaller".into(),
            "",
            bound_chord("cockpit::TextSmaller"),
            Command::TextSmaller,
        );
        add(
            size != ferrite_core::settings::ReadingSize::STANDARD,
            "Text reset".into(),
            "",
            bound_chord("cockpit::TextReset"),
            Command::TextReset,
        );
        // The model's thinking is off the transcript by default; these put
        // its `∴` rows back, and take them away again.
        let thinking = self.prefs.settings.show_thinking;
        add(
            !thinking,
            "Show thinking".into(),
            "",
            None,
            Command::ShowThinking,
        );
        add(
            thinking,
            "Hide thinking".into(),
            "",
            None,
            Command::HideThinking,
        );
        add(
            true,
            "Inbox".into(),
            "",
            bound_chord("cockpit::ToggleNotifications"),
            Command::Notifications,
        );
        // The keys' one door now that `?` has left the bar (SPEC §5.7),
        // beside the Help menu's `Keyboard Shortcuts`.
        add(
            true,
            "Keyboard shortcuts".into(),
            "",
            bound_chord_in("shortcuts::Toggle", Some("ComposerEmpty")),
            Command::Shortcuts,
        );
        add(
            !self.cockpit.read(cx).needs_you().is_empty(),
            "Next request".into(),
            "",
            bound_chord("cockpit::NextDecision"),
            Command::NextRequest,
        );
        add(
            has_prompt,
            "Copy prompt".into(),
            "",
            None,
            Command::CopyPrompt,
        );
        add(
            has_prompt,
            "Resend prompt".into(),
            "",
            None,
            Command::ResendPrompt,
        );
        add(
            rewindable,
            "Rewind to last message".into(),
            "",
            None,
            Command::Rewind,
        );
        add(cuttable, "Fork thread".into(), "", None, Command::Fork);
        add(
            open.is_some(),
            "Model".into(),
            "/model",
            None,
            Command::Model,
        );
        add(ladder, "Effort".into(), "/effort", None, Command::Effort);
        add(
            supports(ferrite_core::ControlKind::SetPermissionMode)
                && open.is_some_and(|open| !open.permission_modes().is_empty()),
            "Permission mode".into(),
            "",
            bound_chord("status::CycleMode"),
            Command::PermissionMode,
        );
        add(
            supports(ferrite_core::ControlKind::RefreshMcp),
            "Refresh MCP".into(),
            "",
            None,
            Command::RefreshMcp,
        );
        add(
            supports(ferrite_core::ControlKind::ReconnectMcp)
                && open.is_some_and(|open| !open.transcript().mcp_servers().is_empty()),
            "Reconnect MCP".into(),
            "",
            None,
            Command::ReconnectMcp,
        );
        add(
            open.is_some_and(|open| {
                open.supports_control(ferrite_core::ControlKind::BackgroundTasks)
                    || !open.transcript().progress().background().is_empty()
            }),
            "Background tasks".into(),
            "",
            None,
            Command::BackgroundTasks,
        );
        add(
            changed,
            "Show changes".into(),
            "",
            None,
            Command::ShowChanges,
        );
        add(checks, "Show checks".into(), "", None, Command::ShowChecks);
        add(
            focused.is_some(),
            "Refresh PR status".into(),
            "",
            bound_chord("cockpit::RefreshPrStatus"),
            Command::RefreshPr,
        );
        add(
            pr.is_some_and(|pr| pr.url.is_some()),
            "Open PR in browser".into(),
            "",
            bound_chord("cockpit::OpenPrInBrowser"),
            Command::OpenPr,
        );
        add(
            watch.as_ref().is_some_and(|(_, watched)| !watched),
            "Watch PR for changes".into(),
            "",
            None,
            Command::WatchPr,
        );
        add(
            watch.as_ref().is_some_and(|(_, watched)| *watched),
            "Stop watching PR".into(),
            "",
            None,
            Command::UnwatchPr,
        );
        add(plan, "Show plan".into(), "", None, Command::ShowPlan);
        add(
            !self.cli_updates.ready().is_empty(),
            "Update CLIs".into(),
            "",
            None,
            Command::UpdateClis,
        );
        add(
            self.ferrite_update.restart_offered(),
            "Restart to update Ferrite".into(),
            "",
            None,
            Command::UpdateFerrite,
        );
        add(
            draft,
            "Discard draft".into(),
            "",
            bound_chord("cockpit::CloseThread"),
            Command::DiscardDraft,
        );
        add(
            !self.prefs.settings.keys.is_empty(),
            "Reset keys".into(),
            "forget the keys in settings.json",
            None,
            Command::ResetKeys,
        );
        let _ = cx;
        rows.extend(self.user_command_rows(cx));
        rows
    }

    /// Every parked Thread, whatever the nav's filter admits (the palette
    /// is not filtered).
    pub(super) fn parked_threads_all(&self) -> Vec<ThreadKey> {
        self.facts
            .parked()
            .iter()
            .copied()
            .filter(|thread| self.pane_for(*thread).is_none())
            .collect()
    }

    /// Run one command of the table: what its key or its old chrome ran.
    pub(super) fn run_command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let thread = self.focused_thread(cx);
        let generation = thread
            .and_then(|thread| self.cockpit.read(cx).thread(thread))
            .map(|open| open.generation());
        match command {
            Command::ParkThread => self.close_thread(&CloseThread, window, cx),
            Command::ShowParked => self.show_parked(window, cx),
            Command::ReviewBranch => self.review_branch(window, cx),
            Command::ReviewChanges => self.open_beside(
                beside::Beside::Review(ferrite_reviewr::model::Scope::Uncommitted),
                window,
                cx,
            ),
            Command::NewThread => self.new_thread(&NewThread, window, cx),
            Command::NewWorktreeThread => self.new_worktree_thread(&NewWorktreeThread, window, cx),
            Command::ReopenLast => self.reopen_thread(&ReopenThread, window, cx),
            Command::NewThreadIn(project) => self.open_draft_in_project(project, cx),
            Command::FilterAll => self.choose_nav_filter(None, cx),
            Command::FilterProject(project) => self.choose_nav_filter(Some(project), cx),
            Command::NewProject => self.open_project_creator(cx),
            Command::EditProject(project) => self.open_project_editor(project, cx),
            Command::Settings => self.open_settings(&OpenSettings, window, cx),
            Command::PullRequests => self.open_prs(None, cx),
            Command::ToggleSidebar => self.toggle_nav_now(cx),
            Command::TogglePanel => self.toggle_panel_now(cx),
            Command::ToggleDetails => self.toggle_details_now(window, cx),
            Command::TextLarger => self.step_text_size(1, cx),
            Command::TextSmaller => self.step_text_size(-1, cx),
            Command::TextReset => self.step_text_size(0, cx),
            Command::ShowThinking | Command::HideThinking => {
                let show = command == Command::ShowThinking;
                self.change_settings(|settings| settings.show_thinking = show, cx);
                cx.notify();
            }
            Command::Notifications => self.toggle_notifications(&ToggleNotifications, window, cx),
            Command::Shortcuts => self.toggle_shortcuts(window, cx),
            Command::NextRequest => self.next_decision(&NextDecision, window, cx),
            Command::CopyPrompt => {
                let index = self.focused(cx);
                self.copy_reading_prompt(index, cx);
            }
            Command::ResendPrompt => {
                let index = self.focused(cx);
                self.resend_reading_prompt(index, cx);
            }
            // The prompt's own Rewind menu, as its ↶ opens it, over the
            // middle of its Pane.
            Command::Rewind => {
                if let Some((thread, block)) =
                    thread.and_then(|thread| Some((thread, self.last_prompt(thread, cx)?)))
                {
                    let at = self
                        .pane_rects(window, cx)
                        .into_iter()
                        .find(|(index, _)| *index == self.focused(cx))
                        .map_or_else(gpui::Point::default, |(_, rect)| {
                            gpui::point(px(rect.x + rect.w / 2.), px(rect.y + rect.h / 2.))
                        });
                    self.open_context_menu(MenuTarget::Prompt { thread, block }, at, cx);
                }
            }
            Command::Fork => {
                if let Some(thread) = thread {
                    self.fork_thread(thread, None, None, cx);
                }
            }
            Command::Model | Command::Effort => {
                if let Some(thread) = thread {
                    if let Some(index) = self.pane_for(thread) {
                        self.focus_pane(index, cx);
                    }
                    if command == Command::Effort {
                        self.open_effort_picker(thread, cx);
                    } else {
                        self.open_provider_picker(thread, cx);
                    }
                }
            }
            Command::PermissionMode => {
                if let (Some(thread), Some(generation)) = (thread, generation) {
                    self.show_popup(
                        Popup::Mode(super::mode_menu::ModeMenu {
                            thread,
                            generation,
                            preview: None,
                        }),
                        cx,
                    );
                }
            }
            Command::RefreshMcp => {
                if let (Some(thread), Some(generation)) = (thread, generation) {
                    self.run_session_control(
                        thread,
                        generation,
                        ferrite_core::SessionControl::RefreshMcp,
                        cx,
                    );
                }
            }
            Command::ReconnectMcp => {
                if let (Some(thread), Some(generation)) = (thread, generation) {
                    let failed: Vec<String> = self
                        .cockpit
                        .read(cx)
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
                        self.show_popup(Popup::Session { thread, generation }, cx);
                    }
                    for server in failed {
                        self.run_session_control(
                            thread,
                            generation,
                            ferrite_core::SessionControl::ReconnectMcp { server },
                            cx,
                        );
                    }
                }
            }
            Command::BackgroundTasks => {
                if let (Some(thread), Some(generation)) = (thread, generation) {
                    self.show_popup(Popup::Session { thread, generation }, cx);
                }
            }
            Command::ShowChanges => {
                if let Some(thread) = thread {
                    match self.opener_unavailable(thread, panel::Opener::Changes, cx) {
                        None => self.run_opener(thread, panel::Opener::Changes, cx),
                        Some(_) => self.open_panel(thread, cx),
                    }
                }
            }
            Command::SurfaceMenu => self.open_surface_menu(cx),
            Command::CloseSurface => {
                if let Some((thread, id)) =
                    thread.and_then(|thread| Some((thread, self.active_surface(thread)?)))
                {
                    self.close_surface(thread, &id, cx);
                }
            }
            Command::ReopenSurface => {
                if let Some(thread) = thread {
                    self.apply_surface(thread, surfaces::SurfaceAction::Reopen);
                    cx.notify();
                }
            }
            Command::ShowChecks => self.open_checks(window, cx),
            Command::RefreshPr => {
                if let Some(thread) = thread {
                    self.check_pr(thread, true, cx);
                }
            }
            Command::OpenPr => {
                if let Some(thread) = thread {
                    self.open_pr(thread, cx);
                }
            }
            Command::WatchPr | Command::UnwatchPr => {
                if let Some((thread, (url, _))) =
                    thread.and_then(|thread| Some((thread, self.pr_watch_target(thread, cx)?)))
                {
                    self.watch_pr(thread, url, command == Command::WatchPr, cx);
                }
            }
            Command::ShowPlan => self.open_plan(window, cx),
            Command::UpdateClis => self.install_cli_updates(cx),
            Command::ImportSession => {
                let into = self.import_target(cx);
                self.open_import_palette(into, window, cx);
            }
            Command::UpdateFerrite => self.restart_to_update(cx),
            Command::DiscardDraft => {
                if let Some(identity) = self.panes.get(self.focused(cx)).map(|pane| pane.identity) {
                    if identity.thread().is_none() {
                        self.close_pane(identity, cx);
                    }
                }
            }
            Command::ResetKeys => self.reset_keys(cx),
            Command::User(index) => self.run_user_command(index, cx),
        }
        cx.notify();
    }

    /// ⌘⇧D, `review branch`: the focused Thread's branch against its
    /// Project's default branch, in the Review beside it.
    fn review_branch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_palette(cx);
        self.open_beside(
            beside::Beside::Review(ferrite_reviewr::model::Scope::Branch),
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
        self.clear_for_float(cx);
        self.floats.palette = None;
        let sheet = cx.new(Shortcuts::new);
        let focus = sheet.read(cx).input.focus_handle(cx);
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
        let step = self.focused_thread(cx).and_then(|thread| {
            let open = self.cockpit.read(cx).thread(thread)?;
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
                    cx,
                );
                cx.notify();
            }
            None => self.tool_cycle_previous(&ToolCyclePrevious, window, cx),
        }
    }

    // ------------------------------------------------------------ drawing

    /// The palette's veil (the board only: the sidebar, the titlebar and
    /// the bottom bar stay as they are) and the palette or the shortcuts
    /// sheet over it, at the palette's geometry: `PALETTE_W` wide, centred on the
    /// board, 56px under its top. A press on the veil closes the float and
    /// nothing else. Closed, the float shrinks away toward its top over
    /// `PALETTE_OUT` (Claude's 120ms) as a face no pointer finds, its veil
    /// lifting over `PALETTE_VEIL`.
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
        let leaving = self.floats.exit.borrow_mut().stage(
            view.as_ref(),
            |was, next| was.entity_id() == next.entity_id(),
            ferrite_ui::motion::PALETTE_OUT,
            cx.background_executor().now(),
            ferrite_ui::motion::reduced_motion(cx),
        );
        let board = self.board_bounds(window, cx);
        let anchor = gpui::Bounds::new(
            gpui::point(px(board.x), px(board.y)),
            gpui::size(px(board.w), px(board.h)),
        );
        let place = FloatPlace::BoardTop {
            top: ferrite_ui::theme::PALETTE_TOP,
            width: ferrite_ui::theme::PALETTE_W,
        };
        let veil_box = || {
            div()
                .absolute()
                .left(px(board.x))
                .top(px(board.y))
                .w(px(board.w))
                .h(px(board.h))
        };
        let Some(view) = view else {
            return leaving
                .map(|(gone, left)| {
                    let veil = deferred(ferrite_ui::motion::veil_out(
                        left,
                        ferrite_ui::motion::PALETTE_VEIL,
                        veil_box(),
                    ))
                    .with_priority(float::VEIL_PRIORITY)
                    .into_any_element();
                    let float = float::hang_element(
                        ferrite_ui::motion::float_out(
                            left,
                            ferrite_ui::motion::PALETTE_OUT,
                            gone,
                            ferrite_ui::motion::FROM_TOP,
                        )
                        .into_any_element(),
                        anchor,
                        place,
                    );
                    vec![veil, float]
                })
                .unwrap_or_default();
        };
        // The veil darkens in under the sheet's own entrance, as Settings'
        // does; a press on it takes effect from its first frame.
        let veil = deferred(ferrite_ui::motion::veil_in(
            "palette-veil-in",
            ferrite_ui::motion::PALETTE_VEIL,
            veil_box()
                .debug_selector(|| "palette-veil".into())
                .bg(rgba(ferrite_ui::theme::VEIL))
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        view.close_palette(cx);
                        view.close_shortcuts(cx);
                    }),
                ),
        ))
        .with_priority(float::VEIL_PRIORITY)
        .into_any_element();
        vec![veil, float::hang(view, anchor, place, window, cx)]
    }
}
