//! Words handed to an agent, more: what tells a program that has not drawn yet, a slow start typed into only once it
//! has, a swallowed first enter pressed again, a later message's lost enter, two messages left in one box, and an
//! agent herdr cannot classify, all held to the same: enter again, never the words again.
use crate::common;
use common::*;
use serde_json::{json, Value};
use sw::json::*;
use sw::submit::{drawn, mark_of, took, NOT_STARTED, TRIES};

fn waits(ready: i64, submit: i64) { unsafe { std::env::set_var("HERDR_PROJECTS_READY_MS", ready.to_string()); std::env::set_var("HERDR_PROJECTS_SUBMIT_MS", submit.to_string()); } }
fn pass(w: &World) { sw::housekeeping::housekeeping(&w.socket, sw::housekeeping::Force::Yes); }
fn matching(w: &World, re: &str) -> Vec<String> { let r = regex::Regex::new(re).unwrap(); calls(w).into_iter().filter(|c| r.is_match(c)).collect() }
fn count(w: &World, re: &str) -> usize { matching(w, re).len() }
fn the_agent(w: &World, id: &str) -> Value { agent_of(w, task(id).at("link").s("pane")) }
fn said_times(w: &World, id: &str, words: &str) -> usize { the_agent(w, id).arr("prompts").iter().filter(|x| text(x).contains(words)).count() }
fn drain() { flush(); sw::runner::drain_jobs(&|job, id, extra| { sw::jobs::run_job(job, id, extra); flush(); }); }
fn quiet() -> World { world_with(Opts { repo: true, settings: json!({ "review.when": "never" }), ..Default::default() }) }
fn tell(id: &str, words: &str) { sw::actions::talk::tell(id, words, false).unwrap(); }
fn first_agent(w: &World, f: impl FnOnce(&mut Value)) { w.patch(|s| { if let Some(p) = s["panes"].as_array_mut().unwrap().iter_mut().find(|p| p.at("agent").is_object()) { f(&mut p["agent"]); } }); }

#[test]
fn what_tells_a_program_that_has_not_drawn_yet_and_an_agent_that_started_from_one_that_sits_there() {
    let launch = r#"claude --session-id abc --settings /tmp/data/tasks/repo/1/hook-agent.json --append-system-prompt-file "/tmp/data/tasks/repo/1/rules.md""#;
    // The shell's echo of the command, wrapped by the terminal and quoted its own way: still only the command.
    assert!(!drawn("user@host:~/repo$ claude --session-id abc --settings /tmp/data/tasks/repo/1/hook-agen\nt.json --append-system-prompt-file '/tmp/data/tasks/repo/\n1/rules.md'\n\n", launch));
    assert!(drawn(&format!("user@host:~/repo$ {launch}\n ▐▛███▜▌ Claude Code\n❯ \n"), launch));
    assert!(drawn("", launch), "nothing to compare: never held up");
    let before = json!({ "agent_status": "idle", "state_change_seq": 44, "terminal_id": "t1" });
    let mark = mark_of(Some(&before)).unwrap();
    let with = |o: Value| { let mut b = before.clone(); for (k, v) in o.as_object().unwrap() { b[k] = v.clone(); } b };
    assert_eq!(took(&mark, Some(&before), None), Some(false), "exactly as before: the text sits in its box");
    assert_eq!(took(&mark, Some(&with(json!({ "agent_status": "done" }))), None), Some(false));
    assert_eq!(took(&mark, Some(&with(json!({ "agent_status": "working" }))), None), Some(true));
    assert_eq!(took(&mark, Some(&with(json!({ "agent_status": "done", "state_change_seq": 46, "completion_seq": 46 }))), None), Some(true), "a turn too short to be seen working");
    assert_eq!(took(&mark, Some(&before), Some(true)), Some(true), "the words stand in its saved conversation");
    assert_eq!(took(&mark, Some(&with(json!({ "agent_status": "unknown" }))), None), Some(false), "herdr cannot classify it: its counters still say it did not move");
    let busy = with(json!({ "agent_status": "working" }));
    assert_eq!(took(&mark_of(Some(&busy)).unwrap(), Some(&busy), None), None, "typed into a running turn: its working says nothing");
    assert_eq!(took(&mark, Some(&with(json!({ "terminal_id": "t2" }))), None), None);
    assert_eq!(took(&mark, None, None), None);
}

