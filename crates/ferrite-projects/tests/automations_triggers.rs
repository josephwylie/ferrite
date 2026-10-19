//! An automation's trigger: a schedule read from plain words or cron (what it says, when it is next due, what is
//! refused), a board event by its name, the events read off a task as it stands, and the rules that name the commands.
use crate::common;
use chrono::TimeZone;
use common::*;
use serde_json::json;
use sw::automations::cron::{cron_next, parse_cron};
use sw::automations::events::marks_of;
use sw::automations::triggers::{next_after, parse_event, parse_schedule, say_trigger};
use sw::json::*;

const MIN: i64 = 60000;
const HOUR: i64 = 60 * MIN;
fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 { chrono::Local.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap().timestamp_millis() }
fn cron(text: &str, after: i64) -> i64 { cron_next(&parse_cron(text).unwrap(), after) }
fn refused(r: sw::err::Res<serde_json::Value>, re: &str) { let e = r.expect_err(re).msg; assert!(regex::Regex::new(re).unwrap().is_match(&e), "{re}: {e}"); }

#[test]
fn a_schedule_in_plain_words_or_as_cron_what_it_says_when_it_is_next_due_and_what_is_refused() {
    assert_eq!(parse_schedule("every 2h").unwrap(), json!({ "kind": "schedule", "text": "every 2h", "every": 2 * HOUR }));
    assert_eq!(parse_schedule("Daily 9:00").unwrap(), json!({ "kind": "schedule", "text": "daily 09:00", "cron": "0 9 * * *" }));
    assert_eq!(parse_schedule("mon 09:00").unwrap(), json!({ "kind": "schedule", "text": "mon 09:00", "cron": "0 9 * * 1" }));
    assert_eq!(parse_schedule("mon,thu 18:30").unwrap().s("cron"), "30 18 * * 1,4");
    assert_eq!(parse_schedule("cron \"0 9 * * 1\"").unwrap(), json!({ "kind": "schedule", "text": "cron 0 9 * * 1", "cron": "0 9 * * 1" }));
    assert_eq!(say_trigger(&parse_schedule("every 30m").unwrap()), "every 30m");
    assert_eq!(say_trigger(&parse_event("merged", false).unwrap()), "when a task is merged");
    assert_eq!(say_trigger(&parse_event("when a task failed its review", true).unwrap()), "when a task failed its review, in every repo");
    // every: counted from when the trigger was set, never from when it last fired
    let since = at(2026, 3, 2, 10, 7);
    assert_eq!(next_after(&parse_schedule("every 2h").unwrap(), since, since), since + 2 * HOUR);
    assert_eq!(next_after(&parse_schedule("every 2h").unwrap(), since + 5 * HOUR, since), since + 6 * HOUR);
    // cron, in this machine's time zone: 2 March 2026 is a Monday
    assert_eq!(cron("0 9 * * 1", at(2026, 3, 2, 8, 0)), at(2026, 3, 2, 9, 0));
    assert_eq!(cron("0 9 * * 1", at(2026, 3, 2, 9, 0)), at(2026, 3, 9, 9, 0));
    assert_eq!(next_after(&parse_schedule("daily 09:00").unwrap(), at(2026, 3, 2, 9, 30), 0), at(2026, 3, 3, 9, 0));
    assert_eq!(cron("*/15 8-10 * * mon-fri", at(2026, 3, 6, 10, 46)), at(2026, 3, 9, 8, 0));
    assert_eq!(cron("0 0 1 jan *", at(2026, 3, 2, 0, 0)), at(2027, 1, 1, 0, 0));
    assert_eq!(cron("0 12 13 * 5", at(2026, 3, 2, 0, 0)), at(2026, 3, 6, 12, 0), "a day of the month and a weekday: either one");
    assert_eq!(cron("0 0 30 2 *", at(2026, 3, 2, 0, 0)), 0);
    refused(parse_schedule("every 2"), r#""2" is not a time with a unit, such as 30m or 2h \(s, m or h\)\. Did you mean 2m\?"#);
    refused(parse_schedule("every 10s"), "once a minute at most");
    refused(parse_schedule("sometimes"), r"is not a schedule\. A schedule is one of: every 2h");
    assert!(parse_cron("0 9 * *").err().unwrap().msg.contains("five fields"));
    refused(parse_schedule("cron 0 9 * *"), "five fields");
    refused(parse_schedule("cron 61 9 * * 1"), r#""61" is not a minute"#);
    refused(parse_schedule("daily 25:00"), "is not a schedule");
    refused(parse_event("merge", false), r#""merge" is not something that happens on the board\. Did you mean merged\? One of: added \(is added\)"#);
}

#[test]
fn the_events_are_read_off_the_task_as_it_stands_and_the_rules_name_the_commands() {
    let _w = world_with(Opts::default()); // no git needed here
    let t = |v: serde_json::Value| -> Obj { let mut o = obj(v); let c = o.at("at").clone(); o.put("created", c); o };
    let kinds = |v: serde_json::Value| -> Vec<String> { marks_of(&t(v)).iter().map(|m| m.split(':').next().unwrap().to_string()).collect() };
    assert_eq!(kinds(json!({ "stage": "approval", "at": 1 })), ["added", "approval"]);
    assert_eq!(marks_of(&t(json!({ "stage": "done", "at": 1, "mergedAt": 7 }))), ["added", "merged:7"]);
    assert_eq!(kinds(json!({ "stage": "asks", "at": 1 })), ["added", "needs-you"]);
    assert_eq!(kinds(json!({ "stage": "failed", "at": 1 })), ["added", "failed"]);
    assert_eq!(kinds(json!({ "stage": "nosummary", "at": 1 })), ["added", "failed"]);
    assert_eq!(kinds(json!({ "stage": "approval", "at": 1, "failure": { "title": "Merge failed" } })), ["added", "needs-you"]);
    assert_eq!(kinds(json!({ "stage": "done", "at": 1 })), ["added"], "called done by hand: nothing was merged");
    assert_eq!(marks_of(&t(json!({ "stage": "working", "at": 1, "reviews": [{ "round": 1, "at": 5, "pass": true }, { "round": 2, "at": 9, "pass": false }] }))), ["added", "review-failed:2:9"]);
    assert_eq!(kinds(json!({ "stage": "working", "at": 1, "reviews": [{ "round": 1, "at": 5, "pass": true }] })), ["added"]);
    assert_eq!(kinds(json!({ "stage": "failed", "at": 1, "reviews": [{ "round": 1, "at": 5, "error": "no verdict" }] })), ["added", "failed", "review-failed"]);
    // the Coordinator's rules and the how-to every chat has name the commands, so an automation is made in plain words
    let coord = sw::coordinator::rules::rules("board");
    for told in [coord.clone(), sw::extras::skill_text::skill_text("board")] {
        for word in ["automations add", "--schedule", "--when <added|approval|review-failed|failed|needs-you|merged>", "--every-repo", "--automation <name>", "no webhook"] { assert!(told.contains(word), "{word}"); }
    }
    assert!(coord.contains("board automations on|off|run|firings|remove <name>"), "{coord}");
    assert!(coord.contains("Make, change, turn off or remove one only when the human asks"));
}
