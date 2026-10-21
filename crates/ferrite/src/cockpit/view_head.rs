//! The view head (SPEC §5.2): the head row's slot over the board.
//!
//! Solo names the focused Thread — its title (a double-click renames it),
//! its Project, and at the right the branch chip, which opens the right
//! panel's Changes tab. A Group names itself — its title, `<project> · N
//! threads`, and `New thread`. There is no layout switch and no PR button.
//!
//! The head is an overlay laid after the titlebar's drag strip
//! (`titlebar::strip`), over the frame the board column already paints
//! (`components::frame_around`): it paints nothing and adds no layout. Only
//! its controls occlude, so a press anywhere else in the row still lands on
//! the strip beneath and drags the window (on Windows, the controls take
//! their region out of the strip's `Drag` hit test the same way).

use super::*;
use crate::components::Tip as _;
use crate::{components, icons, theme};

impl CockpitView {
    /// The head over the board, when the board shows a view to name: a
    /// Thread in Solo, or a Group.
    pub(super) fn view_head(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.empty_board_shown() || self.projects.coordinator_shown() {
            return None;
        }
        // Solo's chip stands 4px in; a Group's button meets its cards' edge.
        let (parts, pad_right) = match self.cockpit.roster().view() {
            View::Solo => (self.solo_head(cx)?, theme::VIEW_HEAD_PAD_R),
            View::Group(group) => (self.group_view_head(group, cx)?, 0.),
        };
        // Windows draws its caption buttons at the row's right end.
        let right = if crate::titlebar::CUSTOM {
            3. * theme::CAPTION_W
        } else {
            theme::BOARD_PAD
        };
        Some(
            div()
                .debug_selector(|| "view-head".into())
                .absolute()
                .top_0()
                .left(px(self.board_left()))
                .right(px(right))
                .h(px(theme::WIN_CHROME_H))
                .flex()
                .items_center()
                .gap(px(theme::VIEW_HEAD_GAP))
                .pl(px(theme::VIEW_HEAD_PAD_L))
                .pr(px(pad_right))
                .font_family(theme::FONT_UI)
                .text_size(px(theme::FS_UI))
                .line_height(px(theme::LH_UI))
                .whitespace_nowrap()
                .children(parts)
                .into_any_element(),
        )
    }

    /// Solo: the focused Thread's title, its Project, a spacer, and the
    /// branch chip while it has a Workspace and a branch to name.
    fn solo_head(&self, cx: &mut Context<Self>) -> Option<Vec<AnyElement>> {
        let thread = self.focused_thread()?;
        let facts = self.facts.get(thread);
        let mut parts = vec![self.view_title(thread, cx)];
        parts.extend(
            facts
                .and_then(|facts| facts.project_label.clone())
                .map(head_meta),
        );
        parts.push(div().flex_1().into_any_element());
        let bound = self
            .cockpit
            .thread(thread)
            .is_some_and(|open| open.workspace().is_some());
        let branch = facts.and_then(|facts| facts.branch.clone());
        if let (true, Some(branch)) = (bound, branch) {
            let totals = facts.and_then(|facts| facts.diff_totals());
            parts.push(self.branch_chip(thread, branch, totals, cx));
        }
        Some(parts)
    }

