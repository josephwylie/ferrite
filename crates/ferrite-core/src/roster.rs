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

use crate::{MachineId, ThreadKey};

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

/// Stable Pane identity across grid reflow: a Thread, by its key (ADR
/// 0045), or a draft.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaneIdentity {
    Thread(ThreadKey),
    Draft(DraftId),
}

impl PaneIdentity {
    pub fn thread(self) -> Option<ThreadKey> {
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
    /// The Pane the operator is in; `None` while they are in none. Every
    /// machine's open Threads have Panes here, and one that merely opened
    /// on another machine puts nobody in it (`takes_focus_unasked`).
    focused: Option<PaneIdentity>,
    /// Threads the operator parked this launch, oldest first — cmd-o pops
    /// the tail, the one just closed. In memory only, deliberately: the
    /// store keeps no park order, so a relaunch forgets it and reopen falls
    /// back to creation order (accepted v1 behavior).
    park_order: Vec<ThreadKey>,
    /// The Thread focus last left: while a draft holds focus, the operator
    /// is still in that Thread's Project.
    left_thread: Option<ThreadKey>,
    drafts: BTreeSet<DraftId>,
    next_draft: u64,
    /// Where a new Thread with no Project starts (`Cockpit::start_machine`):
    /// the one machine whose Threads take an empty screen.
    start: Option<MachineId>,
}

impl Roster {
    // ------------------------------------------------------------- reads

    /// Every open Pane, in order.
    pub fn panes(&self) -> &[PaneIdentity] {
        &self.panes
    }

    pub fn focused(&self) -> Option<PaneIdentity> {
        self.focused
    }

    pub fn focused_thread(&self) -> Option<ThreadKey> {
        self.focused().and_then(PaneIdentity::thread)
    }

    /// The Thread the operator is in: the focused one, or under a focused
    /// draft the open one focus last left. With no Pane focused, none.
    pub fn current_thread(&self) -> Option<ThreadKey> {
        match self.focused? {
            PaneIdentity::Thread(thread) => Some(thread),
            PaneIdentity::Draft(_) => self
                .left_thread
                .filter(|thread| self.pane_of(*thread).is_some()),
        }
    }

    pub fn index_of(&self, identity: PaneIdentity) -> Option<usize> {
        self.panes.iter().position(|pane| *pane == identity)
    }

    pub fn pane_of(&self, thread: ThreadKey) -> Option<usize> {
        self.index_of(PaneIdentity::Thread(thread))
    }

    pub fn has_draft(&self, draft: DraftId) -> bool {
        self.drafts.contains(&draft)
    }

    pub fn park_order(&self) -> &[ThreadKey] {
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
        if self.index_of(identity).is_none() {
            return false;
        }
        if let Some(thread) = self.focused_thread() {
            self.left_thread = Some(thread);
        }
        self.focused = Some(identity);
        true
    }

    /// The machine whose Threads take an empty screen, as machines come
    /// and go. What is focused stays.
    pub(crate) fn set_start(&mut self, machine: Option<MachineId>) {
        self.start = machine;
    }

    /// Whether `pane` may take focus with nobody pressing it: a draft, a
    /// Thread of the machine new Threads start on, or one of `beside`, the
    /// machine of the Pane focus is leaving.
    fn takes_focus_unasked(&self, pane: PaneIdentity, beside: Option<MachineId>) -> bool {
        pane.thread().is_none_or(|thread| {
            Some(thread.machine()) == beside
                || self.start.is_none_or(|start| start == thread.machine())
        })
    }

    // ------------------------------------------------------------- panes

    /// A Thread became open: it takes the next Pane, and focus stays put.
    /// With no Pane focused it takes the screen, unless it is another
    /// machine's: nobody went there.
    pub(crate) fn insert_thread(&mut self, thread: ThreadKey) {
        let identity = PaneIdentity::Thread(thread);
        if !self.panes.contains(&identity) {
            self.panes.push(identity);
        }
        if self.focused.is_none() && self.takes_focus_unasked(identity, None) {
            self.focused = Some(identity);
        }
    }

    /// A Thread stopped being open: its Pane goes, a surviving focused Pane
    /// keeps focus, and a focused one hands it to its neighbour.
    pub(crate) fn remove_thread(&mut self, thread: ThreadKey) {
        self.remove(PaneIdentity::Thread(thread));
    }

