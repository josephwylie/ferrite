//! Native framebuffer artifacts using the real Cockpit renderer.
//! No live providers or operator store. Opt in with `visual-reference`.

use ferrite_core::{
    cockpit::{Cockpit, SpawnRequest, Spawner},
    providers::Session,
    store::{Provider, Store},
    workspace::WorkspaceChoice,
    DecisionAnswer, SessionEvent,
};
use gpui::{AppContext, HeadlessAppContext};
use std::{io, path::PathBuf, sync::mpsc};

struct Fixture(Option<mpsc::Receiver<SessionEvent>>);
struct FixtureSession(mpsc::Receiver<SessionEvent>);
impl Spawner for Fixture {
    fn spawn(&mut self, _: SpawnRequest) -> io::Result<Box<dyn Session>> {
        Ok(Box::new(FixtureSession(
            self.0.take().expect("one fixture Session"),
        )))
    }
}
impl Session for FixtureSession {
    fn events(&self) -> &mpsc::Receiver<SessionEvent> {
        &self.0
    }
    fn send(&mut self, _: &str) -> io::Result<()> {
        Ok(())
    }
    fn interrupt(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn respond_to_decision(&mut self, _: &str, _: DecisionAnswer) -> io::Result<()> {
        Ok(())
    }
}

pub fn capture(output: String) {
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output).expect("create artifact directory");
    let platform = gpui::platform::current_platform(true);
    for (label, width) in [("narrow", 720.), ("wide", 1000.)] {
        for state in [
            "formatting",
            "edges",
            "live",
            "decision",
            "approval",
            "expanded",
            "interrupted",
        ] {
            let store_path = std::env::temp_dir().join(format!(
                "ferrite-reference-{}-{label}-{state}",
                std::process::id()
            ));
            let (sender, receiver) = mpsc::channel();
            let mut core = Cockpit::new(
                Store::open(&store_path).unwrap(),
                Box::new(Fixture(Some(receiver))),
            );
            core.set_suggestions_enabled(false);
            let thread = core
                .open(
                    Provider::Claude,
                    WorkspaceChoice::Main {
                        checkout: std::env::current_dir().unwrap(),
                    },
                )
                .unwrap();
            core.send(
                thread,
                "Review the formatting fixture.  Preserve its spacing.".into(),
            );
            sender
                .send(SessionEvent::ToolStarted {
                    id: "read".into(),
                    name: "Read".into(),
                    input: serde_json::json!({"file_path": "formatting.md"}),
                })
                .unwrap();
            sender
                .send(SessionEvent::ToolCompleted {
                    id: "read".into(),
                    output: "Read the formatting fixture.".into(),
                    is_error: false,
                    result: ferrite_core::ToolResult::Opaque,
                })
                .unwrap();
            let source = match state {
            "formatting" => include_str!("../../../docs/research/cli-capture-2026-09-06/model-source/claude-formatting.md").to_string(),
            "edges" => format!("# Heading emphasis\n\n## Second heading\n\n**bold**, *italic*; `one  two` [label](https://example.com/exact?q=a%20b).\n\n9. Nine\n10. Ten with a wrapped continuation of several words\n\nSeparate list:\n\n100. One hundred with a wrapped continuation\n\n> First quote line\n> Second quote line\n>\n> > Nested quote\n\n```text\n    one  two\n\n\tthree   four  \n```\n\n```python\nprint(\"one  two\") # comment\nvalue = 42\n```\n\n| Name | Number | Long token |\n| :--- | ---: | :--- |\n| double  space | 12 | {} |\n| CJK 漢字 | 100 | é NBSP text |\n\n{}", "X".repeat(80), "W".repeat(124)),
            "live" => "I am checking the supplied fixture.\n\n## Streaming heading\n\n- First item\n- Second item\n\n```rust\nlet count = 2;".into(),
            "decision" => "The decision below preserves exact command and answer text.".into(),
                "approval" => "The next command needs your approval.".into(),
                "expanded" => "Tool details remain independently inspectable.".into(),
            _ => "Earlier work remains available after interruption.".into(),
        };
            sender
                .send(SessionEvent::TextDelta { text: source })
                .unwrap();
            for (id, error, result) in [
                ("ok", false, "one  two\nthree   four"),
                ("error", true, "Example failure\nAdditional detail"),
            ] {
                sender
                    .send(SessionEvent::ToolStarted {
                        id: id.into(),
                        name: "Bash".into(),
                        input: serde_json::json!({"command": "printf 'fixture'"}),
                    })
                    .unwrap();
                sender
                    .send(SessionEvent::ToolCompleted {
                        id: id.into(),
                        output: result.into(),
                        is_error: error,
                        result: ferrite_core::ToolResult::Opaque,
                    })
                    .unwrap();
            }
            match state {
                "live" => {
                    sender
                        .send(SessionEvent::ReasoningSummaryPart {
                            item_id: "reasoning".into(),
                            summary_index: 0,
                            snapshot: true,
                            text: "Checking the supplied spacing and tool results".into(),
                        })
                        .unwrap();
                }
                "approval" => {
                    sender.send(SessionEvent::DecisionRequested {decision:ferrite_core::Decision {
                delivery:Default::default(),id:"fixture-approval".into(),tool_use_id:"approval".into(),tool_name:"Bash".into(),description:"Inspect the fixture without modifying files.".into(), suggestions:vec![],input:serde_json::json!({"command":"printf 'one  two\\n'\ncat formatting.md"})
            }}).unwrap();
                }
                "decision" => {
                    sender.send(SessionEvent::DecisionRequested {decision:ferrite_core::Decision {
                delivery:Default::default(), id:"fixture-question".into(), tool_use_id:"question".into(), tool_name:"AskUserQuestion".into(), description:String::new(), suggestions:vec![],
                input:serde_json::json!({"questions":[{"question":"Which details should remain visible?", "multiSelect":true,"options":[{"label":"Keep the existing implementation and its meaningful suffix (Recommended)","description":"Preserve existing controls and all the spacing in their wrapped descriptions."},{"label":"Include detailed output", "description":"Keep disclosure available for inspection."}]}]})
            }}).unwrap();
                }
                "interrupted" => {
                    sender
                        .send(SessionEvent::TurnEnded {
                            outcome: ferrite_core::TurnOutcome::Interrupted,
                            cost_usd: None,
                        })
                        .unwrap();
                    core.pump();
                    core.send(
                        thread,
                        "Start a new turn; keep earlier tools inspectable.".into(),
                    );
                    sender
                        .send(SessionEvent::TextDelta {
                            text: "New turn in progress.".into(),
                        })
                        .unwrap();
                }
                _ => {
                    sender
                        .send(SessionEvent::TurnEnded {
                            outcome: ferrite_core::TurnOutcome::Completed,
                            cost_usd: None,
                        })
                        .unwrap();
                }
            }
            core.pump();

            let mut cx = HeadlessAppContext::with_platform(
                platform.text_system(),
                std::sync::Arc::new(crate::icons::Assets),
                gpui::platform::current_headless_renderer,
            );
            cx.update(|cx| {
                crate::theme::init_components(cx);
                cx.text_system()
                    .add_fonts(vec![
                        std::borrow::Cow::Borrowed(crate::JBM_REGULAR),
                        std::borrow::Cow::Borrowed(crate::JBM_MEDIUM),
                        std::borrow::Cow::Borrowed(crate::JBM_SEMIBOLD),
                        std::borrow::Cow::Borrowed(crate::JBM_BOLD),
                    ])
                    .unwrap();
            });
            let window = cx
                .open_window(
                    gpui::size(gpui::px(width), gpui::px(1400.)),
                    |window, cx| {
                        let view = cx.new(|cx| {
                            let mut view =
                                super::CockpitView::new_with_provider(core, Provider::Claude, cx);
                            if state == "expanded" {
                                for id in ["read", "ok"] {
                                    view.panes[0]
                                        .toggle_tool(&crate::pane::DisclosureId::Group(id.into()));
                                    view.panes[0]
                                        .toggle_tool(&crate::pane::DisclosureId::Tool(id.into()));
                                }
                            }
                            view
                        });
                        cx.new(|cx| gpui::component::Root::new(view, window, cx))
                    },
                )
                .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
            let _ = window.draw(cx);
            if label == "wide" && state == "edges" {
                let measurements: Vec<_> = ["\tb", "a\tb", "aaaa\tb", "        b"].into_iter().map(|text| {
                    let line = window.text_system().shape_line(text.to_owned().into(), gpui::px(crate::theme::FS_MD), &[gpui::TextRun {
                        len:text.len(),font:gpui::font(crate::theme::FONT_MONO),color:gpui::rgb(crate::theme::TEXT).into(),background_color:None,underline:None,strikethrough:None,
                    }],None);
                    serde_json::json!({"source":text,"b_x_logical_px":f32::from(line.x_for_index(text.len()-1))})
                }).collect();
                std::fs::write(output.join("native-tab-metrics.json"),serde_json::to_vec_pretty(&measurements).unwrap()).unwrap();
            }
        })
        .unwrap();
            cx.capture_screenshot(window.into())
                .unwrap()
                .save(output.join(format!("{state}-{label}.png")))
                .unwrap();
            // Only our fresh disposable store; never the operator's store.
            drop(cx);
            std::fs::remove_dir_all(store_path).expect("remove disposable reference store");
        }
    }
}
