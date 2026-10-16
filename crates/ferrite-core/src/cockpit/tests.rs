//! The Cockpit over its Engine: Sessions, the pump, the roster and the
//! acts a window takes, driven through scripted Sessions.

#[path = "tests/context.rs"]
mod context;
use super::*;
use crate::cockpit::Cockpit;
use crate::roster::{PaneIdentity, View};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{self, Sender};

use crate::store::{Provider, Store};
use crate::transcript::{Body, Class};
use crate::TurnOutcome;

/// A fresh per-test scratch directory.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ferrite-cockpit-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A Session with no process behind it: the test pushes the events a
/// provider would have streamed, and reads back what Ferrite sent.
struct Scripted {
    tx: Sender<SessionEvent>,
    native: Rc<RefCell<Vec<(String, String)>>>,
    rx: Receiver<SessionEvent>,
    sent: Rc<RefCell<Vec<String>>>,
    fail_send: Rc<RefCell<bool>>,
    named: Rc<RefCell<Vec<String>>>,
    tuned_efforts: Rc<RefCell<Vec<Option<String>>>>,
    fail_effort: Rc<RefCell<bool>>,
    answer_later: Rc<RefCell<bool>>,
    answers: Rc<RefCell<Vec<mpsc::SyncSender<io::Result<()>>>>>,
    native_suggestion: Rc<RefCell<Option<String>>>,
}

impl crate::providers::Session for Scripted {
    fn enqueue(&mut self, id: &str, text: &str) -> io::Result<()> {
        self.native.borrow_mut().push((id.into(), text.into()));
        self.tx
            .send(SessionEvent::Queue(crate::QueueEvent::Accepted(
                crate::QueuedPrompt {
                    id: id.into(),
                    client_id: id.into(),
                    text: text.into(),
                },
            )))
            .unwrap();
        Ok(())
    }
    fn cancel_queued(&mut self, id: &str) -> io::Result<()> {
        self.tx
            .send(SessionEvent::Queue(crate::QueueEvent::Cancelled {
                id: id.into(),
                cancelled: true,
                error: None,
            }))
            .unwrap();
        Ok(())
    }

    fn take_suggestion(&mut self) -> Option<String> {
        self.native_suggestion.borrow_mut().take()
    }

    fn set_effort(&mut self, effort: Option<&str>) -> io::Result<SettingAck> {
        if *self.fail_effort.borrow() {
            return Err(io::Error::other("stub refused effort"));
        }
        self.tuned_efforts
            .borrow_mut()
            .push(effort.map(str::to_string));
        if *self.answer_later.borrow() {
            let (answer, ack) = mpsc::sync_channel(1);
            self.answers.borrow_mut().push(answer);
            return Ok(ack);
        }
        Ok(crate::providers::applied())
    }
    fn events(&self) -> &Receiver<SessionEvent> {
        &self.rx
    }

    fn send(&mut self, text: &str) -> std::io::Result<()> {
        if *self.fail_send.borrow() {
            return Err(io::Error::other("stub refused first prompt"));
        }
        self.sent.borrow_mut().push(text.to_string());
        Ok(())
    }

    fn interrupt(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    fn respond_to_decision(
        &mut self,
        _id: &str,
        _answer: crate::DecisionAnswer,
    ) -> std::io::Result<()> {
        Ok(())
    }

    fn set_name(&mut self, name: &str) -> std::io::Result<()> {
        self.named.borrow_mut().push(name.to_string());
        Ok(())
    }
}

/// Hands out Scripted Sessions and keeps the handles the test drives them
/// with. Nothing here spawns a process.
#[derive(Clone, Default)]
struct Fake {
    native: Rc<RefCell<Vec<(String, String)>>>,
    streams: Rc<RefCell<Vec<Sender<SessionEvent>>>>,
    sent: Rc<RefCell<Vec<String>>>,
    providers: Rc<RefCell<Vec<Provider>>>,
    models: Rc<RefCell<Vec<Option<String>>>>,
    efforts: Rc<RefCell<Vec<Option<String>>>>,
    tuned_efforts: Rc<RefCell<Vec<Option<String>>>>,
    fail_effort: Rc<RefCell<bool>>,
    /// While set, the provider answers effort changes when the test
    /// sends on `answers`, not at once.
    answer_later: Rc<RefCell<bool>>,
    answers: Rc<RefCell<Vec<mpsc::SyncSender<io::Result<()>>>>>,
    resumed: Rc<RefCell<Vec<Option<String>>>>,
    /// The extra CLI arguments each spawn was handed.
    args: Rc<RefCell<Vec<Vec<String>>>>,
    cwds: Rc<RefCell<Vec<Option<std::path::PathBuf>>>>,
    additional_directories: Rc<RefCell<Vec<Vec<std::path::PathBuf>>>>,
    /// The title each spawn was handed.
    names: Rc<RefCell<Vec<Option<String>>>>,
    /// Every rename a live Session was told.
    named: Rc<RefCell<Vec<String>>>,
    /// Every spawn call, successes and refusals alike.
    attempts: Rc<RefCell<usize>>,
    /// While set, spawn refuses — how a test makes a restart fail.
    fail: Rc<RefCell<bool>>,
    fail_at: Rc<RefCell<Option<usize>>>,
    fail_send: Rc<RefCell<bool>>,
    suggestions: Rc<RefCell<Vec<crate::suggest::Request>>>,
    native_suggestion: Rc<RefCell<Option<String>>>,
}

impl Fake {
    fn suggest_calls(&self) -> Vec<crate::suggest::Request> {
        self.suggestions.borrow().clone()
    }
    /// Every spawn's choice, in call order.
    fn spawn_pairs(&self) -> Vec<ProviderChoice> {
        self.providers
            .borrow()
            .iter()
            .zip(self.models.borrow().iter())
            .map(|(provider, model)| ProviderChoice {
                provider: *provider,
                model: model.clone(),
            })
            .collect()
    }
}

impl Spawner for Fake {
    fn suggest(&mut self, request: crate::suggest::Request, _: Sender<Suggestion>) {
        self.suggestions.borrow_mut().push(request);
    }
    fn spawn(
        &mut self,
        request: SpawnRequest,
    ) -> std::io::Result<Box<dyn crate::providers::Session>> {
        *self.attempts.borrow_mut() += 1;
        let attempt = *self.attempts.borrow();
        if *self.fail.borrow() || *self.fail_at.borrow() == Some(attempt) {
            return Err(std::io::Error::other("stub refused to spawn"));
        }
        let (tx, rx) = mpsc::channel();
        self.streams.borrow_mut().push(tx.clone());
        self.providers.borrow_mut().push(request.provider);
        self.models
            .borrow_mut()
            .push(request.model.map(|model| model.to_string()));
        self.efforts
            .borrow_mut()
            .push(request.effort.map(|effort| effort.to_string()));
        self.resumed
            .borrow_mut()
            .push(request.resume.map(|target| target.to_string()));
        self.args.borrow_mut().push(request.args.to_vec());
        self.cwds
            .borrow_mut()
            .push(request.cwd.map(|path| path.to_path_buf()));
        self.additional_directories
            .borrow_mut()
            .push(request.additional_directories);
        self.names
            .borrow_mut()
            .push(request.name.map(|name| name.to_string()));
        Ok(Box::new(Scripted {
            tx,
            native: self.native.clone(),
            rx,
            sent: self.sent.clone(),
            fail_send: self.fail_send.clone(),
            named: self.named.clone(),
            tuned_efforts: self.tuned_efforts.clone(),
            fail_effort: self.fail_effort.clone(),
            answer_later: self.answer_later.clone(),
            answers: self.answers.clone(),
            native_suggestion: self.native_suggestion.clone(),
        }))
    }
}

/// The binding for tests that are not about bindings.
fn main_choice() -> WorkspaceChoice {
    WorkspaceChoice::Main {
        checkout: std::env::temp_dir(),
    }
}

fn cockpit(name: &str) -> (Cockpit, Fake) {
    let fake = Fake::default();
    let store = Store::open(scratch(name)).unwrap();
    (Cockpit::new(store, Box::new(fake.clone())), fake)
}

/// An empty store was never worked in; otherwise the answer is the
/// most recently used Thread's Project. A registry lost by hand
/// orphans nothing, so the Threads keep their headers — but a Project
/// the menu no longer offers is not somewhere to send a draft, and the
/// answer is nothing rather than an id nothing can resolve.
#[test]
fn the_last_worked_project_is_a_registered_one_or_nothing() {
    let (mut core, fake) = cockpit("last-worked-project");
    assert_eq!(core.last_worked_project(), None);
    let other = scratch("last-worked-project-other");
    std::fs::create_dir_all(&other).unwrap();

    let thread = core.open(Provider::Claude, main_choice()).unwrap();
    let project = core.project_id(thread).expect("the open Thread is bound");
    assert_eq!(core.last_worked_project(), Some(project));

    // A newer Thread in another Project outranks the older one.
    let newer = core
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: other })
        .unwrap();
    let newest = core.project_id(newer).expect("the open Thread is bound");
    assert_ne!(newest, project);
    assert_eq!(core.last_worked_project(), Some(newest));

    // The same store with its registry file gone: every Thread still
    // names a Project, and the menu offers none of them.
    // `scratch` clears what it names, so the store's own path is spelled
    // out rather than asked for a second time.
    let dir = std::env::temp_dir().join(format!(
        "ferrite-cockpit-{}-last-worked-project",
        std::process::id()
    ));
    std::fs::remove_file(dir.join("registry.json")).unwrap();
    let reopened = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake));
    assert!(reopened.registry().projects().is_empty());
    assert_eq!(reopened.project_id(newer), Some(newest));
    assert_eq!(reopened.last_worked_project(), None);
}

/// An initialised repo with one committed file, for binding tests.
/// Always under a scratch directory — never near a real checkout.
fn init_repo(root: &Path) -> std::path::PathBuf {
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| crate::workspace::git_for_tests(&repo, args);
    git(&["init", "-q", "-b", "main"]);
    git(&["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("file.txt"), "base\n").unwrap();
    git(&["add", "file.txt"]);
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

/// Drive a Thread's Main through making `worktree` — the command it
/// runs, the tree git really adds, the finish — and the listing that
/// names what it made. Answers the worktree's path as git prints it.
fn follow_into(
    cockpit: &mut Cockpit,
    fake: &Fake,
    thread: ThreadId,
    repo: &Path,
    worktree: &Path,
) -> PathBuf {
    // Git's list as it stood before the command — what a sweep hands in.
    cockpit.worktrees_listed(
        thread,
        crate::workspace::worktree_paths(repo).unwrap(),
        Instant::now(),
    );
    let stream = fake.streams.borrow().last().unwrap().clone();
    let leaf = worktree.file_name().unwrap().to_str().unwrap();
    stream
        .send(SessionEvent::ToolStarted {
            id: "wt".into(),
            name: "Bash".into(),
            input: serde_json::json!({
                "command": format!("git worktree add -b {leaf} {}", worktree.display())
            }),
        })
        .unwrap();
    cockpit.pump();
    crate::workspace::git_for_tests(
        repo,
        &["worktree", "add", "-b", leaf, worktree.to_str().unwrap()],
    );
    stream
        .send(SessionEvent::ToolCompleted {
            id: "wt".into(),
            output: String::new(),
            is_error: false,
            result: crate::ToolResult::Opaque,
        })
        .unwrap();
    cockpit.pump();
    assert!(
        cockpit.wants_worktree_listing(),
        "a finished creation asks for a listing now, not next tick"
    );
    assert!(cockpit.worktrees_listed(
        thread,
        crate::workspace::worktree_paths(repo).unwrap(),
        Instant::now()
    ));
    assert!(!cockpit.wants_worktree_listing());
    crate::workspace::follow::canonical(worktree)
}

/// The whole follow flow at the cockpit's seam (`workspace::follow`):
/// Main runs `git worktree add`, git's next listing names what it
/// made, and the binding moves there — persisted in the header, so a
/// park and revive spawns the next Session inside the worktree the
/// agent chose for itself.
#[test]
fn a_thread_follows_the_worktree_its_main_creates() {
    let root = scratch("follow-create");
    let (mut cockpit, fake) = cockpit("follow-create-store");
    let repo = init_repo(&root);
    let thread = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::Main {
                checkout: repo.clone(),
            },
        )
        .unwrap();

    let path = follow_into(&mut cockpit, &fake, thread, &repo, &root.join("wt-a"));

    let bound = cockpit.thread(thread).unwrap().workspace().cloned();
    assert_eq!(
        bound,
        Some(WorkspaceBinding::Worktree {
            repo: repo.clone(),
            path: path.clone(),
        })
    );
    assert_eq!(
        cockpit.peek(thread).unwrap().workspace,
        bound,
        "the header says so without the live Thread"
    );

    cockpit.park(thread).unwrap();
    cockpit.revive(thread).unwrap();
    assert_eq!(fake.cwds.borrow().last().cloned().flatten(), Some(path));
}

/// Claude's `ExitWorktree` finishing puts Main back in the main
/// checkout, and the binding goes with it.
#[test]
fn exiting_a_worktree_returns_the_thread_to_main() {
    let root = scratch("follow-exit");
    let (mut cockpit, fake) = cockpit("follow-exit-store");
    let repo = init_repo(&root);
    let thread = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::Main {
                checkout: repo.clone(),
            },
        )
        .unwrap();
    follow_into(&mut cockpit, &fake, thread, &repo, &root.join("wt-exit"));

    let stream = fake.streams.borrow()[0].clone();
    stream
        .send(SessionEvent::ToolStarted {
            id: "x".into(),
            name: "ExitWorktree".into(),
            input: serde_json::json!({}),
        })
        .unwrap();
    stream
        .send(SessionEvent::ToolCompleted {
            id: "x".into(),
            output: "Exited worktree".into(),
            is_error: false,
            result: crate::ToolResult::Opaque,
        })
        .unwrap();
    cockpit.pump();

    let main = Some(WorkspaceBinding::Main {
        checkout: repo.clone(),
    });
    assert_eq!(cockpit.thread(thread).unwrap().workspace().cloned(), main);
    assert_eq!(cockpit.peek(thread).unwrap().workspace, main);
}

/// A worktree the agent made is the agent's, not Ferrite's: removed
/// while the Thread is live, the next listing sends the Thread back to
/// main; removed while it is parked, revive does the same rather than
/// rebuilding somebody else's tree. Only registered worktrees self-heal.
#[test]
fn a_followed_worktree_that_vanishes_returns_the_thread_to_main() {
    let root = scratch("follow-vanish");
    let (mut cockpit, fake) = cockpit("follow-vanish-store");
    let repo = init_repo(&root);
    let thread = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::Main {
                checkout: repo.clone(),
            },
        )
        .unwrap();
    let main = Some(WorkspaceBinding::Main {
        checkout: repo.clone(),
    });
    let git = |args: &[&str]| crate::workspace::git_for_tests(&repo, args);

    // Live: gone from git's list, gone from the binding.
    let live = follow_into(&mut cockpit, &fake, thread, &repo, &root.join("wt-live"));
    git(&["worktree", "remove", "--force", live.to_str().unwrap()]);
    assert!(cockpit.worktrees_listed(
        thread,
        crate::workspace::worktree_paths(&repo).unwrap(),
        Instant::now()
    ));
    assert_eq!(cockpit.thread(thread).unwrap().workspace().cloned(), main);

    // Parked: revive finds it missing and spawns in the main checkout.
    let parked = follow_into(&mut cockpit, &fake, thread, &repo, &root.join("wt-parked"));
    cockpit.park(thread).unwrap();
    git(&["worktree", "remove", "--force", parked.to_str().unwrap()]);
    cockpit.revive(thread).unwrap();
    assert_eq!(
        fake.cwds.borrow().last().cloned().flatten(),
        Some(repo.clone())
    );
    assert_eq!(cockpit.thread(thread).unwrap().workspace().cloned(), main);
    assert_eq!(cockpit.peek(thread).unwrap().workspace, main);
    assert!(
        !parked.exists(),
        "an adopted worktree is never rebuilt on revive"
    );
}

/// The move rewrites the header, and a longer header shifts every
/// byte after it: a child-history read already in flight holds a
/// checkpoint into the old log and would land short. It is dropped
/// before the rewrite (ADR 0002), like the model and effort setters do.
#[test]
fn a_move_drops_child_history_reads_in_flight() {
    use crate::activity::{ActivityEvent, ActivityInput, AgentKey, ExecutionEvent, Subject};
    let root = scratch("follow-history");
    let (mut cockpit, fake) = cockpit("follow-history-store");
    let repo = init_repo(&root);
    let thread = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::Main {
                checkout: repo.clone(),
            },
        )
        .unwrap();
    let key = AgentKey::new(Provider::Claude, "root", "child");
    let subject = Subject::Subagent(key.clone());
    fake.streams.borrow()[0]
        .send(SessionEvent::Activity(ActivityEvent::Content {
            key,
            id: Some("start".into()),
            event: ExecutionEvent::Text {
                text: "Working".into(),
            },
        }))
        .unwrap();
    cockpit.pump();
    cockpit
        .engine_mut()
        .threads
        .get_mut(&thread)
        .unwrap()
        .activity
        .apply(ActivityInput::Evict(subject.clone()));
    let (loader, _finish) = history::gated_loader();
    cockpit.engine_mut().history_loader = Some(loader);
    assert!(cockpit.ensure_subject_history(thread, &subject).unwrap());
    assert!(
        !cockpit.engine().threads[&thread].history.is_empty(),
        "a read is pending against the old log"
    );

    follow_into(&mut cockpit, &fake, thread, &repo, &root.join("wt-history"));

    assert!(
        cockpit.engine().threads[&thread].history.is_empty(),
        "the pending read was invalidated before the header rewrite"
    );
}

/// A run of Main's edits inside a worktree that already stood moves
/// the Thread there — but a Subagent's never do, however many: it may
/// well be working somewhere else on purpose.
#[test]
fn only_mains_own_work_moves_the_thread() {
    use crate::activity::{ActivityEvent, AgentKey, ExecutionEvent};
    let root = scratch("follow-evidence");
    let (mut cockpit, fake) = cockpit("follow-evidence-store");
    let repo = init_repo(&root);
    let worktree = root.join("wt-standing");
    crate::workspace::git_for_tests(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            "standing",
            worktree.to_str().unwrap(),
        ],
    );
    let thread = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::Main {
                checkout: repo.clone(),
            },
        )
        .unwrap();
    cockpit.worktrees_listed(
        thread,
        crate::workspace::worktree_paths(&repo).unwrap(),
        Instant::now(),
    );
    let stream = fake.streams.borrow()[0].clone();
    let edit = |n: usize| ExecutionEvent::ToolStarted {
        id: format!("e{n}"),
        name: "Edit".into(),
        input: serde_json::json!({ "file_path": worktree.join("file.txt") }),
    };

    let child = AgentKey::new(Provider::Claude, "root", "child");
    for n in 0..crate::workspace::follow::EVIDENCE_CALLS as usize + 2 {
        stream
            .send(SessionEvent::Activity(ActivityEvent::Content {
                key: child.clone(),
                id: Some(format!("c{n}")),
                event: edit(n),
            }))
            .unwrap();
    }
    cockpit.pump();
    assert_eq!(
        cockpit.thread(thread).unwrap().workspace().cloned(),
        Some(WorkspaceBinding::Main {
            checkout: repo.clone(),
        }),
        "a Subagent's work never moves the Thread"
    );

    for n in 0..crate::workspace::follow::EVIDENCE_CALLS as usize {
        stream
            .send(SessionEvent::Activity(ActivityEvent::MainContent {
                id: Some(format!("m{n}")),
                event: edit(n),
            }))
            .unwrap();
    }
    cockpit.pump();
    assert_eq!(
        cockpit.thread(thread).unwrap().workspace().cloned(),
        Some(WorkspaceBinding::Worktree {
            repo: repo.clone(),
            path: crate::workspace::follow::canonical(&worktree),
        })
    );
}

