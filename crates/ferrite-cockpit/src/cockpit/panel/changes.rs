//! The Panel's Changes section: what the Thread edited — the agent's own
//! edits (`ThreadFacts::changed_files`, with their rolled-up `+N −N`), not
//! a git diff against the base (SPEC §8) — and its pull request once one
//! exists.
//!
//! Under the section head, the summary line (`N files changed`, its
//! diffstat, and `Create PR` at the right until a pull request exists),
//! which folds and unfolds every file at once; one block per file, whose
//! hunks fold under its head (`hunks`); `Review changes`; and the pull
//! request's row once there is one. With neither, no section at all.

use super::*;
use ferrite_core::docview::FileChange;
use ferrite_core::workspace::{CheckState, PrState, PullRequest};

impl CockpitView {
    /// The Changes section (see the module doc): none with no edits and no
    /// pull request.
    pub(super) fn changes_section(&self, thread: ThreadId, cx: &mut Context<Self>) -> Option<Div> {
        use ferrite_ui::theme::*;
        let files = self
            .facts
            .get(thread)
            .map(|facts| facts.changed_files.as_slice())
            .unwrap_or_default();
        let pr = self.facts.pr(thread);
        if files.is_empty() && pr.is_none() {
            return None;
        }
        let mut section = div()
            .flex()
            .flex_col()
            .gap(px(SPACE_0_5))
            .child(panel_section_head("panel-changes-head", "Changes"));
        if !files.is_empty() {
            let create_pr = pr.is_none();
            section = section.child(self.changes_summary(thread, files, create_pr, cx));
            let cwd = self.thread_path(thread);
            for (index, file) in files.iter().enumerate() {
                section =
                    section.child(self.panel_file_block(thread, index, file, cwd.as_deref(), cx));
            }
            section = section.child(review_changes(cx));
        }
        Some(section.children(pr.map(|pr| pr_row(thread, pr, cx))))
    }

