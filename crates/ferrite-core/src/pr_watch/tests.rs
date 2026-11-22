//! T3 Code's `pullRequestWatch.test.ts` (0fd48228), case for case, less the
//! cases for what the module doc says Ferrite leaves out: the required
//! rebase, a host whose empty check list says nothing, and a watch saved
//! before passed checks were recorded. The submodules hold the wake texts
//! T3 really sent on Ferrite's own PRs and a real `gh api graphql` read.

use super::*;
use CheckStatus::{ActionRequired, Failure, Pending, Skipped, Success};

const STARTED: &str = "2026-10-02T12:00:00.000Z";

fn ms(at: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(at)
        .expect("a stamp")
        .timestamp_millis()
}

fn watch() -> Cursor {
    Cursor {
        started_ms: ms(STARTED),
        remarks_through_ms: ms(STARTED),
        ..Cursor::default()
    }
}

fn baseline() -> Cursor {
    Cursor {
        awaiting_baseline: Some(Baseline::All),
        ..watch()
    }
}

fn seen_head() -> Cursor {
    Cursor {
        head: Some("aaaaaaaaaa".into()),
        ..watch()
    }
}

fn run(id: u32) -> String {
    format!("https://github.com/o/r/actions/runs/{id}")
}

fn check(name: &str, status: CheckStatus) -> Check {
    Check {
        name: name.into(),
        status,
        url: Some(format!("https://ci.example/{name}")),
        required: false,
        placeholder: false,
    }
}

fn required(name: &str, status: CheckStatus) -> Check {
    Check {
        required: true,
        ..check(name, status)
    }
}

fn at_url(check: Check, url: Option<String>) -> Check {
    Check { url, ..check }
}

fn placeholder(status: CheckStatus) -> Check {
    Check {
        placeholder: true,
        ..check("Workflow approval status", status)
    }
}

fn detail() -> Reading {
    Reading {
        state: PrState::Open,
        head: Some("aaaaaaaaaa".into()),
        base_branch: "main".into(),
        checks: vec![check("lint", Success), check("test", Pending)],
        mergeable: Mergeable::Mergeable,
        viewer: Some("agent-user".into()),
        author: Some("agent-user".into()),
        remarks: Some(Vec::new()),
        merge_queue_removal: None,
        merged_by: None,
        merge_commit: None,
    }
}

fn checks(checks: Vec<Check>) -> Reading {
    Reading { checks, ..detail() }
}

fn pushed(head: &str, reading: Reading) -> Reading {
    Reading {
        head: Some(head.into()),
        ..reading
    }
}

fn said(reading: Reading, remarks: &[Remark]) -> Reading {
    Reading {
        remarks: Some(remarks.to_vec()),
        ..reading
    }
}

fn remark(login: &str, at: &str) -> Remark {
    remark_saying(login, at, "Please rename this.")
}

fn remark_saying(login: &str, at: &str, body: &str) -> Remark {
    Remark {
        id: format!("{login}-{at}"),
        author: Some(login.into()),
        body: body.into(),
        created_ms: ms(at),
        edited_ms: None,
        url: Some(format!("https://github.com/o/r/pull/1#{login}")),
        path: Some("src/index.ts".into()),
        review_state: None,
    }
}

/// These cases read the head once or at one moment, so the no-checks
/// notice never comes due.
fn ev(cursor: &Cursor, reading: &Reading) -> Report {
    evaluate(cursor, reading, 0)
}

fn changes(cursor: &Cursor, reading: &Reading) -> Vec<Change> {
    ev(cursor, reading).changes
}

fn failed(checks: &[Check]) -> Change {
    Change::ChecksFailed(checks.to_vec())
}

fn passed(count: usize, required: bool) -> Change {
    Change::ChecksPassed { count, required }
}

fn finished(passed: usize, failing: &[Check]) -> Change {
    Change::ChecksFinished {
        passed,
        failing: failing.to_vec(),
    }
}

fn awaiting(checks: &[Check]) -> Change {
    Change::AwaitingApproval(checks.to_vec())
}

fn remarks(remarks: &[Remark]) -> Change {
    Change::Remarks(remarks.to_vec())
}

fn conflict() -> Change {
    Change::Conflicting {
        base: "main".into(),
    }
}

