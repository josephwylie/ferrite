//! The nav's side in the window: what the nav lists (`listed_threads`)
//! and draws from (`nav_inputs`), the model a row's Thread runs
//! (`row_model`), where its menu buttons hang their menus, what its
//! presses ask (`on_nav_event`), and the nav's notice, shelves and Snooze
//! rows. The nav itself, its list, keys, search, picks, undo and Snooze
//! field, is `ferrite_nav::view`'s (ADR 0041).

use super::menu_button::{Door, Edge};
use super::*;
use ferrite_core::provider::Provider;
use ferrite_core::snooze::Preset;
use ferrite_nav::view::{
    MenuAt, NavAsked, NavEvent, NavInputs, NavLook, NavMenu, NavSources, NavView,
    OpenThread, UndoAct,
};
use ferrite_ui::source::Source;
use gpui::App;
use std::rc::Rc;

/// What the last nav act has to say, drawn in the list as `nav-notice`.
/// Its text is private: a problem the operator sees also reaches the log.
#[derive(Default)]
pub(super) struct Notice(Option<SharedString>);

impl Notice {
    /// Something went wrong: shown, and logged in the same words.
    pub(super) fn problem(&mut self, text: impl Into<SharedString>) {
        let text = text.into();
        ferrite_core::log!("{text}");
        self.0 = Some(text);
    }

    /// Guidance on what the operator asked for, not a failure: shown only.
    pub(super) fn hint(&mut self, text: impl Into<SharedString>) {
        self.0 = Some(text.into());
    }

    pub(super) fn clear(&mut self) {
        self.0 = None;
    }

    pub(super) fn text(&self) -> Option<&SharedString> {
        self.0.as_ref()
    }
}

impl From<NavMenu> for MenuTarget {
    fn from(menu: NavMenu) -> Self {
        match menu {
            NavMenu::Thread(thread) => MenuTarget::Thread(thread),
            NavMenu::Snooze(thread) => MenuTarget::Snooze(thread),
            NavMenu::Parked => MenuTarget::Parked,
            NavMenu::PrLink(thread) => MenuTarget::PrLink(thread),
            NavMenu::Filter => MenuTarget::NavFilter,
            NavMenu::Factory => MenuTarget::NavFactory,
        }
    }
}

/// The nav's menu, when `target` is on something of the nav's.
fn nav_menu(target: MenuTarget) -> Option<NavMenu> {
    Some(match target {
        MenuTarget::Thread(thread) => NavMenu::Thread(thread),
        MenuTarget::Snooze(thread) => NavMenu::Snooze(thread),
        MenuTarget::Parked => NavMenu::Parked,
        MenuTarget::PrLink(thread) => NavMenu::PrLink(thread),
        MenuTarget::NavFilter => NavMenu::Filter,
        MenuTarget::NavFactory => NavMenu::Factory,
        _ => return None,
    })
}

/// The nav's menu button for `menu`: the Filter menu hangs from its
/// button's right edge, the Factory's and a row's Snooze from their left.
fn nav_door(menu: NavMenu) -> Door {
    let edge = match menu {
        NavMenu::Filter => Edge::Right,
        _ => Edge::Left,
    };
    Door::new(menu.into(), edge)
}

impl CockpitView {
    /// The nav, reading the window through `listed_threads` and
    /// `nav_inputs`, the model through `row_model`, and recording where
    /// its menu buttons stand in the popup slot's `triggers`. Once the
    /// Cockpit is made, it looks as the Cockpit notifies and as the model
    /// says something `Arrived`.
    pub(super) fn new_nav(
        model: &Entity<Cockpit>,
        triggers: popups::Triggers,
        cx: &mut Context<Self>,
    ) -> Entity<NavView> {
        let root = cx.entity();
        let sources = NavSources {
            listed: Source::of(&root, |root: &Self, _: &(), cx| {
                Some(listed_threads(root, root.cockpit.read(cx), cx))
            }),
            look: Source::of(&root, |root: &Self, asked: &NavAsked, cx| {
                let model = root.cockpit.read(cx);
                let (inputs, factories) = nav_inputs(root, model, &asked.listed, &asked.arrivals, cx);
                Some(NavLook { inputs, factories })
            }),
            model_name: Source::of(model, |model: &Cockpit, &(thread, provider), _| {
                Some(row_model(model, thread, provider))
            }),
            door: Rc::new(move |menu: NavMenu, up| {
                ferrite_ui::components::MenuDoor::new(up, triggers.record(Door::key(menu.into())))
            }),
        };
        let nav = cx.new(|cx| NavView::new(sources, cx));
        let (weak, model) = (nav.downgrade(), model.clone());
        cx.defer(move |cx| {
            weak.update(cx, |nav, cx| {
                let wakes = vec![
                    cx.observe(&root, |nav, _, cx| nav.look(cx)),
                    cx.subscribe(&model, |nav, _, _: &Arrived, cx| nav.look(cx)),
                ];
                nav.watch(wakes, cx);
            })
            .ok();
        });
        nav
    }
}

