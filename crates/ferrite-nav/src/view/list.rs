//! The nav's one list, wired (ADR 0040): every machine's Threads in it
//! (ADR 0045) — Pinned and Active cards from the top, under the notice of
//! any machine the window cannot reach, then the Working, Snoozed and
//! Parked shelves anchored over the foot. Each row rides the list's
//! motion (`crate::flip`), keyed by its Thread and its kind, so a card that
//! moves from Active to Working slides there while one that parks leaves
//! as a ghost and enters the Parked shelf as a slim row.

use std::time::Instant;

use ferrite_core::ThreadKey;
use ferrite_pane::link_target::Open;
use ferrite_ui::motion::{fold_body, Reveal, FOLD};
use gpui::prelude::*;
use gpui::{
    div, px, AnyElement, ClickEvent, Context, Div, MouseButton, MouseDownEvent, SharedString,
    Stateful, Window,
};

use super::{MenuAt, NavEvent, NavInputs, NavMenu, NavView};
use crate::flip::{self, Update};
use crate::notifications::Verb;
use crate::{list, NavItem, Shelf, SHELVES};

/// A row of the list as its motion keys it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NavKey {
    Card(ThreadKey),
    Slim(ThreadKey),
}

/// What a row last looked like, for its ghost.
#[derive(Clone)]
pub enum NavFace {
    Card(crate::ThreadRow),
    Slim(crate::ThreadRow, list::SlimTime),
}

/// The list's motion, shared with its elements.
pub type NavMotion = flip::Motion<NavKey, NavFace>;