fn queue_removed(reason: &str) -> Change {
    Change::MergeQueueRemoved {
        reason: Some(reason.into()),
    }
}

fn watched(number: u64, url: &str) -> Watch {
    Watch {
        url: url.into(),
        number,
        repository: "o/r".into(),
        until: Until::Checks,
        cursor: watch(),
    }
}

fn wake(number: u64, url: &str, report: &Report) -> Wake {
    wake_text(&watched(number, url), report).expect("news to tell")
}

const PR_1: &str = "https://github.com/o/r/pull/1";
/// Where Ferrite's own PRs live, which the real wakes and reads name.
const FERRITE: &str = "https://github.com/josephwylie/ferrite";

#[test]
fn baselines_already_passed_checks() {
    for is_required in [false, true] {
        let gate = |status| Check {
            required: is_required,
            ..check("test", status)
        };
        let green = checks(vec![gate(Success)]);
        let base = ev(&baseline(), &green);
        assert_eq!(base.changes, []);
        assert_eq!(base.next.head, green.head);
        assert!(base.next.passed);
        assert_eq!(base.next.passed_checks, ["test"]);
        assert_eq!(base.end, None);
        assert_eq!(changes(&base.next, &green), []);

        let pending = ev(&base.next, &checks(vec![gate(Pending)]));
        assert_eq!(pending.changes, []);
        let failure = ev(&pending.next, &checks(vec![gate(Failure)]));
        assert!(matches!(failure.changes[0], Change::ChecksFailed(_)));
        let pass = ev(&failure.next, &green);
        assert_eq!(pass.changes, [passed(1, is_required)]);
        assert_eq!(
            changes(&pass.next, &pushed("bbbbbbbbbb", green)),
            [passed(1, is_required)]
        );
    }
}

#[test]
fn baselines_existing_failures_and_conflicts_then_reports_new_ones() {
    let broken = Reading {
        mergeable: Mergeable::Conflicting,
        ..checks(vec![check("lint", Failure), check("test", Pending)])
    };
    let base = ev(&baseline(), &broken);
    assert_eq!(base.changes, []);
    assert_eq!(base.next.failed_checks, ["lint"]);
    assert!(!base.next.passed);
    assert!(base.next.conflicting);
    assert_eq!(changes(&base.next, &broken), []);

    let failure = ev(
        &base.next,
        &Reading {
            checks: vec![check("lint", Failure), check("test", Failure)],
            ..broken.clone()
        },
    );
    assert_eq!(
        failure.changes,
        [
            failed(&[check("test", Failure)]),
            finished(0, &[check("lint", Failure), check("test", Failure)]),
        ]
    );
    let clean = ev(&failure.next, &detail());
    assert_eq!(clean.changes, []);
    assert_eq!(
        changes(&clean.next, &broken),
        [failed(&[check("lint", Failure)]), conflict()]
    );
}

#[test]
fn reports_failures_on_the_first_read_only_when_ci_was_pending_at_the_start() {
    let red = checks(vec![check("lint", Failure), check("test", Pending)]);
    let seeded = |pending| Cursor {
        checks_pending: pending,
        ..baseline()
    };
    let pending = ev(&seeded(true), &red);
    assert_eq!(pending.changes, [failed(&[check("lint", Failure)])]);
    assert_eq!(
        changes(
            &pending.next,
            &checks(vec![check("lint", Failure), check("test", Success)])
        ),
        [finished(1, &[check("lint", Failure)])]
    );
    assert_eq!(changes(&seeded(false), &red), []);

    // A read the host could not finish keeps the baseline and must keep the
    // seed with it.
    let incomplete = ev(
        &seeded(true),
        &checks(vec![check("lint", Success), placeholder(Pending)]),
    );
    assert_eq!(incomplete.changes, []);
    assert_eq!(
        changes(
            &incomplete.next,
            &checks(vec![check("lint", Success), check("gated", Failure)])
        ),
        [failed(&[check("gated", Failure)])]
    );
}

#[test]
fn baselines_checks_already_awaiting_approval() {
    let gated = checks(vec![at_url(check("CI", ActionRequired), Some(run(1)))]);
    let base = ev(&baseline(), &gated);
    assert_eq!(base.changes, []);
    assert_eq!(changes(&base.next, &gated), []);
}

