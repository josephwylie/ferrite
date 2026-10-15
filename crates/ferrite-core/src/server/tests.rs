//! The Server over its socket, with scripted Sessions: Decisions answered
//! by two windows, and a window that reconnects resuming from its ring or
//! rebuilding from history.

use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Instant;

use super::*;
use crate::cockpit::mirror::Mirrors;
use crate::engine::{SpawnRequest, Spawner};
use crate::link::{Attached, Client};
use crate::protocol::{Peer, Resume, WireAnswer, WireChoice};
use crate::providers::Session;
use crate::store::Provider;
use crate::transcript::Body;
use crate::{Decision, DecisionAnswer, SessionEvent};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fs-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

type Answers = Arc<Mutex<Vec<(String, DecisionAnswer)>>>;
/// The client ids of the prompts handed to the providers' queues.
type Queued = Arc<Mutex<Vec<String>>>;

struct Scripted {
    events: Receiver<SessionEvent>,
    answers: Answers,
    queued: Queued,
}

impl Session for Scripted {
    fn events(&self) -> &Receiver<SessionEvent> {
        &self.events
    }
    fn send(&mut self, _: &str) -> io::Result<()> {
        Ok(())
    }
    fn interrupt(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn respond_to_decision(&mut self, id: &str, answer: DecisionAnswer) -> io::Result<()> {
        self.answers.lock().unwrap().push((id.into(), answer));
        Ok(())
    }
    /// Takes every prompt, as a provider that confirmed native queuing.
    fn enqueue(&mut self, client_id: &str, _: &str) -> io::Result<()> {
        self.queued.lock().unwrap().push(client_id.into());
        Ok(())
    }
}

/// Each Session it starts hands the test the Sender that plays its provider.
struct Spawn {
    sessions: Sender<Sender<SessionEvent>>,
    answers: Answers,
    queued: Queued,
    /// Sessions start only as the test lets each through; at once without.
    gate: Option<Arc<Mutex<Receiver<()>>>>,
}

impl Spawn {
    fn scripted(&self) -> Scripted {
        let (provider, events) = mpsc::channel();
        self.sessions.send(provider).unwrap();
        Scripted {
            events,
            answers: self.answers.clone(),
            queued: self.queued.clone(),
        }
    }
}

impl Spawner for Spawn {
    fn spawn(&mut self, _: SpawnRequest) -> io::Result<Box<dyn crate::providers::Session>> {
        Ok(Box::new(self.scripted()))
    }
    fn start(&mut self, request: SpawnRequest) -> io::Result<crate::session::SessionLifecycle> {
        use crate::session::SessionLifecycle;
        let Some(gate) = self.gate.clone() else {
            return self.spawn(request).map(SessionLifecycle::ready);
        };
        let session = self.scripted();
        SessionLifecycle::background(move || {
            let _ = gate.lock().unwrap().recv();
            Ok(Box::new(session) as Box<dyn crate::providers::Session + Send>)
        })
    }
}

struct Harness {
    server: std::thread::JoinHandle<i32>,
    /// Every executable the Server restarted into, in place of `exec`.
    relaunched: Receiver<PathBuf>,
    dir: PathBuf,
    address: PathBuf,
    /// The API's socket.
    api: PathBuf,
    sessions: Receiver<Sender<SessionEvent>>,
    answers: Answers,
    queued: Queued,
}

fn start(name: &str) -> Harness {
    serve(name, None)
}

/// A Server whose Sessions start only as the returned Sender lets each
/// through, one `()` each.
fn start_gated(name: &str) -> (Harness, Sender<()>) {
    let (gate, opens) = mpsc::channel();
    (serve(name, Some(Arc::new(Mutex::new(opens)))), gate)
}

fn serve(name: &str, gate: Option<Arc<Mutex<Receiver<()>>>>) -> Harness {
    let dir = scratch(name);
    let store = dir.join("threads");
    let address = link::address(&store);
    let listener = link::listen(&address).unwrap();
    let api = crate::api::address(&store);
    let api_listener = link::listen(&api).unwrap();
    let (sessions, started) = mpsc::channel();
    let answers = Answers::default();
    let queued = Queued::default();
    let spawn = Spawn {
        sessions,
        answers: answers.clone(),
        queued: queued.clone(),
        gate,
    };
    let settings_dir = dir.clone();
    let (relaunches, relaunched) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let engine = Engine::try_new(Store::open(&store).unwrap(), Box::new(spawn)).unwrap();
        let info = ServerInfo {
            version: "0.9.0".into(),
            executable: "/bin/ferrite".into(),
        };
        let (_, renders) = render::remote();
        let defaults = Arc::new(Mutex::new(SessionDefaults::default()));
        let mut server = Server::new(engine, info, renders, defaults, settings_dir);
        server.relaunch = Box::new(move |executable: &Path| {
            relaunches.send(executable.to_owned()).unwrap();
            Ok(())
        });
        let (heard, hear) = mpsc::channel();
        crate::api::accept_all(api_listener, heard.clone()).unwrap();
        link::accept_all(listener, heard).unwrap();
        server.serve(hear)
    });
    Harness {
        server,
        relaunched,
        dir,
        address,
        api,
        sessions: started,
        answers,
        queued,
    }
}

/// A window's end: the link and the mirrors it builds.
struct Window {
    client: Client,
    boot_id: String,
    mirrors: Mirrors,
    replies: Vec<ServerMessage>,
    /// The `decision` and `decision_closed` messages, in order.
    decisions: Vec<ServerMessage>,
}

fn attach(address: &Path) -> Window {
    let Attached {
        client, snapshot, ..
    } = Client::attach(
        address,
        ClientMessage::Hello {
            generation: 1,
            client: Peer {
                name: "test".into(),
                version: "0".into(),
            },
            capabilities: Vec::new(),
        },
    )
    .unwrap();
    let mut window = Window {
        client,
        boot_id: snapshot.boot_id.clone(),
        mirrors: Mirrors::default(),
        replies: Vec::new(),
        decisions: Vec::new(),
    };
    for state in snapshot.threads {
        window.receive(ServerMessage::Thread { revision: 0, state });
    }
    window
}

