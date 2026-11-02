//! The nav tree's keyboard (§5.1.10): ⌘⇧E hands the tree the keys, the
//! way ⌘I hands them to the Inbox. ↑↓ walk the rows the tree draws, in the
//! order it draws them — the Attention list's heads and rows, then each
//! Project's heading, its loose Threads, its Groups and their members —
//! ⏎ opens what the row stands for (a heading's ⏎ folds it), ← folds the
//! fold the row is in or heads and → unfolds it. Esc gives the keys back
//! to the board.
//!
//! One cursor, the pointer's: the row under the keyboard wears the hover
//! face (its wash, its hover action), driven through the same blend a
//! pointer drives, so a row never shows two kinds of "here". It is not the
//! selection, which stays what the board shows. A move scrolls the row
//! into view (`cursor_reveal`).

use super::*;
use crate::nav::{NavCursor, NavItem};
use ferrite_ui::motion;
use gpui::{App, Bounds};

impl CockpitView {
    /// Every row the keyboard can stand on, in the tree's drawing order,
    /// each with the blend key its row's hover face reads. It walks what
    /// `nav_tree` draws, folds and all.
    fn nav_items(&self, cx: &App) -> Vec<NavCursor> {
        let state = self.nav_state();
        let attention = self.attention_list(&state);
        let mut items = Vec::new();
        for block in &attention.sections {
            let fold = NavFold::List(block.section);
            items.push(NavCursor::heading(fold, block.section.key()));
            if !block.folded {
                items.extend(block.rows.iter().map(|row| NavCursor::line(row.thread)));
            }
        }
        let machine_folded = self
            .machine_rows(cx)
            .and_then(|rows| rows.into_iter().find(|row| row.selected))
            .is_some_and(|on| on.folded);
        if machine_folded {
            return items;
        }
        for (index, section) in state.sections.iter().enumerate() {
            match (section.mixed, section.project) {
                (true, _) => items.push(NavCursor::heading(NavFold::Groups, "nav-groups-heading")),
                (false, Some(project)) => items.push(NavCursor::heading(
                    NavFold::Project(project),
                    format!("nav-project-section-{index}"),
                )),
                // `Other` names no Project, and its heading folds nothing.
                (false, None) => {}
            }
            if section.folded {
                continue;
            }
            items.extend(
                section
                    .solos
                    .iter()
                    .map(|row| NavCursor::thread(state.solos[*row].thread, None)),
            );
            for group in section.groups.iter().map(|at| &state.groups[*at]) {
                items.push(NavCursor::group(group.id));
                if !group.folded {
                    items.extend(
                        group
                            .members
                            .iter()
                            .map(|row| NavCursor::thread(row.thread, Some(group.id))),
                    );
                }
            }
        }
        items
    }

    /// ⌘⇧E: the tree takes the keyboard, its cursor on the row the board
    /// shows (else the first). From a view or the rail, the tree comes
    /// back first.
    pub(super) fn focus_nav(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.nav_railed() {
            self.toggle_nav_now(cx);
        }
        self.close_nav_view(cx);
        let items = self.nav_items(cx);
        let shown = self.focused_thread();
        let start = items
            .iter()
            .find(|cursor| match cursor.item {
                NavItem::Thread(thread, _) | NavItem::Line(thread) => Some(thread) == shown,
                _ => false,
            })
            .or(items.first())
            .cloned();
        window.focus(&self.nav_view_focus, cx);
        self.move_nav_cursor(start, window, cx);
    }

