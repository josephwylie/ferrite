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

struct Scripted {
    events: Receiver<SessionEvent>,
    answers: Answers,
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
}

/// Each Session it starts hands the test the Sender that plays its provider.
struct Spawn {
    sessions: Sender<Sender<SessionEvent>>,
    answers: Answers,
}

impl Spawner for Spawn {
    fn spawn(&mut self, _: SpawnRequest) -> io::Result<Box<dyn crate::providers::Session>> {
        let (provider, events) = mpsc::channel();
        self.sessions.send(provider).unwrap();
        Ok(Box::new(Scripted {
            events,
            answers: self.answers.clone(),
        }))
    }
}

struct Harness {
    server: std::thread::JoinHandle<i32>,
    /// Every executable the Server restarted into, in place of `exec`.
    relaunched: Receiver<PathBuf>,
    dir: PathBuf,
    address: PathBuf,
    sessions: Receiver<Sender<SessionEvent>>,
    answers: Answers,
}

fn start(name: &str) -> Harness {
    let dir = scratch(name);
    let store = dir.join("threads");
    let address = link::address(&store);
    let listener = link::listen(&address).unwrap();
    let (sessions, started) = mpsc::channel();
    let answers = Answers::default();
    let spawn = Spawn {
        sessions,
        answers: answers.clone(),
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
        link::accept_all(listener, heard).unwrap();
        server.serve(hear)
    });
    Harness {
        server,
        relaunched,
        dir,
        address,
        sessions: started,
        answers,
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
    drop(provider);
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
    drop(provider);
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
    for _ in 0..after + 2 {
        provider.send(text(".")).unwrap();
    }
    other.watch(thread, None);
    other.until(|window| window.said(thread).iter().any(|said| said.contains("...")));

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
        executable: "/bin/false".into(),
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
        fn spawn(&mut self, request: SpawnRequest) -> io::Result<Box<dyn crate::providers::Session>> {
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
