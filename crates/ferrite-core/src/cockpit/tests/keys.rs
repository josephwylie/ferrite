//! A Cockpit names its Threads and Projects by its machine (ADR 0045).

use super::*;
use crate::{MachineId, MachineKey, SavedThread};

/// A window holding a second machine beside the one on screen, its
/// Engine in this process and saved as `box`.
fn two_machines(name: &str) -> (Cockpit, Fake, MachineId, Fake) {
    let (mut cockpit, here) = cockpit(&format!("{name}-here"));
    let on_box = Fake::default();
    let store = Store::open(scratch(&format!("{name}-box"))).unwrap();
    let machine = cockpit
        .hold_in_process(
            store,
            Box::new(on_box.clone()),
            MachineKey(Some("box".into())),
        )
        .unwrap();
    (cockpit, here, machine, on_box)
}

fn says(cockpit: &Cockpit, thread: ThreadKey, text: &str) -> bool {
    let open = cockpit.thread(thread).unwrap();
    crate::transcript::plain_text(open.transcript().blocks()).contains(text)
}

#[test]
fn a_removed_machines_id_is_never_reissued() {
    let (gone, _) = cockpit("keys-gone");
    let removed = gone.on_screen();
    drop(gone);

    let (next, _) = cockpit("keys-next");
    assert_ne!(next.on_screen(), removed);
}

/// Two machines each hold a Thread under the same id: every read, act
/// and pump output names one of them, never both.
#[test]
fn two_machines_with_the_same_thread_id_are_kept_apart() {
    let (mut cockpit, here, machine, on_box) = two_machines("keys-apart");
    let mine = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let theirs = cockpit
        .open_at(machine, Provider::Claude, main_choice())
        .unwrap();
    assert_eq!(mine.thread(), theirs.thread(), "each machine's first id");
    assert_ne!(mine, theirs);
    assert_eq!(mine.machine(), cockpit.on_screen());
    assert_eq!(theirs.machine(), machine);
    assert_eq!(cockpit.threads(), vec![mine, theirs]);
    assert_eq!(cockpit.known_threads(), [mine, theirs].into());
    cockpit.pump();

    // An act reaches its own machine's Session.
    let _ = cockpit.send(theirs, "to the box".into());
    assert_eq!(on_box.sent.borrow().as_slice(), ["to the box"]);
    assert!(here.sent.borrow().is_empty());
    assert!(says(&cockpit, theirs, "to the box"));
    assert!(!says(&cockpit, mine, "to the box"));

    // What a machine's Session streams lands in its own Thread's update.
    on_box.streams.borrow()[0]
        .send(text("from the box"))
        .unwrap();
    let updates = cockpit.pump();
    assert!(updates.iter().any(|update| update.thread == theirs));
    assert!(updates.iter().all(|update| update.thread != mine));
    assert!(says(&cockpit, theirs, "from the box"));
    assert!(!says(&cockpit, mine, "from the box"));

    // A Decision blocks only the Thread that asked.
    on_box.streams.borrow()[0]
        .send(decision("perm_box", "Write"))
        .unwrap();
    cockpit.pump();
    assert_eq!(cockpit.needs_you(), vec![theirs]);

    // Saved files name each by its machine, and come back to the same key.
    let saved_mine = SavedThread {
        machine: MachineKey(None),
        thread: mine.thread(),
    };
    let saved_theirs = SavedThread {
        machine: MachineKey(Some("box".into())),
        thread: theirs.thread(),
    };
    assert_eq!(cockpit.saved(mine), Some(saved_mine.clone()));
    assert_eq!(cockpit.saved(theirs), Some(saved_theirs.clone()));
    assert_eq!(cockpit.resolve(&saved_mine), Some(mine));
    assert_eq!(cockpit.resolve(&saved_theirs), Some(theirs));

    // Parking one leaves the other open, in the pump's roster too.
    assert_ne!(cockpit.roster.pane_of(mine), cockpit.roster.pane_of(theirs));
    cockpit.park(theirs).unwrap();
    assert_eq!(cockpit.threads(), vec![mine]);
    assert_eq!(cockpit.parked().unwrap(), vec![theirs]);
    assert_eq!(cockpit.known_threads(), [mine, theirs].into());
    assert!(cockpit.roster.pane_of(mine).is_some());
    assert_eq!(cockpit.roster.pane_of(theirs), None);
    assert!(cockpit.thread(theirs).is_none());
    assert!(cockpit.thread(mine).is_some());
}
