//! The API without a Server: the wait rules, the operations' one
//! declaration and the wire it makes — frozen like the client contract,
//! one request fixture per operation under `fixtures/v1`, append-only —
//! and the CLI read from it. The Server answering over its socket is in
//! `server::tests`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::cli::{self, Context};
use super::schema::*;
use super::*;
use crate::protocol::{WireAnswer, WireSound};

// ------------------------------------------------------------ waiting

fn seen(attention: Attention) -> Seen {
    Seen {
        attention,
        starting: false,
    }
}

fn starting() -> Seen {
    Seen {
        attention: Attention::Idle,
        starting: true,
    }
}

fn at(start: Instant, ms: u64) -> Instant {
    start + Duration::from_millis(ms)
}

/// Herdr's rule: the idle a prompt was sent from never answers its wait;
/// the agent must be seen at work first, and then settle.
#[test]
fn a_prompts_wait_settles_only_after_its_agent_was_seen_working() {
    let start = Instant::now();
    let mut wait = Wait::after_prompt(None, start);

    assert_eq!(wait.observe(seen(Attention::Idle), start), None);
    assert_eq!(wait.observe(seen(Attention::Done), at(start, 100)), None);
    assert_eq!(wait.observe(seen(Attention::Working), at(start, 200)), None);
    assert_eq!(
        wait.observe(seen(Attention::Working), at(start, 60_000)),
        None
    );
    assert_eq!(
        wait.observe(seen(Attention::Done), at(start, 61_000)),
        Some(Settled::Reached)
    );
}

/// An agent that stops at a Decision was at work: the wait settles
/// `blocked`, at once.
#[test]
fn a_prompts_wait_settles_blocked_when_its_agent_asks_a_decision() {
    let start = Instant::now();
    let mut wait = Wait::after_prompt(None, start);

    assert_eq!(
        wait.observe(seen(Attention::Blocked), at(start, 300)),
        Some(Settled::Reached)
    );
}

#[test]
fn a_prompt_whose_agent_shows_no_work_stalls_after_five_seconds() {
    let start = Instant::now();
    let mut wait = Wait::after_prompt(None, start);

    assert_eq!(wait.observe(seen(Attention::Idle), start), None);
    assert_eq!(wait.observe(seen(Attention::Idle), at(start, 4_999)), None);
    assert_eq!(
        wait.observe(seen(Attention::Idle), at(start, 5_000)),
        Some(Settled::Stalled)
    );
}

/// A Session still starting holds the prompt: the five seconds count
/// from when it is up, not from the request.
#[test]
fn the_stall_clock_waits_for_the_session_to_be_up() {
    let start = Instant::now();
    let mut wait = Wait::after_prompt(None, start);

    assert_eq!(wait.observe(starting(), start), None);
    assert_eq!(wait.observe(starting(), at(start, 20_000)), None);
    assert_eq!(wait.observe(seen(Attention::Idle), at(start, 21_000)), None);
    assert_eq!(wait.observe(seen(Attention::Idle), at(start, 25_999)), None);
    assert_eq!(
        wait.observe(seen(Attention::Idle), at(start, 26_000)),
        Some(Settled::Stalled)
    );
}

#[test]
fn a_timeout_shorter_than_the_stall_comes_first() {
    let start = Instant::now();
    let mut wait = Wait::after_prompt(Some(Duration::from_secs(2)), start);

    assert_eq!(wait.observe(seen(Attention::Idle), at(start, 1_999)), None);
    assert_eq!(
        wait.observe(seen(Attention::Idle), at(start, 2_000)),
        Some(Settled::TimedOut)
    );
}

/// The timeout counts from the request, through the work.
#[test]
fn a_prompts_wait_times_out_while_its_agent_still_works() {
    let start = Instant::now();
    let mut wait = Wait::after_prompt(Some(Duration::from_secs(30)), start);

    assert_eq!(wait.observe(seen(Attention::Working), start), None);
    assert_eq!(
        wait.observe(seen(Attention::Working), at(start, 30_000)),
        Some(Settled::TimedOut)
    );
}

/// `agent.wait` has no prompt to see land: an agent already at rest
/// answers at once.
#[test]
fn a_plain_wait_answers_at_once_when_the_agent_is_settled() {
    let start = Instant::now();
    for attention in [Attention::Idle, Attention::Done, Attention::Blocked] {
        let mut wait = Wait::until(Until::Settled, None, start);
        assert_eq!(wait.observe(seen(attention), start), Some(Settled::Reached));
    }
    let mut wait = Wait::until(Until::Settled, None, start);
    assert_eq!(wait.observe(seen(Attention::Working), start), None);
    assert_eq!(
        wait.observe(starting(), at(start, 60_000)),
        Some(Settled::Reached)
    );
}

