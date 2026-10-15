//! The Server's coordination without its socket: a start's brief, a
//! replaced coordinator's book, the lessons read from a worker's turns,
//! and the PR feedback sent back to it.

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use super::*;
use crate::server::tests::{
    allow, decision, engine, open_in, repository, scratch, spawn, text, turn_ends,
};
use crate::server::{render, ServerInfo, SessionDefaults};
use crate::workspace::{parse_line_comments_json, CheckState, PrState, PullRequest, Remark};
use crate::SessionEvent;

/// A Server on its own Store and a repository Project, its Sessions
/// scripted: where their providers arrive, held for the test.
fn boot(name: &str) -> (Server, ProjectId, Receiver<Sender<SessionEvent>>) {
    let dir = scratch(name);
    let (spawner, providers) = spawn();
    let mut engine = engine(&dir.join("threads"), spawner);
    let project = engine.register_project(&repository(&dir)).unwrap();
    let info = ServerInfo {
        version: "0.9.0".into(),
        executable: "/bin/ferrite".into(),
    };
    let (_, renders) = render::remote();
    let defaults = Arc::new(Mutex::new(SessionDefaults::default()));
    let server = Server::new(engine, info, renders, defaults, dir);
    (server, project, providers)
}

fn provider(providers: &Receiver<Sender<SessionEvent>>) -> Sender<SessionEvent> {
    providers.recv_timeout(Duration::from_secs(5)).unwrap()
}

fn said(server: &Server, thread: ThreadId) -> String {
    let view = server.engine.thread(thread).unwrap();
    crate::transcript::plain_text(view.transcript().blocks())
}

/// A coordinator and one worker of `project`, recorded in the book
/// without a worktree, the worker's provider at hand.
fn coordinated(
    server: &mut Server,
    project: ProjectId,
    providers: &Receiver<Sender<SessionEvent>>,
    follow_prs: bool,
) -> (ThreadId, Sender<SessionEvent>) {
    let dir = server.settings_dir.clone();
    let coordinator = open_in(&mut server.engine, &dir);
    provider(providers);
    let worker = open_in(&mut server.engine, &dir);
    let worker_provider = provider(providers);
    server
        .coordinating
        .book
        .insert(Coordination {
            project,
            coordinator,
            goal: String::new(),
            instructions: String::new(),
            follow_prs,
            workers: vec![Worker {
                thread: worker,
                task: "Add invoices".into(),
                sent: Default::default(),
                merged: None,
            }],
        })
        .unwrap();
    (worker, worker_provider)
}

fn sent(server: &Server, worker: ThreadId) -> Vec<String> {
    server
        .coordinating
        .book
        .iter()
        .flat_map(|coordination| &coordination.workers)
        .find(|entry| entry.thread == worker)
        .map(|entry| entry.sent.iter().cloned().collect())
        .unwrap_or_default()
}

fn review(id: &str, body: &str) -> PrFeedback {
    PrFeedback {
        pr: PullRequest {
            number: 4,
            state: PrState::Open,
            draft: false,
            url: None,
            checks: Some(CheckState::Passing),
            runs: Vec::new(),
        },
        author: "op".into(),
        head: "c0ffee".into(),
        remarks: vec![Remark {
            id: id.into(),
            author: "ana".into(),
            body: body.into(),
            approves: false,
        }],
    }
}

