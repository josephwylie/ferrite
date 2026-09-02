//! A Group's Pane layout: a binary split tree.
//!
//! SwarmDeck parity — the shape is react-mosaic's `MosaicNode`: a leaf is one
//! Pane, a split hands its area to two children along an axis, `first` taking
//! `ratio` of it. One tree is persisted per Group and drives three things: the
//! rect each Pane paints in, seam drags (resizing the two sides of a split),
//! and dropping one Pane on another (swap at the centre, split at an edge).
//!
//! Geometry is plain `f32` in the caller's units; nothing here knows gpui.
//! Paths address nodes: `false` steps into `first`, `true` into `second`.

use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ThreadId;

/// The smallest share of a split either side can be dragged down to.
pub const MIN_SHARE: f32 = 0.2;

/// The drop core (the "swap" square at a Pane's centre) is this share of the
/// Pane's shorter side, never under `CORE_FLOOR` px, never over
/// `CORE_CEILING` of that side — so a small Pane keeps edge strips to split
/// into.
const CORE_SHARE: f32 = 0.30;
const CORE_FLOOR: f32 = 44.0;
const CORE_CEILING: f32 = 0.70;

/// The bounds a tree's proportions are judged in when no screen is involved.
const UNIT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    w: 1.0,
    h: 1.0,
};

/// Which way a split lays its children out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    /// `first` and `second` side by side; the seam between them is vertical.
    Row,
    /// `first` above `second`; the seam is horizontal.
    Column,
}

/// A side of a Pane's rect — where a dropped Pane lands when it splits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Node {
    Leaf(#[serde(with = "thread_id")] ThreadId),
    Split {
        axis: Axis,
        /// `first`'s share of the split's area, strictly inside 0..1.
        ratio: f32,
        first: Box<Node>,
        second: Box<Node>,
    },
}

/// One Group's arrangement. `root: None` is the empty layout.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Tree {
    #[serde(default)]
    pub root: Option<Node>,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.w
            && point.y >= self.y
            && point.y <= self.y + self.h
    }

    fn area(&self) -> f32 {
        self.w * self.h
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// A seam between two siblings, addressed by the path to its Split node
/// (`false` = first, `true` = second at each step).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SeamId(pub Vec<bool>);

#[derive(Clone, Debug, PartialEq)]
pub struct Seam {
    pub id: SeamId,
    pub axis: Axis,
    /// The band the pointer grabs: the gap between the two sides, widened to
    /// at least `grab` px centred on the gap. It may overlap the Panes'
    /// edges, so hit-test seams before Panes.
    pub band: Rect,
    /// The whole area the split governs.
    pub area: Rect,
}

/// What a drop at a point on a Pane means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Swap,
    Split(Edge),
}

impl Tree {
    /// A near-square grid: `rows = max(1, floor(sqrt(n)))`, `cols =
    /// ceil(n / rows)`, filled row by row. Rows stack in a Column at equal
    /// shares; each row is a Row at equal shares (right-nested, the head
    /// taking `1 / remaining`). A member listed twice is packed once.
    pub fn even(members: &[ThreadId]) -> Tree {
        let mut seen = BTreeSet::new();
        let members: Vec<ThreadId> = members
            .iter()
            .copied()
            .filter(|member| seen.insert(*member))
            .collect();
        if members.is_empty() {
            return Tree::default();
        }
        let rows = ((members.len() as f64).sqrt().floor() as usize).max(1);
        let columns = members.len().div_ceil(rows);
        let rows: Vec<Node> = members
            .chunks(columns)
            .map(|row| even_chain(row.iter().map(|id| Node::Leaf(*id)).collect(), Axis::Row))
            .collect();
        Tree {
            root: Some(even_chain(rows, Axis::Column)),
        }
    }

    /// Every leaf in DFS order, duplicates included so a caller can see them.
    pub fn leaves(&self) -> Vec<ThreadId> {
        let mut out = Vec::new();
        if let Some(root) = &self.root {
            root.collect(&mut out);
        }
        out
    }

    pub fn contains(&self, id: ThreadId) -> bool {
        self.root
            .as_ref()
            .is_some_and(|root| root.path_to(id, &mut Vec::new()))
    }

    /// A duplicate leaf, or a ratio outside (0, 1) or NaN. An empty tree is
    /// sound.
    pub fn is_corrupt(&self) -> bool {
        self.root
            .as_ref()
            .is_some_and(|root| !root.is_sound(&mut BTreeSet::new()))
    }

    /// Add a Pane: the empty tree becomes its leaf; otherwise the largest
    /// leaf (by area at unit bounds; the first in DFS order on a tie) splits
    /// 50/50 along its longer side, the newcomer second. A present id is a
    /// no-op.
    pub fn insert(&mut self, id: ThreadId) {
        if self.contains(id) {
            return;
        }
        let Some(root) = &self.root else {
            self.root = Some(Node::Leaf(id));
            return;
        };
        let mut largest: Option<(Vec<bool>, Rect)> = None;
        root.walk(UNIT, 0.0, &mut Vec::new(), &mut |node, path, area| {
            if matches!(node, Node::Leaf(_))
                && largest
                    .as_ref()
                    .is_none_or(|(_, best)| area.area() > best.area())
            {
                largest = Some((path.to_vec(), area));
            }
        });
        let (path, area) = largest.expect("a non-empty tree has a leaf");
        let axis = if area.w >= area.h {
            Axis::Row
        } else {
            Axis::Column
        };
        let slot = self
            .node_mut(&path)
            .expect("the path was read off this tree");
        let existing = std::mem::replace(slot, Node::Leaf(id));
        *slot = Node::Split {
            axis,
            ratio: 0.5,
            first: Box::new(existing),
            second: Box::new(Node::Leaf(id)),
        };
    }

