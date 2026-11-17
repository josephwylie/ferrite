use super::*;

#[test]
fn a_conversation_progresses_before_incoming_traffic_is_drained() {
    let dir = scratch("intake-progress");
    let (spawner, providers) = spawn();
    let mut engine = engine(&dir.join("threads"), spawner);
    let busy = open_in(&mut engine, &dir);
    let _busy_provider = providers.recv().unwrap();
    let other = open_in(&mut engine, &dir);
    let other_provider = providers.recv().unwrap();
    engine.pump();
    other_provider
        .send(text("another conversation advanced"))
        .unwrap();

    let (_, renders) = render::remote();
    let server = Server::new(
        engine,
        ServerInfo {
            version: "0.9.0".into(),
            executable: "/bin/ferrite".into(),
        },
        renders,
        Arc::new(Mutex::new(SessionDefaults::default())),
        dir.clone(),
    );
    let (heard, hear) = mpsc::channel();
    let (answer, answers) = mpsc::channel();
    heard
        .send(Heard::Api(crate::api::Heard::Joined(1, answer)))
        .unwrap();
    for index in 0..513 {
        let thread = if index == 256 { other } else { busy };
        heard
            .send(Heard::Api(crate::api::Heard::Request(
                1,
                crate::api::Request {
                    id: index.to_string(),
                    method: crate::api::Method::AgentRead(crate::api::schema::AgentReadParams {
                        thread,
                        lines: None,
                    }),
                    caller: None,
                },
            )))
            .unwrap();
    }
    drop(heard);

    assert_eq!(
        server.serve(hear),
        1,
        "the listener closed after the queued traffic"
    );
    let replies: Vec<serde_json::Value> = answers
        .try_iter()
        .map(|line| serde_json::from_slice(&line).unwrap())
        .collect();
    assert_eq!(replies.len(), 513);
    assert_eq!(replies[256]["id"], "256");
    assert!(
        replies[256]["result"]["text"]
            .as_str()
            .unwrap()
            .contains("another conversation advanced"),
        "the other conversation must advance before the remaining traffic: {:?}",
        replies[256]
    );
    assert_eq!(replies.last().unwrap()["id"], "512");
    let _ = std::fs::remove_dir_all(dir);
}

/// The production spawner's shape: a start's preparation, then its scripted
/// Session, on a thread of its own, once the test lets each start through.
struct OffLoop {
    spawn: Spawn,
    gate: Arc<Mutex<mpsc::Receiver<()>>>,
}

fn off_loop() -> (
    Box<dyn Spawner>,
    mpsc::Receiver<Sender<SessionEvent>>,
    Sender<()>,
) {
    let (sessions, providers) = mpsc::channel();
    let (gate, opens) = mpsc::channel();
    let spawner = OffLoop {
        spawn: Spawn {
            launches: Default::default(),
            sessions,
            answers: Answers::default(),
            queued: Queued::default(),
            gate: None,
            can_queue: None,
            controls: Controls::default(),
            installed: Installed::default(),
        },
        gate: Arc::new(Mutex::new(opens)),
    };
    (Box::new(spawner), providers, gate)
}

impl Spawner for OffLoop {
    fn spawn(&mut self, request: SpawnRequest) -> io::Result<Box<dyn Session>> {
        self.spawn.spawn(request)
    }

    fn start(&mut self, request: SpawnRequest) -> io::Result<crate::session::SessionLifecycle> {
        self.start_after(Box::new(|| Ok(())), request)
    }

    fn start_after(
        &mut self,
        prepare: crate::engine::Preparation,
        request: SpawnRequest,
    ) -> io::Result<crate::session::SessionLifecycle> {
        self.spawn.launched(&request);
        let session = self.spawn.scripted();
        let gate = self.gate.clone();
        crate::session::SessionLifecycle::background(move || {
            let _ = gate.lock().unwrap().recv();
            prepare()?;
            Ok(Box::new(session) as Box<dyn Session + Send>)
        })
    }
}

fn first_send(workspace: crate::workspace::WorkspaceChoice) -> crate::protocol::Op {
    crate::protocol::Op::Bootstrap {
        provider: Provider::Claude,
        model: None,
        effort: None,
        workspace: (&workspace).into(),
        prompt: "begin".into(),
        group: None,
        new_group_with: None,
    }
}

/// A first send into a new worktree asks git nothing on the loop: while
/// its worktree is still being made, another Thread's agent is heard.
#[test]
fn a_new_worktree_is_made_off_the_loop_while_other_threads_progress() {
    let dir = scratch("worktree-off-loop");
    let repo = repo_in(&dir);
    let (spawner, providers, gate) = off_loop();
    let mut engine = engine(&dir.join("threads"), spawner);
    let other = open_in(&mut engine, &dir);
    let other_provider = providers.recv().unwrap();
    gate.send(()).unwrap();
    pump_engine_until(&mut engine, "the other Thread never started", |engine| {
        !engine.thread(other).unwrap().starting()
    });

    let asked = crate::workspace::git_spawns_on_this_thread();
    let started = engine.bootstrap_command(
        DraftId::new(1),
        &first_send(crate::workspace::WorkspaceChoice::NewWorktree { repo: repo.clone() }),
    );
    assert!(matches!(started, Ok(None)), "the first send is pending: {started:?}");
    other_provider
        .send(text("another conversation advanced"))
        .unwrap();
    engine.pump();

    assert_eq!(
        crate::workspace::git_spawns_on_this_thread() - asked,
        0,
        "the loop asked git while making the worktree"
    );
    assert!(
        !repo.join(crate::workspace::WORKTREES).exists(),
        "the worktree was made before its start ran"
    );
    assert!(
        crate::transcript::plain_text(engine.thread(other).unwrap().transcript().blocks())
            .contains("another conversation advanced"),
        "the other conversation waited on the worktree"
    );

    gate.send(()).unwrap();
    let settled = settle_first_send(&mut engine);
    let thread = settled.expect("the first send delivers").thread;
    let worktree = engine.thread(thread).unwrap().workspace().unwrap().cwd();
    assert!(worktree.join(".git").exists(), "the worktree stands once started");
    let _ = std::fs::remove_dir_all(dir);
}