/// The model `thread` runs, named as its Composer names it; `provider`'s
/// name until the window is told, since a Thread on another machine says
/// only once it is opened and the default may not be it.
fn row_model(model: &Cockpit, thread: ThreadKey, provider: Provider) -> SharedString {
    let open = model.thread(thread);
    // A Thread not mirrored here has no transcript: its digest says.
    let digest = open.map(|open| open.digest());
    let known = open
        .and_then(|open| open.running_model())
        .or_else(|| digest.as_deref().and_then(|digest| digest.model.as_deref()));
    match known {
        Some(known) => ferrite_core::models::footer_name(
            provider,
            Some(known),
            open.map_or(&[][..], |open| open.models()),
        )
        .into(),
        None => ferrite_ui::provider::title(provider).into(),
    }
}

impl CockpitView {
    /// The Filter menu's rows: every Project, the scoped one checked. The
    /// same repository on two machines is two Projects: another machine's
    /// wears T3 Code's `ProjectEnvironmentBadge`, an icon whose tooltip
    /// says where it is.
    pub(super) fn filter_rows(&self, cx: &gpui::App) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let mut rows = vec![Some((
            menu::Item::new("All projects").checked(self.nav_filter.is_none()),
            MenuVerb::FilterProject(None),
        ))];
        rows.extend(self.every_project(cx).into_iter().map(|(key, project, _)| {
            let item = menu::Item::new(project.title.clone()).checked(self.nav_filter == Some(key));
            let item = match self.machine_label(key.machine(), cx) {
                Some(machine) => item.mark(ferrite_ui::icons::SERVER, format!("On {machine}")),
                None => item,
            };
            Some((item, MenuVerb::FilterProject(Some(key))))
        }));
        rows
    }

    pub(crate) fn shelf_open(&self, shelf: nav::Shelf) -> bool {
        self.prefs.settings.open_shelves.contains(&shelf)
    }

    /// Open or fold `shelf` and remember it. Its body eases open and shut
    /// (`NavView::shelf_shown`), and the list's motion stands still for it:
    /// the rows would travel over the headers below, which the body's own
    /// height already moves.
    pub(crate) fn set_shelf_open(&mut self, shelf: nav::Shelf, open: bool, cx: &mut Context<Self>) {
        if self.shelf_open(shelf) == open {
            return;
        }
        self.nav_view.read(cx).snap.set(true);
        self.change_settings(
            |settings| {
                settings.open_shelves.retain(|other| *other != shelf);
                if open {
                    settings.open_shelves.push(shelf);
                }
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn snooze_rows(&self) -> Vec<(menu::Item, MenuVerb)> {
        let mut rows: Vec<_> = Preset::available(ferrite_core::clock::system_time())
            .into_iter()
            .map(|(preset, _)| (menu::Item::new(preset.label()), MenuVerb::Snooze(preset)))
            .collect();
        rows.push((menu::Item::new("Custom…"), MenuVerb::SnoozeCustom));
        rows
    }

    pub(super) fn snooze_thread(
        &mut self,
        thread: ThreadKey,
        until: Option<std::time::SystemTime>,
        cx: &mut Context<Self>,
    ) {
        let snoozed = self.cockpit.update(cx, |cockpit, _| cockpit.snooze_thread(thread, until));
        self.when_answered(
            snoozed,
            move |view, snoozed, cx| {
                match snoozed {
                    Ok(()) => {
                        if until.is_some() {
                            view.nav_view.update(cx, |nav, cx| {
                                nav.offer_undo(UndoAct::Snoozed(thread), cx)
                            });
                        }
                        view.nav_notice.clear();
                    }
                    Err(error) => view.nav_notice.problem(error.to_string()),
                }
                cx.notify();
            },
            cx,
        );
    }
}

/// Everything the nav draws from the Cockpit, as plain values: `listed`
/// and `arrivals` as the nav noted them. With them, the Projects the
/// Factory door lists.
fn nav_inputs(
    view: &CockpitView,
    model: &Cockpit,
    listed: &[(ThreadKey, nav::Section, Option<std::time::SystemTime>)],
    arrivals: &nav::Arrivals,
    cx: &App,
) -> (NavInputs, Vec<ProjectKey>) {
    use crate::ferrite_update::State;
    let state = nav_state(view, model, listed, arrivals, cx);
    let inbox = view.inbox_rows(&state, cx);
    let projects = factory_projects(view, model, &state);
    let factory = (!projects.is_empty()).then(|| {
        projects.iter().any(|project| {
            model.factory(project.machine()).needs_you(project.project()) > 0
        })
    });
    let open = state
        .open_rows()
        .chain(&state.parked)
        .filter_map(|row| {
            let open = model.thread(row.key)?;
            Some((
                row.key,
                OpenThread {
                    queued: !open.queue().is_empty(),
                    snooze_until: open.snooze().map(|snooze| snooze.until),
                },
            ))
        })
        .collect();
    let shown = view.focused_thread(cx);
    let provider = shown
        .and_then(|key| model.thread(key))
        .map(|open| open.provider())
        .unwrap_or(view.launch_provider);
    // The foot's limits are the shown Thread's machine's, else this
    // computer's.
    let limits = shown
        .map(|key| key.machine())
        .or(model.this_computer())
        .map(|machine| model.account_limits(machine, provider))
        .unwrap_or_default();
    let update = match view.ferrite_update.state() {
        State::Ready { version, .. } => Some(format!("Restart to update to {version}").into()),
        State::HandedOver { version } => Some(format!("Install Ferrite {version}").into()),
        _ => None,
    };
    let inputs = NavInputs {
        machines: view.machine_notices(cx),
        inbox,
        open_shelves: view.prefs.settings.open_shelves.clone(),
        width: view.nav_expanded_width(),
        shown,
        notice: view.nav_notice.text().cloned(),
        filtered: view
            .nav_filter
            .and_then(|project| model.project(project))
            .map(|project| project.title.clone().into()),
        factory,
        menu: view.context_menu().and_then(|menu| nav_menu(menu.target)),
        door_up: view
            .context_menu()
            .filter(|menu| menu.from_door)
            .and_then(|menu| nav_menu(menu.target)),
        open,
        rename: match view.rename.shown() {
            Some((RenameTarget::Thread(thread), editor)) => Some((thread, editor.clone())),
            _ => None,
        },
        limits,
        free_disk: view.sweep.free_disk(),
        update,
        state,
    };
    (inputs, projects)
}

/// The Projects with a Factory entry over the nav's cards: each one an
/// operator's Thread or a Task is in or whose automatic starts are on,
/// the chosen one's alone while the list is filtered to a Project,
/// which shows it even when empty. Any other Project's Factory is the
/// palette's (`Factory: <Project>`).
fn factory_projects(view: &CockpitView, model: &Cockpit, state: &nav::NavState) -> Vec<ProjectKey> {
    let boards: Vec<_> = model.machines().map(|machine| model.factory(machine)).collect();
    let tasks = boards
        .iter()
        .flat_map(|board| board.tasks.iter().map(|task| board.project_key(task.project)));
    let automatic = boards.iter().flat_map(|board| {
        board
            .workflows
            .iter()
            .filter(|workflow| workflow.automation.switch == ferrite_core::factory_board::Switch::On)
            .map(|workflow| board.project_key(workflow.project))
    });
    let mut projects: Vec<ProjectKey> = state
        .open_rows()
        .chain(&state.parked)
        .filter_map(|row| view.facts.get(row.key).and_then(|facts| facts.project))
        .chain(tasks)
        .chain(automatic)
        .chain(view.nav_filter)
        .filter(|project| view.nav_filter.is_none_or(|wanted| wanted == *project))
        .collect();
    projects.sort();
    projects.dedup();
    projects
}

/// The nav's per-frame state (ADR 0040), from caches and O(1) reads
/// only; the Project filter narrows it and nothing else. Every open
/// Thread is in one section, by priority: snoozed, pinned, working,
/// else active. A Thread works while its turn, its background work
/// or a prompt waiting its turn runs, unless a Decision waits. Pinned
/// keeps pin order; Active puts pending Decisions first, oldest
/// request first, then the latest arrival (`nav::Arrivals`), so a
/// Thread done with its work comes back at the top; Working the latest
/// arrival first; Snoozed the soonest to wake first; Parked the park
/// order. Drafts are not rows (#29).
///
/// The one selection is the shown Thread. The empty board shows none.
fn nav_state(
    view: &CockpitView,
    model: &Cockpit,
    listed: &[(ThreadKey, nav::Section, Option<std::time::SystemTime>)],
    arrivals: &nav::Arrivals,
    cx: &App,
) -> nav::NavState {
    use nav::Section;
    let roster = model.roster();
    let shown = (!view.empty_board_shown(cx))
        .then(|| roster.focused_thread())
        .flatten()
        ;
    let row = |key: ThreadKey| nav::ThreadRow {
        selected: shown == Some(key),
        ..view.thread_row(key, cx)
    };
    let pins = view.pins(cx);
    let latest = |row: &nav::ThreadRow| arrivals.latest_first(row.key);
    let mut state = nav::NavState::default();
    let mut pinned = Vec::new();
    let mut snoozed = Vec::new();
    for &(key, section, _) in listed {
        if !view.admitted(key, cx) {
            continue;
        }
        match section {
            Section::Snoozed => {
                let until = model.thread(key)
                    .and_then(|open| open.snooze())
                    .map(|snooze| snooze.until);
                snoozed.push((until, row(key)));
            }
            Section::Pinned => {
                let at = pins.iter().position(|pin| *pin == key);
                pinned.push((at, row(key)));
            }
            Section::Working => state.working.push(row(key)),
            Section::Active => state.active.push(row(key)),
        }
    }
    pinned.sort_by_key(|(at, _)| *at);
    state.pinned = pinned.into_iter().map(|(_, row)| row).collect();
    snoozed.sort_by_key(|(until, _)| *until);
    state.snoozed = snoozed.into_iter().map(|(_, row)| row).collect();
    let queue = model.needs_you();
    state.active.sort_by_key(|row| {
        (
            queue
                .iter()
                .position(|id| *id == row.key)
                .unwrap_or(usize::MAX),
            latest(row),
        )
    });
    state.working.sort_by_key(latest);
    state.parked = view.parked_threads(cx).into_iter().map(row).collect();
    state
}

/// Every open Thread the nav lists, on every machine, the Project
/// filter's or not, with the section it is in and its last activity: what
/// `nav::Arrivals` notes.
pub(super) fn listed_threads(
    view: &CockpitView,
    model: &Cockpit,
    cx: &App,
) -> Vec<(ThreadKey, nav::Section, Option<std::time::SystemTime>)> {
    use nav::Section;
    #[cfg(test)]
    drawn::listed();
    let mut threads: Vec<ThreadKey> = view
        .panes
        .iter()
        .filter_map(|pane| pane.thread())
        .filter(|thread| {
            model.thread(*thread).is_some() && model.operators(*thread)
        })
        .collect();
    threads.sort();
    threads.dedup();
    let pins = view.pins(cx);
    // Every listed Thread's section, the filter's or not: a Thread
    // that moves while filtered out has still moved.
    threads
        .into_iter()
        .map(|thread| {
            let open = model.thread(thread);
            let section = if open.is_some_and(|open| open.snooze().is_some()) {
                Section::Snoozed
            } else if pins.contains(&thread) {
                Section::Pinned
            } else if open.is_some_and(|open| {
                open.pending_decisions().is_empty()
                    && (open.needs_queue() || open.background_work() || !open.queue().is_empty())
            }) {
                Section::Working
            } else {
                Section::Active
            };
            (thread, section, view.facts.last_used(thread))
        })
        .collect()
}

impl CockpitView {
    pub(super) fn on_nav_event(
        &mut self,
        _: &Entity<NavView>,
        event: &NavEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.clone() {
            NavEvent::Open(key) => {
                self.open_nav_row(key, cx);
            }
            NavEvent::OpenResult(key) => {
                // A search result that opened ends the search.
                let opened = |view: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                    view.nav_view.update(cx, |nav, cx| nav.clear_search(cx));
                    window.blur(cx);
                };
                if self.pane_for(key).is_some() {
                    self.focus_thread(key, cx);
                    opened(self, window, cx);
                } else {
                    self.revive_thread_in(
                        key,
                        window,
                        move |view, back, window, cx| {
                            if back {
                                opened(view, window, cx);
                            }
                        },
                        cx,
                    );
                }
            }
            NavEvent::Notice(verb) => self.notice_verb(verb, cx),
            NavEvent::Revive(key) => {
                self.revive_thread(key, cx);
            }
            NavEvent::Wake(key) => self.snooze_thread(key, None, cx),
            NavEvent::Park(keys) => self.park_listed(&keys, cx),
            NavEvent::Unpin(key) => self.set_pinned(key, false, cx),
            NavEvent::Rename(key) => self.start_rename(RenameTarget::Thread(key), cx),
            NavEvent::OpenPr(key, how) => self.open_pr_as(key, how, window, cx),
            NavEvent::Menu(menu, at) => match at {
                MenuAt::Point(at) => self.open_context_menu(menu.into(), at, cx),
                MenuAt::Under(row) => self.open_context_menu_under(menu.into(), row, cx),
            },
            NavEvent::Door(menu) => self.toggle_door(nav_door(menu), cx),
            NavEvent::SetShelfOpen(shelf, open) => self.set_shelf_open(shelf, open, cx),
            NavEvent::Undo(UndoAct::Parked(parked)) => {
                self.undo_parked(parked, cx);
                cx.notify();
            }
            NavEvent::Undo(UndoAct::Snoozed(key)) => self.snooze_thread(key, None, cx),
            NavEvent::SignIn(machine) => self.sign_in(machine, window, cx),
            NavEvent::Reconnect(machine) => {
                self.cockpit.update(cx, |cockpit, _| cockpit.retry_server(machine));
                cx.notify();
            }
            NavEvent::Snooze(key, until) => {
                let snoozed = self.cockpit.update(cx, |cockpit, _| cockpit.snooze_thread(key, Some(until)));
                self.when_answered(
                    snoozed,
                    |view, snoozed, cx| {
                        let refused = match snoozed {
                            Ok(()) => {
                                view.nav_notice.clear();
                                None
                            }
                            Err(error) => Some(error.to_string().into()),
                        };
                        view.nav_view
                            .update(cx, |nav, cx| nav.snooze_answered(refused, cx));
                        cx.notify();
                    },
                    cx,
                );
            }
            NavEvent::AddProject => self.add_project(window, cx),
            NavEvent::NewThread => window.dispatch_action(Box::new(NewThread), cx),
            NavEvent::Settings => self.toggle_settings(cx),
            NavEvent::PullRequests => self.open_prs_from_foot(window, cx),
            NavEvent::Update => self.press_ferrite_update(window, cx),
        }
    }
}

impl CockpitView {
    /// The parked set changed: rebuild the nav's parked rows now, from the
    /// rows the Server published, and fetch what only `git` can say on the
    /// background executor — the rows fill in when it answers.
    pub(super) fn parked_changed(&mut self, cx: &mut Context<Self>) {
        let lookups = self.facts.parked_changed(self.cockpit.read(cx));
        self.look_up(lookups, cx);
    }

    /// Open the inline editor on a Thread's title. A Thread with no stored
    /// title shows its generated one, so that is what the editor opens on:
    /// the operator edits the name they can see.
    pub(super) fn start_rename(&mut self, target: RenameTarget, cx: &mut Context<Self>) {
        let (RenameTarget::Thread(thread) | RenameTarget::ViewTitle(thread)) = target;
        let title = self.facts.name(thread).to_string();
        self.rename.open(target, title, cx);
        cx.notify();
    }

    /// Close the editor; `save` commits the title it holds. A refusal from
    /// the store lands where every other nav notice does; the editor still
    /// closes, so a row is never stuck in edit.
    pub(super) fn finish_rename(&mut self, save: bool, cx: &mut Context<Self>) {
        if !self.rename.is_open() {
            return;
        }
        let asked = if save {
            self.rename.take(cx)
        } else {
            self.rename.close();
            None
        };
        if let Some((thread, title)) = asked {
            let renamed = self.cockpit.update(cx, |cockpit, _| cockpit.rename_thread(thread, &title));
            self.when_answered(
                renamed,
                move |view, renamed, cx| {
                    match renamed {
                        Err(error) => view.nav_notice.problem(error.to_string()),
                        Ok(()) => {
                            view.facts.renamed(view.cockpit.read(cx), thread);
                            view.refresh_names(cx);
                            view.nav_notice.clear();
                        }
                    }
                    cx.notify();
                },
                cx,
            );
        }
        cx.notify();
    }

    /// The Threads the Parked shelf lists and counts: every parked
    /// Thread the Project filter admits; a parked Thread on screen is still
    /// a parked Thread.
    pub(super) fn parked_threads(&self, cx: &gpui::App) -> Vec<ThreadKey> {
        self.facts
            .parked()
            .iter()
            .copied()
            .filter(|thread| self.admitted(*thread, cx))
            .collect()
    }

    /// Whether any Thread, open or parked, records this Project.
    pub(super) fn project_in_use(&self, project: ProjectKey, cx: &gpui::App) -> bool {
        self.cockpit
            .read(cx)
            .threads()
            .into_iter()
            .chain(self.facts.parked().iter().copied())
            .any(|thread| {
                self.cockpit.read(cx).project_of(thread) == Some(project)
            })
    }

    /// A Thread's effective cwd.
    pub(super) fn target_path(&self, target: MenuTarget, cx: &gpui::App) -> Option<std::path::PathBuf> {
        match target {
            MenuTarget::Thread(key)
            | MenuTarget::Pane(key)
            | MenuTarget::Snooze(key) => self.thread_path(key, cx),
            MenuTarget::File(thread) => self.tools.files.read(cx).pressed(thread),
            MenuTarget::FilesMore(_)
            | MenuTarget::ChangesScope(_)
            | MenuTarget::ChangesMore(_)
            | MenuTarget::ChangesBase(_)
            | MenuTarget::Terminal(_)
            | MenuTarget::TerminalMore(_)
            | MenuTarget::SurfaceMenu(_)
            | MenuTarget::SurfaceTab { .. }
            | MenuTarget::PrMore(_)
            | MenuTarget::PrLink(_)
            | MenuTarget::BrowserPage(_) => None,
            MenuTarget::Details(_) => None,
            MenuTarget::Parked
            | MenuTarget::NavFilter
            | MenuTarget::NavFactory
            | MenuTarget::Review { .. }
            | MenuTarget::Prompt { .. }
            | MenuTarget::Factory
            | MenuTarget::Page(_) => None,
        }
    }

    pub(super) fn thread_path(&self, thread: ThreadKey, cx: &gpui::App) -> Option<std::path::PathBuf> {
        if let Some(open) = self.cockpit.read(cx).thread(thread) {
            return ferrite_core::checkout::effective_cwd(
                open.session_project_root(),
                open.workspace(),
            )
            .map(std::path::Path::to_path_buf);
        }
        let meta = self.cockpit.read(cx).peek(thread).ok()?;
        ferrite_core::checkout::effective_cwd(
            meta.session_project_root.as_deref(),
            meta.workspace.as_ref(),
        )
        .map(std::path::Path::to_path_buf)
    }

    /// Delete every Thread the Parked section lists. Each is its own act
    /// against the core, so one that refuses (a dirty legacy worktree, an
    /// unreadable log) does not stop the rest; the refusals are counted
    /// into the nav's banner, with the last reason, and the survivors stay
    /// listed where the operator can deal with them one at a time.
    pub(super) fn delete_parked_threads(&mut self, cx: &mut Context<Self>) {
        let parked = self.parked_threads(cx);
        self.delete_threads(
            parked,
            |view, deleted, cx| {
                let refused: Vec<String> = deleted
                    .into_iter()
                    .filter_map(|(_, deleted)| deleted.err())
                    .map(|error| error.to_string())
                    .collect();
                match (refused.len(), refused.last()) {
                    (_, None) => view.nav_notice.clear(),
                    (1, Some(reason)) => view
                        .nav_notice
                        .problem(format!("1 parked thread not deleted: {reason}")),
                    (count, Some(reason)) => view
                        .nav_notice
                        .problem(format!("{count} parked threads not deleted, last: {reason}")),
                }
                view.sync_panes(cx);
                view.parked_changed(cx);
            },
            cx,
        );
    }

    /// Point navigation at a Project — or back at all of them — and bring
    /// any standing draft with it. The Composer's chip names the Project a
    /// send will run in, and it must not sit there disagreeing with the
    /// Project the operator just chose (the ⌘K palette's filter command,
    /// or a Project just created). Filtered, the tree is that Project's
    /// section. A draft already bootstrapping is on its way out and is left
    /// alone.
    pub(crate) fn choose_nav_filter(&mut self, project: Option<ProjectKey>, cx: &mut Context<Self>) {
        self.nav_filter = project;
        if let Some(project) = project {
            let standing = self.panes.iter().position(|pane| {
                pane.identity
                    .draft()
                    .is_some_and(|draft| !self.cockpit.read(cx).draft_starting(draft))
            });
            if let Some(index) = standing {
                // The same move the Project chip makes, so the workspace
                // choice under it cannot outlive the repo it named.
                self.panes[index].update_draft(cx, |draft| {
                    draft.binding.choose_project(project);
                    draft.error = None;
                });
            }
        }
        cx.notify();
    }

    /// Pin or unpin a Thread (the row menu's `Pin` / `Unpin`): a pinned
    /// Thread is listed under `Pinned`, newest pin last. Saved with the
    /// settings as its machine's Thread (`SavedThread`).
    pub(crate) fn set_pinned(&mut self, key: ThreadKey, pinned: bool, cx: &mut Context<Self>) {
        let Some(pin) = self.cockpit.read(cx).saved(key) else {
            return;
        };
        self.change_settings(
            |settings| {
                settings.pinned_threads.retain(|saved| *saved != pin);
                if pinned {
                    settings.pinned_threads.push(pin);
                }
            },
            cx,
        );
        cx.notify();
    }

    /// The pinned Threads of the machines the window holds, in pin order.
    pub(super) fn pins(&self, cx: &gpui::App) -> Vec<ThreadKey> {
        let cockpit = self.cockpit.read(cx);
        self.prefs
            .settings
            .pinned_threads
            .iter()
            .filter_map(|pin| cockpit.resolve(pin))
            .collect()
    }

    /// Show the parked Threads (N-18): the nav unfolds if it is folded
    /// away, the Parked shelf opens, and the list takes the keys on its
    /// header — the ⌘K palette's "show parked" command.
    pub(crate) fn show_parked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_shelf_open(nav::Shelf::Parked, true, cx);
        self.focus_nav_on(Some(nav::NavCursor::shelf(nav::Shelf::Parked)), window, cx);
    }

    /// The nav's rows as it last read them: what it draws.
    #[cfg(test)]
    pub(super) fn nav_state(&self, cx: &gpui::App) -> nav::NavState {
        self.nav_view
            .read(cx)
            .inputs()
            .map(|inputs| inputs.state.clone())
            .unwrap_or_default()
    }

    /// Does the Project filter admit this Thread? `All projects` admits
    /// everything; a chosen Project admits only Threads whose cached
    /// Project is it — so a Thread whose Project is unknown appears under
    /// `All projects` alone, rather than being quietly filed under someone
    /// else's Project.
    pub(super) fn admitted(&self, thread: ThreadKey, cx: &gpui::App) -> bool {
        // An owned Thread (a Worker, a Coordinator) is its owner's to list.
        if !self.cockpit.read(cx).operators(thread) {
            return false;
        }
        let Some(wanted) = self.nav_filter else {
            return true;
        };
        self.facts.get(thread).and_then(|facts| facts.project) == Some(wanted)
    }

    /// One Thread's nav row, entirely from caches: the name core has (there
    /// is no display name), its state, the cached Project and checkout
    /// facts, and its word. Not selected: `nav_state` decides what carries
    /// the selection.
    pub(super) fn thread_row(&self, key: ThreadKey, cx: &gpui::App) -> nav::ThreadRow {
        let facts = self.facts.get(key);
        let open = self.cockpit.read(cx).thread(key);
        let now = ferrite_core::clock::system_time();
        // A Notice not yet seen makes a quiet Thread done.
        let unread = self
            .cockpit
            .read(cx)
            .notifications(key.machine())
            .attention(key.thread());
        // The tail reads the Pane's own head slot, so the nav and the Pane
        // never disagree about a Thread (C10).
        let slot =
            open.and_then(|open| pane::thread_word(open, facts.map(|facts| &facts.wall), unread));
        // The mark is squared with that word (`pane::dot_state`): a row whose
        // tail says `failed` never wears an idle dot.
        let status = match open {
            None => nav::RowStatus::Parked,
            Some(open) => {
                let failing = facts.is_some_and(|facts| facts.wall.tests_failing);
                nav::RowStatus::of(
                    pane::dot_state(
                        pane::wall_state(
                            Some(&open.turn()),
                            open.pending_decisions()
                                .iter()
                                .any(|pending| pending.decision.blocks_execution()),
                            failing,
                        ),
                        slot.as_ref(),
                    ),
                    unread,
                    open.background_work(),
                )
            }
        };
        let age = facts
            .and_then(|facts| facts.last_used)
            .map(|at| ferrite_ui::age::since_label(at, now))
            .unwrap_or_default();
        let working = matches!(
            status,
            nav::RowStatus::Working | nav::RowStatus::Failing
        );
        let row = nav::ThreadRow {
            key,
            name: self.facts.name(key),
            status,
            project: facts.and_then(|facts| facts.project_label.clone()),
            selected: false,
            tail: nav::NavTail::of(status, slot.as_ref(), age),
            branch: facts.and_then(Self::checkout_branch),
            worktree: facts.and_then(|facts| facts.worktree.clone()),
            pr: self.facts.pr(key).map(nav::NavPr::of),
            provider: open
                .map(|open| open.provider())
                .or_else(|| facts.and_then(|facts| facts.provider)),
            activity_at: facts.and_then(|facts| facts.last_used),
            working_since: open
                .filter(|_| working)
                .and_then(|open| open.turn().started),
            pinned: self.pins(cx).contains(&key),
            retitling: open.is_some_and(|open| open.titling()),
            // The nav wears its own picks (`NavView::card_picked`).
            picked: false,
            elsewhere: self.elsewhere_row(key, cx),
        };
        #[cfg(test)]
        drawn::record_row(
            key,
            format!(
                "{:?} {:?} branch={:?} pr={:?} provider={:?} receded={} {}",
                row.status,
                row.tail,
                row.branch,
                row.pr.as_ref().map(|pr| (pr.number, pr.state)),
                row.provider,
                row.receded(),
                row.name,
            ),
        );
        row
    }

    /// The branch a checkout is on: the live status's, else the cached one.
    pub(super) fn checkout_branch(facts: &crate::facts::ThreadFacts) -> Option<SharedString> {
        facts
            .status
            .as_ref()
            .and_then(|status| status.branch.clone())
            .map(SharedString::from)
            .or_else(|| facts.branch.clone())
    }

    /// When a Thread was last used, for `sort: recent`. A Thread with no
    /// known time sorts to the bottom rather than to the top: an unknown
    /// time is not a recent one.
    pub(super) fn last_used(&self, thread: ThreadKey) -> std::time::SystemTime {
        self.facts
            .last_used(thread)
            .unwrap_or(std::time::UNIX_EPOCH)
    }

    /// The operator went to a Thread the nav's Project scope hides: the
    /// scope widens to all Projects, so the shown Thread has its row in the
    /// list (and the Filter button says why). Owned Threads are never
    /// listed, so nothing here reveals them.
    pub(super) fn widen_scope_to(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        if self.nav_filter.is_some() && self.cockpit.read(cx).operators(thread) && !self.admitted(thread, cx) {
            self.choose_nav_filter(None, cx);
        }
    }

    /// Read free disk for the nav's foot off the UI thread: the least free of the
    /// volumes holding the Threads' checkouts (SPEC §8), or a fixture
    /// world's figure. Never another machine's, whose checkouts are its
    /// own; tests and captures never read the real disk.
    pub(super) fn read_free_disk(&mut self, cx: &mut Context<Self>) {
        #[cfg(any(test, feature = "demo"))]
        if let Some(bytes) = self.cockpit.read(cx).free_disk_fixture() {
            self.free_disk_landed(Some(bytes), cx);
            return;
        }
        if cfg!(any(test, feature = "visual-reference")) {
            return;
        }
        let mut paths: Vec<std::path::PathBuf> = self
            .cockpit
            .read(cx)
            .threads()
            .into_iter()
            .filter(|thread| !self.elsewhere(thread.machine(), cx))
            .filter_map(|thread| self.thread_path(thread, cx))
            .collect();
        paths.sort();
        paths.dedup();
        cx.spawn(async move |this, cx| {
            let free = cx
                .background_executor()
                .spawn(async move {
                    ferrite_core::disk::least_free(paths.iter().map(std::path::PathBuf::as_path))
                })
                .await;
            this.update(cx, |view, cx| view.free_disk_landed(free, cx))
                .ok();
        })
        .detach();
    }

    /// Adopt a free-disk reading; redraw only when the bar's words move.
    pub(super) fn free_disk_landed(&mut self, free: Option<u64>, cx: &mut Context<Self>) {
        if self.sweep.free_disk_read(free) {
            cx.notify();
        }
    }
}

