//! A nav act on several Threads, and the way back. Each Thread's try is its
//! own `Outcome`, so a failure is named where it happened and never hides
//! the others. A Park remembers the Group seats it took the Thread out of,
//! and ⌘Z puts the Thread back in exactly those.

use super::*;
use ferrite_core::groups::Group;
use std::collections::HashMap;

/// One Thread's try at a nav act: what the operator reads when it failed.
pub(super) struct Outcome {
    pub thread: ThreadId,
    pub result: Result<(), String>,
}

/// A Group a Thread sat in when it was parked.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Seat {
    group: GroupId,
    title: String,
    /// The Group's members, in its order, as the Thread left.
    members: Vec<ThreadId>,
    /// Whether the Thread was a guest there rather than at home.
    guest: bool,
    /// Whether the Group had guests: a Group made anew could not seat them.
    guests: bool,
}

impl Seat {
    fn of(group: &Group, thread: ThreadId) -> Self {
        Seat {
            group: group.id,
            title: group.title.clone(),
            members: group.members.clone(),
            guest: group.guests.contains(&thread),
            guests: !group.guests.is_empty(),
        }
    }
}

/// A Thread a Park took, and the seats it left.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ParkedThread {
    thread: ThreadId,
    seats: Vec<Seat>,
}

impl CockpitView {
    /// Park each of `threads` (reversible: the Parked shelf holds them,
    /// and ⌘Z takes it back while the foot's notice stands). A Thread that
    /// would not park is named and stays picked; Undo holds only the rest.
    pub(super) fn park_listed(&mut self, threads: &[ThreadId], cx: &mut Context<Self>) {
        let mut parked = Vec::new();
        let mut outcomes = Vec::new();
        for &thread in threads {
            let seats = self
                .cockpit
                .groups()
                .all_of(thread)
                .map(|group| Seat::of(group, thread))
                .collect();
            let result = self.cockpit.park_thread(thread).map_err(|e| e.to_string());
            if result.is_ok() {
                parked.push(ParkedThread { thread, seats });
            }
            outcomes.push(Outcome { thread, result });
        }
        self.nav_picked
            .retain(|picked| !parked.iter().any(|parked| parked.thread == *picked));
        self.report_failures("park", &outcomes);
        self.sync_panes(cx);
        if !parked.is_empty() {
            self.offer_undo(nav_chrome::UndoAct::Parked(parked), cx);
        }
        cx.notify();
    }

    /// The notice for every failed outcome, each by its Thread's name; none
    /// when all went through.
    pub(super) fn report_failures(&mut self, verb: &str, outcomes: &[Outcome]) {
        let failed: Vec<String> = outcomes
            .iter()
            .filter_map(|outcome| {
                let why = outcome.result.as_ref().err()?;
                Some(format!("{}: {why}", self.facts.name(outcome.thread)))
            })
            .collect();
        self.group_error = (!failed.is_empty())
            .then(|| format!("Couldn't {verb} {}", failed.join("; ")).into());
    }

    /// ⌘Z on a Park: the Threads revive, last parked first, each back in
    /// its Groups.
    pub(super) fn undo_parked(&mut self, parked: Vec<ParkedThread>, cx: &mut Context<Self>) {
        // A Group the Park dissolved comes back under a new id: later seats
        // in it follow.
        let mut remade = HashMap::new();
        let outcomes: Vec<Outcome> = parked
            .into_iter()
            .rev()
            .map(|parked| {
                let result = self
                    .try_revive(parked.thread, cx)
                    .map_err(|e| e.to_string())
                    .and_then(|()| {
                        parked.seats.iter().try_for_each(|seat| {
                            self.reseat(parked.thread, seat, &mut remade)
                                .map_err(|why| format!("back, but not in its group: {why}"))
                        })
                    });
                Outcome { thread: parked.thread, result }
            })
            .collect();
        self.report_failures("undo", &outcomes);
        self.sync_panes(cx);
    }

    /// Put `thread`, revived, back in the Group `seat` names.
    fn reseat(
        &mut self,
        thread: ThreadId,
        seat: &Seat,
        remade: &mut HashMap<GroupId, GroupId>,
    ) -> Result<(), String> {
        let group = remade.get(&seat.group).copied().unwrap_or(seat.group);
        let index = seat.members.iter().position(|member| *member == thread);
        if let Some(now) = self.cockpit.groups().get(group) {
            if now.members.contains(&thread) {
                return Ok(());
            }
            let index = index.map(|at| at.min(now.members.len()));
            let change = if seat.guest {
                GroupChange::Include { thread, group, index }
            } else {
                GroupChange::Join { thread, group, index }
            };
            return self.apply(change).map(drop);
        }
        let title = if seat.title.trim().is_empty() {
            "the group".to_string()
        } else {
            seat.title.clone()
        };
        let [first, second] = seat.members[..] else {
            return Err(format!("{title} has changed since"));
        };
        let other = if first == thread { second } else { first };
        if seat.guests || self.cockpit.groups().of(other).is_some() {
            return Err(format!("{title} has changed since"));
        }
        if self.cockpit.thread(other).is_none() {
            return Err(format!("{} is not open", self.facts.name(other)));
        }
        let made = self.apply(GroupChange::Create { first, second })?;
        if let Some(made) = made {
            remade.insert(seat.group, made);
            if !seat.title.is_empty() {
                self.apply(GroupChange::Rename { group: made, title: seat.title.clone() })?;
            }
        }
        Ok(())
    }

    fn apply(&mut self, change: GroupChange) -> Result<Option<GroupId>, String> {
        self.cockpit
            .apply_group(change)
            .map(|applied| applied.group)
            .map_err(|e| e.to_string())
    }
}