/// Choosing an existing branch moves the project checkout itself onto
/// it before the Session spawns — one checkout, one branch, and the
/// Thread's cwd is the repo the operator is already looking at.
#[test]
fn opening_on_an_existing_branch_switches_the_project_checkout() {
    let root = scratch("branch-open");
    let (mut cockpit, _fake) = cockpit("branch-open-store");
    let repo = init_repo(&root);
    crate::workspace::git_for_tests(&repo, &["switch", "-q", "-c", "feature/side"]);

    let thread = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::Branch {
                checkout: repo.clone(),
                branch: "main".into(),
            },
        )
        .unwrap();

    assert_eq!(
        crate::workspace::checkout_branch(&repo).as_deref(),
        Some("main")
    );
    assert_eq!(
        cockpit.thread(thread).unwrap().workspace().unwrap().cwd(),
        repo
    );
}

/// A fresh branch is minted, created and checked out in the project's
/// own checkout — the no-worktree half of the composer's two ways to
/// start something new.
#[test]
fn opening_on_a_new_branch_creates_it_in_the_project_checkout() {
    let root = scratch("new-branch-open");
    let (mut cockpit, _fake) = cockpit("new-branch-open-store");
    let repo = init_repo(&root);

    cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::NewBranch {
                checkout: repo.clone(),
            },
        )
        .unwrap();

    let branch = crate::workspace::checkout_branch(&repo).expect("a branch");
    assert!(branch.starts_with("ferrite/branch-"), "branch: {branch}");
}

/// The whole binding flow at the cockpit's own seam: the choice becomes
/// a worktree on disk, the Session spawns inside it, and the Pane's
/// chrome can say so.
#[test]
fn opening_with_a_worktree_choice_creates_it_and_spawns_the_session_inside() {
    let root = scratch("worktree-open");
    let (mut cockpit, fake) = cockpit("worktree-open-store");
    let repo = init_repo(&root);

    let thread = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::NewWorktree { repo: repo.clone() },
        )
        .unwrap();

    let Some(WorkspaceBinding::Worktree { path, repo: bound }) = cockpit
        .thread(thread)
        .and_then(|open| open.workspace())
        .cloned()
    else {
        panic!(
            "the Pane must know its binding: {:?}",
            cockpit.thread(thread).and_then(|open| open.workspace())
        );
    };
    assert_eq!(bound, repo);
    assert!(path.join(".git").exists(), "no worktree at {path:?}");
    assert_eq!(
        fake.cwds.borrow().last().unwrap().as_deref(),
        Some(path.as_path()),
        "the Session must spawn inside the worktree"
    );
}

#[test]
fn opening_a_project_exposes_its_additional_directories_without_changing_cwd() {
    let root = scratch("project-extra-roots");
    let primary = root.join("primary");
    let extra = root.join("extra");
    std::fs::create_dir_all(&primary).unwrap();
    std::fs::create_dir_all(&extra).unwrap();
    let (mut cockpit, fake) = cockpit("project-extra-roots-store");
    let project = cockpit.register_project(&primary).unwrap();
    cockpit
        .add_project_directories(project, std::slice::from_ref(&extra))
        .unwrap();

    cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::Main {
                checkout: primary.clone(),
            },
        )
        .unwrap();

    assert_eq!(
        fake.cwds.borrow().last().unwrap().as_deref(),
        Some(primary.as_path())
    );
    assert_eq!(
        fake.additional_directories.borrow().last().unwrap(),
        &[extra.canonicalize().unwrap()]
    );
}

/// #29: opening registers the choice's repo as a project (the registry
/// grows as Threads bind roots), a new worktree lands in the central
/// layout on a registry-minted branch, and the Thread header carries
/// the project id.
#[test]
fn opening_registers_the_project_and_places_the_worktree_centrally() {
    let root = scratch("central-open");
    let dir = scratch("central-open-store");
    let repo = init_repo(&root);
    let fake = Fake::default();
    let mut cockpit = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake));

    let thread = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::NewWorktree { repo: repo.clone() },
        )
        .unwrap();

    let projects = cockpit.registry().projects();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].title, "repo");
    let project = projects[0].id;
    assert_eq!(cockpit.peek(thread).unwrap().project_id, Some(project));

    let entries = cockpit.registry().worktrees(project);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].branch, "ferrite/wt-1");
    let path = cockpit
        .thread(thread)
        .and_then(|open| open.workspace())
        .unwrap()
        .cwd()
        .to_path_buf();
    assert_eq!(entries[0].path, path);
    // The central layout: under the store's worktrees/, never inside
    // the repo or a per-Thread store directory.
    assert!(
        path.starts_with(dir.join("worktrees")),
        "central layout: {path:?}"
    );
    assert_eq!(path.file_name().unwrap(), "ferrite-wt-1");
}

/// #29: choosing an existing worktree adopts the standing tree — no
/// creation, no new branch — and the Session spawns into it.
#[test]
fn opening_with_an_existing_worktree_adopts_the_standing_tree() {
    let root = scratch("adopt-open");
    let (mut cockpit, fake) = cockpit("adopt-open-store");
    let repo = init_repo(&root);
    let first = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::NewWorktree { repo: repo.clone() },
        )
        .unwrap();
    let path = cockpit
        .thread(first)
        .and_then(|open| open.workspace())
        .unwrap()
        .cwd()
        .to_path_buf();
    // The first Thread is gone; its worktree outlives it as the
    // adoptable row. Deleting would remove the tree, so park instead.
    cockpit.park(first).unwrap();

    let second = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::ExistingWorktree {
                repo: repo.clone(),
                path: path.clone(),
            },
        )
        .unwrap();

    assert_eq!(
        cockpit.thread(second).and_then(|open| open.workspace()),
        Some(&WorkspaceBinding::Worktree {
            repo: repo.clone(),
            path: path.clone(),
        })
    );
    assert_eq!(
        fake.cwds.borrow().last().unwrap().as_deref(),
        Some(path.as_path())
    );
    // Adoption minted nothing: one worktree entry, one branch.
    let project = cockpit.registry().projects()[0].id;
    assert_eq!(cockpit.registry().worktrees(project).len(), 1);
    let branches = crate::workspace::git_for_tests(&repo, &["branch", "--list", "ferrite/*"]);
    assert_eq!(branches.matches("ferrite/").count(), 1, "{branches:?}");
}

/// #29: the adoption conflict check — a directory that git does not
/// call a worktree of the repo refuses the whole open, and no Thread
/// is left behind.
#[test]
fn adopting_a_directory_that_is_not_a_worktree_refuses_and_leaves_no_thread() {
    let root = scratch("adopt-refused");
    let (mut cockpit, _fake) = cockpit("adopt-refused-store");
    let repo = init_repo(&root);
    let squatter = root.join("squatter");
    std::fs::create_dir_all(&squatter).unwrap();

    let refused = cockpit.open(
        Provider::Claude,
        WorkspaceChoice::ExistingWorktree {
            repo: repo.clone(),
            path: squatter.clone(),
        },
    );

    assert!(refused.is_err(), "a squatting directory must refuse");
    assert!(cockpit.threads().is_empty());
    assert_eq!(cockpit.parked().unwrap(), vec![], "no half-born Thread");
}

/// A worktree the operator made by hand — one git lists, the registry
/// never minted — adopts the same way: the Thread binds to its path,
/// the Session spawns there, and nothing is created or registered.
#[test]
fn a_hand_made_worktree_is_adopted_without_minting_or_registering() {
    let root = scratch("adopt-hand-made");
    let (mut cockpit, fake) = cockpit("adopt-hand-made-store");
    let repo = init_repo(&root);
    let hand_made = root.join("hand-made");
    crate::workspace::ensure_worktree(&repo, &hand_made, "feature/hand-made").unwrap();

    let thread = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::ExistingWorktree {
                repo: repo.clone(),
                path: hand_made.clone(),
            },
        )
        .unwrap();

    assert_eq!(
        cockpit.thread(thread).and_then(|open| open.workspace()),
        Some(&WorkspaceBinding::Worktree {
            repo: repo.clone(),
            path: hand_made.clone(),
        })
    );
    assert_eq!(
        fake.cwds.borrow().last().unwrap().as_deref(),
        Some(hand_made.as_path())
    );
    let project = cockpit.registry().projects()[0].id;
    assert!(
        cockpit.registry().worktrees(project).is_empty(),
        "adoption registers nothing"
    );
    assert_eq!(
        crate::workspace::worktree_paths(&repo).unwrap().len(),
        2,
        "adoption creates nothing"
    );
}

/// #29's bootstrap failure contract: a Session that will not spawn
/// takes the half-born Thread with it — the draft Pane stays draft and
/// nothing claims to exist. The worktree just created stays, real and
/// registered: it is adoptable, not half-born.
#[test]
fn a_failed_spawn_at_bootstrap_leaves_no_thread_behind() {
    let root = scratch("bootstrap-fails");
    let (mut cockpit, fake) = cockpit("bootstrap-fails-store");
    let repo = init_repo(&root);
    *fake.fail.borrow_mut() = true;

    let refused = cockpit.open(
        Provider::Claude,
        WorkspaceChoice::NewWorktree { repo: repo.clone() },
    );

    assert!(refused.is_err());
    assert!(cockpit.threads().is_empty());
    assert_eq!(cockpit.parked().unwrap(), vec![], "no half-born Thread");
    let project = cockpit.registry().projects()[0].id;
    assert_eq!(
        cockpit.registry().worktrees(project).len(),
        1,
        "the created worktree survives as an adoptable entry"
    );
}

#[test]
fn a_failed_thread_create_removes_the_reserved_worktree() {
    let root = scratch("create-fails-after-reserve");
    let repo = init_repo(&root);
    let store = Store::open(scratch("create-fails-after-reserve-store"))
        .unwrap()
        .refuse_create();
    let mut cockpit = Cockpit::new(store, Box::new(Fake::default()));

    let refused = cockpit.open(Provider::Claude, WorkspaceChoice::NewWorktree { repo });

    assert!(refused.is_err());
    assert!(cockpit.threads().is_empty());
    let project = cockpit.registry().projects()[0].id;
    assert!(cockpit.registry().worktrees(project).is_empty());
}

#[test]
fn a_failed_first_send_leaves_no_thread_or_log() {
    let root = scratch("first-send-fails");
    let (mut cockpit, fake) = cockpit("first-send-fails-store");
    let repo = init_repo(&root);
    *fake.fail_send.borrow_mut() = true;

    let refused = cockpit.bootstrap(
        ProviderChoice {
            provider: Provider::Codex,
            model: Some("announced-model".into()),
        },
        WorkspaceChoice::Main { checkout: repo },
        "exact drafted prompt",
    );

    assert!(refused.is_err());
    assert!(cockpit.threads().is_empty());
    assert!(cockpit.parked().unwrap().is_empty(), "the log was removed");
    assert!(fake.sent.borrow().is_empty());
}

#[test]
fn grouped_bootstrap_persists_membership_before_sending() {
    let root = scratch("group-bootstrap-persist");
    let repo = init_repo(&root);
    let store_dir = scratch("group-bootstrap-persist-store");
    let fake = Fake::default();
    let mut cockpit = Cockpit::new(Store::open(&store_dir).unwrap(), Box::new(fake.clone()));
    let seed = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::Main {
                checkout: repo.clone(),
            },
        )
        .unwrap();
    let second = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::Main {
                checkout: repo.clone(),
            },
        )
        .unwrap();
    let group = cockpit
        .apply_group(GroupChange::Create {
            first: seed,
            second,
        })
        .unwrap()
        .group
        .unwrap();
    let groups_path = store_dir.join("groups.json");
    std::fs::remove_file(&groups_path).unwrap();
    std::fs::create_dir(&groups_path).unwrap();

    let refused = cockpit.bootstrap_in_group(
        ProviderChoice {
            provider: Provider::Claude,
            model: None,
        },
        WorkspaceChoice::Main { checkout: repo },
        "must not be sent",
        group,
    );

    assert!(refused.is_err());
    assert_eq!(fake.sent.borrow().as_slice(), &[] as &[String]);
    assert_eq!(cockpit.threads(), vec![seed, second]);
    assert_eq!(
        cockpit.groups().get(group).unwrap().members,
        vec![seed, second]
    );
    assert!(cockpit.parked().unwrap().is_empty());
}

#[test]
fn opening_on_the_main_checkout_spawns_the_session_there() {
    let root = scratch("main-open");
    let (mut cockpit, fake) = cockpit("main-open-store");
    let repo = init_repo(&root);

    let thread = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::Main {
                checkout: repo.clone(),
            },
        )
        .unwrap();

    assert_eq!(
        cockpit.thread(thread).and_then(|open| open.workspace()),
        Some(&WorkspaceBinding::Main {
            checkout: repo.clone(),
        })
    );
    assert_eq!(
        fake.cwds.borrow().last().unwrap().as_deref(),
        Some(repo.as_path())
    );
}

/// AC: the binding is restored on relaunch — and the worktree comes back
/// on demand even when someone deleted it by hand while Ferrite was off.
#[test]
fn a_relaunch_restores_the_binding_and_spawns_where_it_points() {
    let root = scratch("relaunch-binding");
    let dir = scratch("relaunch-binding-store");
    let repo = init_repo(&root);
    let fake = Fake::default();
    let mut cockpit = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake.clone()));
    let thread = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::NewWorktree { repo: repo.clone() },
        )
        .unwrap();
    let binding = cockpit
        .thread(thread)
        .and_then(|open| open.workspace())
        .cloned()
        .unwrap();
    cockpit.park(thread).unwrap();
    drop(cockpit);
    // While Ferrite is off, the worktree vanishes by hand.
    std::fs::remove_dir_all(binding.cwd()).unwrap();

    // A new run of Ferrite over the same store.
    let relaunched_fake = Fake::default();
    let mut relaunched = Cockpit::new(
        Store::open(&dir).unwrap(),
        Box::new(relaunched_fake.clone()),
    );
    relaunched.revive(thread).unwrap();

    assert_eq!(
        relaunched.thread(thread).and_then(|open| open.workspace()),
        Some(&binding)
    );
    assert!(
        binding.cwd().join(".git").exists(),
        "the worktree must come back on demand"
    );
    assert_eq!(
        relaunched_fake.cwds.borrow().last().unwrap().as_deref(),
        Some(binding.cwd())
    );
}

/// ADR 0018: a revived Thread's git calls are the spawner's to run,
/// ahead of the CLI, never the caller's. A spawner that holds the
/// preparation back leaves the vanished worktree missing after
/// `revive_after` returns; running the preparation brings it back.
#[test]
fn revive_leaves_the_worktree_to_the_spawners_preparation() {
    struct Holding {
        fake: Fake,
        held: Rc<RefCell<Vec<Preparation>>>,
    }
    impl Spawner for Holding {
        fn spawn(
            &mut self,
            request: SpawnRequest,
        ) -> io::Result<Box<dyn crate::providers::Session>> {
            self.fake.spawn(request)
        }
        fn start_after(
            &mut self,
            prepare: Preparation,
            request: SpawnRequest,
        ) -> io::Result<SessionLifecycle> {
            self.held.borrow_mut().push(prepare);
            self.start(request)
        }
    }
    let root = scratch("revive-prepared");
    let dir = scratch("revive-prepared-store");
    let repo = init_repo(&root);
    let mut cockpit = Cockpit::new(Store::open(&dir).unwrap(), Box::new(Fake::default()));
    let thread = cockpit
        .open(Provider::Claude, WorkspaceChoice::NewWorktree { repo })
        .unwrap();
    let worktree = cockpit
        .thread(thread)
        .unwrap()
        .workspace()
        .unwrap()
        .cwd()
        .to_path_buf();
    cockpit.park(thread).unwrap();
    drop(cockpit);
    std::fs::remove_dir_all(&worktree).unwrap();

    let held = Rc::new(RefCell::new(Vec::new()));
    let mut relaunched = Cockpit::new(
        Store::open(&dir).unwrap(),
        Box::new(Holding {
            fake: Fake::default(),
            held: held.clone(),
        }),
    );
    relaunched
        .revive_after(thread, Duration::from_millis(1))
        .unwrap();

    assert!(!worktree.exists(), "no git on the caller's thread");
    let prepare = held.borrow_mut().pop().expect("the spawner was handed it");
    prepare().unwrap();
    assert!(worktree.join(".git").exists(), "the preparation remakes it");
}

/// ADR 0018: a worktree a revival could not place is not given up on.
/// Every later start of the Thread's Session — here the watchdog's
/// restart — remakes a missing worktree before the CLI spawns into it.
#[test]
fn a_restart_remakes_a_missing_worktree_first() {
    let root = scratch("restart-remakes");
    let repo = init_repo(&root);
    let (mut cockpit, _fake) = cockpit("restart-remakes-store");
    let rss = Rc::new(RefCell::new(4 * 1024 * 1024 * 1024));
    cockpit.watch_memory(Box::new(Meter(rss)), 1024 * 1024 * 1024);
    let thread = cockpit
        .open(Provider::Claude, WorkspaceChoice::NewWorktree { repo })
        .unwrap();
    let worktree = cockpit
        .thread(thread)
        .unwrap()
        .workspace()
        .unwrap()
        .cwd()
        .to_path_buf();
    std::fs::remove_dir_all(&worktree).unwrap();

    assert_eq!(cockpit.sweep().len(), 1);

    assert!(worktree.join(".git").exists(), "remade before the restart");
}

/// Registered central worktrees outlive every individual Thread. Two
/// Threads may share one adoption, so deleting either cannot inspect or
/// remove the checkout beneath the other.
#[test]
fn deleting_a_thread_preserves_a_shared_registered_worktree() {
    let root = scratch("delete-worktree");
    let (mut cockpit, _fake) = cockpit("delete-worktree-store");
    let repo = init_repo(&root);
    let first = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::NewWorktree { repo: repo.clone() },
        )
        .unwrap();
    let path = cockpit
        .thread(first)
        .and_then(|open| open.workspace())
        .unwrap()
        .cwd()
        .to_path_buf();
    let second = cockpit
        .open(
            Provider::Codex,
            WorkspaceChoice::ExistingWorktree {
                repo: repo.clone(),
                path: path.clone(),
            },
        )
        .unwrap();
    std::fs::write(path.join("wip.txt"), "uncommitted\n").unwrap();
    cockpit.delete(first).unwrap();
    assert_eq!(
        std::fs::read_to_string(path.join("wip.txt")).unwrap(),
        "uncommitted\n",
        "the registered checkout survives"
    );
    assert!(
        cockpit
            .thread(second)
            .and_then(|open| open.workspace())
            .is_some(),
        "the other Thread still uses the checkout"
    );
    assert_eq!(
        cockpit
            .registry()
            .worktrees(cockpit.registry().projects()[0].id)
            .len(),
        1
    );
}

/// A Thread from before the registry, bound to an unregistered worktree,
/// takes its clean tree with it; the tree is told as removed, as
/// `worktree.remove`'s is (Projects' `worktree.removed`, ADR 0031).
#[test]
fn deleting_a_legacy_worktree_thread_tells_its_tree_removed() {
    let root = scratch("delete-legacy-worktree");
    let repo = init_repo(&root);
    let path = root.join("legacy-tree");
    crate::workspace::git_for_tests(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "legacy",
            path.to_str().unwrap(),
        ],
    );
    let store = Store::open(scratch("delete-legacy-worktree-store")).unwrap();
    let binding = WorkspaceBinding::Worktree {
        repo,
        path: path.clone(),
    };
    let (thread, writer) = store.create(Provider::Claude, None, binding).unwrap();
    drop(writer);
    let mut engine = Engine::try_new(store, Box::new(Fake::default())).unwrap();

    engine.delete(thread).unwrap();

    assert!(!path.exists(), "the clean tree is removed");
    assert_eq!(engine.take_removed_worktrees(), [path]);
    assert!(engine.take_removed_worktrees().is_empty(), "told once");
}

