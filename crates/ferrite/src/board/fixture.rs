//! The rows every board screen test draws, as upstream's own
//! (`tests/common/fixtures.rs`, herdr-projects 1.0, MIT): one task in each
//! state the board shows, with the long, wide and odd text that breaks a
//! layout. The board they make reads no file, so a test or a reference
//! scene draws it as it is.

use serde_json::{json, Value};
use sw_board::board::data::Data;
use sw_board::board::state::{Ctx, State};
use sw_board::board::update::list::relist;
use sw_board::json::*;

use super::render;
use super::Board;

/// The clock every fixture is drawn at.
pub(crate) const NOW: i64 = 1_790_000_000_000;
const MIN: i64 = 60_000;

fn review(pass: bool, findings: Vec<Value>) -> Value {
    json!({ "round": 1, "meets": pass, "gate": "major", "pass": pass, "text": if pass { "The change meets the task." } else { "A title is cut in the middle of a word." }, "findings": findings, "checks": ["Ran the whole check."], "limitations": ["Not tried on another machine."], "head": "abcdef12345" })
}

fn finding(severity: &str, subject: &str, pre: bool) -> Value {
    let mut f = json!({ "id": "F1", "severity": severity, "subject": subject, "where": "src/store/titles.js:14", "proof": "ran it with a title of 81 characters" });
    if pre {
        f["preexisting"] = json!(true);
    }
    f
}

