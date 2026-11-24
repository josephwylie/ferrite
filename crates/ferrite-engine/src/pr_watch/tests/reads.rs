//! A real GraphQL read, the pages and workflow runs it follows, and the
//! watch fact's start and end.

use super::*;
use ferrite_core::pull_request::Stamp;
use serde_json::{json, Value};

const WATCH_426: &str = include_str!("../../../tests/fixtures/gh/pr-426-watch.json");

/// Answers each query in turn, and records what was asked.
fn script(answers: Vec<Value>) -> (Vec<Value>, impl FnMut(Ask) -> Result<Value, ReadError>) {
    let mut answers = answers.into_iter();
    (Vec::new(), move |_ask: Ask| {
        answers.next().ok_or(ReadError::Failed("not scripted".into()))
    })
}

fn no_runs(_: &str) -> Result<Value, ReadError> {
    Ok(json!({ "workflow_runs": [] }))
}

fn read_426(remarks: bool) -> Reading {
    let (_, mut ask) = script(vec![serde_json::from_str(WATCH_426).unwrap()]);
    reading("josephwylie/ferrite", 426, remarks, &mut ask, &mut no_runs).expect("a PR")
}

#[test]
fn reads_a_real_graphql_answer() {
    let reading = read_426(true);
    assert_eq!(reading.state, PrState::Merged);
    assert_eq!(
        reading.head.as_deref(),
        Some("08a842c2ade930b8c8ffa515ab3103b5dc92c2c8")
    );
    assert_eq!(reading.base_branch, "main");
    assert_eq!(reading.mergeable, Mergeable::Unknown);
    assert_eq!(reading.author.as_deref(), Some("josephwylie"));
    assert_eq!(reading.viewer.as_deref(), Some("josephwylie"));
    assert_eq!(reading.merged_by.as_deref(), Some("josephwylie"));
    assert_eq!(
        reading.merge_commit.as_deref(),
        Some("2b69f88c3c052d9c261038aeb0306d5d93a4c762")
    );
    assert_eq!(reading.merge_queue_removal, None);
    let names: Vec<&str> = reading.checks.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "CI / scope",
            "Packaged browser window / scope",
            "macOS (Metal)",
            "CEF pass, production Cockpit window (macos-latest)",
            "CEF pass, production Cockpit window (windows-latest)",
            "Windows (DirectX 11)",
            "Browser verification decisions",
            "Verification decisions",
        ]
    );
    let gate: Vec<&str> = reading
        .checks
        .iter()
        .filter(|c| c.required)
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(gate, ["macOS (Metal)", "Windows (DirectX 11)"]);
    assert_eq!(reading.checks[5].status, Skipped);
    let remarks = reading.remarks.as_ref().expect("comments read");
    assert_eq!(remarks.len(), 2);
    assert_eq!(remarks[0].id, "IC_kwDOUDRvB88AAAABpO-vJg");
    assert_eq!(remarks[0].created_ms, ms("2026-11-21T11:48:29Z"));
    assert_eq!(
        remarks[0].edited_ms,
        Some(ms("2026-11-21T12:24:53Z")),
        "GraphQL says when a remark was edited"
    );

    let watching = watched(426, &format!("{FERRITE}/pull/426"));
    assert_eq!(
        wake_text(&watching, &ev(&watch(), &reading)).expect("ended").text,
        "Pull request #426 (https://github.com/josephwylie/ferrite/pull/426) was merged into main by josephwylie as 2b69f88, so Ferrite stopped watching it."
    );
    // Open, its checks pass the required pair.
    let open = Reading {
        state: PrState::Open,
        remarks: Some(Vec::new()),
        ..reading
    };
    assert!(wake_text(&watching, &ev(&watch(), &open))
        .expect("news")
        .text
        .contains("\n- All 2 required checks passed on 08a842c.\n"));
    assert_eq!(read_426(false).remarks, None, "remarks not asked for");
}

