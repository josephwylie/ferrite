//! `thread.stop`, what another Server sends to stop a Thread it started
//! there (T3 Code's `thread.stop`): Main's turn, its Subagents and its
//! background commands end, and its queue goes back to the Composer. A
//! Thread already stopped, or parked, is stopped again with nothing sent.

use super::*;
use ferrite_core::activity::{ActivityEvent, AgentInfo, AgentKey, AgentStatus, Subject, TranscriptCoverage};
use ferrite_core::progress::{BackgroundTask, ProgressEvent, TaskStatus};
use ferrite_core::{QueueEvent, QueuedPrompt, SessionControl};

/// What a stop asked of the provider, in order.
#[derive(Debug, Clone, PartialEq)]
enum Asked {
    LetGo(String),
    StopTask(String),
    StopAgent(AgentKey),
    Interrupt,
}

type Asks = Arc<Mutex<Vec<Asked>>>;

struct Stoppable {
    events: Receiver<SessionEvent>,
    asks: Asks,
    queued: Queued,
}

impl Session for Stoppable {
    fn events(&self) -> &Receiver<SessionEvent> {
        &self.events
    }
    fn send(&mut self, _: &str) -> io::Result<()> {
        Ok(())
    }
    fn enqueue(&mut self, client_id: &str, _: &str) -> io::Result<()> {
        self.queued.lock().unwrap().push(client_id.into());
        Ok(())
    }
    fn cancel_queued(&mut self, id: &str) -> io::Result<()> {
        self.asks.lock().unwrap().push(Asked::LetGo(id.into()));
        Ok(())
    }
    fn supports_control(&self, kind: ferrite_core::ControlKind) -> bool {
        kind == ferrite_core::ControlKind::StopTask
    }
    fn control(&mut self, action: SessionControl) -> io::Result<()> {
        let SessionControl::StopTask { id } = action else {
            panic!("only a task is stopped: {action:?}");
        };
        self.asks.lock().unwrap().push(Asked::StopTask(id));
        Ok(())
    }
    fn stop_subagent(&mut self, key: &AgentKey) -> io::Result<()> {
        self.asks.lock().unwrap().push(Asked::StopAgent(key.clone()));
        Ok(())
    }
    fn interrupt(&mut self) -> io::Result<()> {
        self.asks.lock().unwrap().push(Asked::Interrupt);
        Ok(())
    }
    fn respond_to_decision(&mut self, _: &str, _: ferrite_core::DecisionAnswer) -> io::Result<()> {
        Ok(())
    }
}

struct Stoppables {
    providers: Sender<Sender<SessionEvent>>,
    asks: Asks,
    queued: Queued,
}

impl Spawner for Stoppables {
    fn spawn(&mut self, _: SpawnRequest) -> io::Result<Box<dyn Session>> {
        let (provider, events) = mpsc::channel();
        self.providers.send(provider).unwrap();
        Ok(Box::new(Stoppable {
            events,
            asks: self.asks.clone(),
            queued: self.queued.clone(),
        }))
    }
}

/// A Server whose API is asked by hand, between pumps of its Engine.
struct Peer {
    server: Server,
    answers: Receiver<Vec<u8>>,
    asks: Asks,
    queued: Queued,
    providers: Receiver<Sender<SessionEvent>>,
    dir: PathBuf,
}

fn peer(name: &str) -> Peer {
    let dir = scratch(name);
    let (providers, started) = mpsc::channel();
    let spawner = Stoppables {
        providers,
        asks: Asks::default(),
        queued: Queued::default(),
    };
    let (asks, queued) = (spawner.asks.clone(), spawner.queued.clone());
    let (_, renders) = render::remote();
    let mut server = Server::new(
        engine(&dir.join("threads"), Box::new(spawner)),
        ServerInfo {
            version: "0.9.0".into(),
            executable: "/bin/ferrite".into(),
        },
        crate::server_id::open(&dir.join("threads")).unwrap(),
        renders,
        Arc::new(Mutex::new(SessionDefaults::default())),
        dir.clone(),
        mpsc::channel().0,
    );
    let (answer, answers) = mpsc::channel();
    server.api_heard(ferrite_core::api::Heard::Joined(1, answer));
    Peer {
        server,
        answers,
        asks,
        queued,
        providers: started,
        dir,
    }
}

impl Peer {
    fn stop(&mut self, thread: ThreadId) -> serde_json::Value {
        let request = request("thread.stop", serde_json::json!({ "thread": thread }));
        self.server
            .api_heard(ferrite_core::api::Heard::Request(1, request));
        serde_json::from_slice(&self.answers.try_recv().expect("answered at once")).unwrap()
    }

    /// A Thread whose agent is up, and the provider that plays it.
    fn open(&mut self) -> (ThreadId, Sender<SessionEvent>) {
        let thread = open_in(&mut self.server.engine, &self.dir);
        let provider = self.providers.recv_timeout(PATIENCE).unwrap();
        pump_engine_until(&mut self.server.engine, "the agent never came up", |engine| {
            !engine.thread(thread).unwrap().starting()
        });
        (thread, provider)
    }

    fn asked(&self) -> Vec<Asked> {
        self.asks.lock().unwrap().clone()
    }
}

fn agent() -> AgentKey {
    AgentKey::new(Provider::Claude, "stop-fixture", "toolu_review")
}

