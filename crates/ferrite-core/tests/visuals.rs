//! `show_visual` end to end against stub CLIs replaying the real captures:
//! the tool offered in-band, its call answered with text and a screenshot,
//! its input drafted while it streams, and the Visual it folds into — live,
//! persisted, and reloaded. The renderer is a fake: the app's browser is the
//! other adapter at the same seam.
//!
//! Captured from the real CLIs (claude 2.1.292, codex 0.160.1) by the
//! throwaway harness described in the fixtures' notes; the model in each
//! named the colour of a solid PNG it was sent, so the image provably
//! reached it.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use ferrite_core::activity::{Activity, ActivityEvent, ActivityInput, ExecutionEvent};
use ferrite_core::providers::{ClaudeConfig, ClaudeSession, CodexConfig, CodexSession};
use ferrite_core::store::{Provider, Store};
use ferrite_core::transcript::{Body, Input};
use ferrite_core::visual::{
    self, Level, RenderRequest, Rendered, Renderer, Reply, Status, Visual, Visuals,
};
use ferrite_core::workspace::WorkspaceBinding;
use ferrite_core::SessionEvent;

/// A tiny PNG's bytes; what matters is that exactly these reach the wire.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\nferrite-fake";

fn dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ferrite-visuals-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{name}"))
}

fn stub(name: &str, script: &str) -> String {
    let path = dir().join(name);
    fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path.display().to_string()
}

/// The fake adapter: records what it was asked and answers as scripted.
struct FakeRenderer {
    requests: Mutex<Vec<RenderRequest>>,
    answer: Result<Rendered, String>,
}

impl Renderer for FakeRenderer {
    fn render(&self, request: RenderRequest, reply: Reply) {
        self.requests.lock().unwrap().push(request);
        let answer = self.answer.clone();
        // Never on the caller's thread: that is the provider's reader.
        std::thread::spawn(move || reply.send(answer));
    }
}

fn renderer(answer: Result<Rendered, String>) -> Arc<FakeRenderer> {
    Arc::new(FakeRenderer {
        requests: Mutex::default(),
        answer,
    })
}

fn shown() -> Result<Rendered, String> {
    Ok(Rendered {
        png: PNG.to_vec(),
        width: 720,
        height: 240,
    })
}

fn visuals(renderer: &Arc<FakeRenderer>, level: Level) -> Option<Visuals> {
    Some(Visuals {
        level,
        renderer: renderer.clone(),
    })
}

fn drain(events: &Receiver<SessionEvent>) -> Vec<SessionEvent> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut drained = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let Ok(event) = events.recv_timeout(left) else {
            return drained;
        };
        let last = matches!(
            event,
            SessionEvent::TurnEnded { .. } | SessionEvent::Closed { .. }
        );
        drained.push(event);
        if last {
            return drained;
        }
    }
}

fn fold(events: &[SessionEvent]) -> Activity {
    let mut activity = Activity::default();
    activity.apply(ActivityInput::Connect { generation: 1 });
    for event in events.iter().cloned() {
        live(&mut activity, event);
    }
    activity
}

fn live(activity: &mut Activity, event: SessionEvent) {
    let at = Instant::now();
    activity.apply(match event {
        SessionEvent::Activity(event) => ActivityInput::Observe {
            generation: 1,
            event,
            at,
        },
        event => ActivityInput::Main {
            input: Input::Event(event),
            at,
        },
    });
}

fn visuals_of(activity: &Activity) -> Vec<Visual> {
    activity
        .view()
        .main()
        .transcript()
        .blocks()
        .iter()
        .filter_map(|block| match &block.body {
            Body::Visual(visual) => Some(visual.clone()),
            _ => None,
        })
        .collect()
}

/// The JSON lines the host wrote, once `done` says they are all there.
fn host_lines(path: &Path, done: impl Fn(&[Value]) -> bool) -> Vec<Value> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let lines: Vec<Value> = fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        if done(&lines) || Instant::now() > deadline {
            return lines;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The response the host sent to the CLI's control request `id`.
fn answer_to<'a>(lines: &'a [Value], id: &str) -> Option<&'a Value> {
    lines
        .iter()
        .find(|line| line["type"] == "control_response" && line["response"]["request_id"] == id)
}

