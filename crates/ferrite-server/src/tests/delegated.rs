//! A Thread another machine's Thread starts to do a piece of work (a
//! Delegate), from this machine's side: one start per key with
//! its task sent inside it, a mode capped at the narrower of the caller's
//! and this machine's that nothing the caller sends can widen, the Labs
//! gate, what `provider.list` offers, and how a follower waits for its
//! turn to end and reads how it ended.

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

/// A capped Thread carries its cap to every Session it starts, a revive
/// with or without arguments of its own too, and its arguments go to the
/// CLI as they were given: the spawner holds each Session within the cap
/// (`spawner`'s `a_capped_session_starts_within_its_cap`).
#[test]
fn a_capped_thread_carries_its_cap_to_every_session() {
    use ferrite_core::runtime_mode::RuntimeMode;
    let harness = start("delegated-cap");
    let mut caller = call(&harness);
    let last = || {
        harness.sessions.recv_timeout(PATIENCE).unwrap();
        let told = harness.told.lock().unwrap();
        (
            told.args.last().cloned().unwrap(),
            *told.caps.last().unwrap(),
        )
    };
    let thread = started(&caller.answer(
        "1",
        "thread.start",
        serde_json::json!({
            "cwd": harness.dir, "args": ["--model", "haiku"], "mode": "auto",
        }),
    ));
    assert_eq!(last(), (Vec::new(), Some(RuntimeMode::Auto)));
    for (id, revive) in [
        ("2", serde_json::json!({ "thread": thread })),
        (
            "3",
            serde_json::json!({ "thread": thread, "args": ["--effort", "high"] }),
        ),
    ] {
        caller.answer(
            "close",
            "thread.close",
            serde_json::json!({ "thread": thread }),
        );
        let revived = caller.answer(id, "thread.revive", revive);
        assert!(revived.get("result").is_some(), "{revived}");
        assert_eq!(last().1, Some(RuntimeMode::Auto), "{id}");
    }

    started(&caller.answer(
        "4",
        "thread.start",
        serde_json::json!({ "cwd": harness.dir }),
    ));
    assert_eq!(last().1, None);
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

/// What a capped start's `args` may not carry, per provider: every
/// argument that sets or widens how far the agent goes without asking,
/// and the name the refusal gives it. The Server alone sets that under a
/// cap.
const WIDENING: &[(&str, &[&str], &str)] = &[
    (
        "claude",
        &["--dangerously-skip-permissions"],
        "--dangerously-skip-permissions",
    ),
    (
        "claude",
        &["--allow-dangerously-skip-permissions"],
        "--allow-dangerously-skip-permissions",
    ),
    (
        "claude",
        &["--permission-mode", "bypassPermissions"],
        "--permission-mode",
    ),
    (
        "claude",
        &["--permission-mode=acceptEdits"],
        "--permission-mode",
    ),
    ("claude", &["--allowedTools", "Write"], "--allowedTools"),
    ("claude", &["--allowedTools=Bash"], "--allowedTools"),
    ("claude", &["--allowed-tools", "Bash"], "--allowed-tools"),
    (
        "claude",
        &["--permission-prompt-tool", "mcp__x__allow"],
        "--permission-prompt-tool",
    ),
    (
        "claude",
        &["--permission-prompts", "host"],
        "--permission-prompts",
    ),
    (
        "claude",
        &["--settings", r#"{"permissions":{"allow":["Bash"]}}"#],
        "--settings",
    ),
    (
        "claude",
        &["--setting-sources", "user"],
        "--setting-sources",
    ),
    ("claude", &["--add-dir", "/"], "--add-dir"),
    ("claude", &["--mcp-config", "servers.json"], "--mcp-config"),
    ("claude", &["--plugin-dir", "plugin"], "--plugin-dir"),
    (
        "claude",
        &["--plugin-url", "https://example.com/p.zip"],
        "--plugin-url",
    ),
    ("claude", &["--agents", "{}"], "--agents"),
    ("claude", &["--agent", "reviewer"], "--agent"),
    ("claude", &["--bare"], "--bare"),
    ("claude", &["--safe-mode"], "--safe-mode"),
    ("codex", &["--yolo"], "--yolo"),
    (
        "codex",
        &["--dangerously-bypass-approvals-and-sandbox"],
        "--dangerously-bypass-approvals-and-sandbox",
    ),
    ("codex", &["-s", "danger-full-access"], "-s"),
    ("codex", &["-sdanger-full-access"], "-s"),
    ("codex", &["--sandbox", "workspace-write"], "--sandbox"),
    ("codex", &["--sandbox=danger-full-access"], "--sandbox"),
    ("codex", &["-a", "never"], "-a"),
    (
        "codex",
        &["--ask-for-approval", "never"],
        "--ask-for-approval",
    ),
    ("codex", &["--approve-for-me"], "--approve-for-me"),
    (
        "codex",
        &["--dangerously-bypass-hook-trust"],
        "--dangerously-bypass-hook-trust",
    ),
    ("codex", &["-p", "yolo"], "-p"),
    ("codex", &["--profile", "yolo"], "--profile"),
    ("codex", &["--add-dir", "/"], "--add-dir"),
    ("codex", &["-c", "approval_policy=never"], "approval_policy"),
    (
        "codex",
        &["-c", "sandbox_mode=danger-full-access"],
        "sandbox_mode",
    ),
    (
        "codex",
        &["--config", "sandbox_workspace_write.network_access=true"],
        "sandbox_workspace_write",
    ),
    (
        "codex",
        &["--config=sandbox_permissions=[\"disk-full-read-access\"]"],
        "sandbox_permissions",
    ),
    ("codex", &["-capproval_policy=never"], "approval_policy"),
    (
        "codex",
        &["-c", "approvals_reviewer=auto_review"],
        "approvals_reviewer",
    ),
    ("codex", &["-c", "profile=yolo"], "profile"),
    (
        "codex",
        &["-c", "profiles.yolo.approval_policy=never"],
        "profiles",
    ),
    (
        "codex",
        &["-c", "mcp_servers.x.command=\"sh\""],
        "mcp_servers",
    ),
];

/// A capped start refuses, naming it, each argument that sets or widens
/// how far its agent goes, and an `env` or `launcher`, either of which can
/// point the CLI at other settings or run another program; it opens
/// nothing. The cap is the Server's to set.
#[test]
fn a_capped_start_refuses_each_argument_that_sets_or_widens_its_permissions() {
    let harness = start("delegated-widening");
    let mut caller = call(&harness);
    let mut refused = |id: &str, params: serde_json::Value, named: &str| {
        let answer = caller.answer(id, "thread.start", params);
        assert_eq!(answer["error"]["code"], "widens_cap", "{named}: {answer}");
        let message = answer["error"]["message"].as_str().unwrap();
        assert!(message.contains(named), "{named}: {answer}");
    };
    for (row, (provider, args, named)) in WIDENING.iter().enumerate() {
        refused(
            &row.to_string(),
            serde_json::json!({
                "cwd": harness.dir, "provider": provider, "args": args,
                "mode": "approval_required",
            }),
            named,
        );
    }
    for provider in ["claude", "codex"] {
        refused(
            "env",
            serde_json::json!({
                "cwd": harness.dir, "provider": provider, "mode": "auto",
                "env": { "CLAUDE_CONFIG_DIR": "/tmp/other" },
            }),
            "CLAUDE_CONFIG_DIR",
        );
        refused(
            "launcher",
            serde_json::json!({
                "cwd": harness.dir, "provider": provider, "mode": "auto",
                "launcher": ["sh", "-c"],
            }),
            "launcher",
        );
    }
    let listed = caller.answer("list", "thread.list", serde_json::json!({}));
    assert_eq!(
        listed["result"]["threads"],
        serde_json::json!([]),
        "{listed}"
    );
    caller.quiet();
    assert!(harness.sessions.try_recv().is_err(), "a Session started");
}

/// The two starts #446's verifier got past the cap with: Claude's skip
/// flag before the cap, and Write and Bash allowed outright. Both are
/// refused, and nothing is sent.
#[test]
fn the_starts_that_got_past_the_cap_are_refused() {
    let harness = start("delegated-verifier");
    let mut caller = call(&harness);
    for (id, args) in [
        (
            "1",
            vec!["--model", "haiku", "--dangerously-skip-permissions"],
        ),
        (
            "2",
            vec!["--allowedTools", "Write", "--allowedTools", "Bash"],
        ),
    ] {
        let answer = caller.answer(
            id,
            "thread.start",
            serde_json::json!({
                "cwd": harness.dir, "args": args, "mode": "approval_required",
                "prompt": "Use the Write tool to create adv_skip.txt",
            }),
        );
        assert_eq!(answer["error"]["code"], "widens_cap", "{answer}");
    }
    caller.quiet();
    assert!(harness.sessions.try_recv().is_err(), "a Session started");
    assert!(harness.told.lock().unwrap().prompts.is_empty());
}

/// An uncapped start is the operator's own: its arguments go to the CLI
/// as they are, whatever they allow.
#[test]
fn an_uncapped_start_keeps_every_argument() {
    let harness = start("delegated-uncapped");
    let mut caller = call(&harness);
    let args = ["--dangerously-skip-permissions", "--allowedTools", "Write"];
    started(&caller.answer(
        "1",
        "thread.start",
        serde_json::json!({ "cwd": harness.dir, "args": args }),
    ));
    harness.sessions.recv_timeout(PATIENCE).unwrap();
    assert_eq!(harness.told.lock().unwrap().args.last().unwrap(), &args);
}

/// A parked capped Thread is revived under its cap: arguments that would
/// widen it are refused there too.
#[test]
fn a_capped_threads_revive_refuses_what_its_start_would() {
    let harness = start("delegated-revive-widening");
    let mut caller = call(&harness);
    let thread = started(&caller.answer(
        "1",
        "thread.start",
        serde_json::json!({ "cwd": harness.dir, "mode": "approval_required" }),
    ));
    harness.sessions.recv_timeout(PATIENCE).unwrap();
    caller.answer("2", "thread.close", serde_json::json!({ "thread": thread }));
    let refused = caller.answer(
        "3",
        "thread.revive",
        serde_json::json!({ "thread": thread, "args": ["--dangerously-skip-permissions"] }),
    );
    assert_eq!(refused["error"]["code"], "widens_cap", "{refused}");
    caller.quiet();
    assert!(harness.sessions.try_recv().is_err(), "a Session started");
}

/// A permission Decision that offers to remember its answer.
fn remembering_decision() -> SessionEvent {
    let SessionEvent::DecisionRequested { mut decision } = super::decision() else {
        unreachable!()
    };
    decision.suggestions = vec![ferrite_core::DecisionChoice {
        label: "Use Accept edits for this session".into(),
        value: accept_edits(),
        standing: true,
    }];
    SessionEvent::DecisionRequested { decision }
}

fn accept_edits() -> serde_json::Value {
    serde_json::json!({ "type": "setMode", "mode": "acceptEdits", "destination": "session" })
}

fn until_blocked(window: &mut Window, thread: ThreadId) {
    window.until(|window| {
        window
            .mirrors
            .view(thread)
            .is_some_and(|view| !view.pending_decisions().is_empty())
    });
}

/// Over the API a capped Thread's Decision is allowed once or refused,
/// never answered with a remembered choice (a mode, a rule), which would
/// widen what its agent does without asking: that is the operator's own,
/// from the window on this machine.
#[test]
fn only_the_window_answers_a_capped_threads_decision_with_a_remembered_choice() {
    let harness = start("delegated-remembered");
    let mut window = attach(&harness.address);
    let mut caller = call(&harness);
    let thread = started(&caller.answer(
        "1",
        "thread.start",
        serde_json::json!({ "cwd": harness.dir, "mode": "approval_required" }),
    ));
    let provider = harness.sessions.recv_timeout(PATIENCE).unwrap();
    window.watch(thread, None);
    provider.send(remembering_decision()).unwrap();
    until_blocked(&mut window, thread);
    for (id, answer) in [
        (
            "2",
            serde_json::json!({ "kind": "allow_always", "input": {}, "suggestion": accept_edits() }),
        ),
        (
            "3",
            serde_json::json!({ "kind": "choose", "value": accept_edits() }),
        ),
    ] {
        let refused = caller.answer(
            id,
            "decision.answer",
            serde_json::json!({ "thread": thread, "decision": "perm_01", "answer": answer }),
        );
        assert_eq!(refused["error"]["code"], "widens_cap", "{refused}");
    }
    assert!(harness.answers.lock().unwrap().is_empty());

    let handle: protocol::WireHandle =
        (&window.mirrors.view(thread).unwrap().pending_decisions()[0].handle).into();
    let taken = window.command(
        10,
        Op::Answer {
            thread,
            handle,
            answer: WireAnswer::AllowAlways {
                input: serde_json::json!({}),
                suggestion: accept_edits(),
            },
        },
    );
    assert!(
        matches!(taken, ServerMessage::Reply { error: None, .. }),
        "{taken:?}"
    );
    assert_eq!(harness.answers.lock().unwrap().len(), 1);
}

/// An uncapped Thread's Decision takes a remembered choice over the API,
/// as before.
#[test]
fn an_uncapped_threads_decision_takes_a_remembered_choice_over_the_api() {
    let harness = start("delegated-remembered-uncapped");
    let mut window = attach(&harness.address);
    let (thread, provider) = open(&mut window, &harness);
    window.watch(thread, None);
    provider.send(remembering_decision()).unwrap();
    until_blocked(&mut window, thread);
    let mut caller = call(&harness);
    let answered = caller.answer(
        "1",
        "decision.answer",
        serde_json::json!({
            "thread": thread, "decision": "perm_01",
            "answer": { "kind": "allow_always", "input": {}, "suggestion": accept_edits() },
        }),
    );
    assert!(answered.get("result").is_some(), "{answered}");
}

/// `ready` with a `prompt` answers once the agent has taken it: a first
/// turn that stops at a Decision is an agent up and at work, not one that
/// is not ready.
#[test]
fn a_ready_start_whose_prompt_meets_a_decision_answers_ready() {
    let harness = start("delegated-ready-blocked");
    let mut caller = call(&harness);
    caller.ask(
        "1",
        "thread.start",
        serde_json::json!({ "cwd": harness.dir, "ready": true, "prompt": "Write the file." }),
    );
    let provider = harness.sessions.recv_timeout(PATIENCE).unwrap();
    sent(&harness, 1);
    provider.send(super::decision()).unwrap();
    let answer = caller.next();
    assert!(answer.get("result").is_some(), "{answer}");
}

/// Ask for a wait until `thread`'s turn ends, answered later.
fn wait_for_the_turn(caller: &mut Caller, id: &str, thread: ThreadId, timeout_ms: u64) {
    caller.ask(
        id,
        "agent.wait",
        serde_json::json!({ "thread": thread, "turn_ended": true, "timeout_ms": timeout_ms }),
    );
}

/// A delegated Thread started with its task, and its provider.
fn delegate(harness: &Harness, caller: &mut Caller, key: &str) -> (ThreadId, Sender<SessionEvent>) {
    let before = harness.told.lock().unwrap().prompts.len();
    let thread = started(&caller.answer("start", "thread.start", delegated(harness, key)));
    let provider = harness.sessions.recv_timeout(PATIENCE).unwrap();
    sent(harness, before + 1);
    (thread, provider)
}

fn turn_ended_with(provider: &Sender<SessionEvent>, outcome: ferrite_core::TurnOutcome) {
    provider
        .send(SessionEvent::TurnEnded {
            outcome,
            cost_usd: None,
        })
        .unwrap();
}

/// A follower's wait holds while the task's turn runs and answers once it
/// ends, saying how; the result is the turn's last answer in full, past
/// what `agent.read`'s tail holds, and not the narration before its tools.
#[test]
fn a_wait_for_the_turn_answers_as_it_ends_and_the_result_is_its_whole_final_reply() {
    let harness = start("delegated-follow");
    takes_delegates(&harness);
    let mut caller = call(&harness);
    let (thread, provider) = delegate(&harness, &mut caller, "k-1");
    wait_for_the_turn(&mut caller, "wait", thread, 10_000);
    caller.quiet();

    provider.send(text("Looking at the test first.")).unwrap();
    provider
        .send(SessionEvent::ToolStarted {
            id: "toolu_1".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "cargo test" }),
        })
        .unwrap();
    let reply: Vec<String> = (1..=120).map(|line| format!("Line {line}.")).collect();
    let reply = reply.join("\n\n");
    provider.send(text(&reply)).unwrap();
    caller.quiet();
    turn_ends(&provider);
    let ended = caller.next();
    assert_eq!(ended["id"], "wait", "{ended}");
    assert_eq!(ended["result"]["thread"]["turn"], "completed", "{ended}");

    let result = caller.answer("r", "agent.result", serde_json::json!({ "thread": thread }));
    assert_eq!(
        result["result"],
        serde_json::json!({
            "thread": thread,
            "outcome": "completed",
            "text": reply,
            "parked": false,
        }),
        "{result}"
    );
}

