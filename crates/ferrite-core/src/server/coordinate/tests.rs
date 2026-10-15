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
use crate::workspace::{CheckState, PrState, PullRequest, Remark};
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
            checks: Some(CheckState::Passing),
            runs: Vec::new(),
        },
        author: "op".into(),
        remarks: vec![Remark {
            id: id.into(),
            author: "ana".into(),
            body: body.into(),
            approves: false,
        }],
    }
}

/// A start for a Project with a coordinator records the brief it is
/// given on that coordinator; one whose coordinator was deleted starts a
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
    let _first = provider(&providers);
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
    let book = &server.coordinating.book;
    let entry = book.of_project(project).unwrap();
    assert_eq!(entry.goal, "Ship billing");
    assert_eq!(entry.instructions, "Never push to main.");
    assert!(entry.follow_prs);

    let worker = Worker {
        thread: ThreadId::new(99),
        task: "Add invoices".into(),
        sent: ["remark:PRR_1".to_string()].into(),
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
