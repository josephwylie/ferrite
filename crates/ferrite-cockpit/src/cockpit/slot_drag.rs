//! A board slot dragged by its head, and native files dragged over a Pane:
//! what the board dims, washes and sizes a ghost by while a drag lasts. A
//! drag's own; nothing outlives its release, and nothing is saved.

use std::cell::RefCell;
use std::collections::HashMap;

use ferrite_core::layout::Zone;
use gpui::prelude::*;
use gpui::{div, px, rgb, rgba, Context, Pixels, Render, SharedString, Window};
use ferrite_core::roster::PaneIdentity;

use super::beside::Leaf;

#[derive(Default)]
pub(super) struct SlotDrag {
    /// The board slot being dragged: it dims in place while its ghost
    /// moves, until the release.
    lifted: Option<Leaf>,
    /// The slot a release would fill and what it would do there, for the
    /// preview wash.
    preview: Option<(Leaf, Zone)>,
    /// Every board slot's size as this frame laid it out, for the ghost a
    /// drag of it shows. Written while the board draws.
    sizes: RefCell<HashMap<Leaf, (f32, f32)>>,
    /// The Pane native files were last dragged over. Read only while a drag
    /// is live (`hovered_by_files`), so a drag that leaves the window
    /// leaves no sheet behind.
    files_over: Option<PaneIdentity>,
}

impl SlotDrag {
    /// The slot being dragged, while one is.
    pub(super) fn lifted(&self) -> Option<Leaf> {
        self.lifted
    }

    /// The slot a release would fill, and how.
    pub(super) fn preview(&self) -> Option<(Leaf, Zone)> {
        self.preview
    }

    /// Slot `source` moved with the pointer: it is the one lifted. Whether
    /// that is news.
    pub(super) fn lift(&mut self, source: Leaf) -> bool {
        let news = self.lifted != Some(source);
        self.lifted = Some(source);
        news
    }

    /// What a release where the pointer is would do. Whether that changed.
    pub(super) fn set_preview(&mut self, preview: Option<(Leaf, Zone)>) -> bool {
        let changed = self.preview != preview;
        self.preview = preview;
        changed
    }

    /// The pointer has left `target`'s slot: its preview goes, if it is the
    /// one standing. Whether it was.
    pub(super) fn left(&mut self, target: Leaf) -> bool {
        let stood = self
            .preview
            .is_some_and(|(previewed, _)| previewed == target);
        if stood {
            self.preview = None;
        }
        stood
    }

    /// The release on `target`'s slot ends the drag: what the last move
    /// said a release there does, if that move was over `target`.
    pub(super) fn drop_on(&mut self, target: Leaf) -> Option<Zone> {
        self.lifted = None;
        self.preview
            .take()
            .filter(|(previewed, _)| *previewed == target)
            .map(|(_, zone)| zone)
    }

    /// A release anywhere ends the drag. Whether there was one to end.
    pub(super) fn release(&mut self) -> bool {
        self.preview.take().is_some() | self.lifted.take().is_some()
    }

    /// The slots' sizes as this frame's board laid them out; none when no
    /// tree lays out slots.
    pub(super) fn laid_out(&self, sizes: impl IntoIterator<Item = (Leaf, (f32, f32))>) {
        *self.sizes.borrow_mut() = sizes.into_iter().collect();
    }

    /// The size `leaf`'s slot was laid out at this frame.
    pub(super) fn size(&self, leaf: Leaf) -> Option<(f32, f32)> {
        self.sizes.borrow().get(&leaf).copied()
    }

    /// Native files moved onto `pane` (`over`) or off it. Whether the Pane
    /// they hover changed.
    pub(super) fn files_moved(&mut self, pane: PaneIdentity, over: bool) -> bool {
        let next = match (over, self.files_over) {
            (true, _) => Some(pane),
            (false, Some(current)) if current == pane => None,
            (false, current) => current,
        };
        let changed = self.files_over != next;
        self.files_over = next;
        changed
    }

    /// The files were dropped: they hover no Pane.
    pub(super) fn files_dropped(&mut self) {
        self.files_over = None;
    }

    /// Whether native files hover `pane` right now: the last Pane their
    /// drag moved over, while that drag is still live.
    pub(super) fn hovered_by_files(&self, pane: PaneIdentity, cx: &gpui::App) -> bool {
        cx.has_active_drag() && self.files_over == Some(pane)
    }

    /// The Pane native files were last dragged over.
    #[cfg(test)]
    pub(super) fn files_over(&self) -> Option<PaneIdentity> {
        self.files_over
    }
}

