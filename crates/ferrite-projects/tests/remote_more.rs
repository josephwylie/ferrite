//! Other machines' work settings: what the asking machine says travels with the task it starts there, its panel reads
//! the task's settings with their places, a row is changed where the task lives, and carried settings outlast the
//! agent's start (a task that waited for a slot starts on the agent its setting names then).
use crate::common;
use common::*;
use serde_json::{json, Value};
use sw::json::*;
use sw::store::settings::lookup::{setting, Scope};

/// The door, as `herdr-projects remote <what> <request>` answers it on this machine (in this process).
fn door(what: &str, req: Value) -> Value { sw::remote::door(what, &req).unwrap_or_else(|e| json!({ "ok": false, "msg": e.msg })) }

#[test]
fn the_asking_machines_settings_arrive_with_the_task_it_starts_and_a_row_is_changed_where_the_task_lives() {
    let w = world_with(Opts { repo: true, settings: json!({ "review.rounds": 1 }), ..Default::default() });
    let t = w.add("Started from afar");
    w.script(json!({ "worker": [{ "status": "working" }] }));
    // delegate, with what the asking machine says for this repo: it stands where this machine's own "you" would
    let sent = json!({ "review.rounds": 5, "worker.agent": "codex", "merge.by": "itself" });
    assert_eq!(door("do", json!({ "action": "delegate", "id": t, "settings": sent, "from": "laptop" })).at("ok"), &json!(true));
    assert_eq!(task(&t).at("carried"), &json!({ "from": "laptop", "settings": sent }));
    assert_eq!(task(&t).s("agent"), "codex");
    assert_eq!(setting("review.rounds", Some(&task(&t))), json!(5));
    assert_eq!(setting("review.rounds", None), json!(1), "this machine's own are untouched");
    // the list carries each setting with its place, for the asking side's task panel
    let list = door("list", json!({}));
    let row = list.arr("tasks").iter().find(|x| x.s("id") == t).unwrap().at("snap").arr("settings").iter().find(|r| r.s("name") == "review.rounds").cloned().unwrap();
    assert_eq!((row.at("value"), row.s("from"), row.at("own")), (&json!(5), "sent from laptop", &json!(false)));
    // a row changed, and put back, from the asking side's settings page
    assert_eq!(door("do", json!({ "action": "set", "id": t, "name": "review.rounds", "value": "2" })), json!({ "ok": true, "msg": "review.rounds: 2, for this task only" }));
    assert_eq!(door("do", json!({ "action": "set", "id": t, "name": "review.rounds" })), json!({ "ok": true, "msg": "review.rounds: 5" }));
    // what is sent is checked like anything else: a wrong value is refused, and nothing starts
    let u = w.add("Refused");
    let no = door("do", json!({ "action": "delegate", "id": u, "settings": { "merge.by": "sometimes" }, "from": "laptop" }));
    assert_eq!(no.at("ok"), &json!(false));
    assert!(no.s("msg").contains("merge.by must be you, itself"), "{no}");
    assert_eq!(task(&u).s("stage"), "todo");
    assert!(task(&u).at("carried").is_null());
}

#[test]
fn carried_settings_outlast_the_agents_start_and_a_queued_task_starts_on_the_agent_its_setting_names_then() {
    let w = world_with(Opts { repo: true, settings: json!({ "review.rounds": 7, "merge.by": "itself", "board.agents": 1 }), ..Default::default() });
    let first = w.add("Carried");
    sw::store::tasks::patch(&first, |t| { t.put("carried", json!({ "from": "laptop", "settings": { "review.rounds": 0, "merge.by": "you" } })); }).unwrap();
    w.script(json!({ "worker": [{ "status": "working" }, { "status": "working" }] }));
    delegate(&first); settle(&first);
    // the first prompt went in and is tracked on the task: what was carried is still what counts, not this machine's own
    let t = task(&first);
    assert_eq!(t.s("stage"), "working");
    assert_eq!(t.at("carried"), &json!({ "from": "laptop", "settings": { "review.rounds": 0, "merge.by": "you" } }));
    assert_eq!((setting("review.rounds", Some(&t)), setting("merge.by", Some(&t))), (json!(0), json!("you")));
    sw::store::tasks::patch(&first, |t| { t.put("sent", json!({ "at": sw::clock::now(), "tries": 1, "text": "x" })); }).unwrap();
    assert_eq!(setting("review.rounds", Some(&task(&first))), json!(0), "the prompt's own record is another field");
    sw::store::tasks::patch(&first, |t| { t.put("sent", Value::Null); }).unwrap();
    assert_eq!((setting("review.rounds", Some(&task(&first))), setting("merge.by", Some(&task(&first)))), (json!(0), json!("you")));
    // a task of this machine reads this machine's own, whatever is tracked of its prompt
    let second = w.add("Waits its turn");
    delegate(&second);
    let t2 = task(&second);
    assert_eq!((t2.s("stage"), t2.s("agent")), ("queued", "claude"));
    assert_eq!(setting("review.rounds", Some(&t2)), json!(7));
    // while it waits, its agent is changed: it starts on that one, with its model
    sw::store::settings::change::set_value("worker.agent", &json!("codex/gpt-5.5"), &Scope::task(&t2)).unwrap();
    sw::store::tasks::patch(&first, |t| { t.put("stage", "stopped"); }).unwrap();
    sw::actions::start::start_queued(&w.socket);
    settle(&second);
    let t2 = task(&second);
    let agent = w.pane_of(&t2).at("agent").clone();
    assert_eq!((t2.s("agent"), agent.s("kind")), ("codex", "codex"));
    let args: Vec<String> = agent.arr("args").iter().map(text).collect();
    let m = args.iter().position(|a| a == "-m").unwrap_or_else(|| panic!("no -m: {args:?}"));
    assert_eq!(args[m + 1], "gpt-5.5");
}