#[test]
fn a_slow_starting_agent_gets_nothing_typed_until_it_has_drawn_its_screen() {
    let w = quiet();
    waits(60000, 3000);
    w.patch(|s| s["slowStart"] = json!(4)); // herdr says "ready" four looks before anything is on its screen
    let id = w.add("Slow start");
    delegate(&id);
    settle(&id);
    let order: Vec<String> = matching(&w, r"^(pane read|agent prompt|agent send-keys)").iter().map(|c| c.split(' ').take(2).collect::<Vec<_>>().join(" ")).collect();
    assert!(order.iter().position(|c| c == "agent prompt").unwrap_or(0) >= 5, "looked at its screen until it was drawn, then typed: {order:?}");
    assert_eq!(count(&w, r"^agent prompt"), 1);
    assert_eq!(count(&w, r"^agent send-keys"), 0, "no enter had to be pressed again");
    assert!(matching(&w, r"^agent prompt")[0].contains("Slow start"));
    assert!(!task(&id).b("sent") && task(&id).s("stage") != "failed");
}

#[test]
fn a_first_enter_swallowed_by_a_starting_agent_is_pressed_again_and_the_text_never_typed_twice() {
    let w = quiet();
    waits(0, 3000); // the wait for its screen gave up: the text goes in while it still starts
    w.patch(|s| s["slowStart"] = json!(99));
    let id = w.add("Swallowed");
    delegate(&id);
    drain();
    let a = the_agent(&w, &id);
    assert!(a.s("box").contains("Swallowed") && a.arr("prompts").is_empty() && a.s("status") == "idle", "typed, not submitted: {a}");
    // Moments after: too early to say, and the board does not call it work.
    pass(&w);
    assert_eq!(count(&w, r"^agent send-keys"), 0);
    assert_eq!((row(&id).t("state"), row(&id).t("note")), ("working".into(), "Starting".into()));
    // A while later it still sits at its prompt: enter, and only enter.
    sw::store::tasks::patch(&id, |t| { let at = t.at("sent").i("at") - 5000; t.obj_mut("sent").put("at", at); }).unwrap();
    pass(&w);
    assert_eq!(matching(&w, r"^agent send-keys").iter().map(|c| c.rsplit(' ').next().unwrap().to_string()).collect::<Vec<_>>(), vec!["enter"]);
    let a = the_agent(&w, &id);
    assert_eq!(a.arr("prompts").len(), 1);
    assert!(text(&a.arr("prompts")[0]).contains("Swallowed") && a.at("box").is_null());
    let done = settle(&id);
    pass(&w);
    assert_eq!((count(&w, r"^agent prompt"), count(&w, r"^agent send-keys")), (1, 1), "started: nothing is pressed again");
    assert!(!task(&id).b("sent") && done.s("stage") != "failed");
}

#[test]
fn a_later_message_whose_enter_is_lost_gets_it_again_and_pressing_enter_yourself_clears_not_started() {
    let w = quiet();
    waits(60000, 0);
    w.script(json!({ "worker": [{ "status": "idle", "note": null }, { "status": "working" }, { "status": "working" }] }));
    let id = w.add("Later");
    delegate(&id);
    settle(&id);
    w.patch(|s| s["swallow"] = json!(1));
    tell(&id, "one more thing");
    assert!(the_agent(&w, &id).s("box").contains("one more thing") && task(&id).b("sent"));
    pass(&w);
    assert_eq!((said_times(&w, &id, "one more thing"), the_agent(&w, &id).t("status")), (1, "working".into()));
    pass(&w);
    assert_eq!(count(&w, r"^agent send-keys .* enter$"), 1);
    assert!(!task(&id).b("sent"));
    // The same for a message nothing gets through: not started, until you press enter in its terminal yourself.
    w.patch(|s| { s["swallow"] = json!(1); s["deaf"] = json!(99); });
    first_agent(&w, |a| a["status"] = json!("idle"));
    tell(&id, "and another");
    for _ in 0..=(TRIES + 1) { pass(&w); }
    assert_eq!((task(&id).t("stage"), row(&id).t("note")), ("failed".into(), NOT_STARTED.to_string()));
    w.patch(|s| s["deaf"] = json!(0));
    use sw::herdr::cli::Fake;
    w.fake.run(&["agent".into(), "send-keys".into(), task(&id).at("link").t("pane"), "enter".into()], ""); // you, pressing enter in the agent's own terminal
    settle(&id);
    assert_eq!(said_times(&w, &id, "and another"), 1);
    let t = task(&id);
    assert_eq!((t.t("stage"), t.at("unsent").is_null(), t.at("failure").is_null()), ("working".into(), true, true));
}