    /// The summary line: `N files changed` and the diffstat, folding and
    /// unfolding every file at once, its chevron on the content edge and
    /// its words on the text column. Under the pointer its words blend up
    /// to `INK_STRONG`, as the nav's fold headings do; a press lands at
    /// `INK_STRONG`. With `create_pr`, Claude's `Create PR` ends it.
    fn changes_summary(
        &self,
        thread: ThreadId,
        files: &[FileChange],
        create_pr: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        use ferrite_ui::theme::*;
        let added = files.iter().map(|file| file.added).sum();
        let removed = files.iter().map(|file| file.removed).sum();
        let count = files.len();
        let noun = if count == 1 { "file" } else { "files" };
        let section_key = SharedString::from(format!("panel-changes-section-{}", thread.get()));
        let all = self.panel_files_unfolded(thread, files.iter().map(|file| file.path.as_str()));
        let paths: Vec<String> = files.iter().map(|file| file.path.clone()).collect();
        div()
            .id("panel-changes-section")
            .debug_selector(|| "panel-changes-section".into())
            .relative()
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(PANEL_SECTION_H))
            .pl(px(PANEL_ROW_TEXT_X))
            .pr(px(PANEL_ROW_ICON_X))
            .text_size(px(FS_META))
            .text_color(ferrite_ui::motion::hover_blend(
                &section_key,
                rgb(INK_SUBTLE).into(),
                rgb(INK_STRONG).into(),
            ))
            .on_hover(ferrite_ui::motion::hover_listener(section_key))
            .active(|style| style.text_color(rgb(INK_STRONG)))
            .cursor_pointer()
            .tooltip(ferrite_ui::menu::tooltip(if all {
                "Collapse all"
            } else {
                "Expand all"
            }))
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.toggle_panel_files(thread, paths.clone(), cx);
            }))
            .child(
                div()
                    .absolute()
                    .left(px(PANEL_ROW_ICON_X))
                    .top(px((PANEL_SECTION_H - PANEL_ROW_ICON) / 2.))
                    .child(ferrite_ui::components::fold_chevron(
                        "panel-files-chevron",
                        all,
                        PANEL_ROW_ICON,
                        INK_SUBTLE,
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .font_weight(W_LABEL)
                    .child(SharedString::from(format!("{count} {noun} changed"))),
            )
            .child(diffstat(added, removed))
            .when(create_pr, |line| {
                line.child(create_pr_button(cx).ml(px(SPACE_3)))
            })
    }

    /// `Create PR`: ask the Thread's agent to commit what is left, push its
    /// branch and open the pull request (`create_pr_prompt`), queued behind
    /// a running turn, as Claude's button does. The PR's row takes the
    /// button's place once the forge reports it.
    fn create_pr(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(index) = self.panel_pane() else {
            return;
        };
        let branch = self.thread_branch(thread);
        let base = self
            .facts
            .get(thread)
            .and_then(|facts| facts.default_branch.clone());
        let prompt = create_pr_prompt(branch.as_deref(), base.as_deref());
        self.send_again(index, thread, prompt, cx);
    }

    /// One changed file's block (r8): its head (48, `RAISED`) — the name,
    /// which opens the file in the preview as a file link does, its `+N −N`,
    /// and its directory under it — and, unfolded, its hunks (`hunks`).
    /// The rest of the head folds and unfolds them; under the pointer its
    /// icon turns to the fold's chevron, which turns a quarter with the
    /// fold (`fold_chevron`).
    fn panel_file_block(
        &self,
        thread: ThreadId,
        index: usize,
        file: &FileChange,
        cwd: Option<&std::path::Path>,
        cx: &mut Context<Self>,
    ) -> Div {
        use ferrite_ui::theme::*;
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
        let shown_path = match &dir {
            Some(dir) => format!("{dir}/{name}"),
            None => name.clone(),
        };
        let actions = file_actions(index, shown_path, path.clone(), dir.is_some(), cx);
        let unfolded = self.panel_file_unfolded(thread, file, cx);
        let group = SharedString::from(format!("panel-file-head-{index}"));
        let slot = div()
            .debug_selector(move || format!("panel-file-fold-{index}"))
            .relative()
            .size(px(PANEL_ROW_ICON))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .group_hover(group.clone(), |style| style.invisible())
                    .child(icon(icons::FILE, PANEL_ROW_ICON, INK_SUBTLE)),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .invisible()
                    .group_hover(group.clone(), |style| style.visible())
                    .child(ferrite_ui::components::fold_chevron(
                        ("panel-file-chevron", index),
                        unfolded,
                        PANEL_ROW_ICON,
                        INK_SUBTLE,
                    )),
            );
        let fold_path = file.path.clone();
        let head = file_head_row(slot, dir.is_some())
            .id(("panel-file-head", index))
            .group(group.clone())
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
                            .text_color(rgb(INK_STRONG))
                            .child(SharedString::from(name))
                            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                                cx.stop_propagation();
                                view.open_preview(path.clone(), None, window, cx);
                            })),
                    )
                    .child(div().flex_1())
                    .child(diffstat(file.added, file.removed)),
            )
            .children(dir.map(|dir| {
                div()
                    .truncate()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(SharedString::from(dir))
            }))
            .child(
                actions
                    .invisible()
                    .group_hover(group, |style| style.visible()),
            );
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .rounded(px(R_CARD))
            .overflow_hidden()
            .child(head)
            .children(self.panel_hunks_element(thread, index, &file.path, unfolded))
    }
}

/// The `Create PR` button at the end of the summary line
/// (`PANEL_LINE_BUTTON_H`): a press stops at it, so the line does not fold.
fn create_pr_button(cx: &mut Context<CockpitView>) -> gpui::Stateful<Div> {
    use ferrite_ui::theme::*;
    let id = gpui::ElementId::from("panel-create-pr");
    let key = ferrite_ui::pointer::hover_key(&id);
    div()
        .id(id)
        .debug_selector(|| "panel-create-pr".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(PANEL_LINE_BUTTON_H))
        .px(px(PANEL_LINE_BUTTON_PAD_X))
        .rounded(px(R_CONTROL))
        .bg(paint::RAISED)
        .text_size(px(FS_LABEL))
        .font_weight(W_LABEL)
        .text_color(rgb(INK_STRONG))
        .hover_control(key)
        .press_control()
        .child("Create PR")
        .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            if let Some(thread) = view
                .panel_pane()
                .and_then(|index| view.panes[index].thread())
            {
                view.create_pr(thread, cx);
            }
        }))
}

