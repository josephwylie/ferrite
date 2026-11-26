//! Each board's split tree as the operator arranged it, and the seam drag
//! that rearranges it. Session-only: a reader's place is remembered until
//! quit. An arrangement is kept on a seam's release or a slot's drop, never
//! per move.

use std::collections::{BTreeSet, HashMap};

use ferrite_core::layout::{self, Edge, SeamId, Tree};
use ferrite_core::roster::PaneIdentity;
use ferrite_core::ThreadKey;

use super::{pane_leaf, reader_leaf, Board, Leaf, READER_SHARE};

#[derive(Default)]
pub(super) struct BoardLayouts {
    /// Boards whose arrangement includes a reader slot, as the operator
    /// left them.
    kept: HashMap<Board, Tree>,
    /// A seam being dragged: the board, the seam, and the tree as it
    /// stands mid-drag.
    seam_drag: Option<SeamDrag>,
}

/// A seam mid-drag.
struct SeamDrag {
    board: Board,
    seam: SeamId,
    tree: Tree,
}

impl BoardLayouts {
    /// `board`'s tree as it should draw right now: the one mid-drag, else
    /// the operator's arrangement fitted to what is open — the Pane, and
    /// the reader slot of each of `readers` (the Threads whose document is
    /// open or sliding shut), asked only when no drag answers. A reader
    /// the tree has not placed yet opens beside its Pane, on the right.
    /// Every slot is at rest here: `slid_tree` draws a slide.
    pub(super) fn tree(&self, board: Board, readers: impl FnOnce() -> Vec<ThreadKey>) -> Tree {
        if let Some(drag) = self.seam_drag.as_ref().filter(|drag| drag.board == board) {
            return drag.tree.clone();
        }
        let readers = readers();
        let base = self.kept.get(&board).cloned().unwrap_or_default();
        let mut panes = vec![pane_leaf(board.0)];
        let readers: Vec<(Leaf, Leaf)> = readers
            .into_iter()
            .map(|owner| (pane_leaf(PaneIdentity::Thread(owner)), reader_leaf(owner)))
            .filter(|(owner, _)| panes.contains(owner))
            .collect();
        let mut tree = base;
        let wanted: BTreeSet<Leaf> = panes
            .iter()
            .copied()
            .chain(readers.iter().map(|(_, reader)| *reader))
            .collect();
        for stale in tree
            .leaves()
            .into_iter()
            .filter(|leaf| !wanted.contains(leaf))
        {
            tree.remove(stale);
        }
        panes.retain(|leaf| !tree.contains(*leaf));
        for leaf in panes {
            tree.insert(leaf);
        }
        for (owner, reader) in readers {
            if !tree.contains(reader) {
                tree.split_share(owner, Edge::Right, reader, READER_SHARE);
            }
        }
        tree
    }

    /// Keep an arrangement the operator made on a board, for the session.
    pub(super) fn commit(&mut self, board: Board, tree: Tree) {
        self.kept.insert(board, tree);
    }

    /// A press on `seam`: the drag holds `tree` until the release.
    pub(super) fn begin_seam_drag(&mut self, board: Board, seam: SeamId, tree: Tree) {
        self.seam_drag = Some(SeamDrag { board, seam, tree });
    }

    /// Whether a seam is held.
    pub(super) fn seam_held(&self) -> bool {
        self.seam_drag.is_some()
    }

    /// Whether the seam held is on `board` and one of `seams`.
    pub(super) fn holds_seam<'a>(&self, board: Board, mut seams: impl Iterator<Item = &'a SeamId>) -> bool {
        self.seam_drag
            .as_ref()
            .is_some_and(|drag| drag.board == board && seams.any(|seam| *seam == drag.seam))
    }

    /// The pointer moved to `pointer` with a seam held, on a board laid out
    /// in `bounds`: the seam follows it, and only the two Panes touching
    /// it change size. Whether the tree moved.
    pub(super) fn drag_seam(&mut self, bounds: layout::Rect, pointer: layout::Point) -> bool {
        let Some(drag) = self.seam_drag.as_mut() else {
            return false;
        };
        drag.tree
            .drag_linked(&drag.seam, bounds, pointer, ferrite_ui::theme::BOARD_SEAM)
    }

    /// The release: the dragged tree is kept, once. Whether a seam was
    /// held.
    pub(super) fn end_seam_drag(&mut self) -> bool {
        let Some(drag) = self.seam_drag.take() else {
            return false;
        };
        self.commit(drag.board, drag.tree);
        true
    }
}
