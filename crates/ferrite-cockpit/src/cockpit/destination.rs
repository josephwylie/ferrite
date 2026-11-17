//! What stands in the board's place: a Project's Factory, or the PRs.

use super::*;

/// A destination over the board, and the Pane focused when it opened:
/// focus moving to another of the operator's own leaves it.
pub(crate) struct Destination {
    pub(super) shows: Shows,
    pub(super) focus_at_open: Option<PaneIdentity>,
}

pub(crate) enum Shows {
    /// A Project's Factory board (ADR 0037).
    Factory(factory::FactoryView),
    /// The PRs list. What it holds lives on `PrsView`, kept for the next
    /// visit.
    Prs,
}

impl CockpitView {
    /// The Factory, when it stands in the board's place.
    pub(super) fn factory(&self) -> Option<&factory::FactoryView> {
        match &self.destination.as_ref()?.shows {
            Shows::Factory(view) => Some(view),
            Shows::Prs => None,
        }
    }

    pub(super) fn factory_mut(&mut self) -> Option<&mut factory::FactoryView> {
        match &mut self.destination.as_mut()?.shows {
            Shows::Factory(view) => Some(view),
            Shows::Prs => None,
        }
    }

    /// Whether the PRs stand in the board's place.
    pub(super) fn prs_shown(&self) -> bool {
        matches!(
            self.destination,
            Some(Destination {
                shows: Shows::Prs,
                ..
            })
        )
    }

    /// A destination (the Factory, the PRs) stands in the board's place.
    pub(super) fn destination_shown(&self) -> bool {
        self.destination.is_some()
    }

    /// Whatever stands in the board's place gives it back.
    pub(super) fn leave_destination(&mut self) {
        self.destination = None;
    }

    /// Focus moved to another of the operator's own Panes (a new draft, a
    /// landing): the destination gives way to it.
    pub(super) fn leave_destination_if_moved(&mut self) {
        let Some(destination) = self.destination.as_ref() else {
            return;
        };
        let now = self.cockpit.roster().focused();
        if now == destination.focus_at_open {
            return;
        }
        // Read through the identity: an owned Thread just reopened may
        // have no Pane in this window yet.
        let owned = now
            .and_then(PaneIdentity::thread)
            .is_some_and(|thread| !self.cockpit.operators(thread));
        if !owned {
            self.destination = None;
        }
    }
}
