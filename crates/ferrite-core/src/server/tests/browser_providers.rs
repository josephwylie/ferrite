//! Real provider protocol adapters into the public Conversation browser channel.
use super::*;
use crate::providers::{ClaudeConfig, ClaudeSession};
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;

fn stub(dir: &Path, script: &str) -> String {
    let path = dir.join("provider");
    std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.display().to_string()
}

fn lines_until(path: &Path, done: impl Fn(&Value) -> bool) -> Vec<Value> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let lines: Vec<Value> = std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        if lines.iter().any(&done) {
            return lines;
        }
        assert!(
            Instant::now() < deadline,
            "provider never answered: {lines:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn claude_browser_opens_the_bound_threads_tab_with_visuals_off() {
    let harness = start("claude-browser-provider");
    let mut window = attach(&harness.address);
    let (thread, _provider) = open(&mut window, &harness);
    let _browser_window = browser_test_window(&harness, thread);
    let host = harness.dir.join("claude-host.jsonl");
    let call = json!({"type":"control_request","request_id":"browser-open","request":{
        "subtype":"mcp_message","server_name":"ferrite","message":{
            "jsonrpc":"2.0","id":8,"method":"tools/call","params":{
                "name":"browser","arguments":{"kind":"action","action":{
                    "kind":"open","url":"https://example.com/provider-owned"
                }}
            }
        }
    }});
    let program = stub(&harness.dir, &format!(
        "case \"$1\" in --version) echo '2.1.292 (Claude Code)'; exit 0;; esac\n\
         echo '{{\"type\":\"control_response\",\"response\":{{\"subtype\":\"success\",\"request_id\":\"req_1\",\"response\":{{}}}}}}'\n\
         echo '{call}'\nexec cat > '{}'", host.display()));
    let session = ClaudeSession::spawn(ClaudeConfig {
        program,
        browser: Some(harness.browser.for_thread(thread)),
        ..Default::default()
    })
    .unwrap();
    let lines = lines_until(&host, |line| {
        line["response"]["request_id"] == "browser-open"
    });
    let answer = lines
        .iter()
        .find(|line| line["response"]["request_id"] == "browser-open")
        .unwrap();
    let result = &answer["response"]["response"]["mcp_response"]["result"];
    assert_eq!(result["isError"], false, "{answer}");
    let value: Value =
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(
        value["state"]["tabs"][0]["url"],
        "https://example.com/provider-owned"
    );
    drop(session);
    window.command(99, Op::Stop { when: When::Now });
    assert_eq!(harness.server.join().unwrap(), 0);
}

