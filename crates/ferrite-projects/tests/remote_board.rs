//! A machine that did not answer, on both boards: never shown as an empty one. Its line with why where its tasks would
//! be, its own board with the reason whole (read with the arrows on a small screen), nothing of it as a message, and
//! an action on a task of a machine that stopped answering says why. The web board says the same.
use crate::common;
use crate::far;
use common::board::open; // Ferrite: not `web_as` (ADR 0031)
use common::*;
use far::{theirs, Far};
use serde_json::{json, Value};
use sw::board::update::{update, Event};
use sw::json::*;
use sw::remote::why::{why_not, Ended};

fn end(code: i32, stderr: &str) -> Ended { Ended { code: Some(code), stderr: stderr.into(), ..Default::default() } }
fn has(text: &str, re: &str) -> bool { regex::Regex::new(re).unwrap().is_match(text) }
/// The list's words with the side panel cut off and the wrapped lines joined.
fn flat(lines: &[String]) -> String { lines.iter().map(|l| l.split('│').next().unwrap_or("").trim().to_string()).collect::<Vec<_>>().join(" ") }
const BASH: &str = "bash: line 1: herdr-projects: command not found\n";
fn answer(b: &mut common::board::Board, label: &str, a: Result<Value, Value>) { update(&mut b.s, Event::Remote { label: label.into(), answer: a }); }
fn list(tasks: Value) -> Result<Value, Value> { Ok(json!({ "host": "x", "version": "0.2.0", "tasks": tasks })) }
fn bash_why(label: &str) -> Value { why_not(&end(127, BASH), label, &format!("me@{label}")) }