#[test]
fn reports_remarks_after_the_watch_started_while_baselining_checks_and_conflicts() {
    let comments = [
        remark("reviewer", "2026-10-02T11:00:00Z"),
        remark("reviewer", "2026-10-02T12:06:00Z"),
    ];
    let green = said(
        Reading {
            mergeable: Mergeable::Conflicting,
            ..checks(vec![check("test", Success)])
        },
        &comments,
    );
    let base = ev(&baseline(), &green);
    assert_eq!(base.changes, [remarks(&comments[1..])]);
    assert!(base.next.passed);
    assert!(base.next.conflicting);
    assert_eq!(base.next.wakes, 1);
    assert_eq!(changes(&base.next, &green), []);
}

#[test]
fn reports_a_failure_once_the_head_sha_appears_after_a_baseline_without_one() {
    let waiting = ev(
        &baseline(),
        &Reading {
            head: None,
            ..checks(vec![check("test", Pending)])
        },
    );
    assert_eq!(waiting.changes, []);
    assert_eq!(
        changes(&waiting.next, &checks(vec![check("test", Failure)])),
        [failed(&[check("test", Failure)])]
    );
}

#[test]
fn keeps_a_baseline_taken_without_a_head_sha_once_the_sha_appears() {
    let green = checks(vec![check("test", Success)]);
    let headless = Reading {
        head: None,
        ..green.clone()
    };
    let base = ev(&baseline(), &headless);
    let known = ev(&base.next, &green);
    assert_eq!(known.changes, []);
    assert_eq!(known.next.head, green.head);
    let unknown = ev(&known.next, &headless);
    assert_eq!(unknown.next.head, green.head);
    assert_eq!(
        changes(&unknown.next, &pushed("bbbbbbbbbb", green)),
        [passed(1, false)]
    );
}

#[test]
fn reports_checks_that_appear_after_a_first_read_without_any() {
    for status in [Success, Failure] {
        let empty = ev(&baseline(), &checks(vec![]));
        assert_eq!(empty.changes, []);
        assert_eq!(empty.next.awaiting_baseline, None);
        assert!(empty.next.checks_pending);
        let expected = if status == Success {
            passed(1, false)
        } else {
            failed(&[check("test", Failure)])
        };
        assert_eq!(
            changes(&empty.next, &checks(vec![check("test", status)])),
            [expected]
        );
    }
}

#[test]
fn keeps_the_check_baseline_through_a_read_whose_approval_lookup_failed() {
    let bot = check("bot", Success);
    let partial = ev(
        &baseline(),
        &checks(vec![bot.clone(), placeholder(Pending)]),
    );
    assert_eq!(partial.changes, []);
    assert_eq!(partial.next.awaiting_baseline, Some(Baseline::Checks));
    let approval = at_url(check("CI", ActionRequired), Some(run(1)));
    for list in [vec![bot.clone()], vec![bot.clone(), approval]] {
        assert_eq!(changes(&partial.next, &checks(list)), []);
    }
}

#[test]
fn reports_pending_advisory_checks_finishing_after_a_required_check_baseline() {
    let initial = checks(vec![required("test", Success), check("bot", Pending)]);
    let base = ev(&baseline(), &initial);
    assert_eq!(base.changes, []);
    let done = checks(vec![required("test", Success), check("bot", Success)]);
    let settled = ev(&base.next, &done);
    assert_eq!(settled.changes, [passed(2, false)]);
    assert_eq!(changes(&settled.next, &done), []);
}