#[test]
fn a_wait_until_one_attention_passes_every_other() {
    let start = Instant::now();
    let mut wait = Wait::until(Until::Attention(Attention::Blocked), None, start);

    for (ms, attention) in [
        (0, Attention::Idle),
        (100, Attention::Working),
        (200, Attention::Done),
        (60_000, Attention::Idle),
    ] {
        assert_eq!(wait.observe(seen(attention), at(start, ms)), None);
    }
    assert_eq!(
        wait.observe(seen(Attention::Blocked), at(start, 60_100)),
        Some(Settled::Reached)
    );
}

// ------------------------------------------------------ the declaration

/// The operations, in the order declared. Names are append-only: a
/// script written against one Ferrite keeps working against the next.
#[test]
fn the_operations_are_the_ones_declared_and_never_renamed() {
    let names: Vec<&str> = Method::OPERATIONS.iter().map(|op| op.name).collect();
    assert_eq!(
        names,
        [
            "thread.start",
            "thread.list",
            "thread.get",
            "thread.focus",
            "thread.rename",
            "thread.close",
            "agent.prompt",
            "agent.wait",
            "agent.read",
            "agent.interrupt",
            "decision.list",
            "decision.answer",
            "project.list",
            "project.get",
            "project.open",
            "project.rename",
            "project.close",
            "worktree.create",
            "worktree.remove",
            "machine.list",
            "events.subscribe",
            "notification.show",
            "project.focus",
            "thread.revive",
        ]
    );
}

#[test]
fn every_operation_is_a_group_and_a_verb_whose_positionals_are_its_parameters() {
    for op in Method::OPERATIONS {
        let (group, verb) = op.words();
        assert!(
            !group.is_empty() && !verb.is_empty() && !verb.contains('.'),
            "{}",
            op.name
        );
        let schema = (op.params)().to_value();
        for name in op.positional {
            assert!(
                schema["properties"].get(*name).is_some(),
                "{}: positional `{name}` is not a parameter",
                op.name
            );
        }
        assert!(!op.summary.is_empty(), "{}", op.name);
    }
}

/// The smallest value a schema admits, for the sample requests below.
/// After Herdr's `operation_catalog_matches_every_wire_method` sampler
/// (Apache-2.0).
fn sample_value(schema: &Value, defs: &Value) -> Value {
    if let Some(reference) = schema["$ref"].as_str() {
        return sample_value(&defs[reference.rsplit('/').next().unwrap()], defs);
    }
    if let Some(value) = schema.get("const") {
        return value.clone();
    }
    if let Some(values) = schema["enum"].as_array() {
        return values[0].clone();
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(branches) = schema[key].as_array() {
            return sample_value(&branches[0], defs);
        }
    }
    let kind = match &schema["type"] {
        Value::Array(kinds) => kinds[0].as_str(),
        kind => kind.as_str(),
    };
    match kind {
        Some("object") => {
            let mut object = serde_json::Map::new();
            for name in schema["required"].as_array().into_iter().flatten() {
                let name = name.as_str().unwrap();
                object.insert(name.into(), sample_value(&schema["properties"][name], defs));
            }
            Value::Object(object)
        }
        Some("array") => json!([]),
        Some("string") => json!("sample"),
        Some("integer" | "number") => json!(1),
        Some("boolean") => json!(false),
        _ => Value::Null,
    }
}

/// Each operation's schema describes what its method decodes: the least
/// request it admits is one this Server reads, under that name. The
/// whole API's schema names every method.
#[test]
fn every_operations_schema_admits_a_request_this_server_reads() {
    let whole = serde_json::to_string(&schemars::schema_for!(Request)).unwrap();
    for op in Method::OPERATIONS {
        let schema = (op.params)().to_value();
        let params = sample_value(&schema, &schema["$defs"]);
        let request: Request = serde_json::from_value(json!({
            "id": "sample",
            "method": op.name,
            "params": params,
        }))
        .unwrap_or_else(|error| panic!("{}: {error}", op.name));
        assert_eq!(request.method.name(), op.name);
        assert!(whole.contains(&format!("\"{}\"", op.name)), "{}", op.name);
    }
}

fn request(method: Method) -> Request {
    Request {
        id: "1".into(),
        method,
    }
}

fn thread() -> ThreadId {
    ThreadId::new(12)
}