#[test]
fn the_board_never_shows_a_machine_that_did_not_answer_as_an_empty_one() {
    let w = world_with(Opts { repo: true, settings: json!({ "board.machines": "vm-a, vm-b, vm-c" }), ..Default::default() });
    let far = Far::new(&w.base);
    w.add("Mine");
    let mut s = open(&w.root, 34);
    update(&mut s.s, Event::Machines(sw::remote::machines()));
    answer(&mut s, "vm-a", Err(bash_why("vm-a")));
    answer(&mut s, "vm-b", list(json!([theirs(1)])));
    answer(&mut s, "vm-c", Ok(json!({ "ok": false, "msg": "Unknown request list" })));
    s.press(&["alt+m"]); // every machine: the others' tasks show, and each one that could not be read has its line
    assert_eq!(s.s.scope, "*");
    let text = s.text();
    assert!(text.contains("Mine") && text.contains("Theirs 1"), "{text}");
    let all = flat(&s.screen(120));
    assert!(has(&all, r"vm-a: its tasks cannot be read: herdr-projects is not found there\. +More: \S.*?\."), "{all}");
    assert!(all.contains("vm-c: its tasks cannot be read: it answered, but not with its tasks."), "{all}");
    assert!(!text.contains("must be on the PATH"));
    // that machine's own board: no "nothing here yet", the reason whole
    s.press(&["alt+m"]); assert_eq!(s.s.scope, "vm-a");
    let all = flat(&s.screen(120));
    assert!(!all.contains("Nothing here yet") && all.contains("vm-a: its tasks cannot be read: herdr-projects is not found there."), "{all}");
    assert!(all.contains("must be on the PATH that a login shell gets") && all.contains(r#"ln -s "$(command -v herdr-projects)""#), "{all}");
    assert!(has(&all, r"What came back:\s+bash: line 1: herdr-projects: command not found"), "{all}");
    // a small screen: what does not fit is reached with the arrows, and a line says that there is more
    s.rows = 24; s.s.side = false;
    let read_down = |s: &mut common::board::Board| { let mut seen: Vec<String> = vec![]; for _ in 0..20 { let now = s.screen(60).join("\n"); if seen.last() == Some(&now) { break; } seen.push(now); s.press(&["down"]); } seen };
    let seen = read_down(&mut s);
    assert!(seen.len() > 1);
    assert!(seen[0].contains("↓ more below (") && !seen[0].contains("↑ more above"), "{}", seen[0]);
    assert!(seen.last().unwrap().contains("↑ more above (") && !seen.last().unwrap().contains("↓ more below"), "{}", seen.last().unwrap());
    let read: String = seen.join("\n").split_whitespace().collect::<Vec<_>>().join(" ");
    for part in [r"or link the program into a folder that is on it already:", r#"ln -s "\$\(command -v herdr-projects\)" ~/\.local/bin/"#, r#"Test it from this machine: ssh \S+@vm-a 'exec \$SHELL -lc "herdr-projects remote list"' \| tail -c 200"#] { assert!(has(&read, part), "{part}: {read}"); }
    // words on their way to that machine have rows of their own: the arrows still read all of it, then reach the row
    for _ in 0..20 { s.press(&["up"]); }
    let job = sw::store::inbox::new_dispatch("@vm-a fix the build", obj(json!({ "remote": "vm-a" }))).unwrap();
    sw::store::inbox::set_dispatch(job.s("id"), obj(json!({ "state": "error", "msg": "vm-a did not answer" })));
    s.load();
    let with_job = read_down(&mut s);
    assert!(with_job.len() > 2, "{with_job:?}");
    assert!(has(&with_job.join("\n").split_whitespace().collect::<Vec<_>>().join(" "), r#"(?s)ln -s "\$\(command -v herdr-projects\)" ~/\.local/bin/.*Test it from this machine: ssh"#));
    assert_eq!(s.s.sel, job.s("id"));
    s.press(&["up"]); assert!(s.s.unread_top < s.s.unread_max, "{} {}", s.s.unread_top, s.s.unread_max);
    sw::store::inbox::clear_dispatch(job.s("id")); s.load();
    for _ in 0..20 { s.press(&["up"]); }
    let small = s.screen(60).join("\n");
    assert!(small.contains("vm-a: its tasks cannot be read") && !small.contains("↑ more above"), "{small}");
    s.rows = 34; s.s.side = true;
    assert!(s.screen(60).join("\n").contains("vm-a: its tasks cannot be read"));
    for l in s.screen(60) { assert!(sw::text::width(&l) <= 60, "{l}"); }
    // nothing of it is a message, however often the machine is asked again
    let msg = s.s.msg.clone();
    answer(&mut s, "vm-a", Err(bash_why("vm-a"))); answer(&mut s, "vm-a", Err(bash_why("vm-a")));
    assert_eq!(s.s.msg, msg); assert!(!has(&msg, "cannot be read|not found"), "{msg}");
    // a machine that answers with no tasks is an empty one, and one that answers again loses its line
    s.press(&["alt+m"]); assert_eq!(s.s.scope, "vm-b"); s.press(&["alt+m"]); assert_eq!(s.s.scope, "vm-c");
    for m in ["vm-a", "vm-b", "vm-c"] { answer(&mut s, m, list(json!([]))); }
    assert!(s.text().contains("Nothing here yet") && !s.text().contains("cannot be read"), "{}", s.text());
    // before a machine's first answer it is being asked, not empty and not failed
    { let m = s.s.remotes.get_mut("vm-c").unwrap(); m.ok = false; m.why = None; }
    s.load();
    assert!(s.text().contains("vm-c: asking for its tasks…") && !s.text().contains("Nothing here yet"), "{}", s.text());
    // an action on a task of a machine that stopped answering says why, once
    s.press(&["alt+m", "alt+m"]);
    answer(&mut s, "vm-b", list(json!([theirs(1)])));
    s.s.sel = "vm-b:far/1".into(); s.load();
    far.ends("vm-b", "do", "", "ssh: connect to host vm-b port 22: Connection refused\n", 255);
    let pin = sw::board::keys::by_name("pin").unwrap().keys[0];
    s.press(&[pin]);
    assert_eq!(s.s.msg, "vm-b did not answer: cannot connect (it refused the connection)");
    assert_eq!(far.asked().last().map(|a| (a.0.clone(), a.1.clone(), a.2.s("action").to_string())), Some(("vm-b".into(), "do".into(), "pin".into())));
}

// Ferrite: the_web_board_says_the_same_an_unread_machine_is_a_line_with_its_reason_behind_it not taken: it is the web board's (crates/web is not taken: ADR 0031).
