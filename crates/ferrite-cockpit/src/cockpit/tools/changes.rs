//! Changes (Claude's, `changes-split.png`, `changes-more-menu.png`): the
//! checkout's diff as one scroll of files, under the Files editor in the
//! dock's column or alone.
//!
//! - **Head:** the scope as a dropdown — `All changes` (against the base
//!   branch), `Uncommitted changes` (the default) or `Compare against ›` a
//!   branch — then `⋯`, expand and close.
//! - **Each file:** `▾ name dir`, its `+N` (and `−N`), a circle that marks it
//!   reviewed (and folds it away), `↗` to open it in Files, then its hunks
//!   as the panel draws them (`panel::hunks`).
//! - **`⋯`:** Collapse all files, Expand all files, Word wrap, Hide
//!   whitespace changes, Refresh.
//!
//! The diff is the Review core's (`ferrite_reviewr::ferrite::review_against`),
//! read off the UI thread whenever the pane opens, the scope changes or
//! Refresh is pressed.

use std::collections::HashSet;

use ferrite_reviewr::ferrite::{review_against, Read};
use ferrite_ui::icons;
use ferrite_ui::theme::{self, paint};

use super::*;
use crate::cockpit::view_head::Column;
use crate::cockpit::context_menu::Submenu;
use crate::cockpit::panel::hunks::{hunks_element, shown, Line, Shown, Sign};

/// What the Changes diff is against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    /// The base branch's merge base, to the worktree.
    All,
    /// `HEAD` to the worktree.
    Uncommitted,
    /// A branch picked by name, in `All changes`' place.
    Against(String),
}

impl Scope {
    fn label(&self) -> String {
        match self {
            Scope::All => "All changes".into(),
            Scope::Uncommitted => "Uncommitted changes".into(),
            Scope::Against(branch) => format!("Compared with {branch}"),
        }
    }

    fn read(&self) -> (Read, Option<String>) {
        match self {
            Scope::All => (Read::Branch, None),
            Scope::Uncommitted => (Read::Uncommitted, None),
            Scope::Against(branch) => (Read::Branch, Some(branch.clone())),
        }
    }
}

/// What a Changes menu row does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verb {
    All,
    Uncommitted,
    /// The branch at this place in the listed branches.
    Compare(usize),
    CollapseAll,
    ExpandAll,
    ToggleWrap,
    ToggleWhitespace,
    Refresh,
}

/// A parent row's submenu: the branches to compare against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Sub;

/// How many branches `Compare against ›` lists.
const BRANCHES_LISTED: usize = 20;

struct File {
    path: String,
    added: u32,
    removed: u32,
    hunks: Shown,
}

enum Load {
    Reading,
    Read(Vec<File>),
    Failed(SharedString),
}

pub(crate) struct Changes {
    scope: Scope,
    load: Load,
    /// Files marked reviewed: folded, their circle filled.
    reviewed: HashSet<String>,
    folded: HashSet<String>,
    wrap: bool,
    hide_whitespace: bool,
    scroll: ScrollHandle,
    /// The branches `Compare against ›` lists, as of the last read.
    branches: Vec<String>,
    /// Bumped by every read, so a late answer for an old scope is dropped.
    generation: u64,
}

impl Changes {
    pub(super) fn new() -> Self {
        Self {
            scope: Scope::Uncommitted,
            load: Load::Reading,
            reviewed: HashSet::new(),
            folded: HashSet::new(),
            wrap: true,
            hide_whitespace: false,
            scroll: ScrollHandle::new(),
            branches: Vec::new(),
            generation: 0,
        }
    }
}

