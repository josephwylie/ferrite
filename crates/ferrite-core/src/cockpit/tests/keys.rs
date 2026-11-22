//! A Cockpit names its Threads and Projects by its machine (ADR 0045).

use super::*;

#[test]
fn the_same_id_on_two_machines_is_two_keys() {
    let (mut here, _) = cockpit("keys-here");
    let (mut on_box, _) = cockpit("keys-box");
    let thread = here.open(Provider::Claude, main_choice()).unwrap();
    assert_eq!(
        on_box.open(Provider::Claude, main_choice()).unwrap(),
        thread
    );

    assert_ne!(here.thread_key(thread), on_box.thread_key(thread));
    assert_eq!(here.thread_key(thread).machine(), here.machine_id());
}

#[test]
fn a_removed_machines_id_is_never_reissued() {
    let (gone, _) = cockpit("keys-gone");
    let removed = gone.machine_id();
    drop(gone);

    let (next, _) = cockpit("keys-next");
    assert_ne!(next.machine_id(), removed);
}
