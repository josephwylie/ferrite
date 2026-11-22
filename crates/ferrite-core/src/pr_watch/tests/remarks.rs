//! Comments and reviews: what is new, whose, and how a wake quotes them.

use super::*;

#[test]
fn keeps_remarks_for_a_later_pass_when_the_conversation_was_not_read_whole() {
    let comments = [remark("reviewer", "2026-10-02T12:06:00Z")];
    let partial = ev(
        &watch(),
        &Reading {
            remarks: None,
            ..detail()
        },
    );
    assert_eq!(partial.changes, []);
    assert!(matches!(
        changes(&partial.next, &said(detail(), &comments))[0],
        Change::Remarks(_)
    ));
}

#[test]
fn reports_a_remark_that_shows_up_late_with_the_same_time_as_a_reported_one() {
    let first = remark("reviewer", "2026-10-02T12:06:00Z");
    let late = Remark {
        id: "late".into(),
        ..remark("bot", "2026-10-02T12:06:00Z")
    };
    let reported = ev(&watch(), &said(detail(), &[first.clone()]));
    let again = ev(
        &reported.next,
        &said(detail(), &[first.clone(), late.clone()]),
    );
    assert_eq!(again.changes, [remarks(&[late])]);
    assert_eq!(again.next.remark_ids, [first.id, "late".into()]);
}

#[test]
fn reports_edits_after_the_watermark_once_and_counts_them_toward_the_wake_limit() {
    let old = remark("greptile[bot]", "2026-10-02T11:00:00Z");
    let watching = Cursor {
        remark_ids: vec![old.id.clone()],
        wakes: WAKE_LIMIT - 1,
        ..seen_head()
    };
    assert_eq!(changes(&watching, &said(detail(), &[old.clone()])), []);
    let edited = Remark {
        edited_ms: Some(ms("2026-10-02T12:06:00Z")),
        ..old.clone()
    };
    let report = ev(&watching, &said(detail(), &[edited.clone()]));
    assert_eq!(report.changes, [remarks(&[edited.clone()])]);
    assert_eq!(report.next.remarks_through_ms, ms("2026-10-02T12:06:00Z"));
    assert_eq!(report.next.remark_ids, [old.id]);
    assert_eq!(report.end, Some(End::CommentLimit));
    assert_eq!(
        changes(&report.next, &said(detail(), &[edited.clone()])),
        []
    );
    let late = Remark {
        id: "late".into(),
        ..edited.clone()
    };
    assert_eq!(
        changes(&report.next, &said(detail(), &[edited, late.clone()])),
        [remarks(&[late])]
    );
}

#[test]
fn wakes_once_per_edit_of_a_remark_seen_at_the_watermark_never_for_the_agents_own() {
    let seen = remark("ci-bot", "2026-10-10T19:52:40.000Z");
    let watching = Cursor {
        remarks_through_ms: seen.created_ms,
        remark_ids: vec![seen.id.clone()],
        wakes: 2,
        ..seen_head()
    };
    assert_eq!(changes(&watching, &said(detail(), &[seen.clone()])), []);
    let edit = |at: &str| Remark {
        edited_ms: Some(ms(at)),
        ..seen.clone()
    };
    let edited = edit("2026-10-10T20:04:50.000Z");
    let first = ev(&watching, &said(detail(), &[edited.clone()]));
    assert_eq!(first.changes, [remarks(&[edited.clone()])]);
    assert_eq!(
        first.next.remarks_through_ms,
        ms("2026-10-10T20:04:50.000Z")
    );
    assert_eq!(changes(&first.next, &said(detail(), &[edited.clone()])), []);
    let again = edit("2026-10-10T20:05:50.000Z");
    let second = ev(&first.next, &said(detail(), &[again.clone()]));
    assert_eq!(second.changes, [remarks(&[again.clone()])]);
    assert_eq!(changes(&second.next, &said(detail(), &[again])), []);
    let own = Remark {
        author: Some("agent-user".into()),
        ..edited
    };
    assert_eq!(changes(&watching, &said(detail(), &[own])), []);
}

