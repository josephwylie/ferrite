#![cfg(unix)]
mod tests {
    use ferrite_core::{
        docview::Instruments,
        providers::{ClaudeConfig, ClaudeSession, CodexConfig, CodexSession},
        transcript::{Body, Input, Transcript},
        SessionEvent,
    };
    use serde_json::{json, Value};
    use std::{fs, os::unix::fs::PermissionsExt, sync::mpsc::Receiver, time::Duration};
    fn replay(provider: &str, label: &str, frames: Vec<Value>) -> Vec<SessionEvent> {
        let path = std::env::temp_dir().join(format!(
            "ferrite-progress-{}-{provider}-{label}",
            std::process::id()
        ));
        let mut script = String::from("#!/bin/sh\n");
        if provider == "claude" {
            script.push_str("case \"$1\" in --version) echo '2.1.243 (Claude Code)'; exit 0;; esac\nread -r request\n");
            script.push_str("echo '{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"req_1\",\"response\":{}}}'\n");
        } else {
            script.push_str("case \"$1\" in --version) echo 'codex-cli 0.149.1'; exit 0;; esac\nread -r request\n");
            script.push_str("echo '{\"id\":1,\"result\":{\"userAgent\":\"probe\"}}'\nread -r request\nread -r request\n");
            script.push_str("echo '{\"id\":2,\"result\":{\"thread\":{\"id\":\"root\"},\"model\":\"probe\",\"modelProvider\":\"probe\",\"approvalPolicy\":\"on-request\",\"sandbox\":{\"type\":\"readOnly\"}}}'\n");
        }
        let sentinel = if provider == "claude" {
            json!({"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"SENTINEL"}}})
        } else {
            json!({"method":"item/agentMessage/delta","params":{"delta":"SENTINEL"}})
        };
        for mut frame in frames.into_iter().chain([sentinel]) {
            if provider == "codex"
                && frame["method"].is_string()
                && frame["params"].get("threadId").is_none()
            {
                frame["params"]["threadId"] = json!("root");
            }
            script.push_str("printf '%s\\n' '");
            script.push_str(&frame.to_string().replace('\'', "'\\''"));
            script.push_str("'\n");
        }
        script.push_str("exec cat > /dev/null\n");
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        fn drain(rx: &Receiver<SessionEvent>) -> Vec<SessionEvent> {
            let mut out = vec![];
            loop {
                let e = rx
                    .recv_timeout(Duration::from_secs(3))
                    .expect("no sentinel");
                if matches!(&e,SessionEvent::TextDelta{text} if text=="SENTINEL") {
                    break;
                }
                out.push(e)
            }
            out
        }
        let out = if provider == "claude" {
            let s = ClaudeSession::spawn(ClaudeConfig {
                program: path.display().to_string(),
                ..Default::default()
            })
            .unwrap();
            drain(s.events())
        } else {
            let s = CodexSession::spawn(CodexConfig {
                program: path.display().to_string(),
                ..Default::default()
            })
            .unwrap();
            drain(s.events())
        };
        fs::remove_file(path).unwrap();
        out
    }
    fn fold(events: Vec<SessionEvent>) -> Transcript {
        let mut t = Transcript::default();
        t.apply(Input::Prompt("Investigate progress".into()));
        for e in events {
            t.apply(Input::Event(e));
        }
        t
    }
    #[test]
    fn claude_prose_and_thinking_reach_transcript() {
        let t = fold(replay(
            "claude",
            "control",
            vec![
                json!({"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Checking files."}}}),
                json!({"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"Considering files."}}}),
            ],
        ));
        assert!(t
            .blocks()
            .iter()
            .any(|b| matches!(&b.body, Body::Paragraph { .. })));
        assert!(t
            .blocks()
            .iter()
            .any(|b| matches!(&b.body,Body::Thinking(s) if s.contains("Considering"))));
    }
    #[test]
    fn codex_prose_and_summary_reach_transcript() {
        let t = fold(replay(
            "codex",
            "control",
            vec![
                json!({"method":"item/agentMessage/delta","params":{"delta":"Checking files."}}),
                json!({"method":"item/reasoning/summaryTextDelta","params":{"delta":"Considering files.","summaryIndex":0}}),
            ],
        ));
        assert!(t
            .blocks()
            .iter()
            .any(|b| matches!(&b.body, Body::Paragraph { .. })));
        assert!(t
            .blocks()
            .iter()
            .any(|b| matches!(&b.body,Body::Thinking(s) if s.contains("Considering"))));
    }
    #[test]
    fn codex_mcp_start_is_visible() {
        let t = fold(replay(
            "codex",
            "mcp",
            vec![
                json!({"method":"item/started","params":{"threadId":"root","turnId":"turn","item":{"type":"mcpToolCall","id":"mcp","server":"search","tool":"lookup","arguments":{},"status":"inProgress"}}}),
            ],
        ));
        assert_eq!(
            Instruments::of(&t).running,
            1,
            "MCP running on wire, absent from transcript"
        );
    }
    #[test]
    fn codex_web_search_is_visible() {
        let t = fold(replay(
            "codex",
            "web",
            vec![
                json!({"method":"item/started","params":{"threadId":"root","turnId":"turn","item":{"type":"webSearch","id":"web","query":"provider docs"}}}),
            ],
        ));
        assert_eq!(
            Instruments::of(&t).running,
            1,
            "Web search on wire, absent from transcript"
        );
    }
    #[test]
    fn codex_plan_is_visible() {
        let t = fold(replay(
            "codex",
            "plan",
            vec![
                json!({"method":"turn/plan/updated","params":{"threadId":"root","turnId":"turn","explanation":"Checking adapters","plan":[{"step":"Read files","status":"inProgress"}]}}),
            ],
        ));
        assert!(
            t.todos().is_some(),
            "Provider plan absent from progress instruments"
        );
    }
    #[test]
    fn codex_command_output_is_visible_before_completion() {
        let t = fold(replay(
            "codex",
            "output",
            vec![
                json!({"method":"item/started","params":{"item":{"type":"commandExecution","id":"cmd","command":"tests","status":"inProgress"}}}),
                json!({"method":"item/commandExecution/outputDelta","params":{"itemId":"cmd","delta":"12 tests passed\n","threadId":"root","turnId":"turn"}}),
            ],
        ));
        assert!(
            t.blocks()
                .iter()
                .any(|b| matches!(&b.body,Body::Tool(tool) if tool.output.is_some())),
            "Command output received but unavailable until completion"
        );
    }
    #[test]
    fn claude_progress_ticks_change_visible_state() {
        let t = fold(replay(
            "claude",
            "progress",
            vec![
                json!({"type":"tool_progress","tool_use_id":"bash","tool_name":"Bash","parent_tool_use_id":null,"elapsed_time_seconds":30.0,"session_id":"root","uuid":"tick"}),
            ],
        ));
        assert!(
            t.progress().caption().is_some(),
            "Tool progress tick yields no visible event"
        );
    }
    #[test]
    fn claude_retry_reason_is_visible() {
        let t = fold(replay(
            "claude",
            "retry",
            vec![
                json!({"type":"system","subtype":"api_retry","attempt":1,"max_retries":10,"retry_delay_ms":5000,"error_status":529,"error":"overloaded_error"}),
            ],
        ));
        assert!(
            t.blocks().len() > 1,
            "Retry wait received but operator sees no explanation"
        );
    }
    #[test]
    fn thinking_supplies_l2_activity() {
        let t = fold(vec![SessionEvent::ThinkingDelta {
            text: "Checking adapter protocol".into(),
        }]);
        assert!(
            Instruments::of(&t).activity.is_some(),
            "Live thinking supplies no L2 activity line"
        );
    }

    #[test]
    fn captured_claude_task_progress_is_visible() {
        let raw = include_str!("fixtures/subagents/claude-overlap-2.1.261.jsonl");
        let frame = raw
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .find(|v| v["type"] == "system" && v["subtype"] == "task_progress")
            .expect("real capture contains task progress");
        let t = fold(replay("claude", "captured-task", vec![frame]));
        assert!(
            !t.progress().background().is_empty(),
            "Captured task progress (description, last tool, elapsed usage) disappears"
        );
    }
    #[test]
    fn captured_claude_api_retry_is_visible() {
        let raw = include_str!("fixtures/claude-todo-2.1.243.jsonl");
        let frame = raw
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .find(|v| v["type"] == "system" && v["subtype"] == "api_retry")
            .expect("real capture contains retry");
        let t = fold(replay("claude", "captured-retry", vec![frame]));
        assert!(t.blocks().len() > 1, "Captured API retry disappears");
    }

    #[test]
    fn native_reasoning_items_keep_distinct_headings_and_deduplicate_snapshots() {
        let item = |method: &str, id: &str, summary: Value| json!({"method": method, "params": {"threadId":"root","turnId":"turn","item":{"type":"reasoning","id":id,"summary":summary}}});
        let delta = |id: &str, text: &str| json!({"method":"item/reasoning/summaryTextDelta","params":{"threadId":"root","itemId":id,"summaryIndex":0,"delta":text}});
        let events = replay(
            "codex",
            "headings",
            vec![
                item("item/started", "first", json!([])),
                delta("first", "**Checking fold"),
                delta("first", " call sites**"),
                item(
                    "item/completed",
                    "first",
                    json!(["**Checking fold call sites**"]),
                ),
                item("item/started", "second", json!([])),
                delta("second", "**Compiling all"),
                item(
                    "item/completed",
                    "second",
                    json!(["**Compiling all modules**"]),
                ),
            ],
        );
        let transcript = fold(events.clone());
        let headings: Vec<_> = transcript
            .blocks()
            .iter()
            .filter_map(|b| match &b.body {
                Body::Thinking(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            headings,
            ["**Checking fold call sites**", "**Compiling all modules**"]
        );
        assert_eq!(
            transcript.progress().caption().as_deref(),
            Some("Compiling all modules")
        );
        // Same separation and latest heading after durable coalescing/replay.
        let path =
            std::env::temp_dir().join(format!("ferrite-progress-store-{}", std::process::id()));
        let store = ferrite_core::store::Store::open(&path).unwrap();
        let (id, mut writer) = store
            .create(
                ferrite_core::store::Provider::Codex,
                None,
                ferrite_core::workspace::WorkspaceBinding::Main {
                    checkout: std::env::temp_dir(),
                },
            )
            .unwrap();
        for event in &events {
            writer.record_event(event, None).unwrap();
        }
        writer.flush().unwrap();
        drop(writer);
        let mut restored = Transcript::default();
        for input in store.load(id).unwrap().inputs() {
            restored.apply(input);
        }
        let restored_headings: Vec<_> = restored
            .blocks()
            .iter()
            .filter_map(|b| match &b.body {
                Body::Thinking(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(restored_headings, headings);
        restored.apply(Input::Revived);
        assert_eq!(
            restored.progress().caption(),
            None,
            "history never pretends to be live"
        );
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn native_summary_snapshot_without_deltas_is_still_visible() {
        let t = fold(replay(
            "codex",
            "snapshot",
            vec![
                json!({"method":"item/completed","params":{"item":{"type":"reasoning","id":"r","summary":["**Inspecting adapters**"]}}}),
            ],
        ));
        assert_eq!(
            t.progress().caption().as_deref(),
            Some("Inspecting adapters")
        );
    }

    #[test]
    fn native_claude_receipts_match_task_identity_and_delete_without_legacy_fallback() {
        let receipt = |result: Value| json!({"type":"user","message":{"content":[]},"tool_use_result":result});
        let mut t = fold(replay(
            "claude",
            "task-receipts",
            vec![
                receipt(json!({"task":{"id":"42","subject":"Read adapters"}})),
                receipt(json!({"task":{"id":"93","subject":"Verify UI"}})),
                receipt(json!({"success":true,"taskId":"93","statusChange":{"to":"in_progress"}})),
                receipt(json!({"success":true,"taskId":"42","statusChange":{"to":"completed"}})),
            ],
        ));
        assert_eq!(t.current_task(), Some("Verify UI"));
        assert_eq!(
            t.todos(),
            Some(ferrite_core::transcript::Todos { done: 1, total: 2 })
        );
        for id in ["42", "93"] {
            t.apply(Input::Event(SessionEvent::Progress {
                event: ferrite_core::progress::ProgressEvent::Task {
                    id: id.into(),
                    subject: String::new(),
                    status: None,
                    deleted: true,
                },
            }));
        }
        assert_eq!(t.current_task(), None);
        assert_eq!(t.todos().unwrap().total, 0);
    }

    #[test]
    fn partial_tool_output_stays_bounded_and_completion_is_authoritative() {
        let mut t = fold(vec![SessionEvent::ToolStarted {
            id: "cmd".into(),
            name: "commandExecution".into(),
            input: json!({"command":"tests"}),
        }]);
        for text in ["12 tests ", "passed\n", &"🦀".repeat(20_000)] {
            t.apply(Input::Event(SessionEvent::ToolOutputDelta {
                id: "cmd".into(),
                text: text.into(),
            }));
        }
        let tool = |t: &Transcript| {
            t.blocks()
                .iter()
                .find_map(|b| {
                    if let Body::Tool(tool) = &b.body {
                        Some(tool.clone())
                    } else {
                        None
                    }
                })
                .unwrap()
        };
        let output = tool(&t).output.unwrap();
        assert!(output.text.starts_with("12 tests passed\n"));
        assert!(output.text.len() <= 64 * 1024);
        assert!(output.omitted_bytes > 0);
        t.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "cmd".into(),
            output: "Final result\n".into(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        }));
        assert_eq!(tool(&t).output.unwrap().text, "Final result\n");
        t.apply(Input::Event(SessionEvent::ToolOutputDelta {
            id: "cmd".into(),
            text: "late".into(),
        }));
        assert_eq!(tool(&t).output.unwrap().text, "Final result\n");
    }

    #[test]
    fn turn_end_clears_main_progress_but_background_requires_its_own_end() {
        use ferrite_core::progress::{ProgressEvent, TaskStatus};
        let mut t = fold(vec![
            SessionEvent::ReasoningSummaryDelta {
                text: "**Checking fold call sites**".into(),
                summary_index: 0,
            },
            SessionEvent::ToolStarted {
                id: "cmd".into(),
                name: "Bash".into(),
                input: json!({"command":"cargo test"}),
            },
            SessionEvent::Progress {
                event: ProgressEvent::Background {
                    id: "child".into(),
                    label: "Review".into(),
                    status: TaskStatus::Working,
                    detail: "Reading files".into(),
                },
            },
        ]);
        t.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Interrupted,
            cost_usd: None,
        }));
        assert_eq!(t.progress().caption(), None);
        assert_eq!(t.progress().working_background(), 1);
        assert_eq!(Instruments::of(&t).running, 0);
        assert_eq!(
            Instruments::of(&t).tests,
            None,
            "interrupted tests are neither passing nor failing"
        );
        t.apply(Input::Revived);
        assert_eq!(t.progress().working_background(), 0);
        assert_eq!(t.progress().background()[0].status, TaskStatus::Unknown);
    }

    #[test]
    fn native_claude_parallel_tool_blocks_are_all_retained() {
        let events = replay(
            "claude",
            "parallel",
            vec![json!({"type":"assistant","message":{"content":[
                {"type":"tool_use","id":"a","name":"Read","input":{"file_path":"a.rs"}},
                {"type":"tool_use","id":"b","name":"Read","input":{"file_path":"b.rs"}}
            ]}})],
        );
        assert_eq!(Instruments::of(&fold(events)).running, 2);
    }

    #[test]
    fn status_text_is_unicode_safe_and_terminal_escapes_never_reach_the_caption() {
        assert_eq!(
            ferrite_core::progress::headline("**Checking fold call sites**\nMore explanation"),
            "Checking fold call sites"
        );
        assert_eq!(
            ferrite_core::progress::one_line("\x1b[31m編譯 模組\x1b[0m\r\n", 20),
            "編譯 模組"
        );
        assert_eq!(ferrite_core::progress::one_line("🦀🦀🦀", 2), "🦀🦀…");
    }

    #[test]
    fn child_notifications_cannot_replace_main_heading_or_finish_main_turn() {
        let codex = fold(replay(
            "codex",
            "scope",
            vec![
                json!({"method":"item/reasoning/summaryTextDelta","params":{"threadId":"root","itemId":"r","summaryIndex":0,"delta":"**Checking Main**"}}),
                json!({"method":"item/reasoning/summaryTextDelta","params":{"threadId":"child","itemId":"c","summaryIndex":0,"delta":"Child heading"}}),
                json!({"method":"turn/completed","params":{"threadId":"child","turn":{"id":"ct","status":"completed"}}}),
            ],
        ));
        assert_eq!(codex.progress().caption().as_deref(), Some("Checking Main"));
        assert_eq!(codex.status(), ferrite_core::transcript::Status::Streaming);
        let claude = fold(replay(
            "claude",
            "scope",
            vec![
                json!({"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"Checking Main"}}}),
                json!({"type":"stream_event","parent_tool_use_id":"child","event":{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"Child heading"}}}),
                json!({"type":"result","parent_tool_use_id":"child","subtype":"success","is_error":false}),
            ],
        ));
        assert_eq!(
            claude.progress().caption().as_deref(),
            Some("Checking Main")
        );
        assert_eq!(claude.status(), ferrite_core::transcript::Status::Streaming);
    }

    #[test]
    fn captured_current_codex_heading_is_visible_once_even_if_completion_repeats() {
        let mut frames: Vec<Value> = include_str!("fixtures/codex-progress-summary-0.153.4.jsonl")
            .lines()
            .map(|line| {
                let mut frame: Value = serde_json::from_str(line).unwrap();
                frame["params"]["threadId"] = json!("root"); // Match only the stub's handshake identity.
                frame
            })
            .collect();
        frames.push(frames.last().unwrap().clone());
        let t = fold(replay("codex", "native-capture", frames));
        let headings: Vec<_> = t
            .blocks()
            .iter()
            .filter_map(|block| match &block.body {
                Body::Thinking(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            headings,
            ["**Finalizing boundary suffix and split handling**"]
        );
        assert_eq!(
            t.progress().caption().as_deref(),
            Some("Finalizing boundary suffix and split handling")
        );
    }

    #[test]
    fn completion_corrects_earlier_summary_parts_in_place() {
        let delta = |index, text| json!({"method":"item/reasoning/summaryTextDelta","params":{"threadId":"root","itemId":"r","summaryIndex":index,"delta":text}});
        let snapshot = json!({"method":"item/completed","params":{"threadId":"root","item":{"type":"reasoning","id":"r","summary":["Checking", "Tests"]}}});
        let t = fold(replay(
            "codex",
            "earlier-part",
            vec![
                delta(0, "Check"),
                delta(1, "Tests"),
                snapshot.clone(),
                snapshot,
            ],
        ));
        let headings: Vec<_> = t
            .blocks()
            .iter()
            .filter_map(|block| match &block.body {
                Body::Thinking(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(headings, ["Checking", "Tests"]);
        assert_eq!(t.progress().caption().as_deref(), Some("Tests"));
    }

    #[test]
    fn repeated_answer_completion_does_not_repeat_the_answer() {
        let snapshot = json!({"method":"item/completed","params":{"threadId":"root","item":{"type":"agentMessage","id":"m","text":"Hello"}}});
        let t = fold(replay(
            "codex",
            "repeat-answer",
            vec![
                json!({"method":"item/agentMessage/delta","params":{"threadId":"root","itemId":"m","delta":"Hello"}}),
                snapshot.clone(),
                snapshot,
            ],
        ));
        let answers: Vec<_> = t
            .blocks()
            .iter()
            .filter_map(|block| block.markdown.as_deref())
            .collect();
        assert_eq!(answers, ["Hello"]);
    }

    #[test]
    fn output_stays_an_exact_utf8_prefix_after_first_omission() {
        let mut t = fold(vec![SessionEvent::ToolStarted {
            id: "c".into(),
            name: "Bash".into(),
            input: json!({}),
        }]);
        for text in ["a".repeat(65535), "é".into(), "Z".into()] {
            t.apply(Input::Event(SessionEvent::ToolOutputDelta {
                id: "c".into(),
                text,
            }));
        }
        let output = t
            .blocks()
            .iter()
            .find_map(|b| match &b.body {
                Body::Tool(t) => t.output.as_ref(),
                _ => None,
            })
            .unwrap();
        assert_eq!(output.text, "a".repeat(65535));
        assert_eq!(output.omitted_bytes, 3);
    }
    fn fold_activity(events: Vec<SessionEvent>) -> ferrite_core::activity::Activity {
        use ferrite_core::activity::{Activity, ActivityInput};
        let mut activity = Activity::default();
        activity.apply(ActivityInput::Connect { generation: 1 });
        for event in events {
            activity.apply(ActivityInput::Main {
                input: Input::Event(event),
                at: std::time::Instant::now(),
            });
        }
        activity
    }

    #[test]
    fn native_codex_children_share_progress_and_do_not_revive_from_stale_turn_deltas() {
        let mut frames = vec![
            json!({"method":"item/reasoning/summaryTextDelta","params":{"threadId":"root","itemId":"main-heading","summaryIndex":0,"delta":"**Main review**"}}),
            json!({"method":"item/completed","params":{"threadId":"root","turnId":"main-turn","item":{"type":"subAgentActivity","id":"spawn","kind":"started","agentThreadId":"child"}}}),
            json!({"method":"turn/started","params":{"threadId":"child","turn":{"id":"child-turn"}}}),
            json!({"method":"item/started","params":{"threadId":"child","turnId":"child-turn","item":{"type":"contextCompaction","id":"compact"}}}),
            json!({"method":"item/reasoning/summaryTextDelta","params":{"threadId":"child","turnId":"child-turn","itemId":"heading","summaryIndex":0,"delta":"**Child review**"}}),
            json!({"method":"item/started","params":{"threadId":"child","turnId":"child-turn","item":{"type":"mcpToolCall","id":"tool","server":"docs","tool":"lookup"}}}),
            json!({"method":"item/mcpToolCall/progress","params":{"threadId":"child","turnId":"child-turn","itemId":"tool","message":"Reading references"}}),
            json!({"method":"item/plan/delta","params":{"threadId":"child","turnId":"child-turn","itemId":"plan","delta":"Read and verify"}}),
            json!({"method":"item/completed","params":{"threadId":"child","turnId":"child-turn","item":{"id":"plan","type":"plan","text":"Read and verify"}}}),
        ];
        let live = fold_activity(replay("codex", "child-progress-live", frames.clone()));
        let children = live.view().children();
        let child = children[0].transcript();
        assert_eq!(
            live.view()
                .main()
                .transcript()
                .progress()
                .caption()
                .as_deref(),
            Some("Main review")
        );
        assert_eq!(child.progress().caption().as_deref(), Some("Child review"));
        assert_eq!(child.status(), ferrite_core::transcript::Status::Streaming);
        let tool = child
            .blocks()
            .iter()
            .find_map(|block| {
                if let Body::Tool(tool) = &block.body {
                    Some(tool)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(tool.name, "docs · lookup");
        assert_eq!(
            child.progress().tool(&tool.call).unwrap().message,
            "Reading references"
        );
        assert_eq!(
            child
                .blocks()
                .iter()
                .filter(|block| block.markdown.as_deref() == Some("Read and verify"))
                .count(),
            1
        );
        assert!(!child
            .blocks()
            .iter()
            .any(|block| matches!(&block.body, Body::Notice(text) if text == "Read and verify")));
        frames.extend([
            json!({"method":"turn/completed","params":{"threadId":"child","turn":{"id":"child-turn","status":"completed"}}}),
            json!({"method":"item/commandExecution/outputDelta","params":{"threadId":"child","turnId":"child-turn","itemId":"tool","delta":"late"}}),
            json!({"method":"item/reasoning/summaryTextDelta","params":{"threadId":"child","turnId":"child-turn","itemId":"heading","summaryIndex":0,"delta":" stale"}}),
            json!({"method":"item/completed","params":{"threadId":"child","turnId":"child-turn","item":{"id":"final-heading","type":"reasoning","summary":["**Final child review**"]}}}),
        ]);
        let ended = fold_activity(replay("codex", "child-progress-ended", frames));
        let children = ended.view().children();
        let child = children[0].transcript();
        assert!(child.progress().caption().is_none());
        assert!(child.turn_elapsed().is_none());
        assert_eq!(ended.view().working_descendants(), 0);
        assert!(child.blocks().iter().any(
            |block| matches!(&block.body, Body::Thinking(text) if text == "**Final child review**")
        ));
        assert_eq!(
            ended
                .view()
                .main()
                .transcript()
                .progress()
                .caption()
                .as_deref(),
            Some("Main review")
        );
    }

    #[test]
    fn native_claude_task_progress_reaches_its_child_before_any_prose() {
        let events = replay(
            "claude",
            "child-progress",
            vec![
                json!({"type":"system","subtype":"init","session_id":"root","model":"probe"}),
                json!({"type":"assistant","session_id":"root","parent_tool_use_id":null,"message":{"content":[{"type":"tool_use","id":"agent","name":"Agent","input":{"description":"Review files","prompt":"Review files"}}]}}),
                json!({"type":"system","subtype":"task_started","session_id":"root","task_type":"local_agent","task_id":"task","tool_use_id":"agent","description":"Review files"}),
                json!({"type":"system","subtype":"task_progress","session_id":"root","task_id":"task","description":"Checking adapter boundaries"}),
            ],
        );
        let activity = fold_activity(events);
        let children = activity.view().children();
        assert_eq!(children.len(), 1);
        assert_eq!(
            children[0].transcript().progress().caption().as_deref(),
            Some("Checking adapter boundaries")
        );
        assert_eq!(
            children[0].transcript().status(),
            ferrite_core::transcript::Status::Streaming
        );
        assert_eq!(
            activity
                .view()
                .main()
                .transcript()
                .progress()
                .caption()
                .as_deref(),
            Some("Working")
        );
        assert!(activity
            .view()
            .main()
            .transcript()
            .progress()
            .background()
            .is_empty());
    }
}
