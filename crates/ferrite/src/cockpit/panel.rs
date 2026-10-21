//! The Solo view's right panel (SPEC §5.6): a fixed column on the frame,
//! right of the board and outside it — not the Beside slot, which still
//! opens inside the board (ADR 0021). Its head sits in the head row over
//! it: the `Changes`, `PR` and `Context` tabs and the button that hides
//! it. Under the head, the shown tab's body; under every tab, the foot's
//! context meter.
//!
//! When it shows (`panel_thread`): in Solo, on a Thread's Pane, with
//! nothing open beside it, while the window has room (`PANEL_SHOW_W`,
//! held down to `PANEL_HIDE_W`) and the operator has not hidden it
//! (`Settings::panel_shown`, ⌥⌘B). Hidden, the board takes its width.
//!
//! One function per tab draws its body (`panel_changes`, `panel_context`,
//! and the PR tab's `panel_pr`, in `pr_tab`, which also reads it): 7b adds
//! a file's hunks under its head (`panel_file_block`, `hunks`).

pub(crate) mod hunks;

use super::*;
use crate::icons::{self, icon};
use ferrite_core::docview::FileChange;
use gpui::relative;

/// A panel tab. A Thread the panel has not shown yet opens on `Changes`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PanelTab {
    #[default]
    Changes,
    Pr,
    Context,
}

/// The panel's own state on the cockpit.
pub(crate) struct Panel {
    /// The last tab each Thread's panel showed.
    tabs: std::collections::HashMap<ThreadId, PanelTab>,
    /// The window is too narrow for it (`fit_panel`): hidden, never saved.
    /// True until the first frame measures, so a narrow window never
    /// flashes it.
    narrow: std::cell::Cell<bool>,
    /// ⌥⌘B while the window is too narrow: the operator's override, for
    /// the session (another ⌥⌘B gives the narrow rule back). Never saved.
    forced_open: bool,
    /// The Changes tab's unfolded files and their hunks.
    hunks: hunks::Hunks,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            tabs: Default::default(),
            narrow: std::cell::Cell::new(true),
            forced_open: false,
            hunks: Default::default(),
        }
    }
}

impl CockpitView {
    /// The Thread whose panel shows this frame (see the module doc), if
    /// any.
    pub(super) fn panel_thread(&self) -> Option<ThreadId> {
        let wanted =
            self.prefs.settings.panel_shown && (!self.panel.narrow.get() || self.panel.forced_open);
        wanted.then(|| self.panel_candidate()).flatten()
    }

    /// The Thread the panel would show for, room and the operator allowing:
    /// the Solo Pane's, with nothing beside it.
    pub(super) fn panel_candidate(&self) -> Option<ThreadId> {
        if self.cockpit.roster().view() != View::Solo
            || self.projects.coordinator_shown()
            || self.empty_board_shown()
            || self.solo_pair().is_some()
        {
            return None;
        }
        let pane = self.panes.get(self.focused())?;
        if pane.preview.document().is_some() {
            return None;
        }
        pane.thread()
    }

    /// What the board leaves at its right: the panel's column, or the
    /// frame's gutter when the panel is hidden (SPEC §4).
    pub(super) fn board_right(&self) -> f32 {
        if self.panel_thread().is_some() {
            crate::theme::PANEL_W
        } else {
            crate::theme::BOARD_PAD
        }
    }

    /// Once per frame, before the board is measured: whether the window
    /// has room for the panel, held between `PANEL_HIDE_W` and
    /// `PANEL_SHOW_W` so a resize across the line does not flicker it.
    pub(super) fn fit_panel(&self, window: &Window) {
        use crate::theme::{PANEL_HIDE_W, PANEL_SHOW_W};
        let width = f32::from(window.viewport_size().width);
        let floor = if self.panel.narrow.get() {
            PANEL_SHOW_W
        } else {
            PANEL_HIDE_W
        };
        self.panel.narrow.set(width < floor);
    }

    /// The tab `thread`'s panel shows.
    pub(crate) fn panel_tab(&self, thread: ThreadId) -> PanelTab {
        self.panel.tabs.get(&thread).copied().unwrap_or_default()
    }