/// `hunks` without the changes that are only whitespace: a changed line
/// blank after trimming goes, and a hunk left with no change goes with it.
pub(crate) fn without_whitespace(hunks: Vec<Vec<Line>>) -> Vec<Vec<Line>> {
    hunks
        .into_iter()
        .map(|hunk| {
            hunk.into_iter()
                .filter(|line| line.sign == Sign::Context || !line.text.trim().is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|hunk| hunk.iter().any(|line| line.sign != Sign::Context))
        .collect()
}

impl CockpitView {
    fn changes_of(&mut self, thread: ThreadId) -> Option<&mut Changes> {
        self.tools.changes.get_mut(&thread)
    }

    /// The Changes state for `thread`, read for the first time it is made.
    pub(in crate::cockpit) fn ensure_changes(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if !self.tools.changes.contains_key(&thread) {
            self.tools.changes.insert(thread, Changes::new());
            self.refresh_changes(thread, cx);
        }
    }

    /// Read the diff again, for the current scope.
    fn refresh_changes(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(root) = self.thread_path(thread) else {
            return;
        };
        let Some(changes) = self.changes_of(thread) else {
            return;
        };
        changes.generation += 1;
        changes.load = Load::Reading;
        changes.branches = ferrite_reviewr::git::list_branches(&root)
            .unwrap_or_default()
            .into_iter()
            .take(BRANCHES_LISTED)
            .map(|branch| branch.name)
            .collect();
        let generation = changes.generation;
        let (read, base) = changes.scope.read();
        cx.spawn(async move |view, cx| {
            let files = cx
                .background_executor()
                .spawn(async move {
                    review_against(&root, &read, base.as_deref()).map(|files| {
                        files
                            .iter()
                            .map(|sides| File {
                                path: sides.file.path.clone(),
                                added: sides.file.additions,
                                removed: sides.file.deletions,
                                hunks: shown(Some(sides)),
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .await;
            view.update(cx, |view, cx| {
                if let Some(changes) = view.changes_of(thread).filter(|c| c.generation == generation) {
                    changes.load = match files {
                        Ok(files) => Load::Read(files),
                        Err(why) => Load::Failed(why.into()),
                    };
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(in crate::cockpit) fn run_changes_verb(
        &mut self,
        thread: ThreadId,
        verb: Verb,
        cx: &mut Context<Self>,
    ) {
        let Some(changes) = self.changes_of(thread) else {
            return;
        };
        match verb {
            Verb::All | Verb::Uncommitted | Verb::Compare(_) => {
                changes.scope = match verb {
                    Verb::All => Scope::All,
                    Verb::Uncommitted => Scope::Uncommitted,
                    Verb::Compare(at) => match changes.branches.get(at) {
                        Some(branch) => Scope::Against(branch.clone()),
                        None => return,
                    },
                    _ => unreachable!(),
                };
                self.refresh_changes(thread, cx);
                return;
            }
            Verb::CollapseAll => {
                if let Load::Read(files) = &changes.load {
                    changes.folded = files.iter().map(|file| file.path.clone()).collect();
                }
            }
            Verb::ExpandAll => changes.folded.clear(),
            Verb::ToggleWrap => changes.wrap = !changes.wrap,
            Verb::ToggleWhitespace => changes.hide_whitespace = !changes.hide_whitespace,
            Verb::Refresh => {
                self.refresh_changes(thread, cx);
                return;
            }
        }
        cx.notify();
    }

    fn toggle_fold(&mut self, thread: ThreadId, path: &str, cx: &mut Context<Self>) {
        if let Some(changes) = self.changes_of(thread) {
            if !changes.folded.remove(path) {
                changes.folded.insert(path.to_string());
            }
            cx.notify();
        }
    }

    fn toggle_reviewed(&mut self, thread: ThreadId, path: &str, cx: &mut Context<Self>) {
        if let Some(changes) = self.changes_of(thread) {
            if changes.reviewed.remove(path) {
                changes.folded.remove(path);
            } else {
                changes.reviewed.insert(path.to_string());
                changes.folded.insert(path.to_string());
            }
            cx.notify();
        }
    }

    /// The rows of the scope dropdown.
    pub(in crate::cockpit) fn changes_scope_rows(&self, thread: ThreadId) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let Some(changes) = self.tools.changes.get(&thread) else {
            return Vec::new();
        };
        let row = |label: &'static str, scope: Scope, verb: Verb| {
            let mut item = menu::Item::new(label);
            item.checked = changes.scope == scope;
            Some((item, MenuVerb::Changes(verb)))
        };
        vec![
            row("All changes", Scope::All, Verb::All),
            row("Uncommitted changes", Scope::Uncommitted, Verb::Uncommitted),
            Some((
                menu::Item::new("Compare against"),
                MenuVerb::Submenu(Submenu::Changes(Sub)),
            )),
        ]
    }

    /// `Compare against ›`: the branches, the standing one ticked.
    pub(in crate::cockpit) fn changes_branch_rows(&self, thread: ThreadId) -> Vec<(menu::Item, MenuVerb)> {
        let Some(changes) = self.tools.changes.get(&thread) else {
            return Vec::new();
        };
        changes
            .branches
            .iter()
            .enumerate()
            .map(|(at, branch)| {
                let mut item = menu::Item::new(branch.clone());
                item.checked = changes.scope == Scope::Against(branch.clone());
                (item, MenuVerb::Changes(Verb::Compare(at)))
            })
            .collect()
    }

    /// The Changes `⋯` menu's rows.
    pub(in crate::cockpit) fn changes_more_rows(&self, thread: ThreadId) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let Some(changes) = self.tools.changes.get(&thread) else {
            return Vec::new();
        };
        let row = |label: &'static str, checked: bool, verb: Verb| {
            let mut item = menu::Item::new(label);
            item.checked = checked;
            Some((item, MenuVerb::Changes(verb)))
        };
        vec![
            row("Collapse all files", false, Verb::CollapseAll),
            row("Expand all files", false, Verb::ExpandAll),
            None,
            row("Word wrap", changes.wrap, Verb::ToggleWrap),
            row("Hide whitespace changes", changes.hide_whitespace, Verb::ToggleWhitespace),
            None,
            row("Refresh", false, Verb::Refresh),
        ]
    }

    // ------------------------------------------------------------ drawing

    pub(in crate::cockpit) fn changes_pane(&self, thread: ThreadId, cx: &mut Context<Self>) -> Div {
        let Some(changes) = self.tools.changes.get(&thread) else {
            return div().flex_1();
        };
        let body = match &changes.load {
            Load::Reading => self.changes_note("Reading changes\u{2026}").into_any_element(),
            Load::Failed(why) => self.changes_note(why.clone()).into_any_element(),
            Load::Read(files) if files.is_empty() => {
                self.changes_empty(thread, &changes.scope, cx).into_any_element()
            }
            Load::Read(files) => div()
                .id("changes-scroll")
                .debug_selector(|| "changes-scroll".into())
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&changes.scroll)
                .children(files.iter().enumerate().map(|(at, file)| self.changes_file(thread, changes, at, file, cx)))
                .into_any_element(),
        };
        div()
            .debug_selector(|| "changes".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .when(self.tools.dock(thread).files, |pane| {
                pane.border_t(px(theme::HAIRLINE_W)).border_color(paint::LINE)
            })
            .child(self.changes_head(thread, changes, cx))
            .child(body)
    }

    /// Nothing changed in the scope: its note, and for the uncommitted scope
    /// a link to the whole branch's diff.
    fn changes_empty(&self, thread: ThreadId, scope: &Scope, cx: &mut Context<Self>) -> Div {
        let note = self.changes_note("No changes");
        if *scope != Scope::Uncommitted {
            return note;
        }
        note.flex_col().gap(px(theme::SPACE_1)).child(
            div()
                .id("changes-view-branch")
                .debug_selector(|| "changes-view-branch".into())
                .cursor_pointer()
                .text_color(rgb(theme::LINK_INK))
                .child("View branch diff")
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.run_changes_verb(thread, Verb::All, cx)
                })),
        )
    }

    fn changes_note(&self, note: impl Into<SharedString>) -> Div {
        div()
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .text_size(px(theme::FS_LABEL))
            .text_color(rgb(theme::INK_SUBTLE))
            .child(note.into())
    }

    fn changes_head(&self, thread: ThreadId, changes: &Changes, cx: &mut Context<Self>) -> Div {
        use theme::*;
        let door = |key: &'static str, glyph: &'static str, tip: &'static str| {
            ferrite_pane::preview::head_door(key, key.into(), glyph)
                .tooltip(ferrite_ui::menu::tooltip(tip))
        };
        let expanded = self.panel.expanded;
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_1))
            .h(px(WIN_CHROME_H))
            .px(px(SIDE_CHAT_INSET))
            .pr(px(super::super::panel::tools_inset() - PANEL_PAD_R))
            .mt(px(self.head_drop(Column::Files)))
            .child(
                div()
                    .id("changes-scope")
                    .debug_selector(|| "changes-scope".into())
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(SPACE_1))
                    .h(px(PANEL_TAB_H))
                    .px(px(PANEL_TAB_PAD_X))
                    .rounded(px(R_CONTROL))
                    .text_size(px(FS_LABEL))
                    .cursor_pointer()
                    .hover(|scope| scope.bg(paint::RAISED))
                    .child(div().min_w_0().truncate().child(SharedString::from(changes.scope.label())))
                    .child(icons::icon(icons::CHEVRON_DOWN, ICON_CHEVRON, INK_SUBTLE))
                    .on_click(cx.listener(move |view, event: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.open_context_menu(MenuTarget::ChangesScope(thread), event.position(), cx);
                    })),
            )
            .child(door("changes-more", icons::ELLIPSIS, "More").on_click(cx.listener(
                move |view, event: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.open_context_menu(MenuTarget::ChangesMore(thread), event.position(), cx);
                },
            )))
            .child(
                door(
                    "changes-expand",
                    if expanded { icons::MINIMIZE } else { icons::MAXIMIZE },
                    if expanded { "Collapse" } else { "Expand" },
                )
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.panel.expanded = !view.panel.expanded;
                    cx.notify();
                })),
            )
            .child(door("changes-close", icons::CLOSE, "Close").on_click(cx.listener(
                move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.press_tool(Tool::Changes, cx);
                },
            )))
    }

    fn changes_file(
        &self,
        thread: ThreadId,
        changes: &Changes,
        at: usize,
        file: &File,
        cx: &mut Context<Self>,
    ) -> Div {
        use theme::*;
        let folded = changes.folded.contains(&file.path);
        let reviewed = changes.reviewed.contains(&file.path);
        let (name, dir) = match file.path.rsplit_once('/') {
            Some((dir, name)) => (name.to_string(), dir.to_string()),
            None => (file.path.clone(), String::new()),
        };
        let (fold_path, review_path, open_path) = (file.path.clone(), file.path.clone(), file.path.clone());
        let stat = div()
            .flex()
            .flex_shrink_0()
            .gap(px(SPACE_1))
            .text_size(px(FS_META))
            .child(div().text_color(rgb(GREEN)).child(SharedString::from(format!("+{}", file.added))))
            .children((file.removed > 0).then(|| {
                div()
                    .text_color(rgb(RED))
                    .child(SharedString::from(format!("\u{2212}{}", file.removed)))
            }));
        // A door of this file's head, its key naming the file's row.
        let door = |key: String, glyph: &'static str, tip: &'static str, ink: u32| {
            let id = SharedString::from(key);
            let selector = id.to_string();
            div()
                .id(id.clone())
                .debug_selector(move || selector.clone())
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .size(px(ICON_BUTTON))
                .rounded(px(R_CONTROL))
                .hover_control(id)
                .child(icons::icon(glyph, ICON_BUTTON_GLYPH, ink))
                .tooltip(ferrite_ui::menu::tooltip(tip))
        };
        let head = div()
            .id(SharedString::from(format!("changes-file-{at}")))
            .debug_selector(move || format!("changes-file-{at}"))
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(PANEL_SECTION_H))
            .px(px(SIDE_CHAT_INSET))
            .cursor_pointer()
            .text_size(px(FS_LABEL))
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.toggle_fold(thread, &fold_path, cx)
            }))
            .child(ferrite_ui::components::chevron_at(
                if folded { 0. } else { 1. },
                ICON_CHEVRON,
                INK_SUBTLE,
            ))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(SPACE_1))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(
                        div()
                            .text_color(rgb(if reviewed { INK_SUBTLE } else { LINK_INK }))
                            .child(SharedString::from(name)),
                    )
                    .child(div().text_color(rgb(INK_SUBTLE)).child(SharedString::from(dir))),
            )
            .child(stat)
            .child(
                door(
                    format!("changes-reviewed-{at}"),
                    if reviewed { icons::CHECK_CIRCLE } else { icons::CIRCLE_DASHED },
                    if reviewed { "Mark as not reviewed" } else { "Mark as reviewed" },
                    if reviewed { GREEN } else { INK_SUBTLE },
                )
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.toggle_reviewed(thread, &review_path, cx);
                })),
            )
            .child(
                door(format!("changes-open-{at}"), icons::EXTERNAL, "Open file", INK_SUBTLE).on_click(
                    cx.listener(move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        if let Some(root) = view.thread_path(thread) {
                            view.open_in_files(thread, root.join(&open_path), None, false, cx);
                        }
                    }),
                ),
            );
        let hunks = (!folded).then(|| {
            let hunks = match (&file.hunks, changes.hide_whitespace) {
                (Shown::Hunks(hunks), true) => Shown::Hunks(without_whitespace(hunks.clone())),
                (other, _) => other.clone(),
            };
            hunks_element(at, &hunks, changes.wrap)
        });
        div().flex().flex_col().flex_shrink_0().child(head).children(hunks)
    }
}

