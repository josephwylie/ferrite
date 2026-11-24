//! What a sync does, worked out from three manifests alone: the state both
//! sides last agreed on (the base), and what each side holds now. A side
//! that changed a path since the base has its way; when both did, the
//! same way is nothing, an edit beats a removal, and of two edits the
//! newer wins, the other kept aside. The first sync, with no base, is the
//! union of both sides and removes nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;

use serde::Serialize;

use crate::tree::{executable_differs, Entry, Manifest, Side};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// This computer's entry taken to the Machine.
    Push(String),
    /// The Machine's entry taken here.
    Pull(String),
    RemoveHere(String),
    RemoveThere(String),
    /// Both sides changed the file, differently: the `winner`'s is taken
    /// to the other side, the other's kept aside first.
    Conflict { path: String, winner: Side },
    /// Changed, and left as each side has it, for `why`.
    Hold { path: String, why: Hold },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Hold {
    /// It would be written on `Side`, which reaches it through a link.
    Linked(Side),
    /// A file on one side and a folder on the other.
    Kinds,
}

impl Action {
    pub fn path(&self) -> &str {
        match self {
            Action::Push(path) | Action::Pull(path) | Action::RemoveHere(path) | Action::RemoveThere(path) => path,
            Action::Conflict { path, .. } | Action::Hold { path, .. } => path,
        }
    }

    /// The side it writes or removes on.
    fn side(&self) -> Option<Side> {
        match self {
            Action::Pull(_) | Action::RemoveHere(_) | Action::Conflict { winner: Side::There, .. } => Some(Side::Here),
            Action::Push(_) | Action::RemoveThere(_) | Action::Conflict { winner: Side::Here, .. } => Some(Side::There),
            Action::Hold { .. } => None,
        }
    }

    fn removes(&self) -> bool {
        matches!(self, Action::RemoveHere(_) | Action::RemoveThere(_))
    }
}

/// What makes `here` and `there` agree, given `base`, in path order. What
/// either side could not list, and all under it, is left as each side has
/// it, and so is each folder that holds it.
pub fn reconcile(base: &Manifest, here: &Manifest, there: &Manifest) -> Vec<Action> {
    let sides = Sides { here, there };
    let mut held = Held::of(here, there);
    let mut actions = BTreeMap::new();
    for path in paths(base, here, there) {
        if held.covers(path) {
            continue;
        }
        let [b, h, t] = [base, here, there].map(|manifest| manifest.entries.get(path));
        if [b, h, t].into_iter().flatten().any(|entry| matches!(entry, Entry::Unknown)) {
            held.0.insert(path);
            continue;
        }
        let Some(action) = decide(path, b, h, t, &sides) else {
            continue;
        };
        if matches!(action, Action::Hold { why: Hold::Kinds, .. }) {
            held.0.insert(path);
        }
        actions.insert(path, action);
    }
    keep_what_survives(&mut actions, &held, &sides);
    let actions = outermost(actions, &sides);
    actions.into_iter().map(|action| through_links(action, &sides)).collect()
}

/// What both sides hold alike now, by path, and the base's entry where
/// they differ or one of them could not list it: the next sync's base.
pub fn agreed(base: &Manifest, here: &Manifest, there: &Manifest) -> Manifest {
    let held = Held::of(here, there);
    let mut agreed = Manifest::default();
    for path in paths(base, here, there) {
        let [b, h, t] = [base, here, there].map(|manifest| manifest.entries.get(path));
        let entry = if !held.covers(path) && same(h, t) { h } else { b };
        if let Some(entry) = entry {
            agreed.entries.insert(path.to_string(), entry.clone());
        }
    }
    agreed
}