impl NavView {
    pub(super) fn content(&self, inputs: &NavInputs, window: &Window, cx: &mut Context<Self>) -> Div {
        #[cfg(any(test, feature = "test-support"))]
        crate::testing::built_nav();
        self.drawn.borrow_mut().clear();
        let list = if self.searching(cx) {
            self.search_results(&inputs.state, cx)
        } else {
            self.list(inputs, window, cx)
        };
        let list = self.sweep_release(list, cx);
        crate::content().w(px(inputs.width))
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
                    .children(crate::edge_fades(&self.scroll))
                    .child(crate::scrollbar(&self.scroll, self.hovered)),
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
    pub fn shelf_rows<'a>(
        &self,
        inputs: &'a NavInputs,
        shelf: Shelf,
    ) -> Vec<&'a crate::ThreadRow> {
        self.rows_of(&inputs.state, shelf, inputs.shelf_open(shelf))
    }

    /// [`Self::shelf_rows`] for a shelf `open` or not.
    fn rows_of<'a>(
        &self,
        state: &'a crate::NavState,
        shelf: Shelf,
        open: bool,
    ) -> Vec<&'a crate::ThreadRow> {
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
                .map(|error| crate::notice(error.clone()).into_any_element()),
        );
        rows.extend(inputs.machines.iter().map(|notice| {
            let machine = notice.machine;
            let sign_in = notice.sign_in;
            crate::machine_notice(machine.get(), notice.words.clone(), cx)
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    cx.emit(if sign_in {
                        NavEvent::SignIn(machine)
                    } else {
                        NavEvent::Reconnect(machine)
                    });
                }))
                .into_any_element()
        }));
        for row in &state.pinned {
            rows.push(self.card(inputs, row, None, &mut faces, cx));
        }
        // The Inbox's order: a request's card opens at its request.
        for notice in &inputs.inbox {
            if let Some(row) = state.active.iter().find(|row| row.key == notice.thread) {
                rows.push(self.card(inputs, row, Some(notice.open_verb()), &mut faces, cx));
            }
        }
        if state.pinned.is_empty() && state.active.is_empty() {
            rows.push(self.empty(inputs, cx));
        }
        let now = cx.background_executor().now();
        let reduced = ferrite_ui::motion::reduced_motion(cx);
        let mut folding = false;
        let mut anchored = false;
        for shelf in SHELVES {
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
        let measure = crate::Measure::Text(cx, inputs.width);
        let ghosts = ghosts.into_iter().map(|ghost| {
            let (top, opacity) = ghost.at(now);
            let face = match &ghost.face {
                NavFace::Card(row) => crate::ghost_card(row, &measure),
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
        self.keys(crate::list_scroll(&self.scroll), cx)
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
                        cx.emit(NavEvent::Menu(NavMenu::Parked, MenuAt::Point(event.position)));
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
        row: &crate::ThreadRow,
        open: Option<Verb>,
        faces: &mut Vec<(NavKey, NavFace)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let thread = row.key;
        faces.push((NavKey::Card(thread), NavFace::Card(row.clone())));
        let title = self.thread_title(inputs, row, true, cx);
        let card = crate::thread_card(
            row,
            title,
            self.card_doors(inputs, row, cx),
            renaming(inputs, thread),
            &crate::Measure::Text(cx, inputs.width),
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
                    cx.emit(NavEvent::Menu(NavMenu::Thread(thread), MenuAt::Point(event.position)));
                }),
            );
        flip::flip_row(&self.motion, NavKey::Card(thread), card).into_any_element()
    }

    /// What a Thread's card wires (`crate::CardDoors`): its hover strip —
    /// Snooze, whose menu opens under it (hidden while a turn is queued),
    /// and Park — for an open Thread; the pin's unpin; and the PR chip,
    /// whose press opens the PR in the Panel's preview (⌘-press, in the
    /// browser). Pin stays in the context menu.
    fn card_doors(&self, inputs: &NavInputs, row: &crate::ThreadRow, cx: &mut Context<Self>) -> crate::CardDoors {
        let thread = row.key;
        let id = row.tag();
        let held = inputs.menu == Some(NavMenu::Snooze(thread));
        let strip = inputs.open.get(&thread).map(|open| {
            let mut buttons = Vec::new();
            if !open.queued {
                buttons.push(
                    self.door(crate::snooze_button(id), NavMenu::Snooze(thread), "Snooze thread", inputs, cx)
                        .into_any_element(),
                );
            }
            buttons.push(
                crate::park_button(id)
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
            crate::strip(buttons, crate::strip_w(!open.queued, &crate::Measure::Text(cx, inputs.width)))
        });
        let pin = crate::pin_glyph(id, true)
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
            crate::pr_chip(id, pr)
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
                        cx.emit(NavEvent::Menu(NavMenu::PrLink(thread), MenuAt::Point(event.position)));
                    }),
                )
                .on_click(|_, _, cx| cx.stop_propagation())
                .into_any_element()
        });
        crate::CardDoors {
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
        row: &crate::ThreadRow,
        hint: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let thread = row.key;
        if let Some((editing, editor)) = &inputs.rename {
            if *editing == thread {
                return ferrite_ui::components::rename_field(editor.clone(), hint)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
                    )
                    .into_any_element();
            }
        }
        crate::rename_target_thread(row.tag(), row.name.clone())
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
    fn hover_card(&self, inputs: &NavInputs, row: &crate::ThreadRow, cx: &gpui::App) -> Option<AnyElement> {
        let snoozing = matches!(inputs.menu, Some(NavMenu::Snooze(_)));
        if snoozing
            || cx.has_active_drag()
            || ferrite_ui::motion::hover_t(&crate::card_key(row.tag())) < 1.0
        {
            return None;
        }
        // T3's row tooltip: another machine's Thread names its machine
        // first, after its Project.
        let mut lines: Vec<_> = row
            .elsewhere
            .iter()
            .map(|elsewhere| (ferrite_ui::icons::SERVER, elsewhere.machine.clone()))
            .collect();
        if let Some(branch) = &row.branch {
            lines.push((ferrite_ui::icons::BRANCH, branch.clone()));
        }
        if let Some(worktree) = &row.worktree {
            lines.push((ferrite_ui::icons::FOLDER, worktree.clone()));
        }
        let provider = row.provider.map(|provider| (provider, self.row_model(row, provider, cx)));
        Some(crate::chrome::hover_card(
            row,
            crate::chrome::HoverLines { lines, provider },
            inputs.width,
        ))
    }

    /// A Snoozed or Parked shelf's slim row, wired: a press opens (a
    /// parked one revives), its action wakes or revives it, a right press
    /// opens its menu.
    fn slim(
        &self,
        inputs: &NavInputs,
        row: &crate::ThreadRow,
        shelf: Shelf,
        faces: &mut Vec<(NavKey, NavFace)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let thread = row.key;
        let id = row.tag();
        let wall = ferrite_core::clock::system_time();
        let (time, action) = match shelf {
            Shelf::Snoozed => {
                let until = inputs
                    .open
                    .get(&thread)
                    .and_then(|open| open.snooze_until)
                    .unwrap_or(wall);
                let wake = list::wake_button(id)
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
                let revive = list::revive_button(id)
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
                    .map(|at| crate::card_age(at, wall))
                    .unwrap_or_default();
                (list::SlimTime::Parked(age), revive.into_any_element())
            }
        };
        faces.push((NavKey::Slim(thread), NavFace::Slim(row.clone(), time.clone())));
        let title = if renaming(inputs, thread) {
            list::SlimTitle::Editor(self.thread_title(inputs, row, false, cx))
        } else {
            list::SlimTitle::Row
        };
        let slim = list::slim_row(row, title, time, action, &crate::Measure::Text(cx, inputs.width))
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
                    cx.emit(NavEvent::Menu(NavMenu::Thread(thread), MenuAt::Point(event.position)));
                }),
            );
        flip::flip_row(&self.motion, NavKey::Slim(thread), slim).into_any_element()
    }
}

/// Whether the rename field is open on `thread`'s title.
fn renaming(inputs: &NavInputs, thread: ThreadKey) -> bool {
    inputs.rename.as_ref().is_some_and(|(editing, _)| *editing == thread)
}