#[test]
fn waits_for_workflow_approval_once_through_reads_that_miss_it_until_the_run_finishes() {
    let bot = check("bot", Success);
    let approval = at_url(check("CI", ActionRequired), Some(run(1)));
    let gated = checks(vec![bot.clone(), approval.clone()]);
    let first = ev(&watch(), &gated);
    assert_eq!(first.changes, [awaiting(&[approval.clone()])]);

    // GitHub's approval read can answer empty while the rest of the checks
    // read fine.
    let missing = ev(&first.next, &checks(vec![bot.clone()]));
    assert_eq!(missing.changes, []);
    assert!(!missing.next.passed);
    assert_eq!(changes(&missing.next, &gated), []);
    let later = at_url(check("Preview", ActionRequired), Some(run(2)));
    assert_eq!(
        changes(
            &missing.next,
            &checks(vec![bot.clone(), approval.clone(), later])
        ),
        []
    );
    let unnamed = at_url(check("Docs", ActionRequired), None);
    let named = checks(vec![bot.clone(), unnamed.clone()]);
    let alone = ev(&watch(), &named);
    assert_eq!(alone.changes, [awaiting(&[unnamed.clone()])]);
    let unlisted_placeholder = Check {
        name: "Workflow approval status".into(),
        placeholder: true,
        ..unnamed.clone()
    };
    let unlisted = ev(
        &alone.next,
        &checks(vec![bot.clone(), unlisted_placeholder]),
    );
    assert_eq!(unlisted.changes, []);
    assert!(!unlisted.next.passed);
    assert_eq!(changes(&unlisted.next, &named), []);
    let dropped = ev(&alone.next, &checks(vec![bot.clone()]));
    assert_eq!(dropped.changes, []);
    assert!(!dropped.next.passed);
    assert_eq!(dropped.next.awaiting_approval, ["Docs"]);
    assert_eq!(changes(&dropped.next, &named), []);
    let named_pending = ev(
        &dropped.next,
        &checks(vec![
            Check {
                required: true,
                ..bot.clone()
            },
            Check {
                status: Pending,
                ..unnamed.clone()
            },
        ]),
    );
    assert_eq!(named_pending.changes, []);
    assert!(!named_pending.next.passed);
    let named_done = checks(vec![
        bot.clone(),
        Check {
            status: Success,
            ..unnamed.clone()
        },
    ]);
    let named_finished = ev(&named_pending.next, &named_done);
    assert_eq!(named_finished.changes, [passed(2, false)]);
    assert_eq!(named_finished.next.awaiting_approval, Vec::<String>::new());
    assert_eq!(changes(&named_finished.next, &named_done), []);
    assert_eq!(
        changes(
            &missing.next,
            &pushed("bbbbbbbbbb", checks(vec![bot.clone()]))
        ),
        [passed(1, false)]
    );

    // Approved, the run's jobs carry their own names under the run's URL.
    let job = at_url(check("lint", Success), Some(format!("{}/job/7", run(1))));
    let pending = at_url(check("test", Pending), Some(format!("{}/job/8", run(1))));
    let gate = Check {
        required: true,
        ..bot.clone()
    };
    let with_pending = |url: Option<String>| {
        checks(vec![
            gate.clone(),
            job.clone(),
            at_url(pending.clone(), url),
        ])
    };
    let running = ev(&missing.next, &with_pending(pending.url.clone()));
    assert_eq!(running.changes, []);
    assert!(!running.next.passed);
    assert_eq!(running.next.awaiting_approval, [run(1)]);
    let overlap = ev(
        &running.next,
        &with_pending(Some(format!("{}/job/9", run(2)))),
    );
    assert_eq!(overlap.changes, []);
    assert!(!overlap.next.passed);
    assert_eq!(overlap.next.awaiting_approval, [run(1)]);
    let unlinked = ev(&overlap.next, &with_pending(None));
    assert_eq!(unlinked.changes, []);
    assert!(!unlinked.next.passed);
    assert_eq!(unlinked.next.awaiting_approval, [run(1)]);
    let advisory = ev(
        &unlinked.next,
        &with_pending(Some("https://ci.example/test".into())),
    );
    assert_eq!(advisory.changes, [passed(1, true)]);
    assert_eq!(advisory.next.awaiting_approval, Vec::<String>::new());
    let completed = checks(vec![
        gate.clone(),
        job.clone(),
        Check {
            status: Success,
            ..pending.clone()
        },
    ]);
    let done = ev(&unlinked.next, &completed);
    assert_eq!(done.changes, [passed(1, true)]);
    assert_eq!(done.next.awaiting_approval, Vec::<String>::new());
    assert_eq!(changes(&done.next, &completed), []);

    assert_eq!(
        changes(&missing.next, &checks(vec![bot, job])),
        [passed(2, false)]
    );
}

