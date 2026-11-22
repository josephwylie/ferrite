//! Delegates (ADR 0047) between two Servers in this process: a parent
//! Server follows its Delegate on a peer Server through Peers, over the
//! peer's API as `ferrited api-bridge` would carry it, and wakes the
//! parent's agent with the result.

use std::sync::atomic::{AtomicBool, Ordering};

use super::pr_watch::{copy_dir, wakes_in};
use super::*;
use crate::delegates::Delegation;
use crate::peers::Peers;
use ferrite_core::machine::Machine;
use ferrite_core::wake::Outcome as WakeOutcome;

/// The first retry's wait in these tests.
const RETRY: Duration = Duration::from_millis(20);

/// How the parent reaches `pc`: at whatever API it points to, while up.
struct Link {
    api: Mutex<PathBuf>,
    up: AtomicBool,
    /// Every request sent there, as JSON lines.
    sent: Arc<Mutex<Vec<u8>>>,
}

impl Link {
    fn to(peer: &Harness) -> Arc<Self> {
        Arc::new(Self {
            api: Mutex::new(peer.api.clone()),
            up: AtomicBool::new(true),
            sent: Arc::default(),
        })
    }

    fn methods(&self) -> Vec<String> {
        String::from_utf8_lossy(&self.sent.lock().unwrap())
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .map(|request| request["method"].as_str().unwrap().to_string())
            .collect()
    }
}

/// A Server whose Settings take Delegates.
fn takes_delegates(dir: &Path, machines: bool) {
    let machines = match machines {
        true => r#","machines":[{"name":"pc","target":"me@pc"}]"#,
        false => "",
    };
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("settings.json"),
        format!(r#"{{"labs":{{"linked_machines":true}}{machines}}}"#),
    )
    .unwrap();
}

fn peer(name: &str) -> Harness {
    let harness = start(name);
    takes_delegates(&harness.dir, false);
    harness
}

/// The parent's Server, reaching `pc` through `link`.
fn reaching(link: &Arc<Link>) -> Configure {
    let link = Arc::clone(link);
    Box::new(move |server: &mut Server| {
        let settings = server.settings_dir.clone();
        let dial = move |_: &Machine| -> ferrite_core::machine::Dial {
            match link.up.load(Ordering::SeqCst) {
                true => fake_ssh(link.api.lock().unwrap().clone(), Arc::clone(&link.sent)),
                false => Box::new(|| Err(io::Error::other("no route to host"))),
            }
        };
        server.delegation =
            Delegation::with(Peers::with(settings, Box::new(dial), Box::new(Instant::now)), RETRY);
    })
}

fn parent_in(dir: PathBuf, link: &Arc<Link>) -> Harness {
    takes_delegates(&dir, true);
    serve_configured(dir, None, Tuning::default(), reaching(link))
}

/// Two Servers, the parent's Thread open in a window, and its agent.
struct World {
    peer: Harness,
    link: Arc<Link>,
    parent: Harness,
    window: Window,
    thread: ThreadId,
    agent: Sender<SessionEvent>,
}

fn world(name: &str) -> World {
    let peer = peer(&format!("{name}-peer"));
    let link = Link::to(&peer);
    let parent = parent_in(scratch(name), &link);
    let mut window = attach(&parent.address);
    let (thread, agent) = open(&mut window, &parent);
    window.watch(thread, None);
    World {
        peer,
        link,
        parent,
        window,
        thread,
        agent,
    }
}

impl World {
    /// A call as the parent's agent makes it, from inside its Thread.
    fn as_agent(&self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let mut caller = call(&self.parent);
        let request = serde_json::json!({
            "id": "1",
            "method": method,
            "params": params,
            "caller": self.thread,
        });
        link::send(&mut caller.stream, &request).unwrap();
        caller.next()
    }

    /// A call from the CLI, as the operator.
    fn operator(&self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let mut params = params;
        params["thread"] = serde_json::json!(self.thread);
        call(&self.parent).answer("1", method, params)
    }

