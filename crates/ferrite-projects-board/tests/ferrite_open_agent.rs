//! Ferrite: Enter on a task opens its agent in the window. Every agent runs in a Ferrite window, so the job counts as
//! in herdr (`cli::in_herdr`): it puts the agent in front through the adapter, and never hands over a terminal
//! (`Then::Attach`), which a window cannot attach.
use std::cell::RefCell;
use std::rc::Rc;

use serde_json::json;
use sw_board::board::job::{self, Job, Then};
use sw_board::herdr::cli::{self, Fake, Ran};

const PANE: &str = "w1:p1";

/// herdr as the adapter answers it: the task's pane, its agent there, and every call it was asked.
#[derive(Default)]
struct Herdr {
    calls: RefCell<Vec<String>>,
}

impl Fake for Herdr {
    fn run(&self, args: &[String], _input: &str) -> Ran {
        self.calls.borrow_mut().push(args.join(" "));
        let words: Vec<&str> = args.iter().map(String::as_str).take(2).collect();
        let result = match words.as_slice() {
            ["pane", "get"] => json!({ "pane": { "pane_id": PANE, "workspace_id": "w1", "tab_id": "w1:t1" } }),
            ["agent", "get"] => json!({ "agent": { "pane_id": PANE, "status": "idle" } }),
            _ => json!({}),
        };
        Ran { status: 0, stdout: json!({ "result": result }).to_string(), stderr: String::new() }
    }
}

#[test]
fn enter_on_a_task_focuses_its_agent_and_never_attaches() {
    let home = std::env::temp_dir().join(format!("ferrite-open-agent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let socket = home.join("herdr.sock").display().to_string();
    // One test in this program: nothing else reads the environment meanwhile.
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("HERDR_PROJECTS_HOME", home.join("sw"));
        std::env::set_var("HERDR_SOCKET_PATH", &socket);
        std::env::remove_var("HERDR_ENV");
    }
    let task = json!({ "id": "proj/1", "repo": "proj", "title": "Open me", "link": { "pane": PANE, "socket": socket } });
    sw_board::store::tasks::save(task.as_object().unwrap()).unwrap();
    let herdr = Rc::new(Herdr::default());
    cli::set_fake(Some(herdr.clone()));

    for (popup, then) in [(false, Then::Opened("proj/1".into())), (true, Then::Leave)] {
        let done = job::run(Job::OpenAgent { id: "proj/1".into(), popup });
        assert_eq!(done.then, [then], "popup {popup}: {:?}", done.flash);
    }
    assert!(herdr.calls.borrow().iter().any(|call| call == &format!("agent focus {PANE}")), "{:?}", herdr.calls.borrow());

    cli::set_fake(None);
    let _ = std::fs::remove_dir_all(&home);
}