/// Each way a turn ends is its outcome; a failed turn's result is its
/// error, not what it said before it failed.
#[test]
fn each_way_a_turn_ends_is_its_outcome_and_a_failed_ones_result_is_its_error() {
    let harness = start("delegated-outcomes");
    takes_delegates(&harness);
    let mut caller = call(&harness);
    for (key, outcome, word) in [
        ("k-done", ferrite_core::TurnOutcome::Completed, "completed"),
        (
            "k-failed",
            ferrite_core::TurnOutcome::Error("rate limited".into()),
            "failed",
        ),
        (
            "k-stopped",
            ferrite_core::TurnOutcome::Interrupted,
            "interrupted",
        ),
    ] {
        let (thread, provider) = delegate(&harness, &mut caller, key);
        provider.send(text("Half way.")).unwrap();
        turn_ended_with(&provider, outcome);
        wait_for_the_turn(&mut caller, key, thread, 10_000);
        let ended = caller.next();
        assert_eq!(ended["result"]["thread"]["turn"], word, "{ended}");
        let result = caller.answer("r", "agent.result", serde_json::json!({ "thread": thread }));
        assert_eq!(result["result"]["outcome"], word, "{result}");
        let text = match word {
            "failed" => "rate limited",
            _ => "Half way.",
        };
        assert_eq!(result["result"]["text"], text, "{result}");
    }
}

