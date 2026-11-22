use super::*;

fn started(n: u64, title: Option<&str>) -> Started {
    Started {
        server: ServerId::new("peer"),
        machine: "pc".into(),
        thread: ThreadId::new(n),
        request: format!("delegate:here:1:{n}"),
        title: title.map(str::to_string),
        task: "Fix the tests".into(),
        provider: Provider::Claude,
        model: None,
    }
}

fn one(title: Option<&str>) -> (Delegates, DelegateId) {
    let mut delegates = Delegates::default();
    let id = delegates.start(started(7, title)).id;
    (delegates, id)
}

#[test]
fn ids_count_up_and_are_never_given_twice() {
    let mut delegates = Delegates::default();
    let first = delegates.start(started(7, None)).id;
    let second = delegates.start(started(8, None)).id;
    assert_eq!((first.to_string(), second.to_string()), ("d1".into(), "d2".into()));
    delegates.end(first, Status::Completed, "done".into());
    assert_eq!(delegates.start(started(9, None)).id.to_string(), "d3");
    assert_eq!("d3".parse::<DelegateId>(), Ok(DelegateId::new(3)));
    assert!("3".parse::<DelegateId>().is_err());
}

#[test]
fn a_record_reads_back_as_it_was_written() {
    let (mut delegates, id) = one(Some("Tests"));
    delegates.cancel(id, None);
    let text = serde_json::to_string(&delegates).unwrap();
    assert!(text.starts_with("[{\"id\":\"d1\""), "{text}");
    assert_eq!(serde_json::from_str::<Delegates>(&text).unwrap(), delegates);
}

#[test]
fn an_open_or_untold_one_holds_its_parent() {
    let (mut delegates, id) = one(None);
    assert!(delegates.holding());
    delegates.end(id, Status::Completed, "done".into());
    assert!(delegates.holding(), "its wake is owed");
    assert_eq!(delegates.untold(), vec![id]);
    assert!(delegates.tell(&[id]));
    assert!(!delegates.holding());
    assert!(!delegates.tell(&[id]), "told once");
}

#[test]
fn what_following_sees_changes_only_an_open_one() {
    let (mut delegates, id) = one(None);
    assert!(delegates.seen(id, Status::Waiting, None));
    assert!(!delegates.seen(id, Status::Waiting, None), "no news");
    assert!(delegates.seen(id, Status::Running, Some("asleep".into())));
    assert!(delegates.end(id, Status::Failed, "boom".into()));
    let ended = delegates.get(id).unwrap();
    assert_eq!((ended.status, ended.problem.as_deref()), (Status::Failed, None));
    assert!(!delegates.seen(id, Status::Running, None));
    assert!(!delegates.end(id, Status::Completed, "late".into()), "the first end stands");
}

#[test]
fn a_cancel_ends_an_open_one_and_owes_its_stop() {
    let (mut delegates, id) = one(None);
    assert_eq!(delegates.cancel(id, Some("not needed")), Some(Cancel::Requested));
    let cancelled = delegates.get(id).unwrap();
    assert_eq!(cancelled.status, Status::Cancelled);
    assert_eq!(cancelled.result.as_deref(), Some("Cancelled: not needed"));
    assert!(cancelled.stop_owed && cancelled.told);
    assert!(!delegates.holding(), "the agent asked, so no wake is owed");
    assert_eq!(delegates.cancel(id, None), Some(Cancel::Ended(Status::Cancelled)));
    assert_eq!(delegates.cancel(DelegateId::new(9), None), None);
}

#[test]
fn a_stop_that_did_not_get_there_says_so_in_the_result() {
    let (mut delegates, id) = one(None);
    delegates.cancel(id, None);
    assert!(delegates.unstopped(id, "pc is asleep".into()));
    assert!(!delegates.unstopped(id, "pc is asleep".into()), "no news");
    assert_eq!(
        delegates.get(id).unwrap().result_text().as_deref(),
        Some("Cancelled. It could not be stopped on pc and may still run there: pc is asleep")
    );
    assert!(delegates.stopped(id));
    assert_eq!(delegates.get(id).unwrap().result_text().as_deref(), Some("Cancelled."));
    assert!(!delegates.stopped(id), "nothing owed");
}