/// What `Create PR` asks the agent, after Claude's own: commit, push, open
/// the pull request with `gh pr create` (on `base` when origin has it) and
/// report its URL — or, when one already exists, push to it and report
/// that one.
fn create_pr_prompt(branch: Option<&str>, base: Option<&str>) -> String {
    let mut prompt = String::from(
        "Create a pull request for the changes in this session.\n\n\
         1. Check for uncommitted changes. If any exist, stage and commit them with a \
         message that describes what changed and why.\n\
         2. Push the branch to the remote.\n\
         3. Open the PR with `gh pr create`. When a base branch is named below and it \
         exists on origin (`git ls-remote --heads origin <base>`), pass it with `--base`; \
         otherwise leave `--base` off and let gh infer it. Write a title that summarizes \
         the change and a body that covers what changed, why, and anything a reviewer \
         should know. If the repo has a PR template, follow it.\n\
         4. Report the PR URL when done.\n\n\
         If a PR already exists for this branch, push any new commits and report its URL \
         instead of creating a duplicate.",
    );
    let target: Vec<String> = [("Branch", branch), ("Base branch", base)]
        .into_iter()
        .filter_map(|(key, value)| Some(format!("{key}: {}", value?)))
        .collect();
    if !target.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(&target.join("\n"));
    }
    prompt
}

/// `Review changes`: the Review beside the Pane on the uncommitted work.
fn review_changes(cx: &mut Context<CockpitView>) -> gpui::Stateful<Div> {
    use ferrite_ui::theme::*;
    let review_id = gpui::ElementId::from("panel-review-changes");
    let review_key = ferrite_ui::pointer::hover_key(&review_id);
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
        .text_color(rgb(INK_STRONG))
        .hover_control(review_key)
        .press_control()
        .child("Review changes")
        .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
            view.open_beside(
                beside::Beside::Review(ferrite_reviewr::model::Scope::Uncommitted),
                window,
                cx,
            );
        }))
}

/// The pull request's row: its glyph and `#N` on the content edge, its
/// state and checks dim after it (`Open · checks failing`, the checks word
/// in its state ink when it needs the operator); a click opens it on the
/// forge.
fn pr_row(
    thread: ThreadId,
    pr: &PullRequest,
    cx: &mut Context<CockpitView>,
) -> gpui::Stateful<Div> {
    use ferrite_ui::theme::*;
    let glyph = match pr.state {
        PrState::Merged => icons::GIT_MERGE,
        PrState::Open | PrState::Closed => icons::GIT_PULL_REQUEST,
    };
    let state = match (pr.state, pr.draft) {
        (PrState::Merged, _) => "Merged",
        (PrState::Closed, _) => "Closed",
        (PrState::Open, true) => "Draft",
        (PrState::Open, false) => "Open",
    };
    let checks = pr.checks.map(|checks| match checks {
        CheckState::Passing => ("checks passing", INK_SUBTLE),
        CheckState::Failing => ("checks failing", BLOCKED),
        CheckState::Pending => ("checks running", INK_SUBTLE),
        CheckState::Skipped => ("checks skipped", INK_SUBTLE),
    });
    super::sections::detail_row(
        "panel-pr",
        glyph,
        format!("#{}", pr.number),
        Some(state.to_string()),
    )
    .mt(px(SPACE_2))
    .cursor_pointer()
    .tooltip(ferrite_ui::menu::action_tooltip(
        "Open on GitHub",
        "cockpit::OpenPrInBrowser",
    ))
    .children(checks.map(|(word, ink)| {
        div()
            .flex_shrink_0()
            .text_size(px(FS_META))
            .text_color(rgb(ink))
            .child(word)
    }))
    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
        view.open_pr(thread, cx);
    }))
}

