//! The limits' two settings (limit.time, limit.tokens): what a value may be, the nearest right one for a refused one,
//! the lookup, never the repo's own file, attempts counted as one task; and the token counters of each program.
use crate::common;
use common::*;
use serde_json::{json, Value};
use std::path::Path;
use sw::json::*;
use sw::store::settings::{change::{checked, set_value}, lookup::{lookup, setting, Scope}, places::yours, values};

fn said(argv: &[&str]) -> sw::err::Res<String> { let a = sw::cli::flags::parse(&argv.iter().map(|s| s.to_string()).collect::<Vec<_>>()); sw::cli::tasks::command(&a.pos[0], &a).map(|(_, s)| s) }
fn re(r: &str, s: &str) -> bool { regex::Regex::new(r).unwrap().is_match(s) }
fn refused(r: sw::err::Res<impl std::fmt::Debug>) -> String { r.map(|v| format!("not refused: {v:?}")).unwrap_or_else(|e| e.msg) }
fn line(file: &Path, o: Value) { use std::io::Write; std::fs::OpenOptions::new().create(true).append(true).open(file).unwrap().write_all(format!("{o}\n").as_bytes()).unwrap(); }

#[test]
fn the_two_settings_what_a_value_may_be_the_nearest_right_one_the_lookup_and_never_the_repos_own_file() {
    let w = world_with(Opts { repo: true, repo_settings: Some(json!({ "limit": { "time": "1m", "tokens": "1k" }, "check": { "timeout": "5m" } })), ..Default::default() });
    let id = w.add("One");
    let at = |name: &str| { let r = lookup(name, &Scope::task(&task(&id)), &yours()).unwrap(); (text(&r.value), r.from) };
    // built in: none. The repo's own file says both: refused, so code an agent writes cannot set or lift a limit.
    assert_eq!((at("limit.time"), at("limit.tokens")), (("none".into(), "built in".into()), ("none".into(), "built in".into())));
    let (_, why) = sw::store::settings::places::repo_file(&w.root, None);
    assert!(re(r"limit\.time is refused: the repo's file may only hold", &why.join("\n")), "{why:?}");
    assert!(!sw::store::settings_list::repo_facts().iter().any(|n| n.starts_with("limit.")));
    // what it may be, and how it is kept
    for (name, typed, kept) in [("limit.time", "2h", "2h"), ("limit.time", " 45M ", "45m"), ("limit.time", "None", "none"), ("limit.tokens", "2m", "2m"), ("limit.tokens", "500K", "500k"), ("limit.tokens", "2000000", "2m"), ("limit.tokens", "1.5m", "1.5m"), ("limit.tokens", "none", "none")] {
        assert_eq!(values::check(name, &json!(typed), "").map(|v| text(&v)).unwrap_or_else(|e| e.msg), kept, "{name} {typed}");
    }
    assert_eq!((values::ms("2h"), values::ms("none"), values::tokens_of("2m"), values::tokens_of("500k"), values::tokens_of("none"), values::tokens_say(1234567.0), values::tokens_say(950.0)), (7200000, 0, 2000000, 500000, 0, "1.23m".into(), "950".into()));
    // a refused value changes nothing and names the nearest right one
    let you = Scope::you();
    assert!(re(r#"limit\.time is a time with a unit, such as 2h \(s, m or h\), or none, not "2 hours"\. Did you mean 2h\?"#, &refused(set_value("limit.time", &json!("2 hours"), &you).map(|f| f.value))));
    assert!(refused(set_value("limit.time", &json!("90"), &you).map(|f| f.value)).contains("Did you mean 90m?"));
    assert!(re(r#"limit\.tokens is a count of tokens, such as 500k or 2m, or none, not "2 million"\. Did you mean 2m\?"#, &refused(set_value("limit.tokens", &json!("2 million"), &you).map(|f| f.value))));
    assert!(refused(set_value("limit.tokens", &json!("0"), &you).map(|f| f.value)).contains("Did you mean none?"));
    assert!(refused(set_value("limit.token", &json!("2m"), &you).map(|f| f.value)).contains("Unknown setting \"limit.token\". Did you mean limit.tokens?"));
    assert_eq!((setting("limit.time", None), setting("limit.tokens", None)), (json!("none"), json!("none")));
    // nearest answer first: you, your entry for the repo, the task
    set_value("limit.time", &json!("4h"), &you).unwrap(); assert_eq!(at("limit.time"), ("4h".into(), "you".into()));
    set_value("limit.time", &json!("3h"), &Scope::repo("proj")).unwrap(); assert_eq!(at("limit.time"), ("3h".into(), "your entry for proj".into()));
    let s = said(&["set", "limit.time", "2h", "--task", &id]).unwrap();
    assert!(s.contains("limit.time for proj/1: 2h  (this task)"), "{s}");
    assert_eq!(at("limit.time"), ("2h".into(), "this task".into()));
    let shown = said(&["show", &id]).unwrap();
    assert!(re(r"\n\* limit\.time +2h +this task\n {2}limit\.tokens +none +built in\n", &shown), "{shown}");
    assert!(said(&["unset", "limit.time", "--task", &id]).unwrap().contains("3h  (your entry for proj)"));
    // an option of delegate, for that one task; a refused one changes nothing on it
    w.script(json!({ "worker": [{ "status": "working" }] }));
    let e = refused(sw::actions::start::delegate(&id, vec![("limit.tokens".into(), json!("lots")), ("limit.time".into(), json!("1h"))]).map(|t| t.t("id")));
    assert!(e.contains("limit.tokens is a count of tokens"), "{e}");
    assert!(task(&id).at("settings").as_object().map(|o| o.is_empty()).unwrap_or(true));
    said(&["delegate", &id, "--limit.time", "1h", "--limit.tokens", "500k"]).unwrap();
    assert_eq!(task(&id).at("settings"), &json!({ "limit.time": "1h", "limit.tokens": "500k" }));
    // and of add, with or without --delegate
    let root = w.root_str();
    said(&["add", "With limits", "--repo", &root, "--limit.time", "45m", "--limit.tokens", "500k"]).unwrap();
    assert_eq!(task("proj/2").at("settings"), &json!({ "limit.time": "45m", "limit.tokens": "500k" }));
    assert!(refused(checked(&[("limit.time".into(), json!("soon"))], &Scope::you())).contains("limit.time is a time"));
    assert!(said(&["add", "Refused", "--repo", &root, "--limit.time", "soon"]).is_err());
    assert_eq!(sw::store::tasks::all().len(), 2);
    // held by an automation, for every task it makes
    let a = sw::automations::list::add(sw::automations::list::Add { name: "Nightly", prompt: "Look.", trigger: Some(sw::automations::triggers::parse_schedule("daily 09:00").unwrap()), repo: "", folder: &w.root, settings: Obj::new(), research: None }).unwrap();
    let s = said(&["set", "limit.tokens", "2m", "--automation", "Nightly"]).unwrap();
    assert!(s.contains("limit.tokens for \"Nightly\": 2m  (this automation)"), "{s}");
    assert_eq!(sw::automations::list::find(a.s("id")).unwrap().at("settings").s("limit.tokens"), "2m");
    // (The old test took Claude Code's token counter away to see `show` say the limit does nothing for it: the
    // programs' table is fixed in Rust, so only the ordinary line is checked.)
    assert!(re(r"limit\.tokens +500k +this task\n", &said(&["show", &id]).unwrap()));
    // A task done several times at once is one task to its limit: an attempt reads its task's limit, and the
    // attempts' tokens count together, whichever of them is asked.
    let top = w.add("Thrice");
    set_value("limit.tokens", &json!("500k"), &Scope::task(&task(&top))).unwrap();
    let a1 = sw::store::attempts::create_attempt(&task(&top), "A", "claude", Obj::new()).unwrap().t("id");
    let a2 = sw::store::attempts::create_attempt(&task(&top), "B", "codex", Obj::new()).unwrap().t("id");
    let put = |id: &str, k: &str, v: Value| { sw::store::tasks::patch(id, |t| { t.put(k, v); }).unwrap(); };
    put(&top, "attempts", json!([{ "id": a1, "letter": "A" }, { "id": a2, "letter": "B" }]));
    put(&a1, "spent", json!({ "total": 300000 })); put(&a2, "spent", json!({ "total": 250000 }));
    assert!(sw::limits::run::limit_said(&task(&a1)));
    assert_eq!((sw::limits::run::used_of(&task(&a1)), sw::limits::run::used_of(&task(&top))), (550000.0, 550000.0));
    assert_eq!(sw::limits::reached::over_now(&task(&a2)), Some(("500k".to_string(), 550000.0)));
    // The chosen attempt's conversation is the task's from then on: read onto the task as well, it still counts once.
    put(&a1, "spent", json!({ "files": { "/c/a.jsonl": { "at": 9, "n": 300000 } }, "total": 300000 }));
    put(&top, "spent", json!({ "files": { "/c/a.jsonl": { "at": 12, "n": 310000 }, "/c/review.jsonl": { "at": 5, "n": 40000 } }, "total": 350000 }));
    assert_eq!((sw::limits::run::used_of(&task(&top)), sw::limits::run::used_of(&task(&a1))), ((310000 + 40000 + 250000) as f64, 600000.0));
    // the Coordinator is told in plain words
    let rules = sw::coordinator::rules::rules("board");
    assert!(re(r"limit\.time[^\n]*limit\.tokens", &rules));
    assert!(rules.contains("\"never more than two hours on this one\" is board set limit.time 2h --task <id>"));
}

#[test]
fn tokens_are_counted_from_where_the_last_count_stopped_each_program_its_own_way() {
    use sw::agents::read_tokens::{claude_tokens, codex_tokens, pi_tokens};
    let base = std::env::temp_dir().join(format!("sw-limits-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base); std::fs::create_dir_all(&base).unwrap();
    let f = base.join("c.jsonl");
    let u = |id: &str, o: i64| line(&f, json!({ "type": "assistant", "message": { "id": id, "usage": { "input_tokens": 5, "cache_creation_input_tokens": 100, "cache_read_input_tokens": 1000, "output_tokens": o } } }));
    // Claude Code: one line per part of a reply, each with the whole reply's use; a reply counts once
    u("m1", 10); u("m1", 10); u("m1", 40);
    let s = claude_tokens(&f, &Value::Null).unwrap(); assert_eq!(s.f("n"), 1145.0);
    assert_eq!(claude_tokens(&f, &s).unwrap(), s, "nothing new: the same answer");
    u("m1", 60); u("m2", 20);
    { use std::io::Write; std::fs::OpenOptions::new().append(true).open(&f).unwrap().write_all(br#"{"type":"assistant","message":{"id":"m3","usage":{"input_tokens":9"#).unwrap(); } // a line still being written waits
    let s = claude_tokens(&f, &s).unwrap(); assert_eq!(s.f("n"), (1145 + 20 + 1125) as f64);
    { use std::io::Write; std::fs::OpenOptions::new().append(true).open(&f).unwrap().write_all(b",\"output_tokens\":1}}}\n").unwrap(); }
    let s = claude_tokens(&f, &s).unwrap(); assert_eq!(s.f("n"), 2300.0);
    std::fs::write(&f, "").unwrap(); u("n1", 1);
    assert_eq!(claude_tokens(&f, &s).unwrap().f("n"), 1106.0, "a shorter file is another file");
    // Codex: the conversation's running total, what came from the cache included; a total that began again is added
    let c = base.join("x.jsonl");
    let total = |i: i64, cached: i64, o: i64| line(&c, json!({ "type": "event_msg", "payload": { "type": "token_count", "info": { "total_token_usage": { "input_tokens": i, "cached_input_tokens": cached, "output_tokens": o } } } }));
    total(1000, 400, 50); total(3000, 2000, 150);
    let x = codex_tokens(&c, &Value::Null).unwrap(); assert_eq!(x.f("n"), 3150.0);
    total(500, 0, 10);
    assert_eq!(codex_tokens(&c, &x).unwrap().f("n"), 3660.0);
    // pi: each reply once
    let p = base.join("p.jsonl");
    line(&p, json!({ "type": "message", "message": { "role": "assistant", "usage": { "input": 100, "output": 7, "cacheRead": 9000, "cacheWrite": 20 } } }));
    line(&p, json!({ "type": "message", "message": { "role": "user" } }));
    assert_eq!(pi_tokens(&p, &Value::Null).unwrap().f("n"), 9127.0);
    let _ = std::fs::remove_dir_all(&base);
}
