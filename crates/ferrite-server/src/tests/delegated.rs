//! A Thread another machine's Thread starts to do a piece of work (a
//! Delegate), from this machine's side: one start per key with
//! its task sent inside it, a mode capped at the narrower of the caller's
//! and this machine's, the Labs gate, and what `provider.list` offers.

use super::*;
use ferrite_core::settings::{Labs, Settings};

/// This machine takes Threads from another machine's (Settings › Labs ›
/// Linked machines).
fn takes_delegates(harness: &Harness) {
    Settings {
        labs: Labs {
            linked_machines: true,
        },
        ..Settings::default()
    }
    .save(&harness.dir)
    .unwrap();
}

fn delegated_from() -> serde_json::Value {
    serde_json::json!({
        "server": "s-mac",
        "label": "mac",
        "thread": 12,
        "title": "Release checklist",
    })
}

/// A delegated start as the parent's Server sends it.
fn delegated(harness: &Harness, key: &str) -> serde_json::Value {
    serde_json::json!({
        "cwd": harness.dir,
        "title": "Fix the login test",
        "request": key,
        "prompt": "Fix the flaky login test.",
        "delegated_from": delegated_from(),
    })
}

fn started(answer: &serde_json::Value) -> ThreadId {
    let thread = answer["result"]["thread"]["thread"]
        .as_u64()
        .unwrap_or_else(|| panic!("{answer}"));
    ThreadId::new(thread)
}

