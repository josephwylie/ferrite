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
        if self.columns.nav_folded() {
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

impl CockpitView {
    pub(super) fn next_thread(&mut self, _: &NextThread, window: &mut Window, cx: &mut Context<Self>) {
        self.step_thread(1, window, cx);
    }

    pub(super) fn previous_thread(&mut self, _: &PreviousThread, window: &mut Window, cx: &mut Context<Self>) {
        self.step_thread(-1, window, cx);
    }
}

impl CockpitView {
    /// Reopen the Thread parked most recently — the one the operator just
    /// closed, which is the one they want back (#17); the core walks the
    /// park order, then creation order.
    pub(super) fn reopen_thread(&mut self, _: &ReopenThread, window: &mut Window, cx: &mut Context<Self>) {
        let Some((thread, asked)) = self.cockpit.update(cx, |cockpit, _| cockpit.reopen_last()) else {
            return;
        };
        // What `switch_thread` reads before the switch, read at the press:
        // the Thread is back only once its Server answers.
        let shown = self.focused_thread(cx);
        let on_stop = self.panel_control_focused(window, cx);
        self.when_answered_in(
            asked,
            window,
            move |view, answer, window, cx| {
                let outcome = nav_undo::Outcome {
                    thread,
                    result: answer.map_err(|e| e.to_string()),
                };
                if outcome.result.is_ok() {
                    view.widen_scope_to(thread, cx);
                    view.sync_panes(cx);
                }
                view.report_failures("reopen", &[outcome]);
                if on_stop && view.focused_thread(cx) != shown {
                    window.blur(cx);
                }
                cx.notify();
            },
            cx,
        );
    }

    /// Close the focused Pane — cmd-w. The core decides what that means
    /// (park, leave, defer onto a draft, discard); the mirror follows.
    pub(super) fn close_thread(&mut self, _: &CloseThread, window: &mut Window, cx: &mut Context<Self>) {
        if self.nav_view.read(cx).holds_keys(window) {
            self.park_nav_cursor(cx);
            return;
        }
        // ⌘W over the lightbox closes it; in the focused Pane's preview or
        // Review it closes that, not the Pane (F-14).
        if let Some((index, _)) = self.lightbox(cx) {
            self.close_lightbox(index, cx);
            return;
        }
        let index = self.focused(cx);
        if let Some(pane) = self.panes.get(index) {
            if self.preview_shown(index, cx) && pane.read(cx).preview.focused(window, cx) {
                self.close_preview(index, cx);
                return;
            }
            if pane.review.focused(window, cx) {
                self.close_review(index, cx);
                return;
            }
        }
        if self.close_panel_in_focus(window, cx) {
            return;
        }
        let Some(identity) = self.cockpit.read(cx).roster().focused() else {
            return;
        };
        self.close_pane(identity, cx);
    }

    /// Close one Pane through the core act (a Thread always parks), its
    /// refusal landing in the nav's notice — and a park that would not
    /// flush is logged, the Pane gone either way.
    pub(super) fn close_pane(&mut self, identity: PaneIdentity, cx: &mut Context<Self>) {
        let closing = self.cockpit.update(cx, |cockpit, _| cockpit.close(identity));
        // A menu on the closed Pane goes with it.
        if self.composer_menu.read(cx).pane() == Some(identity) {
            self.close_composer_menu(cx);
        }
        self.when_answered(
            closing,
            move |view, closed, cx| {
                if let Err(error) = &closed {
                    ferrite_core::log!("pane {identity:?} did not close cleanly: {error}");
                }
                view.sync_panes(cx);
                if let (true, PaneIdentity::Draft(draft)) = (closed.is_ok(), identity) {
                    view.discard_draft(draft, cx);
                }
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }
}