fn request_id(fixture_name: &str, method: &str) -> String {
    fs::read_to_string(fixture(fixture_name))
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|line| line["request"]["message"]["method"] == method)
        .and_then(|line| line["request_id"].as_str().map(str::to_owned))
        .unwrap_or_else(|| panic!("{fixture_name} has no {method}"))
}

const CLAUDE: &str = "claude-show-visual-2.1.292.jsonl";
const CODEX: &str = "codex-show-visual-0.160.1.jsonl";

/// A Claude Session on the replayed capture, its argv and its writes kept.
fn claude(name: &str, visuals: Option<Visuals>) -> (ClaudeSession, PathBuf, PathBuf) {
    let host = dir().join(format!("{name}.host.jsonl"));
    let argv = dir().join(format!("{name}.argv"));
    let _ = fs::remove_file(&host);
    let program = stub(
        name,
        &format!(
            "case \"$1\" in --version) echo '2.1.292 (Claude Code)'; exit 0;; esac\n\
             echo \"$@\" > '{}'\ncat '{}'\nexec cat > '{}'",
            argv.display(),
            fixture(CLAUDE).display(),
            host.display()
        ),
    );
    let session = ClaudeSession::spawn(ClaudeConfig {
        program,
        visuals,
        ..Default::default()
    })
    .unwrap();
    (session, host, argv)
}

#[test]
fn claude_is_offered_the_tool_in_band_and_its_call_gets_text_and_the_screenshot() {
    let renderer = renderer(shown());
    let (session, host, argv) = claude("claude-offered", visuals(&renderer, Level::Automatic));
    let events = drain(session.events());
    let call = request_id(CLAUDE, "tools/call");
    let lines = host_lines(&host, |lines| answer_to(lines, &call).is_some());

    // Declared in the handshake, pre-allowed on the command line.
    let initialize = lines
        .iter()
        .find(|line| line["request"]["subtype"] == "initialize")
        .unwrap();
    assert_eq!(initialize["request"]["sdkMcpServers"], json!(["ferrite"]));
    let argv = fs::read_to_string(argv).unwrap();
    assert!(
        argv.contains("--allowedTools mcp__ferrite__show_visual"),
        "{argv}"
    );

    // The MCP handshake and the tool list, answered over the control channel.
    let init = answer_to(&lines, &request_id(CLAUDE, "initialize")).unwrap();
    let init = &init["response"]["response"]["mcp_response"];
    assert_eq!(init["id"], 0);
    assert_eq!(init["result"]["capabilities"], json!({"tools": {}}));
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    let ready = answer_to(&lines, &request_id(CLAUDE, "notifications/initialized"));
    assert!(
        ready.is_some(),
        "the notification still gets its control response"
    );
    let list = answer_to(&lines, &request_id(CLAUDE, "tools/list")).unwrap();
    let tool = &list["response"]["response"]["mcp_response"]["result"]["tools"][0];
    assert_eq!(tool["name"], "show_visual");
    assert_eq!(tool["_meta"]["anthropic/alwaysLoad"], true);
    assert_eq!(
        tool["description"].as_str().unwrap(),
        visual::definition(Level::Automatic).unwrap().description
    );

    // The call: rendered from its arguments, answered with text + the PNG.
    assert_eq!(
        *renderer.requests.lock().unwrap(),
        vec![RenderRequest {
            id: call_id_in(CLAUDE),
            title: "Hello".into(),
            html: "<h1>Hello world</h1>".into(),
        }]
    );
    let result = &answer_to(&lines, &call).unwrap()["response"]["response"]["mcp_response"];
    assert_eq!(result["id"], 2);
    assert_eq!(result["result"]["isError"], false);
    let content = result["result"]["content"].as_array().unwrap();
    assert_eq!(content[0]["type"], "text");
    assert!(content[0]["text"].as_str().unwrap().contains("720x240"));
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["mimeType"], "image/png");
    use base64::Engine;
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(content[1]["data"].as_str().unwrap())
            .unwrap(),
        PNG
    );

    // Never a Decision; the result line the model saw reads as text.
    assert!(!events
        .iter()
        .any(|event| matches!(event, SessionEvent::DecisionRequested { .. })));
    let outputs: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::Activity(ActivityEvent::MainContent {
                event: ExecutionEvent::ToolCompleted { output, .. },
                ..
            }) => Some(output.as_str()),
            _ => None,
        })
        .collect();
    // 2.1.292 also appends `[Image: source: <its saved copy>]` as text.
    assert_eq!(outputs.len(), 1);
    assert!(
        outputs[0].starts_with("Shown to the operator. Screenshot attached.\n[image]\n"),
        "{outputs:?}"
    );
    assert!(
        !outputs[0].contains("iVBOR"),
        "the recorded screenshot is not carried into the transcript or the log"
    );
    let shown = visuals_of(&fold(&events));
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(shown[0].title, "Hello");
    assert_eq!(shown[0].html, "<h1>Hello world</h1>");
    assert_eq!(shown[0].status, Status::Shown);
}

