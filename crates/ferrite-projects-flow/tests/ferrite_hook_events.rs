//! Ferrite: the events Ferrite's Server hands upstream's `hook` (ferrite-core's `server/projects.rs`, whose own test
//! holds it to the same fixtures) reach a task as herdr's do (ADR 0031). One test, as it sets this process's variables.
use std::path::Path;
use std::rc::Rc;
use sw_flow::herdr::cli::{set_fake, Fake, Ran};
use sw_flow::json::*;
use sw_flow::store::tasks::{get, save};

const SOCKET: &str = "/ferrite/threads/api.sock";

/// herdr with every pane closed.
struct Closed;
impl Fake for Closed {
    fn run(&self, args: &[String], _: &str) -> Ran {
        if args.get(1).map(String::as_str) == Some("get") {
            return Ran { status: 1, stdout: String::new(), stderr: r#"{"error":{"code":"pane_not_found","message":"no such pane"}}"#.into() };
        }
        Ran { status: 0, stdout: r#"{"result":{}}"#.into(), stderr: String::new() }
    }
}

fn fixture(event: &str) -> String {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ferrite-core/src/server/fixtures/projects-hook").join(format!("{event}.json"));
    std::fs::read_to_string(file).unwrap().trim().to_string()
}

fn task(n: i64, pane: &str, worktree: Option<&str>) {
    let mut t = obj(json!({ "id": format!("app/{n}"), "repo": "app", "n": n, "title": "A task", "stage": "working", "msg": "",
        "created": n, "at": n, "agent": "claude", "link": { "socket": SOCKET, "pane": pane, "workspace": "2" } }));
    if let Some(w) = worktree { t.put("worktree", w); }
    save(&t).unwrap();
}
fn gone(n: i64) -> bool { get(&format!("app/{n}")).unwrap().at("last").b("gone") }

fn hear(event: &str) {
    unsafe {
        std::env::set_var("HERDR_PLUGIN_EVENT", event);
        std::env::set_var("HERDR_PLUGIN_EVENT_JSON", fixture(event));
    }
    sw_flow::hook::hook_main();
}

#[test]
fn a_closed_thread_and_a_removed_worktree_reach_their_tasks() {
    let home = std::env::temp_dir().join(format!("sw-ferrite-hook-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    unsafe {
        std::env::set_var("HERDR_PROJECTS_HOME", &home);
        std::env::set_var("HERDR_PROJECTS_INLINE", "1");
        std::env::set_var("HERDR_SOCKET_PATH", SOCKET);
    }
    set_fake(Some(Rc::new(Closed)));
    task(1, "7", None);
    task(2, "8", Some("/work/app-wt-1"));

    hear("pane.closed"); // Thread 7 closed
    assert!(gone(1), "the task of the closed Thread");
    assert!(!gone(2), "not another's");

    hear("worktree.removed"); // /work/app-wt-1 removed
    assert!(gone(2), "the task in the removed worktree");
    let _ = std::fs::remove_dir_all(&home);
}