/// One request per operation, as the fixture of its name freezes it.
fn samples() -> Vec<Request> {
    vec![
        request(Method::ThreadStart(ThreadStartParams {
            title: Some("reviewer".into()),
            ..thread_start()
        })),
        request(Method::ThreadList(Empty {})),
        request(Method::ThreadGet(ThreadParams { thread: thread() })),
        request(Method::ThreadFocus(ThreadParams { thread: thread() })),
        request(Method::ThreadRename(ThreadRenameParams {
            thread: thread(),
            title: "reviewer".into(),
        })),
        request(Method::ThreadClose(ThreadParams { thread: thread() })),
        request(Method::AgentPrompt(AgentPromptParams {
            thread: thread(),
            text: "Review the diff".into(),
            wait: true,
            timeout_ms: Some(120_000),
            until: Vec::new(),
        })),
        request(Method::AgentWait(AgentWaitParams {
            thread: thread(),
            until: Some(Attention::Blocked),
            timeout_ms: Some(60_000),
        })),
        request(Method::AgentRead(AgentReadParams {
            thread: thread(),
            lines: Some(200),
        })),
        request(Method::AgentInterrupt(ThreadParams { thread: thread() })),
        request(Method::DecisionList(DecisionListParams {
            thread: Some(thread()),
        })),
        request(Method::DecisionAnswer(DecisionAnswerParams {
            thread: thread(),
            decision: "perm_01".into(),
            answer: WireAnswer::Allow { input: json!({}) },
        })),
        request(Method::ProjectList(Empty {})),
        request(Method::ProjectGet(ProjectParams {
            project: "3".into(),
        })),
        request(Method::ProjectOpen(ProjectOpenParams {
            cwd: PathBuf::from("/work/app"),
            label: Some("billing".into()),
            focus: true,
        })),
        request(Method::ProjectRename(ProjectRenameParams {
            project: "3".into(),
            label: "billing".into(),
        })),
        request(Method::ProjectClose(ProjectParams {
            project: "3".into(),
        })),
        request(Method::WorktreeCreate(WorktreeCreateParams {
            cwd: PathBuf::from("/work/app"),
            branch: "hp/billing/t-0001".into(),
            base: Some("origin/main".into()),
        })),
        request(Method::WorktreeRemove(WorktreeRemoveParams {
            path: PathBuf::from("/work/.worktrees/app/hp-billing-t-0001"),
        })),
        request(Method::MachineList(Empty {})),
        request(Method::EventsSubscribe(EventsSubscribeParams {
            threads: vec![thread(), ThreadId::new(14)],
        })),
        request(Method::NotificationShow(NotificationShowParams {
            title: "Review ready".into(),
            body: Some("3 findings".into()),
            sound: WireSound::None,
            important: false,
        })),
        request(Method::ProjectFocus(ProjectParams {
            project: "3".into(),
        })),
        request(Method::ThreadRevive(ThreadReviveParams {
            thread: thread(),
            args: vec!["--model".into(), "opus".into()],
            resume: Some("4f2a".into()),
            env: [("HERDR_PROJECTS_DATA_DIR".into(), "/data".into())].into(),
            launcher: Vec::new(),
            ready: true,
            timeout_ms: Some(60_000),
        })),
    ]
}

/// `thread.start` as the frozen fixture has it, every later parameter
/// absent.
fn thread_start() -> ThreadStartParams {
    ThreadStartParams {
        cwd: PathBuf::from("/work/app"),
        provider: Some(Provider::Codex),
        worktree: true,
        title: None,
        focus: false,
        args: Vec::new(),
        branch: None,
        base: None,
        project: None,
        ready: false,
        timeout_ms: None,
        resume: None,
        env: Default::default(),
        launcher: Vec::new(),
    }
}

/// Requests a later parameter changed, frozen beside the operations' own.
fn later_samples() -> Vec<(&'static str, Request)> {
    vec![
        (
            "thread_start_herdr",
            request(Method::ThreadStart(ThreadStartParams {
                title: Some("t-0001".into()),
                focus: true,
                args: vec![
                    "--dangerously-bypass-approvals-and-sandbox".into(),
                    "--model".into(),
                    "gpt-5.5".into(),
                ],
                branch: Some("hp/demo/t-0001".into()),
                base: Some("main".into()),
                project: Some("3".into()),
                ready: true,
                timeout_ms: Some(20_000),
                ..thread_start()
            })),
        ),
        (
            "thread_start_resume",
            request(Method::ThreadStart(ThreadStartParams {
                worktree: false,
                resume: Some("4f2a".into()),
                ..thread_start()
            })),
        ),
        (
            "agent_prompt_until",
            request(Method::AgentPrompt(AgentPromptParams {
                thread: thread(),
                text: "Review the diff".into(),
                wait: false,
                timeout_ms: Some(8_000),
                until: vec![Attention::Working, Attention::Blocked],
            })),
        ),
        (
            "notification_show_chime",
            request(Method::NotificationShow(NotificationShowParams {
                title: "Review ready".into(),
                body: None,
                sound: WireSound::Request,
                important: true,
            })),
        ),
        (
            "thread_start_launch",
            request(Method::ThreadStart(ThreadStartParams {
                worktree: false,
                env: [
                    ("HERDR_PROJECTS_CONFIG_DIR".into(), "/config".into()),
                    ("HERDR_PROJECTS_DATA_DIR".into(), "/data".into()),
                ]
                .into(),
                launcher: [
                    "bwrap",
                    "--dev-bind",
                    "/",
                    "/",
                    "--ro-bind",
                    "/r",
                    "/r",
                    "--",
                ]
                .map(String::from)
                .into(),
                ..thread_start()
            })),
        ),
    ]
}