/// What a dragged slot's ghost draws: its name, the line under it (a
/// Pane's branch, a reader's kind), and the size the slot has on the board.
#[derive(Clone)]
pub(super) struct GhostFace {
    pub(super) title: SharedString,
    pub(super) detail: Option<SharedString>,
    pub(super) reader: bool,
    pub(super) size: (f32, f32),
}

/// The longest a ghost gets on either side: a big Pane shrinks to a card
/// the board stays readable around, keeping the slot's proportions.
pub(super) const GHOST_MAX_W: f32 = 300.0;

pub(super) const GHOST_MAX_H: f32 = 200.0;

/// A ghost's skeleton lines, as shares of its body's width.
pub(super) const GHOST_LINES: [f32; 4] = [0.82, 0.64, 0.74, 0.46];

/// The ghost that follows the pointer while a slot is dragged: a scaled,
/// see-through miniature of the slot, held at the point it was grabbed.
pub(super) struct PaneGhost {
    pub(super) face: GhostFace,
    /// Where the pointer was in the grabbed handle, which gpui keeps under
    /// the pointer. The miniature is shifted so the same point of the
    /// scaled slot sits there instead.
    pub(super) grab: gpui::Point<Pixels>,
}

impl PaneGhost {
    fn scale(&self) -> f32 {
        let (w, h) = self.face.size;
        if w <= 0.0 || h <= 0.0 {
            return 1.0;
        }
        (GHOST_MAX_W / w).min(GHOST_MAX_H / h).min(1.0)
    }
}

impl Render for PaneGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        use ferrite_ui::theme;
        let scale = self.scale();
        let (w, h) = self.face.size;
        let (w, h) = ((w * scale).max(160.0), (h * scale).max(90.0));
        let shift = |at: Pixels| px(f32::from(at) * (1.0 - scale));
        // A miniature Pane in the board's own recipe: its head line (the
        // mark in the glyph column, the title in `W_LABEL`, the branch as
        // meta), a hairline, then a few quiet skeleton lines for its body.
        let mut body = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .gap(px(theme::SPACE_2))
            .px(px(theme::PANE_PAD_X))
            .py(px(theme::SPACE_3));
        for share in GHOST_LINES {
            body = body.child(
                div()
                    .h(px(theme::SPACE_1_5))
                    .w(gpui::relative(share))
                    .bg(theme::paint::SELECTED),
            );
        }
        let marker = if self.face.reader {
            ferrite_ui::icons::icon(ferrite_ui::icons::FILE, theme::ROW_ICON, theme::INK_SUBTLE)
                .into_any_element()
        } else {
            ferrite_ui::components::status_dot(theme::INK_SUBTLE).into_any_element()
        };
        let head = div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(theme::SPACE_2))
            .h(px(theme::PANE_HEAD_H))
            .px(px(theme::HEAD_PAD_X))
            .border_b_1()
            .border_color(theme::paint::LINE)
            .font_family(theme::FONT_UI)
            .child(marker)
            .child(
                div()
                    .min_w_0()
                    .flex_shrink(1.)
                    .truncate()
                    .font_weight(theme::W_LABEL)
                    .text_size(px(theme::FS_UI))
                    .line_height(px(theme::LH_UI))
                    .text_color(rgb(theme::INK_STRONG))
                    .child(self.face.title.clone()),
            )
            .children(self.face.detail.clone().map(|detail| {
                div()
                    .min_w_0()
                    .flex_shrink(1.)
                    .truncate()
                    .text_size(px(theme::FS_UI))
                    .line_height(px(theme::LH_UI))
                    .text_color(rgb(theme::INK_SUBTLE))
                    .child(detail)
            }));
        let card = div()
            .flex()
            .flex_col()
            .w(px(w))
            .h(px(h))
            .overflow_hidden()
            .border_1()
            .border_color(rgba(theme::FOCUS_RING))
            .bg(theme::paint::FLOAT)
            .shadow(ferrite_ui::components::elevation(
                ferrite_ui::components::Elevation::Float,
            ))
            .child(head)
            .child(body);
        // Lifted off the board, the miniature fades up to its see-through
        // ink rather than popping in under the pointer.
        let card = ferrite_ui::motion::fade_in(
            "pane-ghost-in",
            card,
            ferrite_ui::motion::FADE_QUICK,
            theme::DRAG_GHOST_OPACITY,
        );
        div()
            .pl(shift(self.grab.x))
            .pt(shift(self.grab.y))
            .child(card)
    }
}