/// What one path needs, on its own.
fn decide(path: &str, b: Option<&Entry>, h: Option<&Entry>, t: Option<&Entry>, sides: &Sides) -> Option<Action> {
    let path = path.to_string();
    let taken = |side: Side, entry: Option<&Entry>| match (side, entry) {
        (Side::Here, Some(_)) => Action::Push(path.clone()),
        (Side::Here, None) => Action::RemoveThere(path.clone()),
        (Side::There, Some(_)) => Action::Pull(path.clone()),
        (Side::There, None) => Action::RemoveHere(path.clone()),
    };
    Some(match (!same(b, h), !same(b, t)) {
        (false, false) => return None,
        (true, false) => taken(Side::Here, h),
        (false, true) => taken(Side::There, t),
        (true, true) if same(h, t) => return None,
        (true, true) => match (h, t) {
            (None, Some(_)) => Action::Pull(path),
            (Some(_), None) => Action::Push(path),
            (Some(Entry::File { modified: mh, .. }), Some(Entry::File { modified: mt, .. })) => {
                let winner = match (sides.linked(Side::Here, &path), sides.linked(Side::There, &path)) {
                    (true, false) => Side::Here,
                    (false, true) => Side::There,
                    _ if matches!((mh, mt), (Some(h), Some(t)) if t > h) => Side::There,
                    _ => Side::Here,
                };
                Action::Conflict { path, winner }
            }
            _ => Action::Hold { path, why: Hold::Kinds },
        },
    })
}

/// A folder is removed from a side only when nothing under it stays
/// there: one whose removal would take what changed or is held there is
/// made again on the other side instead, and a file put in its place is
/// a clash of kinds. Deepest first, so a folder kept keeps each holding
/// it.
fn keep_what_survives(actions: &mut BTreeMap<&str, Action>, held: &Held, sides: &Sides) {
    let paths: Vec<&str> = actions.keys().rev().copied().collect();
    for path in paths {
        let Some(action) = actions.get(path) else {
            continue;
        };
        let (on, removal) = match action {
            Action::RemoveThere(_) => (Side::There, Action::RemoveThere as fn(String) -> Action),
            Action::RemoveHere(_) => (Side::Here, Action::RemoveHere as fn(String) -> Action),
            Action::Push(_) => (Side::There, Action::RemoveThere as fn(String) -> Action),
            Action::Pull(_) => (Side::Here, Action::RemoveHere as fn(String) -> Action),
            _ => continue,
        };
        if !matches!(sides.of(on).entries.get(path), Some(Entry::Dir)) {
            continue;
        }
        let under = format!("{path}/");
        let stays = sides
            .of(on)
            .entries
            .range::<str, _>((Bound::Included(under.as_str()), Bound::Unbounded))
            .take_while(|(at, _)| at.starts_with(&under))
            .any(|(at, _)| held.covers(at) || actions.get(at.as_str()).is_none_or(|action| *action != removal(at.clone())))
            || held.0.iter().any(|at| at.starts_with(&under));
        if !stays {
            continue;
        }
        let kept = match action {
            Action::RemoveThere(_) => Action::Pull(path.to_string()),
            Action::RemoveHere(_) => Action::Push(path.to_string()),
            _ => Action::Hold {
                path: path.to_string(),
                why: Hold::Kinds,
            },
        };
        actions.insert(path, kept);
    }
}

/// Each removal but those under a folder the same side loses, and nothing
/// under a clash of kinds.
fn outermost(actions: BTreeMap<&str, Action>, sides: &Sides) -> Vec<Action> {
    let mut gone: Vec<(String, Option<Side>)> = Vec::new();
    let mut kept = Vec::new();
    for (path, action) in actions {
        let under = |(at, side): &(String, Option<Side>)| {
            path.starts_with(at.as_str())
                && path.as_bytes().get(at.len()) == Some(&b'/')
                && (side.is_none() || *side == action.side())
        };
        if gone.iter().any(under) {
            continue;
        }
        let dir = |side: Side| matches!(sides.of(side).entries.get(path), Some(Entry::Dir));
        let replaced = action.side().filter(|side| dir(*side) && !dir(side.other()));
        match &action {
            Action::Hold { why: Hold::Kinds, .. } => gone.push((path.to_string(), None)),
            action if action.removes() || replaced.is_some() => gone.push((path.to_string(), action.side())),
            _ => {}
        }
        kept.push(action);
    }
    kept
}

/// `action`, or a hold where it would write through a link on the side it
/// writes to. A link itself may go, never what it points to.
fn through_links(action: Action, sides: &Sides) -> Action {
    let Some(side) = action.side() else {
        return action;
    };
    let path = action.path();
    let blocked = if action.removes() {
        sides.of(side).links.iter().any(|link| path.starts_with(&format!("{link}/")))
    } else {
        sides.linked(side, path)
    };
    if blocked {
        Action::Hold {
            path: path.to_string(),
            why: Hold::Linked(side),
        }
    } else {
        action
    }
}

