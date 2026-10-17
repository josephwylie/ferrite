//! Where new tasks go (ADR 0031). The board files a task, and sends the
//! Coordinator your words, from its folder (`Ctx::cwd`), whose repo its
//! header names (upstream's `docs/several-boards.md`). A terminal board
//! takes the folder of the pane it opened in; Ferrite's opens at a nav
//! Project's root and says so in a strip over the board — `New tasks go
//! to: <Project ▾>` — whose picker moves the folder live. The list, the
//! selection and the box stay as they are: every repo's tasks still show.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, Context, SharedString};

use super::BoardView;
use crate::components;
use crate::theme::{CH, FS_UI, ICON_CHEVRON, LH_UI, ROW, TEXT, TEXT_MUTED};

/// A nav Project new tasks can go to.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Target {
    pub title: SharedString,
    pub root: PathBuf,
    /// The root as the operator reads it (`~/code/ferrite`).
    pub shown: SharedString,
}

/// The strip's part of the board view.
#[derive(Default)]
pub(super) struct Aim {
    pub targets: Vec<Target>,
    pub open: bool,
    /// Aimed before the board was read: applied once it is.
    pub early: Option<String>,
}

impl Aim {
    /// The strip's height: none without Projects to pick from.
    pub fn height(&self) -> f32 {
        if self.targets.is_empty() {
            0.0
        } else {
            ROW
        }
    }
}

/// The Project `cwd` is in: the one whose root holds it, the nearest where
/// roots nest.
pub(crate) fn target_of(targets: &[Target], cwd: &Path) -> Option<usize> {
    targets
        .iter()
        .enumerate()
        .filter(|(_, target)| cwd.starts_with(&target.root))
        .max_by_key(|(_, target)| target.root.components().count())
        .map(|(at, _)| at)
}

impl BoardView {
    /// The Projects the picker offers, read again each time it shows.
    pub(crate) fn set_targets(&mut self, targets: Vec<Target>) {
        self.aim.targets = targets;
    }

    /// New tasks go to `root` from now on.
    pub(crate) fn aim_at(&mut self, root: &Path, cx: &mut Context<Self>) {
        let cwd = root.display().to_string();
        match self.state() {
            Some(state) => state.ctx.cwd = cwd,
            None => self.aim.early = Some(cwd),
        }
        self.follow_selection();
        cx.notify();
    }

    /// The folder new tasks go from.
    pub(crate) fn cwd(&self) -> Option<&str> {
        self.board
            .as_ref()
            .map(|board| board.state.ctx.cwd.as_str())
            .or(self.aim.early.as_deref())
    }

    /// Which Project the strip names: the one the board's folder is in
    /// (the board may move itself to a draft's repo).
    pub(crate) fn aimed(&self) -> Option<usize> {
        target_of(&self.aim.targets, Path::new(self.cwd()?))
    }

    /// `New tasks go to: <Project ▾>`, a cell in from the edge as the
    /// header's name is; none without Projects to pick from.
    pub(super) fn aim_strip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.aim.targets.is_empty() {
            return None;
        }
        let aimed = self.aimed();
        let name: SharedString = match aimed {
            Some(at) => self.aim.targets[at].title.clone(),
            None => self
                .cwd()
                .and_then(|cwd| Path::new(cwd).file_name())
                .map(|leaf| leaf.to_string_lossy().into_owned().into())
                .unwrap_or_default(),
        };
        let choices = self
            .aim
            .targets
            .iter()
            .enumerate()
            .map(|(at, target)| components::Choice {
                label: target.title.clone(),
                checked: aimed == Some(at),
                detail: Some(target.shown.clone()),
                ..Default::default()
            })
            .collect();
        let roots: Rc<Vec<PathBuf>> = Rc::new(
            self.aim
                .targets
                .iter()
                .map(|target| target.root.clone())
                .collect(),
        );
        let weak = cx.entity().downgrade();
        let picking = weak.clone();
        let trigger = components::button("board-aim")
            .debug_selector(|| "board-aim".into())
            .h(px(ROW))
            .px(px(CH))
            .accessibility_label("New tasks go to")
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(CH / 2.0))
                    // The board's own size: the kit's small button is smaller.
                    .text_size(px(FS_UI))
                    .line_height(px(LH_UI))
                    .text_color(rgb(TEXT))
                    .child(name)
                    .child(crate::icons::icon(
                        crate::icons::CHEVRON_DOWN,
                        ICON_CHEVRON,
                        TEXT_MUTED,
                    )),
            );
        let picker = components::ChoiceMenu {
            id: "board-aim".into(),
            // The strip runs across the board's top: the list drops below it.
            anchor: gpui::Anchor::TopLeft,
            trigger,
            choices,
            open: self.aim.open,
            return_focus: self.focus.clone(),
            on_open: Rc::new(move |open, _, cx| {
                let _ = weak.update(cx, |view, cx| {
                    view.aim.open = open;
                    cx.notify();
                });
            }),
            on_pick: Rc::new(move |at, _, cx| {
                let Some(root) = roots.get(at).cloned() else {
                    return;
                };
                let _ = picking.update(cx, |view, cx| view.aim_at(&root, cx));
            }),
        };
        Some(
            components::text_ui()
                .debug_selector(|| "board-aim-strip".into())
                .id("board-aim-strip")
                .flex()
                .flex_shrink_0()
                .items_center()
                .h(px(ROW))
                .pl(px(CH))
                .text_color(rgb(TEXT_MUTED))
                .whitespace_nowrap()
                // Its presses are its own, not a click on the board's cells.
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child("New tasks go to:")
                .child(picker)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(root: &str) -> Target {
        Target {
            title: root.into(),
            root: root.into(),
            shown: root.into(),
        }
    }

    #[test]
    fn a_folder_is_in_the_nearest_project_that_holds_it() {
        let targets = [
            target("/code/a"),
            target("/code/a/vendor"),
            target("/code/b"),
        ];
        let of = |cwd: &str| target_of(&targets, Path::new(cwd));
        assert_eq!(of("/code/a"), Some(0));
        assert_eq!(of("/code/a/.worktrees/x"), Some(0));
        assert_eq!(of("/code/a/vendor/lib"), Some(1));
        assert_eq!(of("/code/ab"), None, "a path, not a prefix of letters");
        assert_eq!(of("/home"), None);
    }
}