/// A start for a Project with a coordinator records the brief it is
/// given on that coordinator and tells it; one whose coordinator was deleted starts a
/// successor that keeps the brief, the workers and what each was sent.
#[test]
fn a_start_records_its_brief_and_a_successor_keeps_the_book() {
    let (mut server, project, providers) = boot("coordinate-start");
    let first = server
        .start_coordinator(
            project,
            Provider::Claude,
            BriefChange {
                goal: Some("Ship billing".into()),
                ..Default::default()
            },
        )
        .unwrap();
    let first_provider = provider(&providers);
    server.engine.pump();
    first_provider.send(text("What first?")).unwrap();
    turn_ends(&first_provider);
    server.engine.pump();
    let again = server
        .start_coordinator(
            project,
            Provider::Claude,
            BriefChange {
                instructions: Some("Never push to main.".into()),
                follow_prs: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(again, first);
    server.engine.pump();
    let told = said(&server, first);
    assert!(
        told.contains("Never push to main."),
        "a new brief is told: {told}"
    );
    let book = &server.coordinating.book;
    let entry = book.of_project(project).unwrap();
    assert_eq!(entry.goal, "Ship billing");
    assert_eq!(entry.instructions, "Never push to main.");
    assert!(entry.follow_prs);

    let worker = Worker {
        thread: ThreadId::new(99),
        task: "Add invoices".into(),
        sent: ["remark:PRR_1".to_string()].into(),
        merged: None,
    };
    let recorded = worker.clone();
    server
        .coordinating
        .book
        .update(first, move |coordination| {
            coordination.workers.push(recorded)
        })
        .unwrap();
    server.execute(Op::Delete { thread: first }).unwrap();
    let successor = server
        .start_coordinator(project, Provider::Claude, BriefChange::default())
        .unwrap();
    let _successor = provider(&providers);
    assert_ne!(successor, first);
    let entry = server.coordinating.book.of_project(project).unwrap();
    assert_eq!(entry.coordinator, successor);
    assert_eq!(entry.goal, "Ship billing");
    assert_eq!(entry.instructions, "Never push to main.");
    assert!(entry.follow_prs);
    assert_eq!(entry.workers, [worker]);
    server.engine.pump();
    let briefed = said(&server, successor);
    assert!(briefed.contains("Ship billing"), "{briefed}");
}

/// A followed PR's remark is sent to its worker once across readings;
/// one a Decision holds back is sent at a later reading; a Project that
/// does not follow PRs sends nothing.
#[test]
fn pr_feedback_is_sent_once_and_waits_out_a_decision() {
    let (mut server, project, providers) = boot("coordinate-feedback");
    let (worker, provider) = coordinated(&mut server, project, &providers, true);

    server.send_feedback(vec![(worker, review("PRR_1", "Rename `total`."))]);
    server.engine.pump();
    assert_eq!(sent(&server, worker), ["remark:PRR_1"]);
    assert_eq!(said(&server, worker).matches("Rename `total`.").count(), 1);
    provider.send(text("Renamed.")).unwrap();
    turn_ends(&provider);
    server.engine.pump();
    server.send_feedback(vec![(worker, review("PRR_1", "Rename `total`."))]);
    server.engine.pump();
    let once = said(&server, worker);
    assert_eq!(once.matches("Rename `total`.").count(), 1, "{once}");

    server
        .prompt(&AgentPromptParams {
            thread: worker,
            text: "Carry on.".into(),
            wait: false,
            timeout_ms: None,
            until: Vec::new(),
        })
        .unwrap();
    provider.send(decision()).unwrap();
    server.engine.pump();
    server.send_feedback(vec![(worker, review("PRR_2", "Add a test."))]);
    assert_eq!(sent(&server, worker), ["remark:PRR_1"], "a Decision waits");
    let handle = server.engine.pending_decisions()[0].handle.clone();
    server.execute(allow(worker, &handle)).unwrap();
    server.engine.pump();
    server.send_feedback(vec![(worker, review("PRR_2", "Add a test."))]);
    assert_eq!(sent(&server, worker), ["remark:PRR_1", "remark:PRR_2"]);

    let (mut server, project, providers) = boot("coordinate-unfollowed");
    let (worker, _provider) = coordinated(&mut server, project, &providers, false);
    server.send_feedback(vec![(worker, review("PRR_1", "Rename `total`."))]);
    server.engine.pump();
    assert!(sent(&server, worker).is_empty());
    assert!(!said(&server, worker).contains("Rename"));
    server.follow_prs();
    assert!(server.coordinating.reading.is_none(), "nothing is read");
}

/// A line comment read from the forge reaches its worker once, led by
/// the place on the diff it is about.
#[test]
fn a_line_comment_is_sent_once_with_its_place() {
    let (mut server, project, providers) = boot("coordinate-line-comment");
    let (worker, provider) = coordinated(&mut server, project, &providers, true);
    let mut reading = review("PRR_1", "");
    reading.remarks = parse_line_comments_json(
        r#"[{"node_id":"PRRC_1","user":{"login":"ana"},"path":"src/bill.rs",
             "line":12,"body":"Rename `total`."}]"#,
    );

    server.send_feedback(vec![(worker, reading.clone())]);
    server.engine.pump();
    assert_eq!(sent(&server, worker), ["remark:PRRC_1"]);
    provider.send(text("Renamed.")).unwrap();
    turn_ends(&provider);
    server.engine.pump();
    server.send_feedback(vec![(worker, reading)]);
    server.engine.pump();
    let told = said(&server, worker);
    assert_eq!(
        told.matches("`src/bill.rs:12`: Rename `total`.").count(),
        1,
        "{told}"
    );
}