struct Sides<'a> {
    here: &'a Manifest,
    there: &'a Manifest,
}

impl Sides<'_> {
    fn of(&self, side: Side) -> &Manifest {
        match side {
            Side::Here => self.here,
            Side::There => self.there,
        }
    }

    /// Whether `side` reaches `path` through a link: it, or a folder it
    /// lies in.
    fn linked(&self, side: Side, path: &str) -> bool {
        let links = &self.of(side).links;
        links.contains(path) || path.match_indices('/').any(|(at, _)| links.contains(&path[..at]))
    }
}

/// The paths left as each side has them, with all under them.
struct Held<'a>(BTreeSet<&'a str>);

impl<'a> Held<'a> {
    fn of(here: &'a Manifest, there: &'a Manifest) -> Self {
        Self(here.problems.keys().chain(there.problems.keys()).map(String::as_str).collect())
    }

    fn covers(&self, path: &str) -> bool {
        self.0.contains(path) || path.match_indices('/').any(|(at, _)| self.0.contains(&path[..at]))
    }
}

fn paths<'a>(base: &'a Manifest, here: &'a Manifest, there: &'a Manifest) -> BTreeSet<&'a str> {
    [base, here, there]
        .into_iter()
        .flat_map(|manifest| manifest.entries.keys())
        .map(String::as_str)
        .collect()
}