#[test]
fn reads_the_newest_run_of_each_check_and_lists_failures_first() {
    let answer = json!({ "data": { "viewer": { "login": "agent-user" }, "repository": { "pullRequest": {
        "state": "OPEN", "headRefOid": "abc", "baseRefName": "main", "mergeable": "CONFLICTING",
        "isCrossRepository": false,
        "commits": { "nodes": [{ "commit": { "statusCheckRollup": { "contexts": { "nodes": [
            { "name": "test", "status": "COMPLETED", "conclusion": "FAILURE", "completedAt": "2026-01-01T00:00:00Z", "startedAt": "2026-01-01T00:00:00Z", "detailsUrl": "https://x/1", "checkSuite": { "workflowRun": { "workflow": { "name": "CI" } } } },
            { "name": "test", "status": "IN_PROGRESS", "conclusion": null, "completedAt": null, "startedAt": "2026-01-01T00:05:00Z", "detailsUrl": "https://x/2", "checkSuite": { "workflowRun": { "workflow": { "name": "CI" } } } },
            { "context": "codecov", "state": "ERROR", "targetUrl": "https://codecov", "createdAt": "2026-01-01T00:00:00Z" },
            {},
            { "name": "", "status": "COMPLETED", "conclusion": "SUCCESS" }
        ], "pageInfo": { "hasNextPage": false } } } } }] },
        "comments": { "pageInfo": { "hasNextPage": false }, "nodes": [
            { "id": "c1", "author": { "login": "bot" }, "body": "hidden", "createdAt": "2026-01-01T00:00:00Z", "isMinimized": true }
        ] },
        "reviews": { "pageInfo": { "hasNextPage": false }, "nodes": [
            { "id": "r1", "author": { "login": "rev" }, "body": "", "state": "COMMENTED", "submittedAt": "2026-01-01T00:00:00Z" },
            { "id": "r2", "author": { "login": "rev" }, "body": "", "state": "APPROVED", "submittedAt": "2026-01-01T00:01:00Z" }
        ] },
        "reviewThreads": { "pageInfo": { "hasNextPage": false }, "nodes": [] }
    } } } });
    let (_, mut ask) = script(vec![answer]);
    let reading = reading("o/r", 1, true, &mut ask, &mut no_runs).expect("a PR");
    assert_eq!(reading.mergeable, Mergeable::Conflicting);
    let read: Vec<(&str, CheckStatus, Option<&str>)> = reading
        .checks
        .iter()
        .map(|c| (c.name.as_str(), c.status, c.url.as_deref()))
        .collect();
    assert_eq!(
        read,
        [
            ("codecov", Failure, Some("https://codecov")),
            ("test", Pending, Some("https://x/2")),
        ]
    );
    let remarks = reading.remarks.expect("comments read");
    assert_eq!(
        remarks.len(),
        1,
        "the hidden comment and the bodiless comment review are left out"
    );
    assert_eq!(remarks[0].review_state.as_deref(), Some("APPROVED"));
}

fn pr(fields: Value) -> Value {
    let mut pr = json!({
        "state": "OPEN", "headRefOid": "abc", "headRefName": "feature", "baseRefName": "main",
        "mergeable": "MERGEABLE", "isCrossRepository": false,
        "headRepositoryOwner": { "login": "fork-owner" },
        "commits": { "nodes": [{ "commit": { "statusCheckRollup": null } }] },
        "comments": { "pageInfo": { "hasNextPage": false }, "nodes": [] },
        "reviews": { "pageInfo": { "hasNextPage": false }, "nodes": [] },
        "reviewThreads": { "pageInfo": { "hasNextPage": false }, "nodes": [] }
    });
    pr.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    json!({ "data": { "viewer": { "login": "agent-user" }, "repository": { "pullRequest": pr } } })
}

fn comment(id: &str, at: &str) -> Value {
    json!({ "id": id, "author": { "login": "rev" }, "body": "Rename this.", "createdAt": at, "url": format!("https://x/{id}") })
}

fn page(nodes: Vec<Value>, after: Option<&str>) -> Value {
    json!({ "pageInfo": { "hasNextPage": after.is_some(), "endCursor": after }, "nodes": nodes })
}