impl Window {
    fn receive(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::Reply { .. } => self.replies.push(message),
            ServerMessage::Decision { .. } | ServerMessage::DecisionClosed { .. } => {
                self.decisions.push(message)
            }
            message => self.mirrors.receive(message, Instant::now()),
        }
    }

    /// Why each Decision this window was told of closed.
    fn closed(&self) -> Vec<ClosedBy> {
        self.decisions
            .iter()
            .filter_map(|message| match message {
                ServerMessage::DecisionClosed { by, .. } => Some(by.clone()),
                _ => None,
            })
            .collect()
    }

    /// Take what arrives until `done` holds, for at most five seconds.
    fn until(&mut self, done: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(self) {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "timed out");
            if let Ok(message) = self.client.messages.recv_timeout(left) {
                self.receive(message);
            }
        }
    }

    fn command(&mut self, id: u64, op: Op) -> ServerMessage {
        self.client
            .send(&ClientMessage::Command(Command { id, op }))
            .unwrap();
        self.until(|window| {
            window
                .replies
                .iter()
                .any(|reply| matches!(reply, ServerMessage::Reply { id: got, .. } if *got == id))
        });
        let at = self
            .replies
            .iter()
            .position(|reply| matches!(reply, ServerMessage::Reply { id: got, .. } if *got == id))
            .unwrap();
        self.replies.remove(at)
    }

    fn watch(&mut self, thread: ThreadId, resume: Option<Resume>) {
        self.client
            .send(&ClientMessage::Watch { thread, resume })
            .unwrap();
    }

    fn said(&self, thread: ThreadId) -> Vec<String> {
        self.mirrors.view(thread).map_or(Vec::new(), |view| {
            view.transcript()
                .blocks()
                .iter()
                .filter(|block| !matches!(block.body, Body::Meta(_)))
                .map(|block| format!("{:?}", block.body))
                .collect()
        })
    }
}

fn open(window: &mut Window, harness: &Harness) -> (ThreadId, Sender<SessionEvent>) {
    let reply = window.command(
        1,
        Op::Open {
            provider: Provider::Claude,
            workspace: WireChoice::Main {
                checkout: harness.dir.clone(),
            },
        },
    );
    let ServerMessage::Reply {
        ok: Some(Done::Thread { thread }),
        ..
    } = reply
    else {
        panic!("open failed: {reply:?}");
    };
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    (thread, provider)
}

fn decision() -> SessionEvent {
    SessionEvent::DecisionRequested {
        decision: Decision {
            delivery: Default::default(),
            kind: Default::default(),
            policy: Default::default(),
            id: "perm_01".into(),
            tool_use_id: "toolu_01".into(),
            tool_name: "Write".into(),
            description: "ferrite-perm.txt".into(),
            input: serde_json::json!({}),
            suggestions: vec![],
        },
    }
}

/// The provider exits, as a real adapter reports it.
fn exit(provider: Sender<SessionEvent>) {
    provider
        .send(SessionEvent::Closed {
            reason: "exited".into(),
        })
        .unwrap();
}

fn text(text: &str) -> SessionEvent {
    SessionEvent::TextDelta { text: text.into() }
}

/// Two windows answer one Decision: the first answer reaches the provider,
/// the second is told it was already answered, and both see it close.
#[test]
fn the_first_answer_wins_and_the_second_hears_already_answered() {
    let harness = start("answers");
    let mut first = attach(&harness.address);
    let mut second = attach(&harness.address);
    let (thread, provider) = open(&mut first, &harness);
    first.watch(thread, None);
    second.watch(thread, None);
    provider.send(decision()).unwrap();
    let pending = |window: &Window| {
        window
            .mirrors
            .view(thread)
            .is_some_and(|view| view.pending().is_some())
    };
    first.until(pending);
    second.until(pending);
    let handle: protocol::WireHandle =
        (&first.mirrors.view(thread).unwrap().activity().decisions()[0].handle).into();
    let answer = |handle: &protocol::WireHandle| Op::Answer {
        thread,
        handle: handle.clone(),
        answer: WireAnswer::Allow {
            input: serde_json::json!({}),
        },
    };
    let won = first.command(10, answer(&handle));
    let lost = second.command(20, answer(&handle));

    assert!(
        matches!(won, ServerMessage::Reply { error: None, .. }),
        "{won:?}"
    );
    assert!(
        matches!(&lost, ServerMessage::Reply { error: Some(error), .. }
            if error.code == ErrorCode::AlreadyAnswered),
        "{lost:?}"
    );
    assert_eq!(
        harness.answers.lock().unwrap().len(),
        1,
        "never answered twice"
    );
    first.until(|window| !pending(window));
    second.until(|window| !pending(window));
    for window in [&mut first, &mut second] {
        window.until(|window| !window.closed().is_empty());
        assert!(matches!(
            &window.decisions[0],
            ServerMessage::Decision { handle: asked, .. } if *asked == handle
        ));
        // The first window to attach is the Server's client 1.
        assert_eq!(window.closed(), vec![ClosedBy::Answered { client: 1 }]);
    }
}

/// The handle of the Decision `window` sees pending on `thread`.
fn pending_handle(window: &Window, thread: ThreadId) -> protocol::WireHandle {
    (&window.mirrors.view(thread).unwrap().activity().decisions()[0].handle).into()
}

fn allow(thread: ThreadId, handle: &protocol::WireHandle) -> Op {
    Op::Answer {
        thread,
        handle: handle.clone(),
        answer: WireAnswer::Allow {
            input: serde_json::json!({}),
        },
    }
}

fn refused_with(reply: &ServerMessage, code: ErrorCode) -> bool {
    matches!(reply, ServerMessage::Reply { error: Some(error), .. } if error.code == code)
}

/// An answer that does not fit its Decision is refused `invalid_answer`
/// and never reaches the provider; the Decision stays pending, and the
/// next valid answer wins.
#[test]
fn an_invalid_answer_leaves_the_decision_pending() {
    let harness = start("invalid");
    let mut window = attach(&harness.address);
    let (thread, provider) = open(&mut window, &harness);
    window.watch(thread, None);
    provider.send(decision()).unwrap();
    window.until(|window| window.decisions.len() == 1);
    let handle = pending_handle(&window, thread);

    let reply = window.command(
        10,
        Op::Answer {
            thread,
            handle: handle.clone(),
            answer: WireAnswer::Form {
                values: serde_json::json!({}),
            },
        },
    );
    assert!(refused_with(&reply, ErrorCode::InvalidAnswer), "{reply:?}");
    assert!(harness.answers.lock().unwrap().is_empty());
    let reply = window.command(11, allow(thread, &handle));
    assert!(
        matches!(reply, ServerMessage::Reply { error: None, .. }),
        "{reply:?}"
    );
    assert_eq!(harness.answers.lock().unwrap().len(), 1);
    window.until(|window| !window.closed().is_empty());
    assert_eq!(window.closed(), vec![ClosedBy::Answered { client: 1 }]);
}

/// A request id is unique only within one provider process: an answer to
/// a replaced Session's Decision is refused even when its successor asks
/// with the same id, and the successor's own Decision stays pending.
#[test]
fn an_answer_to_a_replaced_sessions_decision_is_refused() {
    let harness = start("replaced");
    let mut window = attach(&harness.address);
    let (thread, provider) = open(&mut window, &harness);
    window.watch(thread, None);
    provider.send(decision()).unwrap();
    window.until(|window| window.decisions.len() == 1);
    let replaced = pending_handle(&window, thread);
    exit(provider);
    window.until(|window| !window.closed().is_empty());

    window.command(
        2,
        Op::Send {
            thread,
            text: "again".into(),
        },
    );
    let successor = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    successor.send(decision()).unwrap();
    window.until(|window| window.decisions.len() == 3);
    let current = pending_handle(&window, thread);
    assert_eq!(current.request_id, replaced.request_id);
    assert_ne!(current, replaced);

    let reply = window.command(10, allow(thread, &replaced));
    assert!(
        refused_with(&reply, ErrorCode::AlreadyAnswered),
        "{reply:?}"
    );
    assert!(harness.answers.lock().unwrap().is_empty());
    let reply = window.command(11, allow(thread, &current));
    assert!(
        matches!(reply, ServerMessage::Reply { error: None, .. }),
        "{reply:?}"
    );
}

