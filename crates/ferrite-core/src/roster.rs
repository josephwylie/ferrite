//! The Roster: what the Cockpit shows, headless. Which Panes are open and
//! in what order, which one holds the keyboard, the View — Solo or one
//! Group — and the fullscreen. Every rule about Solo, Group, focus,
//! fullscreen and park order lives here, behind the acts on `Cockpit`
//! (`close`, `reopen`, `enter_group`, `drop`, ...); the window mirrors the
//! roster into its own Pane views and paints. No I/O: reviving, parking
//! and Group changes are the Cockpit's, which asks the roster for the
//! arithmetic and tells it what happened.
//!
//! The roster shows exactly the open Threads plus the drafts: `open`,
//! `revive`, `park` and `delete` keep that invariant themselves, so a
//! Thread can never be open without a Pane or shown without a Session.

use std::collections::BTreeMap;

use crate::groups::{grid, GroupId, Groups};
use crate::ThreadId;

/// A draft Pane's identity (#29): a Composer and a pre-prompt band with no
/// Thread yet. Minted here, stable until the first send makes a Thread of
/// it or the draft is discarded — never persisted as a fake Thread id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DraftId(u64);

impl DraftId {
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Stable Pane identity across grid reflow: a Thread, or a draft.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaneIdentity {
    Thread(ThreadId),
    Draft(DraftId),
}

impl PaneIdentity {
    pub fn thread(self) -> Option<ThreadId> {
        match self {
            PaneIdentity::Thread(thread) => Some(thread),
            PaneIdentity::Draft(_) => None,
        }
    }

    pub fn draft(self) -> Option<DraftId> {
        match self {
            PaneIdentity::Draft(draft) => Some(draft),
            PaneIdentity::Thread(_) => None,
        }
    }
}

/// What the Cockpit is showing. There is no global wall: the operator sees
/// exactly one Thread, or exactly one Group's ordered membership (#28).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    /// The default: exactly the focused Thread's Pane.
    #[default]
    Solo,
    /// One durable Group — two or more Threads from one registered Project.
    Group(GroupId),
}

/// A draft's Group scope (#29): the Group it joins at its first send, and
/// the pair member whose leave is deferred onto it — see `Cockpit::close`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DraftScope {
    pub group: Option<GroupId>,
    pub pending_leave: Option<ThreadId>,
}

/// The grid the visible Panes lay out on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub columns: usize,
    pub rows: usize,
    /// The prototype's tall-left board: exactly four Panes of a Group, two
    /// columns and three rows with the left Pane spanning all of them. The
    /// prototype specifies a board for four Panes and no other count, so
    /// every other count keeps the chunked rows (R-01).
    pub tall_left: bool,
}

#[derive(Debug, Default)]
pub struct Roster {
    panes: Vec<PaneIdentity>,
    focused: usize,
    view: View,
    /// The Pane cmd-f fullscreened, if any: it takes the whole grid area at
    /// Level::Transcript while every other Session keeps streaming (#20).
    /// Deliberate moves re-aim it through `focus` — cmd-w's survivor fills
    /// the screen like the next browser tab — and a Pane removed by a path
    /// that never focused anything reads as *gone* and falls back to the
    /// grid, instead of a bool silently fullscreening whichever Pane
    /// inherited its index.
    fullscreen: Option<PaneIdentity>,
    /// Threads the operator parked this launch, oldest first — cmd-o pops
    /// the tail, the one just closed. In memory only, deliberately: the
    /// store keeps no park order, so a relaunch forgets it and reopen falls
    /// back to creation order (accepted v1 behavior).
    park_order: Vec<ThreadId>,
    drafts: BTreeMap<DraftId, DraftScope>,
    next_draft: u64,
}

impl Roster {
    // ------------------------------------------------------------- reads

    /// Every open Pane, in grid order.
    pub fn panes(&self) -> &[PaneIdentity] {
        &self.panes
    }

    pub fn focused_index(&self) -> usize {
        self.focused
    }

    pub fn focused(&self) -> Option<PaneIdentity> {
        self.panes.get(self.focused).copied()
    }

    pub fn focused_thread(&self) -> Option<ThreadId> {
        self.focused().and_then(PaneIdentity::thread)
    }

