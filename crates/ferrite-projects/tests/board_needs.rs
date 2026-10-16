//! A task that waits for you says what it needs: its mark (? an answer, ! a yes or no, @ a sign-in) and the words beside
//! it, on a row, in the task panel and on the web board; never the look of one that went wrong. A task a limit stopped,
//! or near its limit, says so too.
use serde_json::{json, Value};
use sw::board::state::State;
use sw::board::theme::Theme;
use sw::json::*;
use sw::shown::states::standing;

fn base() -> Value { json!({ "id": "proj/1", "title": "Plan v1", "agent": "claude", "app": "claude", "at": sw::clock::now(), "owner": "me" }) }
fn with(more: Value) -> Obj { let mut b = base(); for (k, v) in more.as_object().unwrap() { b[k] = v.clone(); } obj(b) }
fn asks(kind: Option<&str>) -> Obj { let mut o = with(json!({ "state": "blocked", "stage": "asks", "msg": "What one job must v1 nail?" })); if let Some(k) = kind { o.put("kind", k); } o }
fn login() -> Obj { with(json!({ "state": "blocked", "kind": "auth", "stage": "failed", "msg": "", "failure": { "auth": true, "title": "Needs a login", "detail": "Not logged in", "next": "Sign in to the agent." } })) }
fn broke() -> Obj { with(json!({ "state": "error", "stage": "nosummary", "msg": "Stopped without a summary" })) }
fn row(r: &Obj, width: usize) -> String {
    let t = Theme::default_dark(true);
    let mut s = State::new(Default::default(), Default::default());
    let rec = std::sync::Arc::new(r.clone());
    let lay = sw::board::view::row::row_layout(&mut s, &t, std::slice::from_ref(&rec), width, &[vec![rec.clone()]], 200);
    sw::board::view::row::row_lines(&s, &t, r, width, &lay).iter().map(|l| l.text()).collect::<Vec<_>>().join("\n")
}
fn page(r: &Obj, width: usize, full: bool) -> String { let mut s = State::new(Default::default(), Default::default()); sw::board::view::panel::panel(&mut s, &Theme::default_dark(true), r, width, full, 0).iter().map(|l| l.text()).collect::<Vec<_>>().join("\n") }
fn has(text: &str, re: &str) -> bool { regex::Regex::new(re).unwrap().is_match(text) }

#[test]
fn a_blocked_tasks_mark_says_what_it_needs_and_is_never_the_mark_of_one_that_went_wrong() {
    for width in [40, 80, 160] {
        assert!(has(&row(&asks(Some("question")), width), r"^ \? #\d+ +Plan v1 "), "a question at {width}");
        assert!(has(&row(&asks(Some("permission")), width), r"^ ! #\d+ +Plan v1 "));
        assert!(has(&row(&asks(Some("auth")), width), r"^ @ #\d+ +Plan v1 "));
        assert!(has(&row(&login(), width), r"^ @ #\d+ +Plan v1 "), "a login that failed at {width}");
        assert!(has(&row(&asks(None), width), r"^ ! #\d+ +Plan v1 "));
        assert!(has(&row(&broke(), width), r"^ ✗ #\d+ +Plan v1 "));
    }
    let ready = with(json!({ "state": "blocked", "kind": "permission", "stage": "approval", "msg": "Approve to merge?" }));
    for width in [40, 160] { assert!(has(&row(&ready, width), r"^ ◆ #\d+ +Plan v1 "), "a merge to approve at {width}"); }
    let t = Theme::default_dark(false);
    assert_eq!(sw::board::view::fields::mark(&t, &ready).0[0].c, sw::shown::ink::Ink::Green);
    assert_eq!(sw::board::view::fields::mark(&t, &asks(Some("permission"))).0[0].c, sw::shown::ink::Ink::Red);
    let marks: Vec<String> = [asks(Some("question")), asks(Some("permission")), asks(Some("auth")), asks(None)].iter().map(|r| sw::board::view::fields::mark(&t, r).text()).collect();
    assert_eq!(marks, vec!["?", "!", "@", "!"]);
}

#[test]
fn the_words_beside_the_mark_agree_with_the_kind() {
    assert!(has(&page(&asks(Some("question")), 80, false), r"\n State        \? asks you · 0s ago\n") && has(&page(&asks(Some("question")), 80, false), r"\n Asks you ─+\n What one job must v1 nail\?"), "{}", page(&asks(Some("question")), 80, false));
    assert!(has(&page(&asks(Some("permission")), 80, false), r"\n State        ! asks for your OK · 0s ago\n") && has(&page(&asks(Some("permission")), 80, false), r"\n Asks for your OK ─+\n"));
    assert!(has(&page(&asks(Some("auth")), 80, false), r"\n State        @ needs a login · 0s ago\n") && has(&page(&asks(Some("auth")), 80, false), r"\n Needs a login ─+\n"));
    assert!(has(&page(&asks(None), 80, false), r"\n State        ! needs you · 0s ago\n") && has(&page(&asks(None), 80, false), r"\n Needs you ─+\n"));
    assert!(has(&page(&login(), 80, false), r"\n State        @ needs a login · 0s ago\n") && !page(&login(), 80, false).contains("Went wrong"));
    assert!(has(&page(&broke(), 80, false), r"\n State        ✗ needs you · 0s ago\n"));
    let mut failed = asks(Some("question")); failed.put("failure", json!({ "title": "Merge conflict with main" }));
    assert_eq!(standing(&failed), "needs you");
    assert_eq!(standing(&with(json!({ "state": "blocked", "kind": "permission", "stage": "approval" }))), "ready to approve");
    assert_eq!(standing(&with(json!({ "state": "working", "stage": "working" }))), "working");
}