/// A first send whose worktree cannot be made leaves nothing behind: no
/// Thread, no log, no registered worktree, and the operator hears git.
#[test]
fn a_worktree_that_cannot_be_made_rolls_the_first_send_back() {
    let dir = scratch("worktree-refused");
    let repo = repo_in(&dir);
    crate::workspace::git_for_tests(&repo, &["branch", "taken"]);
    let store = dir.join("threads");
    let (spawner, _providers, gate) = off_loop();
    let mut engine = engine(&store, spawner);

    let started = engine.bootstrap_command(
        DraftId::new(1),
        &first_send(crate::workspace::WorkspaceChoice::NamedWorktree {
            repo: repo.clone(),
            branch: "taken".into(),
            base: None,
        }),
    );
    assert!(matches!(started, Ok(None)), "the first send is pending: {started:?}");
    gate.send(()).unwrap();
    let error = settle_first_send(&mut engine).expect_err("git refuses a taken branch");

    assert!(
        error.to_string().contains("taken"),
        "the operator hears what git said: {error}"
    );
    assert!(engine.threads().is_empty(), "a half-made Thread stayed");
    assert!(
        Store::open(&store).unwrap().thread_ids().unwrap().is_empty(),
        "the Thread's log stayed"
    );
    let registry = engine.registry();
    assert!(
        registry
            .projects()
            .iter()
            .all(|project| registry.worktrees(project.id).is_empty()),
        "the worktree stayed registered"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// A directory squatting where the worktree would go is not the worktree:
/// the first send is refused and its reservation forgotten all the same.
#[test]
fn a_squatted_worktree_path_rolls_the_first_send_back() {
    let dir = scratch("worktree-squatted");
    let repo = repo_in(&dir);
    let squatted = repo.join(crate::workspace::WORKTREES).join("squat");
    std::fs::create_dir_all(squatted.join("kept")).unwrap();
    let (spawner, _providers, gate) = off_loop();
    let mut engine = engine(&dir.join("threads"), spawner);

    let started = engine.bootstrap_command(
        DraftId::new(1),
        &first_send(crate::workspace::WorkspaceChoice::NamedWorktree {
            repo: repo.clone(),
            branch: "squat".into(),
            base: None,
        }),
    );
    assert!(matches!(started, Ok(None)), "the first send is pending: {started:?}");
    gate.send(()).unwrap();
    let error = settle_first_send(&mut engine).expect_err("git refuses a path in use");

    assert!(
        error.to_string().contains("squat"),
        "the operator hears what git said: {error}"
    );
    assert!(engine.threads().is_empty(), "a half-made Thread stayed");
    assert!(squatted.join("kept").exists(), "the squatting directory was touched");
    let registry = engine.registry();
    assert!(
        registry
            .projects()
            .iter()
            .all(|project| registry.worktrees(project.id).is_empty()),
        "the worktree stayed registered"
    );
    let _ = std::fs::remove_dir_all(dir);
}

fn settle_first_send(engine: &mut Engine) -> io::Result<crate::engine::Bootstrapped> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "the first send never settled");
        engine.pump();
        if let Some(settled) = engine.take_bootstrap_results().pop() {
            return settled.result;
        }
    }
}

/// A Thread opened outright answers git's refusal itself, as `thread.start`
/// and a Factory Worker need, even from a spawner that starts off the loop.
#[test]
fn an_opened_thread_whose_worktree_is_refused_is_refused_in_the_reply() {
    let dir = scratch("worktree-refused-open");
    let repo = repo_in(&dir);
    crate::workspace::git_for_tests(&repo, &["branch", "taken"]);
    let (spawner, _providers, _gate) = off_loop();
    let mut engine = engine(&dir.join("threads"), spawner);

    let error = engine
        .open(
            Provider::Claude,
            crate::workspace::WorkspaceChoice::NamedWorktree {
                repo: repo.clone(),
                branch: "taken".into(),
                base: None,
            },
        )
        .expect_err("git refuses a taken branch");

    assert!(error.to_string().contains("taken"), "the reply says what git said: {error}");
    assert!(engine.threads().is_empty(), "a half-made Thread stayed");
    let registry = engine.registry();
    assert!(
        registry
            .projects()
            .iter()
            .all(|project| registry.worktrees(project.id).is_empty()),
        "the refused worktree stayed registered"
    );
    let _ = std::fs::remove_dir_all(dir);
}
