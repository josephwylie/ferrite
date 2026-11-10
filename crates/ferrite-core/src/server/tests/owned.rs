//! Owned Threads (#219, ADR 0037): a Thread started for a Task (a
//! Worker) or for a Project's Factory (its Coordinator) is an ordinary
//! Thread the operator's own lists leave out. The API still lists it,
//! naming its owner, and the owner holds across a restart and a revive.

use super::*;

/// `thread.start` with an owner, as the CLI sends it: the answer and
/// `thread.get` name the owner, and a window holds it on the Thread.
#[test]
fn a_thread_started_for_a_task_names_its_owner() {
    let harness = start("owned-start");
    let (status, said, refused) = cli(
        &harness.api,
        "thread.start",
        serde_json::json!({ "cwd": harness.dir, "owner": { "task": "7" } }),
    );
    assert_eq!((status, refused.len()), (0, 0), "{refused:?}");
    let info = &said[0]["result"]["thread"];
    assert_eq!(info["owner"], serde_json::json!({ "task": "7" }), "{info}");
    let thread = ThreadId::new(info["thread"].as_u64().unwrap());
    harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();

    let (_, said, _) = cli(
        &harness.api,
        "thread.get",
        serde_json::json!({ "thread": thread }),
    );
    assert_eq!(
        said[0]["result"]["thread"]["owner"],
        serde_json::json!({ "task": "7" })
    );

    let mut window = cockpit(&harness);
    pump_until(&mut window, |window| window.thread(thread).is_some());
    assert_eq!(
        window.owner(thread),
        Some(crate::ThreadOwner::Task("7".into()))
    );
}

/// A Worker and a Coordinator, started by the CLI beside a Thread of the
/// operator's own: the API lists all three, naming the owners; a window
/// leaves the owned two out of what it lists and asks of the operator,
/// open or parked, before and after the Server restarts, and a revive
/// keeps the owner. The operator's own Thread is listed as ever.
// A successor takes the store as `exec` hands it over (Unix).
#[cfg(unix)]
#[test]
fn owned_threads_stay_owned_and_unlisted_across_a_restart_and_a_revive() {
    let harness = start("owned-restart");
    let mut caller = call(&harness);
    let opened = caller.answer(
        "1",
        "project.open",
        serde_json::json!({ "cwd": harness.dir }),
    );
    let project = opened["result"]["project"]["id"].clone();
    assert!(project.is_string(), "{opened}");
    let started = |caller: &mut Caller, id: &str, owner: serde_json::Value| {
        let mut params = serde_json::json!({ "cwd": harness.dir });
        if !owner.is_null() {
            params["owner"] = owner;
        }
        let answer = caller.answer(id, "thread.start", params);
        let thread = answer["result"]["thread"]["thread"]
            .as_u64()
            .unwrap_or_else(|| panic!("{answer}"));
        let provider = harness
            .sessions
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        (ThreadId::new(thread), provider)
    };
    let worker_owner = serde_json::json!({ "task": "t-1" });
    let coordinator_owner = serde_json::json!({ "factory": project });
    let (worker, worker_agent) = started(&mut caller, "2", worker_owner.clone());
    let (coordinator, _coordinator_agent) = started(&mut caller, "3", coordinator_owner.clone());
    let (mine, my_agent) = started(&mut caller, "4", serde_json::Value::Null);

    // The API lists every one, naming the owners; the operator's names none.
    let owners = |caller: &mut Caller, id: &str| {
        let listed = caller.answer(id, "thread.list", serde_json::json!({}));
        let owner_of = |thread: ThreadId| {
            listed["result"]["threads"]
                .as_array()
                .unwrap()
                .iter()
                .find(|info| info["thread"] == serde_json::json!(thread))
                .map(|info| info.get("owner").cloned())
        };
        (owner_of(worker), owner_of(coordinator), owner_of(mine))
    };
    assert_eq!(
        owners(&mut caller, "5"),
        (
            Some(Some(worker_owner.clone())),
            Some(Some(coordinator_owner.clone())),
            Some(None)
        )
    );
    let mut subscriber = call(&harness);
    let now = subscriber.answer("s", "events.subscribe", serde_json::json!({}));
    let subscribed = now["result"]["threads"].as_array().unwrap();
    let said = |thread: ThreadId| {
        subscribed
            .iter()
            .find(|info| info["thread"] == serde_json::json!(thread))
            .and_then(|info| info.get("owner").cloned())
    };
    assert_eq!(said(worker), Some(worker_owner.clone()), "{now}");
    assert_eq!(said(mine), None, "{now}");

    // Both ask; only the operator's own is the operator's to answer.
    let mut window = cockpit(&harness);
    worker_agent.send(decision()).unwrap();
    my_agent.send(decision()).unwrap();
    pump_until(&mut window, |window| window.blocked().len() == 2);
    assert_eq!(window.needs_you(), vec![mine]);
    assert!(!window.operators(worker) && !window.operators(coordinator));
    assert!(window.operators(mine));
    assert_eq!(
        window.owner(coordinator).as_ref().map(crate::api::owner),
        Some(crate::api::schema::Owner::Factory(
            project.as_str().unwrap().into()
        ))
    );

    // Parked, the Coordinator is still its Factory's, and not the
    // operator's to list or reopen.
    caller.answer(
        "6",
        "thread.close",
        serde_json::json!({ "thread": coordinator }),
    );
    pump_until(&mut window, |window| window.thread(coordinator).is_none());
    pump_until(&mut window, |window| window.owner(coordinator).is_some());
    assert!(!window.parked().unwrap().contains(&coordinator));

    let mut control = attach(&harness.address);
    control.command(219, Op::Stop { when: When::Now });
    assert_eq!(harness.server.join().unwrap(), 0);
    gone(&harness.address);
    gone(&harness.api);
    let successor = serve_in(harness.dir.clone(), None);
    let mut window = cockpit(&successor);
    pump_until(&mut window, |window| {
        window.thread(worker).is_some() && window.thread(mine).is_some()
    });
    pump_until(&mut window, |window| window.owner(coordinator).is_some());
    assert_eq!(
        window.owner(worker),
        Some(crate::ThreadOwner::Task("t-1".into()))
    );
    assert!(window.operators(mine));
    assert!(!window.parked().unwrap().contains(&coordinator));

    let mut caller = call(&successor);
    let revived = caller.answer(
        "7",
        "thread.revive",
        serde_json::json!({ "thread": coordinator }),
    );
    assert_eq!(
        revived["result"]["thread"]["owner"], coordinator_owner,
        "{revived}"
    );
    assert_eq!(
        owners(&mut caller, "8"),
        (
            Some(Some(worker_owner)),
            Some(Some(coordinator_owner)),
            Some(None)
        )
    );
    pump_until(&mut window, |window| window.thread(coordinator).is_some());
    assert!(!window.operators(coordinator));
}

/// An owner names a Project the Server has: one it has not is refused,
/// and nothing starts.
#[test]
fn a_coordinator_of_an_unknown_project_is_refused() {
    let harness = start("owned-unknown");
    let mut caller = call(&harness);
    let refused = caller.answer(
        "1",
        "thread.start",
        serde_json::json!({ "cwd": harness.dir, "owner": { "factory": "999" } }),
    );
    assert_eq!(refused["error"]["code"], "project_not_found", "{refused}");
    assert!(harness.sessions.try_recv().is_err(), "nothing started");
}
