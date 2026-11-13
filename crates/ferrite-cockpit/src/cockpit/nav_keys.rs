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

use super::*;
use crate::nav::{NavCursor, NavItem, Shelf, SHELVES};
use gpui::{App, Bounds};

impl CockpitView {
    /// Every row the keyboard can stand on, in the list's drawing order:
    /// the local cards (a connected machine's rows are the pointer's),
    /// then each shelf's header and the rows it draws.
    pub(super) fn nav_items(&self, cx: &App) -> Vec<NavCursor> {
        let state = self.nav_state();
        let mut items = Vec::new();
        let machine_folded = self
            .machine_rows(cx)
            .and_then(|rows| rows.into_iter().find(|row| row.selected))
            .is_some_and(|on| on.folded);
        if machine_folded {
            return items;
        }
        items.extend(
            state
                .pinned
                .iter()
                .chain(&state.active)
                .map(|row| NavCursor::thread(row.thread)),
        );
        for shelf in SHELVES {
            if state.shelf(shelf).is_empty() {
                continue;
            }
            items.push(NavCursor::shelf(shelf));
            items.extend(
                self.shelf_rows(&state, shelf)
                    .into_iter()
                    .map(|row| NavCursor::thread(row.thread)),
            );
        }
        items
    }

    /// The first card the keyboard can stand on, else the first row.
    pub(super) fn first_nav_card(&self, cx: &App) -> Option<NavCursor> {
        let items = self.nav_items(cx);
        items
            .iter()
            .find(|cursor| matches!(cursor.item, NavItem::Thread(_)))
            .or(items.first())
            .cloned()
    }

    /// ⌘⇧E: the list takes the keyboard, its cursor on the row the board
    /// shows (else the first card).
    pub(super) fn focus_nav(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shown = self.focused_thread();
        let start = self
            .nav_items(cx)
            .into_iter()
            .find(|cursor| matches!(cursor.item, NavItem::Thread(thread) if Some(thread) == shown))
            .or_else(|| self.first_nav_card(cx));
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
        self.snooze_editor = None;
        self.nav_keys_held = true;
        window.focus(&self.nav_focus, cx);
        // However the keys leave (Esc, a press on the board, ⌘K), the
        // cursor goes with them.
        self.nav_blur = Some(cx.on_blur(&self.nav_focus, window, |view, window, cx| {
            view.release_nav(window, cx)
        }));
        self.move_nav_cursor(start, window, cx);
    }

    /// The list lets go of the keyboard: its cursor clears, and render's
    /// focus rule hands the keys to the focused Pane.
    fn release_nav(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.nav_keys_held = false;
        self.move_nav_cursor(None, window, cx);
    }

    /// Esc, ⏎ on a row, or a press in the list while it holds the keys:
    /// they go back to the board.
    pub(super) fn leave_nav(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.release_nav(window, cx);
        if self.nav_focus.is_focused(window) {
            window.blur(cx);
        }
        cx.notify();
    }

    /// Put the cursor on `next`: its row takes the hover face and the old
    /// one gives it back, and the row scrolls into view.
    fn move_nav_cursor(
        &mut self,
        next: Option<NavCursor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(old) = self.nav_cursor.take() {
            crate::nav::card_hover(old.key, true)(&false, window, cx);
        }
        self.nav_cursor_bounds.set(None);
        if let Some(next) = &next {
            crate::nav::card_hover(next.key.clone(), true)(&true, window, cx);
            self.nav_reveal.set(true);
        }
        self.nav_cursor = next;
        cx.notify();
    }

    /// ↑ / ↓: the row before or after the cursor's, stopping at the ends.
    /// A cursor whose row has gone (folded away) starts over from the top.
    pub(super) fn step_nav(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.nav_items(cx);
        let at = self
            .nav_cursor
            .as_ref()
            .and_then(|cursor| items.iter().position(|item| item.item == cursor.item));
        let next = match at {
            Some(at) => at
                .saturating_add_signed(by)
                .min(items.len().saturating_sub(1)),
            None => 0,
        };
        let next = items.get(next).cloned();
        self.move_nav_cursor(next, window, cx);
    }

    /// Home / End: the list's first or last row.
    fn nav_to_end(&mut self, last: bool, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.nav_items(cx);
        let next = if last { items.last() } else { items.first() }.cloned();
        self.move_nav_cursor(next, window, cx);
    }

