//! The wake texts: how a watch ends, the wakes T3 Code really sent on
//! Ferrite's own PRs, and the give-up and comment-limit words.

use super::*;

fn ended(reading: Reading) -> Wake {
    let report = ev(&watch(), &reading);
    assert_eq!(report.changes, []);
    wake(7, "https://git.cafe/pingdotgg/t3code/pull/7", &report)
}

#[test]
fn says_how_the_pull_request_ended_with_the_merge_commit_and_merger_the_host_reports() {
    let url = "https://git.cafe/pingdotgg/t3code/pull/7";
    let merged = Reading {
        state: PrState::Merged,
        merge_commit: Some("3b2f958603a51d8c42c4ede02eee7b8cb310b2d9".into()),
        ..detail()
    };
    let by = ended(Reading {
        merged_by: Some("juliusmarminge".into()),
        ..merged.clone()
    });
    assert_eq!(
        by.text,
        format!("Pull request #7 ({url}) was merged into main by juliusmarminge as 3b2f958, so Ferrite stopped watching it.")
    );
    assert_eq!(by.outcome, Outcome::Completed);
    assert_eq!(by.summary, "#7: merged, stopped watching");
    // A forge can report a merge without who made it.
    assert_eq!(
        ended(merged).text,
        format!("Pull request #7 ({url}) was merged into main as 3b2f958, so Ferrite stopped watching it.")
    );

    let closed = ended(Reading {
        state: PrState::Closed,
        ..detail()
    });
    assert_eq!(
        closed.text,
        format!("Pull request #7 ({url}) was closed without merging, so Ferrite stopped watching it. If it is reopened, call watch_pull_request to watch it again.")
    );
    assert_eq!(closed.summary, "#7: closed, stopped watching");
}

// The wakes T3 Code sent on Ferrite's own PRs (design-PR-watching.md 5c),
// with "Ferrite" for "T3 Code".

const CLOSING: &str = "Look into each item and act on it as your task requires. This PR stays linked and watched, so do not link, watch, or list it again. End your turn when you are done, and Ferrite wakes you on the next change. When you hand the work back to the user, call unwatch_pull_request first so the thread returns to their inbox.";

fn update(number: u64, lines: &str) -> String {
    format!("Update on pull request #{number} ({FERRITE}/pull/{number}), which Ferrite is watching for you:\n{lines}\n\n{CLOSING}")
}

/// #426 at 08a842c: two required checks (one skipped), six others.
fn pr_426(statuses: [CheckStatus; 6]) -> Reading {
    let names = [
        "CI / scope",
        "Packaged browser window / scope",
        "CEF pass, production Cockpit window (macos-latest)",
        "CEF pass, production Cockpit window (windows-latest)",
        "Browser verification decisions",
        "Verification decisions",
    ];
    let job = |id: u64| Some(format!("{FERRITE}/actions/runs/44905771738/job/{id}"));
    let mut list = vec![
        at_url(required("macOS (Metal)", Success), job(136850959258)),
        at_url(required("Windows (DirectX 11)", Skipped), job(136850960367)),
    ];
    list.extend(names.iter().zip(statuses).map(|(name, status)| {
        let id = match *name {
            "CEF pass, production Cockpit window (windows-latest)" => 136851403250,
            "Browser verification decisions" => 136859264482,
            _ => 1,
        };
        at_url(check(name, status), job(id))
    }));
    pushed("08a842c2ade930b8c8ffa515ab3103b5dc92c2c8", checks(list))
}

