//! The nav list's keyboard: ⌘I (on the first card) or ⌘⇧E (on the shown
//! Thread's) hands the list the keys. ↑↓ walk the rows the list draws, in
//! its order — Pinned and Active cards, then each shelf's header and the
//! rows it draws — and Home/End go to its ends. ⏎ opens what the row
//! stands for (a header's folds or opens its shelf), ← folds the shelf
//! the cursor heads or sits in (from a row, onto its header), → opens it.
//! F2 renames, the ContextMenu key or ⇧F10 opens the row's menu, ⌘W parks
//! the cursor's Thread and ⌘Z takes back the last Park or Snooze. Esc
//! gives the keys back to the board.
//!
//! Only those chords hand the list the keys: a press in the list never
//! takes them (a card's press opens its Thread, whose Composer keeps
//! typing), and a press while the list holds them hands them back first.
//!
//! One cursor, the pointer's: the row under the keyboard wears the hover
//! face, driven through the same blend a pointer drives. It is not the
//! selection, which stays what the board shows, and it clears whenever the
//! list lets go of the keyboard. It stands on a Thread, not a place, so a
//! Thread that changes section (starts working, snoozes, parks) carries it
//! along. A move scrolls the row into view (`cursor_reveal`), and so does
//! the shown Thread changing (`shown_reveal`).
//!
//! The list's order is also the window's: ⌘{ / ⌘} step the screen to the
//! previous or next open Thread it lists, and ⌘1…⌘9 go to one by place
//! (`thread_order`), whoever holds the keys.

use super::nav_view::{MenuAt, NavEvent, NavInputs, NavView};
use super::*;
use crate::nav::{NavCursor, NavItem, Shelf, SHELVES};
use gpui::{App, Bounds};

impl NavView {
    /// Every row the keyboard can stand on, in the list's drawing order:
    /// the cards, every machine's, then each shelf's header and the rows
    /// it draws.
    pub(super) fn items(&self, inputs: &NavInputs) -> Vec<NavCursor> {
        let state = &inputs.state;
        let mut items = Vec::new();
        items.extend(state.pinned.iter().chain(&state.active).map(NavCursor::thread));
        for shelf in SHELVES {
            if state.shelf(shelf).is_empty() {
                continue;
            }
            items.push(NavCursor::shelf(shelf));
            items.extend(
                self.shelf_rows(inputs, shelf)
                    .into_iter()
                    .map(NavCursor::thread),
            );
        }
        items
    }

    /// The first card the keyboard can stand on, else the first row.
    pub(super) fn first_card(&self, inputs: &NavInputs) -> Option<NavCursor> {
        let items = self.items(inputs);
        items
            .iter()
            .find(|cursor| matches!(cursor.item, NavItem::Thread(_)))
            .or(items.first())
            .cloned()
    }

    /// The list takes the keyboard, its cursor on `start`.
    pub(super) fn take_keys(
        &mut self,
        start: Option<NavCursor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.keys_held = true;
        window.focus(&self.focus, cx);
        // However the keys leave (Esc, a press on the board, ⌘K), the
        // cursor goes with them.
        self.blur = Some(cx.on_blur(&self.focus, window, |nav, window, cx| {
            nav.release(window, cx)
        }));
        self.move_cursor(start, window, cx);
    }