/// A Decision pending when its Session ends closes for every window as
/// `session_ended`; one pending when the Server stops, as `server_restart`.
#[test]
fn a_decision_closes_for_every_window_with_why() {
    let harness = start("closes");
    let mut window = attach(&harness.address);
    let (_, provider) = open(&mut window, &harness);
    provider.send(decision()).unwrap();
    window.until(|window| window.decisions.len() == 1);
    exit(provider);
    window.until(|window| !window.closed().is_empty());
    assert_eq!(window.closed(), vec![ClosedBy::SessionEnded]);

    let (_, provider) = open(&mut window, &harness);
    provider.send(decision()).unwrap();
    window.until(|window| window.decisions.len() == 3);
    window.command(3, Op::Stop { when: When::Now });
    window.until(|window| window.closed().len() == 2);
    assert_eq!(window.closed()[1], ClosedBy::ServerRestart);
    assert_eq!(harness.server.join().unwrap(), 0);
}

/// A Thread whose log outweighs how far a window may fall behind still
/// reaches a window that starts watching it: the history is read on the
/// connection's own thread and crosses in pieces that never count as lag.
#[test]
fn a_history_larger_than_the_lag_limit_still_arrives() {
    let harness = start("large");
    let mut steady = attach(&harness.address);
    let (thread, provider) = open(&mut steady, &harness);
    steady.watch(thread, None);
    let ends_with = |said: String| {
        move |window: &Window| {
            window.mirrors.view(thread).is_some_and(|view| {
                view.transcript()
                    .blocks()
                    .last()
                    .is_some_and(|block| format!("{:?}", block.body).contains(&said))
            })
        }
    };
    let turn = "x".repeat(1 << 20);
    let turns = link::LAGGED_BYTES / turn.len() + 4;
    for nth in 0..turns {
        provider.send(text(&format!("{turn} turn {nth}"))).unwrap();
        provider
            .send(SessionEvent::TurnEnded {
                outcome: crate::TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
        // One turn at a time, so the steady window never lags either.
        steady.until(ends_with(format!("turn {nth}")));
    }

    let mut late = attach(&harness.address);
    late.watch(thread, None);
    late.until(ends_with(format!("turn {}", turns - 1)));
    assert!(late.replies.is_empty(), "{:?}", late.replies);
    let reply = late.command(4, Op::SettingsChanged);
    assert!(matches!(reply, ServerMessage::Reply { error: None, .. }));
}

/// A window that drops mid-turn and comes back within this boot resumes
/// from the ring: its transcript then reads as an uninterrupted window's.
#[test]
fn a_resume_within_the_ring_reads_as_if_never_gone() {
    let harness = start("resume");
    let mut steady = attach(&harness.address);
    let (thread, provider) = open(&mut steady, &harness);
    steady.watch(thread, None);
    let mut leaving = attach(&harness.address);
    leaving.watch(thread, None);
    provider.send(text("before ")).unwrap();
    let heard = |text: &'static str| {
        move |window: &Window| window.said(thread).iter().any(|said| said.contains(text))
    };
    leaving.until(heard("before"));

    let Window {
        mut mirrors,
        boot_id,
        ..
    } = leaving;
    provider.send(text("while away")).unwrap();
    steady.until(heard("while away"));
    let mut back = attach(&harness.address);
    std::mem::swap(&mut back.mirrors, &mut mirrors);
    let after = back.mirrors.seq(thread).unwrap();
    back.watch(thread, Some(Resume { boot_id, after }));
    back.until(heard("while away"));

    assert_eq!(back.said(thread), steady.said(thread));
}

/// A resume into another boot (or past the ring) cannot be met: the
/// window rebuilds the Thread from its history and the live state.
#[test]
fn a_resume_the_server_cannot_meet_rebuilds_from_history() {
    let harness = start("rebuild");
    let mut steady = attach(&harness.address);
    let (thread, provider) = open(&mut steady, &harness);
    steady.watch(thread, None);
    provider.send(text("from the log")).unwrap();
    provider
        .send(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    provider.send(decision()).unwrap();
    steady.until(|window| {
        window
            .mirrors
            .view(thread)
            .is_some_and(|view| view.pending().is_some())
    });

    let mut back = attach(&harness.address);
    back.watch(
        thread,
        Some(Resume {
            boot_id: "another boot".into(),
            after: 3,
        }),
    );
    back.until(|window| {
        window
            .mirrors
            .view(thread)
            .is_some_and(|view| view.pending().is_some())
    });

    assert_eq!(back.said(thread), steady.said(thread));
    assert!(back
        .said(thread)
        .iter()
        .any(|said| said.contains("from the log")));
}

/// A Thread closed and opened again while a window was away is a new
/// Activity: the window's `seq` from before cannot resume into it.
#[test]
fn a_resume_never_crosses_a_reopened_thread() {
    let harness = start("reopen");
    let mut window = attach(&harness.address);
    let (thread, provider) = open(&mut window, &harness);
    window.watch(thread, None);
    provider.send(text("first life")).unwrap();
    window.until(|window| {
        window
            .said(thread)
            .iter()
            .any(|said| said.contains("first life"))
    });
    let Window {
        mut mirrors,
        boot_id,
        ..
    } = window;
    let after = mirrors.seq(thread).unwrap();

    let mut other = attach(&harness.address);
    other.command(2, Op::Park { thread });
    other.command(3, Op::Revive { thread });
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let dots = ".".repeat(after as usize + 2);
    for _ in 0..dots.len() {
        provider.send(text(".")).unwrap();
    }
    other.watch(thread, None);
    other.until(|window| window.said(thread).iter().any(|said| said.contains(&dots)));

    let mut back = attach(&harness.address);
    std::mem::swap(&mut back.mirrors, &mut mirrors);
    back.watch(thread, Some(Resume { boot_id, after }));
    back.until(|window| window.said(thread) == other.said(thread));
}

#[test]
fn a_command_this_server_does_not_know_is_answered_unsupported() {
    let harness = start("unknown");
    let mut window = attach(&harness.address);
    let reply = window.command(5, Op::Unknown);
    assert!(
        matches!(&reply, ServerMessage::Reply { error: Some(error), .. }
            if error.code == ErrorCode::Unsupported),
        "{reply:?}"
    );
}

#[test]
fn stop_now_ends_the_server_cleanly() {
    let harness = start("stop");
    let mut window = attach(&harness.address);
    let reply = window.command(9, Op::Stop { when: When::Now });
    assert!(matches!(reply, ServerMessage::Reply { error: None, .. }));
    window.until(|window| window.replies.is_empty());
    assert_eq!(harness.server.join().unwrap(), 0);
}

/// `restart {when: idle}` waits for the agents: not while a turn runs,
/// and then once — windows told first, the Sessions ended, the successor
/// started, and this Server done.
#[test]
fn a_restart_when_idle_waits_for_the_turn_and_happens_once() {
    let harness = start("restart");
    let mut window = attach(&harness.address);
    let (thread, provider) = open(&mut window, &harness);
    window.watch(thread, None);
    window.command(
        2,
        Op::Send {
            thread,
            text: "go".into(),
        },
    );
    provider.send(text("working")).unwrap();
    window.until(|window| {
        window
            .said(thread)
            .iter()
            .any(|said| said.contains("working"))
    });

    let reply = window.command(3, Op::Restart { when: When::Idle });
    assert!(
        matches!(reply, ServerMessage::Reply { error: None, .. }),
        "{reply:?}"
    );
    assert!(
        harness
            .relaunched
            .recv_timeout(Duration::from_millis(300))
            .is_err(),
        "not mid-turn"
    );

    provider
        .send(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    let successor = harness
        .relaunched
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert_eq!(successor, std::env::current_exe().unwrap());
    let restarting = loop {
        match window.client.messages.recv_timeout(Duration::from_secs(5)) {
            Ok(ServerMessage::Restarting { when }) => break when,
            Ok(_) => continue,
            Err(error) => panic!("no restarting: {error}"),
        }
    };
    assert_eq!(restarting, When::Idle);
    assert_eq!(harness.server.join().unwrap(), 0);
    assert!(harness.relaunched.try_recv().is_err(), "restarted once");
}

#[test]
fn only_a_newer_window_running_the_same_executable_asks_for_a_restart() {
    let exe = std::env::current_exe().unwrap();
    let server = |version: &str, executable: &Path| ServerInfo {
        version: version.into(),
        executable: executable.to_owned(),
    };
    assert!(wants_restart("0.10.0", &exe, &server("0.9.3", &exe)));
    assert!(
        !wants_restart("0.9.3", &exe, &server("0.9.3", &exe)),
        "equal never"
    );
    assert!(
        !wants_restart("0.9.0", &exe, &server("0.10.0", &exe)),
        "older never"
    );
    assert!(
        !wants_restart(
            "0.10.0",
            &exe,
            &server(
                "0.9.3",
                Path::new("/Applications/Ferrite.app/Contents/MacOS/ferrite")
            )
        ),
        "another install's Server restarts into itself"
    );
    assert!(!wants_restart("garbage", &exe, &server("0.9.3", &exe)));
    assert!(wants_restart("1.0.0-beta.1", &exe, &server("0.9.9", &exe)));
}

/// What a window finds on its own; a test's finds nothing.
struct NoDiscovery;

impl Spawner for NoDiscovery {
    fn spawn(&mut self, _: SpawnRequest) -> io::Result<Box<dyn crate::providers::Session>> {
        Err(io::Error::other("a window spawns no Session"))
    }
}

/// A Cockpit attached to the harness's Server, as the app attaches.
fn cockpit(harness: &Harness) -> crate::cockpit::Cockpit {
    crate::cockpit::Cockpit::attach(crate::cockpit::Attach {
        store: harness.dir.join("threads"),
        server: crate::cockpit::Endpoint::Local {
            executable: "/bin/false".into(),
        },
        client: Peer {
            name: "test".into(),
            version: "0".into(),
        },
        renderer: None,
        discovery: Box::new(NoDiscovery),
    })
    .unwrap()
}

/// Pump a window until `done` holds, for at most five seconds.
fn pump_until(
    window: &mut crate::cockpit::Cockpit,
    done: impl Fn(&crate::cockpit::Cockpit) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(window) {
        assert!(Instant::now() < deadline, "timed out");
        window.pump();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn shows(window: &crate::cockpit::Cockpit, thread: ThreadId, said: &str) -> bool {
    window.thread(thread).is_some_and(|open| {
        open.transcript()
            .blocks()
            .iter()
            .any(|block| format!("{:?}", block.body).contains(said))
    })
}

/// The app's window over the socket: its acts are commands, what it reads
/// is what the Server's messages built, and a second window sees the same
/// Threads. Of two answers to one Decision only the first is delivered.
#[test]
fn a_window_attached_over_the_socket_drives_the_servers_threads() {
    let harness = start("cockpit");
    let mut window = cockpit(&harness);
    assert!(matches!(
        window.server(),
        Some(crate::cockpit::ServerState::Attached { .. })
    ));
    let thread = window
        .open(
            Provider::Claude,
            crate::workspace::WorkspaceChoice::Main {
                checkout: harness.dir.clone(),
            },
        )
        .unwrap();
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert!(window
        .visible()
        .contains(&crate::roster::PaneIdentity::Thread(thread)));
    window.send(thread, "go".into());
    provider.send(text("on it")).unwrap();
    pump_until(&mut window, |window| shows(window, thread, "on it"));
    let project = window.register_project(&harness.dir).unwrap();
    assert!(window.registry().project(project).is_some());

    let mut second = cockpit(&harness);
    pump_until(&mut second, |second| shows(second, thread, "on it"));

    provider.send(decision()).unwrap();
    let pending = |window: &crate::cockpit::Cockpit| {
        window
            .thread(thread)
            .is_some_and(|open| open.pending().is_some())
    };
    pump_until(&mut window, pending);
    pump_until(&mut second, pending);
    let handle = window.thread(thread).unwrap().activity().decisions()[0]
        .handle
        .clone();
    let allow = DecisionAnswer::Allow {
        input: serde_json::json!({}),
    };
    assert!(window
        .respond_decision(thread, &handle, allow.clone())
        .unwrap());
    assert!(
        !second.respond_decision(thread, &handle, allow).unwrap(),
        "already answered"
    );
    assert_eq!(harness.answers.lock().unwrap().len(), 1);
    pump_until(&mut second, |second| !pending(second));

    window.park(thread).unwrap();
    assert!(window.thread(thread).is_none());
    assert_eq!(window.parked().unwrap(), vec![thread]);
    pump_until(&mut second, |second| second.thread(thread).is_none());
}

/// An Engine on its own Store, its Sessions scripted.
fn engine(dir: &Path, spawner: Box<dyn Spawner>) -> Engine {
    Engine::try_new(Store::open(dir).unwrap(), spawner).unwrap()
}

/// Scripted Sessions, and where their providers arrive: held, so each
/// Session lives as long as the test.
fn spawn() -> (Box<dyn Spawner>, Receiver<Sender<SessionEvent>>) {
    let (sessions, providers) = mpsc::channel();
    let spawner = Spawn {
        sessions,
        answers: Answers::default(),
        queued: Queued::default(),
        gate: None,
    };
    (Box::new(spawner), providers)
}

fn open_in(engine: &mut Engine, dir: &Path) -> ThreadId {
    engine
        .open(
            Provider::Claude,
            crate::workspace::WorkspaceChoice::Main {
                checkout: dir.to_path_buf(),
            },
        )
        .unwrap()
}

#[test]
fn threads_the_operator_parked_stay_parked_at_the_next_boot() {
    let dir = scratch("parked-stay");
    let store = dir.join("threads");
    let (spawner, _providers) = spawn();
    let mut first = engine(&store, spawner);
    let older = open_in(&mut first, &dir);
    let newer = open_in(&mut first, &dir);
    first.send(older, "worked here last".into());
    first.park(older).unwrap();
    first.park(newer).unwrap();
    drop(first);

    assert_eq!(launch_threads(&Store::open(&store).unwrap()), vec![]);
}

#[test]
fn a_boot_resumes_every_thread_that_was_open() {
    let dir = scratch("open-resume");
    let store = dir.join("threads");
    let (spawner, _providers) = spawn();
    let mut first = engine(&store, spawner);
    let one = open_in(&mut first, &dir);
    let two = open_in(&mut first, &dir);
    first.halt_sessions();
    drop(first);

    let (spawner, _resumed) = spawn();
    let mut next = engine(&store, spawner);
    revive_launch(&mut next, launch_threads(&Store::open(&store).unwrap()));
    assert_eq!(next.threads(), vec![one, two]);
}

/// ADR 0018: each resumed Session starts `LAUNCH_STAGGER` after the one
/// before, the wait being its start's, not the boot's.
#[test]
fn a_boot_staggers_the_sessions_it_resumes() {
    use crate::engine::Preparation;
    use crate::session::SessionLifecycle;
    struct Timing {
        inner: Box<dyn Spawner>,
        waits: Arc<Mutex<Vec<Duration>>>,
    }
    impl Spawner for Timing {
        fn spawn(
            &mut self,
            request: SpawnRequest,
        ) -> io::Result<Box<dyn crate::providers::Session>> {
            self.inner.spawn(request)
        }
        fn start_after(
            &mut self,
            prepare: Preparation,
            request: SpawnRequest,
        ) -> io::Result<SessionLifecycle> {
            let started = Instant::now();
            prepare()?;
            self.waits.lock().unwrap().push(started.elapsed());
            self.start(request)
        }
    }
    let dir = scratch("stagger");
    let store = dir.join("threads");
    let (spawner, _providers) = spawn();
    let mut first = engine(&store, spawner);
    for _ in 0..3 {
        open_in(&mut first, &dir);
    }
    first.halt_sessions();
    drop(first);

    let waits = Arc::new(Mutex::new(Vec::new()));
    let (inner, _resumed) = spawn();
    let mut next = engine(
        &store,
        Box::new(Timing {
            inner,
            waits: waits.clone(),
        }),
    );
    revive_launch(&mut next, launch_threads(&Store::open(&store).unwrap()));

    let waits = waits.lock().unwrap();
    assert_eq!(waits.len(), 3);
    for (nth, wait) in waits.iter().enumerate() {
        assert!(*wait >= LAUNCH_STAGGER * nth as u32, "{waits:?}");
    }
}

/// The far end of `ssh box ferrited bridge`, played by the harness's own
/// socket: every byte the window sends is kept, and the window closing its
/// end closes the Server's, as ssh does.
fn fake_ssh(address: PathBuf, sent: Arc<Mutex<Vec<u8>>>) -> crate::machine::Dial {
    struct Upload(link::Stream, Arc<Mutex<Vec<u8>>>);
    impl io::Write for Upload {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.1.lock().unwrap().extend_from_slice(buf);
            self.0.write(buf)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.0.flush()
        }
    }
    impl Drop for Upload {
        fn drop(&mut self) {
            let _ = link::TryClone::shutdown(&self.0);
        }
    }
    Box::new(move || {
        let stream = link::connect(&address)?;
        Ok(crate::machine::Tunnel {
            from: Box::new(link::TryClone::try_clone(&stream)?),
            to: Box::new(Upload(stream, sent.clone())),
            ended: Box::new(|| None),
        })
    })
}

fn bridge(harness: &Harness, sent: &Arc<Mutex<Vec<u8>>>) -> Arc<crate::machine::Bridge> {
    Arc::new(crate::machine::Bridge::new(
        "box",
        harness.dir.join("box").join("bridge.sock"),
        fake_ssh(harness.address.clone(), sent.clone()),
    ))
}

fn peer() -> Peer {
    Peer {
        name: "test".into(),
        version: "0".into(),
    }
}

/// What a client sent, as message types.
fn said(sent: &Arc<Mutex<Vec<u8>>>) -> Vec<String> {
    sent.lock()
        .unwrap()
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| {
            let message: serde_json::Value = serde_json::from_slice(line).unwrap();
            message["type"].as_str().unwrap_or_default().to_string()
        })
        .collect()
}

/// ADR 0026: a window on a machine is a window on its Server, through the
/// bridge — attached in the background, then driving its Threads exactly
/// as a window on this computer's does.
#[test]
fn a_window_on_a_machine_drives_its_threads_through_the_bridge() {
    let harness = start("machine-window");
    let sent = Arc::default();
    let mut window = crate::cockpit::Cockpit::attach(crate::cockpit::Attach {
        store: harness.dir.join("box").join("threads"),
        server: crate::cockpit::Endpoint::Machine(bridge(&harness, &sent)),
        client: peer(),
        renderer: None,
        discovery: Box::new(NoDiscovery),
    })
    .unwrap();
    assert!(
        matches!(
            window.server(),
            Some(crate::cockpit::ServerState::Detached { .. })
        ),
        "a machine is connected to in the background"
    );
    pump_until(&mut window, |window| {
        matches!(
            window.server(),
            Some(crate::cockpit::ServerState::Attached { .. })
        )
    });
    let thread = window
        .open(
            Provider::Claude,
            crate::workspace::WorkspaceChoice::Main {
                checkout: harness.dir.clone(),
            },
        )
        .unwrap();
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    window.send(thread, "go".into());
    provider.send(text("on it")).unwrap();
    pump_until(&mut window, |window| shows(window, thread, "on it"));
    assert!(said(&sent).contains(&"watch".to_string()));
}

/// Only the machine on screen streams transcripts: the others' links say
/// hello and nothing else, and still hear each open Thread's state and
/// every Decision.
#[test]
fn a_machine_off_screen_streams_status_and_never_a_transcript() {
    let harness = start("machine-status");
    let sent = Arc::default();
    let mut status = crate::cockpit::ServerStatus::new(
        harness.dir.join("box").join("threads"),
        crate::cockpit::Endpoint::Machine(bridge(&harness, &sent)),
        peer(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let until = |status: &mut crate::cockpit::ServerStatus,
                 done: &dyn Fn(&[crate::cockpit::StatusRow]) -> bool| {
        while !done(&status.rows()) {
            assert!(Instant::now() < deadline, "timed out");
            status.poll();
            std::thread::sleep(Duration::from_millis(5));
        }
    };

    let mut window = cockpit(&harness);
    let thread = window
        .open(
            Provider::Claude,
            crate::workspace::WorkspaceChoice::Main {
                checkout: harness.dir.clone(),
            },
        )
        .unwrap();
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    until(&mut status, &|rows| {
        rows.iter().any(|row| row.thread == thread)
    });
    window.send(thread, "go".into());
    provider.send(text("on it")).unwrap();
    until(&mut status, &|rows| rows.iter().any(|row| row.working));
    provider.send(decision()).unwrap();
    until(&mut status, &|rows| rows.iter().any(|row| row.waiting));

    assert!(matches!(
        status.server(),
        crate::cockpit::ServerState::Attached { .. }
    ));
    assert_eq!(said(&sent), vec!["hello".to_string()]);
}

// ------------------------------------------------------------- the API

/// An API caller's end (ADR 0027): one connection, every line it is sent.
struct Caller {
    stream: link::Stream,
    heard: Receiver<serde_json::Value>,
}

fn call(harness: &Harness) -> Caller {
    let stream = link::connect(&harness.api).unwrap();
    let mut lines = link::Lines::new(link::TryClone::try_clone(&stream).unwrap());
    let (tell, heard) = mpsc::channel();
    std::thread::spawn(move || {
        while let Ok(Some(line)) = lines.next_line() {
            if tell.send(serde_json::from_slice(&line).unwrap()).is_err() {
                return;
            }
        }
    });
    Caller { stream, heard }
}

impl Caller {
    fn ask(&mut self, id: &str, method: &str, params: serde_json::Value) {
        let request = serde_json::json!({ "id": id, "method": method, "params": params });
        link::send(&mut self.stream, &request).unwrap();
    }

    /// The next line, within five seconds.
    fn next(&self) -> serde_json::Value {
        self.heard
            .recv_timeout(Duration::from_secs(5))
            .expect("an answer within five seconds")
    }

    /// Nothing arrives for a while.
    fn quiet(&self) {
        let line = self.heard.recv_timeout(Duration::from_millis(300));
        assert!(line.is_err(), "unexpected {line:?}");
    }

    fn answer(&mut self, id: &str, method: &str, params: serde_json::Value) -> serde_json::Value {
        self.ask(id, method, params);
        let answer = self.next();
        assert_eq!(answer["id"], id, "{answer}");
        answer
    }
}

fn turn_ends(provider: &Sender<SessionEvent>) {
    provider
        .send(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
}

/// `thread.start` opens a titled Thread a window sees; `agent.prompt` with
/// `wait` holds its answer while the agent works and gives it once the
/// turn has ended.
#[test]
fn a_prompt_with_wait_answers_once_the_agent_worked_and_settled() {
    let harness = start("api-wait");
    let mut caller = call(&harness);
    let started = caller.answer(
        "1",
        "thread.start",
        serde_json::json!({ "cwd": harness.dir, "title": "reviewer" }),
    );
    let info = &started["result"]["thread"];
    assert_eq!(info["title"], "reviewer", "{started}");
    assert_eq!(info["provider"], "claude");
    let thread = info["thread"].as_u64().unwrap();
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();

    caller.ask(
        "2",
        "agent.prompt",
        serde_json::json!({ "thread": thread, "text": "review", "wait": true }),
    );
    caller.quiet();
    turn_ends(&provider);
    let settled = caller.next();
    assert_eq!(settled["id"], "2", "{settled}");
    assert_ne!(
        settled["result"]["thread"]["attention"], "working",
        "{settled}"
    );

    let read = caller.answer("3", "agent.read", serde_json::json!({ "thread": thread }));
    assert!(
        read["result"]["text"]
            .as_str()
            .unwrap()
            .contains("> review"),
        "{read}"
    );
}

/// Herdr's rule: a prompt never goes to an agent a Decision blocks. The
/// Decision is listed under its provider's id, an `agent.wait` for
/// `blocked` hears it, and the API's answer wins like a window's would.
#[test]
fn a_blocked_agent_refuses_prompts_and_its_decision_is_answered_once() {
    let harness = start("api-blocked");
    let mut window = attach(&harness.address);
    let (thread, provider) = open(&mut window, &harness);
    window.watch(thread, None);
    let mut caller = call(&harness);

    caller.ask(
        "1",
        "agent.wait",
        serde_json::json!({ "thread": thread, "until": "blocked" }),
    );
    caller.quiet();
    provider.send(decision()).unwrap();
    let blocked = caller.next();
    assert_eq!(
        blocked["result"]["thread"]["attention"], "blocked",
        "{blocked}"
    );

    let refused = caller.answer(
        "2",
        "agent.prompt",
        serde_json::json!({ "thread": thread, "text": "carry on" }),
    );
    assert_eq!(refused["error"]["code"], "agent_blocked", "{refused}");

    let listed = caller.answer("3", "decision.list", serde_json::json!({}));
    assert_eq!(
        listed["result"]["decisions"][0]["decision"]["id"], "perm_01",
        "{listed}"
    );

    let answered = caller.answer(
        "4",
        "decision.answer",
        serde_json::json!({
            "thread": thread,
            "decision": "perm_01",
            "answer": { "kind": "allow", "input": {} },
        }),
    );
    assert!(answered.get("result").is_some(), "{answered}");
    assert_eq!(harness.answers.lock().unwrap().len(), 1);
    let again = caller.answer(
        "5",
        "decision.answer",
        serde_json::json!({
            "thread": thread,
            "decision": "perm_01",
            "answer": { "kind": "allow", "input": {} },
        }),
    );
    assert_eq!(again["error"]["code"], "unknown_decision", "{again}");
    window.until(|window| !window.closed().is_empty());
    assert!(matches!(
        window.closed()[..],
        [ClosedBy::Answered { client }] if client >= 1 << 32
    ));
}

/// Subscribed before the snapshot is read: the first answer is the state
/// now, and every change after it follows as an event under the same id.
#[test]
fn a_subscriber_hears_the_threads_now_then_every_change() {
    let harness = start("api-events");
    let mut window = attach(&harness.address);
    let (thread, provider) = open(&mut window, &harness);
    let mut subscriber = call(&harness);
    let now = subscriber.answer(
        "events",
        "events.subscribe",
        serde_json::json!({ "threads": [thread] }),
    );
    assert_eq!(now["result"]["threads"][0]["attention"], "idle", "{now}");
    assert_eq!(now["result"]["decisions"], serde_json::json!([]));

    let mut caller = call(&harness);
    caller.answer(
        "1",
        "agent.prompt",
        serde_json::json!({ "thread": thread, "text": "go" }),
    );
    let working = subscriber.next();
    assert_eq!(working["id"], "events");
    assert_eq!(
        working["event"],
        serde_json::json!({ "event": "attention", "thread": thread, "attention": "working" })
    );
    provider.send(decision()).unwrap();
    let asked = subscriber.next();
    assert_eq!(asked["event"]["event"], "decision_requested", "{asked}");
    let blocked = subscriber.next();
    assert_eq!(blocked["event"]["attention"], "blocked", "{blocked}");
}

/// `thread.focus` and `notification.show` reach every window.
#[test]
fn focus_and_toasts_reach_the_windows() {
    let harness = start("api-windows");
    let mut window = attach(&harness.address);
    let (thread, _provider) = open(&mut window, &harness);
    let mut caller = call(&harness);

    caller.answer("1", "thread.focus", serde_json::json!({ "thread": thread }));
    let shown = caller.answer(
        "2",
        "notification.show",
        serde_json::json!({ "title": "Review ready" }),
    );
    assert_eq!(shown["result"]["windows"], 1, "{shown}");
    let mut told = Vec::new();
    while told.len() < 2 {
        match window
            .client
            .messages
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
        {
            message @ (ServerMessage::Focus { .. } | ServerMessage::Notification { .. }) => {
                told.push(message)
            }
            _ => {}
        }
    }
    assert_eq!(
        told,
        [
            ServerMessage::Focus { thread },
            ServerMessage::Notification {
                title: "Review ready".into(),
                body: None,
            },
        ]
    );
    let unknown = caller.answer("3", "thread.focus", serde_json::json!({ "thread": 999 }));
    assert_eq!(unknown["error"]["code"], "unknown_thread", "{unknown}");
}

/// A line that is not a request is answered with why, and the connection
/// stays open for the next.
#[test]
fn an_unknown_method_is_answered_and_the_connection_goes_on() {
    let harness = start("api-unknown");
    let mut caller = call(&harness);
    let unknown = caller.answer("9", "agent.dance", serde_json::json!({}));
    assert_eq!(unknown["error"]["code"], "unknown_method", "{unknown}");
    let listed = caller.answer("10", "thread.list", serde_json::json!({}));
    assert_eq!(listed["result"]["threads"], serde_json::json!([]));
}

/// A prompt to a working agent waits in the provider's queue, and its wait
/// is not answered by the turn before it: between that turn's end and the
/// queued prompt's start the agent is still at work.
#[test]
fn a_prompt_queued_behind_a_turn_waits_for_its_own() {
    let harness = start("api-queued");
    let mut window = attach(&harness.address);
    let (thread, provider) = open(&mut window, &harness);
    let mut caller = call(&harness);
    caller.answer(
        "1",
        "agent.prompt",
        serde_json::json!({ "thread": thread, "text": "first" }),
    );

    caller.ask(
        "2",
        "agent.prompt",
        serde_json::json!({ "thread": thread, "text": "second", "wait": true }),
    );
    caller.quiet();
    turn_ends(&provider);
    caller.quiet();
    let client_id = harness.queued.lock().unwrap()[0].clone();
    provider
        .send(SessionEvent::Queue(crate::QueueEvent::Started {
            historical: false,
            client_id,
            text: None,
        }))
        .unwrap();
    caller.quiet();
    turn_ends(&provider);
    let settled = caller.next();
    assert_eq!(settled["id"], "2", "{settled}");
    assert_ne!(
        settled["result"]["thread"]["attention"], "working",
        "{settled}"
    );
}

/// A prompt to a Session still starting is held until it is up, and its
/// wait does not stall meanwhile; a second is refused rather than replace
/// it.
#[test]
fn a_starting_session_holds_one_prompt_and_refuses_another() {
    let (harness, gate) = start_gated("api-starting");
    let mut caller = call(&harness);
    let started = caller.answer(
        "1",
        "thread.start",
        serde_json::json!({ "cwd": harness.dir }),
    );
    let thread = started["result"]["thread"]["thread"].as_u64().unwrap();
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();

    caller.ask(
        "2",
        "agent.prompt",
        serde_json::json!({ "thread": thread, "text": "review", "wait": true }),
    );
    caller.quiet();
    let again = caller.answer(
        "3",
        "agent.prompt",
        serde_json::json!({ "thread": thread, "text": "and again" }),
    );
    assert_eq!(again["error"]["code"], "agent_starting", "{again}");

    gate.send(()).unwrap();
    let working = caller.answer(
        "4",
        "agent.wait",
        serde_json::json!({ "thread": thread, "until": "working" }),
    );
    assert!(working.get("result").is_some(), "{working}");
    turn_ends(&provider);
    let settled = caller.next();
    assert_eq!(settled["id"], "2", "{settled}");
    let read = caller.answer("5", "agent.read", serde_json::json!({ "thread": thread }));
    let text = read["result"]["text"].as_str().unwrap();
    assert!(text.contains("> review"), "{read}");
    assert!(!text.contains("and again"), "{read}");
}

/// A prompt held for a starting Session is work: a plain `agent.wait` after
/// it is not answered by the idle before the agent even has it.
#[test]
fn a_held_prompt_keeps_a_plain_wait_waiting() {
    let (harness, gate) = start_gated("api-held");
    let mut caller = call(&harness);
    let started = caller.answer(
        "1",
        "thread.start",
        serde_json::json!({ "cwd": harness.dir }),
    );
    let thread = started["result"]["thread"]["thread"].as_u64().unwrap();
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let sent = caller.answer(
        "2",
        "agent.prompt",
        serde_json::json!({ "thread": thread, "text": "review" }),
    );
    assert!(sent.get("result").is_some(), "{sent}");

    caller.ask("3", "agent.wait", serde_json::json!({ "thread": thread }));
    caller.quiet();
    gate.send(()).unwrap();
    caller.quiet();
    turn_ends(&provider);
    let settled = caller.next();
    assert_eq!(settled["id"], "3", "{settled}");
    assert_ne!(
        settled["result"]["thread"]["attention"], "working",
        "{settled}"
    );
}

/// A caller that half-closes once it has asked (`echo … | nc -U`) still
/// hears its wait's answer.
#[cfg(unix)]
#[test]
fn a_half_closed_caller_still_hears_its_wait() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let harness = start("api-half-closed");
    let mut window = attach(&harness.address);
    let (thread, provider) = open(&mut window, &harness);
    let mut caller = call(&harness);
    caller.answer(
        "1",
        "agent.prompt",
        serde_json::json!({ "thread": thread, "text": "go" }),
    );

    let mut script = UnixStream::connect(&harness.api).unwrap();
    let request = serde_json::json!({
        "id": "w",
        "method": "agent.wait",
        "params": { "thread": thread },
    });
    writeln!(script, "{request}").unwrap();
    script.shutdown(std::net::Shutdown::Write).unwrap();
    caller.quiet();
    turn_ends(&provider);
    script
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut line = String::new();
    BufReader::new(script).read_line(&mut line).unwrap();
    let settled: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(settled["id"], "w", "{settled}");
    assert_ne!(
        settled["result"]["thread"]["attention"], "working",
        "{settled}"
    );
}

/// What the CLI prints, one write per line.
struct Printed(Sender<String>);

impl io::Write for Printed {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let _ = self.0.send(String::from_utf8_lossy(bytes).into_owned());
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn printed() -> (Printed, Receiver<String>) {
    let (tell, told) = mpsc::channel();
    (Printed(tell), told)
}

fn line(text: &str) -> serde_json::Value {
    assert!(text.ends_with('\n'), "{text:?}");
    serde_json::from_str(text).unwrap()
}

fn request(method: &str, params: serde_json::Value) -> crate::api::Request {
    serde_json::from_value(serde_json::json!({ "id": "1", "method": method, "params": params }))
        .unwrap()
}

/// One CLI call: its exit status, and what it printed to stdout and stderr.
fn cli(
    socket: &Path,
    method: &str,
    params: serde_json::Value,
) -> (i32, Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let ((mut out, said), (mut err, refused)) = (printed(), printed());
    let status = crate::api::cli::call(&request(method, params), socket, &mut out, &mut err);
    drop((out, err));
    let lines = |told: Receiver<String>| told.iter().map(|text| line(&text)).collect();
    (status, lines(said), lines(refused))
}

/// The CLI over a real socket: an answer on stdout with status 0, a
/// refusal on stderr with status 1, no Server as `server_unavailable`, and
/// a subscription's events as they come.
#[test]
fn the_cli_prints_answers_refusals_and_events() {
    let harness = start("api-cli");
    let mut window = attach(&harness.address);
    let (thread, _provider) = open(&mut window, &harness);

    let (status, said, refused) = cli(
        &harness.api,
        "thread.get",
        serde_json::json!({ "thread": thread }),
    );
    assert_eq!((status, refused.len()), (0, 0), "{refused:?}");
    assert_eq!(
        said[0]["result"]["thread"]["thread"],
        serde_json::json!(thread)
    );

    let (status, said, refused) = cli(
        &harness.api,
        "thread.get",
        serde_json::json!({ "thread": 999 }),
    );
    assert_eq!((status, said.len()), (1, 0), "{said:?}");
    assert_eq!(refused[0]["error"]["code"], "unknown_thread");

    let (status, _, refused) = cli(
        &harness.dir.join("nobody.sock"),
        "thread.list",
        serde_json::json!({}),
    );
    assert_eq!(status, 1);
    assert_eq!(refused[0]["error"]["code"], "server_unavailable");

    let (mut out, said) = printed();
    let subscribe = request(
        "events.subscribe",
        serde_json::json!({ "threads": [thread] }),
    );
    let api = harness.api.clone();
    std::thread::spawn(move || crate::api::cli::call(&subscribe, &api, &mut out, &mut io::sink()));
    let next = || line(&said.recv_timeout(Duration::from_secs(5)).unwrap());
    assert_eq!(next()["result"]["threads"][0]["attention"], "idle");
    call(&harness).answer(
        "1",
        "agent.prompt",
        serde_json::json!({ "thread": thread, "text": "go" }),
    );
    assert_eq!(next()["event"]["attention"], "working");
}

/// A repository with one commit, under `dir`: a Project workers can make
/// worktrees of.
fn repository(dir: &Path) -> PathBuf {
    let repo = dir.join("app");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| crate::workspace::git_for_tests(&repo, args);
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README.md"), "app\n").unwrap();
    git(&["add", "README.md"]);
    git(&[
        "-c",
        "user.email=test@example.invalid",
        "-c",
        "user.name=test",
        "commit",
        "-qm",
        "base",
    ]);
    repo
}

/// ADR 0028 over the API: a Project's coordinator starts briefed (and a
/// second start answers the same one); only a coordinator starts workers;
/// a worker works in a new worktree, briefed with its task, seated in the
/// coordinator's Group; and what its finished turn lists under
/// `## Remember` joins the Project's memory.
#[test]
fn a_coordinator_starts_briefed_workers_whose_lessons_are_remembered() {
    let harness = start("api-coordinator");
    let repo = repository(&harness.dir);
    let mut caller = call(&harness);
    let started = caller.answer(
        "1",
        "coordinator.start",
        serde_json::json!({ "cwd": repo, "goal": "Ship billing" }),
    );
    let coordinator = started["result"]["coordinator"]["thread"]
        .as_u64()
        .unwrap_or_else(|| panic!("{started}"));
    assert_eq!(started["result"]["coordinator"]["title"], "app coordinator");
    assert_eq!(started["result"]["goal"], "Ship billing");
    harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let again = caller.answer("2", "coordinator.start", serde_json::json!({ "cwd": repo }));
    assert_eq!(again["result"]["coordinator"]["thread"], coordinator, "{again}");
    let briefed = caller.answer(
        "3",
        "agent.read",
        serde_json::json!({ "thread": coordinator, "lines": 400 }),
    );
    let briefed = briefed["result"]["text"].as_str().unwrap();
    assert!(briefed.contains("never do the work yourself"), "{briefed}");
    assert!(briefed.contains("Ship billing"), "{briefed}");

    let refused = caller.answer(
        "4",
        "worker.start",
        serde_json::json!({ "thread": 999, "task": "Add invoices" }),
    );
    assert_eq!(refused["error"]["code"], "not_a_coordinator", "{refused}");

    let worker = caller.answer(
        "5",
        "worker.start",
        serde_json::json!({ "thread": coordinator, "task": "Add invoices", "title": "invoices" }),
    );
    let info = &worker["result"]["thread"];
    let thread = info["thread"].as_u64().unwrap_or_else(|| panic!("{worker}"));
    assert_eq!(info["title"], "invoices");
    assert_ne!(info["cwd"], serde_json::json!(repo), "a worktree of its own");
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let briefed = caller.answer(
        "6",
        "agent.read",
        serde_json::json!({ "thread": thread, "lines": 400 }),
    );
    let briefed = briefed["result"]["text"].as_str().unwrap();
    for part in ["## Your task", "Add invoices", "Ship billing", "## Remember"] {
        assert!(briefed.contains(part), "{part:?} in {briefed}");
    }
    let groups = std::fs::read_to_string(harness.dir.join("threads").join("groups.json")).unwrap();
    let groups: serde_json::Value = serde_json::from_str(&groups).unwrap();
    assert_eq!(
        groups["groups"][0]["members"],
        serde_json::json!([coordinator, thread])
    );
    assert_eq!(groups["groups"][0]["coordinator"], coordinator);

    let got = caller.answer(
        "7",
        "coordinator.get",
        serde_json::json!({ "thread": coordinator }),
    );
    assert_eq!(got["result"]["workers"][0]["task"], "Add invoices", "{got}");
    assert_eq!(got["result"]["workers"][0]["need"], "working", "{got}");

    provider
        .send(text(
            "Invoices render.\n\n## Remember\n\n- Invoices live in app/billing\n",
        ))
        .unwrap();
    turn_ends(&provider);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let got = caller.answer(
            "8",
            "coordinator.get",
            serde_json::json!({ "thread": coordinator }),
        );
        let memory = got["result"]["memory"]["text"].as_str().unwrap_or_default();
        if memory.contains("- Invoices live in app/billing") {
            assert_eq!(got["result"]["workers"][0]["need"], "ready_for_review");
            break;
        }
        assert!(Instant::now() < deadline, "never remembered: {got}");
        std::thread::sleep(Duration::from_millis(20));
    }
}