    /// Drop a Pane; its sibling collapses up into the split's slot. The last
    /// leaf leaves the tree empty. False when the id was not present.
    pub fn remove(&mut self, id: ThreadId) -> bool {
        if !self.contains(id) {
            return false;
        }
        self.root = self.root.take().and_then(|root| root.without(id));
        true
    }

    /// Exchange two leaves; the shape and every ratio stay. False when either
    /// is absent or they are the same Pane.
    pub fn swap(&mut self, a: ThreadId, b: ThreadId) -> bool {
        if a == b || !self.contains(a) || !self.contains(b) {
            return false;
        }
        if let Some(root) = &mut self.root {
            root.exchange(a, b);
        }
        true
    }

    /// Move `source` out of wherever it is (if anywhere) and split `target`'s
    /// slot 50/50 so the two share it, `source` on the `edge` side. False when
    /// the target is missing or is the source.
    pub fn split(&mut self, target: ThreadId, edge: Edge, source: ThreadId) -> bool {
        if target == source || !self.contains(target) {
            return false;
        }
        self.remove(source);
        let mut path = Vec::new();
        let root = self.root.as_ref().expect("the target is still here");
        assert!(root.path_to(target, &mut path));
        let slot = self.node_mut(&path).expect("the path was just found");
        let axis = match edge {
            Edge::Left | Edge::Right => Axis::Row,
            Edge::Top | Edge::Bottom => Axis::Column,
        };
        let (first, second) = match edge {
            Edge::Left | Edge::Top => (source, target),
            Edge::Right | Edge::Bottom => (target, source),
        };
        *slot = Node::Split {
            axis,
            ratio: 0.5,
            first: Box::new(Node::Leaf(first)),
            second: Box::new(Node::Leaf(second)),
        };
        true
    }

    /// Fit the tree to exactly `members`: a corrupt or empty tree is rebuilt
    /// as the even grid; otherwise stale leaves go and missing members come
    /// in (in `members` order), keeping the operator's shape. Idempotent.
    pub fn reconcile(&mut self, members: &[ThreadId]) {
        if self.root.is_none() || self.is_corrupt() {
            *self = Tree::even(members);
            return;
        }
        let wanted: BTreeSet<ThreadId> = members.iter().copied().collect();
        for stale in self
            .leaves()
            .into_iter()
            .filter(|leaf| !wanted.contains(leaf))
        {
            self.remove(stale);
        }
        if self.root.is_none() {
            *self = Tree::even(members);
            return;
        }
        for member in members {
            self.insert(*member);
        }
    }

    /// Set a split's ratio, clamped to `MIN_SHARE..=1 - MIN_SHARE`. False
    /// when the path names no Split (or the ratio is NaN).
    pub fn set_ratio(&mut self, seam: &SeamId, ratio: f32) -> bool {
        if ratio.is_nan() {
            return false;
        }
        let Some(Node::Split { ratio: slot, .. }) = self.node_mut(&seam.0) else {
            return false;
        };
        *slot = clamp_share(ratio);
        true
    }

    /// The ratio that puts the seam's centre under `pointer`, within the
    /// split's own area and the share limits — what a drag feeds to
    /// `set_ratio`. A split too narrow to share reports its current ratio.
    pub fn ratio_for(&self, seam: &SeamId, bounds: Rect, pointer: Point, gap: f32) -> Option<f32> {
        let (node, area) = self.locate(seam, bounds, gap)?;
        let Node::Split { axis, ratio, .. } = node else {
            return None;
        };
        let (offset, length) = match axis {
            Axis::Row => (pointer.x - area.x, area.w),
            Axis::Column => (pointer.y - area.y, area.h),
        };
        let usable = length - gap;
        if usable <= 0.0 {
            return Some(*ratio);
        }
        Some(clamp_share((offset - gap / 2.0) / usable))
    }

    /// Every leaf's rect in DFS order. A split shares its length minus the
    /// gap by ratio and pushes `second` past the gap; a single leaf fills the
    /// bounds.
    pub fn rects(&self, bounds: Rect, gap: f32) -> Vec<(ThreadId, Rect)> {
        let mut out = Vec::new();
        if let Some(root) = &self.root {
            root.walk(bounds, gap, &mut Vec::new(), &mut |node, _, area| {
                if let Node::Leaf(id) = node {
                    out.push((*id, area));
                }
            });
        }
        out
    }