#[test]
fn does_not_treat_a_failed_check_read_as_a_rerun() {
    let broken = checks(vec![check("lint", Failure)]);
    let reported = ev(&watch(), &broken);
    assert_eq!(reported.changes.len(), 1);
    let unreadable = ev(&reported.next, &checks(vec![]));
    assert_eq!(changes(&unreadable.next, &broken), []);
}

#[test]
fn wakes_for_the_pull_requests_author_when_the_agent_is_someone_else() {
    let reply = remark("contributor", "2026-10-02T12:06:00Z");
    let contributor = Reading {
        author: Some("contributor".into()),
        ..said(detail(), &[reply.clone()])
    };
    assert_eq!(changes(&watch(), &contributor), [remarks(&[reply])]);
    let no_viewer = Reading {
        viewer: None,
        ..contributor
    };
    assert_eq!(changes(&watch(), &no_viewer), []);
}

#[test]
fn reports_remarks_from_others_once_and_never_the_agents_own() {
    let comments = [
        remark_saying(
            "agent-user",
            "2026-10-02T12:05:00Z",
            "Fixed in the latest push.",
        ),
        remark("macroscope-app[bot]", "2026-10-02T12:06:00Z"),
        remark_saying("reviewer", "2026-10-02T11:00:00Z", "Older than the watch."),
    ];
    let reading = said(detail(), &comments);
    let report = ev(&watch(), &reading);
    assert_eq!(report.changes, [remarks(&comments[1..2])]);
    assert_eq!(report.next.remarks_through_ms, ms("2026-10-02T12:06:00Z"));
    assert_eq!(changes(&report.next, &reading), []);
}

#[test]
fn cuts_a_long_remark_in_the_wake_message_without_splitting_an_emoji() {
    let body = format!("{}🚀 and more", "x".repeat(196));
    let report = ev(
        &watch(),
        &said(
            detail(),
            &[remark_saying("reviewer", "2026-10-02T12:06:00Z", &body)],
        ),
    );
    let text = wake(1, PR_1, &report).text;
    assert!(
        text.contains(&format!("\"{}...\"", "x".repeat(196))),
        "{text}"
    );
}

#[test]
fn quotes_a_remarks_text_without_its_html_tags() {
    let cases = [
        (
            "a bot's leading badge",
            "<a href=\"https://app.coderabbit.ai/change-stack/o/r/pull/1?cs_source=review_comment\"><picture><source media=\"(prefers-color-scheme: dark)\" srcset=\"https://coderabbit.ai/badge-dark.svg\"><img alt=\"Severity > `Minor`\" src=\"https://coderabbit.ai/badge.svg\"></picture></a>\r\n\r\n_⚠️ Potential issue_ | _🟠 Major_\r\n\r\n**Guard the empty list.**",
            "_⚠️ Potential issue_ | _🟠 Major_ **Guard the empty list.**",
        ),
        (
            "tags in prose and a hidden comment",
            "<!-- state -->\n<details>\n<summary>Why</summary>\n\n<p>Guard this</p>, then <br/>retry.\n</details>",
            "Why Guard this , then retry.",
        ),
        (
            "type arguments",
            "Return Promise<void>, not Promise<A> or Vec<code>.",
            "Return Promise<void>, not Promise<A> or Vec<code>.",
        ),
        (
            "code spans and fences",
            "Replace `<div>` with ``<span>``:\n~~~html\n<p>Done</p>\n~~~\r\nor\r\n```html\r\n<b>Done</b>\r\n```",
            "Replace `<div>` with ``<span>``: ~~~html <p>Done</p> ~~~ or ```html <b>Done</b> ```",
        ),
        (
            "indented code",
            "Use this replacement:\n\n    <div>\n      <span>Ready</span>\n    </div>\n\nKeep the <b>wrapper</b>.",
            "Use this replacement: <div> <span>Ready</span> </div> Keep the wrapper .",
        ),
        (
            "an indented continuation of an HTML block",
            "<div>\n    <span>Ready</span>\n</div>",
            "Ready",
        ),
    ];
    for (name, body, quoted) in cases {
        let report = ev(
            &watch(),
            &said(
                detail(),
                &[remark_saying("reviewer", "2026-10-02T12:06:00Z", body)],
            ),
        );
        let text = wake(1, PR_1, &report).text;
        assert!(text.contains(&format!(": \"{quoted}\" ")), "{name}: {text}");
    }
}