/// Deleting a main-bound Thread deletes the Thread — the operator's
/// checkout is not Ferrite's to touch.
#[test]
fn deleting_a_main_bound_thread_leaves_the_checkout_alone() {
    let root = scratch("delete-main");
    let (mut cockpit, _fake) = cockpit("delete-main-store");
    let repo = init_repo(&root);
    let thread = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::Main {
                checkout: repo.clone(),
            },
        )
        .unwrap();
    // The checkout is even dirty — irrelevant: it is not a worktree
    // Ferrite owns.
    std::fs::write(repo.join("operator.txt"), "the operator's own\n").unwrap();

    cockpit.delete(thread).unwrap();

    assert!(cockpit.threads().is_empty());
    assert_eq!(cockpit.parked().unwrap(), vec![]);
    assert_eq!(
        std::fs::read_to_string(repo.join("operator.txt")).unwrap(),
        "the operator's own\n"
    );
}

#[test]
fn deleting_a_grouped_thread_removes_membership_before_the_log() {
    let root = scratch("delete-grouped");
    let repo = init_repo(&root);
    let (mut cockpit, _fake) = cockpit("delete-grouped-store");
    let one = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::Main {
                checkout: repo.clone(),
            },
        )
        .unwrap();
    let two = cockpit
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: repo })
        .unwrap();
    let group = cockpit
        .apply_group(GroupChange::Create {
            first: one,
            second: two,
        })
        .unwrap()
        .group
        .unwrap();

    cockpit.delete(one).unwrap();

    assert!(cockpit.groups().get(group).is_none());
    assert_eq!(cockpit.threads(), vec![two]);
    assert_eq!(cockpit.engine().store.thread_ids().unwrap(), vec![two]);
}

#[test]
fn failed_delete_restores_the_exact_group_snapshot() {
    let store = Store::open(scratch("delete-group-rollback"))
        .unwrap()
        .refuse_delete();
    let mut cockpit = Cockpit::new(store, Box::new(Fake::default()));
    let threads: Vec<_> = (0..4)
        .map(|_| cockpit.open(Provider::Claude, main_choice()).unwrap())
        .collect();
    let first = cockpit
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    let second = cockpit
        .apply_group(GroupChange::Create {
            first: threads[2],
            second: threads[3],
        })
        .unwrap()
        .group
        .unwrap();
    cockpit
        .apply_group(GroupChange::Rename {
            group: first,
            title: "exact title".into(),
        })
        .unwrap();
    cockpit
        .apply_group(GroupChange::MoveGroup {
            group: second,
            index: 0,
        })
        .unwrap();
    let before: Vec<_> = cockpit.groups().iter().cloned().collect();

    assert!(cockpit.delete(threads[0]).is_err());
    assert_eq!(cockpit.groups().iter().cloned().collect::<Vec<_>>(), before);
}

#[test]
fn revive_group_is_ordered_and_rolls_back_when_a_later_member_fails() {
    let (mut cockpit, fake) = cockpit("revive-group-atomic");
    let first = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let second = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let third = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let group = cockpit
        .apply_group(GroupChange::Create { first, second })
        .unwrap()
        .group
        .unwrap();
    cockpit
        .apply_group(GroupChange::Join {
            thread: third,
            group,
            index: None,
        })
        .unwrap();
    cockpit.park(first).unwrap();
    cockpit.park(second).unwrap();
    cockpit.park(third).unwrap();
    cockpit.engine_mut().refuse_park.insert(first);
    *fake.fail_at.borrow_mut() = Some(*fake.attempts.borrow() + 3);

    assert!(matches!(
        cockpit.revive_group(group),
        Err(ReviveGroupError::Rollback { thread, .. }) if thread == first
    ));
    assert!(
        cockpit.threads().is_empty(),
        "the first revival rolled back"
    );
    assert_eq!(
        cockpit.groups().get(group).unwrap().members,
        [first, second, third]
    );

    cockpit.engine_mut().refuse_park.clear();
    *fake.fail_at.borrow_mut() = None;
    assert_eq!(cockpit.revive_group(group).unwrap(), [first, second, third]);
    assert_eq!(cockpit.threads(), [first, second, third]);
}

#[test]
fn thread_title_is_durable_and_blank_rename_is_refused() {
    let dir = scratch("thread-title");
    let fake = Fake::default();
    let mut cockpit = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake.clone()));
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();

    cockpit.rename_thread(thread, "  Parser work  ").unwrap();
    assert_eq!(
        cockpit.thread_title(thread).unwrap().as_deref(),
        Some("Parser work")
    );
    assert!(cockpit.rename_thread(thread, "   ").is_err());
    cockpit.park(thread).unwrap();
    assert_eq!(
        cockpit.thread_title(thread).unwrap().as_deref(),
        Some("Parser work"),
        "the one title seam reads parked Threads too"
    );
    drop(cockpit);

    let mut relaunched = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake));
    relaunched.revive(thread).unwrap();
    assert_eq!(
        relaunched.thread_title(thread).unwrap().as_deref(),
        Some("Parser work")
    );
}

/// AC: two Threads on the same repo in separate worktrees cannot touch
/// each other's tree — through the cockpit's own seam, in the cwds it
/// hands the Sessions.
#[test]
fn two_threads_on_one_repo_work_in_isolated_worktrees() {
    let root = scratch("two-threads");
    let (mut cockpit, fake) = cockpit("two-threads-store");
    let repo = init_repo(&root);

    let one = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::NewWorktree { repo: repo.clone() },
        )
        .unwrap();
    let two = cockpit
        .open(
            Provider::Codex,
            WorkspaceChoice::NewWorktree { repo: repo.clone() },
        )
        .unwrap();

    let cwds = fake.cwds.borrow();
    let cwd_one = cwds[0].clone().expect("thread one has a cwd");
    let cwd_two = cwds[1].clone().expect("thread two has a cwd");
    drop(cwds);
    assert_ne!(cwd_one, cwd_two, "two Threads must not share a tree");
    assert_ne!(one, two);

    // Each "agent" writes in its own workspace; neither sees the other.
    std::fs::write(cwd_one.join("only-one.txt"), "one\n").unwrap();
    std::fs::write(cwd_two.join("file.txt"), "two edited\n").unwrap();
    let status_one = crate::workspace::git_for_tests(&cwd_one, &["status", "--porcelain"]);
    let status_two = crate::workspace::git_for_tests(&cwd_two, &["status", "--porcelain"]);
    assert!(status_one.contains("only-one.txt"), "one: {status_one}");
    assert!(!status_one.contains("file.txt"), "one: {status_one}");
    assert!(status_two.contains("file.txt"), "two: {status_two}");
    assert!(!status_two.contains("only-one.txt"), "two: {status_two}");
    assert!(!cwd_two.join("only-one.txt").exists());
    assert_eq!(
        std::fs::read_to_string(cwd_one.join("file.txt")).unwrap(),
        "base\n"
    );
}

fn text(s: &str) -> SessionEvent {
    SessionEvent::TextDelta { text: s.into() }
}

/// A finished Main turn buys exactly one prediction, carrying both sides
/// of the exchange. Recorded rather than spawned: the suite must never
/// shell out to a real CLI, let alone pay one.
/// The completion observation now times every outcome, so an
/// interrupted turn's row reads `interrupted · 4.1s`. Timing it must not
/// make it a completion: no completed stamp, no "finished" notice, no
/// follow-up prediction, and the transcript still reads not-completed.
#[test]
fn a_timed_interruption_is_never_read_as_a_completion() {
    let (mut cockpit, fake) = cockpit("timed-interruption");
    cockpit.set_notification_grace(Duration::ZERO);
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.send(thread, "work".into());
    fake.streams.borrow()[0].send(text("partial")).unwrap();
    cockpit.pump();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Interrupted,
            cost_usd: None,
        })
        .unwrap();
    cockpit.pump();
    cockpit.pump();
    let transcript = cockpit.thread(thread).unwrap().transcript();
    let ends: Vec<_> = transcript
        .blocks()
        .iter()
        .filter_map(|block| match &block.body {
            Body::TurnEnd(end) => Some(end.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(ends.len(), 1, "the observation times the row, adds none");
    assert_eq!(ends[0].outcome, crate::TurnOutcome::Interrupted);
    assert!(ends[0].elapsed_ms.is_some(), "the interruption is timed");
    assert!(ends[0].text().starts_with("interrupted · "));
    assert!(!transcript.turn_completed());
    assert!(
        cockpit
            .notifications()
            .notices()
            .all(|notice| notice.thread != thread),
        "an interruption raises no finished notice"
    );
    assert!(
        fake.suggest_calls().is_empty(),
        "an interruption buys no follow-up prediction"
    );
}

/// Seen means on screen while the window has focus: a Group's member
/// that finishes in view is read with no focus of its own, a finish
/// while the window is away stays unread until it comes back, and a
/// Thread off the board stays unread whatever the window does.
#[test]
fn a_finish_is_seen_when_it_is_on_screen_while_the_window_has_focus() {
    let (mut cockpit, fake) = cockpit("seen-on-screen");
    cockpit.set_notification_grace(Duration::ZERO);
    let threads: Vec<ThreadId> = (0..3)
        .map(|_| cockpit.open(Provider::Claude, main_choice()).unwrap())
        .collect();
    let group = cockpit
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    cockpit.enter_group(group).unwrap();
    let finish = |cockpit: &mut Cockpit, at: usize| {
        cockpit.send(threads[at], "work".into());
        cockpit.pump();
        fake.streams.borrow()[at]
            .send(SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
        cockpit.pump();
        cockpit.pump();
    };
    let unread = |cockpit: &Cockpit, at: usize| cockpit.notifications().attention(threads[at]);

    finish(&mut cockpit, 1);
    assert!(!unread(&cockpit, 1), "on the board, focused or not");
    finish(&mut cockpit, 2);
    assert!(unread(&cockpit, 2), "off the board");

    cockpit.set_window_active(false);
    finish(&mut cockpit, 0);
    assert!(unread(&cockpit, 0), "on the board while the window is away");
    cockpit.set_window_active(true);
    assert!(!unread(&cockpit, 0), "seen the moment the window is back");
    assert!(unread(&cockpit, 2), "and still not what is off the board");
}

#[test]
fn a_finished_turn_asks_for_one_follow_up_prediction() {
    let (mut cockpit, fake) = cockpit("suggest-trigger");
    let thread = cockpit.open(Provider::Codex, main_choice()).unwrap();
    cockpit.send(thread, "fix the decoder".into());

    fake.streams.borrow()[0]
        .send(text("Fixed it. Want me to run the tests?"))
        .unwrap();
    cockpit.pump();
    assert!(
        fake.suggest_calls().is_empty(),
        "a turn still streaming has nothing settled to predict from"
    );

    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    cockpit.pump();

    let calls = fake.suggest_calls();
    assert_eq!(calls.len(), 1, "one turn, one ask");
    assert_eq!(calls[0].thread, thread);
    assert!(
        calls[0].context.contains("fix the decoder"),
        "{:?}",
        calls[0]
    );
    assert!(
        calls[0].context.contains("Want me to run the tests?"),
        "{:?}",
        calls[0]
    );

    // A frame with no new turn end must not ask again — this is the
    // difference between one Haiku call per turn and one per repaint.
    cockpit.pump();
    cockpit.pump();
    assert_eq!(fake.suggest_calls().len(), 1, "one turn, still one ask");
}

/// Codex requests never depend on discovering or running Claude.
#[test]
fn a_codex_thread_requests_a_codex_prediction() {
    let (mut cockpit, fake) = cockpit("suggest-no-cli");
    let thread = cockpit.open(Provider::Codex, main_choice()).unwrap();
    cockpit.send(thread, "go".into());

    fake.streams.borrow()[0].send(text("Done.")).unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    cockpit.pump();

    assert_eq!(fake.suggest_calls()[0].provider, Provider::Codex);
    assert_eq!(cockpit.thread(thread).unwrap().suggestion(), None);
}

#[test]
fn disabled_suggestions_neither_run_nor_land() {
    let (mut cockpit, fake) = cockpit("suggest-disabled");
    let thread = cockpit.open(Provider::Codex, main_choice()).unwrap();
    cockpit.set_suggestions_enabled(false);
    cockpit.send(thread, "go".into());

    fake.streams.borrow()[0].send(text("Done.")).unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    cockpit.pump();
    assert!(fake.suggest_calls().is_empty());

    cockpit.deliver_suggestion(thread, "Run the tests".into());
    cockpit.pump();
    assert_eq!(cockpit.thread(thread).unwrap().suggestion(), None);

    cockpit.set_suggestions_enabled(true);
    cockpit.send(thread, "again".into());
    fake.streams.borrow()[0].send(text("Done again.")).unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    cockpit.pump();
    assert_eq!(fake.suggest_calls().len(), 1, "re-enabling restores asks");
}

#[test]
fn disabling_suggestions_clears_an_existing_prediction() {
    let (mut cockpit, _fake) = cockpit("suggest-disable-clears");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.deliver_suggestion(thread, "Run the tests".into());
    cockpit.pump();
    assert_eq!(
        cockpit.thread(thread).unwrap().suggestion(),
        Some("Run the tests")
    );

    cockpit.set_suggestions_enabled(false);

    assert_eq!(cockpit.thread(thread).unwrap().suggestion(), None);
}

/// A prediction that comes back for a generation the Thread has moved
/// past is answering a conversation that no longer exists.
#[test]
fn a_stale_prediction_is_dropped() {
    let (mut cockpit, _fake) = cockpit("suggest-stale");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let generation = cockpit.engine().threads[&thread].generation;

    cockpit
        .engine()
        .suggestions
        .0
        .send(Suggestion {
            thread,
            generation: generation.wrapping_add(1),
            revision: cockpit.engine().threads[&thread].suggestion_revision,
            text: "Run the tests".into(),
        })
        .unwrap();
    cockpit.pump();
    assert_eq!(
        cockpit.thread(thread).unwrap().suggestion(),
        None,
        "a reply from a previous generation must not land"
    );

    cockpit
        .engine()
        .suggestions
        .0
        .send(Suggestion {
            thread,
            generation,
            revision: cockpit.engine().threads[&thread].suggestion_revision,
            text: "Run the tests".into(),
        })
        .unwrap();
    cockpit.pump();
    assert_eq!(
        cockpit.thread(thread).unwrap().suggestion(),
        Some("Run the tests")
    );
}

/// The next turn's ask clears the last turn's answer: ghost text that
/// describes a superseded response is worse than the generic line.
#[test]
fn a_new_turn_drops_the_previous_prediction() {
    let (mut cockpit, fake) = cockpit("suggest-supersede");
    let thread = cockpit.open(Provider::Codex, main_choice()).unwrap();
    cockpit.send(thread, "go".into());

    let generation = cockpit.engine().threads[&thread].generation;
    fake.streams.borrow()[0].send(text("Done.")).unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    cockpit.pump();
    cockpit
        .engine()
        .suggestions
        .0
        .send(Suggestion {
            thread,
            generation,
            revision: cockpit.engine().threads[&thread].suggestion_revision,
            text: "Run the tests".into(),
        })
        .unwrap();
    cockpit.pump();
    assert_eq!(
        cockpit.thread(thread).unwrap().suggestion(),
        Some("Run the tests")
    );

    fake.streams.borrow()[0].send(text("Ran them.")).unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    cockpit.pump();
    assert_eq!(
        cockpit.thread(thread).unwrap().suggestion(),
        None,
        "the new turn's ask must drop the old turn's answer"
    );
}

#[test]
fn older_turn_predictions_cannot_replace_the_latest_or_survive_disabling() {
    let (mut cockpit, fake) = cockpit("suggest-turn-revision");
    let thread = cockpit.open(Provider::Codex, main_choice()).unwrap();
    let finish = || {
        fake.streams.borrow()[0].send(text("Done.")).unwrap();
        fake.streams.borrow()[0]
            .send(SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
    };
    cockpit.send(thread, "first".into());
    finish();
    cockpit.pump();
    let first = fake.suggest_calls()[0].clone();
    cockpit.send(thread, "second".into());
    let old = Suggestion {
        thread,
        generation: first.generation,
        revision: first.revision,
        text: "Run the tests".into(),
    };
    // Sending invalidates an outstanding prediction immediately.
    cockpit.engine().suggestions.0.send(old.clone()).unwrap();
    cockpit.pump();
    assert_eq!(cockpit.thread(thread).unwrap().suggestion(), None);
    finish();
    cockpit.pump();
    cockpit.deliver_suggestion(thread, "Commit the fix".into());
    cockpit.pump();
    cockpit.engine().suggestions.0.send(old).unwrap();
    cockpit.pump();
    assert_eq!(
        cockpit.thread(thread).unwrap().suggestion(),
        Some("Commit the fix")
    );
    // Off then on must also discard replies from before the setting change.
    cockpit.deliver_suggestion(thread, "Late reply".into());
    cockpit.set_suggestions_enabled(false);
    cockpit.set_suggestions_enabled(true);
    cockpit.pump();
    assert_eq!(cockpit.thread(thread).unwrap().suggestion(), None);
}

#[test]
fn claude_uses_native_suggestions_without_a_second_model_request() {
    let (mut cockpit, fake) = cockpit("native-suggestion");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.send(thread, "show code".into());
    fake.streams.borrow()[0]
        .send(text("Here is the code."))
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    *fake.native_suggestion.borrow_mut() = Some("write the tests".into());
    let updates = cockpit.pump();
    assert_eq!(
        cockpit.thread(thread).unwrap().suggestion(),
        Some("write the tests")
    );
    assert!(updates.iter().any(|update| update.thread == thread));
    assert!(
        fake.suggest_calls().is_empty(),
        "Claude must not spawn a fallback model"
    );
    // An autonomous next turn also supersedes the old native hint,
    // even when Claude deliberately emits no new suggestion.
    fake.streams.borrow()[0]
        .send(text("Another result."))
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    cockpit.pump();
    assert_eq!(cockpit.thread(thread).unwrap().suggestion(), None);
    cockpit.set_suggestions_enabled(false);
    *fake.native_suggestion.borrow_mut() = Some("late reply".into());
    cockpit.pump();
    assert_eq!(cockpit.thread(thread).unwrap().suggestion(), None);
}

#[test]
fn each_thread_folds_only_its_own_events() {
    let (mut cockpit, fake) = cockpit("own-events");
    let one = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let two = cockpit.open(Provider::Claude, main_choice()).unwrap();

    fake.streams.borrow()[0].send(text("only for one")).unwrap();

    let updates = cockpit.pump();

    // Coalesced: a Thread that produced nothing is not in the frame.
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].thread, one);
    assert_eq!(updates[0].dirty.len(), 1);
    assert_eq!(cockpit.thread(one).unwrap().transcript().blocks().len(), 1);
    assert!(cockpit
        .thread(two)
        .unwrap()
        .transcript()
        .blocks()
        .is_empty());
}

#[test]
fn completion_observation_is_quiet_factual_and_unchanged_after_reopen() {
    let dir = scratch("completion-reference");
    let fake = Fake::default();
    let mut cockpit = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake.clone()));
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.send(thread, "work".into());
    fake.streams.borrow()[0].send(text("answer")).unwrap();
    cockpit.pump();
    let before = cockpit
        .thread(thread)
        .unwrap()
        .transcript()
        .turn_elapsed()
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: Some(0.038),
        })
        .unwrap();
    cockpit.pump();
    let completion = |cockpit: &Cockpit| {
        let rows: Vec<_> = cockpit
            .thread(thread)
            .unwrap()
            .transcript()
            .blocks()
            .iter()
            .filter_map(|block| match &block.body {
                Body::TurnEnd(end) if end.completed() => Some(end.text()),
                _ => None,
            })
            .collect();
        assert_eq!(rows.len(), 1, "one quiet historical completion row");
        rows[0].clone()
    };
    let first = completion(&cockpit);
    let fields: Vec<_> = first.split(" · ").collect();
    assert_eq!(
        fields.len(),
        2,
        "the observed elapsed and the local completion time"
    );
    let elapsed = fields[0]
        .strip_prefix("Worked for ")
        .and_then(|elapsed| elapsed.strip_suffix('s'))
        .expect("the stamp says how long the turn worked, not process runtime");
    // Whole seconds, floored, or `<1` under one second.
    let seconds: f64 = match elapsed.strip_prefix('<') {
        Some(under) => {
            assert_eq!(under, "1", "only a sub-second turn reads `<1s`");
            assert!(before.as_secs_f64() < 1.);
            0.
        }
        None => elapsed.parse().unwrap(),
    };
    assert!(seconds + 1. > before.as_secs_f64());
    assert!(fields[1].contains(':'), "human-readable completion time");
    assert!(
        !first.contains('$') && !first.contains("0.038"),
        "provider cost stays private"
    );
    fake.streams.borrow()[0].send(ended()).unwrap();
    cockpit.pump();
    assert_eq!(
        completion(&cockpit),
        first,
        "repeated end must not invent another completion"
    );
    cockpit.park(thread).unwrap();
    drop(cockpit);
    let mut reopened = Cockpit::new(Store::open(&dir).unwrap(), Box::new(Fake::default()));
    reopened.revive(thread).unwrap();
    assert_eq!(
        completion(&reopened),
        first,
        "replay uses the original observation, never now"
    );
    reopened.send(thread, "next turn".into());
    assert_eq!(
        completion(&reopened),
        first,
        "new work retains historical completion"
    );
}