/// Every page of comments and of a long review thread is read, and a
/// thread's comments carry its file.
#[test]
fn follows_every_page_of_remarks() {
    let first = pr(json!({
        "comments": page(vec![comment("c1", "2026-01-01T00:00:00Z")], Some("c-next")),
        "reviewThreads": page(vec![json!({ "id": "t1", "path": "src/lib.rs", "comments": {
            "totalCount": 2,
            "pageInfo": { "hasNextPage": true, "endCursor": "t-next" },
            "nodes": [comment("t1-a", "2026-01-01T00:02:00Z")]
        } })], None),
    }));
    let comments_page = json!({ "data": { "repository": { "pullRequest": {
        "comments": page(vec![comment("c2", "2026-01-01T00:01:00Z")], None)
    } } } });
    let thread_page = json!({ "data": { "node": {
        "comments": page(vec![comment("t1-b", "2026-01-01T00:03:00Z")], None)
    } } });
    let mut asked = Vec::new();
    let mut answers = vec![first, comments_page, thread_page].into_iter();
    let mut ask = |ask: Ask| {
        asked.push((ask.query, ask.variables));
        answers.next().ok_or(ReadError::Failed("not scripted".into()))
    };
    let read = reading("o/r", 1, true, &mut ask, &mut no_runs).expect("a PR");
    let ids: Vec<(String, Option<String>)> = read
        .remarks
        .expect("read whole")
        .into_iter()
        .map(|remark| (remark.id, remark.path))
        .collect();
    assert_eq!(
        ids,
        [
            ("c1".to_string(), None),
            ("c2".to_string(), None),
            ("t1-a".to_string(), Some("src/lib.rs".to_string())),
            ("t1-b".to_string(), Some("src/lib.rs".to_string())),
        ]
    );
    assert_eq!(asked[1].0, read::COMMENT_PAGE);
    assert_eq!(asked[1].1["after"], "c-next");
    assert_eq!(asked[1].1["number"], 1);
    assert_eq!(asked[2].0, read::THREAD_COMMENTS);
    assert_eq!(asked[2].1, json!({ "thread": "t1", "after": "t-next" }));
}

/// A thread that lists fewer comments than it counts was not read whole,
/// so the remarks are not trusted.
#[test]
fn an_incomplete_thread_leaves_the_remarks_unread() {
    let first = pr(json!({
        "reviewThreads": page(vec![json!({ "id": "t1", "path": "a.rs", "comments": {
            "totalCount": 3,
            "pageInfo": { "hasNextPage": false },
            "nodes": [comment("t1-a", "2026-01-01T00:02:00Z")]
        } })], None),
    }));
    let (_, mut ask) = script(vec![first]);
    let read = reading("o/r", 1, true, &mut ask, &mut no_runs).expect("a PR");
    assert_eq!(read.remarks, None);
}

fn runs(runs: Value) -> impl FnMut(&str) -> Result<Value, ReadError> {
    move |path: &str| {
        assert_eq!(
            path,
            "repos/o/r/actions/runs?head_sha=abc&event=pull_request&per_page=100"
        );
        Ok(json!({ "workflow_runs": runs.clone() }))
    }
}

#[test]
fn an_unfinished_run_with_no_job_yet_is_a_pending_check() {
    let (_, mut ask) = script(vec![pr(json!({}))]);
    let mut rest = runs(json!([
        { "id": 7, "name": "CI", "html_url": "https://github.com/o/r/actions/runs/7", "status": "queued", "head_branch": "feature" },
        { "id": 8, "name": "Docs", "html_url": "https://github.com/o/r/actions/runs/8", "status": "completed", "conclusion": "success" }
    ]));
    let read = reading("o/r", 1, false, &mut ask, &mut rest).expect("a PR");
    assert_eq!(
        read.checks,
        [Check {
            name: "CI".into(),
            status: Pending,
            url: Some("https://github.com/o/r/actions/runs/7".into()),
            required: false,
            placeholder: false,
        }]
    );
}