    /// One seam per split, DFS order (a split before the seams inside it).
    pub fn seams(&self, bounds: Rect, gap: f32, grab: f32) -> Vec<Seam> {
        let mut out = Vec::new();
        if let Some(root) = &self.root {
            root.walk(bounds, gap, &mut Vec::new(), &mut |node, path, area| {
                if let Node::Split { axis, ratio, .. } = node {
                    out.push(Seam {
                        id: SeamId(path.to_vec()),
                        axis: *axis,
                        band: seam_band(*axis, *ratio, area, gap, grab),
                        area,
                    });
                }
            });
        }
        out
    }

    /// The node a path names and the area it governs.
    fn locate(&self, seam: &SeamId, bounds: Rect, gap: f32) -> Option<(&Node, Rect)> {
        let mut node = self.root.as_ref()?;
        let mut area = bounds;
        for &step in &seam.0 {
            let Node::Split {
                axis,
                ratio,
                first,
                second,
            } = node
            else {
                return None;
            };
            let (first_area, second_area) = halves(*axis, *ratio, area, gap);
            (node, area) = if step {
                (second.as_ref(), second_area)
            } else {
                (first.as_ref(), first_area)
            };
        }
        Some((node, area))
    }

    fn node_mut(&mut self, path: &[bool]) -> Option<&mut Node> {
        let mut node = self.root.as_mut()?;
        for &step in path {
            let Node::Split { first, second, .. } = node else {
                return None;
            };
            node = if step {
                second.as_mut()
            } else {
                first.as_mut()
            };
        }
        Some(node)
    }
}

impl Node {
    fn collect(&self, out: &mut Vec<ThreadId>) {
        match self {
            Node::Leaf(id) => out.push(*id),
            Node::Split { first, second, .. } => {
                first.collect(out);
                second.collect(out);
            }
        }
    }

    fn is_sound(&self, seen: &mut BTreeSet<ThreadId>) -> bool {
        match self {
            Node::Leaf(id) => seen.insert(*id),
            Node::Split {
                ratio,
                first,
                second,
                ..
            } => *ratio > 0.0 && *ratio < 1.0 && first.is_sound(seen) && second.is_sound(seen),
        }
    }

    /// Leave the path to the first leaf equal to `id` in `path`; false (and
    /// `path` as it was) when there is none.
    fn path_to(&self, id: ThreadId, path: &mut Vec<bool>) -> bool {
        match self {
            Node::Leaf(leaf) => *leaf == id,
            Node::Split { first, second, .. } => {
                for (step, child) in [(false, first), (true, second)] {
                    path.push(step);
                    if child.path_to(id, path) {
                        return true;
                    }
                    path.pop();
                }
                false
            }
        }
    }

    /// This node with every leaf equal to `id` gone, siblings collapsing up;
    /// None when nothing is left.
    fn without(self, id: ThreadId) -> Option<Node> {
        match self {
            Node::Leaf(leaf) if leaf == id => None,
            Node::Leaf(_) => Some(self),
            Node::Split {
                axis,
                ratio,
                first,
                second,
            } => match (first.without(id), second.without(id)) {
                (Some(first), Some(second)) => Some(Node::Split {
                    axis,
                    ratio,
                    first: Box::new(first),
                    second: Box::new(second),
                }),
                (Some(survivor), None) | (None, Some(survivor)) => Some(survivor),
                (None, None) => None,
            },
        }
    }

    fn exchange(&mut self, a: ThreadId, b: ThreadId) {
        match self {
            Node::Leaf(leaf) => {
                if *leaf == a {
                    *leaf = b;
                } else if *leaf == b {
                    *leaf = a;
                }
            }
            Node::Split { first, second, .. } => {
                first.exchange(a, b);
                second.exchange(a, b);
            }
        }
    }

    /// Visit every node top-down, DFS, with its path and the area it governs.
    fn walk<'a>(
        &'a self,
        area: Rect,
        gap: f32,
        path: &mut Vec<bool>,
        visit: &mut dyn FnMut(&'a Node, &[bool], Rect),
    ) {
        visit(self, path, area);
        if let Node::Split {
            axis,
            ratio,
            first,
            second,
        } = self
        {
            let (first_area, second_area) = halves(*axis, *ratio, area, gap);
            for (step, child, child_area) in
                [(false, first, first_area), (true, second, second_area)]
            {
                path.push(step);
                child.walk(child_area, gap, path, visit);
                path.pop();
            }
        }
    }
}

/// Right-nested equal shares: the head takes `1 / remaining`.
fn even_chain(mut nodes: Vec<Node>, axis: Axis) -> Node {
    let mut node = nodes.pop().expect("a chain has at least one node");
    let mut count = 1;
    while let Some(head) = nodes.pop() {
        count += 1;
        node = Node::Split {
            axis,
            ratio: 1.0 / count as f32,
            first: Box::new(head),
            second: Box::new(node),
        };
    }
    node
}

fn clamp_share(ratio: f32) -> f32 {
    ratio.clamp(MIN_SHARE, 1.0 - MIN_SHARE)
}