    pub fn view(&self) -> View {
        self.view
    }

    pub fn fullscreen(&self) -> Option<PaneIdentity> {
        self.fullscreen
    }

    pub fn index_of(&self, identity: PaneIdentity) -> Option<usize> {
        self.panes.iter().position(|pane| *pane == identity)
    }

    pub fn pane_of(&self, thread: ThreadId) -> Option<usize> {
        self.index_of(PaneIdentity::Thread(thread))
    }

    pub fn draft_scope(&self, draft: DraftId) -> Option<DraftScope> {
        self.drafts.get(&draft).copied()
    }

    /// The member of this Group whose leave is waiting on a draft — see
    /// `Cockpit::close`. At most one: a Group holds at most one pending
    /// draft, and a draft defers at most one leave.
    pub fn pending_leave(&self, group: GroupId) -> Option<ThreadId> {
        self.drafts
            .values()
            .find(|scope| scope.group == Some(group))
            .and_then(|scope| scope.pending_leave)
    }

    /// The draft waiting to join this Group, if one is open.
    pub fn pending_draft(&self, group: GroupId) -> Option<DraftId> {
        self.drafts
            .iter()
            .find(|(_, scope)| scope.group == Some(group))
            .map(|(draft, _)| *draft)
    }

    pub fn park_order(&self) -> &[ThreadId] {
        &self.park_order
    }

    /// The Panes on screen, in order: Solo shows exactly the focused Pane;
    /// a Group shows its members that have a Pane, in the Group's order,
    /// then the draft pending in it. A member whose leave is deferred onto
    /// that draft is already gone as far as the operator is concerned —
    /// the draft is standing in its place, and both at once would show a
    /// pair as three.
    pub fn visible(&self, groups: &Groups) -> Vec<PaneIdentity> {
        match self.view {
            View::Solo => self.focused().into_iter().collect(),
            View::Group(group) => {
                let Some(members) = groups.get(group).map(|group| &group.members) else {
                    return Vec::new();
                };
                let leaving = self.pending_leave(group);
                members
                    .iter()
                    .copied()
                    .filter(|thread| Some(*thread) != leaving)
                    .map(PaneIdentity::Thread)
                    .filter(|identity| self.panes.contains(identity))
                    .chain(self.panes.iter().copied().filter(|identity| {
                        identity
                            .draft()
                            .and_then(|draft| self.drafts.get(&draft))
                            .is_some_and(|scope| scope.group == Some(group))
                    }))
                    .collect()
            }
        }
    }

    /// The grid the visible Panes lay out on: one column in Solo, the
    /// near-square `groups::grid` for a Group, and the prototype's
    /// tall-left board for exactly four.
    pub fn layout(&self, groups: &Groups) -> Layout {
        let visible = self.visible(groups).len();
        let tall_left = matches!(self.view, View::Group(_)) && visible == 4;
        if tall_left {
            return Layout {
                columns: 2,
                rows: 3,
                tall_left,
            };
        }
        let columns = match self.view {
            View::Solo => 1,
            View::Group(_) => grid(visible).1.max(1),
        };
        Layout {
            columns,
            rows: visible.div_ceil(columns).max(1),
            tall_left,
        }
    }

    // ------------------------------------------------------------- focus

    /// The one door to `focused`: every move — keys, clicks, nav rows —
    /// lands here, so fullscreen re-aims with focus. While fullscreen, the
    /// Pane the operator lands on is the Pane that fills the screen
    /// (browser-tab muscle memory). Never *enters* fullscreen, only
    /// re-aims it.
    pub(crate) fn focus(&mut self, identity: PaneIdentity) -> bool {
        let Some(index) = self.index_of(identity) else {
            return false;
        };
        self.focus_index(index);
        true
    }

    pub(crate) fn focus_index(&mut self, index: usize) {
        self.focused = index.min(self.panes.len().saturating_sub(1));
        if self.fullscreen.is_some() {
            self.fullscreen = self.focused();
        }
    }

