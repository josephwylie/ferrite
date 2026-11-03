//! Back and forward through where the operator has been (Claude's title-bar
//! arrows, ⌘[ / ⌘]): the Thread in Solo, a Group, and a Subagent opened
//! from a Pane.
//!
//! The history watches rather than being told. Every frame the Cockpit
//! names where it is (`CockpitView::place`) and `History::visit` keeps it:
//! a new place cuts the forward run and goes on the end, as a browser's
//! does. So every door — a nav row, ⌘1…9, the palette, a notice, a
//! Subagent's row, a draft's first send — is recorded without one of them
//! knowing. A step back or forward lands through the same doors the
//! pointer uses, and the frame after it names where it landed: that
//! replaces the entry stepped to (core may show a Thread in its Group
//! rather than alone), and is never a new visit.
//!
//! A draft and the empty board are no place: there is nothing to come
//! back to once they are gone. A place that went away — a Thread deleted,
//! a Group dissolved — is pruned before it can be stepped to, so the arrows
//! are disabled exactly when there is nowhere to go.

use super::*;
use ferrite_core::activity::Subject;

actions!(cockpit, [GoBack, GoForward]);

/// How many places the history keeps: the oldest goes first.
const DEPTH: usize = 50;

/// Somewhere the history can go back to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    /// A Thread shown alone, on its Main transcript or a Subagent's.
    Thread(ThreadId, Subject),
    /// A Group, and the Pane in it opened on a Subagent, if one is. Focus
    /// moving between its Panes is not a place; opening a Subagent is.
    Group(GroupId, Option<(ThreadId, Subject)>),
}

impl Place {
    /// The Threads this place needs to exist.
    fn threads(&self) -> impl Iterator<Item = ThreadId> + '_ {
        match self {
            Place::Thread(thread, _) => Some(*thread),
            Place::Group(_, drill) => drill.as_ref().map(|(thread, _)| *thread),
        }
        .into_iter()
    }
}

#[derive(Default)]
pub(crate) struct History {
    places: Vec<Place>,
    /// The entry the operator is on.
    at: usize,
    /// A step was taken and has not landed yet: the next place named is
    /// where it landed, not a visit.
    stepping: bool,
}

impl History {
    /// Where the Cockpit is this frame. `None` is a draft or the empty
    /// board: nothing is kept, and a step that ended there has landed.
    pub(crate) fn visit(&mut self, place: Option<Place>) {
        let stepping = std::mem::take(&mut self.stepping);
        let Some(place) = place else {
            return;
        };
        if stepping {
            if let Some(entry) = self.places.get_mut(self.at) {
                *entry = place;
            }
            self.fold_repeats();
            return;
        }
        if self.places.get(self.at) == Some(&place) {
            return;
        }
        self.places.truncate(self.at + 1);
        self.places.push(place);
        if self.places.len() > DEPTH {
            self.places.remove(0);
        }
        self.at = self.places.len() - 1;
    }

    /// Drop every place that can no longer be shown, keeping the operator
    /// on the same entry when it survives and on the one before it when not.
    pub(crate) fn prune(&mut self, reachable: impl Fn(&Place) -> bool) {
        let mut at = self.at;
        let mut index = 0;
        self.places.retain(|place| {
            let keep = reachable(place);
            if !keep && index <= self.at {
                at = at.saturating_sub(1);
            }
            index += 1;
            keep
        });
        // The entry the operator was on went, so the one before it stands
        // in; nothing before it, and the first survivor does.
        self.at = at.min(self.places.len().saturating_sub(1));
        self.fold_repeats();
    }

    pub(crate) fn can_go_back(&self) -> bool {
        self.at > 0
    }

    pub(crate) fn can_go_forward(&self) -> bool {
        self.at + 1 < self.places.len()
    }

    /// One entry back or forward (`delta` ±1), the place to show, if there
    /// is one that way.
    pub(crate) fn step(&mut self, delta: isize) -> Option<Place> {
        let to = self.at.checked_add_signed(delta)?;
        let place = self.places.get(to)?.clone();
        self.at = to;
        self.stepping = true;
        Some(place)
    }

    /// Two neighbours that name the same place are one entry: pruning or a
    /// step that landed elsewhere can leave them side by side.
    fn fold_repeats(&mut self) {
        let mut index = 1;
        while index < self.places.len() {
            if self.places[index] == self.places[index - 1] {
                self.places.remove(index);
                if self.at >= index {
                    self.at -= 1;
                }
            } else {
                index += 1;
            }
        }
    }
}

impl CockpitView {
    /// Where the Cockpit is this frame, as the history keeps it.
    fn place(&self) -> Option<Place> {
        if self.empty_board_shown() {
            return None;
        }
        let subject_of = |thread: ThreadId| {
            let index = self.pane_for(thread)?;
            Some(self.panes[index].selected.clone())
        };
        match self.cockpit.roster().view() {
            View::Solo => {
                let thread = self.focused_thread()?;
                Some(Place::Thread(thread, subject_of(thread)?))
            }
            View::Group(group) => {
                let drill = self.focused_thread().and_then(|thread| {
                    let subject = subject_of(thread)?;
                    (subject != Subject::Main).then_some((thread, subject))
                });
                Some(Place::Group(group, drill))
            }
        }
    }