#[test]
fn completion_observations_follow_only_accepted_live_turn_ends() {
    use crate::activity::{ActivityEvent, AgentKey, ExecutionEvent, Subject};
    let (mut cockpit, fake) = cockpit("completion-attribution-reference");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let key = AgentKey::new(Provider::Claude, "root", "child");
    let emit = |event| {
        fake.streams.borrow()[0]
            .send(SessionEvent::Activity(event))
            .unwrap()
    };
    let count = |cockpit: &Cockpit, subject: &Subject| {
        cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(subject)
            .unwrap()
            .transcript()
            .blocks()
            .iter()
            .filter(|block| matches!(&block.body, Body::TurnEnd(end) if end.completed()))
            .count()
    };
    emit(ActivityEvent::Content {
        key: key.clone(),
        id: Some("live-text".into()),
        event: ExecutionEvent::Text {
            text: "Working".into(),
        },
    });
    cockpit.pump();
    emit(ActivityEvent::HistoryContent {
        key: key.clone(),
        id: Some("historical-end".into()),
        event: ExecutionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        },
    });
    cockpit.pump();
    let subject = Subject::Subagent(key.clone());
    assert_eq!(
        count(&cockpit, &subject),
        0,
        "historical content has no newly observed completion time"
    );
    let end = ActivityEvent::Content {
        key: key.clone(),
        id: Some("live-end".into()),
        event: ExecutionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        },
    };
    emit(end.clone());
    cockpit.pump();
    assert_eq!(count(&cockpit, &subject), 1);
    emit(ActivityEvent::Content {
        key: key.clone(),
        id: Some("next-text".into()),
        event: ExecutionEvent::Text {
            text: "New work".into(),
        },
    });
    cockpit.pump();
    emit(end);
    cockpit.pump();
    assert_eq!(
        count(&cockpit, &subject),
        1,
        "a deduplicated old end cannot stamp the new turn"
    );
    fake.streams.borrow()[0]
        .send(text("Background main work"))
        .unwrap();
    cockpit.pump();
    emit(ActivityEvent::BackgroundTurnEnded {
        outcome: TurnOutcome::Completed,
        cost_usd: None,
    });
    cockpit.pump();
    assert_eq!(
        count(&cockpit, &Subject::Main),
        1,
        "observed autonomous work gets the same completion presentation"
    );
}

#[test]
fn child_completion_survives_a_history_read_already_in_flight() {
    use crate::activity::{ActivityEvent, ActivityInput, AgentKey, ExecutionEvent, Subject};
    let (mut cockpit, fake) = cockpit("completion-history-race");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let key = AgentKey::new(Provider::Claude, "root", "child");
    let subject = Subject::Subagent(key.clone());
    let emit = |event| {
        fake.streams.borrow()[0]
            .send(SessionEvent::Activity(event))
            .unwrap()
    };
    emit(ActivityEvent::Content {
        key: key.clone(),
        id: Some("start".into()),
        event: ExecutionEvent::Text {
            text: "Working".into(),
        },
    });
    cockpit.pump();
    // Arrange the normal evicted-child state, and gate the disk boundary.
    cockpit
        .engine_mut()
        .threads
        .get_mut(&thread)
        .unwrap()
        .activity
        .apply(ActivityInput::Evict(subject.clone()));
    let (loader, finish) = history::gated_loader();
    cockpit.engine_mut().history_loader = Some(loader);
    assert!(cockpit.ensure_subject_history(thread, &subject).unwrap());
    emit(ActivityEvent::Content {
        key,
        id: Some("end".into()),
        event: ExecutionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        },
    });
    cockpit.pump();
    finish(&cockpit.engine().store);
    cockpit.pump();
    let view = cockpit.thread(thread).unwrap().activity();
    let child = view.subject(&subject).unwrap();
    assert!(child.retained());
    assert_eq!(
        child
            .transcript()
            .blocks()
            .iter()
            .filter(|block| matches!(&block.body, Body::TurnEnd(end) if end.completed()))
            .count(),
        1,
        "a prefix reload must merge the completion observed after its checkpoint"
    );
    let completion = |cockpit: &Cockpit| {
        cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&subject)
            .unwrap()
            .transcript()
            .blocks()
            .iter()
            .filter_map(|block| match &block.body {
                Body::TurnEnd(end) if end.completed() => Some(end.text()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let observed = completion(&cockpit);
    // A later ordinary reload must use the same persisted observation,
    // after the in-flight buffer that supplied it above has been consumed.
    cockpit
        .engine_mut()
        .threads
        .get_mut(&thread)
        .unwrap()
        .activity
        .apply(ActivityInput::Evict(subject.clone()));
    let (loader, finish) = history::gated_loader();
    cockpit.engine_mut().history_loader = Some(loader);
    assert!(cockpit.ensure_subject_history(thread, &subject).unwrap());
    finish(&cockpit.engine().store);
    cockpit.pump();
    assert_eq!(
        completion(&cockpit),
        observed,
        "ordinary disk reload must preserve the exact child completion observation"
    );
}

#[test]
fn child_tool_duration_survives_a_history_read_already_in_flight() {
    use crate::activity::{ActivityEvent, ActivityInput, AgentKey, ExecutionEvent, Subject};
    for native_duration in [None, Some(42)] {
        let (mut cockpit, fake) = cockpit("tool-duration-history-race");
        let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
        let key = AgentKey::new(Provider::Claude, "root", "child");
        let subject = Subject::Subagent(key.clone());
        let emit = |id: &str, event| {
            fake.streams.borrow()[0]
                .send(SessionEvent::Activity(ActivityEvent::Content {
                    key: key.clone(),
                    id: Some(id.into()),
                    event,
                }))
                .unwrap()
        };
        let completed = |duration_ms| ExecutionEvent::ToolCompleted {
            id: "tool".into(),
            output: String::new(),
            is_error: false,
            result: crate::ToolResult::Command {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: Some(0),
                duration_ms,
            },
        };
        emit(
            "start",
            ExecutionEvent::ToolStarted {
                id: "tool".into(),
                name: "Bash".into(),
                input: serde_json::json!({}),
            },
        );
        if native_duration.is_some() {
            // A newer native completion must also outrank a duration
            // already present in the frozen disk prefix.
            emit("old-completion", completed(Some(7)));
        }
        cockpit.pump();
        cockpit
            .engine_mut()
            .threads
            .get_mut(&thread)
            .unwrap()
            .activity
            .apply(ActivityInput::Evict(subject.clone()));
        let (loader, finish) = history::gated_loader();
        cockpit.engine_mut().history_loader = Some(loader);
        assert!(cockpit.ensure_subject_history(thread, &subject).unwrap());
        emit("completion", completed(native_duration));
        cockpit.pump();
        assert!(cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&subject)
            .unwrap()
            .timings()
            .is_empty());
        finish(&cockpit.engine().store);
        cockpit.pump();
        let timing = |cockpit: &Cockpit| match cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&subject)
            .unwrap()
            .timings()
            .get("tool")
        {
            Some(ToolTiming::Done(duration)) => *duration,
            timing => {
                panic!("the completed duration must survive the history read: {timing:?}")
            }
        };
        let observed = timing(&cockpit);
        if let Some(ms) = native_duration {
            assert_eq!(observed, Duration::from_millis(ms));
        }
        // Reload from disk without the live-tail buffer: both paths must
        // agree, allowing the log's millisecond precision.
        cockpit
            .engine_mut()
            .threads
            .get_mut(&thread)
            .unwrap()
            .activity
            .apply(ActivityInput::Evict(subject.clone()));
        let (loader, finish) = history::gated_loader();
        cockpit.engine_mut().history_loader = Some(loader);
        assert!(cockpit.ensure_subject_history(thread, &subject).unwrap());
        finish(&cockpit.engine().store);
        cockpit.pump();
        assert_eq!(timing(&cockpit).as_millis(), observed.as_millis());
    }
}

#[test]
fn tool_calls_are_clocked_at_ingestion() {
    let (mut cockpit, fake) = cockpit("timings");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();

    fake.streams.borrow()[0]
        .send(SessionEvent::ToolStarted {
            id: "t1".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "cargo check" }),
        })
        .unwrap();
    cockpit.pump();
    let running = cockpit
        .thread(thread)
        .map(|open| open.tool_timings())
        .unwrap()["t1"];
    assert!(matches!(running, ToolTiming::Running(_)));
    let first = running.elapsed();
    assert!(
        running.elapsed() >= first,
        "a running call's clock never runs backwards"
    );

    fake.streams.borrow()[0]
        .send(SessionEvent::ToolCompleted {
            id: "t1".into(),
            output: String::new(),
            is_error: false,
            result: crate::ToolResult::Opaque,
        })
        .unwrap();
    cockpit.pump();
    let done = cockpit
        .thread(thread)
        .map(|open| open.tool_timings())
        .unwrap()["t1"];
    let ToolTiming::Done(total) = done else {
        panic!("a settled call fixes its total: {done:?}");
    };
    assert!(
        total >= first,
        "the total covers at least what had already elapsed"
    );
    assert_eq!(done.elapsed(), done.elapsed(), "a settled call is fixed");

    // A completion the cockpit never saw start has no clock to read.
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolCompleted {
            id: "ghost".into(),
            output: String::new(),
            is_error: false,
            result: crate::ToolResult::Opaque,
        })
        .unwrap();
    cockpit.pump();
    assert!(!cockpit
        .thread(thread)
        .map(|open| open.tool_timings())
        .unwrap()
        .contains_key("ghost"));
}

fn decision(id: &str, tool: &str) -> SessionEvent {
    SessionEvent::DecisionRequested {
        decision: Decision {
            delivery: Default::default(),
            kind: Default::default(),
            policy: Default::default(),
            id: id.into(),
            tool_use_id: format!("toolu_{id}"),
            tool_name: tool.into(),
            description: "ferrite-perm.txt".into(),
            input: serde_json::json!({ "file_path": "ferrite-perm.txt" }),
            suggestions: vec![],
        },
    }
}

fn ended() -> SessionEvent {
    SessionEvent::TurnEnded {
        outcome: crate::TurnOutcome::Completed,
        cost_usd: None,
    }
}

/// Reports whatever the test tells it to.
struct Meter(Rc<RefCell<u64>>);

impl RssSampler for Meter {
    fn sample(&mut self, _thread: ThreadId, _pid: Option<u32>) -> Option<u64> {
        Some(*self.0.borrow())
    }
}

#[test]
fn a_sent_prompt_reaches_the_provider_the_transcript_and_the_log() {
    let (mut cockpit, fake) = cockpit("send");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();

    cockpit.send(thread, "run the tests".into());

    assert_eq!(fake.sent.borrow().as_slice(), ["run the tests"]);
    let echoed = cockpit
        .thread(thread)
        .unwrap()
        .transcript()
        .blocks()
        .last()
        .cloned();
    assert!(matches!(echoed.map(|b| b.body), Some(Body::Prompt(line)) if line == "run the tests"));
    // And durably: a restart must not lose what the operator asked for.
    cockpit.park(thread).unwrap();
    cockpit.revive(thread).unwrap();
    assert!(cockpit
        .thread(thread)
        .map(|open| open.transcript())
        .unwrap()
        .blocks()
        .iter()
        .any(|block| matches!(&block.body, Body::Prompt(line) if line == "run the tests")));
}

#[test]
fn prompt_history_is_per_thread_and_rebuilt_from_the_log() {
    let (mut cockpit, _) = cockpit("prompt-history-core");
    let first = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let second = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.send(first, "one".into());
    cockpit.send(first, "two".into());
    cockpit.send(second, "other".into());

    assert_eq!(
        cockpit.recall_prompt(first, HistoryDirection::Older, "draft"),
        Some("two".into())
    );
    assert_eq!(
        cockpit.recall_prompt(second, HistoryDirection::Older, "second draft"),
        Some("other".into())
    );

    cockpit.park(first).unwrap();
    cockpit.revive(first).unwrap();
    assert_eq!(
        cockpit.recall_prompt(first, HistoryDirection::Older, "revived draft"),
        Some("two".into())
    );
    assert_eq!(
        cockpit.recall_prompt(first, HistoryDirection::Older, "ignored edit"),
        Some("one".into())
    );
}

#[test]
fn native_queue_history_records_consumption_once() {
    let (mut cockpit, fake) = cockpit("native-history");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.send(thread, "sent".into());
    assert!(cockpit.queue(thread, "held".into()));
    assert_eq!(
        cockpit.thread(thread).unwrap().queued(),
        None,
        "pipe write is not admission"
    );
    cockpit.pump();
    assert_eq!(cockpit.thread(thread).unwrap().queued(), Some("held"));
    fake.streams.borrow()[0].send(ended()).unwrap();
    cockpit.pump();
    assert_eq!(
        fake.sent.borrow().as_slice(),
        ["sent"],
        "turn end never dispatches"
    );
    assert_eq!(
        cockpit.recall_prompt(thread, HistoryDirection::Older, "draft"),
        Some("sent".into())
    );
    let id = fake.native.borrow()[0].0.clone();
    for _ in 0..2 {
        fake.streams.borrow()[0]
            .send(SessionEvent::Queue(crate::QueueEvent::Started {
                historical: false,
                client_id: id.clone(),
                text: None,
            }))
            .unwrap();
    }
    cockpit.pump();
    assert_eq!(cockpit.thread(thread).unwrap().queued(), None);
    assert_eq!(
        cockpit.recall_prompt(thread, HistoryDirection::Older, "draft"),
        Some("held".into())
    );
    let prompts = cockpit
        .thread(thread)
        .unwrap()
        .transcript()
        .blocks()
        .iter()
        .filter(|block| matches!(&block.body, Body::Prompt(text) if text == "held"))
        .count();
    assert_eq!(prompts, 1);
    cockpit.park(thread).unwrap();
    assert_eq!(
        cockpit.engine().store.load(thread).unwrap().prompt_texts(),
        ["sent", "held"]
    );
}

#[test]
fn a_new_cockpit_recovers_prompt_history_from_the_store() {
    let dir = scratch("prompt-history-relaunch");
    let fake = Fake::default();
    let mut cockpit = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake.clone()));
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.send(thread, "before relaunch".into());
    cockpit.park(thread).unwrap();
    drop(cockpit);

    let mut relaunched = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake));
    relaunched.revive(thread).unwrap();
    assert_eq!(
        relaunched.recall_prompt(thread, HistoryDirection::Older, "draft"),
        Some("before relaunch".into())
    );
}

#[test]
fn bootstrap_delivery_is_immediately_recallable() {
    let (mut cockpit, _) = cockpit("prompt-history-bootstrap");
    let thread = cockpit
        .bootstrap(
            ProviderChoice {
                provider: Provider::Claude,
                model: None,
            },
            main_choice(),
            "first prompt",
        )
        .unwrap();

    assert_eq!(
        cockpit.recall_prompt(thread, HistoryDirection::Older, "draft"),
        Some("first prompt".into())
    );
}

/// The exact wire preface for one binding + root pair (#24) — what the
/// provider sees and nothing the operator ever does.
fn preface(binding: &Path, root: &Path) -> String {
    format!(
        "<ferrite-session-context>\n\
         Agent workspace root: {}\n\
         Session project root: {}\n\
         Run edits, commands, tests, builds, and Git operations from the \
         session project root. Use the agent workspace root only for \
         broader workspace context.\n\
         </ferrite-session-context>\n",
        binding.display(),
        root.display()
    )
}

/// A directory that exists, for tests that pick it as the root.
fn existing_root(name: &str) -> std::path::PathBuf {
    let root = scratch(name);
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// AC (#24): the first prompt after a Session start goes to the provider
/// wearing the hidden session-context block; the second carries nothing
/// extra. The transcript and the log keep the operator's raw text only —
/// displayed ≠ sent.
#[test]
fn the_first_prompt_of_a_session_carries_the_hidden_context_and_the_second_does_not() {
    let dir = scratch("preface-store");
    let fake = Fake::default();
    let mut cockpit = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake.clone()));
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    assert_eq!(
        cockpit
            .thread(thread)
            .and_then(|open| open.session_project_root()),
        None
    );
    let root = existing_root("preface-root");
    cockpit
        .set_session_project_root(thread, Some(root.clone()))
        .unwrap();
    assert_eq!(
        cockpit
            .thread(thread)
            .and_then(|open| open.session_project_root()),
        Some(root.as_path())
    );

    cockpit.send(thread, "run the tests".into());
    cockpit.send(thread, "and the lints".into());

    let binding = std::env::temp_dir(); // main_choice()'s checkout
    assert_eq!(
        fake.sent.borrow().as_slice(),
        [
            format!("{}run the tests", preface(&binding, &root)),
            "and the lints".to_string(),
        ]
    );
    // Displayed ≠ sent: the Pane echoes the raw prompt...
    let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
    assert!(blocks
        .iter()
        .any(|b| matches!(&b.body, Body::Prompt(line) if line == "run the tests")));
    assert!(
        !format!("{blocks:?}").contains("ferrite-session-context"),
        "the preface must never reach the transcript"
    );
    // ...and the log stores it raw: the preface exists only on the wire.
    cockpit.park(thread).unwrap();
    let inputs = Store::open(&dir).unwrap().load(thread).unwrap().inputs();
    assert!(inputs.contains(&Input::Prompt("run the tests".into())));
    assert!(
        !format!("{inputs:?}").contains("ferrite-session-context"),
        "the preface must never reach the log"
    );
}

/// AC (#24): changing the selection on a live Thread ends its Session —
/// no mid-session mutation of the root — and the next prompt respawns
/// through the resume path with the preface re-armed, naming the new
/// root. The respawn's cwd is the chain's head (#29): the picked root.
#[test]
fn changing_the_root_ends_the_session_and_the_next_prompt_respawns_with_the_preface() {
    let (mut cockpit, fake) = cockpit("root-change");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let first_root = existing_root("root-change-first");
    let second_root = existing_root("root-change-second");
    cockpit
        .set_session_project_root(thread, Some(first_root.clone()))
        .unwrap();
    cockpit.send(thread, "one".into());
    // The provider names the session; a later respawn must resume it.
    fake.streams
        .borrow()
        .last()
        .unwrap()
        .send(SessionEvent::Init {
            session_id: "sess-1".into(),
            model: "claude-haiku-4-5".into(),
        })
        .unwrap();
    cockpit.pump();
    cockpit.send(thread, "two".into());
    let spawns = fake.streams.borrow().len();

    cockpit
        .set_session_project_root(thread, Some(second_root.clone()))
        .unwrap();
    // The Session ended; nothing respawns until the operator speaks.
    assert_eq!(fake.streams.borrow().len(), spawns);
    cockpit.send(thread, "three".into());

    assert_eq!(fake.streams.borrow().len(), spawns + 1);
    assert_eq!(
        fake.resumed.borrow().last().unwrap().as_deref(),
        Some("sess-1"),
        "the respawn goes through the resume path"
    );
    let binding = std::env::temp_dir();
    assert_eq!(
        fake.cwds.borrow().last().unwrap().as_deref(),
        Some(second_root.as_path()),
        "the respawn's cwd is the chain's head (#29): the picked root"
    );
    assert_eq!(
        fake.sent.borrow().as_slice(),
        [
            format!("{}one", preface(&binding, &first_root)),
            "two".to_string(),
            format!("{}three", preface(&binding, &second_root)),
        ]
    );
}