    /// ⌥⌘B and the head's hide button: show or hide the panel. While the
    /// window is too narrow for it, the flip is an override that is shown
    /// and never saved; otherwise it is the setting, saved.
    pub(super) fn toggle_panel(&mut self, _: &TogglePanel, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_panel_now(cx);
    }

    fn toggle_panel_now(&mut self, cx: &mut Context<Self>) {
        if self.prefs.settings.panel_shown && self.panel.narrow.get() {
            self.panel.forced_open = !self.panel.forced_open;
        } else {
            self.panel.forced_open = false;
            let shown = !self.prefs.settings.panel_shown;
            self.change_settings(|settings| settings.panel_shown = shown, cx);
        }
        cx.notify();
    }

    /// The panel on `tab` for `thread`, shown if it was hidden. Opening
    /// `Context` asks the Session for a fresh reading, as the bar's meter
    /// did.
    pub(crate) fn open_panel_tab(
        &mut self,
        thread: ThreadId,
        tab: PanelTab,
        cx: &mut Context<Self>,
    ) {
        if self.panel_thread().is_none() {
            if !self.prefs.settings.panel_shown {
                self.change_settings(|settings| settings.panel_shown = true, cx);
            }
            if self.panel.narrow.get() {
                self.panel.forced_open = true;
            }
        }
        self.select_panel_tab(thread, tab, cx);
    }

    /// The Solo head's branch chip (SPEC §5.2): the panel's Changes tab,
    /// or — when the panel already shows Changes — the panel hidden.
    pub(crate) fn panel_changes_door(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if self.panel_thread() == Some(thread) && self.panel_tab(thread) == PanelTab::Changes {
            self.toggle_panel_now(cx);
        } else {
            self.open_panel_tab(thread, PanelTab::Changes, cx);
        }
    }

    fn select_panel_tab(&mut self, thread: ThreadId, tab: PanelTab, cx: &mut Context<Self>) {
        let was = self.panel.tabs.insert(thread, tab);
        if tab == PanelTab::Context && was != Some(PanelTab::Context) {
            self.refresh_context(thread);
        }
        cx.notify();
    }

    /// Ask `thread`'s Session to report its context afresh, when it can.
    fn refresh_context(&mut self, thread: ThreadId) {
        let Some(open) = self.cockpit.thread(thread) else {
            return;
        };
        if open.supports_control(ferrite_core::ControlKind::RefreshContext) {
            let generation = open.generation();
            self.run_session_control(
                thread,
                generation,
                ferrite_core::SessionControl::RefreshContext,
            );
        }
    }

    // ------------------------------------------------------------ drawing

    /// The panel's column, when it shows: laid over the window's right
    /// edge from the top of the head row to the bar, after the head row's
    /// drag strip, so its tabs and button take their presses and the gaps
    /// between them still drag the window.
    pub(super) fn panel_element(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        use crate::theme::*;
        let thread = self.panel_thread()?;
        let tab = self.panel_tab(thread);
        let body = match tab {
            PanelTab::Changes => self.panel_changes(thread, cx),
            PanelTab::Pr => self.panel_pr(thread, window, cx),
            PanelTab::Context => self.panel_context(thread, window, cx),
        };
        Some(
            div()
                .debug_selector(|| "panel".into())
                .absolute()
                .top_0()
                .right_0()
                .bottom(px(STATUS_BAR_H))
                .w(px(PANEL_W))
                .flex()
                .flex_col()
                .pl(px(PANEL_PAD_L))
                .pr(px(PANEL_PAD_R))
                .pb(px(PANEL_PAD_B))
                .bg(paint::FRAME)
                .font_family(FONT_UI)
                .text_size(px(FS_UI))
                .line_height(px(LH_UI))
                .text_color(rgb(TEXT))
                .whitespace_nowrap()
                .overflow_hidden()
                .child(self.panel_head(thread, tab, cx))
                .child(
                    div()
                        .id("panel-body")
                        .debug_selector(|| "panel-body".into())
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(body),
                )
                .children(self.panel_foot(thread))
                .into_any_element(),
        )
    }