    /// The root render's hook: forget what is gone, keep where the
    /// Cockpit is.
    pub(super) fn tend_history(&mut self) {
        let parked = self.facts.parked();
        let cockpit = &self.cockpit;
        let exists =
            |thread: ThreadId| cockpit.thread(thread).is_some() || parked.contains(&thread);
        self.history.prune(|place| {
            let group = match place {
                Place::Group(group, _) => cockpit.groups().get(*group).is_some(),
                Place::Thread(..) => true,
            };
            group && place.threads().all(&exists)
        });
        let place = self.place();
        self.history.visit(place);
    }

    pub(super) fn go_back(&mut self, _: &GoBack, window: &mut Window, cx: &mut Context<Self>) {
        self.step_history(-1, window, cx);
    }

    pub(super) fn go_forward(
        &mut self,
        _: &GoForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_history(1, window, cx);
    }

    /// Show the place one entry back or forward, through the doors the
    /// pointer uses: a parked Thread is revived, a Group entered, a
    /// Subagent opened — or put away for Main.
    pub(super) fn step_history(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(place) = self.history.step(delta) else {
            return;
        };
        match place {
            Place::Thread(thread, subject) => {
                if self.pane_for(thread).is_some() {
                    self.focus_thread(thread, cx);
                } else {
                    self.revive_thread(thread, cx);
                }
                self.show_subject(thread, subject, window, cx);
            }
            Place::Group(group, drill) => {
                // Leaving a Subagent for its Group's Main: the Pane that
                // was opened on it goes back to Main.
                let from = self.focused_thread();
                self.enter_group(group, cx);
                match drill {
                    Some((thread, subject)) => self.show_subject(thread, subject, window, cx),
                    None => {
                        if let Some(thread) = from {
                            self.show_subject(thread, Subject::Main, window, cx);
                        }
                    }
                }
            }
        }
        cx.notify();
    }

    /// `thread`'s Pane on `subject`, unless it already is.
    fn show_subject(
        &mut self,
        thread: ThreadId,
        subject: Subject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let shown = self
            .pane_for(thread)
            .map(|index| self.panes[index].selected == subject);
        if shown == Some(false) {
            self.select_subject(thread, subject, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(id: u64) -> Place {
        Place::Thread(ThreadId::new(id), Subject::Main)
    }

    fn walked(places: &[u64]) -> History {
        let mut history = History::default();
        for id in places {
            history.visit(Some(thread(*id)));
        }
        history
    }

    #[test]
    fn a_visit_goes_on_the_end_and_a_repeat_is_not_one() {
        let mut history = walked(&[1, 2, 2, 3]);
        assert_eq!(history.places, [thread(1), thread(2), thread(3)]);
        assert!(history.can_go_back());
        assert!(!history.can_go_forward(), "disabled at the end");
        assert_eq!(history.step(1), None);
        history.visit(None);
        assert_eq!(history.places.len(), 3, "a draft is no place");
    }

    #[test]
    fn stepping_back_then_visiting_cuts_the_forward_run() {
        let mut history = walked(&[1, 2, 3]);
        assert_eq!(history.step(-1), Some(thread(2)));
        history.visit(Some(thread(2)));
        assert!(history.can_go_forward());
        history.visit(Some(thread(4)));
        assert_eq!(history.places, [thread(1), thread(2), thread(4)]);
        assert!(!history.can_go_forward());
    }

    /// Where a step lands replaces the entry stepped to: core showed the
    /// Thread in its Group, and that is no new visit.
    #[test]
    fn a_step_that_lands_elsewhere_replaces_its_entry() {
        let mut history = walked(&[1, 2, 3]);
        history.step(-1);
        let group = Place::Group(GroupId::new(9), None);
        history.visit(Some(group.clone()));
        assert_eq!(history.places, [thread(1), group, thread(3)]);
        assert!(history.can_go_forward(), "the forward run survives");
    }

    #[test]
    fn a_place_that_went_away_is_never_stepped_to() {
        let mut history = walked(&[1, 2, 3, 2]);
        history.prune(|place| *place != thread(3));
        assert_eq!(history.places, [thread(1), thread(2)], "the two 2s fold");
        assert_eq!(history.at, 1);
        let mut history = walked(&[1, 2, 3]);
        history.step(-1);
        history.visit(Some(thread(2)));
        history.prune(|place| *place != thread(2));
        assert_eq!(history.places, [thread(1), thread(3)]);
        assert_eq!(history.at, 0, "the entry before the gone one stands in");
        assert!(history.can_go_forward());
    }

    #[test]
    fn the_oldest_place_goes_first() {
        let history = walked(&(0..DEPTH as u64 + 5).collect::<Vec<_>>());
        assert_eq!(history.places.len(), DEPTH);
        assert_eq!(history.places[0], thread(5));
        assert_eq!(history.at, DEPTH - 1);
    }
}
