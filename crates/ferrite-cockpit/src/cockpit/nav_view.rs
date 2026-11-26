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