impl CockpitView {
    /// Bring `thread` to the front for an explicit open: its Pane, from
    /// wherever the nav row's own click would land. A Thread already
    /// shown stays put.
    pub(super) fn show_thread(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        if self.focused_thread(cx) != Some(thread) || self.destination_shown() {
            self.open_nav_row(thread, cx);
        }
    }

    /// `refresh_branches`, but never skipped: a refresh already under way
    /// may have read git before the moment that asks, so one more follows
    /// it as soon as it lands.
    pub(super) fn refresh_branches_soon(&mut self, cx: &mut Context<Self>) {
        if !self.branch_reads.queue_behind_read() {
            self.refresh_branches(cx);
        }
    }

    /// Refresh checkout labels and their branch status without ever waiting
    /// for Git — or `gh` — in GPUI's pump. The same trip asks git which
    /// worktrees each Thread's repo has, and hands the answer to the core:
    /// that is how a binding follows the agent into a worktree it made
    /// (`workspace::follow`), and the labels re-read on the very next pass.
    /// Git runs only for a Thread whose checkouts' files moved since its
    /// labels were read (`checkouts_stamp`): an idle window starts none.
    /// None of it for another machine: no git here can read its checkouts,
    /// and its Server sends each Thread's branch in its digest.
    pub(crate) fn refresh_branches(&mut self, cx: &mut Context<Self>) {
        // A draft has no Thread yet, but its workspace chip names its
        // Project's checkout, which the operator may switch under it.
        let drafted: Vec<ProjectKey> = self
            .panes
            .iter()
            .filter_map(|pane| pane.read(cx).draft())
            .map(|draft| draft.binding.project())
            .collect();
        self.look_up_checkouts(drafted, cx);
        if self.branch_reads.reading() {
            return;
        }
        // Before anything is read: a creation that finishes from here on
        // is judged by the next pass, not by this one.
        let taken_at = std::time::Instant::now();
        // A worktree a Main just made is named only by a listing taken
        // after it finished, whatever the files said before.
        let listing_wanted = self.cockpit.read(cx).wants_worktree_listing();
        let targets: Vec<_> = self
            .cockpit
            .read(cx)
            .threads()
            .into_iter()
            .filter(|thread| !self.elsewhere(thread.machine(), cx))
            .filter_map(|thread| {
                let open = self.cockpit.read(cx).thread(thread)?;
                let repo = open.workspace().map(|binding| binding.repo().to_path_buf());
                let cwd = ferrite_core::checkout::effective_cwd(
                    open.session_project_root(),
                    open.workspace(),
                )?;
                let project = open
                    .project_id()
                    .and_then(|project| {
                        self.cockpit
                            .read(cx)
                            .registry(thread.machine())
                            .reading()
                            .project(project)
                    });
                let directories = project
                    .map(|project| {
                        project
                            .directories()
                            .enumerate()
                            .map(|(index, directory)| {
                                let checkout = if index == 0 { cwd } else { directory };
                                let label = directory
                                    .file_name()
                                    .map(|name| name.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| directory.display().to_string());
                                (SharedString::from(label), checkout.to_path_buf())
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                Some(branch_reads::Target {
                    thread,
                    cwd: cwd.to_path_buf(),
                    directories,
                    repo,
                })
            })
            .collect();
        let Some(read) = self.branch_reads.begin(targets, listing_wanted) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let mut said = cx.background_executor().spawn(async move { read.run() }).await;
            this.update(cx, |view, cx| {
                view.branch_reads.landed(&mut said);
                let branch_reads::Said { branches, listings, .. } = said;
                let labels = view.facts.set_branches(
                    branches
                        .iter()
                        .map(|(thread, _, status, _)| (*thread, status.clone()))
                        .collect(),
                );
                let roots = view.facts.set_project_branches(
                    branches
                        .into_iter()
                        .map(|(thread, _, _, project_branches)| (thread, project_branches))
                        .collect(),
                );
                let mut moved = false;
                for (thread, listing) in listings {
                    if view.cockpit.update(cx, |cockpit, _| cockpit.worktrees_listed(thread, listing, taken_at)) {
                        moved = true;
                        // Into a worktree: its branch is another PR's.
                        view.check_pr(thread, false, cx);
                    }
                }
                // Git said what it said last time: nothing to redraw. A
                // label that moved is its Thread's: its Pane and its nav
                // row. A binding that moved, everything.
                if moved {
                    cx.notify();
                } else if !labels.is_empty() || !roots.is_empty() {
                    for thread in labels.iter().chain(&roots) {
                        view.notify_thread_pane(*thread, cx);
                    }
                    view.notify_part(parts::Part::Nav, cx);
                    view.notify_frame(cx);
                }
                if moved || view.branch_reads.take_queued() {
                    // The labels above were read for the old cwd, or
                    // before a turn ended; go straight back for the new.
                    view.refresh_branches(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn land_on_thread(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        if let Some(owner) = self.cockpit.read(cx).owner(thread) {
            self.land_on_owned(thread, owner, cx);
            return;
        }
        if self.pane_for(thread).is_some() {
            self.focus_thread(thread, cx);
        } else {
            self.revive_thread(thread, cx);
        }
    }

    /// A running nav row's click: land on that Thread's Pane, in the view
    /// that shows it — the core's one door, like every other deliberate
    /// move.
    pub(super) fn focus_thread(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        self.leave_destination();
        if self.cockpit.update(cx, |cockpit, _| cockpit.focus_thread(thread)) {
            if let Some(index) = self.pane_for(thread) {
                self.focus_pane(index, cx);
            }
            self.widen_scope_to(thread, cx);
            cx.notify();
        }
    }

    /// Revive one parked Thread into the view that shows it: the shared
    /// tail of cmd-o and a parked nav row's click (#21). When it does not
    /// come back, the nav says so.
    pub(super) fn revive_thread(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        self.reopen_threads(vec![thread], "reopen", cx);
    }

    /// `revive_thread` for a caller that follows it with the window in
    /// hand: `then` hears whether the Thread came back.
    pub(super) fn revive_thread_in(
        &mut self,
        thread: ThreadKey,
        window: &mut Window,
        then: impl FnOnce(&mut Self, bool, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let asked = self.cockpit.update(cx, |cockpit, _| cockpit.reopen(thread));
        self.when_answered_in(
            asked,
            window,
            move |view, answer, window, cx| {
                let back = view.reopened(vec![(thread, answer)], "reopen", cx);
                then(view, back, window, cx);
            },
            cx,
        );
    }

    /// Revive each of `threads`, in order, into the view that shows it.
    /// The nav names each that did not come back, by what `verb` was asked.
    pub(super) fn reopen_threads(&mut self, threads: Vec<ThreadKey>, verb: &'static str, cx: &mut Context<Self>) {
        let asked = self.cockpit.update(cx, |cockpit, _| {
            threads.iter().map(|thread| cockpit.reopen(*thread)).collect()
        });
        self.when_all_answered(
            asked,
            move |view, answers, cx| {
                view.reopened(threads.into_iter().zip(answers).collect(), verb, cx);
            },
            cx,
        );
    }

    /// The Server answered for Threads asked back: the board's own state
    /// gives way only to a Thread that is back. Whether all came back.
    pub(super) fn reopened(
        &mut self,
        answers: Vec<(ThreadKey, std::io::Result<()>)>,
        verb: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let outcomes: Vec<nav_undo::Outcome> = answers
            .into_iter()
            .map(|(thread, answer)| nav_undo::Outcome {
                thread,
                result: answer.map_err(|error| error.to_string()),
            })
            .collect();
        for back in outcomes.iter().filter(|outcome| outcome.result.is_ok()) {
            self.leave_destination();
            self.widen_scope_to(back.thread, cx);
        }
        self.sync_panes(cx);
        self.report_failures(verb, &outcomes);
        cx.notify();
        outcomes.iter().all(|outcome| outcome.result.is_ok())
    }
}