#[cfg(test)]
impl CockpitView {
    /// How many files the last read found, once it has landed.
    pub(in crate::cockpit) fn changes_read(&self, thread: ThreadId) -> Option<usize> {
        match &self.tools.changes.get(&thread)?.load {
            Load::Read(files) => Some(files.len()),
            _ => None,
        }
    }

    pub(in crate::cockpit) fn changes_scope_label(&self, thread: ThreadId) -> Option<String> {
        Some(self.tools.changes.get(&thread)?.scope.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(sign: Sign, text: &str) -> Line {
        Line { sign, text: text.to_string().into() }
    }

    #[test]
    fn hiding_whitespace_drops_blank_changes_and_the_hunks_they_leave_bare() {
        let kept = vec![line(Sign::Context, "a"), line(Sign::Added, "b")];
        let bare = vec![line(Sign::Context, "c"), line(Sign::Added, "  "), line(Sign::Removed, "")];
        let mixed = vec![line(Sign::Added, ""), line(Sign::Removed, "x")];
        let out = without_whitespace(vec![kept.clone(), bare, mixed]);
        assert_eq!(out, [kept, vec![line(Sign::Removed, "x")]]);
    }

    #[test]
    fn a_scope_says_what_it_reads() {
        assert_eq!(Scope::Uncommitted.read(), (Read::Uncommitted, None));
        assert_eq!(Scope::All.read(), (Read::Branch, None));
        assert_eq!(Scope::Against("dev".into()).read(), (Read::Branch, Some("dev".into())));
        assert_eq!(Scope::All.label(), "All changes");
    }
}
