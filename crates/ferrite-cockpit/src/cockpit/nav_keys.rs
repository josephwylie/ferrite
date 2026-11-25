//! The nav list's keyboard, the window's side: ⌘⇧E (on the shown Thread's
//! card) and ⌘I (on the first) hand the list the keys, which
//! `ferrite_nav::view` walks; ⌘W parks the cursor's Thread. The list's
//! order is also the window's: ⌘{ / ⌘} step the screen to the previous or
//! next open Thread it lists, and ⌘1…⌘9 go to one by place
//! (`thread_order`), whoever holds the keys.

use super::*;
use ferrite_nav::{NavCursor, NavItem};
use gpui::App;

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
