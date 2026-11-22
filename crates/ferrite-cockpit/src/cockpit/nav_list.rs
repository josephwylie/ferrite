//! The nav's one list, wired (ADR 0040): Pinned and Active cards from the
//! top, each other machine's heading and rows after them, then the Working, Snoozed
//! and Parked shelves anchored over the foot. Each row rides the list's
//! motion (`nav::flip`), keyed by its Thread and its kind, so a card that
//! moves from Active to Working slides there while one that parks leaves
//! as a ghost and enters the Parked shelf as a slim row.

use super::menu_button::{Door, Edge};
use super::nav_view::{MenuAt, NavEvent, NavInputs, NavView};
use super::*;
use crate::machines::{Connection, MachineRow};
use crate::nav::NavMachine;
use crate::nav::flip::{self, Update};
use crate::nav::{list, NavItem, Shelf, SHELVES};
use ferrite_ui::components::ContextPress;
use ferrite_ui::motion::{fold_body, Reveal, FOLD};
use std::time::Instant;

/// A row of the list as its motion keys it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum NavKey {
    Card(ThreadKey),
    Slim(ThreadKey),
}

/// What a row last looked like, for its ghost.
#[derive(Clone)]
pub(crate) enum NavFace {
    Card(nav::ThreadRow),
    Slim(nav::ThreadRow, list::SlimTime),
}

/// The list's motion, shared with its elements.
pub(crate) type NavMotion = flip::Motion<NavKey, NavFace>;

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


impl CockpitView {
    pub(crate) fn shelf_open(&self, shelf: Shelf) -> bool {
        self.prefs.settings.open_shelves.contains(&shelf)
    }

    /// Open or fold `shelf` and remember it. Its body eases open and shut
    /// (`NavView::shelf_shown`), and the list's motion stands still for it:
    /// the rows would travel over the headers below, which the body's own
    /// height already moves.
    pub(crate) fn set_shelf_open(&mut self, shelf: Shelf, open: bool, cx: &mut Context<Self>) {
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
}

impl NavView {
    pub(super) fn content(&self, inputs: &NavInputs, window: &Window, cx: &mut Context<Self>) -> Div {
        #[cfg(test)]
        drawn::built_nav();
        self.drawn.borrow_mut().clear();
        let list = if self.searching(cx) {
            self.search_results(&inputs.state, cx)
        } else {
            self.list(inputs, window, cx)
        };
        let list = self.sweep_release(list, cx);
        nav::content().w(px(inputs.width))
            .child(self.search_row(inputs, window, cx))
            // Over the list, outside its keys: the field's ⏎ and Esc are its own.
            .children(self.snooze_field(inputs))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(list)
                    .children(nav::edge_fades(&self.scroll))
                    .child(nav::scrollbar(&self.scroll, self.hovered)),
            )
            .child(self.foot(inputs, window, cx))
    }

    /// How far `shelf`'s body is open on screen, 0 to 1: it follows the
    /// setting, turning round from where it is when that flips, and is
    /// already there on first sight.
    fn shelf_shown(&self, inputs: &NavInputs, shelf: Shelf, now: Instant, reduced: bool) -> (f32, bool) {
        let open = inputs.shelf_open(shelf);
        let mut folds = self.shelf_folds.borrow_mut();
        let at = match folds.iter().position(|(other, _)| *other == shelf) {
            Some(at) => at,
            None => {
                let fold = if open {
                    Reveal::open(FOLD, FOLD, now)
                } else {
                    Reveal::closed(FOLD, FOLD, now)
                };
                folds.push((shelf, fold));
                folds.len() - 1
            }
        };
        let fold = &mut folds[at].1;
        fold.set(open, now, reduced);
        (fold.value(now, reduced), fold.running(now, reduced))
    }