    /// Esc: the keys go back to the board (render's focus rule hands them
    /// to the focused Pane once the tree lets go).
    fn leave_nav(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.move_nav_cursor(None, window, cx);
        window.blur(cx);
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
            motion::hover_listener_with(old.key, nav::ROW_HOVER)(&false, window, cx);
        }
        if let Some(next) = &next {
            motion::hover_listener_with(next.key.clone(), nav::ROW_HOVER)(&true, window, cx);
            self.nav_reveal.set(true);
        }
        self.nav_cursor = next;
        cx.notify();
    }

    /// ↑ / ↓: the row before or after the cursor's, stopping at the ends.
    /// A cursor whose row has gone (folded away, parked) starts over from
    /// the top.
    pub(super) fn step_nav(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.nav_items(cx);
        let at = self
            .nav_cursor
            .as_ref()
            .and_then(|cursor| items.iter().position(|item| item.item == cursor.item));
        let next = match at {
            Some(at) => at.saturating_add_signed(by).min(items.len().saturating_sub(1)),
            None => 0,
        };
        let next = items.get(next).cloned();
        self.move_nav_cursor(next, window, cx);
    }

    /// ⏎: what a press on the row does — a Thread row opens its Thread, a
    /// Group row its Group, a heading folds or unfolds.
    fn open_nav_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.nav_cursor.as_ref().map(|cursor| cursor.item) else {
            return;
        };
        match item {
            NavItem::Heading(fold) => {
                let folded = self.nav_folds.contains(&fold);
                self.set_nav_fold(fold, !folded, cx);
            }
            NavItem::Line(thread) => {
                self.open_nav_row(thread, None, true, cx);
                self.leave_nav(window, cx);
            }
            NavItem::Thread(thread, group) => {
                self.open_nav_row(thread, group, false, cx);
                self.leave_nav(window, cx);
            }
            NavItem::Group(group) => {
                self.enter_group(group, cx);
                self.leave_nav(window, cx);
            }
        }
    }

    /// ← (`fold`) or →: the fold the cursor's row heads, or for a member
    /// row its Group's. Folding from a member moves the cursor to its
    /// Group, so it never stands on a row the fold took away.
    fn fold_nav_cursor(&mut self, fold: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.nav_cursor.as_ref().map(|cursor| cursor.item) else {
            return;
        };
        let target = match item {
            NavItem::Heading(target) => target,
            NavItem::Group(group) => NavFold::Group(group),
            NavItem::Thread(_, Some(group)) if fold => {
                self.move_nav_cursor(Some(NavCursor::group(group)), window, cx);
                NavFold::Group(group)
            }
            NavItem::Thread(..) | NavItem::Line(_) => return,
        };
        self.set_nav_fold(target, fold, cx);
    }

    /// The tree's keys, on the tree while it holds the keyboard.
    pub(super) fn nav_keys(&self, tree: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        tree.track_focus(&self.nav_view_focus)
            .key_context("NavTree")
            .on_action(cx.listener(|view, _: &nav::SelectNext, window, cx| {
                view.step_nav(1, window, cx)
            }))
            .on_action(cx.listener(|view, _: &nav::SelectPrevious, window, cx| {
                view.step_nav(-1, window, cx)
            }))
            .on_action(
                cx.listener(|view, _: &nav::Open, window, cx| view.open_nav_cursor(window, cx)),
            )
            .on_action(cx.listener(|view, _: &nav::Fold, window, cx| {
                view.fold_nav_cursor(true, window, cx)
            }))
            .on_action(cx.listener(|view, _: &nav::Unfold, window, cx| {
                view.fold_nav_cursor(false, window, cx)
            }))
            .on_action(
                cx.listener(|view, _: &nav::CloseView, window, cx| view.leave_nav(window, cx)),
            )
    }

    /// Under the cursor's row once after a move: scroll the tree so the row
    /// shows whole, by as little as it takes (none when it already does).
    pub(super) fn cursor_reveal(&self, item: NavItem) -> Option<impl IntoElement> {
        let reveal = self.nav_reveal.clone();
        let scroll = self.nav_scroll.clone();
        (self.nav_cursor.as_ref().map(|cursor| cursor.item) == Some(item) && reveal.get()).then(
            move || {
                gpui::canvas(
                    move |row: Bounds<Pixels>, window, _| {
                        if !reveal.replace(false) {
                            return;
                        }
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
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0()
            },
        )
    }

    /// A Thread row's press, and ⏎ on it: an Attention row lands on its
    /// Thread; a member opens its Group on it; an open Thread takes focus;
    /// a parked one revives.
    pub(super) fn open_nav_row(
        &mut self,
        thread: ThreadId,
        group: Option<GroupId>,
        line: bool,
        cx: &mut Context<Self>,
    ) {
        if line {
            self.land_on_thread(thread, cx);
        } else if let Some(group) = group {
            self.enter_group(group, cx);
            self.focus_thread(thread, cx);
        } else if self.pane_for(thread).is_some() {
            self.focus_thread(thread, cx);
        } else {
            self.revive_thread(thread, cx);
        }
    }
}
