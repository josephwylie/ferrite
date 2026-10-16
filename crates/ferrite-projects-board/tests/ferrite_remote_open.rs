//! Ferrite: upstream's tests/remote_open.rs (herdr-projects 1.0, MIT) without its world. Enter on another machine's
//! task: that machine reopens the task's space and puts its agent in front, and the board stays and says to go there
//! from the nav (upstream: herdr's sidebar). Every agent runs in a Ferrite window, so the board always asks for that
//! (`in_herdr`): upstream's case outside a herdr client is not Ferrite's. A machine on an older plugin answers
//! without `focused`, and the job hands over a terminal as upstream's does (the window says it cannot). Also that
//! machine's side of it: its door focuses the pane only when asked to.
//!
//! The other machine is a stand-in for ssh (upstream's `HERDR_PROJECTS_SSH`): a script that writes down what it was
//! asked and answers what the test put in its file.
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde_json::{json, Value};
use sw_board::board::data::Data;
use sw_board::board::job;
use sw_board::board::keys::Press;
use sw_board::board::state::{Ctx, State};
use sw_board::board::update::{list::relist, update, Effect, Event};
use sw_board::herdr::cli::{self, Fake, Ran};
use sw_board::json::*;
use sw_board::remote::Machine;

const STAND_IN: &str = "#!/bin/sh\nprintf '%s\\n' \"$2\" >> \"$(dirname \"$0\")/asked\"\ncat \"$(dirname \"$0\")/answer\"\n";

/// The other machine: what it answers next, and what it was asked.
struct Far {
    dir: PathBuf,
}

impl Far {
    fn new(home: &Path) -> Far {
        use std::os::unix::fs::PermissionsExt;
        let dir = home.join("far");
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("ssh");
        std::fs::write(&script, STAND_IN).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        unsafe { std::env::set_var("HERDR_PROJECTS_SSH", &script) };
        Far { dir }
    }

    fn answers(&self, answer: Value) {
        std::fs::write(self.dir.join("answer"), format!("{answer}\n")).unwrap();
    }

    /// The last request it was asked to do: `herdr-projects remote do <base64 JSON>`.
    fn last_do(&self) -> Value {
        let asked = std::fs::read_to_string(self.dir.join("asked")).unwrap_or_default();
        let line = asked.lines().rfind(|l| l.contains("remote do ")).unwrap_or_default();
        let arg = line.rsplit(' ').next().unwrap_or_default();
        sw_board::util::unbase64(arg).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null)
    }
}

fn theirs() -> Value {
    let now = sw_board::clock::now();
    json!({ "id": "far/1", "title": "Theirs", "stage": "working", "state": "working", "agent": "claude", "app": "claude", "msg": "", "note": "", "at": now, "created": now, "link": { "pane": "w1:p1" }, "snap": { "folder": "~/far" } })
}

/// A board that sees vm-a's tasks, in a Ferrite window, on its one task.
fn board_on_theirs() -> State {
    let data = Data { repo: "proj".into(), session: "default".into(), loaded: true, ..Default::default() };
    let mut s = State::new(Ctx { in_herdr: true, ..Ctx::default() }, data);
    let vm_a = Machine { label: "vm-a".into(), target: "me@vm-a".into(), ..Default::default() };
    update(&mut s, Event::Machines(vec![vm_a]));
    update(&mut s, Event::Remote { label: "vm-a".into(), answer: Ok(json!({ "host": "vm-a", "version": "1.0.1", "tasks": [theirs()] })) });
    update(&mut s, Event::Key(Press::chord("alt+m")));
    s.sel = "vm-a:far/1".into();
    relist(&mut s);
    s
}

/// Enter, with what it started run at once: the terminals it would hand over (an attach's last word).
fn enter(s: &mut State) -> Vec<String> {
    let mut went = vec![];
    let mut fx = update(s, Event::Key(Press::chord("enter")));
    while let Some(e) = fx.pop() {
        match e {
            Effect::Run(j) => fx.extend(update(s, Event::Done(job::run(j)))),
            Effect::Attach { args, .. } => went.push(args.last().cloned().unwrap_or_default()),
            _ => {}
        }
    }
    went
}

/// herdr as the adapter answers it on vm-a: the task's pane, its agent there, and every call it was asked.
#[derive(Default)]
struct Herdr {
    calls: RefCell<Vec<String>>,
}

