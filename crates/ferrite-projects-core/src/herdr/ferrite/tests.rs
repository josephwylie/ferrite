//! Each herdr call answered from a stand-in Server: a socket that answers Ferrite's API from a small world of
//! Projects and Threads, and keeps every request it was asked.
use super::*;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::sync::{Arc, Mutex};

/// What the stand-in Server knows: its Projects and the folder each holds, its Threads (open or parked) and pending
/// Decisions.
#[derive(Default)]
struct World { projects: Vec<Value>, folders: Vec<(String, String)>, threads: Vec<Value>, parked: Vec<Value>, decisions: Vec<Value>, asked: Vec<(String, Value)>, refuse: Vec<(String, Value)>, next: u64 }

/// The home folder, which `workspace create` labels `~`.
fn home() -> String { crate::store::files::user_home().display().to_string() }

impl World {
    /// The Project holding `cwd`, the nearest when several do, as the Server's registry finds it.
    fn containing(&self, cwd: &str) -> Option<String> {
        let holds = |folder: &str| Path::new(cwd).starts_with(folder);
        self.folders.iter().filter(|(_, f)| holds(f)).max_by_key(|(_, f)| f.len()).map(|(id, _)| id.clone())
    }
    /// The Project holding `cwd`, else `cwd` registered as one.
    fn open(&mut self, cwd: &str) -> Value {
        let id = self.containing(cwd).unwrap_or_else(|| {
            let id = (self.projects.len() + 1).to_string();
            let label = Path::new(cwd).file_name().unwrap().to_string_lossy().into_owned();
            self.projects.push(json!({ "id": id, "label": label, "focused": false, "thread_count": 0, "worktree": null }));
            self.folders.push((id.clone(), cwd.into()));
            id
        });
        self.projects.iter().find(|p| p.s("id") == id).unwrap().clone()
    }

    fn reply(&mut self, method: &str, params: &Value) -> Value {
        self.asked.push((method.into(), params.clone()));
        if let Some(i) = self.refuse.iter().position(|(m, _)| m == method) { return json!({ "error": self.refuse.remove(i).1 }); }
        let thread = params.at("thread").as_u64();
        let open = |w: &World| w.threads.iter().position(|t| t.at("thread").as_u64() == thread);
        let unknown = json!({ "error": { "code": "unknown_thread", "message": "no open Thread has that id" } });
        let result = match method {
            "thread.list" => json!({ "threads": self.threads }),
            "project.list" => json!({ "projects": self.projects }),
            "project.get" => match self.projects.iter().find(|p| p.at("id") == params.at("project")) { Some(p) => json!({ "project": p }), None => return json!({ "error": { "code": "project_not_found", "message": "no Project has that id" } }) },
            "project.open" => json!({ "project": self.open(params.s("cwd")) }),
            "thread.start" => {
                let cwd = params.s("cwd");
                let project = match params.so("project") {
                    Some(p) if self.folders.iter().any(|(id, f)| id == p && Path::new(cwd).starts_with(f)) => p.to_string(),
                    Some(_) => return json!({ "error": { "code": "invalid_request", "message": format!("{cwd} is not in that Project's directories") } }),
                    None => self.open(cwd).t("id"),
                };
                self.next += 1;
                let t = json!({ "thread": 20 + self.next, "title": params.at("title"), "provider": params.at("provider"), "attention": "idle", "cwd": cwd, "focused": false,
                    "project": project, "attention_seq": 0, "boot": "b1", "held_prompt": false, "queued_prompts": 0, "draft": false });
                self.threads.push(t.clone());
                json!({ "thread": t })
            }
            "thread.close" => match open(self) { Some(i) => { let t = self.threads.remove(i); self.parked.push(t); json!({ "thread": thread }) } None => return unknown },
            "thread.revive" => match self.parked.iter().position(|t| t.at("thread").as_u64() == thread) {
                Some(i) => { let t = self.parked.remove(i); self.threads.push(t.clone()); json!({ "thread": t }) }
                None => return unknown,
            },
            "thread.rename" => { let i = open(self).unwrap(); self.threads[i]["title"] = params.at("title").clone(); json!({ "thread": self.threads[i] }) }
            "decision.list" => json!({ "decisions": self.decisions }),
            "agent.read" => json!({ "thread": thread, "text": "line one\nline two" }),
            "worktree.create" => json!({ "worktree": { "path": "/store/worktrees/app/t-0001", "branch": params.at("branch") }, "project": "1" }),
            "machine.list" => json!({ "machines": [{ "id": "box", "label": "box", "target": "me@box" }, { "id": "attic", "label": "attic", "target": "attic" }] }),
            _ => json!({ "thread": thread }),
        };
        json!({ "result": result })
    }
}

struct Stand { ferrite: Ferrite, world: Arc<Mutex<World>> }