/// AC (#24): the root survives restart, and a revive is a Session start
/// like any other — its first prompt carries the preface again.
#[test]
fn a_revived_thread_keeps_its_root_and_prefaces_its_first_prompt_again() {
    let (mut cockpit, fake) = cockpit("root-revive");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let root = existing_root("root-revive-root");
    cockpit
        .set_session_project_root(thread, Some(root.clone()))
        .unwrap();
    cockpit.send(thread, "before the park".into());

    cockpit.park(thread).unwrap();
    cockpit.revive(thread).unwrap();

    assert_eq!(
        cockpit
            .thread(thread)
            .and_then(|open| open.session_project_root()),
        Some(root.as_path())
    );
    cockpit.send(thread, "after the revive".into());
    let binding = std::env::temp_dir();
    assert_eq!(
        fake.sent.borrow().last().unwrap(),
        &format!("{}after the revive", preface(&binding, &root))
    );
}

/// AC (#24): a watchdog respawn is a Session start too — the
/// replacement's first prompt carries the preface again.
#[test]
fn a_sweep_respawned_session_prefaces_its_next_prompt() {
    let (mut cockpit, fake) = cockpit("root-sweep");
    let rss = Rc::new(RefCell::new(4 * 1024 * 1024 * 1024));
    cockpit.watch_memory(Box::new(Meter(rss)), 1024 * 1024 * 1024);
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let root = existing_root("root-sweep-root");
    cockpit
        .set_session_project_root(thread, Some(root.clone()))
        .unwrap();
    cockpit.send(thread, "one".into()); // consumes the first preface

    assert_eq!(cockpit.sweep().len(), 1, "the leak replaces the Session");
    cockpit.send(thread, "two".into());

    let binding = std::env::temp_dir();
    assert_eq!(
        fake.sent.borrow().last().unwrap(),
        &format!("{}two", preface(&binding, &root))
    );
}

/// AC (#24): a root gone from disk refuses the send, naming the path and
/// the remedy — never a silent fallback to working in the binding.
/// Nothing reaches the wire, the transcript's history, or the log.
#[test]
fn a_send_with_a_vanished_root_is_refused_naming_the_path_and_the_remedy() {
    let (mut cockpit, fake) = cockpit("root-vanished");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let root = existing_root("root-vanished-root");
    cockpit
        .set_session_project_root(thread, Some(root.clone()))
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();

    cockpit.send(thread, "into the void".into());

    assert!(
        fake.sent.borrow().is_empty(),
        "nothing may reach the provider"
    );
    let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
    let last = blocks.last().unwrap();
    let Body::Notice(line) = &last.body else {
        panic!("the refusal must be a readable notice: {:?}", last.body);
    };
    assert!(
        line.contains(&root.display().to_string()),
        "the path is named: {line}"
    );
    assert!(line.contains("re-pick"), "the remedy is named: {line}");
    assert!(
        !blocks.iter().any(|b| matches!(&b.body, Body::Prompt(_))),
        "a refused prompt is not history"
    );
}

#[test]
fn replacing_a_claude_session_does_not_resubmit_native_pending_work() {
    let (mut cockpit, fake) = cockpit("native-replacement");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    assert!(cockpit.queue(thread, "pending text".into()));
    cockpit.pump();
    cockpit
        .set_session_project_root(thread, Some(existing_root("native-replacement-root")))
        .unwrap();
    cockpit.send(thread, "new input".into());
    fake.streams.borrow()[0]
        .send(SessionEvent::Queue(crate::QueueEvent::Started {
            historical: false,
            client_id: fake.native.borrow()[0].0.clone(),
            text: None,
        }))
        .ok();
    cockpit.pump();
    assert_eq!(fake.native.borrow().len(), 1);
    assert!(!fake
        .sent
        .borrow()
        .iter()
        .any(|text| text.ends_with("pending text")));
    assert!(cockpit.thread(thread).unwrap().transcript().blocks().iter().any(|block| matches!(&block.body, Body::Notice(text) if text.contains("delivery is uncertain"))));
}

/// After a failed watchdog restart the Thread sits in the cockpit with
/// no Session. The next prompt makes exactly one spawn attempt; a
/// failure is one readable Notice on the send-failure surface — not a
/// loop, and not a prompt in the history that never went anywhere.
#[test]
fn a_send_after_a_failed_restart_attempts_one_spawn_and_reports_the_failure() {
    let (mut cockpit, fake) = cockpit("respawn-fails");
    let rss = Rc::new(RefCell::new(u64::MAX));
    cockpit.watch_memory(Box::new(Meter(rss)), 1024);
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    *fake.fail.borrow_mut() = true;
    assert_eq!(cockpit.sweep().len(), 1, "the restart fails; no Session");
    let attempts = *fake.attempts.borrow();

    cockpit.send(thread, "hello".into());

    assert_eq!(
        *fake.attempts.borrow(),
        attempts + 1,
        "exactly one spawn attempt per send"
    );
    assert!(fake.sent.borrow().is_empty());
    let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
    let Body::Notice(line) = &blocks.last().unwrap().body else {
        panic!("the failure must be a Notice: {:?}", blocks.last());
    };
    assert!(line.starts_with("send failed:"), "{line}");
    assert!(
        !blocks.iter().any(|b| matches!(&b.body, Body::Prompt(_))),
        "an unsent prompt is not history"
    );
}

#[test]
fn answering_through_the_cockpit_clears_the_card_and_records_who_did_it() {
    let (mut cockpit, fake) = cockpit("respond");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    fake.streams.borrow()[0]
        .send(decision("perm_01", "Write"))
        .unwrap();
    cockpit.pump();
    let pending = cockpit
        .thread(thread)
        .and_then(|open| open.pending())
        .cloned()
        .unwrap();

    cockpit.respond(
        thread,
        &pending,
        crate::DecisionAnswer::Allow {
            input: pending.input.clone(),
        },
    );

    assert_eq!(cockpit.thread(thread).and_then(|open| open.pending()), None);
    let last = cockpit
        .thread(thread)
        .unwrap()
        .transcript()
        .blocks()
        .last()
        .cloned();
    assert!(matches!(last.map(|b| b.body), Some(Body::Meta(line)) if line == "allowed Write"));

    // A second press answers nothing twice.
    cockpit.respond(
        thread,
        &pending,
        crate::DecisionAnswer::Allow {
            input: pending.input.clone(),
        },
    );
    let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
    assert_eq!(
        blocks
            .iter()
            .filter(|b| matches!(&b.body, Body::Meta(line) if line == "allowed Write"))
            .count(),
        1
    );
}

/// ADR 0025: an agent waiting on a Decision may wait for hours, so the
/// log holds everything before the Decision while it waits — not only
/// after the turn ends — and the answer is flushed as it is given.
#[test]
fn the_log_holds_everything_before_a_pending_decision() {
    let dir = scratch("decision-flush");
    let store = Store::with_flush_interval(&dir, Duration::from_secs(3600)).unwrap();
    let fake = Fake::default();
    let mut cockpit = Cockpit::new(store, Box::new(fake.clone()));
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.send(thread, "write the file".into());
    let log = dir.join(thread.to_string()).join("log.jsonl");
    let on_disk = || std::fs::read_to_string(&log).unwrap();

    fake.streams.borrow()[0]
        .send(text("before the question"))
        .unwrap();
    cockpit.pump();
    assert!(
        !on_disk().contains("before the question"),
        "mid-turn text waits for a boundary"
    );

    fake.streams.borrow()[0]
        .send(decision("perm_01", "Write"))
        .unwrap();
    cockpit.pump();
    assert!(on_disk().contains("before the question"));

    fake.streams.borrow()[0]
        .send(text("while it waits"))
        .unwrap();
    cockpit.pump();
    let pending = cockpit
        .thread(thread)
        .and_then(|open| open.pending())
        .cloned()
        .unwrap();
    cockpit.respond(
        thread,
        &pending,
        crate::DecisionAnswer::Allow {
            input: pending.input.clone(),
        },
    );
    assert!(on_disk().contains("while it waits"));
}

#[test]
fn a_code_fence_comes_back_highlighted_without_the_app_routing_anything() {
    let (mut cockpit, fake) = cockpit("highlight");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();

    fake.streams.borrow()[0]
        .send(text("```rust\nfn main() {}\n```\n\n"))
        .unwrap();
    cockpit.pump();
    // The lexer answers on its own channel; the next frame picks it up.
    let update = cockpit.pump();

    let code = cockpit
        .thread(thread)
        .map(|open| open.transcript())
        .unwrap()
        .blocks()
        .iter()
        .find(|block| matches!(block.body, Body::Code { .. }))
        .cloned()
        .expect("a code block");
    assert!(update.iter().any(|pane| pane.dirty.contains(&code.id)));
    let Body::Code { tokens, .. } = &code.body else {
        unreachable!()
    };
    assert!(
        tokens
            .as_deref()
            .is_some_and(|tokens| tokens.iter().any(|t| t.class == Class::Keyword)),
        "the cockpit owns highlighting: {tokens:?}"
    );
}

#[test]
fn jumping_to_the_next_decision_wraps_around_the_cockpit() {
    let (mut cockpit, fake) = cockpit("next-decision");
    let one = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let two = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let three = cockpit.open(Provider::Claude, main_choice()).unwrap();
    assert_eq!(cockpit.next_blocked(None), None);

    fake.streams.borrow()[0]
        .send(decision("perm_01", "Write"))
        .unwrap();
    fake.streams.borrow()[2]
        .send(decision("perm_03", "Bash"))
        .unwrap();
    cockpit.pump();

    // From nowhere, the first blocked Thread; from one, the next one on.
    assert_eq!(cockpit.next_blocked(None), Some(one));
    assert_eq!(cockpit.next_blocked(Some(one)), Some(three));
    // Past the last, back to the first — the operator keeps pressing one
    // key until the cockpit is quiet.
    assert_eq!(cockpit.next_blocked(Some(three)), Some(one));
    // A Thread with nothing pending is still a valid place to jump
    // from: it lands on the head of the queue, the answer target.
    assert_eq!(cockpit.next_blocked(Some(two)), Some(one));
    assert_eq!(cockpit.answer_target(), Some(one));
}

/// The answer order is the order requests arrived, not ThreadId order:
/// the strip's first row is the answer target, and ⌘D walks the strip.
#[test]
fn the_answer_target_is_the_oldest_request_not_the_lowest_thread() {
    let (mut cockpit, fake) = cockpit("answer-order");
    let one = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let two = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let three = cockpit.open(Provider::Claude, main_choice()).unwrap();
    assert_eq!(cockpit.answer_target(), None);
    assert!(cockpit.needs_you().is_empty());

    fake.streams.borrow()[2]
        .send(decision("perm_03", "Bash"))
        .unwrap();
    cockpit.pump();
    fake.streams.borrow()[0]
        .send(decision("perm_01", "Write"))
        .unwrap();
    cockpit.pump();

    assert_eq!(cockpit.needs_you(), vec![three, one]);
    assert_eq!(cockpit.answer_target(), Some(three));
    assert_eq!(cockpit.next_blocked(None), Some(three));
    assert_eq!(cockpit.next_blocked(Some(three)), Some(one));
    assert_eq!(cockpit.next_blocked(Some(one)), Some(three));
    assert_eq!(cockpit.next_blocked(Some(two)), Some(three));
}

#[test]
fn a_leaking_session_is_restarted_where_the_operator_can_see_it() {
    let (mut cockpit, fake) = cockpit("watchdog");
    let rss = Rc::new(RefCell::new(200 * 1024 * 1024));
    cockpit.watch_memory(Box::new(Meter(rss.clone())), 1024 * 1024 * 1024);
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::Init {
            session_id: "sess-1".into(),
            model: "claude-haiku-4-5".into(),
        })
        .unwrap();
    fake.streams.borrow()[0].send(text("work so far")).unwrap();
    cockpit.pump();

    // Comfortably under: nothing happens.
    assert!(cockpit.sweep().is_empty());

    *rss.borrow_mut() = 4 * 1024 * 1024 * 1024;
    let restarted = cockpit.sweep();

    assert_eq!(restarted.len(), 1);
    assert_eq!(restarted[0].thread, thread);
    assert_eq!(restarted[0].rss, 4 * 1024 * 1024 * 1024);
    // A second Session, told where to pick up — and where to work: the
    // replacement spawns into the same binding the first Session had.
    assert_eq!(fake.resumed.borrow().len(), 2);
    assert_eq!(
        fake.resumed.borrow().last().unwrap().as_deref(),
        Some("sess-1")
    );
    let cwds = fake.cwds.borrow();
    assert_eq!(cwds.last().unwrap(), &cwds[0]);

    let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
    let last = blocks.last().unwrap();
    assert!(
        matches!(&last.body, Body::Notice(line) if line.contains("restarted")),
        "the Pane must say it happened: {:?}",
        last.body
    );
    // The scrollback is not collateral damage.
    assert!(blocks.iter().any(|block| matches!(
        &block.body,
        Body::Paragraph { spans } if spans.iter().any(|s| s.text == "work so far")
    )));
}

#[test]
fn a_restart_finds_the_threads_it_left_behind() {
    let dir = scratch("restart");
    let fake = Fake::default();
    let mut cockpit = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake.clone()));
    let first = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let second = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.park(first).unwrap();
    cockpit.park(second).unwrap();
    drop(cockpit);

    // A new run of Ferrite over the same store.
    let mut restarted = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake));

    assert_eq!(restarted.parked().unwrap(), vec![first, second]);
    restarted.revive(second).unwrap();
    assert_eq!(restarted.threads(), vec![second]);
    // What is open is no longer parked.
    assert_eq!(restarted.parked().unwrap(), vec![first]);
}

#[test]
fn a_parked_thread_revives_with_its_history_and_says_so() {
    let (mut cockpit, fake) = cockpit("revive");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::Init {
            session_id: "sess-1".into(),
            model: "claude-haiku-4-5".into(),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(text("an answer from before"))
        .unwrap();
    fake.streams.borrow()[0].send(ended()).unwrap();
    cockpit.pump();

    cockpit.park(thread).unwrap();
    // A parked Thread costs memory nothing: no Session, no transcript.
    assert!(cockpit.thread(thread).is_none());
    assert!(cockpit.threads().is_empty());

    cockpit.revive(thread).unwrap();

    let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
    let text_of = |body: &Body| match body {
        Body::Paragraph { spans } => spans.iter().map(|s| s.text.clone()).collect::<String>(),
        Body::Meta(line) => line.clone(),
        _ => String::new(),
    };
    assert!(
        blocks
            .iter()
            .any(|b| text_of(&b.body) == "an answer from before"),
        "history should come back: {blocks:?}"
    );
    // Honest about what it is: a new Session wearing an old Thread.
    assert_eq!(
        text_of(&blocks.last().unwrap().body),
        "revived — new Session, history from the log"
    );
    // And the provider is told where to pick up.
    assert_eq!(
        fake.resumed.borrow().last().unwrap().as_deref(),
        Some("sess-1")
    );
}

/// #11: the running cockpit's own import door. The session file is
/// adopted into this cockpit's store — durable and parked when the call
/// returns — and `revive` opens it like any parked Thread, history
/// replayed and the new Session told where to resume.
#[test]
fn a_session_file_imports_through_the_cockpits_own_door() {
    let (mut cockpit, fake) = cockpit("import-door");
    let dir = scratch("import-door-file");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("adopted.jsonl");
    std::fs::write(
        &path,
        concat!(
            r#"{"type":"user","sessionId":"door-9c1d","cwd":"/workspace","message":{"role":"user","content":"first question"}}"#,
            "\n",
            r#"{"type":"assistant","sessionId":"door-9c1d","message":{"model":"claude-haiku-4-5","content":[{"type":"text","text":"first answer"}]}}"#,
            "\n",
        ),
    )
    .unwrap();

    let thread = cockpit.import(&path).unwrap();

    assert!(
        cockpit.parked().unwrap().contains(&thread),
        "the imported Thread is durable and parked, ready to revive"
    );
    cockpit.revive(thread).unwrap();
    assert_eq!(
        cockpit.recall_prompt(thread, HistoryDirection::Older, "draft"),
        Some("first question".into()),
        "imported prompt records participate in recall"
    );
    let transcript = cockpit.thread(thread).unwrap().transcript();
    assert_eq!(transcript.session_id(), Some("door-9c1d"));
    assert!(
        transcript
            .blocks()
            .iter()
            .any(|block| matches!(&block.body, Body::Prompt(text) if text == "first question")),
        "the conversation replays: {:?}",
        transcript.blocks()
    );
    // The whole point of adoption: the new Session resumes the file's
    // own session id.
    assert_eq!(
        fake.resumed.borrow().last().unwrap().as_deref(),
        Some("door-9c1d")
    );
}

/// A file that is not a session file is refused by the import module —
/// through this door, with the same readable error and no Thread.
#[test]
fn the_import_door_passes_a_refusal_through_unchanged() {
    let (mut cockpit, _fake) = cockpit("import-door-refused");
    let dir = scratch("import-door-refused-file");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("junk.jsonl");
    std::fs::write(&path, "not a session file\n").unwrap();

    let refused = cockpit.import(&path);
    assert!(
        matches!(
            refused,
            Err(crate::import::ImportError::Unrecognized { .. })
        ),
        "got {refused:?}"
    );
    assert!(cockpit.parked().unwrap().is_empty(), "no Thread was left");
}

#[test]
fn a_decision_is_pending_against_the_thread_that_raised_it() {
    let (mut cockpit, fake) = cockpit("pending");
    let one = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let two = cockpit.open(Provider::Claude, main_choice()).unwrap();

    fake.streams.borrow()[0]
        .send(decision("perm_01", "Write"))
        .unwrap();
    cockpit.pump();

    assert_eq!(
        cockpit
            .thread(one)
            .and_then(|open| open.pending())
            .unwrap()
            .tool_name,
        "Write"
    );
    assert_eq!(cockpit.thread(two).and_then(|open| open.pending()), None);
    assert_eq!(cockpit.blocked(), vec![one]);
}

#[test]
fn answering_a_decision_that_is_no_longer_pending_is_refused_not_forwarded() {
    let (mut cockpit, fake) = cockpit("stale");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    fake.streams.borrow()[0]
        .send(decision("perm_01", "Write"))
        .unwrap();
    cockpit.pump();

    assert!(cockpit.answer(thread, "perm_01"));
    assert_eq!(cockpit.thread(thread).and_then(|open| open.pending()), None);

    // A second keystroke on a card already answered, and an answer to a
    // request this Thread never had: neither may reach the provider.
    assert!(!cockpit.answer(thread, "perm_01"));
    assert!(!cockpit.answer(thread, "perm_99"));
}

#[test]
fn a_turn_that_ends_takes_its_unanswered_decision_with_it() {
    let (mut cockpit, fake) = cockpit("moot");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    fake.streams.borrow()[0]
        .send(decision("perm_01", "Write"))
        .unwrap();
    cockpit.pump();

    // The turn was interrupted, or the provider gave up waiting: the
    // request is gone, so the card must not linger over the Composer.
    fake.streams.borrow()[0].send(ended()).unwrap();
    cockpit.pump();

    assert_eq!(cockpit.thread(thread).and_then(|open| open.pending()), None);
    assert!(cockpit.blocked().is_empty());
    assert!(!cockpit.answer(thread, "perm_01"));
}

#[test]
fn native_pending_prompts_survive_turn_end_without_client_dispatch() {
    let (mut cockpit, fake) = cockpit("native-queued");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    fake.streams.borrow()[0].send(text("working")).unwrap();
    cockpit.pump();
    assert!(cockpit.queue(thread, "and then run the tests".into()));
    cockpit.pump();
    fake.streams.borrow()[0].send(ended()).unwrap();
    cockpit.pump();
    assert!(fake.sent.borrow().is_empty());
    assert_eq!(fake.native.borrow().len(), 1);
    assert_eq!(
        cockpit.thread(thread).unwrap().queued(),
        Some("and then run the tests")
    );
}

/// Two prompts held at once both show, the latest on top; Escape on a
/// Codex turn stops it and sends everything held as one prompt, oldest
/// first — a late start notice for a resent id inserts nothing twice.
#[test]
fn escape_on_codex_resends_the_whole_pile_as_one_prompt() {
    let (mut cockpit, fake) = cockpit("codex-escape-resend");
    let thread = cockpit.open(Provider::Codex, main_choice()).unwrap();
    cockpit.send(thread, "start".into());
    fake.streams.borrow()[0].send(text("working")).unwrap();
    cockpit.pump();
    assert!(cockpit.queue(thread, "first".into()));
    assert!(cockpit.queue(thread, "second".into()));
    cockpit.pump();
    let open = cockpit.thread(thread).unwrap();
    assert_eq!(open.queued_all(), ["second", "first"], "newest on top");
    assert_eq!(open.queued(), Some("second"));

    cockpit.interrupt(thread);
    cockpit.pump();
    assert_eq!(
        fake.sent.borrow().as_slice(),
        ["start"],
        "nothing goes out until the turn has actually stopped"
    );
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Interrupted,
            cost_usd: None,
        })
        .unwrap();
    cockpit.pump();
    assert_eq!(
        fake.sent.borrow().as_slice(),
        ["start", "first\n\nsecond"],
        "the pile goes out as one prompt, oldest first"
    );
    assert!(cockpit.thread(thread).unwrap().queued_all().is_empty());

    // A resent id that the provider still reports as started is not a
    // second prompt; nor does its cancellation receipt say anything.
    let id = fake.native.borrow()[0].0.clone();
    fake.streams.borrow()[0]
        .send(SessionEvent::Queue(crate::QueueEvent::Started {
            historical: false,
            client_id: id,
            text: Some("first".into()),
        }))
        .unwrap();
    cockpit.pump();
    let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
    let firsts = blocks
        .iter()
        .filter(|block| matches!(&block.body, Body::Prompt(text) if text == "first"))
        .count();
    assert_eq!(
        firsts, 0,
        "the resend carries it; a late start adds nothing"
    );
    assert!(
        !blocks
            .iter()
            .any(|block| matches!(&block.body, Body::Notice(_))),
        "Ferrite's own cancellations are not the operator's news"
    );
    assert_eq!(cockpit.take_retrieved_prompt(thread), None);
}

