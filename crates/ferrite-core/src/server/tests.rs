//! The Server over its socket, with scripted Sessions: Decisions answered
//! by two windows, and a window that reconnects resuming from its ring or
//! rebuilding from history.

use std::path::Path;

use crate::engine::HELD_AT_REST;
use std::sync::mpsc::Sender;
use std::time::Instant;

use super::*;
use crate::cockpit::mirror::Mirrors;
use crate::engine::{SpawnRequest, Spawner};
use crate::link::{Attached, Client};
use crate::protocol::{Peer, Resume, WireAnswer, WireChoice, WireEntry};
use crate::providers::Session;
use crate::store::Provider;
use crate::transcript::Body;
use crate::{Decision, DecisionAnswer, SessionEvent};

#[cfg(unix)]
mod herdr;
mod park;

pub(super) fn scratch(name: &str) -> PathBuf {
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
    can_queue: CanQueue,
    controls: Controls,
}

/// The controls run on the providers' Sessions, in order.
type Controls = Arc<Mutex<Vec<crate::SessionControl>>>;

/// Whether the provider takes prompts into its queue yet: always, unless
/// the test holds it shut, as Claude is until its first turn starts.
type CanQueue = Option<Arc<std::sync::atomic::AtomicBool>>;

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
    /// Takes every prompt once it can queue at all.
    fn enqueue(&mut self, client_id: &str, _: &str) -> io::Result<()> {
        if self
            .can_queue
            .as_ref()
            .is_some_and(|open| !open.load(std::sync::atomic::Ordering::SeqCst))
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "the provider cannot queue yet",
            ));
        }
        self.queued.lock().unwrap().push(client_id.into());
        Ok(())
    }
    fn permission_modes(&self) -> Vec<crate::PermissionModeChoice> {
        ["default", "plan"]
            .map(|mode| crate::PermissionModeChoice {
                value: mode.into(),
                label: mode.into(),
            })
            .to_vec()
    }
    fn supports_control(&self, kind: crate::ControlKind) -> bool {
        matches!(
            kind,
            crate::ControlKind::RefreshMcp | crate::ControlKind::BackgroundTasks
        )
    }
    fn control(&mut self, action: crate::SessionControl) -> io::Result<()> {
        self.controls.lock().unwrap().push(action);
        Ok(())
    }
    /// Finds the one file named exactly what was asked for.
    fn search_files(
        &mut self,
        query: &str,
        reply: crate::providers::FileSearchReply,
    ) -> io::Result<()> {
        let _ = reply.send(Ok(vec![crate::providers::FileSuggestion {
            path: query.into(),
            is_directory: false,
            matched: Vec::new(),
        }]));
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
    can_queue: CanQueue,
    controls: Controls,
}

