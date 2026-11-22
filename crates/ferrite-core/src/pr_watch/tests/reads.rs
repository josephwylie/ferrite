//! A real `gh pr view` read, and the check and remark rules it follows.

use super::*;

const VIEW_426: &str = include_str!("../../../tests/fixtures/gh/pr-426-view.json");
const REQUIRED_426: &str = include_str!("../../../tests/fixtures/gh/pr-426-required.json");

#[test]
fn reads_a_real_gh_pr_view() {
    let reading = parse_watch_json(VIEW_426, Some(REQUIRED_426)).expect("a PR");
    assert_eq!(reading.state, PrState::Merged);
    assert_eq!(
        reading.head.as_deref(),
        Some("08a842c2ade930b8c8ffa515ab3103b5dc92c2c8")
    );
    assert_eq!(reading.base_branch, "main");
    assert_eq!(reading.mergeable, Mergeable::Unknown);
    assert_eq!(reading.author.as_deref(), Some("josephwylie"));
    assert_eq!(reading.viewer, None);
    assert_eq!(reading.merged_by.as_deref(), Some("josephwylie"));
    assert_eq!(
        reading.merge_commit.as_deref(),
        Some("2b69f88c3c052d9c261038aeb0306d5d93a4c762")
    );
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
    assert_eq!(
        reading.checks[4].url.as_deref(),
        Some("https://github.com/josephwylie/ferrite/actions/runs/44905771738/job/136859503238")
    );
    let remarks = reading.remarks.as_ref().expect("comments read");
    assert_eq!(remarks.len(), 2);
    assert_eq!(remarks[0].id, "IC_kwDOUDRvB88AAAABpO-vJg");
    assert_eq!(remarks[0].created_ms, ms("2026-11-21T11:48:29Z"));

    let watching = watched(426, &format!("{FERRITE}/pull/426"));
    assert_eq!(
        wake_text(&watching, &ev(&watch(), &reading)).expect("ended").text,
        "Pull request #426 (https://github.com/josephwylie/ferrite/pull/426) was merged into main by josephwylie as 2b69f88, so Ferrite stopped watching it."
    );
    // Open, its checks pass the required pair; without the required read,
    // every check is the gate.
    let open = Reading {
        state: PrState::Open,
        remarks: Some(Vec::new()),
        ..reading
    };
    let told = |reading: &Reading| wake_text(&watching, &ev(&watch(), reading)).expect("news");
    assert!(told(&open)
        .text
        .contains("\n- All 2 required checks passed on 08a842c.\n"));
    let unflagged = Reading {
        state: PrState::Open,
        remarks: Some(Vec::new()),
        ..parse_watch_json(VIEW_426, None).expect("a PR")
    };
    assert!(told(&unflagged)
        .text
        .contains("\n- All 8 checks passed on 08a842c.\n"));
}

#[test]
fn reads_the_newest_run_of_each_check_and_lists_failures_first() {
    let view = r#"{"state":"OPEN","headRefOid":"abc","baseRefName":"main","mergeable":"CONFLICTING","comments":[{"id":"c1","author":{"login":"bot"},"body":"hidden","createdAt":"2026-01-01T00:00:00Z","isMinimized":true}],"reviews":[{"id":"r1","author":{"login":"rev"},"body":"","state":"COMMENTED","submittedAt":"2026-01-01T00:00:00Z"},{"id":"r2","author":{"login":"rev"},"body":"","state":"APPROVED","submittedAt":"2026-01-01T00:01:00Z"}],"statusCheckRollup":[
        {"__typename":"CheckRun","name":"test","workflowName":"CI","status":"COMPLETED","conclusion":"FAILURE","completedAt":"2026-01-01T00:00:00Z","startedAt":"2026-01-01T00:00:00Z","detailsUrl":"https://x/1"},
        {"__typename":"CheckRun","name":"test","workflowName":"CI","status":"IN_PROGRESS","conclusion":"","completedAt":"0001-01-01T00:00:00Z","startedAt":"2026-01-01T00:05:00Z","detailsUrl":"https://x/2"},
        {"__typename":"StatusContext","context":"codecov","state":"ERROR","targetUrl":"https://codecov"},
        {"__typename":"CheckRun","name":"","status":"COMPLETED","conclusion":"SUCCESS"}
    ]}"#;
    let reading = parse_watch_json(view, None).expect("a PR");
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
    assert_eq!(parse_watch_json("{}", None), None);
}