#[test]
fn the_kind_comes_from_where_the_task_stands_and_the_terminals_records_carry_it() {
    let kind = |t: Value, live: Value| { let s = sw::state::show(&with(t), &live, sw::clock::now()); (s.state, s.kind) };
    assert_eq!(kind(json!({ "stage": "asks" }), Value::Null), ("blocked", Some("question".into())));
    assert_eq!(kind(json!({ "stage": "asks", "kind": "auth" }), Value::Null), ("blocked", Some("auth".into())));
    assert_eq!(kind(json!({ "stage": "approval" }), Value::Null), ("blocked", Some("permission".into())));
    assert_eq!(kind(json!({ "stage": "failed", "failure": { "auth": true } }), Value::Null), ("blocked", Some("auth".into())));
    assert_eq!(kind(json!({ "stage": "failed", "failure": {} }), Value::Null), ("error", None));
    assert_eq!(kind(json!({ "stage": "working" }), json!({ "status": "blocked" })), ("blocked", None));
    let web = Theme::web();
    let at = |r: &Obj| sw::board::view::fields::mark(&web, r);
    assert_eq!([asks(Some("question")), asks(Some("permission")), asks(Some("auth")), asks(None), broke()].iter().map(|r| at(r).text()).collect::<Vec<_>>(), vec!["?", "!", "@", "!", "✗"]);
    assert!([asks(Some("question")), asks(Some("auth")), broke()].iter().all(|r| at(r).0[0].c == sw::shown::ink::Ink::Red));
    assert!(sw::board::update::beat::osc(&asks(Some("question"))).starts_with("\x1b]7501;state=blocked:id=proj/1:kind=question:"));
    assert!(!sw::board::update::beat::osc(&broke()).contains("kind="));
}

#[test]
fn a_task_a_limit_stopped_and_one_near_its_limit_say_so() {
    let stopped = |ls: Value, msg: &str| { let mut r = with(json!({ "stage": "stopped", "state": "blocked", "msg": msg, "raw": msg, "limitStop": ls, "link": { "pane": "p1" } })); r.put("kind", Value::Null); r };
    let time = stopped(json!({ "what": "time", "limit": "2h", "role": "agent", "used": 8400000, "at": 1 }), "stopped at its 2h limit");
    assert_eq!(standing(&time), "needs you");
    let p = page(&time, 110, true);
    assert!(has(&p, r"\n Stopped at a limit ─+\n It stopped at its 2h limit, after 2h20m\. Its work is kept as it is and nothing was merged\.\n"), "{p}");
    assert!(p.contains("herdr-projects set limit.time <value> --task proj/1"));
    let flat = |r: &Obj| regex::Regex::new(r"\s+").unwrap().replace_all(&page(r, 110, true), " ").into_owned();
    assert!(has(&flat(&time), r"carries on, and so does a reply to the agent\. \S+ takes the work as it is, through the check and the review\."), "{}", flat(&time));
    assert!(has(&flat(&stopped(json!({ "what": "time", "limit": "30m", "role": "reviewer", "used": 1860000, "at": 1 }), "stopped at its 30m limit")), r"The reviewer stopped at its 30m limit, after 31m\.[\s\S]*carries on: it is reviewed again"));
    assert!(has(&flat(&stopped(json!({ "what": "tokens", "limit": "350k", "role": "reviewer", "review": true, "used": 360000, "at": 1 }), "stopped at 350k tokens")), r"It stopped at 350k tokens, with 360k tokens used\.[\s\S]*carries on: the review goes to the agent"));
    assert!(has(&flat(&stopped(json!({ "what": "tokens", "limit": "5m", "role": "reviewer", "ready": true, "used": 5200000, "at": 1 }), "stopped at 5m tokens")), r"It stopped at 5m tokens, with 5\.2m tokens used\.[\s\S]*carries on: its work was finished and reviewed, and is put up for approval"));
    let near = with(json!({ "state": "working", "stage": "working", "msg": "Running the tests", "limits": { "time": { "limit": "2h", "used": 6000000 }, "tokens": { "limit": "500k", "used": 410000, "kind": "claude" }, "near": "1h40m of its 2h · 410k of 500k tokens" } }));
    assert!(has(&page(&near, 110, true), r"\n Time         1h40m of 2h, this run\n Tokens       410k of 500k\n"), "{}", page(&near, 110, true));
    assert_eq!(sw::shown::row_says::sentence(&near).text(), "1h40m of its 2h · 410k of 500k tokens · Running the tests");
    let uncounted = with(json!({ "state": "working", "stage": "working", "msg": "Running the tests", "limits": { "tokens": { "limit": "500k", "used": 0, "kind": "a-program-with-no-counter" } } }));
    assert!(page(&uncounted, 110, true).contains("\n Tokens       not counted for this program · limit 500k\n"));
}
