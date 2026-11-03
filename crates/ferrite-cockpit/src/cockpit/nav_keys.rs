//! The nav tree's keyboard (§5.1.10): ⌘⇧E hands the tree the keys, the
//! way ⌘I hands them to the Inbox. ↑↓ walk the rows the tree draws, in the
//! order it draws them — each Project's heading, its loose Threads, its
//! Groups and their members —
//! ⏎ opens what the row stands for (a heading's ⏎ folds it), ← folds the
//! fold the row heads, or from a row inside one moves to its head and
//! folds it, and → unfolds what the row heads. Esc gives the keys back to
//! the board.
//!
//! Only ⌘⇧E hands the tree the keys: a press in the tree never takes them
//! (a row's press opens its Thread, whose Composer keeps typing), and a
//! press while the tree holds them hands them back first.
//!
//! One cursor, the pointer's: the row under the keyboard wears the hover
//! face (its wash, its hover action), driven through the same blend a
//! pointer drives, so a row never shows two kinds of "here". It is not the
//! selection, which stays what the board shows, and it clears whenever the
//! tree lets go of the keyboard. A move scrolls the row into view
//! (`cursor_reveal`).

use super::*;
use crate::nav::{NavCursor, NavItem};
use ferrite_ui::motion;
use gpui::{App, Bounds};

impl CockpitView {
    /// Every row the keyboard can stand on, in the tree's drawing order,
    /// each with the blend key its row's hover face reads and the fold's
    /// head it sits under (`Placed`). It walks what `nav_tree` draws, folds
    /// and all.
    fn nav_items(&self, cx: &App) -> Vec<Placed> {
        let state = self.nav_state();
        let mut items = Vec::new();
        let machine_folded = self
            .machine_rows(cx)
            .and_then(|rows| rows.into_iter().find(|row| row.selected))
            .is_some_and(|on| on.folded);
        if machine_folded {
            return items;
        }
        for (index, section) in state.sections.iter().enumerate() {
            let head = match (section.mixed, section.project) {
                (true, _) => Some(NavCursor::heading(NavFold::Groups, "nav-groups-heading")),
                (false, Some(project)) => Some(NavCursor::heading(
                    NavFold::Project(project),
                    format!("nav-project-section-{index}"),
                )),
                // `Other` names no Project, and its heading folds nothing.
                (false, None) => None,
            };
            items.extend(head.clone().map(Placed::head));
            if section.folded {
                continue;
            }
            items.extend(section.solos.iter().map(|row| {
                Placed::under(
                    NavCursor::thread(state.solos[*row].thread, None),
                    head.as_ref(),
                )
            }));
            for group in section.groups.iter().map(|at| &state.groups[*at]) {
                let row = NavCursor::group(group.id);
                items.push(Placed::under(row.clone(), head.as_ref()));
                if !group.folded {
                    items.extend(group.members.iter().map(|member| {
                        Placed::under(NavCursor::thread(member.thread, Some(group.id)), Some(&row))
                    }));
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
            .map(|placed| &placed.cursor)
            .find(|cursor| match cursor.item {
                NavItem::Thread(thread, _) => Some(thread) == shown,
                _ => false,
            })
            .or(items.first().map(|placed| &placed.cursor))
            .cloned();
        self.nav_keys_held = true;
        window.focus(&self.nav_view_focus, cx);
        // However the keys leave (Esc, a press on the board, ⌘K, ⌘I), the
        // cursor goes with them.
        self.nav_blur = Some(
            cx.on_blur(&self.nav_view_focus, window, |view, window, cx| {
                view.release_nav(window, cx)
            }),
        );
        self.move_nav_cursor(start, window, cx);
    }

    /// The tree lets go of the keyboard: its cursor clears, and render's
    /// focus rule hands the keys to the focused Pane.
    fn release_nav(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.nav_keys_held = false;
        self.move_nav_cursor(None, window, cx);
    }

    /// Esc, ⏎ on a row, or a press in the tree while it holds the keys:
    /// they go back to the board.
    fn leave_nav(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.release_nav(window, cx);
        if self.nav_view_focus.is_focused(window) {
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
            motion::hover_listener(old.key)(&false, window, cx);
        }
        if let Some(next) = &next {
            motion::hover_listener(next.key.clone())(&true, window, cx);
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
        let at = self.nav_cursor.as_ref().and_then(|cursor| {
            items
                .iter()
                .position(|placed| placed.cursor.item == cursor.item)
        });
        let next = match at {
            Some(at) => at
                .saturating_add_signed(by)
                .min(items.len().saturating_sub(1)),
            None => 0,
        };
        let next = items.get(next).map(|placed| placed.cursor.clone());
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
            NavItem::Thread(thread, group) => {
                self.open_nav_row(thread, group, cx);
                self.leave_nav(window, cx);
            }
            NavItem::Group(group) => {
                self.enter_group(group, cx);
                self.leave_nav(window, cx);
            }
        }
    }

    /// ← (`fold`) or →: the fold the cursor's row heads (a heading's, a
    /// Group's). ← on a row with nothing under it (a Thread, an Attention
    /// row) moves to the head it sits under and folds that, as an outline's
    /// ← goes to the parent, so the cursor never stands on a row the fold
    /// took away. → there does nothing.
    fn fold_nav_cursor(&mut self, fold: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.nav_cursor.as_ref().map(|cursor| cursor.item) else {
            return;
        };
        let head = match item {
            NavItem::Heading(_) | NavItem::Group(_) => self.nav_cursor.clone(),
            NavItem::Thread(..) if fold => {
                let parent = self
                    .nav_items(cx)
                    .into_iter()
                    .find(|placed| placed.cursor.item == item)
                    .and_then(|placed| placed.parent);
                if parent.is_some() {
                    self.move_nav_cursor(parent.clone(), window, cx);
                }
                parent
            }
            NavItem::Thread(..) => None,
        };
        let target = match head.map(|head| head.item) {
            Some(NavItem::Heading(target)) => target,
            Some(NavItem::Group(group)) => NavFold::Group(group),
            _ => return,
        };
        self.set_nav_fold(target, fold, cx);
    }

    /// The tree's keys, on the tree while it holds the keyboard.
    pub(super) fn nav_keys(&self, tree: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        tree.track_focus(&self.nav_view_focus)
            .key_context("NavTree")
            // A press never takes the keys (it runs before the tree's own
            // focus-on-press, which it prevents, and so does the root's):
            // the Composer keeps typing. While the tree holds them, a press
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

    /// A Thread row's press, and ⏎ on it: a member opens its Group on it;
    /// an open Thread takes focus; a parked one revives.
    pub(super) fn open_nav_row(
        &mut self,
        thread: ThreadId,
        group: Option<GroupId>,
        cx: &mut Context<Self>,
    ) {
        if let Some(group) = group {
            self.enter_group(group, cx);
            self.focus_thread(thread, cx);
        } else if self.pane_for(thread).is_some() {
            self.focus_thread(thread, cx);
        } else {
            self.revive_thread(thread, cx);
        }
    }
}

/// A row the keyboard can stand on, and the head of the fold it sits in
/// (a heading's, a Group's): where ← goes from a row that heads nothing.
struct Placed {
    cursor: NavCursor,
    parent: Option<NavCursor>,
}

impl Placed {
    fn head(cursor: NavCursor) -> Self {
        Placed {
            cursor,
            parent: None,
        }
    }

    fn under(cursor: NavCursor, parent: Option<&NavCursor>) -> Self {
        Placed {
            cursor,
            parent: parent.cloned(),
        }
    }
}
