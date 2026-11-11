//! Recovery and the Archive (#226): a Worker whose Session was lost
//! mid-Step is revived once and told it was interrupted; a second loss
//! stops the Task; Completed Tasks leave the board after 7 days or with
//! Clear, on a passed-in clock.

use super::*;

/// The Worker as the Server sees it after its Session was lost: no
/// Session, at rest, its turn count starting again.
fn lost(turns: u64) -> BTreeMap<ThreadId, Seen> {
    BTreeMap::from([(
        WORKER,
        Seen {
            busy: false,
            turns,
            decision: None,
            lost: true,
        },
    )])
}

fn activity(factory: &Factory, task: &str) -> Vec<String> {
    factory
        .task(task)
        .unwrap()
        .activity
        .iter()
        .map(|entry| entry.text.clone())
        .collect()
}

/// AC: a Worker whose Session ends mid-Step, with no restart, is revived
/// once: the same Thread is told it was interrupted, to continue and to
/// report, and the activity says the Session ended (not that the agent
/// could not finish). The Task is still Working.
#[test]
fn a_session_lost_mid_step_revives_the_same_worker_once() {
    let mut factory = Factory::in_memory();
    let task = working(&mut factory);
    let effects = factory.step(4, &lost(0));
    let [Effect::Prompt { thread, text }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(*thread, WORKER, "the same Thread");
    assert!(text.contains("interrupted"), "{text}");
    assert!(text.contains("report outcome finished"), "{text}");
    assert_eq!(status(&factory, &task), Status::Working);
    let said = activity(&factory, &task);
    assert!(
        said.last().unwrap().contains("session ended unexpectedly"),
        "{said:?}"
    );
    assert!(!said.iter().any(|line| line.contains("Unable")), "{said:?}");

    // Revived: its new Session works, and nothing more is sent.
    assert!(factory.step(5, &seen(true, 0)).is_empty());
}