#[test]
fn does_not_pass_or_fail_while_the_host_cannot_tell_whether_workflows_await_approval() {
    let bot = check("bot", Success);
    let unknown = at_url(placeholder(ActionRequired), None);
    let unread = ev(&watch(), &checks(vec![bot.clone(), unknown.clone()]));
    assert_eq!(unread.changes, []);
    assert!(!unread.next.passed);
    let read = ev(&unread.next, &checks(vec![bot.clone()]));
    assert_eq!(read.changes, [passed(1, false)]);
    let blocked = ev(
        &read.next,
        &checks(vec![
            Check {
                status: Pending,
                ..bot.clone()
            },
            unknown,
        ]),
    );
    assert!(!blocked.next.passed);
    assert_eq!(
        changes(&blocked.next, &checks(vec![bot])),
        [passed(1, false)]
    );
}

#[test]
fn reports_each_failure_at_once_even_while_another_check_never_finishes() {
    let bot = check("CodeRabbit", Pending);
    let first = checks(vec![
        check("lint", Failure),
        check("test", Pending),
        bot.clone(),
    ]);
    let lint = ev(&watch(), &first);
    assert_eq!(lint.changes, [failed(&[check("lint", Failure)])]);
    assert_eq!(changes(&lint.next, &first), []);

    let second = checks(vec![check("lint", Failure), check("test", Failure), bot]);
    let test = ev(&lint.next, &second);
    assert_eq!(test.changes, [failed(&[check("test", Failure)])]);

    let rerun = ev(&test.next, &first);
    assert_eq!(changes(&rerun.next, &second).len(), 1);

    assert_eq!(changes(&test.next, &pushed("bbbbbbbbbb", second)).len(), 1);

    let approval = at_url(check("CI", ActionRequired), Some(run(1)));
    let gated = checks(vec![approval.clone(), check("lint", Failure)]);
    assert_eq!(
        changes(&watch(), &gated),
        [failed(&[check("lint", Failure)]), awaiting(&[approval])]
    );
}

#[test]
fn reports_passed_once_the_required_checks_pass_whatever_the_others_do() {
    let green = checks(vec![
        required("test", Success),
        required("lint", Success),
        check("bot", Pending),
    ]);
    let pass = ev(&watch(), &green);
    assert_eq!(pass.changes, [passed(2, true)]);
    assert_eq!(changes(&pass.next, &green), []);

    let plain = checks(vec![check("test", Success), check("bot", Pending)]);
    assert_eq!(changes(&watch(), &plain), []);
}

#[test]
fn reports_passed_again_when_a_required_check_shows_up_already_passed() {
    let tests = checks(vec![
        required("Tests", Success),
        check("Smoke Tests", Pending),
    ]);
    let first = ev(&watch(), &tests);
    assert_eq!(first.changes, [passed(1, true)]);

    let gated = checks(vec![
        required("Tests", Success),
        check("Smoke Tests", Success),
        required("Smoke Tests Gate", Success),
    ]);
    let second = ev(&first.next, &gated);
    assert_eq!(second.changes, [passed(2, true)]);
    assert_eq!(changes(&second.next, &gated), []);

    let pending = checks(vec![
        required("Tests", Success),
        required("Smoke Tests Gate", Pending),
    ]);
    let waiting = ev(&first.next, &pending);
    assert_eq!(waiting.changes, []);
    assert_eq!(changes(&waiting.next, &gated), [passed(2, true)]);
}

// regression (T3 #23712): required checks woke the agent early and the rest
// finishing never did.
#[test]
fn wakes_again_when_the_other_checks_finish_after_the_required_ones_passed() {
    let early = checks(vec![required("test", Success), check("e2e", Pending)]);
    let first = ev(&watch(), &early);
    assert_eq!(first.changes, [passed(1, true)]);

    let done_checks = vec![required("test", Success), check("e2e", Success)];
    let done = checks(done_checks.clone());
    let last = ev(&first.next, &done);
    assert!(matches!(last.changes[..], [Change::ChecksPassed { .. }]));
    assert_eq!(changes(&last.next, &done), []);
    // A later advisory job running and passing is not more news.
    let with_bot = |status| {
        let mut list = done_checks.clone();
        list.push(check("bot", status));
        checks(list)
    };
    let churn = ev(&last.next, &with_bot(Pending));
    assert_eq!(churn.changes, []);
    assert_eq!(changes(&churn.next, &with_bot(Success)), []);

    // A pending check that turns required on the next read still finishes
    // with one wake.
    let promoted = checks(vec![required("test", Success), required("e2e", Success)]);
    let gated = ev(&first.next, &promoted);
    assert_eq!(gated.changes, [passed(2, true)]);
    assert_eq!(changes(&gated.next, &promoted), []);

    // Nothing else was pending when the required ones passed, so nothing
    // more is owed.
    let green = ev(&watch(), &done);
    assert_eq!(green.changes, [passed(1, true)]);
    assert_eq!(changes(&green.next, &with_bot(Success)), []);
}

