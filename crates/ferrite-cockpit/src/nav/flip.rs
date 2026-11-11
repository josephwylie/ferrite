//! The nav list's motion (T3's `Sidebar.motion.ts`; `docs/design/
//! t3-reference/motion.md` §4/§5): a row that moves slides from where it
//! was drawn, one that enters fades in from a little above (or rides the
//! moved row above it), and one that leaves is drawn as a ghost fading
//! down, all over `motion::NAV_LIST`.
//!
//! The list is laid out where it ends up; each row's offset from there is
//! applied while it prepaints (`FlipRow`), so a frame never shows a row at
//! its new place before it starts to travel. A row's top is read against
//! the list's own (`FlipList`), so scrolling moves nothing. A ghost is
//! known before layout, from the rows the render leaves out, and drawn
//! from the face the row last had.
//!
//! An update lands at once on the first paint, under reduced motion, when
//! the owner asks (`Update::snap`: a shelf toggle, a drag, the list
//! replaced by search results), and when more than `NAV_LIST_MAX_FADED`
//! rows would enter or leave in it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;
use std::time::Instant;

use ferrite_ui::motion::{self, Tween};
use ferrite_ui::theme::{MOTION_NAV_LIST_TRAVEL, NAV_LIST_MAX_FADED};
use gpui::{
    point, px, AnyElement, App, Bounds, Element, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Window,
};