/// The id the capture's `tools/call` names, as its `_meta` carries it.
fn call_id_in(name: &str) -> String {
    fs::read_to_string(fixture(name))
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|line| {
            line["request"]["message"]["params"]["_meta"]["claudecode/toolUseId"]
                .as_str()
                .map(str::to_owned)
        })
        .unwrap()
}

#[test]
fn claude_drafts_the_visual_while_its_input_streams() {
    let renderer = renderer(shown());
    let (session, _, _) = claude("claude-drafts", visuals(&renderer, Level::WhenAsked));
    let events = drain(session.events());
    let drafts: Vec<&SessionEvent> = events
        .iter()
        .filter(|event| matches!(event, SessionEvent::ToolDraft { .. }))
        .collect();
    let Some(SessionEvent::ToolDraft { id, name, input }) = drafts.first() else {
        panic!("no draft streamed: {events:?}");
    };
    assert_eq!(id, &call_id_in(CLAUDE));
    assert_eq!(name, visual::CLAUDE_NAME);
    assert_eq!(
        input["title"], "Hello",
        "the first delta's title, read as it stood"
    );

    // Folding stops at the first draft: a Visual already drawing.
    let start = events
        .iter()
        .position(|event| matches!(event, SessionEvent::ToolDraft { .. }))
        .unwrap();
    let drawing = visuals_of(&fold(&events[..=start]));
    assert_eq!(drawing.len(), 1);
    assert_eq!(drawing[0].status, Status::Drawing);
    assert_eq!(drawing[0].title, "Hello");
    // The settled start takes the same row on to Checking.
    let started = events
        .iter()
        .position(|event| match event {
            SessionEvent::Activity(ActivityEvent::MainContent {
                event: ExecutionEvent::ToolStarted { name, .. },
                ..
            }) => visual::is_tool(name),
            _ => false,
        })
        .unwrap();
    let checking = visuals_of(&fold(&events[..=started]));
    assert_eq!(checking.len(), 1);
    assert_eq!(checking[0].status, Status::Checking);
    assert_eq!(checking[0].html, "<h1>Hello world</h1>");
}

#[test]
fn a_failed_render_answers_claude_with_an_error_and_no_image() {
    let renderer = renderer(Err("the browser crashed".into()));
    let (session, host, _) = claude("claude-fails", visuals(&renderer, Level::WhenAsked));
    drain(session.events());
    let call = request_id(CLAUDE, "tools/call");
    let lines = host_lines(&host, |lines| answer_to(lines, &call).is_some());
    let result =
        &answer_to(&lines, &call).unwrap()["response"]["response"]["mcp_response"]["result"];
    assert_eq!(result["isError"], true);
    let content = result["content"].as_array().unwrap();
    assert_eq!(content.len(), 1, "no image: {content:?}");
    assert!(content[0]["text"]
        .as_str()
        .unwrap()
        .contains("the browser crashed"));
}

#[test]
fn claude_without_visuals_declares_and_allows_nothing() {
    for (name, visuals) in [
        ("claude-none", None),
        ("claude-off", visuals(&renderer(shown()), Level::Off)),
    ] {
        let (session, host, argv) = claude(name, visuals);
        drain(session.events());
        let lines = host_lines(&host, |lines| {
            lines
                .iter()
                .any(|line| line["request"]["subtype"] == "initialize")
        });
        drop(session);
        let initialize = lines
            .iter()
            .find(|line| line["request"]["subtype"] == "initialize")
            .unwrap();
        assert!(
            initialize["request"].get("sdkMcpServers").is_none(),
            "{name}"
        );
        assert!(
            !fs::read_to_string(argv).unwrap().contains("--allowedTools"),
            "{name}"
        );
    }
}