/// Whether two entries hold the same: a file by its bytes and, where both
/// sides keep them, its executable bits.
fn same(one: Option<&Entry>, other: Option<&Entry>) -> bool {
    match (one, other) {
        (None, None) | (Some(Entry::Dir), Some(Entry::Dir)) => true,
        (
            Some(Entry::File { size, sha256, mode, .. }),
            Some(Entry::File {
                size: other_size,
                sha256: other_sha256,
                mode: other_mode,
                ..
            }),
        ) => size == other_size && sha256 == other_sha256 && !executable_differs(*mode, *other_mode),
        (Some(Entry::Symlink { target }), Some(Entry::Symlink { target: other })) => target == other,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Side::{Here, There};

    fn file(bytes: &str, modified: u64) -> Entry {
        Entry::File {
            size: bytes.len() as u64,
            sha256: bytes.into(),
            mode: None,
            modified: Some(modified),
        }
    }

    fn manifest(entries: &[(&str, Entry)]) -> Manifest {
        Manifest {
            entries: entries.iter().map(|(path, entry)| (path.to_string(), entry.clone())).collect(),
            ..Manifest::default()
        }
    }

    fn one(entry: Option<Entry>) -> Manifest {
        manifest(&entry.map(|entry| ("f", entry)).into_iter().collect::<Vec<_>>())
    }

    /// A side's state of one file against the base.
    #[derive(Debug, Clone, Copy)]
    enum State {
        Absent,
        Unchanged,
        Changed,
        Created,
        Deleted,
    }

    /// Each side's file, given its state, where the base holds `old` (or
    /// nothing, for a side that created it). Both sides that change it
    /// change it to bytes of their own.
    fn entry(state: State, side: Side) -> (Option<Entry>, Option<Entry>) {
        let mine = match side {
            Here => file("here", 2),
            There => file("there", 1),
        };
        match state {
            State::Absent => (None, None),
            State::Unchanged => (Some(file("old", 0)), Some(file("old", 0))),
            State::Changed => (Some(file("old", 0)), Some(mine)),
            State::Created => (None, Some(mine)),
            State::Deleted => (Some(file("old", 0)), None),
        }
    }

    #[test]
    fn every_pair_of_what_each_side_did_to_a_file() {
        use State::*;
        let push = || vec![Action::Push("f".into())];
        let pull = || vec![Action::Pull("f".into())];
        let newer_here = || vec![Action::Conflict { path: "f".into(), winner: Here }];
        let table: &[(State, State, Vec<Action>)] = &[
            (Unchanged, Unchanged, vec![]),
            (Changed, Unchanged, push()),
            (Unchanged, Changed, pull()),
            (Deleted, Unchanged, vec![Action::RemoveThere("f".into())]),
            (Unchanged, Deleted, vec![Action::RemoveHere("f".into())]),
            (Changed, Changed, newer_here()),
            (Deleted, Deleted, vec![]),
            (Deleted, Changed, pull()),
            (Changed, Deleted, push()),
            (Created, Absent, push()),
            (Absent, Created, pull()),
            (Created, Created, newer_here()),
        ];
        for (here_did, there_did, expected) in table {
            let (base, here) = entry(*here_did, Here);
            let (other_base, there) = entry(*there_did, There);
            let base = base.or(other_base);
            assert_eq!(
                reconcile(&one(base), &one(here), &one(there)),
                *expected,
                "here {here_did:?}, there {there_did:?}"
            );
        }
    }

    #[test]
    fn a_file_on_only_one_side_goes_to_the_other_and_the_first_sync_removes_nothing() {
        let here = manifest(&[("a", file("a", 0)), ("both", file("same", 0)), ("mine", Entry::Dir), ("mine/x", file("x", 0))]);
        let there = manifest(&[("b", file("b", 0)), ("both", file("same", 9))]);
        assert_eq!(
            reconcile(&Manifest::default(), &here, &there),
            [
                Action::Push("a".into()),
                Action::Pull("b".into()),
                Action::Push("mine".into()),
                Action::Push("mine/x".into()),
            ]
        );
    }

    #[test]
    fn both_changed_to_the_same_bytes_is_nothing() {
        let base = one(Some(file("old", 0)));
        assert!(reconcile(&base, &one(Some(file("new", 5))), &one(Some(file("new", 7)))).is_empty());
    }

    #[test]
    fn of_two_edits_the_newer_wins_and_a_tie_goes_to_this_computer() {
        let base = one(Some(file("old", 0)));
        let conflict = |here: u64, there: u64| reconcile(&base, &one(Some(file("h", here))), &one(Some(file("t", there))));
        assert_eq!(conflict(1, 2), [Action::Conflict { path: "f".into(), winner: There }]);
        assert_eq!(conflict(2, 1), [Action::Conflict { path: "f".into(), winner: Here }]);
        assert_eq!(conflict(2, 2), [Action::Conflict { path: "f".into(), winner: Here }]);
    }

    #[test]
    fn a_removed_folder_goes_whole_unless_something_in_it_changed_on_the_other_side() {
        let base = manifest(&[("a", Entry::Dir), ("a/old", file("o", 0)), ("b", Entry::Dir), ("b/old", file("o", 0))]);
        let here = Manifest::default();
        let there = manifest(&[("a", Entry::Dir), ("a/old", file("o", 0)), ("b", Entry::Dir), ("b/old", file("o", 0)), ("b/new", file("n", 1))]);
        assert_eq!(
            reconcile(&base, &here, &there),
            [
                Action::RemoveThere("a".into()),
                Action::Pull("b".into()),
                Action::Pull("b/new".into()),
                Action::RemoveThere("b/old".into()),
            ]
        );
    }

    #[test]
    fn a_folder_kept_deep_down_keeps_each_folder_that_holds_it() {
        let base = manifest(&[("a", Entry::Dir), ("a/b", Entry::Dir), ("a/b/c", file("c", 0))]);
        let there = manifest(&[("a", Entry::Dir), ("a/b", Entry::Dir), ("a/b/c", file("c", 0)), ("a/b/new", file("n", 1))]);
        assert_eq!(
            reconcile(&base, &Manifest::default(), &there),
            [
                Action::Pull("a".into()),
                Action::Pull("a/b".into()),
                Action::RemoveThere("a/b/c".into()),
                Action::Pull("a/b/new".into()),
            ]
        );
    }

    #[test]
    fn a_file_and_a_folder_at_one_path_are_held_with_all_under_it() {
        let here = manifest(&[("x", file("x", 0))]);
        let there = manifest(&[("x", Entry::Dir), ("x/inner", file("i", 0))]);
        assert_eq!(
            reconcile(&Manifest::default(), &here, &there),
            [Action::Hold { path: "x".into(), why: Hold::Kinds }]
        );

        let base = manifest(&[("x", Entry::Dir), ("x/a", file("a", 0))]);
        let replaced = manifest(&[("x", file("x", 0))]);
        let added_to = manifest(&[("x", Entry::Dir), ("x/a", file("a", 0)), ("x/new", file("n", 1))]);
        assert_eq!(
            reconcile(&base, &replaced, &added_to),
            [Action::Hold { path: "x".into(), why: Hold::Kinds }],
            "a file put over a folder the other side added to"
        );
        let unchanged = manifest(&[("x", Entry::Dir), ("x/a", file("a", 0))]);
        assert_eq!(reconcile(&base, &replaced, &unchanged), [Action::Push("x".into())], "the folder goes for the file");
    }

    #[test]
    fn what_a_side_could_not_list_is_held_and_no_folder_holding_it_is_removed() {
        let base = manifest(&[("a", Entry::Dir), ("a/secret", file("s", 0)), ("a/x", file("x", 0)), ("b", file("b", 0))]);
        let here = Manifest::default();
        let mut there = manifest(&[("a", Entry::Dir), ("a/x", file("x", 0)), ("b", file("b2", 1))]);
        there.problems.insert("a/secret".into(), "Permission denied".into());
        assert_eq!(
            reconcile(&base, &here, &there),
            [Action::Pull("a".into()), Action::RemoveThere("a/x".into()), Action::Pull("b".into())]
        );

        let mut unlisted = manifest(&[("b", file("b", 0))]);
        unlisted.problems.insert("a".into(), "Permission denied".into());
        assert!(reconcile(&base, &unlisted, &base).is_empty(), "nothing under a folder this side could not list looks gone");
    }

    #[test]
    fn nothing_is_written_through_a_link_and_the_linked_side_wins_a_conflict() {
        let base = manifest(&[("skills", Entry::Dir), ("skills/a", file("old", 0)), ("skills/b", file("old", 0))]);
        let mut here = manifest(&[("skills", Entry::Dir), ("skills/a", file("new", 1)), ("skills/b", file("mine", 1))]);
        let mut there = manifest(&[("skills", Entry::Dir), ("skills/a", file("old", 0)), ("skills/b", file("theirs", 5))]);
        there.links.insert("skills".into());
        assert_eq!(
            reconcile(&base, &here, &there),
            [
                Action::Hold { path: "skills/a".into(), why: Hold::Linked(There) },
                Action::Conflict { path: "skills/b".into(), winner: There },
            ]
        );
        here.links.insert("skills/b".into());
        assert_eq!(
            reconcile(&base, &here, &there)[1],
            Action::Hold { path: "skills/b".into(), why: Hold::Linked(Here) },
            "linked on both sides"
        );
    }

    #[test]
    fn a_link_itself_may_be_removed_never_what_lies_through_it() {
        let base = manifest(&[("link", Entry::Dir), ("link/x", file("x", 0)), ("real", Entry::Dir), ("real/y", file("y", 0))]);
        let mut there = base.clone();
        there.links.insert("link".into());
        let here = manifest(&[("real", Entry::Dir)]);
        assert_eq!(
            reconcile(&base, &here, &there),
            [Action::RemoveThere("link".into()), Action::RemoveThere("real/y".into())]
        );
        let mut through = base.clone();
        through.links.insert("real".into());
        assert_eq!(
            reconcile(&base, &manifest(&[("link", Entry::Dir), ("link/x", file("x", 0)), ("real", Entry::Dir)]), &through),
            [Action::Hold { path: "real/y".into(), why: Hold::Linked(There) }]
        );
    }

    #[test]
    fn the_base_keeps_what_the_sides_still_differ_on() {
        let base = manifest(&[("same", file("s", 0)), ("differs", file("old", 0)), ("gone", file("g", 0))]);
        let here = manifest(&[("same", file("s", 3)), ("differs", file("new", 1)), ("new", file("n", 1))]);
        let there = manifest(&[("same", file("s", 4)), ("differs", file("old", 0)), ("new", file("n", 2))]);
        assert_eq!(
            agreed(&base, &here, &there),
            manifest(&[("differs", file("old", 0)), ("new", file("n", 1)), ("same", file("s", 3))])
        );
    }
}