    /// ⏎: what a press on the row does — a Thread's opens it, a header's
    /// folds or opens its shelf.
    fn open_nav_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.nav_cursor.as_ref().map(|cursor| cursor.item) else {
            return;
        };
        match item {
            NavItem::Shelf(shelf) => {
                let open = self.shelf_open(shelf);
                self.set_shelf_open(shelf, !open, cx);
            }
            NavItem::Thread(thread) => {
                self.open_nav_row(thread, cx);
                self.leave_nav(window, cx);
            }
        }
    }

    /// F2: the cursor's card renames its Thread in place; the field takes
    /// the keys, so the list lets go of them.
    fn rename_nav_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(NavItem::Thread(thread)) = self.nav_cursor.as_ref().map(|cursor| cursor.item)
        else {
            return;
        };
        self.leave_nav(window, cx);
        self.start_rename(RenameTarget::Thread(thread), cx);
    }

    /// ← (`fold`) or →: the shelf the cursor heads, or (←) the one its row
    /// sits in, the cursor moving onto that header first so it never
    /// stands on a row the fold took away.
    fn fold_nav_cursor(&mut self, fold: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.nav_cursor.as_ref().map(|cursor| cursor.item) else {
            return;
        };
        let shelf = match item {
            NavItem::Shelf(shelf) => shelf,
            NavItem::Thread(thread) if fold => {
                let state = self.nav_state();
                let Some(shelf) = SHELVES
                    .into_iter()
                    .find(|shelf| state.shelf(*shelf).iter().any(|row| row.thread == thread))
                else {
                    return;
                };
                self.move_nav_cursor(Some(NavCursor::shelf(shelf)), window, cx);
                shelf
            }
            NavItem::Thread(_) => return,
        };
        self.set_shelf_open(shelf, !fold, cx);
    }

    /// The ContextMenu key or ⇧F10: the cursor's row's menu, under it — a
    /// Thread's, or the Parked header's.
    fn open_nav_cursor_menu(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.nav_cursor.as_ref().map(|cursor| cursor.item) else {
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
        match self.nav_cursor_bounds.get() {
            Some(row) => self.open_context_menu_under(target, row, cx),
            None => self.open_context_menu(target, gpui::Point::default(), cx),
        }
    }

    /// ⌘W while the list holds the keys: the cursor's Thread parks, and
    /// the cursor stays on it, now in the Parked shelf.
    pub(super) fn park_nav_cursor(&mut self, cx: &mut Context<Self>) {
        if let Some(NavItem::Thread(thread)) = self.nav_cursor.as_ref().map(|cursor| cursor.item) {
            if self.cockpit.thread(thread).is_some() {
                self.park_listed(&[thread], cx);
            }
        }
    }

    /// The list's keys, on the list while it holds the keyboard.
    pub(super) fn nav_keys(&self, list: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        list.track_focus(&self.nav_focus)
            .key_context("NavTree")
            // A press never takes the keys (it runs before the list's own
            // focus-on-press, which it prevents, and so does the root's):
            // the Composer keeps typing. While the list holds them, a press
            // hands them back first, as a press on the board would.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    if view.nav_keys_held {
                        view.leave_nav(window, cx);
                    }
                }),
            )
            .on_action(
                cx.listener(|view, _: &nav::SelectNext, window, cx| view.step_nav(1, window, cx)),
            )
            .on_action(cx.listener(|view, _: &nav::SelectPrevious, window, cx| {
                view.step_nav(-1, window, cx)
            }))
            .on_action(cx.listener(|view, _: &nav::SelectFirst, window, cx| {
                view.nav_to_end(false, window, cx)
            }))
            .on_action(cx.listener(|view, _: &nav::SelectLast, window, cx| {
                view.nav_to_end(true, window, cx)
            }))
            .on_action(
                cx.listener(|view, _: &nav::Open, window, cx| view.open_nav_cursor(window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &nav::Fold, window, cx| {
                    view.fold_nav_cursor(true, window, cx)
                }),
            )
            .on_action(cx.listener(|view, _: &nav::Unfold, window, cx| {
                view.fold_nav_cursor(false, window, cx)
            }))
            .on_action(
                cx.listener(|view, _: &nav::CloseView, window, cx| view.leave_nav(window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &nav::Rename, window, cx| view.rename_nav_cursor(window, cx)),
            )
            .on_action(cx.listener(|view, _: &nav::OpenMenu, _, cx| view.open_nav_cursor_menu(cx)))
            .on_action(cx.listener(|view, _: &nav::Undo, _, cx| view.undo_nav(cx)))
    }

    /// On the cursor's row: where it is drawn (for ⇧F10's menu), and once
    /// after a move, a scroll that shows it whole.
    pub(super) fn cursor_reveal(&self, item: NavItem) -> Option<impl IntoElement> {
        let reveal = self.nav_reveal.clone();
        let drawn = self.nav_cursor_bounds.clone();
        let scroll = self.nav_scroll.clone();
        (self.nav_cursor.as_ref().map(|cursor| cursor.item) == Some(item)).then(move || {
            gpui::canvas(
                move |row: Bounds<Pixels>, window, _| {
                    drawn.set(Some(row));
                    if reveal.replace(false) {
                        scroll_into_view(&scroll, row, window);
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0()
        })
    }

    /// On the shown Thread's row: once each time the shown Thread changes
    /// (a focus move from the board, the palette, a toast), a scroll that
    /// shows it whole (#231).
    pub(super) fn shown_reveal(&self, row: &nav::ThreadRow) -> Option<impl IntoElement> {
        let thread = row.thread;
        let shown = self.nav_shown.clone();
        let scroll = self.nav_scroll.clone();
        (row.selected && shown.get() != Some(thread)).then(move || {
            gpui::canvas(
                move |row: Bounds<Pixels>, window, _| {
                    if shown.replace(Some(thread)) != Some(thread) {
                        scroll_into_view(&scroll, row, window);
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0()
        })
    }

    /// A Thread row's press, and ⏎ on it: an open Thread takes focus (on
    /// the Group on screen when it holds it, else Solo); a parked one
    /// revives.
    pub(super) fn open_nav_row(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if self.pane_for(thread).is_some() {
            self.focus_thread(thread, cx);
        } else {
            self.revive_thread(thread, cx);
        }
    }
}

/// Scroll the list so `row` shows whole, by as little as it takes (none
/// when it already does).
fn scroll_into_view(scroll: &ScrollHandle, row: Bounds<Pixels>, window: &mut Window) {
    let view = scroll.bounds();
    let mut offset = scroll.offset();
    if row.top() < view.top() {
        offset.y += view.top() - row.top();
    } else if row.bottom() > view.bottom() {
        offset.y -= row.bottom() - view.bottom();
    } else {
        return;
    }
    scroll.set_offset(offset);
    window.refresh();
}