/// Pre-allowed on the command line, the CLI should never ask; if it does
/// anyway, Ferrite allows it itself rather than raise a Decision.
#[test]
fn a_permission_request_for_the_tool_is_allowed_without_a_decision() {
    let host = dir().join("claude-ask.host.jsonl");
    let _ = fs::remove_file(&host);
    let ask = json!({"type": "control_request", "request_id": "ask-1", "request": {
        "subtype": "can_use_tool", "tool_name": "mcp__ferrite__show_visual",
        "input": {"title": "t", "html": "<p>"}, "tool_use_id": "toolu_x"}});
    let program = stub(
        "claude-ask",
        &format!(
            "case \"$1\" in --version) echo '2.1.292 (Claude Code)'; exit 0;; esac\n\
             echo '{{\"type\":\"control_response\",\"response\":{{\"subtype\":\"success\",\"request_id\":\"req_1\",\"response\":{{}}}}}}'\n\
             echo '{ask}'\n\
             echo '{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"\"}}'\n\
             exec cat > '{}'",
            host.display()
        ),
    );
    let session = ClaudeSession::spawn(ClaudeConfig {
        program,
        visuals: visuals(&renderer(shown()), Level::WhenAsked),
        ..Default::default()
    })
    .unwrap();
    let events = drain(session.events());
    assert!(!events
        .iter()
        .any(|event| matches!(event, SessionEvent::DecisionRequested { .. })));
    let lines = host_lines(&host, |lines| answer_to(lines, "ask-1").is_some());
    let allowed = &answer_to(&lines, "ask-1").unwrap()["response"]["response"];
    assert_eq!(allowed["behavior"], "allow");
    assert_eq!(
        allowed["updatedInput"],
        json!({"title": "t", "html": "<p>"})
    );
}

/// A Codex Session on the replayed capture, its writes kept.
fn codex(name: &str, visuals: Option<Visuals>) -> (CodexSession, PathBuf) {
    let host = dir().join(format!("{name}.host.jsonl"));
    let _ = fs::remove_file(&host);
    let program = stub(
        name,
        &format!(
            "case \"$1\" in --version) echo 'codex-cli 0.160.1'; exit 0;; esac\n\
             echo '{{\"id\":3,\"result\":{{\"data\":[]}}}}'\n\
             cat '{}'\nexec cat > '{}'",
            fixture(CODEX).display(),
            host.display()
        ),
    );
    let session = CodexSession::spawn(CodexConfig {
        program,
        visuals,
        ..Default::default()
    })
    .unwrap();
    (session, host)
}

#[test]
fn codex_is_offered_a_direct_dynamic_tool_and_its_call_gets_text_and_the_screenshot() {
    let renderer = renderer(shown());
    let (session, host) = codex("codex-offered", visuals(&renderer, Level::WhenAsked));
    let events = drain(session.events());
    let lines = host_lines(&host, |lines| lines.iter().any(|line| line["id"] == 0));

    let start = lines
        .iter()
        .find(|line| line["method"] == "thread/start")
        .unwrap();
    let tools = &start["params"]["dynamicTools"];
    assert_eq!(tools[0]["type"], "namespace");
    assert_eq!(tools[0]["name"], "ferrite");
    assert_eq!(tools[0]["tools"][0]["name"], "show_visual");
    assert_eq!(
        tools[0]["tools"][0]["description"].as_str().unwrap(),
        visual::definition(Level::WhenAsked).unwrap().description
    );
    assert_eq!(
        start["params"]["config"]["features"]["code_mode"]["direct_only_tool_namespaces"],
        json!(["ferrite"])
    );

    assert_eq!(
        renderer.requests.lock().unwrap()[0].html,
        "<h1>Hello world</h1>"
    );
    let answer = lines.iter().find(|line| line["id"] == 0).unwrap();
    assert_eq!(answer["result"]["success"], true);
    let items = answer["result"]["contentItems"].as_array().unwrap();
    assert_eq!(items[0]["type"], "inputText");
    assert_eq!(items[1]["type"], "inputImage");
    use base64::Engine;
    let url = items[1]["imageUrl"].as_str().unwrap();
    let data = url.strip_prefix("data:image/png;base64,").unwrap();
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .unwrap(),
        PNG
    );

    assert!(!events
        .iter()
        .any(|event| matches!(event, SessionEvent::DecisionRequested { .. })));
    let shown = visuals_of(&fold(&events));
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(shown[0].title, "Hello");
    assert_eq!(shown[0].status, Status::Shown);
}