/// A Thread never prompted has no turn to end, and a turn a Decision
/// blocks has not ended: neither answers the wait, which runs out.
#[test]
fn a_thread_never_prompted_and_a_blocked_turn_do_not_end_the_wait() {
    let harness = start("delegated-blocked");
    let mut caller = call(&harness);
    let thread = started(&caller.answer(
        "1",
        "thread.start",
        serde_json::json!({ "cwd": harness.dir }),
    ));
    let provider = harness.sessions.recv_timeout(PATIENCE).unwrap();
    let unprompted = caller.answer(
        "2",
        "agent.wait",
        serde_json::json!({ "thread": thread, "turn_ended": true, "timeout_ms": 300 }),
    );
    assert_eq!(unprompted["error"]["code"], "timeout", "{unprompted}");

    caller.answer(
        "3",
        "agent.prompt",
        serde_json::json!({ "thread": thread, "text": "Write the file." }),
    );
    provider.send(decision()).unwrap();
    let blocked = caller.answer(
        "4",
        "agent.wait",
        serde_json::json!({ "thread": thread, "until": "blocked" }),
    );
    assert_eq!(blocked["result"]["thread"].get("turn"), None, "{blocked}");
    let waiting = caller.answer(
        "5",
        "agent.wait",
        serde_json::json!({ "thread": thread, "turn_ended": true, "timeout_ms": 300 }),
    );
    assert_eq!(waiting["error"]["code"], "timeout", "{waiting}");
}

