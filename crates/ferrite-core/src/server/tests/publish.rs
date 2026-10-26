//! What the Engine tells its windows and API callers of each open Thread
//! costs what changed, not what is open: a Thread nothing happened to is
//! neither built again nor sent, and what a caller asks of an open Thread
//! is answered from memory, never from its log.

use super::*;
use crate::protocol::ThreadState;

/// Each Thread state in `messages`, with its revision.
fn states(messages: &[ServerMessage]) -> Vec<(u64, &ThreadState)> {
    messages
        .iter()
        .filter_map(|message| match message {
            ServerMessage::Thread { revision, state } => Some((*revision, state)),
            _ => None,
        })
        .collect()
}

/// Two open Threads, each with its agent, everything about them already
/// told.
fn two_told(name: &str) -> (Engine, [ThreadId; 2], [Sender<SessionEvent>; 2], u64) {
    let (spawner, providers) = spawn();
    let dir = scratch(name);
    let mut engine = engine(&dir.join("threads"), spawner);
    let first = open_in(&mut engine, &dir);
    let first_agent = providers.recv().unwrap();
    let second = open_in(&mut engine, &dir);
    let second_agent = providers.recv().unwrap();
    engine.pump();
    let told = engine.take_messages();
    let revision = states(&told)
        .into_iter()
        .map(|(revision, _)| revision)
        .max()
        .expect("both Threads were told");
    (
        engine,
        [first, second],
        [first_agent, second_agent],
        revision,
    )
}

#[test]
fn a_thread_nothing_happened_to_is_neither_built_nor_sent() {
    let (mut engine, _, _agents, _) = two_told("publish-quiet");
    let built = engine.states_built();
    for _ in 0..3 {
        engine.pump();
        let messages = engine.take_messages();
        assert!(
            states(&messages).is_empty(),
            "nothing changed: {messages:?}"
        );
    }
    assert_eq!(engine.states_built(), built, "no state was built again");
}

#[test]
fn an_act_on_one_thread_publishes_it_once_with_a_new_revision() {
    let (mut engine, [first, second], _agents, told) = two_told("publish-act");
    let built = engine.states_built();
    engine.rename_thread(second, "Renamed").unwrap();

    let messages = engine.take_messages();
    let published = states(&messages);
    assert_eq!(published.len(), 1, "only the renamed Thread: {messages:?}");
    let (revision, state) = published[0];
    assert_eq!(state.thread, second);
    assert_eq!(state.title.as_deref(), Some("Renamed"));
    assert!(revision > told, "a new revision: {revision} after {told}");
    assert_eq!(engine.states_built(), built + 1, "{first} was not built");

    assert!(states(&engine.take_messages()).is_empty(), "told once");
}

#[test]
fn what_an_agent_says_of_itself_is_published_once() {
    let (mut engine, [first, second], [_, agent], _) = two_told("publish-heard");
    let built = engine.states_built();
    agent
        .send(SessionEvent::Commands {
            commands: vec![crate::SessionCommand {
                name: "review".into(),
                description: "Review the change".into(),
                path: None,
            }],
        })
        .unwrap();
    pump_engine_until(&mut engine, "the commands never landed", |engine| {
        !engine.thread(second).unwrap().commands().is_empty()
    });

    let messages = engine.take_messages();
    let published = states(&messages);
    assert_eq!(
        published.len(),
        1,
        "only the Thread that heard: {messages:?}"
    );
    assert_eq!(published[0].1.thread, second);
    assert_eq!(published[0].1.commands.len(), 1);
    assert_eq!(
        engine.states_built(),
        built + 1,
        "{first} was not built while {second} heard"
    );
    engine.pump();
    assert!(states(&engine.take_messages()).is_empty(), "told once");
}

#[test]
fn an_open_threads_project_is_answered_without_reading_its_log() {
    let (spawner, providers) = spawn();
    let dir = scratch("project-in-memory");
    let store = dir.join("threads");
    let mut engine = engine(&store, spawner);
    let thread = open_in(&mut engine, &dir);
    let _agent = providers.recv().unwrap();
    let project = engine.project_id(thread).expect("bound to a Project");

    // A log that could not be read: only memory can answer now.
    std::fs::remove_file(store.join(thread.to_string()).join("log.jsonl")).unwrap();

    assert_eq!(engine.project_id(thread), Some(project));
    let info = crate::api::thread_info(&engine, thread).unwrap();
    assert_eq!(info.project, Some(crate::api::project_id(project)));
    let listed = crate::api::project_infos(&engine);
    let row = listed
        .iter()
        .find(|row| row.id == crate::api::project_id(project))
        .unwrap();
    assert_eq!(row.thread_count, 1);
}