#[test]
fn its_parent_stopping_interrupts_the_open_and_stops_every_one_there() {
    let mut delegates = Delegates::default();
    let ended = delegates.start(started(7, None)).id;
    let open = delegates.start(started(8, None)).id;
    delegates.end(ended, Status::Completed, "done".into());
    assert!(delegates.stop_with_parent());
    let (ended, open) = (delegates.get(ended).unwrap(), delegates.get(open).unwrap());
    assert_eq!((ended.status, ended.result.as_deref()), (Status::Completed, Some("done")));
    assert_eq!(
        (open.status, open.result.as_deref()),
        (Status::Interrupted, Some("Stopped with its parent."))
    );
    assert!(ended.stop_owed && open.stop_owed);
    assert!(!delegates.holding(), "a stopped parent is woken by none");
    assert!(!Delegates::default().stop_with_parent());
}

#[test]
fn a_wake_carries_each_result_in_t3_words() {
    let mut delegates = Delegates::default();
    let first = delegates.start(started(7, Some("Tests"))).id;
    let second = delegates.start(started(8, None)).id;
    delegates.end(first, Status::Completed, "All green.".into());
    delegates.end(second, Status::Failed, "It broke.".into());
    let wake = delegates.wake(&[first, second]).unwrap();
    assert_eq!(
        wake.text,
        "Delegated task d1 (Tests) [completed]:\nAll green.\n\nDelegated task d2 [failed]:\nIt broke."
    );
    assert_eq!(wake.outcome, Outcome::Failed);
    assert_eq!(wake.summary, "2 delegated tasks ended");
    assert_eq!(wake.source, crate::wake::Source::Delegates, "its row wears the bot");
    let alone = delegates.wake(&[first]).unwrap();
    assert_eq!(
        (alone.outcome, alone.summary.as_str()),
        (Outcome::Completed, "Delegated task \"Tests\" completed")
    );
}

#[test]
fn a_result_past_the_budget_is_left_for_task_status() {
    let mut delegates = Delegates::default();
    let long = delegates.start(started(7, None)).id;
    let short = delegates.start(started(8, None)).id;
    delegates.end(long, Status::Completed, "x".repeat(RESULT_INLINE_BYTES));
    delegates.end(short, Status::Interrupted, "Stopped.".into());
    let wake = delegates.wake(&[long, short]).unwrap();
    assert_eq!(
        wake.text,
        "Delegated task d1 [completed]. Use task_status with taskId d1 to read the result.\n\nDelegated task d2 [interrupted]:\nStopped."
    );
    assert_eq!(wake.outcome, Outcome::Updated);
    let open = delegates.start(started(9, None)).id;
    assert_eq!(delegates.wake(&[open]), None, "an open one has nothing to tell");
}

#[test]
fn the_window_reads_each_one_and_whether_they_hold() {
    let (mut delegates, id) = one(Some("Tests"));
    let seen = Delegating::of(&delegates);
    assert!(seen.holds);
    let row = &seen.delegates[0];
    assert_eq!(
        (row.id, row.machine.as_str(), row.thread, row.title.as_deref(), row.status),
        (id, "pc", ThreadId::new(7), Some("Tests"), Status::Starting)
    );
    assert!(delegates.seen(id, Status::Running, Some("pc did not answer".into())));
    let row = &Delegating::of(&delegates).delegates[0];
    assert_eq!(row.problem.as_deref(), Some("pc did not answer"));
    assert_eq!(row.unstopped(), None, "following is held up, but no stop is owed");
    delegates.cancel(id, None);
    assert!(delegates.unstopped(id, "pc is asleep".into()));
    let seen = Delegating::of(&delegates);
    assert!(!seen.holds, "the agent asked: no wake is owed");
    assert_eq!(
        seen.delegates[0].unstopped().as_deref(),
        Some("It could not be stopped on pc and may still run there: pc is asleep")
    );
    assert!(Delegating::of(&Delegates::default()).is_empty());
}