/// A later parameter freezes as its own fixture: what a script sends with
/// it today is read the same by every later Server.
#[test]
fn every_later_parameter_has_a_frozen_request_that_round_trips() {
    for (name, sample) in later_samples() {
        let path = fixtures().join(format!("{name}.json"));
        let encoded = protocol::encode(&sample);
        let frozen = std::fs::read(&path).unwrap_or_else(|_| panic!("no fixture {name}"));
        assert_eq!(
            String::from_utf8_lossy(&encoded),
            String::from_utf8_lossy(&frozen),
            "{name}"
        );
        assert_eq!(
            decode(frozen.strip_suffix(b"\n").unwrap()).as_ref(),
            Ok(&sample)
        );
    }
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/api/fixtures/v1")
}

fn fixture_name(request: &Request) -> String {
    request.method.name().replace('.', "_")
}

/// A request a script sends today is read by every later Server: each
/// operation's fixture decodes to its sample and re-encodes byte for byte.
/// A missing one is written by running with `FERRITE_BLESS_FIXTURES=1`,
/// which never overwrites one.
#[test]
fn every_operation_has_a_frozen_request_that_round_trips() {
    let bless = std::env::var_os("FERRITE_BLESS_FIXTURES").is_some();
    let samples = samples();
    let mut missing = Vec::new();
    for op in Method::OPERATIONS {
        let sample = samples
            .iter()
            .find(|sample| sample.method.name() == op.name)
            .unwrap_or_else(|| panic!("no sample for {}", op.name));
        let path = fixtures().join(format!("{}.json", fixture_name(sample)));
        let encoded = protocol::encode(sample);
        let Ok(frozen) = std::fs::read(&path) else {
            if bless {
                std::fs::write(&path, &encoded).unwrap();
            } else {
                missing.push(op.name);
            }
            continue;
        };
        assert_eq!(
            String::from_utf8_lossy(&encoded),
            String::from_utf8_lossy(&frozen),
            "{}: this build no longer encodes the frozen request",
            op.name
        );
        assert_eq!(
            decode(frozen.strip_suffix(b"\n").unwrap()).as_ref(),
            Ok(sample),
            "{}",
            op.name
        );
    }
    assert!(
        missing.is_empty(),
        "no fixture for {missing:?}: run with FERRITE_BLESS_FIXTURES=1"
    );
}

/// A fixture outlives its sample: every one on disk still decodes.
#[test]
fn every_frozen_request_still_decodes() {
    for entry in std::fs::read_dir(fixtures()).unwrap() {
        let path = entry.unwrap().path();
        let frozen = std::fs::read(&path).unwrap();
        let request = decode(frozen.strip_suffix(b"\n").unwrap());
        assert!(request.is_ok(), "{path:?}: {request:?}");
    }
}

/// What comes back is frozen too: an answer, a refusal and an event.
#[test]
fn answers_refusals_and_events_keep_their_shape() {
    let info = ThreadInfo {
        thread: thread(),
        title: Some("reviewer".into()),
        provider: Provider::Codex,
        attention: Attention::Done,
        cwd: Some(PathBuf::from("/work/app")),
        focused: true,
        project: Some("3".into()),
        provider_session: Some("4f2a".into()),
        attention_seq: 7,
        completion_seq: Some(6),
        boot: "1f-2a".into(),
        held_prompt: false,
        queued_prompts: 1,
        draft: true,
        worktree: Some(PathBuf::from("/work/.worktrees/app/invoices")),
        branch: Some("hp/billing/invoices".into()),
    };
    let answer = Response::Result {
        id: "1".into(),
        result: json!({ "thread": info }),
    };
    assert_eq!(
        serde_json::to_value(&answer).unwrap(),
        json!({"id": "1", "result": {"thread": {
            "thread": 12,
            "title": "reviewer",
            "provider": "codex",
            "attention": "done",
            "cwd": "/work/app",
            "focused": true,
            "project": "3",
            "provider_session": "4f2a",
            "attention_seq": 7,
            "completion_seq": 6,
            "boot": "1f-2a",
            "held_prompt": false,
            "queued_prompts": 1,
            "draft": true,
            "worktree": "/work/.worktrees/app/invoices",
            "branch": "hp/billing/invoices",
        }}})
    );
    let not_ready = Response::Error {
        id: "1".into(),
        error: ApiError::new(Code::AgentNotReady, "blocked").on(thread()),
    };
    assert_eq!(
        serde_json::to_value(&not_ready).unwrap(),
        json!({"id": "1", "error": {"code": "agent_not_ready", "message": "blocked", "thread": 12}})
    );
    let project = ProjectInfo {
        id: "3".into(),
        label: "billing".into(),
        focused: false,
        thread_count: 2,
        worktree: Some(ProjectCheckout {
            repo_key: PathBuf::from("/work/app"),
            checkout_path: PathBuf::from("/work/app"),
            is_linked_worktree: false,
        }),
    };
    assert_eq!(
        serde_json::to_value(&project).unwrap(),
        json!({
            "id": "3",
            "label": "billing",
            "focused": false,
            "thread_count": 2,
            "worktree": {
                "repo_key": "/work/app",
                "checkout_path": "/work/app",
                "is_linked_worktree": false,
            },
        })
    );
    let refusal = Response::Error {
        id: "1".into(),
        error: ApiError::new(Code::AgentPromptStalled, "no work"),
    };
    assert_eq!(
        serde_json::to_value(&refusal).unwrap(),
        json!({"id": "1", "error": {"code": "agent_prompt_stalled", "message": "no work"}})
    );
    let events = [
        (
            Event::Attention {
                thread: thread(),
                attention: Attention::Working,
            },
            json!({"event": "attention", "thread": 12, "attention": "working"}),
        ),
        (
            Event::ThreadClosed { thread: thread() },
            json!({"event": "thread_closed", "thread": 12}),
        ),
        (
            Event::DecisionClosed {
                thread: thread(),
                handle: WireHandle {
                    generation: 2,
                    serial: 5,
                    request_id: "perm_01".into(),
                },
                by: ClosedBy::Answered { client: 1 << 32 },
            },
            json!({
                "event": "decision_closed",
                "thread": 12,
                "handle": {"generation": 2, "serial": 5, "request_id": "perm_01"},
                "by": {"kind": "answered", "client": 4_294_967_296_u64},
            }),
        ),
    ];
    for (event, shape) in events {
        let told = Response::Event {
            id: "7".into(),
            event,
        };
        assert_eq!(
            serde_json::to_value(&told).unwrap(),
            json!({"id": "7", "event": shape})
        );
    }
}