/// Wait until the providers were sent `count` prompts.
fn sent(harness: &Harness, count: usize) -> Vec<String> {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let prompts = harness.told.lock().unwrap().prompts.clone();
        if prompts.len() >= count {
            return prompts;
        }
        assert!(Instant::now() < deadline, "only {prompts:?} were sent");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The start carries its task: the Thread opens, is titled, and its
/// agent is sent the task once. The same key again answers with that
/// Thread and starts and sends nothing; the Thread keeps who delegated it.
#[test]
fn a_repeated_start_answers_its_first_thread_and_sends_its_task_once() {
    let harness = start("delegated-once");
    takes_delegates(&harness);
    let mut caller = call(&harness);
    let first = caller.answer(
        "1",
        "thread.start",
        delegated(&harness, "delegate:s-mac:12:k-1"),
    );
    let thread = started(&first);
    assert_eq!(first["result"]["thread"]["title"], "Fix the login test");
    assert_eq!(
        first["result"]["thread"]["delegated_from"],
        delegated_from()
    );
    let _provider = harness.sessions.recv_timeout(PATIENCE).unwrap();
    assert_eq!(sent(&harness, 1), vec!["Fix the flaky login test."]);

    let again = caller.answer(
        "2",
        "thread.start",
        delegated(&harness, "delegate:s-mac:12:k-1"),
    );
    assert_eq!(started(&again), thread, "{again}");
    caller.quiet();
    assert!(
        harness.sessions.try_recv().is_err(),
        "a second Session started"
    );
    assert_eq!(harness.told.lock().unwrap().prompts.len(), 1);
    let listed = caller.answer("3", "thread.list", serde_json::json!({}));
    assert_eq!(
        listed["result"]["threads"].as_array().unwrap().len(),
        1,
        "{listed}"
    );

    let got = caller.answer("4", "thread.get", serde_json::json!({ "thread": thread }));
    assert_eq!(got["result"]["thread"]["delegated_from"], delegated_from());
    let other = caller.answer(
        "5",
        "thread.start",
        delegated(&harness, "delegate:s-mac:12:k-2"),
    );
    assert_ne!(started(&other), thread, "{other}");
}

/// A key whose Thread was parked since finds it still: the start is
/// refused `thread_parked`, naming it, and nothing starts.
#[test]
fn a_repeated_start_whose_thread_is_parked_names_it() {
    let harness = start("delegated-parked");
    takes_delegates(&harness);
    let mut caller = call(&harness);
    let first = caller.answer("1", "thread.start", delegated(&harness, "k-1"));
    let thread = started(&first);
    let _provider = harness.sessions.recv_timeout(PATIENCE).unwrap();
    caller.answer("2", "thread.close", serde_json::json!({ "thread": thread }));

    let again = caller.answer("3", "thread.start", delegated(&harness, "k-1"));
    assert_eq!(again["error"]["code"], "thread_parked", "{again}");
    assert_eq!(again["error"]["thread"], serde_json::json!(thread));
    caller.quiet();
    assert!(harness.sessions.try_recv().is_err(), "a Session started");
}

/// While this machine's Labs gate is off, a start from another machine's
/// Thread is refused and opens nothing; the operator's own starts, keyed
/// or not, go on.
#[test]
fn a_delegated_start_is_refused_while_linked_machines_is_off() {
    let harness = start("delegated-gate");
    let mut caller = call(&harness);
    let refused = caller.answer("1", "thread.start", delegated(&harness, "k-1"));
    assert_eq!(refused["error"]["code"], "linked_machines_off", "{refused}");
    let listed = caller.answer("2", "thread.list", serde_json::json!({}));
    assert_eq!(
        listed["result"]["threads"],
        serde_json::json!([]),
        "{listed}"
    );

    let own = caller.answer(
        "3",
        "thread.start",
        serde_json::json!({ "cwd": harness.dir, "request": "k-1" }),
    );
    started(&own);
}

/// A capped Thread runs at the narrower of its cap and what this
/// machine's settings and its own arguments start it at: a broad start is
/// narrowed by the arguments that win over the others, a narrow one is
/// never broadened.
#[test]
fn a_capped_start_runs_at_the_narrower_of_its_cap_and_this_machines_mode() {
    let harness = start("delegated-cap");
    let mut caller = call(&harness);
    let mut start = |id: &str, params: serde_json::Value| {
        started(&caller.answer(id, "thread.start", params));
        harness.sessions.recv_timeout(PATIENCE).unwrap();
        harness.told.lock().unwrap().args.last().cloned().unwrap()
    };
    // Codex's own sandbox is unknown here, so it counts as full access.
    let codex = start(
        "1",
        serde_json::json!({ "cwd": harness.dir, "provider": "codex", "mode": "auto_accept_edits" }),
    );
    assert_eq!(codex, ["-a", "on-request", "-s", "workspace-write"]);
    let claude = start(
        "2",
        serde_json::json!({
            "cwd": harness.dir,
            "args": ["--dangerously-skip-permissions"],
            "mode": "auto",
        }),
    );
    assert_eq!(
        claude,
        [
            "--dangerously-skip-permissions",
            "--permission-mode",
            "auto"
        ]
    );
    // Claude's default asks first: a broader cap leaves it as it is.
    let narrow = start(
        "3",
        serde_json::json!({ "cwd": harness.dir, "mode": "full_access" }),
    );
    assert_eq!(narrow, Vec::<String>::new());
}

/// `provider.list` (T3 Code's `orchestrator_capabilities`): each provider
/// with its models and their efforts, the modes a start may cap at, and
/// the mode this machine's settings start a Thread at.
#[test]
fn provider_list_says_what_this_machine_runs_threads_on() {
    let harness = start("delegated-providers");
    let mut caller = call(&harness);
    let listed = caller.answer("1", "provider.list", serde_json::json!({}));
    let providers = listed["result"]["providers"].as_array().unwrap();
    let named = |name: &str| {
        providers
            .iter()
            .find(|provider| provider["provider"] == name)
            .unwrap_or_else(|| panic!("no {name} in {listed}"))
    };
    for name in ["claude", "codex"] {
        let provider = named(name);
        assert!(
            !provider["models"].as_array().unwrap().is_empty(),
            "{provider}"
        );
        assert!(provider["models"][0]["model"].is_string(), "{provider}");
        assert_eq!(
            provider["modes"],
            serde_json::json!([
                "approval_required",
                "auto_accept_edits",
                "auto",
                "full_access"
            ])
        );
    }
    assert_eq!(named("claude")["mode"], "approval_required");
    assert_eq!(named("codex")["mode"], "full_access");
    assert_eq!(listed["result"]["default_provider"], "claude");
}
