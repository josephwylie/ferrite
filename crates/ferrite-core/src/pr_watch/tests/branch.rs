//! Conflicts, the merge queue, the comment-wake limit and the no-checks
//! clock.

use super::*;

#[test]
fn reports_a_conflict_once_until_the_branch_is_clean_again() {
    let mergeable = |mergeable| Reading {
        mergeable,
        ..detail()
    };
    let conflicting = mergeable(Mergeable::Conflicting);
    let first = ev(&watch(), &conflicting);
    assert_eq!(first.changes, [conflict()]);
    let unread = ev(
        &first.next,
        &Reading {
            head: None,
            ..conflicting.clone()
        },
    );
    assert_eq!(unread.changes, []);
    assert_eq!(changes(&unread.next, &conflicting), []);
    let recomputing = ev(&first.next, &mergeable(Mergeable::Unknown));
    assert_eq!(changes(&recomputing.next, &conflicting), []);
    let clean = ev(&first.next, &detail());
    assert_eq!(changes(&clean.next, &conflicting), [conflict()]);
    let recomputed = ev(
        &first.next,
        &pushed("bbbbbbbbbb", mergeable(Mergeable::Unknown)),
    );
    assert_eq!(
        changes(&recomputed.next, &pushed("bbbbbbbbbb", conflicting)),
        [conflict()]
    );
}

fn removed(reading: &Reading, id: &str, at: &str, reason: &str) -> Reading {
    Reading {
        merge_queue_removal: Some(QueueRemoval {
            id: id.into(),
            removed_ms: ms(at),
            reason: Some(reason.into()),
        }),
        ..reading.clone()
    }
}

#[test]
fn reports_each_merge_queue_removal_once_while_the_head_checks_stay_green() {
    let green = checks(vec![check("test", Success)]);
    let watching = ev(&watch(), &green).next;
    let dropped = removed(&green, "first", "2026-10-02T12:10:00Z", "Merge conflict");
    let first = ev(&watching, &dropped);
    assert_eq!(first.changes, [queue_removed("Merge conflict")]);
    let told = wake(1, PR_1, &first);
    assert_eq!(told.outcome, Outcome::Failed);
    assert_eq!(told.summary, "#1: removed from merge queue");
    assert_eq!(changes(&first.next, &dropped), []);
    // A read without queue news keeps the removal already told.
    let quiet = ev(&first.next, &green);
    assert_eq!(changes(&quiet.next, &dropped), []);

    let again = removed(&green, "second", "2026-10-02T12:20:00Z", "Merge conflict");
    assert_eq!(changes(&first.next, &again).len(), 1);
    let before = removed(&green, "before", "2026-10-02T11:00:00Z", "Merge conflict");
    let base = ev(
        &Cursor {
            head: watching.head.clone(),
            passed: true,
            passed_checks: watching.passed_checks.clone(),
            ..watch()
        },
        &before,
    );
    assert_eq!(base.changes, []);
    let skewed = removed(&green, "new", "2026-10-02T11:59:58Z", "Merge conflict");
    for prior in [&watching, &base.next] {
        let told = ev(prior, &skewed);
        assert_eq!(told.changes, [queue_removed("Merge conflict")]);
        assert_eq!(changes(&told.next, &skewed), []);
    }
}

#[test]
fn reports_a_merge_queue_removal_stamped_in_the_second_the_watch_started() {
    let watching = Cursor {
        started_ms: ms("2026-10-02T12:00:00.500Z"),
        ..watch()
    };
    let at = |at| removed(&detail(), "first", at, "CI failed");
    assert_eq!(
        changes(&watching, &at("2026-10-02T12:00:00Z")),
        [queue_removed("CI failed")]
    );
    assert_eq!(changes(&watching, &at("2026-10-02T11:59:59Z")), []);
}

#[test]
fn stays_quiet_when_the_queue_lets_go_of_a_pull_request_by_merging_it() {
    let watching = ev(&watch(), &detail()).next;
    let merging = removed(&detail(), "landed", "2026-10-02T12:10:00Z", "merged");
    let read = ev(&watching, &merging);
    assert_eq!(read.changes, []);
    assert_eq!(read.next.merge_queue, QueueSeen::Removal("landed".into()));
}

