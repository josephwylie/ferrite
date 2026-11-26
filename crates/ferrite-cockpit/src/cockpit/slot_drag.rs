//! A board slot dragged by its head, and native files dragged over a Pane:
//! what the board dims, washes and sizes a ghost by while a drag lasts. A
//! drag's own; nothing outlives its release, and nothing is saved.

use std::cell::RefCell;
use std::collections::HashMap;

use ferrite_core::layout::Zone;
use ferrite_core::roster::PaneIdentity;

use super::Leaf;

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