    /// Start d1 on `pc`, and the provider of its Thread there.
    fn delegate(&self) -> (ThreadId, Sender<SessionEvent>) {
        let started = self.as_agent(
            "delegate.start",
            serde_json::json!({
                "machine": "pc",
                "task": "Fix the flaky login test.",
                "checkout": self.peer.dir,
                "title": "Login test",
                "request": "k-1",
            }),
        );
        let delegate = &started["result"]["delegate"];
        assert_eq!(delegate["delegate"], "d1", "{started}");
        assert_eq!(started["result"]["next"], ferrite_core::delegate::NEXT);
        let child = ThreadId::new(delegate["thread"].as_u64().unwrap());
        let provider = self.peer.sessions.recv_timeout(PATIENCE).unwrap();
        (child, provider)
    }

    fn d1(&self) -> serde_json::Value {
        self.operator("delegate.get", serde_json::json!({ "delegate": "d1" }))["result"]["delegate"]
            .clone()
    }

    /// Wait until d1's record says `field` is `value`.
    fn d1_until(&self, field: &str, value: serde_json::Value) -> serde_json::Value {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let d1 = self.d1();
            if d1[field] == value {
                return d1;
            }
            assert!(Instant::now() < deadline, "d1 never had {field} {value}: {d1}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// What the parent's agent was sent, once it has `count` prompts.
    fn prompts(&self, count: usize) -> Vec<String> {
        until("the parent's prompts", || self.parent.told.lock().unwrap().prompts.len() >= count);
        self.parent.told.lock().unwrap().prompts.clone()
    }

    fn attention(&self) -> serde_json::Value {
        self.operator("thread.get", serde_json::json!({}))["result"]["thread"]["attention"].clone()
    }
}

fn until(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn ends(provider: &Sender<SessionEvent>, reply: &str, outcome: ferrite_core::TurnOutcome) {
    provider.send(text(reply)).unwrap();
    provider
        .send(SessionEvent::TurnEnded {
            outcome,
            cost_usd: None,
        })
        .unwrap();
}

/// AC: a Delegate starts one Thread on `pc` with its task, capped at the
/// parent's mode and naming who delegated it; the parent counts as working
/// while it runs; and its reply wakes the parent's agent in T3's words, as
/// a wake, not the operator's prompt.
#[test]
fn a_delegates_reply_wakes_its_parent_in_t3_words() {
    let mut world = world("delegates-wake");
    let (child, provider) = world.delegate();
    let got = call(&world.peer).answer("1", "thread.get", serde_json::json!({ "thread": child }));
    let from = &got["result"]["thread"]["delegated_from"];
    assert_eq!(from["thread"], serde_json::json!(world.thread), "{got}");
    assert_eq!(
        from["server"].as_str(),
        Some(server_id::open(&world.parent.dir.join("threads")).unwrap().as_str())
    );
    assert_eq!(
        world.peer.told.lock().unwrap().caps,
        vec![Some(ferrite_core::runtime_mode::RuntimeMode::ApprovalRequired)]
    );
    until("the task there", || {
        world.peer.told.lock().unwrap().prompts == ["Fix the flaky login test."]
    });
    world.d1_until("status", "running".into());
    assert_eq!(world.attention(), "working", "an open Delegate holds its parent");

    ends(&provider, "All green.", ferrite_core::TurnOutcome::Completed);
    let woken = "Delegated task d1 (Login test) [completed]:\nAll green.";
    assert_eq!(world.prompts(1), vec![woken]);
    let thread = world.thread;
    world
        .window
        .until(|window| !wakes_in(window, thread).is_empty());
    let (note, said) = wakes_in(&world.window, thread).remove(0);
    assert_eq!(said, woken);
    assert_eq!(note.summary, "Delegated task \"Login test\" finished");
    assert_eq!(note.outcome, WakeOutcome::Completed);
    let d1 = world.d1();
    assert_eq!((&d1["status"], &d1["result"]), (&"completed".into(), &"All green.".into()));
    // The same key finds it again, and starts nothing there.
    let again = world.as_agent(
        "delegate.start",
        serde_json::json!({ "machine": "pc", "task": "again", "checkout": world.peer.dir, "request": "k-1" }),
    );
    assert_eq!(again["result"]["delegate"]["delegate"], "d1", "{again}");
    assert!(world.peer.sessions.try_recv().is_err(), "a second Thread started there");
}

/// A failed turn there fails the Delegate with its error.
#[test]
fn a_failed_turn_there_fails_the_delegate_with_its_error() {
    let world = world("delegates-failed");
    let (_, provider) = world.delegate();
    world.d1_until("status", "running".into());
    ends(
        &provider,
        "Half way.",
        ferrite_core::TurnOutcome::Error("rate limited".into()),
    );
    assert_eq!(
        world.prompts(1),
        vec!["Delegated task d1 (Login test) [failed]:\nrate limited"]
    );
}

/// AC: a cancel ends it here at once, with no wake; a stop that cannot get
/// there is owed, said in its result, and sent once `pc` answers again.
#[test]
fn a_cancel_stops_it_there_and_an_unreachable_stop_is_owed_until_it_gets_there() {
    let world = world("delegates-cancel");
    let (_, _provider) = world.delegate();
    world.d1_until("status", "running".into());
    world.link.up.store(false, Ordering::SeqCst);
    let cancelled = world.as_agent(
        "delegate.cancel",
        serde_json::json!({ "delegate": "d1", "reason": "not needed" }),
    );
    assert_eq!(
        cancelled["result"],
        serde_json::json!({ "delegate": "d1", "status": "cancel_requested" })
    );
    let d1 = world.d1_until("stop_owed", true.into());
    assert_eq!(d1["status"], "cancelled");
    world.d1_until(
        "result",
        "Cancelled: not needed It could not be stopped on pc and may still run there: pc did not answer: no route to host"
            .into(),
    );
    world.link.up.store(true, Ordering::SeqCst);
    let d1 = world.d1_until("stop_owed", false.into());
    assert_eq!(d1["result"], "Cancelled: not needed");
    assert!(world.link.methods().contains(&"thread.stop".to_string()));
    assert_eq!(world.attention(), "idle", "a cancelled Delegate holds nothing");
    assert!(world.parent.told.lock().unwrap().prompts.is_empty(), "no wake");
    let again = world.as_agent("delegate.cancel", serde_json::json!({ "delegate": "d1" }));
    assert_eq!(again["result"]["status"], "cancelled", "{again}");
    let unknown = world.as_agent("delegate.get", serde_json::json!({ "delegate": "d9" }));
    assert_eq!(unknown["error"]["code"], "unknown_delegate", "{unknown}");
}

/// The window draws what the parent's digest says of its Delegates, and its
/// stop is the agent's `delegate.cancel`: the Delegate ends at once, and
/// holds its parent no longer.
#[test]
fn the_window_reads_a_delegate_from_its_digest_and_cancels_it() {
    use ferrite_core::delegate::{DelegateId, Status};
    let mut world = world("delegates-window");
    let (child, _provider) = world.delegate();
    world.d1_until("status", "running".into());
    let thread = world.thread;
    let delegating = |window: &Window| window.digest(thread).map(|digest| digest.delegating);
    world.window.until(|window| {
        delegating(window).is_some_and(|seen| seen.delegates.first().is_some_and(|d| d.status == Status::Running))
    });
    let seen = delegating(&world.window).unwrap();
    let row = &seen.delegates[0];
    assert_eq!(
        (row.id, row.machine.as_str(), row.thread, row.title.as_deref()),
        (DelegateId::new(1), "pc", child, Some("Login test"))
    );
    assert_eq!(
        row.server,
        server_id::open(&world.peer.dir.join("threads")).unwrap(),
        "the window finds the machine that answers as it"
    );
    assert!(seen.holds, "an open one holds its parent");

    let reply = world.window.command(
        92,
        Op::CancelDelegate {
            thread,
            delegate: DelegateId::new(1),
        },
    );
    assert!(
        matches!(reply, ServerMessage::Reply { ok: Some(Done::Changed { changed: true }), .. }),
        "{reply:?}"
    );
    assert_eq!(world.d1()["status"], "cancelled");
    world.window.until(|window| {
        delegating(window).is_some_and(|seen| seen.delegates[0].status == Status::Cancelled && !seen.holds)
    });
    assert!(world.parent.told.lock().unwrap().prompts.is_empty(), "no wake");
}

/// AC: interrupting the parent's Main leaves its Delegate running (the
/// operator's rule, not T3's); parking the parent stops it there.
#[test]
fn interrupting_the_parent_leaves_its_delegate_and_parking_it_stops_it() {
    let world = world("delegates-park");
    let (_, _provider) = world.delegate();
    world.d1_until("status", "running".into());
    world.operator("agent.interrupt", serde_json::json!({}));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(world.d1()["status"], "running");

    world.operator("thread.close", serde_json::json!({}));
    let d1 = world.d1_until("stop_owed", false.into());
    assert_eq!(
        (&d1["status"], &d1["result"]),
        (&"interrupted".into(), &"Stopped with its parent.".into())
    );
    assert!(world.link.methods().contains(&"thread.stop".to_string()));
}

/// AC: a restarted Server follows its Delegate again from the record
/// alone, and the reply still wakes the parent.
#[test]
fn a_restarted_server_follows_its_delegate_from_the_record() {
    let world = world("delegates-restart");
    let (_, provider) = world.delegate();
    world.d1_until("status", "running".into());
    let World {
        peer: _peer,
        link,
        parent,
        mut window,
        agent: _agent,
        ..
    } = world;
    window.command(90, Op::Stop { when: When::Now });
    assert_eq!(parent.server.join().unwrap(), 0);
    let dir = scratch("delegates-restart-successor");
    copy_dir(&parent.dir, &dir);
    let successor = serve_configured(dir, None, Tuning::default(), reaching(&link));

    ends(&provider, "Fixed.", ferrite_core::TurnOutcome::Completed);
    // The successor starts the parent's agent for the wake.
    let _woken = successor.sessions.recv_timeout(PATIENCE).unwrap();
    until("the wake", || !successor.told.lock().unwrap().prompts.is_empty());
    assert_eq!(
        successor.told.lock().unwrap().prompts,
        vec!["Delegated task d1 (Login test) [completed]:
Fixed."]
    );
}

/// AC (G-A8): a Thread there parked once its turn ended is still read,
/// not failed. A follower's wait in flight finishes on its own link, so
/// the link drops before the turn ends and the result is asked after.
#[test]
fn a_child_parked_there_is_read_not_failed() {
    let world = world("delegates-parked-child");
    let (child, provider) = world.delegate();
    world.d1_until("status", "running".into());
    world.link.up.store(false, Ordering::SeqCst);
    ends(&provider, "Done there.", ferrite_core::TurnOutcome::Completed);
    world.d1_until("problem", "pc did not answer: no route to host".into());
    call(&world.peer).answer("c", "thread.close", serde_json::json!({ "thread": child }));
    world.link.up.store(true, Ordering::SeqCst);
    assert_eq!(
        world.prompts(1),
        vec!["Delegated task d1 (Login test) [completed]:
Done there."]
    );
}

/// AC: a machine pointed at another Server fails its Delegate, in words
/// that say so. The wait in flight ends with the turn; the next call finds
/// the other Server.
#[test]
fn a_machine_pointed_elsewhere_fails_its_delegate() {
    let world = world("delegates-elsewhere");
    let other = start("delegates-elsewhere-other");
    let (_, provider) = world.delegate();
    world.d1_until("status", "running".into());
    *world.link.api.lock().unwrap() = other.api.clone();
    ends(&provider, "Done.", ferrite_core::TurnOutcome::Completed);
    let woken = world.prompts(1);
    assert!(
        woken[0].starts_with("Delegated task d1 (Login test) [failed]:
pc now answers as the Server"),
        "{woken:?}"
    );
}

/// A Thread may start a Delegate only while this machine's Labs gate is
/// on.
#[test]
fn no_delegate_starts_while_linked_machines_is_off() {
    let world = world("delegates-gate");
    std::fs::write(
        world.parent.dir.join("settings.json"),
        r#"{"machines":[{"name":"pc","target":"me@pc"}]}"#,
    )
    .unwrap();
    let refused = world.as_agent(
        "delegate.start",
        serde_json::json!({ "machine": "pc", "task": "x", "checkout": world.peer.dir }),
    );
    assert_eq!(refused["error"]["code"], "linked_machines_off", "{refused}");
    assert!(world.link.methods().is_empty(), "nothing was sent there");
}

/// A Thread may act on another machine only while this machine's Labs
/// gate is on: nor ask the machines, nor what one runs.
#[test]
fn nothing_asks_the_machines_while_linked_machines_is_off() {
    let world = world("delegates-gate-asks");
    std::fs::write(
        world.parent.dir.join("settings.json"),
        r#"{"machines":[{"name":"pc","target":"me@pc"}]}"#,
    )
    .unwrap();
    for (method, params) in [
        ("machine.list", serde_json::json!({ "reach": true })),
        ("provider.list", serde_json::json!({ "machine": "pc" })),
    ] {
        let refused = world.as_agent(method, params);
        assert_eq!(
            (&refused["error"]["code"], &refused["error"]["message"]),
            (
                &"linked_machines_off".into(),
                &"Linked machines is off on this machine, so no tool can act on another machine. \
                  The user can turn it on in Settings → Labs."
                    .into()
            )
        );
    }
    assert!(world.link.methods().is_empty(), "nothing was sent there");
}

/// AC: a machine with Linked machines off refuses the Delegate's Thread,
/// and the start says so in T3's words.
#[test]
fn a_machine_with_linked_machines_off_takes_no_delegate() {
    let world = world("delegates-gate-there");
    std::fs::write(world.peer.dir.join("settings.json"), "{}").unwrap();
    let refused = world.as_agent(
        "delegate.start",
        serde_json::json!({ "machine": "pc", "task": "x", "checkout": world.peer.dir }),
    );
    assert_eq!(
        (&refused["error"]["code"], &refused["error"]["message"]),
        (
            &"linked_machines_off".into(),
            &"pc has Linked machines turned off. Turn it on in its Settings → Labs.".into()
        )
    );
}

/// `root` is a checkout whose primary remote is `url`.
fn remote(root: &Path, url: &str) {
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::write(
        root.join(".git/config"),
        format!("[remote \"origin\"]\n\turl = {url}\n"),
    )
    .unwrap();
}

/// A Project on `harness`'s Server whose repository is `url`, in `dir`:
/// its id and directory.
fn project_of(harness: &Harness, dir: &Path, url: &str) -> (String, PathBuf) {
    use ferrite_core::workspace::git_for_tests as git;
    let repo = repo_in(dir);
    git(&repo, &["remote", "add", "origin", url]);
    let opened = call(harness).answer("p", "project.open", serde_json::json!({ "cwd": repo }));
    let id = opened["result"]["project"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("{opened}"));
    (id.to_string(), dunce::canonicalize(&repo).unwrap())
}

impl World {
    /// The Thread there of a start answered `started`: its `thread.get`.
    fn child(&self, started: &serde_json::Value) -> serde_json::Value {
        let child = &started["result"]["delegate"]["thread"];
        assert!(child.is_u64(), "{started}");
        let got = call(&self.peer).answer("1", "thread.get", serde_json::json!({ "thread": child }));
        got["result"]["thread"].clone()
    }
}

/// AC: a start that names no checkout works in the Project there whose
/// repository is its parent's, however each machine spells the remote;
/// one that names a Project works in its directory.
#[test]
fn a_delegate_works_in_the_project_there_with_its_parents_repository() {
    let world = world("delegates-repository");
    remote(&world.parent.dir, "git@github.com:Acme/App.git");
    let (project, directory) =
        project_of(&world.peer, &world.peer.dir, "https://github.com/acme/app");
    let started = world.as_agent(
        "delegate.start",
        serde_json::json!({ "machine": "pc", "task": "Fix it.", "request": "k-1" }),
    );
    let child = world.child(&started);
    assert_eq!(
        (child["cwd"].as_str().map(PathBuf::from), &child["project"]),
        (Some(directory.clone()), &serde_json::json!(project))
    );
    let _first = world.peer.sessions.recv_timeout(PATIENCE).unwrap();
    let named = world.as_agent(
        "delegate.start",
        serde_json::json!({ "machine": "pc", "task": "And this.", "project": project, "request": "k-2" }),
    );
    let child = world.child(&named);
    assert_eq!(child["cwd"].as_str().map(PathBuf::from), Some(directory));
}

/// AC: with no one Project there to work in, a start is refused in T3's
/// words before any Thread starts there.
#[test]
fn a_delegate_with_no_one_project_there_is_refused() {
    let world = world("delegates-no-project");
    let start = || {
        world.as_agent(
            "delegate.start",
            serde_json::json!({ "machine": "pc", "task": "x" }),
        )["error"]
            .clone()
    };
    let refused = start();
    assert_eq!(
        (&refused["code"], &refused["message"]),
        (
            &"target_required".into(),
            &"Pass `project`: this Thread's project has no repository to match there.".into()
        )
    );
    remote(&world.parent.dir, "https://github.com/acme/app.git");
    let refused = start();
    assert_eq!(refused["code"], "project_not_found", "{refused}");
    let message = refused["message"].as_str().unwrap();
    assert!(
        message.starts_with(
            "No project there has this repository (github.com/acme/app). Pass `project`; \
             `project.list` with this `machine` lists them."
        ),
        "{refused}"
    );
    let (one, _) = project_of(&world.peer, &world.peer.dir.join("one"), "git@github.com:acme/app");
    let (two, _) = project_of(&world.peer, &world.peer.dir.join("two"), "git@github.com:acme/app");
    let refused = start();
    let message = refused["message"].as_str().unwrap();
    assert!(
        message.starts_with("Several projects there have this repository: ")
            && message.contains(&format!("({one})"))
            && message.contains(&format!("({two})")),
        "{refused}"
    );
    assert!(!world.link.methods().contains(&"thread.start".to_string()));
}

/// AC: the model, effort and mode asked are what its Thread there runs,
/// and the record says so; a mode broader than the parent's, or another
/// provider with no model, is refused before anything starts there.
#[test]
fn a_delegate_runs_the_model_effort_and_mode_asked() {
    let world = world("delegates-model");
    let started = world.as_agent(
        "delegate.start",
        serde_json::json!({
            "machine": "pc", "task": "x", "checkout": world.peer.dir,
            "model": "opus", "effort": "high", "mode": "approval_required",
        }),
    );
    let delegate = &started["result"]["delegate"];
    assert_eq!(
        (&delegate["provider"], &delegate["model"]),
        (&"claude".into(), &"opus".into()),
        "{started}"
    );
    let _provider = world.peer.sessions.recv_timeout(PATIENCE).unwrap();
    {
        let told = world.peer.told.lock().unwrap();
        let asked = ["--model", "opus", "--effort", "high"].map(String::from);
        assert!(told.args[0].ends_with(&asked), "{:?}", told.args);
        assert_eq!(
            told.caps,
            vec![Some(ferrite_core::runtime_mode::RuntimeMode::ApprovalRequired)]
        );
    }
    let broader = world.as_agent(
        "delegate.start",
        serde_json::json!({ "machine": "pc", "task": "x", "checkout": world.peer.dir, "mode": "full_access" }),
    );
    assert_eq!(broader["error"]["code"], "widens_cap", "{broader}");
    let codex = world.as_agent(
        "delegate.start",
        serde_json::json!({ "machine": "pc", "task": "x", "checkout": world.peer.dir, "provider": "codex" }),
    );
    assert_eq!(
        (&codex["error"]["code"], &codex["error"]["message"]),
        (
            &"target_required".into(),
            &"Pass `model` from `provider.list` with this `machine`.".into()
        ),
        "{codex}"
    );
    let methods = world.link.methods();
    let starts = methods.iter().filter(|method| *method == "thread.start").count();
    assert_eq!(starts, 1);
}

/// AC: `machine.list` with `reach` says whether each machine answers now,
/// and `provider.list` of a machine is that machine's own; each in T3's
/// words when it cannot be.
#[test]
fn the_linked_machines_are_asked_whether_they_answer_and_what_they_run() {
    let world = world("delegates-reach");
    let listed = world.as_agent("machine.list", serde_json::json!({ "reach": true }));
    assert_eq!(listed["result"]["machines"][0]["status"], "reachable", "{listed}");
    let providers = world.as_agent("provider.list", serde_json::json!({ "machine": "pc" }));
    assert_eq!(providers["result"]["providers"][1]["provider"], "codex", "{providers}");
    let unasked = world.as_agent("machine.list", serde_json::json!({}));
    assert_eq!(
        unasked["result"]["machines"][0].get("status"),
        None,
        "without reach, nothing is asked"
    );
    world.link.up.store(false, Ordering::SeqCst);
    let listed = world.as_agent("machine.list", serde_json::json!({ "reach": true }));
    let pc = &listed["result"]["machines"][0];
    assert_eq!(
        (&pc["status"], &pc["problem"]),
        (&"unreachable".into(), &"pc did not answer: no route to host".into())
    );
    let unreached = world.as_agent("provider.list", serde_json::json!({ "machine": "pc" }));
    assert_eq!(unreached["error"]["message"], "pc did not answer: no route to host");
    let unknown = world.as_agent("provider.list", serde_json::json!({ "machine": "vps" }));
    assert_eq!(
        unknown["error"]["message"],
        "This machine is not linked to vps. `machine.list` lists the linked ones."
    );
}

/// AC: a Delegate that names no model runs the one its parent's Session
/// runs, as its provider announced it, though the parent was started with
/// none named; `delegate.defaults` says so.
#[test]
fn a_delegate_runs_the_model_its_parent_runs() {
    let world = world("delegates-running-model");
    let model = "claude-haiku-4-5";
    world
        .agent
        .send(SessionEvent::ModelChanged {
            model: model.into(),
        })
        .unwrap();
    let defaults = || {
        world.as_agent("delegate.defaults", serde_json::json!({ "thread": world.thread }))
            ["result"]
            .clone()
    };
    until("the parent's model", || defaults()["model"] == model);
    assert_eq!(defaults()["provider"], "claude");
    world.as_agent(
        "delegate.start",
        serde_json::json!({ "machine": "pc", "task": "x", "checkout": world.peer.dir }),
    );
    let _provider = world.peer.sessions.recv_timeout(PATIENCE).unwrap();
    let told = world.peer.told.lock().unwrap();
    let asked = ["--model", model].map(String::from);
    assert!(told.args[0].ends_with(&asked), "{:?}", told.args);
}

/// AC: the agent's own Delegate tools start one on `pc` in the Project
/// with its repository, answer T3's `next`, and read its result once it
/// has woken the agent.
#[test]
fn the_agents_own_tools_delegate_to_a_linked_machine() {
    let world = world("delegates-tools");
    remote(&world.parent.dir, "git@github.com:acme/app.git");
    project_of(&world.peer, &world.peer.dir, "https://github.com/acme/app");
    let tools = world.parent.tools.for_thread(world.thread, None, true);
    let use_tool = |tool: &str, arguments: serde_json::Value| {
        let (told, said) = mpsc::channel();
        tools.call(tool, &arguments, true, move |answer| {
            told.send((answer.text, answer.is_error)).unwrap()
        });
        let (text, error) = said.recv_timeout(PATIENCE).unwrap();
        assert!(!error, "{tool}: {text}");
        serde_json::from_str::<serde_json::Value>(&text).unwrap()
    };
    let machines = use_tool("machine_list", serde_json::json!({}));
    assert_eq!(machines["links"][0]["status"], "reachable", "{machines}");
    let capabilities = use_tool("orchestrator_capabilities", serde_json::json!({}));
    assert_eq!(
        capabilities["inheritedProviderInstanceId"], "claude",
        "{capabilities}"
    );
    let there = use_tool(
        "orchestrator_capabilities",
        serde_json::json!({ "machineId": "pc" }),
    );
    assert_eq!(there["providers"][1]["driverKind"], "codex", "{there}");
    let projects = use_tool("project_list", serde_json::json!({ "machineId": "pc" }));
    assert_eq!(
        projects["projects"][0]["repositoryIdentity"]["canonicalKey"],
        "github.com/acme/app",
        "{projects}"
    );
    let task = use_tool(
        "delegate_task",
        serde_json::json!({
            "task": "Fix the flaky login test.",
            "title": "Login test",
            "target": { "machineId": "pc" },
        }),
    );
    assert_eq!(
        (&task["taskId"], &task["next"]),
        (&"d1".into(), &ferrite_core::delegate::NEXT.into()),
        "{task}"
    );
    let provider = world.peer.sessions.recv_timeout(PATIENCE).unwrap();
    world.d1_until("status", "running".into());
    ends(&provider, "All green.", ferrite_core::TurnOutcome::Completed);
    assert_eq!(
        world.prompts(1),
        vec!["Delegated task d1 (Login test) [completed]:\nAll green."]
    );
    let status = use_tool("task_status", serde_json::json!({ "taskId": "d1" }));
    assert_eq!(
        (&status["workState"], &status["summary"]),
        (&"result_available".into(), &"All green.".into())
    );
}