#[test]
fn reports_an_owed_non_required_failure_once_after_the_required_notice() {
    let gate = required("test", Success);
    let first = ev(&watch(), &checks(vec![gate.clone(), check("e2e", Pending)]));
    assert_eq!(first.changes, [passed(1, true)]);
    let broken = checks(vec![gate.clone(), check("e2e", Failure)]);
    let failure = ev(&first.next, &broken);
    assert_eq!(failure.changes, [failed(&[check("e2e", Failure)])]);
    let repeated = ev(&failure.next, &broken);
    assert_eq!(repeated.changes, []);
    let recovered = checks(vec![gate, check("e2e", Success)]);
    let last = ev(&repeated.next, &recovered);
    assert_eq!(last.changes, [passed(2, false)]);
    assert_eq!(changes(&last.next, &recovered), []);
}

#[test]
fn clears_old_head_owed_checks_when_a_new_head_is_pushed() {
    let gate = required("test", Success);
    let first = ev(&watch(), &checks(vec![gate.clone(), check("e2e", Pending)]));
    assert_eq!(first.next.owed_checks, ["e2e"]);
    let unread_head = pushed("bbbbbbbbbb", checks(vec![]));
    let unread = ev(&first.next, &unread_head);
    assert_eq!(unread.changes, []);
    assert_eq!(unread.next.owed_checks, Vec::<String>::new());
    let head = Reading {
        checks: vec![gate.clone()],
        ..unread_head.clone()
    };
    let new_head = ev(&unread.next, &head);
    assert_eq!(new_head.changes, [passed(1, true)]);
    assert_eq!(new_head.next.owed_checks, Vec::<String>::new());
    let late_result = Reading {
        checks: vec![gate, check("e2e", Success)],
        ..unread_head
    };
    let last = ev(&new_head.next, &late_result);
    assert_eq!(last.changes, []);
    assert_eq!(changes(&last.next, &late_result), []);
}

// regression (T3 #30530): a failure told while other checks ran left the
// agent waiting for them forever.
#[test]
fn wakes_once_when_the_other_checks_finish_after_one_failed() {
    for is_required in [true, false] {
        let gated = |name: &str, status| Check {
            required: is_required,
            ..check(name, status)
        };
        let labels = gated("labels", Failure);
        let running = checks(vec![
            labels.clone(),
            gated("test", Pending),
            gated("docs", Pending),
        ]);
        let failure = ev(&watch(), &running);
        assert_eq!(failure.changes, [failed(&[labels.clone()])]);
        assert_eq!(changes(&failure.next, &running), []);

        let done_checks = vec![
            labels.clone(),
            gated("test", Success),
            gated("docs", Skipped),
        ];
        let done = checks(done_checks.clone());
        let finish = ev(&failure.next, &done);
        assert_eq!(finish.changes, [finished(1, &[labels.clone()])]);
        assert_eq!(wake(1, PR_1, &finish).outcome, Outcome::Failed);
        assert_eq!(changes(&finish.next, &done), []);

        // A later job running and passing is not more news.
        let with_bot = |status| {
            let mut list = done_checks.clone();
            list.push(check("bot", status));
            checks(list)
        };
        let churn = ev(&finish.next, &with_bot(Pending));
        assert_eq!(churn.changes, []);
        assert_eq!(changes(&churn.next, &with_bot(Success)), []);

        // A watch started after the failure is still owed the news that the
        // rest finished.
        let late = ev(&baseline(), &running);
        assert_eq!(late.changes, []);
        assert_eq!(changes(&late.next, &done), finish.changes);
    }
}