/// The two areas a split hands its children: the length minus the gap is
/// shared by `ratio`, and `second` ends where the area ends — so neither
/// child ever leaves the area, even when the gap is wider than it.
fn halves(axis: Axis, ratio: f32, area: Rect, gap: f32) -> (Rect, Rect) {
    match axis {
        Axis::Row => {
            let usable = (area.w - gap).max(0.0);
            let first = usable * ratio;
            let second = usable - first;
            (
                Rect { w: first, ..area },
                Rect {
                    x: area.x + area.w - second,
                    w: second,
                    ..area
                },
            )
        }
        Axis::Column => {
            let usable = (area.h - gap).max(0.0);
            let first = usable * ratio;
            let second = usable - first;
            (
                Rect { h: first, ..area },
                Rect {
                    y: area.y + area.h - second,
                    h: second,
                    ..area
                },
            )
        }
    }
}

fn seam_band(axis: Axis, ratio: f32, area: Rect, gap: f32, grab: f32) -> Rect {
    let (first, second) = halves(axis, ratio, area, gap);
    match axis {
        Axis::Row => {
            let (start, end) = (first.x + first.w, second.x);
            let width = (end - start).max(grab);
            Rect {
                x: (start + end) / 2.0 - width / 2.0,
                w: width,
                ..area
            }
        }
        Axis::Column => {
            let (start, end) = (first.y + first.h, second.y);
            let height = (end - start).max(grab);
            Rect {
                y: (start + end) / 2.0 - height / 2.0,
                h: height,
                ..area
            }
        }
    }
}

/// SwarmDeck's drop rule: a centred square core with side
/// `clamp(0.30 * min(w, h), 44, 0.70 * min(w, h))` (the ceiling wins when
/// the two disagree) reads as Swap; outside it, the nearest edge reads as
/// Split there. Ties go Top, Bottom, Left, Right.
pub fn zone(pointer: Point, rect: Rect) -> Zone {
    let shorter = rect.w.min(rect.h);
    let side = (CORE_SHARE * shorter)
        .max(CORE_FLOOR)
        .min(CORE_CEILING * shorter);
    let half = side / 2.0;
    let dx = (pointer.x - (rect.x + rect.w / 2.0)).abs();
    let dy = (pointer.y - (rect.y + rect.h / 2.0)).abs();
    if dx <= half && dy <= half {
        return Zone::Swap;
    }
    let left = pointer.x - rect.x;
    let right = rect.x + rect.w - pointer.x;
    let top = pointer.y - rect.y;
    let bottom = rect.y + rect.h - pointer.y;
    let nearest = left.min(right).min(top).min(bottom);
    let edge = if nearest == top {
        Edge::Top
    } else if nearest == bottom {
        Edge::Bottom
    } else if nearest == left {
        Edge::Left
    } else {
        Edge::Right
    };
    Zone::Split(edge)
}

/// The preview a drop paints: the whole Pane for a swap, the half the dropped
/// Pane would take for a split.
pub fn zone_rect(rect: Rect, zone: Zone) -> Rect {
    let half_w = rect.w / 2.0;
    let half_h = rect.h / 2.0;
    match zone {
        Zone::Swap => rect,
        Zone::Split(Edge::Left) => Rect { w: half_w, ..rect },
        Zone::Split(Edge::Right) => Rect {
            x: rect.x + half_w,
            w: half_w,
            ..rect
        },
        Zone::Split(Edge::Top) => Rect { h: half_h, ..rect },
        Zone::Split(Edge::Bottom) => Rect {
            y: rect.y + half_h,
            h: half_h,
            ..rect
        },
    }
}

/// Leaves persist as the Thread's number, the way `groups.json` writes
/// members.
mod thread_id {
    use super::*;

    pub fn serialize<S: Serializer>(id: &ThreadId, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(id.get())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<ThreadId, D::Error> {
        u64::deserialize(deserializer).map(ThreadId::new)
    }
}

#[cfg(test)]
mod tests {
    use super::Axis::{Column, Row};
    use super::*;

    fn id(n: u64) -> ThreadId {
        ThreadId::new(n)
    }

    fn ids(range: std::ops::Range<u64>) -> Vec<ThreadId> {
        range.map(ThreadId::new).collect()
    }

    fn leaf(n: u64) -> Node {
        Node::Leaf(id(n))
    }

