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
    assert_eq!(note.summary, "Delegated task \"Login test\" completed");
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
        "Cancelled: not needed It could not be stopped on pc and may still run there: pc could not be reached: no route to host"
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
    world.d1_until("problem", "pc could not be reached: no route to host".into());
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