    /// The list lets go of the keyboard: its cursor clears, and the
    /// Cockpit's focus rule hands the keys to the focused Pane.
    fn release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.keys_held = false;
        self.move_cursor(None, window, cx);
    }

    /// Esc, ⏎ on a row, or a press in the list while it holds the keys:
    /// they go back to the board.
    pub(super) fn leave(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.release(window, cx);
        if self.focus.is_focused(window) {
            window.blur(cx);
        }
        cx.notify();
    }

    /// Put the cursor on `next`: its row takes the hover face and the old
    /// one gives it back, and the row scrolls into view.
    fn move_cursor(&mut self, next: Option<NavCursor>, window: &mut Window, cx: &mut Context<Self>) {
        let painter = Some(cx.entity_id());
        if let Some(old) = self.cursor.take() {
            crate::nav::card_hover(old.key, true, painter)(&false, window, cx);
        }
        self.cursor_bounds.set(None);
        if let Some(next) = &next {
            crate::nav::card_hover(next.key.clone(), true, painter)(&true, window, cx);
            self.reveal.set(true);
        }
        self.cursor = next;
        cx.notify();
    }

    /// The rows the keyboard walks now.
    fn items_now(&self) -> Vec<NavCursor> {
        self.inputs()
            .map(|inputs| self.items(inputs))
            .unwrap_or_default()
    }

    /// ↑ / ↓: the row before or after the cursor's among `items`,
    /// stopping at the ends. A cursor whose row has gone (folded away)
    /// starts over from the top.
    pub(super) fn step(
        &mut self,
        by: isize,
        items: &[NavCursor],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at = self
            .cursor
            .as_ref()
            .and_then(|cursor| items.iter().position(|item| item.item == cursor.item));
        let next = match at {
            Some(at) => at
                .saturating_add_signed(by)
                .min(items.len().saturating_sub(1)),
            None => 0,
        };
        let next = items.get(next).cloned();
        self.move_cursor(next, window, cx);
    }

    /// Home / End: the list's first or last row.
    fn to_end(&mut self, last: bool, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.items_now();
        let next = if last { items.last() } else { items.first() }.cloned();
        self.move_cursor(next, window, cx);
    }

    /// ⏎: what a press on the row does — a Thread's opens it, a header's
    /// folds or opens its shelf.
    fn open_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.cursor.as_ref().map(|cursor| cursor.item) else {
            return;
        };
        match item {
            NavItem::Shelf(shelf) => {
                let open = self.inputs().is_some_and(|inputs| inputs.shelf_open(shelf));
                cx.emit(NavEvent::SetShelfOpen(shelf, !open));
            }
            NavItem::Thread(thread) => {
                cx.emit(NavEvent::Open(thread));
                self.leave(window, cx);
            }
        }
    }

    /// F2: the cursor's card renames its Thread in place; the field takes
    /// the keys, so the list lets go of them.
    fn rename_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(NavItem::Thread(thread)) = self.cursor.as_ref().map(|cursor| cursor.item) else {
            return;
        };
        self.leave(window, cx);
        cx.emit(NavEvent::Rename(thread));
    }

    /// ← (`fold`) or →: the shelf the cursor heads, or (←) the one its row
    /// sits in, the cursor moving onto that header first so it never
    /// stands on a row the fold took away.
    fn fold_cursor(&mut self, fold: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.cursor.as_ref().map(|cursor| cursor.item) else {
            return;
        };
        let shelf = match item {
            NavItem::Shelf(shelf) => shelf,
            NavItem::Thread(thread) if fold => {
                let Some(inputs) = self.inputs() else {
                    return;
                };
                let Some(shelf) = SHELVES.into_iter().find(|shelf| {
                    inputs.state.shelf(*shelf).iter().any(|row| row.key == thread)
                }) else {
                    return;
                };
                self.move_cursor(Some(NavCursor::shelf(shelf)), window, cx);
                shelf
            }
            NavItem::Thread(_) => return,
        };
        cx.emit(NavEvent::SetShelfOpen(shelf, !fold));
    }

    /// The ContextMenu key or ⇧F10: the cursor's row's menu, under it — a
    /// Thread's, or the Parked header's.
    fn open_cursor_menu(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.cursor.as_ref().map(|cursor| cursor.item) else {
            return;
        };
        let target = match item {
            NavItem::Thread(thread) => {
                self.pick_for_menu(thread, cx);
                MenuTarget::Thread(thread)
            }
            NavItem::Shelf(Shelf::Parked) => MenuTarget::Parked,
            NavItem::Shelf(_) => return,
        };
        let at = match self.cursor_bounds.get() {
            Some(row) => MenuAt::Under(row),
            None => MenuAt::Point(gpui::Point::default()),
        };
        cx.emit(NavEvent::Menu(target, at));
    }

    /// The list's keys, on the list while it holds the keyboard.
    pub(super) fn keys(&self, list: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        list.track_focus(&self.focus)
            .key_context("NavTree")
            // A press never takes the keys (it runs before the list's own
            // focus-on-press, which it prevents, and so does the root's):
            // the Composer keeps typing. While the list holds them, a press
            // hands them back first, as a press on the board would.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|nav, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    if nav.keys_held {
                        nav.leave(window, cx);
                    }
                }),
            )
            .on_action(cx.listener(|nav, _: &nav::SelectNext, window, cx| {
                let items = nav.items_now();
                nav.step(1, &items, window, cx)
            }))
            .on_action(cx.listener(|nav, _: &nav::SelectPrevious, window, cx| {
                let items = nav.items_now();
                nav.step(-1, &items, window, cx)
            }))
            .on_action(
                cx.listener(|nav, _: &nav::SelectFirst, window, cx| nav.to_end(false, window, cx)),
            )
            .on_action(
                cx.listener(|nav, _: &nav::SelectLast, window, cx| nav.to_end(true, window, cx)),
            )
            .on_action(cx.listener(|nav, _: &nav::Open, window, cx| nav.open_cursor(window, cx)))
            .on_action(
                cx.listener(|nav, _: &nav::Fold, window, cx| nav.fold_cursor(true, window, cx)),
            )
            .on_action(
                cx.listener(|nav, _: &nav::Unfold, window, cx| nav.fold_cursor(false, window, cx)),
            )
            .on_action(cx.listener(|nav, _: &nav::CloseView, window, cx| nav.leave(window, cx)))
            .on_action(cx.listener(|nav, _: &nav::Rename, window, cx| nav.rename_cursor(window, cx)))
            .on_action(cx.listener(|nav, _: &nav::OpenMenu, _, cx| nav.open_cursor_menu(cx)))
            .on_action(cx.listener(|nav, _: &nav::Undo, _, cx| nav.undo_nav(cx)))
    }

    /// On the cursor's row: where it is drawn (for ⇧F10's menu), and once
    /// after a move, a scroll that shows it whole.
    pub(super) fn cursor_reveal(&self, item: NavItem) -> Option<impl IntoElement> {
        let reveal = self.reveal.clone();
        let drawn = self.cursor_bounds.clone();
        let scroll = self.scroll.clone();
        (self.cursor.as_ref().map(|cursor| cursor.item) == Some(item)).then(move || {
            gpui::canvas(
                move |row: Bounds<Pixels>, window, cx| {
                    drawn.set(Some(row));
                    if reveal.replace(false) {
                        scroll_into_view(&scroll, row, window, cx);
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0()
        })
    }

    /// On the highlighted search result: once after the highlight moves, a
    /// scroll that shows it whole. The keyboard stays in the field.
    pub(super) fn search_reveal(&self) -> impl IntoElement {
        let reveal = self.reveal.clone();
        let scroll = self.scroll.clone();
        gpui::canvas(
            move |row: Bounds<Pixels>, window, cx| {
                if reveal.replace(false) {
                    scroll_into_view(&scroll, row, window, cx);
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    }

    /// On the shown Thread's row: once each time the shown Thread changes
    /// (a focus move from the board, the palette, a toast), a scroll that
    /// shows it whole (#231).
    pub(super) fn shown_reveal(&self, row: &nav::ThreadRow) -> Option<impl IntoElement> {
        let thread = row.key;
        let shown = self.shown.clone();
        let scroll = self.scroll.clone();
        (row.selected && shown.get() != Some(thread)).then(move || {
            gpui::canvas(
                move |row: Bounds<Pixels>, window, cx| {
                    if shown.replace(Some(thread)) != Some(thread) {
                        scroll_into_view(&scroll, row, window, cx);
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0()
        })
    }
}

impl CockpitView {
    /// ⌘⇧E: the list takes the keyboard, its cursor on the row the board
    /// shows (else the first card).
    pub(super) fn focus_nav(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shown = self
            .focused_thread(cx)
            ;
        let nav = self.nav_view.read(cx);
        let start = nav.inputs().and_then(|inputs| {
            nav.items(inputs)
                .into_iter()
                .find(|cursor| matches!(cursor.item, NavItem::Thread(thread) if Some(thread) == shown))
                .or_else(|| nav.first_card(inputs))
        });
        self.focus_nav_on(start, window, cx);
    }

    pub(super) fn focus_nav_on(
        &mut self,
        start: Option<NavCursor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.nav_folded() {
            self.toggle_nav_now(cx);
        }
        self.nav_view.update(cx, |nav, cx| nav.close_snooze(cx));
        self.nav_view
            .update(cx, |nav, cx| nav.take_keys(start, window, cx));
    }

    /// The nav's first card, as ⌘I's cursor starts on it.
    pub(super) fn first_nav_card(&self, cx: &App) -> Option<NavCursor> {
        let nav = self.nav_view.read(cx);
        nav.first_card(nav.inputs()?)
    }

    /// ⌘W while the list holds the keys: the cursor's Thread parks, and
    /// the cursor stays on it, now in the Parked shelf.
    pub(super) fn park_nav_cursor(&mut self, cx: &mut Context<Self>) {
        let cursor = self.nav_view.read(cx).cursor.as_ref().map(|cursor| cursor.item);
        if let Some(NavItem::Thread(key)) = cursor {
            if self.cockpit.read(cx).thread(key).is_some() {
                self.park_listed(&[key], cx);
            }
        }
    }

    /// The Threads ⌘{ / ⌘} step through and ⌘1…⌘9 count: the open ones
    /// the sidebar draws, in its order. A parked row is not one of them: a
    /// key that walks must not spend a Session reviving a Thread.
    pub(super) fn thread_order(&self, cx: &App) -> Vec<ThreadKey> {
        let nav = self.nav_view.read(cx);
        nav.inputs()
            .map(|inputs| nav.items(inputs))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|cursor| match cursor.item {
                NavItem::Thread(key) => Some(key),
                NavItem::Shelf(_) => None,
            })
            .filter(|thread| self.pane_for(*thread).is_some())
            .collect()
    }

    /// ⌘} (`delta` 1) and ⌘{ (-1): show the next or the previous Thread in
    /// the sidebar's order, wrapping at its ends. Off the list (a draft, the
    /// empty board), ⌘} starts at the first and ⌘{ at the last.
    pub(super) fn step_thread(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let order = self.thread_order(cx);
        let len = order.len() as isize;
        if len == 0 {
            return;
        }
        let at = self
            .focused_thread(cx)
            .and_then(|shown| order.iter().position(|thread| *thread == shown));
        let next = match at {
            Some(at) => (at as isize + delta).rem_euclid(len),
            None if delta > 0 => 0,
            None => len - 1,
        };
        let thread = order[next as usize];
        self.switch_thread(window, cx, |view, _, cx| view.focus_thread(thread, cx));
    }

    /// ⌘1…⌘9: show the sidebar's `ordinal`th Thread (`thread_order`). A
    /// digit past the list does nothing.
    pub(super) fn go_to_thread(&mut self, ordinal: usize, window: &mut Window, cx: &mut Context<Self>) {
        let thread = ordinal
            .checked_sub(1)
            .and_then(|at| self.thread_order(cx).get(at).copied());
        if let Some(thread) = thread {
            self.switch_thread(window, cx, |view, _, cx| view.focus_thread(thread, cx));
        }
    }

    /// A Thread row's press, and ⏎ on it: an open Thread takes the screen;
    /// a parked one revives.
    pub(super) fn open_nav_row(&mut self, key: ThreadKey, cx: &mut Context<Self>) {
        if self.pane_for(key).is_some() {
            self.focus_thread(key, cx);
        } else {
            self.revive_thread(key, cx);
        }
    }
}

/// Scroll the list so `row` shows whole with its margin, by as little as it
/// takes (none when it already does): the first row brought back leaves the
/// list at its very top, with nothing above it to fade. Called while the
/// list is drawn, too late for this draw (a refresh mid-draw is dropped),
/// so the view drawing the list draws again once this one is done.
fn scroll_into_view(
    scroll: &ScrollHandle,
    row: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    let view = scroll.bounds();
    let mut offset = scroll.offset();
    let margin = px(ferrite_ui::theme::NAV_CARD_ITEM_PAD_Y);
    let (top, bottom) = (row.top() - margin, row.bottom() + margin);
    if top < view.top() {
        offset.y += view.top() - top;
    } else if bottom > view.bottom() {
        offset.y -= bottom - view.bottom();
    } else {
        return;
    }
    scroll.set_offset(offset);
    let view = window.current_view();
    window.defer(cx, move |_, cx| cx.notify(view));
}