#[test]
fn a_line_that_is_not_a_request_is_answered_with_why() {
    let unknown = decode(br#"{"id":"9","method":"agent.dance","params":{}}"#);
    assert!(matches!(
        unknown,
        Err(Response::Error { ref id, ref error }) if id == "9" && error.code == Code::UnknownMethod
    ));
    let wrong = decode(br#"{"id":"4","method":"agent.prompt","params":{"thread":"x"}}"#);
    assert!(matches!(
        wrong,
        Err(Response::Error { ref id, ref error }) if id == "4" && error.code == Code::InvalidRequest
    ));
    let numbered = decode(br#"{"id":7,"method":"thread.list","params":{}}"#);
    assert!(matches!(
        numbered,
        Err(Response::Error { ref id, ref error }) if id == "7" && error.code == Code::InvalidRequest
    ));
    let garbage = decode(b"not json");
    assert!(matches!(
        garbage,
        Err(Response::Error { ref id, ref error }) if id.is_empty() && error.code == Code::InvalidRequest
    ));
}

// ---------------------------------------------------------------- the CLI

fn op(name: &str) -> &'static Operation {
    Method::OPERATIONS
        .iter()
        .find(|op| op.name == name)
        .unwrap()
}

fn context() -> Context {
    Context {
        thread: Some("7".into()),
        cwd: Some(PathBuf::from("/work/app")),
    }
}

fn parse(name: &str, args: &[&str]) -> Result<Method, String> {
    cli::parse(op(name), args, &context()).map(|request| request.method)
}

#[test]
fn positionals_come_in_their_declared_order_and_flags_by_their_type() {
    assert_eq!(
        parse(
            "agent.prompt",
            &["12", "Review the diff", "--wait", "--timeout-ms", "5000"]
        ),
        Ok(Method::AgentPrompt(AgentPromptParams {
            thread: thread(),
            text: "Review the diff".into(),
            wait: true,
            timeout_ms: Some(5000),
            until: Vec::new(),
        }))
    );
    assert_eq!(
        parse("agent.wait", &["12", "--until=blocked"]),
        Ok(Method::AgentWait(AgentWaitParams {
            thread: thread(),
            until: Some(Attention::Blocked),
            timeout_ms: None,
        }))
    );
    assert_eq!(
        parse("events.subscribe", &["--threads", "12", "--threads", "14"]),
        Ok(Method::EventsSubscribe(EventsSubscribeParams {
            threads: vec![thread(), ThreadId::new(14)],
        }))
    );
    assert_eq!(
        parse(
            "decision.answer",
            &[
                "12",
                "perm_01",
                "--answer",
                r#"{"kind":"deny","message":"not now"}"#
            ]
        ),
        Ok(Method::DecisionAnswer(DecisionAnswerParams {
            thread: thread(),
            decision: "perm_01".into(),
            answer: WireAnswer::Deny {
                message: "not now".into(),
            },
        }))
    );
}

#[test]
fn current_names_the_calling_thread_and_frees_its_position() {
    assert_eq!(
        parse("agent.prompt", &["--current", "status?"]),
        Ok(Method::AgentPrompt(AgentPromptParams {
            thread: ThreadId::new(7),
            text: "status?".into(),
            wait: false,
            timeout_ms: None,
            until: Vec::new(),
        }))
    );
    let outside = Context {
        thread: None,
        ..context()
    };
    let refused = cli::parse(op("thread.get"), &["--current"], &outside).unwrap_err();
    assert!(refused.contains("FERRITE_THREAD_ID"), "{refused}");
    let refused = cli::parse(op("thread.list"), &["--current"], &context()).unwrap_err();
    assert!(refused.contains("takes no Thread"), "{refused}");
}

/// `--machine` is the CLI's own, anywhere before `--`; it never names a
/// Thread with `--current`, whose id is this computer's Store's.
#[test]
fn machine_is_a_global_flag_before_the_arguments() {
    let split = cli::machine_flag;
    assert_eq!(
        split(&["--machine", "box", "thread", "list"]),
        Ok((Some("box"), vec!["thread", "list"]))
    );
    assert_eq!(
        split(&["thread", "get", "3", "--machine=me@box"]),
        Ok((Some("me@box"), vec!["thread", "get", "3"]))
    );
    assert_eq!(
        split(&["agent", "prompt", "3", "--", "--machine", "box"]),
        Ok((None, vec!["agent", "prompt", "3", "--", "--machine", "box"]))
    );
    assert_eq!(
        split(&["agent", "prompt", "--current", "hi"]),
        Ok((None, vec!["agent", "prompt", "--current", "hi"]))
    );
    assert!(split(&["thread", "list", "--machine"]).is_err());
    assert!(split(&["--machine", "a", "--machine", "b", "thread", "list"]).is_err());
    let refused = split(&["--machine", "box", "agent", "read", "--current"]).unwrap_err();
    assert!(refused.contains("by id"), "{refused}");
    assert_eq!(
        split(&[
            "--machine",
            "box",
            "agent",
            "prompt",
            "3",
            "--",
            "--current"
        ]),
        Ok((Some("box"), vec!["agent", "prompt", "3", "--", "--current"]))
    );
}

/// A machine is found among the saved ones by name or ssh target; one
/// unknown is a wrong command line, one turned off is refused, and neither
/// is dialed.
#[test]
fn machine_names_a_saved_enabled_machine() {
    let dir = std::env::temp_dir().join(format!("ferrite-cli-machines-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("settings.json"),
        r#"{"machines":[{"name":"box","target":"me@box","enabled":false}]}"#,
    )
    .unwrap();
    let run = |words: &[&str]| {
        let args: Vec<String> = std::iter::once("ferrite")
            .chain(words.iter().copied())
            .map(String::from)
            .collect();
        cli::run(&args, &dir.join("threads"), &dir)
    };
    assert_eq!(run(&["--machine", "elsewhere", "thread", "list"]), Some(2));
    assert_eq!(run(&["--machine", "box", "thread", "list"]), Some(1));
    assert_eq!(run(&["--machine", "me@box", "thread", "list"]), Some(1));
    assert_eq!(run(&["--machine", "box", "--skill"]), Some(2));
    let _ = std::fs::remove_dir_all(&dir);
}

/// After `--` nothing is a flag: a prompt may start with dashes.
#[test]
fn after_a_double_dash_every_word_is_an_argument() {
    assert_eq!(
        parse("agent.prompt", &["--wait", "12", "--", "--dry-run first"]),
        Ok(Method::AgentPrompt(AgentPromptParams {
            thread: thread(),
            text: "--dry-run first".into(),
            wait: true,
            timeout_ms: None,
            until: Vec::new(),
        }))
    );
    let refused = parse("agent.prompt", &["12", "--dry-run first"]).unwrap_err();
    assert!(refused.contains("takes no --dry-run first"), "{refused}");
}

#[test]
fn a_thread_starts_where_the_cli_runs_unless_told() {
    let started = |args: &[&str]| match parse("thread.start", args) {
        Ok(Method::ThreadStart(params)) => params,
        other => panic!("{other:?}"),
    };
    assert_eq!(started(&[]).cwd, PathBuf::from("/work/app"));
    assert_eq!(
        started(&["--cwd", "web"]).cwd,
        PathBuf::from("/work/app/web")
    );
    assert_eq!(
        started(&["--cwd", "/elsewhere"]).cwd,
        PathBuf::from("/elsewhere")
    );
    let reviewer = started(&["--provider", "codex", "--worktree", "--title", "reviewer"]);
    assert_eq!(reviewer.provider, Some(Provider::Codex));
    assert!(reviewer.worktree);
    assert_eq!(reviewer.title.as_deref(), Some("reviewer"));
}

/// A worktree is named by its directory, read from where the CLI runs, as
/// Herdr's `worktree remove` names its Space.
#[test]
fn a_worktree_is_removed_by_its_directory() {
    let removed = |args: &[&str]| match parse("worktree.remove", args) {
        Ok(Method::WorktreeRemove(params)) => params.path,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        removed(&["../t-0001"]),
        PathBuf::from("/work/app/../t-0001")
    );
    assert_eq!(
        removed(&["--path", "/wt/t-0001"]),
        PathBuf::from("/wt/t-0001")
    );
    match parse(
        "worktree.create",
        &["--branch", "hp/demo/t-0001", "--base", "main"],
    ) {
        Ok(Method::WorktreeCreate(params)) => {
            assert_eq!(params.cwd, PathBuf::from("/work/app"));
            assert_eq!(params.branch, "hp/demo/t-0001");
            assert_eq!(params.base.as_deref(), Some("main"));
        }
        other => panic!("{other:?}"),
    }
}

/// On a machine this computer's directory means nothing: a `cwd` is the
/// machine's own absolute path, and never left out.
#[test]
fn on_a_machine_a_cwd_is_given_whole() {
    let remote = Context {
        cwd: None,
        ..context()
    };
    let start = |name: &str, args: &[&str]| cli::parse(op(name), args, &remote);
    match start("thread.start", &["--cwd", "/srv/app"]).map(|request| request.method) {
        Ok(Method::ThreadStart(params)) => assert_eq!(params.cwd, PathBuf::from("/srv/app")),
        other => panic!("{other:?}"),
    }
    for (name, args) in [
        ("thread.start", &[][..]),
        ("thread.start", &["--cwd", "app"][..]),
    ] {
        let refused = start(name, args).unwrap_err();
        assert!(
            refused.contains("absolute path there"),
            "{name} {args:?}: {refused}"
        );
    }
    assert!(start("thread.list", &[]).is_ok(), "no cwd, none asked");
}

#[test]
fn a_wrong_command_line_says_what_is_wrong() {
    for (name, args, says) in [
        ("thread.get", &["12", "--wait"][..], "takes no --wait"),
        ("thread.get", &["12", "13"][..], "takes no more arguments"),
        ("thread.get", &["twelve"][..], "not a whole number"),
        ("thread.get", &[][..], "thread"),
        ("agent.wait", &["12", "--until", "soon"][..], "soon"),
        ("agent.read", &["12", "--lines"][..], "needs a value"),
    ] {
        let refused = parse(name, args).unwrap_err();
        assert!(refused.contains(says), "{name} {args:?}: {refused}");
    }
}

#[test]
fn help_lists_every_command_with_its_arguments() {
    let help = cli::usage(None);
    for op in Method::OPERATIONS {
        let (group, verb) = op.words();
        assert!(
            help.contains(&format!("ferrite {group} {verb}")),
            "{}",
            op.name
        );
        assert!(help.contains(op.summary), "{}", op.name);
    }
    assert!(help.contains("<thread> <text>"));
    assert!(help.contains("[--wait]"));
    assert!(help.contains("[--timeout-ms <n>]"));
    assert!(help.contains("[--until <idle|working|done|blocked>]"));
    assert!(help.contains("--answer <json>"));
    assert!(help.contains("--machine <name>"));
    let agent = cli::usage(Some("agent"));
    assert!(agent.contains("ferrite agent wait") && !agent.contains("ferrite thread"));
}

#[test]
fn the_cli_leaves_the_apps_own_arguments_alone() {
    let args = |words: &[&str]| -> Vec<String> {
        std::iter::once("ferrite")
            .chain(words.iter().copied())
            .map(String::from)
            .collect()
    };
    let store = Path::new("/nonexistent");
    for app in [
        &[][..],
        &["--demo"][..],
        &["--import", "a.jsonl"][..],
        &["server"][..],
    ] {
        assert_eq!(cli::run(&args(app), store, store), None, "{app:?}");
    }
    assert_eq!(cli::run(&args(&["thread"]), store, store), Some(0));
    assert_eq!(cli::run(&args(&["thread", "dance"]), store, store), Some(2));
}

#[test]
fn the_bundled_skill_names_the_safety_rules() {
    assert!(cli::SKILL.starts_with("---\nname: ferrite\n"));
    for rule in [
        "Never answer another agent's Decision without the user",
        "does not prove the prompt was not delivered",
        "Never resend a prompt blindly",
    ] {
        assert!(cli::SKILL.contains(rule), "{rule}");
    }
    for op in Method::OPERATIONS {
        let (group, verb) = op.words();
        assert!(
            cli::SKILL.contains(&format!("\"$F\" {group} {verb}")),
            "the skill never shows `{}`",
            op.name
        );
    }
}

/// `ferrite --version` reads as Herdr's does: the name, then the version.
#[test]
fn the_version_is_the_name_then_semver() {
    let version = cli::version();
    let semver = version.strip_prefix("ferrite ").unwrap();
    let parts: Vec<&str> = semver
        .split(['-', '+'])
        .next()
        .unwrap()
        .split('.')
        .collect();
    assert_eq!(parts.len(), 3, "{version}");
    assert!(
        parts.iter().all(|part| part.parse::<u64>().is_ok()),
        "{version}"
    );
}

/// What herdr-projects asks with: a Thread's start, its prompt confirmed
/// working or blocked, a Project by id and label.
#[test]
fn herdr_projects_calls_read_as_their_parameters() {
    match parse(
        "thread.start",
        &[
            "--worktree",
            "--branch",
            "hp/demo/t-0001",
            "--base",
            "main",
            "--project",
            "3",
            "--ready",
            "--timeout-ms",
            "20000",
            "--focus",
            "--args",
            "--model",
            "--args",
            "opus",
        ],
    ) {
        Ok(Method::ThreadStart(params)) => {
            assert_eq!(params.branch.as_deref(), Some("hp/demo/t-0001"));
            assert_eq!(params.base.as_deref(), Some("main"));
            assert_eq!(params.project.as_deref(), Some("3"));
            assert!(params.ready && params.focus && params.worktree);
            assert_eq!(params.timeout_ms, Some(20_000));
            assert_eq!(params.args, ["--model", "opus"]);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        parse(
            "agent.prompt",
            &["12", "go", "--until", "working", "--until", "blocked"]
        ),
        Ok(Method::AgentPrompt(AgentPromptParams {
            thread: thread(),
            text: "go".into(),
            wait: false,
            timeout_ms: None,
            until: vec![Attention::Working, Attention::Blocked],
        }))
    );
    assert_eq!(
        parse("project.rename", &["3", "billing"]),
        Ok(Method::ProjectRename(ProjectRenameParams {
            project: "3".into(),
            label: "billing".into(),
        }))
    );
    match parse(
        "thread.start",
        &[
            "--env",
            r#"{"HERDR_PROJECTS_DATA_DIR":"/data"}"#,
            "--launcher",
            "bwrap",
            "--launcher",
            "--ro-bind",
            "--launcher",
            "--",
        ],
    ) {
        Ok(Method::ThreadStart(params)) => {
            assert_eq!(params.env["HERDR_PROJECTS_DATA_DIR"], "/data");
            assert_eq!(params.launcher, ["bwrap", "--ro-bind", "--"]);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        parse("thread.revive", &["12", "--resume", "4f2a", "--ready"]),
        Ok(Method::ThreadRevive(ThreadReviveParams {
            thread: thread(),
            args: Vec::new(),
            resume: Some("4f2a".into()),
            env: Default::default(),
            launcher: Vec::new(),
            ready: true,
            timeout_ms: None,
        }))
    );
    assert_eq!(
        parse("project.focus", &["3"]),
        Ok(Method::ProjectFocus(ProjectParams {
            project: "3".into()
        }))
    );
    assert_eq!(
        parse(
            "notification.show",
            &["Review ready", "--sound", "done", "--important"]
        ),
        Ok(Method::NotificationShow(NotificationShowParams {
            title: "Review ready".into(),
            body: None,
            sound: WireSound::Done,
            important: true,
        }))
    );
    match parse("project.open", &["--label", "demo"]) {
        Ok(Method::ProjectOpen(params)) => {
            assert_eq!(params.cwd, PathBuf::from("/work/app"));
            assert_eq!(params.label.as_deref(), Some("demo"));
        }
        other => panic!("{other:?}"),
    }
}

/// Herdr's delivery confirmation: a prompt's wait `until` working or
/// blocked answers as soon as the agent is seen so, never on the idle it
/// was sent from, and stalls as any prompt's wait does.
#[test]
fn a_prompts_wait_until_working_answers_once_the_prompt_is_taken() {
    let start = Instant::now();
    let until = || Until::AnyOf(vec![Attention::Working, Attention::Blocked]);
    let mut wait = Wait::after_prompt_until(until(), None, start);
    assert_eq!(wait.observe(seen(Attention::Idle), start), None);
    assert_eq!(
        wait.observe(seen(Attention::Working), at(start, 100)),
        Some(Settled::Reached)
    );

    let mut wait = Wait::after_prompt_until(until(), None, start);
    assert_eq!(
        wait.observe(seen(Attention::Blocked), at(start, 100)),
        Some(Settled::Reached)
    );

    let mut wait = Wait::after_prompt_until(until(), None, start);
    assert_eq!(wait.observe(seen(Attention::Idle), start), None);
    assert_eq!(
        wait.observe(seen(Attention::Idle), at(start, 5_000)),
        Some(Settled::Stalled)
    );
}

/// `thread.start --ready`, as Herdr's `agent start`: answered once the
/// Session is up and idle or done; a blocked agent (a trust dialog) or one
/// not up in time is not ready.
#[test]
fn a_ready_wait_answers_once_the_agent_is_up_and_at_rest() {
    let start = Instant::now();
    let ready = || Wait::until(Until::Ready, Some(Duration::from_secs(20)), start);

    let mut wait = ready();
    assert_eq!(wait.observe(starting(), start), None);
    assert_eq!(wait.observe(seen(Attention::Working), at(start, 100)), None);
    assert_eq!(
        wait.observe(seen(Attention::Idle), at(start, 200)),
        Some(Settled::Reached)
    );

    let mut wait = ready();
    assert_eq!(
        wait.observe(seen(Attention::Blocked), at(start, 100)),
        Some(Settled::NotReady)
    );

    let mut wait = ready();
    assert_eq!(wait.observe(starting(), at(start, 19_999)), None);
    assert_eq!(
        wait.observe(starting(), at(start, 20_000)),
        Some(Settled::NotReady)
    );
}