    /// The Thread's title at the view's weight: double-click renames it,
    /// in place, as a Pane head's title does (`pane_title`).
    fn view_title(&self, thread: ThreadId, cx: &mut Context<Self>) -> AnyElement {
        if let Some((RenameTarget::ViewTitle(editing), editor)) = &self.rename {
            if *editing == thread {
                return rename_field(editor.clone())
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
                    )
                    .into_any_element();
            }
        }
        div()
            .id(("view-title", thread.get() as usize))
            .debug_selector(|| "view-title".into())
            .occlude()
            .min_w_0()
            .truncate()
            .text_size(px(theme::FS_TITLE))
            .font_weight(theme::W_STRONG)
            .text_color(rgb(theme::TEXT_STRONG))
            .child(self.facts.name(thread).clone())
            .tooltip(crate::menu::tooltip("Rename \u{b7} double-click"))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if event.click_count >= 2 {
                        view.start_rename(RenameTarget::ViewTitle(thread), cx);
                    }
                }),
            )
            .into_any_element()
    }

    /// The branch chip: `branch` icon, the branch, and the Thread's summed
    /// diffstat in Geist Mono, on `RAISED`. A click opens the panel's
    /// Changes tab (`show_panel_changes`).
    fn branch_chip(
        &self,
        thread: ThreadId,
        branch: SharedString,
        totals: Option<(usize, usize)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let stat = |text: String, ink: u32| {
            div()
                .flex_shrink_0()
                .font_family(theme::FONT_CODE)
                .text_size(px(theme::FS_META))
                .text_color(rgb(ink))
                .child(text)
        };
        let mut chip = components::faded_button(
            ("branch-chip", thread.get() as usize),
            theme::paint::RAISED.into(),
            theme::paint::SELECTED.into(),
            theme::paint::PRESS.into(),
            rgb(theme::TEXT).into(),
            cx,
        )
        .debug_selector(|| "branch-chip".into())
        .flex_shrink_0()
        .h(px(theme::CONTROL_H))
        .px(px(theme::CONTROL_CHIP_PAD_X))
        .rounded(px(theme::R_CONTROL))
        .accessibility_label("Changes")
        .tip("Changes");
        let mut row = div()
            .flex()
            .items_center()
            .gap(px(theme::CONTROL_CHIP_GAP))
            .child(icons::icon(
                icons::BRANCH,
                theme::CONTROL_CHIP_ICON,
                theme::TEXT_DIM,
            ))
            .child(
                div()
                    .debug_selector(|| "branch-chip-name".into())
                    .min_w_0()
                    .truncate()
                    .text_size(px(theme::FS_LABEL))
                    .text_color(rgb(theme::TEXT))
                    .child(branch),
            );
        if let Some((added, removed)) = totals {
            row = row
                .child(stat(format!("+{added}"), theme::RUNNING))
                .child(stat(format!("\u{2212}{removed}"), theme::BLOCKED));
        }
        chip = chip.child(components::tabular(row));
        control(
            chip.on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.show_panel_changes(thread, cx);
            })),
        )
    }

    /// The branch chip's act (SPEC §5.2): open the right panel on its
    /// Changes tab, or toggle the panel when it already shows Changes.
    ///
    /// SEAM for phase 7a: the panel does not exist yet, so this does
    /// nothing. When the panel lands, this body becomes "if the panel is
    /// shown on Changes, hide it; else show it on Changes" for `thread`.
    pub(super) fn show_panel_changes(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let _ = (thread, cx);
    }

    /// A Group: its title, `<project> · N threads`, a spacer, and `New
    /// thread`, which starts a draft in this Group.
    fn group_view_head(
        &self,
        group: ferrite_core::groups::GroupId,
        cx: &mut Context<Self>,
    ) -> Option<Vec<AnyElement>> {
        let found = self.cockpit.groups().get(group)?;
        let count = found.members.len();
        // The Project, when every member that names one names the same.
        let mut labels = found
            .members
            .iter()
            .filter_map(|member| self.facts.get(*member)?.project_label.clone());
        let project = labels
            .next()
            .filter(|first| labels.all(|label| label == *first));
        let threads = format!("{count} thread{}", if count == 1 { "" } else { "s" });
        let meta = match project {
            Some(project) => format!("{project} \u{b7} {threads}"),
            None => threads,
        };
        let title = div()
            .debug_selector(|| "view-title".into())
            .min_w_0()
            .truncate()
            .text_size(px(theme::FS_TITLE))
            .font_weight(theme::W_STRONG)
            .text_color(rgb(theme::TEXT_STRONG))
            .child(SharedString::from(found.title.clone()))
            .into_any_element();
        let new = control(
            components::secondary_button(
                ("group-new-thread", group.get() as usize),
                Some(icons::PLUS),
                "New thread",
                cx,
            )
            .debug_selector(|| "group-new-thread".into())
            .flex_shrink_0()
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.open_draft_with_placement(DraftTarget::Main, DraftPlacement::CurrentGroup, cx);
            })),
        );
        Some(vec![
            title,
            head_meta(meta.into()),
            div().flex_1().into_any_element(),
            new,
        ])
    }
}

/// The head's second word, `TEXT_DIM`: the Project, or a Group's tally.
fn head_meta(text: SharedString) -> AnyElement {
    div()
        .flex_shrink_0()
        .text_color(rgb(theme::TEXT_DIM))
        .child(text)
        .into_any_element()
}

/// A control in the head row: it takes its box out of the drag strip
/// beneath (`occlude`), so a press there is the control's alone.
fn control(button: impl IntoElement) -> AnyElement {
    div()
        .flex_shrink_0()
        .occlude()
        .child(button)
        .into_any_element()
}