#[test]
fn a_forks_run_awaiting_approval_is_an_action_required_check() {
    let (_, mut ask) = script(vec![pr(json!({ "isCrossRepository": true }))]);
    let mut rest = runs(json!([
        { "id": 9, "name": "CI", "html_url": "https://github.com/o/r/actions/runs/9", "status": "completed", "conclusion": "action_required", "head_branch": "feature" },
        { "id": 10, "name": "Other", "status": "completed", "conclusion": "action_required", "head_branch": "elsewhere" }
    ]));
    let read = reading("o/r", 1, false, &mut ask, &mut rest).expect("a PR");
    let read: Vec<(&str, CheckStatus)> = read.checks.iter().map(|c| (c.name.as_str(), c.status)).collect();
    assert_eq!(read, [("CI", ActionRequired)]);

    // Unreadable runs on a fork leave T3's placeholder, which holds the
    // verdict open; a rate limit fails the read.
    let (_, mut ask) = script(vec![pr(json!({ "isCrossRepository": true }))]);
    let mut failing = |_: &str| Err(ReadError::Failed("502".into()));
    let read = reading("o/r", 1, false, &mut ask, &mut failing).expect("a PR");
    assert_eq!(read.checks.len(), 1);
    assert!(read.checks[0].placeholder);
    let (_, mut ask) = script(vec![pr(json!({}))]);
    let mut limited = |_: &str| Err(ReadError::RateLimited);
    assert_eq!(
        reading("o/r", 1, false, &mut ask, &mut limited),
        Err(ReadError::RateLimited)
    );
}

#[test]
fn a_missing_pull_request_is_refused() {
    let (_, mut ask) = script(vec![json!({
        "data": { "repository": { "pullRequest": null } },
        "errors": [{ "type": "NOT_FOUND", "message": "Could not resolve to a PullRequest with the number of 9." }]
    })]);
    assert_eq!(
        reading("o/r", 9, true, &mut ask, &mut no_runs),
        Err(ReadError::Refused(
            "Could not resolve to a PullRequest with the number of 9.".into()
        ))
    );
}

#[test]
fn the_fingerprint_moves_with_checks_and_remarks() {
    let answer = |edited: &str, success: u64| {
        json!({ "data": { "repository": { "pullRequest": {
            "state": "OPEN", "mergeable": "MERGEABLE", "headRefOid": "abc",
            "comments": { "totalCount": 2, "nodes": [{ "lastEditedAt": edited }, { "lastEditedAt": null }] },
            "reviews": { "totalCount": 0, "nodes": [] },
            "reviewThreads": { "totalCount": 1 },
            "mergeQueueRemovals": { "filteredCount": 0 },
            "commits": { "nodes": [{ "commit": { "statusCheckRollup": { "contexts": {
                "checkRunCountsByState": [{ "state": "SUCCESS", "count": success }, { "state": "FAILURE", "count": 0 }, { "state": "IN_PROGRESS", "count": 1 }],
                "statusContextCountsByState": []
            } } } }] }
        } } } })
    };
    let read = |edited: &str, success: u64| {
        let (_, mut ask) = script(vec![answer(edited, success)]);
        fingerprint("o/r", 1, &mut ask).expect("a fingerprint")
    };
    let first = read("2026-01-01T00:00:00Z", 2);
    assert_eq!(first.status, "OPEN MERGEABLE abc IN_PROGRESS:1,SUCCESS:2  0");
    assert_eq!(first.remarks, "2 2026-01-01T00:00:00Z 0  1");
    assert_ne!(read("2026-01-01T00:00:00Z", 3).status, first.status);
    assert_ne!(read("2026-01-01T00:09:00Z", 2).remarks, first.remarks);
}

fn target(number: u64) -> Target {
    Target::of("o/r", number).expect("a PR")
}