fn tasks(status: TaskStatus) -> SessionEvent {
    SessionEvent::Progress {
        event: ProgressEvent::BackgroundSnapshot {
            tasks: vec![BackgroundTask {
                id: "task:1".into(),
                label: "cargo test".into(),
                status,
                detail: "local_bash".into(),
            }],
        },
    }
}

fn agent_is(state: AgentStatus) -> SessionEvent {
    SessionEvent::Activity(ActivityEvent::Status { key: agent(), state })
}

/// A Thread at work on everything at once: Main's turn runs, a prompt
/// waits in the provider's queue, a Subagent works and a command runs in
/// the background.
fn at_work(peer: &mut Peer) -> (ThreadId, Sender<SessionEvent>) {
    let (thread, provider) = peer.open();
    let engine = &mut peer.server.engine;
    engine
        .execute(&Op::Send {
            thread,
            text: "go".into(),
        })
        .unwrap();
    provider.send(text("working")).unwrap();
    engine.pump();
    engine
        .execute(&Op::Queue {
            thread,
            text: "and then this".into(),
        })
        .unwrap();
    let client_id = peer.queued.lock().unwrap()[0].clone();
    provider
        .send(SessionEvent::Queue(QueueEvent::Accepted(QueuedPrompt {
            id: "native:1".into(),
            client_id,
            text: "and then this".into(),
        })))
        .unwrap();
    let mut info = AgentInfo::new(agent());
    info.parent = Some(Subject::Main);
    info.coverage = TranscriptCoverage::Live;
    provider
        .send(SessionEvent::Activity(ActivityEvent::Discovered(info)))
        .unwrap();
    provider.send(agent_is(AgentStatus::Working)).unwrap();
    provider.send(tasks(TaskStatus::Working)).unwrap();
    pump_engine_until(engine, "the work never showed", |engine| {
        let view = engine.thread(thread).unwrap();
        view.busy()
            && view.provider_queue_len() == 1
            && view.activity().running_agents().count() == 1
            && view.transcript().progress().working_background() == 1
    });
    (thread, provider)
}

/// The queue is let go of first, so the interrupt cannot start it; then
/// the command, the Subagent and Main's turn are stopped. The prompt that
/// waited is back in the Composer, not lost.
#[test]
fn a_stop_ends_the_turn_the_subagents_and_the_commands_and_holds_the_queue() {
    let mut peer = peer("stop-all");
    let (thread, _provider) = at_work(&mut peer);

    let stopped = peer.stop(thread);

    assert_eq!(stopped["result"], serde_json::json!({ "thread": thread }), "{stopped}");
    assert_eq!(
        peer.asked(),
        [
            Asked::LetGo("native:1".into()),
            Asked::StopTask("task:1".into()),
            Asked::StopAgent(agent()),
            Asked::Interrupt,
        ]
    );
    let engine = &mut peer.server.engine;
    assert_eq!(engine.thread(thread).unwrap().provider_queue_len(), 0);
    assert_eq!(
        engine.take_retrieved_prompt(thread),
        Some(("and then this".into(), false))
    );
    let _ = std::fs::remove_dir_all(&peer.dir);
}

/// Once the provider has ended what it was told to, a second stop finds
/// nothing running and sends nothing, as does a stop of a Thread at rest.
#[test]
fn a_stopped_thread_is_stopped_again_with_nothing_sent() {
    let mut peer = peer("stop-again");
    let (thread, provider) = at_work(&mut peer);
    peer.stop(thread);
    let sent = peer.asked().len();
    turn_ends(&provider);
    provider.send(agent_is(AgentStatus::Interrupted)).unwrap();
    provider.send(tasks(TaskStatus::Stopped)).unwrap();
    pump_engine_until(&mut peer.server.engine, "the work never ended", |engine| {
        !engine.thread(thread).unwrap().activity().background_work()
            && !engine.thread(thread).unwrap().busy()
    });

    let again = peer.stop(thread);

    assert_eq!(again["result"], serde_json::json!({ "thread": thread }), "{again}");
    assert_eq!(peer.asked().len(), sent, "{:?}", peer.asked());
    let (idle, _idle_provider) = peer.open();
    let at_rest = peer.stop(idle);
    assert_eq!(at_rest["result"], serde_json::json!({ "thread": idle }), "{at_rest}");
    assert_eq!(peer.asked().len(), sent, "{:?}", peer.asked());
    let _ = std::fs::remove_dir_all(&peer.dir);
}

/// A parked Thread runs nothing, so it is stopped already, not unknown;
/// a Thread the Store never held is.
#[test]
fn a_parked_thread_is_already_stopped_and_an_unknown_one_is_not_found() {
    let mut peer = peer("stop-parked");
    let (thread, _provider) = peer.open();
    peer.server.engine.park(thread).unwrap();

    let parked = peer.stop(thread);
    let unknown = peer.stop(ThreadId::new(99));

    assert_eq!(parked["result"], serde_json::json!({ "thread": thread }), "{parked}");
    assert_eq!(unknown["error"]["code"], "unknown_thread", "{unknown}");
    assert!(peer.asked().is_empty(), "{:?}", peer.asked());
    let _ = std::fs::remove_dir_all(&peer.dir);
}