    fn remove(&mut self, identity: PaneIdentity) {
        let at = self.index_of(identity);
        self.panes.retain(|pane| *pane != identity);
        if let PaneIdentity::Draft(draft) = identity {
            self.drafts.remove(&draft);
        }
        if self.focused == Some(identity) {
            self.focused = at.and_then(|at| self.heir(identity, at));
        }
    }

    /// Who takes focus from `gone`, which stood at `at`: the Pane now
    /// there, else the nearest before it, of those that may take it
    /// unasked.
    fn heir(&self, gone: PaneIdentity, at: usize) -> Option<PaneIdentity> {
        let beside = gone.thread().map(ThreadKey::machine);
        let takes = |pane: &&PaneIdentity| self.takes_focus_unasked(**pane, beside);
        let (before, after) = self.panes.split_at(at.min(self.panes.len()));
        after
            .iter()
            .find(takes)
            .or_else(|| before.iter().rev().find(takes))
            .copied()
    }

    // ------------------------------------------------------------ drafts

    /// A draft Pane (#29) opens at the end and takes focus.
    pub(crate) fn open_draft(&mut self) -> DraftId {
        self.next_draft += 1;
        let draft = DraftId(self.next_draft);
        self.drafts.insert(draft);
        self.panes.push(PaneIdentity::Draft(draft));
        self.focus(PaneIdentity::Draft(draft));
        draft
    }

    /// The first send made a Thread of the draft: the Thread takes the
    /// draft's own slot, not a new one at the end. False for a draft the
    /// roster no longer holds.
    pub(crate) fn draft_became(&mut self, draft: DraftId, thread: ThreadKey) -> bool {
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
        if self.focused == Some(PaneIdentity::Draft(draft)) {
            self.focused = Some(identity);
        }
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

    pub(crate) fn note_parked(&mut self, thread: ThreadKey) {
        self.park_order.push(thread);
    }

    pub(crate) fn note_revived(&mut self, thread: ThreadKey) {
        self.park_order.retain(|parked| *parked != thread);
    }

    pub(crate) fn pop_park_order(&mut self) -> Option<ThreadKey> {
        self.park_order.pop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roster(threads: &[ThreadKey]) -> Roster {
        let mut roster = Roster::default();
        for thread in threads {
            roster.insert_thread(*thread);
        }
        roster
    }

    #[test]
    fn only_the_focused_pane_is_on_screen() {
        let threads: Vec<ThreadKey> = (1..=3)
            .map(|thread| ThreadKey::fixture(0, thread))
            .collect();
        let mut roster = roster(&threads);
        assert_eq!(roster.visible(), [PaneIdentity::Thread(threads[0])]);
        roster.focus(PaneIdentity::Thread(threads[2]));
        assert_eq!(roster.visible(), [PaneIdentity::Thread(threads[2])]);
        roster.remove_thread(threads[2]);
        assert_eq!(roster.visible().len(), 1, "a survivor takes the screen");
        assert!(Roster::default().visible().is_empty());
    }

    /// Every machine's open Threads have Panes, but only a press goes to
    /// another machine's: one arriving on an empty screen, or standing
    /// next to the Pane that closed, is not where the operator is.
    #[test]
    fn another_machines_thread_never_takes_focus_unasked() {
        let (here, there) = (ThreadKey::fixture(0, 1), ThreadKey::fixture(1, 1));
        let mut roster = Roster::default();
        roster.set_start(Some(here.machine()));

        roster.insert_thread(there);
        assert_eq!(roster.focused(), None, "it opened; nobody went there");
        assert_eq!(roster.current_thread(), None);
        assert!(roster.visible().is_empty(), "the board stays empty");

        roster.insert_thread(here);
        assert_eq!(roster.focused(), Some(PaneIdentity::Thread(here)));
        roster.remove_thread(here);
        assert_eq!(roster.focused(), None, "its neighbour is on another machine");

        assert!(roster.focus(PaneIdentity::Thread(there)));
        assert_eq!(roster.current_thread(), Some(there), "a press goes there");
        let beside = ThreadKey::fixture(1, 2);
        roster.insert_thread(beside);
        roster.remove_thread(there);
        assert_eq!(
            roster.focused(),
            Some(PaneIdentity::Thread(beside)),
            "closing one of its Threads stays on that machine"
        );
    }

    #[test]
    fn a_draft_becomes_a_thread_in_its_own_slot() {
        let threads: Vec<ThreadKey> = (1..=3)
            .map(|thread| ThreadKey::fixture(0, thread))
            .collect();
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