#[test]
fn words_the_wakes_t3_sent_on_one_head_failure_required_pass_failure_then_all_passed() {
    let watched_426 = watched(426, &format!("{FERRITE}/pull/426"));
    let say = |report: &Report| wake_text(&watched_426, report).expect("news").text;
    let mut first = pr_426([Pending; 6]);
    first.checks[0].status = Pending;
    first.checks[1].status = Pending;
    first.checks[5].status = Failure;
    let windows = ev(&watch(), &first);
    assert_eq!(
        say(&windows),
        update(426, "- Checks failed on 08a842c:\n  - CEF pass, production Cockpit window (windows-latest) https://github.com/josephwylie/ferrite/actions/runs/44905771738/job/136851403250")
    );

    // The required pair finish while the windows job reruns.
    let required_done = ev(&windows.next, &pr_426([Pending; 6]));
    assert_eq!(
        say(&required_done),
        update(426, "- All 2 required checks passed on 08a842c.")
    );

    let browser = ev(
        &required_done.next,
        &pr_426([Success, Success, Success, Success, Failure, Pending]),
    );
    assert_eq!(
        say(&browser),
        update(426, "- Checks failed on 08a842c:\n  - Browser verification decisions https://github.com/josephwylie/ferrite/actions/runs/44905771738/job/136859264482")
    );

    // The owed wake: every check, the skipped one included, once all settle.
    let all = ev(&browser.next, &pr_426([Success; 6]));
    assert_eq!(say(&all), update(426, "- All 8 checks passed on 08a842c."));
    assert_eq!(
        wake_text(&watched_426, &ev(&all.next, &pr_426([Success; 6]))),
        None
    );
}

#[test]
fn words_a_merge_and_an_update_in_one_wake_with_the_plural_closing() {
    let merged = ev(
        &watch(),
        &Reading {
            state: PrState::Merged,
            merged_by: Some("josephwylie".into()),
            merge_commit: Some("2b69f88c3c052d9c261038aeb0306d5d93a4c762".into()),
            ..detail()
        },
    );
    let green = ev(
        &watch(),
        &pushed(
            "f8c1821aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            checks(vec![
                required("macOS (Metal)", Success),
                required("Windows (DirectX 11)", Skipped),
                check("scope", Pending),
            ]),
        ),
    );
    let both = combine(vec![
        wake(426, &format!("{FERRITE}/pull/426"), &merged),
        wake(431, &format!("{FERRITE}/pull/431"), &green),
    ])
    .expect("two wakes");
    assert_eq!(
        both.text,
        "Pull request #426 (https://github.com/josephwylie/ferrite/pull/426) was merged into main by josephwylie as 2b69f88, so Ferrite stopped watching it.\n\nUpdate on pull request #431 (https://github.com/josephwylie/ferrite/pull/431), which Ferrite is watching for you:\n- All 2 required checks passed on f8c1821.\n\nLook into each item and act on it as your task requires. Each PR Ferrite still watches stays linked and watched, so do not link, watch, or list them again. End your turn when you are done, and Ferrite wakes you on the next change. When you hand the work back to the user, call unwatch_pull_request for each first so the thread returns to their inbox."
    );
    assert_eq!(both.outcome, Outcome::Completed);
    assert_eq!(
        both.summary,
        "#426: merged, stopped watching; #431: checks passed"
    );
    assert_eq!(combine(Vec::new()), None);
}

#[test]
fn words_a_give_up_and_the_comment_limit() {
    let gave_up = Report {
        changes: Vec::new(),
        next: watch(),
        end: Some(End::Unreadable { problem: None }),
    };
    let told = wake(1, PR_1, &gave_up);
    assert_eq!(
        told.text,
        "Ferrite stopped watching pull request #1 (https://github.com/o/r/pull/1) because it failed to read it from the host 8 times in a row. Check it yourself, and call watch_pull_request only if it is still open."
    );
    assert_eq!(told.summary, "#1: stopped watching, could not read it");

    let tired = Cursor {
        wakes: WAKE_LIMIT - 1,
        ..seen_head()
    };
    let report = ev(
        &tired,
        &said(detail(), &[remark("reviewer", "2026-10-02T12:10:00Z")]),
    );
    let told = wake(1, PR_1, &report);
    assert_eq!(
        told.text,
        "Update on pull request #1 (https://github.com/o/r/pull/1), which Ferrite is watching for you:\n- 1 new comment:\n  - reviewer on src/index.ts: \"Please rename this.\" https://github.com/o/r/pull/1#reviewer\n\nFerrite stopped watching after 10 comment-only updates in a row. Call watch_pull_request to watch it again."
    );
    assert_eq!(told.outcome, Outcome::Updated);
    assert_eq!(told.summary, "#1: new comments, stopped watching");
}