/// Claude's CLI keeps and runs its own queue across an interrupt, so
/// Escape there sends nothing from Ferrite.
#[test]
fn escape_on_claude_leaves_the_native_queue_to_the_provider() {
    let (mut cockpit, fake) = cockpit("claude-escape-native");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.send(thread, "start".into());
    fake.streams.borrow()[0].send(text("working")).unwrap();
    cockpit.pump();
    assert!(cockpit.queue(thread, "held".into()));
    cockpit.pump();
    cockpit.interrupt(thread);
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Interrupted,
            cost_usd: None,
        })
        .unwrap();
    cockpit.pump();
    assert_eq!(fake.sent.borrow().as_slice(), ["start"]);
    assert_eq!(cockpit.thread(thread).unwrap().queued(), Some("held"));
}

#[test]
fn a_native_prompt_is_retrieved_only_after_cancel_receipt() {
    let (mut cockpit, fake) = cockpit("native-unqueue");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    assert!(cockpit.queue(thread, "run the tets".into()));
    cockpit.pump();
    assert_eq!(cockpit.unqueue(thread), None);
    assert_eq!(cockpit.take_retrieved_prompt(thread), None);
    assert_eq!(
        cockpit.thread(thread).unwrap().queued(),
        Some("run the tets")
    );
    cockpit.pump();
    assert_eq!(
        cockpit
            .take_retrieved_prompt(thread)
            .map(|(text, _)| text)
            .as_deref(),
        Some("run the tets")
    );
    assert_eq!(cockpit.thread(thread).unwrap().queued(), None);
    assert!(fake.sent.borrow().is_empty());
}

#[test]
fn a_turn_ending_with_nothing_held_sends_nothing() {
    let (mut cockpit, fake) = cockpit("nothing-held");
    cockpit.open(Provider::Claude, main_choice()).unwrap();

    fake.streams.borrow()[0].send(ended()).unwrap();
    cockpit.pump();

    assert!(fake.sent.borrow().is_empty());
}

/// #23: the Session's announced command menu becomes the Thread's — the
/// `/` popover's source — without touching the transcript or the log; a
/// revived Thread starts with none until its own Session speaks.
#[test]
fn a_commands_event_becomes_the_thread_menu_and_never_the_history() {
    let (mut cockpit, fake) = cockpit("commands");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    assert!(
        cockpit
            .thread(thread)
            .map(|open| open.commands())
            .unwrap_or_default()
            .is_empty(),
        "nothing is static"
    );

    let menu = vec![crate::SessionCommand {
        name: "compact".into(),
        description: "summarize context".into(),
        path: None,
    }];
    fake.streams.borrow()[0]
        .send(SessionEvent::Commands {
            commands: menu.clone(),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::PermissionMode {
            mode: "acceptEdits".into(),
        })
        .unwrap();
    cockpit.pump();

    assert_eq!(
        cockpit
            .thread(thread)
            .map(|open| open.commands())
            .unwrap_or_default(),
        menu.as_slice()
    );
    assert_eq!(
        cockpit
            .thread(thread)
            .and_then(|open| open.permission_mode()),
        Some("acceptEdits")
    );
    assert!(
        cockpit
            .thread(thread)
            .unwrap()
            .transcript()
            .blocks()
            .is_empty(),
        "a menu is not conversation"
    );

    // Session state only: the log replays nothing, so a revived Thread
    // waits for its own Session's announcement.
    cockpit.park(thread).unwrap();
    cockpit.revive(thread).unwrap();
    assert!(cockpit
        .thread(thread)
        .map(|open| open.commands())
        .unwrap_or_default()
        .is_empty());
    assert_eq!(
        cockpit
            .thread(thread)
            .and_then(|open| open.permission_mode()),
        None
    );
}

/// #25: the Session's announced model list becomes the Thread's — the
/// provider picker's rows — on the same lane as the command menu:
/// never the history, never the log, gone with the Session.
#[test]
fn a_models_event_becomes_the_thread_menu_and_never_the_history() {
    let (mut cockpit, fake) = cockpit("models");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    assert!(
        cockpit
            .thread(thread)
            .map(|open| open.models())
            .unwrap_or_default()
            .is_empty(),
        "nothing is static"
    );

    fake.streams.borrow()[0]
        .send(SessionEvent::Models {
            models: vec!["sonnet".into(), "opus".into(), "haiku".into()],
        })
        .unwrap();
    cockpit.pump();

    let values: Vec<&str> = cockpit
        .thread(thread)
        .map(|open| open.models())
        .unwrap_or_default()
        .iter()
        .map(|model| model.value.as_str())
        .collect();
    assert_eq!(values, ["sonnet", "opus", "haiku"]);
    assert!(
        cockpit
            .thread(thread)
            .unwrap()
            .transcript()
            .blocks()
            .is_empty(),
        "a model list is not conversation"
    );

    cockpit.park(thread).unwrap();
    cockpit.revive(thread).unwrap();
    assert!(cockpit
        .thread(thread)
        .map(|open| open.models())
        .unwrap_or_default()
        .is_empty());
}

#[test]
fn learned_provider_models_survive_parking_and_a_restart() {
    let (mut cockpit, fake) = cockpit("learned-models");
    let dir = cockpit.engine().store.dir().to_path_buf();
    let mut expected = Vec::new();
    for (index, provider) in [Provider::Claude, Provider::Codex].into_iter().enumerate() {
        let thread = cockpit.open(provider, main_choice()).unwrap();
        let models = vec![ModelInfo {
            value: format!("future-{provider:?}"),
            display: format!("Future {provider:?}"),
            detail: "Discovered by the provider".into(),
            resolved: Some(format!("resolved-{provider:?}")),
            efforts: vec!["medium".into(), "max".into()],
            default_effort: Some("medium".into()),
        }];
        fake.streams.borrow()[index]
            .send(SessionEvent::Models {
                models: models.clone(),
            })
            .unwrap();
        cockpit.pump();
        assert_eq!(cockpit.model_catalog(provider), models);
        cockpit.park(thread).unwrap();
        assert_eq!(
            cockpit.model_catalog(provider),
            models,
            "parking keeps the learned fallback"
        );
        expected.push((provider, models));
    }
    drop(cockpit);
    let reopened = Cockpit::new(Store::open(dir).unwrap(), Box::new(Fake::default()));
    assert!(
        reopened.threads().is_empty(),
        "loading menus creates no Sessions"
    );
    for (provider, models) in expected {
        assert_eq!(
            reopened.model_catalog(provider),
            models,
            "all metadata survives a restart"
        );
    }
}

#[test]
fn a_new_provider_menu_replaces_the_learned_menu_in_full() {
    let (mut cockpit, fake) = cockpit("replace-learned-models");
    cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.open(Provider::Claude, main_choice()).unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::Models {
            models: vec!["retired-model".into()],
        })
        .unwrap();
    cockpit.pump();
    let models = vec![ModelInfo::bare("future-model")];
    fake.streams.borrow()[1]
        .send(SessionEvent::Models {
            models: models.clone(),
        })
        .unwrap();
    cockpit.pump();
    assert_eq!(
        cockpit.model_catalog(Provider::Claude),
        models,
        "an older live Session must not reintroduce retired models"
    );
    fake.streams.borrow()[1]
        .send(SessionEvent::Models { models: Vec::new() })
        .unwrap();
    cockpit.pump();
    assert_eq!(
        cockpit.model_catalog(Provider::Claude),
        models,
        "an empty announcement preserves the last usable menu"
    );
}

/// AC (#25): choosing a provider before the first prompt replaces the
/// Session on the spot — eager, so the new provider's menus arrive
/// while the operator is still choosing — on a fresh Transcript, with
/// the choice durable in the header. The first send goes to that live
/// Session; nothing respawns for it.
#[test]
fn choosing_a_provider_pre_lock_respawns_eagerly_and_the_first_send_uses_it() {
    let (mut cockpit, fake) = cockpit("provider-pick");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    // The old Session had already spoken; none of it may linger.
    fake.streams.borrow()[0]
        .send(SessionEvent::Init {
            session_id: "sess-old".into(),
            model: "claude-haiku-4-5".into(),
        })
        .unwrap();
    fake.streams.borrow()[0].send(text("old provider")).unwrap();
    cockpit.pump();

    cockpit
        .set_provider(
            thread,
            ProviderChoice {
                provider: Provider::Codex,
                model: Some("gpt-5.4-mini".into()),
            },
        )
        .unwrap();

    assert_eq!(fake.streams.borrow().len(), 2, "the respawn is eager");
    assert_eq!(
        fake.spawn_pairs().last().unwrap(),
        &ProviderChoice {
            provider: Provider::Codex,
            model: Some("gpt-5.4-mini".into()),
        }
    );
    assert_eq!(
        fake.resumed.borrow().last().unwrap(),
        &None,
        "the old provider's id means nothing to the new one"
    );
    assert_eq!(
        cockpit.thread(thread).map(|open| open.provider()),
        Some(Provider::Codex)
    );
    assert_eq!(
        cockpit.thread(thread).and_then(|open| open.model()),
        Some("gpt-5.4-mini")
    );
    assert!(
        cockpit
            .thread(thread)
            .unwrap()
            .transcript()
            .blocks()
            .is_empty(),
        "the old Init and prose must not linger"
    );
    // Durable: a crash right now still revives onto the choice.
    let meta = cockpit.peek(thread).unwrap();
    assert_eq!(meta.provider, Provider::Codex);
    assert_eq!(meta.model, Some("gpt-5.4-mini".into()));

    cockpit.send(thread, "first prompt".into());
    assert_eq!(fake.streams.borrow().len(), 2, "no respawn for the send");
    assert_eq!(fake.sent.borrow().as_slice(), ["first prompt"]);
}

/// After the first prompt a provider switch is a handover: the new
/// Provider spawns fresh (the old id means nothing to it), the
/// Transcript and history stay, the header changes, the lock stays
/// armed, and the next prompt — exactly the next one — carries the
/// earlier conversation digested ahead of the operator's text. Mid-turn
/// it is refused.
#[test]
fn a_provider_switch_after_the_first_prompt_hands_the_conversation_over() {
    let (mut cockpit, fake) = cockpit("provider-handover");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.send(thread, "hello".into());
    let stream = fake.streams.borrow()[0].clone();
    stream
        .send(SessionEvent::Init {
            session_id: "sess-1".into(),
            model: "claude-opus-5".into(),
        })
        .unwrap();
    stream.send(text("hi ")).unwrap();
    stream.send(text("there")).unwrap();
    stream.send(ended()).unwrap();
    cockpit.pump();
    cockpit.send(thread, "second".into());
    stream.send(text("again")).unwrap();
    stream.send(ended()).unwrap();
    cockpit.pump();
    let blocks_before = cockpit.thread(thread).unwrap().transcript().blocks().len();

    cockpit
        .set_provider(
            thread,
            ProviderChoice {
                provider: Provider::Codex,
                model: Some("gpt-5.6-sol".into()),
            },
        )
        .unwrap();

    assert_eq!(fake.streams.borrow().len(), 2, "the new Provider spawned");
    assert_eq!(
        fake.spawn_pairs().last().unwrap(),
        &ProviderChoice {
            provider: Provider::Codex,
            model: Some("gpt-5.6-sol".into()),
        }
    );
    assert_eq!(
        fake.resumed.borrow().last().unwrap(),
        &None,
        "fresh, no resume"
    );
    assert_eq!(
        fake.efforts.borrow().last().unwrap(),
        &None,
        "the effort resets"
    );
    let open = cockpit.thread(thread).unwrap();
    assert_eq!(open.provider(), Provider::Codex);
    assert_eq!(open.model(), Some("gpt-5.6-sol"));
    assert!(open.first_prompt_sent(), "the lock stays armed");
    let blocks = open.transcript().blocks();
    assert_eq!(blocks.len(), blocks_before + 1, "the conversation stays");
    assert!(matches!(
        &blocks.last().unwrap().body,
        Body::Notice(line)
            if line == "continued on Codex · GPT-5.6 Sol — the earlier conversation is handed over as context"
    ));
    let meta = cockpit.peek(thread).unwrap();
    assert_eq!(meta.provider, Provider::Codex);
    assert_eq!(meta.model, Some("gpt-5.6-sol".into()));

    cockpit.send(thread, "third".into());
    cockpit.send(thread, "fourth".into());
    assert_eq!(
        &fake.sent.borrow()[2..],
        [
            "Earlier in this Thread (on Claude):\n\n\
             User: hello\nAssistant: hi there\n\n\
             User: second\nAssistant: again\n\n\
             third"
                .to_string(),
            "fourth".to_string(),
        ],
        "the carry goes out once, ahead of the next prompt only"
    );
    // The log has the operator's raw prompt, not the carry.
    assert!(cockpit
        .thread(thread)
        .unwrap()
        .transcript()
        .blocks()
        .iter()
        .any(|b| matches!(&b.body, Body::Prompt(line) if line == "third")));

    // Mid-turn: refused, nothing spawned.
    fake.streams.borrow()[1].send(text("working")).unwrap();
    cockpit.pump();
    let attempts = *fake.attempts.borrow();
    assert!(matches!(
        cockpit.set_provider(
            thread,
            ProviderChoice {
                provider: Provider::Claude,
                model: None,
            },
        ),
        Err(ProvisionError::Busy)
    ));
    assert_eq!(*fake.attempts.borrow(), attempts);
    assert_eq!(cockpit.peek(thread).unwrap().provider, Provider::Codex);
}