    /// The rows a shelf draws: all of them open (Parked's first pages),
    /// folded only the shown Thread's.
    pub(crate) fn shelf_rows<'a>(
        &self,
        inputs: &'a NavInputs,
        shelf: Shelf,
    ) -> Vec<&'a nav::ThreadRow> {
        self.rows_of(&inputs.state, shelf, inputs.shelf_open(shelf))
    }

    /// [`Self::shelf_rows`] for a shelf `open` or not.
    fn rows_of<'a>(
        &self,
        state: &'a nav::NavState,
        shelf: Shelf,
        open: bool,
    ) -> Vec<&'a nav::ThreadRow> {
        let rows = state.shelf(shelf);
        if !open {
            return rows.iter().filter(|row| row.selected).collect();
        }
        match shelf {
            Shelf::Parked => rows.iter().take(self.parked_shown).collect(),
            _ => rows.iter().collect(),
        }
    }

    /// The list (ADR 0040), scrolling, holding its keys (`nav_keys`).
    fn list(
        &self,
        inputs: &NavInputs,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let state = &inputs.state;
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut faces: Vec<(NavKey, NavFace)> = Vec::new();
        rows.extend(
            inputs
                .notice
                .as_ref()
                .map(|error| nav::notice(error.clone()).into_any_element()),
        );
        // With machines (ADR 0026): this computer heads the list, which
        // folds under it; every other machine follows its cards, its
        // Threads under its heading.
        let machines = inputs.machines.as_ref();
        let home = machines.and_then(|rows| rows.iter().find(|machine| machine.home));
        if let Some(home) = home {
            rows.push(self.machine_heading(&home.row, cx));
        }
        let folded = home.is_some_and(|home| home.row.folded);
        for row in state.pinned.iter().filter(|_| !folded) {
            rows.push(self.card(inputs, row, None, &mut faces, cx));
        }
        for notice in inputs.inbox.iter().filter(|_| !folded) {
            let local = state.active.iter().find(|row| row.key == notice.thread);
            rows.push(match local {
                Some(row) => self.card(inputs, row, Some(notice.open_verb()), &mut faces, cx),
                None => {
                    let verb = notice.open_verb();
                    let index = rows.len();
                    // Another machine's Thread: dimmed while that machine is
                    // lost, as its rows under its heading are.
                    let lost = machines.into_iter().flatten().any(|machine| {
                        machine.row.held == Some(notice.thread.machine())
                            && machine.row.connection != Connection::Online
                    });
                    nav::notice_row(index, notice, false, &nav::Measure::Text(cx, inputs.width))
                        .when(lost, |row| row.opacity(ferrite_ui::theme::MACHINE_LOST_OPACITY))
                        .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                            cx.emit(NavEvent::Notice(verb.clone()));
                            window.blur(cx);
                        }))
                        .into_any_element()
                }
            });
        }
        if let Some(machines) = machines {
            rows.extend(self.other_machines(inputs, machines, cx));
        }
        if !folded && state.pinned.is_empty() && state.active.is_empty() {
            rows.push(self.empty(inputs, cx));
        }
        let now = cx.background_executor().now();
        let reduced = ferrite_ui::motion::reduced_motion(cx);
        let mut folding = false;
        let mut anchored = false;
        for shelf in SHELVES.into_iter().filter(|_| !folded) {
            let all = state.shelf(shelf);
            if all.is_empty() {
                continue;
            }
            if !anchored {
                rows.push(list::spacer().into_any_element());
                anchored = true;
            }
            rows.push(self.shelf_header(inputs, shelf, all.len(), cx));
            // Its body: the rows between the shown Thread's (which a fold
            // keeps) ride one clip each, so they close without the
            // Thread's row, or the rows below, jumping.
            let (shown, moving) = self.shelf_shown(inputs, shelf, now, reduced);
            folding |= moving;
            let name = list::shelf_key(shelf);
            let mut body: Vec<AnyElement> = Vec::new();
            let mut segments = 0;
            let mut clip = |body: &mut Vec<AnyElement>, rows: &mut Vec<AnyElement>| {
                if !body.is_empty() {
                    segments += 1;
                    let id = SharedString::from(format!("{name}-fold-{segments}"));
                    let clipped = div().flex().flex_col().children(std::mem::take(body));
                    rows.push(fold_body(id, shown, clipped).into_any_element());
                }
            };
            for row in self.rows_of(state, shelf, shown > 0.0) {
                let face = match shelf {
                    Shelf::Working => self.card(inputs, row, None, &mut faces, cx),
                    Shelf::Snoozed | Shelf::Parked => self.slim(inputs, row, shelf, &mut faces, cx),
                };
                if row.selected {
                    clip(&mut body, &mut rows);
                    rows.push(face);
                } else {
                    body.push(face);
                }
            }
            if shelf == Shelf::Parked && shown > 0.0 && all.len() > self.parked_shown {
                body.push(
                    list::show_more_parked()
                        .on_click(cx.listener(|nav, _: &ClickEvent, _, cx| {
                            nav.parked_shown += ferrite_ui::theme::NAV_PARKED_PAGE;
                            cx.notify();
                        }))
                        .into_any_element(),
                );
            }
            clip(&mut body, &mut rows);
        }
        let ghosts = self.motion.borrow_mut().begin(Update {
            rows: faces,
            snap: self.snap.replace(folding)
                || folding
                || cx.has_active_drag()
                || self.viewport.replace(window.viewport_size()) != window.viewport_size(),
            now,
            reduced,
        });
        let measure = nav::Measure::Text(cx, inputs.width);
        let ghosts = ghosts.into_iter().map(|ghost| {
            let (top, opacity) = ghost.at(now);
            let face = match &ghost.face {
                NavFace::Card(row) => nav::ghost_card(row, &measure),
                NavFace::Slim(row, time) => list::ghost_slim(row, time.clone(), &measure),
            };
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(top))
                .opacity(opacity)
                .child(face)
        });
        if folding || self.motion.borrow().running(now) {
            ferrite_ui::motion::request_frame(window);
        }
        let body = list::body().children(rows).children(ghosts);
        self.keys(nav::list_scroll(&self.scroll), cx)
            .child(flip::flip_list(&self.motion, body))
    }

    /// The list with nothing above its shelves: T3's words, centred, and
    /// Ferrite's way forward under them.
    fn empty(&self, inputs: &NavInputs, cx: &mut Context<Self>) -> AnyElement {
        let state = &inputs.state;
        let anything = !(state.working.is_empty() && state.snoozed.is_empty())
            || !state.parked.is_empty();
        let words = if inputs.filtered.is_none() && anything {
            "Nothing needs you".to_string()
        } else {
            list::empty_words(inputs.filtered.as_deref())
        };
        let parked = (!state.parked.is_empty() && !inputs.shelf_open(Shelf::Parked)).then(|| {
            list::parked_link()
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                    cx.emit(NavEvent::SetShelfOpen(Shelf::Parked, true));
                }))
                .into_any_element()
        });
        list::empty("nav-empty", words, parked).into_any_element()
    }

    /// A shelf's header, wired: its press folds or opens the shelf.
    fn shelf_header(&self, inputs: &NavInputs, shelf: Shelf, count: usize, cx: &mut Context<Self>) -> AnyElement {
        let open = inputs.shelf_open(shelf);
        let cursor = self.cursor.as_ref().map(|cursor| cursor.item) == Some(NavItem::Shelf(shelf));
        list::shelf_heading(shelf, count, open, cursor)
            .children(self.cursor_reveal(NavItem::Shelf(shelf)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _: &MouseDownEvent, _, cx| {
                    cx.emit(NavEvent::SetShelfOpen(shelf, !open));
                }),
            )
            .when(shelf == Shelf::Parked, |header| {
                header.on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|_, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        cx.emit(NavEvent::Menu(MenuTarget::Parked, MenuAt::Point(event.position)));
                    }),
                )
            })
            .into_any_element()
    }

    /// A Thread's card, wired, riding the list's motion: a press opens it
    /// (`open`, a request's own door, else its Thread), ⌘/⇧ picks it, a
    /// right press opens its menu, and a drag carries it onto the board.
    fn card(
        &self,
        inputs: &NavInputs,
        row: &nav::ThreadRow,
        open: Option<Verb>,
        faces: &mut Vec<(NavKey, NavFace)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let thread = row.key;
        faces.push((NavKey::Card(thread), NavFace::Card(row.clone())));
        let title = self.thread_title(inputs, thread, row.name.clone(), true, cx);
        let card = nav::thread_card(
            row,
            title,
            self.card_doors(inputs, row, cx),
            renaming(inputs, thread),
            &nav::Measure::Text(cx, inputs.width),
        )
        .children(self.cursor_reveal(NavItem::Thread(thread)))
        .children(self.shown_reveal(row))
        .children(self.hover_card(inputs, row, cx));
        let card = self.pickable_card(card, thread, cx)
            .on_click(cx.listener(move |nav, event: &ClickEvent, window, cx| {
                cx.stop_propagation();
                if nav.pick_card(thread, event.modifiers(), cx) {
                    return;
                }
                nav.close_snooze(cx);
                cx.emit(match &open {
                    Some(verb) => NavEvent::Notice(verb.clone()),
                    None => NavEvent::Open(thread),
                });
                window.blur(cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |nav, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    nav.pick_for_menu(thread, cx);
                    cx.emit(NavEvent::Menu(MenuTarget::Thread(thread), MenuAt::Point(event.position)));
                }),
            );
        flip::flip_row(&self.motion, NavKey::Card(thread), card).into_any_element()
    }

    /// What a Thread's card wires (`nav::CardDoors`): its hover strip —
    /// Snooze, whose menu opens under it (hidden while a turn is queued),
    /// and Park — for an open Thread; the pin's unpin; and the PR chip,
    /// whose press opens the PR in the Panel's preview (⌘-press, in the
    /// browser). Pin stays in the context menu.
    fn card_doors(&self, inputs: &NavInputs, row: &nav::ThreadRow, cx: &mut Context<Self>) -> nav::CardDoors {
        let thread = row.key;
        let held = inputs.menu == Some(MenuTarget::Snooze(thread));
        let strip = inputs.open.get(&thread).map(|open| {
            let mut buttons = Vec::new();
            if !open.queued {
                buttons.push(
                    inputs
                        .doors
                        .button(
                            nav::snooze_button(thread),
                            Door::new(MenuTarget::Snooze(thread), Edge::Left),
                            "Snooze thread",
                            cx,
                        )
                        .into_any_element(),
                );
            }
            buttons.push(
                nav::park_button(thread)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |nav, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            nav.arm_park_sweep(thread, cx);
                        }),
                    )
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .into_any_element(),
            );
            nav::strip(buttons, nav::strip_w(!open.queued, &nav::Measure::Text(cx, inputs.width)))
        });
        let pin = nav::pin_glyph(thread, true)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    cx.emit(NavEvent::Unpin(thread));
                }),
            )
            .on_click(|_, _, cx| cx.stop_propagation())
            .into_any_element();
        let pr = row.pr.as_ref().map(|pr| {
            nav::pr_chip(thread, pr)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        cx.emit(NavEvent::OpenPr(thread, Open::of_modifiers(event.modifiers)));
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        cx.emit(NavEvent::Menu(MenuTarget::PrLink(thread), MenuAt::Point(event.position)));
                    }),
                )
                .on_click(|_, _, cx| cx.stop_propagation())
                .into_any_element()
        });
        nav::CardDoors {
            strip,
            strip_held: held,
            pr,
            pin: Some(pin),
        }
    }

    /// A Thread's title cell: the rename field while it edits, else its
    /// title, whose second click renames it.
    fn thread_title(
        &self,
        inputs: &NavInputs,
        thread: ThreadKey,
        title: SharedString,
        hint: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some((editing, editor)) = &inputs.rename {
            if *editing == thread {
                return rename_field(editor.clone(), hint)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
                    )
                    .into_any_element();
            }
        }
        nav::rename_target_thread(thread, title)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                    // The first click focuses the Thread, the second
                    // renames it.
                    if event.click_count < 2 {
                        return;
                    }
                    cx.stop_propagation();
                    cx.emit(NavEvent::Rename(thread));
                }),
            )
            .into_any_element()
    }

    /// A card's hover detail, while the pointer rests on it — never during
    /// a drag or with its Snooze menu open.
    fn hover_card(&self, inputs: &NavInputs, row: &nav::ThreadRow, cx: &gpui::App) -> Option<AnyElement> {
        let thread = row.key;
        let snoozing = matches!(inputs.menu, Some(MenuTarget::Snooze(_)));
        if snoozing
            || cx.has_active_drag()
            || ferrite_ui::motion::hover_t(&crate::nav::card_key(thread)) < 1.0
        {
            return None;
        }
        let mut lines = Vec::new();
        if let Some(branch) = &row.branch {
            lines.push((ferrite_ui::icons::BRANCH, branch.clone()));
        }
        if let Some(worktree) = &row.worktree {
            lines.push((ferrite_ui::icons::FOLDER, worktree.clone()));
        }
        if let Some(provider) = row.provider {
            lines.push((
                ferrite_ui::icons::LAPTOP,
                SharedString::from(ferrite_ui::provider::title(provider)),
            ));
        }
        Some(crate::nav::chrome::hover_card(
            row,
            crate::nav::chrome::HoverLines { lines },
            inputs.width,
        ))
    }

    /// A Snoozed or Parked shelf's slim row, wired: a press opens (a
    /// parked one revives), its action wakes or revives it, a right press
    /// opens its menu.
    fn slim(
        &self,
        inputs: &NavInputs,
        row: &nav::ThreadRow,
        shelf: Shelf,
        faces: &mut Vec<(NavKey, NavFace)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let thread = row.key;
        let wall = ferrite_core::clock::system_time();
        let (time, action) = match shelf {
            Shelf::Snoozed => {
                let until = inputs
                    .open
                    .get(&thread)
                    .and_then(|open| open.snooze_until)
                    .unwrap_or(wall);
                let wake = list::wake_button(thread)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            cx.emit(NavEvent::Wake(thread));
                        }),
                    )
                    .on_click(|_, _, cx| cx.stop_propagation());
                (
                    list::SlimTime::Snoozed(list::until_wake(until, wall)),
                    wake.into_any_element(),
                )
            }
            _ => {
                let revive = list::revive_button(thread)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            cx.emit(NavEvent::Revive(thread));
                        }),
                    )
                    .on_click(|_, _, cx| cx.stop_propagation());
                let age = row
                    .activity_at
                    .map(|at| nav::card_age(at, wall))
                    .unwrap_or_default();
                (list::SlimTime::Parked(age), revive.into_any_element())
            }
        };
        faces.push((NavKey::Slim(thread), NavFace::Slim(row.clone(), time.clone())));
        let title = if renaming(inputs, thread) {
            list::SlimTitle::Editor(self.thread_title(inputs, thread, row.name.clone(), false, cx))
        } else {
            list::SlimTitle::Row
        };
        let slim = list::slim_row(row, title, time, action, &nav::Measure::Text(cx, inputs.width))
            .children(self.cursor_reveal(NavItem::Thread(thread)))
            .children(self.shown_reveal(row))
            .on_click(cx.listener(move |nav, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                nav.close_snooze(cx);
                cx.emit(NavEvent::Open(thread));
                window.blur(cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |nav, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    nav.pick_for_menu(thread, cx);
                    cx.emit(NavEvent::Menu(MenuTarget::Thread(thread), MenuAt::Point(event.position)));
                }),
            );
        flip::flip_row(&self.motion, NavKey::Slim(thread), slim).into_any_element()
    }

    /// A machine's heading: its press, or its triangle's, folds it; a
    /// held machine's right-click offers what can be made on it.
    fn machine_heading(&self, row: &MachineRow, cx: &mut Context<Self>) -> AnyElement {
        let index = row.index;
        let held = row.held;
        let fold = nav::machine_fold(index, row.folded).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |_, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(NavEvent::FoldMachine(index));
            }),
        );
        nav::machine_heading(row, fold)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _: &MouseDownEvent, _, cx| {
                    cx.emit(NavEvent::FoldMachine(index));
                }),
            )
            .when_some(held, |heading, machine| {
                let nav = cx.entity().downgrade();
                heading.context_press(move |at, _, cx| {
                    let menu = NavEvent::Menu(MenuTarget::Machine(machine), MenuAt::Point(at));
                    let _ = nav.update(cx, |_, cx| cx.emit(menu));
                })
            })
            .into_any_element()
    }

    /// Every machine but this computer: its heading and, unfolded, the
    /// Threads it has open. A Thread's press opens it in place, beside
    /// whatever the board shows; its title renames as a card's does; its
    /// right-click opens the Thread's menu, as a card's does.
    fn other_machines(&self, inputs: &NavInputs, machines: &[NavMachine], cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut drawn = Vec::new();
        for machine in machines.iter().filter(|machine| !machine.home) {
            drawn.push(self.machine_heading(&machine.row, cx));
            if machine.row.folded {
                continue;
            }
            let lost = machine.row.connection != Connection::Online;
            let rows = inputs
                .state
                .elsewhere
                .iter()
                .filter(|row| Some(row.key.machine()) == machine.row.held);
            for row in rows {
                let key = row.key;
                let nav = cx.entity().downgrade();
                let title = self.thread_title(inputs, key, row.name.clone(), false, cx);
                drawn.push(
                    nav::machine_thread_row(row, lost, title)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |_, _: &MouseDownEvent, _, cx| {
                                cx.emit(NavEvent::Open(key));
                            }),
                        )
                        .context_press(move |at, _, cx| {
                            let _ = nav.update(cx, |nav, cx| {
                                nav.pick_for_menu(key, cx);
                                cx.emit(NavEvent::Menu(MenuTarget::Thread(key), MenuAt::Point(at)));
                            });
                        })
                        .into_any_element(),
                );
            }
        }
        drawn
    }
}

/// Whether the rename field is open on `thread`'s title.
fn renaming(inputs: &NavInputs, thread: ThreadKey) -> bool {
    inputs.rename.as_ref().is_some_and(|(editing, _)| *editing == thread)
}