/// Lessons are read whenever the worker is at rest with something new:
/// a turn that waited on a Decision on the way, and a turn that came and
/// went between two passes.
#[test]
fn lessons_are_read_from_every_turn_that_ended() {
    let (mut server, project, providers) = boot("coordinate-lessons");
    let (worker, provider) = coordinated(&mut server, project, &providers, false);
    let memory = |server: &Server| server.coordinating.book.memory(project);

    server
        .execute(Op::Send {
            thread: worker,
            text: "Add invoices.".into(),
        })
        .unwrap();
    server.engine.pump();
    server.remember_lessons();
    provider.send(decision()).unwrap();
    server.engine.pump();
    server.remember_lessons();
    let handle = server.engine.pending_decisions()[0].handle.clone();
    server.execute(allow(worker, &handle)).unwrap();
    provider
        .send(text(
            "Done.\n\n## Remember\n\n- Invoices live in app/billing\n",
        ))
        .unwrap();
    turn_ends(&provider);
    server.engine.pump();
    server.remember_lessons();
    assert_eq!(memory(&server), "- Invoices live in app/billing\n");

    server
        .execute(Op::Send {
            thread: worker,
            text: "Add totals.".into(),
        })
        .unwrap();
    provider
        .send(text("Done.\n\n## Remember\n\n- Totals round half up\n"))
        .unwrap();
    turn_ends(&provider);
    server.engine.pump();
    server.remember_lessons();
    assert_eq!(
        memory(&server),
        "- Invoices live in app/billing\n- Totals round half up\n"
    );
}

/// A start from a subdirectory or a worktree of a Project answers that
/// Project; one from a repository no Project holds registers its main
/// checkout.
#[test]
fn a_start_from_anywhere_in_a_project_finds_it() {
    let (mut server, project, _providers) = boot("coordinate-project-at");
    let repo = server
        .engine
        .registry()
        .project(project)
        .unwrap()
        .root
        .clone();
    let sub = repo.join("src");
    std::fs::create_dir_all(&sub).unwrap();
    assert_eq!(server.project_at(&sub).unwrap(), project);
    let tree = server.settings_dir.join("tree");
    crate::workspace::git_for_tests(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "side",
            tree.to_str().unwrap(),
        ],
    );
    assert_eq!(server.project_at(&tree).unwrap(), project);

    let other = crate::server::tests::repository(&server.settings_dir.join("other"));
    let inner = other.join("lib");
    std::fs::create_dir_all(&inner).unwrap();
    let registered = server.project_at(&inner).unwrap();
    assert_ne!(registered, project);
    let root = &server.engine.registry().project(registered).unwrap().root;
    assert_eq!(*root, std::fs::canonicalize(&other).unwrap());
}

/// A blank task or title is refused before a worker exists, so a refused
/// start leaves nothing to brief.
#[test]
fn a_blank_task_or_title_starts_no_worker() {
    let (mut server, project, providers) = boot("coordinate-blank");
    let coordinator = server
        .start_coordinator(project, Provider::Claude, BriefChange::default())
        .unwrap();
    let _coordinator = provider(&providers);
    let threads = server.engine.threads();
    for (task, title) in [
        (" ", None),
        ("Add invoices", Some(" ")),
        ("Add invoices", Some("")),
    ] {
        let refused = server
            .start_worker(WorkerStartParams {
                thread: coordinator,
                task: task.into(),
                title: title.map(String::from),
                provider: None,
            })
            .unwrap_err();
        assert_eq!(refused.code, Code::InvalidRequest, "{task:?} {title:?}");
    }
    assert_eq!(server.engine.threads(), threads);
    let entry = server.coordinating.book.of_project(project).unwrap();
    assert!(entry.workers.is_empty());
}