/// The displayed history is bounded and trims its oldest turns; the
/// agent's context is not. However much the Transcript trims, the log
/// keeps every event in order, the resume id persists, and a provider
/// switch hands over every exchange, read from disk.
#[test]
fn trimming_the_displayed_history_never_loses_agent_context() {
    let (mut cockpit, fake) = cockpit("retention-context");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let mut streamed = Vec::new();
    // Six turns of 1 MiB answers: past the 4 MiB the display keeps.
    for turn in 0..6 {
        cockpit.send(thread, format!("prompt {turn}"));
        let stream = fake.streams.borrow()[0].clone();
        if turn == 0 {
            stream
                .send(SessionEvent::Init {
                    session_id: "sess-1".into(),
                    model: "claude-opus-5".into(),
                })
                .unwrap();
        }
        let mut answer = String::new();
        for delta in 0..64 {
            let delta = format!("turn {turn} delta {delta} {}\n\n", "w".repeat(16 * 1024));
            answer.push_str(&delta);
            stream.send(text(&delta)).unwrap();
        }
        stream.send(ended()).unwrap();
        for _ in 0..4 {
            cockpit.pump();
        }
        streamed.push((format!("prompt {turn}"), answer));
    }
    let shown = cockpit.thread(thread).unwrap().transcript().blocks();
    assert!(
        !shown
            .iter()
            .any(|block| matches!(&block.body, Body::Prompt(line) if line == "prompt 0")),
        "the display trimmed its oldest turns"
    );

    // (1) Every event is in the log, in order.
    let log = cockpit.engine().store.load(thread).unwrap();
    let mut prompts = Vec::new();
    let mut answers = String::new();
    for input in log.inputs() {
        match input {
            Input::Prompt(line) => prompts.push(line),
            Input::Event(SessionEvent::TextDelta { text }) => answers.push_str(&text),
            _ => {}
        }
    }
    let expected: String = streamed.iter().map(|(_, answer)| answer.as_str()).collect();
    assert_eq!(
        prompts,
        (0..6)
            .map(|turn| format!("prompt {turn}"))
            .collect::<Vec<_>>()
    );
    assert!(
        answers == expected,
        "the log holds every streamed delta, in order"
    );
    // (2) The resume id persists.
    assert_eq!(log.resume_target(), Some("sess-1"));

    // (3) A provider switch hands over every exchange, from disk.
    cockpit
        .set_provider(
            thread,
            ProviderChoice {
                provider: Provider::Codex,
                model: Some("gpt-5.6-sol".into()),
            },
        )
        .unwrap();
    // Past 2 MiB the exchanges are read off the UI thread, and the
    // switch commits on the pump that finds them read.
    let mut handover = None;
    for _ in 0..500 {
        cockpit.pump();
        handover = cockpit.engine().store.load(thread).unwrap().last_handover();
        if handover.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let handover = handover.expect("the switch commits once its exchanges are read");
    assert_eq!(handover.exchanges.len(), streamed.len());
    assert!(
        handover.exchanges == streamed,
        "every exchange, the trimmed ones too, reaches the handover"
    );
}

#[test]
fn changing_effort_after_handover_keeps_the_session_and_delivers_context() {
    let (mut cockpit, fake) = cockpit("handover-live-effort");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.send(thread, "hello".into());
    fake.streams.borrow()[0].send(text("hi")).unwrap();
    fake.streams.borrow()[0].send(ended()).unwrap();
    cockpit.pump();
    cockpit
        .set_provider(
            thread,
            ProviderChoice {
                provider: Provider::Codex,
                model: Some("gpt-5.6-sol".into()),
            },
        )
        .unwrap();
    // A choice before Init must not require a resume target either.
    cockpit.set_effort(thread, Some("high".into())).unwrap();
    fake.streams.borrow()[1]
        .send(SessionEvent::Init {
            session_id: "empty-codex-thread".into(),
            model: "gpt-5.6-sol".into(),
        })
        .unwrap();
    cockpit.pump();

    cockpit.set_effort(thread, Some("max".into())).unwrap();
    cockpit.send(thread, "next".into());

    assert_eq!(*fake.attempts.borrow(), 2, "effort must not replace Codex");
    assert_eq!(
        *fake.tuned_efforts.borrow(),
        [Some("high".into()), Some("max".into())]
    );
    assert_eq!(
        fake.sent.borrow().last().unwrap(),
        "Earlier in this Thread (on Claude):\n\nUser: hello\nAssistant: hi\n\nnext"
    );
    assert_eq!(
        cockpit.thread(thread).unwrap().transcript().session_id(),
        Some("empty-codex-thread")
    );
    assert_eq!(cockpit.peek(thread).unwrap().effort.as_deref(), Some("max"));
}

/// The carry rides between the session-context preface and the
/// operator's text, so the new Provider reads where it works before
/// what was said.
#[test]
fn the_carry_rides_between_the_preface_and_the_prompt() {
    let (mut cockpit, fake) = cockpit("handover-preface");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let root = existing_root("handover-preface-root");
    cockpit
        .set_session_project_root(thread, Some(root.clone()))
        .unwrap();
    cockpit.send(thread, "hello".into());
    let stream = fake.streams.borrow().last().unwrap().clone();
    stream.send(text("hi")).unwrap();
    stream.send(ended()).unwrap();
    cockpit.pump();

    cockpit
        .set_provider(
            thread,
            ProviderChoice {
                provider: Provider::Codex,
                model: None,
            },
        )
        .unwrap();
    cockpit.send(thread, "next".into());

    let binding = std::env::temp_dir();
    assert_eq!(
        fake.sent.borrow().last().unwrap(),
        &format!(
            "{}Earlier in this Thread (on Claude):\n\nUser: hello\nAssistant: hi\n\nnext",
            preface(&binding, &root)
        )
    );
}

/// The digest is bounded: the newest dozen pairs, within the character
/// budget, oldest first; an answer that never came is left out; one
/// pair too long for the whole budget keeps its head.
#[test]
fn the_carry_digest_keeps_the_newest_pairs_within_budget() {
    let exchanges: Vec<(String, String)> = (1..=14)
        .map(|n| (format!("q{n}"), format!("a{n}")))
        .collect();
    let digest = carry_digest(Provider::Codex, &exchanges);
    assert!(digest.starts_with("Earlier in this Thread (on Codex):\n\nUser: q3\nAssistant: a3\n\n"));
    assert!(digest.ends_with("User: q14\nAssistant: a14"));
    assert!(!digest.contains("q2\n"), "only the newest twelve");

    let unanswered = vec![("just asked".to_string(), String::new())];
    assert_eq!(
        carry_digest(Provider::Claude, &unanswered),
        "Earlier in this Thread (on Claude):\n\nUser: just asked"
    );

    let long = "x".repeat(4_000);
    let heavy = vec![
        ("old".to_string(), long.clone()),
        ("mid".to_string(), long.clone()),
        ("new".to_string(), "short".to_string()),
    ];
    let digest = carry_digest(Provider::Claude, &heavy);
    assert!(digest.len() <= CARRY_CHARS + 64, "{}", digest.len());
    assert!(!digest.contains("User: old"), "the oldest goes first");
    assert!(digest.contains("User: mid"));
    assert!(digest.ends_with("User: new\nAssistant: short"));

    let huge = vec![("one".to_string(), "y".repeat(20_000))];
    let digest = carry_digest(Provider::Claude, &huge);
    assert!(digest.len() < 7_000);
    assert!(digest.ends_with('…'));
}

/// The model, unlike the provider, is never locked: after the first
/// prompt a change replaces the Session with one resuming the same
/// conversation under the new model — the transcript stays, the header
/// on disk follows, and a running turn refuses rather than being cut.
#[test]
fn the_model_can_change_after_the_first_prompt_by_resuming() {
    let (mut cockpit, fake) = cockpit("model-after-prompt");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.send(thread, "hello".into());
    fake.streams.borrow()[0]
        .send(SessionEvent::Init {
            session_id: "sess-1".into(),
            model: "claude-opus-5".into(),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta { text: "hi".into() })
        .unwrap();
    cockpit.pump();
    // Mid-turn: refused, nothing spawned.
    assert!(cockpit.thread(thread).is_some_and(|open| open.busy()));
    let attempts = *fake.attempts.borrow();
    assert!(matches!(
        cockpit.set_model(thread, Some("claude-fable-5-1".into())),
        Err(ProvisionError::Busy)
    ));
    assert_eq!(*fake.attempts.borrow(), attempts);

    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    cockpit.pump();
    let blocks_before = cockpit.thread(thread).unwrap().transcript().blocks().len();

    cockpit
        .set_model(thread, Some("claude-fable-5-1".into()))
        .unwrap();

    // A new Session, resuming the conversation, under the new model.
    let pairs = fake.spawn_pairs();
    assert_eq!(
        pairs.last().unwrap().model.as_deref(),
        Some("claude-fable-5-1")
    );
    assert_eq!(pairs.last().unwrap().provider, Provider::Claude);
    assert_eq!(
        fake.resumed.borrow().last().unwrap().as_deref(),
        Some("sess-1")
    );
    let open = cockpit.thread(thread).unwrap();
    assert_eq!(open.model(), Some("claude-fable-5-1"));
    assert!(open.first_prompt_sent(), "the lock is untouched");
    assert!(
        open.transcript().blocks().len() > blocks_before,
        "the conversation stays, and says the model changed"
    );
    assert_eq!(
        cockpit.peek(thread).unwrap().model.as_deref(),
        Some("claude-fable-5-1")
    );
    // The same choice again is a no-op.
    let attempts = *fake.attempts.borrow();
    cockpit
        .set_model(thread, Some("claude-fable-5-1".into()))
        .unwrap();
    assert_eq!(*fake.attempts.borrow(), attempts);

    // Parked: the header alone changes.
    cockpit.park(thread).unwrap();
    cockpit.set_model(thread, None).unwrap();
    assert_eq!(cockpit.peek(thread).unwrap().model, None);
}

/// A Thread is named by the operator, else by its first prompt (open
/// or parked, off the first records alone), else by its number.
#[test]
fn a_thread_is_named_by_its_title_then_its_first_prompt_then_its_number() {
    let (mut cockpit, _fake) = cockpit("display-title");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    assert_eq!(cockpit.display_title(thread, true), "thread-1");
    cockpit.send(thread, "  Fix the   flaky\ntest in ci  ".into());
    assert_eq!(cockpit.display_title(thread, true), "Fix the flaky");
    assert_eq!(
        cockpit.display_title(thread, false),
        "thread-1",
        "auto-titling can be off"
    );
    cockpit.park(thread).unwrap();
    assert_eq!(
        cockpit.display_title(thread, true),
        "Fix the flaky",
        "parked: read off the log"
    );
    cockpit.rename_thread(thread, "CI flake").unwrap();
    assert_eq!(cockpit.display_title(thread, true), "CI flake");
    cockpit.revive(thread).unwrap();
    assert_eq!(
        cockpit.display_title(thread, false),
        "CI flake",
        "a given title always wins"
    );

    assert_eq!(
        title_from_prompt("a".repeat(60).as_str()).chars().count(),
        49
    );
    assert!(title_from_prompt(&"word ".repeat(20)).ends_with('…'));
    assert_eq!(title_from_prompt("   \n\n  "), "");
}

/// A parked Thread that has spoken cannot switch off its log alone —
/// the handover needs the live conversation — but revived it can, and
/// a switch parked before its next prompt still owes the carry on
/// revive: the new Provider spawns without the old Init, the Notice is
/// replayed, and the digest goes out with the first prompt.
#[test]
fn a_parked_switch_waits_for_revive_and_a_revived_switch_still_carries() {
    let (mut cockpit, fake) = cockpit("provider-handover-parked");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.send(thread, "hello".into());
    let stream = fake.streams.borrow()[0].clone();
    stream
        .send(SessionEvent::Init {
            session_id: "sess-1".into(),
            model: "claude-opus-5".into(),
        })
        .unwrap();
    stream.send(text("hi")).unwrap();
    stream.send(ended()).unwrap();
    cockpit.pump();
    cockpit.park(thread).unwrap();

    // Parked: the store-only path refuses.
    assert!(matches!(
        cockpit.set_provider(
            thread,
            ProviderChoice {
                provider: Provider::Codex,
                model: None,
            },
        ),
        Err(ProvisionError::Locked)
    ));
    assert_eq!(cockpit.peek(thread).unwrap().provider, Provider::Claude);

    cockpit.revive(thread).unwrap();
    assert!(cockpit
        .thread(thread)
        .is_some_and(|open| open.first_prompt_sent()));
    cockpit
        .set_provider(
            thread,
            ProviderChoice {
                provider: Provider::Codex,
                model: None,
            },
        )
        .unwrap();
    // Codex can name a thread before a first prompt creates its rollout.
    // Old Ferrite versions then tried to resume it when effort changed.
    let codex = fake.streams.borrow().last().unwrap().clone();
    codex
        .send(SessionEvent::Init {
            session_id: "empty-codex-thread".into(),
            model: "gpt-5.6-sol".into(),
        })
        .unwrap();
    codex
        .send(SessionEvent::Closed {
            reason: "could not start: no rollout found for thread id empty-codex-thread".into(),
        })
        .unwrap();
    cockpit.pump();
    // Parked again before any prompt reached Codex.
    cockpit.park(thread).unwrap();
    cockpit.revive(thread).unwrap();

    assert_eq!(fake.providers.borrow().last().unwrap(), &Provider::Codex);
    assert_eq!(
        fake.resumed.borrow().last().unwrap(),
        &None,
        "the undelivered handover needs a fresh Session, not the empty Codex id"
    );
    let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
    assert!(
        blocks.iter().any(|b| matches!(
            &b.body,
            Body::Notice(line) if line.starts_with("continued on Codex · default")
        )),
        "the handover replays: {blocks:?}"
    );
    cockpit.send(thread, "next".into());
    cockpit.send(thread, "after".into());
    let sent = fake.sent.borrow();
    assert_eq!(
        sent[sent.len() - 2],
        "Earlier in this Thread (on Claude):\n\nUser: hello\nAssistant: hi\n\nnext"
    );
    assert_eq!(sent[sent.len() - 1], "after");
}

#[test]
fn a_refused_effort_keeps_the_choice_and_pending_handover() {
    let (mut cockpit, fake) = cockpit("refused-effort");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.send(thread, "hello".into());
    fake.streams.borrow()[0].send(text("hi")).unwrap();
    fake.streams.borrow()[0].send(ended()).unwrap();
    cockpit.pump();
    cockpit
        .set_provider(
            thread,
            ProviderChoice {
                provider: Provider::Codex,
                model: None,
            },
        )
        .unwrap();
    *fake.fail_effort.borrow_mut() = true;
    assert!(matches!(
        cockpit.set_effort(thread, Some("max".into())),
        Err(ProvisionError::Tune(_))
    ));
    assert_eq!(cockpit.peek(thread).unwrap().effort, None);
    assert_eq!(cockpit.thread(thread).unwrap().effort(), None);
    assert_eq!(*fake.attempts.borrow(), 2);
    cockpit.send(thread, "next".into());
    assert_eq!(
        fake.sent.borrow().last().unwrap(),
        "Earlier in this Thread (on Claude):\n\nUser: hello\nAssistant: hi\n\nnext"
    );
}

/// Effort changes in place before and after the first prompt, refused
/// mid-turn, with the header following and a model change keeping it. A provider
/// change resets it — the old provider's word may mean nothing.
#[test]
fn effort_changes_in_place_and_persists() {
    let (mut cockpit, fake) = cockpit("effort");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    assert_eq!(cockpit.thread(thread).and_then(|open| open.effort()), None);
    assert_eq!(fake.efforts.borrow().last().unwrap(), &None);

    cockpit.set_effort(thread, Some("high".into())).unwrap();
    assert_eq!(fake.streams.borrow().len(), 1, "no respawn pre-lock");
    assert_eq!(
        fake.tuned_efforts.borrow().last().unwrap().as_deref(),
        Some("high")
    );
    assert_eq!(fake.resumed.borrow().last().unwrap(), &None);
    assert_eq!(
        cockpit.thread(thread).and_then(|open| open.effort()),
        Some("high")
    );
    assert_eq!(
        cockpit.peek(thread).unwrap().effort.as_deref(),
        Some("high")
    );

    cockpit.send(thread, "hello".into());
    let stream = fake.streams.borrow()[0].clone();
    stream
        .send(SessionEvent::Init {
            session_id: "sess-1".into(),
            model: "claude-opus-5".into(),
        })
        .unwrap();
    stream.send(text("working")).unwrap();
    cockpit.pump();
    let attempts = *fake.attempts.borrow();
    assert!(matches!(
        cockpit.set_effort(thread, Some("max".into())),
        Err(ProvisionError::Busy)
    ));
    assert_eq!(*fake.attempts.borrow(), attempts);
    stream.send(ended()).unwrap();
    cockpit.pump();

    cockpit.set_effort(thread, Some("max".into())).unwrap();
    assert_eq!(
        fake.tuned_efforts.borrow().last().unwrap().as_deref(),
        Some("max")
    );
    assert_eq!(*fake.attempts.borrow(), 1, "no respawn post-lock");
    assert_eq!(cockpit.peek(thread).unwrap().effort.as_deref(), Some("max"));
    let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
    assert!(matches!(
        &blocks.last().unwrap().body,
        Body::Notice(line) if line == "effort changed to max"
    ));
    // The same choice again is a no-op.
    let attempts = *fake.attempts.borrow();
    cockpit.set_effort(thread, Some("max".into())).unwrap();
    assert_eq!(*fake.attempts.borrow(), attempts);
    assert_eq!(fake.tuned_efforts.borrow().len(), 2);

    // A model change keeps the effort.
    cockpit
        .set_model(thread, Some("claude-fable-5-1".into()))
        .unwrap();
    assert_eq!(
        fake.efforts.borrow().last().unwrap().as_deref(),
        Some("max")
    );
    assert_eq!(
        fake.models.borrow().last().unwrap().as_deref(),
        Some("claude-fable-5-1")
    );
    let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
    assert!(matches!(
        &blocks.last().unwrap().body,
        Body::Notice(line) if line == "model changed to Fable 5.1"
    ));

    // Parked: the header alone changes, and a revive reads it.
    cockpit.park(thread).unwrap();
    cockpit.set_effort(thread, None).unwrap();
    assert_eq!(cockpit.peek(thread).unwrap().effort, None);
    assert_eq!(
        cockpit.peek(thread).unwrap().model.as_deref(),
        Some("claude-fable-5-1")
    );
    cockpit.revive(thread).unwrap();
    assert_eq!(fake.efforts.borrow().last().unwrap(), &None);
    assert_eq!(cockpit.thread(thread).and_then(|open| open.effort()), None);

    // A provider change pre-lock resets the effort.
    let fresh = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.set_effort(fresh, Some("xhigh".into())).unwrap();
    cockpit
        .set_provider(
            fresh,
            ProviderChoice {
                provider: Provider::Codex,
                model: None,
            },
        )
        .unwrap();
    assert_eq!(fake.efforts.borrow().last().unwrap(), &None);
    assert_eq!(cockpit.peek(fresh).unwrap().effort, None);
    assert_eq!(cockpit.thread(fresh).and_then(|open| open.effort()), None);
}

/// The last Notice in a Thread's transcript.
fn last_notice(cockpit: &Cockpit, thread: ThreadId) -> String {
    let blocks = cockpit.thread(thread).unwrap().transcript().blocks();
    match &blocks.last().unwrap().body {
        Body::Notice(line) => line.clone(),
        body => panic!("the last block is not a Notice: {body:?}"),
    }
}

/// A provider that answers a setting change later holds it pending:
/// the Thread's effort and its header move on the pump that finds the
/// answer, and another change waits for it.
#[test]
fn an_effort_change_commits_when_the_provider_accepts_it() {
    let (mut cockpit, fake) = cockpit("effort-answered-later");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    *fake.answer_later.borrow_mut() = true;

    cockpit.set_effort(thread, Some("high".into())).unwrap();
    let open = cockpit.thread(thread).unwrap();
    assert!(open.tuning());
    assert_eq!(open.effort(), None, "not before the provider accepts it");
    assert_eq!(cockpit.peek(thread).unwrap().effort, None);
    assert!(matches!(
        cockpit.set_effort(thread, Some("max".into())),
        Err(ProvisionError::Tuning)
    ));
    cockpit.pump();
    assert!(cockpit.thread(thread).unwrap().tuning(), "still unanswered");

    fake.answers.borrow_mut().remove(0).send(Ok(())).unwrap();
    let frame = cockpit.pump();
    assert!(frame.iter().any(|update| update.thread == thread));
    let open = cockpit.thread(thread).unwrap();
    assert!(!open.tuning());
    assert_eq!(open.effort(), Some("high"));
    assert_eq!(
        cockpit.peek(thread).unwrap().effort.as_deref(),
        Some("high")
    );
    assert_eq!(last_notice(&cockpit, thread), "effort changed to high");
}

/// A change the provider refuses, or never answers, changes nothing,
/// and the transcript says why.
#[test]
fn an_effort_change_refused_after_the_fact_changes_nothing() {
    let (mut cockpit, fake) = cockpit("effort-refused-later");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    *fake.answer_later.borrow_mut() = true;

    cockpit.set_effort(thread, Some("max".into())).unwrap();
    fake.answers
        .borrow_mut()
        .remove(0)
        .send(Err(io::Error::other("unsupported effort")))
        .unwrap();
    cockpit.pump();
    assert!(!cockpit.thread(thread).unwrap().tuning());
    assert_eq!(cockpit.thread(thread).unwrap().effort(), None);
    assert_eq!(cockpit.peek(thread).unwrap().effort, None);
    assert_eq!(
        last_notice(&cockpit, thread),
        "effort unchanged: unsupported effort"
    );

    cockpit.set_effort(thread, Some("max".into())).unwrap();
    // The provider went away without answering.
    fake.answers.borrow_mut().clear();
    cockpit.pump();
    assert_eq!(cockpit.thread(thread).unwrap().effort(), None);
    assert_eq!(
        last_notice(&cockpit, thread),
        "effort unchanged: effort change was not acknowledged"
    );
}

/// The Thread's title reaches its Session: a rename is told to the live
/// one, and every later spawn — revive, retune — is handed it.
#[test]
fn a_thread_s_title_reaches_its_session_and_every_respawn() {
    let (mut cockpit, fake) = cockpit("thread-name");
    let thread = cockpit.open(Provider::Codex, main_choice()).unwrap();
    assert_eq!(fake.names.borrow().last().unwrap(), &None);

    cockpit.rename_thread(thread, "CI flake").unwrap();
    assert_eq!(fake.named.borrow().as_slice(), ["CI flake"]);

    cockpit.park(thread).unwrap();
    cockpit.revive(thread).unwrap();
    assert_eq!(
        fake.names.borrow().last().unwrap().as_deref(),
        Some("CI flake")
    );

    cockpit
        .set_model(thread, Some("gpt-5.6-sol".into()))
        .unwrap();
    assert_eq!(
        fake.names.borrow().last().unwrap().as_deref(),
        Some("CI flake")
    );
    // A parked Thread's rename has no Session to tell; the next spawn
    // carries it.
    cockpit.park(thread).unwrap();
    cockpit.rename_thread(thread, "CI flake, again").unwrap();
    assert_eq!(fake.named.borrow().len(), 1);
    cockpit.revive(thread).unwrap();
    assert_eq!(
        fake.names.borrow().last().unwrap().as_deref(),
        Some("CI flake, again")
    );
}

/// AC (#25): a parked-never-prompted Thread revives onto its chosen
/// provider and model — the durable header is what every later spawn
/// reads.
#[test]
fn a_park_then_revive_keeps_the_chosen_provider_and_model() {
    let (mut cockpit, fake) = cockpit("provider-revive");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit
        .set_provider(
            thread,
            ProviderChoice {
                provider: Provider::Codex,
                model: Some("gpt-5.4-mini".into()),
            },
        )
        .unwrap();

    cockpit.park(thread).unwrap();
    cockpit.revive(thread).unwrap();

    assert_eq!(
        fake.spawn_pairs().last().unwrap(),
        &ProviderChoice {
            provider: Provider::Codex,
            model: Some("gpt-5.4-mini".into()),
        }
    );
    assert_eq!(
        cockpit.thread(thread).map(|open| open.provider()),
        Some(Provider::Codex)
    );
    assert_eq!(
        cockpit.thread(thread).and_then(|open| open.model()),
        Some("gpt-5.4-mini")
    );
    assert!(
        !cockpit
            .thread(thread)
            .is_some_and(|open| open.first_prompt_sent()),
        "still unlocked"
    );
}

/// AC (#25): a CLI that fails to spawn refuses the whole switch — the
/// old Session keeps serving, the header is untouched, and the error
/// carries the provider's words.
#[test]
fn a_failed_provider_spawn_leaves_the_old_session_serving() {
    let (mut cockpit, fake) = cockpit("provider-spawn-fails");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    *fake.fail.borrow_mut() = true;

    let refused = cockpit.set_provider(
        thread,
        ProviderChoice {
            provider: Provider::Codex,
            model: None,
        },
    );

    let Err(ProvisionError::Spawn(e)) = refused else {
        panic!("expected the spawn refusal: {refused:?}");
    };
    assert!(e.to_string().contains("stub refused to spawn"));
    assert_eq!(
        cockpit.thread(thread).map(|open| open.provider()),
        Some(Provider::Claude)
    );
    assert_eq!(cockpit.peek(thread).unwrap().provider, Provider::Claude);
    // The old Session still serves: the next send spawns nothing new.
    *fake.fail.borrow_mut() = false;
    cockpit.send(thread, "still here".into());
    assert_eq!(fake.sent.borrow().as_slice(), ["still here"]);
    assert_eq!(fake.streams.borrow().len(), 1);
}

/// Re-picking what the Thread already is changes nothing: no teardown,
/// no respawn, no rewrite — the common path must stay friction-free.
#[test]
fn re_picking_the_current_provider_and_model_is_a_no_op() {
    let (mut cockpit, fake) = cockpit("provider-noop");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    fake.streams.borrow()[0]
        .send(text("still serving"))
        .unwrap();
    cockpit.pump();

    cockpit
        .set_provider(
            thread,
            ProviderChoice {
                provider: Provider::Claude,
                model: None,
            },
        )
        .unwrap();

    assert_eq!(fake.streams.borrow().len(), 1, "nothing respawned");
    assert!(
        !cockpit
            .thread(thread)
            .unwrap()
            .transcript()
            .blocks()
            .is_empty(),
        "and nothing was torn down"
    );
}

// ------------------------------------------- what is on screen (#28)

/// `n` open Threads on the main checkout, in open order.
fn opened(cockpit: &mut Cockpit, n: usize) -> Vec<ThreadId> {
    (0..n)
        .map(|_| cockpit.open(Provider::Claude, main_choice()).unwrap())
        .collect()
}

fn pair(cockpit: &mut Cockpit, first: ThreadId, second: ThreadId) -> GroupId {
    cockpit
        .apply_group(GroupChange::Create { first, second })
        .unwrap()
        .group
        .unwrap()
}

#[test]
fn open_threads_are_the_roster_and_solo_shows_the_focused_one() {
    let (mut cockpit, _) = cockpit("roster-solo");
    let threads = opened(&mut cockpit, 3);
    let panes: Vec<ThreadId> = cockpit
        .roster()
        .panes()
        .iter()
        .filter_map(|pane| pane.thread())
        .collect();
    assert_eq!(
        panes, threads,
        "every open Thread has a Pane, in open order"
    );
    assert_eq!(cockpit.visible(), [PaneIdentity::Thread(threads[0])]);
    assert!(cockpit.focus(PaneIdentity::Thread(threads[2])));
    assert_eq!(cockpit.visible(), [PaneIdentity::Thread(threads[2])]);
    assert_eq!(
        crate::layout::grid_shape(
            cockpit.visible().len(),
            crate::layout::Rect {
                x: 0.0,
                y: 0.0,
                w: 1134.0,
                h: 838.0
            }
        )
        .0,
        1
    );
    cockpit.park(threads[2]).unwrap();
    assert_eq!(
        cockpit.roster().panes().len(),
        2,
        "a parked Thread has no Pane"
    );
    assert_eq!(
        cockpit.roster().focused_thread(),
        Some(threads[1]),
        "focus clamps onto a survivor"
    );
}

#[test]
fn closing_parks_in_one_act_and_a_group_member_leaves_onto_the_ordinal_survivor() {
    let (mut cockpit, _) = cockpit("roster-close");
    let threads = opened(&mut cockpit, 3);
    let group = pair(&mut cockpit, threads[0], threads[1]);
    cockpit
        .apply_group(GroupChange::Join {
            thread: threads[2],
            group,
            index: None,
        })
        .unwrap();
    cockpit.enter_group(group).unwrap();
    cockpit.focus(PaneIdentity::Thread(threads[1]));

    // cmd-w on a member: out of the Group and parked, in one press.
    cockpit.close(PaneIdentity::Thread(threads[1])).unwrap();
    assert!(
        cockpit.parked().unwrap().contains(&threads[1]),
        "one park parks a Group member"
    );
    assert!(cockpit.groups().of(threads[1]).is_none(), "and it left");
    assert_eq!(cockpit.threads().len(), 2);
    assert_eq!(cockpit.roster().view(), View::Group(group));
    assert_eq!(
        cockpit.roster().focused_thread(),
        Some(threads[2]),
        "the Thread that took the parked one's ordinal"
    );

    cockpit.close(PaneIdentity::Thread(threads[2])).unwrap();
    assert!(cockpit.parked().unwrap().contains(&threads[2]));
    assert_eq!(
        cockpit.roster().view(),
        View::Solo,
        "a pair losing a member dissolves"
    );
    assert_eq!(
        cockpit.roster().focused_thread(),
        Some(threads[0]),
        "onto the survivor"
    );
    assert!(cockpit.groups().get(group).is_none());

    cockpit.close(PaneIdentity::Thread(threads[0])).unwrap();
    assert!(
        cockpit.parked().unwrap().contains(&threads[0]),
        "a Solo close parks"
    );
    assert_eq!(
        cockpit.roster().park_order(),
        [threads[1], threads[2], threads[0]]
    );
    assert!(cockpit.roster().panes().is_empty());
}

#[test]
fn parking_a_member_whose_group_is_not_on_screen_leaves_and_parks() {
    let (mut cockpit, _) = cockpit("roster-park-offscreen");
    let threads = opened(&mut cockpit, 3);
    let group = pair(&mut cockpit, threads[0], threads[1]);
    cockpit
        .apply_group(GroupChange::Join {
            thread: threads[2],
            group,
            index: None,
        })
        .unwrap();
    assert_eq!(cockpit.roster().view(), View::Solo);

    cockpit.park_thread(threads[1]).unwrap();
    assert!(cockpit.parked().unwrap().contains(&threads[1]));
    assert_eq!(
        cockpit.groups().get(group).unwrap().members,
        [threads[0], threads[2]]
    );
    assert_eq!(cockpit.roster().park_order(), [threads[1]]);
}

#[test]
fn a_new_thread_started_from_a_group_remains_outside_the_group() {
    let (mut cockpit, _) = cockpit("roster-loose-draft");
    let threads = opened(&mut cockpit, 2);
    let group = pair(&mut cockpit, threads[0], threads[1]);
    cockpit.enter_group(group).unwrap();
    let draft = cockpit.open_draft();
    assert_eq!(cockpit.roster().view(), View::Solo);
    assert_eq!(
        cockpit.visible(),
        [PaneIdentity::Draft(draft)],
        "the new draft Pane opens outside the Group"
    );
    assert_eq!(cockpit.roster().draft_scope(draft).unwrap().group, None);

    let done = cockpit
        .bootstrap_draft(
            draft,
            ProviderChoice {
                provider: Provider::Claude,
                model: None,
            },
            main_choice(),
            "new card",
            None,
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        cockpit.groups().get(group).unwrap().members,
        threads,
        "sending the draft does not add the Thread to the Group"
    );
    assert!(cockpit.groups().of(done.thread).is_none());
    assert_eq!(
        cockpit.roster().focused_thread(),
        Some(done.thread),
        "the Thread took the draft's own slot"
    );
    assert_eq!(cockpit.roster().draft_scope(draft), None);
}

#[test]
fn a_draft_beside_a_loose_thread_creates_and_opens_a_group_on_send() {
    let (mut cockpit, _) = cockpit("roster-new-group-draft");
    let first = opened(&mut cockpit, 1)[0];
    cockpit.focus(PaneIdentity::Thread(first));
    let draft = cockpit.open_draft_for_new_group(first);

    assert_eq!(
        cockpit.roster().draft_scope(draft).unwrap().new_group_with,
        Some(first)
    );
    assert_eq!(
        cockpit.visible(),
        [PaneIdentity::Thread(first), PaneIdentity::Draft(draft)],
        "the pending pair is visible before the Draft becomes durable"
    );
    assert_eq!(cockpit.roster().focused(), Some(PaneIdentity::Draft(draft)));
    assert_eq!(
        crate::layout::grid_shape(
            cockpit.visible().len(),
            crate::layout::Rect {
                x: 0.0,
                y: 0.0,
                w: 1134.0,
                h: 838.0
            }
        )
        .0,
        2
    );
    assert!(cockpit.groups().of(first).is_none());
    let done = cockpit
        .bootstrap_draft(
            draft,
            ProviderChoice {
                provider: Provider::Claude,
                model: None,
            },
            main_choice(),
            "work beside the first thread",
            None,
        )
        .unwrap()
        .unwrap();

    let group = cockpit.groups().of(done.thread).unwrap();
    assert_eq!(group.members, [first, done.thread]);
    assert_eq!(cockpit.roster().view(), View::Group(group.id));
    assert_eq!(cockpit.roster().focused_thread(), Some(done.thread));
    assert_eq!(
        cockpit.visible(),
        [
            PaneIdentity::Thread(first),
            PaneIdentity::Thread(done.thread)
        ]
    );
}

#[test]
fn entering_a_group_shows_parked_members_parked_and_keeps_focus_on_a_member() {
    let (mut cockpit, _) = cockpit("roster-enter");
    let threads = opened(&mut cockpit, 3);
    let group = pair(&mut cockpit, threads[1], threads[2]);
    // A member parked without leaving: cmd-w would take it out of the
    // Group, so this is the raw park a relaunch leaves behind.
    cockpit.park(threads[2]).unwrap();
    cockpit.roster.note_parked(threads[2]);
    assert!(cockpit.parked().unwrap().contains(&threads[2]));
    assert_eq!(cockpit.groups().of(threads[2]).map(|g| g.id), Some(group));
    cockpit.focus(PaneIdentity::Thread(threads[1]));

    cockpit.enter_group(group).unwrap();
    assert_eq!(cockpit.roster().view(), View::Group(group));
    assert_eq!(
        cockpit.roster().focused_thread(),
        Some(threads[1]),
        "focus stays on the member the operator was on"
    );
    assert!(
        cockpit.thread(threads[2]).is_none(),
        "entering spends no Session on the parked member"
    );
    assert!(
        cockpit
            .visible()
            .contains(&PaneIdentity::Thread(threads[2])),
        "but it keeps its slot on the board, as its parked tile"
    );
    assert_eq!(cockpit.roster().park_order(), [threads[2]]);

    // ⏎ on the tile wakes it in place, and cmd-o forgets it.
    cockpit.wake(threads[2]).unwrap();
    assert!(cockpit.thread(threads[2]).is_some(), "woken");
    assert_eq!(cockpit.roster().focused_thread(), Some(threads[2]));
    assert_eq!(cockpit.roster().view(), View::Group(group));
    assert!(
        cockpit.roster().park_order().is_empty(),
        "and cmd-o forgets it"
    );
    cockpit.focus(PaneIdentity::Thread(threads[1]));

    assert!(cockpit.focus_thread(threads[0]));
    assert_eq!(
        cockpit.roster().view(),
        View::Solo,
        "a loose Thread shows Solo"
    );
    cockpit.enter_group(group).unwrap();
    assert_eq!(
        cockpit.roster().focused_thread(),
        Some(threads[1]),
        "entering from outside lands on the first member"
    );
}

#[test]
fn reopen_walks_this_launches_park_order_then_creation_order() {
    let (mut cockpit, _) = cockpit("roster-reopen");
    let threads = opened(&mut cockpit, 3);
    // Parked before this launch, as far as the roster can know.
    cockpit.park(threads[2]).unwrap();
    cockpit.close(PaneIdentity::Thread(threads[0])).unwrap();
    cockpit.close(PaneIdentity::Thread(threads[1])).unwrap();
    assert!(cockpit.roster().panes().is_empty());

    let (first, opened) = cockpit.reopen_last().unwrap();
    opened.unwrap();
    assert_eq!(first, threads[1], "the last park comes back first");
    assert_eq!(cockpit.reopen_last().unwrap().0, threads[0]);
    assert_eq!(
        cockpit.reopen_last().unwrap().0,
        threads[2],
        "the order drained: creation order"
    );
    assert!(cockpit.reopen_last().is_none());
    assert_eq!(
        cockpit.roster().focused_thread(),
        Some(threads[2]),
        "reopen focuses"
    );
}

#[test]
fn fullscreen_follows_focus_and_survives_a_close_but_not_an_external_park() {
    let (mut cockpit, _) = cockpit("roster-fullscreen");
    let threads = opened(&mut cockpit, 3);
    cockpit.toggle_fullscreen();
    assert_eq!(
        cockpit.roster().fullscreen(),
        Some(PaneIdentity::Thread(threads[0]))
    );
    cockpit.step_focus(1);
    assert_eq!(
        cockpit.roster().fullscreen(),
        Some(PaneIdentity::Thread(threads[0])),
        "Solo has one visible Pane to step through"
    );
    cockpit.focus(PaneIdentity::Thread(threads[1]));
    assert_eq!(
        cockpit.roster().fullscreen(),
        Some(PaneIdentity::Thread(threads[1]))
    );
    cockpit.close(PaneIdentity::Thread(threads[1])).unwrap();
    assert_eq!(
        cockpit.roster().fullscreen(),
        Some(PaneIdentity::Thread(threads[2])),
        "the survivor fills the screen, like the next tab"
    );
    cockpit.park(threads[2]).unwrap();
    assert_eq!(
        cockpit.roster().fullscreen(),
        None,
        "parked under the roster: back to the grid"
    );
}

#[test]
fn a_drop_out_of_a_pair_tracks_its_origin_and_lands_on_the_survivor() {
    let (mut cockpit, _) = cockpit("roster-drop");
    let threads = opened(&mut cockpit, 3);
    let group = pair(&mut cockpit, threads[0], threads[1]);
    cockpit.enter_group(group).unwrap();
    cockpit
        .drop(
            Drag::Thread {
                thread: threads[0],
                group: Some(group),
            },
            View::Group(group),
            DropTarget::ThreadRow {
                thread: threads[2],
                group: None,
                index: 0,
            },
        )
        .unwrap();
    assert_eq!(cockpit.roster().view(), View::Solo);
    assert_eq!(cockpit.roster().focused_thread(), Some(threads[1]));
    assert!(cockpit.groups().get(group).is_none());
}

#[test]
fn the_next_decision_lands_in_the_view_that_shows_the_waiting_thread() {
    let (mut cockpit, fake) = cockpit("roster-decision");
    let threads = opened(&mut cockpit, 3);
    let group = pair(&mut cockpit, threads[1], threads[2]);
    fake.streams.borrow()[2]
        .send(decision("perm", "Write"))
        .unwrap();
    cockpit.pump();
    assert_eq!(cockpit.next_decision(), Some(threads[2]));
    assert_eq!(cockpit.roster().view(), View::Group(group));
    assert_eq!(cockpit.roster().focused_thread(), Some(threads[2]));
}

#[test]
#[ignore = "local performance probe"]
fn local_thread_revival_probe() {
    let store_dir = std::path::PathBuf::from("/tmp/ferrite-perf.7xYdTK");
    for id in [1_u64, 5_u64] {
        let store = Store::open(&store_dir).unwrap();
        let fake = Fake::default();
        let mut cockpit = Cockpit::new(store, Box::new(fake));
        let started = std::time::Instant::now();
        cockpit.revive(ThreadId::new(id)).unwrap();
        let elapsed = started.elapsed();
        let open = cockpit.thread(ThreadId::new(id)).unwrap();
        let subject = open
            .activity()
            .subject(&crate::activity::Subject::Main)
            .unwrap();
        eprintln!(
            "PERF thread={id} revive_ms={:.3} blocks={}",
            elapsed.as_secs_f64() * 1000.0,
            subject.transcript().blocks().len(),
        );
    }
}

/// What only the window knows reaches the Engine for the API (ADR 0030):
/// the Thread its focused Pane shows while it has the operator's focus,
/// and which Composers hold unsent text.
#[test]
fn a_window_tells_which_thread_it_shows_and_which_composer_holds_text() {
    let (mut core, _fake) = cockpit("window-says");
    let first = core.open(Provider::Claude, main_choice()).unwrap();
    let second = core.open(Provider::Claude, main_choice()).unwrap();
    assert!(core.focus_thread(second));
    core.pump();
    assert_eq!(core.engine().focused_thread(), Some(second));

    core.set_window_active(false);
    core.pump();
    assert_eq!(core.engine().focused_thread(), None);

    core.set_composing(first, true);
    core.pump();
    assert!(core.engine().composing(first));
    assert!(!core.engine().composing(second));
    core.set_composing(first, false);
    core.pump();
    assert!(!core.engine().composing(first));
}

/// A Thread parked with unsent text took its Composer with it: revived, it
/// reports no `draft`, so Projects may prompt it (ADR 0030).
#[test]
fn a_parked_thread_holds_no_draft_when_revived() {
    let (mut core, _fake) = cockpit("draft-parked");
    let thread = core.open(Provider::Claude, main_choice()).unwrap();
    core.set_composing(thread, true);
    core.pump();
    assert!(core.engine().composing(thread));

    core.park(thread).unwrap();
    core.pump();
    assert!(!core.engine().composing(thread));
    core.revive(thread).unwrap();
    core.pump();
    assert!(!core.engine().composing(thread));
}

/// A window that says a closed Thread's Composer holds text is too late:
/// nothing is held for it.
#[test]
fn a_closed_thread_takes_no_draft() {
    let (mut core, _fake) = cockpit("draft-closed");
    let thread = core.open(Provider::Claude, main_choice()).unwrap();
    core.park(thread).unwrap();
    core.pump();
    core.engine_mut().window_says(
        7,
        &crate::protocol::Op::Composing {
            thread,
            unsent: true,
        },
    );
    assert!(!core.engine().composing(thread));
}

/// `thread.start --args` (ADR 0030): the Thread's launch arguments are
/// saved, so a revive (and a Server's restart) starts its agent with them.
#[test]
fn a_thread_started_with_args_is_revived_with_them() {
    let (mut core, fake) = cockpit("launch-args");
    let args = vec!["--permission-mode".to_string(), "plan".to_string()];
    let launch = crate::engine::Launch {
        args: args.clone(),
        ..Default::default()
    };
    let thread = core
        .engine_mut()
        .open_with(Provider::Claude, main_choice(), launch)
        .unwrap();
    core.park(thread).unwrap();
    core.revive(thread).unwrap();
    assert_eq!(*fake.args.borrow(), [args.clone(), args]);
}

/// `thread.start --resume` (ADR 0030): the first Session carries on the
/// provider's own conversation, and the Thread reports it as its own.
#[test]
fn a_thread_started_on_a_provider_conversation_resumes_it() {
    let (mut core, fake) = cockpit("launch-resume");
    let launch = crate::engine::Launch {
        resume: Some("sess-9".into()),
        ..Default::default()
    };
    let thread = core
        .engine_mut()
        .open_with(Provider::Claude, main_choice(), launch)
        .unwrap();
    assert_eq!(
        fake.resumed.borrow().last().unwrap().as_deref(),
        Some("sess-9")
    );
    let view = core.engine().thread(thread).unwrap();
    assert_eq!(view.provider_session(), Some("sess-9"));
}