#[test]
fn a_resumed_codex_thread_keeps_its_tool_and_is_told_to_call_it_directly() {
    let host = dir().join("codex-resume.host.jsonl");
    let _ = fs::remove_file(&host);
    let program = stub(
        "codex-resume",
        &format!(
            "case \"$1\" in --version) echo 'codex-cli 0.160.1'; exit 0;; esac\n\
             echo '{{\"id\":1,\"result\":{{}}}}'\n\
             echo '{{\"id\":3,\"result\":{{\"data\":[]}}}}'\n\
             echo '{{\"id\":2,\"result\":{{\"thread\":{{\"id\":\"t\"}},\"model\":\"m\"}}}}'\n\
             exec cat > '{}'",
            host.display()
        ),
    );
    let session = CodexSession::spawn(CodexConfig {
        program,
        resume: Some("t".into()),
        visuals: visuals(&renderer(shown()), Level::Automatic),
        ..Default::default()
    })
    .unwrap();
    let lines = host_lines(&host, |lines| {
        lines.iter().any(|line| line["method"] == "thread/resume")
    });
    drop(session);
    let resume = lines
        .iter()
        .find(|line| line["method"] == "thread/resume")
        .unwrap();
    assert!(resume["params"].get("dynamicTools").is_none());
    assert_eq!(
        resume["params"]["config"]["features"]["code_mode"]["direct_only_tool_namespaces"],
        json!(["ferrite"])
    );
}

#[test]
fn codex_without_visuals_offers_nothing_and_refuses_a_stray_call() {
    let (session, host) = codex("codex-none", None);
    drain(session.events());
    let lines = host_lines(&host, |lines| lines.iter().any(|line| line["id"] == 0));
    let start = lines
        .iter()
        .find(|line| line["method"] == "thread/start")
        .unwrap();
    assert!(start["params"].get("dynamicTools").is_none());
    assert!(start["params"].get("config").is_none());
    let answer = lines.iter().find(|line| line["id"] == 0).unwrap();
    assert_eq!(answer["result"]["success"], false);
}

/// The Visual a live Session folded is the one a relaunch reads back from
/// the log: drafts are never written, the start and result are.
#[test]
fn a_visual_is_persisted_and_reloads_as_it_was_shown() {
    let renderer = renderer(shown());
    let (session, _, _) = claude("claude-persist", visuals(&renderer, Level::WhenAsked));
    let events = drain(session.events());
    drop(session);

    let store_dir = dir().join("store");
    let _ = fs::remove_dir_all(&store_dir);
    let mut operator_saw = Activity::default();
    operator_saw.apply(ActivityInput::Connect { generation: 1 });
    let thread = {
        let store = Store::open(&store_dir).unwrap();
        let (id, mut writer) = store
            .create(
                Provider::Claude,
                None,
                WorkspaceBinding::Main {
                    checkout: std::env::temp_dir(),
                },
            )
            .unwrap();
        for event in events {
            writer.record_event(&event, None).unwrap();
            live(&mut operator_saw, event);
        }
        id
    };
    let store = Store::open(&store_dir).unwrap();
    let snapshot = store.load(thread).unwrap();
    let mut restored = Activity::default();
    for input in snapshot.activity_inputs() {
        restored.apply(input);
    }
    let reloaded = visuals_of(&restored);
    assert_eq!(reloaded, visuals_of(&operator_saw));
    assert_eq!(reloaded.len(), 1);
    assert_eq!(reloaded[0].status, Status::Shown);
    assert_eq!(reloaded[0].html, "<h1>Hello world</h1>");
}