impl Fake for Herdr {
    fn run(&self, args: &[String], _input: &str) -> Ran {
        self.calls.borrow_mut().push(args.join(" "));
        let words: Vec<&str> = args.iter().map(String::as_str).take(2).collect();
        let result = match words.as_slice() {
            ["pane", "get"] => json!({ "pane": { "pane_id": "w1:p1", "workspace_id": "w1", "tab_id": "w1:t1" } }),
            ["agent", "get"] => json!({ "agent": { "pane_id": "w1:p1", "status": "idle" } }),
            _ => json!({}),
        };
        Ran { status: 0, stdout: json!({ "result": result }).to_string(), stderr: String::new() }
    }
}

// One test in this program: nothing else reads the environment meanwhile.
#[test]
fn enter_on_another_machines_agent_has_it_put_in_front_there_and_its_door_focuses_only_when_asked() {
    let home = std::env::temp_dir().join(format!("ferrite-remote-open-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let socket = home.join("herdr.sock").display().to_string();
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("HERDR_PROJECTS_HOME", home.join("sw"));
        std::env::set_var("HERDR_SOCKET_PATH", &socket);
    }
    let far = Far::new(&home);

    // vm-a is asked to put it in front; the board stays where it is and says where it is.
    far.answers(json!({ "ok": true, "msg": "", "pane": "w3:p2", "workspace": "w3", "hasAgent": true, "focused": true }));
    let mut s = board_on_theirs();
    assert!(enter(&mut s).is_empty(), "no terminal is handed over");
    let asked = far.last_do();
    assert_eq!((asked.s("action"), asked.s("id"), asked.at("focus")), ("reopen", "far/1", &json!(true)));
    assert_eq!(s.msg, "vm-a: its agent is in front there. Pick vm-a in the nav to go to it: “Theirs”; local there brings you back here.");
    assert_eq!(s.sel, "vm-a:far/1");
    // Its agent had exited and could not be restarted: its bare terminal is what is in front.
    far.answers(json!({ "ok": true, "msg": "", "pane": "w3:p2", "workspace": "w3", "hasAgent": false, "focused": true }));
    assert!(enter(&mut s).is_empty());
    assert!(s.msg.starts_with("vm-a: its terminal (its agent has exited) is in front there."), "{}", s.msg);
    // vm-a refused: why is said, and nothing else happens.
    far.answers(json!({ "ok": false, "msg": "vm-a's herdr did not go to it: no such pane" }));
    assert!(enter(&mut s).is_empty());
    assert_eq!(s.msg, "vm-a's herdr did not go to it: no such pane");
    // A machine on an older plugin does not know `focus`: the job hands this terminal over, as upstream's does.
    far.answers(json!({ "ok": true, "msg": "", "pane": "w3:p2", "workspace": "w3", "hasAgent": true }));
    let went = enter(&mut s);
    assert_eq!(went.len(), 1);
    assert!(went[0].ends_with("herdr agent attach w3:p2"), "{went:?}");

    // vm-a's side: its door reopens the task, and puts its agent in front only when asked to.
    let task = json!({ "id": "proj/1", "repo": "proj", "title": "Add a greeting", "link": { "pane": "w1:p1", "socket": socket } });
    sw_board::store::tasks::save(task.as_object().unwrap()).unwrap();
    let herdr = Rc::new(Herdr::default());
    cli::set_fake(Some(herdr.clone()));
    let focused = || herdr.calls.borrow().iter().filter(|c| c.starts_with("agent focus")).count();
    let a = sw_board::remote::door("do", &json!({ "action": "reopen", "id": "proj/1" })).unwrap();
    assert!(a.b("ok") && a.b("hasAgent"), "{a}");
    assert_eq!((a.s("pane"), a.b("focused")), ("w1:p1", false));
    assert_eq!(focused(), 0);
    let a = sw_board::remote::door("do", &json!({ "action": "reopen", "id": "proj/1", "focus": true })).unwrap();
    assert!(a.b("ok") && a.b("focused"), "{a}");
    assert!(herdr.calls.borrow().iter().any(|c| c == "agent focus w1:p1"), "{:?}", herdr.calls.borrow());

    cli::set_fake(None);
    let _ = std::fs::remove_dir_all(&home);
}
