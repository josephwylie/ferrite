//! The Roster: what the Cockpit shows, headless. Which Panes are open and
//! in what order, which one holds the keyboard, and the park order. The
//! window shows exactly the focused Pane; the window mirrors the roster
//! into its own Pane views and paints. No I/O: reviving and parking are the
//! Cockpit's, which asks the roster for the arithmetic and tells it what
//! happened.
//!
//! The roster shows exactly the open Threads plus the drafts: `open`,
//! `revive`, `park` and `delete` keep that invariant themselves, so a
//! Thread can never be open without a Pane or shown without a Session.

use std::collections::BTreeSet;

use crate::ThreadId;

/// A draft Pane's identity (#29): a Composer and a pre-prompt band with no
/// Thread yet. Minted here, stable until the first send makes a Thread of
/// it or the draft is discarded — never persisted as a fake Thread id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DraftId(u64);

impl DraftId {
    /// A draft id from its number — for a window mapping a stand-in leaf
    /// back to the draft it stood for; the roster mints the real ones.
    pub fn new(id: u64) -> Self {
        Self(id)
    }

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

#[derive(Debug, Default)]
pub struct Roster {
    panes: Vec<PaneIdentity>,
    focused: usize,
    /// Threads the operator parked this launch, oldest first — cmd-o pops
    /// the tail, the one just closed. In memory only, deliberately: the
    /// store keeps no park order, so a relaunch forgets it and reopen falls
    /// back to creation order (accepted v1 behavior).
    park_order: Vec<ThreadId>,
    /// The Thread focus last left: while a draft holds focus, the operator
    /// is still in that Thread's Project.
    left_thread: Option<ThreadId>,
    drafts: BTreeSet<DraftId>,
    next_draft: u64,
}

impl Roster {
    // ------------------------------------------------------------- reads

    /// Every open Pane, in order.
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

    /// The Thread the operator is in: the focused one, else the open one
    /// focus last left.
    pub fn current_thread(&self) -> Option<ThreadId> {
        self.focused_thread().or(self
            .left_thread
            .filter(|thread| self.pane_of(*thread).is_some()))
    }

    pub fn index_of(&self, identity: PaneIdentity) -> Option<usize> {
        self.panes.iter().position(|pane| *pane == identity)
    }

    pub fn pane_of(&self, thread: ThreadId) -> Option<usize> {
        self.index_of(PaneIdentity::Thread(thread))
    }

    pub fn has_draft(&self, draft: DraftId) -> bool {
        self.drafts.contains(&draft)
    }

    pub fn park_order(&self) -> &[ThreadId] {
        &self.park_order
    }

    /// The Pane on screen: exactly the focused one.
    pub fn visible(&self) -> Vec<PaneIdentity> {
        self.focused().into_iter().collect()
    }

    // ------------------------------------------------------------- focus

    /// The one door to `focused`: every move — keys, clicks, nav rows —
    /// lands here.
    pub(crate) fn focus(&mut self, identity: PaneIdentity) -> bool {
        let Some(index) = self.index_of(identity) else {
            return false;
        };
        self.focus_index(index);
        true
    }

    pub(crate) fn focus_index(&mut self, index: usize) {
        if let Some(thread) = self.focused_thread() {
            self.left_thread = Some(thread);
        }
        self.focused = index.min(self.panes.len().saturating_sub(1));
    }

    // ------------------------------------------------------------- panes

    /// A Thread became open: it takes the next Pane, and focus stays put.
    pub(crate) fn insert_thread(&mut self, thread: ThreadId) {
        let identity = PaneIdentity::Thread(thread);
        if !self.panes.contains(&identity) {
            self.panes.push(identity);
        }
    }

    /// A Thread stopped being open: its Pane goes, and a surviving focused
    /// Pane keeps focus.
    pub(crate) fn remove_thread(&mut self, thread: ThreadId) {
        self.remove(PaneIdentity::Thread(thread));
    }

    fn remove(&mut self, identity: PaneIdentity) {
        let focused = self.focused();
        self.panes.retain(|pane| *pane != identity);
        if let PaneIdentity::Draft(draft) = identity {
            self.drafts.remove(&draft);
        }
        self.focused = focused
            .and_then(|focused| self.index_of(focused))
            .unwrap_or_else(|| self.focused.min(self.panes.len().saturating_sub(1)));
    }

    // ------------------------------------------------------------ drafts

    /// A draft Pane (#29) opens at the end and takes focus.
    pub(crate) fn open_draft(&mut self) -> DraftId {
        self.next_draft += 1;
        let draft = DraftId(self.next_draft);
        self.drafts.insert(draft);
        self.panes.push(PaneIdentity::Draft(draft));
        self.focus_index(self.panes.len() - 1);
        draft
    }

    /// The first send made a Thread of the draft: the Thread takes the
    /// draft's own slot, not a new one at the end. False for a draft the
    /// roster no longer holds.
    pub(crate) fn draft_became(&mut self, draft: DraftId, thread: ThreadId) -> bool {
        if !self.drafts.remove(&draft) {
            return false;
        }
        let identity = PaneIdentity::Thread(thread);
        // `open` already gave the Thread a Pane at the end; the draft's slot
        // is the one the operator is looking at.
        self.panes.retain(|pane| *pane != identity);
        let Some(slot) = self.index_of(PaneIdentity::Draft(draft)) else {
            return false;
        };
        self.panes[slot] = identity;
        self.focused = self.focused.min(self.panes.len().saturating_sub(1));
        true
    }

    /// Discard a draft Pane: nothing durable dies with it. False for a
    /// draft the roster no longer holds.
    pub(crate) fn remove_draft(&mut self, draft: DraftId) -> bool {
        if !self.drafts.contains(&draft) {
            return false;
        }
        self.remove(PaneIdentity::Draft(draft));
        true
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roster(threads: &[ThreadId]) -> Roster {
        let mut roster = Roster::default();
        for thread in threads {
            roster.insert_thread(*thread);
        }
        roster
    }

    #[test]
    fn only_the_focused_pane_is_on_screen() {
        let threads: Vec<ThreadId> = (1..=3).map(ThreadId::new).collect();
        let mut roster = roster(&threads);
        assert_eq!(roster.visible(), [PaneIdentity::Thread(threads[0])]);
        roster.focus(PaneIdentity::Thread(threads[2]));
        assert_eq!(roster.visible(), [PaneIdentity::Thread(threads[2])]);
        roster.remove_thread(threads[2]);
        assert_eq!(roster.visible().len(), 1, "a survivor takes the screen");
        assert!(Roster::default().visible().is_empty());
    }

    #[test]
    fn a_draft_becomes_a_thread_in_its_own_slot() {
        let threads: Vec<ThreadId> = (1..=3).map(ThreadId::new).collect();
        let mut roster = roster(&threads[..2]);
        let draft = roster.open_draft();
        // `open` gives the new Thread a Pane at the end first.
        roster.insert_thread(threads[2]);
        assert!(roster.draft_became(draft, threads[2]));
        assert_eq!(
            roster.panes(),
            [
                PaneIdentity::Thread(threads[0]),
                PaneIdentity::Thread(threads[1]),
                PaneIdentity::Thread(threads[2])
            ]
        );
        assert_eq!(roster.focused(), Some(PaneIdentity::Thread(threads[2])));
        assert!(!roster.has_draft(draft));
    }
}