impl Stand {
    fn new(name: &str) -> Stand {
        let dir = std::env::temp_dir().join(format!("sw-ferrite-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let socket = dir.join("api.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let world = Arc::new(Mutex::new(World {
            projects: vec![json!({ "id": "1", "label": "app", "focused": false, "thread_count": 0, "worktree": { "repo_key": "/r/app", "checkout_path": "/r/app", "is_linked_worktree": false } }),
                json!({ "id": "2", "label": "me", "focused": false, "thread_count": 0, "worktree": null })],
            folders: vec![("1".into(), "/r/app".into()), ("2".into(), home())],
            ..Default::default()
        }));
        let served = world.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut line = String::new();
                if BufReader::new(&stream).read_line(&mut line).is_err() { continue; }
                let request: Value = serde_json::from_str(&line).unwrap();
                let reply = served.lock().unwrap().reply(request.s("method"), request.at("params"));
                let _ = stream.write_all(format!("{}\n", json!({ "id": request.at("id"), "result": reply.at("result"), "error": reply.at("error") })).as_bytes());
            }
        });
        Stand { ferrite: Ferrite { server: Server { socket, timeout: Duration::from_secs(75) }, panes: Panes { path: dir.join("ferrite-panes.json") } }, world }
    }
    /// herdr's printed answer to a command, its `result` unwrapped as `cli::run` does.
    fn run(&self, words: &[&str]) -> HRes<Value> {
        let out = self.ferrite.answer(&words.iter().map(|w| w.to_string()).collect::<Vec<_>>())?;
        let j: Value = serde_json::from_str(&out).unwrap_or_else(|_| json!({ "text": out }));
        Ok(if j.at("result").is_null() { j } else { j.at("result").clone() })
    }
    fn raw(&self, words: &[&str]) -> HRes<String> { self.ferrite.answer(&words.iter().map(|w| w.to_string()).collect::<Vec<_>>()) }
    fn asked(&self) -> Vec<(String, Value)> { self.world.lock().unwrap().asked.clone() }
    fn last(&self, method: &str) -> Value { self.asked().into_iter().rev().find(|(m, _)| m == method).map(|(_, p)| p).unwrap_or_else(|| panic!("never asked {method}")) }
    fn count(&self, method: &str) -> usize { self.asked().iter().filter(|(m, _)| m == method).count() }
    /// A pane placed in Project 1, at `/r/app`.
    fn tab(&self) -> String {
        self.run(&["tab", "create", "--workspace", "1", "--cwd", "/r/app", "--label", "t-0001", "--env", "HERDR_PROJECTS_DATA_DIR=/data", "--no-focus"]).unwrap().at("root_pane").t("pane_id")
    }
}

fn refused(r: HRes<Value>) -> HerdrError { r.expect_err("refused") }

#[test]
fn an_unwired_command_is_refused_by_name_only() {
    let args: Vec<String> = ["plugin", "pane", "open", "--plugin", "herdr-projects"].map(String::from).into();
    let Err(e) = launch(Some("/nowhere/api.sock"), &args, "", 1000) else { panic!("a command ran") };
    assert_eq!(e.code, "not_wired");
    assert_eq!(e.message, "Ferrite has not wired `herdr plugin pane open` to its Server yet.");
}

#[test]
fn a_bare_flag_is_named_as_it_is() {
    let Err(e) = launch(Some("/nowhere/api.sock"), &["--default-config".to_string()], "", 1000) else { panic!("a command ran") };
    assert!(e.message.contains("`herdr --default-config`"));
}

/// The terminal runtime's calls and herdr's own setup have no Ferrite counterpart: refused before anything is asked.
#[test]
fn what_only_a_terminal_has_is_refused_unasked() {
    let s = Stand::new("unwired");
    for words in [&["pane", "layout", "--pane", "p1"][..], &["pane", "zoom", "p1", "--on"], &["pane", "move", "p1", "--new-tab", "--workspace", "2"], &["plugin", "list", "--json"], &["plugin", "config-dir", "herdr-web-ui"],
        &["plugin", "disable", "herdr-superwork"], &["terminal", "session", "control", "t1", "--takeover"], &["config", "check"], &["server", "reload-config"]] {
        assert_eq!(refused(s.run(words)).code, "not_wired", "{words:?}");
    }
    assert_eq!(ask("popup.close", &json!({}), None).unwrap_err().code, "not_wired");
    // herdr's tab events are never subscribed to: nothing connects.
    let events = s.ferrite.server.socket.with_file_name("events.sock");
    let listener = UnixListener::bind(&events).unwrap();
    listener.set_nonblocking(true).unwrap();
    super::super::socket::on_tab_shown(Some(events.display().to_string()), |_| panic!("never"), || panic!("never"));
    assert!(listener.accept().is_err());
    assert!(s.asked().is_empty());
}

/// The nav draws its rows from the board's data: what herdr's sidebar is told answers at once, asking nothing.
#[test]
fn sidebar_reports_are_silent() {
    let s = Stand::new("metadata");
    s.run(&["pane", "report-metadata", "p1", "--source", "plugin:herdr-projects", "--display-agent", "t-0001", "--token", "summary=x"]).unwrap();
    s.run(&["workspace", "report-metadata", "1", "--source", "plugin:herdr-projects", "--clear-token", "summary"]).unwrap();
    assert!(s.asked().is_empty());
}