/// Lessons the memory could not take are read again once it can.
#[test]
fn lessons_not_remembered_are_read_again() {
    let (mut server, project, providers) = boot("coordinate-lessons-retry");
    let (worker, provider) = coordinated(&mut server, project, &providers, false);
    server.remember_lessons();
    let memory = server.coordinating.book.memory_path(project);
    let blocker = memory.parent().unwrap().to_path_buf();
    std::fs::write(&blocker, "not a directory").unwrap();

    server
        .execute(Op::Send {
            thread: worker,
            text: "Add invoices.".into(),
        })
        .unwrap();
    provider
        .send(text(
            "Done.\n\n## Remember\n\n- Invoices live in app/billing\n",
        ))
        .unwrap();
    turn_ends(&provider);
    server.engine.pump();
    server.remember_lessons();
    assert!(server.coordinating.book.memory(project).is_empty());

    std::fs::remove_file(&blocker).unwrap();
    server.remember_lessons();
    assert_eq!(
        server.coordinating.book.memory(project),
        "- Invoices live in app/billing\n"
    );
}

fn reading(state: PrState, head: &str) -> PrFeedback {
    let mut pr = review("PRR_1", "Rename `total`.");
    pr.pr.state = state;
    pr.head = head.into();
    pr
}

/// A coordinator following PRs, at rest, and its worker in its own
/// worktree, at rest with a commit on its branch: the commit, and where
/// the worktree is.
fn merging(
    server: &mut Server,
    project: ProjectId,
    providers: &Receiver<Sender<SessionEvent>>,
) -> (ThreadId, ThreadId, Sender<SessionEvent>, String, PathBuf) {
    let follow = BriefChange {
        follow_prs: Some(true),
        ..Default::default()
    };
    let coordinator = server
        .start_coordinator(project, Provider::Claude, follow)
        .unwrap();
    let coordinator_provider = provider(providers);
    server
        .start_worker(WorkerStartParams {
            thread: coordinator,
            task: "Add invoices".into(),
            title: None,
            provider: None,
        })
        .unwrap();
    let worker_provider = provider(providers);
    let worker = server
        .coordinating
        .book
        .of_project(project)
        .unwrap()
        .workers[0]
        .thread;
    for provider in [&coordinator_provider, &worker_provider] {
        provider.send(text("Done.")).unwrap();
        turn_ends(provider);
    }
    server.engine.pump();
    let tree = server
        .engine
        .thread(worker)
        .unwrap()
        .workspace()
        .unwrap()
        .cwd()
        .to_path_buf();
    std::fs::write(tree.join("invoices.md"), "invoices\n").unwrap();
    let git = |args: &[&str]| crate::workspace::git_for_tests(&tree, args);
    git(&["add", "invoices.md"]);
    git(&[
        "-c",
        "user.email=test@example.invalid",
        "-c",
        "user.name=test",
        "commit",
        "-qm",
        "invoices",
    ]);
    let head = git(&["rev-parse", "HEAD"]).trim().to_string();
    (coordinator, worker, worker_provider, head, tree)
}

/// One pass off the pump thread, landed.
fn pass(server: &mut Server) {
    server.coordinating.read_at = None;
    server.follow_prs();
    let deadline = Instant::now() + Duration::from_secs(10);
    while server.coordinating.reading.is_some() {
        assert!(Instant::now() < deadline, "the pass never landed");
        std::thread::sleep(Duration::from_millis(10));
        server.follow_prs();
    }
}

fn merged(server: &Server, worker: ThreadId) -> Option<Merged> {
    server
        .coordinating
        .book
        .iter()
        .flat_map(|coordination| &coordination.workers)
        .find(|entry| entry.thread == worker)
        .and_then(|entry| entry.merged.clone())
}