impl Spawn {
    fn scripted(&self) -> Scripted {
        let (provider, events) = mpsc::channel();
        self.sessions.send(provider).unwrap();
        Scripted {
            events,
            answers: self.answers.clone(),
            queued: self.queued.clone(),
            can_queue: self.can_queue.clone(),
            controls: self.controls.clone(),
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
    serve(name, None, None)
}

/// A Server whose Sessions start only as the returned Sender lets each
/// through, one `()` each.
fn start_gated(name: &str) -> (Harness, Sender<()>) {
    let (gate, opens) = mpsc::channel();
    (serve(name, Some(Arc::new(Mutex::new(opens))), None), gate)
}

/// A Server whose Projects run `projects`, when given, for their hooks.
fn serve(name: &str, gate: Option<Arc<Mutex<Receiver<()>>>>, projects: Option<PathBuf>) -> Harness {
    serve_in(scratch(name), gate, projects)
}

/// `serve`, on the Store already in `dir`: a restart's successor.
fn serve_in(
    dir: PathBuf,
    gate: Option<Arc<Mutex<Receiver<()>>>>,
    projects: Option<PathBuf>,
) -> Harness {
    let store = dir.join("threads");
    let projects = projects.map(|program| {
        let env = super::projects::environment(&store, Path::new("/bin/ferrite"), &program);
        super::projects::Projects::started(program, env)
    });
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
        can_queue: None,
        controls: Controls::default(),
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
        server.projects = projects;
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
    /// How many `history` and `history_end` messages the Server sent this
    /// window: a resume the ring met needs none.
    histories: usize,
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
        histories: 0,
    };
    for state in snapshot.threads {
        window.receive(ServerMessage::Thread { revision: 0, state });
    }
    for protocol::ThreadDigest { thread, digest } in snapshot.digests.unwrap_or_default() {
        window.receive(ServerMessage::Digest { thread, digest });
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
            message => {
                if matches!(
                    message,
                    ServerMessage::History { .. } | ServerMessage::HistoryEnd { .. }
                ) {
                    self.histories += 1;
                }
                self.mirrors.receive(message, Instant::now())
            }
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
        self.until_within(Duration::from_secs(5), done);
    }

    fn until_within(&mut self, limit: Duration, done: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + limit;
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

    fn digest(&self, thread: ThreadId) -> Option<crate::digest::Digest> {
        self.mirrors.view(thread)?.sent_digest().cloned()
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

pub(super) fn decision() -> SessionEvent {
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

pub(super) fn text(text: &str) -> SessionEvent {
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
            .is_some_and(|view| !view.activity().decisions().is_empty())
    };
    first.until(pending);
    second.until(pending);
    let handle: protocol::WireHandle =
        (&first.mirrors.view(thread).unwrap().pending_decisions()[0].handle).into();
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

/// The handle of the Decision `window` sees pending on `thread`, other
/// than `but`, once its watch has shown it: the watch may land after the
/// Decision's own message.
fn pending_handle(
    window: &mut Window,
    thread: ThreadId,
    but: Option<&protocol::WireHandle>,
) -> protocol::WireHandle {
    let pending = |window: &Window| {
        let view = window.mirrors.view(thread)?;
        let decisions = view.activity().decisions();
        let handle: protocol::WireHandle = (&decisions.first()?.handle).into();
        (Some(&handle) != but).then_some(handle)
    };
    window.until(|window| pending(window).is_some());
    pending(window).unwrap()
}

pub(super) fn allow(thread: ThreadId, handle: &protocol::WireHandle) -> Op {
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
    let handle = pending_handle(&mut window, thread, None);

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

/// A window on a machine browses the machine's folders through its Server
/// (ADR 0026): `list_dir` answers the directory's entries in name order,
/// each saying whether it is a directory.
#[test]
fn list_dir_answers_a_directory_on_the_servers_computer() {
    let harness = start("list-dir");
    let folder = harness.dir.join("src");
    std::fs::create_dir_all(folder.join("ferrite")).unwrap();
    std::fs::write(folder.join("notes.md"), "").unwrap();
    let mut window = attach(&harness.address);

    let reply = window.command(
        1,
        Op::ListDir {
            path: folder.clone(),
        },
    );
    let ServerMessage::Reply {
        ok: Some(Done::Listing { path, entries }),
        ..
    } = reply
    else {
        panic!("{reply:?}");
    };
    assert_eq!(path, folder);
    assert_eq!(
        entries,
        [
            WireEntry {
                name: "ferrite".into(),
                directory: true,
            },
            WireEntry {
                name: "notes.md".into(),
                directory: false,
            },
        ]
    );

    let reply = window.command(
        2,
        Op::ListDir {
            path: folder.join("nowhere"),
        },
    );
    assert!(refused_with(&reply, ErrorCode::Failed), "{reply:?}");
}

/// A window on a machine offers the machine's worktrees and branches
/// (ADR 0026): `list_repo` reads the Project's repo with git where the
/// Server runs, off its pump, naming the main checkout's branch apart.
#[test]
fn list_repo_answers_a_projects_worktrees_and_branches_read_by_the_server() {
    let harness = start("list-repo");
    let repo = repository(&harness.dir);
    let tree = harness.dir.join("fix");
    let git = |args: &[&str]| crate::workspace::git_for_tests(&repo, args);
    git(&["worktree", "add", "-q", "-b", "fix", tree.to_str().unwrap()]);
    git(&["branch", "spare"]);
    let mut window = cockpit(&harness);
    let project = window.register_project(&repo).unwrap();

    let answer = window.list_repo(project).unwrap();
    let listed = std::cell::RefCell::new(None);
    pump_until(&mut window, |_| {
        if let Ok(answered) = answer.try_recv() {
            *listed.borrow_mut() = Some(answered);
        }
        listed.borrow().is_some()
    });
    let listing = listed.into_inner().unwrap().unwrap();
    assert_eq!(listing.checked_out.as_deref(), Some("main"));
    // Git uses forward slashes on Windows, while canonicalize may return
    // a verbatim path. Compare directory identity, not those spellings.
    let mut worktrees = listing.worktrees;
    for worktree in &mut worktrees {
        worktree.path = std::fs::canonicalize(&worktree.path).unwrap();
    }
    assert_eq!(
        worktrees,
        [crate::workspace::ListedWorktree {
            path: std::fs::canonicalize(&tree).unwrap(),
            branch: Some("fix".into()),
        }]
    );
    assert_eq!(listing.branches, ["fix", "main", "spare"]);
}

/// A review reads git where the Server runs (ADR 0021): a window asks
/// for a checkout's uncommitted work, its branch's commits and one
/// commit's diff, a file of it, a search of it and its pull request, and
/// hears why when the path is no repo.
#[test]
fn a_review_is_read_by_git_where_the_server_runs() {
    let harness = start("review");
    let repo = repository(&harness.dir);
    let git = |args: &[&str]| crate::workspace::git_for_tests(&repo, args);
    git(&["checkout", "-q", "-b", "dev"]);
    std::fs::write(repo.join("README.md"), "app, reviewed\n").unwrap();
    git(&[
        "-c",
        "user.email=test@example.invalid",
        "-c",
        "user.name=test",
        "commit",
        "-qam",
        "reviewed",
    ]);
    std::fs::write(repo.join("notes.md"), "fresh\n").unwrap();
    let mut window = cockpit(&harness);
    fn landed<T>(
        window: &mut crate::cockpit::Cockpit,
        answer: io::Result<Receiver<io::Result<T>>>,
    ) -> io::Result<T> {
        let answer = answer.unwrap();
        let landed = std::cell::RefCell::new(None);
        pump_until(window, |_| {
            if let Ok(read) = answer.try_recv() {
                *landed.borrow_mut() = Some(read);
            }
            landed.borrow().is_some()
        });
        landed.into_inner().unwrap()
    }

    use ferrite_reviewr::ferrite::Read;
    let asked = window.review_diff(&repo, Read::Uncommitted);
    let uncommitted = landed(&mut window, asked).unwrap();
    assert_eq!(uncommitted.len(), 1);
    assert_eq!(uncommitted[0].file.path, "notes.md");
    assert_eq!(
        uncommitted[0].sides,
        Ok((String::new(), "fresh\n".to_string()))
    );

    let asked = window.review_commits(&repo);
    let commits = landed(&mut window, asked).unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].subject, "reviewed");
    assert_eq!(commits[0].author, "test");
    let pick = ferrite_reviewr::model::CommitPick::single(&commits[0].sha);
    let asked = window.review_diff(&repo, Read::Commits(pick));
    let files = landed(&mut window, asked).unwrap();
    assert_eq!(files[0].file.path, "README.md");
    let (old, new) = files[0].sides.clone().unwrap();
    assert_eq!(new, "app, reviewed\n");
    assert_ne!(old, new);

    let asked = window.read_review_file(&repo, "notes.md");
    assert_eq!(landed(&mut window, asked).unwrap(), "fresh\n");
    let asked = window.read_review_file(&repo, "../elsewhere");
    assert!(
        landed(&mut window, asked).is_err(),
        "only inside the checkout"
    );

    let asked = window.search_repo(&repo, "reviewed");
    let found = landed(&mut window, asked).unwrap();
    assert!(
        found.code.iter().any(|hit| hit.path == "README.md"),
        "{found:?}"
    );

    let asked = window.review_pull_request(&repo);
    let pr = landed(&mut window, asked).unwrap();
    assert!(
        pr.pr
            .is_err_and(|note| note.contains("remote named upstream or origin")),
        "no forge remote, so the note says what the PR tab needs"
    );

    let asked = window.review_diff(&harness.dir, Read::Branch);
    let refused = landed(&mut window, asked).unwrap_err();
    assert!(
        refused.to_string().contains("not a git repository"),
        "{refused}"
    );
}

/// A Thread row's PR is read by `gh` where the Server runs: a window asks
/// for a checkout's PR and hears the Server's reading. A checkout with no
/// GitHub remote has none to say, and that silence is an answer, not an
/// error.
#[test]
fn a_pull_request_is_read_by_gh_where_the_server_runs() {
    let harness = start("pull-request");
    let repo = repository(&harness.dir);
    let mut window = cockpit(&harness);
    let answer = window.pull_request(&repo).unwrap();
    let landed = std::cell::RefCell::new(None);
    // This request starts the runner's installed gh (and its git subprocesses),
    // unlike in-process fixture replies. Allow cold CLI startup under CI load.
    pump_until_within(&mut window, Duration::from_secs(30), |_| {
        if let Ok(read) = answer.try_recv() {
            *landed.borrow_mut() = Some(read);
        }
        landed.borrow().is_some()
    });
    assert_eq!(landed.into_inner().unwrap().unwrap(), None);
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
    let replaced = pending_handle(&mut window, thread, None);
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
    let current = pending_handle(&mut window, thread, Some(&replaced));
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
    // Anywhere: a turn's completion row follows its text.
    let shows_turn = |said: String| {
        move |window: &Window| {
            window.mirrors.view(thread).is_some_and(|view| {
                view.transcript()
                    .blocks()
                    .iter()
                    .any(|block| format!("{:?}", block.body).contains(&said))
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
        steady.until(shows_turn(format!("turn {nth}")));
    }

    let mut late = attach(&harness.address);
    late.watch(thread, None);
    // Tens of megabytes through a debug build: seconds, more on a busy machine.
    late.until_within(
        Duration::from_secs(120),
        shows_turn(format!("turn {}", turns - 1)),
    );
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

/// A reopened Thread replays its log before anyone watches it; the first
/// window to watch still leaves a ring behind it to resume from.
#[test]
fn a_resume_after_a_reopen_needs_no_second_history() {
    let harness = start("resume-reopened");
    let mut steady = attach(&harness.address);
    let (thread, provider) = open(&mut steady, &harness);
    provider.send(text("first life ")).unwrap();
    steady.command(2, Op::Park { thread });
    steady.command(3, Op::Revive { thread });
    steady.command(
        4,
        Op::Composing {
            thread,
            unsent: true,
        },
    );
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let heard = |text: &'static str| {
        move |window: &Window| window.said(thread).iter().any(|said| said.contains(text))
    };
    steady.watch(thread, None);
    let mut leaving = attach(&harness.address);
    leaving.watch(thread, None);
    provider.send(text("before ")).unwrap();
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

    assert_eq!(back.histories, 0, "resumed from the ring");
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
            .is_some_and(|view| !view.activity().decisions().is_empty())
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
            .is_some_and(|view| !view.activity().decisions().is_empty())
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
        window.mirrors.seq(thread).is_some()
            && window
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
    other.command(
        4,
        Op::Composing {
            thread,
            unsent: true,
        },
    );
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

/// A background shell outlives its turn; ending the Session would end it.
#[test]
fn a_restart_when_idle_waits_for_background_work() {
    use crate::progress::{ProgressEvent, TaskStatus};
    let background = |status| SessionEvent::Progress {
        event: ProgressEvent::Background {
            id: "shell".into(),
            label: "npm run dev".into(),
            status,
            detail: String::new(),
        },
    };
    let harness = start("restart-background");
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
    provider.send(background(TaskStatus::Working)).unwrap();
    provider
        .send(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();

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
        "not while the shell runs"
    );

    provider.send(background(TaskStatus::Completed)).unwrap();
    harness
        .relaunched
        .recv_timeout(Duration::from_secs(5))
        .expect("restarted once the shell ended");
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
    pump_until_within(window, Duration::from_secs(5), done);
}

fn pump_until_within(
    window: &mut crate::cockpit::Cockpit,
    limit: Duration,
    done: impl Fn(&crate::cockpit::Cockpit) -> bool,
) {
    let deadline = Instant::now() + limit;
    while !done(window) {
        assert!(Instant::now() < deadline, "timed out");
        window.pump();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Whether the window's digest of `thread` reads `said` on its wall card.
fn digest_says(window: &crate::cockpit::Cockpit, thread: ThreadId, said: &str) -> bool {
    window.thread(thread).is_some_and(|open| {
        open.digest()
            .wall
            .lines
            .iter()
            .any(|line| line.text.contains(said))
    })
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
    second.draws_transcripts([thread]);
    pump_until(&mut second, |second| shows(second, thread, "on it"));

    provider.send(decision()).unwrap();
    let pending = |window: &crate::cockpit::Cockpit| {
        window
            .thread(thread)
            .is_some_and(|open| open.pending().is_some())
    };
    pump_until(&mut window, pending);
    pump_until(&mut second, pending);
    let handle = window.thread(thread).unwrap().pending_decisions()[0]
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

/// A window never saw the requests answered before it attached. Their
/// serials still belong to the Server, for both providers' question forms.
#[test]
fn a_reopened_window_can_answer_questions() {
    for (delivery, already_pending) in [
        (crate::DecisionDelivery::Blocking, false),
        (crate::DecisionDelivery::Blocking, true),
        (crate::DecisionDelivery::Async, false),
        (crate::DecisionDelivery::Async, true),
    ] {
        let harness = start(&format!("reopen-{delivery:?}-{already_pending}"));
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
        provider.send(decision()).unwrap();
        let pending = |window: &crate::cockpit::Cockpit| {
            window
                .thread(thread)
                .is_some_and(|open| open.pending().is_some())
        };
        pump_until(&mut window, pending);
        let first_handle = window.thread(thread).unwrap().pending_decisions()[0]
            .handle
            .clone();
        assert!(window
            .respond_decision(
                thread,
                &first_handle,
                DecisionAnswer::Allow {
                    input: serde_json::json!({})
                }
            )
            .unwrap());
        pump_until(&mut window, |window| !pending(window));

        let question = SessionEvent::DecisionRequested {
            decision: Decision {
                id: "question-after-reopen".into(),
                delivery,
                kind: crate::DecisionKind::Questions(vec![crate::questions::Question {
                    id: None,
                    question: "Does typing work now?".into(),
                    header: String::new(),
                    options: vec![crate::questions::Choice {
                        label: "Yes".into(),
                        description: String::new(),
                        preview: None,
                    }],
                    multi_select: false,
                    secret: false,
                    allow_other: true,
                }]),
                policy: Default::default(),
                tool_use_id: "question-call".into(),
                tool_name: "Question".into(),
                description: String::new(),
                input: serde_json::json!({}),
                suggestions: vec![],
            },
        };
        if already_pending {
            provider.send(question.clone()).unwrap();
            pump_until(&mut window, pending);
        }
        drop(window);
        let mut reopened = cockpit(&harness);
        reopened.draws_transcripts([thread]);
        pump_until(&mut reopened, |window| {
            window
                .thread(thread)
                .is_some_and(|open| open.activity().connected())
        });
        if !already_pending {
            provider.send(question).unwrap();
        }
        pump_until(&mut reopened, pending);
        let handle = reopened.thread(thread).unwrap().pending_decisions()[0]
            .handle
            .clone();
        assert_eq!(
            handle.serial,
            first_handle.serial + 1,
            "the window must retain the Server's identity"
        );
        let answer = DecisionAnswer::Questions {
            answers: vec![if already_pending {
                crate::questions::Answer {
                    picks: vec![],
                    other: Some("Typing works".into()),
                }
            } else {
                crate::questions::Answer {
                    picks: vec![0],
                    other: None,
                }
            }],
        };
        let sent = reopened
            .respond_decision(thread, &handle, answer.clone())
            .unwrap();
        assert!(
            sent,
            "Send must reach the provider after reopening the window: {delivery:?}, {handle:?}"
        );
        assert_eq!(harness.answers.lock().unwrap().len(), 2);
        assert_eq!(harness.answers.lock().unwrap().last().unwrap().1, answer);
        if delivery == crate::DecisionDelivery::Async {
            pump_until(&mut reopened, |window| {
                window.thread(thread).unwrap().pending_decisions()[0].submitting
            });
            provider
                .send(SessionEvent::Activity(
                    crate::activity::ActivityEvent::DecisionReply {
                        id: handle.request_id.clone(),
                        error: None,
                    },
                ))
                .unwrap();
        }
        pump_until(&mut reopened, |window| !pending(window));
    }
}

/// An Engine on its own Store, its Sessions scripted.
pub(super) fn engine(dir: &Path, spawner: Box<dyn Spawner>) -> Engine {
    Engine::try_new(Store::open(dir).unwrap(), spawner).unwrap()
}

/// Scripted Sessions, and where their providers arrive: held, so each
/// Session lives as long as the test.
pub(super) fn spawn() -> (Box<dyn Spawner>, Receiver<Sender<SessionEvent>>) {
    let (sessions, providers) = mpsc::channel();
    let spawner = Spawn {
        sessions,
        answers: Answers::default(),
        queued: Queued::default(),
        gate: None,
        can_queue: None,
        controls: Controls::default(),
    };
    (Box::new(spawner), providers)
}

/// A Thread an earlier Server left open, and this one's Engine on the same
/// Store after its boot reopened it, its agents started by `spawner`.
fn restored(name: &str, spawner: Box<dyn Spawner>) -> (Engine, ThreadId) {
    restored_after(name, spawner, |_, _, _| {})
}

/// `restored`, once the earlier Server's Thread had `lived`: given its
/// Engine, the Thread and the Sender playing its agent.
fn restored_after(
    name: &str,
    spawner: Box<dyn Spawner>,
    lived: impl FnOnce(&mut Engine, ThreadId, &Sender<SessionEvent>),
) -> (Engine, ThreadId) {
    let dir = scratch(name);
    let store = dir.join("threads");
    let (first_spawner, providers) = spawn();
    let mut first = engine(&store, first_spawner);
    let thread = open_in(&mut first, &dir);
    let agent = providers.recv().unwrap();
    lived(&mut first, thread, &agent);
    first.halt_sessions();
    drop(first);
    let mut next = engine(&store, spawner);
    revive_launch(&mut next, launch_threads(&Store::open(&store).unwrap()));
    // A window shows it.
    next.set_watched([thread].into());
    (next, thread)
}

/// The operator spoke to the agent, which named its conversation `claude-1`.
fn spoken(engine: &mut Engine, thread: ThreadId, agent: &Sender<SessionEvent>) {
    agent
        .send(SessionEvent::Init {
            session_id: "claude-1".into(),
            model: "claude-model".into(),
        })
        .unwrap();
    engine.send(thread, "first".into());
    pump_engine_until(engine, "the Init never landed", |engine| {
        engine.thread(thread).unwrap().provider_session() == Some("claude-1")
    });
}

/// Pump `engine` until `done`, failing as `what` after five seconds.
fn pump_engine_until(engine: &mut Engine, what: &str, done: impl Fn(&Engine) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(engine) {
        assert!(Instant::now() < deadline, "{what}");
        engine.pump();
    }
}

/// The operator's prompts in `thread`'s transcript, in order.
fn prompts(engine: &Engine, thread: ThreadId) -> Vec<String> {
    engine
        .thread(thread)
        .unwrap()
        .transcript()
        .blocks()
        .iter()
        .filter_map(|block| match &block.body {
            Body::Prompt(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

pub(super) fn open_in(engine: &mut Engine, dir: &Path) -> ThreadId {
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

/// ADR 0032: the open Threads come back open, but nothing is running in
/// them, so no agent starts until one is used.
#[test]
fn a_boot_reopens_every_open_thread_without_starting_its_agent() {
    let dir = scratch("open-resume");
    let store = dir.join("threads");
    let (spawner, _providers) = spawn();
    let mut first = engine(&store, spawner);
    let one = open_in(&mut first, &dir);
    let two = open_in(&mut first, &dir);
    first.halt_sessions();
    drop(first);

    let (spawner, resumed) = spawn();
    let mut next = engine(&store, spawner);
    revive_launch(&mut next, launch_threads(&Store::open(&store).unwrap()));
    assert_eq!(next.threads(), vec![one, two]);
    assert!(resumed.try_recv().is_err(), "no agent started");
}

/// Codex keeps a Thread's queued prompts across its Session, but only a
/// running agent sends them: a Thread whose saved queue still holds some
/// starts its agent at boot.
#[test]
fn a_boot_starts_the_agent_of_a_thread_with_queued_prompts() {
    let dir = scratch("open-queued");
    let store = dir.join("threads");
    let (spawner, _providers) = spawn();
    let mut first = engine(&store, spawner);
    let idle = open_in(&mut first, &dir);
    let queued = first
        .open(
            Provider::Codex,
            crate::workspace::WorkspaceChoice::Main {
                checkout: dir.clone(),
            },
        )
        .unwrap();
    first.halt_sessions();
    drop(first);
    std::fs::write(
        store.join(queued.to_string()).join("queue.json"),
        r#"{"pending":{"client-1":"after this, run the tests"},"consumed":[]}"#,
    )
    .unwrap();

    let (spawner, resumed) = spawn();
    let mut next = engine(&store, spawner);
    revive_launch(&mut next, launch_threads(&Store::open(&store).unwrap()));
    assert_eq!(next.threads(), vec![idle, queued]);
    resumed
        .recv_timeout(Duration::from_secs(5))
        .expect("the queued Thread's agent started");
    assert!(resumed.try_recv().is_err(), "and only that one");
}

/// What an agent was started with.
#[derive(Debug, PartialEq)]
struct Start {
    provider: Provider,
    model: Option<String>,
    effort: Option<String>,
    resume: Option<String>,
    permission_mode: Option<String>,
}

impl Start {
    fn of(request: &SpawnRequest) -> Self {
        let owned = |word: Option<&str>| word.map(str::to_string);
        Self {
            provider: request.provider,
            model: owned(request.model),
            effort: owned(request.effort),
            resume: owned(request.resume),
            permission_mode: owned(request.permission_mode),
        }
    }

    /// `provider`'s agent with nothing chosen and nothing to resume.
    fn bare(provider: Provider) -> Self {
        Self {
            provider,
            model: None,
            effort: None,
            resume: None,
            permission_mode: None,
        }
    }
}

/// Each agent started, in order.
type Started = Arc<Mutex<Vec<Start>>>;

/// `inner`'s Sessions, each start recorded.
struct Recorded {
    inner: Box<dyn Spawner>,
    started: Started,
}

impl Spawner for Recorded {
    fn spawn(&mut self, request: SpawnRequest) -> io::Result<Box<dyn Session>> {
        self.started.lock().unwrap().push(Start::of(&request));
        self.inner.spawn(request)
    }
    fn start(&mut self, request: SpawnRequest) -> io::Result<crate::session::SessionLifecycle> {
        self.started.lock().unwrap().push(Start::of(&request));
        self.inner.start(request)
    }
}

/// `inner`, each start recorded.
fn recording(inner: Box<dyn Spawner>) -> (Box<dyn Spawner>, Started) {
    let started = Started::default();
    let spawner = Recorded {
        inner,
        started: started.clone(),
    };
    (Box::new(spawner), started)
}

/// `spawn`, each start recorded.
fn recorded() -> (Box<dyn Spawner>, Started, Receiver<Sender<SessionEvent>>) {
    let (inner, providers) = spawn();
    let (spawner, started) = recording(inner);
    (spawner, started, providers)
}

/// A Thread that came back with nothing running starts its agent when it
/// is sent a prompt, on the conversation it left, and reads as if the agent
/// had been there all along.
#[test]
fn a_restored_thread_starts_its_agent_on_send_without_a_trace() {
    let (spawner, started, _agents) = recorded();
    let (mut next, thread) = restored_after("restored-send", spawner, spoken);
    let rows = |engine: &Engine| {
        engine
            .thread(thread)
            .unwrap()
            .transcript()
            .blocks()
            .iter()
            .map(|block| block.body.clone())
            .collect::<Vec<_>>()
    };
    let before = rows(&next);
    assert!(
        !before.iter().any(|body| matches!(body, Body::Meta(_))),
        "{before:?}"
    );

    next.send(thread, "carry on".into());
    next.pump();
    assert_eq!(
        *started.lock().unwrap(),
        vec![Start {
            resume: Some("claude-1".into()),
            ..Start::bare(Provider::Claude)
        }]
    );
    let mut expected = before;
    expected.push(Body::Prompt("carry on".into()));
    assert_eq!(rows(&next), expected);
}

/// Whether the Engine still holds the Session `agent` plays.
fn running(agent: &Sender<SessionEvent>) -> bool {
    agent
        .send(SessionEvent::ModelChanged {
            model: "claude-model".into(),
        })
        .is_ok()
}

/// A Thread whose agent named its conversation `claude-1` and has nothing
/// to do, on a clock frozen for the test: the Engine, the Thread, the
/// agent and the clock.
fn at_rest(
    name: &str,
    spawner: Box<dyn Spawner>,
    providers: &Receiver<Sender<SessionEvent>>,
) -> (
    Engine,
    ThreadId,
    Sender<SessionEvent>,
    crate::clock::ThreadFixture,
) {
    let clock = crate::clock::ThreadFixture::install(chrono::Local::now());
    let dir = scratch(name);
    let mut engine = engine(&dir.join("threads"), spawner);
    let thread = open_in(&mut engine, &dir);
    let agent = providers.recv().unwrap();
    agent
        .send(SessionEvent::Init {
            session_id: "claude-1".into(),
            model: "claude-model".into(),
        })
        .unwrap();
    pump_engine_until(&mut engine, "the Init never landed", |engine| {
        engine.thread(thread).unwrap().provider_session() == Some("claude-1")
    });
    (engine, thread, agent, clock)
}

/// An agent with nothing to do for its span at rest stops, and nothing shows
/// it: the transcript, status and generation stand, and the next prompt
/// starts one on the same conversation.
#[test]
fn an_agent_settled_its_span_stops_without_a_trace() {
    let (spawner, started, providers) = recorded();
    let (mut engine, thread, agent, clock) = at_rest("settled-stops", spawner, &providers);
    let seen = |engine: &Engine| {
        let view = engine.thread(thread).unwrap();
        let rows: Vec<Body> = view
            .transcript()
            .blocks()
            .iter()
            .map(|block| block.body.clone())
            .collect();
        (rows, view.generation(), view.starting(), view.busy())
    };
    let before = seen(&engine);

    clock.advance(HELD_AT_REST - Duration::from_secs(1));
    engine.pump();
    assert!(running(&agent), "not yet its span");
    engine.pump();
    clock.advance(Duration::from_secs(1));
    engine.pump();
    assert!(!running(&agent), "the agent stopped");
    assert_eq!(seen(&engine), before);

    started.lock().unwrap().clear();
    engine.send(thread, "carry on".into());
    engine.pump();
    assert_eq!(
        *started.lock().unwrap(),
        vec![Start {
            resume: Some("claude-1".into()),
            ..Start::bare(Provider::Claude)
        }]
    );
}

/// A provider's CLI replaced on disk ends its settled agents at once, as
/// unseen as its span at rest would, so the next start runs the new release.
#[test]
fn an_updated_cli_ends_a_settled_agent_without_a_trace() {
    let (spawner, providers) = spawn();
    let (mut engine, thread, agent, _clock) = at_rest("updated-cli", spawner, &providers);
    let rows = |engine: &Engine| engine.thread(thread).unwrap().transcript().blocks().len();
    let before = rows(&engine);
    engine.end_stale_sessions(Provider::Claude);
    engine.pump();
    assert!(!running(&agent), "the agent stopped");
    assert_eq!(rows(&engine), before, "nothing shows it");
}

/// Work in flight holds the agent however long it runs, and its ten
/// minutes count from when the work settled, not from before it.
#[test]
fn work_in_flight_keeps_the_agent_and_restarts_its_span() {
    let (spawner, providers) = spawn();
    let (mut engine, _thread, agent, clock) = at_rest("in-flight", spawner, &providers);
    let background = |tasks: Vec<crate::progress::BackgroundTask>| SessionEvent::Progress {
        event: crate::progress::ProgressEvent::BackgroundSnapshot { tasks },
    };
    clock.advance(HELD_AT_REST / 2);
    engine.pump();
    agent
        .send(background(vec![crate::progress::BackgroundTask {
            id: "build".into(),
            label: "cargo build".into(),
            status: crate::progress::TaskStatus::Working,
            detail: String::new(),
        }]))
        .unwrap();
    engine.pump();
    clock.advance(HELD_AT_REST);
    engine.pump();
    assert!(running(&agent), "a background task holds it");

    agent.send(background(Vec::new())).unwrap();
    engine.pump();
    clock.advance(HELD_AT_REST - Duration::from_secs(1));
    engine.pump();
    assert!(running(&agent), "its span counts from the settle");
    clock.advance(Duration::from_secs(1));
    engine.pump();
    assert!(!running(&agent), "the agent stopped");
}

/// Asking an agent something it answers in its own time (a control, a
/// file search) is using it: its its span at rest count from the ask.
#[test]
fn an_ask_restarts_the_agents_ten_minutes() {
    let (spawner, providers) = spawn();
    let (mut engine, thread, agent, clock) = at_rest("asked", spawner, &providers);
    clock.advance(HELD_AT_REST - Duration::from_secs(1));
    engine.pump();
    engine
        .control(thread, crate::SessionControl::RefreshMcp)
        .unwrap();
    clock.advance(Duration::from_secs(1));
    engine.pump();
    assert!(running(&agent), "a control was just asked of it");

    clock.advance(HELD_AT_REST - Duration::from_secs(2));
    engine.pump();
    let _answer = engine.search_files(thread, "main.rs").unwrap();
    clock.advance(HELD_AT_REST - Duration::from_secs(1));
    engine.pump();
    assert!(running(&agent), "a file search was just asked of it");
    clock.advance(Duration::from_secs(1));
    engine.pump();
    assert!(!running(&agent), "the agent stopped");
}

/// An agent holding work it will start itself, out of Ferrite's sight,
/// stays however long it is settled, until that work is gone.
#[test]
fn scheduled_work_keeps_the_agent() {
    let (spawner, providers) = spawn();
    let (mut engine, _thread, agent, clock) = at_rest("scheduled", spawner, &providers);
    agent
        .send(SessionEvent::ScheduledWork { pending: true })
        .unwrap();
    engine.pump();
    clock.advance(HELD_AT_REST * 3);
    engine.pump();
    assert!(running(&agent), "scheduled work holds it");

    agent
        .send(SessionEvent::ScheduledWork { pending: false })
        .unwrap();
    engine.pump();
    clock.advance(HELD_AT_REST);
    engine.pump();
    assert!(!running(&agent), "the agent stopped");
}

/// Scheduled work is the Thread's to hold, not its Activity's: windows
/// are told nothing of it, and what the agent says after still crosses.
#[test]
fn scheduled_work_crosses_to_no_window() {
    let (spawner, providers) = spawn();
    let (mut engine, _thread, agent, _clock) = at_rest("scheduled-wire", spawner, &providers);
    let _ = engine.take_messages();
    for pending in [true, false] {
        agent.send(SessionEvent::ScheduledWork { pending }).unwrap();
    }
    agent.send(text("after\n\n")).unwrap();
    engine.pump();

    let messages = engine.take_messages();
    let inputs = messages
        .iter()
        .filter(|message| matches!(message, ServerMessage::Input { .. }))
        .count();
    assert_eq!(inputs, 1, "only what it said: {messages:?}");
}

/// A model, effort or Provider chosen for a Thread whose agent is not
/// running starts none: the choice is kept, and the next agent starts with
/// it, in the permission mode the last one ran in.
#[test]
fn a_choice_made_without_an_agent_is_the_next_agents() {
    let (spawner, started, _agents) = recorded();
    let (mut engine, thread) =
        restored_after("restored-model", spawner, |engine, thread, agent| {
            spoken(engine, thread, agent);
            agent
                .send(SessionEvent::PermissionMode {
                    mode: "plan".into(),
                })
                .unwrap();
            pump_engine_until(engine, "the mode never landed", |engine| {
                engine.thread(thread).unwrap().permission_mode() == Some("plan")
            });
        });
    engine.set_model(thread, Some("opus".into())).unwrap();
    engine.set_effort(thread, Some("high".into())).unwrap();
    assert!(started.lock().unwrap().is_empty(), "no agent started");
    let view = engine.thread(thread).unwrap();
    assert_eq!((view.model(), view.effort()), (Some("opus"), Some("high")));
    engine.send(thread, "carry on".into());
    engine.pump();
    assert_eq!(
        *started.lock().unwrap(),
        vec![Start {
            model: Some("opus".into()),
            effort: Some("high".into()),
            resume: Some("claude-1".into()),
            permission_mode: Some("plan".into()),
            ..Start::bare(Provider::Claude)
        }]
    );

    let (spawner, started, _agents) = recorded();
    let (mut engine, thread) = restored_after("restored-provider", spawner, spoken);
    engine
        .set_provider(
            thread,
            crate::engine::ProviderChoice {
                provider: Provider::Codex,
                model: None,
            },
        )
        .unwrap();
    engine.pump();
    assert!(started.lock().unwrap().is_empty(), "no agent started");
    assert_eq!(engine.thread(thread).unwrap().provider(), Provider::Codex);
    engine.send(thread, "carry on".into());
    engine.pump();
    assert_eq!(*started.lock().unwrap(), vec![Start::bare(Provider::Codex)]);
}

/// A model chosen while the agent started for the operator's writing is
/// still starting is not refused: the next agent starts with it.
#[test]
fn a_choice_made_while_the_agent_starts_unseen_is_kept() {
    let (inner, _agents, _gate) = spawn_gated();
    let (spawner, started) = recording(inner);
    let (mut engine, thread) = restored("restored-unseen-choice", spawner);
    composing(&mut engine, thread);
    assert_eq!(started.lock().unwrap().len(), 1);

    engine.set_model(thread, Some("opus".into())).unwrap();
    let started = started.lock().unwrap();
    assert_eq!(started.len(), 2, "still writing: the next starts at once");
    assert_eq!(started[1].model.as_deref(), Some("opus"));
}

/// Escape while a prompt waits for the agent to start stops it: the prompt
/// goes back to the Composer, and nothing starts again for it.
#[test]
fn escape_while_the_agent_starts_gives_the_prompt_back() {
    let (spawner, started, _gate) = spawn_gated();
    let (mut engine, thread) = restored("restored-escape", spawner);
    engine.send(thread, "go".into());
    started
        .recv_timeout(Duration::from_secs(5))
        .expect("the send started the agent");
    engine.interrupt(thread);
    engine.pump();
    engine.pump();
    assert!(started.try_recv().is_err(), "nothing started again");
    assert_eq!(
        engine.take_retrieved_prompt(thread),
        Some(("go".to_string(), false))
    );
    assert!(!engine.thread(thread).unwrap().starting());
}

/// Scripted Sessions that start only as the returned Sender lets each
/// through, and where their providers arrive.
fn spawn_gated() -> (Box<dyn Spawner>, Receiver<Sender<SessionEvent>>, Sender<()>) {
    let (sessions, providers) = mpsc::channel();
    let (gate, opens) = mpsc::channel();
    let spawner = Spawn {
        sessions,
        answers: Answers::default(),
        queued: Queued::default(),
        gate: Some(Arc::new(Mutex::new(opens))),
        can_queue: None,
        controls: Controls::default(),
    };
    (Box::new(spawner), providers, gate)
}

fn composing(engine: &mut Engine, thread: ThreadId) {
    engine.window_says(
        1,
        &Op::Composing {
            thread,
            unsent: true,
        },
    );
    engine.pump();
}

/// What the agent last said of itself: its commands, models, permission
/// modes, controls and MCP servers.
fn announcements() -> Vec<SessionEvent> {
    vec![
        SessionEvent::Commands {
            commands: vec![crate::SessionCommand {
                name: "review".into(),
                description: "Review the change".into(),
                path: None,
            }],
        },
        SessionEvent::Models {
            models: vec![crate::ModelInfo::bare("opus")],
        },
        SessionEvent::PermissionMode {
            mode: "plan".into(),
        },
        SessionEvent::McpServers {
            servers: vec![crate::McpServer {
                name: "linear".into(),
                status: crate::McpStatus::NeedsAuth,
                error: None,
            }],
        },
    ]
}

/// A Thread with no agent still offers what its last one said of itself,
/// after a restart too, without starting one to ask.
#[test]
fn a_thread_without_its_agent_offers_what_the_agent_last_said() {
    let (spawner, started) = spawn();
    let (engine, thread) =
        restored_after("restored-announced", spawner, |engine, thread, agent| {
            for event in announcements() {
                agent.send(event).unwrap();
            }
            pump_engine_until(engine, "the agent was not heard", |engine| {
                let view = engine.thread(thread).unwrap();
                !view.transcript().mcp_servers().is_empty()
                    && view.supports_control(crate::ControlKind::RefreshMcp)
            });
        });
    let view = engine.thread(thread).unwrap();
    assert_eq!(view.commands()[0].name, "review");
    assert_eq!(view.models()[0].value, "opus");
    assert_eq!(view.permission_mode(), Some("plan"));
    assert_eq!(view.permission_modes().len(), 2);
    assert!(view.supports_control(crate::ControlKind::RefreshMcp));
    assert!(!view.supports_control(crate::ControlKind::StopTask));
    assert_eq!(view.transcript().mcp_servers()[0].name, "linear");
    assert!(started.try_recv().is_err(), "no agent started to ask");
}

/// The operator starting to write to a restored Thread starts its agent,
/// so it is ready by the time the prompt is sent; nothing shows it, and
/// nothing is sent for it.
#[test]
fn writing_to_a_restored_thread_starts_its_agent_unseen() {
    let (spawner, started, _gate) = spawn_gated();
    let (mut engine, thread) = restored("restored-compose", spawner);
    let view = engine.thread(thread).unwrap();
    let before = (view.transcript().blocks().to_vec(), view.generation());
    composing(&mut engine, thread);

    started
        .recv_timeout(Duration::from_secs(5))
        .expect("the agent started");
    let view = engine.thread(thread).unwrap();
    assert!(!view.starting(), "a wake shows nothing");
    assert_eq!(
        (view.transcript().blocks().to_vec(), view.generation()),
        before,
        "and invalidates nothing a window holds"
    );

    engine.send(thread, "go".into());
    assert!(
        engine.thread(thread).unwrap().starting(),
        "a prompt waiting on it shows it starting"
    );
}

/// A Thread whose session project root is gone starts no agent for a
/// keystroke: the send it leads to refuses, naming the path.
#[test]
fn writing_to_a_thread_whose_root_is_gone_starts_no_agent() {
    let (spawner, started) = spawn();
    let (mut engine, thread) = restored("restored-vanished", spawner);
    let root = scratch("restored-vanished-root");
    engine
        .set_session_project_root(thread, Some(root.clone()))
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    composing(&mut engine, thread);
    assert!(started.try_recv().is_err(), "no agent started");
}

/// A Provider change while the Thread's agent starts starts the new one;
/// writing to the Thread meanwhile starts no other.
#[test]
fn writing_during_a_provider_change_starts_no_other_agent() {
    let (spawner, started, _gate) = spawn_gated();
    let (mut engine, thread) = restored("restored-switch", spawner);
    composing(&mut engine, thread);
    started
        .recv_timeout(Duration::from_secs(5))
        .expect("writing started the agent");
    engine
        .set_provider(
            thread,
            crate::engine::ProviderChoice {
                provider: Provider::Codex,
                model: None,
            },
        )
        .unwrap();
    started
        .recv_timeout(Duration::from_secs(5))
        .expect("the change started an agent");
    composing(&mut engine, thread);
    assert!(started.try_recv().is_err(), "and nothing else did");
}

/// A control or file search asked of a Thread whose agent is not running
/// starts it, unseen, and reaches it once it is up.
#[test]
fn asking_a_thread_without_its_agent_starts_it_and_asks_it() {
    let (sessions, started) = mpsc::channel();
    let (gate, opens) = mpsc::channel();
    let controls = Controls::default();
    let spawner = Spawn {
        sessions,
        answers: Answers::default(),
        queued: Queued::default(),
        gate: Some(Arc::new(Mutex::new(opens))),
        can_queue: None,
        controls: controls.clone(),
    };
    let (mut engine, thread) =
        restored_after("restored-ask", Box::new(spawner), |engine, thread, _| {
            pump_engine_until(engine, "the agent's controls were not heard", |engine| {
                let view = engine.thread(thread).unwrap();
                view.supports_control(crate::ControlKind::RefreshMcp)
            });
        });

    assert!(
        engine
            .control(thread, crate::SessionControl::ReloadMcp)
            .is_err(),
        "one the agent did not offer is refused, and starts nothing"
    );
    assert!(started.try_recv().is_err());
    engine
        .control(thread, crate::SessionControl::RefreshMcp)
        .unwrap();
    let files = engine.search_files(thread, "parser.rs").unwrap();
    let _agent = started
        .recv_timeout(Duration::from_secs(5))
        .expect("asking started the agent");
    assert!(started.try_recv().is_err(), "once");
    assert!(!engine.thread(thread).unwrap().starting(), "unseen");
    assert!(controls.lock().unwrap().is_empty(), "not before it is up");

    gate.send(()).unwrap();
    pump_engine_until(&mut engine, "the control never reached the agent", |_| {
        !controls.lock().unwrap().is_empty()
    });
    assert_eq!(
        *controls.lock().unwrap(),
        vec![crate::SessionControl::RefreshMcp]
    );
    let found = files.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    assert_eq!(found[0].path, "parser.rs");
}

/// A restored Thread's agent takes a moment to start; what the operator
/// queues meanwhile waits for it, in order, instead of being refused.
#[test]
fn prompts_queued_while_the_agent_starts_wait_for_it() {
    let (sessions, _providers) = mpsc::channel();
    let (gate, opens) = mpsc::channel();
    let queued = Queued::default();
    let (mut engine, thread) = restored(
        "restored-queue",
        Box::new(Spawn {
            sessions,
            answers: Answers::default(),
            queued: queued.clone(),
            gate: Some(Arc::new(Mutex::new(opens))),
            can_queue: None,
            controls: Controls::default(),
        }),
    );
    engine.send(thread, "first".into());
    assert!(engine.thread(thread).unwrap().starting());
    assert!(engine.queue(thread, "second".into()), "queued, not refused");
    assert_eq!(
        engine.thread(thread).unwrap().queued_all(),
        vec!["second", "first"]
    );

    gate.send(()).unwrap();
    pump_engine_until(&mut engine, "the agent never started", |engine| {
        !engine.thread(thread).unwrap().starting()
    });
    assert_eq!(prompts(&engine, thread), vec!["first"]);
    assert_eq!(queued.lock().unwrap().len(), 1, "second waits in its queue");
}

/// Claude can queue a prompt only once its first turn has started. What
/// waited behind the first prompt for its agent waits on, in order, until
/// the provider can queue it, or goes out as its own turn when the one
/// ahead ends first.
#[test]
fn prompts_wait_for_a_provider_that_cannot_queue_yet() {
    let (sessions, providers) = mpsc::channel();
    let (gate, opens) = mpsc::channel();
    let queued = Queued::default();
    let can_queue = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (mut engine, thread) = restored(
        "restored-cannot-queue",
        Box::new(Spawn {
            sessions,
            answers: Answers::default(),
            queued: queued.clone(),
            gate: Some(Arc::new(Mutex::new(opens))),
            can_queue: Some(can_queue.clone()),
            controls: Controls::default(),
        }),
    );
    engine.send(thread, "first".into());
    assert!(engine.queue(thread, "second".into()));
    assert!(engine.queue(thread, "third".into()));

    gate.send(()).unwrap();
    pump_engine_until(&mut engine, "the agent never started", |engine| {
        !engine.thread(thread).unwrap().starting()
    });
    engine.pump();
    assert_eq!(prompts(&engine, thread), vec!["first"]);
    assert_eq!(
        engine.thread(thread).unwrap().queued_all(),
        vec!["third", "second"],
        "nothing is lost while the provider cannot queue"
    );

    let provider: Sender<SessionEvent> = providers.recv().unwrap();
    provider
        .send(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    pump_engine_until(&mut engine, "second never went out", |engine| {
        prompts(engine, thread).len() == 2
    });
    assert_eq!(prompts(&engine, thread), vec!["first", "second"]);
    assert_eq!(engine.thread(thread).unwrap().queued_all(), vec!["third"]);

    can_queue.store(true, std::sync::atomic::Ordering::SeqCst);
    pump_engine_until(&mut engine, "third never reached the queue", |_| {
        queued.lock().unwrap().len() == 1
    });
    assert_eq!(prompts(&engine, thread), vec!["first", "second"]);
}

/// Reopening a parked Thread shows it again; its agent starts only when
/// the Thread is used (ADR 0032).
#[test]
fn reopening_a_thread_starts_no_agent() {
    let harness = start("reopen-lazy");
    let mut window = attach(&harness.address);
    let (thread, _provider) = open(&mut window, &harness);
    window.command(2, Op::Park { thread });
    let reply = window.command(3, Op::Revive { thread });
    assert!(
        matches!(reply, ServerMessage::Reply { error: None, .. }),
        "{reply:?}"
    );
    assert!(
        harness
            .sessions
            .recv_timeout(Duration::from_millis(300))
            .is_err(),
        "no agent started"
    );
}

/// The rows of the next `parked` message: the Engine reads them off its
/// pump thread.
fn next_parked(engine: &mut Engine) -> Vec<protocol::ParkedThread> {
    for _ in 0..500 {
        let rows = engine
            .take_messages()
            .into_iter()
            .rev()
            .find_map(|message| match message {
                ServerMessage::Parked { threads, .. } => Some(threads),
                _ => None,
            });
        if let Some(rows) = rows {
            return rows;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no parked message");
}

/// ADR 0026: a window on a machine lists its parked Threads off the
/// `parked` message. Nothing is read until such a window asks, and a row
/// is read again whenever its Thread changes while it stays parked.
#[test]
fn the_parked_rows_follow_every_change_to_a_parked_thread() {
    let dir = scratch("parked-rows");
    let (spawner, _providers) = spawn();
    let mut engine = engine(&dir.join("threads"), spawner);
    let thread = open_in(&mut engine, &dir);
    engine.park(thread).unwrap();
    for _ in 0..5 {
        assert!(
            !engine
                .take_messages()
                .iter()
                .any(|message| matches!(message, ServerMessage::Parked { .. })),
            "no window lists them: nothing is read"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    engine.list_parked(true);
    let rows = next_parked(&mut engine);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].thread, thread);
    assert_eq!(rows[0].provider, Some(Provider::Claude));

    engine.rename_thread(thread, "Release notes").unwrap();
    assert_eq!(
        next_parked(&mut engine)[0].title.as_deref(),
        Some("Release notes")
    );

    engine
        .set_provider(
            thread,
            crate::engine::ProviderChoice {
                provider: Provider::Codex,
                model: None,
            },
        )
        .unwrap();
    assert_eq!(next_parked(&mut engine)[0].provider, Some(Provider::Codex));

    let file = dir.join("adopted.jsonl");
    std::fs::write(
        &file,
        concat!(
            r#"{"type":"user","sessionId":"rows-9c1d","cwd":"/workspace","message":{"role":"user","content":"first question"}}"#,
            "\n",
            r#"{"type":"assistant","sessionId":"rows-9c1d","message":{"model":"claude-haiku-4-5","content":[{"type":"text","text":"first answer"}]}}"#,
            "\n",
        ),
    )
    .unwrap();
    let imported = engine.import(&file).unwrap();
    let listed = |rows: Vec<protocol::ParkedThread>| -> Vec<ThreadId> {
        rows.into_iter().map(|row| row.thread).collect()
    };
    assert_eq!(listed(next_parked(&mut engine)), [thread, imported]);

    engine.delete(thread).unwrap();
    assert_eq!(listed(next_parked(&mut engine)), [imported]);
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

pub(super) fn turn_ends(provider: &Sender<SessionEvent>) {
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

/// A Decision whose answer is being submitted (an async one's, which the
/// provider closes later) is answered already: `decision.list` no longer
/// offers it, so a key meant for the next Decision never answers it twice.
#[test]
fn a_decision_whose_answer_is_being_submitted_is_not_listed() {
    let harness = start("api-submitting");
    let mut window = attach(&harness.address);
    let (thread, provider) = open(&mut window, &harness);
    window.watch(thread, None);
    let mut caller = call(&harness);

    let SessionEvent::DecisionRequested { mut decision } = decision() else {
        unreachable!()
    };
    decision.delivery = crate::DecisionDelivery::Async;
    provider
        .send(SessionEvent::DecisionRequested { decision })
        .unwrap();
    window.until(|window| !window.decisions.is_empty());
    let listed = caller.answer("1", "decision.list", serde_json::json!({}));
    assert_eq!(
        listed["result"]["decisions"][0]["decision"]["id"], "perm_01",
        "{listed}"
    );

    let answered = caller.answer(
        "2",
        "decision.answer",
        serde_json::json!({
            "thread": thread,
            "decision": "perm_01",
            "answer": { "kind": "allow", "input": {} },
        }),
    );
    assert!(answered.get("result").is_some(), "{answered}");
    let listed = caller.answer("3", "decision.list", serde_json::json!({}));
    assert_eq!(
        listed["result"]["decisions"],
        serde_json::json!([]),
        "{listed}"
    );
    assert_eq!(harness.answers.lock().unwrap().len(), 1);
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
        let message = window
            .client
            .messages
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        if matches!(
            message,
            ServerMessage::Focus { .. } | ServerMessage::Notification { .. }
        ) {
            told.push(message);
        }
    }
    assert_eq!(
        told,
        [
            ServerMessage::Focus { thread },
            ServerMessage::Notification {
                title: "Review ready".into(),
                body: None,
                sound: protocol::WireSound::None,
                important: false,
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
/// wait does not stall meanwhile; a second waits behind it, in the
/// provider's queue once the Session is up (ADR 0032).
#[test]
fn a_starting_session_holds_one_prompt_and_queues_another() {
    let (harness, gate) = start_gated("api-starting");
    let mut caller = call(&harness);
    let started = caller.answer(
        "1",
        "thread.start",
        serde_json::json!({ "cwd": harness.dir }),
    );
    let thread = started["result"]["thread"]["thread"].as_u64().unwrap();
    let _provider = harness
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
    assert!(again.get("error").is_none(), "{again}");

    gate.send(()).unwrap();
    let working = caller.answer(
        "4",
        "agent.wait",
        serde_json::json!({ "thread": thread, "until": "working" }),
    );
    assert!(working.get("result").is_some(), "{working}");
    let deadline = Instant::now() + Duration::from_secs(5);
    while harness.queued.lock().unwrap().is_empty() {
        assert!(
            Instant::now() < deadline,
            "the second never reached the provider's queue"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(harness.queued.lock().unwrap().len(), 1);
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

/// A window's own word about itself, as its Cockpit says it.
fn tell(window: &mut Window, id: u64, op: Op) {
    let reply = window.command(id, op);
    assert!(
        matches!(reply, ServerMessage::Reply { error: None, .. }),
        "{reply:?}"
    );
}

/// The next message of one kind a window is told, within five seconds.
fn told(window: &mut Window, wanted: impl Fn(&ServerMessage) -> bool) -> ServerMessage {
    loop {
        let message = window
            .client
            .messages
            .recv_timeout(Duration::from_secs(5))
            .expect("a message within five seconds");
        if wanted(&message) {
            return message;
        }
    }
}

/// What herdr-projects reads of an agent (ADR 0030): a Thread started
/// `ready` answers once it is up, landing the windows on it; the windows
/// say which Thread the operator is looking at and which Composer holds
/// unsent text; Attention changes are counted; a prompt `until` working
/// answers as soon as the agent took it.
#[test]
fn a_thread_reports_what_a_coordinator_reads_of_it() {
    let harness = start("api-facts");
    let mut window = attach(&harness.address);
    let mut caller = call(&harness);
    let started = caller.answer(
        "1",
        "thread.start",
        serde_json::json!({ "cwd": harness.dir, "ready": true, "focus": true }),
    );
    let info = &started["result"]["thread"];
    let thread = info["thread"].as_u64().unwrap();
    assert_eq!(info["attention"], "idle", "{started}");
    assert!(info["project"].is_string(), "{started}");
    for (fact, value) in [
        ("focused", serde_json::json!(false)),
        ("draft", serde_json::json!(false)),
        ("held_prompt", serde_json::json!(false)),
        ("queued_prompts", serde_json::json!(0)),
        ("attention_seq", serde_json::json!(0)),
    ] {
        assert_eq!(info[fact], value, "{fact}: {started}");
    }
    assert!(info.get("worktree").is_none(), "{started}");
    assert!(
        info.get("completion_seq").is_none(),
        "no turn ended: {started}"
    );
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let thread_id = ThreadId::new(thread);
    assert_eq!(
        told(&mut window, |message| matches!(
            message,
            ServerMessage::Focus { .. }
        )),
        ServerMessage::Focus { thread: thread_id }
    );

    tell(
        &mut window,
        50,
        Op::Focused {
            thread: Some(thread_id),
        },
    );
    tell(
        &mut window,
        51,
        Op::Composing {
            thread: thread_id,
            unsent: true,
        },
    );
    let got = caller.answer("2", "thread.get", serde_json::json!({ "thread": thread }));
    assert_eq!(got["result"]["thread"]["focused"], true, "{got}");
    assert_eq!(got["result"]["thread"]["draft"], true, "{got}");
    let projects = caller.answer("3", "project.list", serde_json::json!({}));
    let project = &projects["result"]["projects"][0];
    assert_eq!(
        project["id"], got["result"]["thread"]["project"],
        "{projects}"
    );
    assert_eq!(project["focused"], true, "{projects}");
    assert_eq!(project["thread_count"], 1, "{projects}");

    let confirmed = caller.answer(
        "4",
        "agent.prompt",
        serde_json::json!({
            "thread": thread,
            "text": "go",
            "until": ["working", "blocked"],
            "timeout_ms": 8000,
        }),
    );
    assert_eq!(
        confirmed["result"]["thread"]["attention"], "working",
        "{confirmed}"
    );
    let counted = caller.answer("5", "thread.get", serde_json::json!({ "thread": thread }));
    assert_eq!(counted["result"]["thread"]["attention_seq"], 1, "{counted}");
    assert!(
        counted["result"]["thread"].get("completion_seq").is_none(),
        "{counted}"
    );

    // A turn that ends is counted, and named by the count it ended at.
    provider
        .send(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let ended = loop {
        let got = caller.answer("5", "thread.get", serde_json::json!({ "thread": thread }));
        if got["result"]["thread"]["attention_seq"] == 2 || Instant::now() >= deadline {
            break got;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(ended["result"]["thread"]["completion_seq"], 2, "{ended}");

    // A window that left takes what it said with it.
    drop(window);
    let deadline = Instant::now() + Duration::from_secs(5);
    let away = loop {
        let got = caller.answer("6", "thread.get", serde_json::json!({ "thread": thread }));
        if got["result"]["thread"]["focused"] == false || Instant::now() >= deadline {
            break got;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(away["result"]["thread"]["focused"], false, "{away}");
    assert_eq!(away["result"]["thread"]["draft"], false, "{away}");
}

/// Herdr's trust dialog: an agent blocked as it starts is not ready, and
/// the refusal names the Thread, which stays.
#[test]
fn a_thread_blocked_as_it_starts_is_not_ready() {
    let (harness, gate) = start_gated("api-not-ready");
    let mut caller = call(&harness);
    caller.ask(
        "1",
        "thread.start",
        serde_json::json!({ "cwd": harness.dir, "ready": true, "timeout_ms": 10000 }),
    );
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    caller.quiet();
    provider.send(decision()).unwrap();
    gate.send(()).unwrap();
    let refused = caller.next();
    assert_eq!(refused["error"]["code"], "agent_not_ready", "{refused}");
    let thread = refused["error"]["thread"].clone();
    let got = caller.answer("2", "thread.get", serde_json::json!({ "thread": thread }));
    assert_eq!(got["result"]["thread"]["attention"], "blocked", "{got}");
}

/// An agent not up within its time is not ready either.
#[test]
fn a_thread_not_up_in_time_is_not_ready() {
    let (harness, _gate) = start_gated("api-not-up");
    let mut caller = call(&harness);
    let refused = caller.answer(
        "1",
        "thread.start",
        serde_json::json!({ "cwd": harness.dir, "ready": true, "timeout_ms": 200 }),
    );
    assert_eq!(refused["error"]["code"], "agent_not_ready", "{refused}");
    assert!(refused["error"]["thread"].is_u64(), "{refused}");
}

/// A Thread is interrupted and closed by id, as Herdr's `send-keys esc`
/// and pane close; a closed one is not found.
#[test]
fn a_thread_is_interrupted_and_closed_by_id() {
    let harness = start("api-close");
    let mut window = attach(&harness.address);
    let (thread, _provider) = open(&mut window, &harness);
    let mut caller = call(&harness);
    caller.answer(
        "1",
        "agent.prompt",
        serde_json::json!({ "thread": thread, "text": "go" }),
    );
    let stopped = caller.answer(
        "2",
        "agent.interrupt",
        serde_json::json!({ "thread": thread }),
    );
    assert!(stopped.get("result").is_some(), "{stopped}");
    let closed = caller.answer("3", "thread.close", serde_json::json!({ "thread": thread }));
    assert_eq!(
        closed["result"]["thread"],
        serde_json::json!(thread),
        "{closed}"
    );
    let again = caller.answer("4", "thread.close", serde_json::json!({ "thread": thread }));
    assert_eq!(again["error"]["code"], "unknown_thread", "{again}");
    let gone = caller.answer(
        "5",
        "agent.interrupt",
        serde_json::json!({ "thread": thread }),
    );
    assert_eq!(gone["error"]["code"], "unknown_thread", "{gone}");
}

/// Herdr's `pane quit` then `agent start` in that pane: a closed Thread
/// is started again in place, keeping its id, once its agent is ready.
#[test]
fn a_closed_thread_is_revived_in_place() {
    let harness = start("api-revive");
    let mut caller = call(&harness);
    let started = caller.answer(
        "1",
        "thread.start",
        serde_json::json!({ "cwd": harness.dir, "args": ["--model", "opus"] }),
    );
    let thread = started["result"]["thread"]["thread"].clone();
    let open = caller.answer(
        "2",
        "thread.revive",
        serde_json::json!({ "thread": thread }),
    );
    assert_eq!(open["error"]["code"], "invalid_request", "{open}");
    caller.answer("3", "thread.close", serde_json::json!({ "thread": thread }));
    let revived = caller.answer(
        "4",
        "thread.revive",
        serde_json::json!({ "thread": thread, "resume": "4f2a", "ready": true }),
    );
    assert_eq!(revived["result"]["thread"]["thread"], thread, "{revived}");
    assert_eq!(
        revived["result"]["thread"]["attention"], "idle",
        "{revived}"
    );
    let got = caller.answer("5", "thread.get", serde_json::json!({ "thread": thread }));
    assert!(got.get("result").is_some(), "{got}");
    let unknown = caller.answer("6", "thread.revive", serde_json::json!({ "thread": 999 }));
    assert_eq!(unknown["error"]["code"], "unknown_thread", "{unknown}");
}

/// The board scopes restart flags to its remembered Provider. A stale
/// request preserves the actual Provider's launch and conversation.
#[test]
fn api_revival_scopes_launch_overrides_to_their_provider() {
    let harness = start("api-revive-provider");
    let mut caller = call(&harness);
    let started = caller.answer(
        "1",
        "thread.start",
        serde_json::json!({
            "cwd": harness.dir, "provider": "codex", "ready": true,
            "args": ["-c", "model_reasoning_effort=high"],
        }),
    );
    let thread = started["result"]["thread"]["thread"].clone();
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    provider
        .send(SessionEvent::Init {
            session_id: "codex-current".into(),
            model: "codex-model".into(),
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let got = caller.answer("2", "thread.get", serde_json::json!({ "thread": thread }));
        if got["result"]["thread"]["provider_session"] == "codex-current" {
            break;
        }
        assert!(Instant::now() < deadline, "Init was not observed: {got}");
        std::thread::sleep(Duration::from_millis(10));
    }
    caller.answer("3", "thread.close", serde_json::json!({ "thread": thread }));
    let revived = caller.answer(
        "4",
        "thread.revive",
        serde_json::json!({
            "thread": thread, "launch_provider": "claude", "ready": true,
            "args": ["--session-id", "claude-old", "--dangerously-skip-permissions"],
            "resume": "claude-old",
        }),
    );
    assert_eq!(
        revived["result"]["thread"]["provider"], "codex",
        "{revived}"
    );
    assert_eq!(
        revived["result"]["thread"]["provider_session"], "codex-current",
        "{revived}"
    );
    let saved = harness
        .dir
        .join("threads")
        .join(thread.as_u64().unwrap().to_string())
        .join("args.json");
    let args: serde_json::Value = serde_json::from_slice(&std::fs::read(saved).unwrap()).unwrap();
    assert_eq!(args["provider"], "codex");
    assert_eq!(
        args["args"],
        serde_json::json!(["-c", "model_reasoning_effort=high"])
    );
}

/// Herdr's `state_change_seq` never repeats within a run: a Thread's turn
/// after its revive counts past every turn before its park, so a turn
/// handled once is never taken for the next one. The boot names the run.
#[test]
fn a_revived_thread_counts_on_from_its_boot() {
    let harness = start("api-revive-count");
    let mut caller = call(&harness);
    let started = caller.answer(
        "1",
        "thread.start",
        serde_json::json!({ "cwd": harness.dir, "ready": true }),
    );
    let thread = started["result"]["thread"]["thread"].clone();
    let boot = started["result"]["thread"]["boot"].clone();
    assert!(
        boot.as_str().is_some_and(|boot| !boot.is_empty()),
        "{started}"
    );
    let turn = |caller: &mut Caller, id: &str| {
        let provider = harness
            .sessions
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        caller.answer(
            id,
            "agent.prompt",
            serde_json::json!({ "thread": thread, "text": "go", "until": ["working"], "timeout_ms": 8000 }),
        );
        provider
            .send(SessionEvent::TurnEnded {
                outcome: crate::TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let got = caller.answer(id, "thread.get", serde_json::json!({ "thread": thread }));
            if got["result"]["thread"]["completion_seq"].is_u64() || Instant::now() >= deadline {
                break (got, provider);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    let (first, _provider) = turn(&mut caller, "2");
    let handled = first["result"]["thread"]["completion_seq"]
        .as_u64()
        .unwrap();

    caller.answer("3", "thread.close", serde_json::json!({ "thread": thread }));
    let revived = caller.answer(
        "4",
        "thread.revive",
        serde_json::json!({ "thread": thread, "ready": true }),
    );
    assert!(
        revived["result"]["thread"]["attention_seq"]
            .as_u64()
            .unwrap()
            > handled,
        "its opening is counted past the handled turn: {revived}"
    );
    assert!(
        revived["result"]["thread"].get("completion_seq").is_none(),
        "{revived}"
    );
    let (second, _provider) = turn(&mut caller, "5");
    let info = &second["result"]["thread"];
    assert!(
        info["completion_seq"].as_u64().unwrap() > handled,
        "{second}"
    );
    assert_eq!(info["boot"], boot, "{second}");
}

/// Herdr's `workspace focus`: every window lands on the Project's first
/// open Thread.
#[test]
fn a_project_is_focused_by_id() {
    let harness = start("api-project-focus");
    let mut window = attach(&harness.address);
    let mut caller = call(&harness);
    // Side by side: a folder inside a Project's root is that Project's.
    let busy = harness.dir.join("busy");
    std::fs::create_dir_all(&busy).unwrap();
    let started = caller.answer("1", "thread.start", serde_json::json!({ "cwd": busy }));
    let info = &started["result"]["thread"];
    let thread = ThreadId::new(info["thread"].as_u64().unwrap());
    let focused = caller.answer(
        "2",
        "project.focus",
        serde_json::json!({ "project": info["project"] }),
    );
    assert_eq!(
        focused["result"]["project"]["id"], info["project"],
        "{focused}"
    );
    assert_eq!(focused["result"]["thread"], info["thread"], "{focused}");
    assert_eq!(
        told(&mut window, |message| matches!(
            message,
            ServerMessage::Focus { .. }
        )),
        ServerMessage::Focus { thread }
    );
    let missing = caller.answer(
        "3",
        "project.focus",
        serde_json::json!({ "project": "999" }),
    );
    assert_eq!(missing["error"]["code"], "project_not_found", "{missing}");

    // A Project with no open Thread has nothing to land on: nothing moves.
    let empty = harness.dir.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let opened = caller.answer("4", "project.open", serde_json::json!({ "cwd": empty }));
    let unfocused = caller.answer(
        "5",
        "project.focus",
        serde_json::json!({ "project": opened["result"]["project"]["id"] }),
    );
    assert!(unfocused["result"]["project"].is_object(), "{unfocused}");
    assert!(unfocused["result"].get("thread").is_none(), "{unfocused}");
}

/// Herdr's Spaces are Ferrite's Projects: opened by directory (once),
/// labelled, renamed, started in, and closed, which parks their Threads.
#[test]
fn projects_are_opened_renamed_and_closed() {
    let harness = start("api-projects");
    let mut caller = call(&harness);
    let home = harness.dir.join("demo");
    std::fs::create_dir_all(&home).unwrap();

    let opened = caller.answer(
        "1",
        "project.open",
        serde_json::json!({ "cwd": home, "label": "demo" }),
    );
    let project = opened["result"]["project"].clone();
    assert_eq!(project["label"], "demo", "{opened}");
    assert_eq!(project["thread_count"], 0, "{opened}");
    assert!(project["worktree"].is_null(), "{opened}");
    let id = project["id"].as_str().unwrap().to_string();
    let again = caller.answer(
        "2",
        "project.open",
        serde_json::json!({ "cwd": home, "label": "other" }),
    );
    assert_eq!(again["result"]["project"]["id"], id.as_str(), "{again}");
    assert_eq!(again["result"]["project"]["label"], "demo", "{again}");

    let renamed = caller.answer(
        "3",
        "project.rename",
        serde_json::json!({ "project": id, "label": "billing" }),
    );
    assert_eq!(
        renamed["result"]["project"]["label"], "billing",
        "{renamed}"
    );

    let started = caller.answer(
        "4",
        "thread.start",
        serde_json::json!({ "cwd": home, "project": id }),
    );
    let thread = started["result"]["thread"]["thread"].clone();
    assert_eq!(
        started["result"]["thread"]["project"],
        id.as_str(),
        "{started}"
    );
    // As Herdr's tab starts in its cwd, a Thread started in a directory
    // inside the Project works there, in that Project.
    let inside = home.join("crates").join("api");
    std::fs::create_dir_all(&inside).unwrap();
    let nested = caller.answer(
        "4b",
        "thread.start",
        serde_json::json!({ "cwd": inside, "project": id }),
    );
    assert_eq!(
        nested["result"]["thread"]["project"],
        id.as_str(),
        "{nested}"
    );
    assert_eq!(
        nested["result"]["thread"]["cwd"],
        std::fs::canonicalize(&inside)
            .unwrap()
            .display()
            .to_string(),
        "{nested}"
    );
    let elsewhere = caller.answer(
        "5",
        "thread.start",
        serde_json::json!({ "cwd": harness.dir, "project": id }),
    );
    assert_eq!(elsewhere["error"]["code"], "invalid_request", "{elsewhere}");
    let got = caller.answer("6", "project.get", serde_json::json!({ "project": id }));
    assert_eq!(got["result"]["project"]["thread_count"], 2, "{got}");

    let closed = caller.answer("10", "project.close", serde_json::json!({ "project": id }));
    assert_eq!(
        closed["result"]["parked"],
        serde_json::json!([thread, nested["result"]["thread"]["thread"]]),
        "{closed}"
    );
    assert_eq!(closed["result"]["project"]["thread_count"], 0, "{closed}");
    let parked = caller.answer("11", "thread.get", serde_json::json!({ "thread": thread }));
    assert_eq!(parked["error"]["code"], "unknown_thread", "{parked}");
    let missing = caller.answer("12", "project.get", serde_json::json!({ "project": "999" }));
    assert_eq!(missing["error"]["code"], "project_not_found", "{missing}");
}

/// A repository with one commit on `main`, under `dir`.
fn repo_in(dir: &Path) -> PathBuf {
    use crate::workspace::git_for_tests as git;
    let repo = dir.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("file.txt"), "base\n").unwrap();
    git(&repo, &["add", "file.txt"]);
    git(
        &repo,
        &[
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "user.name=test",
            "commit",
            "-qm",
            "base",
        ],
    );
    repo
}

/// Herdr's worktree rules: a Thread starts in a new worktree on the branch
/// it names, or opens one that stands; its worktree is removed only when
/// clean, never forced, and its branch stays.
#[test]
fn a_worktree_is_removed_only_when_clean_and_its_branch_stays() {
    use crate::workspace::git_for_tests as git;
    let harness = start("api-worktree");
    let repo = repo_in(&harness.dir);
    let mut caller = call(&harness);
    let started = caller.answer(
        "1",
        "thread.start",
        serde_json::json!({
            "cwd": repo,
            "worktree": true,
            "branch": "hp/demo/t-0001",
            "base": "main",
        }),
    );
    let info = &started["result"]["thread"];
    let thread = info["thread"].clone();
    let tree = PathBuf::from(info["worktree"].as_str().expect("a worktree"));
    assert_eq!(info["cwd"], info["worktree"], "{started}");
    assert_eq!(info["branch"], "hp/demo/t-0001", "{started}");
    assert!(tree.join("file.txt").exists(), "{started}");
    let projects = caller.answer("2", "project.list", serde_json::json!({}));
    assert_eq!(
        projects["result"]["projects"][0]["worktree"]["is_linked_worktree"], false,
        "{projects}"
    );

    std::fs::write(tree.join("notes.txt"), "mine\n").unwrap();
    let dirty = caller.answer("3", "worktree.remove", serde_json::json!({ "path": tree }));
    assert_eq!(
        dirty["error"]["code"], "dirty_worktree_requires_force",
        "{dirty}"
    );
    assert!(tree.exists());
    let still = caller.answer("4", "thread.get", serde_json::json!({ "thread": thread }));
    assert!(
        still.get("result").is_some(),
        "a refusal leaves the Thread open: {still}"
    );

    std::fs::remove_file(tree.join("notes.txt")).unwrap();
    let removed = caller.answer("5", "worktree.remove", serde_json::json!({ "path": tree }));
    assert_eq!(
        removed["result"]["parked"],
        serde_json::json!([thread]),
        "{removed}"
    );
    assert!(!tree.exists());
    assert!(!git(&repo, &["branch", "--list", "hp/demo/t-0001"])
        .trim()
        .is_empty());
    let parked = caller.answer("6", "thread.get", serde_json::json!({ "thread": thread }));
    assert_eq!(parked["error"]["code"], "unknown_thread", "{parked}");
    let again = caller.answer("6", "worktree.remove", serde_json::json!({ "path": tree }));
    assert_eq!(again["error"]["code"], "worktree_not_found", "{again}");
    let main = caller.answer("6", "worktree.remove", serde_json::json!({ "path": repo }));
    assert_eq!(main["error"]["code"], "worktree_not_found", "{main}");

    let standing = harness.dir.join("standing");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "standing",
            standing.to_str().unwrap(),
        ],
    );
    let opened = caller.answer(
        "7",
        "thread.start",
        serde_json::json!({ "cwd": standing, "worktree": true }),
    );
    assert_eq!(
        opened["result"]["thread"]["worktree"],
        serde_json::json!(standing),
        "{opened}"
    );

    // Herdr's `worktree create`, then the agent started in it: the Thread
    // opens the worktree that stands.
    let created = caller.answer(
        "8",
        "worktree.create",
        serde_json::json!({ "cwd": repo, "branch": "hp/demo/t-0002", "base": "main" }),
    );
    let made = PathBuf::from(created["result"]["worktree"]["path"].as_str().unwrap());
    assert_eq!(
        created["result"]["worktree"]["branch"], "hp/demo/t-0002",
        "{created}"
    );
    assert!(created["result"]["project"].is_string(), "{created}");
    assert!(made.join("file.txt").exists(), "{created}");
    let taken = caller.answer(
        "9",
        "worktree.create",
        serde_json::json!({ "cwd": repo, "branch": "hp/demo/t-0002" }),
    );
    assert_eq!(
        taken["error"]["code"], "failed",
        "an existing branch is never taken over: {taken}"
    );
    let started = caller.answer("10", "thread.start", serde_json::json!({ "cwd": made }));
    assert_eq!(
        started["result"]["thread"]["worktree"],
        serde_json::json!(made),
        "{started}"
    );
    assert_eq!(
        started["result"]["thread"]["branch"], "hp/demo/t-0002",
        "{started}"
    );
    // Nothing bound to it is lost: the Thread is parked with it.
    let thread = started["result"]["thread"]["thread"].clone();
    let removed = caller.answer("11", "worktree.remove", serde_json::json!({ "path": made }));
    assert_eq!(
        removed["result"]["parked"],
        serde_json::json!([thread]),
        "{removed}"
    );
}

/// The saved machines a coordinator can ask, by name, as Herdr's
/// `machine list` gives them; one turned off is never asked.
#[test]
fn the_saved_machines_are_listed_by_name() {
    let harness = start("api-machines");
    std::fs::write(
        harness.dir.join("settings.json"),
        r#"{"machines":[{"name":"box","target":"me@box"},{"name":"off","target":"me@off","enabled":false}]}"#,
    )
    .unwrap();
    let listed = call(&harness).answer("1", "machine.list", serde_json::json!({}));
    assert_eq!(
        listed["result"]["machines"],
        serde_json::json!([{ "id": "box", "label": "box", "target": "me@box" }]),
        "{listed}"
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

/// One CLI call to a machine through `dial`, as [`cli`].
fn cli_machine(
    dial: &crate::machine::Dial,
    method: &str,
    params: serde_json::Value,
) -> (i32, Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let ((mut out, said), (mut err, refused)) = (printed(), printed());
    let status =
        crate::api::cli::call_machine(&request(method, params), "box", dial, &mut out, &mut err);
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

/// `--machine`: the same call through a machine's Tunnel (here a socket
/// to this Server's API, where `ssh … ferrited api-bridge` would be),
/// and an ssh that ends without an answer, or before it could be asked,
/// refused as `server_unavailable` in the words of its Failure.
#[test]
fn the_cli_asks_a_machine_through_its_tunnel() {
    let harness = start("api-cli-machine");
    let mut window = attach(&harness.address);
    let (thread, _provider) = open(&mut window, &harness);
    let call = |dial: &crate::machine::Dial| {
        cli_machine(dial, "thread.get", serde_json::json!({ "thread": thread }))
    };

    let api = harness.api.clone();
    let tunnel: crate::machine::Dial = Box::new(move || {
        let stream = link::connect(&api)?;
        Ok(crate::machine::Tunnel {
            from: Box::new(link::TryClone::try_clone(&stream)?),
            to: Box::new(stream),
            ended: Box::new(|| None),
        })
    });
    let (status, said, refused) = call(&tunnel);
    assert_eq!((status, refused.len()), (0, 0), "{refused:?}");
    assert_eq!(
        said[0]["result"]["thread"]["thread"],
        serde_json::json!(thread)
    );

    /// An ssh already gone: its stdin is a broken pipe.
    struct Gone;
    impl io::Write for Gone {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let usage = "usage: ferrited server | ferrited bridge".to_string();
    for (failure, says) in [
        (crate::machine::Failure::SignIn, "sign in"),
        (crate::machine::Failure::NotInstalled, "`ferrited`"),
        (crate::machine::Failure::Unreachable(usage), "older"),
    ] {
        for gone in [false, true] {
            let failure = failure.clone();
            let ended: crate::machine::Dial = Box::new(move || {
                let failure = failure.clone();
                Ok(crate::machine::Tunnel {
                    from: Box::new(io::empty()),
                    to: if gone {
                        Box::new(Gone)
                    } else {
                        Box::new(io::sink())
                    },
                    ended: Box::new(move || Some(failure)),
                })
            });
            let (status, said, refused) = call(&ended);
            assert_eq!((status, said.len()), (1, 0), "{said:?}");
            assert_eq!(refused[0]["error"]["code"], "server_unavailable");
            let message = refused[0]["error"]["message"].as_str().unwrap();
            assert!(message.contains(says), "{message}");
        }
    }
}

/// `ferrited api-bridge` never starts a Server: with none it says so and
/// fails, which the CLI's ssh hands back as `server_unavailable`.
#[test]
fn the_api_bridge_with_no_server_fails_without_starting_one() {
    let store = std::env::temp_dir().join(format!("ferrite-api-bridge-{}", std::process::id()));
    std::fs::create_dir_all(&store).unwrap();
    assert_eq!(super::bridge::api(&store), 1);
    assert!(
        std::fs::read_dir(&store).unwrap().next().is_none(),
        "no Server was started in the Store"
    );
    let _ = std::fs::remove_dir_all(&store);
}

/// A repository with one commit, under `dir`: a Project Threads can make
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

/// herdr-projects' hook hears the Threads as herdr's panes (ADR 0031): an
/// agent detected as one opens, its status as its Attention changes, the
/// pane closed as it parks, each by this Server's API socket.
#[cfg(unix)]
#[test]
fn projects_hook_hears_threads_as_herdrs_panes() {
    let bin = scratch("projects-hooks-bin");
    let (program, file) = super::projects::tests::recorder(&bin, 0);
    let harness = serve("projects-hooks", None, Some(program));
    let mut window = attach(&harness.address);
    let (thread, _provider) = open(&mut window, &harness);
    call(&harness).answer(
        "1",
        "agent.prompt",
        serde_json::json!({ "thread": thread, "text": "go" }),
    );
    window.command(2, Op::Park { thread });

    let socket = harness.api.display().to_string();
    let pane = format!(r#""pane_id":"{thread}""#);
    let heard = |event: &str, says: &str| {
        let said = |call: &String| {
            call.starts_with(&format!("hook {event} "))
                && call.contains(&pane)
                && call.contains(says)
                && call.ends_with(&format!(" {socket} none"))
        };
        let calls = super::projects::tests::calls(&file, |calls| calls.iter().any(said));
        assert!(
            calls.iter().any(said),
            "no {event} saying {says}: {calls:#?}"
        );
    };
    heard("pane.agent_detected", r#""agent":"claude""#);
    heard("pane.agent_status_changed", r#""agent_status":"working""#);
    heard("pane.closed", r#"{"event":"pane.closed","#);
}

/// herdr-projects' hook hears a worktree `worktree.remove` removed as
/// herdr's `worktree.removed`, by its path.
#[cfg(unix)]
#[test]
fn projects_hook_hears_a_removed_worktree() {
    let bin = scratch("projects-worktree-bin");
    let (program, file) = super::projects::tests::recorder(&bin, 0);
    let harness = serve("projects-worktree", None, Some(program));
    let repo = repo_in(&harness.dir);
    let mut caller = call(&harness);
    let started = caller.answer(
        "1",
        "thread.start",
        serde_json::json!({ "cwd": repo, "worktree": true, "branch": "t-1", "base": "main" }),
    );
    let tree = started["result"]["thread"]["worktree"].clone();
    let removed = caller.answer("2", "worktree.remove", serde_json::json!({ "path": tree }));
    assert!(removed.get("result").is_some(), "{removed}");

    let said = format!(
        r#"{{"event":"worktree.removed","data":{{"workspace_id":{tree},"worktree":{{"path":{tree}}}}}}}"#
    );
    let heard = |call: &String| call.starts_with("hook worktree.removed ") && call.contains(&said);
    let calls = super::projects::tests::calls(&file, |calls| calls.iter().any(heard));
    assert!(
        calls.iter().any(heard),
        "no worktree.removed for {tree}: {calls:#?}"
    );
}

#[test]
fn manual_install_with_the_same_version_is_detected() {
    use std::time::{Duration, SystemTime};
    let dir = std::env::temp_dir().join(format!("ferrite-build-age-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let executable = dir.join("ferrite");
    let file = std::fs::File::create(&executable).unwrap();
    let boot = SystemTime::now() - Duration::from_secs(60);
    let server = ServerInfo {
        version: "0.4.0".into(),
        executable: executable.clone(),
    };
    file.set_modified(boot - Duration::from_secs(1)).unwrap();
    assert!(!executable_replaced_since(&executable, &server, boot));
    file.set_modified(boot + Duration::from_secs(1)).unwrap();
    assert!(executable_replaced_since(&executable, &server, boot));
    assert!(!executable_replaced_since(
        &dir.join("another"),
        &server,
        boot
    ));
    std::fs::remove_dir_all(dir).unwrap();
}

/// A window reads every open Thread it does not watch from its digest
/// (ADR 0032): what the agent does, the Decision it waits on, its Wall
/// card. Nothing of its history or stream crosses; a window attaching
/// later reads the same digest from its snapshot; a closed Thread's goes.
#[test]
fn a_window_that_watches_nothing_reads_each_thread_from_its_digest() {
    let harness = start("digests");
    let mut window = attach(&harness.address);
    let (thread, provider) = open(&mut window, &harness);
    provider.send(text("Reading the spike notes\n\n")).unwrap();
    provider.send(decision()).unwrap();

    window.until(|window| {
        window
            .digest(thread)
            .is_some_and(|digest| !digest.pending.is_empty())
    });

    let digest = window.digest(thread).unwrap();
    assert_eq!(
        digest.wall.lines.first().map(|line| line.text.as_str()),
        Some("\u{25cf} Reading the spike notes")
    );
    assert_eq!(window.histories, 0, "no history crossed");
    assert!(window.said(thread).is_empty(), "no input crossed");
    let late = attach(&harness.address);
    assert_eq!(late.digest(thread), Some(digest));

    window.command(2, Op::Park { thread });
    window.until(|window| window.digest(thread).is_none());
}

/// A Thread's digest goes out as it changes: a stream at most four times
/// a second, as the Wall card always refolded, and a Decision at once.
#[test]
fn a_stream_digests_four_times_a_second_and_a_decision_at_once() {
    let (spawner, providers) = spawn();
    let (mut engine, thread, agent, clock) = at_rest("digest-pace", spawner, &providers);
    let holds = |engine: &Engine, said: &str| {
        engine
            .thread(thread)
            .unwrap()
            .transcript()
            .blocks()
            .iter()
            .any(|block| format!("{:?}", block.body).contains(said))
    };
    let digests = |engine: &mut Engine| -> Vec<crate::digest::Digest> {
        engine
            .take_messages()
            .into_iter()
            .filter_map(|message| match message {
                ServerMessage::Digest { thread: of, digest } if of == thread => Some(digest),
                _ => None,
            })
            .collect()
    };
    let first_line =
        |digest: &crate::digest::Digest| digest.wall.lines.first().map(|line| line.text.clone());
    let streamed = |engine: &mut Engine, said: &str| {
        agent.send(text(said)).unwrap();
        pump_engine_until(engine, "the text never landed", |engine| {
            holds(engine, said)
        });
    };
    streamed(&mut engine, "Reading");
    clock.advance(Duration::from_millis(250));
    engine.pump();
    assert_eq!(
        digests(&mut engine).last().and_then(first_line).as_deref(),
        Some("\u{25cf} Reading")
    );

    clock.advance(Duration::from_millis(250));
    streamed(&mut engine, " the notes");
    assert_eq!(
        digests(&mut engine).last().and_then(first_line).as_deref(),
        Some("\u{25cf} Reading the notes"),
        "a quarter second since the last: at once"
    );
    streamed(&mut engine, " again");
    clock.advance(Duration::from_millis(249));
    engine.pump();
    assert_eq!(digests(&mut engine), Vec::new(), "not yet a quarter second");
    clock.advance(Duration::from_millis(1));
    engine.pump();
    assert_eq!(
        digests(&mut engine).last().and_then(first_line).as_deref(),
        Some("\u{25cf} Reading the notes again")
    );

    agent.send(decision()).unwrap();
    pump_engine_until(&mut engine, "the Decision never landed", |engine| {
        engine.thread(thread).unwrap().pending().is_some()
    });
    assert_eq!(
        digests(&mut engine)
            .last()
            .map(|digest| digest.pending.len()),
        Some(1),
        "at once"
    );
}

/// A window mirrors the settled Threads it draws at Transcript level (ADR
/// 0032). One off screen reads from its digest and holds none of its
/// transcript; drawn, its history arrives; five minutes off screen, the
/// window lets it go again.
#[test]
fn a_window_mirrors_only_the_threads_it_draws() {
    let clock = crate::clock::ThreadFixture::install(chrono::Local::now());
    let harness = start("mirrors-drawn");
    let mut other = attach(&harness.address);
    let (thread, provider) = open(&mut other, &harness);
    let mut window = cockpit(&harness);
    window.send(thread, "go".into());
    provider.send(text("on it\n\n")).unwrap();
    turn_ends(&provider);

    pump_until(&mut window, |window| digest_says(window, thread, "on it"));
    pump_until(&mut window, |window| {
        window
            .thread(thread)
            .is_some_and(|open| !open.digest().at_work)
    });
    // Held while it worked, and as long after as the Server holds it.
    clock.advance(HELD_AT_REST);
    window.pump();
    assert!(!shows(&window, thread, "on it"), "no transcript off screen");

    window.draws_transcripts([thread]);
    pump_until(&mut window, |window| shows(window, thread, "on it"));
    clock.advance(Duration::from_secs(31 * 60));
    window.pump();
    assert!(
        shows(&window, thread, "on it"),
        "still drawn, however long nothing redraws"
    );

    window.draws_transcripts([]);
    clock.advance(Duration::from_secs(5 * 60 - 1));
    window.pump();
    assert!(shows(&window, thread, "on it"), "kept a while off screen");
    clock.advance(Duration::from_secs(1));
    window.pump();
    assert!(!shows(&window, thread, "on it"), "let go");
}

/// A Thread at work stays streamed whole however long it is off screen:
/// the operator comes back to it as it is, never to a reload. Five minutes
/// after it settles the window lets it go.
#[test]
fn a_thread_at_work_stays_mirrored_off_screen() {
    let clock = crate::clock::ThreadFixture::install(chrono::Local::now());
    let harness = start("mirrors-working");
    let mut other = attach(&harness.address);
    let (thread, provider) = open(&mut other, &harness);
    let mut window = cockpit(&harness);
    window.draws_transcripts([thread]);
    window.send(thread, "go".into());
    provider.send(text("on it\n\n")).unwrap();
    pump_until(&mut window, |window| shows(window, thread, "on it"));
    pump_until(&mut window, |window| {
        window
            .thread(thread)
            .is_some_and(|open| open.digest().working)
    });

    window.draws_transcripts([]);
    clock.advance(Duration::from_secs(20 * 60));
    window.pump();
    assert!(shows(&window, thread, "on it"), "kept while it works");

    turn_ends(&provider);
    pump_until(&mut window, |window| {
        window
            .thread(thread)
            .is_some_and(|open| !open.digest().at_work)
    });
    clock.advance(Duration::from_secs(5 * 60 - 1));
    window.pump();
    assert!(
        shows(&window, thread, "on it"),
        "kept a while after it settles"
    );
    clock.advance(Duration::from_secs(1));
    window.pump();
    assert!(!shows(&window, thread, "on it"), "let go");
}

/// A Thread watched mid-turn reads as it is: working, as its digest said
/// before its history arrived, never idle until its next event.
#[test]
fn a_thread_watched_mid_turn_reads_working() {
    let harness = start("watched-mid-turn");
    let mut other = attach(&harness.address);
    let (thread, provider) = open(&mut other, &harness);
    provider
        .send(SessionEvent::RunState {
            state: crate::RunState::Running,
        })
        .unwrap();
    provider.send(text("thinking\n\n")).unwrap();
    other.until(|other| {
        other.digest(thread).is_some_and(|digest| {
            digest
                .wall
                .lines
                .iter()
                .any(|line| line.text.contains("thinking"))
        })
    });

    // Attached mid-turn: what it reads of the turn comes in the history.
    let mut window = cockpit(&harness);
    pump_until(&mut window, |window| shows(window, thread, "thinking"));
    let status = window.thread(thread).map(|open| open.turn().status);
    assert_eq!(status, Some(crate::transcript::Status::Streaming));
}

/// A Thread at work is streamed whole whether or not this window has
/// drawn it: the operator opens it to find it as it is, never loading.
/// Five minutes after it settles the window lets it go.
#[test]
fn a_thread_at_work_is_mirrored_though_never_drawn() {
    let clock = crate::clock::ThreadFixture::install(chrono::Local::now());
    let harness = start("working-unseen");
    let mut window = cockpit(&harness);
    window.draws_transcripts([]);
    let mut other = attach(&harness.address);
    let (thread, provider) = open(&mut other, &harness);
    other.command(
        2,
        Op::Send {
            thread,
            text: "go".into(),
        },
    );
    provider.send(text("on it\n\n")).unwrap();
    pump_until(&mut window, |window| {
        window
            .thread(thread)
            .is_some_and(|open| open.digest().working)
    });

    clock.advance(Duration::from_secs(20 * 60));
    window.pump();
    pump_until(&mut window, |window| shows(window, thread, "on it"));

    turn_ends(&provider);
    pump_until(&mut window, |window| {
        window
            .thread(thread)
            .is_some_and(|open| !open.digest().at_work)
    });
    clock.advance(Duration::from_secs(5 * 60 - 1));
    window.pump();
    assert!(
        shows(&window, thread, "on it"),
        "kept a while after it settles"
    );
    clock.advance(Duration::from_secs(1));
    window.pump();
    assert!(!shows(&window, thread, "on it"), "let go");
}

/// A small Thread's history never waits on a big one's: watched together,
/// the small one is drawn first though the big one was asked for first.
#[test]
fn a_small_history_never_waits_on_a_big_one() {
    let harness = start("histories-together");
    let mut other = attach(&harness.address);
    let (big, big_provider) = open(&mut other, &harness);
    let (small, small_provider) = open(&mut other, &harness);
    let long = "x".repeat(400 * 1024);
    for _ in 0..16 {
        big_provider.send(text(&format!("{long}\n\n"))).unwrap();
        turn_ends(&big_provider);
    }
    small_provider.send(text("small\n\n")).unwrap();
    turn_ends(&small_provider);
    other.until_within(Duration::from_secs(30), |other| {
        [big, small].iter().all(|thread| {
            other
                .digest(*thread)
                .is_some_and(|digest| !digest.at_work && !digest.wall.lines.is_empty())
        })
    });

    let mut window = attach(&harness.address);
    window.watch(big, None);
    window.watch(small, None);
    let mut ended = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    while ended.len() < 2 {
        assert!(Instant::now() < deadline, "timed out: {ended:?}");
        if let Ok(ServerMessage::HistoryEnd { thread, .. }) =
            window.client.messages.recv_timeout(Duration::from_secs(1))
        {
            ended.push(thread);
        }
    }
    assert_eq!(ended, vec![small, big]);
}

/// A window drawing a Thread it let go shows its transcript whole or not
/// at all: a long history crossing the link in pieces never draws its
/// oldest part on the way to the latest.
#[test]
fn a_thread_drawn_again_arrives_whole() {
    let harness = start("arrives-whole");
    let mut other = attach(&harness.address);
    let (thread, provider) = open(&mut other, &harness);
    other.command(
        2,
        Op::Send {
            thread,
            text: "go".into(),
        },
    );
    let long = "x".repeat(400 * 1024);
    // Turns of their own, so the log holds them as inputs of their own.
    provider.send(text(&format!("oldest {long}\n\n"))).unwrap();
    for _ in 0..12 {
        turn_ends(&provider);
        provider.send(text(&format!("{long}\n\n"))).unwrap();
    }
    turn_ends(&provider);
    provider.send(text("latest\n\n")).unwrap();
    turn_ends(&provider);
    other.until_within(Duration::from_secs(30), |other| {
        other.digest(thread).is_some_and(|digest| {
            !digest.at_work
                && digest
                    .wall
                    .lines
                    .iter()
                    .any(|line| line.text.contains("latest"))
        })
    });
    // A window that comes to it settled holds none of it until it draws it.
    let mut window = cockpit(&harness);
    pump_until(&mut window, |window| digest_says(window, thread, "latest"));
    assert!(!shows(&window, thread, "latest"));

    // Drawn every frame as fast as the window can, as the app draws while
    // the pieces cross.
    window.draws_transcripts([thread]);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !shows(&window, thread, "latest") {
        assert!(Instant::now() < deadline, "timed out");
        window.pump();
        assert!(
            !shows(&window, thread, "oldest") || shows(&window, thread, "latest"),
            "drew part of its history"
        );
    }
}

/// A window that says what it draws before its first frame streams no
/// Thread it does not draw: attaching to a Server with Threads open reads
/// their digests, and none of their histories (ADR 0032).
#[test]
fn a_window_attaching_streams_no_thread_it_does_not_draw() {
    let harness = start("attach-draws-nothing");
    let mut other = attach(&harness.address);
    let (thread, provider) = open(&mut other, &harness);
    provider.send(text("before the window\n\n")).unwrap();
    turn_ends(&provider);
    other.until(|other| {
        other
            .digest(thread)
            .is_some_and(|digest| !digest.wall.lines.is_empty())
    });

    let mut window = cockpit(&harness);
    pump_until(&mut window, |window| {
        digest_says(window, thread, "before the window")
    });
    for _ in 0..20 {
        window.pump();
        std::thread::sleep(Duration::from_millis(5));
    }

    assert!(!shows(&window, thread, "before the window"));
}

/// A window showing a Subagent reads its card in the digest at once, though
/// nothing streams: the shown Subject is a moment of its own.
#[test]
fn showing_a_subagent_refolds_the_digest_at_once() {
    use crate::activity::{ActivityEvent, AgentInfo, AgentKey, ExecutionEvent, Subject};
    let (spawner, providers) = spawn();
    let (mut engine, thread, agent, _clock) = at_rest("digest-shown", spawner, &providers);
    let key = AgentKey::new(Provider::Claude, "claude-1", "child");
    let mut info = AgentInfo::new(key.clone());
    info.parent = Some(Subject::Main);
    agent
        .send(SessionEvent::Activity(ActivityEvent::Discovered(info)))
        .unwrap();
    agent
        .send(SessionEvent::Activity(ActivityEvent::Content {
            key: key.clone(),
            id: None,
            event: ExecutionEvent::TextDelta {
                text: "Reading the spike notes".into(),
            },
        }))
        .unwrap();
    pump_engine_until(&mut engine, "the Subagent never spoke", |engine| {
        engine
            .thread(thread)
            .unwrap()
            .activity()
            .children()
            .iter()
            .any(|child| !child.transcript().blocks().is_empty())
    });
    engine.take_messages();

    let subject = Subject::Subagent(key);
    engine.set_visible_subject(thread, subject.clone());
    engine.pump();

    let shown = engine
        .take_messages()
        .into_iter()
        .filter_map(|message| match message {
            ServerMessage::Digest { thread: of, digest } if of == thread => digest.shown,
            _ => None,
        })
        .last();
    assert_eq!(
        shown.map(|shown| shown.subject),
        Some(crate::protocol::WireSubject::from(&subject))
    );
}

/// A Thread whose agent has ended and that no window watches holds only its
/// digest (ADR 0032). Watched, read or sent to, it reads exactly as it did,
/// on the same generation, and the next prompt resumes its conversation.
#[test]
fn a_thread_nobody_uses_holds_only_its_digest_until_used() {
    let (spawner, started, providers) = recorded();
    let (mut engine, thread, agent, clock) = at_rest("held", spawner, &providers);
    agent.send(text("All done here.\n\n")).unwrap();
    agent
        .send(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    pump_engine_until(&mut engine, "the turn never ended", |engine| {
        engine.thread(thread).unwrap().transcript().turn_completed()
    });
    let said = |engine: &Engine| -> Vec<String> {
        engine
            .thread(thread)
            .unwrap()
            .transcript()
            .blocks()
            .iter()
            .map(|block| format!("{:?}", block.body))
            .collect()
    };
    let before = said(&engine);
    let generation = engine.thread(thread).unwrap().generation();
    engine.set_watched(BTreeSet::new());
    let let_go = |engine: &mut Engine| {
        clock.advance(HELD_AT_REST);
        engine.pump();
        engine.take_messages();
    };

    let_go(&mut engine);
    assert!(said(&engine).is_empty(), "nothing held but the digest");
    let digest = engine.snapshot().digests.unwrap().remove(0).digest;
    assert_eq!(
        digest.wall.lines.first().map(|line| line.text.as_str()),
        Some("\u{25cf} All done here.")
    );

    engine.set_watched([thread].into());
    assert_eq!(said(&engine), before, "watched, it reads as it did");
    assert_eq!(engine.thread(thread).unwrap().generation(), generation);

    engine.set_watched(BTreeSet::new());
    let_go(&mut engine);
    assert!(said(&engine).is_empty());
    started.lock().unwrap().clear();
    engine
        .execute(&Op::Send {
            thread,
            text: "and the docs".into(),
        })
        .unwrap();
    engine.pump();
    let mut expected = before.clone();
    expected.push(format!("{:?}", Body::Prompt("and the docs".into())));
    assert_eq!(said(&engine), expected);
    assert_eq!(engine.thread(thread).unwrap().generation(), generation);
    assert_eq!(
        *started.lock().unwrap(),
        vec![Start {
            resume: Some("claude-1".into()),
            ..Start::bare(Provider::Claude)
        }]
    );
}

/// A Thread whose agent exited and that no window watches is let go to its
/// digest; the API's read and a window's watch read it back whole, what
/// its last agent announced included.
#[test]
fn a_thread_let_go_reads_back_whole_for_the_api_and_a_watch() {
    let harness = start("let-go");
    let mut window = attach(&harness.address);
    let (thread, provider) = open(&mut window, &harness);
    for event in announcements() {
        provider.send(event).unwrap();
    }
    provider.send(text("Kept on disk\n\n")).unwrap();
    turn_ends(&provider);
    exit(provider);
    window.until(|window| {
        window
            .digest(thread)
            .is_some_and(|digest| digest.status == crate::protocol::WireStatus::Closed)
    });
    std::thread::sleep(Duration::from_millis(100));

    let mut caller = call(&harness);
    let read = caller.answer("1", "agent.read", serde_json::json!({ "thread": thread }));
    assert!(
        read["result"]["text"]
            .as_str()
            .unwrap()
            .contains("Kept on disk"),
        "{read}"
    );
    window.watch(thread, None);
    window.until(|window| {
        window.mirrors.seq(thread).is_some()
            && window
                .said(thread)
                .iter()
                .any(|said| said.contains("Kept on disk"))
    });
    let mirror = window.mirrors.view(thread).unwrap();
    assert_eq!(
        mirror.transcript().mcp_servers()[..],
        [crate::McpServer {
            name: "linear".into(),
            status: crate::McpStatus::NeedsAuth,
            error: None,
        }],
        "the watch carries what the last agent announced"
    );
}

/// A Thread let go to its digest, as
/// `a_thread_nobody_uses_holds_only_its_digest_until_used` leaves one:
/// the Engine, the Thread, what its transcript reads and the clock.
fn let_go(
    name: &str,
) -> (
    Engine,
    ThreadId,
    Vec<String>,
    crate::clock::ThreadFixture,
    Receiver<Sender<SessionEvent>>,
) {
    let (spawner, providers) = spawn();
    let (mut engine, thread, agent, clock) = at_rest(name, spawner, &providers);
    agent.send(text("All done here.\n\n")).unwrap();
    agent
        .send(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    pump_engine_until(&mut engine, "the turn never ended", |engine| {
        engine.thread(thread).unwrap().transcript().turn_completed()
    });
    let before = said_in(&engine, thread);
    engine.set_watched(BTreeSet::new());
    clock.advance(HELD_AT_REST);
    engine.pump();
    engine.take_messages();
    assert!(said_in(&engine, thread).is_empty(), "let go");
    (engine, thread, before, clock, providers)
}

fn said_in(engine: &Engine, thread: ThreadId) -> Vec<String> {
    engine
        .thread(thread)
        .unwrap()
        .transcript()
        .blocks()
        .iter()
        .map(|block| format!("{:?}", block.body))
        .collect()
}

/// What a window says of a Thread on every tick (its worktree listing, the
/// Subject it shows) never reads a let-go Thread's log back: only an act
/// that reads its transcript does (ADR 0032).
#[test]
fn a_windows_routine_acts_leave_a_let_go_thread_let_go() {
    let (mut engine, thread, _, _clock, _providers) = let_go("routine-acts");

    engine
        .execute(&Op::WorktreesListed {
            thread,
            listed: Vec::new(),
            age_ms: 0,
        })
        .unwrap();
    engine
        .execute(&Op::ShowSubject {
            thread,
            subject: protocol::WireSubject::Main,
        })
        .unwrap();
    engine.take_messages();

    assert!(said_in(&engine, thread).is_empty(), "still let go");
}

/// Read back, a Thread stays held a minute before it may be let go again,
/// so whatever reads it often does not read its log each time.
#[test]
fn a_thread_read_back_stays_held_a_minute() {
    let (mut engine, thread, before, clock, _providers) = let_go("held-a-minute");

    engine.set_watched([thread].into());
    engine.set_watched(BTreeSet::new());
    engine.pump();
    engine.take_messages();
    assert_eq!(said_in(&engine, thread), before, "held");
    clock.advance(Duration::from_secs(59));
    engine.pump();
    engine.take_messages();
    assert_eq!(said_in(&engine, thread), before, "still held");
    clock.advance(Duration::from_secs(1));
    engine.pump();
    engine.take_messages();
    assert!(said_in(&engine, thread).is_empty(), "let go again");
}

/// An act that writes to a let-go Thread reads it back first, so what it
/// writes lands on its history and stays: an effort change says so.
#[test]
fn an_act_that_writes_reads_a_let_go_thread_back_first() {
    let (mut engine, thread, before, _clock, _providers) = let_go("writes-read-back");

    engine
        .execute(&Op::SetEffort {
            thread,
            effort: Some("high".into()),
        })
        .unwrap();

    let said = said_in(&engine, thread);
    assert_eq!(said[..before.len()], before[..], "on its history");
    assert!(said.len() > before.len(), "{said:?}");
}

/// The old Server's endpoint, gone as `exec` would take it: a Unix
/// socket's file stays behind a thread's Server, a Windows pipe never does.
fn gone(address: &Path) {
    match std::fs::remove_file(address) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => panic!("{error}"),
        _ => {}
    }
}

/// A window that asks for a restart sees it through: restarting, then
/// restarted once it reaches the successor, with the version it runs.
#[test]
fn a_window_sees_its_restart_through() {
    use crate::cockpit::ServerRestart;
    let harness = start("restart-seen");
    let mut window = cockpit(&harness);
    pump_until(&mut window, |window| {
        matches!(
            window.server(),
            Some(crate::cockpit::ServerState::Attached { .. })
        )
    });

    window.restart_server(When::Now).unwrap();
    assert_eq!(window.server_restart(), Some(&ServerRestart::Restarting));
    harness
        .relaunched
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert_eq!(harness.server.join().unwrap(), 0);
    pump_until(&mut window, |window| {
        matches!(
            window.server(),
            Some(crate::cockpit::ServerState::Detached { .. })
        )
    });
    assert_eq!(window.server_restart(), Some(&ServerRestart::Restarting));
    // Pressed again while it is down: the restart under way is the one.
    window.restart_server(When::Now).unwrap();
    assert_eq!(window.server_restart(), Some(&ServerRestart::Restarting));

    // `exec` closes the old Server's sockets; this thread's stay open.
    gone(&harness.address);
    gone(&harness.api);
    let _successor = serve_in(harness.dir.clone(), None, None);
    pump_until_within(&mut window, Duration::from_secs(10), |window| {
        window.server_restart() != Some(&ServerRestart::Restarting)
    });
    assert_eq!(
        window.server_restart(),
        Some(&ServerRestart::Restarted {
            version: "0.9.0".into()
        })
    );
}

/// A restart the window does not see come back in time reads as failed,
/// with why, rather than as nothing at all; back at last, restarted.
#[test]
fn a_restart_not_back_in_time_reads_failed_until_it_is() {
    use crate::cockpit::ServerRestart;
    let clock = crate::clock::ThreadFixture::install(chrono::Local::now());
    let harness = start("restart-lost");
    let mut window = cockpit(&harness);
    pump_until(&mut window, |window| {
        matches!(
            window.server(),
            Some(crate::cockpit::ServerState::Attached { .. })
        )
    });

    window.restart_server(When::Now).unwrap();
    harness
        .relaunched
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    clock.advance(Duration::from_secs(29));
    window.pump();
    assert_eq!(window.server_restart(), Some(&ServerRestart::Restarting));
    clock.advance(Duration::from_secs(1));
    window.pump();
    assert!(
        matches!(window.server_restart(), Some(ServerRestart::Failed { .. })),
        "{:?}",
        window.server_restart()
    );

    // Back at last on a new boot: it did restart, and now reads so.
    assert_eq!(harness.server.join().unwrap(), 0);
    gone(&harness.address);
    gone(&harness.api);
    let _successor = serve_in(harness.dir.clone(), None, None);
    pump_until_within(&mut window, Duration::from_secs(10), |window| {
        matches!(
            window.server_restart(),
            Some(ServerRestart::Restarted { .. })
        )
    });
}