    /// The head (46, in the head row): the three tabs, then the button
    /// that hides the panel. On Windows it stops short of the caption
    /// buttons, which keep the row's right end.
    fn panel_head(&self, thread: ThreadId, shown: PanelTab, cx: &mut Context<Self>) -> Div {
        use crate::theme::*;
        let facts = self.facts.get(thread);
        let changed = facts.map_or(0, |facts| facts.changed_files.len());
        let pr = self.facts.pr(thread).map(|pr| pr.number);
        let tab = |tab: PanelTab, label: &'static str, count: Option<AnyElement>| {
            let id = gpui::ElementId::from(SharedString::from(format!("panel-tab-{tab:?}")));
            let key = crate::pointer::hover_key(&id);
            let active = tab == shown;
            div()
                .id(id)
                .debug_selector(move || format!("panel-tab-{tab:?}"))
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(SPACE_1_5))
                .h(px(PANEL_TAB_H))
                .px(px(PANEL_TAB_PAD_X))
                .rounded(px(R_CONTROL))
                .text_size(px(FS_LABEL))
                .font_weight(W_LABEL)
                .occlude()
                .map(|face| {
                    if active {
                        face.bg(paint::SELECTED).text_color(rgb(TEXT_STRONG))
                    } else {
                        face.text_color(rgb(TEXT_MUTED))
                            .hover_raised(key)
                            .press_raised()
                    }
                })
                .child(label)
                .children(count)
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.select_panel_tab(thread, tab, cx);
                }))
        };
        let changes_count = (changed > 0).then(|| {
            div()
                .text_size(px(FS_META))
                .font_weight(W_BODY)
                .text_color(rgb(TEXT_DIM))
                .child(SharedString::from(changed.to_string()))
                .into_any_element()
        });
        let pr_number = pr.map(|number| {
            div()
                .font_family(FONT_CODE)
                .text_size(px(FS_CHROME_CODE))
                .font_weight(W_BODY)
                .text_color(rgb(TEXT_DIM))
                .child(SharedString::from(format!("#{number}")))
                .into_any_element()
        });
        let hide_id = gpui::ElementId::from("panel-hide");
        let hide_key = crate::pointer::hover_key(&hide_id);
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_1))
            .h(px(WIN_CHROME_H))
            .when(crate::titlebar::CUSTOM, |head| {
                head.pr(px(3. * CAPTION_W - PANEL_PAD_R))
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(SPACE_0_5))
                    .child(tab(PanelTab::Changes, "Changes", changes_count))
                    .child(tab(PanelTab::Pr, "PR", pr_number))
                    .child(tab(PanelTab::Context, "Context", None)),
            )
            .child(
                div()
                    .id(hide_id)
                    .debug_selector(|| "panel-hide".into())
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .size(px(ICON_BUTTON))
                    .rounded(px(R_TAB))
                    .occlude()
                    .hover_control(hide_key)
                    .press_control()
                    .tooltip(crate::menu::action_tooltip(
                        "Hide panel",
                        "cockpit::TogglePanel",
                    ))
                    .child(icon(icons::PANEL_RIGHT, ICON_BUTTON_GLYPH, TEXT_DIM))
                    .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                        view.toggle_panel_now(cx);
                    })),
            )
    }

    /// The Changes tab: the branch row, the section line, one block per
    /// file the Thread edited, and `Review changes` — or `No changes yet`.
    /// The files are the agent's own edits (`ThreadFacts::changed_files`,
    /// with their rolled-up `+N −N`), not a git diff against the base
    /// (SPEC §8).
    fn panel_changes(&self, thread: ThreadId, cx: &mut Context<Self>) -> AnyElement {
        use crate::theme::*;
        let Some(facts) = self.facts.get(thread) else {
            return div().into_any_element();
        };
        let branch = facts
            .status
            .as_ref()
            .and_then(|status| status.branch.clone())
            .map(SharedString::from)
            .or_else(|| facts.branch.clone());
        let mut body = div().flex().flex_col().gap(px(SPACE_0_5));
        if let Some(branch) = branch {
            body = body.child(self.panel_branch_row(branch, facts));
        }
        let files = &facts.changed_files;
        if files.is_empty() {
            return body
                .child(
                    div()
                        .debug_selector(|| "panel-no-changes".into())
                        .flex()
                        .items_center()
                        .h(px(PANEL_SECTION_H))
                        .mt(px(SPACE_2))
                        .px(px(PANEL_ROW_ICON_X))
                        .text_size(px(FS_META))
                        .text_color(rgb(TEXT_DIM))
                        .child("No changes yet"),
                )
                .into_any_element();
        }
        let added = files.iter().map(|file| file.added).sum();
        let removed = files.iter().map(|file| file.removed).sum();
        let count = files.len();
        let noun = if count == 1 { "file" } else { "files" };
        body = body.child(
            div()
                .debug_selector(|| "panel-changes-section".into())
                .flex()
                .items_center()
                .h(px(PANEL_SECTION_H))
                .mt(px(SPACE_2))
                .px(px(PANEL_ROW_ICON_X))
                .text_size(px(FS_META))
                .text_color(rgb(TEXT_DIM))
                .child(
                    div()
                        .flex_1()
                        .font_weight(W_LABEL)
                        .child(SharedString::from(format!("{count} {noun} changed"))),
                )
                .child(diffstat(added, removed)),
        );
        let cwd = self.thread_path(thread);
        for (index, file) in files.iter().enumerate() {
            body = body.child(self.panel_file_block(thread, index, file, cwd.as_deref(), cx));
        }
        let review_id = gpui::ElementId::from("panel-review-changes");
        let review_key = crate::pointer::hover_key(&review_id);
        body.child(
            div()
                .id(review_id)
                .debug_selector(|| "panel-review-changes".into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .h(px(PANEL_BUTTON_H))
                .mt(px(SPACE_3))
                .rounded(px(R_CONTROL))
                .bg(paint::RAISED)
                .text_size(px(FS_LABEL))
                .font_weight(W_LABEL)
                .text_color(rgb(TEXT_STRONG))
                .hover_control(review_key)
                .press_control()
                .child("Review changes")
                .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
                    view.open_beside(
                        beside::Beside::Review(ferrite_reviewr::model::Scope::Uncommitted),
                        window,
                        cx,
                    );
                })),
        )
        .into_any_element()
    }

    /// The branch row (48): the branch, `from <default branch>` under it,
    /// and the checkout's drift and dirt at the right when git has said
    /// (`↑2 ↓1 · 3 dirty`).
    fn panel_branch_row(&self, branch: SharedString, facts: &crate::facts::ThreadFacts) -> Div {
        use crate::theme::*;
        let base = facts.default_branch.clone();
        let drift = facts.status.as_ref().and_then(|status| {
            let mut words = Vec::new();
            match (status.ahead, status.behind) {
                (0, 0) => {}
                (ahead, 0) => words.push(format!("\u{2191}{ahead}")),
                (0, behind) => words.push(format!("\u{2193}{behind}")),
                (ahead, behind) => words.push(format!("\u{2191}{ahead} \u{2193}{behind}")),
            }
            if status.dirty > 0 {
                words.push(format!("{} dirty", status.dirty));
            }
            (!words.is_empty()).then(|| words.join(" \u{b7} "))
        });
        panel_row(icons::BRANCH, TEXT_DIM)
            .debug_selector(|| "panel-branch".into())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(W_LABEL)
                            .text_color(rgb(TEXT_STRONG))
                            .child(branch),
                    )
                    .children(drift.map(|drift| {
                        div()
                            .flex_shrink_0()
                            .text_size(px(FS_META))
                            .text_color(rgb(TEXT_DIM))
                            .child(SharedString::from(drift))
                    })),
            )
            .children(base.map(|base| {
                div()
                    .truncate()
                    .text_size(px(FS_META))
                    .text_color(rgb(TEXT_DIM))
                    .child(SharedString::from(format!("from {base}")))
            }))
    }

    /// One changed file's block (r8): its head (48, `RAISED`) — the name,
    /// which opens the file in the reader as a file link does, its `+N −N`,
    /// and its directory under it — and, unfolded, its hunks (`hunks`).
    /// The rest of the head folds and unfolds them; under the pointer its
    /// icon turns to the fold's chevron.
    fn panel_file_block(
        &self,
        thread: ThreadId,
        index: usize,
        file: &FileChange,
        cwd: Option<&std::path::Path>,
        cx: &mut Context<Self>,
    ) -> Div {
        use crate::theme::*;
        let raw = std::path::PathBuf::from(&file.path);
        let path = match cwd {
            Some(cwd) if !raw.is_absolute() => cwd.join(&raw),
            _ => raw,
        };
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        // The directory as the operator knows it: under the Thread's
        // checkout, from there; elsewhere, whole.
        let dir = path
            .parent()
            .map(|dir| {
                cwd.and_then(|cwd| dir.strip_prefix(cwd).ok())
                    .unwrap_or(dir)
                    .display()
                    .to_string()
            })
            .filter(|dir| !dir.is_empty());
        let unfolded = self.panel_file_unfolded(thread, file, cx);
        let group = SharedString::from(format!("panel-file-head-{index}"));
        let chevron = if unfolded {
            icons::CHEVRON_DOWN
        } else {
            icons::CHEVRON_RIGHT
        };
        let slot = div()
            .debug_selector(move || format!("panel-file-fold-{index}"))
            .relative()
            .size(px(PANEL_ROW_ICON))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .group_hover(group.clone(), |style| style.invisible())
                    .child(icon(icons::FILE, PANEL_ROW_ICON, TEXT_MUTED)),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .invisible()
                    .group_hover(group.clone(), |style| style.visible())
                    .child(icon(chevron, PANEL_ROW_ICON, TEXT_MUTED)),
            );
        let fold_path = file.path.clone();
        let head = panel_row_with(slot)
            .id(("panel-file-head", index))
            .group(group)
            .bg(paint::RAISED)
            .cursor_pointer()
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.toggle_panel_file(thread, fold_path.clone(), cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .child(
                        div()
                            .id(("panel-file", index))
                            .debug_selector(move || format!("panel-file-{index}"))
                            .flex_shrink(1.)
                            .min_w_0()
                            .truncate()
                            .cursor_pointer()
                            .font_weight(W_LABEL)
                            .text_color(rgb(TEXT_STRONG))
                            .child(SharedString::from(name))
                            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                                cx.stop_propagation();
                                view.open_beside(
                                    beside::Beside::Reader {
                                        path: path.clone(),
                                        line: None,
                                    },
                                    window,
                                    cx,
                                );
                            })),
                    )
                    .child(div().flex_1())
                    .child(diffstat(file.added, file.removed)),
            )
            .children(dir.map(|dir| {
                div()
                    .truncate()
                    .text_size(px(FS_META))
                    .text_color(rgb(TEXT_DIM))
                    .child(SharedString::from(dir))
            }));
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .rounded(px(R_BLOCK))
            .overflow_hidden()
            .child(head)
            .children(
                unfolded
                    .then(|| self.panel_hunks_element(thread, index, &file.path))
                    .flatten(),
            )
    }

    /// The Context tab: what fills the Thread's context window, by
    /// category, and its account windows (`pane::context_usage`).
    fn panel_context(
        &self,
        thread: ThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::theme::*;
        let Some(open) = self.cockpit.thread(thread) else {
            return div().into_any_element();
        };
        let Some(usage) = open.transcript().usage() else {
            return div()
                .pt(px(SPACE_2))
                .px(px(PANEL_ROW_ICON_X))
                .text_size(px(FS_META))
                .text_color(rgb(TEXT_DIM))
                .child("Context not reported yet")
                .into_any_element();
        };
        // The tab scrolls inside the body, under the head and over the
        // foot.
        let room = f32::from(window.viewport_size().height) - WIN_CHROME_H - STATUS_BAR_H;
        let error = self
            .session_control_error
            .as_ref()
            .filter(|(shown, generation, _)| *shown == thread && *generation == open.generation())
            .map(|(_, _, error)| error.clone());
        div()
            .debug_selector(|| "panel-context".into())
            .flex()
            .flex_col()
            .pt(px(SPACE_2))
            .child(pane::context_usage(
                usage,
                self.cockpit.account_limits(open.provider()),
                open.transcript().context_details(),
                open.transcript().usage_details(),
                open.transcript().last_cost(),
                self.context_usage_expanded,
                room,
                cx.listener(|view, _: &ClickEvent, _, cx| {
                    view.context_usage_expanded = !view.context_usage_expanded;
                    cx.notify();
                }),
            ))
            .children(error.map(|error| pane::card_error(error).id("session-control-error")))
            .into_any_element()
    }

    /// The foot under every tab: a hairline, `Context` with its
    /// `84k / 200k`, and the context meter. None until the Session reports
    /// its usage: no reading is invented.
    fn panel_foot(&self, thread: ThreadId) -> Option<Div> {
        use crate::theme::*;
        let open = self.cockpit.thread(thread)?;
        let usage = open.transcript().usage()?;
        let window = usage.context_window.filter(|window| *window > 0);
        let reading = match window {
            Some(window) => format!(
                "{} / {}",
                pane::compact_count(usage.total_tokens),
                pane::compact_count(window)
            ),
            None => pane::compact_count(usage.total_tokens),
        };
        let meter = window.map(|window| {
            context_meter(
                usage.total_tokens,
                window,
                open.transcript().context_details(),
            )
        });
        Some(
            div()
                .debug_selector(move || format!("panel-foot-{}", usage.total_tokens))
                .flex()
                .flex_col()
                .flex_shrink_0()
                .gap(px(PANEL_FOOT_GAP))
                .pt(px(PANEL_FOOT_PAD_T))
                .pb(px(PANEL_FOOT_PAD_B))
                .px(px(PANEL_ROW_ICON_X))
                .border_t_1()
                .border_color(paint::HAIRLINE)
                .text_size(px(FS_META))
                .child(
                    div()
                        .flex()
                        .text_color(rgb(TEXT_DIM))
                        .child(div().flex_1().font_weight(W_LABEL).child("Context"))
                        .child(
                            div()
                                .text_color(rgb(TEXT_MUTED))
                                .child(SharedString::from(reading)),
                        ),
                )
                .children(meter),
        )
    }
}

