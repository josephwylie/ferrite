//! The nav's one list, wired (ADR 0040): Pinned and Active cards from the
//! top, a machine off screen's rows after them, then the Working, Snoozed
//! and Parked shelves anchored over the foot. Each row rides the list's
//! motion (`nav::flip`), keyed by its Thread and its kind, so a card that
//! moves from Active to Working slides there while one that parks leaves
//! as a ghost and enters the Parked shelf as a slim row.

use super::*;
use crate::nav::flip::{self, Update};
use crate::nav::{list, NavItem, Shelf, SHELVES};

/// A row of the list as its motion keys it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum NavKey {
    Card(ThreadId),
    Slim(ThreadId),
}

/// What a row last looked like, for its ghost.
#[derive(Clone)]
pub(crate) enum NavFace {
    Card(nav::ThreadRow),
    Slim(nav::ThreadRow, list::SlimTime),
}

/// The list's motion, shared with its elements.
pub(crate) type NavMotion = flip::Motion<NavKey, NavFace>;

impl CockpitView {
    /// The nav's content at its own width (`nav::content`): the head in the
    /// head row, the search row, the list — or, while the field holds
    /// text, its results — with its fades and scrollbar, and the foot.
    pub(super) fn nav_content(
        &self,
        state: &nav::NavState,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        #[cfg(test)]
        drawn::built_nav();
        self.nav_drawn.borrow_mut().clear();
        let list = if self.nav_searching(cx) {
            self.search_results(state, cx)
        } else {
            self.nav_list(state, window, cx)
        };
        let list = self.sweep_release(list, cx);
        nav::content()
            .child(nav::head(
                crate::frame().head_drag(!self.overlay_open(), self.maximized),
                self.nav_toggle(false, cx),
            ))
            .child(self.search_row(window, cx))
            // Over the list, outside its keys: the field's ⏎ and Esc are its own.
            .children(self.snooze_field())
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(list)
                    .children(nav::edge_fades(&self.nav_scroll))
                    .child(nav::scrollbar(&self.nav_scroll, self.nav_hovered)),
            )
            .child(self.nav_foot(window, cx))
    }

    /// Whether `shelf` is open (`Settings::open_shelves`).
    pub(crate) fn shelf_open(&self, shelf: Shelf) -> bool {
        self.prefs.settings.open_shelves.contains(&shelf)
    }

    /// Open or fold `shelf`, at once, and remember it. The list's motion
    /// stands still for it: the shelf's rows would travel over the headers
    /// below.
    pub(crate) fn set_shelf_open(&mut self, shelf: Shelf, open: bool, cx: &mut Context<Self>) {
        if self.shelf_open(shelf) == open {
            return;
        }
        self.nav_snap.set(true);
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

    /// The rows a shelf draws: all of them open (Parked's first pages),
    /// folded only the shown Thread's.
    pub(crate) fn shelf_rows<'a>(
        &self,
        state: &'a nav::NavState,
        shelf: Shelf,
    ) -> Vec<&'a nav::ThreadRow> {
        let rows = state.shelf(shelf);
        if !self.shelf_open(shelf) {
            return rows.iter().filter(|row| row.selected).collect();
        }
        match shelf {
            Shelf::Parked => rows.iter().take(self.nav_parked_shown).collect(),
            _ => rows.iter().collect(),
        }
    }

    /// The list (ADR 0040), scrolling, holding its keys (`nav_keys`).
    fn nav_list(
        &self,
        state: &nav::NavState,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut faces: Vec<(NavKey, NavFace)> = Vec::new();
        rows.extend(
            self.group_error
                .as_ref()
                .map(|error| nav::notice(error.clone()).into_any_element()),
        );
        // With machines (ADR 0026): the one the window is on heads the
        // list, which folds under it; the others follow its cards.
        let machines = self.machine_rows(cx);
        let on = machines
            .as_ref()
            .and_then(|rows| rows.iter().find(|row| row.selected));
        if let Some(on) = on {
            rows.push(self.machine_heading(on, cx));
        }
        let folded = on.is_some_and(|on| on.folded);
        if !folded {
            rows.extend(self.factory_nav_rows(state, cx));
        }
        for row in state.pinned.iter().filter(|_| !folded) {
            rows.push(self.nav_card(row, None, &mut faces, cx));
        }
        for notice in self.inbox_rows(state, cx).into_iter().filter(|_| !folded) {
            let local = notice
                .away
                .is_none()
                .then(|| state.active.iter().find(|row| row.thread == notice.thread))
                .flatten();
            rows.push(match local {
                Some(row) => self.nav_card(row, Some(notice.open_verb()), &mut faces, cx),
                None => {
                    let verb = notice.open_verb();
                    let index = rows.len();
                    nav::notice_row(index, &notice, false, &nav::Measure::Text(cx))
                        .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                            view.notice_verb(verb.clone(), cx);
                            window.blur(cx);
                        }))
                        .into_any_element()
                }
            });
        }
        if let Some(machines) = &machines {
            rows.extend(self.machines_off_screen(machines, cx));
        }
        if !folded && state.pinned.is_empty() && state.active.is_empty() {
            rows.push(self.nav_empty(state, cx));
        }
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
            rows.push(self.shelf_header(shelf, all.len(), cx));
            for row in self.shelf_rows(state, shelf) {
                rows.push(match shelf {
                    Shelf::Working => self.nav_card(row, None, &mut faces, cx),
                    Shelf::Snoozed | Shelf::Parked => self.nav_slim(row, shelf, &mut faces, cx),
                });
            }
            if shelf == Shelf::Parked
                && self.shelf_open(shelf)
                && all.len() > self.nav_parked_shown
            {
                rows.push(
                    list::show_more_parked()
                        .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                            view.nav_parked_shown += ferrite_ui::theme::NAV_PARKED_PAGE;
                            cx.notify();
                        }))
                        .into_any_element(),
                );
            }
        }
        let now = cx.background_executor().now();
        let ghosts = self.nav_motion.borrow_mut().begin(Update {
            rows: faces,
            snap: self.nav_snap.replace(false)
                || self.nav_drag_live.get()
                || self.nav_viewport.replace(window.viewport_size()) != window.viewport_size(),
            now,
            reduced: ferrite_ui::motion::reduced_motion(cx),
        });
        let measure = nav::Measure::Text(cx);
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
        if self.nav_motion.borrow().running(now) {
            ferrite_ui::motion::request_frame(window);
        }
        let body = list::body().children(rows).children(ghosts);
        let body = if self.machine_lost() {
            machines::held(body, "machine-lost-list").into_any_element()
        } else {
            body.into_any_element()
        };
        self.nav_keys(nav::list_scroll(&self.nav_scroll), cx)
            .child(flip::flip_list(&self.nav_motion, body))
    }

    /// The list with nothing above its shelves: T3's words, centred, and
    /// Ferrite's way forward under them.
    fn nav_empty(&self, state: &nav::NavState, cx: &mut Context<Self>) -> AnyElement {
        let filtered = self
            .nav_filter
            .and_then(|project| self.cockpit.registry().project(project))
            .map(|project| project.title.clone());
        let anything = !(state.working.is_empty() && state.snoozed.is_empty())
            || !state.parked.is_empty();
        let words = if filtered.is_none() && anything {
            "Nothing needs you".to_string()
        } else {
            list::empty_words(filtered.as_deref())
        };
        let parked = (!state.parked.is_empty() && !self.shelf_open(Shelf::Parked)).then(|| {
            list::parked_link()
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    view.set_shelf_open(Shelf::Parked, true, cx);
                }))
                .into_any_element()
        });
        list::empty("nav-empty", words, parked).into_any_element()
    }

    /// A shelf's header, wired: its press folds or opens the shelf.
    fn shelf_header(&self, shelf: Shelf, count: usize, cx: &mut Context<Self>) -> AnyElement {
        let open = self.shelf_open(shelf);
        let cursor = self.nav_cursor.as_ref().map(|cursor| cursor.item) == Some(NavItem::Shelf(shelf));
        list::shelf_heading(shelf, count, open, cursor)
            .children(self.cursor_reveal(NavItem::Shelf(shelf)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    view.set_shelf_open(shelf, !open, cx);
                }),
            )
            .when(shelf == Shelf::Parked, |header| {
                header.on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|view, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        view.open_context_menu(MenuTarget::Parked, event.position, cx);
                    }),
                )
            })
            .into_any_element()
    }

    /// A Thread's card, wired, riding the list's motion: a press opens it
    /// (`open`, a request's own door, else its Thread), ⌘/⇧ picks it, a
    /// right press opens its menu, and a drag carries it onto the board.
    fn nav_card(
        &self,
        row: &nav::ThreadRow,
        open: Option<Verb>,
        faces: &mut Vec<(NavKey, NavFace)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let thread = row.thread;
        faces.push((NavKey::Card(thread), NavFace::Card(row.clone())));
        let title = self.editable_thread_title(thread, row.name.clone(), cx);
        let card = nav::thread_card(
            row,
            title,
            self.card_doors(thread, cx),
            self.renaming_thread(thread),
            &nav::Measure::Text(cx),
        )
        .children(self.cursor_reveal(NavItem::Thread(thread)))
        .children(self.shown_reveal(row))
        .children(self.hover_card(row, cx));
        let face = self.thread_ghost(thread);
        let nav_edge = self.nav_width();
        let chip = row.clone();
        let lifted = matches!(
            self.nav_dragging,
            Some(Drag::Thread { thread: dragged, .. }) if dragged == thread
        );
        let card = self
            .pickable_card(card, thread, cx)
            .when(lifted, |card| card.opacity(ferrite_ui::theme::DRAG_SOURCE_OPACITY))
            .on_drag(
                NavDrag {
                    drag: Drag::Thread {
                        thread,
                        group: None,
                    },
                },
                move |_, grab, _, cx| {
                    let face = face.clone();
                    let chip = chip.clone();
                    cx.new(|_| PaneGhost {
                        face,
                        grab,
                        row_left_of: Some((nav_edge, NavChip::Thread(chip))),
                    })
                },
            )
            // A click, not a press: pressing a card to drag it onto the
            // board must leave the board showing.
            .on_click(cx.listener(move |view, event: &ClickEvent, window, cx| {
                cx.stop_propagation();
                if view.pick_card(thread, event.modifiers(), cx) {
                    return;
                }
                view.snooze_editor = None;
                match &open {
                    Some(verb) => view.notice_verb(verb.clone(), cx),
                    None => view.open_nav_row(thread, cx),
                }
                window.blur(cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.pick_for_menu(thread, cx);
                    view.open_context_menu(MenuTarget::Thread(thread), event.position, cx);
                }),
            );
        flip::flip_row(&self.nav_motion, NavKey::Card(thread), card).into_any_element()
    }

    /// A card's hover detail, while the pointer rests on it — never during
    /// a drag or with its Snooze menu open.
    fn hover_card(&self, row: &nav::ThreadRow, cx: &gpui::App) -> Option<AnyElement> {
        let thread = row.thread;
        let snoozing = matches!(
            self.context_menu.as_ref().map(|menu| menu.target),
            Some(MenuTarget::Snooze(_))
        );
        if snoozing
            || self.nav_drag_live.get()
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
        ))
    }

    /// A Snoozed or Parked shelf's slim row, wired: a press opens (a
    /// parked one revives), its action wakes or revives it, a right press
    /// opens its menu.
    fn nav_slim(
        &self,
        row: &nav::ThreadRow,
        shelf: Shelf,
        faces: &mut Vec<(NavKey, NavFace)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let thread = row.thread;
        let wall = ferrite_core::clock::system_time();
        let (time, action) = match shelf {
            Shelf::Snoozed => {
                let until = self
                    .cockpit
                    .thread(thread)
                    .and_then(|open| open.snooze())
                    .map_or(wall, |snooze| snooze.until);
                let wake = list::wake_button(thread)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            view.snooze_thread(thread, None, cx);
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
                        cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            view.revive_thread(thread, cx);
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
        let slim = list::slim_row(row, time, action, &nav::Measure::Text(cx))
            .children(self.cursor_reveal(NavItem::Thread(thread)))
            .children(self.shown_reveal(row))
            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                view.snooze_editor = None;
                view.open_nav_row(thread, cx);
                window.blur(cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.pick_for_menu(thread, cx);
                    view.open_context_menu(MenuTarget::Thread(thread), event.position, cx);
                }),
            );
        flip::flip_row(&self.nav_motion, NavKey::Slim(thread), slim).into_any_element()
    }
}