#[test]
fn codex_browser_opens_the_bound_threads_tab_with_visuals_off() {
    use crate::providers::{CodexConfig, CodexSession};
    let harness = start("codex-browser-provider");
    let mut window = attach(&harness.address);
    let (thread, _provider) = open(&mut window, &harness);
    let _browser_window = browser_test_window(&harness, thread);
    let host = harness.dir.join("codex-host.jsonl");
    let call = json!({"jsonrpc":"2.0","id":8,"method":"item/tool/call","params":{
        "threadId":"native-thread", "namespace":"ferrite","tool":"browser","callId":"browser-open",
        "arguments":{"kind":"action","action":{
            "kind":"open","url":"https://example.com/provider-owned"
        }}
    }});
    let program = stub(&harness.dir, &format!(
        "case \"$1\" in --version) echo 'codex-cli 0.167.0'; exit 0;; esac\n\
         echo '{{\"id\":1,\"result\":{{}}}}'\n\
         echo '{{\"id\":3,\"result\":{{\"data\":[]}}}}'\n\
         echo '{{\"id\":2,\"result\":{{\"thread\":{{\"id\":\"native-thread\"}},\"model\":\"m\"}}}}'\n\
         echo '{call}'\nexec cat > '{}'", host.display()));
    let session = CodexSession::spawn(CodexConfig {
        program,
        browser: Some(harness.browser.for_thread(thread)),
        ..Default::default()
    })
    .unwrap();
    let lines = lines_until(&host, |line| line["id"] == 8);
    let start = lines
        .iter()
        .find(|line| line["method"] == "thread/start")
        .unwrap();
    assert_eq!(
        start["params"]["dynamicTools"][0]["tools"][0]["name"],
        "browser"
    );
    assert_eq!(
        start["params"]["config"]["tools"]["update_plan"]["enabled"],
        true
    );
    let answer = lines.iter().find(|line| line["id"] == 8).unwrap();
    assert_eq!(answer["result"]["success"], true, "{answer}");
    let value: Value = serde_json::from_str(
        answer["result"]["contentItems"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        value["state"]["tabs"][0]["url"],
        "https://example.com/provider-owned"
    );
    drop(session);
    window.command(99, Op::Stop { when: When::Now });
    assert_eq!(harness.server.join().unwrap(), 0);
}

/// Script only the external CLI transport; browser requests still traverse the
/// bound client, the real Server and its attached window protocol.
fn provider_call(
    harness: &Harness,
    thread: ThreadId,
    codex: bool,
    operation: Value,
    resume: bool,
) -> (Box<dyn crate::providers::Session>, PathBuf) {
    let host = harness.dir.join("provider-host.jsonl");
    let (version, handshake, call) = if codex {
        (
            "codex-cli 0.167.0",
            vec![
                json!({"id":1,"result":{}}),
                json!({"id":3,"result":{"data":[]}}),
                json!({"id":2,"result":{"thread":{"id":"native-thread"},"model":"m"}}),
            ],
            json!({"jsonrpc":"2.0","id":8,"method":"item/tool/call","params":{
                "threadId":"native-thread","namespace":"ferrite","tool":"browser","callId":"browser-call","arguments":operation
            }}),
        )
    } else {
        (
            "2.1.292 (Claude Code)",
            vec![json!({"type":"control_response","response":{
                "subtype":"success","request_id":"req_1","response":{}
            }})],
            json!({"type":"control_request","request_id":"browser-call","request":{
                "subtype":"mcp_message","server_name":"ferrite","message":{
                    "jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"browser","arguments":operation}
                }
            }}),
        )
    };
    let output = handshake
        .into_iter()
        .chain([call])
        .map(|line| format!("echo '{line}'\n"))
        .collect::<String>();
    let program = stub(
        &harness.dir,
        &format!(
            "case \"$1\" in --version) echo '{version}'; exit 0;; esac\n{output}exec cat > '{}'",
            host.display()
        ),
    );
    let browser = Some(harness.browser.for_thread(thread));
    let (renderer, _renders) = super::super::render::remote();
    let visuals = Some(crate::visual::Visuals {
        level: crate::visual::Level::WhenAsked,
        renderer: Arc::new(renderer),
    });
    let session: Box<dyn crate::providers::Session> = if codex {
        Box::new(
            crate::providers::CodexSession::spawn(crate::providers::CodexConfig {
                program,
                browser,
                visuals,
                resume: resume.then(|| "native-thread".into()),
                ..Default::default()
            })
            .unwrap(),
        )
    } else {
        Box::new(
            ClaudeSession::spawn(ClaudeConfig {
                program,
                browser,
                ..Default::default()
            })
            .unwrap(),
        )
    };
    (session, host)
}

fn browser_answer(lines: &[Value], codex: bool) -> &Value {
    if codex {
        &lines.iter().find(|line| line["id"] == 8).unwrap()["result"]
    } else {
        &lines
            .iter()
            .find(|line| line["response"]["request_id"] == "browser-call")
            .unwrap()["response"]["response"]["mcp_response"]["result"]
    }
}

fn call_answered(line: &Value) -> bool {
    line["id"] == 8 || line["response"]["request_id"] == "browser-call"
}

#[test]
fn both_providers_deliver_screenshot_png_content_from_the_bound_live_tab() {
    use crate::browser::{Action, Control, Outcome};
    // A complete one-pixel PNG, identical to the attached window's capture.
    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a0JkAAAAASUVORK5CYII=";
    for codex in [false, true] {
        let harness = start(if codex {
            "codex-browser-png"
        } else {
            "claude-browser-png"
        });
        let mut window = attach(&harness.address);
        let (thread, _provider) = open(&mut window, &harness);
        let mut owner = browser_test_window(&harness, thread);
        let opened = harness
            .browser
            .for_thread(thread)
            .submit(browser_test_action(Action::Open {
                url: "https://example.com/screenshot".into(),
            }))
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        let (session, host) = provider_call(
            &harness,
            thread,
            codex,
            json!({
                "kind":"control","document":opened.state.tabs[0].document(),"revision":opened.revision,
                "control":{"kind":"screenshot"}
            }),
            false,
        );
        browser_test_answer(
            &mut owner,
            Control::Screenshot,
            Outcome::Success {
                value: json!({"png_base64":PNG,"width":1,"height":1}),
            },
        );
        let lines = lines_until(&host, call_answered);
        if codex {
            let start = lines
                .iter()
                .find(|line| line["method"] == "thread/start")
                .unwrap();
            let params = &start["params"];
            assert_eq!(params["dynamicTools"].as_array().unwrap().len(), 1);
            let names: Vec<_> = params["dynamicTools"][0]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .map(|tool| tool["name"].as_str().unwrap())
                .collect();
            assert_eq!(names, ["show_visual", "browser"]);
            assert_eq!(params["config"]["tools"]["update_plan"]["enabled"], true);
            assert_eq!(
                params["config"]["features"]["code_mode"]["direct_only_tool_namespaces"],
                json!(["ferrite"])
            );
        }
        let answer = browser_answer(&lines, codex);
        let content = if codex {
            &answer["contentItems"]
        } else {
            &answer["content"]
        };
        assert_eq!(content.as_array().unwrap().len(), 2, "{answer}");
        if codex {
            assert_eq!(content[1]["type"], "inputImage");
            assert_eq!(
                content[1]["imageUrl"],
                format!("data:image/png;base64,{PNG}")
            );
        } else {
            assert_eq!(content[1]["type"], "image");
            assert_eq!(content[1]["mimeType"], "image/png");
            assert_eq!(content[1]["data"], PNG);
        }
        let text: Value = serde_json::from_str(content[0]["text"].as_str().unwrap()).unwrap();
        assert!(
            text["state"].get("owner").is_none(),
            "window capability must remain private"
        );
        assert!(
            text["value"].get("png_base64").is_none(),
            "image must reach native content"
        );
        drop(session);
        window.command(99, Op::Stop { when: When::Now });
        assert_eq!(harness.server.join().unwrap(), 0);
    }
}

#[test]
fn both_providers_reject_page_supplied_thread_routing_without_opening_a_tab() {
    for codex in [false, true] {
        let harness = start(if codex {
            "codex-browser-routing"
        } else {
            "claude-browser-routing"
        });
        let mut window = attach(&harness.address);
        let (thread, _provider) = open(&mut window, &harness);
        let _owner = browser_test_window(&harness, thread);
        let (session, host) = provider_call(
            &harness,
            thread,
            codex,
            json!({
                "kind":"action","thread":999,"action":{"kind":"open","url":"https://example.com/forbidden"}
            }),
            false,
        );
        let lines = lines_until(&host, call_answered);
        let answer = browser_answer(&lines, codex);
        let content = if codex {
            assert_eq!(answer["success"], false);
            &answer["contentItems"]
        } else {
            assert_eq!(answer["isError"], true);
            &answer["content"]
        };
        assert!(
            content[0]["text"]
                .as_str()
                .unwrap()
                .contains("Unknown browser field: thread"),
            "{answer}"
        );
        let state = harness
            .browser
            .for_thread(thread)
            .submit(crate::browser::Operation::State)
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert!(
            state.state.tabs.is_empty(),
            "rejected routing must not create a tab"
        );
        drop(session);
        window.command(99, Op::Stop { when: When::Now });
        assert_eq!(harness.server.join().unwrap(), 0);
    }
}

#[test]
fn resumed_codex_browser_call_preserves_history_and_explains_legacy_tool_absence() {
    let harness = start("codex-browser-resume");
    let mut window = attach(&harness.address);
    let (thread, _provider) = open(&mut window, &harness);
    let _owner = browser_test_window(&harness, thread);
    let (session, host) = provider_call(&harness, thread, true, json!({"kind":"state"}), true);
    let lines = lines_until(&host, call_answered);
    let resume = lines
        .iter()
        .find(|line| line["method"] == "thread/resume")
        .unwrap();
    assert_eq!(resume["params"]["threadId"], "native-thread");
    assert!(
        resume["params"].get("dynamicTools").is_none(),
        "resume has no dynamic tool replacement API"
    );
    let instructions = resume["params"]["developerInstructions"].as_str().unwrap();
    assert!(instructions.contains(crate::update_plan::GUIDANCE));
    assert!(instructions.contains("If ferrite.browser is absent"));
    assert!(instructions.contains("start a new Thread"));
    assert_eq!(
        resume["params"]["config"]["tools"]["update_plan"]["enabled"],
        true
    );
    assert_eq!(
        browser_answer(&lines, true)["success"],
        true,
        "a previously capable Thread still dispatches"
    );
    drop(session);
    window.command(99, Op::Stop { when: When::Now });
    assert_eq!(harness.server.join().unwrap(), 0);
}

#[test]
fn both_providers_report_unavailable_when_no_browser_window_is_attached() {
    for codex in [false, true] {
        let harness = start(if codex {
            "codex-browser-unavailable"
        } else {
            "claude-browser-unavailable"
        });
        let mut window = attach(&harness.address);
        let (thread, _provider) = open(&mut window, &harness);
        let (session, host) =
            provider_call(&harness, thread, codex, json!({"kind":"state"}), false);
        let lines = lines_until(&host, call_answered);
        let answer = browser_answer(&lines, codex);
        let content = if codex {
            assert_eq!(answer["success"], false);
            &answer["contentItems"]
        } else {
            assert_eq!(answer["isError"], true);
            &answer["content"]
        };
        assert!(
            content[0]["text"]
                .as_str()
                .unwrap()
                .contains("Conversation browser unavailable"),
            "{answer}"
        );
        drop(session);
        window.command(99, Op::Stop { when: When::Now });
        assert_eq!(harness.server.join().unwrap(), 0);
    }
}