/// How far a row of `height` travels in or out: at most 40px.
pub fn travel(height: f32) -> f32 {
    height.min(MOTION_NAV_LIST_TRAVEL)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Laid {
    top: f32,
    height: f32,
}

#[derive(Clone, Copy, Debug)]
struct Travel {
    offset: Tween,
    opacity: Option<Tween>,
}

impl Travel {
    fn offset(&self, now: Instant) -> f32 {
        self.offset.value(now, false)
    }

    fn opacity(&self, now: Instant) -> f32 {
        self.opacity.map_or(1.0, |fade| fade.value(now, false))
    }

    fn running(&self, now: Instant) -> bool {
        self.offset.running(now, false)
            || self.opacity.is_some_and(|fade| fade.running(now, false))
    }
}

/// A row that left the list, drawn from its last face where it was.
#[derive(Clone, Debug)]
pub struct Ghost<K, G> {
    pub key: K,
    pub face: G,
    top: f32,
    offset: Tween,
    opacity: Tween,
}

impl<K, G> Ghost<K, G> {
    /// Its top in the list at `now`, and its opacity.
    pub fn at(&self, now: Instant) -> (f32, f32) {
        (
            self.top + self.offset.value(now, false),
            self.opacity.value(now, false),
        )
    }

    fn running(&self, now: Instant) -> bool {
        self.opacity.running(now, false)
    }
}

/// What the owner says about this render's list.
pub struct Update<K, G> {
    /// Every row drawn, in drawing order, with the face its ghost would
    /// wear.
    pub rows: Vec<(K, G)>,
    /// Land at once whatever changed.
    pub snap: bool,
    pub now: Instant,
    pub reduced: bool,
}

/// The list's rows as last laid out, what is moving, and what is leaving.
pub struct ListMotion<K, G> {
    laid: HashMap<K, Laid>,
    next: HashMap<K, Laid>,
    moving: HashMap<K, Travel>,
    /// This frame's moves, for an entering row to ride.
    moved: HashMap<K, f32>,
    faces: HashMap<K, G>,
    order: Vec<K>,
    ghosts: Vec<Ghost<K, G>>,
    origin: f32,
    snap: bool,
    now: Option<Instant>,
}

impl<K, G> Default for ListMotion<K, G> {
    fn default() -> Self {
        ListMotion {
            laid: HashMap::new(),
            next: HashMap::new(),
            moving: HashMap::new(),
            moved: HashMap::new(),
            faces: HashMap::new(),
            order: Vec::new(),
            ghosts: Vec::new(),
            origin: 0.0,
            snap: false,
            now: None,
        }
    }
}

impl<K: Copy + Eq + Hash, G: Clone> ListMotion<K, G> {
    /// The render's rows: those last laid out and now missing leave as
    /// ghosts; the ghosts still fading are returned to be drawn.
    pub fn begin(&mut self, update: Update<K, G>) -> Vec<Ghost<K, G>> {
        let Update {
            rows,
            snap,
            now,
            reduced,
        } = update;
        let drawn: HashMap<K, ()> = rows.iter().map(|(key, _)| (*key, ())).collect();
        let gone: Vec<K> = self
            .laid
            .keys()
            .copied()
            .filter(|key| !drawn.contains_key(key))
            .collect();
        let entering = rows
            .iter()
            .filter(|(key, _)| !self.laid.contains_key(key))
            .count();
        self.snap = snap
            || reduced
            || self.laid.is_empty()
            || gone.len() + entering > NAV_LIST_MAX_FADED;
        if self.snap {
            self.ghosts.clear();
            self.moving.clear();
        } else {
            for key in gone {
                if self.ghosts.iter().any(|ghost| ghost.key == key) {
                    continue;
                }
                let (Some(laid), Some(face)) = (self.laid.get(&key).copied(), self.faces.get(&key))
                else {
                    continue;
                };
                let from = self
                    .moving
                    .remove(&key)
                    .map_or(0.0, |moving| moving.offset(now));
                self.ghosts.push(Ghost {
                    key,
                    face: face.clone(),
                    top: laid.top,
                    offset: Tween::new(from, from + travel(laid.height), motion::NAV_LIST, now),
                    opacity: Tween::new(1.0, 0.0, motion::NAV_LIST, now),
                });
            }
        }
        self.ghosts
            .retain(|ghost| ghost.running(now) && !drawn.contains_key(&ghost.key));
        self.order = rows.iter().map(|(key, _)| *key).collect();
        self.faces = rows.into_iter().collect();
        self.now = Some(now);
        self.ghosts.clone()
    }

    /// The list lays out from `origin` (its top in the window) this frame.
    fn frame(&mut self, origin: f32) {
        self.origin = origin;
        self.next.clear();
        self.moved.clear();
    }

    /// Where `key`, laid out at `top` (in the window) and `height` tall,
    /// draws this frame: its offset from there, and its opacity.
    pub fn place(&mut self, key: K, top: f32, height: f32) -> (f32, f32) {
        let now = self.now.unwrap_or_else(Instant::now);
        let at = top - self.origin;
        self.next.insert(key, Laid { top: at, height });
        if self.snap {
            self.moving.remove(&key);
            return (0.0, 1.0);
        }
        match self.laid.get(&key).copied() {
            Some(was) if was.top != at => {
                let carried = self.moving.get(&key).copied();
                let from = was.top + carried.map_or(0.0, |moving| moving.offset(now)) - at;
                self.moved.insert(key, from);
                self.moving.insert(
                    key,
                    Travel {
                        offset: Tween::new(from, 0.0, motion::NAV_LIST, now),
                        opacity: carried.and_then(|moving| {
                            moving.opacity.filter(|fade| fade.running(now, false)).map(|_| {
                                Tween::new(moving.opacity(now), 1.0, motion::NAV_LIST, now)
                            })
                        }),
                    },
                );
            }
            Some(_) => {}
            None => {
                let ride = self
                    .order
                    .iter()
                    .take_while(|other| **other != key)
                    .filter_map(|other| self.moved.get(other))
                    .last()
                    .copied();
                let from = ride.unwrap_or(-travel(height));
                self.moving.insert(
                    key,
                    Travel {
                        offset: Tween::new(from, 0.0, motion::NAV_LIST, now),
                        opacity: Some(Tween::new(0.0, 1.0, motion::NAV_LIST, now)),
                    },
                );
            }
        }
        match self.moving.get(&key) {
            Some(moving) if moving.running(now) => (moving.offset(now), moving.opacity(now)),
            Some(_) => {
                self.moving.remove(&key);
                (0.0, 1.0)
            }
            None => (0.0, 1.0),
        }
    }

    /// The frame's layout is the next update's "before".
    fn end(&mut self) {
        self.laid = std::mem::take(&mut self.next);
        self.snap = false;
    }

    /// Where `key` draws from its laid-out top at `now`, while it travels.
    #[cfg(test)]
    pub fn offset(&self, key: K, now: Instant) -> Option<f32> {
        self.moving
            .get(&key)
            .filter(|moving| moving.running(now))
            .map(|moving| moving.offset(now))
    }

    /// How many rows that left are still fading as ghosts at `now`.
    #[cfg(test)]
    pub fn ghost_count(&self, now: Instant) -> usize {
        self.ghosts.iter().filter(|ghost| ghost.running(now)).count()
    }

    /// Whether anything still travels: the owner asks for frames.
    pub fn running(&self, now: Instant) -> bool {
        self.moving.values().any(|moving| moving.running(now))
            || self.ghosts.iter().any(|ghost| ghost.running(now))
    }
}

/// The shared handle the list's elements read.
pub type Motion<K, G> = Rc<RefCell<ListMotion<K, G>>>;

/// The list: its top is where its rows' tops are read from, and once its
/// rows are laid out their places are the next update's "before".
pub struct FlipList<K, G> {
    motion: Motion<K, G>,
    child: Option<AnyElement>,
}

pub fn flip_list<K, G>(motion: &Motion<K, G>, child: impl IntoElement) -> FlipList<K, G> {
    FlipList {
        motion: motion.clone(),
        child: Some(child.into_any_element()),
    }
}

/// One row of the list, drawn where it travels.
pub struct FlipRow<K, G> {
    motion: Motion<K, G>,
    key: K,
    child: Option<AnyElement>,
}

pub fn flip_row<K, G>(motion: &Motion<K, G>, key: K, child: impl IntoElement) -> FlipRow<K, G> {
    FlipRow {
        motion: motion.clone(),
        key,
        child: Some(child.into_any_element()),
    }
}

impl<K: 'static, G: 'static> IntoElement for FlipList<K, G>
where
    K: Copy + Eq + Hash,
    G: Clone,
{
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl<K, G> Element for FlipList<K, G>
where
    K: Copy + Eq + Hash + 'static,
    G: Clone + 'static,
{
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let id = self.child.as_mut().expect("laid out once").request_layout(window, cx);
        (id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.motion.borrow_mut().frame(f32::from(bounds.top()));
        if let Some(child) = &mut self.child {
            child.prepaint(window, cx);
        }
        let mut motion = self.motion.borrow_mut();
        motion.end();
        if motion.running(cx.background_executor().now()) {
            motion::request_frame(window);
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(child) = &mut self.child {
            child.paint(window, cx);
        }
    }
}

impl<K: 'static, G: 'static> IntoElement for FlipRow<K, G>
where
    K: Copy + Eq + Hash,
    G: Clone,
{
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl<K, G> Element for FlipRow<K, G>
where
    K: Copy + Eq + Hash + 'static,
    G: Clone + 'static,
{
    type RequestLayoutState = ();
    type PrepaintState = f32;

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let id = self.child.as_mut().expect("laid out once").request_layout(window, cx);
        (id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> f32 {
        let (offset, opacity) = self.motion.borrow_mut().place(
            self.key,
            f32::from(bounds.top()),
            f32::from(bounds.size.height),
        );
        if let Some(child) = &mut self.child {
            window.with_element_offset(point(px(0.0), px(offset)), |window| {
                window.with_element_opacity(Some(opacity), |window| child.prepaint(window, cx))
            });
        }
        opacity
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        opacity: &mut f32,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(child) = &mut self.child {
            window.with_element_opacity(Some(*opacity), |window| child.paint(window, cx));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    type Motion = ListMotion<u32, &'static str>;

    /// One frame: the render's rows (key, top, height), laid out from 0.
    fn frame(motion: &mut Motion, rows: &[(u32, f32, f32)], now: Instant, snap: bool) -> Vec<(u32, f32, f32)> {
        motion.begin(Update {
            rows: rows.iter().map(|(key, ..)| (*key, "face")).collect(),
            snap,
            now,
            reduced: false,
        });
        motion.frame(0.0);
        let placed = rows
            .iter()
            .map(|(key, top, height)| {
                let (offset, opacity) = motion.place(*key, *top, *height);
                (*key, offset, opacity)
            })
            .collect();
        motion.end();
        placed
    }

    fn ms(at: Instant, ms: u64) -> Instant {
        at + Duration::from_millis(ms)
    }

    #[test]
    fn the_first_paint_lands_and_a_reorder_slides_from_the_old_top() {
        let mut motion = Motion::default();
        let t0 = Instant::now();
        let first = frame(&mut motion, &[(1, 0.0, 83.0), (2, 83.0, 83.0)], t0, false);
        assert_eq!(first, [(1, 0.0, 1.0), (2, 0.0, 1.0)], "no entrance on first paint");
        let swapped = frame(&mut motion, &[(2, 0.0, 83.0), (1, 83.0, 83.0)], ms(t0, 100), false);
        assert_eq!(swapped, [(2, 83.0, 1.0), (1, -83.0, 1.0)], "drawn where they were");
        let mid = frame(&mut motion, &[(2, 0.0, 83.0), (1, 83.0, 83.0)], ms(t0, 175), false);
        let offset = mid[0].1;
        assert!(0.0 < offset && offset < 83.0, "between its old and new top: {offset}");
        let done = frame(&mut motion, &[(2, 0.0, 83.0), (1, 83.0, 83.0)], ms(t0, 260), false);
        assert_eq!(done, [(2, 0.0, 1.0), (1, 0.0, 1.0)], "home after 150ms");
        assert!(!motion.running(ms(t0, 260)));
    }

    #[test]
    fn an_interrupted_move_carries_on_from_where_it_is() {
        let mut motion = Motion::default();
        let t0 = Instant::now();
        frame(&mut motion, &[(1, 0.0, 83.0), (2, 83.0, 83.0)], t0, false);
        frame(&mut motion, &[(2, 0.0, 83.0), (1, 83.0, 83.0)], ms(t0, 10), false);
        let before = frame(&mut motion, &[(2, 0.0, 83.0), (1, 83.0, 83.0)], ms(t0, 60), false);
        let painted = 0.0 + before[0].1;
        let back = frame(&mut motion, &[(1, 0.0, 83.0), (2, 83.0, 83.0)], ms(t0, 60), false);
        let (_, offset, _) = back.iter().find(|(key, ..)| *key == 2).copied().unwrap();
        assert!((83.0 + offset - painted).abs() < 1e-3, "it starts from where it was painted, no jump");
    }

    #[test]
    fn an_entering_row_drops_in_and_a_leaving_one_ghosts_down() {
        let mut motion = Motion::default();
        let t0 = Instant::now();
        frame(&mut motion, &[(1, 0.0, 36.0)], t0, false);
        let entered = frame(&mut motion, &[(2, 0.0, 83.0), (1, 83.0, 36.0)], ms(t0, 10), false);
        assert_eq!(entered[0], (2, -40.0, 0.0), "from 40px above, transparent");
        assert_eq!(entered[1], (1, -83.0, 1.0), "the row below slides down");
        frame(&mut motion, &[(1, 0.0, 36.0)], ms(t0, 20), false);
        assert_eq!(motion.ghosts.len(), 1);
        assert_eq!(motion.ghosts[0].key, 2);
        let (top, opacity) = motion.ghosts[0].at(ms(t0, 20));
        assert_eq!(opacity, 1.0);
        assert!(top < 0.0, "it leaves from where it was painted: {top}");
        let (end, faded) = motion.ghosts[0].at(ms(t0, 170));
        assert!((end - top - 40.0).abs() < 1e-3 && faded == 0.0, "40px down, transparent");
        frame(&mut motion, &[(1, 0.0, 36.0)], ms(t0, 171), false);
        assert!(motion.ghosts.is_empty(), "gone after 150ms");
    }

    #[test]
    fn an_entering_row_rides_the_moved_row_above_it() {
        let mut motion = Motion::default();
        let t0 = Instant::now();
        frame(&mut motion, &[(1, 0.0, 83.0)], t0, false);
        let placed = frame(&mut motion, &[(1, 36.0, 83.0), (2, 119.0, 36.0)], ms(t0, 10), false);
        assert_eq!(placed[0].1, -36.0);
        assert_eq!(placed[1], (2, -36.0, 0.0), "it rides the row above");
    }

    #[test]
    fn a_snap_a_reduced_frame_and_a_crowd_land_at_once() {
        let t0 = Instant::now();
        let mut motion = Motion::default();
        frame(&mut motion, &[(1, 0.0, 83.0), (2, 83.0, 83.0)], t0, false);
        let snapped = frame(&mut motion, &[(2, 0.0, 83.0), (1, 83.0, 83.0)], ms(t0, 10), true);
        assert_eq!(snapped, [(2, 0.0, 1.0), (1, 0.0, 1.0)], "a shelf toggle snaps");

        let mut motion = Motion::default();
        frame(&mut motion, &[(1, 0.0, 83.0)], t0, false);
        motion.begin(Update {
            rows: vec![(2, "face"), (1, "face")],
            snap: false,
            now: ms(t0, 10),
            reduced: true,
        });
        motion.frame(0.0);
        assert_eq!(motion.place(2, 0.0, 83.0), (0.0, 1.0), "reduced motion");
        motion.end();

        let mut motion = Motion::default();
        frame(&mut motion, &[(0, 0.0, 36.0)], t0, false);
        let crowd: Vec<(u32, f32, f32)> = (1..=41).map(|key| (key, key as f32 * 37.0, 36.0)).collect();
        let placed = frame(&mut motion, &crowd, ms(t0, 10), false);
        assert!(placed.iter().all(|(_, offset, opacity)| *offset == 0.0 && *opacity == 1.0), "41 fading rows snap");
        assert!(motion.ghosts.is_empty());
    }

    #[test]
    fn travel_is_at_most_forty() {
        assert_eq!(travel(83.0), 40.0);
        assert_eq!(travel(36.0), 36.0);
    }
}