/// A file block's head row (`PANEL_ROW_H`): its icon slot on the card's
/// inset (`PANEL_CARD_PAD_X`), its lines (the caller's children) on the
/// text column, centred down the row. The icon sits on the first of two
/// lines, or centred beside one.
fn file_head_row(slot: impl IntoElement, two_lines: bool) -> Div {
    use ferrite_ui::theme::*;
    div()
        .relative()
        .flex()
        .flex_col()
        .justify_center()
        .flex_shrink_0()
        .h(px(PANEL_ROW_H))
        .pl(px(PANEL_CARD_PAD_X + PANEL_ROW_TEXT_X))
        .pr(px(PANEL_CARD_PAD_X))
        .rounded(px(R_CONTROL))
        .child(
            div()
                .absolute()
                .left(px(PANEL_CARD_PAD_X))
                .top(px(if two_lines {
                    PANEL_ROW_ICON_Y
                } else {
                    (PANEL_ROW_H - PANEL_ROW_ICON) / 2.
                }))
                .child(slot),
        )
}

/// A changed file's actions, laid over its head's diffstat and shown while
/// the pointer is on the head (`PANEL_FILE_ACTION`): copy the path as the
/// block reads it, and open the file in the app the system gives it. A
/// click on the name previews it; the rest of the head folds its hunks.
fn file_actions(
    index: usize,
    shown_path: String,
    path: std::path::PathBuf,
    two_lines: bool,
    cx: &mut Context<CockpitView>,
) -> Div {
    use ferrite_ui::theme::*;
    let door = |id: &'static str, glyph: &'static str, tooltip: &'static str| {
        let element = gpui::ElementId::from((id, index));
        let key = ferrite_ui::pointer::hover_key(&element);
        div()
            .id(element)
            .debug_selector(move || format!("{id}-{index}"))
            .flex()
            .items_center()
            .justify_center()
            .size(px(PANEL_FILE_ACTION))
            .rounded(px(R_CONTROL))
            .cursor_pointer()
            .bg(ferrite_ui::motion::hover_blend(
                &key,
                paint::RAISED.into(),
                paint::SELECTED.into(),
            ))
            .on_hover(ferrite_ui::motion::hover_listener(key))
            .press_control()
            .tooltip(ferrite_ui::menu::tooltip(tooltip))
            .child(icon(glyph, PANEL_FILE_ACTION_GLYPH, INK_STRONG))
    };
    div()
        .absolute()
        // Centred on the name's line.
        .top(px((PANEL_ROW_H
            - LH_UI
            - if two_lines { LH_META } else { 0. })
            / 2.
            + (LH_UI - PANEL_FILE_ACTION) / 2.))
        .right(px(SPACE_1_5))
        .flex()
        .gap(px(SPACE_0_5))
        .pl(px(SPACE_1))
        .bg(paint::RAISED)
        .child(
            div()
                .debug_selector(move || format!("panel-file-copy-{index}"))
                .child(
                    ferrite_ui::components::copy_button(("panel-file-copy", index), shown_path)
                        .tooltip("Copy path")
                        .door(PANEL_FILE_ACTION, PANEL_FILE_ACTION_GLYPH, R_CONTROL)
                        .on_raised(),
                ),
        )
        .child(
            door("panel-file-open", icons::EXTERNAL, "Open in its app").on_click(cx.listener(
                move |_, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    ferrite_pane::preview::open_original(&path, window, cx);
                },
            )),
        )
}

/// A file count's `+N −N` in the chrome's code face, the signs in the diff
/// hues (`pane::trail_diff_stat`).
fn diffstat(added: usize, removed: usize) -> Div {
    use ferrite_ui::theme::*;
    div()
        .flex_shrink_0()
        .font_family(FONT_CODE)
        .text_size(px(FS_CHROME_CODE))
        .child(pane::trail_diff_stat(added, removed))
}

#[cfg(test)]
mod tests {
    use super::create_pr_prompt;

    /// `Create PR` names the branch and its base when it knows them, and
    /// says nothing of either when it does not.
    #[test]
    fn the_create_pr_prompt_names_its_target() {
        let prompt = create_pr_prompt(Some("fix-nav"), Some("main"));
        assert!(prompt.starts_with("Create a pull request for the changes in this session."));
        assert!(prompt.contains("`gh pr create`"));
        assert!(prompt.ends_with("\n\nBranch: fix-nav\nBase branch: main"));
        let bare = create_pr_prompt(None, None);
        assert!(bare.ends_with("instead of creating a duplicate."));
    }
}