/// A parked Thread is not gone: its result still reads from its log;
/// closing or stopping it changes nothing and answers it, as `thread.stop`
/// does; what answers with or acts on its live agent is refused
/// `thread_parked`, naming it, and does nothing; a Thread the Store never
/// held is `unknown_thread` to every one.
#[test]
fn a_parked_threads_result_still_reads_and_only_a_gone_one_is_unknown() {
    let harness = start("delegated-parked-result");
    takes_delegates(&harness);
    let mut caller = call(&harness);
    let (thread, provider) = delegate(&harness, &mut caller, "k-1");
    provider.send(text("All fixed.")).unwrap();
    turn_ends(&provider);
    wait_for_the_turn(&mut caller, "wait", thread, 10_000);
    assert_eq!(caller.next()["result"]["thread"]["turn"], "completed");
    caller.answer(
        "close",
        "thread.close",
        serde_json::json!({ "thread": thread }),
    );

    let result = caller.answer("r", "agent.result", serde_json::json!({ "thread": thread }));
    assert_eq!(
        result["result"],
        serde_json::json!({
            "thread": thread,
            "outcome": "completed",
            "text": "All fixed.",
            "parked": true,
        }),
        "{result}"
    );
    let calls = [
        ("thread.get", serde_json::json!({})),
        ("thread.focus", serde_json::json!({})),
        ("thread.rename", serde_json::json!({ "title": "Renamed" })),
        ("agent.prompt", serde_json::json!({ "text": "carry on" })),
        ("agent.wait", serde_json::json!({})),
        ("agent.wait", serde_json::json!({ "turn_ended": true })),
        ("agent.read", serde_json::json!({})),
        ("agent.interrupt", serde_json::json!({})),
    ];
    let on = |thread: serde_json::Value, params: &serde_json::Value| {
        let mut params = params.clone();
        params["thread"] = thread;
        params
    };
    for (method, params) in &calls {
        let parked = caller.answer(method, method, on(serde_json::json!(thread), params));
        assert_eq!(parked["error"]["code"], "thread_parked", "{parked}");
        assert_eq!(
            parked["error"]["thread"],
            serde_json::json!(thread),
            "{parked}"
        );
    }
    let title = caller.answer("title", "thread.list", serde_json::json!({}));
    assert!(!title.to_string().contains("Renamed"), "{title}");
    assert_eq!(
        harness.told.lock().unwrap().prompts.len(),
        1,
        "a prompt was sent"
    );
    for method in ["thread.close", "thread.stop"] {
        let done = caller.answer(method, method, serde_json::json!({ "thread": thread }));
        assert_eq!(
            done["result"],
            serde_json::json!({ "thread": thread }),
            "{done}"
        );
    }

    let more = [
        ("agent.result", serde_json::json!({})),
        ("thread.close", serde_json::json!({})),
        ("thread.stop", serde_json::json!({})),
    ];
    for (method, params) in calls.iter().chain(&more) {
        let gone = caller.answer(method, method, on(serde_json::json!(999), params));
        assert_eq!(gone["error"]["code"], "unknown_thread", "{method}: {gone}");
    }
}