    /// Walk the visible Panes by `delta`, wrapping.
    pub(crate) fn step(&mut self, delta: isize, groups: &Groups) {
        let visible = self.visible(groups);
        if visible.is_empty() {
            return;
        }
        let at = self
            .focused()
            .and_then(|focused| visible.iter().position(|pane| *pane == focused))
            .unwrap_or(0);
        let next = (at as isize + delta).rem_euclid(visible.len() as isize) as usize;
        self.focus(visible[next]);
    }

    pub(crate) fn set_view(&mut self, view: View) {
        self.view = view;
    }

    /// cmd-f (#20): the focused Pane takes the whole cockpit; cmd-f again
    /// restores the grid.
    pub(crate) fn toggle_fullscreen(&mut self) {
        self.fullscreen = match self.fullscreen {
            Some(_) => None,
            None => self.focused(),
        };
    }

    // ------------------------------------------------------------- panes

    /// A Thread became open: it takes the next Pane, and focus stays put.
    pub(crate) fn insert_thread(&mut self, thread: ThreadId) {
        let identity = PaneIdentity::Thread(thread);
        if !self.panes.contains(&identity) {
            self.panes.push(identity);
        }
    }

    /// A Thread stopped being open under the roster — parked or deleted by
    /// a path that focused nothing. Its Pane goes, the clamped index keeps
    /// focus, and a fullscreen aimed at it falls back to the grid rather
    /// than showing a Pane the operator did not pick.
    pub(crate) fn remove_thread(&mut self, thread: ThreadId) {
        let identity = PaneIdentity::Thread(thread);
        if self.fullscreen == Some(identity) {
            self.fullscreen = None;
        }
        self.remove(identity);
    }

    /// The operator closed this Pane: it goes, the clamped survivor takes
    /// focus — and, while fullscreen, the screen (#20): closing a browser
    /// tab shows the next tab, not an overview. Closing the last Pane
    /// leaves nothing to aim at, and fullscreen falls back to the (empty)
    /// grid.
    pub(crate) fn close_pane(&mut self, identity: PaneIdentity) {
        let re_aim = self.fullscreen == Some(identity);
        self.remove(identity);
        if re_aim {
            self.fullscreen = self.focused();
        }
    }

    /// Re-aim the fullscreen at whatever holds focus now — `close`'s
    /// survivor rule, for the paths that park under the roster first.
    pub(crate) fn fullscreen_focused(&mut self) {
        self.fullscreen = self.focused();
    }

    fn remove(&mut self, identity: PaneIdentity) {
        self.panes.retain(|pane| *pane != identity);
        if let PaneIdentity::Draft(draft) = identity {
            self.drafts.remove(&draft);
        }
        self.focused = self.focused.min(self.panes.len().saturating_sub(1));
    }

    // ------------------------------------------------------------ drafts

    /// A draft Pane (#29) opens at the end of the grid and takes focus.
    pub(crate) fn open_draft(&mut self, scope: DraftScope) -> DraftId {
        self.next_draft += 1;
        let draft = DraftId(self.next_draft);
        self.drafts.insert(draft, scope);
        self.panes.push(PaneIdentity::Draft(draft));
        self.focus_index(self.panes.len() - 1);
        draft
    }

    /// A pair member's leave is deferred onto this draft (`Cockpit::close`).
    pub(crate) fn defer_leave(&mut self, draft: DraftId, thread: ThreadId) {
        if let Some(scope) = self.drafts.get_mut(&draft) {
            scope.pending_leave = Some(thread);
        }
    }

    /// The first send made a Thread of the draft: the Thread takes the
    /// draft's own slot — not a new one at the end — and a fullscreen aimed
    /// at the draft now shows the Thread. The scope comes back so the
    /// Cockpit can apply the leave it was holding.
    pub(crate) fn draft_became(&mut self, draft: DraftId, thread: ThreadId) -> Option<DraftScope> {
        let scope = self.drafts.remove(&draft)?;
        let identity = PaneIdentity::Thread(thread);
        // `open` already gave the Thread a Pane at the end; the draft's slot
        // is the one the operator is looking at.
        self.panes.retain(|pane| *pane != identity);
        let slot = self.index_of(PaneIdentity::Draft(draft))?;
        self.panes[slot] = identity;
        if self.fullscreen == Some(PaneIdentity::Draft(draft)) {
            self.fullscreen = Some(identity);
        }
        self.focused = self.focused.min(self.panes.len().saturating_sub(1));
        Some(scope)
    }