/// A worker whose PR merged is parked once at rest; its worktree goes,
/// and its branch, at the PR's head; it is resolved: out of the open
/// workers and the Group, its coordinator told, and bound to the main
/// checkout, so a revive builds no worktree again.
#[test]
fn a_merged_worker_at_rest_is_cleaned_up_and_resolved() {
    let (mut server, project, providers) = boot("coordinate-merged");
    let (coordinator, worker, _provider, head, tree) = merging(&mut server, project, &providers);
    let branch = server
        .engine
        .registry()
        .branch_for(&tree)
        .unwrap()
        .to_string();
    let repo = server
        .engine
        .registry()
        .project(project)
        .unwrap()
        .root
        .clone();
    assert!(server.engine.groups().of(worker).is_some());

    server.land(Reading {
        prs: vec![(worker, reading(PrState::Merged, &head), true)],
        cleanups: Vec::new(),
    });
    assert!(server.engine.thread(worker).is_none(), "parked");
    pass(&mut server);

    assert!(!tree.exists());
    let branches = crate::workspace::branches(&repo).unwrap();
    assert!(!branches.contains(&branch), "{branches:?}");
    assert!(merged(&server, worker).unwrap().resolved);
    assert!(server.engine.groups().of(worker).is_none());
    let got = server.coordinator_result(coordinator).unwrap();
    assert_eq!(got["workers"], json!([]));
    assert_eq!(got["closed"][0]["merged"]["resolved"], json!(true));
    server.engine.pump();
    let told = said(&server, coordinator);
    assert!(told.contains("Pull request #4 of worker Thread"), "{told}");
    assert!(told.contains("its branch is deleted"), "{told}");

    server.execute(Op::Revive { thread: worker }).unwrap();
    let _revived = provider(&providers);
    let workspace = server.engine.thread(worker).unwrap().workspace().cloned();
    assert_eq!(workspace, Some(WorkspaceBinding::Main { checkout: repo }));
    assert!(!tree.exists());
    pass(&mut server);
    assert!(server.engine.thread(worker).is_some(), "resolved once");
}

/// A closed PR, a working worker and a dirty checkout leave the worker
/// open. A worktree git refuses to remove stays, and goes at a later
/// pass; a branch that is not at the PR's head is kept.
#[test]
fn a_merged_worker_is_cleaned_up_only_when_nothing_is_lost() {
    let (mut server, project, providers) = boot("coordinate-merged-kept");
    let (coordinator, worker, worker_provider, _head, tree) =
        merging(&mut server, project, &providers);
    let branch = server
        .engine
        .registry()
        .branch_for(&tree)
        .unwrap()
        .to_string();
    let repo = server
        .engine
        .registry()
        .project(project)
        .unwrap()
        .root
        .clone();
    let land = |server: &mut Server, state: PrState, clean: bool| {
        server.land(Reading {
            prs: vec![(worker, reading(state, "c0ffee"), clean)],
            cleanups: Vec::new(),
        });
    };

    land(&mut server, PrState::Closed, true);
    server
        .execute(Op::Send {
            thread: worker,
            text: "Add totals.".into(),
        })
        .unwrap();
    land(&mut server, PrState::Merged, true);
    assert!(merged(&server, worker).is_none(), "a working worker stays");
    turn_ends(&worker_provider);
    server.engine.pump();
    land(&mut server, PrState::Merged, false);
    assert!(merged(&server, worker).is_none(), "a dirty checkout stays");
    assert!(server.engine.thread(worker).is_some());

    land(&mut server, PrState::Merged, true);
    assert!(server.engine.thread(worker).is_none(), "parked");
    std::fs::write(tree.join("wip.md"), "wip\n").unwrap();
    pass(&mut server);
    assert!(tree.join("wip.md").exists(), "never forced");
    assert!(!merged(&server, worker).unwrap().resolved);

    std::fs::remove_file(tree.join("wip.md")).unwrap();
    pass(&mut server);
    assert!(!tree.exists());
    assert!(merged(&server, worker).unwrap().resolved);
    let branches = crate::workspace::branches(&repo).unwrap();
    assert!(branches.contains(&branch), "kept: {branches:?}");
    server.engine.pump();
    let told = said(&server, coordinator);
    assert!(
        told.contains(&format!("its branch `{branch}` is kept")),
        "{told}"
    );
}