#[test]
fn says_the_checks_finished_only_once_every_check_has_a_maintainers_approval_included() {
    let lint = required("lint", Failure);
    let unit = required("unit", Success);
    let failure = ev(
        &watch(),
        &checks(vec![
            lint.clone(),
            required("unit", Pending),
            check("e2e", Pending),
        ]),
    );
    assert_eq!(failure.changes, [failed(&[lint.clone()])]);

    let advisory = checks(vec![lint.clone(), unit.clone(), check("e2e", Pending)]);
    let waiting = ev(&failure.next, &advisory);
    assert_eq!(waiting.changes, []);

    let approval = at_url(check("deploy", ActionRequired), Some(run(1)));
    let held = checks(vec![
        lint.clone(),
        unit.clone(),
        check("e2e", Success),
        approval.clone(),
    ]);
    let holding = ev(&waiting.next, &held);
    assert_eq!(holding.changes, [awaiting(&[approval.clone()])]);

    let unknown = at_url(placeholder(ActionRequired), None);
    let deployed = Check {
        status: Success,
        ..approval
    };
    let e2e = check("e2e", Failure);
    let unread = ev(
        &holding.next,
        &checks(vec![
            lint.clone(),
            unit.clone(),
            check("e2e", Success),
            deployed.clone(),
            unknown,
        ]),
    );
    assert_eq!(unread.changes, []);

    // A failure that arrives with the last result shares its wake.
    let done = checks(vec![lint.clone(), unit, e2e.clone(), deployed]);
    let finish = ev(&unread.next, &done);
    assert_eq!(
        finish.changes,
        [failed(&[e2e.clone()]), finished(2, &[lint.clone(), e2e])]
    );
    assert_eq!(changes(&finish.next, &done), []);

    // The wake is owed just the same when only an approval the read omits
    // is outstanding.
    let gated = at_url(check("deploy", ActionRequired), Some(run(2)));
    let asked = ev(&watch(), &checks(vec![required("lint", Pending), gated]));
    let omitted = ev(&asked.next, &checks(vec![lint.clone()]));
    assert_eq!(omitted.changes, [failed(&[lint.clone()])]);
    let job = at_url(
        check("deploy-job", Success),
        Some(format!("{}/job/1", run(2))),
    );
    assert_eq!(
        changes(&omitted.next, &checks(vec![lint.clone(), job])),
        [finished(1, &[lint])]
    );
}

#[test]
fn wakes_when_a_rerun_finishes_while_another_failure_still_blocks_the_checks() {
    let labels = check("labels", Failure);
    let both = checks(vec![labels.clone(), check("test", Failure)]);
    // Nothing else ran when these failed, so their own wake is the last word.
    let failure = ev(&watch(), &both);
    assert_eq!(
        failure.changes,
        [failed(&[labels.clone(), check("test", Failure)])]
    );
    assert_eq!(changes(&failure.next, &both), []);

    let rerun = ev(
        &failure.next,
        &checks(vec![labels.clone(), check("test", Pending)]),
    );
    assert_eq!(rerun.changes, []);
    let recovered = checks(vec![labels.clone(), check("test", Success)]);
    assert_eq!(
        changes(&rerun.next, &recovered),
        [finished(1, &[labels.clone()])]
    );

    // A rerun that starts and ends between two reads is the same news.
    assert_eq!(
        changes(&failure.next, &recovered),
        [finished(1, &[labels.clone()])]
    );

    assert_eq!(
        changes(&rerun.next, &pushed("bbbbbbbbbb", recovered)),
        [failed(&[labels])]
    );
}

// regression (T3 #16208): PR watches burned GitHub's rate limit and gave up.
#[test]
fn does_not_report_passed_again_for_a_new_passed_check_where_none_is_required() {
    let first = ev(&watch(), &checks(vec![check("test", Success)]));
    assert_eq!(first.changes, [passed(1, false)]);
    let both = checks(vec![check("test", Success), check("lint", Success)]);
    assert_eq!(changes(&first.next, &both), []);
}

mod branch;
mod reads;
mod remarks;
mod wakes;