/// The rows, by the state they show (upstream's `fixture_rows`, on a task
/// of repo `proj` titled as upstream's world adds it).
pub(crate) fn rows(now: i64) -> Vec<(&'static str, Obj)> {
    let mut base = Obj::new();
    for (k, v) in [
        ("id", json!("proj/1")),
        ("title", json!("Max length for task titles")),
        ("repo", json!("proj")),
        ("owner", json!("me")),
        ("agent", json!("claude")),
        ("app", json!("claude")),
        ("at", json!(now - MIN)),
        ("reviews", json!([])),
        ("reviewRound", json!(0)),
        ("description", json!("A task title should have a maximum length. A longer one is cut between words.\n\nSecond paragraph, with a/very/long/path/that/has/no/space/in/it/at/all/src/board/view/row-wrap.js and more words after it.")),
    ] {
        base.insert(k.into(), v);
    }
    let failed = review(
        false,
        vec![
            finding("critical", "A title is cut in the middle of a word", false),
            finding("minor", "The limit is written in two places", false),
            finding("major", "The old menu flickers", true),
        ],
    );
    let with = |n: usize, extra: Value| -> Obj {
        let mut o = base.clone();
        o.put("id", format!("proj/{n}"));
        for (k, v) in extra.as_object().unwrap() {
            if v.is_null() {
                o.remove(k);
            } else {
                o.insert(k.clone(), v.clone());
            }
        }
        o
    };
    vec![
        (
            "to do",
            with(
                1,
                json!({ "agent": null, "app": "", "state": "idle", "stage": "todo" }),
            ),
        ),
        (
            "yours, in progress",
            with(
                2,
                json!({ "agent": null, "app": "me", "state": "working", "stage": "progress", "progress": 40, "eta": 90, "msg": "Drafting" }),
            ),
        ),
        (
            "queued",
            with(
                3,
                json!({ "state": "working", "stage": "queued", "queuePlace": 2, "msg": "Waiting for a free slot" }),
            ),
        ),
        (
            "starting",
            with(
                4,
                json!({ "state": "working", "stage": "starting", "msg": "Starting" }),
            ),
        ),
        (
            "working",
            with(
                5,
                json!({ "state": "working", "stage": "working", "progress": 60, "eta": 11, "msg": "Found the cause; now checking every caller of the resize handler for the same mistake" }),
            ),
        ),
        (
            "working, long numbers",
            with(
                12345,
                json!({ "state": "working", "stage": "working", "progress": 100, "eta": 1439, "msg": "x".repeat(300), "at": now - 400 * 24 * 60 * MIN }),
            ),
        ),
        (
            "checking",
            with(
                7,
                json!({ "state": "working", "stage": "checking", "progress": 65, "eta": 3, "msg": "Running the check · tests 31 of 47" }),
            ),
        ),
        (
            "reviewing",
            with(
                8,
                json!({ "state": "working", "stage": "reviewing", "progress": 35, "eta": 6, "msg": "Reading the diff.", "reviewRound": 1 }),
            ),
        ),
        (
            "asks you",
            with(
                9,
                json!({ "state": "blocked", "kind": "question", "stage": "asks", "msg": "Should the old settings file be removed, or kept for one more version so that nobody loses what they set?" }),
            ),
        ),
        (
            "needs a login",
            with(
                10,
                json!({ "state": "blocked", "kind": "auth", "stage": "asks", "msg": "Needs a login" }),
            ),
        ),
        (
            "ready to approve",
            with(
                11,
                json!({ "state": "blocked", "kind": "permission", "stage": "approval", "pr": "https://github.com/a/b/pull/60", "prState": "passing", "stat": " 19 files changed, 340 insertions(+), 27 deletions(-)", "reviews": [review(true, vec![])], "review": review(true, vec![]), "reviewRound": 1, "msg": "Approve to merge? Review: Pass. Done." }),
            ),
        ),
        (
            "review failed",
            with(
                12,
                json!({ "state": "blocked", "kind": "permission", "stage": "approval", "pr": "https://github.com/a/b/pull/123456", "prState": "failing", "reviews": [failed.clone()], "review": failed, "reviewRound": 1, "msg": "Approve to merge? Review: Fail · 1 critical, 1 minor, 1 pre-existing." }),
            ),
        ),
        (
            "went wrong",
            with(
                13,
                json!({ "state": "error", "stage": "failed", "msg": "Conflict with main", "failure": { "stage": "merge", "phase": "sync", "title": "Conflict with main", "detail": "Conflicting files:\nsrc/board/view/row.js\nsrc/board/view/a-file-with-a-very-long-name-and-no-space-anywhere-in-it.js", "agentCanFix": true, "next": "Resolve the conflict." } }),
            ),
        ),
        (
            "stopped without a summary",
            with(
                14,
                json!({ "state": "error", "stage": "nosummary", "msg": "Stopped without a summary" }),
            ),
        ),
        (
            "done",
            with(
                15,
                json!({ "state": "done", "stage": "done", "pr": "https://github.com/a/b/pull/60", "prState": "merged", "prMerged": true, "reviews": [review(true, vec![])], "review": review(true, vec![]), "reviewRound": 1, "msg": "Merged: https://github.com/a/b/pull/60", "said": "Titles longer than 60 characters are cut between words." }),
            ),
        ),
        (
            "archived",
            with(
                16,
                json!({ "state": "done", "stage": "done", "archivedAt": now - MIN, "msg": "Merged" }),
            ),
        ),
        (
            "research only",
            with(
                17,
                json!({ "state": "working", "stage": "working", "research": true, "msg": "Reading the logs" }),
            ),
        ),
        (
            "a wide title",
            with(
                18,
                json!({ "title": "日本語のタイトルはとても長いので二倍の幅になります 🚀🚀 and then some more", "state": "working", "stage": "working", "progress": 5, "msg": "幅の広い文字 🎉 の行" }),
            ),
        ),
        (
            "a title at the limit",
            with(
                19,
                json!({ "title": "T".repeat(60), "state": "idle", "stage": "todo" }),
            ),
        ),
    ]
}

/// The board on the fixture rows, `proj/5` selected, in Ferrite's theme.
pub(crate) fn board() -> Board {
    let items = rows(NOW)
        .into_iter()
        .map(|(_, row)| std::sync::Arc::new(row))
        .collect();
    let data = Data {
        items,
        loaded: true,
        session: "default".into(),
        repo: "proj".into(),
        repo_root: "/work/proj".into(),
        ..Default::default()
    };
    let ctx = Ctx {
        cwd: "/work/proj".into(),
        ..Ctx::default()
    };
    let mut state = State::new(ctx, data);
    state.now = NOW;
    state.sel = "proj/5".into();
    relist(&mut state);
    Board {
        state,
        theme: render::ferrite_theme(false),
    }
}