/// A two-line panel row (48): its icon at the row's left, its two lines
/// (the caller's children) on the text column, centred down the row.
fn panel_row(glyph: &'static str, ink: u32) -> Div {
    panel_row_with(icon(glyph, crate::theme::PANEL_ROW_ICON, ink))
}

/// A two-line panel row whose icon slot holds `slot`.
fn panel_row_with(slot: impl IntoElement) -> Div {
    use crate::theme::*;
    div()
        .relative()
        .flex()
        .flex_col()
        .justify_center()
        .flex_shrink_0()
        .h(px(PANEL_ROW_H))
        .pl(px(PANEL_ROW_TEXT_X))
        .pr(px(PANEL_ROW_ICON_X))
        .rounded(px(R_CONTROL))
        .child(
            div()
                .absolute()
                .left(px(PANEL_ROW_ICON_X))
                .top(px(PANEL_ROW_ICON_Y))
                .child(slot),
        )
}

/// A file count's `+N −N` in the chrome's code face, the signs in the diff
/// hues (`pane::trail_diff_stat`).
fn diffstat(added: usize, removed: usize) -> Div {
    use crate::theme::*;
    div()
        .flex_shrink_0()
        .font_family(FONT_CODE)
        .text_size(px(FS_CHROME_CODE))
        .child(pane::trail_diff_stat(added, removed))
}