/// Watching again starts from what the ended watch told, with the
/// remarks counted from the new start.
#[test]
fn watching_again_starts_from_the_ended_watch() {
    let mut fact = WatchFact::default();
    assert!(!fact.watch(target(1), None, Some(true), 1_000));
    let watch = fact.on("O/R", 1).expect("on, names ignore case").clone();
    assert_eq!(watch.until, Until::Checks);
    assert_eq!(watch.url, "https://github.com/o/r/pull/1");
    assert_eq!(watch.cursor.awaiting_baseline, Some(Baseline::All));
    assert!(watch.cursor.checks_pending);
    assert_eq!(
        (watch.cursor.started_ms, watch.cursor.remarks_through_ms),
        (1_000, 1_000)
    );
    assert!(fact.watch(target(1), Some(Until::Merged), None, 2_000));
    assert_eq!(fact.on("o/r", 1).unwrap().until, Until::Merged);

    let told = Cursor {
        head: Some("abc".into()),
        failed_checks: vec!["test".into()],
        conflicting: true,
        wakes: 4,
        remark_ids: vec!["c1".into()],
        awaiting_baseline: None,
        ..watch.cursor.clone()
    };
    assert!(!fact.advance("o/r", 1, 999, told.clone()), "another watch's read");
    assert!(fact.advance("o/r", 1, 1_000, told.clone()));
    assert!(fact.end("o/r", 1, None, None));
    assert_eq!(fact.on, []);
    assert_eq!(fact.ended[0].cursor, told);

    fact.watch(target(1), None, None, 5_000);
    let again = &fact.on("o/r", 1).unwrap().cursor;
    assert_eq!(again.head.as_deref(), Some("abc"));
    assert_eq!(again.failed_checks, ["test"]);
    assert!(again.conflicting);
    assert_eq!(again.awaiting_baseline, None, "the old watch's baseline is done");
    assert_eq!((again.wakes, again.remark_ids.len()), (0, 0));
    assert_eq!((again.started_ms, again.remarks_through_ms), (5_000, 5_000));
    assert_eq!(fact.ended, [], "the new watch supersedes the ended one");

    fact.watch(target(2), None, None, 6_000);
    assert!(fact.end_all());
    assert_eq!(fact.on, []);
    assert_eq!(fact.ended.len(), 2);
    assert!(!fact.end_all());
}

#[test]
fn a_target_is_a_github_pull_request() {
    assert_eq!(
        Target::of_url("https://github.com/josephwylie/ferrite/pull/440/"),
        Target::of("josephwylie/ferrite", 440)
    );
    assert_eq!(Target::of_url("https://github.com/o/r/issues/4"), None);
    assert_eq!(Target::of_url("https://gitlab.com/o/r/pull/4"), None);
    assert_eq!(Target::of("o/r/../x", 4), None);
    assert_eq!(Target::of("o/r", 0), None);
}

/// A waiting wake is owed while its watch is on, on the head its news is
/// about; a push makes check news stale but not remarks; the notice that
/// ended a watch is owed until a new watch replaces it (T3's
/// `isStalePullRequestWatchStamp`).
#[test]
fn a_waiting_wake_is_owed_only_while_its_news_stands() {
    let target = || Target::of("acme/app", 7).unwrap();
    let mut fact = WatchFact::default();
    fact.watch(target(), None, None, 10);
    let stamp = |head: Option<&str>, ended: bool| Stamp {
        repository: "Acme/App".into(),
        number: 7,
        started_ms: 10,
        head: head.map(str::to_string),
        ended,
    };
    let mut on_head = fact.on[0].cursor.clone();
    on_head.head = Some("abc".into());
    assert!(fact.advance("acme/app", 7, 10, on_head.clone()));
    assert!(fact.owes(&stamp(Some("abc"), false)));
    assert!(fact.owes(&stamp(None, false)), "remarks outlive a push");
    on_head.head = Some("def".into());
    assert!(fact.advance("acme/app", 7, 10, on_head));
    assert!(!fact.owes(&stamp(Some("abc"), false)), "a push made it stale");
    assert!(fact.end("acme/app", 7, Some(10), None));
    assert!(!fact.owes(&stamp(None, false)), "unwatched");
    assert!(fact.owes(&stamp(None, true)), "the ending notice");
    fact.watch(target(), None, None, 20);
    assert!(!fact.owes(&stamp(None, true)), "a new watch replaced it");
}