#[test]
fn does_not_spend_the_comment_wake_limit_on_check_results() {
    let tired = Cursor {
        wakes: WAKE_LIMIT - 1,
        ..seen_head()
    };
    let result = ev(&tired, &checks(vec![check("lint", Failure)]));
    assert_eq!(result.end, None);
    assert_eq!(result.next.wakes, 0);
}

#[test]
fn stops_after_the_wake_limit_unless_the_head_moves() {
    let comments = [remark("reviewer", "2026-10-02T12:10:00Z")];
    let tired = Cursor {
        wakes: WAKE_LIMIT - 1,
        ..seen_head()
    };
    assert_eq!(
        ev(&tired, &said(detail(), &comments)).end,
        Some(End::CommentLimit)
    );
    let moved = ev(&tired, &said(pushed("cccccccccc", detail()), &comments));
    assert_eq!(moved.end, None);
    assert_eq!(moved.next.wakes, 1);
}

/// Reads `read` every ten minutes for an hour, with the news of each.
fn an_hour_later(first: Cursor, read: &Reading) -> Vec<Vec<Change>> {
    let mut current = first;
    (0..=60)
        .step_by(10)
        .map(|minute| {
            let report = evaluate(&current, read, minute * 60_000);
            current = report.next;
            report.changes
        })
        .collect()
}

#[test]
fn does_not_tell_a_watch_never_waiting_on_its_seen_head_that_it_has_none() {
    // Such a watch may have seen this head's checks pending, which records
    // nothing.
    let quiet = an_hour_later(seen_head(), &checks(vec![]));
    assert!(quiet.iter().all(Vec::is_empty));
    let moved = an_hour_later(seen_head(), &pushed("bbbbbbbbbb", checks(vec![])));
    assert_eq!(moved.concat(), [Change::NoChecks { minutes: 20 }]);
    let unknown = checks(vec![placeholder(ActionRequired)]);
    assert_eq!(an_hour_later(baseline(), &unknown).concat(), []);
    let unread = evaluate(&baseline(), &unknown, 0);
    assert_eq!(
        evaluate(&unread.next, &checks(vec![]), 60 * 60_000).changes,
        [Change::NoChecks { minutes: 60 }]
    );
}

#[test]
fn starts_the_no_checks_clock_while_a_fresh_watch_awaits_its_baseline() {
    assert_eq!(
        an_hour_later(baseline(), &checks(vec![])).concat(),
        [Change::NoChecks { minutes: 20 }]
    );
}

#[test]
fn counts_checks_as_pending_until_every_one_a_review_bots_included_has_finished() {
    let pending = |list: Vec<Check>, from: Cursor| ev(&from, &checks(list)).next.checks_pending;
    let ci = required("test", Success);
    assert!(pending(
        vec![ci.clone(), check("CodeRabbit", Pending)],
        watch()
    ));
    assert!(!pending(
        vec![ci.clone(), check("CodeRabbit", Success)],
        watch()
    ));
    assert!(!pending(
        vec![ci.clone(), check("CodeRabbit", Failure)],
        watch()
    ));
    assert!(!pending(vec![ci, check("deploy", ActionRequired)], watch()));
    // A failed read keeps the last answer, as does one with only an
    // unreadable approval check.
    let answered = |checks_pending| Cursor {
        checks_pending,
        ..seen_head()
    };
    assert!(!pending(vec![], answered(false)));
    let unreadable = Check {
        placeholder: true,
        ..check("approvals", ActionRequired)
    };
    assert!(pending(vec![unreadable], answered(true)));
}

#[test]
fn waits_a_while_for_a_new_heads_checks() {
    let new_head = pushed("bbbbbbbbbb", checks(vec![]));
    assert!(ev(&seen_head(), &new_head).next.checks_pending);

    // Once the no-checks window passes, the watch stops waiting for them.
    let mut current = seen_head();
    for minute in [0, 20] {
        current = evaluate(&current, &new_head, minute * 60_000).next;
    }
    assert!(!current.checks_pending);

    // A watch with no answer waits on an empty list only while the
    // no-checks clock runs.
    let unanswered = |since| {
        ev(
            &Cursor {
                head: Some("bbbbbbbbbb".into()),
                checks_awaited_since_ms: since,
                ..watch()
            },
            &new_head,
        )
        .next
        .checks_pending
    };
    assert!(!unanswered(None));
    assert!(unanswered(Some(ms(STARTED))));
}