/// The foot's meter: one segment per category the Session says fills the
/// window, in its `CTX_*` ink and at its share, 2px apart, then the empty
/// track. A Session that names no categories gives one segment. Past the
/// warn fraction every segment turns `ATTENTION`, past full `BLOCKED`.
fn context_meter(total: u64, window: u64, details: Option<&ferrite_core::ContextDetails>) -> Div {
    use crate::theme::*;
    let fraction = total as f32 / window as f32;
    let state = (fraction >= CTX_METER_WARN).then(|| pane::ctx_meter_ink(fraction));
    let mut segments: Vec<(u32, f32)> = details
        .map(|details| pane::context_segments(&details.categories, window))
        .unwrap_or_default();
    if segments.is_empty() {
        segments.push((CTX_CYCLE[0], fraction));
    }
    let segment = |ink: u32| div().h_full().rounded(px(R_TIGHT)).bg(rgb(ink));
    div()
        .debug_selector(|| "panel-meter".into())
        .flex()
        .gap(px(SPACE_0_5))
        .h(px(PANEL_METER_H))
        .children(segments.into_iter().map(|(ink, share)| {
            segment(state.unwrap_or(ink))
                .flex_shrink_0()
                .w(relative(share.clamp(0., 1.)))
        }))
        .child(segment(PANEL_METER_TRACK).flex_1())
}