#[test]
fn two_messages_left_in_the_same_box_are_both_handed_over_again_on_retry() {
    let w = quiet();
    waits(60000, 0);
    w.script(json!({ "worker": [{ "status": "idle", "note": null }, { "status": "working" }] }));
    let id = w.add("Two unsent");
    delegate(&id);
    settle(&id);
    w.patch(|s| { s["swallow"] = json!(2); s["deaf"] = json!(99); s["pids"] = json!(true); });
    tell(&id, "FIRST UNSENT MESSAGE");
    tell(&id, "SECOND UNSENT MESSAGE"); // typed on top of the first: one box, one message now
    let both = regex::Regex::new(r"FIRST UNSENT MESSAGE[\s\S]*SECOND UNSENT MESSAGE").unwrap();
    assert!(both.is_match(task(&id).at("sent").s("text")));
    for _ in 0..=(TRIES + 1) { pass(&w); }
    assert_eq!(task(&id).s("stage"), "failed");
    assert!(both.is_match(task(&id).at("unsent").s("text")));
    // Told not started, you say a third thing: it joins the two in the box, and none is forgotten.
    w.patch(|s| s["swallow"] = json!(1));
    tell(&id, "THIRD");
    for _ in 0..=(TRIES + 1) { pass(&w); }
    let all = regex::Regex::new(r"FIRST UNSENT MESSAGE[\s\S]*SECOND UNSENT MESSAGE[\s\S]*THIRD").unwrap();
    assert!(all.is_match(task(&id).at("unsent").s("text")), "{}", task(&id).at("unsent"));
    w.patch(|s| s["deaf"] = json!(0));
    sw::actions::finish_line::retry(&id).unwrap();
    settle(&id);
    let again = the_agent(&w, &id).arr("prompts").to_vec();
    assert_eq!(again.len(), 1);
    assert!(all.is_match(&text(&again[0])));
}

#[test]
fn an_agent_herdr_cannot_classify_is_checked_like_any_other() {
    let w = quiet();
    waits(60000, 0);
    w.script(json!({ "worker": [{ "status": "unknown", "note": null }, { "status": "working" }] }));
    let id = w.add("Unknown");
    delegate(&id);
    settle(&id);
    assert_eq!(the_agent(&w, &id).s("status"), "unknown");
    w.patch(|s| s["swallow"] = json!(1));
    tell(&id, "FOLLOW UP");
    assert!(task(&id).b("sent"));
    for _ in 0..2 { pass(&w); settle(&id); }
    assert_eq!(count(&w, r"^agent send-keys .* enter$"), 1);
    assert_eq!(said_times(&w, &id, "FOLLOW UP"), 1);
    assert!(the_agent(&w, &id).at("box").is_null() && !task(&id).b("sent"));
    // And when no enter gets through: not started, with the text for retry.
    w.patch(|s| { s["swallow"] = json!(1); s["deaf"] = json!(99); });
    first_agent(&w, |a| a["status"] = json!("unknown"));
    tell(&id, "NEVER TAKEN");
    for _ in 0..(TRIES + 3) { pass(&w); settle(&id); }
    assert_eq!((task(&id).t("stage"), row(&id).t("note")), ("failed".into(), NOT_STARTED.to_string()));
    assert!(task(&id).at("unsent").s("text").contains("NEVER TAKEN"));
}