#[test]
fn a_server_that_is_not_running_is_no_server_and_one_that_never_answers_a_timeout() {
    let gone = Ferrite { server: Server { socket: "/nowhere/api.sock".into(), timeout: Duration::from_secs(1) }, panes: Panes { path: "/nowhere/panes.json".into() } };
    assert_eq!(gone.answer(&["workspace".into(), "list".into()]).unwrap_err().code, "no_server");
    let dir = std::env::temp_dir().join(format!("sw-ferrite-silent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let listener = UnixListener::bind(dir.join("api.sock")).unwrap();
    // Holds every connection, answering none. (Collecting `incoming()` would size its Vec by the endless iterator's hint.)
    std::thread::spawn(move || { let mut held = Vec::new(); for c in listener.incoming() { held.push(c); } });
    let silent = Server { socket: dir.join("api.sock"), timeout: Duration::from_millis(100) };
    let e = silent.call("thread.list", json!({})).unwrap_err();
    assert_eq!((e.code.as_str(), e.message.as_str()), ("timeout", "herdr did not answer in time"));
}

/// Projects are Spaces, a linked worktree's folder a Space of its own; a Thread is a pane, tab and agent.
#[test]
fn projects_and_worktrees_are_spaces_and_threads_their_panes() {
    let s = Stand::new("spaces");
    s.world.lock().unwrap().threads = vec![
        json!({ "thread": 12, "title": "hp-demo", "provider": "claude", "attention": "working", "cwd": "/wt", "focused": true, "project": "1", "provider_session": "s-1", "attention_seq": 7, "completion_seq": 6, "boot": "b1", "worktree": "/wt" }),
        json!({ "thread": 13, "title": "mine", "provider": "codex", "attention": "idle", "cwd": "/home/me", "focused": false, "project": "2", "attention_seq": 2, "boot": "b1" }),
    ];
    let spaces = s.run(&["workspace", "list"]).unwrap();
    let ids: Vec<&str> = spaces.arr("workspaces").iter().map(|w| w.s("workspace_id")).collect();
    assert_eq!(ids, ["1", "2", "/wt"]);
    let app = &spaces.arr("workspaces")[0];
    assert_eq!((app.i("pane_count"), app.b("focused")), (0, false), "its one Thread works in its worktree's Space");
    assert_eq!(app.at("worktree").s("repo_root"), "/r/app");
    let wt = &spaces.arr("workspaces")[2];
    assert_eq!((wt.s("label"), wt.i("pane_count"), wt.b("focused")), ("wt", 1, true));
    assert_eq!(wt.at("worktree"), &json!({ "checkout_path": "/wt", "repo_root": "/r/app", "repo_key": "/r/app", "is_linked_worktree": true }));
    assert!(!spaces.arr("workspaces")[1].at("worktree").is_object(), "outside git a Project has no worktree");
    assert_eq!(s.run(&["workspace", "get", "/wt"]).unwrap().at("workspace").s("workspace_id"), "/wt");
    assert_eq!(refused(s.run(&["workspace", "get", "9"])).code, "workspace_not_found");

    let agent = s.run(&["agent", "get", "12"]).unwrap();
    let a = agent.at("agent");
    assert_eq!((a.s("pane_id"), a.s("tab_id"), a.s("workspace_id"), a.s("name")), ("12", "12", "/wt", "hp-demo"));
    assert_eq!((a.s("agent"), a.s("agent_status"), a.i("state_change_seq")), ("claude", "working", 7));
    assert_eq!(a.at("agent_session"), &json!({ "kind": "id", "value": "s-1" }));
    assert_eq!(a.i("completion_seq"), 6, "the count its last turn ended at");
    assert!(s.run(&["agent", "get", "13"]).unwrap().at("agent").get("completion_seq").is_none(), "absent until a turn ended");
    assert_eq!(s.run(&["agent", "get", "mine"]).unwrap().at("agent").s("pane_id"), "13", "an agent by its name");
    assert_eq!(refused(s.run(&["agent", "get", "99"])).code, "agent_not_found");
    assert_eq!(refused(s.run(&["pane", "get", "99"])).code, "pane_not_found");
    assert_eq!(refused(s.run(&["tab", "get", "99"])).code, "tab_not_found");
    assert_eq!(s.run(&["agent", "list"]).unwrap().arr("agents").len(), 2);
    let panes = s.run(&["pane", "list", "--workspace", "2"]).unwrap();
    assert_eq!(panes.arr("panes").len(), 1);
    assert_eq!(panes.arr("panes")[0].s("cwd"), "/home/me");
    assert_eq!(s.run(&["tab", "list", "--workspace", "/wt"]).unwrap().arr("tabs")[0].s("label"), "hp-demo");
}

/// A placed pane is at its shell until its agent starts: idle, no agent, nothing read, nothing asked to place it.
#[test]
fn a_placed_pane_waits_at_its_shell() {
    let s = Stand::new("placed");
    let made = s.run(&["workspace", "create", "--cwd", "/r/app", "--label", "app", "--env", "A=1", "--no-focus"]).unwrap();
    assert_eq!(s.last("project.open"), json!({ "cwd": "/r/app" }));
    let space = made.at("workspace").t("workspace_id");
    let pane = made.at("root_pane");
    assert_eq!((pane.s("workspace_id"), pane.s("cwd"), pane.s("agent_status")), (space.as_str(), "/r/app", "unknown"));
    assert!(pane.at("agent").is_null());
    assert_eq!(made.at("tab").s("tab_id"), pane.s("pane_id"));
    let id = pane.t("pane_id");
    let info = s.run(&["pane", "process-info", "--pane", &id]).unwrap();
    let i = info.at("process_info");
    assert_eq!(i.i("foreground_process_group_id"), i.i("shell_pid"), "nothing but its shell");
    assert_eq!(crate::herdr::cli::kids(i.i("shell_pid")), Ok(vec![]), "a pid no process has");
    assert_eq!(s.raw(&["pane", "read", &id, "--source", "visible"]).unwrap(), "");
    assert_eq!(refused(s.run(&["agent", "get", &id])).code, "agent_not_found");
    for key in ["esc", "ctrl+c", "enter"] { s.run(&["agent", "send-keys", &id, key]).unwrap(); }
    s.run(&["pane", "send-text", &id, "\u{3}"]).unwrap();
    s.run(&["tab", "focus", &id]).unwrap();
    assert_eq!(s.count("thread.focus") + s.count("agent.interrupt") + s.count("decision.answer"), 0);
    let split = s.run(&["pane", "split", &id, "--direction", "right", "--no-focus"]).unwrap();
    assert_eq!((split.at("pane").s("workspace_id"), split.at("pane").s("cwd")), (space.as_str(), "/r/app"));
    assert_eq!(s.run(&["pane", "list", "--workspace", &space]).unwrap().arr("panes").len(), 2);
}

/// The Coordinator's place: a tab `Coordinator` in the home Space, its agent an ordinary one.
#[test]
fn the_coordinator_is_a_staged_tab_in_the_home_project() {
    let s = Stand::new("coordinator");
    let made = s.run(&["tab", "create", "--workspace", "2", "--cwd", &format!("{}/.herdr-projects/coordinator", home()), "--label", "Coordinator", "--no-focus"]).unwrap();
    let pane = made.at("root_pane").t("pane_id");
    assert_eq!(s.run(&["tab", "get", &pane]).unwrap().at("tab"), &json!({ "tab_id": pane, "workspace_id": "2", "label": "Coordinator", "pane_count": 1, "focused": false }));
    let started = s.run(&["agent", "start", "sw-coordinator", "--kind", "claude", "--pane", &pane, "--timeout", "60000"]).unwrap();
    assert_eq!((started.at("agent").s("pane_id"), started.at("agent").s("name")), (pane.as_str(), "sw-coordinator"));
    assert_eq!(s.last("thread.start")["project"], "2");
    assert_eq!(s.run(&["tab", "get", &pane]).unwrap().at("tab").s("label"), "Coordinator", "its tab keeps its label");
    assert!(refused(s.run(&["tab", "create", "--workspace", "9", "--cwd", "/x"])).code.contains("not_found"));
}

/// `agent start` starts the Thread placed in the pane, ready, with the board's arguments and environment; a resume among
/// the arguments is the start's own. The pane's id names the Thread from then on.
#[test]
fn an_agent_starts_as_the_pane_placed_its_thread() {
    let s = Stand::new("start");
    let pane = s.tab();
    let started = s.run(&["agent", "start", "hp-demo-t-0001", "--kind", "claude", "--pane", &pane, "--timeout", "60000", "--", "--model", "opus", "--resume", "s-1"]).unwrap();
    assert_eq!(s.last("thread.start"), json!({ "args": ["--model", "opus"], "env": { "HERDR_PROJECTS_DATA_DIR": "/data" }, "launcher": [], "ready": true, "timeout_ms": 60000,
        "resume": "s-1", "cwd": "/r/app", "provider": "claude", "title": "hp-demo-t-0001", "project": "1" }));
    let a = started.at("agent");
    assert_eq!((a.s("pane_id"), a.s("workspace_id"), a.s("agent"), a.s("agent_status")), (pane.as_str(), "1", "claude", "idle"));
    assert_eq!(s.run(&["agent", "get", &pane]).unwrap().at("agent").s("terminal_id"), "21@b1");
    let panes = s.run(&["pane", "list"]).unwrap();
    assert_eq!(panes.arr("panes").iter().map(|p| p.t("pane_id")).collect::<Vec<_>>(), [pane.clone()], "the Thread is the pane, once");
    assert_eq!(refused(s.run(&["agent", "start", "x", "--kind", "claude", "--pane", &pane])).code, "pane_busy");
    // codex's resume is a word, then the conversation.
    let other = s.tab();
    s.run(&["agent", "start", "hp-demo-t-0002", "--kind", "codex", "--pane", &other, "--", "-m", "gpt-5.5", "resume", "019a"]).unwrap();
    assert_eq!((s.last("thread.start")["args"].clone(), s.last("thread.start")["resume"].clone()), (json!(["-m", "gpt-5.5"]), json!("019a")));
    assert_eq!(split_resume("claude", &["--resume".into(), "--dangerous".into()]), (vec!["--resume".to_string(), "--dangerous".into()], None), "a word that only looks like one stays");
}

/// Agent kinds Ferrite has no Provider for fail at their start, clearly, before anything starts.
#[test]
fn a_kind_without_a_provider_fails_clearly() {
    let s = Stand::new("no-provider");
    let pane = s.tab();
    for kind in ["pi", "agy", "copilot", "cursor", "devin", "droid", "grok", "hermes", "kilo", "kimi", "letta", "mastracode", "omp", "opencode", "qodercli", "qwen"] {
        let e = refused(s.run(&["agent", "start", "x", "--kind", kind, "--pane", &pane]));
        assert_eq!(e.code, "no_provider", "{kind}");
        let start = crate::agents::start_failed::Start { kind, setting: "worker.agent", again: "delegate again" };
        let why = crate::agents::start_failed::explain(&start, &e.message, "");
        assert_eq!(why.short, format!("Ferrite cannot run {kind}"));
    }
    assert_eq!(s.count("thread.start"), 0);
}

/// herdr's `pane quit` then `agent start` in that pane: the Thread is parked, then started again in place, keeping its
/// id; another kind starts a new Thread there.
#[test]
fn an_agent_quit_and_started_again_is_its_thread_revived() {
    let s = Stand::new("revive");
    let pane = s.tab();
    s.run(&["agent", "start", "hp-demo-t-0001", "--kind", "claude", "--pane", &pane]).unwrap();
    assert!(s.ferrite.quit(&pane).is_ok());
    assert_eq!(s.last("thread.close"), json!({ "thread": 21 }));
    assert!(refused(s.run(&["agent", "get", &pane])).code == "agent_not_found", "the pane is at its shell");
    assert!(s.run(&["pane", "get", &pane]).unwrap().at("pane").at("agent").is_null());
    let again = s.run(&["agent", "start", "hp-demo-t-0001-b", "--kind", "claude", "--pane", &pane, "--", "--resume", "s-1"]).unwrap();
    assert_eq!(s.last("thread.revive"), json!({ "args": [], "env": { "HERDR_PROJECTS_DATA_DIR": "/data" }, "launcher": [], "ready": true, "timeout_ms": 60000, "resume": "s-1", "thread": 21, "launch_provider": "claude" }));
    assert_eq!(s.last("thread.rename"), json!({ "thread": 21, "title": "hp-demo-t-0001-b" }));
    assert_eq!(again.at("agent").s("terminal_id"), "21@b1", "the same terminal while the Server runs");
    assert_eq!(s.count("thread.start"), 1);
    s.ferrite.quit(&pane).unwrap();
    s.run(&["agent", "start", "hp-demo-t-0001", "--kind", "codex", "--pane", &pane]).unwrap();
    assert_eq!(s.count("thread.start"), 2, "another kind is another Thread");
    assert_eq!(s.run(&["agent", "get", &pane]).unwrap().at("agent").s("terminal_id"), "22@b1");
}

#[test]
fn a_native_provider_switch_is_remembered_when_the_pane_is_parked() {
    let s = Stand::new("handover-revive");
    let pane = s.tab();
    s.run(&["agent", "start", "coordinator", "--kind", "claude", "--pane", &pane]).unwrap();
    s.world.lock().unwrap().threads[0]["provider"] = json!("codex");
    assert_eq!(s.run(&["agent", "get", &pane]).unwrap().at("agent").s("agent"), "codex");
    s.ferrite.quit(&pane).unwrap();
    s.run(&["agent", "start", "coordinator", "--kind", "codex", "--pane", &pane, "--", "resume", "codex-current"]).unwrap();
    assert_eq!(s.count("thread.start"), 1, "the native handover keeps the Thread");
    assert_eq!(s.last("thread.revive").s("launch_provider"), "codex");
    assert_eq!(s.last("thread.revive").s("resume"), "codex-current");
}

/// herdr's terminal id changes when its server restarts, its `state_change_seq` starting over (`flow/hook.rs`
/// `after_handled`): a Thread's is its id in the Server's boot.
#[test]
fn a_terminal_is_another_after_the_server_restarts() {
    let s = Stand::new("restart");
    let pane = s.tab();
    s.run(&["agent", "start", "hp-demo-t-0001", "--kind", "claude", "--pane", &pane]).unwrap();
    let before = s.run(&["agent", "get", &pane]).unwrap().at("agent").t("terminal_id");
    s.world.lock().unwrap().threads[0]["boot"] = json!("b2");
    let after = s.run(&["agent", "get", &pane]).unwrap().at("agent").t("terminal_id");
    assert_eq!((before.as_str(), after.as_str()), ("21@b1", "21@b2"));
    assert_eq!(s.run(&["pane", "get", &pane]).unwrap().at("pane").s("terminal_id"), "21@b2");
}

/// A start refused `agent_not_ready` leaves its Thread, and the pane names it; a folder outside the Project starts in
/// the Project it is.
#[test]
fn a_thread_left_by_a_refused_start_is_the_panes() {
    let s = Stand::new("not-ready");
    let pane = s.tab();
    {
        let mut w = s.world.lock().unwrap();
        w.refuse.push(("thread.start".into(), json!({ "code": "invalid_request", "message": "/r/app is not in that Project's directories" })));
        w.refuse.push(("thread.start".into(), json!({ "code": "agent_not_ready", "message": "Thread 30 started, but its agent is blocked", "thread": 30 })));
        w.threads.push(json!({ "thread": 30, "title": "t", "provider": "claude", "attention": "blocked", "cwd": "/r/app", "project": "5" }));
    }
    assert_eq!(refused(s.run(&["agent", "start", "t", "--kind", "claude", "--pane", &pane])).code, "agent_not_ready");
    assert!(s.last("thread.start").get("project").is_none(), "tried again without the Project");
    assert_eq!(s.run(&["agent", "get", &pane]).unwrap().at("agent").s("agent_status"), "blocked");
}

/// A plan writer in a read-only mount: herdr's `pane run` of `bwrap … -- <program>` is a Thread run through that
/// launcher.
#[test]
fn a_plan_writer_runs_through_its_mount() {
    let mut s = Stand::new("sandbox");
    // `Herdr::pane_run`'s deadline.
    s.ferrite.server.timeout = Duration::from_secs(30);
    let pane = s.tab();
    let line = "env HERDR_PROJECTS_AGENT=hp-demo-t-0001 bwrap --dev-bind / / --ro-bind /r/app /r/app --die-with-parent -- claude --permission-mode plan 'it'\\''s'";
    s.run(&["pane", "run", &pane, line]).unwrap();
    let start = s.last("thread.start");
    assert_eq!(start["launcher"], json!(["bwrap", "--dev-bind", "/", "/", "--ro-bind", "/r/app", "/r/app", "--die-with-parent", "--"]));
    assert_eq!((start["args"].clone(), start["title"].clone(), start["provider"].clone()), (json!(["--permission-mode", "plan", "it's"]), json!("hp-demo-t-0001"), json!("claude")));
    assert_eq!(start["timeout_ms"], 25000, "ready, or refused naming its Thread, before the request's deadline");
    assert_eq!(s.run(&["agent", "get", &pane]).unwrap().at("agent").s("name"), "hp-demo-t-0001");
    assert_eq!(refused(s.run(&["pane", "run", &pane, "make test"])).code, "not_wired");
}

/// A worktree is made by the Server, its Space its path; removed by that path, never forced; a Project is no worktree.
#[test]
fn a_worktree_is_made_opened_and_removed_by_its_path() {
    let s = Stand::new("worktree");
    let made = s.run(&["worktree", "create", "--cwd", "/r/app", "--branch", "hp/demo/t-0001", "--base", "origin/main", "--label", "Demo t-0001", "--no-focus"]).unwrap();
    assert_eq!(s.last("worktree.create"), json!({ "cwd": "/r/app", "branch": "hp/demo/t-0001", "base": "origin/main" }));
    let path = "/store/worktrees/app/t-0001";
    assert_eq!(made.at("worktree"), &json!({ "path": path, "branch": "hp/demo/t-0001", "open_workspace_id": path }));
    let w = made.at("workspace");
    assert_eq!((w.s("workspace_id"), w.s("label"), w.at("worktree").s("repo_root")), (path, "Demo t-0001", "/r/app"));
    assert_eq!((made.at("root_pane").s("cwd"), made.at("root_pane").s("workspace_id")), (path, path));
    let opened = s.run(&["worktree", "open", "--cwd", "/r/app", "--path", path, "--no-focus"]).unwrap();
    assert!(opened.b("already_open"));
    assert_eq!(opened.at("root_pane").s("pane_id"), made.at("root_pane").s("pane_id"));
    let fresh = s.run(&["worktree", "open", "--cwd", "/r/app", "--path", "/r/.worktrees/old", "--label", "Old", "--no-focus"]).unwrap();
    assert!(!fresh.b("already_open"));
    assert_eq!(fresh.at("workspace").at("worktree").s("repo_root"), "/r/app");

    assert_eq!(refused(s.run(&["worktree", "remove", "--workspace", path, "--force"])).code, "invalid_request");
    assert_eq!(refused(s.run(&["worktree", "remove", "--workspace", "1"])).code, "workspace_not_found");
    assert_eq!(s.count("worktree.remove"), 0);
    s.world.lock().unwrap().refuse.push(("worktree.remove".into(), json!({ "code": "dirty_worktree_requires_force", "message": "it has changes; nothing was removed" })));
    assert_eq!(refused(s.run(&["worktree", "remove", "--workspace", path])).code, "dirty_worktree_requires_force");
    s.run(&["worktree", "remove", "--workspace", path]).unwrap();
    assert_eq!(s.last("worktree.remove"), json!({ "path": path }));
    assert_eq!(refused(s.run(&["workspace", "get", path])).code, "workspace_not_found", "gone with its panes");
}

/// Closing a Project's Space closes what the board placed there; the operator's own Threads in it stay.
#[test]
fn a_spaces_close_parks_only_the_boards_threads() {
    let s = Stand::new("close");
    s.world.lock().unwrap().threads.push(json!({ "thread": 13, "title": "mine", "provider": "codex", "attention": "idle", "cwd": "/r/app", "project": "1" }));
    let pane = s.tab();
    s.run(&["agent", "start", "t", "--kind", "claude", "--pane", &pane]).unwrap();
    let staged = s.tab();
    s.run(&["workspace", "close", "1"]).unwrap();
    assert_eq!(s.count("thread.close"), 1);
    assert_eq!(s.last("thread.close"), json!({ "thread": 21 }));
    assert_eq!(refused(s.run(&["pane", "get", &staged])).code, "pane_not_found");
    assert_eq!(s.run(&["workspace", "get", "1"]).unwrap().at("workspace").i("pane_count"), 1, "the operator's Thread");
    s.run(&["workspace", "focus", "1"]).unwrap();
    assert_eq!(s.last("project.focus"), json!({ "project": "1" }));
    s.run(&["workspace", "rename", "1", "billing", "app"]).unwrap();
    assert_eq!(s.last("project.rename"), json!({ "project": "1", "label": "billing app" }));
    // A tab of the operator's is a Thread renamed and closed by its id.
    s.run(&["tab", "rename", "13", "reviewer"]).unwrap();
    assert_eq!(s.last("thread.rename"), json!({ "thread": 13, "title": "reviewer" }));
    s.run(&["tab", "close", "13"]).unwrap();
    assert_eq!(s.last("thread.close"), json!({ "thread": 13 }));
}

/// herdr's `workspace create` always makes a Space: two in one Project are two Spaces, each renamed, focused and
/// closed alone, and the Project keeps its label. The Coordinator's home Space `~` holds the home Project, as a task in
/// a folder under it does; closing or renaming the task's Space leaves the Coordinator, and `~`, as they were.
#[test]
fn each_space_made_is_its_own_inside_its_project() {
    let s = Stand::new("made-spaces");
    let start = |pane: &str, name: &str| { s.run(&["agent", "start", name, "--kind", "claude", "--pane", pane]).unwrap().at("agent").at("terminal_id").as_str().unwrap().split('@').next().unwrap().parse::<u64>().unwrap() };
    let made = |cwd: &str, label: &[&str]| s.run(&[&["workspace", "create", "--cwd", cwd][..], label, &["--no-focus"]].concat()).unwrap();

    let home_made = made(&home(), &[]);
    let home_space = home_made.at("workspace").t("workspace_id");
    assert_eq!(home_made.at("workspace").s("label"), "~");
    let coordinator = s.run(&["tab", "create", "--workspace", &home_space, "--cwd", &format!("{}/.herdr-projects/coordinator", home()), "--label", "Coordinator", "--no-focus"]).unwrap().at("root_pane").t("pane_id");
    let coordinator_thread = start(&coordinator, "sw-coordinator");
    assert_eq!(s.last("thread.start")["project"], "2", "the home Project");

    let notes = format!("{}/notes", home());
    let task = made(&notes, &["--label", "fix notes"]);
    let task_space = task.at("workspace").t("workspace_id");
    assert_ne!(task_space, home_space, "a Space of its own, in the same Project");
    let task_thread = start(task.at("root_pane").s("pane_id"), "hp-notes-t-0001");
    assert_eq!(s.last("thread.start")["project"], "2");
    assert_eq!(s.last("thread.start")["cwd"], json!(notes));

    s.run(&["workspace", "rename", &task_space, "fix", "notes", "(review)"]).unwrap();
    assert_eq!(s.count("project.rename"), 0, "the Project keeps its label");
    assert_eq!(s.run(&["workspace", "get", &task_space]).unwrap().at("workspace").s("label"), "fix notes (review)");
    assert_eq!(s.run(&["workspace", "get", "2"]).unwrap().at("workspace").s("label"), "me");

    s.run(&["workspace", "focus", &task_space]).unwrap();
    assert_eq!(s.last("thread.focus"), json!({ "thread": task_thread }));
    s.run(&["workspace", "focus", "2"]).unwrap();
    assert_eq!(s.last("project.focus"), json!({ "project": "2" }));

    s.run(&["workspace", "close", &task_space]).unwrap();
    assert_eq!(s.asked().iter().filter(|(m, _)| m == "thread.close").map(|(_, p)| p.clone()).collect::<Vec<_>>(), [json!({ "thread": task_thread })]);
    assert_eq!(refused(s.run(&["workspace", "get", &task_space])).code, "workspace_not_found", "gone with its panes");
    assert_eq!(s.run(&["agent", "get", &coordinator]).unwrap().at("agent").s("terminal_id"), format!("{coordinator_thread}@b1"), "the Coordinator runs on");
    let spaces = s.run(&["workspace", "list"]).unwrap();
    let home_now: Vec<&str> = spaces.arr("workspaces").iter().filter(|w| w.s("label") == "~").map(|w| w.s("workspace_id")).collect();
    assert_eq!(home_now, [home_space.as_str()], "`~` is found again, once");

    // Two tasks in one repository: each closes its own.
    let a = made("/r/app", &["--label", "a"]);
    let b = made("/r/app", &["--label", "b"]);
    let (a_thread, b_thread) = (start(a.at("root_pane").s("pane_id"), "a"), start(b.at("root_pane").s("pane_id"), "b"));
    assert_eq!(a.at("workspace").at("worktree").s("repo_root"), "/r/app", "a Space in a repository's checkout");
    s.run(&["workspace", "close", a.at("workspace").s("workspace_id")]).unwrap();
    assert_eq!(s.last("thread.close"), json!({ "thread": a_thread }));
    assert_eq!(s.run(&["agent", "get", b.at("root_pane").s("pane_id")]).unwrap().at("agent").s("terminal_id"), format!("{b_thread}@b1"));
}

/// Prompts are confirmed taken; reads are the transcript's tail; focus lands every window.
#[test]
fn prompts_reads_and_focus_reach_the_thread() {
    let s = Stand::new("prompt");
    let pane = s.tab();
    s.run(&["agent", "start", "t", "--kind", "claude", "--pane", &pane]).unwrap();
    let prompted = s.run(&["agent", "prompt", &pane, "-x read the brief"]).unwrap();
    assert_eq!(s.last("agent.prompt"), json!({ "thread": 21, "text": "-x read the brief", "until": ["working", "blocked"], "timeout_ms": 70000 }), "answered before the request's deadline");
    assert_eq!(prompted.at("agent").s("pane_id"), pane);
    assert_eq!(s.raw(&["agent", "read", &pane, "--source", "recent-unwrapped", "--lines", "120"]).unwrap(), "line one\nline two");
    assert_eq!(s.last("agent.read"), json!({ "thread": 21, "lines": 120 }));
    s.raw(&["pane", "read", &pane, "--source", "visible"]).unwrap();
    assert_eq!(s.last("agent.read"), json!({ "thread": 21 }));
    for words in [&["agent", "focus", pane.as_str()][..], &["tab", "focus", &pane], &["agent", "attach", &pane]] {
        s.run(words).unwrap();
        assert_eq!(s.last("thread.focus"), json!({ "thread": 21 }));
    }
    assert_eq!(s.count("thread.focus"), 3);
    let info = s.run(&["pane", "process-info", "--pane", &pane]).unwrap();
    assert_ne!(info.at("process_info").i("foreground_process_group_id"), info.at("process_info").i("shell_pid"), "its agent runs");
    assert_eq!(s.raw(&["integration", "status"]).unwrap(), "");
}

/// The Coordinator keeps a refused prompt queued only for herdr's retryable
/// errors. An older Server's occupied startup slot must use that same
/// contract.
#[test]
fn a_prompt_waiting_for_startup_is_retryable_without_restarting_the_agent() {
    let s = Stand::new("prompt-starting");
    let pane = s.tab();
    s.run(&["agent", "start", "sw-coordinator", "--kind", "codex", "--pane", &pane]).unwrap();
    let message = "this Thread's Session is starting and already holds a prompt; nothing was sent: retry once the Thread is working";
    s.world.lock().unwrap().refuse.push(("agent.prompt".into(), json!({ "code": "agent_starting", "message": message })));

    let error = refused(s.run(&["agent", "prompt", &pane, "Hello?"]));
    assert_eq!(error.code, "agent_not_ready", "the board must keep retrying this queued message");
    assert_eq!(error.message, message);

    s.run(&["agent", "prompt", &pane, "Hello?"]).unwrap();
    assert_eq!(s.last("agent.prompt").s("text"), "Hello?");
    assert_eq!(s.count("thread.start"), 1);
    assert_eq!(s.count("thread.close"), 0);
}

const PERMISSION: &str = r#"[{"thread":21,"handle":{},"decision":{"id":"perm_01","kind":{"kind":"approval"},"description":"Run git push","tool_name":"Bash","input":{"command":"git push"},"suggestions":[{"label":"always","value":{"rule":"Bash(git push)"},"standing":true}]}}]"#;

/// Escape interrupts; a menu key answers the pending Decision; text answers a question that takes it.
#[test]
fn keys_and_text_become_what_they_meant() {
    let s = Stand::new("keys");
    let pane = s.tab();
    s.run(&["agent", "start", "t", "--kind", "claude", "--pane", &pane]).unwrap();
    s.run(&["agent", "send-keys", &pane, "esc"]).unwrap();
    assert_eq!(s.last("agent.interrupt"), json!({ "thread": 21 }));
    s.run(&["agent", "send-keys", &pane, "enter"]).unwrap();
    assert_eq!(refused(s.run(&["agent", "send-keys", &pane, "down"])).code, "no_decision");
    s.world.lock().unwrap().decisions = serde_json::from_str(PERMISSION).unwrap();
    s.run(&["agent", "send-keys", &pane, "1"]).unwrap();
    assert_eq!(s.last("decision.answer"), json!({ "thread": 21, "decision": "perm_01", "answer": { "kind": "allow", "input": { "command": "git push" } } }));
    s.run(&["agent", "send-keys", &pane, "2", "enter"]).unwrap();
    assert_eq!(s.last("decision.answer")["answer"]["kind"], "allow_always");
    s.run(&["agent", "send-keys", &pane, "n"]).unwrap();
    assert_eq!(s.last("decision.answer")["answer"]["kind"], "deny");
    let answered = s.count("decision.answer");
    assert_eq!(refused(s.run(&["agent", "send-keys", &pane, "up", "up"])).code, "invalid_answer");
    assert_eq!(refused(s.run(&["pane", "send-text", &pane, "yes"])).code, "invalid_answer");
    assert_eq!(s.count("decision.answer"), answered, "refused before anything is sent");
    s.world.lock().unwrap().decisions = vec![json!({ "thread": 21, "decision": { "id": "q1", "kind": { "kind": "questions", "questions": [{ "question": "Which?", "options": [{ "label": "a" }], "allow_other": true }] } } })];
    s.run(&["pane", "send-text", &pane, "-- neither"]).unwrap();
    assert_eq!(s.last("decision.answer")["answer"], json!({ "kind": "questions", "answers": [{ "picks": [], "other": "-- neither" }] }));
}

/// The saved machines, in their order, each the one session of its Server.
#[test]
fn machines_are_listed_in_their_order() {
    let s = Stand::new("machines");
    let listed: Value = serde_json::from_str(&s.raw(&["machine", "list", "--json"]).unwrap()).unwrap();
    assert_eq!(listed, json!([{ "id": "box", "label": "box", "target": "me@box", "session": "default", "enabled": true }, { "id": "attic", "label": "attic", "target": "attic", "session": "default", "enabled": true }]));
}

/// One Server per computer: herdr's one session, `default`.
#[test]
fn the_one_session_is_the_servers_socket() {
    assert!(socket().ends_with("api.sock"));
    assert_eq!(crate::herdr::cli::session_name(&socket()), "default");
    assert_eq!(herdr_error(Refused::new("project_not_found", "x"), "workspace").code, "workspace_not_found");
}

#[test]
fn board_thread_ownership_survives_replacement_and_pane_removal() {
    let stand = Stand::new("owned-threads");
    let panes = &stand.ferrite.panes;
    let pane = panes.place(Pane { thread: Some(41), ..Default::default() }).unwrap();
    panes.update(&pane, |pane| pane.thread = Some(42)).unwrap();
    panes.retain(|_, _| false).unwrap();
    assert!(panes.all().unwrap().is_empty());
    assert_eq!(panes.owned_threads().unwrap(), vec![41, 42]);
}

#[test]
fn existing_pane_files_supply_ownership_before_the_next_write() {
    let stand = Stand::new("owned-legacy");
    let panes = &stand.ferrite.panes;
    std::fs::write(&panes.path, serde_json::to_vec(&json!({
        "next": 1, "panes": { "p1": { "space": "1", "cwd": "/r/app", "thread": 43 } }
    })).unwrap()).unwrap();
    assert_eq!(panes.owned_threads().unwrap(), vec![43]);
    panes.retain(|_, _| false).unwrap();
    assert_eq!(panes.owned_threads().unwrap(), vec![43]);
}