    fn split(axis: Axis, ratio: f32, first: Node, second: Node) -> Node {
        Node::Split {
            axis,
            ratio,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    fn of(root: Node) -> Tree {
        Tree { root: Some(root) }
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    fn at(x: f32, y: f32) -> Point {
        Point { x, y }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn same(a: Rect, b: Rect) -> bool {
        close(a.x, b.x) && close(a.y, b.y) && close(a.w, b.w) && close(a.h, b.h)
    }

    fn inside(inner: Rect, outer: Rect) -> bool {
        inner.x >= outer.x - 1e-3
            && inner.y >= outer.y - 1e-3
            && inner.x + inner.w <= outer.x + outer.w + 1e-3
            && inner.y + inner.h <= outer.y + outer.h + 1e-3
    }

    fn set(ids: &[ThreadId]) -> BTreeSet<ThreadId> {
        ids.iter().copied().collect()
    }

    fn root_ratio(tree: &Tree) -> f32 {
        match &tree.root {
            Some(Node::Split { ratio, .. }) => *ratio,
            _ => panic!("no split at the root"),
        }
    }

    /// (rows, columns) of an even grid: the Column chain down, the widest
    /// Row chain across.
    fn shape(tree: &Tree) -> (usize, usize) {
        fn chain(node: &Node, axis: Axis) -> Vec<&Node> {
            match node {
                Node::Split {
                    axis: found,
                    first,
                    second,
                    ..
                } if *found == axis => {
                    let mut out = vec![first.as_ref()];
                    out.extend(chain(second, axis));
                    out
                }
                _ => vec![node],
            }
        }
        let root = tree.root.as_ref().expect("a grid");
        let rows = chain(root, Column);
        let columns = rows.iter().map(|row| chain(row, Row).len()).max().unwrap();
        (rows.len(), columns)
    }

    #[test]
    fn even_grids_pack_the_spec_locked_rows_and_columns() {
        let expected = [(1, 1), (1, 2), (1, 3), (2, 2), (2, 3), (2, 3)];
        for (index, want) in expected.iter().enumerate() {
            let members = ids(1..index as u64 + 2);
            let tree = Tree::even(&members);
            assert_eq!(shape(&tree), *want, "n = {}", members.len());
            assert_eq!(tree.leaves(), members);
            assert!(!tree.is_corrupt());
        }
        assert_eq!(Tree::even(&[]), Tree::default());
        assert_eq!(shape(&Tree::even(&ids(1..10))), (3, 3));
        assert_eq!(shape(&Tree::even(&ids(1..101))), (10, 10));
    }

    #[test]
    fn even_grids_hand_every_pane_an_equal_cell() {
        let rects = Tree::even(&ids(1..7)).rects(UNIT, 0.0);
        for (_, cell) in &rects {
            assert!(close(cell.w, 1.0 / 3.0) && close(cell.h, 0.5), "{cell:?}");
        }
        assert!(same(rects[0].1, rect(0.0, 0.0, 1.0 / 3.0, 0.5)));
        assert!(same(rects[3].1, rect(0.0, 0.5, 1.0 / 3.0, 0.5)));
        assert!(same(rects[5].1, rect(2.0 / 3.0, 0.5, 1.0 / 3.0, 0.5)));
        // A short last row stretches across.
        let five = Tree::even(&ids(1..6)).rects(UNIT, 0.0);
        assert!(same(five[3].1, rect(0.0, 0.5, 0.5, 0.5)));
        assert!(same(five[4].1, rect(0.5, 0.5, 0.5, 0.5)));
        // A member listed twice is packed once.
        assert_eq!(Tree::even(&[id(1), id(1), id(2)]).leaves(), [id(1), id(2)]);
    }

    #[test]
    fn insert_splits_the_largest_leaf_along_its_longer_side_with_the_newcomer_second() {
        let mut tree = Tree::default();
        tree.insert(id(1));
        assert_eq!(tree, of(leaf(1)));
        // A square leaf splits as a Row (w >= h).
        tree.insert(id(2));
        assert_eq!(tree, of(split(Row, 0.5, leaf(1), leaf(2))));
        // Both halves are taller than wide and tie on area: the first in DFS
        // order splits, as a Column.
        tree.insert(id(3));
        assert_eq!(
            tree,
            of(split(
                Row,
                0.5,
                split(Column, 0.5, leaf(1), leaf(3)),
                leaf(2)
            ))
        );
        let before = tree.clone();
        tree.insert(id(2));
        assert_eq!(tree, before, "a present id is a no-op");
        // The largest leaf wins over the first.
        let mut wide = of(split(Row, 0.3, leaf(1), leaf(2)));
        wide.insert(id(3));
        assert_eq!(
            wide,
            of(split(
                Row,
                0.3,
                leaf(1),
                split(Column, 0.5, leaf(2), leaf(3))
            ))
        );
        // A wide leaf splits as a Row.
        let mut stack = of(split(Column, 0.5, leaf(1), leaf(2)));
        stack.insert(id(3));
        assert_eq!(
            stack,
            of(split(
                Column,
                0.5,
                split(Row, 0.5, leaf(1), leaf(3)),
                leaf(2)
            ))
        );
    }

    #[test]
    fn remove_collapses_the_sibling_up_and_the_last_leaf_empties_the_tree() {
        let mut tree = of(split(
            Row,
            0.3,
            leaf(1),
            split(Column, 0.6, leaf(2), leaf(3)),
        ));
        assert!(tree.remove(id(2)));
        assert_eq!(tree, of(split(Row, 0.3, leaf(1), leaf(3))));
        assert!(!tree.remove(id(2)));
        assert!(tree.remove(id(1)));
        assert_eq!(tree, of(leaf(3)));
        assert!(tree.remove(id(3)));
        assert_eq!(tree, Tree::default());
        assert!(!tree.remove(id(3)));
    }

    #[test]
    fn swap_exchanges_two_leaves_and_keeps_every_ratio() {
        let mut tree = of(split(
            Row,
            0.3,
            leaf(1),
            split(Column, 0.6, leaf(2), leaf(3)),
        ));
        assert!(tree.swap(id(1), id(3)));
        assert_eq!(
            tree,
            of(split(
                Row,
                0.3,
                leaf(3),
                split(Column, 0.6, leaf(2), leaf(1))
            ))
        );
        let before = tree.clone();
        assert!(!tree.swap(id(1), id(1)));
        assert!(!tree.swap(id(1), id(9)));
        assert_eq!(tree, before);
    }

    #[test]
    fn split_moves_the_source_onto_the_named_edge_of_the_target() {
        let start = of(split(
            Row,
            0.3,
            leaf(1),
            split(Column, 0.6, leaf(2), leaf(3)),
        ));
        let cases = [
            (Edge::Left, split(Row, 0.5, leaf(3), leaf(1))),
            (Edge::Right, split(Row, 0.5, leaf(1), leaf(3))),
            (Edge::Top, split(Column, 0.5, leaf(3), leaf(1))),
            (Edge::Bottom, split(Column, 0.5, leaf(1), leaf(3))),
        ];
        for (edge, slot) in cases {
            let mut tree = start.clone();
            assert!(tree.split(id(1), edge, id(3)));
            assert_eq!(tree, of(split(Row, 0.3, slot, leaf(2))), "{edge:?}");
        }
        // A source not yet in the tree simply lands.
        let mut tree = start.clone();
        assert!(tree.split(id(2), Edge::Top, id(4)));
        assert_eq!(
            tree,
            of(split(
                Row,
                0.3,
                leaf(1),
                split(Column, 0.6, split(Column, 0.5, leaf(4), leaf(2)), leaf(3))
            ))
        );
        let mut tree = start.clone();
        assert!(!tree.split(id(9), Edge::Left, id(1)), "missing target");
        assert!(!tree.split(id(1), Edge::Left, id(1)), "self");
        assert_eq!(tree, start);
    }

    #[test]
    fn reconcile_heals_corrupt_duplicate_and_stale_trees_and_then_holds_still() {
        let members = ids(1..4);
        let mut duplicate = of(split(Row, 0.5, leaf(1), leaf(1)));
        assert!(duplicate.is_corrupt());
        duplicate.reconcile(&members);
        assert_eq!(duplicate, Tree::even(&members));
        for bad in [0.0, 1.0, -0.2, 1.5, f32::NAN] {
            let mut tree = of(split(Row, bad, leaf(1), leaf(2)));
            assert!(tree.is_corrupt(), "ratio {bad}");
            tree.reconcile(&members);
            assert_eq!(tree, Tree::even(&members));
        }
        let mut empty = Tree::default();
        empty.reconcile(&members);
        assert_eq!(empty, Tree::even(&members));
        // Stale leaves go, missing members come, the operator's shape stays.
        let mut stale = of(split(
            Row,
            0.3,
            leaf(1),
            split(Column, 0.6, leaf(2), leaf(9)),
        ));
        stale.reconcile(&members);
        assert_eq!(
            stale,
            of(split(
                Row,
                0.3,
                leaf(1),
                split(Column, 0.5, leaf(2), leaf(3))
            ))
        );
        let again = stale.clone();
        stale.reconcile(&members);
        assert_eq!(stale, again, "idempotent");
        let mut gone = of(split(Row, 0.5, leaf(8), leaf(9)));
        gone.reconcile(&members);
        assert_eq!(gone, Tree::even(&members), "all leaves stale");
        let mut tree = Tree::even(&members);
        tree.reconcile(&[]);
        assert_eq!(tree, Tree::default(), "no members");
    }

    #[test]
    fn set_ratio_clamps_to_the_minimum_share_and_needs_a_split() {
        let mut tree = of(split(
            Row,
            0.5,
            leaf(1),
            split(Column, 0.5, leaf(2), leaf(3)),
        ));
        let root = SeamId(vec![]);
        let inner = SeamId(vec![true]);
        assert!(tree.set_ratio(&root, 0.05));
        assert!(tree.set_ratio(&inner, 0.95));
        assert_eq!(
            tree,
            of(split(
                Row,
                MIN_SHARE,
                leaf(1),
                split(Column, 1.0 - MIN_SHARE, leaf(2), leaf(3))
            ))
        );
        assert!(tree.set_ratio(&root, 0.4));
        assert!(close(root_ratio(&tree), 0.4));
        assert!(
            !tree.set_ratio(&SeamId(vec![false]), 0.5),
            "a leaf has no seam"
        );
        assert!(
            !tree.set_ratio(&SeamId(vec![true, true, true]), 0.5),
            "the path runs off the tree"
        );
        assert!(!tree.set_ratio(&root, f32::NAN));
        assert!(!Tree::default().set_ratio(&root, 0.5));
    }

    #[test]
    fn rects_share_the_bounds_minus_the_gap_and_stay_inside() {
        let bounds = rect(10.0, 20.0, 300.0, 200.0);
        let tree = Tree::even(&ids(1..7));
        let rects = tree.rects(bounds, 4.0);
        assert_eq!(rects.len(), 6);
        for (_, cell) in &rects {
            assert!(inside(*cell, bounds), "{cell:?}");
            assert!(close(cell.h, 98.0), "{cell:?}");
        }
        // A row's widths plus its two gaps span the bounds exactly.
        let top: Vec<Rect> = rects[..3].iter().map(|(_, cell)| *cell).collect();
        assert!(close(top[0].x, 10.0));
        assert!(close(top[1].x, top[0].x + top[0].w + 4.0));
        assert!(close(top[2].x, top[1].x + top[1].w + 4.0));
        assert!(close(top[2].x + top[2].w, 310.0));
        // The second row sits past the horizontal gap and ends at the bottom.
        assert!(close(rects[3].1.y, 20.0 + 98.0 + 4.0));
        assert!(close(rects[3].1.y + rects[3].1.h, 220.0));
        assert_eq!(
            of(leaf(1)).rects(bounds, 4.0),
            vec![(id(1), bounds)],
            "a single leaf fills the bounds"
        );
        assert!(Tree::default().rects(bounds, 4.0).is_empty());
        // Bounds narrower than the gap never push a rect outside.
        let tiny = rect(0.0, 0.0, 5.0, 5.0);
        for (_, cell) in tree.rects(tiny, 4.0) {
            assert!(
                inside(cell, tiny) && cell.w >= 0.0 && cell.h >= 0.0,
                "{cell:?}"
            );
        }
    }

    #[test]
    fn seams_sit_in_the_gap_between_their_two_sides_widened_to_the_grab() {
        let bounds = rect(0.0, 0.0, 100.0, 50.0);
        let tree = of(split(
            Row,
            0.5,
            leaf(1),
            split(Column, 0.5, leaf(2), leaf(3)),
        ));
        let rects = tree.rects(bounds, 4.0);
        assert!(same(rects[0].1, rect(0.0, 0.0, 48.0, 50.0)));
        assert!(same(rects[1].1, rect(52.0, 0.0, 48.0, 23.0)));
        assert!(same(rects[2].1, rect(52.0, 27.0, 48.0, 23.0)));
        let seams = tree.seams(bounds, 4.0, 10.0);
        assert_eq!(seams.len(), 2);
        let outer = &seams[0];
        assert_eq!(outer.id, SeamId(vec![]));
        assert_eq!(outer.axis, Row);
        assert!(same(outer.area, bounds));
        assert!(same(outer.band, rect(45.0, 0.0, 10.0, 50.0)));
        let inner = &seams[1];
        assert_eq!(inner.id, SeamId(vec![true]));
        assert_eq!(inner.axis, Column);
        assert!(same(inner.area, rect(52.0, 0.0, 48.0, 50.0)));
        assert!(same(inner.band, rect(52.0, 20.0, 48.0, 10.0)));
        // A grab narrower than the gap leaves the band as the gap.
        let narrow = tree.seams(bounds, 4.0, 2.0);
        assert!(same(narrow[0].band, rect(48.0, 0.0, 4.0, 50.0)));
        // Every seam of a grid runs between the rects on its two sides.
        let bounds = rect(10.0, 20.0, 300.0, 200.0);
        let grid = Tree::even(&ids(1..7));
        let cells = grid.rects(bounds, 4.0);
        let seams = grid.seams(bounds, 4.0, 8.0);
        assert_eq!(seams.len(), 5);
        for seam in &seams {
            assert!(inside(seam.band, seam.area), "{seam:?}");
            let centre = at(
                seam.band.x + seam.band.w / 2.0,
                seam.band.y + seam.band.h / 2.0,
            );
            for (_, cell) in cells.iter().filter(|(_, cell)| inside(*cell, seam.area)) {
                let clear = match seam.axis {
                    Row => cell.x + cell.w <= centre.x + 1e-3 || cell.x >= centre.x - 1e-3,
                    Column => cell.y + cell.h <= centre.y + 1e-3 || cell.y >= centre.y - 1e-3,
                };
                assert!(clear, "{seam:?} crosses {cell:?}");
            }
        }
    }

    #[test]
    fn ratio_for_puts_the_seam_under_the_pointer_and_round_trips_through_set_ratio() {
        let bounds = rect(0.0, 0.0, 100.0, 50.0);
        let mut tree = of(split(Row, 0.5, leaf(1), leaf(2)));
        let root = SeamId(vec![]);
        let read = |tree: &Tree, x: f32| tree.ratio_for(&root, bounds, at(x, 25.0), 4.0).unwrap();
        assert!(close(read(&tree, 50.0), 0.5));
        assert!(close(read(&tree, 26.0), 0.25));
        assert!(close(read(&tree, 0.0), MIN_SHARE));
        assert!(close(read(&tree, 500.0), 1.0 - MIN_SHARE));
        assert_eq!(
            tree.ratio_for(&SeamId(vec![false]), bounds, at(26.0, 25.0), 4.0),
            None
        );
        let ratio = read(&tree, 26.0);
        assert!(tree.set_ratio(&root, ratio));
        let seam = &tree.seams(bounds, 4.0, 10.0)[0];
        assert!(close(seam.band.x + seam.band.w / 2.0, 26.0));
        // A Column seam reads the pointer's y.
        let mut stack = of(split(Column, 0.5, leaf(1), leaf(2)));
        let ratio = stack.ratio_for(&root, bounds, at(10.0, 12.0), 4.0).unwrap();
        assert!(close(ratio, 10.0 / 46.0));
        assert!(stack.set_ratio(&root, ratio));
        let seam = &stack.seams(bounds, 4.0, 10.0)[0];
        assert!(close(seam.band.y + seam.band.h / 2.0, 12.0));
        // A split with no room to share reports its current ratio.
        let cramped = tree
            .ratio_for(&root, rect(0.0, 0.0, 3.0, 3.0), at(1.0, 1.0), 4.0)
            .unwrap();
        assert!(close(cramped, root_ratio(&tree)));
    }

    #[test]
    fn a_drop_swaps_in_the_centre_core_and_splits_toward_the_nearest_edge() {
        // 30% of the 100 px short side is 30, under the 44 px floor: half is 22.
        let pane = rect(0.0, 0.0, 200.0, 100.0);
        assert_eq!(zone(at(100.0, 50.0), pane), Zone::Swap);
        assert_eq!(zone(at(121.0, 50.0), pane), Zone::Swap);
        assert_eq!(zone(at(100.0, 71.0), pane), Zone::Swap);
        assert_eq!(
            zone(at(123.0, 50.0), pane),
            Zone::Split(Edge::Top),
            "just past the core, as far from the top as the bottom: Top wins"
        );
        assert_eq!(zone(at(100.0, 10.0), pane), Zone::Split(Edge::Top));
        assert_eq!(zone(at(100.0, 95.0), pane), Zone::Split(Edge::Bottom));
        assert_eq!(zone(at(5.0, 50.0), pane), Zone::Split(Edge::Left));
        assert_eq!(zone(at(195.0, 50.0), pane), Zone::Split(Edge::Right));
        // A big Pane's core grows with it: 300 px here, half 150.
        let big = rect(0.0, 0.0, 1000.0, 1000.0);
        assert_eq!(zone(at(640.0, 500.0), big), Zone::Swap);
        assert_eq!(zone(at(660.0, 500.0), big), Zone::Split(Edge::Right));
        // A tiny Pane's core is capped at 70% so its edges stay droppable:
        // 35 px here, half 17.5.
        let tiny = rect(0.0, 0.0, 50.0, 50.0);
        assert_eq!(zone(at(42.0, 25.0), tiny), Zone::Swap);
        assert_eq!(zone(at(43.0, 25.0), tiny), Zone::Split(Edge::Right));
        // Ties: Bottom beats Left, Top beats Left, Left beats Right.
        let square = rect(0.0, 0.0, 100.0, 100.0);
        assert_eq!(zone(at(5.0, 95.0), square), Zone::Split(Edge::Bottom));
        assert_eq!(zone(at(5.0, 5.0), square), Zone::Split(Edge::Top));
        let tall = rect(0.0, 0.0, 100.0, 200.0);
        assert_eq!(zone(at(50.0, 140.0), tall), Zone::Split(Edge::Left));
        // Offset rects measure from their own origin.
        let shifted = rect(300.0, 400.0, 200.0, 100.0);
        assert_eq!(zone(at(400.0, 450.0), shifted), Zone::Swap);
        assert_eq!(zone(at(305.0, 450.0), shifted), Zone::Split(Edge::Left));
    }

    #[test]
    fn zone_rects_preview_the_half_a_drop_takes() {
        let pane = rect(10.0, 20.0, 100.0, 60.0);
        assert_eq!(zone_rect(pane, Zone::Swap), pane);
        assert_eq!(
            zone_rect(pane, Zone::Split(Edge::Left)),
            rect(10.0, 20.0, 50.0, 60.0)
        );
        assert_eq!(
            zone_rect(pane, Zone::Split(Edge::Right)),
            rect(60.0, 20.0, 50.0, 60.0)
        );
        assert_eq!(
            zone_rect(pane, Zone::Split(Edge::Top)),
            rect(10.0, 20.0, 100.0, 30.0)
        );
        assert_eq!(
            zone_rect(pane, Zone::Split(Edge::Bottom)),
            rect(10.0, 50.0, 100.0, 30.0)
        );
        assert!(pane.contains(at(10.0, 20.0)));
        assert!(pane.contains(at(110.0, 80.0)));
        assert!(!pane.contains(at(111.0, 80.0)));
    }

    #[test]
    fn trees_round_trip_through_json_with_leaves_as_thread_numbers() {
        let tree = of(split(
            Row,
            0.3,
            leaf(1),
            split(Column, 0.6, leaf(2), leaf(3)),
        ));
        let json = serde_json::to_string(&tree).unwrap();
        assert!(json.contains(r#""Leaf":1"#), "{json}");
        assert!(json.contains(r#""axis":"Row""#), "{json}");
        assert_eq!(serde_json::from_str::<Tree>(&json).unwrap(), tree);
        assert_eq!(serde_json::from_str::<Tree>("{}").unwrap(), Tree::default());
        assert_eq!(
            serde_json::from_str::<Tree>(r#"{"root":null}"#).unwrap(),
            Tree::default()
        );
        assert_eq!(
            set(&serde_json::from_str::<Tree>(&json).unwrap().leaves()),
            set(&ids(1..4))
        );
    }
}