    /// Discard a draft Pane: nothing durable dies with it. The scope comes
    /// back so the Cockpit can apply a leave it was holding.
    pub(crate) fn remove_draft(&mut self, draft: DraftId) -> Option<DraftScope> {
        let scope = self.drafts.get(&draft).copied()?;
        self.close_pane(PaneIdentity::Draft(draft));
        Some(scope)
    }

    // -------------------------------------------------------- park order

    pub(crate) fn note_parked(&mut self, thread: ThreadId) {
        self.park_order.push(thread);
    }

    pub(crate) fn note_revived(&mut self, thread: ThreadId) {
        self.park_order.retain(|parked| *parked != thread);
    }

    pub(crate) fn pop_park_order(&mut self) -> Option<ThreadId> {
        self.park_order.pop()
    }

    // -------------------------------------------------------------- heal

    /// A Group the operator was looking at no longer exists: Solo.
    pub(crate) fn heal_view(&mut self, groups: &Groups) {
        if let View::Group(group) = self.view {
            if groups.get(group).is_none() {
                self.view = View::Solo;
            }
        }
    }

    /// After a Group change: the view heals, and if the focused Pane is no
    /// longer on screen the first visible one takes focus — or, with
    /// nothing left to show in the Group, the view falls back to Solo.
    pub(crate) fn heal_focus(&mut self, groups: &Groups) {
        self.heal_view(groups);
        let View::Group(_) = self.view else {
            return;
        };
        let visible = self.visible(groups);
        if self
            .focused()
            .is_some_and(|focused| visible.contains(&focused))
        {
            return;
        }
        match visible.first() {
            Some(first) => {
                self.focus(*first);
            }
            None => self.view = View::Solo,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::groups::GroupChange;
    use crate::store::{Provider, Store};
    use crate::workspace::WorkspaceBinding;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ferrite-roster-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// `n` stored Threads on one project, and a Groups over them.
    fn groups(name: &str, n: usize) -> (Groups, Vec<ThreadId>) {
        let dir = scratch(name);
        let store = Store::open(&dir).unwrap();
        let mut registry = crate::workspace::registry::Registry::open(store.dir()).unwrap();
        let project = registry.register(&dir).unwrap();
        let threads: Vec<ThreadId> = (0..n)
            .map(|_| {
                store
                    .create(
                        Provider::Claude,
                        Some(project),
                        WorkspaceBinding::Main {
                            checkout: dir.clone(),
                        },
                    )
                    .unwrap()
                    .0
            })
            .collect();
        (Groups::load(store.dir()).unwrap(), threads)
    }

    fn roster(threads: &[ThreadId]) -> Roster {
        let mut roster = Roster::default();
        for thread in threads {
            roster.insert_thread(*thread);
        }
        roster
    }

    #[test]
    fn solo_shows_the_focused_pane_and_a_group_shows_its_membership_in_order() {
        let (mut groups, threads) = groups("visible", 4);
        let group = groups
            .apply(GroupChange::Create {
                first: threads[2],
                second: threads[0],
            })
            .unwrap()
            .group
            .unwrap();
        let mut roster = roster(&threads);
        assert_eq!(roster.visible(&groups), [PaneIdentity::Thread(threads[0])]);
        roster.focus(PaneIdentity::Thread(threads[3]));
        assert_eq!(roster.visible(&groups), [PaneIdentity::Thread(threads[3])]);
        assert_eq!(roster.layout(&groups).columns, 1);

        roster.set_view(View::Group(group));
        assert_eq!(
            roster.visible(&groups),
            [
                PaneIdentity::Thread(threads[2]),
                PaneIdentity::Thread(threads[0])
            ],
            "the Group's own order, not the grid's"
        );
        let draft = roster.open_draft(DraftScope {
            group: Some(group),
            pending_leave: None,
        });
        assert_eq!(roster.visible(&groups).len(), 3, "the pending draft shows");
        assert_eq!(roster.focused(), Some(PaneIdentity::Draft(draft)));
        roster.defer_leave(draft, threads[2]);
        assert_eq!(
            roster.visible(&groups),
            [
                PaneIdentity::Thread(threads[0]),
                PaneIdentity::Draft(draft)
            ],
            "a member whose leave waits on the draft is already gone"
        );
    }

    #[test]
    fn four_of_a_group_lay_out_on_the_tall_left_board_and_others_on_the_grid() {
        let (mut groups, threads) = groups("layout", 6);
        let group = groups
            .apply(GroupChange::Create {
                first: threads[0],
                second: threads[1],
            })
            .unwrap()
            .group
            .unwrap();
        for thread in &threads[2..] {
            groups
                .apply(GroupChange::Join {
                    thread: *thread,
                    group,
                    index: None,
                })
                .unwrap();
        }
        let mut roster = roster(&threads[..4]);
        roster.set_view(View::Group(group));
        assert_eq!(
            roster.layout(&groups),
            Layout {
                columns: 2,
                rows: 3,
                tall_left: true
            }
        );
        roster.insert_thread(threads[4]);
        roster.insert_thread(threads[5]);
        assert_eq!(
            roster.layout(&groups),
            Layout {
                columns: 3,
                rows: 2,
                tall_left: false
            }
        );
    }

    #[test]
    fn focus_is_the_one_door_and_fullscreen_follows_it() {
        let (groups, threads) = groups("fullscreen", 3);
        let mut roster = roster(&threads);
        roster.toggle_fullscreen();
        assert_eq!(roster.fullscreen(), Some(PaneIdentity::Thread(threads[0])));
        roster.focus(PaneIdentity::Thread(threads[1]));
        assert_eq!(
            roster.fullscreen(),
            Some(PaneIdentity::Thread(threads[1])),
            "a deliberate move re-aims the fullscreen"
        );
        // Closed by the operator: the survivor fills the screen.
        roster.close_pane(PaneIdentity::Thread(threads[1]));
        assert_eq!(roster.fullscreen(), Some(PaneIdentity::Thread(threads[2])));
        // Gone under the roster: the grid comes back.
        roster.remove_thread(threads[2]);
        assert_eq!(roster.fullscreen(), None);
        assert_eq!(roster.focused(), Some(PaneIdentity::Thread(threads[0])));
        roster.step(1, &groups);
        assert_eq!(roster.focused(), Some(PaneIdentity::Thread(threads[0])), "Solo steps nowhere");
        roster.toggle_fullscreen();
        roster.toggle_fullscreen();
        assert_eq!(roster.fullscreen(), None);
    }

    #[test]
    fn a_draft_becomes_a_thread_in_its_own_slot() {
        let (_groups, threads) = groups("became", 3);
        let mut roster = roster(&threads[..2]);
        let draft = roster.open_draft(DraftScope::default());
        roster.toggle_fullscreen();
        // `open` gives the new Thread a Pane at the end first.
        roster.insert_thread(threads[2]);
        let scope = roster.draft_became(draft, threads[2]).unwrap();
        assert_eq!(scope, DraftScope::default());
        assert_eq!(
            roster.panes(),
            [
                PaneIdentity::Thread(threads[0]),
                PaneIdentity::Thread(threads[1]),
                PaneIdentity::Thread(threads[2])
            ]
        );
        assert_eq!(roster.focused(), Some(PaneIdentity::Thread(threads[2])));
        assert_eq!(roster.fullscreen(), Some(PaneIdentity::Thread(threads[2])));
        assert_eq!(roster.draft_scope(draft), None);
    }

    #[test]
    fn healing_lands_on_the_first_visible_pane_or_falls_back_to_solo() {
        let (mut groups, threads) = groups("heal", 3);
        let group = groups
            .apply(GroupChange::Create {
                first: threads[0],
                second: threads[1],
            })
            .unwrap()
            .group
            .unwrap();
        let mut roster = roster(&threads);
        roster.set_view(View::Group(group));
        roster.focus(PaneIdentity::Thread(threads[2]));
        roster.heal_focus(&groups);
        assert_eq!(roster.focused(), Some(PaneIdentity::Thread(threads[0])));
        groups
            .apply(GroupChange::Leave { thread: threads[0] })
            .unwrap();
        roster.heal_focus(&groups);
        assert_eq!(roster.view(), View::Solo, "a dissolved Group heals to Solo");
    }
}
